//! Non-executable recommendations: a recommendation is never a decision or authorization.
//! This module validates the domain payload; persistence and visibility checks belong to
//! the authoring boundary, not to this pure value contract.
use super::{WorkbenchError, valid_text};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RecommendationKind {
    PreserveEvidence,
    ReviewPrivileges,
    SeparateDuties,
    IndependentReview,
    VerifyProvenance,
    GatherInformation,
    Delay,
    Inaction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Reversibility {
    Reversible,
    PartiallyReversible,
    Irreversible,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecommendationAlternative {
    pub kind: RecommendationKind,
    pub rationale: String,
    pub expected_consequences: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecommendationInput {
    pub kind: RecommendationKind,
    pub recipient_actor_id: Uuid,
    pub purpose: String,
    pub context: String,
    pub rationale: String,
    pub ground_object_ids: Vec<Uuid>,
    pub conditions: String,
    pub alternatives: Vec<RecommendationAlternative>,
    pub expected_consequences: String,
    pub uncertainty: String,
    pub risk: String,
    pub reversibility: Reversibility,
    pub revises_recommendation_id: Option<Uuid>,
}

impl RecommendationInput {
    /// Checks structure only; it does not attest to the truth, safety or authority of a draft.
    pub fn validate(&self) -> Result<(), WorkbenchError> {
        let text = [
            &self.purpose,
            &self.context,
            &self.rationale,
            &self.conditions,
            &self.expected_consequences,
            &self.uncertainty,
            &self.risk,
        ];
        let grounds = self.ground_object_ids.iter().collect::<BTreeSet<_>>();
        if self.recipient_actor_id.is_nil()
            || self.revises_recommendation_id.is_some_and(|id| id.is_nil())
            || text.iter().any(|value| !valid_text(value, 4096))
            || grounds.is_empty()
            || grounds.len() > 64
            || grounds.len() != self.ground_object_ids.len()
            || self.ground_object_ids.iter().any(Uuid::is_nil)
            || self.alternatives.is_empty()
            || self.alternatives.len() > 8
            || self.alternatives.iter().any(|alternative| {
                alternative.kind == self.kind
                    || !valid_text(&alternative.rationale, 4096)
                    || !valid_text(&alternative.expected_consequences, 4096)
            })
        {
            return Err(WorkbenchError::Invalid);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft() -> RecommendationInput {
        RecommendationInput {
            kind: RecommendationKind::ReviewPrivileges,
            recipient_actor_id: Uuid::new_v4(),
            purpose: "Protect access to records without attributing wrongdoing".into(),
            context: "A possible leak with unresolved attribution".into(),
            rationale: "A structural exposure can exist independently of a leak".into(),
            ground_object_ids: vec![Uuid::new_v4()],
            conditions: "Human review and separate authorization".into(),
            alternatives: vec![RecommendationAlternative {
                kind: RecommendationKind::Inaction,
                rationale: "No changes while evidence is incomplete".into(),
                expected_consequences: "Exposure may persist".into(),
            }],
            expected_consequences: "Improved visibility of privileges".into(),
            uncertainty: "The suspected leak is not established".into(),
            risk: "Review may disclose the investigation".into(),
            reversibility: Reversibility::Reversible,
            revises_recommendation_id: None,
        }
    }

    #[test]
    fn low_confidence_suspicion_does_not_preclude_proportionate_recommendation() {
        assert!(draft().validate().is_ok());
    }

    #[test]
    fn explicit_grounds_and_distinct_alternative_are_required() {
        let mut input = draft();
        input.ground_object_ids.push(input.ground_object_ids[0]);
        assert!(input.validate().is_err());
        input.ground_object_ids.pop();
        input.alternatives[0].kind = input.kind;
        assert!(input.validate().is_err());
    }

    #[test]
    fn authorization_and_execution_cannot_be_smuggled_into_the_payload() {
        let mut json = serde_json::to_value(draft()).unwrap();
        json["authorized"] = true.into();
        assert!(serde_json::from_value::<RecommendationInput>(json).is_err());
        let mut json = serde_json::to_value(draft()).unwrap();
        json["kind"] = "KINETIC_ATTACK".into();
        assert!(serde_json::from_value::<RecommendationInput>(json).is_err());
    }
}
