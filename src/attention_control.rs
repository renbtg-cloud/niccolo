//! v0.58-v0.60: explainable attention reasons and human-selected pins.
//! The system reports reasons; it never assigns an automatic priority rank.
use crate::analysis_structure::AnalysisScope;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum AttentionControlError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("privileged SQL login prohibited")]
    Privileged,
    #[error("invalid attention request")]
    Invalid,
    #[error("idempotency conflict")]
    Conflict,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PinEvent {
    pub event_id: Uuid,
    pub target_type: String,
    pub target_id: Uuid,
    pub kind: String,
    pub reason: String,
    pub recorded_by: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurrentPin {
    pub analysis_id: Uuid,
    pub target_type: String,
    pub target_id: Uuid,
    pub reason: String,
    pub recorded_by: Uuid,
    pub event_id: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttentionReason {
    pub target_type: String,
    pub target_id: Uuid,
    pub reason_kind: String,
    pub human_selected: bool,
    pub automatically_ranked: bool,
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
    read_only: bool,
) -> Result<Transaction<'_, Postgres>, AttentionControlError> {
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
        return Err(AttentionControlError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

fn valid_reason(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 4096 && !value.chars().any(char::is_control)
}

pub async fn append_pin_event(
    pool: &PgPool,
    scope: AnalysisScope,
    event: &PinEvent,
) -> Result<(), AttentionControlError> {
    if !matches!(
        event.target_type.as_str(),
        "CLAIM" | "UNCERTAINTY" | "ACTION_PROPOSAL"
    ) || !matches!(event.kind.as_str(), "PINNED" | "UNPINNED")
        || !valid_reason(&event.reason)
    {
        return Err(AttentionControlError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, false).await?;
    sqlx::query(
        "INSERT INTO niccolo.situation_attention_pin_events \
         (workspace_id,situation_id,analysis_id,event_id,target_type,target_id,event_kind,reason,recorded_by_principal_id) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT DO NOTHING",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(event.event_id)
    .bind(&event.target_type)
    .bind(event.target_id)
    .bind(&event.kind)
    .bind(&event.reason)
    .bind(event.recorded_by)
    .execute(&mut *tx)
    .await?;
    let existing: Option<(Uuid, String, Uuid, String, String, Uuid)> = sqlx::query_as(
        "SELECT analysis_id,target_type,target_id,event_kind,reason,recorded_by_principal_id \
         FROM niccolo.situation_attention_pin_events \
         WHERE workspace_id=$1 AND situation_id=$2 AND event_id=$3",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(event.event_id)
    .fetch_optional(&mut *tx)
    .await?;
    if existing
        != Some((
            scope.analysis,
            event.target_type.clone(),
            event.target_id,
            event.kind.clone(),
            event.reason.clone(),
            event.recorded_by,
        ))
    {
        return Err(AttentionControlError::Conflict);
    }
    tx.commit().await?;
    Ok(())
}

pub async fn current_pins(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
) -> Result<Vec<CurrentPin>, AttentionControlError> {
    let mut tx = scoped(pool, workspace, true).await?;
    type Row = (Uuid, String, Uuid, String, Uuid, Uuid);
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT analysis_id,target_type,target_id,reason,recorded_by_principal_id,event_id FROM (\
           SELECT DISTINCT ON (analysis_id,target_type,target_id) analysis_id,target_type,target_id,reason,recorded_by_principal_id,event_id,event_kind,created_at \
           FROM niccolo.situation_attention_pin_events WHERE workspace_id=$1 AND situation_id=$2 \
           ORDER BY analysis_id,target_type,target_id,created_at DESC,event_id DESC\
         ) x WHERE event_kind='PINNED' ORDER BY analysis_id,target_type,target_id",
    )
    .bind(workspace)
    .bind(situation)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(rows
        .into_iter()
        .map(
            |(analysis_id, target_type, target_id, reason, recorded_by, event_id)| CurrentPin {
                analysis_id,
                target_type,
                target_id,
                reason,
                recorded_by,
                event_id,
            },
        )
        .collect())
}

pub async fn reasons(
    pool: &PgPool,
    scope: AnalysisScope,
) -> Result<Vec<AttentionReason>, AttentionControlError> {
    let mut tx = scoped(pool, scope.workspace, true).await?;
    let unresolved: Vec<Uuid> = sqlx::query_scalar(
        "SELECT u.uncertainty_id FROM niccolo.situation_analysis_uncertainties u \
         WHERE u.workspace_id=$1 AND u.situation_id=$2 AND u.analysis_id=$3 \
         AND NOT EXISTS(SELECT 1 FROM niccolo.situation_analysis_uncertainty_events e \
           WHERE e.workspace_id=u.workspace_id AND e.situation_id=u.situation_id AND e.analysis_id=u.analysis_id \
             AND e.uncertainty_id=u.uncertainty_id AND e.event_kind IN ('CLARIFIED','OUT_OF_SCOPE')) \
         ORDER BY u.uncertainty_id",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .fetch_all(&mut *tx)
    .await?;
    let undecided: Vec<Uuid> = sqlx::query_scalar(
        "SELECT p.proposal_id FROM niccolo.situation_analysis_action_proposals p \
         WHERE p.workspace_id=$1 AND p.situation_id=$2 AND p.analysis_id=$3 \
         AND NOT EXISTS(SELECT 1 FROM niccolo.situation_analysis_action_decisions d \
           WHERE d.workspace_id=p.workspace_id AND d.situation_id=p.situation_id AND d.analysis_id=p.analysis_id AND d.proposal_id=p.proposal_id) \
         ORDER BY p.proposal_id",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .fetch_all(&mut *tx)
    .await?;
    let pinned: Vec<(String, Uuid)> = sqlx::query_as(
        "SELECT target_type,target_id FROM (\
           SELECT DISTINCT ON (target_type,target_id) target_type,target_id,event_kind,created_at,event_id \
           FROM niccolo.situation_attention_pin_events \
           WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 \
           ORDER BY target_type,target_id,created_at DESC,event_id DESC\
         ) x WHERE event_kind='PINNED' ORDER BY target_type,target_id",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;

    let mut out = Vec::new();
    out.extend(unresolved.into_iter().map(|target_id| AttentionReason {
        target_type: "UNCERTAINTY".into(),
        target_id,
        reason_kind: "UNRESOLVED_UNCERTAINTY".into(),
        human_selected: false,
        automatically_ranked: false,
    }));
    out.extend(undecided.into_iter().map(|target_id| AttentionReason {
        target_type: "ACTION_PROPOSAL".into(),
        target_id,
        reason_kind: "UNDECIDED_ACTION".into(),
        human_selected: false,
        automatically_ranked: false,
    }));
    out.extend(
        pinned
            .into_iter()
            .map(|(target_type, target_id)| AttentionReason {
                target_type,
                target_id,
                reason_kind: "HUMAN_PIN".into(),
                human_selected: true,
                automatically_ranked: false,
            }),
    );
    out.sort_by_key(|r| (r.target_type.clone(), r.target_id, r.reason_kind.clone()));
    Ok(out)
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pin_reason_rejects_control_characters() {
        for value in ["reason\nforged", "reason\rforged", "reason\tforged", "x\0y"] {
            assert!(!valid_reason(value));
        }
        assert!(valid_reason("Human wants this contradiction kept visible."));
    }
}
