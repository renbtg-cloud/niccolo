//! v0.118-v0.120: deterministic human briefing assembled from explicit read models.
use crate::analysis_structure::AnalysisScope;
use crate::{analysis_assumptions, analysis_compare, decision_points};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};

#[derive(Debug, thiserror::Error)]
pub enum BriefingError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("assumptions unavailable: {0}")]
    Assumptions(#[from] analysis_assumptions::AssumptionError),
    #[error("decisions unavailable: {0}")]
    Decisions(#[from] decision_points::DecisionError),
    #[error("pending unavailable: {0}")]
    Pending(#[from] analysis_compare::AnalysisCompareError),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanBriefing {
    pub input_revision: i64,
    pub current_revision: i64,
    pub input_is_current: bool,
    pub assumption_count: usize,
    pub alternative_count: usize,
    pub decision_count: usize,
    pub open_question_count: usize,
    pub open_condition_count: usize,
    pub unresolved_uncertainty_count: usize,
    pub undecided_action_count: usize,
    pub automatically_ranked: bool,
    pub briefing_decides_for_human: bool,
    pub briefing_starts_analysis: bool,
    pub briefing_executes_actions: bool,
}
async fn scoped(
    pool: &PgPool,
    workspace: uuid::Uuid,
) -> Result<Transaction<'_, Postgres>, BriefingError> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *tx)
        .await?;
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

pub async fn build(pool: &PgPool, scope: AnalysisScope) -> Result<HumanBriefing, BriefingError> {
    let mut tx = scoped(pool, scope.workspace).await?;
    let row: (i64, i64) = sqlx::query_as("SELECT a.input_revision,s.current_revision FROM niccolo.situation_analysis_snapshots a JOIN niccolo.situations s USING(workspace_id,situation_id) WHERE a.workspace_id=$1 AND a.situation_id=$2 AND a.analysis_id=$3")
        .bind(scope.workspace)
        .bind(scope.situation)
        .bind(scope.analysis)
        .fetch_one(&mut *tx)
        .await?;
    tx.commit().await?;
    let lens = analysis_assumptions::lens(pool, scope).await?;
    let decisions = decision_points::package(pool, scope).await?;
    let pending = analysis_compare::human_pending(pool, scope).await?;
    Ok(HumanBriefing {
        input_revision: row.0,
        current_revision: row.1,
        input_is_current: row.0 == row.1,
        assumption_count: lens.assumptions.len(),
        alternative_count: lens.alternatives.len(),
        decision_count: decisions.decisions.len(),
        open_question_count: pending.open_question_ids.len(),
        open_condition_count: pending.open_condition_ids.len(),
        unresolved_uncertainty_count: pending.unresolved_uncertainty_ids.len(),
        undecided_action_count: pending.undecided_action_ids.len(),
        automatically_ranked: false,
        briefing_decides_for_human: false,
        briefing_starts_analysis: false,
        briefing_executes_actions: false,
    })
}
pub fn canonical_json(v: &HumanBriefing) -> Result<String, serde_json::Error> {
    serde_json::to_string(v)
}
pub fn sha256(v: &HumanBriefing) -> Result<String, serde_json::Error> {
    Ok(format!(
        "{:x}",
        Sha256::digest(canonical_json(v)?.as_bytes())
    ))
}
