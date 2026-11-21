//! An interpretation affects reasoning only by explicit, version-pinned reliance.
use super::{
    ExpertArgument, GroundingRouteRef, GroundingSetView, ReassessmentDisposition,
    RouteApplicabilityAssessment, WorkbenchError, assessment::downstream_routes,
    inquiry_response::ResponseInterpretationView, scoped, valid_text,
};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InterpretationDependencyInput {
    pub argument_id: Uuid,
    pub grounding_set_id: Uuid,
    pub interpretation_id: Uuid,
    pub revision_id: Uuid,
    pub confirmation_event_id: Option<Uuid>,
    pub rationale: String,
}
impl InterpretationDependencyInput {
    fn route(&self) -> GroundingRouteRef {
        GroundingRouteRef {
            argument_id: self.argument_id,
            grounding_set_id: self.grounding_set_id,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InterpretationDependency {
    pub dependency_id: Uuid,
    pub input: InterpretationDependencyInput,
    pub recorded_by: Uuid,
    pub recorded_at: String,
    pub replayed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InterpretationDependencyView {
    pub dependency: InterpretationDependency,
    pub current_revision_id: Uuid,
    pub current_confirmation_event_id: Option<Uuid>,
    pub interpretation_revised: bool,
    pub meaning_confirmation_changed: bool,
    pub requires_review: bool,
    pub potential_downstream_routes: Vec<GroundingRouteRef>,
    pub latest_review: Option<InterpretationReview>,
    pub review_history: Vec<InterpretationReview>,
    pub downstream_reviews: Vec<InterpretationRouteReviewView>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InterpretationReviewInput {
    pub argument_id: Uuid,
    pub grounding_set_id: Uuid,
    pub reviewed_revision_id: Uuid,
    pub reviewed_confirmation_event_id: Option<Uuid>,
    pub disposition: ReassessmentDisposition,
    pub note: String,
}
impl InterpretationReviewInput {
    fn route(&self) -> GroundingRouteRef {
        GroundingRouteRef {
            argument_id: self.argument_id,
            grounding_set_id: self.grounding_set_id,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InterpretationReview {
    pub review_event_id: Uuid,
    pub dependency_id: Uuid,
    pub input: InterpretationReviewInput,
    pub recorded_by: Uuid,
    pub recorded_at: String,
    pub replayed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InterpretationRouteReviewView {
    pub route: GroundingRouteRef,
    pub latest_review: Option<InterpretationReview>,
    pub review_history: Vec<InterpretationReview>,
    pub requires_review: bool,
}

async fn reviews(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    situation: Uuid,
    id: Option<Uuid>,
) -> Result<Vec<InterpretationReview>, WorkbenchError> {
    type Row = (
        Uuid,
        Uuid,
        Uuid,
        Uuid,
        Uuid,
        Option<Uuid>,
        String,
        String,
        Uuid,
        String,
    );
    let rows:Vec<Row>=sqlx::query_as("SELECT review_event_id,dependency_id,argument_id,grounding_set_id,reviewed_revision_id,reviewed_confirmation_event_id,disposition,note,recorded_by_principal_id,created_at::text FROM niccolo.epistemic_interpretation_dependency_reviews WHERE workspace_id=$1 AND situation_id=$2 AND ($3::uuid IS NULL OR review_event_id=$3) ORDER BY event_no").bind(workspace).bind(situation).bind(id).fetch_all(&mut **tx).await?;
    rows.into_iter()
        .map(
            |(
                review_event_id,
                dependency_id,
                argument_id,
                grounding_set_id,
                reviewed_revision_id,
                reviewed_confirmation_event_id,
                disposition,
                note,
                recorded_by,
                recorded_at,
            )| {
                Ok(InterpretationReview {
                    review_event_id,
                    dependency_id,
                    input: InterpretationReviewInput {
                        argument_id,
                        grounding_set_id,
                        reviewed_revision_id,
                        reviewed_confirmation_event_id,
                        disposition: ReassessmentDisposition::from_db(&disposition)?,
                        note,
                    },
                    recorded_by,
                    recorded_at,
                    replayed: false,
                })
            },
        )
        .collect()
}

pub async fn review(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    actor: Uuid,
    dependency_id: Uuid,
    event: Uuid,
    mut input: InterpretationReviewInput,
) -> Result<InterpretationReview, WorkbenchError> {
    if event.is_nil() || !valid_text(&input.note, 4096) {
        return Err(WorkbenchError::Invalid);
    }
    input.note = input.note.trim().to_owned();
    // Allow exact retries of historical review events without retargeting them.
    let mut tx = scoped(pool, workspace, false).await?;
    if let Some(mut stored) = reviews(&mut tx, workspace, situation, Some(event))
        .await?
        .pop()
    {
        if stored.input != input
            || stored.dependency_id != dependency_id
            || stored.recorded_by != actor
        {
            return Err(WorkbenchError::Conflict);
        }
        stored.replayed = true;
        tx.commit().await?;
        return Ok(stored);
    }
    tx.commit().await?;
    let snapshot = super::snapshot(pool, workspace, situation).await?;
    let view = snapshot
        .interpretation_dependencies
        .iter()
        .find(|v| v.dependency.dependency_id == dependency_id)
        .ok_or(WorkbenchError::Invisible)?;
    if !view.interpretation_revised && !view.meaning_confirmation_changed {
        return Err(WorkbenchError::Invalid);
    }
    if input.reviewed_revision_id != view.current_revision_id
        || input.reviewed_confirmation_event_id != view.current_confirmation_event_id
    {
        return Err(WorkbenchError::Conflict);
    }
    if input.route() != view.dependency.input.route()
        && !view.potential_downstream_routes.contains(&input.route())
    {
        return Err(WorkbenchError::Invalid);
    }
    let mut tx = scoped(pool, workspace, false).await?;
    // A concurrent newer meaning event cannot clear attention: the review remains pinned to the version inspected.
    let inserted=sqlx::query("INSERT INTO niccolo.epistemic_interpretation_dependency_reviews(workspace_id,situation_id,review_event_id,dependency_id,argument_id,grounding_set_id,interpretation_id,reviewed_revision_id,reviewed_confirmation_event_id,disposition,note,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12) ON CONFLICT DO NOTHING").bind(workspace).bind(situation).bind(event).bind(dependency_id).bind(input.argument_id).bind(input.grounding_set_id).bind(view.dependency.input.interpretation_id).bind(input.reviewed_revision_id).bind(input.reviewed_confirmation_event_id).bind(input.disposition.db_value()).bind(&input.note).bind(actor).execute(&mut *tx).await?.rows_affected()==1;
    let mut stored = reviews(&mut tx, workspace, situation, Some(event))
        .await?
        .pop()
        .ok_or(WorkbenchError::Conflict)?;
    if stored.input != input || stored.dependency_id != dependency_id || stored.recorded_by != actor
    {
        return Err(WorkbenchError::Conflict);
    }
    stored.replayed = !inserted;
    tx.commit().await?;
    Ok(stored)
}

async fn list(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    situation: Uuid,
    id: Option<Uuid>,
) -> Result<Vec<InterpretationDependency>, WorkbenchError> {
    type Row = (
        Uuid,
        Uuid,
        Uuid,
        Uuid,
        Uuid,
        Option<Uuid>,
        String,
        Uuid,
        String,
    );
    let rows:Vec<Row>=sqlx::query_as("SELECT dependency_id,argument_id,grounding_set_id,interpretation_id,revision_id,confirmation_event_id,rationale,recorded_by_principal_id,created_at::text FROM niccolo.epistemic_interpretation_dependencies WHERE workspace_id=$1 AND situation_id=$2 AND ($3::uuid IS NULL OR dependency_id=$3) ORDER BY created_at,dependency_id").bind(workspace).bind(situation).bind(id).fetch_all(&mut **tx).await?;
    Ok(rows
        .into_iter()
        .map(
            |(
                dependency_id,
                argument_id,
                grounding_set_id,
                interpretation_id,
                revision_id,
                confirmation_event_id,
                rationale,
                recorded_by,
                recorded_at,
            )| InterpretationDependency {
                dependency_id,
                input: InterpretationDependencyInput {
                    argument_id,
                    grounding_set_id,
                    interpretation_id,
                    revision_id,
                    confirmation_event_id,
                    rationale,
                },
                recorded_by,
                recorded_at,
                replayed: false,
            },
        )
        .collect())
}

pub async fn record(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    actor: Uuid,
    id: Uuid,
    mut input: InterpretationDependencyInput,
) -> Result<InterpretationDependency, WorkbenchError> {
    if id.is_nil() || !valid_text(&input.rationale, 4096) {
        return Err(WorkbenchError::Invalid);
    }
    input.rationale = input.rationale.trim().to_owned();
    let mut tx = scoped(pool, workspace, false).await?;
    let visible:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM niccolo.epistemic_arguments a JOIN niccolo.epistemic_grounding_sets g USING(workspace_id,argument_id) WHERE a.workspace_id=$1 AND a.situation_id=$2 AND a.argument_id=$3 AND g.grounding_set_id=$4) AND EXISTS(SELECT 1 FROM niccolo.epistemic_utterance_interpretations i JOIN niccolo.epistemic_interpretation_revisions r USING(workspace_id,interpretation_id) JOIN niccolo.epistemic_question_responses q ON q.workspace_id=i.workspace_id AND q.utterance_id=i.utterance_id WHERE i.workspace_id=$1 AND q.situation_id=$2 AND i.interpretation_id=$5 AND r.revision_id=$6 AND ($7::uuid IS NULL OR EXISTS(SELECT 1 FROM niccolo.epistemic_interpretation_confirmation_events c WHERE c.workspace_id=i.workspace_id AND c.interpretation_id=i.interpretation_id AND c.revision_id=r.revision_id AND c.event_id=$7)))").bind(workspace).bind(situation).bind(input.argument_id).bind(input.grounding_set_id).bind(input.interpretation_id).bind(input.revision_id).bind(input.confirmation_event_id).fetch_one(&mut *tx).await?;
    if !visible {
        return Err(WorkbenchError::Invisible);
    }
    let inserted=sqlx::query("INSERT INTO niccolo.epistemic_interpretation_dependencies(workspace_id,situation_id,dependency_id,argument_id,grounding_set_id,interpretation_id,revision_id,confirmation_event_id,rationale,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT DO NOTHING").bind(workspace).bind(situation).bind(id).bind(input.argument_id).bind(input.grounding_set_id).bind(input.interpretation_id).bind(input.revision_id).bind(input.confirmation_event_id).bind(&input.rationale).bind(actor).execute(&mut *tx).await?.rows_affected()==1;
    let mut stored = list(&mut tx, workspace, situation, Some(id))
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

pub(super) async fn views(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    situation: Uuid,
    interpretations: &[ResponseInterpretationView],
    arguments: &[ExpertArgument],
    sets: &[GroundingSetView],
    applicability: &[RouteApplicabilityAssessment],
) -> Result<Vec<InterpretationDependencyView>, WorkbenchError> {
    let mut review_by_route: BTreeMap<(Uuid, GroundingRouteRef), Vec<InterpretationReview>> =
        BTreeMap::new();
    for review in reviews(tx, workspace, situation, None).await? {
        review_by_route
            .entry((review.dependency_id, review.input.route()))
            .or_default()
            .push(review);
    }
    let mut result = vec![];
    for dependency in list(tx, workspace, situation, None).await? {
        let meaning = interpretations
            .iter()
            .find(|v| v.interpretation_id == dependency.input.interpretation_id)
            .ok_or(WorkbenchError::Invisible)?;
        let current_revision_id = meaning.current_revision_id;
        let current_confirmation_event_id = meaning
            .confirmations
            .iter()
            .rev()
            .find(|c| c.revision_id == current_revision_id)
            .map(|c| c.event_id);
        let interpretation_revised = current_revision_id != dependency.input.revision_id;
        let meaning_confirmation_changed = !interpretation_revised
            && current_confirmation_event_id != dependency.input.confirmation_event_id;
        let changed = interpretation_revised || meaning_confirmation_changed;
        let potential_downstream_routes = if changed {
            downstream_routes(dependency.input.route(), arguments, sets, applicability)
        } else {
            vec![]
        };
        let review_view = |route, history: Vec<InterpretationReview>| {
            let latest_review = history
                .iter()
                .rev()
                .find(|r| {
                    r.input.reviewed_revision_id == current_revision_id
                        && r.input.reviewed_confirmation_event_id == current_confirmation_event_id
                })
                .cloned();
            let requires_review = changed
                && latest_review
                    .as_ref()
                    .is_none_or(|r| r.input.disposition != ReassessmentDisposition::Retained);
            InterpretationRouteReviewView {
                route,
                latest_review,
                review_history: history,
                requires_review,
            }
        };
        let direct_route = dependency.input.route();
        let direct = review_view(
            direct_route,
            review_by_route
                .remove(&(dependency.dependency_id, direct_route))
                .unwrap_or_default(),
        );
        let downstream_reviews = potential_downstream_routes
            .iter()
            .map(|r| {
                review_view(
                    *r,
                    review_by_route
                        .remove(&(dependency.dependency_id, *r))
                        .unwrap_or_default(),
                )
            })
            .collect();
        result.push(InterpretationDependencyView {
            dependency,
            current_revision_id,
            current_confirmation_event_id,
            interpretation_revised,
            meaning_confirmation_changed,
            requires_review: direct.requires_review,
            potential_downstream_routes,
            latest_review: direct.latest_review,
            review_history: direct.review_history,
            downstream_reviews,
        });
    }
    Ok(result)
}
