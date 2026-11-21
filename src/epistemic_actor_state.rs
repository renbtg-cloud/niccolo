//! Attributed, independent epistemic-state descriptions. Nothing implies another state.
use super::{
    WorkbenchError, assessment::evidence_visible, object_available_in_situation, scoped, valid_text,
};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StateDimension {
    Exposure,
    Access,
    Perception,
    Attention,
    Examination,
    Comprehension,
    Recall,
    Suspicion,
    Inclination,
    Doubt,
    Belief,
    SuspendedJudgment,
    ClaimedKnowledge,
    PerceivedKnowledge,
    EvidenceSupportedKnowledge,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StateJudgment {
    Indicated,
    NotIndicated,
    Unknown,
    Contested,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateInput {
    pub subject_actor_id: Uuid,
    pub target_object_id: Uuid,
    pub dimension: StateDimension,
    pub judgment: StateJudgment,
    pub context: String,
    pub temporal_scope: String,
    pub rationale: String,
    pub basis_evidence_id: Option<Uuid>,
    pub revises_event_id: Option<Uuid>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateEvent {
    pub event_id: Uuid,
    pub input: StateInput,
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
type StateRow = (
    Uuid,
    Uuid,
    Uuid,
    String,
    String,
    String,
    String,
    String,
    Option<Uuid>,
    Option<Uuid>,
    Uuid,
    String,
);
async fn list(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    situation: Uuid,
    event: Option<Uuid>,
) -> Result<Vec<StateEvent>, WorkbenchError> {
    let rows:Vec<StateRow>=sqlx::query_as("SELECT event_id,subject_actor_id,target_object_id,dimension,judgment,context,temporal_scope,rationale,basis_evidence_id,revises_event_id,recorded_by_principal_id,created_at::text FROM niccolo.actor_epistemic_state_events WHERE workspace_id=$1 AND situation_id=$2 AND ($3::uuid IS NULL OR event_id=$3) ORDER BY event_no").bind(workspace).bind(situation).bind(event).fetch_all(&mut **tx).await?;
    rows.into_iter()
        .map(
            |(
                event_id,
                subject_actor_id,
                target_object_id,
                dimension,
                judgment,
                context,
                temporal_scope,
                rationale,
                basis_evidence_id,
                revises_event_id,
                recorded_by,
                recorded_at,
            )| {
                Ok(StateEvent {
                    event_id,
                    input: StateInput {
                        subject_actor_id,
                        target_object_id,
                        dimension: parse(dimension)?,
                        judgment: parse(judgment)?,
                        context,
                        temporal_scope,
                        rationale,
                        basis_evidence_id,
                        revises_event_id,
                    },
                    recorded_by,
                    recorded_at,
                    replayed: false,
                })
            },
        )
        .collect()
}
pub(super) async fn list_in_snapshot(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    situation: Uuid,
) -> Result<Vec<StateEvent>, WorkbenchError> {
    list(tx, workspace, situation, None).await
}

pub async fn record(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    recorder: Uuid,
    event: Uuid,
    mut input: StateInput,
) -> Result<StateEvent, WorkbenchError> {
    if event.is_nil()
        || input.revises_event_id == Some(event)
        || !valid_text(&input.context, 2048)
        || !valid_text(&input.temporal_scope, 2048)
        || !valid_text(&input.rationale, 4096)
        || (input.dimension == StateDimension::EvidenceSupportedKnowledge
            && input.judgment == StateJudgment::Indicated
            && input.basis_evidence_id.is_none())
    {
        return Err(WorkbenchError::Invalid);
    }
    input.context = input.context.trim().to_owned();
    input.temporal_scope = input.temporal_scope.trim().to_owned();
    input.rationale = input.rationale.trim().to_owned();
    let mut tx = scoped(pool, workspace, false).await?;
    if let Some(mut prior) = list(&mut tx, workspace, situation, Some(event))
        .await?
        .pop()
    {
        if prior.recorded_by != recorder || prior.input != input {
            return Err(WorkbenchError::Conflict);
        }
        prior.replayed = true;
        tx.commit().await?;
        return Ok(prior);
    }
    let actor_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM niccolo.actors WHERE workspace_id=$1 AND actor_id=$2)",
    )
    .bind(workspace)
    .bind(input.subject_actor_id)
    .fetch_one(&mut *tx)
    .await?;
    let target =
        object_available_in_situation(&mut tx, workspace, situation, input.target_object_id)
            .await?;
    if !actor_exists
        || !target.as_deref().is_some_and(|t| {
            matches!(
                t,
                "PROPOSITION"
                    | "HYPOTHESIS"
                    | "ASSUMPTION"
                    | "QUESTION"
                    | "INFORMATION_GAP"
                    | "FINDING"
            )
        })
    {
        return Err(WorkbenchError::Invisible);
    }
    if let Some(basis) = input.basis_evidence_id {
        if !evidence_visible(&mut tx, workspace, situation, basis).await? {
            return Err(WorkbenchError::Invisible);
        }
    }
    if let Some(prior_id) = input.revises_event_id {
        let prior = list(&mut tx, workspace, situation, Some(prior_id))
            .await?
            .pop()
            .ok_or(WorkbenchError::Invisible)?;
        if prior.recorded_by != recorder
            || prior.input.subject_actor_id != input.subject_actor_id
            || prior.input.target_object_id != input.target_object_id
            || prior.input.dimension != input.dimension
            || prior.input.context != input.context
            || prior.input.temporal_scope != input.temporal_scope
        {
            return Err(WorkbenchError::Invalid);
        }
    }
    let inserted=sqlx::query("INSERT INTO niccolo.actor_epistemic_state_events(workspace_id,situation_id,event_id,subject_actor_id,target_object_id,dimension,judgment,context,temporal_scope,rationale,basis_evidence_id,revises_event_id,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13) ON CONFLICT DO NOTHING").bind(workspace).bind(situation).bind(event).bind(input.subject_actor_id).bind(input.target_object_id).bind(code(input.dimension)).bind(code(input.judgment)).bind(&input.context).bind(&input.temporal_scope).bind(&input.rationale).bind(input.basis_evidence_id).bind(input.revises_event_id).bind(recorder).execute(&mut *tx).await?.rows_affected()==1;
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
