//! Explicitly recorded communicative occurrences, separate from their meaning or truth.
use super::{
    WorkbenchError, assessment::evidence_visible, object_available_in_situation, scoped, valid_text,
};
use crate::epistemic_identity::{
    self, EpistemicIdentityError, HumanUtterance, InterpretationProducer, InterpretationView,
    MeaningConfirmation, NewUtterance, create_utterance_in_tx,
};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use std::collections::BTreeMap;
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InterpretationInput {
    pub expected_current_revision_id: Option<Uuid>,
    pub interpretation_text: String,
    pub rationale: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfirmationInput {
    pub state: MeaningConfirmation,
    pub note: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InterpretationRevisionRecord {
    pub revision_id: Uuid,
    pub supersedes_revision_id: Option<Uuid>,
    pub interpretation_text: String,
    pub rationale: String,
    pub producer: InterpretationProducer,
    pub recorded_by: Uuid,
    pub recorded_at: String,
    pub meaning_confirmation: MeaningConfirmation,
    pub meaning_is_provisional: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeaningConfirmationRecord {
    pub event_id: Uuid,
    pub revision_id: Uuid,
    pub state: MeaningConfirmation,
    pub note: String,
    pub recorded_by: Uuid,
    pub recorded_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseInterpretationView {
    pub utterance_id: Uuid,
    pub interpretation_id: Uuid,
    pub current_revision_id: Uuid,
    pub revisions: Vec<InterpretationRevisionRecord>,
    pub confirmations: Vec<MeaningConfirmationRecord>,
    pub confirmation_establishes_truth: bool,
}

/// Both history and current meaning are read in the same snapshot as the response.
pub(super) async fn interpretations_in_snapshot(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    situation: Uuid,
) -> Result<Vec<ResponseInterpretationView>, WorkbenchError> {
    type RevisionRow = (
        Uuid,
        Uuid,
        Uuid,
        Option<Uuid>,
        String,
        String,
        String,
        Uuid,
        String,
        Option<String>,
    );
    let rows:Vec<RevisionRow> = sqlx::query_as("SELECT i.utterance_id,i.interpretation_id,r.revision_id,r.supersedes_revision_id,r.interpretation_text,r.rationale,r.producer_kind,r.recorded_by_principal_id,r.created_at::text,(SELECT c.confirmation_state FROM niccolo.epistemic_interpretation_confirmation_events c WHERE c.workspace_id=r.workspace_id AND c.interpretation_id=r.interpretation_id AND c.revision_id=r.revision_id ORDER BY c.event_no DESC LIMIT 1) FROM niccolo.epistemic_utterance_interpretations i JOIN niccolo.epistemic_interpretation_revisions r USING(workspace_id,interpretation_id) WHERE i.workspace_id=$1 AND EXISTS(SELECT 1 FROM niccolo.epistemic_question_responses q WHERE q.workspace_id=i.workspace_id AND q.situation_id=$2 AND q.utterance_id=i.utterance_id) ORDER BY i.interpretation_id,r.revision_no").bind(workspace).bind(situation).fetch_all(&mut **tx).await?;
    let mut views: BTreeMap<Uuid, ResponseInterpretationView> = BTreeMap::new();
    for (
        utterance_id,
        interpretation_id,
        revision_id,
        supersedes_revision_id,
        interpretation_text,
        rationale,
        producer,
        recorded_by,
        recorded_at,
        confirmation,
    ) in rows
    {
        let view = views
            .entry(interpretation_id)
            .or_insert_with(|| ResponseInterpretationView {
                utterance_id,
                interpretation_id,
                current_revision_id: revision_id,
                revisions: vec![],
                confirmations: vec![],
                confirmation_establishes_truth: false,
            });
        let meaning_confirmation =
            MeaningConfirmation::from_db(confirmation.as_deref()).map_err(identity_error)?;
        view.current_revision_id = revision_id;
        view.revisions.push(InterpretationRevisionRecord {
            revision_id,
            supersedes_revision_id,
            interpretation_text,
            rationale,
            producer: InterpretationProducer::from_db(&producer).map_err(identity_error)?,
            recorded_by,
            recorded_at,
            meaning_confirmation,
            meaning_is_provisional: meaning_confirmation != MeaningConfirmation::Confirmed,
        });
    }
    let confirmations:Vec<(Uuid,Uuid,Uuid,String,String,Uuid,String)> = sqlx::query_as("SELECT c.interpretation_id,c.event_id,c.revision_id,c.confirmation_state,c.note,c.recorded_by_principal_id,c.created_at::text FROM niccolo.epistemic_interpretation_confirmation_events c JOIN niccolo.epistemic_utterance_interpretations i USING(workspace_id,interpretation_id) WHERE c.workspace_id=$1 AND EXISTS(SELECT 1 FROM niccolo.epistemic_question_responses q WHERE q.workspace_id=i.workspace_id AND q.situation_id=$2 AND q.utterance_id=i.utterance_id) ORDER BY c.event_no").bind(workspace).bind(situation).fetch_all(&mut **tx).await?;
    for (id, event_id, revision_id, state, note, recorded_by, recorded_at) in confirmations {
        if let Some(view) = views.get_mut(&id) {
            view.confirmations.push(MeaningConfirmationRecord {
                event_id,
                revision_id,
                state: MeaningConfirmation::from_db(Some(&state)).map_err(identity_error)?,
                note,
                recorded_by,
                recorded_at,
            });
        }
    }
    Ok(views.into_values().collect())
}

// Response links, utterances and interpretation identities are append-only. Check
// the exact Situation association before calling the existing identity writer.
async fn response_target(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    utterance: Uuid,
    interpretation: Option<Uuid>,
    revision: Option<Uuid>,
) -> Result<(), WorkbenchError> {
    let mut tx = scoped(pool, workspace, true).await?;
    let visible:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM niccolo.epistemic_question_responses q WHERE q.workspace_id=$1 AND q.situation_id=$2 AND q.utterance_id=$3 AND ($4::uuid IS NULL OR EXISTS(SELECT 1 FROM niccolo.epistemic_utterance_interpretations i WHERE i.workspace_id=q.workspace_id AND i.utterance_id=q.utterance_id AND i.interpretation_id=$4 AND ($5::uuid IS NULL OR EXISTS(SELECT 1 FROM niccolo.epistemic_interpretation_revisions r WHERE r.workspace_id=i.workspace_id AND r.interpretation_id=i.interpretation_id AND r.revision_id=$5)))))").bind(workspace).bind(situation).bind(utterance).bind(interpretation).bind(revision).fetch_one(&mut *tx).await?;
    tx.commit().await?;
    if visible {
        Ok(())
    } else {
        Err(WorkbenchError::Invisible)
    }
}

pub async fn record_interpretation(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    actor: Uuid,
    utterance: Uuid,
    interpretation: Uuid,
    revision: Uuid,
    input: InterpretationInput,
) -> Result<InterpretationView, WorkbenchError> {
    if interpretation.is_nil() || revision.is_nil() {
        return Err(WorkbenchError::Invalid);
    }
    let previous = input.expected_current_revision_id;
    response_target(
        pool,
        workspace,
        situation,
        utterance,
        previous.map(|_| interpretation),
        previous,
    )
    .await?;
    let result = if let Some(previous) = previous {
        epistemic_identity::revise_interpretation(
            pool,
            workspace,
            actor,
            interpretation,
            previous,
            revision,
            &input.interpretation_text,
            &input.rationale,
            InterpretationProducer::Human,
        )
        .await
    } else {
        epistemic_identity::create_interpretation(
            pool,
            workspace,
            actor,
            interpretation,
            utterance,
            revision,
            &input.interpretation_text,
            &input.rationale,
            InterpretationProducer::Human,
        )
        .await
    };
    result.map_err(identity_error)
}

pub async fn confirm_interpretation(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    actor: Uuid,
    utterance: Uuid,
    interpretation: Uuid,
    revision: Uuid,
    event: Uuid,
    input: ConfirmationInput,
) -> Result<InterpretationView, WorkbenchError> {
    if event.is_nil() {
        return Err(WorkbenchError::Invalid);
    }
    response_target(
        pool,
        workspace,
        situation,
        utterance,
        Some(interpretation),
        Some(revision),
    )
    .await?;
    epistemic_identity::record_meaning_confirmation(
        pool,
        workspace,
        actor,
        interpretation,
        revision,
        event,
        input.state,
        &input.note,
    )
    .await
    .map_err(identity_error)
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
