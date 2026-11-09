//! Portable adversarial-review dossier.
//!
//! The dossier is an inspectable package of already-recorded Niccolò state. It does not
//! manufacture a verdict, certify source authenticity, grant authority, or execute actions.
//! It intentionally combines the canonical model-facing packet with the human handoff so a
//! hostile reviewer can inspect observations, hypotheses, missingness, provenance, decisions,
//! reversal conditions and the human-control boundary in one deterministic artifact.

use crate::analysis_structure::AnalysisScope;
use crate::{reasoning_handoff, reasoning_packet};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum DossierError {
    #[error("reasoning packet failed: {0}")]
    Packet(#[from] reasoning_packet::PacketError),
    #[error("reasoning handoff failed: {0}")]
    Handoff(#[from] reasoning_handoff::HandoffError),
    #[error("serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
}

#[derive(Clone, Debug, Serialize)]
pub struct AdversarialDossier {
    pub format: String,
    pub version: u32,
    pub workspace_id: Uuid,
    pub situation_id: Uuid,
    pub analysis_id: Uuid,
    pub reasoning_packet: reasoning_packet::ReasoningPacket,
    pub human_handoff: reasoning_handoff::ReasoningHandoffBundle,
    pub dossier_asserts_verdict: bool,
    pub dossier_asserts_source_authenticity: bool,
    pub dossier_asserts_collection_completeness: bool,
    pub dossier_grants_authority: bool,
    pub dossier_executes_actions: bool,
    pub intended_for_adversarial_review: bool,
}

pub async fn build(pool: &PgPool, scope: AnalysisScope) -> Result<AdversarialDossier, DossierError> {
    let packet = reasoning_packet::build(
        pool,
        AnalysisScope {
            workspace: scope.workspace,
            situation: scope.situation,
            analysis: scope.analysis,
        },
    )
    .await?;
    let handoff = reasoning_handoff::bundle(
        pool,
        AnalysisScope {
            workspace: scope.workspace,
            situation: scope.situation,
            analysis: scope.analysis,
        },
    )
    .await?;

    Ok(AdversarialDossier {
        format: "niccolo.adversarial-dossier".into(),
        version: 1,
        workspace_id: scope.workspace,
        situation_id: scope.situation,
        analysis_id: scope.analysis,
        reasoning_packet: packet,
        human_handoff: handoff,
        dossier_asserts_verdict: false,
        dossier_asserts_source_authenticity: false,
        dossier_asserts_collection_completeness: false,
        dossier_grants_authority: false,
        dossier_executes_actions: false,
        intended_for_adversarial_review: true,
    })
}

pub fn canonical_json(dossier: &AdversarialDossier) -> Result<Vec<u8>, DossierError> {
    Ok(serde_json::to_vec(dossier)?)
}

pub fn sha256(dossier: &AdversarialDossier) -> Result<String, DossierError> {
    Ok(format!("{:x}", Sha256::digest(canonical_json(dossier)?)))
}

