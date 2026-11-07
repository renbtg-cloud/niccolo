# Active test progress

## v1.241-v1.320 — model-observed source coverage and human-context provenance

FINAL STATUS: DONE

Validated on Debian 13 / PostgreSQL 17.11 with the current 80-migration schema:

- tentative-MVP OpenAI-compatible provider E2E: 0/1 failed; 1/1 passed;
- model-reported invalid human-context usage rollback: 0/1 failed; 1/1 passed;
- analysis coverage focused suite: 0/8 failed; 8/8 passed;
- full Rust/PostgreSQL regression, executed in bounded suites: 0/404 failed; 404/404 passed;
- frontend: 0/32 failed; 32/32 passed across 8 test files;
- production frontend build: passed;
- rustfmt check: passed;
- strict Clippy with `pg-integration` and `-D warnings`: passed.

Validated boundaries:

- a successful `INSPECT_SOURCES` step records only frozen fragments actually included in that model-facing reasoning packet as examined;
- claim source provenance can then reference those examined fragments and remains an analysis-use relation rather than proof;
- `ASSESS_CONTEXT` may explicitly report which frozen human contributions it used;
- model-reported contribution IDs must belong to the analysis's frozen Situation revision;
- an invented/non-frozen contribution ID rejects the whole reasoning update atomically;
- synthesis may link a claim to human context only through the pre-existing closed context relation set (`INFORMS`, `QUALIFIES`, `CONFLICTS_WITH_REPORTED_CONTEXT`);
- BFF epistemic readback preserves source provenance and human-context provenance separately and explicitly reports `human_context_is_evidence=false`.

---

## v1.201-v1.240 — model-produced claim provenance

FINAL STATUS: DONE

Validated from a fresh Debian 13 / PostgreSQL 17.11 environment using all 80 migrations:

- provider-created claim-to-source link focused regression: 0/1 failed; 1/1 passed;
- frozen source/identity reasoning packet: 0/3 failed; 3/3 passed;
- full Rust/PostgreSQL regression, executed in bounded suites: 0/403 failed; 403/403 passed;
- frontend: 0/32 failed; 32/32 passed across 8 test files;
- production frontend build: passed;
- rustfmt check: passed;
- strict Clippy with `pg-integration` and `-D warnings`: passed.

Validated boundaries:

- a model-produced source link can reference only a claim in the same synthesis artifact;
- source-link relation is a closed set (`SUPPORTS_AS_USED`, `CONTRADICTS_AS_USED`, `CONTEXT_ONLY`);
- the referenced evidence must already be an examined fragment of the frozen analysis;
- an invented/non-examined evidence UUID fails closed and the synthesis artifact is rolled back atomically;
- recording a support relation remains analysis-use provenance, not proof or a truth claim;
- identity bindings remain frozen at the analysis identity-event horizon and later corrections do not rewrite earlier reasoning input.

Environment recovery also exposed and repaired incomplete identity-freeze integration in the remote checkpoint (missing snapshot/packet field initialization and an unavailable SQLx derive/helper path). The repaired implementation compiles offline before the regression suite is run.

---

## v1.041-v1.080 — frozen-source reasoning

FINAL STATUS: DONE

Validated on the committed test suite and current implementation:

- focused frozen-revision source selection: 0/1 failed; 1/1 passed;
- deterministic truncation/boundedness: 0/1 failed; 1/1 passed;
- tentative-MVP provider E2E: 0/1 failed; 1/1 passed;
- full Rust/PostgreSQL regression: 0/401 failed; 401/401 passed;
- frontend: 0/30 failed; 30/30 passed across 8 test files;
- production frontend build: passed;
- rustfmt check: passed;
- strict Clippy with pg-integration: passed.

Validated boundaries:

- model-facing source fragments come only from the frozen Situation revision;
- sources attached after the freeze are excluded;
- frozen source packets are capped at 256 fragments;
- truncation is explicit rather than silent;
- repeated packet construction is deterministic and produces a stable hash;
- the real OpenAI-compatible TCP provider path receives frozen source text and human context as distinct material.

This file also remains the active five-minute checkpoint surface for ongoing work.
