# Active work progress

## Adaptive worker admission — ARM-003 first scheduler slice (verified)

PR #29 merged into main at `519d575d`.
- Added atomic workspace-wide active execution lease ceilings. Competing workers serialize admission on the workspace row, preventing several workers from observing the same free capacity and oversubscribing it.
- New explicit `worker_once_adaptive` path: live Linux resource snapshot -> ARM policy advice -> bounded claim for `SELF_MANAGED` / `HYBRID`; `EXTERNALLY_MANAGED` continues to delegate parallelism.
- Critical pressure can reduce admission to one active path, preserving progress while blocking uncontrolled parallel expansion.
- Existing `worker_once` and direct execution remain unchanged for compatibility; adaptive behavior is opt-in at this layer.
- Concurrent PostgreSQL integration races eight workers against an active-lease ceiling of two and verifies exactly two admissions, then verifies capacity becomes reusable after release.
- Full Linux CI for PR #29 passed unit/rustfmt/strict Clippy, frontend, migrations and PostgreSQL regression.
- Still not implemented: cgroup mutation, GPU scheduling/residency, predictive resource planning, persisted resource telemetry, or the epistemic-degradation/anytime-result controller itself.


## Adaptive resource policy modes — ARM-002 advisory layer (verified)

PR #27 merged into main at `4a3c081f`.
- Added explicit `SELF_MANAGED`, `EXTERNALLY_MANAGED` and `HYBRID` resource-management modes over the verified Linux telemetry from ARM-001.
- Policy computes a conservative observed parallel-operation ceiling from effective CPU entitlement and memory pressure/headroom, including constrained and critical pressure states.
- Critical memory pressure preserves one path of progress instead of pretending unlimited capacity; delegated mode observes and reports pressure but does not authorize Niccolò to apply tuning automatically.
- The layer is deliberately advisory: it does not mutate cgroups, worker concurrency, model selection or epistemic state. Capacity remains distinct from epistemic quality.
- Full Linux Actions passed for PR #27. Next work is bounded scheduler/admission wiring, with explicit operator mode and safe fallback when telemetry is unavailable.


## Linux effective-resource observation — ARM-001 (verified)

PR #26 merged into main at `f538ea31` (superseding the transient duplicate ARM branch/PR bookkeeping).
- New read-only `runtime_resources` observer measures the process/cgroup-v2 envelope rather than assuming host totals are available to Niccolò.
- Linux observations include process RSS/peak RSS, memory current/limit/headroom/pressure, CPU quota, effective cpuset, conservative effective CPU capacity, CPU throttling and PID current/limit.
- Unknown and explicitly unlimited cgroup limits remain distinct; malformed kernel data fails closed; cgroup paths cannot escape the observation root.
- Runtime capacity is explicitly **not** epistemic quality, does not change concurrency/model choice, does not mutate cgroups and does not claim GPU observation exists.
- Live Linux CI exercises the actual observer. Full Actions runs `42429716582` and `42430147310` passed the exact implementation head: **464/464 PostgreSQL-feature Rust tests**, **140/140 library tests**, **66/66 frontend tests**, **3/3 offline Python export tests**, all **83 SQL migrations**, rustfmt and strict Clippy.
- Next ARM slice: operator-selected `self_managed` / `externally_managed` / `hybrid` policy and conservative admission/concurrency advice before any scheduler mutation.


## Operation deadline/budget observability — ARM/TIME first slice (verified)

PR #23 merged into main at `562e52d3`.
- Existing hard-stop operation budgeting now exposes PostgreSQL-clock **remaining deadline milliseconds**, invocation capacity and token capacity, plus explicit stop reasons.
- New authenticated read-only endpoint: `GET /v1/workspaces/{workspace}/situations/{situation}/bff/operations/{operation}/budget`.
- Budget counters remain operational limits, **not epistemic quality, probability or truth**. The API explicitly reports that graceful quality degradation is not yet implemented.
- Exhausted signed counters are clamped at zero; actual PostgreSQL deadline expiry is integration-tested.
- A preexisting agentic-plan visibility test was hardened from scheduler-only polling to a bounded wall-clock wait after recurring CI flakiness.
- Linux Actions run `42428951000` passed: **456/456 PostgreSQL-feature Rust tests**, **132/132 library tests**, **66/66 frontend tests**, **3/3 offline Python export tests**, all **83 SQL migrations**, rustfmt and strict Clippy.
- Next ARM work can safely build on the deadline/budget accounting plus the verified Linux effective-resource observer: self-managed/delegated policy, admission/concurrency adaptation and eventually honest anytime partial-result/degradation semantics.


## User-owned Git archival and Python/CSV explanation — MVP first slice (verified)

