//! v0.43-v0.44: action proposals and append-only human decisions.
//! A proposal is never execution; ACCEPTED is not an execution receipt.
use crate::analysis_structure::AnalysisScope;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum ActionError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("privileged SQL login prohibited")]
    Privileged,
    #[error("invalid action request")]
    Invalid,
    #[error("idempotency conflict")]
    Conflict,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionProposal {
    pub proposal_id: Uuid,
    pub claim_id: Option<Uuid>,
    pub action_text: String,
    pub rationale: String,
    pub risk_note: Option<String>,
    pub proposed_by: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionDecision {
    pub proposal_id: Uuid,
    pub decision_id: Uuid,
    pub kind: String,
    pub note: String,
    pub decided_by: Uuid,
}

type ProposalRow = (Option<Uuid>, String, String, Option<String>, Uuid);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgencyReport {
    pub proposal_count: i64,
    pub decision_count: i64,
    pub accepted_count: i64,
    pub rejected_count: i64,
    pub deferred_count: i64,
    pub modified_count: i64,
    pub undecided_count: i64,
    pub accepted_means_executed: bool,
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
    read_only: bool,
) -> Result<Transaction<'_, Postgres>, ActionError> {
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
        return Err(ActionError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

pub async fn record_proposal(
    pool: &PgPool,
    scope: AnalysisScope,
    p: &ActionProposal,
) -> Result<(), ActionError> {
    if p.action_text.trim().is_empty()
        || p.action_text.len() > 4096
        || p.rationale.trim().is_empty()
        || p.rationale.len() > 4096
        || p.risk_note
            .as_ref()
            .is_some_and(|v| v.trim().is_empty() || v.len() > 4096)
    {
        return Err(ActionError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, false).await?;
    sqlx::query("INSERT INTO niccolo.situation_analysis_action_proposals \
        (workspace_id,situation_id,analysis_id,proposal_id,claim_id,action_text,rationale,risk_note,proposed_by_principal_id) \
        VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT DO NOTHING")
        .bind(scope.workspace).bind(scope.situation).bind(scope.analysis).bind(p.proposal_id)
        .bind(p.claim_id).bind(&p.action_text).bind(&p.rationale).bind(&p.risk_note).bind(p.proposed_by)
        .execute(&mut *tx).await?;
    let existing: Option<ProposalRow> = sqlx::query_as(
        "SELECT claim_id,action_text,rationale,risk_note,proposed_by_principal_id FROM niccolo.situation_analysis_action_proposals \
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND proposal_id=$4")
        .bind(scope.workspace).bind(scope.situation).bind(scope.analysis).bind(p.proposal_id)
        .fetch_optional(&mut *tx).await?;
    if existing
        != Some((
            p.claim_id,
            p.action_text.clone(),
            p.rationale.clone(),
            p.risk_note.clone(),
            p.proposed_by,
        ))
    {
        return Err(ActionError::Conflict);
    }
    tx.commit().await?;
    Ok(())
}

pub async fn append_decision(
    pool: &PgPool,
    scope: AnalysisScope,
    d: &ActionDecision,
) -> Result<(), ActionError> {
    if !matches!(
        d.kind.as_str(),
        "ACCEPTED" | "REJECTED" | "DEFERRED" | "MODIFIED"
    ) || d.note.trim().is_empty()
        || d.note.len() > 4096
    {
        return Err(ActionError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, false).await?;
    sqlx::query("INSERT INTO niccolo.situation_analysis_action_decisions \
        (workspace_id,situation_id,analysis_id,proposal_id,decision_id,decision_kind,note,decided_by_principal_id) \
        VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING")
        .bind(scope.workspace).bind(scope.situation).bind(scope.analysis).bind(d.proposal_id)
        .bind(d.decision_id).bind(&d.kind).bind(&d.note).bind(d.decided_by)
        .execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn agency_report(
    pool: &PgPool,
    scope: AnalysisScope,
) -> Result<AgencyReport, ActionError> {
    let mut tx = scoped(pool, scope.workspace, true).await?;
    let proposal_count: i64 = sqlx::query_scalar("SELECT count(*) FROM niccolo.situation_analysis_action_proposals WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3")
        .bind(scope.workspace).bind(scope.situation).bind(scope.analysis).fetch_one(&mut *tx).await?;
    let (decision_count, accepted_count, rejected_count, deferred_count, modified_count): (i64,i64,i64,i64,i64) = sqlx::query_as(
        "SELECT count(*),count(*) FILTER(WHERE decision_kind='ACCEPTED'),count(*) FILTER(WHERE decision_kind='REJECTED'),\
         count(*) FILTER(WHERE decision_kind='DEFERRED'),count(*) FILTER(WHERE decision_kind='MODIFIED') \
         FROM niccolo.situation_analysis_action_decisions WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3")
        .bind(scope.workspace).bind(scope.situation).bind(scope.analysis).fetch_one(&mut *tx).await?;
    let undecided_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM niccolo.situation_analysis_action_proposals p WHERE p.workspace_id=$1 AND p.situation_id=$2 AND p.analysis_id=$3 \
         AND NOT EXISTS(SELECT 1 FROM niccolo.situation_analysis_action_decisions d WHERE d.workspace_id=p.workspace_id AND d.situation_id=p.situation_id AND d.analysis_id=p.analysis_id AND d.proposal_id=p.proposal_id)")
        .bind(scope.workspace).bind(scope.situation).bind(scope.analysis).fetch_one(&mut *tx).await?;
    tx.commit().await?;
    Ok(AgencyReport {
        proposal_count,
        decision_count,
        accepted_count,
        rejected_count,
        deferred_count,
        modified_count,
        undecided_count,
        accepted_means_executed: false,
    })
}
