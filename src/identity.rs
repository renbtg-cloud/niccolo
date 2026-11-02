//! User-confirmed, per-document speaker identity mappings, with append-only
//! correction/retraction events. Never infer speaker identity from matching text.
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum IdentityError {
    #[error("database error: {0}")]
    Sql(#[from] sqlx::Error),
    #[error("privileged database connection forbidden")]
    PrivilegedRole,
    #[error("invalid identity input")]
    Input,
    #[error("reused identity/event ID conflicts with original attribution")]
    Conflict,
    #[error("workspace or document unavailable")]
    Invisible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityAction {
    Assign,
    Clear,
}
impl IdentityAction {
    fn sql(self) -> &'static str {
        match self {
            Self::Assign => "ASSIGN",
            Self::Clear => "CLEAR",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssignmentEvent {
    pub event_id: Uuid,
    pub event_no: i64,
    pub document_id: Uuid,
    pub speaker_label: String,
    pub action: String,
    pub person_id: Option<Uuid>,
    pub actor_principal_id: Uuid,
    pub human_reason: String,
    pub replayed: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveBinding {
    pub speaker_label: String,
    pub person_id: Option<Uuid>,
    pub event_id: Uuid,
    pub event_no: i64,
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
    read_only: bool,
) -> Result<sqlx::Transaction<'_, sqlx::Postgres>, IdentityError> {
    let mut tx = pool.begin().await?;
    if read_only {
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await?;
    }
    let privileged: bool = sqlx::query_scalar(
        "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(&mut *tx)
    .await?;
    if privileged {
        return Err(IdentityError::PrivilegedRole);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    Ok(tx)
}

type StoredIdentityRow = (i64, Uuid, String, String, Option<Uuid>, String, Uuid);
type IdentityHistoryRow = (Uuid, i64, Uuid, String, String, Option<Uuid>, Uuid, String);

pub async fn create_identity(
    pool: &PgPool,
    workspace: Uuid,
    person: Uuid,
    display_label: &str,
) -> Result<bool, IdentityError> {
    if display_label.trim().is_empty()
        || display_label.len() > 512
        || display_label.chars().any(char::is_control)
    {
        return Err(IdentityError::Input);
    }
    let mut tx = scoped(pool, workspace, false).await?;
    let visible: Option<Uuid> =
        sqlx::query_scalar("SELECT workspace_id FROM niccolo.workspaces WHERE workspace_id=$1")
            .bind(workspace)
            .fetch_optional(&mut *tx)
            .await?;
    if visible.is_none() {
        return Err(IdentityError::Invisible);
    }
    let inserted = sqlx::query(
        "INSERT INTO niccolo.identity_subjects(workspace_id,person_id,display_label) \
        VALUES ($1,$2,$3) ON CONFLICT(workspace_id,person_id) DO NOTHING",
    )
    .bind(workspace)
    .bind(person)
    .bind(display_label)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    if !inserted {
        let existing:Option<String>=sqlx::query_scalar(
            "SELECT display_label FROM niccolo.identity_subjects WHERE workspace_id=$1 AND person_id=$2")
            .bind(workspace).bind(person).fetch_optional(&mut *tx).await?;
        if existing.as_deref() != Some(display_label) {
            return Err(IdentityError::Conflict);
        }
    }
    tx.commit().await?;
    Ok(inserted)
}

/// Serialize mappings per workspace so concurrent corrections have a definite
/// order. HTTP-layer authorization provides the actor ID, never an input field.
/// Explicit API boundary carries 8 independent attribution fields plus pool.
#[expect(
    clippy::too_many_arguments,
    reason = "the callsite binds all audit fields explicitly"
)]
pub async fn record_event(
    pool: &PgPool,
    workspace: Uuid,
    actor: Uuid,
    event_id: Uuid,
    document_id: Uuid,
    speaker_label: &str,
    action: IdentityAction,
    person_id: Option<Uuid>,
    human_reason: &str,
) -> Result<AssignmentEvent, IdentityError> {
    if speaker_label.trim().is_empty()
        || speaker_label.len() > 512
        || speaker_label.chars().any(char::is_control)
        || human_reason.trim().is_empty()
        || human_reason.len() > 2048
        || human_reason.chars().any(char::is_control)
        || matches!(
            (action, person_id),
            (IdentityAction::Assign, None) | (IdentityAction::Clear, Some(_))
        )
    {
        return Err(IdentityError::Input);
    }
    let mut tx = scoped(pool, workspace, false).await?;
    let visible: Option<Uuid> = sqlx::query_scalar(
        "SELECT workspace_id FROM niccolo.workspaces WHERE workspace_id=$1 FOR UPDATE",
    )
    .bind(workspace)
    .fetch_optional(&mut *tx)
    .await?;
    if visible.is_none() {
        return Err(IdentityError::Invisible);
    }
    let inserted:Option<i64>=sqlx::query_scalar(
        "INSERT INTO niccolo.identity_events(workspace_id,event_id,document_id,speaker_label,operation,person_id,human_reason,actor_principal_id) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT (workspace_id,event_id) DO NOTHING RETURNING event_no")
        .bind(workspace).bind(event_id).bind(document_id).bind(speaker_label)
        .bind(action.sql()).bind(person_id).bind(human_reason).bind(actor)
        .fetch_optional(&mut *tx).await?;
    let event:Option<StoredIdentityRow>=sqlx::query_as(
        "SELECT event_no,document_id,speaker_label,operation,person_id,human_reason,actor_principal_id \
         FROM niccolo.identity_events WHERE workspace_id=$1 AND event_id=$2")
        .bind(workspace).bind(event_id).fetch_optional(&mut *tx).await?;
    let Some((
        event_no,
        stored_doc,
        stored_speaker,
        stored_op,
        stored_person,
        reason,
        stored_actor,
    )) = event
    else {
        return Err(IdentityError::Conflict);
    };
    if (
        stored_doc,
        stored_speaker.as_str(),
        stored_op.as_str(),
        stored_person,
        reason.as_str(),
        stored_actor,
    ) != (
        document_id,
        speaker_label,
        action.sql(),
        person_id,
        human_reason,
        actor,
    ) {
        return Err(IdentityError::Conflict);
    }
    tx.commit().await?;
    Ok(AssignmentEvent {
        event_id,
        event_no,
        document_id,
        speaker_label: stored_speaker,
        action: stored_op,
        person_id: stored_person,
        actor_principal_id: stored_actor,
        human_reason: reason,
        replayed: inserted.is_none(),
    })
}

pub async fn effective_bindings(
    pool: &PgPool,
    workspace: Uuid,
    document: Uuid,
) -> Result<Vec<EffectiveBinding>, IdentityError> {
    let mut tx = scoped(pool, workspace, true).await?;
    let doc: Option<Uuid> = sqlx::query_scalar(
        "SELECT document_id FROM niccolo.source_documents WHERE workspace_id=$1 AND document_id=$2",
    )
    .bind(workspace)
    .bind(document)
    .fetch_optional(&mut *tx)
    .await?;
    if doc.is_none() {
        return Err(IdentityError::Invisible);
    }
    let rows: Vec<(String, Option<Uuid>, Uuid, i64)> = sqlx::query_as(
        "SELECT DISTINCT ON (speaker_label) speaker_label,person_id,event_id,event_no \
         FROM niccolo.identity_events WHERE workspace_id=$1 AND document_id=$2 \
         ORDER BY speaker_label,event_no DESC",
    )
    .bind(workspace)
    .bind(document)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(rows
        .into_iter()
        .map(
            |(speaker_label, person_id, event_id, event_no)| EffectiveBinding {
                speaker_label,
                person_id,
                event_id,
                event_no,
            },
        )
        .collect())
}

pub async fn event_history(
    pool: &PgPool,
    workspace: Uuid,
    document: Uuid,
    after: i64,
    limit: i64,
) -> Result<(Vec<AssignmentEvent>, Option<i64>), IdentityError> {
    if !(0..=i64::MAX - 1).contains(&after) || !(1..=100).contains(&limit) {
        return Err(IdentityError::Input);
    }
    let mut tx = scoped(pool, workspace, true).await?;
    let doc: Option<Uuid> = sqlx::query_scalar(
        "SELECT document_id FROM niccolo.source_documents WHERE workspace_id=$1 AND document_id=$2",
    )
    .bind(workspace)
    .bind(document)
    .fetch_optional(&mut *tx)
    .await?;
    if doc.is_none() {
        return Err(IdentityError::Invisible);
    }
    let rows:Vec<IdentityHistoryRow>=sqlx::query_as(
      "SELECT event_id,event_no,document_id,speaker_label,operation,person_id,actor_principal_id,human_reason \
       FROM niccolo.identity_events WHERE workspace_id=$1 AND document_id=$2 AND event_no>$3 \
       ORDER BY event_no LIMIT $4")
       .bind(workspace).bind(document).bind(after).bind(limit+1).fetch_all(&mut *tx).await?;
    tx.commit().await?;
    let more = rows.len() as i64 > limit;
    let events: Vec<_> = rows
        .into_iter()
        .take(limit as usize)
        .map(
            |(
                event_id,
                event_no,
                document_id,
                speaker_label,
                action,
                person_id,
                actor_principal_id,
                human_reason,
            )| AssignmentEvent {
                event_id,
                event_no,
                document_id,
                speaker_label,
                action,
                person_id,
                actor_principal_id,
                human_reason,
                replayed: false,
            },
        )
        .collect();
    let next_after = if more {
        events.last().map(|x| x.event_no)
    } else {
        None
    };
    Ok((events, next_after))
}
