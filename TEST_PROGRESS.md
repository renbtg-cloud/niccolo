# Active test progress

## v1.841-v2.000 — prepared demo and buyer-facing review surfaces

FINAL STATUS: DONE

Frontend validation executed on a clean GitHub Actions runner from the exact code branch:

- frontend: 0/51 failed; 51/51 passed across 16 test files;
- production frontend build: passed;
- validation branch differed from the work branch only by the temporary validation workflow and trigger sentinel.

The backend is unchanged in this block. The current Rust/PostgreSQL baseline remains 410/410 green.

Validated behavior:

- prepared demo links now render as a named Prepared Situation instead of exposing workspace/Situation UUIDs as primary UI; identifiers remain inspectable under details;
- bearer token remains separate, required, excluded from URL hints and memory-only;
- Situation shows transparent focus/source/context counts without inventing a completeness/readiness score or blocking a focus-only first pass;
- Review renders the concrete competing interpretations carried by deterministic handoff, including rationale and explicit non-fact/non-ranking semantics;
- Review is self-contained with the published Niccolò answer and coverage note at the top while preserving historical-input labeling;
- buyer-facing structural guard tests protect Prepared Situation, non-gating Situation guidance and concrete Review interpretation surfaces.

---

## v1.801-v1.840 — prepared demo connection hints without secret leakage

FINAL STATUS: DONE

Frontend-focused validation:

- frontend: 0/45 failed; 45/45 passed across 14 test files;
- production frontend build: passed;
- 4/4 changed frontend files were synchronized to GitHub with blob SHAs identical to the tested local files.

The backend is unchanged in this block. The current Rust/PostgreSQL baseline remains 410/410 green.

Validated behavior:

- prepared demo links may prefill only workspace and Situation identifiers;
- bearer tokens are never read from URL parameters;
- bearer tokens remain memory-only and are not persisted;
- absent URL hints leave connection fields empty rather than inventing defaults;
- the connection UI explicitly communicates the non-secret/secret boundary.

---

## v1.761-v1.800 — safe analysis recovery across connection boundaries

FINAL STATUS: DONE

Frontend-focused validation:

- frontend: 0/42 failed; 42/42 passed across 13 test files;
- production frontend build: passed;
- 5/5 changed frontend files were synchronized to GitHub with blob SHAs identical to the tested local files.

The backend is unchanged in this block. The current Rust/PostgreSQL baseline remains 410/410 green.

Validated behavior:

- when connected with no explicit analysis selection, the UI recovers the newest analysis from server-side history for the current workspace + Situation;
- analysis-history cache keys are workspace- and Situation-scoped, so recovery cannot collide across different Situations;
- disconnect clears selected analysis and active operation before another connection can be established;
- bearer token, workspace and Situation credentials remain session-only and are not persisted to browser storage;
- no analysis UUID is invented client-side during recovery; empty history leaves selection empty.

---

## v1.721-v1.760 — primary Situation → Analyze → Review journey

FINAL STATUS: DONE

Frontend-focused validation:

- frontend: 0/39 failed; 39/39 passed across 11 test files;
- production frontend build: passed;
- 4/4 changed frontend files were synchronized to GitHub with blob SHAs identical to the tested local files.

The backend is unchanged in this block. The immediately preceding Rust/PostgreSQL baseline remains 410/410 green.

Validated behavior:

- the primary navigation now leads with Situation → Analyze → Review before technical inspection surfaces;
- an existing Situation offers a direct “Analyze this Situation” continuation;
- the analysis page presents the ordinary synchronous path as the default human-facing action;
- sync/async transport choice and reasoning step budget remain available, but are moved under Advanced execution;
- a completed answer links directly to human Review for reasoning, clarifications and next steps;
- transport/budget controls remain engineering controls over the same durable intent semantics; hiding them from the primary path does not alter backend meaning.

---

## v1.681-v1.720 — ReviewPage hook-order stability

FINAL STATUS: DONE

Frontend-focused validation:

- frontend: 0/38 failed; 38/38 passed across 10 test files;
- production frontend build: passed;
- a structural guard test now fails if ReviewPage declares React/TanStack hooks after its first conditional return.

Validated behavior:

- ReviewPage now declares all state/query/mutation hooks before connect/analysis/error conditional returns;
- changing from disconnected to connected, or from no selected analysis to a selected analysis, no longer changes the component's hook call order;
- receipt, clarification and reanalysis behavior is unchanged; this block is demo/runtime stability only.

