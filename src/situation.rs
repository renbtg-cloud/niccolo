//! Bounded Situation state for visible reanalysis.
//!
//! This module deliberately does not implement hypothesis-rejection memory or
//! machine belief scoring. It preserves human-provided premises and immutable
//! snapshots so later reasoning can say which state it actually used.
use crate::EvidenceId;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PremiseId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PremiseKind {
    /// Context asserted by the user. It is attributable, not automatically verified.
    UserReported,
    /// Explicit counterfactual / WHAT-IF. It must never silently become actual evidence.
    Hypothetical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PremiseStatus {
    Active,
    Retracted,
    CorrectedBy(PremiseId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Premise {
    pub id: PremiseId,
    pub kind: PremiseKind,
    pub text: String,
    pub source_refs: BTreeSet<EvidenceId>,
    pub status: PremiseStatus,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SituationRevision {
    pub revision: u64,
    pub focus: String,
    pub active_premises: BTreeSet<PremiseId>,
    pub source_refs: BTreeSet<EvidenceId>,
    /// Stable digest of exactly the focus/premise/source snapshot used.
    pub fingerprint: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnalysisSnapshot {
    pub revision: u64,
    pub fingerprint: String,
    pub focus: String,
    pub premises: Vec<Premise>,
    pub source_refs: BTreeSet<EvidenceId>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum SituationError {
    EmptyText,
    DuplicatePremise(PremiseId),
    MissingPremise(PremiseId),
    InactivePremise(PremiseId),
    DuplicateRevision,
}

#[derive(Clone, Debug)]
pub struct Situation {
    focus: String,
    premises: BTreeMap<PremiseId, Premise>,
    source_refs: BTreeSet<EvidenceId>,
    revisions: Vec<SituationRevision>,
}

impl Situation {
    pub fn new(
        focus: &str,
        source_refs: impl IntoIterator<Item = EvidenceId>,
    ) -> Result<Self, SituationError> {
        let focus = clean_text(focus)?;
        let mut situation = Self {
            focus,
            premises: BTreeMap::new(),
            source_refs: source_refs.into_iter().collect(),
            revisions: Vec::new(),
        };
        situation.checkpoint()?;
        Ok(situation)
    }

    pub fn focus(&self) -> &str {
        &self.focus
    }

    pub fn revisions(&self) -> &[SituationRevision] {
        &self.revisions
    }

    pub fn premise(&self, id: PremiseId) -> Option<&Premise> {
        self.premises.get(&id)
    }

    pub fn set_focus(&mut self, focus: &str) -> Result<&SituationRevision, SituationError> {
        self.focus = clean_text(focus)?;
        self.checkpoint()
    }

    pub fn add_source_ref(&mut self, id: EvidenceId) -> Result<&SituationRevision, SituationError> {
        self.source_refs.insert(id);
        self.checkpoint()
    }

    pub fn add_user_report(
        &mut self,
        id: PremiseId,
        text: &str,
        source_refs: impl IntoIterator<Item = EvidenceId>,
    ) -> Result<&SituationRevision, SituationError> {
        self.add_premise(id, PremiseKind::UserReported, text, source_refs)
    }

    pub fn add_what_if(
        &mut self,
        id: PremiseId,
        text: &str,
        source_refs: impl IntoIterator<Item = EvidenceId>,
    ) -> Result<&SituationRevision, SituationError> {
        self.add_premise(id, PremiseKind::Hypothetical, text, source_refs)
    }

    pub fn correct(
        &mut self,
        old: PremiseId,
        replacement: PremiseId,
        text: &str,
        source_refs: impl IntoIterator<Item = EvidenceId>,
    ) -> Result<&SituationRevision, SituationError> {
        if self.premises.contains_key(&replacement) {
            return Err(SituationError::DuplicatePremise(replacement));
        }
        let kind = match self.premises.get(&old) {
            Some(p) if p.status == PremiseStatus::Active => p.kind,
            Some(_) => return Err(SituationError::InactivePremise(old)),
            None => return Err(SituationError::MissingPremise(old)),
        };
        let replacement_premise = Premise {
            id: replacement,
            kind,
            text: clean_text(text)?,
            source_refs: source_refs.into_iter().collect(),
            status: PremiseStatus::Active,
        };
        self.premises
            .get_mut(&old)
            .expect("presence checked above")
            .status = PremiseStatus::CorrectedBy(replacement);
        self.premises.insert(replacement, replacement_premise);
        self.checkpoint()
    }

    pub fn retract(&mut self, id: PremiseId) -> Result<&SituationRevision, SituationError> {
        match self.premises.get_mut(&id) {
            Some(p) if p.status == PremiseStatus::Active => p.status = PremiseStatus::Retracted,
            Some(_) => return Err(SituationError::InactivePremise(id)),
            None => return Err(SituationError::MissingPremise(id)),
        }
        self.checkpoint()
    }

    pub fn snapshot(&self) -> AnalysisSnapshot {
        let revision = self
            .revisions
            .last()
            .expect("Situation always has an initial revision");
        AnalysisSnapshot {
            revision: revision.revision,
            fingerprint: revision.fingerprint.clone(),
            focus: revision.focus.clone(),
            premises: revision
                .active_premises
                .iter()
                .filter_map(|id| self.premises.get(id).cloned())
                .collect(),
            source_refs: revision.source_refs.clone(),
        }
    }

    fn add_premise(
        &mut self,
        id: PremiseId,
        kind: PremiseKind,
        text: &str,
        source_refs: impl IntoIterator<Item = EvidenceId>,
    ) -> Result<&SituationRevision, SituationError> {
        if self.premises.contains_key(&id) {
            return Err(SituationError::DuplicatePremise(id));
        }
        self.premises.insert(
            id,
            Premise {
                id,
                kind,
                text: clean_text(text)?,
                source_refs: source_refs.into_iter().collect(),
                status: PremiseStatus::Active,
            },
        );
        self.checkpoint()
    }

    fn checkpoint(&mut self) -> Result<&SituationRevision, SituationError> {
        let active_premises: BTreeSet<_> = self
            .premises
            .values()
            .filter(|p| p.status == PremiseStatus::Active)
            .map(|p| p.id)
            .collect();
        let revision = self.revisions.len() as u64 + 1;
        if self
            .revisions
            .last()
            .is_some_and(|r| r.revision == revision)
        {
            return Err(SituationError::DuplicateRevision);
        }
        let fingerprint = fingerprint(
            &self.focus,
            &active_premises,
            &self.source_refs,
            &self.premises,
        );
        self.revisions.push(SituationRevision {
            revision,
            focus: self.focus.clone(),
            active_premises,
            source_refs: self.source_refs.clone(),
            fingerprint,
        });
        Ok(self.revisions.last().expect("revision just pushed"))
    }
}

fn clean_text(input: &str) -> Result<String, SituationError> {
    let trimmed = input.trim();
    if trimmed.is_empty() || trimmed.len() > 4096 || trimmed.chars().any(char::is_control) {
        return Err(SituationError::EmptyText);
    }
    Ok(trimmed.to_owned())
}

fn fingerprint(
    focus: &str,
    active: &BTreeSet<PremiseId>,
    sources: &BTreeSet<EvidenceId>,
    premises: &BTreeMap<PremiseId, Premise>,
) -> String {
    let mut digest = Sha256::new();
    digest.update(b"niccolo-situation-v1\0");
    digest.update(focus.as_bytes());
    digest.update(b"\0sources\0");
    for id in sources {
        digest.update(id.0.to_be_bytes());
    }
    digest.update(b"\0premises\0");
    for id in active {
        let p = &premises[id];
        digest.update(id.0.to_be_bytes());
        digest.update([match p.kind {
            PremiseKind::UserReported => 1,
            PremiseKind::Hypothetical => 2,
        }]);
        digest.update(p.text.as_bytes());
        digest.update(b"\0");
        for source in &p.source_refs {
            digest.update(source.0.to_be_bytes());
        }
    }
    format!("{:x}", digest.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn material_correction_creates_new_snapshot_without_rewriting_old_one() {
        let mut s = Situation::new("Why did Alice object?", [EvidenceId(10)]).unwrap();
        s.add_user_report(PremiseId(1), "Alice knew on Monday", [EvidenceId(11)])
            .unwrap();
        let before = s.snapshot();
        s.correct(
            PremiseId(1),
            PremiseId(2),
            "Alice anticipated it on Monday but learned the final decision Tuesday",
            [EvidenceId(12)],
        )
        .unwrap();
        let after = s.snapshot();

        assert_ne!(before.revision, after.revision);
        assert_ne!(before.fingerprint, after.fingerprint);
        assert_eq!(before.premises[0].text, "Alice knew on Monday");
        assert_eq!(after.premises.len(), 1);
        assert_eq!(after.premises[0].id, PremiseId(2));
        assert_eq!(
            s.premise(PremiseId(1)).unwrap().status,
            PremiseStatus::CorrectedBy(PremiseId(2))
        );
    }

    #[test]
    fn what_if_is_explicitly_hypothetical_and_never_promoted_by_snapshotting() {
        let mut s = Situation::new("What changes?", []).unwrap();
        s.add_what_if(PremiseId(7), "WHAT IF Roberto never saw the email?", [])
            .unwrap();
        for _ in 0..3 {
            let snap = s.snapshot();
            assert_eq!(snap.premises[0].kind, PremiseKind::Hypothetical);
        }
    }

    #[test]
    fn retraction_removes_current_premise_but_history_remains() {
        let mut s = Situation::new("Understand the meeting", []).unwrap();
        s.add_user_report(PremiseId(3), "The director attended", [])
            .unwrap();
        let before = s.snapshot();
        s.retract(PremiseId(3)).unwrap();
        let after = s.snapshot();
        assert_eq!(before.premises.len(), 1);
        assert!(after.premises.is_empty());
        assert_eq!(
            s.premise(PremiseId(3)).unwrap().status,
            PremiseStatus::Retracted
        );
    }

    #[test]
    fn focus_and_new_source_change_fingerprint_and_revision() {
        let mut s = Situation::new("Original question", [EvidenceId(1)]).unwrap();
        let first = s.snapshot();
        s.set_focus("A narrower question").unwrap();
        let second = s.snapshot();
        s.add_source_ref(EvidenceId(2)).unwrap();
        let third = s.snapshot();
        assert!(first.revision < second.revision && second.revision < third.revision);
        assert_ne!(first.fingerprint, second.fingerprint);
        assert_ne!(second.fingerprint, third.fingerprint);
    }

    #[test]
    fn same_sequence_produces_same_fingerprint() {
        fn build() -> AnalysisSnapshot {
            let mut s = Situation::new("Question", [EvidenceId(9), EvidenceId(2)]).unwrap();
            s.add_user_report(PremiseId(1), "Context", [EvidenceId(4)])
                .unwrap();
            s.add_what_if(PremiseId(2), "Alternative", []).unwrap();
            s.snapshot()
        }
        assert_eq!(build().fingerprint, build().fingerprint);
    }

    #[test]
    fn inactive_premise_cannot_be_corrected_or_retracted_twice() {
        let mut s = Situation::new("Question", []).unwrap();
        s.add_user_report(PremiseId(1), "Context", []).unwrap();
        s.retract(PremiseId(1)).unwrap();
        assert_eq!(
            s.retract(PremiseId(1)),
            Err(SituationError::InactivePremise(PremiseId(1)))
        );
        assert_eq!(
            s.correct(PremiseId(1), PremiseId(2), "replacement", []),
            Err(SituationError::InactivePremise(PremiseId(1)))
        );
    }
}
