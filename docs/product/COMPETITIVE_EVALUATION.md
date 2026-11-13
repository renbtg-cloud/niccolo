# Niccolò / Duhme — Competitive Evaluation and Algorithm Evolution (v0.2 POC)

**Architectural status:** mandatory product capability, *not* a proven competitive advantage.

**Implementation status, November 13, 2026:** blind-input, independent-scoring and improvement-ticket contracts exist and are tested. PostgreSQL now also provides a transactional reservation ledger for optional paid probes: concurrent reservations serialize on the workspace/account policy row, open estimates contribute to reserved spend, and settlement/release moves or returns that capacity exactly once. **No benchmark frontier-model call, genuine frontier comparison or independent judging has occurred.**

## Two separate evaluation modes

### A. Opportunistic cheap-model versus frontier-model checks

- Occasionally sample tasks first routed to a cheaper model and run an **additional** evaluation against an eligible frontier model, whether or not that frontier model is the user's default choice.
- Only if the account/workspace policy enables background evaluations, the provider is permitted, the model is not prohibited, the specific raw-data egress is authorized, and spending remains **well below** the user's explicit cost cap.
- The strictness is a user-configured utilization threshold. **The POC uses 30% as an illustrative configuration, not a product-wide mandated percentage.** Do not charge autonomously when the cap is missing.
- Make atomic budget reservations in PostgreSQL before any remote request. The implemented reservation ledger serializes concurrent reservations, binds each reservation to one recorded benchmark run, counts existing reserved + spent amounts against the configured cap/threshold, and records settlement/release idempotently. The pure Rust preflight remains advisory; the PostgreSQL reservation is the concurrency boundary. Retry overhead and multi-call/tool estimates still need to be included in the amount reserved by the eventual runner.
- Compare factuality, epistemic nuance, calibration and question-generation—not merely verbosity or judge preference. Admit that sometimes a cheap model performs equally well or better. Record cases where a frontier call does **not** justify the incremental cost.
- Make all extra evaluations auditable and user-controllable; do not send personal communications to an unapproved provider merely because the user hasn't prohibited that specific model name.

### B. Niccolò's complete investigation versus a capable blind frontier agent (CENTRAL)

Two contestants receive the **same authorized original evidence universe and investigation objective**. Their internal machinery differs by design:

**Niccolò** may use its original documents, persistent human-context representation, attributed/nested-belief graph, historical source-lineage maps, counterhypotheses, learned heuristics, and specialized agents.

**Blind frontier agent** may access original messages/documents and original metadata (sender, timestamp, recipient, original source, etc.) but must NOT receive Niccolò-generated summaries, inference graphs, provenance *deduced* from records, human-profile features, candidate suspects, proposed inquiries, prior Niccolò answers, self-evaluations or internal reasoning traces. The agent is encouraged to infer its **own** provenance and hypotheses from originals.

The frontier opponent must be substantive: frontier model, corpus search, original retrieval, metadata inspection, private scratchpad, multiple investigation steps, reflection/replanning, and enough context/budget to solve difficult cases. Our offline contract defaults to an illustrative **12-step** agent recipe with 4 tool categories. Actual comparisons should report both (i) comparable time/compute cost and (ii) strongest reasonable frontier budget, without concealing resource differences.

### Blindness and leakage controls

- Use independent input assemblers with an explicit typed `RawCase` / `BlindFrontierRequest` boundary, and separate process/context for opponent execution.
- Grant the adversary read-only access to the authorized originals and original metadata only. Never attach Niccolò's graph/embeddings or tool-call traces to its prompt or tool result.
- Prevent accidental feedback: don't provide the baseline with benchmark expected answers, unpublished grading rubrics with answers, or the other contestant's output.
- Test sentinels to catch known contamination paths, log exact source IDs/input manifests and model/tool versions, and conduct periodic independent leakage audits.
- The current Rust type boundary is useful but **does not by itself guarantee isolation**. Actual agent tool adapters and outbound network logs require additional checks.

## Independent adjudication

Evaluate by dimensions: (1) factual correctness, (2) nested-belief/attribution correctness, (3) epistemic discipline (not promoting hypotheses/rumors to facts), (4) source traceability, (5) useful unexpected questions, (6) calibrated uncertainty. Also **report separately** token/monetary cost, wall time, evidence access, failures, and repeatability.

