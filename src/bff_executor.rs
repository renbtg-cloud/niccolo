//! v0.301-v0.310: one execution path behind every BFF delivery surface.
//! Sync, async, pull and future push transports observe the same durable operation.
use crate::bff_operations::{self, OperationScope, OperationView};
use crate::model_adapter::ReasoningModel;
use crate::{agentic_runtime, reanalysis_runtime, resource_policy, runtime_resources};
use sqlx::PgPool;
use std::{future::Future, time::Duration};
use uuid::Uuid;

const EXECUTION_LEASE_MS: i64 = 300_000;
const EXECUTION_HEARTBEAT_MS: u64 = 60_000;

#[derive(Debug, thiserror::Error)]
pub enum BffExecutionError {
    #[error("operation unavailable: {0}")]
    Operation(#[from] bff_operations::BffOperationError),
    #[error("agentic execution failed: {0}")]
    Agentic(String),
    #[error("reanalysis execution failed: {0}")]
    Reanalysis(String),
    #[error("runtime resource observation failed: {0}")]
    Resources(#[from] runtime_resources::ResourceError),
    #[error("runtime resource policy failed: {0}")]
    ResourcePolicy(#[from] resource_policy::ResourcePolicyError),
    #[error("execution lease lost")]
    ExecutionLeaseLost,
    #[error("execution lease heartbeat failed: {0}")]
    LeaseHeartbeat(String),
}

async fn mark_failed_best_effort(
    pool: &PgPool,
    scope: OperationScope,
    actor: Uuid,
    code: &str,
    message: &str,
) {
    if let Ok(current) = bff_operations::get(pool, scope).await
        && !current.terminal
    {
        let _ =
            bff_operations::fail(pool, scope, current.state_version, actor, code, message).await;
    }
}

fn agentic_outcome_is_stalled(state: &str) -> bool {
    matches!(state, "BLOCKED" | "IDLE")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkerRun {
    pub scope: OperationScope,
    pub view: OperationView,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LeaseHeartbeatState {
    Alive,
    BoundaryReached,
    Lost,
}

async fn supervise_execution<F, H, HFut>(
    work: F,
    heartbeat_every: Duration,
    mut heartbeat: H,
) -> Result<(), BffExecutionError>
where
    F: Future<Output = Result<(), BffExecutionError>>,
    H: FnMut() -> HFut,
    HFut: Future<Output = Result<LeaseHeartbeatState, BffExecutionError>>,
{
    debug_assert!(!heartbeat_every.is_zero());
    tokio::pin!(work);
    let mut ticker = tokio::time::interval(heartbeat_every);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // Tokio intervals tick immediately once. Consume that tick so the first
    // renewal happens after a real fraction of the lease has elapsed.
    ticker.tick().await;
    loop {
        tokio::select! {
            biased;
            result = &mut work => return result,
            _ = ticker.tick() => match heartbeat().await? {
                LeaseHeartbeatState::Alive => {}
                LeaseHeartbeatState::BoundaryReached => return Ok(()),
                LeaseHeartbeatState::Lost => return Err(BffExecutionError::ExecutionLeaseLost),
            }
        }
    }
}

async fn run_with_owned_lease(
    pool: &PgPool,
    scope: OperationScope,
    actor: Uuid,
    model: &(dyn ReasoningModel + Send + Sync),
    lease_owner: &str,
) -> Result<OperationView, BffExecutionError> {
    if !bff_operations::execution_owned_by(pool, scope, lease_owner).await? {
        return Ok(bff_operations::get(pool, scope).await?);
    }

    let work = async {
        let current = bff_operations::get(pool, scope).await?;
        let running = if current.state == "QUEUED" {
            bff_operations::start(pool, scope, current.state_version, actor).await?
        } else {
            current
        };
        match running.kind.as_str() {
            "AGENTIC_ANALYSIS" => {
                let outcome = agentic_runtime::run_until_boundary(pool, scope, actor, model)
                    .await
                    .map_err(|e| BffExecutionError::Agentic(e.to_string()))?;
                // A blocked/idle plan has no autonomous progress path. Returning
                // Ok used to strand the durable operation in RUNNING forever.
                if agentic_outcome_is_stalled(&outcome.state) {
                    Err(BffExecutionError::Agentic(format!(
                        "agentic plan cannot advance: {}",
                        outcome.state
                    )))
                } else {
                    Ok(())
                }
            }
            "REANALYZE" => reanalysis_runtime::execute(pool, scope, actor)
                .await
                .map(|_| ())
                .map_err(|e| BffExecutionError::Reanalysis(e.to_string())),
            _ => bff_operations::execute_deterministic(pool, scope, actor)
                .await
                .map(|_| ())
                .map_err(BffExecutionError::from),
        }
    };

    let result = supervise_execution(
        work,
        Duration::from_millis(EXECUTION_HEARTBEAT_MS),
        || async {
            match bff_operations::heartbeat_execution(pool, scope, lease_owner, EXECUTION_LEASE_MS)
                .await
            {
                Ok(true) => Ok(LeaseHeartbeatState::Alive),
                Ok(false) => {
                    let current = bff_operations::get(pool, scope)
                        .await
                        .map_err(|error| BffExecutionError::LeaseHeartbeat(error.to_string()))?;
                    if current.terminal || current.state == "WAITING_FOR_HUMAN" {
                        Ok(LeaseHeartbeatState::BoundaryReached)
                    } else {
                        Ok(LeaseHeartbeatState::Lost)
                    }
                }
                Err(error) => Err(BffExecutionError::LeaseHeartbeat(error.to_string())),
            }
        },
    )
    .await;

    match result {
        Ok(()) => {
            let _ = bff_operations::release_execution(pool, scope, lease_owner).await;
            Ok(bff_operations::get(pool, scope).await?)
        }
        Err(BffExecutionError::ExecutionLeaseLost) => {
            // Losing ownership is a delivery handoff, not a domain failure.
            // Another worker may already be executing the same durable operation.
            // Returning the durable view lets synchronous callers poll normally.
            Ok(bff_operations::get(pool, scope).await?)
        }
        Err(error @ BffExecutionError::LeaseHeartbeat(_)) => {
            // A transient database failure makes lease ownership unknowable.
            // Do not clear the lease early: letting its existing TTL expire gives
            // stale-step recovery time to become safe before another worker claims it.
            Err(error)
        }
        Err(error) => {
            let _ = bff_operations::release_execution(pool, scope, lease_owner).await;
            let code = match &error {
                BffExecutionError::Agentic(_) => "AGENTIC_EXECUTION_FAILED",
                BffExecutionError::Reanalysis(_) => "REANALYSIS_EXECUTION_FAILED",
                BffExecutionError::Operation(_)
                | BffExecutionError::Resources(_)
                | BffExecutionError::ResourcePolicy(_) => "BFF_EXECUTION_FAILED",
                BffExecutionError::ExecutionLeaseLost | BffExecutionError::LeaseHeartbeat(_) => {
                    unreachable!("lease supervision errors handled above")
                }
            };
            let message = error.to_string();
            mark_failed_best_effort(pool, scope, actor, code, &message).await;
            Err(error)
        }
    }
}

/// Execute at most one queued/recoverable operation in an already-authorized workspace.
pub async fn worker_once(
    pool: &PgPool,
    workspace: Uuid,
    actor: Uuid,
    worker_id: &str,
    model: &(dyn ReasoningModel + Send + Sync),
) -> Result<Option<WorkerRun>, BffExecutionError> {
    let Some(scope) =
        bff_operations::claim_next_in_workspace(pool, workspace, worker_id, EXECUTION_LEASE_MS)
            .await?
    else {
        return Ok(None);
    };
    let view = run_with_owned_lease(pool, scope, actor, model, worker_id).await?;
    Ok(Some(WorkerRun { scope, view }))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdaptiveWorkerAttempt {
    pub resources: runtime_resources::ResourceSnapshot,
    pub advice: resource_policy::ResourceAdvice,
    pub run: Option<WorkerRun>,
}

/// Observe the current Linux runtime envelope and apply the configured
/// admission mode before claiming work.
///
/// Self-managed and hybrid modes enforce the advised workspace-wide active
/// lease ceiling atomically. Externally-managed mode only reports the advice
/// and leaves parallelism to the operator/orchestrator.
pub async fn worker_once_adaptive(
    pool: &PgPool,
    workspace: Uuid,
    actor: Uuid,
    worker_id: &str,
    model: &(dyn ReasoningModel + Send + Sync),
    policy: &resource_policy::ResourcePolicy,
) -> Result<AdaptiveWorkerAttempt, BffExecutionError> {
    let resources = runtime_resources::observe()?;
    let advice = resource_policy::advise(&resources, policy)?;
    let scope = if advice.apply_concurrency_automatically {
        bff_operations::claim_next_in_workspace_limited(
            pool,
            workspace,
            worker_id,
            EXECUTION_LEASE_MS,
            advice.recommended_parallel_operations,
        )
        .await?
    } else {
        bff_operations::claim_next_in_workspace(pool, workspace, worker_id, EXECUTION_LEASE_MS)
            .await?
    };
    let run = match scope {
        Some(scope) => {
            let view = run_with_owned_lease(pool, scope, actor, model, worker_id).await?;
            Some(WorkerRun { scope, view })
        }
        None => None,
    };
    Ok(AdaptiveWorkerAttempt {
        resources,
        advice,
        run,
    })
}

/// Execute one already-submitted operation until its natural BFF boundary.
/// A human boundary is a successful return with a non-terminal WAITING_FOR_HUMAN state.
pub async fn execute(
    pool: &PgPool,
    scope: OperationScope,
    actor: Uuid,
    model: &(dyn ReasoningModel + Send + Sync),
) -> Result<OperationView, BffExecutionError> {
    let current = bff_operations::get(pool, scope).await?;
    if current.terminal || current.state == "WAITING_FOR_HUMAN" {
        return Ok(current);
    }
    let lease_owner = format!("exec-{}", Uuid::new_v4());
    if !bff_operations::claim_execution(pool, scope, &lease_owner, EXECUTION_LEASE_MS).await? {
        return Ok(bff_operations::get(pool, scope).await?);
    }
    run_with_owned_lease(pool, scope, actor, model, &lease_owner).await
}

#[cfg(test)]
mod stalled_outcome_tests {
    use super::*;

    #[test]
    fn only_undeliverable_agentic_states_fail_the_operation() {
        assert!(agentic_outcome_is_stalled("BLOCKED"));
        assert!(agentic_outcome_is_stalled("IDLE"));
        assert!(!agentic_outcome_is_stalled("WAITING_FOR_HUMAN"));
        assert!(!agentic_outcome_is_stalled("SUCCEEDED"));
    }

    #[tokio::test]
    async fn supervisor_renews_while_work_is_pending() {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };

        let heartbeats = Arc::new(AtomicUsize::new(0));
        let observed = heartbeats.clone();
        let result = supervise_execution(
            async {
                tokio::time::sleep(Duration::from_millis(55)).await;
                Ok(())
            },
            Duration::from_millis(10),
            move || {
                let observed = observed.clone();
                async move {
                    observed.fetch_add(1, Ordering::SeqCst);
                    Ok(LeaseHeartbeatState::Alive)
                }
            },
        )
        .await;

        assert!(result.is_ok());
        assert!(heartbeats.load(Ordering::SeqCst) >= 2);
    }

    #[tokio::test]
    async fn supervisor_stops_without_failing_domain_work_when_lease_is_lost() {
        let result = supervise_execution(
            async {
                tokio::time::sleep(Duration::from_millis(100)).await;
                Ok(())
            },
            Duration::from_millis(5),
            || async { Ok(LeaseHeartbeatState::Lost) },
        )
        .await;

        assert!(matches!(result, Err(BffExecutionError::ExecutionLeaseLost)));
    }

    #[tokio::test]
    async fn supervisor_accepts_a_boundary_that_completed_between_heartbeats() {
        let result = supervise_execution(
            async {
                tokio::time::sleep(Duration::from_millis(100)).await;
                Ok(())
            },
            Duration::from_millis(5),
            || async { Ok(LeaseHeartbeatState::BoundaryReached) },
        )
        .await;

        assert!(result.is_ok());
    }
}
