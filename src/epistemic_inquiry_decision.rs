//! A human-authored investigation decision and the exact judgments consulted.
//! No decision in this ledger executes a question, authorizes access or resolves a gap.
use super::{WorkbenchError, scoped, valid_text};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use std::collections::BTreeSet;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InquiryDisposition {
    PursueNext,
    Defer,
    DoNotPursue,
}

impl InquiryDisposition {
    fn code(self) -> &'static str {
        match self {
            Self::PursueNext => "PURSUE_NEXT",
            Self::Defer => "DEFER",
            Self::DoNotPursue => "DO_NOT_PURSUE",
        }
    }

    fn parse(code: &str) -> Result<Self, WorkbenchError> {
        match code {
            "PURSUE_NEXT" => Ok(Self::PursueNext),
            "DEFER" => Ok(Self::Defer),
            "DO_NOT_PURSUE" => Ok(Self::DoNotPursue),
            _ => Err(WorkbenchError::Invalid),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InquiryDecisionInput {
    pub question_id: Uuid,
    pub considered_question_ids: Vec<Uuid>,
    pub assessment_event_ids: Vec<Uuid>,
    pub disposition: InquiryDisposition,
    pub rationale: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InquiryDecision {
    pub decision_id: Uuid,
    pub input: InquiryDecisionInput,
    pub recorded_by: Uuid,
    pub recorded_at: String,
    pub replayed: bool,
}

type DecisionRow = (
    Uuid,
    Uuid,
    Vec<Uuid>,
    Vec<Uuid>,
    String,
    String,
    Uuid,
    String,
);

async fn list_filtered(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    situation: Uuid,
    decision: Option<Uuid>,
) -> Result<Vec<InquiryDecision>, WorkbenchError> {
    let rows: Vec<DecisionRow> = sqlx::query_as("SELECT decision_id,question_id,considered_question_ids,assessment_event_ids,disposition,rationale,recorded_by_principal_id,created_at::text FROM niccolo.epistemic_question_decisions WHERE workspace_id=$1 AND situation_id=$2 AND ($3::uuid IS NULL OR decision_id=$3) ORDER BY decision_no")
        .bind(workspace).bind(situation).bind(decision).fetch_all(&mut **tx).await?;
    rows.into_iter()
        .map(
            |(
                decision_id,
                question_id,
                considered_question_ids,
                assessment_event_ids,
                disposition,
                rationale,
                recorded_by,
                recorded_at,
            )| {
                Ok(InquiryDecision {
                    decision_id,
                    input: InquiryDecisionInput {
                        question_id,
                        considered_question_ids,
                        assessment_event_ids,
                        disposition: InquiryDisposition::parse(&disposition)?,
                        rationale,
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
) -> Result<Vec<InquiryDecision>, WorkbenchError> {
    list_filtered(tx, workspace, situation, None).await
}

pub async fn record(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    actor: Uuid,
    decision: Uuid,
    mut input: InquiryDecisionInput,
) -> Result<InquiryDecision, WorkbenchError> {
    if decision.is_nil()
        || !valid_text(&input.rationale, 4096)
        || !(1..=3).contains(&input.considered_question_ids.len())
        || !input.considered_question_ids.contains(&input.question_id)
        || input
            .considered_question_ids
            .iter()
            .collect::<BTreeSet<_>>()
            .len()
            != input.considered_question_ids.len()
        || input.assessment_event_ids.len() > 64
        || input
            .assessment_event_ids
            .iter()
            .collect::<BTreeSet<_>>()
            .len()
            != input.assessment_event_ids.len()
    {
        return Err(WorkbenchError::Invalid);
    }
    input.rationale = input.rationale.trim().to_owned();
    let mut tx = scoped(pool, workspace, false).await?;
    let existing = list_filtered(&mut tx, workspace, situation, Some(decision))
        .await?
        .pop();
    if let Some(mut existing) = existing {
        if existing.input != input || existing.recorded_by != actor {
            return Err(WorkbenchError::Conflict);
        }
        existing.replayed = true;
        tx.commit().await?;
        return Ok(existing);
    }
    let question_count:i64=sqlx::query_scalar("SELECT count(*) FROM niccolo.epistemic_questions WHERE workspace_id=$1 AND situation_id=$2 AND question_id=ANY($3)").bind(workspace).bind(situation).bind(&input.considered_question_ids).fetch_one(&mut *tx).await?;
    if question_count as usize != input.considered_question_ids.len() {
        return Err(WorkbenchError::Invisible);
    }
    let assessment_count:i64=sqlx::query_scalar("SELECT count(*) FROM niccolo.epistemic_assessment_events WHERE workspace_id=$1 AND situation_id=$2 AND event_id=ANY($3)").bind(workspace).bind(situation).bind(&input.assessment_event_ids).fetch_one(&mut *tx).await?;
    if assessment_count as usize != input.assessment_event_ids.len() {
        return Err(WorkbenchError::Invisible);
    }
    let valid_count:i64=sqlx::query_scalar("SELECT count(*) FROM niccolo.epistemic_assessment_events WHERE workspace_id=$1 AND situation_id=$2 AND event_id=ANY($3) AND target_object_id=ANY($4) AND (dimension IN ('RELEVANCE','DISCRIMINATORY_POWER','ACQUISITION_COST','ACCESSIBILITY','CONTAMINATION_RISK','URGENCY','AUTHORIZATION_STATUS','INVESTIGATION_COVERAGE') OR left(dimension,10)='INTEGRITY_')").bind(workspace).bind(situation).bind(&input.assessment_event_ids).bind(&input.considered_question_ids).fetch_one(&mut *tx).await?;
    if valid_count != assessment_count {
        return Err(WorkbenchError::Invalid);
    }
    let inserted=sqlx::query("INSERT INTO niccolo.epistemic_question_decisions(workspace_id,situation_id,decision_id,question_id,considered_question_ids,assessment_event_ids,disposition,rationale,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT DO NOTHING")
        .bind(workspace).bind(situation).bind(decision).bind(input.question_id).bind(&input.considered_question_ids).bind(&input.assessment_event_ids).bind(input.disposition.code()).bind(&input.rationale).bind(actor).execute(&mut *tx).await?.rows_affected()==1;
    let mut result = list_filtered(&mut tx, workspace, situation, Some(decision))
        .await?
        .pop()
        .ok_or(WorkbenchError::Conflict)?;
    if result.input != input || result.recorded_by != actor {
        return Err(WorkbenchError::Conflict);
    }
    result.replayed = !inserted;
    tx.commit().await?;
    Ok(result)
}