- Start with synthetic cases with known ground truth. Then use consented, privacy-respecting hard cases with independent evidence and expert review.
- Compare under multiple seeds/paraphrases, adversarial distractors, time changes, missing evidence and contradictory testimonies. Account for stochastic variation and confidence intervals before generalizing.
- Keep held-out cases inaccessible to development and to both contestants. Resist contamination by later training/tuning.
- Prefer human/domain-expert or genuinely independent and documented adjudication. Self-evaluation from a contestant cannot establish it won. A third LLM judge, where used, needs calibration and bias checks.
- There must be no winner declared when no reliable truth/reference or independent adjudication exists.
- Do not collapse different metrics into a cherry-picked aggregate. Publishing an average requires the full per-dimension results and unresolved failures.
- Track model families/build IDs and dates; frontier systems change over time. A single past win is not evidence of permanent superiority.

## Continuous improvement loop

1. **Capture a failure or uncertainty**: cost-gated pairwise assessment, independent review and source IDs, with a reproducible run configuration.
2. **Diagnose** which layer is deficient: parsing, identity resolution, provenance, temporal reasoning, nested attribution, hypothesis search, question selection, LLM selection, or evaluator quality.
3. **Generate a falsifiable candidate**, ideally with a small isolated implementation, regression test, adversarial counterexample and measurable success criteria.
4. **Execute POCs**, including before/after code, input fixture, reproducible commands, independent adjudications, per-dimension scores and cost comparisons. An appealing narrative or frontier model's confident answer is not proof.
5. **Retest on held-out cases** and previously solved cases to detect regressions, confounds, overfitting and metric hacking.
6. **Escalate to Cambrian Radiation** with the executable POC, risk/benefit, observed failures, counterarguments, cost and recommended change; or stage a candidate under separately authorized autonomous self-improvement policies. Core production algorithms are NOT silently rewritten or deployed based on one score.
7. **Monitor the competitive frontier** continuously as models improve. This is an ongoing empirical objective, not an unconditional marketing claim.

## Reproducible current counterexample

The executable `niccolo-benchmark-demo` compares a naive rule that counts three copied reports as three corroborations against the real Rust `EvidenceIndex` lineage algorithm:

- Naive witness count: **3**.
- Documented transmission roots: **1** (all three from one memo).
- Established statistical independence: **false**.

This is a genuine **algorithmic POC** showing why origin-aware aggregation beats naive counting in this narrow case. It is **not** a frontier-LLM comparison. The Rust unit tests enforce revocation, cycles and distinct roots not implying statistical independence.

The unit tests also exercise a **synthetic invented scoring example** (Niccolò 71.00 / frontier 82.00) to verify the scoring and improvement-ticket logic. Those numbers are fixtures, **not measured performance**.

## Data model in the POC

- `niccolo.evaluation_policies`: defaults disabled and budget policy per workspace/account.
- `niccolo.benchmark_runs`: mode, allowed original source IDs, blind input/tool recipe, provider/model, audit and estimated/actual costs.
- `niccolo.benchmark_scores`: per-dimension independent judgments.
- `niccolo.improvement_experiments`: proposal, reproduction recipe, algorithmic hypothesis and statuses.
- `niccolo.benchmark_budget_reservations`: immutable reservation identity plus bounded RESERVED → SETTLED/RELEASED accounting, tied to one benchmark run and one workspace/account policy.
- All are workspace-scoped with forced RLS. Integration tests exercise reservation replay, settlement/release and concurrent overspend prevention on PostgreSQL 17.

**Remaining blockers before real frontier calls:** authorized provider API keys, explicit user/workspace data-egress consent, an isolated baseline agent implementation, genuine model output capture, independent grading and a held-out corpus. The reservation ledger removes the earlier concurrent-budget-reservation blocker; it does **not** itself authorize provider traffic or prove that estimated cost equals provider billing. None of the remaining prerequisites is secretly assumed from an attachment or a general product ambition.

## Quick run

```bash
export PATH=/mnt/data/niccolo-tools/rust/bin:$PATH
cd /mnt/data/niccolo-poc
cargo test --offline
cargo clippy --offline --all-targets -- -D warnings
cargo run --offline --bin niccolo-benchmark-demo
```

The original benchmark smoke still exercises the offline `0001` + `0002` POC. The transactional reservation path lives in `sql/0114_benchmark_budget_reservations.sql` and is verified by the full migration chain plus `tests/benchmark_budget_integration.rs`; it depends on the later authorization/RLS schema and therefore should not be validated by applying `0114` in isolation.
