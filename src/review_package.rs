//! v0.54-v0.56: deterministic portable review package and explicit governance boundaries.
use crate::analysis_actions::{self, AgencyReport};
use crate::analysis_read_models::{self, AttentionSummary, HumanSteeringSummary, ReadModelError};
use crate::analysis_structure::AnalysisScope;
use crate::analysis_timeline::{self, TimelineError, TimelineEvent};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum ReviewPackageError {
    #[error("read model failed: {0}")]
    Read(#[from] ReadModelError),
    #[error("timeline failed: {0}")]
    Timeline(#[from] TimelineError),
    #[error("action report failed: {0}")]
    Action(#[from] analysis_actions::ActionError),
    #[error("serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GovernanceBoundary {
    pub accepted_proposal_means_executed: bool,
    pub human_review_mutates_original_claim: bool,
    pub uncertainty_event_deletes_origin: bool,
    pub context_presence_means_analysis_use: bool,
    pub link_count_means_independent_evidence_count: bool,
    pub system_auto_prioritizes_human_attention: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewPackage {
    pub format: String,
    pub version: u32,
    pub workspace_id: Uuid,
    pub situation_id: Uuid,
    pub analysis_id: Uuid,
    pub agency: AgencyReport,
    pub attention: AttentionSummary,
    pub human_steering: HumanSteeringSummary,
    pub timeline: Vec<TimelineEvent>,
    pub governance: GovernanceBoundary,
}

fn hex(bytes: &[u8]) -> String {
    const H: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(H[(b >> 4) as usize] as char);
        out.push(H[(b & 15) as usize] as char);
    }
    out
}

pub async fn build(
    pool: &PgPool,
    scope: AnalysisScope,
) -> Result<ReviewPackage, ReviewPackageError> {
    let agency = analysis_actions::agency_report(pool, scope).await?;
    let attention = analysis_read_models::attention(pool, scope).await?;
    let human_steering = analysis_read_models::human_steering(pool, scope).await?;
    let timeline = analysis_timeline::list(pool, scope).await?;
    Ok(ReviewPackage {
        format: "niccolo.human-review-package".into(),
        version: 1,
        workspace_id: scope.workspace,
        situation_id: scope.situation,
        analysis_id: scope.analysis,
        agency,
        attention,
        human_steering,
        timeline,
        governance: GovernanceBoundary {
            accepted_proposal_means_executed: false,
            human_review_mutates_original_claim: false,
            uncertainty_event_deletes_origin: false,
            context_presence_means_analysis_use: false,
            link_count_means_independent_evidence_count: false,
            system_auto_prioritizes_human_attention: false,
        },
    })
}

pub async fn to_json(pool: &PgPool, scope: AnalysisScope) -> Result<String, ReviewPackageError> {
    Ok(serde_json::to_string(&build(pool, scope).await?)?)
}
pub async fn sha256(pool: &PgPool, scope: AnalysisScope) -> Result<String, ReviewPackageError> {
    Ok(hex(&Sha256::digest(to_json(pool, scope).await?.as_bytes())))
}
