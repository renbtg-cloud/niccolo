//! v0.93, v0.95-v0.97: bounded handoff pages, explicit reanalysis comparison, lineage completeness, and unranked human-pending state.
use crate::analysis_structure::AnalysisScope;
use serde::Serialize;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum AnalysisCompareError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("privileged SQL login prohibited")]
    Privileged,
    #[error("invalid comparison request")]
    Invalid,
    #[error("analysis pair is not a direct reanalysis")]
    NotDirectReanalysis,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ClaimPageItem {
    pub claim_id: Uuid,
    pub ordinal: i32,
    pub kind: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ClaimPage {
    pub items: Vec<ClaimPageItem>,
    pub next_ordinal: Option<i32>,
    pub bounded: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ReanalysisComparison {
    pub prior_analysis_id: Uuid,
    pub new_analysis_id: Uuid,
    pub prior_claim_count: i64,
    pub new_claim_count: i64,
    pub explicit_lineage_links: i64,
    pub explicitly_dropped_prior_claims: Vec<Uuid>,
    pub prior_claims_without_lineage: Vec<Uuid>,
    pub new_claims_without_lineage: Vec<Uuid>,
    pub absence_of_lineage_means_removed_or_added: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LineageCompleteness {
    pub prior_claim_count: i64,
    pub prior_claims_with_lineage: i64,
    pub new_claim_count: i64,
    pub new_claims_with_lineage: i64,
    pub prior_complete: bool,
    pub new_complete: bool,
    pub completeness_means_correctness: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HumanPending {
    pub open_question_ids: Vec<Uuid>,
    pub open_condition_ids: Vec<Uuid>,
    pub unresolved_uncertainty_ids: Vec<Uuid>,
    pub undecided_action_ids: Vec<Uuid>,
    pub automatically_ranked: bool,
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
) -> Result<Transaction<'_, Postgres>, AnalysisCompareError> {
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
        return Err(AnalysisCompareError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

pub async fn claim_page(
    pool: &PgPool,
    scope: AnalysisScope,
    after_ordinal: Option<i32>,
    limit: i64,
) -> Result<ClaimPage, AnalysisCompareError> {
    if !(1..=100).contains(&limit) || after_ordinal.is_some_and(|v| v < 0) {
        return Err(AnalysisCompareError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace).await?;
    let rows: Vec<(Uuid, i32, String, String)> = sqlx::query_as(
        "SELECT claim_id,ordinal,claim_kind,claim_text FROM niccolo.situation_analysis_claims \
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND ordinal>$4 \
         ORDER BY ordinal,claim_id LIMIT $5",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(after_ordinal.unwrap_or(0))
    .bind(limit + 1)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    let more = rows.len() as i64 > limit;
    let items: Vec<_> = rows
        .into_iter()
        .take(limit as usize)
        .map(|(claim_id, ordinal, kind, text)| ClaimPageItem {
            claim_id,
            ordinal,
            kind,
            text,
        })
        .collect();
    let next_ordinal = more.then(|| items.last().expect("non-empty bounded page").ordinal);
    Ok(ClaimPage {
        items,
        next_ordinal,
        bounded: true,
    })
}

pub async fn compare_direct_reanalysis(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    prior: Uuid,
    new_analysis: Uuid,
) -> Result<ReanalysisComparison, AnalysisCompareError> {
    let mut tx = scoped(pool, workspace).await?;
    let direct: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM niccolo.situation_analysis_snapshots WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND reanalysis_of=$4)",
    )
    .bind(workspace)
    .bind(situation)
    .bind(new_analysis)
    .bind(prior)
    .fetch_one(&mut *tx)
    .await?;
    if !direct {
        return Err(AnalysisCompareError::NotDirectReanalysis);
    }
    let prior_claim_count: i64 = sqlx::query_scalar("SELECT count(*) FROM niccolo.situation_analysis_claims WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3")
        .bind(workspace).bind(situation).bind(prior).fetch_one(&mut *tx).await?;
    let new_claim_count: i64 = sqlx::query_scalar("SELECT count(*) FROM niccolo.situation_analysis_claims WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3")
        .bind(workspace).bind(situation).bind(new_analysis).fetch_one(&mut *tx).await?;
    let explicit_lineage_links: i64 = sqlx::query_scalar("SELECT count(*) FROM niccolo.situation_analysis_claim_lineage WHERE workspace_id=$1 AND situation_id=$2 AND prior_analysis_id=$3 AND new_analysis_id=$4")
        .bind(workspace).bind(situation).bind(prior).bind(new_analysis).fetch_one(&mut *tx).await?;
    let explicitly_dropped_prior_claims: Vec<Uuid> = sqlx::query_scalar("SELECT prior_claim_id FROM niccolo.situation_analysis_claim_lineage WHERE workspace_id=$1 AND situation_id=$2 AND prior_analysis_id=$3 AND new_analysis_id=$4 AND relation='DROPPED' ORDER BY prior_claim_id")
        .bind(workspace).bind(situation).bind(prior).bind(new_analysis).fetch_all(&mut *tx).await?;
    let prior_claims_without_lineage: Vec<Uuid> = sqlx::query_scalar("SELECT c.claim_id FROM niccolo.situation_analysis_claims c WHERE c.workspace_id=$1 AND c.situation_id=$2 AND c.analysis_id=$3 AND NOT EXISTS(SELECT 1 FROM niccolo.situation_analysis_claim_lineage l WHERE l.workspace_id=c.workspace_id AND l.situation_id=c.situation_id AND l.prior_analysis_id=$3 AND l.new_analysis_id=$4 AND l.prior_claim_id=c.claim_id) ORDER BY c.claim_id")
        .bind(workspace).bind(situation).bind(prior).bind(new_analysis).fetch_all(&mut *tx).await?;
    let new_claims_without_lineage: Vec<Uuid> = sqlx::query_scalar("SELECT c.claim_id FROM niccolo.situation_analysis_claims c WHERE c.workspace_id=$1 AND c.situation_id=$2 AND c.analysis_id=$4 AND NOT EXISTS(SELECT 1 FROM niccolo.situation_analysis_claim_lineage l WHERE l.workspace_id=c.workspace_id AND l.situation_id=c.situation_id AND l.prior_analysis_id=$3 AND l.new_analysis_id=$4 AND l.new_claim_id=c.claim_id) ORDER BY c.claim_id")
        .bind(workspace).bind(situation).bind(prior).bind(new_analysis).fetch_all(&mut *tx).await?;
    tx.commit().await?;
    Ok(ReanalysisComparison {
        prior_analysis_id: prior,
        new_analysis_id: new_analysis,
        prior_claim_count,
        new_claim_count,
        explicit_lineage_links,
        explicitly_dropped_prior_claims,
        prior_claims_without_lineage,
        new_claims_without_lineage,
        absence_of_lineage_means_removed_or_added: false,
    })
}

pub async fn lineage_completeness(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    prior: Uuid,
    new_analysis: Uuid,
) -> Result<LineageCompleteness, AnalysisCompareError> {
    let c = compare_direct_reanalysis(pool, workspace, situation, prior, new_analysis).await?;
    let prior_with = c.prior_claim_count - c.prior_claims_without_lineage.len() as i64;
    let new_with = c.new_claim_count - c.new_claims_without_lineage.len() as i64;
    Ok(LineageCompleteness {
        prior_claim_count: c.prior_claim_count,
        prior_claims_with_lineage: prior_with,
        new_claim_count: c.new_claim_count,
        new_claims_with_lineage: new_with,
        prior_complete: prior_with == c.prior_claim_count,
        new_complete: new_with == c.new_claim_count,
        completeness_means_correctness: false,
    })
}

pub async fn human_pending(
    pool: &PgPool,
    scope: AnalysisScope,
) -> Result<HumanPending, AnalysisCompareError> {
    let mut tx = scoped(pool, scope.workspace).await?;
    let open_question_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT DISTINCT l.question_id FROM niccolo.situation_analysis_claim_question_links l \
         WHERE l.workspace_id=$1 AND l.situation_id=$2 AND l.analysis_id=$3 \
         AND NOT EXISTS(SELECT 1 FROM niccolo.situation_analysis_clarification_answers a WHERE a.workspace_id=l.workspace_id AND a.situation_id=l.situation_id AND a.analysis_id=l.analysis_id AND a.question_id=l.question_id) \
         ORDER BY l.question_id",
    ).bind(scope.workspace).bind(scope.situation).bind(scope.analysis).fetch_all(&mut *tx).await?;
    let open_condition_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT c.condition_id FROM niccolo.situation_analysis_claim_revision_conditions c \
         WHERE c.workspace_id=$1 AND c.situation_id=$2 AND c.analysis_id=$3 AND coalesce((SELECT e.event_kind FROM niccolo.situation_analysis_claim_condition_events e WHERE e.workspace_id=c.workspace_id AND e.situation_id=c.situation_id AND e.analysis_id=c.analysis_id AND e.claim_id=c.claim_id AND e.condition_id=c.condition_id ORDER BY e.created_at DESC,e.event_id DESC LIMIT 1),'REOPENED') IN ('REOPENED','NOT_MET') ORDER BY c.condition_id",
    ).bind(scope.workspace).bind(scope.situation).bind(scope.analysis).fetch_all(&mut *tx).await?;
    let unresolved_uncertainty_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT u.uncertainty_id FROM niccolo.situation_analysis_uncertainties u WHERE u.workspace_id=$1 AND u.situation_id=$2 AND u.analysis_id=$3 AND NOT EXISTS(SELECT 1 FROM niccolo.situation_analysis_uncertainty_events e WHERE e.workspace_id=u.workspace_id AND e.situation_id=u.situation_id AND e.analysis_id=u.analysis_id AND e.uncertainty_id=u.uncertainty_id AND e.event_kind IN ('CLARIFIED','OUT_OF_SCOPE')) ORDER BY u.uncertainty_id",
    ).bind(scope.workspace).bind(scope.situation).bind(scope.analysis).fetch_all(&mut *tx).await?;
    let undecided_action_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT p.proposal_id FROM niccolo.situation_analysis_action_proposals p WHERE p.workspace_id=$1 AND p.situation_id=$2 AND p.analysis_id=$3 AND NOT EXISTS(SELECT 1 FROM niccolo.situation_analysis_action_decisions d WHERE d.workspace_id=p.workspace_id AND d.situation_id=p.situation_id AND d.analysis_id=p.analysis_id AND d.proposal_id=p.proposal_id) ORDER BY p.proposal_id",
    ).bind(scope.workspace).bind(scope.situation).bind(scope.analysis).fetch_all(&mut *tx).await?;
    tx.commit().await?;
    Ok(HumanPending {
        open_question_ids,
        open_condition_ids,
        unresolved_uncertainty_ids,
        undecided_action_ids,
        automatically_ranked: false,
    })
}
