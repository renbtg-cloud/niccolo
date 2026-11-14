//! v0.62-v0.64: review checkpoints, situation dashboard, and deterministic situation export.
use crate::analysis_read_models::{self, AnalysisIndexItem, ReadModelError};
use crate::attention_control::{self, AttentionControlError, CurrentPin};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum ReviewSessionError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("privileged SQL login prohibited")]
    Privileged,
    #[error("invalid review checkpoint")]
    Invalid,
    #[error("review checkpoint replay conflicts with stored content")]
    Conflict,
    #[error("checkpoint not visible")]
    Invisible,
    #[error("read model failed: {0}")]
    Read(#[from] ReadModelError),
    #[error("attention model failed: {0}")]
    Attention(#[from] AttentionControlError),
    #[error("serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewCheckpoint {
    pub checkpoint_id: Uuid,
    pub observed_revision: i64,
    pub observed_analysis_id: Option<Uuid>,
    pub note: Option<String>,
    pub recorded_by: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SinceCheckpoint {
    pub checkpoint_id: Uuid,
    pub observed_revision: i64,
    pub current_revision: i64,
    pub revision_delta: i64,
    pub analyses_on_newer_revisions: i64,
    pub human_events_after_checkpoint: i64,
    pub changed_since_checkpoint: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SituationDashboard {
    pub situation_id: Uuid,
    pub current_revision: i64,
    pub analysis_count: i64,
    pub analyses_on_current_revision: i64,
    pub unresolved_uncertainties: i64,
    pub undecided_action_proposals: i64,
    pub human_pins: i64,
    pub reanalysis_requests: i64,
    pub undecided_reanalysis_suggestions: i64,
    pub active_action_authorizations: i64,
    pub system_auto_prioritizes_attention: bool,
    pub authorization_means_execution: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SituationReviewExport {
    pub format: String,
    pub version: u32,
    pub workspace_id: Uuid,
    pub situation_id: Uuid,
    pub dashboard: SituationDashboard,
    pub analyses: Vec<AnalysisIndexItem>,
    pub pins: Vec<CurrentPin>,
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
    read_only: bool,
) -> Result<Transaction<'_, Postgres>, ReviewSessionError> {
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
        return Err(ReviewSessionError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

fn valid_note(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 4096 && !value.chars().any(char::is_control)
}

pub async fn record_checkpoint(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    item: &ReviewCheckpoint,
) -> Result<(), ReviewSessionError> {
    if item.observed_revision < 1
        || item.note.as_ref().is_some_and(|value| !valid_note(value))
    {
        return Err(ReviewSessionError::Invalid);
    }
    let mut tx = scoped(pool, workspace, false).await?;
    sqlx::query(
        "INSERT INTO niccolo.situation_review_checkpoints \
         (workspace_id,situation_id,checkpoint_id,observed_revision,observed_analysis_id,note,recorded_by_principal_id) \
         VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT DO NOTHING",
    )
    .bind(workspace)
    .bind(situation)
    .bind(item.checkpoint_id)
    .bind(item.observed_revision)
    .bind(item.observed_analysis_id)
    .bind(&item.note)
    .bind(item.recorded_by)
    .execute(&mut *tx)
    .await?;
    let stored: Option<(i64, Option<Uuid>, Option<String>, Uuid)> = sqlx::query_as(
        "SELECT observed_revision,observed_analysis_id,note,recorded_by_principal_id
           FROM niccolo.situation_review_checkpoints
          WHERE workspace_id=$1 AND situation_id=$2 AND checkpoint_id=$3",
    )
    .bind(workspace)
    .bind(situation)
    .bind(item.checkpoint_id)
    .fetch_optional(&mut *tx)
    .await?;
    if stored
        != Some((
            item.observed_revision,
            item.observed_analysis_id,
            item.note.clone(),
            item.recorded_by,
        ))
    {
        return Err(ReviewSessionError::Conflict);
    }
    tx.commit().await?;
    Ok(())
}

pub async fn since_checkpoint(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    checkpoint: Uuid,
) -> Result<SinceCheckpoint, ReviewSessionError> {
    let mut tx = scoped(pool, workspace, true).await?;
    let row: Option<(i64, i64, i64, i64)> = sqlx::query_as(
        "SELECT c.observed_revision,s.current_revision,\
          (SELECT count(*) FROM niccolo.situation_analysis_snapshots a WHERE a.workspace_id=c.workspace_id AND a.situation_id=c.situation_id AND a.input_revision>c.observed_revision),\
          (SELECT count(*) FROM (\
             SELECT created_at FROM niccolo.situation_analysis_claim_reviews r WHERE r.workspace_id=c.workspace_id AND r.situation_id=c.situation_id AND r.created_at>c.created_at \
             UNION ALL SELECT created_at FROM niccolo.situation_analysis_uncertainty_events e WHERE e.workspace_id=c.workspace_id AND e.situation_id=c.situation_id AND e.created_at>c.created_at \
             UNION ALL SELECT created_at FROM niccolo.situation_analysis_action_decisions d WHERE d.workspace_id=c.workspace_id AND d.situation_id=c.situation_id AND d.created_at>c.created_at \
             UNION ALL SELECT created_at FROM niccolo.situation_attention_pin_events p WHERE p.workspace_id=c.workspace_id AND p.situation_id=c.situation_id AND p.created_at>c.created_at\
           ) h) \
         FROM niccolo.situation_review_checkpoints c JOIN niccolo.situations s USING(workspace_id,situation_id) \
         WHERE c.workspace_id=$1 AND c.situation_id=$2 AND c.checkpoint_id=$3",
    )
    .bind(workspace)
    .bind(situation)
    .bind(checkpoint)
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    let Some((
        observed_revision,
        current_revision,
        analyses_on_newer_revisions,
        human_events_after_checkpoint,
    )) = row
    else {
        return Err(ReviewSessionError::Invisible);
    };
    Ok(SinceCheckpoint {
        checkpoint_id: checkpoint,
        observed_revision,
        current_revision,
        revision_delta: current_revision - observed_revision,
        analyses_on_newer_revisions,
        human_events_after_checkpoint,
        changed_since_checkpoint: current_revision != observed_revision
            || analyses_on_newer_revisions > 0
            || human_events_after_checkpoint > 0,
    })
}

pub async fn dashboard(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
) -> Result<SituationDashboard, ReviewSessionError> {
    let mut tx = scoped(pool, workspace, true).await?;
    let current_revision: i64 = sqlx::query_scalar(
        "SELECT current_revision FROM niccolo.situations WHERE workspace_id=$1 AND situation_id=$2",
    )
    .bind(workspace)
    .bind(situation)
    .fetch_one(&mut *tx)
    .await?;
    let analysis_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM niccolo.situation_analysis_snapshots WHERE workspace_id=$1 AND situation_id=$2",
    )
    .bind(workspace)
    .bind(situation)
    .fetch_one(&mut *tx)
    .await?;
    let analyses_on_current_revision: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM niccolo.situation_analysis_snapshots WHERE workspace_id=$1 AND situation_id=$2 AND input_revision=$3",
    )
    .bind(workspace)
    .bind(situation)
    .bind(current_revision)
    .fetch_one(&mut *tx)
    .await?;
    let unresolved_uncertainties: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM niccolo.situation_analysis_uncertainties u WHERE u.workspace_id=$1 AND u.situation_id=$2 \
         AND NOT EXISTS(SELECT 1 FROM niccolo.situation_analysis_uncertainty_events e WHERE e.workspace_id=u.workspace_id AND e.situation_id=u.situation_id AND e.analysis_id=u.analysis_id AND e.uncertainty_id=u.uncertainty_id AND e.event_kind IN ('CLARIFIED','OUT_OF_SCOPE'))",
    )
    .bind(workspace)
    .bind(situation)
    .fetch_one(&mut *tx)
    .await?;
    let undecided_action_proposals: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM niccolo.situation_analysis_action_proposals p WHERE p.workspace_id=$1 AND p.situation_id=$2 \
         AND NOT EXISTS(SELECT 1 FROM niccolo.situation_analysis_action_decisions d WHERE d.workspace_id=p.workspace_id AND d.situation_id=p.situation_id AND d.analysis_id=p.analysis_id AND d.proposal_id=p.proposal_id)",
    )
    .bind(workspace)
    .bind(situation)
    .fetch_one(&mut *tx)
    .await?;
    let human_pins: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM (SELECT DISTINCT ON (analysis_id,target_type,target_id) event_kind FROM niccolo.situation_attention_pin_events WHERE workspace_id=$1 AND situation_id=$2 ORDER BY analysis_id,target_type,target_id,created_at DESC,event_id DESC) x WHERE event_kind='PINNED'",
    )
    .bind(workspace)
    .bind(situation)
    .fetch_one(&mut *tx)
    .await?;
    let reanalysis_requests: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM niccolo.situation_reanalysis_requests WHERE workspace_id=$1 AND situation_id=$2",
    )
    .bind(workspace)
    .bind(situation)
    .fetch_one(&mut *tx)
    .await?;
    let undecided_reanalysis_suggestions: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM niccolo.situation_reanalysis_suggestions s WHERE s.workspace_id=$1 AND s.situation_id=$2 \
         AND NOT EXISTS(SELECT 1 FROM niccolo.situation_reanalysis_suggestion_decisions d WHERE d.workspace_id=s.workspace_id AND d.situation_id=s.situation_id AND d.analysis_id=s.analysis_id AND d.suggestion_id=s.suggestion_id)",
    )
    .bind(workspace)
    .bind(situation)
    .fetch_one(&mut *tx)
    .await?;
    let active_action_authorizations: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM (SELECT DISTINCT ON (analysis_id,proposal_id) event_kind FROM niccolo.situation_action_authorization_events WHERE workspace_id=$1 AND situation_id=$2 ORDER BY analysis_id,proposal_id,created_at DESC,authorization_id DESC) x WHERE event_kind='AUTHORIZED'",
    )
    .bind(workspace)
    .bind(situation)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(SituationDashboard {
        situation_id: situation,
        current_revision,
        analysis_count,
        analyses_on_current_revision,
        unresolved_uncertainties,
        undecided_action_proposals,
        human_pins,
        reanalysis_requests,
        undecided_reanalysis_suggestions,
        active_action_authorizations,
        system_auto_prioritizes_attention: false,
        authorization_means_execution: false,
    })
}

