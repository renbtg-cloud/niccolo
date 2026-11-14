//! v0.90: deterministic reasoning handoff bundle for a human or another agent.
//! A handoff transfers inspectable state; it delegates neither authority nor execution.
use crate::analysis_assumptions;
use crate::analysis_claims;
use crate::analysis_structure::AnalysisScope;
use crate::{
    collection_provenance, decision_points, decision_readiness, hypothesis_portfolio,
    integrity_scope, source_scope, temporal_state,
};
use crate::review_workflow;
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum HandoffError {
    #[error("claim read failed: {0}")]
    Claims(#[from] analysis_claims::ClaimError),
    #[error("analysis lens read failed: {0}")]
    Assumptions(#[from] analysis_assumptions::AssumptionError),
    #[error("review read failed: {0}")]
    Review(#[from] review_workflow::ReviewWorkflowError),
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("handoff dependency failed: {0}")]
    Dependency(String),
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
pub struct HandoffClarification {
    pub question_id: Uuid,
    pub text: String,
    pub reason: String,
    pub input_revision: i64,
    pub answer_count: i64,
    pub answered: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HandoffCandidateInterpretation {
    pub interpretation_id: Uuid,
    pub text: String,
    pub rationale: String,
    pub is_fact: bool,
    pub automatically_ranked: bool,
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
    pub clarifications: Vec<HandoffClarification>,
    pub candidate_interpretations: Vec<HandoffCandidateInterpretation>,
    pub revision_conditions: Vec<HandoffCondition>,
    pub hypothesis_portfolio: hypothesis_portfolio::HypothesisPortfolio,
    pub temporal_assertions: Vec<temporal_state::TemporalAssertion>,
    pub source_scope: Vec<source_scope::SourceScopeAssertion>,
    pub collection_events: Vec<collection_provenance::CollectionEvent>,
    pub integrity_assessments: Vec<integrity_scope::IntegrityAssessment>,
    pub decisions: decision_points::DecisionPackage,
    pub decision_readiness: Vec<decision_readiness::DecisionReadinessView>,
    pub clarifications_are_evidence: bool,
    pub candidate_interpretations_are_facts: bool,
    pub candidate_interpretations_are_automatically_ranked: bool,
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
    let clarification_rows: Vec<(Uuid, String, String, i64, i64)> = sqlx::query_as(
        "SELECT q.question_id,q.question,c.reason,s.input_revision,
                (SELECT count(*) FROM niccolo.situation_analysis_clarification_answers a
                 JOIN niccolo.situation_contributions ac
                   ON ac.workspace_id=a.workspace_id
                  AND ac.situation_id=a.situation_id
                  AND ac.contribution_id=a.contribution_id
                 WHERE a.workspace_id=c.workspace_id AND a.situation_id=c.situation_id
                   AND a.analysis_id=c.analysis_id AND a.question_id=c.question_id
                   AND ac.status='ACTIVE')::bigint
         FROM niccolo.situation_analysis_clarification_questions c
         JOIN niccolo.investigative_questions q USING(workspace_id,question_id)
         JOIN niccolo.situation_analysis_snapshots s USING(workspace_id,situation_id,analysis_id)
         WHERE c.workspace_id=$1 AND c.situation_id=$2 AND c.analysis_id=$3
         ORDER BY q.question_id",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .fetch_all(&mut *tx)
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
    let clarifications = clarification_rows
        .into_iter()
        .map(
            |(question_id, text, reason, input_revision, answer_count)| HandoffClarification {
                question_id,
                text,
                reason,
                input_revision,
                answer_count,
                answered: answer_count > 0,
            },
        )
        .collect();
    let candidate_interpretations = analysis_assumptions::candidate_interpretations(pool, scope)
        .await?
        .into_iter()
        .map(|candidate| HandoffCandidateInterpretation {
            interpretation_id: candidate.interpretation_id,
            text: candidate.text,
            rationale: candidate.rationale,
            is_fact: false,
            automatically_ranked: false,
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
    let dep = |e: &dyn std::fmt::Display| HandoffError::Dependency(e.to_string());
    let hypothesis_portfolio = hypothesis_portfolio::portfolio(pool, scope)
        .await
        .map_err(|e| dep(&e))?;
    let temporal_assertions = temporal_state::list(pool, scope).await.map_err(|e| dep(&e))?;
    let source_scope = source_scope::list(pool, scope).await.map_err(|e| dep(&e))?;
    let collection_events = collection_provenance::list(pool, scope)
        .await
        .map_err(|e| dep(&e))?;
    let integrity_assessments = integrity_scope::list(pool, scope)
        .await
        .map_err(|e| dep(&e))?;
    let decisions = decision_points::package(pool, scope).await.map_err(|e| dep(&e))?;
    let decision_readiness = decision_readiness::latest(pool, scope)
        .await
        .map_err(|e| dep(&e))?;
    Ok(ReasoningHandoffBundle {
        format: "niccolo.reasoning-handoff".into(),
        version: 3,
        workspace_id: scope.workspace,
        situation_id: scope.situation,
        analysis_id: scope.analysis,
        goal,
        claims,
        open_questions,
        clarifications,
        candidate_interpretations,
        revision_conditions,
        hypothesis_portfolio,
        temporal_assertions,
        source_scope,
        collection_events,
        integrity_assessments,
        decisions,
        decision_readiness,
        clarifications_are_evidence: false,
        candidate_interpretations_are_facts: false,
        candidate_interpretations_are_automatically_ranked: false,
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
