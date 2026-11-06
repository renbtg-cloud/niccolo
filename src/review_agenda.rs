//! v0.99-v0.100: deterministic human review agenda and integrity hash.
use crate::analysis_compare::{self, HumanPending};
use crate::analysis_structure::AnalysisScope;
use crate::review_governance::{self, CurrentDisposition};
use crate::review_workflow::ReviewSessionScope;
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::PgPool;

#[derive(Debug, thiserror::Error)]
pub enum ReviewAgendaError {
    #[error("review governance failed: {0}")]
    Governance(#[from] review_governance::ReviewGovernanceError),
    #[error("analysis read failed: {0}")]
    Analysis(#[from] analysis_compare::AnalysisCompareError),
    #[error("serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ReviewAgenda {
    pub format: String,
    pub version: u32,
    pub selected_scope: Vec<String>,
    pub pending: HumanPending,
    pub session_dispositions: Vec<AgendaDisposition>,
    pub automatically_ranked: bool,
    pub session_disposition_becomes_future_policy: bool,
    pub agenda_starts_analysis: bool,
    pub agenda_executes_actions: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AgendaDisposition {
    pub target_type: String,
    pub target_id: uuid::Uuid,
    pub disposition: String,
    pub note: String,
    pub remembered_future_policy: bool,
}

pub async fn build(
    pool: &PgPool,
    session: ReviewSessionScope,
    analysis: AnalysisScope,
) -> Result<ReviewAgenda, ReviewAgendaError> {
    let selected_scope = review_governance::current_scope(pool, session).await?;
    let pending = analysis_compare::human_pending(pool, analysis).await?;
    let session_dispositions = review_governance::current_dispositions(pool, session)
        .await?
        .into_iter()
        .map(|d: CurrentDisposition| AgendaDisposition {
            target_type: d.target_type,
            target_id: d.target_id,
            disposition: d.disposition,
            note: d.note,
            remembered_future_policy: d.remembered_future_policy,
        })
        .collect();
    Ok(ReviewAgenda {
        format: "niccolo.review-agenda".into(),
        version: 1,
        selected_scope,
        pending,
        session_dispositions,
        automatically_ranked: false,
        session_disposition_becomes_future_policy: false,
        agenda_starts_analysis: false,
        agenda_executes_actions: false,
    })
}

pub fn canonical_json(agenda: &ReviewAgenda) -> Result<String, ReviewAgendaError> {
    Ok(serde_json::to_string(agenda)?)
}

pub fn sha256(agenda: &ReviewAgenda) -> Result<String, ReviewAgendaError> {
    Ok(format!(
        "{:x}",
        Sha256::digest(canonical_json(agenda)?.as_bytes())
    ))
}
