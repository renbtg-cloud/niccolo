//! Human-readable epistemic read model for the first-party UI.
//! Grouping happens server-side so the browser renders domain state rather than inventing it.
use crate::{analysis_assumptions, analysis_claims, analysis_structure::AnalysisScope, human_agency};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum EpistemicError {
    #[error("epistemic view dependency failed: {0}")]
    Dependency(String),
}

fn dep<E: std::fmt::Display>(e: E) -> EpistemicError {
    EpistemicError::Dependency(e.to_string())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpistemicClaim {
    pub claim_id: Uuid,
    pub ordinal: i32,
    pub kind: String,
    pub text: String,
    pub qualification: Option<String>,
    pub assumptions: Vec<analysis_assumptions::AssumptionView>,
    pub alternatives: Vec<analysis_assumptions::AlternativeView>,
    pub evidence_gaps: Vec<human_agency::EvidenceGapView>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpistemicView {
    pub analysis_id: Uuid,
    pub claims: Vec<EpistemicClaim>,
    pub unlinked_assumptions: Vec<analysis_assumptions::AssumptionView>,
    pub unlinked_evidence_gaps: Vec<human_agency::EvidenceGapView>,
    pub claims_are_verified_facts: bool,
    pub assumptions_are_facts: bool,
    pub alternatives_are_automatically_ranked: bool,
    pub satisfied_gap_proves_claim: bool,
    pub browser_is_domain_authority: bool,
}

pub async fn get(pool: &PgPool, scope: AnalysisScope) -> Result<EpistemicView, EpistemicError> {
    let claims = analysis_claims::list_claims(
        pool,
        scope.workspace,
        scope.situation,
        scope.analysis,
    )
    .await
    .map_err(dep)?;
    let lens = analysis_assumptions::lens(pool, scope).await.map_err(dep)?;
    let gaps = human_agency::evidence_gaps(pool, scope)
        .await
        .map_err(dep)?;

    let grouped = claims
        .into_iter()
        .map(|claim| EpistemicClaim {
            claim_id: claim.claim_id,
            ordinal: claim.ordinal,
            kind: claim.kind,
            text: claim.text,
            qualification: claim.qualification,
            assumptions: lens
                .assumptions
                .iter()
                .filter(|item| item.claim_id == Some(claim.claim_id))
                .cloned()
                .collect(),
            alternatives: lens
                .alternatives
                .iter()
                .filter(|item| item.claim_id == claim.claim_id)
                .cloned()
                .collect(),
            evidence_gaps: gaps
                .iter()
                .filter(|item| item.claim_id == Some(claim.claim_id))
                .cloned()
                .collect(),
        })
        .collect();

    Ok(EpistemicView {
        analysis_id: scope.analysis,
        claims: grouped,
        unlinked_assumptions: lens
            .assumptions
            .into_iter()
            .filter(|item| item.claim_id.is_none())
            .collect(),
        unlinked_evidence_gaps: gaps
            .into_iter()
            .filter(|item| item.claim_id.is_none())
            .collect(),
        claims_are_verified_facts: false,
        assumptions_are_facts: lens.assumptions_are_facts,
        alternatives_are_automatically_ranked: lens.alternatives_are_automatically_ranked,
        satisfied_gap_proves_claim: false,
        browser_is_domain_authority: false,
    })
}
