# Niccolò / Duhme — experimental Rust core (v0.9)

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

- Does not include public-ready network ingress, LLM adapters, a connected frontend, production-grade email/WhatsApp parsers, object storage or full temporal/permission contracts. The loopback-only HTTP API and bounded plain-text importers are experimental, not production services.
- Multi-parent graph traversal is implemented, but neither distinct graph roots nor separate uploaded documents establish independent eyewitnesses or truthfulness.
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

## v0.6 — source import, revisions and byte-accurate provenance

[Rust parsers](src/source_import.rs) accept bounded UTF-8 plain-text `.eml` and two WhatsApp export header styles, keeping exact source-byte offsets, unverified speaker labels and SHA-256. [SQLx ingestion](src/postgres_ingest.rs) persists metadata + attributed evidence atomically, rejects conflicting retries, and reuses the same records on identical retries. A source document's original bytes must be retained by a separate authorized content store.

[SQL migration 0003](sql/0003_sources_and_revisions.sql) adds source fragments and graph-revision triggers. [SQL migration 0004](sql/0004_source_integrity.sql) enforces document bounds/overlap and protects source metadata. The graph distinguishes *shared original reports* from *shared uploaded source files*. Neither conclusion authenticates identities or testimonies.

**Reproducible PostgreSQL integration, limitations, threat model and test evidence:** [SOURCES_SQLX_V006.md](SOURCES_SQLX_V006.md). All 89 tests pass against a fresh PostgreSQL 17.11 database; additional deterministic parser mutations and concurrent ingestion tests are included.

```bash
bash scripts/run-v006-validation.sh
```

The command requires an already-migrated disposable database and two explicit environment variables, `NICCOLO_PG_ADMIN_URL` and `NICCOLO_PG_SERVICE_URL`. The restricted login cannot bypass RLS; an upstream trusted identity boundary is still **missing**, so do not expose this experimental backend directly to clients.

## v0.7 — workspace authentication, API and paginated source metadata

The Rust SQLx backend now exposes a **loopback-only** Axum HTTP interface with database-verified bearer-token permissions. No HTTP request may set the RLS workspace scope merely by asserting its workspace UUID: the token must have a live matching membership with the requested read/write permission. Operators can provision, atomically rotate or revoke opaque tokens through a separate trusted CLI. The app DB role has execute-only access to the auth decision, not the private token/membership tables. The source catalog provides bounded UUID-keyset pagination; assessment never promotes provenance to factual truth.

This is **still a POC**, not a production multitenant security implementation. In particular, the existing mutable workspace GUC remains dangerous under SQL injection or stolen DB credentials; revocation can race an in-flight authorized request. See [HTTP_AUTH_V007.md](HTTP_AUTH_V007.md) for the API, schema migration `sql/0005_authz.sql`, the operator instructions, exact threat model, and full tests.

```bash
bash scripts/run-v007-validation.sh   # requires disposable migrated test DB and two env URLs
cargo run --offline --bin niccolo-api   # 127.0.0.1:38080, NICCOLO_API_PORT to change
```

## v0.8–v0.9 — explicit human identity, feedback, and source pointers

[HUMAN_STEERING_V009.md](HUMAN_STEERING_V009.md) documents the human-confirmed, document-specific identity correction history (v0.8), distinct user feedback (`rejected` vs `insufficient_evidence` vs `unknown`), actor-specific positions that preserve disagreement, and human-authored investigative questions linked to original evidence via typed references (v0.9). New migrations `0006..0008`, SQLx modules and loopback-only API routes are backed by real PostgreSQL integration tests. No consensus, truth, confidence, identity or testimonial independence is inferred.

```bash
bash scripts/run-v009-validation.sh
PSQL_BIN=psql bash scripts/run-v009-loopback-smoke.sh
```

**Known authorization gap:** the HTTP token check and the data transaction are separate, and the workspace GUC remains mutable by SQL running as the service. Do not expose this prototype to untrusted users; the controls do not defend against SQL injection or stolen database credentials.
