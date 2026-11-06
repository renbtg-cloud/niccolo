//! Small dependency-free experiment, not the complete Duhme v0180 contract.
//! Describes *what a source says* without promoting it to ground truth.
pub mod analysis_context_summary;
pub mod analysis_context_usage;
pub mod analysis_coverage;
pub mod analysis_delta;
pub mod analysis_output;
pub mod analysis_review;
pub mod analysis_snapshot;
pub mod api;
pub mod authz;
pub mod benchmark;
pub mod clarification;
pub mod clarification_answer;
pub mod dag_evolution;
pub mod discovery;
pub mod evolution;
pub mod human_feedback;
pub mod hypotheses;
pub mod identity;
pub mod postgres_ingest;
pub mod postgres_repository;
pub mod provenance_dag;
pub mod question_refs;
pub mod situation;
pub mod situation_store;
pub mod source_catalog;
pub mod source_import;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Person(pub String);

impl Person {
    pub fn named(s: &str) -> Self {
        Self(s.to_owned())
    }
}

/// An expression can recursively contain another attributed expression.
/// Representability of deep nesting is not evidence of the actual mental state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expression {
    Event {
        subject: Person,
        predicate: String,
        object: Person,
    },
    Says {
        speaker: Person,
        content: Box<Expression>,
    },
    Believes {
        holder: Person,
        content: Box<Expression>,
    },
}

impl Expression {
    pub fn event(subject: &str, predicate: &str, object: &str) -> Self {
        Self::Event {
            subject: Person::named(subject),
            predicate: predicate.to_owned(),
            object: Person::named(object),
        }
    }
    pub fn says(speaker: &str, content: Self) -> Self {
        Self::Says {
            speaker: Person::named(speaker),
            content: Box::new(content),
        }
    }
    pub fn believes(holder: &str, content: Self) -> Self {
        Self::Believes {
            holder: Person::named(holder),
            content: Box::new(content),
        }
    }
    pub fn nesting_depth(&self) -> usize {
        match self {
            Self::Event { .. } => 0,
            Self::Says { content, .. } | Self::Believes { content, .. } => {
                1 + content.nesting_depth()
            }
        }
    }
}

impl fmt::Display for Expression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Event {
                subject,
                predicate,
                object,
            } => {
                write!(f, "{} {} {}", subject.0, predicate, object.0)
            }
            Self::Says { speaker, content } => write!(f, "{} says [{}]", speaker.0, content),
            Self::Believes { holder, content } => write!(f, "{} believes [{}]", holder.0, content),
        }
    }
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct EvidenceId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EvidenceClass {
    DirectObservation,
    ReportedSpeech,
    CopyOrTranscript,
    AnalystInference,
}

/// `parent` documents a *known* transmission/derivation link, not guaranteed
/// statistical independence of evidence with different roots.
#[derive(Clone, Debug)]
pub struct Evidence {
    pub id: EvidenceId,
    pub parent: Option<EvidenceId>,
    pub class: EvidenceClass,
    pub content: String,
    pub admissible: bool,
}

