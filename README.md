# Niccolò

**A thinking companion for complex human situations.**

Niccolò helps people examine situations where the evidence is fragmented, accounts disagree, intentions are uncertain, and a premature answer would be less useful than a better question. The restricted MVP brings human-supplied context, external-artifact references, attributed representations, competing interpretations, and possible next actions into one inspectable line of inquiry.

Its purpose is to **strengthen human judgment, not replace it**. The person defines what matters, contributes context at any moment, challenges the system's interpretations, and retains authority over consequential decisions and actions.

Niccolò is developed by Cambrian Radiation Computing. This repository contains its Rust/PostgreSQL core, local HTTP API, first-party web interface, tests, and design/validation documents.

## What Niccolò does

A **Situation** is the working space for a human question or project. A Situation can combine multiple records and evolving human context without flattening uncertainty into a single authoritative story.

| Capability | How it serves the person |
| --- | --- |
| **Reference external artifacts** | Record that an email, export, document, image, audio file, video, or physical object exists elsewhere without uploading, opening, or parsing it. Preserve declared identity, location, scope, attribution, and limitations. |
| **Separate evidence from interpretation** | Distinguish a source quotation, a person's reported belief, an inference, a human-authored hypothetical scenario, and a question worth investigating. |
| **Reason across relationships** | Compare claims, people, events, timelines, and potentially conflicting accounts without automatically declaring one of them true. |
| **Discover useful questions** | Propose a small number of promising investigative leads and explain what missing information could change the analysis. |
| **Examine human-authored alternatives** | Let the investigator record competing hypotheses, assumptions, and what-if scenarios, then expose their explicit consequences, tensions, gaps, and reasons to revise a conclusion. Niccolò does not invent the hypotheses. |
| **Work with changing context** | Let the human correct an attribution, retract a premise, add an artifact reference or representation, challenge a hypothesis, or change priorities; keep the analysis history intelligible. |
| **Plan without commandeering** | Organize bounded investigation steps, note dependencies and stop conditions, and pause at decisions requiring the human. |
| **Remember the reasoning trail** | Preserve provenance, frozen input revisions, feedback, and the differences between earlier and later analyses. |

These are **product capabilities and directions**, not a promise that every interaction in the table is fully available in the current interface. The implementation and intended coverage are distinguished below.

## A typical Situation

Imagine two people describing the same event differently in private email and a group conversation. In the restricted MVP, the user can reference both external records, explicitly supply the relevant descriptions or transcriptions with attribution and scope, add a personal recollection, and ask: *What can I reasonably conclude, what remains ambiguous, and what should I ask next?*

Niccolò should show which statements have documentary support, distinguish a copied report from an independent observation, identify apparent tensions without inventing motives, trace the consequences of hypotheses or what-if assumptions explicitly authored by the investigator, and propose a few targeted ways to clarify the picture. A user can reject an interpretation as wrong, say that the evidence is insufficient, or say they do not know; those responses are not treated as interchangeable.

If new context arrives, the previous analysis remains associated with the evidence and assumptions available at the time. A new analysis can explicitly revisit it rather than rewriting history.

The same pattern applies to research, interpersonal communication, decision preparation, and other evidence-rich human situations. Niccolò is **not** a substitute for professional legal, clinical, or other regulated judgment.

## Illustrative adversarial use case

For a detailed example of Niccolò reasoning about humans under conflicting incentives, uncertain provenance, distrusted operators, politically sensitive context, and a consequential decision, see **[The Aurelius & Finch Bonus Review](docs/product/USE_CASE_LAW_FIRM_BONUS_REVIEW.md)** or the **[PDF edition](docs/product/USE_CASE_LAW_FIRM_BONUS_REVIEW.pdf)**.

## Using the product

The intended experience is organized around a Situation, rather than a single isolated prompt:

1. **Start with an intention.** Describe the situation, a concern, or a decision to explore.
2. **Add explicit material.** Reference artifacts where they already live and supply attributed descriptions, transcriptions, assumptions, recollections, or investigative preferences. The MVP accepts no file upload and invokes no parser.
3. **Explore.** Review attributed claims, disagreements, human-authored alternative explanations, their structural impacts, open questions, and the system's proposed next investigative steps.
4. **Steer.** Correct, reject, question, prioritize, and add context whenever useful. A human correction creates a traceable revision rather than silently overwriting the record.
5. **Decide what happens next.** Choose whether to pursue a lead, request more information, revisit an answer, or stop. Review and approval are not automatic permission to execute external actions.

