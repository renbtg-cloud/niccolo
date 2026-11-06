//! v0.161-v0.180: proposed source requests, purpose-bound support assessments,
//! explicit conflict sets, and deterministic handback to the human owner.
use crate::analysis_structure::AnalysisScope;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum HandoffError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("privileged SQL login prohibited")]
    Privileged,
    #[error("invalid handoff input")]
    Invalid,
}
async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
    ro: bool,
) -> Result<Transaction<'_, Postgres>, HandoffError> {
    let mut tx = pool.begin().await?;
    sqlx::query(if ro {
        "SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY"
    } else {
        "SET TRANSACTION ISOLATION LEVEL READ COMMITTED"
    })
    .execute(&mut *tx)
    .await?;
    let p: bool = sqlx::query_scalar(
        "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(&mut *tx)
    .await?;
    if p {
        return Err(HandoffError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}
fn valid(s: &str) -> bool {
    !s.trim().is_empty() && s.len() <= 4096
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceRequestInput {
    pub request_id: Uuid,
    pub text: String,
    pub source_kind: String,
    pub rationale: String,
    pub proposed_by: Uuid,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceRequestEvent {
    pub event_id: Uuid,
    pub kind: String,
    pub note: String,
    pub recorded_by: Uuid,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRequestView {
    pub request_id: Uuid,
    pub text: String,
    pub source_kind: String,
    pub rationale: String,
    pub state: String,
    pub accepted_means_fetched: bool,
}
pub async fn propose_source_request(
    pool: &PgPool,
    scope: AnalysisScope,
    r: &SourceRequestInput,
) -> Result<(), HandoffError> {
    if !valid(&r.text)
        || r.source_kind.trim().is_empty()
        || r.source_kind.len() > 256
        || !valid(&r.rationale)
    {
        return Err(HandoffError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, false).await?;
    sqlx::query("INSERT INTO niccolo.situation_analysis_source_requests(workspace_id,situation_id,analysis_id,request_id,request_text,source_kind,rationale,proposed_by_principal_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING").bind(scope.workspace).bind(scope.situation).bind(scope.analysis).bind(r.request_id).bind(&r.text).bind(&r.source_kind).bind(&r.rationale).bind(r.proposed_by).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}
pub async fn append_source_request_event(
    pool: &PgPool,
    scope: AnalysisScope,
    request: Uuid,
    e: &SourceRequestEvent,
) -> Result<(), HandoffError> {
    if !matches!(
        e.kind.as_str(),
        "ACCEPTED" | "DECLINED" | "DEFERRED" | "REOPENED"
    ) || !valid(&e.note)
    {
        return Err(HandoffError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, false).await?;
    sqlx::query("INSERT INTO niccolo.situation_analysis_source_request_events(workspace_id,situation_id,analysis_id,request_id,event_id,event_kind,note,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING").bind(scope.workspace).bind(scope.situation).bind(scope.analysis).bind(request).bind(e.event_id).bind(&e.kind).bind(&e.note).bind(e.recorded_by).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}
pub async fn source_requests(
    pool: &PgPool,
    scope: AnalysisScope,
) -> Result<Vec<SourceRequestView>, HandoffError> {
    let mut tx = scoped(pool, scope.workspace, true).await?;
    let rows:Vec<(Uuid,String,String,String,Option<String>)>=sqlx::query_as("SELECT r.request_id,r.request_text,r.source_kind,r.rationale,(SELECT e.event_kind FROM niccolo.situation_analysis_source_request_events e WHERE e.workspace_id=r.workspace_id AND e.situation_id=r.situation_id AND e.analysis_id=r.analysis_id AND e.request_id=r.request_id ORDER BY e.created_at DESC,e.event_id DESC LIMIT 1) FROM niccolo.situation_analysis_source_requests r WHERE r.workspace_id=$1 AND r.situation_id=$2 AND r.analysis_id=$3 ORDER BY r.request_id").bind(scope.workspace).bind(scope.situation).bind(scope.analysis).fetch_all(&mut *tx).await?;
    tx.commit().await?;
    Ok(rows
        .into_iter()
        .map(
            |(request_id, text, source_kind, rationale, state)| SourceRequestView {
                request_id,
                text,
                source_kind,
                rationale,
                state: state.unwrap_or_else(|| "PROPOSED".into()),
                accepted_means_fetched: false,
            },
        )
        .collect())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClaimAssessmentInput {
    pub assessment_id: Uuid,
    pub purpose: String,
    pub assessment: String,
    pub basis_note: String,
    pub recorded_by: Uuid,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimAssessmentView {
    pub assessment_id: Uuid,
    pub purpose: String,
    pub assessment: String,
    pub basis_note: String,
    pub establishes_truth: bool,
    pub reusable_for_other_purposes: bool,
}
pub async fn assess_claim(
    pool: &PgPool,
    scope: AnalysisScope,
    claim: Uuid,
    a: &ClaimAssessmentInput,
) -> Result<(), HandoffError> {
    if !valid(&a.purpose)
        || !matches!(
            a.assessment.as_str(),
            "SUFFICIENT_FOR_PURPOSE" | "INSUFFICIENT" | "CONTESTED" | "UNKNOWN"
        )
        || !valid(&a.basis_note)
    {
        return Err(HandoffError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, false).await?;
    sqlx::query("INSERT INTO niccolo.situation_analysis_claim_assessments(workspace_id,situation_id,analysis_id,claim_id,assessment_id,purpose,assessment,basis_note,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT DO NOTHING").bind(scope.workspace).bind(scope.situation).bind(scope.analysis).bind(claim).bind(a.assessment_id).bind(&a.purpose).bind(&a.assessment).bind(&a.basis_note).bind(a.recorded_by).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}
pub async fn claim_assessments(
    pool: &PgPool,
    scope: AnalysisScope,
    claim: Uuid,
) -> Result<Vec<ClaimAssessmentView>, HandoffError> {
    let mut tx = scoped(pool, scope.workspace, true).await?;
    let rows:Vec<(Uuid,String,String,String)>=sqlx::query_as("SELECT assessment_id,purpose,assessment,basis_note FROM niccolo.situation_analysis_claim_assessments WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND claim_id=$4 ORDER BY created_at,assessment_id").bind(scope.workspace).bind(scope.situation).bind(scope.analysis).bind(claim).fetch_all(&mut *tx).await?;
    tx.commit().await?;
    Ok(rows
        .into_iter()
        .map(
            |(assessment_id, purpose, assessment, basis_note)| ClaimAssessmentView {
                assessment_id,
                purpose,
                assessment,
                basis_note,
                establishes_truth: false,
                reusable_for_other_purposes: false,
            },
        )
        .collect())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConflictInput {
    pub conflict_id: Uuid,
    pub description: String,
    pub claim_ids: Vec<Uuid>,
    pub recorded_by: Uuid,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictView {
    pub conflict_id: Uuid,
    pub description: String,
    pub claim_ids: Vec<Uuid>,
    pub automatically_resolved: bool,
}
pub async fn record_conflict(
    pool: &PgPool,
    scope: AnalysisScope,
    c: &ConflictInput,
) -> Result<(), HandoffError> {
    if !valid(&c.description) || c.claim_ids.len() < 2 {
        return Err(HandoffError::Invalid);
    }
    let mut ids = c.claim_ids.clone();
    ids.sort_unstable();
    ids.dedup();
    if ids.len() < 2 {
        return Err(HandoffError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, false).await?;
    sqlx::query("INSERT INTO niccolo.situation_analysis_conflict_sets(workspace_id,situation_id,analysis_id,conflict_id,description,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT DO NOTHING").bind(scope.workspace).bind(scope.situation).bind(scope.analysis).bind(c.conflict_id).bind(&c.description).bind(c.recorded_by).execute(&mut *tx).await?;
    for id in ids {
        sqlx::query("INSERT INTO niccolo.situation_analysis_conflict_claims(workspace_id,situation_id,analysis_id,conflict_id,claim_id) VALUES($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING").bind(scope.workspace).bind(scope.situation).bind(scope.analysis).bind(c.conflict_id).bind(id).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(())
}
pub async fn conflicts(
    pool: &PgPool,
    scope: AnalysisScope,
) -> Result<Vec<ConflictView>, HandoffError> {
    let mut tx = scoped(pool, scope.workspace, true).await?;
    let rows:Vec<(Uuid,String)>=sqlx::query_as("SELECT conflict_id,description FROM niccolo.situation_analysis_conflict_sets WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 ORDER BY conflict_id").bind(scope.workspace).bind(scope.situation).bind(scope.analysis).fetch_all(&mut *tx).await?;
    let mut out = Vec::with_capacity(rows.len());
    for (conflict_id, description) in rows {
        let claim_ids:Vec<Uuid>=sqlx::query_scalar("SELECT claim_id FROM niccolo.situation_analysis_conflict_claims WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND conflict_id=$4 ORDER BY claim_id").bind(scope.workspace).bind(scope.situation).bind(scope.analysis).bind(conflict_id).fetch_all(&mut *tx).await?;
        out.push(ConflictView {
            conflict_id,
            description,
            claim_ids,
            automatically_resolved: false,
        });
    }
    tx.commit().await?;
    Ok(out)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanHandback {
    pub goal_count: i64,
    pub priority_count: i64,
    pub active_constraint_count: i64,
    pub proposed_or_open_next_step_count: i64,
    pub triggered_stop_count: i64,
    pub source_request_count: i64,
    pub conflict_count: i64,
    pub human_owns_project: bool,
    pub system_may_execute: bool,
    pub system_may_rewrite_goals: bool,
}
pub async fn handback(pool: &PgPool, scope: AnalysisScope) -> Result<HumanHandback, HandoffError> {
    let mut tx = scoped(pool, scope.workspace, true).await?;
    let goal_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM niccolo.situation_goals WHERE workspace_id=$1 AND situation_id=$2",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .fetch_one(&mut *tx)
    .await?;
    let priority_count:i64=sqlx::query_scalar("SELECT count(*) FROM niccolo.situation_human_priorities WHERE workspace_id=$1 AND situation_id=$2").bind(scope.workspace).bind(scope.situation).fetch_one(&mut *tx).await?;
    let active_constraint_count:i64=sqlx::query_scalar("SELECT count(*) FROM niccolo.situation_human_constraints c WHERE c.workspace_id=$1 AND c.situation_id=$2 AND COALESCE((SELECT e.event_kind FROM niccolo.situation_human_constraint_events e WHERE e.workspace_id=c.workspace_id AND e.situation_id=c.situation_id AND e.constraint_id=c.constraint_id ORDER BY e.created_at DESC,e.event_id DESC LIMIT 1),'ACTIVE')<>'WAIVED'").bind(scope.workspace).bind(scope.situation).fetch_one(&mut *tx).await?;
    let proposed_or_open_next_step_count:i64=sqlx::query_scalar("SELECT count(*) FROM niccolo.situation_analysis_next_steps s WHERE s.workspace_id=$1 AND s.situation_id=$2 AND s.analysis_id=$3 AND COALESCE((SELECT e.event_kind FROM niccolo.situation_analysis_next_step_events e WHERE e.workspace_id=s.workspace_id AND e.situation_id=s.situation_id AND e.analysis_id=s.analysis_id AND e.step_id=s.step_id ORDER BY e.created_at DESC,e.event_id DESC LIMIT 1),'PROPOSED') IN ('PROPOSED','REOPENED','DEFERRED')").bind(scope.workspace).bind(scope.situation).bind(scope.analysis).fetch_one(&mut *tx).await?;
    let triggered_stop_count:i64=sqlx::query_scalar("SELECT count(*) FROM niccolo.situation_analysis_stop_conditions c WHERE c.workspace_id=$1 AND c.situation_id=$2 AND c.analysis_id=$3 AND COALESCE((SELECT e.event_kind FROM niccolo.situation_analysis_stop_condition_events e WHERE e.workspace_id=c.workspace_id AND e.situation_id=c.situation_id AND e.analysis_id=c.analysis_id AND e.condition_id=c.condition_id ORDER BY e.created_at DESC,e.event_id DESC LIMIT 1),'CLEAR')='TRIGGERED'").bind(scope.workspace).bind(scope.situation).bind(scope.analysis).fetch_one(&mut *tx).await?;
    let source_request_count:i64=sqlx::query_scalar("SELECT count(*) FROM niccolo.situation_analysis_source_requests WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3").bind(scope.workspace).bind(scope.situation).bind(scope.analysis).fetch_one(&mut *tx).await?;
    let conflict_count:i64=sqlx::query_scalar("SELECT count(*) FROM niccolo.situation_analysis_conflict_sets WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3").bind(scope.workspace).bind(scope.situation).bind(scope.analysis).fetch_one(&mut *tx).await?;
    tx.commit().await?;
    Ok(HumanHandback {
        goal_count,
        priority_count,
        active_constraint_count,
        proposed_or_open_next_step_count,
        triggered_stop_count,
        source_request_count,
        conflict_count,
        human_owns_project: true,
        system_may_execute: false,
        system_may_rewrite_goals: false,
    })
}
pub fn handback_sha256(v: &HumanHandback) -> Result<String, serde_json::Error> {
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_string(v)?.as_bytes())
    ))
}
