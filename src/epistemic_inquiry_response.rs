//! Explicitly recorded communicative occurrences, separate from their meaning or truth.
use super::{
    WorkbenchError, assessment::evidence_visible, object_available_in_situation, scoped, valid_text,
};
use crate::epistemic_identity::{
    EpistemicIdentityError, HumanUtterance, NewUtterance, create_utterance_in_tx,
};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResponseInput {
    pub utterance_id: Uuid,
    pub utterance_text: String,
    pub speaker_actor_id: Option<Uuid>,
    pub source_evidence_id: Option<Uuid>,
    pub source_contribution_id: Option<Uuid>,
    pub rationale: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestionResponse {
    pub response_id: Uuid,
    pub question_id: Uuid,
    pub utterance: HumanUtterance,
    pub rationale: String,
    pub recorded_by: Uuid,
    pub recorded_at: String,
    pub replayed: bool,
}

fn identity_error(error: EpistemicIdentityError) -> WorkbenchError {
    match error {
        EpistemicIdentityError::Database(e) => WorkbenchError::Database(e),
        EpistemicIdentityError::Privileged => WorkbenchError::Privileged,
        EpistemicIdentityError::Invalid => WorkbenchError::Invalid,
        EpistemicIdentityError::Invisible => WorkbenchError::Invisible,
        EpistemicIdentityError::Conflict => WorkbenchError::Conflict,
    }
}

pub(super) async fn list_in_snapshot(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    situation: Uuid,
    response: Option<Uuid>,
) -> Result<Vec<QuestionResponse>, WorkbenchError> {
    type ResponseRow = (
        Uuid,
        Uuid,
        Uuid,
        String,
        Option<Uuid>,
        Option<Uuid>,
        Option<Uuid>,
        Uuid,
        String,
        Uuid,
        String,
    );
    let rows: Vec<ResponseRow> = sqlx::query_as("SELECT r.response_id,r.question_id,u.utterance_id,u.utterance_text,u.speaker_actor_id,u.source_evidence_id,u.source_contribution_id,o.created_by_principal_id,r.rationale,r.recorded_by_principal_id,r.created_at::text FROM niccolo.epistemic_question_responses r JOIN niccolo.epistemic_human_utterances u USING(workspace_id,situation_id,utterance_id) JOIN niccolo.epistemic_objects o ON o.workspace_id=u.workspace_id AND o.object_id=u.utterance_id WHERE r.workspace_id=$1 AND r.situation_id=$2 AND ($3::uuid IS NULL OR r.response_id=$3) ORDER BY r.created_at,r.response_id")
        .bind(workspace).bind(situation).bind(response).fetch_all(&mut **tx).await?;
    Ok(rows
        .into_iter()
        .map(
            |(
                response_id,
                question_id,
                utterance_id,
                utterance_text,
                speaker_actor_id,
                source_evidence_id,
                source_contribution_id,
                created_by,
                rationale,
                recorded_by,
                recorded_at,
            )| QuestionResponse {
                response_id,
                question_id,
                utterance: HumanUtterance {
                    utterance_id,
                    situation_id: Some(situation),
                    utterance_text,
                    language: "en".into(),
                    speaker_actor_id,
                    source_evidence_id,
                    source_contribution_id,
                    created_by,
                    replayed: false,
                },
                rationale,
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
    question: Uuid,
    response: Uuid,
    input: ResponseInput,
) -> Result<QuestionResponse, WorkbenchError> {
    if response.is_nil()
        || input.utterance_id.is_nil()
        || !valid_text(&input.utterance_text, 8192)
        || !valid_text(&input.rationale, 4096)
        || (input.source_evidence_id.is_some() && input.source_contribution_id.is_some())
    {
        return Err(WorkbenchError::Invalid);
    }
    let mut tx = scoped(pool, workspace, false).await?;
    match object_available_in_situation(&mut tx, workspace, situation, question)
        .await?
        .as_deref()
    {
        Some("QUESTION") => {}
        Some(_) => return Err(WorkbenchError::Invalid),
        None => return Err(WorkbenchError::Invisible),
    }
    if let Some(id) = input.speaker_actor_id {
        let visible: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM niccolo.actors WHERE workspace_id=$1 AND actor_id=$2)",
        )
        .bind(workspace)
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
        if !visible {
            return Err(WorkbenchError::Invisible);
        }
    }
    if let Some(id) = input.source_evidence_id {
        if !evidence_visible(&mut tx, workspace, situation, id).await? {
            return Err(WorkbenchError::Invisible);
        }
    }
    if let Some(id) = input.source_contribution_id {
        let visible: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM niccolo.situation_contributions WHERE workspace_id=$1 AND situation_id=$2 AND contribution_id=$3 AND kind='USER_REPORTED')").bind(workspace).bind(situation).bind(id).fetch_one(&mut *tx).await?;
        if !visible {
            return Err(WorkbenchError::Invisible);
        }
    }
    let utterance = create_utterance_in_tx(
        &mut tx,
        workspace,
        actor,
        NewUtterance {
            utterance_id: input.utterance_id,
            situation_id: Some(situation),
            utterance_text: &input.utterance_text,
            language: "en",
            speaker_actor_id: input.speaker_actor_id,
            source_evidence_id: input.source_evidence_id,
            source_contribution_id: input.source_contribution_id,
        },
    )
    .await
    .map_err(identity_error)?;
    let rationale = input.rationale.trim();
    let inserted = sqlx::query("INSERT INTO niccolo.epistemic_question_responses(workspace_id,situation_id,question_id,response_id,utterance_id,rationale,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT DO NOTHING").bind(workspace).bind(situation).bind(question).bind(response).bind(input.utterance_id).bind(rationale).bind(actor).execute(&mut *tx).await?.rows_affected()==1;
    let mut stored = list_in_snapshot(&mut tx, workspace, situation, Some(response))
        .await?
        .pop()
        .ok_or(WorkbenchError::Conflict)?;
    if stored.question_id != question
        || stored.utterance.utterance_id != utterance.utterance_id
        || stored.rationale != rationale
        || stored.recorded_by != actor
    {
        return Err(WorkbenchError::Conflict);
    }
    stored.replayed = !inserted;
    stored.utterance.replayed = utterance.replayed;
    tx.commit().await?;
    Ok(stored)
}
