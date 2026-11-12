# Niccolò MVP 1.0 — runtime budgets, user-owned history, and explainable exports

## Product invariants (approved direction; not all implemented)

- **The Niccolò PostgreSQL database is the single authoritative and complete operational state** for Situations, inputs, claims, investigations, judgments, checkpoints, policies, and publication outcomes. "Authoritative state" is not a claim that any assertion in the database is factually true.
- **The user's own Git repository is a portable historical publication destination**, not a replacement database and not an epistemic dependence graph. The user connects and authorizes their own GitHub repository early; work published there remains under that user's Git account after leaving Niccolò, subject to the Git provider's own terms and account controls. No promise of eternal retention or unlimited availability.
- Git export is **selective**: the user-supplied information, investigated material authorized for release, relevant conclusions, explanations, and progress. Never a routine PostgreSQL dump, secret material, hidden prompts, unapproved private source contents, or the entire internal operational schema.
- **Python plus CSV and/or simple data files** is the ONLY explainable presentation-export format committed to MVP 1.0. The deliverable should allow a third party to see *what was investigated*, *what was said*, *which deterministic procedures were actually applied*, *what the result was*, and *what remains uncertain*. Only reproduce calculations that are actually reproducible with the exported inputs. Distinguish recorded model interpretation from deterministic calculation.
- External editing, Python extensions, local simulations, Git-based analysis, and new third-party reasoning tools are **the recipient's responsibility**. Niccolò does not have to support them. **Reingestion is explicitly out of scope** for MVP 1.0, as are Excel/VBA, GitLab and additional export adapters.
- These constraints are intentionally narrower than a general-purpose adapter framework. Implement the simplest coherent vertical slice rather than an extensibility platform.

## Git is historical publication, not the reasoning DAG

The UI communicates plain states; Git details are optional:

| Product state | Illustrative Git ref | Meaning |
| --- | --- | --- |
| Received | `intake` | Input arrived. No analysis, validity or reasoning milestone implied. |
| In progress | `work` | Some work may be provisional, partial, budget-limited or unreviewed. |
| Milestone published | `main` with optional tags | A significant checkpoint was reached, not necessarily a verified factual conclusion. |

The exact branch topology remains an implementation choice. No requirement to create one branch per investigation or merge every intake event. Repository refs and commits track published snapshots, not dependence edges or truth values. A milestone may be a timed partial result, human-acknowledged review, first completed pass, or final synthesis; the reason for declaring it must be recorded.

**Durability and idempotency:** PostgreSQL records intended export, target repository/ref, payload digest and publication outcome. Network push is not a database transaction. Verify remote commit/ref after pushing; retries must not duplicate content or silently lose an accepted input. Failure to publish to Git must not cause an investigation's PostgreSQL state to vanish. All publishing is gated by explicit account/repository permissions, release selection and provider authorization. Never claim data is preserved remotely before verifying the destination.

**Offboarding:** a user-owned Git repository persists independently of Niccolò as long as the user's account/provider retains it. Any managed export-retention period (for example 180 days) would apply only to a Niccolò-controlled service, not remove or guarantee user-owned Git data. Do not add managed retention infrastructure for MVP without need.

## Explainable presentation bundle: first implementation slice

The initial implementation can project a **published, workspace-authorized analysis** to a minimal, deterministic, inspectable package:

- `data/analysis.json`: rich snapshot with explicit result, coverage note, claim text and recorded provenance *references*.
- `data/*.csv`: recipient-friendly tables of available claims, claim/evidence relations, human context relations, review notes and clarifications.
- `inspect.py`: standard-library Python script to inspect and explain those **recorded relationships and counts** without a Niccolò server.
- `README.md`: human-oriented interpretation, source coverage and caveats.
- `SHA256SUMS.json`: integrity information for package file bytes; **not a digital signature or authentication of the author**.

A first package generated from the existing immutable analysis bundle **does not yet include complete original source bytes, document excerpt text, human confidence trace, provenance clustering evaluations, all intermediary reasoning decisions, or the complete operational event log**. It must disclose those omissions prominently. It must not invent links or reconstruct LLM output that was never recorded.

A bounded examined-source slice is now implemented: the ordinary portable package remains source-ID-only, while a separate write-authorized request can include only source fragment text that was explicitly recorded as examined by that immutable analysis. That sensitive variant is capped at 128 fragments and 512 KiB of excerpt text, excludes original uploaded document bytes, labels the excerpts as non-truth, and still performs no remote publication. A separate read-authorized scope preview reports only the recorded examined-fragment count, excerpt byte count and export bounds before the sensitive action is enabled; it does not expose source text. Successful sensitive exports write a workspace-scoped immutable authorization event containing the actor, bounded scope and SHA-256 of the exact package; retries with the same event ID must match that recorded scope and digest.

The portable package now also exports the recorded epistemic-policy history relevant to the Situation plus human credence records attached to the selected analysis. Those records stay explicitly typed as human/configuration controls: human percentages are not evidence or calibrated truth probabilities, and policy history does not prove that a particular policy generated the published answer.

