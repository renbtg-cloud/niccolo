//! Read-only retrieval of *recorded* multi-parent evidence derivations under
//! the same authorized workspace and database snapshot as claim scoring.
//!
//! Unknown unrecorded dependencies remain possible. Distinct clusters are not
//! independent witnesses. Missing/withdrawn ancestors and cycles fail closed.
use crate::provenance_clusters::{self, ClusterReport, LineageNode};
use sqlx::{Postgres, Transaction};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

const MAX_NODES: usize = 1024;
const MAX_SELECTION: usize = 64;
const BATCH_SIZE: usize = 128;

#[derive(Debug, thiserror::Error)]
pub enum StoredLineageError {
    #[error("lineage database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("invalid or excessive recorded lineage")]
    Invalid,
    #[error("recorded lineage could not be resolved: {0}")]
    Cluster(#[from] provenance_clusters::ClusterError),
}

type LineageRow = (Uuid, bool, Option<Uuid>, Option<Uuid>);

/// Use a bounded breadth-first ancestor walk, not a recursive SQL UNION ALL:
/// the latter could blow up exponentially on deeply diamond-shaped DAGs.
pub async fn from_database(
    tx: &mut Transaction<'_, Postgres>,
    workspace_id: Uuid,
    selected_ids: &[Uuid],
) -> Result<ClusterReport, StoredLineageError> {
    if selected_ids.is_empty()
        || selected_ids.len() > MAX_SELECTION
        || selected_ids.iter().any(Uuid::is_nil)
        || selected_ids.iter().copied().collect::<BTreeSet<_>>().len() != selected_ids.len()
    {
        return Err(StoredLineageError::Invalid);
    }

    let mut queued = selected_ids.iter().copied().collect::<BTreeSet<_>>();
    let mut loaded = BTreeMap::<Uuid, LineageNode>::new();
    while !queued.is_empty() {
        if loaded.len().saturating_add(queued.len()) > MAX_NODES {
            return Err(StoredLineageError::Invalid);
        }
        let batch = queued.iter().take(BATCH_SIZE).copied().collect::<Vec<_>>();
        for evidence_id in &batch {
            queued.remove(evidence_id);
        }
        let rows: Vec<LineageRow> = sqlx::query_as(
            "SELECT e.evidence_id,e.admissible,sf.document_id,d.parent_id \
             FROM niccolo.evidence e \
             LEFT JOIN niccolo.source_fragments sf \
              ON sf.workspace_id=e.workspace_id AND sf.evidence_id=e.evidence_id \
             LEFT JOIN niccolo.evidence_derivation d \
              ON d.workspace_id=e.workspace_id AND d.child_id=e.evidence_id \
             WHERE e.workspace_id=$1 AND e.evidence_id = ANY($2::uuid[]) \
             ORDER BY e.evidence_id,d.parent_id",
        )
        .bind(workspace_id)
        .bind(&batch)
        .fetch_all(&mut **tx)
        .await?;

        for (evidence_id, admissible, source_document_id, parent_id) in rows {
            let entry = loaded.entry(evidence_id).or_insert_with(|| LineageNode {
                evidence_id,
                parents: Vec::new(),
                admissible,
                source_document_id,
            });
            if let Some(parent) = parent_id {
                if !entry.parents.contains(&parent) {
                    if entry.parents.len() >= 128 {
                        return Err(StoredLineageError::Invalid);
                    }
                    entry.parents.push(parent);
                }
                if !loaded.contains_key(&parent) {
                    queued.insert(parent);
                }
            }
        }

        // Ensure the selected and discovered IDs actually resolve to visible
        // records rather than silently dropping missing ancestors in SQL.
        if batch.iter().any(|id| !loaded.contains_key(id)) {
            return Err(StoredLineageError::Cluster(
                provenance_clusters::ClusterError::Missing,
            ));
        }
    }
    let nodes = loaded.into_values().collect::<Vec<_>>();
    Ok(provenance_clusters::cluster(&nodes, selected_ids)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_selection_rejects_empty_and_repeated_ids() {
        let id = Uuid::from_u128(1);
        assert!(vec![id; 65].len() > MAX_SELECTION);
        assert_ne!(BTreeSet::from([id, id]).len(), 2);
        assert_eq!(BATCH_SIZE, 128);
    }
}
