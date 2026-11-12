//! Immutable audit records for explicit grounded claim-weight evaluations.
//!
//! A stored record freezes the exact declared strengths, selected policy and
//! deterministic output that were evaluated at one point in time. It does not
//! turn the heuristic score into a probability, fact, evidence item or verdict.
use crate::{
    analysis_structure::AnalysisScope,
    authz,
    epistemic_weights::WeightPolicy,
    grounded_claim_weights::{DeclaredEvidenceStrength, GroundedBalance},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum GroundedScoreStoreError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("serialization failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid grounded score record")]
    Invalid,
    #[error("grounded score ID replay conflicts with recorded artifact")]
    Conflict,
    #[error("privileged SQL session is prohibited")]
    Privileged,
}

#[derive(Clone, Debug)]
pub struct NewGroundedScoreRecord {
    pub scope: AnalysisScope,
    pub claim_id: Uuid,
    pub score_id: Uuid,
    pub policy_source: String,
    pub policy: WeightPolicy,
    pub human_declared_confidence_percent: Option<u8>,
    pub declared_evidence: Vec<DeclaredEvidenceStrength>,
    pub result: GroundedBalance,
    pub recorded_by: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroundedScoreRecordView {
    pub claim_id: Uuid,
    pub score_id: Uuid,
    pub policy_source: String,
    pub policy: WeightPolicy,
    pub human_declared_confidence_percent: Option<u8>,
    pub declared_evidence: Vec<DeclaredEvidenceStrength>,
    pub result: GroundedBalance,
    pub result_sha256: String,
    pub recorded_by_principal_id: Uuid,
    pub created_at: String,
    pub immutable_record: bool,
    pub score_is_probability_of_truth: bool,
    pub establishes_claim_truth: bool,
    pub is_documentary_evidence: bool,
    pub modifies_published_analysis: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroundedScoreRecordPage {
    pub records: Vec<GroundedScoreRecordView>,
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
    read_only: bool,
) -> Result<Transaction<'_, Postgres>, GroundedScoreStoreError> {
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
        return Err(GroundedScoreStoreError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

fn valid(input: &NewGroundedScoreRecord) -> bool {
    !input.claim_id.is_nil()
        && !input.score_id.is_nil()
        && !input.recorded_by.is_nil()
        && !input.policy_source.trim().is_empty()
        && input.policy_source.len() <= 128
        && !input.policy_source.contains('\0')
        && input.result.claim_id == input.claim_id
        && !input.declared_evidence.is_empty()
        && input.declared_evidence.len() <= 64
        && input
            .human_declared_confidence_percent
            .is_none_or(|value| value <= 100)
        && !input.result.report.score_is_probability_of_truth
        && !input.result.report.establishes_fact
        && !input.result.establishes_claim_truth
        && !input.result.modifies_historical_analysis
}

fn digest(result: &GroundedBalance) -> Result<String, serde_json::Error> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(result)?)))
}

type StoredRow = (
    Uuid,
    Uuid,
    String,
    Value,
    Option<i16>,
    Value,
    Value,
    String,
    Uuid,
    String,
);

fn view(row: StoredRow) -> Result<GroundedScoreRecordView, GroundedScoreStoreError> {
    let confidence = row
        .4
        .map(|value| u8::try_from(value).map_err(|_| GroundedScoreStoreError::Conflict))
        .transpose()?;
    let policy: WeightPolicy = serde_json::from_value(row.3)?;
    let declared_evidence: Vec<DeclaredEvidenceStrength> = serde_json::from_value(row.5)?;
    let result: GroundedBalance = serde_json::from_value(row.6)?;
    let actual_digest = digest(&result)?;
    if actual_digest != row.7
        || result.claim_id != row.0
        || result.report.score_is_probability_of_truth
        || result.report.establishes_fact
        || result.establishes_claim_truth
        || result.modifies_historical_analysis
    {
        return Err(GroundedScoreStoreError::Conflict);
    }
    Ok(GroundedScoreRecordView {
        claim_id: row.0,
        score_id: row.1,
        policy_source: row.2,
        policy,
        human_declared_confidence_percent: confidence,
        declared_evidence,
        result,
        result_sha256: row.7,
        recorded_by_principal_id: row.8,
        created_at: row.9,
        immutable_record: true,
        score_is_probability_of_truth: false,
        establishes_claim_truth: false,
        is_documentary_evidence: false,
        modifies_published_analysis: false,
    })
}

