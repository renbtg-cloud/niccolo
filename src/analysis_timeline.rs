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

/// v0.57: bounded keyset page over the deterministic analysis timeline.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimelineCursor {
    pub created_at: String,
    pub event_type: String,
    pub object_id: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimelinePage {
    pub items: Vec<TimelineEvent>,
    pub next: Option<TimelineCursor>,
}

pub async fn page(
    pool: &PgPool,
    scope: AnalysisScope,
    after: Option<&TimelineCursor>,
    limit: i64,
) -> Result<TimelinePage, TimelineError> {
    if !(1..=100).contains(&limit) {
        return Ok(TimelinePage {
            items: Vec::new(),
            next: None,
        });
    }
    let mut tx = scoped(pool, scope.workspace).await?;
    type Row = (String, Uuid, String, Uuid, String);
    let (after_time, after_type, after_id) = after
        .map(|c| {
            (
                Some(c.created_at.as_str()),
                Some(c.event_type.as_str()),
                Some(c.object_id),
            )
        })
        .unwrap_or((None, None, None));
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT event_type,object_id,detail,actor,created_at::text FROM (\
         SELECT 'CLAIM'::text event_type,claim_id object_id,claim_text detail,created_by_principal_id actor,created_at FROM niccolo.situation_analysis_claims WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 \
         UNION ALL SELECT 'CLAIM_REVIEW',review_id,note,reviewer_principal_id,created_at FROM niccolo.situation_analysis_claim_reviews WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 \
         UNION ALL SELECT 'UNCERTAINTY',uncertainty_id,statement,recorded_by_principal_id,created_at FROM niccolo.situation_analysis_uncertainties WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 \
         UNION ALL SELECT 'UNCERTAINTY_EVENT',event_id,note,recorded_by_principal_id,created_at FROM niccolo.situation_analysis_uncertainty_events WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 \
         UNION ALL SELECT 'ACTION_PROPOSAL',proposal_id,action_text,proposed_by_principal_id,created_at FROM niccolo.situation_analysis_action_proposals WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 \
         UNION ALL SELECT 'ACTION_DECISION',decision_id,note,decided_by_principal_id,created_at FROM niccolo.situation_analysis_action_decisions WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3\
         ) e WHERE ($4::timestamptz IS NULL OR (created_at,event_type,object_id) > ($4::timestamptz,$5::text,$6::uuid)) \
         ORDER BY created_at,event_type,object_id LIMIT $7",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(after_time)
    .bind(after_type)
    .bind(after_id)
    .bind(limit + 1)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    let more = rows.len() as i64 > limit;
    let items: Vec<TimelineEvent> = rows
        .into_iter()
        .take(limit as usize)
        .map(
            |(event_type, object_id, detail, actor, created_at)| TimelineEvent {
                event_type,
                object_id,
                detail,
                actor,
                created_at,
            },
        )
        .collect();
    let next = more.then(|| {
        let last = items.last().expect("non-empty page when more rows exist");
        TimelineCursor {
            created_at: last.created_at.clone(),
            event_type: last.event_type.clone(),
            object_id: last.object_id,
        }
    });
    Ok(TimelinePage { items, next })
}
