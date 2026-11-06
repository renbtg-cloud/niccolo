//! v0.45: deterministic audit timeline across analysis and human steering events.
use crate::analysis_structure::AnalysisScope;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum TimelineError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("privileged SQL login prohibited")]
    Privileged,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimelineEvent {
    pub event_type: String,
    pub object_id: Uuid,
    pub detail: String,
    pub actor: Uuid,
    pub created_at: String,
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
) -> Result<Transaction<'_, Postgres>, TimelineError> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *tx)
        .await?;
    let privileged: bool = sqlx::query_scalar(
        "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(&mut *tx)
    .await?;
    if privileged {
        return Err(TimelineError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

pub async fn list(
    pool: &PgPool,
    scope: AnalysisScope,
) -> Result<Vec<TimelineEvent>, TimelineError> {
    let mut tx = scoped(pool, scope.workspace).await?;
    type Row = (String, Uuid, String, Uuid, String);
    let rows:Vec<Row>=sqlx::query_as(
        "SELECT event_type,object_id,detail,actor,created_at::text FROM (\
         SELECT 'CLAIM'::text event_type,claim_id object_id,claim_text detail,created_by_principal_id actor,created_at FROM niccolo.situation_analysis_claims WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 \
         UNION ALL SELECT 'CLAIM_REVIEW',review_id,note,reviewer_principal_id,created_at FROM niccolo.situation_analysis_claim_reviews WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 \
         UNION ALL SELECT 'UNCERTAINTY',uncertainty_id,statement,recorded_by_principal_id,created_at FROM niccolo.situation_analysis_uncertainties WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 \
         UNION ALL SELECT 'UNCERTAINTY_EVENT',event_id,note,recorded_by_principal_id,created_at FROM niccolo.situation_analysis_uncertainty_events WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 \
         UNION ALL SELECT 'ACTION_PROPOSAL',proposal_id,action_text,proposed_by_principal_id,created_at FROM niccolo.situation_analysis_action_proposals WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 \
         UNION ALL SELECT 'ACTION_DECISION',decision_id,note,decided_by_principal_id,created_at FROM niccolo.situation_analysis_action_decisions WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3\
         ) e ORDER BY created_at,event_type,object_id")
        .bind(scope.workspace).bind(scope.situation).bind(scope.analysis).fetch_all(&mut *tx).await?;
    tx.commit().await?;
    Ok(rows
        .into_iter()
        .map(
            |(event_type, object_id, detail, actor, created_at)| TimelineEvent {
                event_type,
                object_id,
                detail,
                actor,
                created_at,
            },
        )
        .collect())
}
