//! v0.91-v0.94 and v0.98: append-only human review scope, handoff receipts, and session-local dispositions.
use crate::review_workflow::ReviewSessionScope;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum ReviewGovernanceError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("privileged SQL login prohibited")]
    Privileged,
    #[error("invalid review governance input")]
    Invalid,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScopeEvent {
    pub event_id: Uuid,
    pub scope_kind: String,
    pub event_kind: String,
    pub reason: String,
    pub recorded_by: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandoffReceipt {
    pub receipt_id: Uuid,
    pub handoff_sha256: String,
    pub kind: String,
    pub note: Option<String>,
    pub recipient: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReceiptView {
    pub receipt_id: Uuid,
    pub handoff_sha256: String,
    pub kind: String,
    pub recipient: Uuid,
    pub receipt_means_agreement: bool,
    pub receipt_grants_authority: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionDisposition {
    pub disposition_id: Uuid,
    pub target_type: String,
    pub target_id: Uuid,
    pub disposition: String,
    pub note: String,
    pub recorded_by: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CurrentDisposition {
    pub target_type: String,
    pub target_id: Uuid,
    pub disposition: String,
    pub note: String,
    pub remembered_future_policy: bool,
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
    read_only: bool,
) -> Result<Transaction<'_, Postgres>, ReviewGovernanceError> {
    let mut tx = pool.begin().await?;
    sqlx::query(if read_only {
        "SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY"
    } else {
        "SET TRANSACTION ISOLATION LEVEL READ COMMITTED"
    })
    .execute(&mut *tx)
    .await?;
    let privileged: bool = sqlx::query_scalar(
        "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(&mut *tx)
    .await?;
    if privileged {
        return Err(ReviewGovernanceError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

fn text_ok(v: &str, max: usize) -> bool {
    !v.trim().is_empty() && v.len() <= max
}

pub async fn append_scope_event(
    pool: &PgPool,
    scope: ReviewSessionScope,
    event: &ScopeEvent,
) -> Result<(), ReviewGovernanceError> {
    if !matches!(
        event.scope_kind.as_str(),
        "ALL" | "CLAIMS" | "SOURCES" | "CONTEXT" | "ACTIONS" | "QUESTIONS" | "CONDITIONS"
    ) || !matches!(event.event_kind.as_str(), "SELECTED" | "DESELECTED")
        || !text_ok(&event.reason, 2048)
    {
        return Err(ReviewGovernanceError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, false).await?;
    sqlx::query(
        "INSERT INTO niccolo.situation_review_scope_events \
         (workspace_id,situation_id,session_id,event_id,scope_kind,event_kind,reason,recorded_by_principal_id) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.session)
    .bind(event.event_id)
    .bind(&event.scope_kind)
    .bind(&event.event_kind)
    .bind(&event.reason)
    .bind(event.recorded_by)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn current_scope(
    pool: &PgPool,
    scope: ReviewSessionScope,
) -> Result<Vec<String>, ReviewGovernanceError> {
    let mut tx = scoped(pool, scope.workspace, true).await?;
    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT scope_kind FROM ( \
           SELECT DISTINCT ON (scope_kind) scope_kind,event_kind \
           FROM niccolo.situation_review_scope_events \
           WHERE workspace_id=$1 AND situation_id=$2 AND session_id=$3 \
           ORDER BY scope_kind,created_at DESC,event_id DESC \
         ) x WHERE event_kind='SELECTED' ORDER BY scope_kind",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.session)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(rows)
}

pub async fn record_handoff_receipt(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    analysis: Uuid,
    receipt: &HandoffReceipt,
) -> Result<(), ReviewGovernanceError> {
    if receipt.handoff_sha256.len() != 64
        || !receipt
            .handoff_sha256
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        || !matches!(receipt.kind.as_str(), "RECEIVED" | "REVIEWED" | "DECLINED")
        || receipt.note.as_ref().is_some_and(|v| !text_ok(v, 4096))
    {
        return Err(ReviewGovernanceError::Invalid);
    }
    let mut tx = scoped(pool, workspace, false).await?;
    sqlx::query(
        "INSERT INTO niccolo.situation_reasoning_handoff_receipts \
         (workspace_id,situation_id,analysis_id,receipt_id,handoff_sha256,receipt_kind,note,recipient_principal_id) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .bind(receipt.receipt_id)
    .bind(&receipt.handoff_sha256)
    .bind(&receipt.kind)
    .bind(&receipt.note)
    .bind(receipt.recipient)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn handoff_receipts(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    analysis: Uuid,
) -> Result<Vec<ReceiptView>, ReviewGovernanceError> {
    let mut tx = scoped(pool, workspace, true).await?;
    let rows: Vec<(Uuid, String, String, Uuid)> = sqlx::query_as(
        "SELECT receipt_id,handoff_sha256,receipt_kind,recipient_principal_id \
         FROM niccolo.situation_reasoning_handoff_receipts \
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 \
         ORDER BY created_at,receipt_id",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(rows
        .into_iter()
        .map(
            |(receipt_id, handoff_sha256, kind, recipient)| ReceiptView {
                receipt_id,
                handoff_sha256,
                kind,
                recipient,
                receipt_means_agreement: false,
                receipt_grants_authority: false,
            },
        )
        .collect())
}

pub async fn record_disposition(
    pool: &PgPool,
    scope: ReviewSessionScope,
    item: &SessionDisposition,
) -> Result<(), ReviewGovernanceError> {
    if !matches!(
        item.target_type.as_str(),
        "CLAIM" | "QUESTION" | "CONDITION" | "ACTION_PROPOSAL" | "UNCERTAINTY"
    ) || !matches!(
        item.disposition.as_str(),
        "KEEP_OPEN" | "DEFERRED" | "RESOLVED_FOR_THIS_SESSION" | "OUT_OF_SCOPE_FOR_THIS_SESSION"
    ) || !text_ok(&item.note, 4096)
    {
        return Err(ReviewGovernanceError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, false).await?;
    sqlx::query(
        "INSERT INTO niccolo.situation_review_session_dispositions \
         (workspace_id,situation_id,session_id,disposition_id,target_type,target_id,disposition,note,recorded_by_principal_id) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT DO NOTHING",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.session)
    .bind(item.disposition_id)
    .bind(&item.target_type)
    .bind(item.target_id)
    .bind(&item.disposition)
    .bind(&item.note)
    .bind(item.recorded_by)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn current_dispositions(
    pool: &PgPool,
    scope: ReviewSessionScope,
) -> Result<Vec<CurrentDisposition>, ReviewGovernanceError> {
    let mut tx = scoped(pool, scope.workspace, true).await?;
    let rows: Vec<(String, Uuid, String, String)> = sqlx::query_as(
        "SELECT target_type,target_id,disposition,note FROM ( \
           SELECT DISTINCT ON (target_type,target_id) target_type,target_id,disposition,note,created_at,disposition_id \
           FROM niccolo.situation_review_session_dispositions \
           WHERE workspace_id=$1 AND situation_id=$2 AND session_id=$3 \
           ORDER BY target_type,target_id,created_at DESC,disposition_id DESC \
         ) x ORDER BY target_type,target_id",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.session)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(rows
        .into_iter()
        .map(
            |(target_type, target_id, disposition, note)| CurrentDisposition {
                target_type,
                target_id,
                disposition,
                note,
                remembered_future_policy: false,
            },
        )
        .collect())
}