PR #22 merged into main at `e20fcfea`; implementation/plan are documented in `MVP_PORTABILITY_AND_ADAPTIVE_RUNTIME.md`.
- Product commitments: PostgreSQL remains the **sole authoritative operational state**; the user owns the intended GitHub destination; `intake`, `work` and published milestones are historical publication concepts, **not** the epistemic dependence DAG.
- MVP 1.0 presentation scope is Python plus CSV and/or simple JSON data, to explain real recorded results to other people. No Excel/VBA, local simulations, reingestion, GitLab, automatic dump of PostgreSQL or general-purpose adapter platform.
- Explicit investigation time, computing and monetary budgets; future anytime results and **honestly disclosed epistemic degradation**; self-managed/delegated resource modes and Linux-first effective-limit observation. These are requirements, **not yet implemented resource-control behavior**.
- Real implementation: authorized **GET** `/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/presentation-package` returns a bounded deterministic collection of files: analysis JSON, CSV for recorded claim/context/evidence relationships, offline Python inspection, README with known omissions and SHA-256 file digests. No publication to Git is attempted by this endpoint.
- A first packet contains the **published analysis bundle**, not source fragment bodies, complete intermediate model reasoning, numeric scoring receipts or a complete input history; it explicitly discloses those omissions. It does not recreate LLM reasoning or assert independent witness certainty.
- Linux Actions run `42427377902`: **450/450 full PostgreSQL-feature Rust tests**, **66/66 frontend tests**, **3/3 offline Python inspector tests**, all **83 SQL migrations**, strict Clippy and rustfmt passed; new negative security test confirms unauthenticated and cross-workspace export denial.
- Remaining before user-owned historical preservation exists end-to-end: explicit input export consent/scope, Python-visible actual heuristic receipts and source material when authorized, artifact packaging/download UI, GitHub connection and verified remote publishing with retries, provisional and milestone checkpoint persistence. Do not mark those as complete.


## Human-configurable reasoning Playground — EPI-017 first slice (verified)

PR #21 merged to main at `dc05aad` (replaces closed, unmerged PR #20).
- First-party React Playground is discoverable under **Technical**, without disturbing the main Situation → Analyze → Review journey.
- Trial parameters: support and contradiction multipliers, origin-group cap, subjective human confidence and fictional report-source overlap. Uses the real authenticated baseline-vs-trial comparison API.
- Trial comparisons never persist state, verify evidence, invoke an LLM, certify probability or rewrite historical analysis.
- A **separate explicit action** can record a new append-only weighting policy version for the Situation or as the workspace default; human rationale is mandatory. Existing backend authorization determines write access.
- Stale async previews from an earlier configuration/session are not shown as current.
- Linux CI run `42421075754` passed: **444/444 Rust tests**, **66/66 frontend tests**, 83 PostgreSQL migrations, formatting, strict Clippy and production build.
- This is **not** a model-level replay; EPI-016 remains separate.

## PostgreSQL-grounded multi-parent scoring — EPI-014 operational integration (verified)

PR #19 merged to main at `b8ee3c6`.
- Selected, examined, claim-linked evidences are walked through their **recorded** parent derivations in a bounded repeatable-read PostgreSQL snapshot.
- Source clustering counts reports from different documents with the same recorded ancestor as dependent, not independent. The score exposes a structural dependence component key, not a certified original source ID.
- Revoked ancestry and cycles fail closed; source graph revision is returned with each calculation. Unknown unrecorded relationships may still exist.
- Linux CI run `42420338447` passed **444/444 Rust tests** and 58/58 frontend tests (before the later Playground), all 83 migrations and strict Clippy. The five focused provenance integrations include a cross-document common-ancestor test.
- A previously flaky async HTTP CI test now uses a bounded wall-clock timeout instead of 30 immediate scheduler yields.

## Multi-parent provenance dependence — EPI-014 foundation (verified)

PR #18 merged to main at `fd3cb44`:
- New `src/provenance_clusters.rs` pure Rust helper computes conservative transitive dependence components for selected evidence linked by the same source document or any **documented common ancestor**.
- Multi-parent ancestry, diamonds and chains are handled; missing declared ancestors, cycles and withdrawn ancestors fail closed. It does not certify truth, witness independence or completeness of unknown provenance relationships.
- This is the pure helper now **also has an operational PostgreSQL integration** via PR #19; the earlier PR #18 remains the independently validated algorithmic foundation.
- CI run `42418745135`: **0/440 Rust tests failed (440/440 passed)**, including **122/122 library tests**; **0/58 frontend tests failed (58/58 passed)**; the focused API integration suites, all 83 SQL migrations, strict Clippy, formatting and frontend build passed.
- Completed later in PR #19: closure over recorded PostgreSQL derivation edges, conservative component grouping and safe failure for cyclic/withdrawn ancestry. Distinct clusters still do not prove witness independence.


