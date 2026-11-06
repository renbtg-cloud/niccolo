//! v0.67-v0.68: explicit authorization is separate from proposal, acceptance, and execution.
use crate::analysis_structure::AnalysisScope;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum AuthorizationError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("privileged SQL login prohibited")]
    Privileged,
    #[error("invalid authorization request")]
    Invalid,
    #[error("idempotency conflict")]
    Conflict,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorizationEvent {
    pub proposal_id: Uuid,
    pub authorization_id: Uuid,
    pub kind: String,
    pub accepted_decision_id: Option<Uuid>,
    pub scope: String,
    pub recorded_by: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorizationStatus {
    pub proposal_id: Uuid,
    pub authorized: bool,
    pub authorization_scope: Option<String>,
    pub authorization_is_execution: bool,
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
    read_only: bool,
) -> Result<Transaction<'_, Postgres>, AuthorizationError> {
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
        return Err(AuthorizationError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

pub async fn append_event(
    pool: &PgPool,
    scope: AnalysisScope,
    event: &AuthorizationEvent,
) -> Result<(), AuthorizationError> {
    if !matches!(event.kind.as_str(), "AUTHORIZED" | "REVOKED")
        || event.scope.trim().is_empty()
        || event.scope.len() > 4096
        || (event.kind == "AUTHORIZED" && event.accepted_decision_id.is_none())
        || (event.kind == "REVOKED" && event.accepted_decision_id.is_some())
    {
        return Err(AuthorizationError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, false).await?;
    sqlx::query(
        "INSERT INTO niccolo.situation_action_authorization_events \
         (workspace_id,situation_id,analysis_id,proposal_id,authorization_id,event_kind,accepted_decision_id,authorization_scope,recorded_by_principal_id) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT DO NOTHING",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(event.proposal_id)
    .bind(event.authorization_id)
    .bind(&event.kind)
    .bind(event.accepted_decision_id)
    .bind(&event.scope)
    .bind(event.recorded_by)
    .execute(&mut *tx)
    .await?;
    let existing: Option<(String, Option<Uuid>, String, Uuid)> = sqlx::query_as(
        "SELECT event_kind,accepted_decision_id,authorization_scope,recorded_by_principal_id \
         FROM niccolo.situation_action_authorization_events \
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND proposal_id=$4 AND authorization_id=$5",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(event.proposal_id)
    .bind(event.authorization_id)
    .fetch_optional(&mut *tx)
    .await?;
    if existing
        != Some((
            event.kind.clone(),
            event.accepted_decision_id,
            event.scope.clone(),
            event.recorded_by,
        ))
    {
        return Err(AuthorizationError::Conflict);
    }
    tx.commit().await?;
    Ok(())
}

pub async fn statuses(
    pool: &PgPool,
    scope: AnalysisScope,
) -> Result<Vec<AuthorizationStatus>, AuthorizationError> {
    let mut tx = scoped(pool, scope.workspace, true).await?;
    let rows: Vec<(Uuid, String, String)> = sqlx::query_as(
        "SELECT proposal_id,event_kind,authorization_scope FROM (\
          SELECT DISTINCT ON (proposal_id) proposal_id,event_kind,authorization_scope,created_at,authorization_id \
          FROM niccolo.situation_action_authorization_events \
          WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 \
          ORDER BY proposal_id,created_at DESC,authorization_id DESC\
        ) x ORDER BY proposal_id",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(rows
        .into_iter()
        .map(
            |(proposal_id, kind, authorization_scope)| AuthorizationStatus {
                proposal_id,
                authorized: kind == "AUTHORIZED",
                authorization_scope: (kind == "AUTHORIZED").then_some(authorization_scope),
                authorization_is_execution: false,
            },
        )
        .collect())
}

/// v0.66: lifecycle read model keeps proposal, human decision, authorization and execution distinct.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionLifecycle {
    pub proposal_id: Uuid,
    pub latest_decision: Option<String>,
    pub authorized: bool,
    pub executed: bool,
}

pub async fn lifecycle(
    pool: &PgPool,
    scope: AnalysisScope,
) -> Result<Vec<ActionLifecycle>, AuthorizationError> {
    let mut tx = scoped(pool, scope.workspace, true).await?;
    type Row = (Uuid, Option<String>, Option<String>);
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT p.proposal_id,\
          (SELECT d.decision_kind FROM niccolo.situation_analysis_action_decisions d \
           WHERE d.workspace_id=p.workspace_id AND d.situation_id=p.situation_id AND d.analysis_id=p.analysis_id AND d.proposal_id=p.proposal_id \
           ORDER BY d.created_at DESC,d.decision_id DESC LIMIT 1),\
          (SELECT a.event_kind FROM niccolo.situation_action_authorization_events a \
           WHERE a.workspace_id=p.workspace_id AND a.situation_id=p.situation_id AND a.analysis_id=p.analysis_id AND a.proposal_id=p.proposal_id \
           ORDER BY a.created_at DESC,a.authorization_id DESC LIMIT 1) \
         FROM niccolo.situation_analysis_action_proposals p \
         WHERE p.workspace_id=$1 AND p.situation_id=$2 AND p.analysis_id=$3 ORDER BY p.proposal_id",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(rows
        .into_iter()
        .map(
            |(proposal_id, latest_decision, authorization)| ActionLifecycle {
                proposal_id,
                latest_decision,
                authorized: authorization.as_deref() == Some("AUTHORIZED"),
                executed: false,
            },
        )
        .collect())
}
