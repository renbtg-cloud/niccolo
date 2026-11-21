//! Explicit expert-authored epistemic assessments. No aggregation, truth
//! promotion, or inference of independence from the assessor's judgment.
use super::{WorkbenchError, object_available_in_situation, scoped, valid_text};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AssessmentDimension {
    Credibility,
    Relevance,
    Reliability,
    Independence,
    DiscriminatoryPower,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AssessmentResult {
    Credible,
    Questionable,
    NotCredible,
    Relevant,
    LimitedRelevance,
    Irrelevant,
    Reliable,
    LimitedReliability,
    Unreliable,
    IndependenceSupported,
    DependenceIndicated,
    Dependent,
    HighPower,
    LimitedPower,
    NoPower,
    Unknown,
    Contested,
}

impl AssessmentDimension {
    pub fn accepts(self, result: AssessmentResult) -> bool {
        use AssessmentResult::*;
        matches!(result, Unknown | Contested)
            || match self {
                Self::Credibility => matches!(result, Credible | Questionable | NotCredible),
                Self::Relevance => matches!(result, Relevant | LimitedRelevance | Irrelevant),
                Self::Reliability => matches!(result, Reliable | LimitedReliability | Unreliable),
                Self::Independence => matches!(
                    result,
                    IndependenceSupported | DependenceIndicated | Dependent
                ),
                Self::DiscriminatoryPower => matches!(result, HighPower | LimitedPower | NoPower),
            }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AssessmentConfidence {
    Low,
    Moderate,
    High,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum AssessmentTarget {
    Object {
        object_id: Uuid,
    },
    Evidence {
        evidence_id: Uuid,
    },
    Relation {
        relation_id: Uuid,
    },
    Route {
        argument_id: Uuid,
        grounding_set_id: Uuid,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum AssessmentGround {
    Object { object_id: Uuid },
    Evidence { evidence_id: Uuid },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssessmentInput {
    pub target: AssessmentTarget,
    pub dimension: AssessmentDimension,
    pub result: AssessmentResult,
    pub confidence: AssessmentConfidence,
    pub criterion: String,
    pub context: String,
    pub rationale: String,
    pub grounds: Vec<AssessmentGround>,
    pub revises_event_id: Option<Uuid>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssessmentEvent {
    pub event_id: Uuid,
    pub input: AssessmentInput,
    pub recorded_by: Uuid,
    pub recorded_at: String,
    pub replayed: bool,
}

fn code<T: Serialize>(value: T) -> String {
    serde_json::to_value(value)
        .expect("closed enum serialization")
        .as_str()
        .expect("closed enum string")
        .to_owned()
}
fn parse<T: for<'de> Deserialize<'de>>(value: String) -> Result<T, WorkbenchError> {
    serde_json::from_value(serde_json::Value::String(value)).map_err(|_| WorkbenchError::Invalid)
}

async fn evidence_visible(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    situation: Uuid,
    id: Uuid,
) -> Result<bool, WorkbenchError> {
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM niccolo.source_fragments f JOIN niccolo.situation_sources s ON s.workspace_id=f.workspace_id AND s.document_id=f.document_id WHERE f.workspace_id=$1 AND s.situation_id=$2 AND f.evidence_id=$3)")
        .bind(workspace).bind(situation).bind(id).fetch_one(&mut **tx).await?)
}

type TargetColumns = (
    Option<Uuid>,
    Option<Uuid>,
    Option<Uuid>,
    Option<Uuid>,
    Option<Uuid>,
);
impl AssessmentTarget {
    fn columns(&self) -> TargetColumns {
        match *self {
            Self::Object { object_id } => (Some(object_id), None, None, None, None),
            Self::Evidence { evidence_id } => (None, Some(evidence_id), None, None, None),
            Self::Relation { relation_id } => (None, None, Some(relation_id), None, None),
            Self::Route {
                argument_id,
                grounding_set_id,
            } => (None, None, None, Some(argument_id), Some(grounding_set_id)),
        }
    }
    fn from_columns(c: TargetColumns) -> Result<Self, WorkbenchError> {
        match c {
            (Some(object_id), None, None, None, None) => Ok(Self::Object { object_id }),
            (None, Some(evidence_id), None, None, None) => Ok(Self::Evidence { evidence_id }),
            (None, None, Some(relation_id), None, None) => Ok(Self::Relation { relation_id }),
            (None, None, None, Some(argument_id), Some(grounding_set_id)) => Ok(Self::Route {
                argument_id,
                grounding_set_id,
            }),
            _ => Err(WorkbenchError::Invalid),
        }
    }
    async fn visible(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        workspace: Uuid,
        situation: Uuid,
    ) -> Result<bool, WorkbenchError> {
        Ok(match *self {
            Self::Object {object_id} => object_available_in_situation(tx,workspace,situation,object_id).await?.is_some(),
            Self::Evidence {evidence_id} => evidence_visible(tx,workspace,situation,evidence_id).await?,
            Self::Relation {relation_id} => sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM niccolo.epistemic_relations WHERE workspace_id=$1 AND situation_id=$2 AND relation_id=$3)").bind(workspace).bind(situation).bind(relation_id).fetch_one(&mut **tx).await?,
            Self::Route {argument_id,grounding_set_id} => sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM niccolo.epistemic_arguments a JOIN niccolo.epistemic_grounding_sets g USING(workspace_id,argument_id) WHERE a.workspace_id=$1 AND a.situation_id=$2 AND a.argument_id=$3 AND g.grounding_set_id=$4)").bind(workspace).bind(situation).bind(argument_id).bind(grounding_set_id).fetch_one(&mut **tx).await?,
        })
    }
}

type EventRow = (
    Uuid,
    Option<Uuid>,
    Option<Uuid>,
    Option<Uuid>,
    Option<Uuid>,
    Option<Uuid>,
    Option<Uuid>,
    String,
    String,
    String,
    String,
    String,
    String,
    Uuid,
    String,
);

pub(super) async fn list_in_snapshot(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    situation: Uuid,
    only: Option<Uuid>,
) -> Result<Vec<AssessmentEvent>, WorkbenchError> {
    let rows: Vec<EventRow> = sqlx::query_as("SELECT event_id,revises_event_id,target_object_id,target_evidence_id,target_relation_id,target_argument_id,target_grounding_set_id,dimension,result,confidence,criterion,context,rationale,recorded_by_principal_id,created_at::text FROM niccolo.epistemic_assessment_events WHERE workspace_id=$1 AND situation_id=$2 AND ($3::uuid IS NULL OR event_id=$3) ORDER BY event_no")
        .bind(workspace).bind(situation).bind(only).fetch_all(&mut **tx).await?;
    let basis: Vec<(Uuid,Option<Uuid>,Option<Uuid>)> = sqlx::query_as("SELECT b.event_id,b.ground_object_id,b.ground_evidence_id FROM niccolo.epistemic_assessment_basis b JOIN niccolo.epistemic_assessment_events e USING(workspace_id,event_id) WHERE b.workspace_id=$1 AND e.situation_id=$2 AND ($3::uuid IS NULL OR b.event_id=$3) ORDER BY b.event_id,b.ordinal")
        .bind(workspace).bind(situation).bind(only).fetch_all(&mut **tx).await?;
    let mut grouped = BTreeMap::<Uuid, Vec<AssessmentGround>>::new();
    for (event_id, obj, evi) in basis {
        let g = match (obj, evi) {
            (Some(object_id), None) => AssessmentGround::Object { object_id },
            (None, Some(evidence_id)) => AssessmentGround::Evidence { evidence_id },
            _ => return Err(WorkbenchError::Invalid),
        };
        grouped.entry(event_id).or_default().push(g);
    }
    rows.into_iter()
        .map(
            |(
                event_id,
                revises_event_id,
                o,
                e,
                r,
                a,
                g,
                dimension,
                result,
                confidence,
                criterion,
                context,
                rationale,
                recorded_by,
                recorded_at,
            )| {
                Ok(AssessmentEvent {
                    event_id,
                    input: AssessmentInput {
                        target: AssessmentTarget::from_columns((o, e, r, a, g))?,
                        dimension: parse(dimension)?,
                        result: parse(result)?,
                        confidence: parse(confidence)?,
                        criterion,
                        context,
                        rationale,
                        grounds: grouped.remove(&event_id).unwrap_or_default(),
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

pub async fn record(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    actor: Uuid,
    event_id: Uuid,
    mut input: AssessmentInput,
) -> Result<AssessmentEvent, WorkbenchError> {
    if event_id.is_nil()
        || input.revises_event_id == Some(event_id)
        || !input.dimension.accepts(input.result)
        || !valid_text(&input.criterion, 2048)
        || !valid_text(&input.context, 2048)
        || !valid_text(&input.rationale, 4096)
        || input.grounds.len() > 64
        || input.grounds.iter().copied().collect::<BTreeSet<_>>().len() != input.grounds.len()
    {
        return Err(WorkbenchError::Invalid);
    }
    input.criterion = input.criterion.trim().to_owned();
    input.context = input.context.trim().to_owned();
    input.rationale = input.rationale.trim().to_owned();
    let mut tx = scoped(pool, workspace, false).await?;
    if !input.target.visible(&mut tx, workspace, situation).await? {
        return Err(WorkbenchError::Invisible);
    }
    for ground in &input.grounds {
        let visible = match *ground {
            AssessmentGround::Object { object_id } => {
                object_available_in_situation(&mut tx, workspace, situation, object_id)
                    .await?
                    .is_some()
            }
            AssessmentGround::Evidence { evidence_id } => {
                evidence_visible(&mut tx, workspace, situation, evidence_id).await?
            }
        };
        if !visible {
            return Err(WorkbenchError::Invisible);
        }
    }
    if let Some(previous) = input.revises_event_id {
        let p = list_in_snapshot(&mut tx, workspace, situation, Some(previous))
            .await?
            .pop()
            .ok_or(WorkbenchError::Invisible)?;
        if p.recorded_by != actor
            || p.input.target != input.target
            || p.input.dimension != input.dimension
            || p.input.criterion != input.criterion
            || p.input.context != input.context
        {
            return Err(WorkbenchError::Invalid);
        }
    }
    let (o, e, r, a, g) = input.target.columns();
    let inserted = sqlx::query("INSERT INTO niccolo.epistemic_assessment_events(workspace_id,situation_id,event_id,revises_event_id,target_object_id,target_evidence_id,target_relation_id,target_argument_id,target_grounding_set_id,dimension,result,confidence,criterion,context,rationale,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16) ON CONFLICT DO NOTHING")
        .bind(workspace).bind(situation).bind(event_id).bind(input.revises_event_id).bind(o).bind(e).bind(r).bind(a).bind(g).bind(code(input.dimension)).bind(code(input.result)).bind(code(input.confidence)).bind(&input.criterion).bind(&input.context).bind(&input.rationale).bind(actor).execute(&mut *tx).await?.rows_affected()==1;
    if inserted {
        for (i, ground) in input.grounds.iter().enumerate() {
            let (o, e) = match *ground {
                AssessmentGround::Object { object_id } => (Some(object_id), None),
                AssessmentGround::Evidence { evidence_id } => (None, Some(evidence_id)),
            };
            sqlx::query("INSERT INTO niccolo.epistemic_assessment_basis(workspace_id,event_id,ordinal,ground_object_id,ground_evidence_id) VALUES($1,$2,$3,$4,$5)").bind(workspace).bind(event_id).bind(i as i32).bind(o).bind(e).execute(&mut *tx).await?;
        }
    }
    let mut stored = list_in_snapshot(&mut tx, workspace, situation, Some(event_id))
        .await?
        .pop()
        .ok_or(WorkbenchError::Conflict)?;
    if stored.input != input || stored.recorded_by != actor {
        return Err(WorkbenchError::Conflict);
    }
    stored.replayed = !inserted;
    tx.commit().await?;
    Ok(stored)
}