## Source-grounded claim weighting — EPI-013 (verified)

PR #17 is merged into main at `1ab6a57`.
- Authenticated read-only endpoint evaluates user-specified numeric strengths **only for existing, examined, explicitly claim-linked source fragments**.
- Stance (support/contradiction/context) comes from stored claim-evidence relations, not a caller-provided stance. Document IDs come from source-fragment rows, not client-supplied origins.
- Missing/forged links are rejected; multiple contradictory links for one evidence/claim pair are rejected as ambiguous; requests cannot invent a document origin or verified truth.
- Conservative grouping by **source document** is explicit, but this is NOT multi-parent derivation-graph traversal, and separate documents are NOT established independent witnesses.
- It does not change historical analysis, silently promote subjective opinion into fact, compute calibrated truth probabilities, or authorize consequential action.
- The newly verified CI run `42418046760` passed **431/431 Rust tests** in the full PostgreSQL-feature suite, **2/2 grounded API integration tests**, **3/3 human credence integration tests**, **58/58 frontend tests**, all 83 migrations, strict Clippy, Rust formatting and frontend production build.
- Further bounded work: EPI-012 (human confidence annotations BEFORE an analysis claim exists), EPI-015 (durable evaluation trace including applied policy and graph revision), EPI-016 (full isolated, non-destructive model replay). EPI-014 has since been completed in PR #19.


## Epistemic reasoning weights — verified baseline

Milestones EPI-001 through EPI-011 have been merged into main through PR #16:
- Explicit, typed distinction between human subjective confidence, evidence balance, claim status and investigation priority.
- Append-only human confidence records on analysis claims, with actor attribution, RLS, replay conflict semantics and HTTP read/write APIs.
- Deterministic conservative evidence-weight grouping, without auto-promoting opinions or duplicate sources into verified facts.
- Append-only workspace defaults and Situation-specific heuristic weight policies with effective-policy resolution.
- Read-only preview and baseline/trial comparison endpoints; neither is a complete replay of model reasoning or a calibrated truth probability.
- Current human confidence and current effective heuristic policy are supplied to reasoning without rewriting frozen historical input.
- PostgreSQL migrations 0082 and 0083 added.

Acceptance in GitHub Actions: **0/427 Rust tests failed (427/427 passed)** in the full PostgreSQL-feature regression; **0/3 focused API/PostgreSQL tests failed (3/3 passed)**; **0/58 frontend tests failed (58/58 passed)**; all 83 SQL migrations, strict Clippy, formatting and production frontend build succeeded. The CI workflow is stored in `.github/workflows/epistemic-reasoning.yml`.

Still outside this completed slice: pre-analysis document-level subjective confidence, provenance-grounded automatic numerical claim weighting, multi-parent root resolution, and full isolated Replay Playground. See `EPISTEMIC_WEIGHTING_MVP.md` for bounded follow-ons EPI-012…017. The Linux-first MVP should not be delayed by post-MVP model routing or OS-specific installer work.


This file is an intentionally frequent checkpoint for long or uncertain development/test runs.

## 2026-11-07

Recovered the current repository into a fresh Debian 13 environment and rebuilt the offline Rust/Node/PostgreSQL toolchain. The resumed `main` checkpoint exposed a small incomplete identity-freeze integration; it has been repaired locally and the whole current test surface is green.

