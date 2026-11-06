//! v0.71: bounded, deterministic activity stream across a whole Situation.
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum ActivityError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("privileged SQL login prohibited")]
    Privileged,
    #[error("invalid page size")]
    Invalid,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SituationActivityEvent {
    pub event_type: String,
    pub analysis_id: Option<Uuid>,
    pub object_id: Uuid,
    pub detail: String,
    pub actor: Uuid,
    pub created_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivityCursor {
    pub created_at: String,
    pub event_type: String,
    pub object_id: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivityPage {
    pub items: Vec<SituationActivityEvent>,
    pub next: Option<ActivityCursor>,
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
) -> Result<Transaction<'_, Postgres>, ActivityError> {
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
        return Err(ActivityError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

const ACTIVITY_SQL: &str = "SELECT event_type,analysis_id,object_id,detail,actor,created_at::text FROM (\
 SELECT 'CLAIM'::text event_type,analysis_id,claim_id object_id,claim_text detail,created_by_principal_id actor,created_at FROM niccolo.situation_analysis_claims WHERE workspace_id=$1 AND situation_id=$2 \
 UNION ALL SELECT 'CLAIM_REVIEW',analysis_id,review_id,note,reviewer_principal_id,created_at FROM niccolo.situation_analysis_claim_reviews WHERE workspace_id=$1 AND situation_id=$2 \
 UNION ALL SELECT 'UNCERTAINTY',analysis_id,uncertainty_id,statement,recorded_by_principal_id,created_at FROM niccolo.situation_analysis_uncertainties WHERE workspace_id=$1 AND situation_id=$2 \
 UNION ALL SELECT 'UNCERTAINTY_EVENT',analysis_id,event_id,note,recorded_by_principal_id,created_at FROM niccolo.situation_analysis_uncertainty_events WHERE workspace_id=$1 AND situation_id=$2 \
 UNION ALL SELECT 'ACTION_PROPOSAL',analysis_id,proposal_id,action_text,proposed_by_principal_id,created_at FROM niccolo.situation_analysis_action_proposals WHERE workspace_id=$1 AND situation_id=$2 \
 UNION ALL SELECT 'ACTION_DECISION',analysis_id,decision_id,note,decided_by_principal_id,created_at FROM niccolo.situation_analysis_action_decisions WHERE workspace_id=$1 AND situation_id=$2 \
 UNION ALL SELECT 'ATTENTION_PIN',analysis_id,event_id,reason,recorded_by_principal_id,created_at FROM niccolo.situation_attention_pin_events WHERE workspace_id=$1 AND situation_id=$2 \
 UNION ALL SELECT 'REANALYSIS_REQUEST',prior_analysis_id,request_id,reason,requested_by_principal_id,created_at FROM niccolo.situation_reanalysis_requests WHERE workspace_id=$1 AND situation_id=$2 \
 UNION ALL SELECT 'REVIEW_CHECKPOINT',observed_analysis_id,checkpoint_id,coalesce(note,'review checkpoint'),recorded_by_principal_id,created_at FROM niccolo.situation_review_checkpoints WHERE workspace_id=$1 AND situation_id=$2 \
 UNION ALL SELECT 'ACTION_AUTHORIZATION',analysis_id,authorization_id,authorization_scope,recorded_by_principal_id,created_at FROM niccolo.situation_action_authorization_events WHERE workspace_id=$1 AND situation_id=$2 \
 UNION ALL SELECT 'REANALYSIS_SUGGESTION',analysis_id,suggestion_id,reason,proposed_by_principal_id,created_at FROM niccolo.situation_reanalysis_suggestions WHERE workspace_id=$1 AND situation_id=$2 \
 UNION ALL SELECT 'REANALYSIS_SUGGESTION_DECISION',analysis_id,decision_id,note,decided_by_principal_id,created_at FROM niccolo.situation_reanalysis_suggestion_decisions WHERE workspace_id=$1 AND situation_id=$2 \
 UNION ALL SELECT 'CONTEXT_EXCLUSION',analysis_id,contribution_id,reason,recorded_by_principal_id,created_at FROM niccolo.situation_analysis_context_exclusions WHERE workspace_id=$1 AND situation_id=$2 \
 UNION ALL SELECT 'CLAIM_OUTPUT_SPAN',analysis_id,claim_id,(byte_start::text||':'||byte_end::text),recorded_by_principal_id,created_at FROM niccolo.situation_analysis_claim_output_spans WHERE workspace_id=$1 AND situation_id=$2 \
 UNION ALL SELECT 'REVIEW_PACKAGE_ACKNOWLEDGEMENT',analysis_id,acknowledgement_id,acknowledgement_kind,acknowledged_by_principal_id,created_at FROM niccolo.situation_review_package_acknowledgements WHERE workspace_id=$1 AND situation_id=$2\
 ) e";

type Row = (String, Option<Uuid>, Uuid, String, Uuid, String);

fn map_rows(rows: Vec<Row>) -> Vec<SituationActivityEvent> {
    rows.into_iter()
        .map(
            |(event_type, analysis_id, object_id, detail, actor, created_at)| {
                SituationActivityEvent {
                    event_type,
                    analysis_id,
                    object_id,
                    detail,
                    actor,
                    created_at,
                }
            },
        )
        .collect()
}

pub async fn page(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    after: Option<&ActivityCursor>,
    limit: i64,
) -> Result<ActivityPage, ActivityError> {
    if !(1..=200).contains(&limit) {
        return Err(ActivityError::Invalid);
    }
    let mut tx = scoped(pool, workspace).await?;
    let (t, ty, id) = after
        .map(|c| {
            (
                Some(c.created_at.as_str()),
                Some(c.event_type.as_str()),
                Some(c.object_id),
            )
        })
        .unwrap_or((None, None, None));
    let sql = format!(
        "{ACTIVITY_SQL} WHERE ($3::timestamptz IS NULL OR (created_at,event_type,object_id)>($3::timestamptz,$4::text,$5::uuid)) ORDER BY created_at,event_type,object_id LIMIT $6"
    );
    let rows: Vec<Row> = sqlx::query_as(&sql)
        .bind(workspace)
        .bind(situation)
        .bind(t)
        .bind(ty)
        .bind(id)
        .bind(limit + 1)
        .fetch_all(&mut *tx)
        .await?;
    tx.commit().await?;
    let more = rows.len() as i64 > limit;
    let items = map_rows(rows.into_iter().take(limit as usize).collect());
    let next = more.then(|| {
        let last = items.last().expect("non-empty page when more rows exist");
        ActivityCursor {
            created_at: last.created_at.clone(),
            event_type: last.event_type.clone(),
            object_id: last.object_id,
        }
    });
    Ok(ActivityPage { items, next })
}

pub async fn list_all(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
) -> Result<Vec<SituationActivityEvent>, ActivityError> {
    let mut tx = scoped(pool, workspace).await?;
    let sql = format!("{ACTIVITY_SQL} ORDER BY created_at,event_type,object_id");
    let rows: Vec<Row> = sqlx::query_as(&sql)
        .bind(workspace)
        .bind(situation)
        .fetch_all(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(map_rows(rows))
}
