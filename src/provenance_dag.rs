//! Traceable multi-parent provenance with conservative failure semantics.
//! Finding different traceable roots NEVER establishes statistical independence,
//! factual correctness, motive, intent, or that witnesses coordinated.
use crate::EvidenceId;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceNode {
    pub id: EvidenceId,
    /// One record may derive from multiple other records, none implying truth.
    pub parents: BTreeSet<EvidenceId>,
    pub admissible: bool,
}
impl SourceNode {
    pub fn original(id: u64) -> Self {
        Self {
            id: EvidenceId(id),
            parents: BTreeSet::new(),
            admissible: true,
        }
    }
    pub fn derived(id: u64, parents: impl IntoIterator<Item = EvidenceId>) -> Self {
        Self {
            id: EvidenceId(id),
            parents: parents.into_iter().collect(),
            admissible: true,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum TraceError {
    EmptyReports,
    InvalidId,
    Duplicate(EvidenceId),
    Missing(EvidenceId),
    Withdrawn(EvidenceId),
    Cycle(EvidenceId),
}

#[derive(Debug, PartialEq, Eq)]
pub struct CommonOrigin {
    pub left_report_index: usize,
    pub right_report_index: usize,
    pub origins: BTreeSet<EvidenceId>,
}
#[derive(Debug, PartialEq, Eq)]
pub struct SourceAssessment {
    pub report_count: usize,
    pub traceable_roots: BTreeSet<EvidenceId>,
    pub roots_per_report: Vec<BTreeSet<EvidenceId>>,
    pub shared_origins: Vec<CommonOrigin>,
    /// Distinct observed roots are *not* statistical independence of witnesses.
    pub independence_established: bool,
}

#[derive(Default, Clone)]
pub struct SourceGraph {
    nodes: BTreeMap<EvidenceId, SourceNode>,
}
impl SourceGraph {
    pub fn insert(&mut self, node: SourceNode) -> Result<(), TraceError> {
        if node.id.0 == 0 || node.parents.iter().any(|p| p.0 == 0) {
            return Err(TraceError::InvalidId);
        }
        if self.nodes.contains_key(&node.id) {
            return Err(TraceError::Duplicate(node.id));
        }
        self.nodes.insert(node.id, node);
        Ok(())
    }
    pub fn set_admissible(&mut self, id: EvidenceId, admissible: bool) -> Result<(), TraceError> {
        let node = self.nodes.get_mut(&id).ok_or(TraceError::Missing(id))?;
        node.admissible = admissible;
        Ok(())
    }
    /// Iterative DFS, so very deep non-cyclic lineages do not consume call stack.
    /// Every reachable ancestor must exist and remain admissible; otherwise fail closed.
    pub fn roots(&self, id: EvidenceId) -> Result<BTreeSet<EvidenceId>, TraceError> {
        let mut active = BTreeSet::new();
        let mut visited = BTreeSet::new();
        let mut origins = BTreeSet::new();
        let mut stack = vec![(id, false)];
        while let Some((current, done)) = stack.pop() {
            if done {
                active.remove(&current);
                visited.insert(current);
                continue;
            }
            if visited.contains(&current) {
                continue;
            }
            if !active.insert(current) {
                return Err(TraceError::Cycle(current));
            }
            let node = self
                .nodes
                .get(&current)
                .ok_or(TraceError::Missing(current))?;
            if !node.admissible {
                return Err(TraceError::Withdrawn(current));
            }
            if node.parents.is_empty() {
                origins.insert(current);
            }
            stack.push((current, true));
            for parent in node.parents.iter().rev() {
                // defer cycle test until the node is popped: sibling shared ancestors aren't cycles.
                stack.push((*parent, false));
            }
        }
        Ok(origins)
    }
    pub fn assess(&self, reports: &[EvidenceId]) -> Result<SourceAssessment, TraceError> {
        if reports.is_empty() {
            return Err(TraceError::EmptyReports);
        }
        let mut roots_per_report = Vec::with_capacity(reports.len());
        let mut roots = BTreeSet::new();
        for &report in reports {
            let r = self.roots(report)?;
            roots.extend(r.iter().copied());
            roots_per_report.push(r);
        }
        let mut shared_origins = Vec::new();
        for left in 0..reports.len() {
            for right in left + 1..reports.len() {
                let overlap = roots_per_report[left]
                    .intersection(&roots_per_report[right])
                    .copied()
                    .collect::<BTreeSet<_>>();
                if !overlap.is_empty() {
                    shared_origins.push(CommonOrigin {
                        left_report_index: left,
                        right_report_index: right,
                        origins: overlap,
                    });
                }
            }
        }
        Ok(SourceAssessment {
            report_count: reports.len(),
            traceable_roots: roots,
            roots_per_report,
            shared_origins,
            independence_established: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dag_evolution::{HOLDOUT, TRAIN};
    use crate::evolution::parse_fixtures;

    fn id(n: u64) -> EvidenceId {
        EvidenceId(n)
    }
    #[test]
    fn multi_parent_and_shared_origin_detected() {
        let mut g = SourceGraph::default();
        g.insert(SourceNode::original(1)).unwrap();
        g.insert(SourceNode::original(2)).unwrap();
        g.insert(SourceNode::derived(3, [id(1), id(2)])).unwrap();
        g.insert(SourceNode::derived(4, [id(1)])).unwrap();
        let r = g.assess(&[id(3), id(4)]).unwrap();
        assert_eq!(r.traceable_roots, BTreeSet::from([id(1), id(2)]));
        assert_eq!(r.shared_origins.len(), 1);
        assert_eq!(r.shared_origins[0].origins, BTreeSet::from([id(1)]));
        assert!(!r.independence_established);
    }
    #[test]
    fn diamond_is_not_cycle() {
        let mut g = SourceGraph::default();
        g.insert(SourceNode::original(1)).unwrap();
        g.insert(SourceNode::derived(2, [id(1)])).unwrap();
        g.insert(SourceNode::derived(3, [id(1)])).unwrap();
        g.insert(SourceNode::derived(4, [id(2), id(3)])).unwrap();
        assert_eq!(g.roots(id(4)).unwrap(), BTreeSet::from([id(1)]));
    }
    #[test]
    fn cycle_anywhere_fails_closed() {
        let mut g = SourceGraph::default();
        g.insert(SourceNode::original(1)).unwrap();
        g.insert(SourceNode::derived(2, [id(3)])).unwrap();
        g.insert(SourceNode::derived(3, [id(2)])).unwrap();
        g.insert(SourceNode::derived(4, [id(1), id(3)])).unwrap();
        assert!(matches!(g.roots(id(4)), Err(TraceError::Cycle(_))));
    }
    #[test]
    fn unknown_or_revoked_branch_invalidates_merge() {
        let mut g = SourceGraph::default();
        g.insert(SourceNode::original(1)).unwrap();
        g.insert(SourceNode::derived(3, [id(1), id(2)])).unwrap();
        assert_eq!(g.roots(id(3)), Err(TraceError::Missing(id(2))));
        g.insert(SourceNode::original(2)).unwrap();
        g.set_admissible(id(2), false).unwrap();
        assert_eq!(g.roots(id(3)), Err(TraceError::Withdrawn(id(2))));
    }
    #[test]
    fn duplicates_and_invalid_nodes() {
        let mut g = SourceGraph::default();
        g.insert(SourceNode::original(1)).unwrap();
        assert_eq!(
            g.insert(SourceNode::original(1)),
            Err(TraceError::Duplicate(id(1)))
        );
        assert_eq!(
            g.insert(SourceNode::original(0)),
            Err(TraceError::InvalidId)
        );
        assert_eq!(g.assess(&[]), Err(TraceError::EmptyReports));
    }
    #[test]
    fn deep_derivations_use_no_recursion() {
        let mut g = SourceGraph::default();
        g.insert(SourceNode::original(1)).unwrap();
        for n in 2..=10_000 {
            g.insert(SourceNode::derived(n, [id(n - 1)])).unwrap();
        }
        assert_eq!(g.roots(id(10_000)).unwrap(), BTreeSet::from([id(1)]));
    }
    /// Uses the same synthetic fixtures as the compiled evolution candidates,
    /// to ensure that the library implementation and candidate agree on the oracle.
    #[test]
    fn library_matches_33_fixture_expectations() {
        let mut total = 0;
        for raw in [TRAIN, HOLDOUT] {
            for case in parse_fixtures(raw).unwrap() {
                total += 1;
                let mut graph = SourceGraph::default();
                let mut invalid = false;
                for token in case.edges.split(',') {
                    let fields = token.split(':').collect::<Vec<_>>();
                    if fields.len() != 3 {
                        invalid = true;
                        break;
                    }
                    let number = fields[0].parse::<u64>().unwrap();
                    let mut parents = Vec::new();
                    if fields[1] != "0" {
                        for piece in fields[1].split('+') {
                            let p = piece.parse::<u64>().unwrap();
                            if parents.contains(&id(p)) {
                                invalid = true;
                            }
                            parents.push(id(p));
                        }
                    }
                    let mut node = SourceNode::derived(number, parents);
                    node.admissible = fields[2] == "A";
                    if fields[2] != "A" && fields[2] != "R" {
                        invalid = true;
                    }
                    if graph.insert(node).is_err() {
                        invalid = true;
                    }
                }
                let reports = case
                    .observations
                    .split(',')
                    .map(|v| id(v.parse::<u64>().unwrap()))
                    .collect::<Vec<_>>();
                let got = if invalid {
                    "UNKNOWN".to_string()
                } else {
                    match graph.assess(&reports) {
                        Ok(a) => format!("COUNT:{}", a.traceable_roots.len()),
                        Err(_) => "UNKNOWN".into(),
                    }
                };
                assert_eq!(got, case.expected, "fixture {} mismatch", case.id);
            }
        }
        assert_eq!(total, 33);
    }
}
