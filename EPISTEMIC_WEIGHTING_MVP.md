# Epistemic weights — late-MVP incremental plan

This document separates the following concepts by **type**, origin and semantics. None is an automatic probability of truth.

1. **Evidence provenance:** which immutable source fragment says something, and its documented derivation root. Different roots do NOT establish independent witnesses.
2. **Human confidence:** a named human's 0–100 subjective degree of belief in a **specific claim**, with rationale and timestamp; the number is *not* automatically calibrated.
3. **Operational premise:** a human may explicitly authorize treating an uncertain claim as a working assumption. This never upgrades it into a verified event.
4. **Heuristic evidence balance:** reproducible, inspectable support/contradiction contributions, grouped by documented source origin. These are relative reasoning aids, **not posterior probabilities**.
5. **Investigation priority:** where to spend attention; not to be conflated with confidence, materiality, or truth.
6. **Analysis quality/completeness:** unresolved gaps, challenges and clarifications; not a probability of correctness.

## Ordered milestones

- EPI-001 — Document the above contract and explicitly distinguish numerical confidence from evidence and attention scores.
- EPI-002 — Implement a small deterministic, pure-Rust evidence balance calculator with explainable grouping and tests. Treat missing origin or unknown independence conservatively.
- EPI-003 — Persist append-only **human** confidence about an existing analysis claim, with workspace RLS, rationale and idempotent replay, preserving the actor.
- EPI-004 — Add a deliberately narrow authenticated API to record and inspect this claim-scoped confidence.
- EPI-005 — Expose current claim confidence explicitly to reasoning, *not* as part of frozen historical evidence; preserve the original analysis horizon.
- EPI-006 — Add integration/regression tests, including forged evidence references and replay conflicts.
- EPI-007 — Enable configurable weights and a read-only preview/playground only after the basic mechanism is proven end-to-end.
- EPI-008 — Persist workspace-default and Situation-specific weight policies with append-only author attribution, RLS and effective-policy selection.
- EPI-009 — Make previews inherit configured policies, with explicit, non-persisting trial overrides.
- EPI-010 — Compare baseline and trial evidence-balance scores side by side; clearly state this is NOT a full reasoning replay.
- EPI-011 — Include effective heuristics beside human credence in *current human control* of the model packet; never in frozen evidence.


## Guardrails

- A source's report, the user's belief and the model's inference never collapse into one indistinguishable assertion.
- Absence of counterevidence does not create certainty.
- Don't infer statistical independence from distinct provenance roots.
- An operational premise may guide exploration; mark every dependent conclusion as conditional.
- No numerical scoring can authorize real-world actions or mutate frozen evidence.
- Privacy: do not send source text or confidence records to Cambrian Radiation; sending information to a configured external LLM must remain an explicit provider/data-egress choice.
- No new model-routing complexity, desktop installer work or OS-portability work in this milestone block.

## Illustrative example

User: “I am attaching a PDF and subjectively estimate a 90% chance that André will leave for a competitor within months.”

Store: **human-declared confidence 90**, scoped to the precise claim, with the user's rationale. Record what the PDF actually says separately. A 90 input does **not** mean a model independently measured 90% certainty.

## Validation

A successfully written GitHub commit is not a passing Rust/SQL test. The acceptance gate is cargo fmt, strict clippy, PostgreSQL migration clean-install, Rust integration suite and an end-to-end API exercise. If a toolchain is unavailable, report that explicitly and keep experimental changes out of main until validated.

## Next bounded milestones (not implemented by EPI-001…011)

- EPI-012 — At document import / Situation contribution time, support a human's explicitly scoped numerical judgment before any model-generated claim exists. Keep that judgment separate from document contents and snapshot it at analysis start.
- EPI-013 — Ground every scored evidence contribution in an authorized, examined evidence record and an actual claim-support link; eliminate caller-supplied hypothetical origin IDs from production scoring.
- EPI-014 — Resolve multi-parent provenance roots and overlapping derivation components conservatively; do not confuse distinct roots with statistically independent witnesses.
- EPI-015 — Record per-evaluation policy IDs, source graph revision, weight arithmetic and algorithm version to make a decision trace auditable.
- EPI-016 — Read-only, isolated Reasoning Playground: replay from frozen historical inputs and compare results of full model runs, clearly distinguishing stochastic variability from configuration effects.
- EPI-017 — Provide graphical controls and documentation for workspace vs Situation policy, confidence provenance and preview comparisons, without making advanced controls compulsory for novice users.

**Status boundary:** EPI-001…011 supply a typed and explainable foundation, not a full probabilistic belief-revision engine. In particular, a human's 90% belief must not be silently treated as a model-calibrated posterior.
