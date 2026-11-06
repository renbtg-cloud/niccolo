//! v0.79-v0.80: deterministic Situation activity ledger and hash with explicit governance boundaries.
use crate::situation_activity::{self, ActivityError, SituationActivityEvent};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum ControlLedgerError {
    #[error("activity failed: {0}")]
    Activity(#[from] ActivityError),
    #[error("serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlLedgerBoundaries {
    pub pin_means_auto_priority: bool,
    pub acknowledgement_means_agreement: bool,
    pub reanalysis_request_means_started: bool,
    pub reanalysis_suggestion_means_started: bool,
    pub authorization_means_execution: bool,
    pub context_exclusion_means_context_is_false: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SituationControlLedger {
    pub format: String,
    pub version: u32,
    pub workspace_id: Uuid,
    pub situation_id: Uuid,
    pub events: Vec<SituationActivityEvent>,
    pub boundaries: ControlLedgerBoundaries,
}

pub async fn build(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
) -> Result<SituationControlLedger, ControlLedgerError> {
    Ok(SituationControlLedger {
        format: "niccolo.situation-control-ledger".into(),
        version: 1,
        workspace_id: workspace,
        situation_id: situation,
        events: situation_activity::list_all(pool, workspace, situation).await?,
        boundaries: ControlLedgerBoundaries {
            pin_means_auto_priority: false,
            acknowledgement_means_agreement: false,
            reanalysis_request_means_started: false,
            reanalysis_suggestion_means_started: false,
            authorization_means_execution: false,
            context_exclusion_means_context_is_false: false,
        },
    })
}

pub async fn to_json(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
) -> Result<String, ControlLedgerError> {
    Ok(serde_json::to_string(
        &build(pool, workspace, situation).await?,
    )?)
}

pub async fn sha256(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
) -> Result<String, ControlLedgerError> {
    let digest = Sha256::digest(to_json(pool, workspace, situation).await?.as_bytes());
    Ok(digest.iter().map(|b| format!("{b:02x}")).collect())
}