impl Evidence {
    pub fn new(id: u64, parent: Option<u64>, class: EvidenceClass, content: &str) -> Self {
        Self {
            id: EvidenceId(id),
            parent: parent.map(EvidenceId),
            class,
            content: content.to_owned(),
            admissible: true,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum EvidenceError {
    DuplicateId(EvidenceId),
    Missing(EvidenceId),
    DerivationCycle(EvidenceId),
    NotAdmissible(EvidenceId),
}

#[derive(Default)]
pub struct EvidenceIndex {
    evidence: BTreeMap<EvidenceId, Evidence>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct LineageAssessment {
    pub admitted_reports: usize,
    pub traceable_roots: BTreeSet<EvidenceId>,
    /// Never assert statistical independence merely from distinct roots.
    pub independence_established: bool,
}

impl EvidenceIndex {
    pub fn insert(&mut self, evidence: Evidence) -> Result<(), EvidenceError> {
        if self.evidence.contains_key(&evidence.id) {
            return Err(EvidenceError::DuplicateId(evidence.id));
        }
        self.evidence.insert(evidence.id, evidence);
        Ok(())
    }
    pub fn get(&self, id: EvidenceId) -> Option<&Evidence> {
        self.evidence.get(&id)
    }
    pub fn set_admissible(&mut self, id: EvidenceId, admitted: bool) -> Result<(), EvidenceError> {
        let e = self
            .evidence
            .get_mut(&id)
            .ok_or(EvidenceError::Missing(id))?;
        e.admissible = admitted;
        Ok(())
    }
    pub fn root(&self, id: EvidenceId) -> Result<EvidenceId, EvidenceError> {
        let mut current = id;
        let mut seen = BTreeSet::new();
        loop {
            if !seen.insert(current) {
                return Err(EvidenceError::DerivationCycle(current));
            }
            let ev = self
                .evidence
                .get(&current)
                .ok_or(EvidenceError::Missing(current))?;
            if !ev.admissible {
                return Err(EvidenceError::NotAdmissible(current));
            }
            match ev.parent {
                Some(parent) => current = parent,
                None => return Ok(current),
            }
        }
    }
    /// Group by documented transmission origin. This does NOT certify witness
    /// independence, deliberate coordination, factual correctness or motive.
    pub fn assess_lineages(&self, ids: &[EvidenceId]) -> Result<LineageAssessment, EvidenceError> {
        let mut roots = BTreeSet::new();
        for id in ids {
            roots.insert(self.root(*id)?);
        }
        Ok(LineageAssessment {
            admitted_reports: ids.len(),
            traceable_roots: roots,
            independence_established: false,
        })
    }
}

/// Hypothesis-generation output is a question, not an evidence record.
#[derive(Debug, PartialEq, Eq)]
pub struct InvestigativeQuestion {
    pub question: String,
    pub triggered_by: Vec<EvidenceId>,
}

pub fn propose_access_question(ids: Vec<EvidenceId>) -> InvestigativeQuestion {
    InvestigativeQuestion {
        question: "Who knew about the visit, and who could arrange the discovery?".to_owned(),
        triggered_by: ids,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample_index() -> EvidenceIndex {
        let mut index = EvidenceIndex::default();
        index
            .insert(Evidence::new(
                1,
                None,
                EvidenceClass::ReportedSpeech,
                "Clerk's memorandum about a blue-thread packet",
            ))
            .unwrap();
        for id in 2..=4 {
            index
                .insert(Evidence::new(
                    id,
                    Some(1),
                    EvidenceClass::CopyOrTranscript,
                    "Witness testimony repeats the memorandum",
                ))
                .unwrap();
        }
        index
    }
    #[test]
    fn one_original_three_copies_is_one_traceable_root() {
        let summary = sample_index()
            .assess_lineages(&[EvidenceId(2), EvidenceId(3), EvidenceId(4)])
            .unwrap();
        assert_eq!(summary.admitted_reports, 3);
        assert_eq!(summary.traceable_roots, BTreeSet::from([EvidenceId(1)]));
        assert!(!summary.independence_established);
    }
    #[test]
    fn two_different_roots_are_not_certified_independent() {
        let mut i = sample_index();
        i.insert(Evidence::new(
            5,
            None,
            EvidenceClass::DirectObservation,
            "Gate ledger",
        ))
        .unwrap();
        let s = i.assess_lineages(&[EvidenceId(2), EvidenceId(5)]).unwrap();
        assert_eq!(s.traceable_roots.len(), 2);
        assert!(!s.independence_established);
    }
    #[test]
    fn missing_parent_fails_closed() {
        let mut i = EvidenceIndex::default();
        i.insert(Evidence::new(
            8,
            Some(9),
            EvidenceClass::ReportedSpeech,
            "orphan",
        ))
        .unwrap();
        assert_eq!(
            i.root(EvidenceId(8)),
            Err(EvidenceError::Missing(EvidenceId(9)))
        );
    }
    #[test]
    fn derivation_cycle_is_rejected() {
        let mut i = EvidenceIndex::default();
        i.insert(Evidence::new(
            8,
            Some(9),
            EvidenceClass::CopyOrTranscript,
            "copy A",
        ))
        .unwrap();
        i.insert(Evidence::new(
            9,
            Some(8),
            EvidenceClass::CopyOrTranscript,
            "copy B",
        ))
        .unwrap();
        assert_eq!(
            i.root(EvidenceId(8)),
            Err(EvidenceError::DerivationCycle(EvidenceId(8)))
        );
    }
    #[test]
    fn duplicate_ids_are_rejected() {
        let mut i = sample_index();
        assert_eq!(
            i.insert(Evidence::new(
                1,
                None,
                EvidenceClass::ReportedSpeech,
                "another"
            )),
            Err(EvidenceError::DuplicateId(EvidenceId(1)))
        );
    }
    #[test]
    fn withdrawn_source_disqualifies_copies() {
        let mut i = sample_index();
        i.set_admissible(EvidenceId(1), false).unwrap();
        assert_eq!(
            i.assess_lineages(&[EvidenceId(2)]),
            Err(EvidenceError::NotAdmissible(EvidenceId(1)))
        );
    }
    #[test]
    fn withdrawn_copy_disqualifies_just_that_copy() {
        let mut i = sample_index();
        i.set_admissible(EvidenceId(2), false).unwrap();
        assert_eq!(
            i.root(EvidenceId(2)),
            Err(EvidenceError::NotAdmissible(EvidenceId(2)))
        );
        assert_eq!(i.root(EvidenceId(3)), Ok(EvidenceId(1)));
    }
    #[test]
    fn missing_evidence_is_error() {
        assert_eq!(
            sample_index().root(EvidenceId(99)),
            Err(EvidenceError::Missing(EvidenceId(99)))
        );
    }
    #[test]
    fn changing_nonexistent_admissibility_errors() {
        assert_eq!(
            sample_index().set_admissible(EvidenceId(99), false),
            Err(EvidenceError::Missing(EvidenceId(99)))
        );
    }
    #[test]
    fn nested_belief_is_not_flattened_into_event() {
        let atom = Expression::event("Mary", "criticized", "James");
        let expr = Expression::says("Jane", Expression::believes("Joe", atom.clone()));
        assert_ne!(atom, expr);
        assert_eq!(expr.nesting_depth(), 2);
        assert_eq!(
            expr.to_string(),
            "Jane says [Joe believes [Mary criticized James]]"
        );
    }
    #[test]
    fn arbitrarily_nested_beliefs_are_representable_not_verified() {
        let mut e = Expression::event("Mary", "criticized", "James");
        for _ in 0..128 {
            e = Expression::believes("Someone", e);
        }
        assert_eq!(e.nesting_depth(), 128);
    }
    #[test]
    fn speech_and_belief_are_not_same_claim() {
        let atom = Expression::event("Mary", "criticized", "James");
        assert_ne!(
            Expression::says("Jane", atom.clone()),
            Expression::believes("Jane", atom)
        );
    }
    #[test]
    fn hypothesized_question_is_not_evidence() {
        let q = propose_access_question(vec![EvidenceId(3)]);
        assert!(q.question.ends_with('?'));
        assert_eq!(q.triggered_by, vec![EvidenceId(3)]);
    }
    #[test]
    fn empty_report_set_produces_no_roots() {
        let s = sample_index().assess_lineages(&[]).unwrap();
        assert!(s.traceable_roots.is_empty());
        assert_eq!(s.admitted_reports, 0);
    }
    #[test]
    fn same_evidence_twice_does_not_create_two_roots() {
        let s = sample_index()
            .assess_lineages(&[EvidenceId(2), EvidenceId(2)])
            .unwrap();
        assert_eq!(s.traceable_roots.len(), 1);
        assert!(!s.independence_established);
    }
}
pub mod analysis_claims;
pub mod analysis_manifest;
pub mod analysis_export;
pub mod analysis_integrity;
pub mod analysis_structure;
pub mod analysis_actions;
pub mod analysis_timeline;
pub mod analysis_read_models;
pub mod review_package;
pub mod attention_control;
pub mod reanalysis_control;
pub mod action_authorization;
pub mod review_session;
pub mod situation_activity;
pub mod inspection;
pub mod review_acknowledgement;
pub mod control_ledger;
pub mod review_workflow;
pub mod reasoning_handoff;
pub mod review_governance;
pub mod analysis_compare;
pub mod review_agenda;
pub mod review_checklist;
pub mod decision_points;
pub mod analysis_assumptions;
pub mod human_briefing;
pub mod human_agency;
pub mod human_control;


pub mod bff_executor;

pub mod agentic_policy;
pub mod proactive_briefing;
pub mod reanalysis_revision;

pub mod operation_budget;

pub mod reanalysis_runtime;
pub mod analysis_quality;
pub mod reanalysis_advisor;
pub mod investigation_policy;