---

## v1.641-v1.680 — current vs historical handoff receipt semantics

FINAL STATUS: DONE

Focused validation on Debian 13 / PostgreSQL 17.11 with the 81-migration schema:

- adversarial handoff receipt HTTP test, including post-receipt handoff mutation: passed;
- frontend: 0/37 failed; 37/37 passed across 9 test files;
- production frontend build: passed;
- strict Clippy with `pg-integration` and `-D warnings`: passed.

The full Rust/PostgreSQL baseline remains the immediately preceding 410/410 v1.561-v1.600 regression. This block changes only receipt read-model currentness and corresponding presentation/tests.

Validated behavior:

- receipt list responses expose the current deterministic handoff SHA;
- each receipt is classified by the backend as matching or not matching the current handoff;
- a receipt recorded as `REVIEWED` for an older hash remains historical after reasoning state changes the handoff;
- historical review never silently becomes review of the current handoff;
- the Review UI reports current-handoff receipt counts separately from historical receipt counts.

---

## v1.601-v1.640 — inspectable handoff receipt audit trail

FINAL STATUS: DONE

Focused validation on Debian 13 / PostgreSQL 17.11 with the 81-migration schema:

- handoff receipt domain test: passed;
- adversarial handoff receipt HTTP test: passed;
- frontend: 0/37 failed; 37/37 passed across 9 test files;
- production frontend build: passed;
- changed Rust files pass rustfmt check;
- strict Clippy with `pg-integration` and `-D warnings`: passed.

The immediately preceding v1.561-v1.600 checkpoint ran the full Rust/PostgreSQL regression at 410/410. This block changes only receipt read-model/audit presentation fields and corresponding assertions; it does not change schema or receipt mutation semantics.

Validated behavior:

- listed handoff receipts now expose their human note and database-recorded timestamp in addition to hash, kind and authenticated recipient;
- the historical receipt event remains append-only and tied to the exact handoff SHA;
- the Review UI shows each receipt event rather than only aggregate counts;
- `REVIEWED` remains an audit statement about reviewing a bundle, not agreement with its claims and not authority to act.

---

## v1.561-v1.600 — SHA-bound authenticated reasoning handoff receipts

FINAL STATUS: DONE

Validated on Debian 13 / PostgreSQL 17.11 with the 81-migration schema:

- focused adversarial handoff-receipt API test: passed;
- full BFF HTTP integration suite: 0/17 failed; 17/17 passed;
- existing review-governance handoff receipt milestone remained green;
- full Rust/PostgreSQL regression, executed in bounded suites: 0/410 failed; 410/410 passed;
- frontend: 0/37 failed; 37/37 passed across 9 test files;
- production frontend build: passed;
- changed Rust files pass rustfmt check;
- strict Clippy with `pg-integration` and `-D warnings`: passed.

Validated behavior:

- a receipt binds to the exact current deterministic handoff SHA-256;
- stale handoff hashes fail closed with HTTP 409 and require the human to refresh before recording a receipt;
- the receipt recipient is derived from the authenticated principal, never accepted from browser JSON;
- read-only credentials cannot record receipts;
- exact replay of the same receipt identifier and payload is idempotent and returns replay state;
- reusing a receipt identifier with changed hash/kind/note/recipient fails closed as a replay conflict;
- `RECEIVED`, `REVIEWED` and `DECLINED` remain distinct append-only human events;
- receipt means neither agreement nor authority;
- the first-party Review UI can record and summarize receipts without changing the handoff or executing any action.

---

## v1.521-v1.560 — analysis-scoped reasoning handoff read model

FINAL STATUS: DONE

Validated on Debian 13 / PostgreSQL 17.11 with the 81-migration schema:

- focused handoff HTTP read-model test: passed;
- full BFF HTTP integration suite: 0/16 failed; 16/16 passed;
- full Rust/PostgreSQL regression, executed in bounded suites: 0/409 failed; 409/409 passed;
- frontend: 0/35 failed; 35/35 passed across 9 test files;
- production frontend build: passed;
- strict Clippy with `pg-integration` and `-D warnings`: passed;
- changed Rust files pass rustfmt check.

Validated behavior:

- the deterministic reasoning handoff is readable directly from the analysis surface, independent of active operation state;
- the read model returns the exact v2 handoff plus its SHA-256;
- candidate interpretations and clarification state survive into this read surface;
- the HTTP surface remains explicitly non-authoritative and non-executing;
- the first-party React review page exposes handoff version, SHA, claim count, candidate count and unanswered clarification count;
- browser presentation derives counts only from the backend-owned handoff and never invents authority or action semantics.

