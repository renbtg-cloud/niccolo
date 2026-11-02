# Niccolò / Duhme — experimental Rust core (v0.6)

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

- Does not include an HTTP service, LLM adapters, connected frontend, production-grade email/WhatsApp parsers, object storage or full temporal/permission contracts. Bounded text importers in v0.6 are experimental, not complete handling of real-world exports.
- Multi-parent graph traversal exists; neither distinct roots nor separate files establish independent eyewitnesses or truthfulness.
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

The [PostgreSQL adapter](src/postgres_repository.rs) binds a workspace to a transaction-local RLS scope and reads nodes and derivation edges in one read-only, repeatable-read snapshot. It explicitly rejects `SUPERUSER`/`BYPASSRLS` logins, maps UUIDs safely into the tested multi-parent graph, and fails closed on withdrawals/cycles/over-budget graphs. See [POSTGRES_SQLX_V005.md](POSTGRES_SQLX_V005.md) for setup, integration test commands, and the remaining authorization risks. PostgreSQL integration tests require an actual disposable database; without `pg-integration`, the Rust unit tests remain runnable offline.


## v0.6 — bounded import, revisions and byte-accurate provenance

[Rust parsers](src/source_import.rs) extract unverified textual attributions from bounded UTF-8 plain-text `.eml` and two WhatsApp export header styles, preserving exact byte offsets and SHA-256. [SQLx ingestion](src/postgres_ingest.rs) writes evidence and source metadata atomically with idempotent retries. Original bytes are retained separately by an authorized source store, not inside `source_documents`; fragments live in PostgreSQL `evidence.content`.

[Migrations 0003](sql/0003_sources_and_revisions.sql) and [0004](sql/0004_source_integrity.sql) add source fragments, graph revisions, byte bounds, overlap checks and immutable source metadata. Graph assessment distinguishes *common provenance roots* from *shared uploaded source documents* without treating either as proof of independence or coordination.

See [v0.6 technical documentation](SOURCES_SQLX_V006.md) for installation, security limitations, a fresh PostgreSQL 17 test procedure, and the full test matrix. **0/89 failures, 89/89 tests passed** on the development Linux VM and again on an independently initialized test database.

```bash
bash scripts/run-v006-validation.sh
```

This command requires `NICCOLO_PG_ADMIN_URL` and `NICCOLO_PG_SERVICE_URL` on a previously migrated, disposable database. **Authentication and trusted binding of workspace identity are still missing**; never expose this experimental SQLx adapter directly as a public service.
