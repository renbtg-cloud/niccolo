//! Append-only declared-purpose history for a Situation.
//! Purpose is human-owned context. It is not evidence and never rewrites earlier analysis snapshots.

use crate::situation_store;
use serde::Serialize;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum PurposeError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("privileged SQL login prohibited")]
    Privileged,
    #[error("invalid purpose input")]
    Invalid,
    #[error("situation is not visible")]
    Invisible,
    #[error("purpose event identifier conflicts with stored state")]
    Conflict,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PurposeEvent {
    pub event_id: Uuid,
    pub event_no: i64,
    pub purpose_text: String,
    pub reason: String,
    pub downstream_use: Option<String>,
    pub recorded_by: Uuid,
    pub replayed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewPurpose<'a> {
    pub purpose_text: &'a str,
    pub reason: &'a str,
    pub downstream_use: Option<&'a str>,
}

fn clean(input: &str) -> Result<String, PurposeError> {
    situation_store::clean_text(input).map_err(|_| PurposeError::Invalid)
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
    read_only: bool,
) -> Result<Transaction<'_, Postgres>, PurposeError> {
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
        return Err(PurposeError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

pub async fn record(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    event_id: Uuid,
    actor: Uuid,
    input: NewPurpose<'_>,
) -> Result<PurposeEvent, PurposeError> {
    let purpose_text = clean(input.purpose_text)?;
    let reason = clean(input.reason)?;
    let downstream_use = input.downstream_use.map(clean).transpose()?;
    let mut tx = scoped(pool, workspace, false).await?;

    let visible: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM niccolo.situations WHERE workspace_id=$1 AND situation_id=$2)",
    )
    .bind(workspace)
    .bind(situation)
    .fetch_one(&mut *tx)
    .await?;
    if !visible {
        return Err(PurposeError::Invisible);
    }

    let stored: Option<(i64, String, String, Option<String>, Uuid)> = sqlx::query_as(
        "SELECT event_no,purpose_text,reason,downstream_use,recorded_by_principal_id
           FROM niccolo.situation_purpose_events
          WHERE workspace_id=$1 AND situation_id=$2 AND event_id=$3",
    )
    .bind(workspace)
    .bind(situation)
    .bind(event_id)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some((event_no, old_purpose, old_reason, old_use, old_actor)) = stored {
        if old_purpose != purpose_text
            || old_reason != reason
            || old_use != downstream_use
            || old_actor != actor
        {
            return Err(PurposeError::Conflict);
        }
        tx.commit().await?;
        return Ok(PurposeEvent {
            event_id,
            event_no,
            purpose_text: old_purpose,
            reason: old_reason,
            downstream_use: old_use,
            recorded_by: old_actor,
            replayed: true,
        });
    }

    let event_no: i64 = sqlx::query_scalar(
        "INSERT INTO niccolo.situation_purpose_events
         (workspace_id,situation_id,event_id,purpose_text,reason,downstream_use,recorded_by_principal_id)
         VALUES($1,$2,$3,$4,$5,$6,$7)
         RETURNING event_no",
    )
    .bind(workspace)
    .bind(situation)
    .bind(event_id)
    .bind(&purpose_text)
    .bind(&reason)
    .bind(&downstream_use)
    .bind(actor)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(PurposeEvent {
        event_id,
        event_no,
        purpose_text,
        reason,
        downstream_use,
        recorded_by: actor,
        replayed: false,
    })
}

pub async fn history(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
) -> Result<Vec<PurposeEvent>, PurposeError> {
    let mut tx = scoped(pool, workspace, true).await?;
    let visible: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM niccolo.situations WHERE workspace_id=$1 AND situation_id=$2)",
    )
    .bind(workspace)
    .bind(situation)
    .fetch_one(&mut *tx)
    .await?;
    if !visible {
        return Err(PurposeError::Invisible);
    }
    let rows: Vec<(Uuid, i64, String, String, Option<String>, Uuid)> = sqlx::query_as(
        "SELECT event_id,event_no,purpose_text,reason,downstream_use,recorded_by_principal_id
           FROM niccolo.situation_purpose_events
          WHERE workspace_id=$1 AND situation_id=$2
          ORDER BY event_no",
    )
    .bind(workspace)
    .bind(situation)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(rows
        .into_iter()
        .map(
            |(event_id, event_no, purpose_text, reason, downstream_use, recorded_by)| {
                PurposeEvent {
                    event_id,
                    event_no,
                    purpose_text,
                    reason,
                    downstream_use,
                    recorded_by,
                    replayed: false,
                }
            },
        )
        .collect())
}

pub(crate) async fn latest_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    situation: Uuid,
) -> Result<Option<(i64, String)>, PurposeError> {
    Ok(sqlx::query_as(
        "SELECT event_no,purpose_text
           FROM niccolo.situation_purpose_events
          WHERE workspace_id=$1 AND situation_id=$2
          ORDER BY event_no DESC
          LIMIT 1",
    )
    .bind(workspace)
    .bind(situation)
    .fetch_optional(&mut **tx)
    .await?)
}
