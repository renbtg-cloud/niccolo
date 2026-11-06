# Niccolò v0.250 — V1 architecture checkpoint

This checkpoint deliberately optimizes for a **logic-rich V1** rather than a visually elaborate prototype.
The public product version for the first cohort is **V1.0.0.0.0.0**; the internal Rust package version remains
an implementation checkpoint and is not a product-readiness claim.

## Product boundary

Niccolò may organize, compare, expose uncertainty, propose investigations, request clarification and prepare
next steps. The human owns the project, the decisions and the authorization to act. Recorded acceptance,
review, priority or rationale is never silently converted into execution authority.

## v0.31–v0.180 — explicit reasoning and human-agency state

The PostgreSQL core now persists structured claims, support links, human reviews, reanalysis lineage,
clarification impact, deterministic manifests/exports, uncertainty, action proposals and decisions, review
sessions, attention controls, rationales, decision points, assumptions/alternatives, evidence gaps, human goals,
priorities, constraints, projected consequences, stop conditions, source requests, purpose-bound assessments,
conflict sets and deterministic handback records.

These records are designed to make reasoning inspectable without claiming that a stored relation establishes
truth, independence, correctness or authorization. Human-owned control events are append-only and attributed.

## v0.181–v0.200 — durable BFF operations

The BFF has one durable operation model with multiple interaction styles over the same domain semantics:

- synchronous submission: create/execute/wait and return the terminal result;
- asynchronous submission: create the same operation and return its operation ID immediately;
- pull-oriented observation: conditional `GET` with `ETag` / `If-None-Match`, server polling hints and
  cursor-paginated operation events;
- explicit cancellation and human-pause/resume states.

Idempotency keys bind to canonical input hashes. Operation state is versioned, result/error envelopes are
shared across interaction styles, and asynchronous work propagates only the validated authorization digest,
never the raw bearer token.

This intentionally permits different **transport/interaction implementations** for the same task without
forking the underlying domain logic.

## v0.211–v0.225 — durable agentic controller

Agentic work is represented as a versioned, hashed DAG of bounded steps rather than one opaque prompt. The
controller records dependencies, selects ready steps deterministically, enforces a hard step budget, rejects
cycles, can stop at explicit human-decision steps and resumes the same durable operation after human input.

Initial step kinds cover source inspection, context assessment, clarification, synthesis, assumption and
alternative checking, stop-condition checks, explicit human decisions and handback preparation. This is the
controller substrate; it is not yet a claim that a frontier-model reasoning adapter is complete.

## v0.226–v0.250 — React V1 shell, structurally complete and visually simple

The first-party UI is now React-based. Visual treatment is deliberately simple; state architecture is not.

- TanStack Query owns server state and cache/revalidation.
- Zustand owns client/session/view state.
- React Router owns navigation state.
- React Hook Form + Zod own form state and validation.
- The BFF client supports synchronous, asynchronous and pull-oriented operation flows.
- Dense inspector/review surfaces may be desktop-first in V1 rather than forcing premature mobile parity.
- Credentials remain memory-only; they are not persisted in local storage.

The server embeds the production React build and retains truthful static fallback text. V1 may primarily use
traditional request/response and pull; server push can be added as another observation interface without
redesigning the operation model.

## Validation at this checkpoint

On a PostgreSQL 17.11 database created from scratch with migrations `0001` through `0071`:

- 0/349 Rust checks failed; 349/349 passed across 42 suites.
- 0/4 React tests failed; 4/4 passed.
- strict Clippy (`-D warnings`) passed.
- `rustfmt --check` passed.
- the production React bundle built offline.

The checkpoint is still experimental. It does not yet mean the full V1 agentic experience is complete: model
adapters, controller policies, end-to-end source ingestion, richer reasoning orchestration and cohort deployment
still require implementation and adversarial validation.