pub async fn export(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
) -> Result<SituationReviewExport, ReviewSessionError> {
    Ok(SituationReviewExport {
        format: "niccolo.situation-review".into(),
        version: 1,
        workspace_id: workspace,
        situation_id: situation,
        dashboard: dashboard(pool, workspace, situation).await?,
        analyses: analysis_read_models::analysis_index(pool, workspace, situation).await?,
        pins: attention_control::current_pins(pool, workspace, situation).await?,
    })
}

pub async fn to_json(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
) -> Result<String, ReviewSessionError> {
    Ok(serde_json::to_string(
        &export(pool, workspace, situation).await?,
    )?)
}

pub async fn sha256(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
) -> Result<String, ReviewSessionError> {
    let digest = Sha256::digest(to_json(pool, workspace, situation).await?.as_bytes());
    Ok(digest.iter().map(|b| format!("{b:02x}")).collect())
}

/// v0.65: immutable acknowledgement/checkpoint history; reviewing something does not accept it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointHistoryItem {
    pub checkpoint_id: Uuid,
    pub observed_revision: i64,
    pub observed_analysis_id: Option<Uuid>,
    pub note: Option<String>,
    pub recorded_by: Uuid,
    pub review_means_acceptance: bool,
}

pub async fn checkpoint_history(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
) -> Result<Vec<CheckpointHistoryItem>, ReviewSessionError> {
    let mut tx = scoped(pool, workspace, true).await?;
    type Row = (Uuid, i64, Option<Uuid>, Option<String>, Uuid);
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT checkpoint_id,observed_revision,observed_analysis_id,note,recorded_by_principal_id \
         FROM niccolo.situation_review_checkpoints WHERE workspace_id=$1 AND situation_id=$2 \
         ORDER BY created_at,checkpoint_id",
    )
    .bind(workspace)
    .bind(situation)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(rows
        .into_iter()
        .map(
            |(checkpoint_id, observed_revision, observed_analysis_id, note, recorded_by)| {
                CheckpointHistoryItem {
                    checkpoint_id,
                    observed_revision,
                    observed_analysis_id,
                    note,
                    recorded_by,
                    review_means_acceptance: false,
                }
            },
        )
        .collect())
}

