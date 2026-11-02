# Niccolò / Duhme — offline evidence + benchmarking POC 0.2

A deliberately narrow, **runnable** Rust + PostgreSQL proof of concept. It is not a release of Duhme and does not claim to implement the full v0180 contracts.

## Completed experimental slice

- Rust 1.99.1 compilation and offline Cargo tests without external crates.
- Recursively attributed statements: an event, what someone *says*, and what someone *believes* are different expressions.
- Trace documented transmission/derivation origin of testimonies; three copies of one memo are not three independent observations.
- Distinct roots are **not** treated as proof of statistical independence.
- A lost or access-revoked ancestor fails closed; a copied document cannot silently retain authority when its origin was withdrawn.
- A proposed investigatory question remains explicitly **not evidence**.
- PostgreSQL 17 schema with workspace-scoped composite FKs, forced row-level security and basic recorded evidence, derivation, propositions, and investigative questions.

## New capability: frontier comparisons and continuous algorithm evolution

Read **[COMPETITIVE_EVALUATION.md](COMPETITIVE_EVALUATION.md)** for the mandatory blind frontier-agent benchmark, opportunistic cheap-vs-frontier comparison, quota/permission gate, independent adjudication and reproducible improvement experiments.

The current implementation enforces parts of the boundary and provides synthetic/offline tests; it **does not call frontier models or establish competitive superiority**.

## Run Rust POC

```bash
export PATH=/mnt/data/niccolo-tools/rust/bin:$PATH
cd /mnt/data/niccolo-poc
cargo test --offline
cargo run --offline --bin niccolo-demo
cargo run --offline --bin niccolo-benchmark-demo
```

## Run SQL POC

Apply `sql/0001_init.sql` and `sql/0002_benchmark_evolution.sql` in a disposable PostgreSQL database; then run `scripts/pg-benchmark-smoke.sql`. It tests tenant-isolated benchmark policy, results and improvement tickets, and rolls back all seeded records.

## Deliberate non-goals and known limitations

- Does not include Axum, Tokio, SQLx, LLM adapters, frontend, real WhatsApp/.eml ingestion, object storage or full temporal/permission contracts yet. Cargo crates must be transferred/cached to build those offline.
- Root-lineage grouping uses one known parent per source item, not the complete multi-parent lineage and source-independence model needed for production.
- Recursive expression depth is representational only; human beliefs are not proven by serialization.
- No security model is complete just because RLS exists. PostgreSQL roles, authentication, SQL access, export/erasure workflows and trusted authorization binding are still required.
- All demo data is synthetic; no user conversations or personal records are involved.
- Competitive benchmarking against actual frontier models, background model calls, blind process isolation and atomic budget reservations still need separate implementation and authorization.

## v0.3 — actual generated Rust candidates + independent-process benchmark

See [EVOLUTION_ENGINE.md](EVOLUTION_ENGINE.md). This adds reproducible code generation from a fixed trusted template, independent compilation, adversarial training/holdout tests, actual subprocess concurrency and a TSV report. No arbitrary code execution, no online model calls and no deployment are included.

```bash
bash scripts/run-evolution.sh 8 31
```