Closed/validated in this resumed block:
- v1.161-v1.200 frozen human identity bindings remain attached to the historical analysis horizon;
- v1.201-v1.240 model-produced claim provenance is accepted only for frozen/examined evidence and fails atomically for invented evidence IDs;
- v1.241-v1.280 successful model source inspection now persists exactly the frozen fragments actually presented to the model as examined coverage;
- v1.281-v1.320 model-reported human-context usage is constrained to frozen contributions; claim-to-context provenance remains explicitly distinct from evidence;
- v1.321-v1.360 model-proposed clarifications now persist as typed reasoning updates before human handoff;
- v1.361-v1.400 stop-condition checking and handoff preparation are deterministic steps, eliminating unnecessary LLM calls while preserving auditable controller state;
- v1.401-v1.440 competing interpretations are durable pre-synthesis reasoning state, feed later packets, and remain explicitly non-factual/unranked;
- v1.441-v1.480 persisted clarification questions now feed later reasoning packets with a bounded, explicit truncation contract;
- v1.481-v1.520 deterministic handoff v2 now carries clarification state and pre-synthesis candidate interpretations, so the human handback preserves the reasoning trail rather than only final claims;
- v1.521-v1.560 the deterministic handoff is now a stable analysis-scoped BFF read model and is visible in the first-party human review UI after operation state is gone;
- v1.561-v1.600 authenticated humans can now record SHA-bound handoff receipts (received/reviewed/declined); exact replay is idempotent, conflicting replay fails closed, and receipt never means agreement or authority;
- v1.601-v1.640 receipt history is now fully inspectable in the read model and Review UI with note, timestamp, recipient, kind and exact handoff hash while preserving non-agreement/non-authority semantics;
- v1.641-v1.680 receipt currentness is backend-owned: receipts for prior handoff hashes remain historical and can never imply that the current deterministic handoff was reviewed;
- v1.681-v1.720 ReviewPage hook ordering is stable across connect/analysis transitions and protected by a structural regression test;
- v1.721-v1.760 the first-party product now presents Situation → Analyze → Review as the primary journey; transport mode and step budget remain available only as advanced execution controls;
- v1.761-v1.800 reconnect with no explicit selection now recovers the newest analysis for the current Situation, while disconnect clears stale analysis/operation UI state and credentials remain memory-only;
- v1.801-v1.840 prepared demo links may prefill non-secret workspace/Situation identifiers while bearer tokens remain excluded from URLs and memory-only;
- v1.841-v1.880 prepared demo links now present as a Prepared Situation with UUIDs demoted to inspectable details while the bearer token remains separate;
- v1.881-v1.920 Situation shows transparent focus/source/context preparation signals without a readiness score or gate;
- v1.921-v1.960 Human Review now shows the concrete competing interpretations and rationale carried by deterministic handoff;
- v1.961-v2.000 Human Review is self-contained with the published answer and coverage note while preserving historical-input semantics;
- v2.001-v2.040 technical inspection pages are demoted behind a Technical navigation group while Situation → Analyze → Review remains primary;
- v2.041-v2.080 connected sessions expose a token-free prepared demo link for sharing; bearer credentials remain separate;
- v2.081-v2.120 Analyze keeps operation/agentic/event JSON behind Technical execution details instead of permanent primary-page clutter;
- v2.121-v2.160 primary Situation/connection copy removes BFF/backend jargon while keeping technical surfaces available;
- v2.161-v2.200 Situation now makes explicit source-identity review discoverable when records are attached;
- v2.201-v2.240 Identity Review reports human mapping coverage without converting coverage into truth or making full mapping a gate;
- v2.241-v2.280 controller pauses are presented as explicit human input with product-language Continue/Reconsider/Stop choices and no inferred action consent;
- v2.281-v2.320 agentic-plan polling stops at complete, blocked and waiting-for-human boundaries until an explicit action can change state;
- 81/81 SQL migrations apply from scratch on PostgreSQL 17.11;
- 410/410 Rust/PostgreSQL tests pass in bounded suites;
- 58/58 frontend tests and the production build pass;
- rustfmt and strict Clippy pass.

Next work should continue from the evidence produced by the running product rather than add post-MVP provider-routing complexity. Capability-aware LLM routing/BYOK ideas are deliberately parked in `POST_MVP_TO_SPECIFY.md`.

## 2026-11-06

Current resumed target:
- validate the corrected remote main state;
- implement a real LLM provider path behind the existing ReasoningModel boundary;
- wire provider selection/configuration into the runtime without weakening durable invocation logging, timeout, budget or failover behavior;
- add persistent test cases/fixtures before long validation;
- continue tentative-MVP milestones in small publishable increments.

Checkpoint policy: after each successful push, the next commit routine starts at +5 minutes even if the only useful change is this progress file.

## v0.961-v1.000 — explicit human feedback

Closed after focused validation:
- PostgreSQL human-feedback integration: 17/17 passed, 0 failed.
- Frontend tests: 30/30 passed, 0 failed.
- Production frontend build completed successfully.
- Distinct human positions remain separate: rejection, insufficient evidence and unknown are not collapsed into one state.

## Active next blocks

- v1.001-v1.040: native in-process OpenAI-compatible provider hardening. Earlier commits ed8b37c and 27e0e29 used stale v0.641-v0.680 wording; this note corrects the tracking label without rewriting history.
- v1.041-v1.080: frozen source fragments in the reasoning packet, so the real provider receives the actual source material rather than document IDs alone.
- follow-on: model-produced claim provenance must bind only to source/context IDs present in the frozen reasoning input.

## v1.001-v1.040 — native OpenAI-compatible provider

Closed after validation:
- native provider contract tests: 3/3 passed;
- legacy/config compatibility tests: 3/3 passed;
- tentative-MVP E2E using the native provider: 1/1 passed;
- rustfmt check passed;
- strict Clippy with pg-integration passed;
- niccolo-api now selects the native in-process provider from the existing environment configuration.
- the curl implementation remains compatibility/test code only; it is no longer the runtime path.

Next: v1.041-v1.080 makes frozen source fragments part of the reasoning packet and proves the provider receives the actual source bytes/text.
