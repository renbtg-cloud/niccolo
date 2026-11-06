//! v0.73-v0.76: explicit traceability gaps without promoting them to truth judgments.
use crate::analysis_structure::AnalysisScope;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum InspectionError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("privileged SQL login prohibited")]
    Privileged,
    #[error("invalid inspection record")]
    Invalid,
    #[error("idempotency conflict")]
    Conflict,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextExclusion {
    pub contribution_id: Uuid,
    pub reason_kind: String,
    pub reason: String,
    pub recorded_by: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClaimOutputSpan {
    pub claim_id: Uuid,
    pub byte_start: i64,
    pub byte_end: i64,
    pub recorded_by: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceabilitySummary {
    pub frozen_context_count: i64,
    pub used_context_count: i64,
    pub excluded_context_count: i64,
    pub accounted_context_count: i64,
    pub all_frozen_context_accounted: bool,
    pub examined_fragment_count: i64,
    pub claim_count: i64,
    pub claims_with_output_span: i64,
    pub all_claims_mapped_to_output: bool,
    pub output_bytes: i64,
    pub accounting_means_truth: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InspectionRecord {
    pub output_present: bool,
    pub manifest_present: bool,
    pub claims_present: bool,
    pub all_claims_mapped_to_output: bool,
    pub all_frozen_context_accounted: bool,
    pub unresolved_uncertainty_count: i64,
    pub human_review_packet_has_traceability: bool,
    pub traceability_means_correctness: bool,
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
    read_only: bool,
) -> Result<Transaction<'_, Postgres>, InspectionError> {
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
        return Err(InspectionError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

pub async fn record_context_exclusion(
    pool: &PgPool,
    scope: AnalysisScope,
    item: &ContextExclusion,
) -> Result<(), InspectionError> {
    if item.reason.trim().is_empty()
        || item.reason.len() > 4096
        || !matches!(
            item.reason_kind.as_str(),
            "OUT_OF_SCOPE"
                | "REDUNDANT_FOR_THIS_ANALYSIS"
                | "INSUFFICIENT_RELEVANCE"
                | "CONFLICTING_SCOPE"
                | "OTHER"
        )
    {
        return Err(InspectionError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, false).await?;
    let input_revision: i64 = sqlx::query_scalar(
        "SELECT input_revision FROM niccolo.situation_analysis_snapshots WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO niccolo.situation_analysis_context_exclusions \
         (workspace_id,situation_id,analysis_id,input_revision,contribution_id,reason_kind,reason,recorded_by_principal_id) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(input_revision)
    .bind(item.contribution_id)
    .bind(&item.reason_kind)
    .bind(&item.reason)
    .bind(item.recorded_by)
    .execute(&mut *tx)
    .await?;
    let existing: Option<(String, String, Uuid)> = sqlx::query_as(
        "SELECT reason_kind,reason,recorded_by_principal_id FROM niccolo.situation_analysis_context_exclusions \
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND contribution_id=$4",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(item.contribution_id)
    .fetch_optional(&mut *tx)
    .await?;
    if existing
        != Some((
            item.reason_kind.clone(),
            item.reason.clone(),
            item.recorded_by,
        ))
    {
        return Err(InspectionError::Conflict);
    }
    tx.commit().await?;
    Ok(())
}

pub async fn record_output_span(
    pool: &PgPool,
    scope: AnalysisScope,
    item: &ClaimOutputSpan,
) -> Result<(), InspectionError> {
    if item.byte_start < 0 || item.byte_end <= item.byte_start {
        return Err(InspectionError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, false).await?;
    sqlx::query(
        "INSERT INTO niccolo.situation_analysis_claim_output_spans \
         (workspace_id,situation_id,analysis_id,claim_id,byte_start,byte_end,recorded_by_principal_id) \
         VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT DO NOTHING",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(item.claim_id)
    .bind(item.byte_start)
    .bind(item.byte_end)
    .bind(item.recorded_by)
    .execute(&mut *tx)
    .await?;
    let existing: Option<(i64, i64, Uuid)> = sqlx::query_as(
        "SELECT byte_start,byte_end,recorded_by_principal_id FROM niccolo.situation_analysis_claim_output_spans \
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND claim_id=$4",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(item.claim_id)
    .fetch_optional(&mut *tx)
    .await?;
    if existing != Some((item.byte_start, item.byte_end, item.recorded_by)) {
        return Err(InspectionError::Conflict);
    }
    tx.commit().await?;
    Ok(())
}

pub async fn traceability(
    pool: &PgPool,
    scope: AnalysisScope,
) -> Result<TraceabilitySummary, InspectionError> {
    let mut tx = scoped(pool, scope.workspace, true).await?;
    let input_revision: i64 = sqlx::query_scalar(
        "SELECT input_revision FROM niccolo.situation_analysis_snapshots WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .fetch_one(&mut *tx)
    .await?;
    let frozen_context_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM niccolo.situation_revision_contributions WHERE workspace_id=$1 AND situation_id=$2 AND revision=$3",
    )
    .bind(scope.workspace).bind(scope.situation).bind(input_revision).fetch_one(&mut *tx).await?;
    let used_context_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM niccolo.situation_analysis_used_contributions WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3",
    )
    .bind(scope.workspace).bind(scope.situation).bind(scope.analysis).fetch_one(&mut *tx).await?;
    let excluded_context_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM niccolo.situation_analysis_context_exclusions WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3",
    )
    .bind(scope.workspace).bind(scope.situation).bind(scope.analysis).fetch_one(&mut *tx).await?;
    let examined_fragment_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM niccolo.situation_analysis_examined_fragments WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3",
    )
    .bind(scope.workspace).bind(scope.situation).bind(scope.analysis).fetch_one(&mut *tx).await?;
    let claim_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM niccolo.situation_analysis_claims WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3",
    )
    .bind(scope.workspace).bind(scope.situation).bind(scope.analysis).fetch_one(&mut *tx).await?;
    let claims_with_output_span: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM niccolo.situation_analysis_claim_output_spans WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3",
    )
    .bind(scope.workspace).bind(scope.situation).bind(scope.analysis).fetch_one(&mut *tx).await?;
    let output_bytes: i64 = sqlx::query_scalar(
        "SELECT octet_length(answer_text)::bigint FROM niccolo.situation_analysis_outputs WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3",
    )
    .bind(scope.workspace).bind(scope.situation).bind(scope.analysis).fetch_one(&mut *tx).await?;
    tx.commit().await?;
    let accounted_context_count = used_context_count + excluded_context_count;
    Ok(TraceabilitySummary {
        frozen_context_count,
        used_context_count,
        excluded_context_count,
        accounted_context_count,
        all_frozen_context_accounted: accounted_context_count == frozen_context_count,
        examined_fragment_count,
        claim_count,
        claims_with_output_span,
        all_claims_mapped_to_output: claim_count > 0 && claim_count == claims_with_output_span,
        output_bytes,
        accounting_means_truth: false,
    })
}

pub async fn inspection_record(
    pool: &PgPool,
    scope: AnalysisScope,
) -> Result<InspectionRecord, InspectionError> {
    let t = traceability(pool, scope).await?;
    let mut tx = scoped(pool, scope.workspace, true).await?;
    let output_present: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM niccolo.situation_analysis_outputs WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3)",
    )
    .bind(scope.workspace).bind(scope.situation).bind(scope.analysis).fetch_one(&mut *tx).await?;
    let manifest_present: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM niccolo.situation_analysis_manifests WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3)",
    )
    .bind(scope.workspace).bind(scope.situation).bind(scope.analysis).fetch_one(&mut *tx).await?;
    let unresolved_uncertainty_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM niccolo.situation_analysis_uncertainties u WHERE u.workspace_id=$1 AND u.situation_id=$2 AND u.analysis_id=$3 \
         AND NOT EXISTS(SELECT 1 FROM niccolo.situation_analysis_uncertainty_events e WHERE e.workspace_id=u.workspace_id AND e.situation_id=u.situation_id AND e.analysis_id=u.analysis_id AND e.uncertainty_id=u.uncertainty_id AND e.event_kind IN ('CLARIFIED','OUT_OF_SCOPE'))",
    )
    .bind(scope.workspace).bind(scope.situation).bind(scope.analysis).fetch_one(&mut *tx).await?;
    tx.commit().await?;
    let claims_present = t.claim_count > 0;
    let human_review_packet_has_traceability = output_present
        && manifest_present
        && claims_present
        && t.all_claims_mapped_to_output
        && t.all_frozen_context_accounted;
    Ok(InspectionRecord {
        output_present,
        manifest_present,
        claims_present,
        all_claims_mapped_to_output: t.all_claims_mapped_to_output,
        all_frozen_context_accounted: t.all_frozen_context_accounted,
        unresolved_uncertainty_count,
        human_review_packet_has_traceability,
        traceability_means_correctness: false,
    })
}
