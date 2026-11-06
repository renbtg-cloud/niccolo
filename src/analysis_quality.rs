//! v0.851-v0.900: deterministic post-analysis completeness signal, not a truth score.
use crate::{
    adversarial_challenge, analysis_assumptions, analysis_claims,
    analysis_structure::AnalysisScope, clarification, human_agency,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
#[derive(Debug, thiserror::Error)]
pub enum QualityError {
    #[error("quality dependency failed: {0}")]
    Dependency(String),
}
fn dep<E: std::fmt::Display>(e: E) -> QualityError {
    QualityError::Dependency(e.to_string())
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct QualityReport {
    pub claim_count: usize,
    pub assumption_count: usize,
    pub alternative_count: usize,
    pub open_evidence_gap_count: usize,
    pub clarification_count: usize,
    pub adversarial_challenge_count: usize,
    pub open_material_challenge_count: usize,
    pub provisional: bool,
    pub human_attention_recommended: bool,
    pub completeness_is_truth_score: bool,
}
pub async fn report(pool: &PgPool, scope: AnalysisScope) -> Result<QualityReport, QualityError> {
    let claims =
        analysis_claims::list_claims(pool, scope.workspace, scope.situation, scope.analysis)
            .await
            .map_err(dep)?;
    let lens = analysis_assumptions::lens(pool, scope).await.map_err(dep)?;
    let gaps = human_agency::evidence_gaps(pool, scope)
        .await
        .map_err(dep)?;
    let (questions, _) = clarification::list(
        pool,
        scope.workspace,
        scope.situation,
        scope.analysis,
        None,
        100,
    )
    .await
    .map_err(dep)?;
    let challenges = adversarial_challenge::list(pool, scope)
        .await
        .map_err(dep)?;
    let open = gaps
        .iter()
        .filter(|g| !matches!(g.state.as_str(), "SATISFIED" | "WAIVED"))
        .count();
    let material = challenges
        .iter()
        .filter(|c| c.open && c.materiality >= 60)
        .count();
    let provisional = open > 0 || !questions.is_empty() || material > 0;
    Ok(QualityReport {
        claim_count: claims.len(),
        assumption_count: lens.assumptions.len(),
        alternative_count: lens.alternatives.len(),
        open_evidence_gap_count: open,
        clarification_count: questions.len(),
        adversarial_challenge_count: challenges.len(),
        open_material_challenge_count: material,
        provisional,
        human_attention_recommended: provisional,
        completeness_is_truth_score: false,
    })
}
