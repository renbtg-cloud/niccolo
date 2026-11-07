//! Read-only claim-scoped evidence balance, validated against analyzed source links.
//!
//! The submitted numbers are human-supplied heuristic strengths. This module
//! derives stance and document grouping from the DATABASE rather than trusting
//! request-supplied provenance, but still does not establish factual truth,
//! witness independence or a calibrated statistical probability.
use crate::{
    analysis_structure::AnalysisScope,
    authz, grounded_lineage_store,
    provenance_clusters::ClusterReport,
    epistemic_weights::{
        self, BalanceReport, Stance, WeightError, WeightPolicy, WeightedObservation,
    },
};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use std::collections::BTreeSet;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum GroundedWeightError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("invalid claim-scoped weighting request")]
    Invalid,
    #[error("analysis claim is not visible")]
    MissingClaim,
    #[error("evidence is not linked to this claim as examined source")]
    UnlinkedEvidence,
    #[error("evidence has conflicting analysis-use relations")]
    AmbiguousLink,
    #[error("invalid score: {0}")]
    Weight(#[from] WeightError),
    #[error("recorded multi-parent lineage failed: {0}")]
    Lineage(#[from] grounded_lineage_store::StoredLineageError),
    #[error("privileged runtime database role prohibited")]
    Privileged,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeclaredEvidenceStrength {
    pub evidence_id: Uuid,
    pub declared_strength: u16,
    pub rationale: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResolvedClaimEvidence {
    pub evidence_id: Uuid,
    pub document_id: Uuid,
    pub recorded_relation: String,
    pub derived_stance: Stance,
    pub declared_strength: u16,
    /// Structural dependence group representative, not a certified source root.
    pub dependence_component_id: Option<Uuid>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GroundedBalance {
    pub claim_id: Uuid,
    pub report: BalanceReport,
    pub resolved_evidence: Vec<ResolvedClaimEvidence>,
    /// All recorded ancestors of selected evidence inside one PG snapshot.
    pub recorded_dependence: ClusterReport,
    pub grouping_mode: String,
    pub source_graph_revision: i64,
    pub inputs_checked_against_database: bool,
    pub stance_derived_from_analysis_links: bool,
    pub origins_grouped_by_document_id: bool,
    pub derivation_graph_traversed: bool,
    pub independent_witnesses_established: bool,
    pub establishes_claim_truth: bool,
    pub modifies_historical_analysis: bool,
}

pub fn convert(
    evidence: &DeclaredEvidenceStrength,
    relation: &str,
    document_id: Uuid,
) -> Result<(WeightedObservation, ResolvedClaimEvidence), GroundedWeightError> {
    let stance = match relation {
        "SUPPORTS_AS_USED" => Stance::Supports,
        "CONTRADICTS_AS_USED" => Stance::Contradicts,
        "CONTEXT_ONLY" => Stance::ContextOnly,
        _ => return Err(GroundedWeightError::Invalid),
    };
    Ok((
        WeightedObservation {
            evidence_id: evidence.evidence_id,
            documented_origin_id: Some(document_id),
            stance,
            declared_strength: evidence.declared_strength,
            rationale: evidence.rationale.clone(),
        },
        ResolvedClaimEvidence {
            evidence_id: evidence.evidence_id,
            document_id,
            recorded_relation: relation.into(),
            derived_stance: stance,
            declared_strength: evidence.declared_strength,
            dependence_component_id: None,
        },
    ))
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
) -> Result<Transaction<'_, Postgres>, GroundedWeightError> {
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
        return Err(GroundedWeightError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

/// A database-backed assessment for an existing analysis claim.
///
/// A score is not proof that the analysis link was factually correct.
/// The grouping is documented *source document*, not certified independence
/// and not a full traversal of the multi-parent derivation DAG.
pub async fn evaluate_claim(
    pool: &PgPool,
    scope: AnalysisScope,
    claim_id: Uuid,
    policy: WeightPolicy,
    human_declared_confidence_percent: Option<u8>,
    declared: &[DeclaredEvidenceStrength],
) -> Result<GroundedBalance, GroundedWeightError> {
    if declared.is_empty() || declared.len() > 64 {
        return Err(GroundedWeightError::Invalid);
    }
    let mut seen = BTreeSet::new();
    for e in declared {
        if !seen.insert(e.evidence_id)
            || e.declared_strength > 100
            || e.rationale.trim().is_empty()
            || e.rationale.len() > 4096
            || e.rationale.contains('\0')
        {
            return Err(GroundedWeightError::Invalid);
        }
    }

    let mut tx = scoped(pool, scope.workspace).await?;
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM niccolo.situation_analysis_claims \
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND claim_id=$4)",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(claim_id)
    .fetch_one(&mut *tx)
    .await?;
    if !exists {
        return Err(GroundedWeightError::MissingClaim);
    }

    let mut converted = Vec::with_capacity(declared.len());
    let mut resolved = Vec::with_capacity(declared.len());
    for e in declared {
        let links: Vec<(String, Uuid)> = sqlx::query_as(
            "SELECT f.relation, sf.document_id \
             FROM niccolo.situation_analysis_claim_fragments f \
             JOIN niccolo.source_fragments sf \
              ON sf.workspace_id=f.workspace_id AND sf.evidence_id=f.evidence_id \
             WHERE f.workspace_id=$1 AND f.situation_id=$2 AND f.analysis_id=$3 \
              AND f.claim_id=$4 AND f.evidence_id=$5",
        )
        .bind(scope.workspace)
        .bind(scope.situation)
        .bind(scope.analysis)
        .bind(claim_id)
        .bind(e.evidence_id)
        .fetch_all(&mut *tx)
        .await?;
        match links.as_slice() {
            [] => return Err(GroundedWeightError::UnlinkedEvidence),
            [(relation, document_id)] => {
                let (item, proof) = convert(e, relation, *document_id)?;
                converted.push(item);
                resolved.push(proof);
            }
            _ => return Err(GroundedWeightError::AmbiguousLink),
        }
    }
    // Retrieve every *recorded* ancestor in this same repeatable-read
    // transaction. Never accept a client-provided root or grouping key.
    let selected = declared.iter().map(|item| item.evidence_id).collect::<Vec<_>>();
    let dependence = grounded_lineage_store::from_database(&mut tx, scope.workspace, &selected)
        .await?;
    let source_graph_revision: i64 = sqlx::query_scalar(
        "SELECT COALESCE((
            SELECT revision FROM niccolo.graph_revisions WHERE workspace_id=$1
        ),0)::bigint",
    )
    .bind(scope.workspace)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;

    let mut component_by_evidence = std::collections::BTreeMap::new();
    for cluster in &dependence.clusters {
        for id in &cluster.selected_evidence_ids {
            component_by_evidence.insert(*id, cluster.component_key);
        }
    }
    for (observation, proof) in converted.iter_mut().zip(resolved.iter_mut()) {
        let key = *component_by_evidence
            .get(&observation.evidence_id)
            .ok_or(GroundedWeightError::Lineage(
                grounded_lineage_store::StoredLineageError::Invalid,
            ))?;
        // This legacy field is the computational grouping key, not a literal
        // provenance root. Explicit typed metadata below makes that distinction.
        observation.documented_origin_id = Some(key);
        proof.dependence_component_id = Some(key);
    }
    let report =
        epistemic_weights::evaluate(policy, human_declared_confidence_percent, &converted)?;
    Ok(GroundedBalance {
        claim_id,
        report,
        resolved_evidence: resolved,
        recorded_dependence: dependence,
        grouping_mode: "RECORDED_DEPENDENCE_COMPONENT".into(),
        source_graph_revision,
        inputs_checked_against_database: true,
        stance_derived_from_analysis_links: true,
        origins_grouped_by_document_id: false,
        derivation_graph_traversed: true,
        independent_witnesses_established: false,
        establishes_claim_truth: false,
        modifies_historical_analysis: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn converts_existing_link_relation_not_caller_stance() {
        let input = DeclaredEvidenceStrength {
            evidence_id: Uuid::from_u128(1),
            declared_strength: 90,
            rationale: "my assessment".into(),
        };
        let (o, v) = convert(&input, "CONTRADICTS_AS_USED", Uuid::from_u128(2)).unwrap();
        assert_eq!(o.stance, Stance::Contradicts);
        assert_eq!(o.documented_origin_id, Some(Uuid::from_u128(2)));
        assert_eq!(v.recorded_relation, "CONTRADICTS_AS_USED");
        assert!(convert(&input, "MAKES_TRUE", Uuid::from_u128(2)).is_err());
    }
    #[test]
    fn context_only_is_not_support() {
        let i = DeclaredEvidenceStrength {
            evidence_id: Uuid::from_u128(1),
            declared_strength: 99,
            rationale: "context".into(),
        };
        let (x, _) = convert(&i, "CONTEXT_ONLY", Uuid::from_u128(2)).unwrap();
        assert_eq!(x.stance, Stance::ContextOnly);
    }
}
