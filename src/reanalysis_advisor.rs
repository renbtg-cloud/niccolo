//! v0.501-v0.550: material-change advisor for explicit, human-approved reanalysis.
use crate::{analysis_delta, analysis_structure::AnalysisScope, reanalysis_control};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum AdvisorError {
    #[error("reanalysis advisor dependency failed: {0}")]
    Dependency(String),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReanalysisRecommendation {
    pub recommended: bool,
    pub materiality_score: u16,
    pub reasons: Vec<String>,
    pub input_revision: i64,
    pub current_revision: i64,
    pub old_output_must_be_preserved: bool,
    pub human_approval_required: bool,
    pub recommendation_starts_analysis: bool,
}
fn dep<E: std::fmt::Display>(e: E) -> AdvisorError {
    AdvisorError::Dependency(e.to_string())
}

pub async fn assess(
    pool: &PgPool,
    scope: AnalysisScope,
) -> Result<ReanalysisRecommendation, AdvisorError> {
    let d = analysis_delta::get(pool, scope.workspace, scope.situation, scope.analysis)
        .await
        .map_err(dep)?;
    let mut score = 0u16;
    let mut reasons = Vec::new();
    if d.focus_before != d.focus_now {
        score += 45;
        reasons.push("focus changed after frozen input".into());
    }
    if !d.added_sources.is_empty() {
        score += 30;
        reasons.push(format!("{} source(s) added", d.added_sources.len()));
    }
    if !d.removed_sources.is_empty() {
        score += 35;
        reasons.push(format!("{} source(s) removed", d.removed_sources.len()));
    }
    if !d.added_contributions.is_empty() {
        score += 20;
        reasons.push(format!(
            "{} human context contribution(s) added",
            d.added_contributions.len()
        ));
    }
    if !d.removed_contributions.is_empty() {
        score += 35;
        reasons.push(format!(
            "{} frozen contribution(s) corrected/retracted",
            d.removed_contributions.len()
        ));
    }
    score = score.min(100);
    Ok(ReanalysisRecommendation {
        recommended: score >= 20,
        materiality_score: score,
        reasons,
        input_revision: d.input_revision,
        current_revision: d.current_revision,
        old_output_must_be_preserved: true,
        human_approval_required: true,
        recommendation_starts_analysis: false,
    })
}

pub async fn persist_suggestion(
    pool: &PgPool,
    scope: AnalysisScope,
    suggestion_id: Uuid,
    actor: Uuid,
    recommendation: &ReanalysisRecommendation,
) -> Result<(), AdvisorError> {
    if !recommendation.recommended {
        return Ok(());
    }
    let reason = format!(
        "Material Situation delta score {}: {}",
        recommendation.materiality_score,
        recommendation.reasons.join("; ")
    );
    reanalysis_control::record_suggestion(
        pool,
        scope,
        &reanalysis_control::ReanalysisSuggestion {
            suggestion_id,
            reason_kind: "CONTEXT_DELTA".into(),
            reason,
            proposed_by: actor,
        },
    )
    .await
    .map_err(dep)
}
