# Niccolò / Duhme — experimental Rust core (v0.5)

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
cargo test --offline
cargo run --offline --bin niccolo-demo
cargo run --offline --bin niccolo-benchmark-demo
```

## Run SQL POC

Apply `sql/0001_init.sql` and `sql/0002_benchmark_evolution.sql` in a disposable PostgreSQL database; then run `scripts/pg-benchmark-smoke.sql`. It tests tenant-isolated benchmark policy, results and improvement tickets, and rolls back all seeded records.

## Deliberate non-goals and known limitations

- Does not include an HTTP service, LLM adapters, frontend, real WhatsApp/.eml ingestion, object storage or full temporal/permission contracts yet. SQLx now loads the provenance graph from a restricted PostgreSQL connection; other runtime services remain future work.
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

## v0.4 — multi-parent provenance DAG and rule exploration

The new [`SourceGraph`](src/provenance_dag.rs) reports roots of evidence across **multiple derivation parents** (including shared origins), fails closed on missing/withdrawn/cyclic ancestry, and avoids recursive stack use.

The [v0.4 rule search](DAG_EVOLUTION_V004.md) enumerates 12 constrained rule variants, generates independent Rust executables, checks 9 training and 24 holdout cases, and performs concurrency and latency measurements only for qualifying candidates. A 256-case seeded cross-check compares the winning strategy with the library implementation.

```bash
bash scripts/run-dag-evolution.sh 8 23
```

**No LLM code generation, live frontier benchmarking, autonomous deployment, arbitrary user code execution, or real-world belief inference has been implemented.**

## v0.5 — real SQLx + PostgreSQL graph adapter

The [PostgreSQL adapter](src/postgres_repository.rs) binds a workspace to a transaction-local RLS scope and reads nodes and derivation edges in one read-only, repeatable-read snapshot. It explicitly rejects `SUPERUSER`/`BYPASSRLS` logins, maps UUIDs safely into the tested multi-parent graph, and fails closed on withdrawals/cycles/over-budget graphs. See [POSTGRES_SQLX_V005.md](POSTGRES_SQLX_V005.md) for setup, integration test commands, and the remaining authorization risks. PostgreSQL integration tests require an actual disposable database; without the `pg-integration` feature, the original 57 unit tests remain runnable offline.
