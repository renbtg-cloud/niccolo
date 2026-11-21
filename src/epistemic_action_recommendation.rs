//! Non-executable recommendations: a recommendation is never a decision or authorization.
//! Validate the domain payload separately from the scoped authoring boundary.
use super::{WorkbenchError, object_available_in_situation, scoped, situation_visible, valid_text};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recommendation {
    pub recommendation_id: Uuid,
    pub input: RecommendationInput,
    pub recorded_by: Uuid,
    pub recorded_at: String,
    pub replayed: bool,
}

async fn list_filtered(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    situation: Uuid,
    recommendation: Option<Uuid>,
) -> Result<Vec<Recommendation>, WorkbenchError> {
    let rows: Vec<(Uuid, String, Uuid, String)> = sqlx::query_as(
        "SELECT recommendation_id,payload::text,recorded_by_principal_id,created_at::text \
         FROM niccolo.epistemic_action_recommendations \
         WHERE workspace_id=$1 AND situation_id=$2 AND ($3::uuid IS NULL OR recommendation_id=$3) \
         ORDER BY recommendation_no",
    )
    .bind(workspace)
    .bind(situation)
    .bind(recommendation)
    .fetch_all(&mut **tx)
    .await?;
    rows.into_iter()
        .map(|(recommendation_id, payload, recorded_by, recorded_at)| {
            let input = serde_json::from_str::<RecommendationInput>(&payload)
                .map_err(|_| WorkbenchError::Invalid)?;
            input.validate()?;
            Ok(Recommendation {
                recommendation_id,
                input,
                recorded_by,
                recorded_at,
                replayed: false,
            })
        })
        .collect()
}

pub async fn list(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
) -> Result<Vec<Recommendation>, WorkbenchError> {
    let mut tx = scoped(pool, workspace, true).await?;
    if !situation_visible(&mut tx, workspace, situation).await? {
        return Err(WorkbenchError::Invisible);
    }
    let result = list_filtered(&mut tx, workspace, situation, None).await?;
    tx.commit().await?;
    Ok(result)
}

pub(super) async fn list_in_snapshot(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    situation: Uuid,
) -> Result<Vec<Recommendation>, WorkbenchError> {
    list_filtered(tx, workspace, situation, None).await
}

pub async fn record(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    recorder: Uuid,
    recommendation: Uuid,
    input: RecommendationInput,
) -> Result<Recommendation, WorkbenchError> {
    input.validate()?;
    if recommendation.is_nil() || input.revises_recommendation_id == Some(recommendation) {
        return Err(WorkbenchError::Invalid);
    }
    let mut tx = scoped(pool, workspace, false).await?;
    if !situation_visible(&mut tx, workspace, situation).await? {
        return Err(WorkbenchError::Invisible);
    }
    if let Some(mut existing) = list_filtered(&mut tx, workspace, situation, Some(recommendation))
        .await?
        .pop()
    {
        if existing.input != input || existing.recorded_by != recorder {
            return Err(WorkbenchError::Conflict);
        }
        existing.replayed = true;
        tx.commit().await?;
        return Ok(existing);
    }
    let recipient_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM niccolo.actors WHERE workspace_id=$1 AND actor_id=$2)",
    )
    .bind(workspace)
    .bind(input.recipient_actor_id)
    .fetch_one(&mut *tx)
    .await?;
    if !recipient_exists {
        return Err(WorkbenchError::Invisible);
    }
    for ground in &input.ground_object_ids {
        if object_available_in_situation(&mut tx, workspace, situation, *ground)
            .await?
            .is_none()
        {
            return Err(WorkbenchError::Invisible);
        }
    }
    if let Some(prior_id) = input.revises_recommendation_id {
        let prior = list_filtered(&mut tx, workspace, situation, Some(prior_id))
            .await?
            .pop()
            .ok_or(WorkbenchError::Invisible)?;
        if prior.input.recipient_actor_id != input.recipient_actor_id {
            return Err(WorkbenchError::Invalid);
        }
    }
    let payload = serde_json::to_string(&input).map_err(|_| WorkbenchError::Invalid)?;
    let inserted = sqlx::query("INSERT INTO niccolo.epistemic_action_recommendations(workspace_id,situation_id,recommendation_id,recipient_actor_id,ground_object_ids,revises_recommendation_id,payload,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,$6,$7::jsonb,$8) ON CONFLICT DO NOTHING")
        .bind(workspace).bind(situation).bind(recommendation).bind(input.recipient_actor_id).bind(&input.ground_object_ids).bind(input.revises_recommendation_id).bind(payload).bind(recorder).execute(&mut *tx).await?.rows_affected() == 1;
    let mut stored = list_filtered(&mut tx, workspace, situation, Some(recommendation))
        .await?
        .pop()
        .ok_or(WorkbenchError::Conflict)?;
    if stored.input != input || stored.recorded_by != recorder {
        return Err(WorkbenchError::Conflict);
    }
    stored.replayed = !inserted;
    tx.commit().await?;
    Ok(stored)
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
