# Niccolò / Duhme — experimental Rust core (v0.17)

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

## v0.11 — operational readiness and response privacy

The loopback HTTP API now sets `Cache-Control: no-store`, `X-Content-Type-Options: nosniff`,
`Referrer-Policy: no-referrer`, and `X-Frame-Options: DENY` for **every** response,
including framework-generated 404/405/413 responses. This is defense in depth,
not a replacement for TLS, reverse-proxy controls or a full security review.

`/healthz` is a liveness check only; it does not touch the database.
`/readyz` now reports HTTP 200 only when the database is connected through
an unprivileged role with `niccolo_api` membership, and the workspace/source
schema and restrictive bearer-RLS authorization function are installed.
An administrator connection or legacy unrestricted service role returns 503,
even if a plain `SELECT 1` would succeed.

The `tests/operational_boundary_integration.rs` integration tests exercise
these three cases against real PostgreSQL with separate restricted, legacy,
and privileged connections. Readiness does not prove the entire SQL schema,
gateway or network edge is secure. Never expose the prototype directly to
untrusted networks. See the v0.10 migration for restrictive row-level security.

Operational readiness also inspects **every** workspace-scoped table for
`FORCE ROW LEVEL SECURITY` and all four restrictive API policies for
SELECT/INSERT/UPDATE/DELETE. Live adversarial HTTP probes verified that
removing any required policy or disabling forced RLS changes `/readyz`
from HTTP 200 to HTTP 503, and restoring it changes readiness back to 200.
The probes use a separate disposable database; no live customer database
or real user data was touched. A malicious database owner can still modify
policy functions: this does not claim resistance to privileged SQL injection.

Bearer parsing rejects duplicate `Authorization` headers before a credential
is propagated to the SQL transaction context. Oversized request bodies and
framework-generated errors retain the response privacy headers.

Run the entire local migrated-database validation with
`scripts/run-v011-validation.sh` (requires three explicit test-only DB URLs).

## v0.12 — bounded provenance-based investigative leads

The authenticated, read-only `POST /v1/workspaces/{workspace}/leads`
accepts the same JSON body as `/assess`, e.g. `{"report_ids":["uuid1","uuid2"]}`,
and returns up to three deterministic, **unapproved investigative proposals**.
The engine prioritizes a check of common evidence origins, shared uploaded
source-document context, and original-record attribution. If no overlap is
recorded, it asks for separately documented context without claiming that
source boundaries establish witness independence. A single report prompts
source attribution and original context checks. Each proposal includes a
stable kind, the associated report indices, bounded provenance pointers (at
most eight UUIDs), and `provenance_ids_truncated` when that limit applies.

Every call requires valid read permission for the named workspace before the
SQLx adapter loads evidence. Cycles, missing parents and withdrawn provenance
fail closed; invisible evidence IDs never become suggestions. The response
indicates `human_review_required=true`, `independence_established=false`, and
`generated_from=deterministic_provenance_heuristics`. No LLM, psychological
interpretation, independent source verification, autonomous follow-up or
persistent question creation occurs here. Humans may inspect and decide
whether to add a proposal through the existing audited question API.

Reproduce with `scripts/run-v012-validation.sh` on an already-migrated,
**disposable** PostgreSQL database. Provide explicit admin, legacy and
restricted API DB URLs. The complete tests include live authorization,
revocation and cross-workspace rejection.

## v0.13 — attributed, immutable, two-source human hypotheses

This release adds **human-authored** review hypotheses without treating
apparent contradiction as a verified fact. The new SQL migration
`sql/0010_manual_hypotheses.sql` stores a specific human actor, two different
workspace-local evidence IDs, the manually selected label
`apparent_contradiction`, `attribution_question` or `timing_question`,
and an immutable note. Both evidence foreign keys include `workspace_id`.
The table is protected by forced RLS, plus all four restrictive API policies
that bind the transaction to the validated bearer credential.

