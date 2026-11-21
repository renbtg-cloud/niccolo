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
    Interest,
    Curiosity,
    Indifference,
    Aversion,
    LimitedInquiry,
    DeniabilityPreservation,
    DelegatedKnowing,
    DocumentationAvoidance,
    TrustCompetence,
    TrustConfidentiality,
    TrustTruthfulness,
    Loyalty,
    EgoProtection,
    IdentityProtection,
}
impl StateDimension {
    pub fn is_protective_motive(self) -> bool {
        matches!(self, Self::EgoProtection | Self::IdentityProtection)
    }
    pub fn is_trust(self) -> bool {
        matches!(
            self,
            Self::TrustCompetence | Self::TrustConfidentiality | Self::TrustTruthfulness
        )
    }
    pub fn is_orientation(self) -> bool {
        matches!(
            self,
            Self::Interest
                | Self::Curiosity
                | Self::Indifference
                | Self::Aversion
                | Self::LimitedInquiry
                | Self::DeniabilityPreservation
                | Self::DelegatedKnowing
                | Self::DocumentationAvoidance
        )
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OrientationScope {
    ImmediateReaction,
    PersistentDisposition,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StateJudgment {
    Indicated,
    NotIndicated,
    Unknown,
    Contested,
    DistrustIndicated,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateInput {
    pub subject_actor_id: Uuid,
    pub target_object_id: Option<Uuid>,
    pub target_state_event_id: Option<Uuid>,
    pub target_actor_id: Option<Uuid>,
    pub dimension: StateDimension,
    pub judgment: StateJudgment,
    pub context: String,
    pub temporal_scope: String,
    pub orientation_scope: Option<OrientationScope>,
    pub orientation_goal: Option<String>,
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
    Option<Uuid>,
    Option<Uuid>,
    Option<Uuid>,
    String,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
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
    let rows:Vec<StateRow>=sqlx::query_as("SELECT event_id,subject_actor_id,target_object_id,target_state_event_id,target_actor_id,dimension,judgment,context,temporal_scope,orientation_scope,orientation_goal,rationale,basis_evidence_id,revises_event_id,recorded_by_principal_id,created_at::text FROM niccolo.actor_epistemic_state_events WHERE workspace_id=$1 AND situation_id=$2 AND ($3::uuid IS NULL OR event_id=$3) ORDER BY event_no").bind(workspace).bind(situation).bind(event).fetch_all(&mut **tx).await?;
    rows.into_iter()
        .map(
            |(
                event_id,
                subject_actor_id,
                target_object_id,
                target_state_event_id,
                target_actor_id,
                dimension,
                judgment,
                context,
                temporal_scope,
                orientation_scope,
                orientation_goal,
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
                        target_state_event_id,
                        target_actor_id,
                        dimension: parse(dimension)?,
                        judgment: parse(judgment)?,
                        context,
                        temporal_scope,
                        orientation_scope: orientation_scope.map(parse).transpose()?,
                        orientation_goal,
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
        || [
            input.target_object_id,
            input.target_state_event_id,
            input.target_actor_id,
        ]
        .into_iter()
        .flatten()
        .count()
            != 1
        || input.target_state_event_id == Some(event)
        || (input.dimension.is_trust() && input.target_actor_id.is_none())
        || (input.dimension.is_protective_motive() && input.target_object_id.is_none())
        || (input.dimension == StateDimension::Loyalty && input.target_state_event_id.is_some())
        || (!input.dimension.is_trust()
            && input.dimension != StateDimension::Loyalty
            && input.target_actor_id.is_some())
        || (input.judgment == StateJudgment::DistrustIndicated && !input.dimension.is_trust())
        || !valid_text(&input.context, 2048)
        || !valid_text(&input.temporal_scope, 2048)
        || !valid_text(&input.rationale, 4096)
        || (input.dimension.is_orientation()
            && (input.orientation_scope.is_none()
                || !input
                    .orientation_goal
                    .as_deref()
                    .is_some_and(|goal| valid_text(goal, 2048))))
        || (!input.dimension.is_orientation()
            && (input.orientation_scope.is_some() || input.orientation_goal.is_some()))
        || (input.dimension == StateDimension::EvidenceSupportedKnowledge
            && input.judgment == StateJudgment::Indicated
            && input.basis_evidence_id.is_none())
    {
        return Err(WorkbenchError::Invalid);
    }
    input.context = input.context.trim().to_owned();
    input.temporal_scope = input.temporal_scope.trim().to_owned();
    input.rationale = input.rationale.trim().to_owned();
    input.orientation_goal = input.orientation_goal.map(|goal| goal.trim().to_owned());
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
    let target_visible = if let Some(object_id) = input.target_object_id {
        object_available_in_situation(&mut tx, workspace, situation, object_id)
            .await?
            .as_deref()
            .is_some_and(|t| {
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
    } else if let Some(target_actor) = input.target_actor_id {
        sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM niccolo.actors WHERE workspace_id=$1 AND actor_id=$2)",
        )
        .bind(workspace)
        .bind(target_actor)
        .fetch_one(&mut *tx)
        .await?
    } else {
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM niccolo.actor_epistemic_state_events WHERE workspace_id=$1 AND situation_id=$2 AND event_id=$3)").bind(workspace).bind(situation).bind(input.target_state_event_id).fetch_one(&mut *tx).await?
    };
    if !actor_exists || !target_visible {
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
            || prior.input.target_state_event_id != input.target_state_event_id
            || prior.input.target_actor_id != input.target_actor_id
            || prior.input.dimension != input.dimension
            || prior.input.context != input.context
            || prior.input.temporal_scope != input.temporal_scope
            || prior.input.orientation_scope != input.orientation_scope
            || prior.input.orientation_goal != input.orientation_goal
        {
            return Err(WorkbenchError::Invalid);
        }
    }
    let inserted=sqlx::query("INSERT INTO niccolo.actor_epistemic_state_events(workspace_id,situation_id,event_id,subject_actor_id,target_object_id,target_state_event_id,target_actor_id,dimension,judgment,context,temporal_scope,rationale,basis_evidence_id,revises_event_id,recorded_by_principal_id,orientation_scope,orientation_goal) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17) ON CONFLICT DO NOTHING").bind(workspace).bind(situation).bind(event).bind(input.subject_actor_id).bind(input.target_object_id).bind(input.target_state_event_id).bind(input.target_actor_id).bind(code(input.dimension)).bind(code(input.judgment)).bind(&input.context).bind(&input.temporal_scope).bind(&input.rationale).bind(input.basis_evidence_id).bind(input.revises_event_id).bind(recorder).bind(input.orientation_scope.map(code)).bind(&input.orientation_goal).execute(&mut *tx).await?.rows_affected()==1;
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
