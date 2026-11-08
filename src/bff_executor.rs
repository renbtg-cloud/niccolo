//! v0.301-v0.310: one execution path behind every BFF delivery surface.
//! Sync, async, pull and future push transports observe the same durable operation.
use crate::bff_operations::{self, OperationScope, OperationView};
use crate::model_adapter::ReasoningModel;
use crate::{
    agentic_runtime, reanalysis_runtime, resource_policy, runtime_resources,
};
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
    #[error("resource admission policy invalid: {0}")]
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
            "AGENTIC_ANALYSIS" => agentic_runtime::run_until_boundary(pool, scope, actor, model)
                .await
                .map(|_| ())
                .map_err(|e| BffExecutionError::Agentic(e.to_string())),
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
            BffExecutionError::Operation(_) => "BFF_EXECUTION_FAILED",
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
    pub run: Option<WorkerRun>,
    pub advice: Option<resource_policy::ResourceAdvice>,
    /// True only when local/self-managed policy could not observe resources and
    /// conservatively fell back to one active lease. It is not a quality score.
    pub observation_fallback_used: bool,
    /// None means the external resource manager remains authoritative for
    /// admission. Some(n) is the local lease cap used for this attempt.
    pub local_active_lease_limit: Option<u16>,
}

/// Execute at most one queued/recoverable operation with bounded resource-aware
/// admission. This is the first scheduler wiring slice:
///
/// - SELF_MANAGED/HYBRID apply the advisory concurrency ceiling to *worker*
///   lease claims; unknown/unavailable telemetry falls back to one active lease.
/// - EXTERNALLY_MANAGED observes when possible but does not locally cap claims.
/// - Direct sync/async execution surfaces are intentionally unchanged here, so
///   this is not yet a process-wide concurrency guarantee.
/// - Critical pressure preserves one path of progress instead of stopping all work.
pub async fn worker_once_with_resource_policy(
    pool: &PgPool,
    workspace: Uuid,
    actor: Uuid,
    worker_id: &str,
    model: &(dyn ReasoningModel + Send + Sync),
    policy: &resource_policy::ResourcePolicy,
) -> Result<AdaptiveWorkerAttempt, BffExecutionError> {
    resource_policy::validate(policy)?;

    let observed = runtime_resources::observe();
    let advice = observed
        .as_ref()
        .ok()
        .and_then(|snapshot| resource_policy::advise(snapshot, policy).ok());

    let (scope, observation_fallback_used, local_active_lease_limit) = match policy.mode {
        resource_policy::ResourceManagementMode::ExternallyManaged => (
            bff_operations::claim_next_in_workspace(pool, workspace, worker_id, 300_000).await?,
            false,
            None,
        ),
        resource_policy::ResourceManagementMode::SelfManaged
        | resource_policy::ResourceManagementMode::Hybrid => {
            let (limit, fallback) = match &advice {
                Some(value) => (value.recommended_parallel_operations, false),
                None => (1, true),
            };
            (
                bff_operations::claim_next_in_workspace_with_limit(
                    pool,
                    workspace,
                    worker_id,
                    300_000,
                    limit,
                )
                .await?,
                fallback,
                Some(limit),
            )
        }
    };

    let run = match scope {
        Some(scope) => {
            let view = run_with_owned_lease(pool, scope, actor, model, worker_id).await?;
            Some(WorkerRun { scope, view })
        }
        None => None,
    };

    Ok(AdaptiveWorkerAttempt {
        run,
        advice,
        observation_fallback_used,
        local_active_lease_limit,
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
