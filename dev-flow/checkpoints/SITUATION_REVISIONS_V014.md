# Niccolò v0.14 — bounded Situation revisions

This experiment implements the approved collaborative-reasoning requirement that a bounded
Situation preserve enough state for visible reanalysis after user additions, corrections,
retractions, focus changes, or explicit WHAT-IF premises.

## Implemented boundary

- `Situation` starts with a current conversational focus and a bounded set of source references.
- User-reported context and WHAT-IF premises are distinct typed inputs.
- WHAT-IF remains hypothetical across snapshots; snapshotting never turns it into evidence.
- Corrections create a replacement premise while preserving the corrected record.
- Retractions remove a premise from the current active set without deleting history.
- Every state-changing operation creates an immutable revision snapshot.
- Every revision has a stable SHA-256 fingerprint of the exact focus, active premise content/kind,
  premise source references, and Situation-level source references.
- `AnalysisSnapshot` gives a reasoning invocation the exact revision/fingerprint it must cite later;
  a later correction cannot retroactively alter that snapshot.

## Deliberate non-goals

This milestone does not implement the still-unapproved `MVP-HYP-001` semantics for remembering
rejected/deprioritized machine hypotheses. It does not assign probabilities, infer truth from user
claims, infer mental states, execute LLM calls, or claim that a source reference authenticates the
premise. Persistence and HTTP routes are separate later milestones; this core is intentionally
unit-testable without PostgreSQL so its state-transition semantics can be hardened first.
