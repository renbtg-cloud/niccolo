//! Attributed choice among the options on an exact recommendation revision.
//! Recording a choice does not grant authorization, produce a plan, or execute it.
use super::{WorkbenchError, scoped, situation_visible, valid_text};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DecisionDisposition {
    SelectPrimary,
    SelectAlternative,
    Defer,
    Decline,
}

impl DecisionDisposition {
    fn code(self) -> &'static str {
        match self {
            Self::SelectPrimary => "SELECT_PRIMARY",
            Self::SelectAlternative => "SELECT_ALTERNATIVE",
            Self::Defer => "DEFER",
            Self::Decline => "DECLINE",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionInput {
    pub recommendation_id: Uuid,
    pub disposition: DecisionDisposition,
    /// Zero-based index into the immutable alternatives of the pinned recommendation.
    pub alternative_index: Option<u8>,
    pub rationale: String,
    pub revises_decision_id: Option<Uuid>,
}

impl DecisionInput {
    fn validate(&self, decision: Uuid) -> Result<(), WorkbenchError> {
        if decision.is_nil()
            || self.recommendation_id.is_nil()
            || self
                .revises_decision_id
                .is_some_and(|id| id.is_nil() || id == decision)
            || !valid_text(&self.rationale, 4096)
            || (self.disposition == DecisionDisposition::SelectAlternative)
                != self.alternative_index.is_some()
            || self.alternative_index.is_some_and(|index| index >= 8)
        {
            return Err(WorkbenchError::Invalid);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decision {
    pub decision_id: Uuid,
    pub input: DecisionInput,
    pub recorded_by: Uuid,
    pub recorded_at: String,
    pub replayed: bool,
}

type DecisionRow = (
    Uuid,
    Uuid,
    String,
    Option<i32>,
    String,
    Option<Uuid>,
    Uuid,
    String,
);

async fn list_filtered(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    situation: Uuid,
    decision: Option<Uuid>,
) -> Result<Vec<Decision>, WorkbenchError> {
    let rows: Vec<DecisionRow> = sqlx::query_as("SELECT decision_id,recommendation_id,disposition,alternative_index,rationale,revises_decision_id,recorded_by_principal_id,created_at::text FROM niccolo.epistemic_action_decisions WHERE workspace_id=$1 AND situation_id=$2 AND ($3::uuid IS NULL OR decision_id=$3) ORDER BY decision_no")
        .bind(workspace).bind(situation).bind(decision).fetch_all(&mut **tx).await?;
    rows.into_iter()
        .map(
            |(
                decision_id,
                recommendation_id,
                disposition,
                alternative_index,
                rationale,
                revises_decision_id,
                recorded_by,
                recorded_at,
            )| {
                let input = DecisionInput {
                    recommendation_id,
                    disposition: serde_json::from_value(serde_json::Value::String(disposition))
                        .map_err(|_| WorkbenchError::Invalid)?,
                    alternative_index: alternative_index
                        .map(u8::try_from)
                        .transpose()
                        .map_err(|_| WorkbenchError::Invalid)?,
                    rationale,
                    revises_decision_id,
                };
                input.validate(decision_id)?;
                Ok(Decision {
                    decision_id,
                    input,
                    recorded_by,
                    recorded_at,
                    replayed: false,
                })
            },
        )
        .collect()
}

pub async fn list(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
) -> Result<Vec<Decision>, WorkbenchError> {
    let mut tx = scoped(pool, workspace, true).await?;
    if !situation_visible(&mut tx, workspace, situation).await? {
        return Err(WorkbenchError::Invisible);
    }
    let result = list_filtered(&mut tx, workspace, situation, None).await?;
    tx.commit().await?;
    Ok(result)
}

pub async fn record(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    recorder: Uuid,
    decision: Uuid,
    input: DecisionInput,
) -> Result<Decision, WorkbenchError> {
    input.validate(decision)?;
    let mut tx = scoped(pool, workspace, false).await?;
    if !situation_visible(&mut tx, workspace, situation).await? {
        return Err(WorkbenchError::Invisible);
    }
    if let Some(mut prior) = list_filtered(&mut tx, workspace, situation, Some(decision))
        .await?
        .pop()
    {
        if prior.input != input || prior.recorded_by != recorder {
            return Err(WorkbenchError::Conflict);
        }
        prior.replayed = true;
        tx.commit().await?;
        return Ok(prior);
    }
    let count: i32 = sqlx::query_scalar("SELECT jsonb_array_length(payload->'alternatives') FROM niccolo.epistemic_action_recommendations WHERE workspace_id=$1 AND situation_id=$2 AND recommendation_id=$3")
        .bind(workspace).bind(situation).bind(input.recommendation_id).fetch_optional(&mut *tx).await?.ok_or(WorkbenchError::Invisible)?;
    if input
        .alternative_index
        .is_some_and(|index| i32::from(index) >= count)
    {
        return Err(WorkbenchError::Invalid);
    }
    if let Some(prior_id) = input.revises_decision_id {
        let prior = list_filtered(&mut tx, workspace, situation, Some(prior_id))
            .await?
            .pop()
            .ok_or(WorkbenchError::Invisible)?;
        if prior.recorded_by != recorder || prior.input.recommendation_id != input.recommendation_id
        {
            return Err(WorkbenchError::Invalid);
        }
    }
    let inserted = sqlx::query("INSERT INTO niccolo.epistemic_action_decisions(workspace_id,situation_id,decision_id,recommendation_id,disposition,alternative_index,rationale,revises_decision_id,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT DO NOTHING")
        .bind(workspace).bind(situation).bind(decision).bind(input.recommendation_id).bind(input.disposition.code()).bind(input.alternative_index.map(i32::from)).bind(&input.rationale).bind(input.revises_decision_id).bind(recorder).execute(&mut *tx).await?.rows_affected()==1;
    let mut stored = list_filtered(&mut tx, workspace, situation, Some(decision))
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

    #[test]
    fn selection_requires_only_the_corresponding_option_index() {
        let id = Uuid::new_v4();
        let mut input = DecisionInput {
            recommendation_id: Uuid::new_v4(),
            disposition: DecisionDisposition::SelectAlternative,
            alternative_index: Some(0),
            rationale: "Choose additional inquiry".into(),
            revises_decision_id: None,
        };
        assert!(input.validate(id).is_ok());
        input.alternative_index = None;
        assert!(input.validate(id).is_err());
        input.alternative_index = Some(8);
        assert!(input.validate(id).is_err());
        input.alternative_index = Some(0);
        input.disposition = DecisionDisposition::Defer;
        assert!(input.validate(id).is_err());
        input.alternative_index = None;
        assert!(input.validate(id).is_ok());
        input.revises_decision_id = Some(id);
        assert!(input.validate(id).is_err());
    }

    #[test]
    fn decision_cannot_encode_authorization_execution_or_outcome() {
        let input = serde_json::json!({"recommendation_id":Uuid::new_v4(),"disposition":"SELECT_PRIMARY","alternative_index":null,"rationale":"Human choice","revises_decision_id":null});
        assert!(serde_json::from_value::<DecisionInput>(input.clone()).is_ok());
        for key in ["authorization", "execute", "actual_outcome"] {
            let mut invalid = input.clone();
            invalid[key] = true.into();
            assert!(serde_json::from_value::<DecisionInput>(invalid).is_err());
        }
    }
}
