//! v0.61, v0.69-v0.70: reanalysis remains an explicit human-controlled request path.
use crate::analysis_structure::AnalysisScope;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum ReanalysisControlError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("privileged SQL login prohibited")]
    Privileged,
    #[error("invalid reanalysis control request")]
    Invalid,
    #[error("idempotency conflict")]
    Conflict,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReanalysisRequest {
    pub request_id: Uuid,
    pub prior_analysis_id: Uuid,
    pub reason: String,
    pub steering_event_type: String,
    pub steering_event_id: Uuid,
    pub requested_by: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReanalysisSuggestion {
    pub suggestion_id: Uuid,
    pub reason_kind: String,
    pub reason: String,
    pub proposed_by: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SuggestionDecision {
    pub suggestion_id: Uuid,
    pub decision_id: Uuid,
    pub kind: String,
    pub note: String,
    pub decided_by: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuggestionStatus {
    pub suggestion_id: Uuid,
    pub reason_kind: String,
    pub reason: String,
    pub latest_decision: Option<String>,
    pub requested_means_analysis_started: bool,
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
    read_only: bool,
) -> Result<Transaction<'_, Postgres>, ReanalysisControlError> {
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
        return Err(ReanalysisControlError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

fn valid_text(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 4096 && !value.chars().any(char::is_control)
}

pub async fn record_request(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    item: &ReanalysisRequest,
) -> Result<(), ReanalysisControlError> {
    if !valid_text(&item.reason)
        || !matches!(
            item.steering_event_type.as_str(),
            "CLAIM_REVIEW"
                | "CLARIFICATION_ANSWER"
                | "UNCERTAINTY_EVENT"
                | "ACTION_DECISION"
                | "ATTENTION_PIN_EVENT"
        )
    {
        return Err(ReanalysisControlError::Invalid);
    }
    let mut tx = scoped(pool, workspace, false).await?;
    sqlx::query(
        "INSERT INTO niccolo.situation_reanalysis_requests \
         (workspace_id,situation_id,request_id,prior_analysis_id,reason,steering_event_type,steering_event_id,requested_by_principal_id) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING",
    )
    .bind(workspace)
    .bind(situation)
    .bind(item.request_id)
    .bind(item.prior_analysis_id)
    .bind(&item.reason)
    .bind(&item.steering_event_type)
    .bind(item.steering_event_id)
    .bind(item.requested_by)
    .execute(&mut *tx)
    .await?;
    let existing: Option<(Uuid, String, String, Uuid, Uuid)> = sqlx::query_as(
        "SELECT prior_analysis_id,reason,steering_event_type,steering_event_id,requested_by_principal_id \
         FROM niccolo.situation_reanalysis_requests WHERE workspace_id=$1 AND situation_id=$2 AND request_id=$3",
    )
    .bind(workspace)
    .bind(situation)
    .bind(item.request_id)
    .fetch_optional(&mut *tx)
    .await?;
    if existing
        != Some((
            item.prior_analysis_id,
            item.reason.clone(),
            item.steering_event_type.clone(),
            item.steering_event_id,
            item.requested_by,
        ))
    {
        return Err(ReanalysisControlError::Conflict);
    }
    tx.commit().await?;
    Ok(())
}

pub async fn record_suggestion(
    pool: &PgPool,
    scope: AnalysisScope,
    item: &ReanalysisSuggestion,
) -> Result<(), ReanalysisControlError> {
    if !valid_text(&item.reason)
        || !matches!(
            item.reason_kind.as_str(),
            "UNCERTAINTY" | "CONTEXT_DELTA" | "CLAIM_REVIEW" | "CLARIFICATION" | "OTHER"
        )
    {
        return Err(ReanalysisControlError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, false).await?;
    sqlx::query(
        "INSERT INTO niccolo.situation_reanalysis_suggestions \
         (workspace_id,situation_id,analysis_id,suggestion_id,reason_kind,reason,proposed_by_principal_id) \
         VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT DO NOTHING",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(item.suggestion_id)
    .bind(&item.reason_kind)
    .bind(&item.reason)
    .bind(item.proposed_by)
    .execute(&mut *tx)
    .await?;
    let existing: Option<(String, String, Uuid)> = sqlx::query_as(
        "SELECT reason_kind,reason,proposed_by_principal_id
           FROM niccolo.situation_reanalysis_suggestions
          WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND suggestion_id=$4",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(item.suggestion_id)
    .fetch_optional(&mut *tx)
    .await?;
    if existing != Some((item.reason_kind.clone(), item.reason.clone(), item.proposed_by)) {
        return Err(ReanalysisControlError::Conflict);
    }
    tx.commit().await?;
    Ok(())
}

pub async fn append_suggestion_decision(
    pool: &PgPool,
    scope: AnalysisScope,
    item: &SuggestionDecision,
) -> Result<(), ReanalysisControlError> {
    if !valid_text(&item.note)
        || !matches!(item.kind.as_str(), "REQUESTED" | "DISMISSED" | "DEFERRED")
    {
        return Err(ReanalysisControlError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, false).await?;
    sqlx::query(
        "INSERT INTO niccolo.situation_reanalysis_suggestion_decisions \
         (workspace_id,situation_id,analysis_id,suggestion_id,decision_id,decision_kind,note,decided_by_principal_id) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(item.suggestion_id)
    .bind(item.decision_id)
    .bind(&item.kind)
    .bind(&item.note)
    .bind(item.decided_by)
    .execute(&mut *tx)
    .await?;
    let existing: Option<(Uuid, String, String, Uuid)> = sqlx::query_as(
        "SELECT suggestion_id,decision_kind,note,decided_by_principal_id
           FROM niccolo.situation_reanalysis_suggestion_decisions
          WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND decision_id=$4",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(item.decision_id)
    .fetch_optional(&mut *tx)
    .await?;
    if existing != Some((item.suggestion_id, item.kind.clone(), item.note.clone(), item.decided_by)) {
        return Err(ReanalysisControlError::Conflict);
    }
    tx.commit().await?;
    Ok(())
}

pub async fn suggestion_statuses(
    pool: &PgPool,
    scope: AnalysisScope,
) -> Result<Vec<SuggestionStatus>, ReanalysisControlError> {
    let mut tx = scoped(pool, scope.workspace, true).await?;
    type Row = (Uuid, String, String, Option<String>);
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT s.suggestion_id,s.reason_kind,s.reason,(\
           SELECT d.decision_kind FROM niccolo.situation_reanalysis_suggestion_decisions d \
           WHERE d.workspace_id=s.workspace_id AND d.situation_id=s.situation_id AND d.analysis_id=s.analysis_id AND d.suggestion_id=s.suggestion_id \
           ORDER BY d.created_at DESC,d.decision_id DESC LIMIT 1) \
         FROM niccolo.situation_reanalysis_suggestions s \
         WHERE s.workspace_id=$1 AND s.situation_id=$2 AND s.analysis_id=$3 ORDER BY s.suggestion_id",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(rows
        .into_iter()
        .map(
            |(suggestion_id, reason_kind, reason, latest_decision)| SuggestionStatus {
                suggestion_id,
                reason_kind,
                reason,
                latest_decision,
                requested_means_analysis_started: false,
            },
        )
        .collect())
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reanalysis_text_rejects_control_characters() {
        for value in ["reason\nforged", "reason\rforged", "note\tforged", "x\0y"] {
            assert!(!valid_text(value));
        }
        assert!(valid_text("Reanalyse only after this human steering event."));
    }
}
