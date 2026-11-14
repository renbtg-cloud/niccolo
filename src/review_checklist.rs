//! v0.101-v0.105: explicit human-owned review checklist state.
use crate::review_workflow::ReviewSessionScope;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum ChecklistError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("privileged SQL login prohibited")]
    Privileged,
    #[error("invalid checklist input")]
    Invalid,
    #[error("checklist replay conflicts with stored content")]
    Conflict,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChecklistEvent {
    pub event_id: Uuid,
    pub kind: String,
    pub note: String,
    pub recorded_by: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChecklistState {
    pub item_id: Uuid,
    pub text: String,
    pub state: String,
    pub closed_for_session: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChecklistSummary {
    pub items: Vec<ChecklistState>,
    pub open_count: i64,
    pub done_count: i64,
    pub waived_count: i64,
    pub complete_for_session: bool,
    pub complete_means_analysis_approved: bool,
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
    read_only: bool,
) -> Result<Transaction<'_, Postgres>, ChecklistError> {
    let mut tx = pool.begin().await?;
    sqlx::query(if read_only {
        "SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY"
    } else {
        "SET TRANSACTION ISOLATION LEVEL READ COMMITTED"
    })
    .execute(&mut *tx)
    .await?;
    let privileged: bool = sqlx::query_scalar(
        "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(&mut *tx)
    .await?;
    if privileged {
        return Err(ChecklistError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

fn valid_text(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 4096 && !value.chars().any(char::is_control)
}

fn valid_scope(scope: ReviewSessionScope) -> bool {
    !scope.workspace.is_nil() && !scope.situation.is_nil() && !scope.session.is_nil()
}

pub async fn create_item(
    pool: &PgPool,
    scope: ReviewSessionScope,
    item: Uuid,
    text: &str,
    principal: Uuid,
) -> Result<(), ChecklistError> {
    if !valid_scope(scope) || item.is_nil() || principal.is_nil() || !valid_text(text) {
        return Err(ChecklistError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, false).await?;
    sqlx::query("INSERT INTO niccolo.situation_review_checklist_items(workspace_id,situation_id,session_id,item_id,item_text,created_by_principal_id) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT DO NOTHING")
        .bind(scope.workspace).bind(scope.situation).bind(scope.session).bind(item).bind(text).bind(principal).execute(&mut *tx).await?;
    let stored: Option<(String, Uuid)> = sqlx::query_as(
        "SELECT item_text,created_by_principal_id
           FROM niccolo.situation_review_checklist_items
          WHERE workspace_id=$1 AND situation_id=$2 AND session_id=$3 AND item_id=$4",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.session)
    .bind(item)
    .fetch_optional(&mut *tx)
    .await?;
    if stored != Some((text.to_owned(), principal)) {
        return Err(ChecklistError::Conflict);
    }
    tx.commit().await?;
    Ok(())
}

pub async fn append_event(
    pool: &PgPool,
    scope: ReviewSessionScope,
    item: Uuid,
    event: &ChecklistEvent,
) -> Result<(), ChecklistError> {
    if !valid_scope(scope)
        || item.is_nil()
        || event.event_id.is_nil()
        || event.recorded_by.is_nil()
        || !matches!(event.kind.as_str(), "DONE" | "REOPENED" | "WAIVED")
        || !valid_text(&event.note)
    {
        return Err(ChecklistError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, false).await?;
    sqlx::query("INSERT INTO niccolo.situation_review_checklist_events(workspace_id,situation_id,session_id,item_id,event_id,event_kind,note,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING")
        .bind(scope.workspace).bind(scope.situation).bind(scope.session).bind(item).bind(event.event_id).bind(&event.kind).bind(&event.note).bind(event.recorded_by).execute(&mut *tx).await?;
    let stored: Option<(Uuid, String, String, Uuid)> = sqlx::query_as(
        "SELECT item_id,event_kind,note,recorded_by_principal_id
           FROM niccolo.situation_review_checklist_events
          WHERE workspace_id=$1 AND situation_id=$2 AND session_id=$3 AND event_id=$4",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.session)
    .bind(event.event_id)
    .fetch_optional(&mut *tx)
    .await?;
    if stored != Some((item, event.kind.clone(), event.note.clone(), event.recorded_by)) {
        return Err(ChecklistError::Conflict);
    }
    tx.commit().await?;
    Ok(())
}

pub async fn summary(
    pool: &PgPool,
    scope: ReviewSessionScope,
) -> Result<ChecklistSummary, ChecklistError> {
    if !valid_scope(scope) {
        return Err(ChecklistError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, true).await?;
    let rows: Vec<(Uuid,String,Option<String>)> = sqlx::query_as("SELECT i.item_id,i.item_text,(SELECT e.event_kind FROM niccolo.situation_review_checklist_events e WHERE e.workspace_id=i.workspace_id AND e.situation_id=i.situation_id AND e.session_id=i.session_id AND e.item_id=i.item_id ORDER BY e.created_at DESC,e.event_id DESC LIMIT 1) FROM niccolo.situation_review_checklist_items i WHERE i.workspace_id=$1 AND i.situation_id=$2 AND i.session_id=$3 ORDER BY i.item_id")
        .bind(scope.workspace).bind(scope.situation).bind(scope.session).fetch_all(&mut *tx).await?;
    tx.commit().await?;
    let items: Vec<_> = rows
        .into_iter()
        .map(|(item_id, text, state)| {
            let state = state.unwrap_or_else(|| "OPEN".into());
            let closed_for_session = matches!(state.as_str(), "DONE" | "WAIVED");
            ChecklistState {
                item_id,
                text,
                state,
                closed_for_session,
            }
        })
        .collect();
    let open_count = items.iter().filter(|x| !x.closed_for_session).count() as i64;
    let done_count = items.iter().filter(|x| x.state == "DONE").count() as i64;
    let waived_count = items.iter().filter(|x| x.state == "WAIVED").count() as i64;
    Ok(ChecklistSummary {
        complete_for_session: !items.is_empty() && open_count == 0,
        items,
        open_count,
        done_count,
        waived_count,
        complete_means_analysis_approved: false,
    })
}

pub fn canonical_json(value: &ChecklistSummary) -> Result<String, serde_json::Error> {
    serde_json::to_string(value)
}
pub fn sha256(value: &ChecklistSummary) -> Result<String, serde_json::Error> {
    let bytes = canonical_json(value)?;
    Ok(format!("{:x}", Sha256::digest(bytes.as_bytes())))
}


#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn nil_review_scope_is_rejected_without_database_access() {
        let pool = PgPool::connect_lazy("postgres://nobody@127.0.0.1/unused").unwrap();
        let scope = ReviewSessionScope {
            workspace: Uuid::nil(),
            situation: Uuid::new_v4(),
            session: Uuid::new_v4(),
        };
        assert!(matches!(
            create_item(
                &pool,
                scope,
                Uuid::new_v4(),
                "Review this",
                Uuid::new_v4()
            )
            .await,
            Err(ChecklistError::Invalid)
        ));
    }

    #[test]
    fn checklist_text_rejects_control_characters() {
        for value in ["item\nforged", "note\rforged", "note\tforged", "x\0y"] {
            assert!(!valid_text(value));
        }
        assert!(valid_text("Confirm the unresolved contradiction with the user."));
    }
}
