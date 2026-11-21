//! Conservative source-dependence components for epistemic reasoning.
//!
//! Pure computation on an explicit, complete set of known lineage nodes.
//! This module does not inspect PostgreSQL or certify statistical independence.
//! Derivation links and shared documents can prove known overlap, but the
//! absence of such links is not proof of independently acquired information.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LineageNode {
    pub evidence_id: Uuid,
    pub parents: Vec<Uuid>,
    pub admissible: bool,
    /// May be absent for derivation-only ancestors.
    pub source_document_id: Option<Uuid>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DependenceCluster {
    /// A stable structural representative, not a verified original source.
    pub component_key: Uuid,
    pub selected_evidence_ids: Vec<Uuid>,
    pub documented_roots: Vec<Uuid>,
    pub shared_document_ids: Vec<Uuid>,
    pub represents_statistical_independence: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ClusterReport {
    pub clusters: Vec<DependenceCluster>,
    pub selected_count: usize,
    pub known_lineage_node_count: usize,
    /// Only all parents NAMED by this input are present; unknown omitted links remain possible.
    pub all_declared_ancestors_present: bool,
    pub different_clusters_establish_independence: bool,
    pub establishes_truth: bool,
}

#[derive(Debug, thiserror::Error, Eq, PartialEq)]
pub enum ClusterError {
    #[error("invalid, duplicate or excessively large graph/selection")]
    Invalid,
    #[error("referenced evidence or parent was not supplied")]
    Missing,
    #[error("lineage has a cycle")]
    Cycle,
    #[error("lineage has a revoked or inadmissible ancestor")]
    Withdrawn,
}

struct Trace {
    roots: BTreeSet<Uuid>,
    documents: BTreeSet<Uuid>,
}

fn trace_lineage(node: Uuid, nodes: &BTreeMap<Uuid, &LineageNode>) -> Result<Trace, ClusterError> {
    let mut trace = Trace {
        roots: BTreeSet::new(),
        documents: BTreeSet::new(),
    };
    let mut active = BTreeSet::new();
    let mut visited = BTreeSet::new();
    let mut stack = vec![(node, false)];
    while let Some((id, done)) = stack.pop() {
        if done {
            active.remove(&id);
            visited.insert(id);
            continue;
        }
        if visited.contains(&id) {
            continue;
        }
        if !active.insert(id) {
            return Err(ClusterError::Cycle);
        }
        let source = nodes.get(&id).ok_or(ClusterError::Missing)?;
        if !source.admissible {
            return Err(ClusterError::Withdrawn);
        }
        if source.parents.is_empty() {
            trace.roots.insert(id);
        }
        if let Some(doc) = source.source_document_id {
            trace.documents.insert(doc);
        }
        stack.push((id, true));
        stack.extend(source.parents.iter().rev().map(|parent| (*parent, false)));
    }
    Ok(trace)
}

fn find(parents: &mut [usize], mut i: usize) -> usize {
    while parents[i] != i {
        let next = parents[i];
        parents[i] = parents[next];
        i = next;
    }
    i
}

fn union(parents: &mut [usize], a: usize, b: usize) {
    let (left, right) = (find(parents, a), find(parents, b));
    if left != right {
        parents[right] = left;
    }
}

/// Group selected observations when they share any known root OR any recorded
/// source document along their ancestry. Transitivity is deliberate: A related to B, B related to C
/// means all three must be kept in one conservative dependence component.
///
/// The supplied graph must include every known ancestor of selected evidence.
/// An absent parent fails closed. It does NOT claim undiscovered relationships
/// are impossible, and it must not be fed untrusted caller-supplied lineage.
pub fn cluster(
    input_nodes: &[LineageNode],
    selected_ids: &[Uuid],
) -> Result<ClusterReport, ClusterError> {
    if input_nodes.is_empty()
        || input_nodes.len() > 4096
        || selected_ids.is_empty()
        || selected_ids.len() > 64
    {
        return Err(ClusterError::Invalid);
    }
    let mut nodes = BTreeMap::new();
    for n in input_nodes {
        if n.evidence_id.is_nil()
            || n.parents.len() > 128
            || n.parents.iter().any(Uuid::is_nil)
            || n.parents.contains(&n.evidence_id)
            || n.parents.iter().copied().collect::<BTreeSet<_>>().len() != n.parents.len()
            || nodes.insert(n.evidence_id, n).is_some()
        {
            return Err(ClusterError::Invalid);
        }
    }
    let selected = selected_ids.iter().copied().collect::<BTreeSet<_>>();
    if selected.len() != selected_ids.len() {
        return Err(ClusterError::Invalid);
    }
    let selected = selected.into_iter().collect::<Vec<_>>();
    let mut by_selected = Vec::with_capacity(selected.len());
    for id in &selected {
        by_selected.push(trace_lineage(*id, &nodes)?);
    }

    let mut groups = (0..selected.len()).collect::<Vec<_>>();
    for i in 0..selected.len() {
        for j in i + 1..selected.len() {
            let same_document = !by_selected[i]
                .documents
                .is_disjoint(&by_selected[j].documents);
            let share_origin = !by_selected[i].roots.is_disjoint(&by_selected[j].roots);
            if same_document || share_origin {
                union(&mut groups, i, j);
            }
        }
    }
    let mut clusters_by_group: BTreeMap<usize, DependenceCluster> = BTreeMap::new();
    for i in 0..selected.len() {
        let group = find(&mut groups, i);
        let record = clusters_by_group
            .entry(group)
            .or_insert_with(|| DependenceCluster {
                component_key: selected[i],
                selected_evidence_ids: Vec::new(),
                documented_roots: Vec::new(),
                shared_document_ids: Vec::new(),
                represents_statistical_independence: false,
            });
        record.component_key = record.component_key.min(selected[i]);
        record.selected_evidence_ids.push(selected[i]);
        record
            .documented_roots
            .extend(by_selected[i].roots.iter().copied());
        record
            .shared_document_ids
            .extend(by_selected[i].documents.iter().copied());
    }
    let mut clusters = clusters_by_group.into_values().collect::<Vec<_>>();
    for item in &mut clusters {
        item.documented_roots.sort_unstable();
        item.documented_roots.dedup();
        item.shared_document_ids.sort_unstable();
        item.shared_document_ids.dedup();
    }
    clusters.sort_by_key(|c| c.component_key);
    Ok(ClusterReport {
        clusters,
        selected_count: selected.len(),
        known_lineage_node_count: nodes.len(),
        all_declared_ancestors_present: true, // declared ancestors supplied; no global completeness claim
        different_clusters_establish_independence: false,
        establishes_truth: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }
    fn node(n: u128, parents: &[u128], doc: Option<u128>) -> LineageNode {
        LineageNode {
            evidence_id: id(n),
            parents: parents.iter().map(|x| id(*x)).collect(),
            admissible: true,
            source_document_id: doc.map(id),
        }
    }
    #[test]
    fn two_documents_derived_from_same_root_are_one_cluster() {
        let nodes = vec![
            node(1, &[], None),
            node(2, &[1], Some(20)),
            node(3, &[1], Some(30)),
        ];
        let r = cluster(&nodes, &[id(2), id(3)]).unwrap();
        assert_eq!(r.clusters.len(), 1);
        assert_eq!(r.clusters[0].documented_roots, vec![id(1)]);
        assert_eq!(r.clusters[0].shared_document_ids, vec![id(20), id(30)]);
        assert!(!r.different_clusters_establish_independence);
    }
    #[test]
    fn different_roots_on_same_document_are_one_cluster() {
        let r = cluster(
            &[node(1, &[], Some(20)), node(2, &[], Some(20))],
            &[id(1), id(2)],
        )
        .unwrap();
        assert_eq!(r.clusters.len(), 1);
        assert_eq!(r.clusters[0].documented_roots, vec![id(1), id(2)]);
    }
    #[test]
    fn separate_fragments_of_ancestral_document_link_derived_documents() {
        let nodes = vec![
            node(1, &[], Some(20)),
            node(2, &[], Some(20)),
            node(3, &[1], Some(30)),
            node(4, &[2], Some(40)),
        ];
        let r = cluster(&nodes, &[id(3), id(4)]).unwrap();
        assert_eq!(r.clusters.len(), 1);
        assert_eq!(r.clusters[0].documented_roots, vec![id(1), id(2)]);
        assert_eq!(
            r.clusters[0].shared_document_ids,
            vec![id(20), id(30), id(40)]
        );
        assert!(!r.different_clusters_establish_independence);
    }
    #[test]
    fn transitive_overlap_is_conservatively_merged() {
        let nodes = vec![
            node(1, &[], None),
            node(2, &[], None),
            node(3, &[], None),
            node(10, &[1, 2], Some(40)),
            node(11, &[2, 3], Some(50)),
            node(12, &[3], Some(60)),
        ];
        let r = cluster(&nodes, &[id(10), id(11), id(12)]).unwrap();
        assert_eq!(r.clusters.len(), 1);
        assert_eq!(r.clusters[0].documented_roots, vec![id(1), id(2), id(3)]);
    }
    #[test]
    fn distinct_roots_do_not_certify_independence() {
        let r = cluster(
            &[node(1, &[], Some(10)), node(2, &[], Some(20))],
            &[id(1), id(2)],
        )
        .unwrap();
        assert_eq!(r.clusters.len(), 2);
        assert!(!r.different_clusters_establish_independence);
    }
    #[test]
    fn missing_ancestor_fails_closed() {
        assert_eq!(
            cluster(&[node(2, &[1], Some(20))], &[id(2)]),
            Err(ClusterError::Missing)
        );
    }
    #[test]
    fn cycles_fail_closed() {
        assert_eq!(
            cluster(&[node(1, &[2], None), node(2, &[1], Some(20))], &[id(2)]),
            Err(ClusterError::Cycle)
        );
    }
    #[test]
    fn inadmissible_ancestor_fails_closed() {
        let mut n = node(1, &[], None);
        n.admissible = false;
        assert_eq!(
            cluster(&[n, node(2, &[1], Some(20))], &[id(2)]),
            Err(ClusterError::Withdrawn)
        );
    }
    #[test]
    fn duplicate_selection_or_nodes_are_invalid() {
        let a = node(1, &[], None);
        assert_eq!(
            cluster(&[a.clone(), a.clone()], &[id(1)]),
            Err(ClusterError::Invalid)
        );
        assert_eq!(cluster(&[a], &[id(1), id(1)]), Err(ClusterError::Invalid));
    }
    #[test]
    fn diamond_is_not_a_cycle() {
        let nodes = vec![
            node(1, &[], None),
            node(2, &[1], None),
            node(3, &[1], None),
            node(4, &[2, 3], Some(20)),
        ];
        let r = cluster(&nodes, &[id(4)]).unwrap();
        assert_eq!(r.clusters[0].documented_roots, vec![id(1)]);
    }
}
