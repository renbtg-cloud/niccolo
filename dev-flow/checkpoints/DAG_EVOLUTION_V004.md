# v0.4 — provenance DAG and constrained reasoning-rule search

This is an **offline Rust proof of concept**, not a production Niccolò release.

## What is genuinely new

- A `SourceGraph` library (`src/provenance_dag.rs`) supports **multiple derivation parents** for each evidence record, with iterative traversal so long chains do not exhaust the call stack.
- It exposes **roots per report** and **shared origins across report pairs**, and fails closed on inaccessible/withdrawn, absent or cyclic ancestry. The graph does not infer that witnesses coordinated, that a statement is true, or that distinct roots are statistically independent.
- `RuleSpec` (`src/dag_evolution.rs`) is a tiny **typed domain-specific intermediate representation** with three choices for traversal depth, two missing-evidence policies, and two withdrawal policies. The harness enumerates all 12 combinations, generates standalone Rust source **only from our trusted fixed template**, compiles separate Linux binaries, runs synthetic training and holdout suites, performs process-level benchmarks and concurrency checks, and produces a tabular report.
- The source code of candidates contains **no fixture oracle**; only the evidence graph and observations are passed to candidate stdin. An ID check and an exact-input check detect duplicated train/holdout examples.
- A seeded **256-case graph cross-check** runs the strongest candidate against the independent library implementation. This is a consistency test, not an independent scientific ground truth.

## Reproduce

Requires Rust stable 1.99.1 or newer, a Linux compiler toolchain, and no third-party Cargo crates.

```bash
cargo test --offline
cargo clippy --offline --all-targets -- -D warnings
bash scripts/run-dag-evolution.sh 8 23
```

The script checks formatting, runs tests, checks Clippy and runs a real compiled-binary experiment. It writes `reports/dag-evolution-v004.tsv` (wall-clock timings on the machine where it runs).

For comparison, the original v0.3 experiment remains available:

```bash
bash scripts/run-evolution.sh 8 31
```

## Results on the development Linux VM

- **Rust tests**: 0/57 failed, 57/57 passed.
- **Constrained program search**: 12 candidate configurations compiled and executed; 11/12 rejected by correctness gates; 1/12 eligible **FOR REVIEW ONLY**.
- **Eligible candidate**: full transitive traversal + fail closed on missing ancestors + fail closed on withdrawn ancestors; 0/9 training failures, 0/24 holdout failures, 0/8 concurrent process failures.
- The 256-case deterministic differential check passed (0/256 mismatches). Note that these tests reuse the same conceptual origin-traversal semantics as the runtime library.
- **No candidate promoted to production.** Wall-clock latency includes process startup and IPC, not only algorithm execution.

## Limits and threats to validity

- Fixtures are small, hand-authored synthetic evidence graphs. The holdout is in the repository and is **not** a genuinely secret independent evaluation set. We cannot claim this is performance on real human interactions.
- The v0.3 library's older `EvidenceIndex` still covers the single-parent experiment; the new `SourceGraph` is a separate proposed core module. PostgreSQL already has an `evidence_derivation` multi-parent table, but the Rust module is **not connected to PostgreSQL yet**.
- The typed rule grammar is deliberately tiny. Enumerating 12 configurations is **not autonomous free-form code generation**, general agentic programming, or model-assisted discovery.
- The generated programs are trusted templates. The timeouts in `run_candidate` are **not a security sandbox**; user-supplied or LLM-generated arbitrary code must not be executed through this path.
- No real external LLM/frontier calls, production deployment, customer data, live authorization policies or network access. The independent frontier baseline remains a future implementation.
- Source roots are documented provenance boundaries, not necessarily independent humans or distinct original facts. The system does not read intentions out of source graphs.

## Next implementation slice

Join `SourceGraph` to workspace-scoped PostgreSQL records with a read-only repository adapter and explicit authorized workspace context. After that, add original-source ingestion with evidence-preserving references (e.g., `.eml` and exported messages). User-defined executable reasoning packs require a real sandbox before accepting untrusted code.
