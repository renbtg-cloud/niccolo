//! Descriptions of interventions and subsequent events, not commands or permission grants.
//! An observed event and a causal assessment of that event are separate records.
use super::{WorkbenchError, assessment::evidence_visible, scoped, situation_visible, valid_text};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Characterization {
    Reported,
    Observed,
    Inferred,
    Contested,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RecordKind {
    AuthorizationReport,
    ExistingPlanReport,
    ExecutionOccurrence,
    EventObservation,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InterventionInput {
    pub kind: RecordKind,
    pub linked_decision_id: Option<Uuid>,
    pub characterization: Characterization,
    pub description: String,
    pub event_temporal_scope: String,
    pub acquisition_temporal_scope: String,
    pub basis_evidence_id: Option<Uuid>,
    pub revises_record_id: Option<Uuid>,
}

impl InterventionInput {
    /// Structural validation never verifies an external event or permission.
    pub fn validate(&self, id: Uuid) -> Result<(), WorkbenchError> {
        if id.is_nil()
            || self.linked_decision_id.is_some_and(|i| i.is_nil())
            || self.basis_evidence_id.is_some_and(|i| i.is_nil())
            || self
                .revises_record_id
                .is_some_and(|i| i.is_nil() || i == id)
            || !valid_text(&self.description, 4096)
            || !valid_text(&self.event_temporal_scope, 2048)
            || !valid_text(&self.acquisition_temporal_scope, 2048)
            || (self.characterization == Characterization::Observed
                && self.basis_evidence_id.is_none())
        {
            return Err(WorkbenchError::Invalid);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CausalAssessmentKind {
    PredatedIntervention,
    PossiblyInduced,
    PossiblyIntensified,
    PossiblyIndependent,
    Undetermined,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CausalAssessmentInput {
    pub execution_record_id: Uuid,
    pub event_observation_record_id: Uuid,
    pub assessment: CausalAssessmentKind,
    pub rationale: String,
    pub alternative_explanations: String,
    pub uncertainty: String,
    pub basis_evidence_id: Option<Uuid>,
    pub revises_assessment_id: Option<Uuid>,
}

impl CausalAssessmentInput {
    /// Even a plausible causal account is not an observed causal fact.
    pub fn validate(&self, id: Uuid) -> Result<(), WorkbenchError> {
        if id.is_nil()
            || self.execution_record_id.is_nil()
            || self.event_observation_record_id.is_nil()
            || self.execution_record_id == self.event_observation_record_id
            || self.basis_evidence_id.is_some_and(|i| i.is_nil())
            || self
                .revises_assessment_id
                .is_some_and(|i| i.is_nil() || i == id)
            || !valid_text(&self.rationale, 4096)
            || !valid_text(&self.alternative_explanations, 4096)
            || !valid_text(&self.uncertainty, 4096)
        {
            return Err(WorkbenchError::Invalid);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttributedRecord<T> {
    pub id: Uuid,
    pub input: T,
    pub recorded_by: Uuid,
    pub recorded_at: String,
    pub replayed: bool,
}

pub type InterventionRecord = AttributedRecord<InterventionInput>;
pub type CausalAssessment = AttributedRecord<CausalAssessmentInput>;

fn code<T: Serialize>(value: T) -> String {
    serde_json::to_value(value)
        .expect("closed enum")
        .as_str()
        .expect("enum string")
        .to_owned()
}

type StoredRow = (Uuid, serde_json::Value, Uuid, String);
fn decode<T: serde::de::DeserializeOwned>(
    rows: Vec<StoredRow>,
) -> Result<Vec<AttributedRecord<T>>, WorkbenchError> {
    rows.into_iter()
        .map(|(id, payload, recorded_by, recorded_at)| {
            Ok(AttributedRecord {
                id,
                input: serde_json::from_value(payload).map_err(|_| WorkbenchError::Invalid)?,
                recorded_by,
                recorded_at,
                replayed: false,
            })
        })
        .collect()
}

async fn records(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    situation: Uuid,
    id: Option<Uuid>,
) -> Result<Vec<InterventionRecord>, WorkbenchError> {
    decode(sqlx::query_as("SELECT record_id,to_jsonb(r)-ARRAY['workspace_id','situation_id','record_id','record_no','recorded_by_principal_id','created_at'],recorded_by_principal_id,created_at::text FROM niccolo.epistemic_intervention_records r WHERE workspace_id=$1 AND situation_id=$2 AND ($3::uuid IS NULL OR record_id=$3) ORDER BY record_no").bind(workspace).bind(situation).bind(id).fetch_all(&mut **tx).await?)
}

async fn assessments(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    situation: Uuid,
    id: Option<Uuid>,
) -> Result<Vec<CausalAssessment>, WorkbenchError> {
    decode(sqlx::query_as("SELECT assessment_id,to_jsonb(r)-ARRAY['workspace_id','situation_id','assessment_id','assessment_no','recorded_by_principal_id','created_at'],recorded_by_principal_id,created_at::text FROM niccolo.epistemic_intervention_causal_assessments r WHERE workspace_id=$1 AND situation_id=$2 AND ($3::uuid IS NULL OR assessment_id=$3) ORDER BY assessment_no").bind(workspace).bind(situation).bind(id).fetch_all(&mut **tx).await?)
}

async fn check_scope(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    situation: Uuid,
    evidence: Option<Uuid>,
) -> Result<(), WorkbenchError> {
    if !situation_visible(tx, workspace, situation).await? {
        return Err(WorkbenchError::Invisible);
    }
    if let Some(id) = evidence {
        if !evidence_visible(tx, workspace, situation, id).await? {
            return Err(WorkbenchError::Invisible);
        }
    }
    Ok(())
}

fn check_replay<T: PartialEq>(
    mut stored: AttributedRecord<T>,
    input: &T,
    recorder: Uuid,
    replayed: bool,
) -> Result<AttributedRecord<T>, WorkbenchError> {
    if &stored.input != input || stored.recorded_by != recorder {
        return Err(WorkbenchError::Conflict);
    }
    stored.replayed = replayed;
    Ok(stored)
}

pub async fn list(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
) -> Result<(Vec<InterventionRecord>, Vec<CausalAssessment>), WorkbenchError> {
    let mut tx = scoped(pool, workspace, true).await?;
    check_scope(&mut tx, workspace, situation, None).await?;
    let result = (
        records(&mut tx, workspace, situation, None).await?,
        assessments(&mut tx, workspace, situation, None).await?,
    );
    tx.commit().await?;
    Ok(result)
}

pub async fn record(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    recorder: Uuid,
    id: Uuid,
    input: InterventionInput,
) -> Result<InterventionRecord, WorkbenchError> {
    input.validate(id)?;
    let mut tx = scoped(pool, workspace, false).await?;
    check_scope(&mut tx, workspace, situation, input.basis_evidence_id).await?;
    if let Some(prior) = records(&mut tx, workspace, situation, Some(id))
        .await?
        .pop()
    {
        let result = check_replay(prior, &input, recorder, true)?;
        tx.commit().await?;
        return Ok(result);
    }
    if let Some(decision) = input.linked_decision_id {
        let visible: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM niccolo.epistemic_action_decisions WHERE workspace_id=$1 AND situation_id=$2 AND decision_id=$3)").bind(workspace).bind(situation).bind(decision).fetch_one(&mut *tx).await?;
        if !visible {
            return Err(WorkbenchError::Invisible);
        }
    }
    if let Some(prior) = input.revises_record_id {
        let prior = records(&mut tx, workspace, situation, Some(prior))
            .await?
            .pop()
            .ok_or(WorkbenchError::Invisible)?;
        if prior.recorded_by != recorder
            || prior.input.kind != input.kind
            || prior.input.linked_decision_id != input.linked_decision_id
        {
            return Err(WorkbenchError::Invalid);
        }
    }
    let inserted = sqlx::query("INSERT INTO niccolo.epistemic_intervention_records(workspace_id,situation_id,record_id,kind,linked_decision_id,characterization,description,event_temporal_scope,acquisition_temporal_scope,basis_evidence_id,revises_record_id,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12) ON CONFLICT DO NOTHING").bind(workspace).bind(situation).bind(id).bind(code(input.kind)).bind(input.linked_decision_id).bind(code(input.characterization)).bind(&input.description).bind(&input.event_temporal_scope).bind(&input.acquisition_temporal_scope).bind(input.basis_evidence_id).bind(input.revises_record_id).bind(recorder).execute(&mut *tx).await?.rows_affected()==1;
    let stored = records(&mut tx, workspace, situation, Some(id))
        .await?
        .pop()
        .ok_or(WorkbenchError::Conflict)?;
    let result = check_replay(stored, &input, recorder, !inserted)?;
    tx.commit().await?;
    Ok(result)
}

pub async fn assess(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    recorder: Uuid,
    id: Uuid,
    input: CausalAssessmentInput,
) -> Result<CausalAssessment, WorkbenchError> {
    input.validate(id)?;
    let mut tx = scoped(pool, workspace, false).await?;
    check_scope(&mut tx, workspace, situation, input.basis_evidence_id).await?;
    if let Some(prior) = assessments(&mut tx, workspace, situation, Some(id))
        .await?
        .pop()
    {
        let result = check_replay(prior, &input, recorder, true)?;
        tx.commit().await?;
        return Ok(result);
    }
    for (pin, kind) in [
        (input.execution_record_id, RecordKind::ExecutionOccurrence),
        (
            input.event_observation_record_id,
            RecordKind::EventObservation,
        ),
    ] {
        let pinned = records(&mut tx, workspace, situation, Some(pin))
            .await?
            .pop()
            .ok_or(WorkbenchError::Invisible)?;
        if pinned.input.kind != kind {
            return Err(WorkbenchError::Invalid);
        }
    }
    if let Some(prior) = input.revises_assessment_id {
        let prior = assessments(&mut tx, workspace, situation, Some(prior))
            .await?
            .pop()
            .ok_or(WorkbenchError::Invisible)?;
        if prior.recorded_by != recorder
            || prior.input.execution_record_id != input.execution_record_id
            || prior.input.event_observation_record_id != input.event_observation_record_id
        {
            return Err(WorkbenchError::Invalid);
        }
    }
    let inserted = sqlx::query("INSERT INTO niccolo.epistemic_intervention_causal_assessments(workspace_id,situation_id,assessment_id,execution_record_id,event_observation_record_id,assessment,rationale,alternative_explanations,uncertainty,basis_evidence_id,revises_assessment_id,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12) ON CONFLICT DO NOTHING").bind(workspace).bind(situation).bind(id).bind(input.execution_record_id).bind(input.event_observation_record_id).bind(code(input.assessment)).bind(&input.rationale).bind(&input.alternative_explanations).bind(&input.uncertainty).bind(input.basis_evidence_id).bind(input.revises_assessment_id).bind(recorder).execute(&mut *tx).await?.rows_affected()==1;
    let stored = assessments(&mut tx, workspace, situation, Some(id))
        .await?
        .pop()
        .ok_or(WorkbenchError::Conflict)?;
    let result = check_replay(stored, &input, recorder, !inserted)?;
    tx.commit().await?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observations_need_evidence_but_reports_do_not_establish_authority() {
        let id = Uuid::new_v4();
        let mut record = InterventionInput {
            kind: RecordKind::AuthorizationReport,
            linked_decision_id: None,
            characterization: Characterization::Reported,
            description: "A manager says access was restricted".into(),
            event_temporal_scope: "January".into(),
            acquisition_temporal_scope: "March interview".into(),
            basis_evidence_id: None,
            revises_record_id: None,
        };
        assert!(record.validate(id).is_ok());
        record.characterization = Characterization::Observed;
        assert!(record.validate(id).is_err());
        record.basis_evidence_id = Some(Uuid::new_v4());
        assert!(record.validate(id).is_ok());
        record.revises_record_id = Some(id);
        assert!(record.validate(id).is_err());
    }

    #[test]
    fn later_event_requires_independent_causal_assessment_with_alternatives() {
        let id = Uuid::new_v4();
        let mut assessment = CausalAssessmentInput {
            execution_record_id: Uuid::new_v4(),
            event_observation_record_id: Uuid::new_v4(),
            assessment: CausalAssessmentKind::PossiblyInduced,
            rationale: "The change followed the access restriction".into(),
            alternative_explanations: "Prior career plans or unrelated conditions".into(),
            uncertainty: "Sequence alone cannot establish causation".into(),
            basis_evidence_id: None,
            revises_assessment_id: None,
        };
        assert!(assessment.validate(id).is_ok());
        assessment.alternative_explanations.clear();
        assert!(assessment.validate(id).is_err());
        assessment.alternative_explanations = "Unknown".into();
        assessment.event_observation_record_id = assessment.execution_record_id;
        assert!(assessment.validate(id).is_err());
    }
}
