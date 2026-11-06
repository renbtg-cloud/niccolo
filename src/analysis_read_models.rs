//! v0.47-v0.53: bounded read models for human inspection.
//! Counts describe recorded usage and steering; they do not establish truth or independence.
use crate::analysis_structure::AnalysisScope;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum ReadModelError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("privileged SQL login prohibited")]
    Privileged,
    #[error("analysis not visible")]
    Invisible,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimSupportSummary {
    pub claim_id: Uuid,
    pub supports_as_used: i64,
    pub contradicts_as_used: i64,
    pub context_only_fragments: i64,
    pub informs_context: i64,
    pub qualifies_context: i64,
    pub conflicts_with_reported_context: i64,
    pub link_counts_are_independent_evidence_count: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimReviewSummary {
    pub claim_id: Uuid,
    pub challenged: i64,
    pub insufficient_evidence: i64,
    pub unknown_to_user: i64,
    pub corrections: i64,
    pub comments: i64,
    pub review_mutates_claim: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReanalysisChangeSummary {
    pub prior_analysis_id: Uuid,
    pub new_analysis_id: Uuid,
    pub retained: i64,
    pub refined: i64,
    pub reversed: i64,
    pub dropped: i64,
    pub split: i64,
    pub merged: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClarificationEffectSummary {
    pub target_analysis_id: Uuid,
    pub informs: i64,
    pub qualifies: i64,
    pub resolves_for_this_analysis: i64,
    pub introduces_uncertainty: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalysisIndexItem {
    pub analysis_id: Uuid,
    pub input_revision: i64,
    pub current_revision: i64,
    pub input_is_current: bool,
    pub output_present: bool,
    pub claim_count: i64,
    pub uncertainty_count: i64,
    pub proposal_count: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttentionSummary {
    pub unresolved_uncertainties: i64,
    pub undecided_action_proposals: i64,
    pub automatically_prioritized: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanSteeringSummary {
    pub clarification_answers: i64,
    pub claim_reviews: i64,
    pub action_decisions: i64,
    pub uncertainty_events: i64,
    pub steering_is_future_policy: bool,
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
) -> Result<Transaction<'_, Postgres>, ReadModelError> {
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
        return Err(ReadModelError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

pub async fn claim_support(
    pool: &PgPool,
    scope: AnalysisScope,
    claim: Uuid,
) -> Result<ClaimSupportSummary, ReadModelError> {
    let mut tx = scoped(pool, scope.workspace).await?;
    let visible:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM niccolo.situation_analysis_claims WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND claim_id=$4)")
        .bind(scope.workspace).bind(scope.situation).bind(scope.analysis).bind(claim).fetch_one(&mut *tx).await?;
    if !visible {
        return Err(ReadModelError::Invisible);
    }
    let (supports,contradicts,context_only):(i64,i64,i64)=sqlx::query_as("SELECT count(*) FILTER(WHERE relation='SUPPORTS_AS_USED'),count(*) FILTER(WHERE relation='CONTRADICTS_AS_USED'),count(*) FILTER(WHERE relation='CONTEXT_ONLY') FROM niccolo.situation_analysis_claim_fragments WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND claim_id=$4")
        .bind(scope.workspace).bind(scope.situation).bind(scope.analysis).bind(claim).fetch_one(&mut *tx).await?;
    let (informs,qualifies,conflicts):(i64,i64,i64)=sqlx::query_as("SELECT count(*) FILTER(WHERE relation='INFORMS'),count(*) FILTER(WHERE relation='QUALIFIES'),count(*) FILTER(WHERE relation='CONFLICTS_WITH_REPORTED_CONTEXT') FROM niccolo.situation_analysis_claim_contributions WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND claim_id=$4")
        .bind(scope.workspace).bind(scope.situation).bind(scope.analysis).bind(claim).fetch_one(&mut *tx).await?;
    tx.commit().await?;
    Ok(ClaimSupportSummary {
        claim_id: claim,
        supports_as_used: supports,
        contradicts_as_used: contradicts,
        context_only_fragments: context_only,
        informs_context: informs,
        qualifies_context: qualifies,
        conflicts_with_reported_context: conflicts,
        link_counts_are_independent_evidence_count: false,
    })
}

pub async fn claim_reviews(
    pool: &PgPool,
    scope: AnalysisScope,
    claim: Uuid,
) -> Result<ClaimReviewSummary, ReadModelError> {
    let mut tx = scoped(pool, scope.workspace).await?;
    let row:Option<(i64,i64,i64,i64,i64)>=sqlx::query_as("SELECT count(*) FILTER(WHERE review_kind='CHALLENGED'),count(*) FILTER(WHERE review_kind='INSUFFICIENT_EVIDENCE'),count(*) FILTER(WHERE review_kind='UNKNOWN_TO_USER'),count(*) FILTER(WHERE review_kind='CORRECTION'),count(*) FILTER(WHERE review_kind='COMMENT') FROM niccolo.situation_analysis_claims c LEFT JOIN niccolo.situation_analysis_claim_reviews r USING(workspace_id,situation_id,analysis_id,claim_id) WHERE c.workspace_id=$1 AND c.situation_id=$2 AND c.analysis_id=$3 AND c.claim_id=$4 GROUP BY c.claim_id")
        .bind(scope.workspace).bind(scope.situation).bind(scope.analysis).bind(claim).fetch_optional(&mut *tx).await?;
    let Some((challenged, insufficient_evidence, unknown_to_user, corrections, comments)) = row
    else {
        return Err(ReadModelError::Invisible);
    };
    tx.commit().await?;
    Ok(ClaimReviewSummary {
        claim_id: claim,
        challenged,
        insufficient_evidence,
        unknown_to_user,
        corrections,
        comments,
        review_mutates_claim: false,
    })
}

pub async fn reanalysis_changes(
    pool: &PgPool,
    scope: AnalysisScope,
) -> Result<ReanalysisChangeSummary, ReadModelError> {
    let mut tx = scoped(pool, scope.workspace).await?;
    let prior:Option<Uuid>=sqlx::query_scalar("SELECT reanalysis_of FROM niccolo.situation_analysis_snapshots WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3")
        .bind(scope.workspace).bind(scope.situation).bind(scope.analysis).fetch_optional(&mut *tx).await?.flatten();
    let Some(prior_analysis_id) = prior else {
        return Err(ReadModelError::Invisible);
    };
    let (retained,refined,reversed,dropped,split,merged):(i64,i64,i64,i64,i64,i64)=sqlx::query_as("SELECT count(*) FILTER(WHERE relation='RETAINED'),count(*) FILTER(WHERE relation='REFINED'),count(*) FILTER(WHERE relation='REVERSED'),count(*) FILTER(WHERE relation='DROPPED'),count(*) FILTER(WHERE relation='SPLIT'),count(*) FILTER(WHERE relation='MERGED') FROM niccolo.situation_analysis_claim_lineage WHERE workspace_id=$1 AND situation_id=$2 AND new_analysis_id=$3")
        .bind(scope.workspace).bind(scope.situation).bind(scope.analysis).fetch_one(&mut *tx).await?;
    tx.commit().await?;
    Ok(ReanalysisChangeSummary {
        prior_analysis_id,
        new_analysis_id: scope.analysis,
        retained,
        refined,
        reversed,
        dropped,
        split,
        merged,
    })
}

pub async fn clarification_effects(
    pool: &PgPool,
    scope: AnalysisScope,
) -> Result<ClarificationEffectSummary, ReadModelError> {
    let mut tx = scoped(pool, scope.workspace).await?;
    let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM niccolo.situation_analysis_snapshots WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3)")
        .bind(scope.workspace).bind(scope.situation).bind(scope.analysis).fetch_one(&mut *tx).await?;
    if !exists {
        return Err(ReadModelError::Invisible);
    }
    let (informs,qualifies,resolves,introduces):(i64,i64,i64,i64)=sqlx::query_as("SELECT count(*) FILTER(WHERE recorded_effect='INFORMS'),count(*) FILTER(WHERE recorded_effect='QUALIFIES'),count(*) FILTER(WHERE recorded_effect='RESOLVES_FOR_THIS_ANALYSIS'),count(*) FILTER(WHERE recorded_effect='INTRODUCES_UNCERTAINTY') FROM niccolo.situation_analysis_clarification_claim_impacts WHERE workspace_id=$1 AND situation_id=$2 AND target_analysis_id=$3")
        .bind(scope.workspace).bind(scope.situation).bind(scope.analysis).fetch_one(&mut *tx).await?;
    tx.commit().await?;
    Ok(ClarificationEffectSummary {
        target_analysis_id: scope.analysis,
        informs,
        qualifies,
        resolves_for_this_analysis: resolves,
        introduces_uncertainty: introduces,
    })
}

pub async fn analysis_index(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
) -> Result<Vec<AnalysisIndexItem>, ReadModelError> {
    let mut tx = scoped(pool, workspace).await?;
    type Row = (Uuid, i64, i64, bool, i64, i64, i64);
    let rows:Vec<Row>=sqlx::query_as("SELECT a.analysis_id,a.input_revision,s.current_revision,EXISTS(SELECT 1 FROM niccolo.situation_analysis_outputs o WHERE o.workspace_id=a.workspace_id AND o.situation_id=a.situation_id AND o.analysis_id=a.analysis_id), (SELECT count(*) FROM niccolo.situation_analysis_claims c WHERE c.workspace_id=a.workspace_id AND c.situation_id=a.situation_id AND c.analysis_id=a.analysis_id), (SELECT count(*) FROM niccolo.situation_analysis_uncertainties u WHERE u.workspace_id=a.workspace_id AND u.situation_id=a.situation_id AND u.analysis_id=a.analysis_id), (SELECT count(*) FROM niccolo.situation_analysis_action_proposals p WHERE p.workspace_id=a.workspace_id AND p.situation_id=a.situation_id AND p.analysis_id=a.analysis_id) FROM niccolo.situation_analysis_snapshots a JOIN niccolo.situations s USING(workspace_id,situation_id) WHERE a.workspace_id=$1 AND a.situation_id=$2 ORDER BY a.input_revision,a.analysis_id")
        .bind(workspace).bind(situation).fetch_all(&mut *tx).await?;
    tx.commit().await?;
    Ok(rows
        .into_iter()
        .map(
            |(
                analysis_id,
                input_revision,
                current_revision,
                output_present,
                claim_count,
                uncertainty_count,
                proposal_count,
            )| AnalysisIndexItem {
                analysis_id,
                input_revision,
                current_revision,
                input_is_current: input_revision == current_revision,
                output_present,
                claim_count,
                uncertainty_count,
                proposal_count,
            },
        )
        .collect())
}

pub async fn attention(
    pool: &PgPool,
    scope: AnalysisScope,
) -> Result<AttentionSummary, ReadModelError> {
    let mut tx = scoped(pool, scope.workspace).await?;
    let unresolved_uncertainties:i64=sqlx::query_scalar("SELECT count(*) FROM niccolo.situation_analysis_uncertainties u WHERE u.workspace_id=$1 AND u.situation_id=$2 AND u.analysis_id=$3 AND NOT EXISTS(SELECT 1 FROM niccolo.situation_analysis_uncertainty_events e WHERE e.workspace_id=u.workspace_id AND e.situation_id=u.situation_id AND e.analysis_id=u.analysis_id AND e.uncertainty_id=u.uncertainty_id AND e.event_kind IN ('CLARIFIED','OUT_OF_SCOPE'))")
        .bind(scope.workspace).bind(scope.situation).bind(scope.analysis).fetch_one(&mut *tx).await?;
    let undecided_action_proposals:i64=sqlx::query_scalar("SELECT count(*) FROM niccolo.situation_analysis_action_proposals p WHERE p.workspace_id=$1 AND p.situation_id=$2 AND p.analysis_id=$3 AND NOT EXISTS(SELECT 1 FROM niccolo.situation_analysis_action_decisions d WHERE d.workspace_id=p.workspace_id AND d.situation_id=p.situation_id AND d.analysis_id=p.analysis_id AND d.proposal_id=p.proposal_id)")
        .bind(scope.workspace).bind(scope.situation).bind(scope.analysis).fetch_one(&mut *tx).await?;
    tx.commit().await?;
    Ok(AttentionSummary {
        unresolved_uncertainties,
        undecided_action_proposals,
        automatically_prioritized: false,
    })
}

pub async fn human_steering(
    pool: &PgPool,
    scope: AnalysisScope,
) -> Result<HumanSteeringSummary, ReadModelError> {
    let mut tx = scoped(pool, scope.workspace).await?;
    let clarification_answers:i64=sqlx::query_scalar("SELECT count(*) FROM niccolo.situation_analysis_clarification_answers WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3").bind(scope.workspace).bind(scope.situation).bind(scope.analysis).fetch_one(&mut *tx).await?;
    let claim_reviews:i64=sqlx::query_scalar("SELECT count(*) FROM niccolo.situation_analysis_claim_reviews WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3").bind(scope.workspace).bind(scope.situation).bind(scope.analysis).fetch_one(&mut *tx).await?;
    let action_decisions:i64=sqlx::query_scalar("SELECT count(*) FROM niccolo.situation_analysis_action_decisions WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3").bind(scope.workspace).bind(scope.situation).bind(scope.analysis).fetch_one(&mut *tx).await?;
    let uncertainty_events:i64=sqlx::query_scalar("SELECT count(*) FROM niccolo.situation_analysis_uncertainty_events WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3").bind(scope.workspace).bind(scope.situation).bind(scope.analysis).fetch_one(&mut *tx).await?;
    tx.commit().await?;
    Ok(HumanSteeringSummary {
        clarification_answers,
        claim_reviews,
        action_decisions,
        uncertainty_events,
        steering_is_future_policy: false,
    })
}
