//! v0.301-v0.310: one execution path behind every BFF delivery surface.
//! Sync, async, pull and future push transports observe the same durable operation.
use crate::bff_operations::{self, OperationScope, OperationView};
use crate::{agentic_runtime, reanalysis_runtime};
use crate::model_adapter::ReasoningModel;
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
        let _ = bff_operations::fail(
            pool,
            scope,
            current.state_version,
            actor,
            code,
            message,
        )
        .await;
    }
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

    // Every executable task enters RUNNING through the same durable transition.
    let running = if current.state == "QUEUED" {
        bff_operations::start(pool, scope, current.state_version, actor).await?
    } else {
        current
    };

    let result = match running.kind.as_str() {
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
    };

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
