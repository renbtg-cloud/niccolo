# Active work progress

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
- 81/81 SQL migrations apply from scratch on PostgreSQL 17.11;
- 410/410 Rust/PostgreSQL tests pass in bounded suites;
- 42/42 frontend tests and the production build pass;
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
