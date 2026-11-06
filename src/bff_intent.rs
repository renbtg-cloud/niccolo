//! Human-facing BFF intent layer.
//!
//! The browser expresses what the human is trying to do. This module translates
//! that intent into the durable Niccolo analysis/operation model without moving
//! domain semantics into React or creating separate sync/async implementations.
use crate::analysis_snapshot::{self, AnalysisInputSnapshot};
use crate::bff_operations::{self, NewOperation, OperationScope, OperationView};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HumanIntent {
    AboutToActOrSay,
    Understand,
    FigureOutWhatMatters,
}

impl HumanIntent {
    pub fn default_goal(self) -> &'static str {
        match self {
            Self::AboutToActOrSay => {
                "Help me reason carefully about what I am about to say or do before I choose."
            }
            Self::Understand => {
                "Help me understand this Situation, separating evidence, inference, assumptions, and competing interpretations."
            }
            Self::FigureOutWhatMatters => {
                "Help me figure out what matters most in this Situation and what deserves my attention next."
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct IntentRequest {
    pub analysis_id: Uuid,
    pub intent: HumanIntent,
    pub goal: Option<String>,
    pub reanalysis_of: Option<Uuid>,
    pub idempotency_key: Option<String>,
    pub step_budget: Option<i32>,
    #[serde(default)]
    pub context: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct IntentLaunch {
    pub analysis_id: Uuid,
    pub intent: HumanIntent,
    pub analysis_replayed: bool,
    pub operation: OperationView,
}

#[derive(Debug, thiserror::Error)]
pub enum IntentError {
    #[error("invalid human intent request")]
    Invalid,
    #[error("analysis snapshot unavailable: {0}")]
    Analysis(#[from] analysis_snapshot::AnalysisSnapshotError),
    #[error("operation unavailable: {0}")]
    Operation(#[from] bff_operations::BffOperationError),
}

fn cleaned_goal(request: &IntentRequest) -> Result<String, IntentError> {
    let value = request
        .goal
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| request.intent.default_goal());
    if value.len() > 4096 || value.chars().any(char::is_control) {
        return Err(IntentError::Invalid);
    }
    Ok(value.to_owned())
}

fn operation_input(request: &IntentRequest, goal: &str) -> Value {
    json!({
        "human_intent": request.intent,
        "goal": goal,
        "context": request.context,
        "human_owns_project": true,
        "automatic_external_action_allowed": false
    })
}

/// Prepare one human-facing Niccolo task. Sync and async delivery surfaces call
/// this same function and differ only in whether they wait for execution.
pub async fn submit(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    operation: Uuid,
    actor: Uuid,
    request: &IntentRequest,
) -> Result<IntentLaunch, IntentError> {
    if request.analysis_id == operation || request.reanalysis_of == Some(request.analysis_id) {
        return Err(IntentError::Invalid);
    }
    let goal = cleaned_goal(request)?;
    let analysis: AnalysisInputSnapshot = analysis_snapshot::begin(
        pool,
        workspace,
        actor,
        situation,
        request.analysis_id,
        &goal,
        request.reanalysis_of,
    )
    .await?;
    let scope = OperationScope {
        workspace,
        situation,
        operation,
    };
    let operation = bff_operations::submit(
        pool,
        scope,
        &NewOperation {
            kind: "AGENTIC_ANALYSIS".to_owned(),
            target_analysis_id: Some(request.analysis_id),
            parent_operation_id: None,
            idempotency_key: request.idempotency_key.clone(),
            input: operation_input(request, &goal),
            deadline_at: None,
            step_budget: request.step_budget.unwrap_or(32),
            requested_by: actor,
        },
    )
    .await?;
    Ok(IntentLaunch {
        analysis_id: analysis.analysis_id,
        intent: request.intent,
        analysis_replayed: analysis.replayed,
        operation,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_public_intents_have_nonempty_human_goals() {
        for intent in [
            HumanIntent::AboutToActOrSay,
            HumanIntent::Understand,
            HumanIntent::FigureOutWhatMatters,
        ] {
            assert!(!intent.default_goal().trim().is_empty());
        }
    }

    #[test]
    fn intent_serialization_is_stable_for_the_bff_contract() {
        assert_eq!(
            serde_json::to_string(&HumanIntent::FigureOutWhatMatters).unwrap(),
            "\"FIGURE_OUT_WHAT_MATTERS\""
        );
    }
}
