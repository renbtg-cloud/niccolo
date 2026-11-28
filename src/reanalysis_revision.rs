//! v1.701-v1.800: explain what is recorded as changed across a direct reanalysis.
//! Input deltas and claim lineage are facts about records; they do not prove causal reason or truth.
use crate::{
    analysis_compare,
    analysis_structure::AnalysisScope,
    analysis_workbench_support::{self, Basis, FrozenSharedOrigin},
};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use std::collections::BTreeMap;
use uuid::Uuid;
#[derive(Debug, thiserror::Error)]
pub enum RevisionError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("comparison unavailable: {0}")]
    Compare(#[from] analysis_compare::AnalysisCompareError),
    #[error("analysis pair not visible")]
    Invisible,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevisionExplanation {
    pub prior_analysis_id: Uuid,
    pub new_analysis_id: Uuid,
    pub prior_input_revision: i64,
    pub new_input_revision: i64,
    pub focus_changed: bool,
    pub added_source_count: i64,
    pub removed_source_count: i64,
    pub added_context_count: i64,
    pub removed_context_count: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shared_origin_changes: Option<SharedOriginChanges>,
    pub retained: i64,
    pub refined: i64,
    pub reversed: i64,
    pub dropped: i64,
    pub split: i64,
    pub merged: i64,
    pub unlinked_prior_claims: Vec<Uuid>,
    pub unlinked_new_claims: Vec<Uuid>,
    pub unlinked_claims_are_inferred_change: bool,
    pub input_change_proves_causal_reason_for_output_change: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedOriginChanges {
    pub added: Vec<FrozenSharedOrigin>,
    pub removed: Vec<FrozenSharedOrigin>,
    pub changed: Vec<FrozenSharedOrigin>,
    pub comparison_is_partial: bool,
}

fn origin_changes(prior: Option<Basis>, current: Option<Basis>) -> Option<SharedOriginChanges> {
    let comparison_is_partial = prior
        .iter()
        .chain(current.iter())
        .any(Basis::shared_origins_are_partial);
    let indexed = |basis: Option<Basis>| {
        basis
            .into_iter()
            .flat_map(|basis| basis.shared_artifact_origins)
            .map(|origin| {
                (
                    (origin.conclusion_object_id, origin.source_representation_id),
                    origin,
                )
            })
            .collect::<BTreeMap<_, _>>()
    };
    let mut prior = indexed(prior);
    let mut added = Vec::new();
    let mut changed = Vec::new();
    for (key, origin) in indexed(current) {
        match prior.remove(&key) {
            None => added.push(origin),
            Some(previous) if previous != origin => changed.push(origin),
            Some(_) => {}
        }
    }
    let removed = prior.into_values().collect::<Vec<_>>();
    (comparison_is_partial || !added.is_empty() || !removed.is_empty() || !changed.is_empty())
        .then_some(SharedOriginChanges {
            added,
            removed,
            changed,
            comparison_is_partial,
        })
}

async fn scoped(pool: &PgPool, w: Uuid) -> Result<Transaction<'_, Postgres>, RevisionError> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *tx)
        .await?;
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(w.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}
pub async fn explain(
    pool: &PgPool,
    scope: AnalysisScope,
) -> Result<RevisionExplanation, RevisionError> {
    let mut tx = scoped(pool, scope.workspace).await?;
    let pair:Option<(Uuid,i64,i64,String,String)>=sqlx::query_as("SELECT n.reanalysis_of,p.input_revision,n.input_revision,pr.focus,nr.focus FROM niccolo.situation_analysis_snapshots n JOIN niccolo.situation_analysis_snapshots p ON p.workspace_id=n.workspace_id AND p.situation_id=n.situation_id AND p.analysis_id=n.reanalysis_of JOIN niccolo.situation_revisions pr ON pr.workspace_id=p.workspace_id AND pr.situation_id=p.situation_id AND pr.revision=p.input_revision JOIN niccolo.situation_revisions nr ON nr.workspace_id=n.workspace_id AND nr.situation_id=n.situation_id AND nr.revision=n.input_revision WHERE n.workspace_id=$1 AND n.situation_id=$2 AND n.analysis_id=$3").bind(scope.workspace).bind(scope.situation).bind(scope.analysis).fetch_optional(&mut *tx).await?;
    let Some((prior, prev_rev, new_rev, prev_focus, new_focus)) = pair else {
        return Err(RevisionError::Invisible);
    };
    let added_source_count:i64=sqlx::query_scalar("SELECT count(*) FROM(SELECT document_id FROM niccolo.situation_revision_sources WHERE workspace_id=$1 AND situation_id=$2 AND revision=$4 EXCEPT SELECT document_id FROM niccolo.situation_revision_sources WHERE workspace_id=$1 AND situation_id=$2 AND revision=$3)x").bind(scope.workspace).bind(scope.situation).bind(prev_rev).bind(new_rev).fetch_one(&mut *tx).await?;
    let removed_source_count:i64=sqlx::query_scalar("SELECT count(*) FROM(SELECT document_id FROM niccolo.situation_revision_sources WHERE workspace_id=$1 AND situation_id=$2 AND revision=$3 EXCEPT SELECT document_id FROM niccolo.situation_revision_sources WHERE workspace_id=$1 AND situation_id=$2 AND revision=$4)x").bind(scope.workspace).bind(scope.situation).bind(prev_rev).bind(new_rev).fetch_one(&mut *tx).await?;
    let added_context_count:i64=sqlx::query_scalar("SELECT count(*) FROM(SELECT contribution_id FROM niccolo.situation_revision_contributions WHERE workspace_id=$1 AND situation_id=$2 AND revision=$4 EXCEPT SELECT contribution_id FROM niccolo.situation_revision_contributions WHERE workspace_id=$1 AND situation_id=$2 AND revision=$3)x").bind(scope.workspace).bind(scope.situation).bind(prev_rev).bind(new_rev).fetch_one(&mut *tx).await?;
    let removed_context_count:i64=sqlx::query_scalar("SELECT count(*) FROM(SELECT contribution_id FROM niccolo.situation_revision_contributions WHERE workspace_id=$1 AND situation_id=$2 AND revision=$3 EXCEPT SELECT contribution_id FROM niccolo.situation_revision_contributions WHERE workspace_id=$1 AND situation_id=$2 AND revision=$4)x").bind(scope.workspace).bind(scope.situation).bind(prev_rev).bind(new_rev).fetch_one(&mut *tx).await?;
    let(retained,refined,reversed,dropped,split,merged):(i64,i64,i64,i64,i64,i64)=sqlx::query_as("SELECT count(*) FILTER(WHERE relation='RETAINED'),count(*) FILTER(WHERE relation='REFINED'),count(*) FILTER(WHERE relation='REVERSED'),count(*) FILTER(WHERE relation='DROPPED'),count(*) FILTER(WHERE relation='SPLIT'),count(*) FILTER(WHERE relation='MERGED') FROM niccolo.situation_analysis_claim_lineage WHERE workspace_id=$1 AND situation_id=$2 AND prior_analysis_id=$3 AND new_analysis_id=$4").bind(scope.workspace).bind(scope.situation).bind(prior).bind(scope.analysis).fetch_one(&mut *tx).await?;
    let prior_basis = analysis_workbench_support::read_in(
        &mut tx,
        AnalysisScope {
            analysis: prior,
            ..scope
        },
    )
    .await?;
    let current_basis = analysis_workbench_support::read_in(&mut tx, scope).await?;
    tx.commit().await?;
    let c = analysis_compare::compare_direct_reanalysis(
        pool,
        scope.workspace,
        scope.situation,
        prior,
        scope.analysis,
    )
    .await?;
    Ok(RevisionExplanation {
        prior_analysis_id: prior,
        new_analysis_id: scope.analysis,
        prior_input_revision: prev_rev,
        new_input_revision: new_rev,
        focus_changed: prev_focus != new_focus,
        added_source_count,
        removed_source_count,
        added_context_count,
        removed_context_count,
        shared_origin_changes: origin_changes(prior_basis, current_basis),
        retained,
        refined,
        reversed,
        dropped,
        split,
        merged,
        unlinked_prior_claims: c.prior_claims_without_lineage,
        unlinked_new_claims: c.new_claims_without_lineage,
        unlinked_claims_are_inferred_change: false,
        input_change_proves_causal_reason_for_output_change: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        analysis_workbench_support::FrozenOriginDerivation, epistemic_workbench::GroundingRouteRef,
    };

    #[test]
    fn compares_origin_keys_and_reports_incomplete_frozen_support() {
        let origin = |source, route| FrozenSharedOrigin {
            conclusion_object_id: Uuid::from_u128(1),
            source_artifact_id: Uuid::from_u128(2),
            source_representation_id: Uuid::from_u128(source),
            source_artifact_label: "Interview".into(),
            source_scope_locator: "Answer".into(),
            derivations: vec![FrozenOriginDerivation {
                relationship_id: Uuid::from_u128(3),
                via_relationship_id: None,
                routes: vec![GroundingRouteRef {
                    argument_id: Uuid::from_u128(4),
                    grounding_set_id: Uuid::from_u128(route),
                }],
            }],
        };
        let basis = |origins, truncated| {
            Some(Basis {
                routes: Vec::new(),
                truncated,
                shared_artifact_origins: origins,
                shared_artifact_origins_truncated: false,
            })
        };
        let unchanged = origin(5, 6);
        let prior = basis(vec![unchanged.clone(), origin(7, 8), origin(9, 10)], false);
        let current = basis(vec![unchanged, origin(7, 11), origin(12, 13)], true);
        let changes = origin_changes(prior, current).unwrap();
        assert_eq!(changes.added, vec![origin(12, 13)]);
        assert_eq!(changes.removed, vec![origin(9, 10)]);
        assert_eq!(changes.changed, vec![origin(7, 11)]);
        assert!(changes.comparison_is_partial);
        assert!(origin_changes(None, None).is_none());
    }
}