The local web interface is intentionally functional and visually restrained. Some product workflows described above require further integration; see **[Product coverage](#product-coverage-implemented-and-planned)**.

## Architecture

```text
                        Human / first-party web UI
                                    |
                      React application (frontend/)
                                    |
                           HTTP BFF / API
                                    |
              Situation, analysis and operation services
                      /             |              \
          artifact and         bounded agentic       model integration
          reasoning models      step controller        (evolving)
                      \             |              /
                         PostgreSQL / SQLx
                                    |
                      immutable revisions, evidence,
                      decisions, provenance and events
```

- **Core:** Rust; explicit domain models for evidence attribution, provenance, Situation revisions, claims, uncertainty, review, and human control.
- **Persistence:** PostgreSQL with SQLx, workspace-scoped relationships, migrations, and row-level security boundaries.
- **Backend:** Axum HTTP API with a durable operation model. Synchronous, asynchronous, and pull-oriented observation are different ways to interact with the same underlying operation.
- **Controller:** A bounded, versioned graph of investigative steps with dependencies, hard budgets, stop conditions, and human pause/resume boundaries.
- **Frontend:** React, React Router, TanStack Query, Zustand, React Hook Form, and Zod.
- **Model layer:** provider/model interaction and evaluation are independent concerns; the core must not assume that a generated answer is evidence or that a model may authorize action.

Technical details and architectural contracts: [Investigator-assisted MVP](docs/architecture/INVESTIGATOR_ASSISTED_MVP.md), [parser-free external artifacts and parallel observation](docs/architecture/NICCOLO_MVP_EXTERNAL_ARTIFACTS_AND_PARALLEL_OBSERVATION_CONTRACT.md), [MVP composite evidence and human-guided circumstances](docs/architecture/MVP_COMPOSITE_EVIDENCE_AND_HUMAN_GUIDED_CONTEXT.md) (historical context where it conflicts), [MVP_ARCHITECTURE_V250.md](dev-flow/checkpoints/MVP_ARCHITECTURE_V250.md), [COMPETITIVE_EVALUATION.md](docs/product/COMPETITIVE_EVALUATION.md), and the focused engineering documents in the repository.

## First-time setup on one PC

If you want only the shortest user path, follow the **[First-user MVP walkthrough](docs/product/FIRST_USER_MVP_WALKTHROUGH.md)**.

For a **new, dedicated PostgreSQL 17 database**, the MVP provides a first-run
bootstrap that builds the React UI and Rust server, applies SQL migrations,
creates a restricted API login, and issues a 30-day private bearer token.

Prerequisites: Node.js/npm, Rust 1.99.1, PostgreSQL 17 client tools (`psql`),
and a running PostgreSQL server/database that the provided admin account owns.
On Windows, use Git Bash.

Set `NICCOLO_PG_ADMIN_URL` and `NICCOLO_PG_SECURE_PASSWORD` in your shell's
environment (do not include them in git commits or shared logs), then run:

```bash
bash scripts/bootstrap-local-mvp.sh
```

The script refuses to overwrite a previously initialized database or local
credentials. It also refuses to change the password of an existing
`niccolo_secure` PostgreSQL role: **roles belong to the whole PostgreSQL
cluster**, not just a database. Use a dedicated cluster for an independent
installation rather than letting bootstrap silently modify another
installation's service credentials.

The local API listens on loopback port `38080` by default. Set `NICCOLO_API_PORT`
before bootstrap to select a different port. Bootstrap records it in the local
configuration and uses it in the prepared browser URL.

It writes private files under the ignored `.niccolo-local/` folder:
`local.env` (restricted service DB URL), `identifiers.txt` (workspace and
Situation identifiers), and `token.txt` (bearer token). Treat the folder
as sensitive even on Windows filesystems where `chmod` may not enforce ACLs.

For a local OpenAI-compatible model runtime, configure
`NICCOLO_LLM_BASE_URL` (for example `http://127.0.0.1:11434/v1`) and
`NICCOLO_LLM_MODEL` before starting the API. Keyless HTTP is permitted only
on loopback; remote HTTPS providers require `NICCOLO_LLM_API_KEY`. For slower models,
set `NICCOLO_LLM_MAX_TIME_SECS` before starting. Its default is 25 seconds, with a
configurable range of 1 to 120 seconds. Explicit analysis operation budgets still apply.
Use the provider base URL through its version path, for example `/v1`.
Supply credentials through `NICCOLO_LLM_API_KEY` and keep query parameters
and fragments out of the base URL. Some local models require
`NICCOLO_LLM_JSON_MODE=false`.

For repeated local use, those exports may be kept in the ignored private file
`.niccolo-local/model.env`; `scripts/start-local-mvp.sh` sources it
automatically when present. Keep any API key in that private file, never in the
prepared browser URL or repository.

Start the already-bootstrapped local MVP with one command:

```bash
bash scripts/start-local-mvp.sh
```

The helper prints a prepared loopback URL containing only the non-secret
workspace and Situation identifiers. Open that URL, paste the bearer token from
`.niccolo-local/token.txt`, and choose **Connect**. The token is deliberately
never placed in the URL or printed by the start helper.

The prepared URL is also stored in `.niccolo-local/prepared-url.txt`. The
first Situation is then created from the web interface by entering its focus.
This is a local single-PC workflow, **not** an Internet-facing or
production-hardening guide. The setup currently assumes the user has installed
PostgreSQL already.

## Run locally

For an MVP binary containing the **current** React interface, use `bash scripts/build-mvp.sh` after installing the prerequisites below. This command builds `frontend/dist` before compiling `niccolo-api`, whose embedded HTML/JavaScript/CSS are captured at Rust compile time. Running Cargo alone after changing React sources can serve stale UI assets.


### Prerequisites

- Linux environment with Rust/Cargo compatible with this repository (the offline toolchain bundle targets Rust 1.99.1).
- PostgreSQL 17 and its client tools.
- Node.js and npm for the React frontend.
- A database used **only for development/testing**, plus restricted application credentials for API operations.
- Dependencies from the configured Cargo vendor/cache and npm lockfile, or a permitted online installation source.

The backend can be exercised independently of the frontend. Do not use live personal records for initial setup or tests.

### 1. Verify the Rust core

From the repository root:

```bash
cargo test --offline
cargo run --offline --bin niccolo-demo
```

If you have not installed the offline Cargo cache, run the equivalent commands without `--offline` after configuring access to dependencies. The exact test selection requiring PostgreSQL is controlled by the `pg-integration` feature.

### 2. Prepare PostgreSQL

Create a **disposable database** and separate database accounts for migration/administration and restricted runtime access. Apply SQL files from `sql/` **in ascending filename order**, including migrations added after earlier architecture checkpoints.

The API needs a dedicated non-superuser connection that passes its role and authorization checks. The database owner URL must not be supplied to the HTTP process.

Environment variables used by the repository's validation and service tooling:

| Variable | Purpose |
| --- | --- |
| `NICCOLO_PG_ADMIN_URL` | Owner/admin connection for controlled migrations and test fixtures. |
| `NICCOLO_PG_SERVICE_URL` | Restricted legacy service connection used by applicable integration tests. |
| `NICCOLO_PG_SECURE_URL` | Dedicated restricted API connection used by the HTTP service and current validation suites. |
| `NICCOLO_API_PORT` | Optional loopback API port; default `38080`. |

Provisioning roles, schema migrations, and token issuance are security-sensitive setup tasks. Follow the database and API instructions in [HTTP_AUTH_V007.md](docs/reference/HTTP_AUTH_V007.md) and the corresponding newer migration scripts; do not use an administrator URL as the runtime credential.

### 3. Run the API

With the secure runtime database URL configured:

```bash
cargo run --offline --bin niccolo-api
```

The service binds to `127.0.0.1` and defaults to port `38080`.

```bash
curl -i http://127.0.0.1:38080/healthz
curl -i http://127.0.0.1:38080/readyz
```

`/healthz` checks liveness; `/readyz` also evaluates selected database/runtime prerequisites. An HTTP 200 from either endpoint is not a certification of production security.

Workspace operations require a token and workspace-specific authorization. The operator CLI `niccolo-provision` is intended for trusted local provisioning, not public user registration. See [HTTP_AUTH_V007.md](docs/reference/HTTP_AUTH_V007.md) for its command syntax and the documented API authorization boundary.

### 4. Run the React frontend

```bash
cd frontend
npm ci
npm run dev
```

To build and test it:

```bash
npm test
npm run build
```

The development UI and the loopback API are distinct processes. Configure their local connectivity according to your development environment; the React development server alone does not provision a PostgreSQL database or bypass API authentication.

### 5. Validate changes

For Rust formatting, linting, and tests:

```bash
cargo fmt --all -- --check
cargo clippy --offline --all-targets --features pg-integration -- -D warnings
cargo test --offline --all-targets --features pg-integration
```

Integration suites require the configured disposable PostgreSQL database, schema, and appropriate roles. The repository's `scripts/run-v*-validation.sh` scripts reproduce focused historical checkpoints. `scripts/run-v030-validation.sh`, for example, requires the three PostgreSQL URLs listed above. A successful older checkpoint is **not** a substitute for testing all subsequently changed modules and migrations.

The normal `niccolo-api` build and the current `pg-integration` acceptance tests are parser-free. Historical import code requires the independent `legacy-source-import` feature; `legacy-pg-integration` combines it with PostgreSQL fixtures only for deliberately selected compatibility tests while those paths are retired. None of these test features is part of the restricted MVP runtime. The historical `niccolo-source-demo` binary cannot be built without explicit compatibility activation.

For the current restricted-MVP acceptance boundary, run `bash scripts/run-parser-free-mvp-validation.sh`. It checks the default API build, unit tests, PostgreSQL artifact contracts, retired byte routes, browser regressions, and the production bundle as one explicit gate.

## Building on Niccolò

The backend is organized by responsibility rather than by one undifferentiated reasoning prompt:

| Area | Starting points |
| --- | --- |
| HTTP endpoints and entrypoint | `src/api.rs`, `src/api_main.rs` |
| Situation revisions and storage | `src/situation.rs`, `src/situation_store.rs` |
| Parser-free artifact references and representations | `src/artifacts.rs`, `src/reasoning_packet.rs` |
| Historical import compatibility (not MVP runtime) | `src/source_import.rs`, `src/postgres_ingest.rs` |
| Analysis, claims, and review | `src/analysis_*.rs`, `src/review_*.rs` |
| Human feedback, decisions, and authorization | `src/human_*.rs`, `src/action_authorization.rs` |
| Durable operations and agentic orchestration | `src/bff_*.rs`, `src/agentic_*.rs` |
| Database schema | `sql/` sequential migrations |
| First-party web application | `frontend/src/` |
| Behavioral and integration tests | `tests/`, `frontend/src/**/*.test.js`, validation scripts |

When extending the system, preserve the distinctions between *source*, *attributed statement*, *human contribution*, *human-authored hypothesis or assumption*, *system analysis*, and *verified observation*. Preserve historical analysis snapshots, human-originated decisions and workspace boundaries. Tests should assert these invariants, not only happy-path HTTP results.

A useful development sequence is: identify the domain invariant; add a migration/domain contract if required; implement the service/API boundary; add adversarial and authorization tests; exercise the relevant human interaction; update documentation of the public interface.

## Product coverage: implemented and planned

The two categories below are intentionally explicit. **A planned capability belongs in a product README when it explains what Niccolò is meant to become; it must not be mistaken for a working feature.**

### Established engineering foundations in this repository

- Provenance-aware artifact, representation, source, and evidence models, including multi-parent derivations and withdrawal-sensitive traversal.
- Parser-free external-artifact identities, declared storage references, attributed human representations, qualified relationships, immutable Situation revisions, and frozen reasoning packets.
- Runtime and browser boundaries that accept no file upload, expose no PDF preview, retain no new artifact bytes, dereference no locator, and invoke no parser in the restricted MVP.
- Persistent Situations, human contributions, corrections, retractions, and immutable analysis-input snapshots.
- Claims, support relationships, alternative interpretations, uncertainties, review/feedback records, and reanalysis lineage.
- Deterministic Workbench impact analysis for explicitly human-authored hypotheses and what-if assumptions: explicit grounding routes, downstream conclusions, contradictions, and testing questions remain inspectable without becoming truth claims or aggregate scores.
- Optional model draft in the Workbench for a note supplied by the investigator. The investigator can edit objects and connections and explicitly record the reviewed structure. The recorded objects and connections link back to the input, the model proposal and the investigator's revision.
- The Workbench can ask a model to describe selected objects and their relationships in prose. The result links each model-written paragraph to the selected graph material and can be edited and downloaded as text.
- On request, the Workbench can suggest questions or what-ifs from a selected graph, show the cited objects, and let the investigator decide whether to record a proposal.
- Workspace-scoped PostgreSQL persistence, documented bearer authorization boundaries, local HTTP endpoints, and integration tests.
- Durable BFF operations with multiple observation styles; structured, budgeted agentic-controller primitives.
- A first-party React interface and associated client-state/test infrastructure.

These foundations are not equivalent to a complete consumer onboarding, production-ready data security, or end-to-end autonomous analysis service.

### Product direction and planned capabilities

- **Human-centered discovery:** proactive but bounded exploration of a Situation, including two or three promising investigative leads with reasons and evidence gaps.
- **Post-MVP artifact interpretation:** evaluate text files, email, messaging, PDFs, images, audio, and video as external artifact observation layers after the MVP. Drafting from a note supplied directly by the investigator is within MVP scope.
- **Contextual interpretation:** let investigators represent possible motivations, perceptions, misunderstandings, and social dynamics as explicitly human-authored competing hypotheses, never asserted diagnoses or facts about people.
- **Goal-directed reasoning:** select what to investigate next according to the person's purpose, budget, stop conditions, missing evidence, and available tools.
- **Human-guided iteration:** learn from distinct kinds of feedback, preserve provenance and disagreement, and perform selective reanalysis when new information actually changes the question.
- **Model integration and empirical evolution:** evaluate candidate reasoning strategies against reproducible tests, compare model costs and quality, and improve the product without quietly delegating essential authority to a third party.
- **Usable end-to-end experience:** explicit artifact references, attributed representations, inspectable reasoning, informed decisions, and controlled next actions through the product interface.
- **Cohort and operational readiness:** robust authorization, privacy, retention/deletion, observability, deployment, and adversarial validation appropriate to real-world use.

The direction is not a roadmap guarantee or a claim that all these integrations are already active. Engineering checkpoint records and detailed test limitations belong in their respective technical documents, not interleaved with the product introduction.

## Safety and reasoning principles

- **The human owns the decision.** Suggestions, reviews, and approvals are distinct from permission to perform an external action.
- **Attribution before assertion.** A statement about someone's beliefs remains attributed; repeated copies are not independent witnesses.
- **Uncertainty must survive.** The system should say what it does not know, present plausible alternatives, and ask questions when evidence matters.
- **History is not rewritten.** New context can justify reanalysis without retroactively changing what an earlier answer saw.
- **Security must be demonstrated.** Workspace scoping and RLS are components of a security architecture, not a substitute for a reviewed deployment boundary.

## Further reading

- [MVP_ARCHITECTURE_V250.md](dev-flow/checkpoints/MVP_ARCHITECTURE_V250.md) — domain, BFF, controller and frontend architecture.
- [HTTP_AUTH_V007.md](docs/reference/HTTP_AUTH_V007.md) — local API authorization model and development provisioning.
- [SITUATIONS_V016.md](docs/reference/SITUATIONS_V016.md) — Situation state and immutable revisions.
- [COMPETITIVE_EVALUATION.md](docs/product/COMPETITIVE_EVALUATION.md) — benchmarking and candidate algorithm evolution.
- `sql/` and `scripts/` — executable schema and validation assets.
- [Documentation guide](docs/README.md) — current product orientation and technical references.
- [Engineering history](dev-flow/README.md) — historical checkpoints, validation records, and decisions (not current product claims).
