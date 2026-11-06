//! v0.90: deterministic reasoning handoff bundle for a human or another agent.
//! A handoff transfers inspectable state; it delegates neither authority nor execution.
use crate::analysis_claims;
use crate::analysis_structure::AnalysisScope;
use crate::review_workflow;
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum HandoffError {
    #[error("claim read failed: {0}")]
    Claims(#[from] analysis_claims::ClaimError),
    #[error("review read failed: {0}")]
    Review(#[from] review_workflow::ReviewWorkflowError),
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
}

async fn scoped(pool: &PgPool, workspace: Uuid) -> Result<Transaction<'_, Postgres>, HandoffError> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *tx)
        .await?;
    let privileged: bool = sqlx::query_scalar(
        "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(&mut *tx)
    .await?;
    if privileged {
        return Err(sqlx::Error::Protocol("privileged SQL login prohibited".into()).into());
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HandoffClaim {
    pub claim_id: Uuid,
    pub ordinal: i32,
    pub kind: String,
    pub text: String,
    pub qualification: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HandoffOpenQuestion {
    pub claim_id: Uuid,
    pub question_id: Uuid,
    pub relation_kind: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HandoffCondition {
    pub claim_id: Uuid,
    pub condition_id: Uuid,
    pub trigger_kind: String,
    pub text: String,
    pub latest_event: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ReasoningHandoffBundle {
    pub format: String,
    pub version: u32,
    pub workspace_id: Uuid,
    pub situation_id: Uuid,
    pub analysis_id: Uuid,
    pub goal: String,
    pub claims: Vec<HandoffClaim>,
    pub open_questions: Vec<HandoffOpenQuestion>,
    pub revision_conditions: Vec<HandoffCondition>,
    pub handoff_grants_authority: bool,
    pub handoff_starts_analysis: bool,
    pub handoff_executes_actions: bool,
}

pub async fn bundle(
    pool: &PgPool,
    scope: AnalysisScope,
) -> Result<ReasoningHandoffBundle, HandoffError> {
    let mut tx = scoped(pool, scope.workspace).await?;
    let goal: String = sqlx::query_scalar(
        "SELECT goal FROM niccolo.situation_analysis_snapshots WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    let claims =
        analysis_claims::list_claims(pool, scope.workspace, scope.situation, scope.analysis)
            .await?
            .into_iter()
            .map(|c| HandoffClaim {
                claim_id: c.claim_id,
                ordinal: c.ordinal,
                kind: c.kind,
                text: c.text,
                qualification: c.qualification,
            })
            .collect();
    let open_questions = review_workflow::claim_questions(pool, scope)
        .await?
        .into_iter()
        .filter(|q| !q.answered)
        .map(|q| HandoffOpenQuestion {
            claim_id: q.claim_id,
            question_id: q.question_id,
            relation_kind: q.relation_kind,
        })
        .collect();
    let revision_conditions = review_workflow::revision_conditions(pool, scope)
        .await?
        .into_iter()
        .map(|c| HandoffCondition {
            claim_id: c.claim_id,
            condition_id: c.condition_id,
            trigger_kind: c.trigger_kind,
            text: c.text,
            latest_event: c.latest_event,
        })
        .collect();
    Ok(ReasoningHandoffBundle {
        format: "niccolo.reasoning-handoff".into(),
        version: 1,
        workspace_id: scope.workspace,
        situation_id: scope.situation,
        analysis_id: scope.analysis,
        goal,
        claims,
        open_questions,
        revision_conditions,
        handoff_grants_authority: false,
        handoff_starts_analysis: false,
        handoff_executes_actions: false,
    })
}

pub fn canonical_json(bundle: &ReasoningHandoffBundle) -> Result<String, HandoffError> {
    Ok(serde_json::to_string(bundle)?)
}

pub fn sha256(bundle: &ReasoningHandoffBundle) -> Result<String, HandoffError> {
    let json = canonical_json(bundle)?;
    Ok(format!("{:x}", Sha256::digest(json.as_bytes())))
}
