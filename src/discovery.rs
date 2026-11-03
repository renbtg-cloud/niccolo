//! Conservative, deterministic investigative lead proposals.
//! They ask what to verify; they never assert that accounts are independent,
//! coordinated, false or true, nor mutate the provenance graph.
use crate::postgres_repository::WorkspaceAssessment;
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InvestigativeLead {
    pub kind: &'static str,
    pub question: &'static str,
    pub report_indices: Vec<usize>,
    pub provenance_ids: Vec<Uuid>,
    /// The API never silently suggests it returned all supporting IDs.
    pub provenance_ids_truncated: bool,
}

/// Return at most three distinct prompts in stable priority order. Every
/// referenced ID originates in the already-authorized graph assessment.
pub fn propose(assessment: &WorkspaceAssessment) -> Vec<InvestigativeLead> {
    let mut out = Vec::with_capacity(3);
    if let Some(shared) = assessment.shared_origins.first() {
        out.push(InvestigativeLead {
            kind: "verify_shared_origin",
            question: "Can the common originating record be inspected and attributed directly?",
            report_indices: vec![shared.left_report_index, shared.right_report_index],
            provenance_ids: shared.origins.iter().take(8).copied().collect(),
            provenance_ids_truncated: shared.origins.len() > 8,
        });
    }
    if let Some(shared) = assessment.shared_source_documents.first() {
        out.push(InvestigativeLead {
            kind: "inspect_shared_document",
            question: "Which original messages or excerpts share this uploaded document, and in what context?",
            report_indices: vec![shared.left_report_index, shared.right_report_index],
            provenance_ids: shared.documents.iter().take(8).copied().collect(),
            provenance_ids_truncated: shared.documents.len() > 8,
        });
    }
    // A root is a provenance boundary, not necessarily a firsthand observer.
    out.push(InvestigativeLead {
        kind: "verify_source_attribution",
        question: "Who supplied the earliest available record, when, and what establishes its attribution?",
        report_indices: (0..assessment.report_count.min(2)).collect(),
        provenance_ids: assessment.traceable_roots.iter().take(8).copied().collect(),
        provenance_ids_truncated: assessment.traceable_roots.len() > 8,
    });
    if out.len() < 2 && assessment.report_count > 1 {
        out.push(InvestigativeLead {
            kind: "seek_independent_context",
            question: "Is there separately documented context for these reports, beyond their recorded source roots?",
            report_indices: vec![0, 1],
            provenance_ids: Vec::new(),
            provenance_ids_truncated: false,
        });
    }
    if out.len() < 2 {
        out.push(InvestigativeLead {
            kind: "verify_original_context",
            question: "Can the full original context be checked before treating this excerpt as representative?",
            report_indices: vec![0],
            provenance_ids: Vec::new(),
            provenance_ids_truncated: false,
        });
    }
    out.truncate(3);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::postgres_repository::{SharedDocuments, SharedOrigins};
    use std::collections::BTreeSet;
    fn sample(n: usize) -> WorkspaceAssessment {
        WorkspaceAssessment {
            report_count: n,
            document_ids_per_report: vec![BTreeSet::new(); n],
            shared_source_documents: vec![],
            roots_per_report: vec![BTreeSet::new(); n],
            traceable_roots: BTreeSet::new(),
            shared_origins: vec![],
            independence_established: false,
        }
    }
    #[test]
    fn single_report_proposes_two_nonassertive_checks() {
        let r = propose(&sample(1));
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].kind, "verify_source_attribution");
        assert_eq!(r[1].kind, "verify_original_context");
    }
    #[test]
    fn multiple_reports_without_overlap_do_not_claim_independence() {
        let r = propose(&sample(2));
        assert_eq!(r.len(), 2);
        assert_eq!(r[1].kind, "seek_independent_context");
    }
    #[test]
    fn shared_origin_is_an_investigative_cue_not_a_truth_claim() {
        let mut a = sample(2);
        let id = Uuid::new_v4();
        a.shared_origins.push(SharedOrigins {
            left_report_index: 0,
            right_report_index: 1,
            origins: BTreeSet::from([id]),
        });
        let r = propose(&a);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].kind, "verify_shared_origin");
        assert_eq!(r[0].provenance_ids, vec![id]);
        assert!(r.iter().all(|x| !x.question.contains("independent fact")));
    }
    #[test]
    fn shared_document_is_a_separate_lead() {
        let mut a = sample(2);
        let id = Uuid::new_v4();
        a.shared_source_documents.push(SharedDocuments {
            left_report_index: 0,
            right_report_index: 1,
            documents: BTreeSet::from([id]),
        });
        let r = propose(&a);
        assert_eq!(r[0].kind, "inspect_shared_document");
        assert_eq!(r[0].provenance_ids, vec![id]);
    }
    #[test]
    fn provenance_identifiers_are_bounded_with_disclosed_truncation() {
        let mut a = sample(2);
        let roots = (0..100).map(|_| Uuid::new_v4()).collect::<BTreeSet<_>>();
        a.shared_origins.push(SharedOrigins {
            left_report_index: 0,
            right_report_index: 1,
            origins: roots,
        });
        let leads = propose(&a);
        assert_eq!(leads[0].provenance_ids.len(), 8);
        assert!(leads[0].provenance_ids_truncated);
    }
    #[test]
    fn at_most_three_and_deterministic() {
        let mut a = sample(2);
        for _ in 0..100 {
            a.shared_origins.push(SharedOrigins {
                left_report_index: 0,
                right_report_index: 1,
                origins: BTreeSet::from([Uuid::new_v4()]),
            });
            a.shared_source_documents.push(SharedDocuments {
                left_report_index: 0,
                right_report_index: 1,
                documents: BTreeSet::from([Uuid::new_v4()]),
            });
        }
        let before = a.clone();
        assert_eq!(propose(&a), propose(&a));
        assert_eq!(a, before);
        assert_eq!(propose(&a).len(), 3);
    }
}