An explicit write-authorized grounded claim evaluation can now be recorded as an immutable, retry-safe artifact. The record freezes the selected policy, human-declared confidence (if any), declared evidence strengths, database-validated grounded result, source-graph revision and SHA-256 of the deterministic result. The portable package exports those stored artifacts verbatim plus a compact CSV summary. A stored score remains a heuristic balance: it is not a probability of truth, proof, documentary evidence or a mutation of the published analysis.

Subsequent bounded work: expose the record action cleanly in the human review UI, make recorded-vs-live weighting visually unmistakable, and implement explicit publication to the **user-owned GitHub repository** with retry and publication verification.

## Adaptive execution + bounded investigation time

A Niccolò investigation is an **anytime computation**. It should yield useful provisional work when possible, and improve it while time, CPU, memory, GPU availability, token/cost budgets and operator policy allow.

- Explicit budgets: wall-clock **deadline**, memory/CPU/GPU footprint and fair-share priorities, invocation/token/API monetary budget, and user preference for speed/depth. Time can be a hard deadline or a desired target.
- **Degraded epistemic quality is allowed and often preferable to a never-arriving answer.** A deadline/pressure-driven downgrade must be visible: what was skipped, which model/reasoning strategy was used, evidence examined, outstanding contradictions, why work ended, and whether output is provisional.
- Do **not** relabel a partial investigation as complete; do not confuse computed score, subjective confidence, proof or calibration.
- Resource modes: `self_managed`, `externally_managed` and, if worthwhile, `hybrid`. Even externally managed processes should observe their own *effective* quota/memory limits so that OOM is not a surprise. Do not assume host total memory or host CPU count equals the process entitlement.
- Observe Linux first; design adapters for Windows/macOS without promising functional low-level support now. Use native cgroup/OS limits where necessary, layered on a cross-platform telemetry interface. A management policy never bypasses host/container quotas.
- Control priority/concurrency/queue admission/cache or model residency with hysteresis, safe minimums and backpressure. Allow reserved synthesis time, and record interrupted or omitted work when the deadline arrives.
- A speculative information-value-per-second heuristic may help scheduling later, but it is **not a calibrated estimate of epistemic quality**.

## Bounded implementation sequence

| Milestone | Purpose | Status |
| --- | --- | --- |
| REX-001 | Record portable user-owned historical Git / Python+data MVP contract | This specification |
| REX-002 | Build deterministic Python+CSV/JSON explanation package from real published analysis with truthful omissions | Verified first slice (PR #22) |
| REX-003 | Authorized extraction of select investigated inputs / scoring provenance | Verified bounded slices: examined source text export, Situation policy history, analysis human credence, and immutable explicitly recorded grounded-score artifacts; original uploaded document bytes remain omitted |
| GIT-001 | Connect user-owned GitHub repository, explicit export scope and permissions | Not implemented |
| GIT-002 | Publish intake and provisional checkpoints with remote confirmation and retries | Not implemented |
| GIT-003 | Publish meaningful milestones, keeping received/provisional/published visibly distinct | Not implemented |
| ARM-001 | Observe effective Linux process/container resources; portable adapter boundary | Verified Linux first slice (PR #26) |
| ARM-002 | Self-managed/delegated modes; bounded budgets and visible degradation | Partial verified slice: self/external/hybrid policy advice, resource-aware atomic worker admission and operation-wide invocation/token/deadline enforcement; HTTP-spawned execution and quality-degradation scheduling are not yet adaptive |
| ARM-003 | Anytime partial-result checkpoints, bounded deadline and finalization time | Partial verified slice: immutable per-step checkpoints, explicit human/budget boundaries, provisional terminal result on model-budget exhaustion, and configurable deadline reserve for deterministic finalization; broader adaptive scheduling remains open |

The operation-wide invocation/token/deadline accounting and Linux effective-resource observer are verified prerequisites. Agentic execution now records immutable, digest-checked progress checkpoints after completed steps and exposes them through a bounded read API. When the next model call cannot start because the invocation/token/deadline budget is exhausted—or the configured deadline enters its deterministic finalization reserve—the operation can terminate successfully with an explicitly **provisional partial result** that records the stop reason, next unexecuted step, current quality report, budget accounting and handoff digest. None of those artifacts establishes truth, completeness or permission to act. The worker library also has an atomic workspace-wide adaptive admission ceiling: self-managed/hybrid policy can cap competing execution leases from observed capacity, while externally managed mode leaves concurrency to the operator. HTTP-spawned admission is still not dynamically scheduled from effective resource pressure, and the current checkpoint policy is a bounded first slice rather than a general information-value scheduler.

### Security and evidence rules

1. Never commit secrets or authentication tokens to the user's Git repository.
2. No automatic remote upload of sensitive personal documents without prior express authorization and visibility into what will be exported. Git commit history is difficult to erase completely after publication.
3. Provide a preview of export scope; require permission appropriate to the repository (prefer fine-grained GitHub credentials and private repository). Do not silently change repository visibility.
4. Version algorithms and input horizons; disclose any absent inputs, unstable third-party model behavior, heuristic approximations and policy-driven omissions.
5. Do not claim a source, human assertion or LLM conclusion is independently corroborated merely because it has a Git commit or a CSV record.

**Guiding principle:** user time and ownership matter; the best *feasible, disclosed* investigation is preferable to an indefinitely delayed idealized result.