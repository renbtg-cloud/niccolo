# Niccolò v0.9 — human steering, attributed disagreement, and source references

**Status:** offline, PostgreSQL-backed experimental implementation, not a production human-reasoning product. No LLM is called, and no assertion of truth, witness independence or agreement is inferred.

## What has been implemented

- `0006_human_identity.sql` and `src/identity.rs` (v0.8): manual document-specific speaker-label → person mapping. `ASSIGN`/`CLEAR` corrections are append-only. The same label in two documents is **not** automatically the same person. All writes record the verified token principal, not a user-supplied actor ID.
- `0007_human_feedback.sql` and `src/human_feedback.rs` (v0.9): explicit human-authored investigative questions and distinct feedback of `rejected`, `insufficient_evidence` or `unknown`. Events preserve actor, reason, chronology and exact retry identity. `GET /positions` returns the *most recent answer per human principal*, including disagreement; it never computes consensus. Earlier answers remain available in the paged history.
- `0008_question_evidence_refs.sql` and `src/question_refs.rs` (v0.9): attributed, append-only references to workspace-scoped existing evidence. Relations are `references`, `challenges`, `follow_up_on`. They do not mutate the original evidence or imply the referenced evidence is true, independent or supporting the proposed question.

All IDs are UUIDs, all text lengths are bounded, and unsupported enum values/unknown JSON fields are refused. PostgreSQL RLS is force-enabled on the new tables. Composite foreign keys prevent cross-workspace question–evidence linking. The application role has SELECT/INSERT on append-only event tables but no UPDATE/DELETE; additional triggers reject rewriting even if grants were accidentally broadened. A replay with the same event ID and changed content or actor is a conflict, rather than last-write-wins.

## HTTP operations (loopback-only demonstration)

All URLs below are prefixed `/v1/workspaces/{workspace_uuid}`. Every operation checks a live opaque bearer token and requested workspace permission in PostgreSQL **before** calling a workspace-scoped adapter. Token management remains separate in `niccolo-provision`.

| Method | Route suffix | Required capability | Behavior |
|---|---|---|---|
| `POST` | `/identities/{person}` | write | Create a user-labeled identity, exact retries reuse it |
| `POST` | `/identity-events/{event}` | write | Assign or clear a document-local speaker mapping, with human reason |
| `GET` | `/documents/{document}/identities` | read | Effective mapping per speaker |
| `GET` | `/documents/{document}/identity-history` | read | Paginated, attributed correction history |
| `POST` | `/questions/{question}` | write | Create human-authored question; identical retries 200, new 201 |
| `GET` | `/questions?limit=25&after=uuid` | read | UUID keyset pagination, 1–100 entries |
| `POST` | `/questions/{question}/feedback/{event}` | write | Append explicitly typed feedback event; actor bound to token |
| `GET` | `/questions/{question}/feedback?limit=50&after=integer` | read | Chronological event history, 1–100 entries |
| `GET` | `/questions/{question}/positions?limit=25&after=uuid` | read | Latest feedback **per actor**, never consensus |
| `POST` | `/questions/{question}/references/{reference}` | write | Append pointer to an existing workspace evidence record |
| `GET` | `/questions/{question}/references?limit=25&after=uuid` | read | UUID-keyset evidence references |

Example feedback body: `{"response":"insufficient_evidence","human_reason":"Need to inspect the original export"}`. Example reference: `{"evidence_id":"<uuid>","relation":"challenges","human_reason":"This may have been copied"}`. A proposed question, annotation or feedback event is **not** evidence of any underlying proposition. Responses explicitly preserve that distinction.

## Test instructions

Install Rust 1.99.1+, vendor Cargo dependencies, PostgreSQL 17, and the project's SQL migrations in filename order `0001..0008` on a **disposable** database. Use two PostgreSQL URLs: `NICCOLO_PG_ADMIN_URL` for the privileged test-fixture owner and `NICCOLO_PG_SERVICE_URL` for a restricted login granted membership in `niccolo_app`. The request-handling service must never use the admin URL.

```bash
bash scripts/run-v009-validation.sh
# Optional real socket test, requires curl, psql and Python3:
PSQL_BIN=psql NICCOLO_API_PORT=38732 bash scripts/run-v009-loopback-smoke.sh
```

On the November 2, 2026 Linux environment, 149/149 Rust tests passed on a newly-created PostgreSQL 17.11 database; `cargo clippy --offline --all-targets --features pg-integration -- -D warnings` and `cargo fmt --check` passed. The separate loopback TCP smoke ran 10/10 checks, including token revocation. The two input-format preview commands also succeeded. Reproduce and update the counts on the target machine rather than treating them as guaranteed performance.

## Critical limitations and explicit security threats

1. This proof of concept authenticates at the HTTP boundary, **then opens a different data transaction**. It does not bind the authorization check and every protected data statement into one inviolable transaction. An authorization revocation can race with a previously authorized request. The workspace scope is currently a mutable PostgreSQL GUC; an attacker with arbitrary SQL execution as `niccolo_service` may override it. PostgreSQL RLS alone is not a defense against SQL injection or stolen DB credentials.
2. Opaque bearer tokens require HTTPS, short-lived credentials, robust secret storage, rotation, rate limiting, network ingress restrictions, logging redaction, CSRF review where browsers are involved, and a threat-model review before public deployment. The server deliberately binds loopback only.
3. Imported `.eml` and WhatsApp formats are intentionally narrow; the original bytes need a separate authorized, durable storage service. Synthetic speaker labels are not identity verification.
4. Historical answers of a revoked principal remain in the immutable audit. Position views are *not* voting, reputation, confidence or evidence aggregation. Mutually inconsistent answers are deliberately displayed separately.
5. No operational retention/erasure, cross-workspace organizational permissions, trusted end-user signup, legal/compliance workflows or complete MVP frontend is implemented. Append-only audit needs a separately designed compliant retention/erasure model before handling live personal data.
6. A foreign key validates existence and ownership, not source correctness, witness independence or testimony truth. No automatic inference of psychological states or credibility.

## Next safe engineering steps

- Transaction-bound, non-forgeable authorization and audit context; remove client-facing or SQL-injection-reachable arbitrary RLS scope mutation.
- Rate limits, request IDs and structured logging with secret and payload redaction.
- Original immutable content storage with bounded MIME parsers, safe imports and a documented erasure strategy.
- Human-in-the-loop review/consent and tests on realistic data that is not private user content; separate blind assessment and calibration of prospective agentic reasoning.
