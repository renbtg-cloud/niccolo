//! v0.551-v0.600: React-oriented aggregate read model.
//! It reduces client orchestration without moving domain authority into the browser.
use crate::{
    agentic_controller, analysis_review,
    analysis_structure::AnalysisScope,
    bff_operations::{self, OperationScope},
    investigation_policy, proactive_briefing, reanalysis_advisor, situation_store,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum DashboardError {
    #[error("dashboard dependency failed: {0}")]
    Dependency(String),
}
fn dep<E: std::fmt::Display>(e: E) -> DashboardError {
    DashboardError::Dependency(e.to_string())
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DashboardAnalysis {
    pub analysis_id: Uuid,
    pub input_revision: i64,
    pub current_revision: i64,
    pub input_is_current: bool,
    pub output_present: bool,
    pub examined_fragment_count: i64,
    pub catalogued_fragment_count: i64,
    pub used_context_count: i64,
    pub input_context_count: i64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dashboard {
    pub situation_id: Uuid,
    pub current_revision: i64,
    pub focus: String,
    pub source_count: usize,
    pub active_context_count: usize,
    pub analysis: DashboardAnalysis,
    pub investigation: investigation_policy::InvestigationQueue,
    pub proactive: proactive_briefing::ProactiveBriefing,
    pub reanalysis: reanalysis_advisor::ReanalysisRecommendation,
    pub operation: Option<bff_operations::OperationView>,
    pub controller: Option<agentic_controller::ControllerView>,
    pub browser_may_render_simple_views: bool,
    pub browser_is_domain_authority: bool,
    pub assembled_atomically: bool,
}
pub async fn get(
    pool: &PgPool,
    scope: AnalysisScope,
    operation: Option<Uuid>,
) -> Result<Dashboard, DashboardError> {
    let situation = situation_store::get_snapshot(pool, scope.workspace, scope.situation, None)
        .await
        .map_err(dep)?;
    let review = analysis_review::get(pool, scope.workspace, scope.situation, scope.analysis)
        .await
        .map_err(dep)?;
    let investigation = investigation_policy::queue(pool, scope, 8)
        .await
        .map_err(dep)?;
    let proactive = proactive_briefing::build(pool, scope).await.map_err(dep)?;
    let reanalysis = reanalysis_advisor::assess(pool, scope).await.map_err(dep)?;
    let (operation, controller) = if let Some(id) = operation {
        let os = OperationScope {
            workspace: scope.workspace,
            situation: scope.situation,
            operation: id,
        };
        let op = bff_operations::get(pool, os).await.map_err(dep)?;
        let ctrl = agentic_controller::view(pool, os).await.ok();
        (Some(op), ctrl)
    } else {
        (None, None)
    };
    Ok(Dashboard {
        situation_id: scope.situation,
        current_revision: situation.revision,
        focus: situation.focus,
        source_count: situation.sources.len(),
        active_context_count: situation.contributions.len(),
        analysis: DashboardAnalysis {
            analysis_id: scope.analysis,
            input_revision: review.input_revision,
            current_revision: review.current_revision,
            input_is_current: review.input_is_current,
            output_present: review.output_present,
            examined_fragment_count: review.examined_fragment_count,
            catalogued_fragment_count: review.catalogued_fragment_count,
            used_context_count: review.used_context_count,
            input_context_count: review.input_context_count,
        },
        investigation,
        proactive,
        reanalysis,
        operation,
        controller,
        browser_may_render_simple_views: true,
        browser_is_domain_authority: false,
        assembled_atomically: false,
    })
}
