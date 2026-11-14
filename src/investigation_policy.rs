//! v0.451-v0.500: bounded proactive investigation queue.
//! Scores are deterministic scheduling heuristics, never probability or truth.
use crate::{
    analysis_structure::AnalysisScope, clarification, clarification_answer, human_agency,
    human_feedback, human_handoff,
};
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

fn clarification_information_value(value: &clarification::ClarificationValue) -> u16 {
    let hypothesis_component =
        (value.discriminates_interpretation_ids.len().min(3) as u16) * 10;
    let outcome_component = if value.possible_outcome_count >= 2 { 10 } else { 0 };
    let decision_component = value.decision_relevance.unwrap_or(0) / 3;
    let cost_penalty = value.acquisition_cost.unwrap_or(0) / 4;
    let alternative_penalty = if value.cheaper_alternative.is_some() { 8 } else { 0 };
    45u16
        .saturating_add(hypothesis_component)
        .saturating_add(outcome_component)
        .saturating_add(decision_component)
        .saturating_sub(cost_penalty)
        .saturating_sub(alternative_penalty)
        .min(100)
}

// A deferred request is a human steering decision, not an active lead.
// Accepted requests remain actionable until their source is actually obtained;
// accepting a request never claims that evidence was fetched.
fn actionable_source_request(state: &str) -> bool {
    matches!(state, "PROPOSED" | "ACCEPTED" | "REOPENED")
}

// Human steering on an explicit next step must survive ranking. ACCEPTED still
// means "the human wants to pursue this", not "Niccolò executed it".
fn actionable_next_step(state: &str) -> bool {
    matches!(state, "PROPOSED" | "ACCEPTED" | "REOPENED")
}

fn next_step_information_value(kind: &str, state: &str) -> u16 {
    let base = match kind {
        "REANALYZE" => 87,
        "GATHER_EVIDENCE" => 85,
        "ASK_HUMAN" => 83,
        "USER_ACTION" => 78,
        _ => 70,
    };
    base + u16::from(matches!(state, "ACCEPTED" | "REOPENED")) * 3
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
    let next_steps = human_agency::next_steps(pool, scope).await.map_err(dep)?;
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
    // Human answers must not be resurfaced as if the same question were
    // still awaiting an answer. The answer remains attributed context,
    // not an upgrade to verified evidence.
    let answered_question_ids = clarification_answer::answered_question_ids(
        pool,
        scope.workspace,
        scope.situation,
        scope.analysis,
    )
    .await
    .map_err(dep)?;
    // Keep append-only human rejection meaningful, while preserving
    // disagreement: a question stays eligible when any recorded actor's
    // latest position is UNKNOWN or INSUFFICIENT_EVIDENCE.
    let wholly_rejected_question_ids = human_feedback::wholly_rejected_question_ids(
        pool,
        scope.workspace,
        scope.situation,
        scope.analysis,
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
    for r in requests
        .into_iter()
        .filter(|r| actionable_source_request(&r.state))
    {
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
    for step in next_steps
        .steps
        .into_iter()
        .filter(|step| actionable_next_step(&step.state))
    {
        let score = next_step_information_value(&step.kind, &step.state);
        items.push(InvestigationCandidate {
            id: step.step_id,
            kind: "NEXT_STEP".into(),
            text: step.text,
            rationale: format!(
                "{}; human steering state: {}; proposed step kind: {}",
                step.rationale, step.state, step.kind
            ),
            information_value_score: score,
            score_is_heuristic: true,
            requires_human_action: true,
            system_may_execute_external_action: false,
        });
    }
    for q in questions
        .into_iter()
        .filter(|q| {
            !answered_question_ids.contains(&q.question_id)
                && !wholly_rejected_question_ids.contains(&q.question_id)
        })
    {
        let (score, rationale) = if let Some(value) = question_values.get(&q.question_id) {
            let score = clarification_information_value(value);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn value(
        hypotheses: usize,
        outcomes: usize,
        decision_relevance: u16,
        acquisition_cost: u16,
        cheaper: bool,
    ) -> clarification::ClarificationValue {
        clarification::ClarificationValue {
            question_id: Uuid::nil(),
            discriminates_interpretation_ids: (0..hypotheses)
                .map(|i| Uuid::from_u128((i + 1) as u128))
                .collect(),
            possible_outcome_count: outcomes,
            decision_relevance: Some(decision_relevance),
            acquisition_cost: Some(acquisition_cost),
            cheaper_alternative: cheaper.then(|| "Use the already available ledger".into()),
        }
    }

    #[test]
    fn human_next_step_state_controls_proactive_eligibility() {
        for state in ["PROPOSED", "ACCEPTED", "REOPENED"] {
            assert!(actionable_next_step(state), "{state}");
        }
        for state in ["DEFERRED", "DISMISSED", "UNKNOWN", ""] {
            assert!(!actionable_next_step(state), "{state}");
        }
    }

    #[test]
    fn accepted_or_reopened_next_step_gets_small_priority_bump_without_execution_claim() {
        assert_eq!(
            next_step_information_value("GATHER_EVIDENCE", "ACCEPTED"),
            next_step_information_value("GATHER_EVIDENCE", "PROPOSED") + 3
        );
        assert!(next_step_information_value("REANALYZE", "PROPOSED") <= 100);
    }

    #[test]
    fn deferred_and_declined_source_requests_are_not_active_leads() {
        for state in ["PROPOSED", "ACCEPTED", "REOPENED"] {
            assert!(actionable_source_request(state), "{state}");
        }
        for state in ["DEFERRED", "DECLINED", "UNKNOWN", ""] {
            assert!(!actionable_source_request(state), "{state}");
        }
    }

    #[test]
    fn cheap_decision_relevant_discriminator_outranks_generic_baseline() {
        let strong = value(2, 2, 100, 5, false);
        assert!(clarification_information_value(&strong) > 86);
    }

    #[test]
    fn expensive_low_value_question_is_demoted() {
        let weak = value(0, 1, 10, 90, true);
        assert!(clarification_information_value(&weak) < 60);
    }

    #[test]
    fn question_score_is_bounded() {
        let maximal = value(20, 8, 100, 0, false);
        assert_eq!(clarification_information_value(&maximal), 100);
    }
}
