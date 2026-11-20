//! Persistent semantic identity and revisable understanding of human utterances.
//!
//! Existing analysis claims remain local occurrences/representations. This module adds
//! workspace-level semantic identity without silently deduplicating claim occurrences,
//! provenance or credibility. Human-language interpretation is deliberately separate
//! from `situation_analysis_candidate_interpretations`, which are candidate hypotheses.
use crate::analysis_structure::AnalysisScope;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum EpistemicIdentityError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("privileged SQL login prohibited")]
    Privileged,
    #[error("invalid epistemic identity request")]
    Invalid,
    #[error("epistemic object is not visible")]
    Invisible,
    #[error("idempotency or revision conflict")]
    Conflict,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InterpretationProducer {
    Model,
    Human,
    Deterministic,
}
impl InterpretationProducer {
    fn db_value(self) -> &'static str {
        match self {
            Self::Model => "MODEL",
            Self::Human => "HUMAN",
            Self::Deterministic => "DETERMINISTIC",
        }
    }
    fn from_db(v: &str) -> Result<Self, EpistemicIdentityError> {
        match v {
            "MODEL" => Ok(Self::Model),
            "HUMAN" => Ok(Self::Human),
            "DETERMINISTIC" => Ok(Self::Deterministic),
            _ => Err(EpistemicIdentityError::Invalid),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MeaningConfirmation {
    Unconfirmed,
    Confirmed,
    Rejected,
}
impl MeaningConfirmation {
    fn db_value(self) -> Option<&'static str> {
        match self {
            Self::Unconfirmed => None,
            Self::Confirmed => Some("CONFIRMED"),
            Self::Rejected => Some("REJECTED"),
        }
    }
    fn from_db(v: Option<&str>) -> Result<Self, EpistemicIdentityError> {
        match v {
            None => Ok(Self::Unconfirmed),
            Some("CONFIRMED") => Ok(Self::Confirmed),
            Some("REJECTED") => Ok(Self::Rejected),
            Some(_) => Err(EpistemicIdentityError::Invalid),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proposition {
    pub proposition_id: Uuid,
    pub statement_text: String,
    pub language: String,
    pub created_by: Uuid,
    pub replayed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanUtterance {
    pub utterance_id: Uuid,
    pub situation_id: Option<Uuid>,
    pub utterance_text: String,
    pub language: String,
    pub speaker_actor_id: Option<Uuid>,
    pub source_evidence_id: Option<Uuid>,
    pub source_contribution_id: Option<Uuid>,
    pub created_by: Uuid,
    pub replayed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewUtterance<'a> {
    pub utterance_id: Uuid,
    pub situation_id: Option<Uuid>,
    pub utterance_text: &'a str,
    pub language: &'a str,
    pub speaker_actor_id: Option<Uuid>,
    pub source_evidence_id: Option<Uuid>,
    pub source_contribution_id: Option<Uuid>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InterpretationView {
    pub interpretation_id: Uuid,
    pub utterance_id: Uuid,
    pub revision_id: Uuid,
    pub supersedes_revision_id: Option<Uuid>,
    pub interpretation_text: String,
    pub rationale: String,
    pub producer: InterpretationProducer,
    pub recorded_by: Uuid,
    pub meaning_confirmation: MeaningConfirmation,
    pub meaning_is_provisional: bool,
    pub confirmation_establishes_truth: bool,
    pub replayed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimInterpretationDependency {
    pub situation_id: Uuid,
    pub analysis_id: Uuid,
    pub claim_id: Uuid,
    pub interpretation_id: Uuid,
    pub interpretation_revision_id: Uuid,
    pub rationale: String,
    pub recorded_by: Uuid,
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
    read_only: bool,
) -> Result<Transaction<'_, Postgres>, EpistemicIdentityError> {
    let mut tx = pool.begin().await?;
    let mode = if read_only {
        "SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY"
    } else {
        "SET TRANSACTION ISOLATION LEVEL READ COMMITTED"
    };
    sqlx::query(mode).execute(&mut *tx).await?;
    let privileged: bool = sqlx::query_scalar(
        "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(&mut *tx)
    .await?;
    if privileged {
        return Err(EpistemicIdentityError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

fn valid_text(value: &str, max: usize) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty() && value.len() <= max && !value.contains('\0')
}

async fn ensure_object(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    object_id: Uuid,
    object_kind: &str,
    actor: Uuid,
) -> Result<bool, EpistemicIdentityError> {
    let inserted = sqlx::query(
        "INSERT INTO niccolo.epistemic_objects(workspace_id,object_id,object_kind,created_by_principal_id) \
         VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING",
    )
    .bind(workspace)
    .bind(object_id)
    .bind(object_kind)
    .bind(actor)
    .execute(&mut **tx)
    .await?
    .rows_affected()
        == 1;
    let stored: Option<(String, Uuid)> = sqlx::query_as(
        "SELECT object_kind,created_by_principal_id FROM niccolo.epistemic_objects \
         WHERE workspace_id=$1 AND object_id=$2",
    )
    .bind(workspace)
    .bind(object_id)
    .fetch_optional(&mut **tx)
    .await?;
    match stored {
        Some((kind, by)) if kind == object_kind && by == actor => Ok(inserted),
        Some(_) => Err(EpistemicIdentityError::Conflict),
        None => Err(EpistemicIdentityError::Invisible),
    }
}

pub async fn create_proposition(
    pool: &PgPool,
    workspace: Uuid,
    actor: Uuid,
    proposition_id: Uuid,
    statement_text: &str,
) -> Result<Proposition, EpistemicIdentityError> {
    if !valid_text(statement_text, 8192) {
        return Err(EpistemicIdentityError::Invalid);
    }
    let statement_text = statement_text.trim();
    let mut tx = scoped(pool, workspace, false).await?;
    let object_inserted =
        ensure_object(&mut tx, workspace, proposition_id, "PROPOSITION", actor).await?;
    let proposition_inserted = sqlx::query(
        "INSERT INTO niccolo.epistemic_propositions(workspace_id,proposition_id,statement_text,language) \
         VALUES($1,$2,$3,'en') ON CONFLICT DO NOTHING",
    )
    .bind(workspace)
    .bind(proposition_id)
    .bind(statement_text)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    let stored: Option<(String, String)> = sqlx::query_as(
        "SELECT statement_text,language FROM niccolo.epistemic_propositions \
         WHERE workspace_id=$1 AND proposition_id=$2",
    )
    .bind(workspace)
    .bind(proposition_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((stored_text, language)) = stored else {
        return Err(EpistemicIdentityError::Invisible);
    };
    if stored_text != statement_text || language != "en" {
        return Err(EpistemicIdentityError::Conflict);
    }
    tx.commit().await?;
    Ok(Proposition {
        proposition_id,
        statement_text: stored_text,
        language,
        created_by: actor,
        replayed: !object_inserted && !proposition_inserted,
    })
}

pub async fn link_claim_to_proposition(
    pool: &PgPool,
    scope: AnalysisScope,
    actor: Uuid,
    claim_id: Uuid,
    proposition_id: Uuid,
    rationale: &str,
) -> Result<bool, EpistemicIdentityError> {
    if !valid_text(rationale, 2048) {
        return Err(EpistemicIdentityError::Invalid);
    }
    let rationale = rationale.trim();
    let mut tx = scoped(pool, scope.workspace, false).await?;
    let inserted = sqlx::query(
        "INSERT INTO niccolo.situation_analysis_claim_propositions \
         (workspace_id,situation_id,analysis_id,claim_id,proposition_id,rationale,recorded_by_principal_id) \
         VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT DO NOTHING",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(claim_id)
    .bind(proposition_id)
    .bind(rationale)
    .bind(actor)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    let stored: Option<(String, Uuid)> = sqlx::query_as(
        "SELECT rationale,recorded_by_principal_id FROM niccolo.situation_analysis_claim_propositions \
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND claim_id=$4 AND proposition_id=$5",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(claim_id)
    .bind(proposition_id)
    .fetch_optional(&mut *tx)
    .await?;
    match stored {
        Some((stored_rationale, by)) if stored_rationale == rationale && by == actor => {
            tx.commit().await?;
            Ok(!inserted)
        }
        Some(_) => Err(EpistemicIdentityError::Conflict),
        None => Err(EpistemicIdentityError::Invisible),
    }
}

pub async fn create_utterance(
    pool: &PgPool,
    workspace: Uuid,
    actor: Uuid,
    item: NewUtterance<'_>,
) -> Result<HumanUtterance, EpistemicIdentityError> {
    if item.language != "en"
        || !valid_text(item.utterance_text, 8192)
        || (item.source_evidence_id.is_some() && item.source_contribution_id.is_some())
        || (item.source_contribution_id.is_some() && item.situation_id.is_none())
    {
        return Err(EpistemicIdentityError::Invalid);
    }
    let text = item.utterance_text.trim();
    let mut tx = scoped(pool, workspace, false).await?;
    let object_inserted =
        ensure_object(&mut tx, workspace, item.utterance_id, "UTTERANCE", actor).await?;
    let row_inserted = sqlx::query(
        "INSERT INTO niccolo.epistemic_human_utterances \
         (workspace_id,utterance_id,situation_id,utterance_text,language,speaker_actor_id,source_evidence_id,source_contribution_id) \
         VALUES($1,$2,$3,$4,'en',$5,$6,$7) ON CONFLICT DO NOTHING",
    )
    .bind(workspace)
    .bind(item.utterance_id)
    .bind(item.situation_id)
    .bind(text)
    .bind(item.speaker_actor_id)
    .bind(item.source_evidence_id)
    .bind(item.source_contribution_id)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    type UtteranceRow = (
        Option<Uuid>,
        String,
        String,
        Option<Uuid>,
        Option<Uuid>,
        Option<Uuid>,
    );
    let stored: Option<UtteranceRow> = sqlx::query_as(
        "SELECT situation_id,utterance_text,language,speaker_actor_id,source_evidence_id,source_contribution_id \
         FROM niccolo.epistemic_human_utterances WHERE workspace_id=$1 AND utterance_id=$2",
    )
    .bind(workspace)
    .bind(item.utterance_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((
        situation_id,
        stored_text,
        language,
        speaker_actor_id,
        source_evidence_id,
        source_contribution_id,
    )) = stored
    else {
        return Err(EpistemicIdentityError::Invisible);
    };
    if situation_id != item.situation_id
        || stored_text != text
        || language != "en"
        || speaker_actor_id != item.speaker_actor_id
        || source_evidence_id != item.source_evidence_id
        || source_contribution_id != item.source_contribution_id
    {
        return Err(EpistemicIdentityError::Conflict);
    }
    tx.commit().await?;
    Ok(HumanUtterance {
        utterance_id: item.utterance_id,
        situation_id,
        utterance_text: stored_text,
        language,
        speaker_actor_id,
        source_evidence_id,
        source_contribution_id,
        created_by: actor,
        replayed: !object_inserted && !row_inserted,
    })
}

type CurrentInterpretationRow = (
    Uuid,
    Uuid,
    Option<Uuid>,
    String,
    String,
    String,
    Uuid,
    Option<String>,
);

async fn current_interpretation_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    interpretation_id: Uuid,
) -> Result<InterpretationView, EpistemicIdentityError> {
    let row: Option<CurrentInterpretationRow> = sqlx::query_as(
        "SELECT i.utterance_id,r.revision_id,r.supersedes_revision_id,r.interpretation_text,r.rationale, \
                r.producer_kind,r.recorded_by_principal_id, \
                (SELECT e.confirmation_state FROM niccolo.epistemic_interpretation_confirmation_events e \
                  WHERE e.workspace_id=r.workspace_id AND e.interpretation_id=r.interpretation_id \
                    AND e.revision_id=r.revision_id ORDER BY e.event_no DESC LIMIT 1) \
           FROM niccolo.epistemic_utterance_interpretations i \
           JOIN LATERAL (SELECT rr.* FROM niccolo.epistemic_interpretation_revisions rr \
             WHERE rr.workspace_id=i.workspace_id AND rr.interpretation_id=i.interpretation_id \
             ORDER BY rr.revision_no DESC LIMIT 1) r ON TRUE \
          WHERE i.workspace_id=$1 AND i.interpretation_id=$2",
    )
    .bind(workspace)
    .bind(interpretation_id)
    .fetch_optional(&mut **tx)
    .await?;
    let Some((
        utterance_id,
        revision_id,
        supersedes_revision_id,
        interpretation_text,
        rationale,
        producer_kind,
        recorded_by,
        confirmation,
    )) = row
    else {
        return Err(EpistemicIdentityError::Invisible);
    };
    let meaning_confirmation = MeaningConfirmation::from_db(confirmation.as_deref())?;
    Ok(InterpretationView {
        interpretation_id,
        utterance_id,
        revision_id,
        supersedes_revision_id,
        interpretation_text,
        rationale,
        producer: InterpretationProducer::from_db(&producer_kind)?,
        recorded_by,
        meaning_confirmation,
        meaning_is_provisional: meaning_confirmation != MeaningConfirmation::Confirmed,
        confirmation_establishes_truth: false,
        replayed: false,
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "interpretation identity, first revision and attribution are independently explicit"
)]
pub async fn create_interpretation(
    pool: &PgPool,
    workspace: Uuid,
    actor: Uuid,
    interpretation_id: Uuid,
    utterance_id: Uuid,
    revision_id: Uuid,
    interpretation_text: &str,
    rationale: &str,
    producer: InterpretationProducer,
) -> Result<InterpretationView, EpistemicIdentityError> {
    if !valid_text(interpretation_text, 8192) || !valid_text(rationale, 4096) {
        return Err(EpistemicIdentityError::Invalid);
    }
    let interpretation_text = interpretation_text.trim();
    let rationale = rationale.trim();
    let mut tx = scoped(pool, workspace, false).await?;
    let object_inserted = ensure_object(
        &mut tx,
        workspace,
        interpretation_id,
        "UTTERANCE_INTERPRETATION",
        actor,
    )
    .await?;
    let parent_inserted = sqlx::query(
        "INSERT INTO niccolo.epistemic_utterance_interpretations(workspace_id,interpretation_id,utterance_id) \
         VALUES($1,$2,$3) ON CONFLICT DO NOTHING",
    )
    .bind(workspace)
    .bind(interpretation_id)
    .bind(utterance_id)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    let stored_parent: Option<Uuid> = sqlx::query_scalar(
        "SELECT interpretation_id FROM niccolo.epistemic_utterance_interpretations \
         WHERE workspace_id=$1 AND utterance_id=$2",
    )
    .bind(workspace)
    .bind(utterance_id)
    .fetch_optional(&mut *tx)
    .await?;
    if stored_parent != Some(interpretation_id) {
        return Err(EpistemicIdentityError::Conflict);
    }
    let revision_inserted = sqlx::query(
        "INSERT INTO niccolo.epistemic_interpretation_revisions \
         (workspace_id,interpretation_id,revision_id,supersedes_revision_id,interpretation_text,rationale,producer_kind,recorded_by_principal_id) \
         VALUES($1,$2,$3,NULL,$4,$5,$6,$7) ON CONFLICT DO NOTHING",
    )
    .bind(workspace)
    .bind(interpretation_id)
    .bind(revision_id)
    .bind(interpretation_text)
    .bind(rationale)
    .bind(producer.db_value())
    .bind(actor)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    let stored_revision: Option<(Option<Uuid>, String, String, String, Uuid)> = sqlx::query_as(
        "SELECT supersedes_revision_id,interpretation_text,rationale,producer_kind,recorded_by_principal_id \
         FROM niccolo.epistemic_interpretation_revisions \
         WHERE workspace_id=$1 AND interpretation_id=$2 AND revision_id=$3",
    )
    .bind(workspace)
    .bind(interpretation_id)
    .bind(revision_id)
    .fetch_optional(&mut *tx)
    .await?;
    if stored_revision
        != Some((
            None,
            interpretation_text.to_owned(),
            rationale.to_owned(),
            producer.db_value().to_owned(),
            actor,
        ))
    {
        return Err(EpistemicIdentityError::Conflict);
    }
    let mut view = current_interpretation_in_tx(&mut tx, workspace, interpretation_id).await?;
    if view.revision_id != revision_id {
        return Err(EpistemicIdentityError::Conflict);
    }
    view.replayed = !object_inserted && !parent_inserted && !revision_inserted;
    tx.commit().await?;
    Ok(view)
}

#[expect(
    clippy::too_many_arguments,
    reason = "optimistic revision identity and interpretation provenance are independently explicit"
)]
pub async fn revise_interpretation(
    pool: &PgPool,
    workspace: Uuid,
    actor: Uuid,
    interpretation_id: Uuid,
    expected_current_revision_id: Uuid,
    new_revision_id: Uuid,
    interpretation_text: &str,
    rationale: &str,
    producer: InterpretationProducer,
) -> Result<InterpretationView, EpistemicIdentityError> {
    if new_revision_id == expected_current_revision_id
        || !valid_text(interpretation_text, 8192)
        || !valid_text(rationale, 4096)
    {
        return Err(EpistemicIdentityError::Invalid);
    }
    let interpretation_text = interpretation_text.trim();
    let rationale = rationale.trim();
    let mut tx = scoped(pool, workspace, false).await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1::text || ':' || $2::text,0))")
        .bind(workspace)
        .bind(interpretation_id)
        .execute(&mut *tx)
        .await?;
    let locked: Option<Uuid> = sqlx::query_scalar(
        "SELECT interpretation_id FROM niccolo.epistemic_utterance_interpretations \
         WHERE workspace_id=$1 AND interpretation_id=$2",
    )
    .bind(workspace)
    .bind(interpretation_id)
    .fetch_optional(&mut *tx)
    .await?;
    if locked.is_none() {
        return Err(EpistemicIdentityError::Invisible);
    }
    let current: Option<Uuid> = sqlx::query_scalar(
        "SELECT revision_id FROM niccolo.epistemic_interpretation_revisions \
         WHERE workspace_id=$1 AND interpretation_id=$2 ORDER BY revision_no DESC LIMIT 1",
    )
    .bind(workspace)
    .bind(interpretation_id)
    .fetch_optional(&mut *tx)
    .await?;
    if current != Some(expected_current_revision_id) {
        // An exact retry of the new revision is still allowed.
        let replay: Option<(Option<Uuid>, String, String, String, Uuid)> = sqlx::query_as(
            "SELECT supersedes_revision_id,interpretation_text,rationale,producer_kind,recorded_by_principal_id \
             FROM niccolo.epistemic_interpretation_revisions \
             WHERE workspace_id=$1 AND interpretation_id=$2 AND revision_id=$3",
        )
        .bind(workspace)
        .bind(interpretation_id)
        .bind(new_revision_id)
        .fetch_optional(&mut *tx)
        .await?;
        if replay
            == Some((
                Some(expected_current_revision_id),
                interpretation_text.to_owned(),
                rationale.to_owned(),
                producer.db_value().to_owned(),
                actor,
            ))
            && current == Some(new_revision_id)
        {
            let mut view =
                current_interpretation_in_tx(&mut tx, workspace, interpretation_id).await?;
            view.replayed = true;
            tx.commit().await?;
            return Ok(view);
        }
        return Err(EpistemicIdentityError::Conflict);
    }
    sqlx::query(
        "INSERT INTO niccolo.epistemic_interpretation_revisions \
         (workspace_id,interpretation_id,revision_id,supersedes_revision_id,interpretation_text,rationale,producer_kind,recorded_by_principal_id) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(workspace)
    .bind(interpretation_id)
    .bind(new_revision_id)
    .bind(expected_current_revision_id)
    .bind(interpretation_text)
    .bind(rationale)
    .bind(producer.db_value())
    .bind(actor)
    .execute(&mut *tx)
    .await?;
    let mut view = current_interpretation_in_tx(&mut tx, workspace, interpretation_id).await?;
    view.replayed = false;
    tx.commit().await?;
    Ok(view)
}

#[expect(
    clippy::too_many_arguments,
    reason = "meaning confirmation event identity and attribution are independently explicit"
)]
pub async fn record_meaning_confirmation(
    pool: &PgPool,
    workspace: Uuid,
    actor: Uuid,
    interpretation_id: Uuid,
    revision_id: Uuid,
    event_id: Uuid,
    state: MeaningConfirmation,
    note: &str,
) -> Result<InterpretationView, EpistemicIdentityError> {
    let Some(state_db) = state.db_value() else {
        return Err(EpistemicIdentityError::Invalid);
    };
    if !valid_text(note, 2048) {
        return Err(EpistemicIdentityError::Invalid);
    }
    let note = note.trim();
    let mut tx = scoped(pool, workspace, false).await?;
    let inserted = sqlx::query(
        "INSERT INTO niccolo.epistemic_interpretation_confirmation_events \
         (workspace_id,interpretation_id,revision_id,event_id,confirmation_state,note,recorded_by_principal_id) \
         VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT DO NOTHING",
    )
    .bind(workspace)
    .bind(interpretation_id)
    .bind(revision_id)
    .bind(event_id)
    .bind(state_db)
    .bind(note)
    .bind(actor)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    let stored: Option<(String, String, Uuid)> = sqlx::query_as(
        "SELECT confirmation_state,note,recorded_by_principal_id \
         FROM niccolo.epistemic_interpretation_confirmation_events \
         WHERE workspace_id=$1 AND interpretation_id=$2 AND revision_id=$3 AND event_id=$4",
    )
    .bind(workspace)
    .bind(interpretation_id)
    .bind(revision_id)
    .bind(event_id)
    .fetch_optional(&mut *tx)
    .await?;
    if stored != Some((state_db.to_owned(), note.to_owned(), actor)) {
        return Err(EpistemicIdentityError::Conflict);
    }
    let mut view = current_interpretation_in_tx(&mut tx, workspace, interpretation_id).await?;
    // The effective state is the latest event for the current revision, not necessarily
    // the event being replayed by this call.
    view.replayed = !inserted;
    tx.commit().await?;
    Ok(view)
}

pub async fn get_interpretation(
    pool: &PgPool,
    workspace: Uuid,
    interpretation_id: Uuid,
) -> Result<InterpretationView, EpistemicIdentityError> {
    let mut tx = scoped(pool, workspace, true).await?;
    let view = current_interpretation_in_tx(&mut tx, workspace, interpretation_id).await?;
    tx.commit().await?;
    Ok(view)
}

pub async fn link_claim_interpretation_dependency(
    pool: &PgPool,
    scope: AnalysisScope,
    actor: Uuid,
    claim_id: Uuid,
    interpretation_id: Uuid,
    interpretation_revision_id: Uuid,
    rationale: &str,
) -> Result<bool, EpistemicIdentityError> {
    if !valid_text(rationale, 2048) {
        return Err(EpistemicIdentityError::Invalid);
    }
    let rationale = rationale.trim();
    let mut tx = scoped(pool, scope.workspace, false).await?;
    let inserted = sqlx::query(
        "INSERT INTO niccolo.situation_analysis_claim_interpretation_dependencies \
         (workspace_id,situation_id,analysis_id,claim_id,interpretation_id,interpretation_revision_id,rationale,recorded_by_principal_id) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(claim_id)
    .bind(interpretation_id)
    .bind(interpretation_revision_id)
    .bind(rationale)
    .bind(actor)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    let stored: Option<(String, Uuid)> = sqlx::query_as(
        "SELECT rationale,recorded_by_principal_id \
         FROM niccolo.situation_analysis_claim_interpretation_dependencies \
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND claim_id=$4 \
           AND interpretation_id=$5 AND interpretation_revision_id=$6",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(claim_id)
    .bind(interpretation_id)
    .bind(interpretation_revision_id)
    .fetch_optional(&mut *tx)
    .await?;
    match stored {
        Some((stored_rationale, by)) if stored_rationale == rationale && by == actor => {
            tx.commit().await?;
            Ok(!inserted)
        }
        Some(_) => Err(EpistemicIdentityError::Conflict),
        None => Err(EpistemicIdentityError::Invisible),
    }
}

pub async fn list_stale_claim_interpretation_dependencies(
    pool: &PgPool,
    workspace: Uuid,
    interpretation_id: Uuid,
) -> Result<Vec<ClaimInterpretationDependency>, EpistemicIdentityError> {
    let mut tx = scoped(pool, workspace, true).await?;
    let current_revision: Option<Uuid> = sqlx::query_scalar(
        "SELECT revision_id FROM niccolo.epistemic_interpretation_revisions \
         WHERE workspace_id=$1 AND interpretation_id=$2 ORDER BY revision_no DESC LIMIT 1",
    )
    .bind(workspace)
    .bind(interpretation_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(current_revision) = current_revision else {
        return Err(EpistemicIdentityError::Invisible);
    };
    type DependencyRow = (Uuid, Uuid, Uuid, Uuid, String, Uuid);
    let rows: Vec<DependencyRow> = sqlx::query_as(
        "SELECT situation_id,analysis_id,claim_id,interpretation_revision_id,rationale,recorded_by_principal_id \
         FROM niccolo.situation_analysis_claim_interpretation_dependencies \
         WHERE workspace_id=$1 AND interpretation_id=$2 AND interpretation_revision_id<>$3 \
         ORDER BY situation_id,analysis_id,claim_id,interpretation_revision_id",
    )
    .bind(workspace)
    .bind(interpretation_id)
    .bind(current_revision)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(rows
        .into_iter()
        .map(
            |(
                situation_id,
                analysis_id,
                claim_id,
                interpretation_revision_id,
                rationale,
                recorded_by,
            )| {
                ClaimInterpretationDependency {
                    situation_id,
                    analysis_id,
                    claim_id,
                    interpretation_id,
                    interpretation_revision_id,
                    rationale,
                    recorded_by,
                }
            },
        )
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meaning_confirmation_is_not_truth_confirmation() {
        assert_eq!(MeaningConfirmation::Confirmed.db_value(), Some("CONFIRMED"));
        assert_ne!(
            MeaningConfirmation::Confirmed,
            MeaningConfirmation::Unconfirmed
        );
    }

    #[test]
    fn producer_mapping_is_closed() {
        assert_eq!(InterpretationProducer::Model.db_value(), "MODEL");
        assert!(InterpretationProducer::from_db("MAGIC").is_err());
    }
}