pub async fn record(
    pool: &PgPool,
    input: &NewGroundedScoreRecord,
) -> Result<(GroundedScoreRecordView, bool), GroundedScoreStoreError> {
    if !valid(input) {
        return Err(GroundedScoreStoreError::Invalid);
    }
    let policy = serde_json::to_value(input.policy)?;
    let declared = serde_json::to_value(&input.declared_evidence)?;
    let result = serde_json::to_value(&input.result)?;
    let result_sha256 = digest(&input.result)?;
    let confidence = input.human_declared_confidence_percent.map(i16::from);

    let mut tx = scoped(pool, input.scope.workspace, false).await?;
    let inserted = sqlx::query(
        "INSERT INTO niccolo.situation_analysis_grounded_score_records
         (workspace_id,situation_id,analysis_id,claim_id,score_id,policy_source,policy,
          human_declared_confidence_percent,declared_evidence,result,result_sha256,
          recorded_by_principal_id)
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
         ON CONFLICT DO NOTHING",
    )
    .bind(input.scope.workspace)
    .bind(input.scope.situation)
    .bind(input.scope.analysis)
    .bind(input.claim_id)
    .bind(input.score_id)
    .bind(input.policy_source.trim())
    .bind(&policy)
    .bind(confidence)
    .bind(&declared)
    .bind(&result)
    .bind(&result_sha256)
    .bind(input.recorded_by)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;

    let row: StoredRow = sqlx::query_as(
        "SELECT claim_id,score_id,policy_source,policy,human_declared_confidence_percent,
                declared_evidence,result,result_sha256,recorded_by_principal_id,created_at::text
           FROM niccolo.situation_analysis_grounded_score_records
          WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3
            AND claim_id=$4 AND score_id=$5",
    )
    .bind(input.scope.workspace)
    .bind(input.scope.situation)
    .bind(input.scope.analysis)
    .bind(input.claim_id)
    .bind(input.score_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(GroundedScoreStoreError::Conflict)?;
    if row.2 != input.policy_source.trim()
        || row.3 != policy
        || row.4 != confidence
        || row.5 != declared
        || row.6 != result
        || row.7 != result_sha256
        || row.8 != input.recorded_by
    {
        return Err(GroundedScoreStoreError::Conflict);
    }
    let stored = view(row)?;
    tx.commit().await?;
    Ok((stored, !inserted))
}

pub async fn list_for_analysis(
    pool: &PgPool,
    scope: AnalysisScope,
    limit: i64,
) -> Result<GroundedScoreRecordPage, GroundedScoreStoreError> {
    if !(1..=512).contains(&limit) {
        return Err(GroundedScoreStoreError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, true).await?;
    let rows: Vec<StoredRow> = sqlx::query_as(
        "SELECT claim_id,score_id,policy_source,policy,human_declared_confidence_percent,
                declared_evidence,result,result_sha256,recorded_by_principal_id,created_at::text
           FROM niccolo.situation_analysis_grounded_score_records
          WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3
          ORDER BY created_at,claim_id,score_id
          LIMIT $4",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(limit)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(GroundedScoreRecordPage {
        records: rows
            .into_iter()
            .map(view)
            .collect::<Result<Vec<_>, GroundedScoreStoreError>>()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_view_rejects_tampered_result_digest() {
        let claim = Uuid::from_u128(4);
        let result = GroundedBalance {
            claim_id: claim,
            report: crate::epistemic_weights::BalanceReport {
                support_points: 40,
                counter_points: 10,
                net_points: 30,
                input_item_count: 1,
                origins: vec![],
                human_declared_confidence_percent: None,
                score_is_probability_of_truth: false,
                independence_established: false,
                human_confidence_is_documentary_evidence: false,
                establishes_fact: false,
            },
            resolved_evidence: vec![],
            recorded_dependence: crate::provenance_clusters::ClusterReport {
                clusters: vec![],
                selected_count: 0,
                known_lineage_node_count: 0,
                all_declared_ancestors_present: true,
                different_clusters_establish_independence: false,
                establishes_truth: false,
            },
            grouping_mode: "RECORDED_DEPENDENCE_COMPONENT".into(),
            source_graph_revision: 1,
            inputs_checked_against_database: true,
            stance_derived_from_analysis_links: true,
            origins_grouped_by_document_id: false,
            derivation_graph_traversed: true,
            independent_witnesses_established: false,
            establishes_claim_truth: false,
            modifies_historical_analysis: false,
        };
        let row: StoredRow = (
            claim,
            Uuid::from_u128(5),
            "SITUATION".into(),
            serde_json::to_value(WeightPolicy::default()).unwrap(),
            None,
            serde_json::json!([]),
            serde_json::to_value(&result).unwrap(),
            "0".repeat(64),
            Uuid::from_u128(6),
            "2026-11-12 00:00:00+00".into(),
        );
        assert!(matches!(view(row), Err(GroundedScoreStoreError::Conflict)));
    }

    #[test]
    fn rejects_records_that_claim_probability_or_truth() {
        let claim = Uuid::from_u128(4);
        let input = NewGroundedScoreRecord {
            scope: AnalysisScope {
                workspace: Uuid::from_u128(1),
                situation: Uuid::from_u128(2),
                analysis: Uuid::from_u128(3),
            },
            claim_id: claim,
            score_id: Uuid::from_u128(5),
            policy_source: "EXPLICIT_TRIAL".into(),
            policy: WeightPolicy::default(),
            human_declared_confidence_percent: None,
            declared_evidence: vec![DeclaredEvidenceStrength {
                evidence_id: Uuid::from_u128(6),
                declared_strength: 50,
                rationale: "human-declared strength".into(),
            }],
            result: GroundedBalance {
                claim_id: claim,
                report: crate::epistemic_weights::BalanceReport {
                    support_points: 0,
                    counter_points: 0,
                    net_points: 0,
                    input_item_count: 1,
                    origins: vec![],
                    human_declared_confidence_percent: None,
                    score_is_probability_of_truth: true,
                    independence_established: false,
                    human_confidence_is_documentary_evidence: false,
                    establishes_fact: false,
                },
                resolved_evidence: vec![],
                recorded_dependence: crate::provenance_clusters::ClusterReport {
                    clusters: vec![],
                    selected_count: 0,
                    known_lineage_node_count: 0,
                    all_declared_ancestors_present: true,
                    different_clusters_establish_independence: false,
                    establishes_truth: false,
                },
                grouping_mode: "RECORDED_DEPENDENCE_COMPONENT".into(),
                source_graph_revision: 0,
                inputs_checked_against_database: true,
                stance_derived_from_analysis_links: true,
                origins_grouped_by_document_id: false,
                derivation_graph_traversed: true,
                independent_witnesses_established: false,
                establishes_claim_truth: false,
                modifies_historical_analysis: false,
            },
            recorded_by: Uuid::from_u128(7),
        };
        assert!(!valid(&input));
    }
}
