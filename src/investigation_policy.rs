//! v0.451-v0.500: bounded proactive investigation queue.
//! Scores are deterministic scheduling heuristics, never probability or truth.
use crate::{analysis_structure::AnalysisScope, clarification, human_agency, human_handoff};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum InvestigationError {
    #[error("investigation dependency failed: {0}")]
    Dependency(String),
    #[error("invalid investigation budget")]
    Invalid,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvestigationCandidate {
    pub id: Uuid,
    pub kind: String,
    pub text: String,
    pub rationale: String,
    pub information_value_score: u16,
    pub score_is_heuristic: bool,
    pub requires_human_action: bool,
    pub system_may_execute_external_action: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvestigationQueue {
    pub items: Vec<InvestigationCandidate>,
    pub candidate_count: usize,
    pub branch_budget: usize,
    pub truncated: bool,
    pub ranking_establishes_truth: bool,
    pub human_owns_external_actions: bool,
}
fn dep<E: std::fmt::Display>(e: E) -> InvestigationError {
    InvestigationError::Dependency(e.to_string())
}

pub async fn queue(
    pool: &PgPool,
    scope: AnalysisScope,
    branch_budget: usize,
) -> Result<InvestigationQueue, InvestigationError> {
    if !(1..=32).contains(&branch_budget) {
        return Err(InvestigationError::Invalid);
    }
    let gaps = human_agency::evidence_gaps(pool, scope)
        .await
        .map_err(dep)?;
    let conflicts = human_handoff::conflicts(pool, scope).await.map_err(dep)?;
    let requests = human_handoff::source_requests(pool, scope)
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
    let question_values = clarification::values(
        pool,
        scope.workspace,
        scope.situation,
        scope.analysis,
    )
    .await
    .map_err(dep)?
    .into_iter()
    .map(|v| (v.question_id, v))
    .collect::<std::collections::BTreeMap<_, _>>();
    let mut items = Vec::new();
    for g in gaps
        .into_iter()
        .filter(|g| !matches!(g.state.as_str(), "SATISFIED" | "WAIVED"))
    {
        let mut score = 82u16;
        if g.claim_id.is_some() {
            score += 8;
        }
        if g.requested_source_kind.is_some() {
            score += 5;
        }
        items.push(InvestigationCandidate {
            id: g.gap_id,
            kind: "EVIDENCE_GAP".into(),
            text: g.description,
            rationale: g
                .requested_source_kind
                .map(|k| format!("requested source kind: {k}"))
                .unwrap_or_else(|| "missing support explicitly recorded".into()),
            information_value_score: score,
            score_is_heuristic: true,
            requires_human_action: false,
            system_may_execute_external_action: false,
        });
    }
    for c in conflicts {
        items.push(InvestigationCandidate {
            id: c.conflict_id,
            kind: "CONFLICT".into(),
            text: c.description,
            rationale: format!("{} claims are explicitly in conflict", c.claim_ids.len()),
            information_value_score: 90,
            score_is_heuristic: true,
            requires_human_action: false,
            system_may_execute_external_action: false,
        });
    }
    for r in requests.into_iter().filter(|r| r.state != "DECLINED") {
        items.push(InvestigationCandidate {
            id: r.request_id,
            kind: "SOURCE_REQUEST".into(),
            text: r.text,
            rationale: r.rationale,
            information_value_score: 88,
            score_is_heuristic: true,
            requires_human_action: true,
            system_may_execute_external_action: false,
        });
    }
    for q in questions {
        let (score, rationale) = if let Some(value) = question_values.get(&q.question_id) {
            let hypothesis_component =
                (value.discriminates_interpretation_ids.len().min(3) as u16) * 10;
            let outcome_component = if value.possible_outcome_count >= 2 { 10 } else { 0 };
            let decision_component = value.decision_relevance.unwrap_or(0) / 3;
            let cost_penalty = value.acquisition_cost.unwrap_or(0) / 4;
            let alternative_penalty = if value.cheaper_alternative.is_some() { 8 } else { 0 };
            let score = 45u16
                .saturating_add(hypothesis_component)
                .saturating_add(outcome_component)
                .saturating_add(decision_component)
                .saturating_sub(cost_penalty)
                .saturating_sub(alternative_penalty)
                .min(100);
            (
                score,
                format!(
                    "{}; discriminates {} interpretation(s), {} possible outcome class(es), decision relevance {:?}, acquisition cost {:?}{}",
                    q.reason,
                    value.discriminates_interpretation_ids.len(),
                    value.possible_outcome_count,
                    value.decision_relevance,
                    value.acquisition_cost,
                    value.cheaper_alternative.as_ref().map(|x| format!(", cheaper alternative: {x}")).unwrap_or_default()
                ),
            )
        } else {
            (86, q.reason)
        };
        items.push(InvestigationCandidate {
            id: q.question_id,
            kind: "CLARIFICATION".into(),
            text: q.text,
            rationale,
            information_value_score: score,
            score_is_heuristic: true,
            requires_human_action: true,
            system_may_execute_external_action: false,
        });
    }
    items.sort_by(|a, b| {
        b.information_value_score
            .cmp(&a.information_value_score)
            .then_with(|| a.kind.cmp(&b.kind))
            .then_with(|| a.id.cmp(&b.id))
    });
    let candidate_count = items.len();
    let truncated = candidate_count > branch_budget;
    items.truncate(branch_budget);
    Ok(InvestigationQueue {
        items,
        candidate_count,
        branch_budget,
        truncated,
        ranking_establishes_truth: false,
        human_owns_external_actions: true,
    })
}
