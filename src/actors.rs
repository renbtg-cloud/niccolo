//! Substrate-neutral observable actor registry.
//!
//! Actors may be human, software, mixed, organizational or unknown. Kind is
//! revisable descriptive metadata, not a prerequisite for investigation and not
//! an assertion that a mind or consciousness exists.
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum ActorError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("privileged SQL login prohibited")]
    Privileged,
    #[error("invalid actor input")]
    Invalid,
    #[error("actor or source unavailable")]
    Invisible,
    #[error("actor identifier replay conflict")]
    Conflict,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ActorView {
    pub actor_id: Uuid,
    pub display_label: String,
    pub effective_kind: Option<String>,
    pub kind_is_investigation_gate: bool,
    pub kind_establishes_mind: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ActorEvent {
    pub event_id: Uuid,
    pub event_no: i64,
    pub operation: String,
    pub actor_kind: Option<String>,
    pub rationale: String,
    pub recorded_by: Uuid,
    pub replayed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ActorBinding {
    pub event_id: Uuid,
    pub event_no: i64,
    pub document_id: Uuid,
    pub speaker_label: String,
    pub operation: String,
    pub actor_id: Option<Uuid>,
    pub rationale: String,
    pub recorded_by: Uuid,
    pub replayed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FrozenActorBinding {
    pub document_id: Uuid,
    pub speaker_label: String,
    pub actor_id: Option<Uuid>,
    pub display_label: Option<String>,
    pub actor_kind: Option<String>,
    pub binding_event_id: Uuid,
    pub binding_event_no: i64,
    pub actor_kind_is_revisable_metadata: bool,
    pub actor_kind_establishes_mind: bool,
}

fn unsafe_directional_char(ch: char) -> bool {
    matches!(
        ch,
        '\u{061c}'
            | '\u{200e}'
            | '\u{200f}'
            | '\u{202a}'
            | '\u{202b}'
            | '\u{202c}'
            | '\u{202d}'
            | '\u{202e}'
            | '\u{2066}'
            | '\u{2067}'
            | '\u{2068}'
            | '\u{2069}'
    )
}

fn valid_label(value: &str, max: usize) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty()
        && value.len() <= max
        && !value
            .chars()
            .any(|ch| ch.is_control() || unsafe_directional_char(ch))
}

fn valid_note(value: &str, max: usize) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty()
        && value.len() <= max
        && !value.chars().any(|ch| {
            (ch.is_control() && !matches!(ch, '\n' | '\r' | '\t'))
                || unsafe_directional_char(ch)
        })
}

fn valid_kind(value: &str) -> bool {
    matches!(
        value,
        "UNKNOWN" | "HUMAN" | "SOFTWARE" | "MIXED" | "ORGANIZATION"
    )
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
    read_only: bool,
) -> Result<Transaction<'_, Postgres>, ActorError> {
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
        return Err(ActorError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

pub async fn create(
    pool: &PgPool,
    workspace: Uuid,
    actor: Uuid,
    actor_id: Uuid,
    display_label: &str,
) -> Result<(ActorView, bool), ActorError> {
    if !valid_label(display_label, 512) {
        return Err(ActorError::Invalid);
    }
    let display_label = display_label.trim();
    let mut tx = scoped(pool, workspace, false).await?;
    let inserted = sqlx::query(
        "INSERT INTO niccolo.actors(workspace_id,actor_id,display_label,created_by_principal_id)
         VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING",
    )
    .bind(workspace)
    .bind(actor_id)
    .bind(display_label)
    .bind(actor)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    let row: Option<(String, Uuid)> = sqlx::query_as(
        "SELECT display_label,created_by_principal_id
           FROM niccolo.actors WHERE workspace_id=$1 AND actor_id=$2",
    )
    .bind(workspace)
    .bind(actor_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((stored_label, stored_by)) = row else {
        return Err(ActorError::Invisible);
    };
    if stored_label != display_label || stored_by != actor {
        return Err(ActorError::Conflict);
    }
    tx.commit().await?;
    Ok((
        ActorView {
            actor_id,
            display_label: stored_label,
            effective_kind: None,
            kind_is_investigation_gate: false,
            kind_establishes_mind: false,
        },
        !inserted,
    ))
}


pub async fn list(pool: &PgPool, workspace: Uuid) -> Result<Vec<ActorView>, ActorError> {
    let mut tx = scoped(pool, workspace, true).await?;
    let rows: Vec<(Uuid, String, Option<String>)> = sqlx::query_as(
        "SELECT a.actor_id,a.display_label,
                (SELECT CASE WHEN e.operation='SET' THEN e.actor_kind ELSE NULL END
                   FROM niccolo.actor_kind_events e
                  WHERE e.workspace_id=a.workspace_id AND e.actor_id=a.actor_id
                  ORDER BY e.event_no DESC LIMIT 1)
           FROM niccolo.actors a
          WHERE a.workspace_id=$1
          ORDER BY a.created_at,a.actor_id
          LIMIT 256",
    )
    .bind(workspace)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(rows
        .into_iter()
        .map(|(actor_id, display_label, effective_kind)| ActorView {
            actor_id,
            display_label,
            effective_kind,
            kind_is_investigation_gate: false,
            kind_establishes_mind: false,
        })
        .collect())
}

pub async fn record_kind(
    pool: &PgPool,
    workspace: Uuid,
    recorder: Uuid,
    actor_id: Uuid,
    event_id: Uuid,
    actor_kind: Option<&str>,
    rationale: &str,
) -> Result<ActorEvent, ActorError> {
    if !valid_note(rationale, 2048)
        || actor_kind.is_some_and(|kind| !valid_kind(kind))
    {
        return Err(ActorError::Invalid);
    }
    let operation = if actor_kind.is_some() { "SET" } else { "CLEAR" };
    let mut tx = scoped(pool, workspace, false).await?;
    let visible: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM niccolo.actors WHERE workspace_id=$1 AND actor_id=$2)",
    )
    .bind(workspace)
    .bind(actor_id)
    .fetch_one(&mut *tx)
    .await?;
    if !visible {
        return Err(ActorError::Invisible);
    }
    let inserted = sqlx::query(
        "INSERT INTO niccolo.actor_kind_events
         (workspace_id,actor_id,event_id,operation,actor_kind,rationale,recorded_by_principal_id)
         VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT DO NOTHING",
    )
    .bind(workspace)
    .bind(actor_id)
    .bind(event_id)
    .bind(operation)
    .bind(actor_kind)
    .bind(rationale.trim())
    .bind(recorder)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    let row: Option<(i64, String, Option<String>, String, Uuid)> = sqlx::query_as(
        "SELECT event_no,operation,actor_kind,rationale,recorded_by_principal_id
           FROM niccolo.actor_kind_events
          WHERE workspace_id=$1 AND actor_id=$2 AND event_id=$3",
    )
    .bind(workspace)
    .bind(actor_id)
    .bind(event_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((event_no, stored_operation, stored_kind, stored_reason, stored_by)) = row else {
        return Err(ActorError::Invisible);
    };
    if stored_operation != operation
        || stored_kind.as_deref() != actor_kind
        || stored_reason != rationale.trim()
        || stored_by != recorder
    {
        return Err(ActorError::Conflict);
    }
    tx.commit().await?;
    Ok(ActorEvent {
        event_id,
        event_no,
        operation: stored_operation,
        actor_kind: stored_kind,
        rationale: stored_reason,
        recorded_by: stored_by,
        replayed: !inserted,
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "binding identity and source attribution are independent explicit inputs"
)]
pub async fn bind_source_speaker(
    pool: &PgPool,
    workspace: Uuid,
    recorder: Uuid,
    document_id: Uuid,
    speaker_label: &str,
    event_id: Uuid,
    actor_id: Option<Uuid>,
    rationale: &str,
) -> Result<ActorBinding, ActorError> {
    if !valid_label(speaker_label, 512) || !valid_note(rationale, 2048) {
        return Err(ActorError::Invalid);
    }
    let operation = if actor_id.is_some() { "ASSIGN" } else { "CLEAR" };
    let speaker_label = speaker_label.trim();
    let mut tx = scoped(pool, workspace, false).await?;
    let inserted = sqlx::query(
        "INSERT INTO niccolo.actor_binding_events
         (workspace_id,document_id,speaker_label,event_id,operation,actor_id,rationale,recorded_by_principal_id)
         VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING",
    )
    .bind(workspace)
    .bind(document_id)
    .bind(speaker_label)
    .bind(event_id)
    .bind(operation)
    .bind(actor_id)
    .bind(rationale.trim())
    .bind(recorder)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    let row: Option<(i64, String, Option<Uuid>, String, Uuid)> = sqlx::query_as(
        "SELECT event_no,operation,actor_id,rationale,recorded_by_principal_id
           FROM niccolo.actor_binding_events
          WHERE workspace_id=$1 AND document_id=$2 AND speaker_label=$3 AND event_id=$4",
    )
    .bind(workspace)
    .bind(document_id)
    .bind(speaker_label)
    .bind(event_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((event_no, stored_operation, stored_actor, stored_reason, stored_by)) = row else {
        return Err(ActorError::Invisible);
    };
    if stored_operation != operation
        || stored_actor != actor_id
        || stored_reason != rationale.trim()
        || stored_by != recorder
    {
        return Err(ActorError::Conflict);
    }
    tx.commit().await?;
    Ok(ActorBinding {
        event_id,
        event_no,
        document_id,
        speaker_label: speaker_label.into(),
        operation: stored_operation,
        actor_id: stored_actor,
        rationale: stored_reason,
        recorded_by: stored_by,
        replayed: !inserted,
    })
}

pub async fn frozen_bindings(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    revision: i64,
    kind_event_no_max: i64,
    binding_event_no_max: i64,
) -> Result<Vec<FrozenActorBinding>, ActorError> {
    let mut tx = scoped(pool, workspace, true).await?;
    let rows: Vec<(Uuid, String, String, Option<Uuid>, Uuid, i64)> = sqlx::query_as(
        "SELECT DISTINCT ON (b.document_id,b.speaker_label)
                b.document_id,b.speaker_label,b.operation,b.actor_id,b.event_id,b.event_no
           FROM niccolo.situation_revision_sources rs
           JOIN niccolo.actor_binding_events b
             ON b.workspace_id=rs.workspace_id AND b.document_id=rs.document_id
          WHERE rs.workspace_id=$1 AND rs.situation_id=$2 AND rs.revision=$3
            AND b.event_no <= $4
          ORDER BY b.document_id,b.speaker_label,b.event_no DESC",
    )
    .bind(workspace)
    .bind(situation)
    .bind(revision)
    .bind(binding_event_no_max)
    .fetch_all(&mut *tx)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for (document_id, speaker_label, operation, actor_id, event_id, event_no) in rows {
        let (actor_id, display_label, actor_kind) = if operation == "ASSIGN" {
            let Some(actor_id) = actor_id else {
                return Err(ActorError::Conflict);
            };
            let display_label: Option<String> = sqlx::query_scalar(
                "SELECT display_label FROM niccolo.actors WHERE workspace_id=$1 AND actor_id=$2",
            )
            .bind(workspace)
            .bind(actor_id)
            .fetch_optional(&mut *tx)
            .await?;
            let actor_kind: Option<String> = sqlx::query_scalar(
                "SELECT actor_kind
                   FROM niccolo.actor_kind_events
                  WHERE workspace_id=$1 AND actor_id=$2 AND event_no <= $3
                  ORDER BY event_no DESC LIMIT 1",
            )
            .bind(workspace)
            .bind(actor_id)
            .bind(kind_event_no_max)
            .fetch_optional(&mut *tx)
            .await?
            .flatten();
            (Some(actor_id), display_label, actor_kind)
        } else {
            (None, None, None)
        };
        out.push(FrozenActorBinding {
            document_id,
            speaker_label,
            actor_id,
            display_label,
            actor_kind,
            binding_event_id: event_id,
            binding_event_no: event_no,
            actor_kind_is_revisable_metadata: true,
            actor_kind_establishes_mind: false,
        });
    }
    tx.commit().await?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actor_labels_reject_control_and_directional_spoofing() {
        assert!(valid_label("Ana María", 512));
        assert!(!valid_label("Ana\nFake", 512));
        assert!(!valid_label("Ana\u{202e}Fake", 512));
        assert!(valid_note("Correction after review.\nSecond line.", 2048));
        assert!(!valid_note("Correction\u{202e}spoof", 2048));
        assert!(!valid_note("Correction\u{1b}[2J", 2048));
    }

    #[test]
    fn actor_kind_is_metadata_not_investigation_gate() {
        for kind in ["UNKNOWN", "HUMAN", "SOFTWARE", "MIXED", "ORGANIZATION"] {
            assert!(valid_kind(kind));
        }
        assert!(!valid_kind("HAS_MIND"));
    }
}