---

## v1.481-v1.520 — deterministic handoff v2 preserves reasoning trail

FINAL STATUS: DONE

Validated on Debian 13 / PostgreSQL 17.11 with the 81-migration schema:

- focused handoff v2 review suite: 0/8 failed; 8/8 passed;
- deterministic runtime handoff regression: passed;
- full Rust/PostgreSQL regression, executed in bounded suites: 0/408 failed; 408/408 passed;
- frontend: 0/32 failed; 32/32 passed across 8 test files;
- production frontend build: passed;
- rustfmt check: passed;
- strict Clippy with `pg-integration` and `-D warnings`: passed.

Validated behavior:

- the deterministic handoff format is now version 2;
- candidate interpretations survive into handoff as explicit non-facts and remain explicitly unranked;
- analysis clarification questions survive into handoff with question text, reason, frozen input revision, answer count and answered state;
- clarification records remain explicitly non-evidence;
- answering a clarification changes the handoff SHA-256 without rewriting or removing the original question;
- existing claims, claim-linked open questions, revision conditions and non-authority/non-execution boundaries remain intact;
- repeated handoff construction over unchanged state remains byte-deterministic and hash-stable.

---

## v1.441-v1.480 — clarification lineage in later reasoning

FINAL STATUS: DONE

Validated on Debian 13 / PostgreSQL 17.11 with the 81-migration schema:

- focused clarification persistence-to-synthesis runtime path: passed;
- explicit clarification packet budget/determinism test: passed;
- modern milestone suite: 0/33 failed; 33/33 passed;
- tentative-MVP provider E2E: 0/1 failed; 1/1 passed;
- full Rust/PostgreSQL regression, executed in bounded suites: 0/408 failed; 408/408 passed;
- frontend: 0/32 failed; 32/32 passed across 8 test files;
- production frontend build: passed;
- rustfmt check: passed;
- strict Clippy with `pg-integration` and `-D warnings`: passed.

Validated behavior:

- clarification questions discovered by `PROPOSE_CLARIFICATION` no longer evaporate before later model steps;
- later reasoning packets carry the exact persisted question ID, text, reason and originating input revision;
- `SYNTHESIZE` therefore sees the concrete unresolved question rather than only an aggregate clarification count;
- at most 100 clarification records enter one reasoning packet;
- additional records are disclosed through `clarifications_truncated=true` rather than silently omitted;
- repeated packet construction over unchanged state preserves clarification ordering/content and produces the same SHA-256.

---

## v1.321-v1.440 — durable reasoning trail and deterministic boundaries

FINAL STATUS: DONE

Validated on Debian 13 / PostgreSQL 17.11:

- fresh schema reconstruction: 81/81 migrations applied from scratch;
- fresh-schema readiness boundary: 0/3 failed; 3/3 passed;
- fresh-schema secure RLS audit: 0/10 failed; 10/10 passed;
- full Rust/PostgreSQL regression, executed in bounded suites: 0/407 failed; 407/407 passed;
- tentative-MVP provider E2E: 0/1 failed; 1/1 passed;
- modern milestone suite: 0/32 failed; 32/32 passed;
- frontend: 0/32 failed; 32/32 passed across 8 test files;
- production frontend build: passed;
- rustfmt check: passed;
- strict Clippy with `pg-integration` and `-D warnings`: passed.

Validated behavior:

- `PROPOSE_CLARIFICATION` is now a real reasoning-update step: model-discovered questions survive the invocation and are persisted before human handoff;
- `CHECK_STOP_CONDITIONS` is deterministic and reads recorded stop state instead of spending an LLM call;
- `PREPARE_HANDOFF` is deterministic and produces a versioned, SHA-256-bound handoff whose contract explicitly grants no authority and executes no action;
- `CHECK_ALTERNATIVES` now persists pre-synthesis candidate interpretations instead of discarding them;
- candidate interpretations are model-generated possibilities, not facts, claims, decisions, or automatic rankings;
- persisted candidates enter later reasoning packets and are therefore available to synthesis/challenge;
- candidate replay with the same identifier but changed content fails closed without rewriting the original;
- the BFF epistemic view and React reasoning surface expose candidates with explicit non-fact/non-ranking boundaries;
- the new workspace-scoped candidate table satisfies the same four restrictive API RLS gates required by readiness.

---

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
