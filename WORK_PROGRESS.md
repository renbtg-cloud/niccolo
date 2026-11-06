# Active work progress

This file is an intentionally frequent checkpoint for long or uncertain development/test runs.

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