- `POST /v1/workspaces/{workspace}/hypotheses/{hypothesis}` accepts
  `{ "kind": "apparent_contradiction", "left_evidence_id": "uuid", "right_evidence_id": "uuid", "human_note": "..." }`
  and returns HTTP 201 (inserted), 200 (identical replay), 409 (conflicting
  replay), 403 (no write grant), 404 (unknown/cross-workspace evidence), or
  400 (invalid inputs). The actor comes from the validated token.
- `GET /v1/workspaces/{workspace}/hypotheses?limit=25&after=uuid` lists
  up to 100 records per page using stable UUID keyset ordering.
- Every record explicitly says `status=HUMAN_HYPOTHESIS` and
  `is_verified_fact=false`; it does not revise source evidence or constitute
  a factual finding.

Six dedicated integration tests cover identical/conflicting replay, read-only
membership, foreign workspace FKs, digest-gated RLS, immutable metadata,
24 concurrent identical writes, and nine-record pagination. The schema was
applied from scratch with all ten migrations in a disposable PostgreSQL
instance. A privileged DB owner can always alter the schema; these tests do
not claim to defend against a compromised PostgreSQL administrator.

Reproduce with `scripts/run-v013-validation.sh` after applying migrations
`0001..0010` to a **disposable** database and configuring the admin, legacy,
and restricted application DB URLs.

## v0.14 — immutable Situation revisions for visible reanalysis

The experimental core now includes a bounded `Situation` state that preserves the current focus,
user-reported contextual premises, explicit `WHAT IF` premises, source references, corrections and
retractions as immutable revision snapshots. Each snapshot carries a deterministic SHA-256
fingerprint of exactly the focus, active premises and source references used by a reasoning step.
A correction creates a new premise and marks the old one as corrected; a retraction removes a
premise from the current snapshot without erasing its historical record. Hypothetical premises keep
an explicit `Hypothetical` kind and are never silently promoted to evidence.

This is deliberately narrower than the unresolved `MVP-HYP-001`: it does **not** decide durable
semantics for remembered rejection/deprioritization of machine hypotheses, probability scores, or
full dependency repair. Those remain design decisions rather than accidental implementation policy.

## v0.15 — dependency-free first-party Situation shell

The loopback Axum process now serves a small first-party web shell at `/`. The page presents the
three approved entry intentions inside one workflow, catalogues selected `.txt`/`.eml` filenames
without claiming they were uploaded or analyzed, accepts user-reported context and explicit WHAT-IF
premises, and shows visible browser-side revisions. It intentionally performs no model call and no
hidden source analysis yet.

The shell is embedded in the Rust binary with `include_str!` and uses browser-native ES modules;
there is no CDN or runtime filesystem dependency. Its state reducer is tested with Node 24's built-in
test runner, so this milestone does not depend on the currently inconsistent prefetched npm lockfile.
All HTTP responses now also receive a restrictive Content-Security-Policy header.


## v0.16 — persistent Situation revisions

[SITUATIONS_V016.md](SITUATIONS_V016.md) moves bounded Situation state into PostgreSQL.
The authenticated API now persists focus, incremental source-document membership, attributed
user-reported / hypothetical / investigative-preference contributions, corrections and retractions.
Every material change creates an immutable revision with a stable SHA-256 fingerprint and exact
active source/contribution membership. Historical snapshots are never rewritten to look as though
they contained later context. Migration `sql/0011_situations.sql` adds six forced-RLS tables and
all restrictive `niccolo_api` gates.

## v0.17 — frozen analysis-input snapshots

[ANALYSIS_SNAPSHOTS_V017.md](ANALYSIS_SNAPSHOTS_V017.md) adds an immutable binding between
an analysis request and the exact Situation revision/fingerprint it may use. If the human adds
context while an operation is in flight, the original analysis binding remains on its older input.
An idempotent retry of the same `analysis_id` also returns the original input rather than silently
rebinding to newer context. This is a persistence/concurrency primitive only: no model is called and
no unperformed source analysis is claimed.

Apply migrations `0001..0012` to a disposable PostgreSQL 17 database, then reproduce the complete
validation with `scripts/run-v017-validation.sh`. The script requires the admin, legacy service and
dedicated secure API URLs and also runs the dependency-free Node state tests.
