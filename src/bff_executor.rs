//! v0.301-v0.310: one execution path behind every BFF delivery surface.
//! Sync, async, pull and future push transports observe the same durable operation.
use crate::bff_operations::{self, OperationScope, OperationView};
use crate::model_adapter::ReasoningModel;
use crate::{agentic_runtime, reanalysis_runtime, resource_policy, runtime_resources};
use sqlx::PgPool;
use uuid::Uuid;

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkerRun {
    pub scope: OperationScope,
    pub view: OperationView,
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

    let result = async {
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
                if matches!(outcome.state.as_str(), "BLOCKED" | "IDLE") {
                    Err(BffExecutionError::Agentic(format!(
                        "agentic plan cannot advance: {}", outcome.state
                    )))
                } else {
                    Ok(())
                }
            },
            "REANALYZE" => reanalysis_runtime::execute(pool, scope, actor)
                .await
                .map(|_| ())
                .map_err(|e| BffExecutionError::Reanalysis(e.to_string())),
            _ => bff_operations::execute_deterministic(pool, scope, actor)
                .await
                .map(|_| ())
                .map_err(BffExecutionError::from),
        }
    }
    .await;

    let _ = bff_operations::release_execution(pool, scope, lease_owner).await;
    if let Err(error) = result {
        let code = match &error {
            BffExecutionError::Agentic(_) => "AGENTIC_EXECUTION_FAILED",
            BffExecutionError::Reanalysis(_) => "REANALYSIS_EXECUTION_FAILED",
            BffExecutionError::Operation(_)
            | BffExecutionError::Resources(_)
            | BffExecutionError::ResourcePolicy(_) => "BFF_EXECUTION_FAILED",
        };
        let message = error.to_string();
        mark_failed_best_effort(pool, scope, actor, code, &message).await;
        return Err(error);
    }
    Ok(bff_operations::get(pool, scope).await?)
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
        bff_operations::claim_next_in_workspace(pool, workspace, worker_id, 300_000).await?
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
            300_000,
            advice.recommended_parallel_operations,
        )
        .await?
    } else {
        bff_operations::claim_next_in_workspace(pool, workspace, worker_id, 300_000).await?
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
    if !bff_operations::claim_execution(pool, scope, &lease_owner, 300_000).await? {
        return Ok(bff_operations::get(pool, scope).await?);
    }
    run_with_owned_lease(pool, scope, actor, model, &lease_owner).await
}
