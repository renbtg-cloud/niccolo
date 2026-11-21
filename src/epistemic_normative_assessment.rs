//! Independent human-authored assessments of norms in a modeled situation.
//! A claimed authorization here never grants Niccolò permissions or determines legality.
use super::{
    WorkbenchError, assessment::evidence_visible, object_available_in_situation, scoped, valid_text,
};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NormativeDimension {
    Applicability,
    Authorization,
    Obligation,
    Prohibition,
    Compliance,
    Violation,
    Precedence,
    Exception,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NormativeJudgment {
    Indicated,
    NotIndicated,
    Unknown,
    Contested,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NormativeInput {
    pub subject_actor_id: Uuid,
    pub action_object_id: Uuid,
    pub norm_object_id: Uuid,
    pub issuer_actor_id: Uuid,
    pub competing_norm_object_id: Option<Uuid>,
    pub competing_issuer_actor_id: Option<Uuid>,
    pub dimension: NormativeDimension,
    pub judgment: NormativeJudgment,
    pub context: String,
    pub temporal_scope: String,
    pub rationale: String,
    pub basis_evidence_id: Option<Uuid>,
    pub revises_event_id: Option<Uuid>,
}

impl NormativeInput {
    pub fn validate(&self, event: Uuid) -> Result<(), WorkbenchError> {
        if [
            event,
            self.subject_actor_id,
            self.action_object_id,
            self.norm_object_id,
            self.issuer_actor_id,
        ]
        .into_iter()
        .any(|id| id.is_nil())
            || [
                self.competing_norm_object_id,
                self.competing_issuer_actor_id,
                self.basis_evidence_id,
                self.revises_event_id,
            ]
            .into_iter()
            .flatten()
            .any(|id| id.is_nil())
            || self.action_object_id == self.norm_object_id
            || self
                .competing_norm_object_id
                .is_some_and(|id| id == self.action_object_id || id == self.norm_object_id)
            || self.competing_norm_object_id.is_some() != self.competing_issuer_actor_id.is_some()
            || matches!(
                self.dimension,
                NormativeDimension::Precedence | NormativeDimension::Exception
            ) != self.competing_norm_object_id.is_some()
            || self.revises_event_id == Some(event)
            || !valid_text(&self.context, 2048)
            || !valid_text(&self.temporal_scope, 2048)
            || !valid_text(&self.rationale, 4096)
        {
            return Err(WorkbenchError::Invalid);
        }
        Ok(())
    }

    fn same_identity(&self, other: &Self) -> bool {
        self.subject_actor_id == other.subject_actor_id
            && self.action_object_id == other.action_object_id
            && self.norm_object_id == other.norm_object_id
            && self.issuer_actor_id == other.issuer_actor_id
            && self.competing_norm_object_id == other.competing_norm_object_id
            && self.competing_issuer_actor_id == other.competing_issuer_actor_id
            && self.dimension == other.dimension
            && self.context == other.context
            && self.temporal_scope == other.temporal_scope
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormativeEvent {
    pub event_id: Uuid,
    pub input: NormativeInput,
    pub recorded_by: Uuid,
    pub recorded_at: String,
    pub replayed: bool,
}

fn code<T: Serialize>(value: T) -> String {
    serde_json::to_value(value)
        .expect("closed enum")
        .as_str()
        .expect("enum string")
        .to_owned()
}
fn parse<T: for<'de> Deserialize<'de>>(value: String) -> Result<T, WorkbenchError> {
    serde_json::from_value(serde_json::Value::String(value)).map_err(|_| WorkbenchError::Invalid)
}

async fn list(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    situation: Uuid,
    event: Option<Uuid>,
) -> Result<Vec<NormativeEvent>, WorkbenchError> {
    let rows = sqlx::query("SELECT event_id,subject_actor_id,action_object_id,norm_object_id,issuer_actor_id,competing_norm_object_id,competing_issuer_actor_id,dimension,judgment,context,temporal_scope,rationale,basis_evidence_id,revises_event_id,recorded_by_principal_id,created_at::text AS recorded_at FROM niccolo.epistemic_normative_assessments WHERE workspace_id=$1 AND situation_id=$2 AND ($3::uuid IS NULL OR event_id=$3) ORDER BY event_no")
        .bind(workspace).bind(situation).bind(event).fetch_all(&mut **tx).await?;
    rows.into_iter()
        .map(|row| {
            Ok(NormativeEvent {
                event_id: row.try_get("event_id")?,
                input: NormativeInput {
                    subject_actor_id: row.try_get("subject_actor_id")?,
                    action_object_id: row.try_get("action_object_id")?,
                    norm_object_id: row.try_get("norm_object_id")?,
                    issuer_actor_id: row.try_get("issuer_actor_id")?,
                    competing_norm_object_id: row.try_get("competing_norm_object_id")?,
                    competing_issuer_actor_id: row.try_get("competing_issuer_actor_id")?,
                    dimension: parse(row.try_get("dimension")?)?,
                    judgment: parse(row.try_get("judgment")?)?,
                    context: row.try_get("context")?,
                    temporal_scope: row.try_get("temporal_scope")?,
                    rationale: row.try_get("rationale")?,
                    basis_evidence_id: row.try_get("basis_evidence_id")?,
                    revises_event_id: row.try_get("revises_event_id")?,
                },
                recorded_by: row.try_get("recorded_by_principal_id")?,
                recorded_at: row.try_get("recorded_at")?,
                replayed: false,
            })
        })
        .collect()
}

pub(super) async fn list_in_snapshot(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    situation: Uuid,
) -> Result<Vec<NormativeEvent>, WorkbenchError> {
    list(tx, workspace, situation, None).await
}

pub async fn record(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    recorder: Uuid,
    event: Uuid,
    mut input: NormativeInput,
) -> Result<NormativeEvent, WorkbenchError> {
    input.validate(event)?;
    input.context = input.context.trim().to_owned();
    input.temporal_scope = input.temporal_scope.trim().to_owned();
    input.rationale = input.rationale.trim().to_owned();
    let mut tx = scoped(pool, workspace, false).await?;
    if let Some(mut existing) = list(&mut tx, workspace, situation, Some(event))
        .await?
        .pop()
    {
        if existing.recorded_by != recorder || existing.input != input {
            return Err(WorkbenchError::Conflict);
        }
        existing.replayed = true;
        tx.commit().await?;
        return Ok(existing);
    }
    for actor in [
        Some(input.subject_actor_id),
        Some(input.issuer_actor_id),
        input.competing_issuer_actor_id,
    ]
    .into_iter()
    .flatten()
    {
        let visible: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM niccolo.actors WHERE workspace_id=$1 AND actor_id=$2)",
        )
        .bind(workspace)
        .bind(actor)
        .fetch_one(&mut *tx)
        .await?;
        if !visible {
            return Err(WorkbenchError::Invisible);
        }
    }
    for object in [
        Some(input.action_object_id),
        Some(input.norm_object_id),
        input.competing_norm_object_id,
    ]
    .into_iter()
    .flatten()
    {
        if !object_available_in_situation(&mut tx, workspace, situation, object)
            .await?
            .as_deref()
            .is_some_and(|kind| {
                matches!(
                    kind,
                    "PROPOSITION" | "HYPOTHESIS" | "ASSUMPTION" | "FINDING"
                )
            })
        {
            return Err(WorkbenchError::Invisible);
        }
    }
    if let Some(basis) = input.basis_evidence_id {
        if !evidence_visible(&mut tx, workspace, situation, basis).await? {
            return Err(WorkbenchError::Invisible);
        }
    }
    if let Some(prior) = input.revises_event_id {
        let previous = list(&mut tx, workspace, situation, Some(prior))
            .await?
            .pop()
            .ok_or(WorkbenchError::Invisible)?;
        if previous.recorded_by != recorder || !previous.input.same_identity(&input) {
            return Err(WorkbenchError::Invalid);
        }
    }
    let inserted = sqlx::query("INSERT INTO niccolo.epistemic_normative_assessments(workspace_id,situation_id,event_id,subject_actor_id,action_object_id,norm_object_id,issuer_actor_id,competing_norm_object_id,competing_issuer_actor_id,dimension,judgment,context,temporal_scope,rationale,basis_evidence_id,revises_event_id,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17) ON CONFLICT DO NOTHING")
        .bind(workspace).bind(situation).bind(event).bind(input.subject_actor_id).bind(input.action_object_id).bind(input.norm_object_id).bind(input.issuer_actor_id).bind(input.competing_norm_object_id).bind(input.competing_issuer_actor_id).bind(code(input.dimension)).bind(code(input.judgment)).bind(&input.context).bind(&input.temporal_scope).bind(&input.rationale).bind(input.basis_evidence_id).bind(input.revises_event_id).bind(recorder).execute(&mut *tx).await?.rows_affected() == 1;
    let mut result = list(&mut tx, workspace, situation, Some(event))
        .await?
        .pop()
        .ok_or(WorkbenchError::Conflict)?;
    if result.recorded_by != recorder || result.input != input {
        return Err(WorkbenchError::Conflict);
    }
    result.replayed = !inserted;
    tx.commit().await?;
    Ok(result)
}
