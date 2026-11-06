//! v0.701-v0.775: explicit human-requested reanalysis operation.
//! It creates a new frozen analysis snapshot linked to the prior one; it never rewrites history.
use crate::{
    analysis_snapshot,
    bff_operations::{self, OperationScope},
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;
#[derive(Debug, thiserror::Error)]
pub enum ReanalysisRuntimeError {
    #[error("operation unavailable: {0}")]
    Operation(#[from] bff_operations::BffOperationError),
    #[error("snapshot unavailable: {0}")]
    Snapshot(#[from] analysis_snapshot::AnalysisSnapshotError),
    #[error("invalid reanalysis operation")]
    Invalid,
    #[error("invalid reanalysis input: {0}")]
    Json(#[from] serde_json::Error),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReanalysisTaskInput {
    pub new_analysis_id: Uuid,
    pub goal: String,
}
pub async fn execute(
    pool: &PgPool,
    scope: OperationScope,
    actor: Uuid,
) -> Result<bff_operations::OperationView, ReanalysisRuntimeError> {
    let op = bff_operations::get(pool, scope).await?;
    if op.kind != "REANALYZE" || op.target_analysis_id.is_none() {
        return Err(ReanalysisRuntimeError::Invalid);
    }
    if op.terminal {
        return Ok(op);
    }
    let running = if op.state == "QUEUED" {
        bff_operations::start(pool, scope, op.state_version, actor).await?
    } else if op.state == "RUNNING" {
        op
    } else {
        return Err(ReanalysisRuntimeError::Invalid);
    };
    let input: ReanalysisTaskInput =
        serde_json::from_value(bff_operations::task_input(pool, scope).await?)?;
    if input.goal.trim().is_empty()
        || input.goal.len() > 4096
        || Some(input.new_analysis_id) == running.target_analysis_id
    {
        return Err(ReanalysisRuntimeError::Invalid);
    }
    let prior = running.target_analysis_id.expect("validated");
    let snap = analysis_snapshot::begin(
        pool,
        scope.workspace,
        actor,
        scope.situation,
        input.new_analysis_id,
        &input.goal,
        Some(prior),
    )
    .await?;
    let result = serde_json::json!({"new_analysis_id":snap.analysis_id,"prior_analysis_id":prior,"input_revision":snap.input_revision,"input_fingerprint":snap.input_fingerprint,"historical_output_preserved":true,"agentic_execution_started":false,"human_requested_operation":true});
    Ok(bff_operations::succeed(pool, scope, running.state_version, actor, &result).await?)
}
