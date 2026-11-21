//! Explicit reconsideration of a decision's exact assessed basis.
use super::{
    WorkbenchError, assessment::AssessmentEvent, inquiry_decision::InquiryDecision, scoped,
    valid_text,
};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DecisionBasisDisposition {
    Retained,
    ReconsiderationRequired,
    Deferred,
}
impl DecisionBasisDisposition {
    fn code(self) -> &'static str {
        match self {
            Self::Retained => "RETAINED",
            Self::ReconsiderationRequired => "RECONSIDERATION_REQUIRED",
            Self::Deferred => "DEFERRED",
        }
    }
    fn parse(value: &str) -> Result<Self, WorkbenchError> {
        match value {
            "RETAINED" => Ok(Self::Retained),
            "RECONSIDERATION_REQUIRED" => Ok(Self::ReconsiderationRequired),
            "DEFERRED" => Ok(Self::Deferred),
            _ => Err(WorkbenchError::Invalid),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionBasisReviewInput {
    pub original_assessment_event_id: Uuid,
    pub reviewed_assessment_event_id: Uuid,
    pub disposition: DecisionBasisDisposition,
    pub note: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionBasisReview {
    pub review_event_id: Uuid,
    pub decision_id: Uuid,
    pub input: DecisionBasisReviewInput,
    pub recorded_by: Uuid,
    pub recorded_at: String,
    pub replayed: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionBasisChange {
    pub decision_id: Uuid,
    pub original_assessment_event_id: Uuid,
    pub current_assessment_event_id: Uuid,
    pub latest_review: Option<DecisionBasisReview>,
    pub review_history: Vec<DecisionBasisReview>,
    pub requires_review: bool,
}

type ReviewRow = (Uuid, Uuid, Uuid, Uuid, String, String, Uuid, String);
async fn list(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    situation: Uuid,
    event: Option<Uuid>,
) -> Result<Vec<DecisionBasisReview>, WorkbenchError> {
    let rows:Vec<ReviewRow>=sqlx::query_as("SELECT review_event_id,decision_id,original_assessment_event_id,reviewed_assessment_event_id,disposition,note,recorded_by_principal_id,created_at::text FROM niccolo.epistemic_question_decision_reviews WHERE workspace_id=$1 AND situation_id=$2 AND ($3::uuid IS NULL OR review_event_id=$3) ORDER BY review_no").bind(workspace).bind(situation).bind(event).fetch_all(&mut **tx).await?;
    rows.into_iter()
        .map(
            |(
                review_event_id,
                decision_id,
                original_assessment_event_id,
                reviewed_assessment_event_id,
                disposition,
                note,
                recorded_by,
                recorded_at,
            )| {
                Ok(DecisionBasisReview {
                    review_event_id,
                    decision_id,
                    input: DecisionBasisReviewInput {
                        original_assessment_event_id,
                        reviewed_assessment_event_id,
                        disposition: DecisionBasisDisposition::parse(&disposition)?,
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

fn successor(original: Uuid, revisions: &BTreeMap<Uuid, Uuid>) -> Uuid {
    let mut current = original;
    let mut seen = BTreeSet::new();
    while seen.insert(current) {
        let Some(next) = revisions.get(&current) else {
            break;
        };
        current = *next;
    }
    current
}

pub(super) async fn changes_in_snapshot(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    situation: Uuid,
    decisions: &[InquiryDecision],
    assessments: &[AssessmentEvent],
) -> Result<Vec<DecisionBasisChange>, WorkbenchError> {
    let revisions: BTreeMap<_, _> = assessments
        .iter()
        .filter_map(|a| a.input.revises_event_id.map(|prior| (prior, a.event_id)))
        .collect();
    let mut history: BTreeMap<(Uuid, Uuid), Vec<DecisionBasisReview>> = BTreeMap::new();
    for review in list(tx, workspace, situation, None).await? {
        history
            .entry((
                review.decision_id,
                review.input.original_assessment_event_id,
            ))
            .or_default()
            .push(review);
    }
    let mut changes = vec![];
    for decision in decisions {
        for &original in &decision.input.assessment_event_ids {
            let current = successor(original, &revisions);
            if original == current {
                continue;
            }
            let review_history = history
                .remove(&(decision.decision_id, original))
                .unwrap_or_default();
            let latest_review = review_history
                .iter()
                .rev()
                .find(|r| r.input.reviewed_assessment_event_id == current)
                .cloned();
            let requires_review = !latest_review
                .as_ref()
                .is_some_and(|r| r.input.disposition == DecisionBasisDisposition::Retained);
            changes.push(DecisionBasisChange {
                decision_id: decision.decision_id,
                original_assessment_event_id: original,
                current_assessment_event_id: current,
                latest_review,
                review_history,
                requires_review,
            });
        }
    }
    Ok(changes)
}

pub async fn record(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    actor: Uuid,
    decision_id: Uuid,
    event: Uuid,
    mut input: DecisionBasisReviewInput,
) -> Result<DecisionBasisReview, WorkbenchError> {
    if event.is_nil()
        || input.original_assessment_event_id == input.reviewed_assessment_event_id
        || !valid_text(&input.note, 4096)
    {
        return Err(WorkbenchError::Invalid);
    }
    input.note = input.note.trim().to_owned();
    let mut tx = scoped(pool, workspace, false).await?;
    if let Some(mut existing) = list(&mut tx, workspace, situation, Some(event))
        .await?
        .pop()
    {
        if existing.decision_id != decision_id
            || existing.input != input
            || existing.recorded_by != actor
        {
            return Err(WorkbenchError::Conflict);
        }
        existing.replayed = true;
        tx.commit().await?;
        return Ok(existing);
    }
    let basis:Option<Vec<Uuid>>=sqlx::query_scalar("SELECT assessment_event_ids FROM niccolo.epistemic_question_decisions WHERE workspace_id=$1 AND situation_id=$2 AND decision_id=$3").bind(workspace).bind(situation).bind(decision_id).fetch_optional(&mut *tx).await?;
    if !basis
        .ok_or(WorkbenchError::Invisible)?
        .contains(&input.original_assessment_event_id)
    {
        return Err(WorkbenchError::Invalid);
    }
    let current:Option<Uuid>=sqlx::query_scalar("WITH RECURSIVE chain(id) AS (SELECT $3::uuid UNION SELECT a.event_id FROM niccolo.epistemic_assessment_events a JOIN chain c ON a.revises_event_id=c.id WHERE a.workspace_id=$1 AND a.situation_id=$2) SELECT c.id FROM chain c WHERE NOT EXISTS (SELECT 1 FROM niccolo.epistemic_assessment_events a WHERE a.workspace_id=$1 AND a.situation_id=$2 AND a.revises_event_id=c.id)").bind(workspace).bind(situation).bind(input.original_assessment_event_id).fetch_optional(&mut *tx).await?;
    if current != Some(input.reviewed_assessment_event_id) {
        return Err(WorkbenchError::Conflict);
    }
    let inserted=sqlx::query("INSERT INTO niccolo.epistemic_question_decision_reviews(workspace_id,situation_id,review_event_id,decision_id,original_assessment_event_id,reviewed_assessment_event_id,disposition,note,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT DO NOTHING").bind(workspace).bind(situation).bind(event).bind(decision_id).bind(input.original_assessment_event_id).bind(input.reviewed_assessment_event_id).bind(input.disposition.code()).bind(&input.note).bind(actor).execute(&mut *tx).await?.rows_affected()==1;
    let mut result = list(&mut tx, workspace, situation, Some(event))
        .await?
        .pop()
        .ok_or(WorkbenchError::Conflict)?;
    if result.decision_id != decision_id || result.input != input || result.recorded_by != actor {
        return Err(WorkbenchError::Conflict);
    }
    result.replayed = !inserted;
    tx.commit().await?;
    Ok(result)
}