/// v0.72: compact human-control state. Counts are state, never an instruction to auto-act.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanControlState {
    pub human_pins: i64,
    pub reanalysis_requests: i64,
    pub undecided_reanalysis_suggestions: i64,
    pub active_action_authorizations: i64,
    pub review_checkpoints: i64,
    pub review_package_acknowledgements: i64,
    pub automatic_reanalysis_started: bool,
    pub external_actions_executed: bool,
}

pub async fn human_control_state(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
) -> Result<HumanControlState, ReviewSessionError> {
    let d = dashboard(pool, workspace, situation).await?;
    let mut tx = scoped(pool, workspace, true).await?;
    let review_checkpoints: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM niccolo.situation_review_checkpoints WHERE workspace_id=$1 AND situation_id=$2",
    )
    .bind(workspace)
    .bind(situation)
    .fetch_one(&mut *tx)
    .await?;
    let review_package_acknowledgements: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM niccolo.situation_review_package_acknowledgements WHERE workspace_id=$1 AND situation_id=$2",
    )
    .bind(workspace)
    .bind(situation)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(HumanControlState {
        human_pins: d.human_pins,
        reanalysis_requests: d.reanalysis_requests,
        undecided_reanalysis_suggestions: d.undecided_reanalysis_suggestions,
        active_action_authorizations: d.active_action_authorizations,
        review_checkpoints,
        review_package_acknowledgements,
        automatic_reanalysis_started: false,
        external_actions_executed: false,
    })
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkpoint_note_rejects_control_characters() {
        for value in ["checkpoint\nforged", "note\rforged", "note\tforged", "x\0y"] {
            assert!(!valid_note(value));
        }
        assert!(valid_note("Reviewed without implying acceptance."));
    }
}
