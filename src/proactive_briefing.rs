//! v1.601-v1.700: 2-3 bounded proactive leads for the user.
//! Leads are prioritization prompts, not evidence, findings, or autonomous actions.
use crate::{analysis_structure::AnalysisScope, investigation_policy};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;
#[derive(Debug, thiserror::Error)]
pub enum BriefingError {
    #[error("proactive briefing unavailable: {0}")]
    Dependency(String),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProactiveLead {
    pub id: Uuid,
    pub kind: String,
    pub text: String,
    pub why_now: String,
    pub information_value_score: u16,
    pub lead_is_evidence: bool,
    pub lead_establishes_truth: bool,
    pub user_may_ignore: bool,
    pub system_may_execute_external_action: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProactiveBriefing {
    pub leads: Vec<ProactiveLead>,
    pub candidate_count: usize,
    pub truncated: bool,
    pub user_owns_which_lead_to_pursue: bool,
}
pub async fn build(
    pool: &PgPool,
    scope: AnalysisScope,
) -> Result<ProactiveBriefing, BriefingError> {
    let q = investigation_policy::queue(pool, scope, 3)
        .await
        .map_err(|e| BriefingError::Dependency(e.to_string()))?;
    let leads = q
        .items
        .into_iter()
        .map(|x| ProactiveLead {
            id: x.id,
            kind: x.kind,
            text: x.text,
            why_now: x.rationale,
            information_value_score: x.information_value_score,
            lead_is_evidence: false,
            lead_establishes_truth: false,
            user_may_ignore: true,
            system_may_execute_external_action: false,
        })
        .collect();
    Ok(ProactiveBriefing {
        leads,
        candidate_count: q.candidate_count,
        truncated: q.truncated,
        user_owns_which_lead_to_pursue: true,
    })
}
