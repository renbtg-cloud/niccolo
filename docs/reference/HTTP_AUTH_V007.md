# Niccolò v0.7 — constrained HTTP boundary and database-backed authorization

This is an offline-verifiable proof of concept, **not a production identity provider**. All test principals/workspaces/sources are synthetic. The service listens on **loopback only** (`127.0.0.1`, configurable port), with no TLS termination, external LLM access, or public tenant onboarding.

> **Current-head note:** this file originated at the v0.7 checkpoint. The current HTTP
> service no longer accepts the legacy `niccolo_app`/service role as its runtime
> boundary. It requires `NICCOLO_PG_SECURE_URL`, membership in `niccolo_api`,
> a non-`SUPERUSER`/non-`BYPASSRLS` login, and four restrictive bearer-aware
> RLS gates on every workspace-scoped table. `/readyz` verifies that policy shape
> before reporting ready. Historical details below are retained where they explain
> how the boundary evolved; current setup should apply **all** migrations in order
> or use `scripts/bootstrap-local-mvp.sh`.

## New source of trust

`sql/0005_authz.sql` adds private `niccolo_auth.principals`, `tokens`, and `memberships`, plus one `SECURITY DEFINER` decision function. An operator separately provisions a **random ~244-bit UUIDv4-derived bearer secret** (`ncl_` plus 64 lowercase hexadecimal characters). Only its SHA-256 digest is persisted; the raw token is returned once and must be delivered securely. The request role receives **no SELECT/INSERT/UPDATE/DELETE** privileges on the authorization tables; it may invoke only the narrow `niccolo_auth.authorize(digest, workspace_uuid, 'read'|'write')` function.

Each data request parses the bearer header, hashes the token and checks the requested capability and workspace in the database before using the existing RLS-scoped SQLx adapter. The UUID in the HTTP path is a **requested resource**, never an authorization claim. The database handles expiration, token revocation, principal deactivation and membership revocation. Writing grants reading; read-only grants cannot write. Token rotation writes the replacement and revokes the old token **in one transaction**, preserving its expiration.

### Threat model and non-claims

- No user-supplied SQL; request values are bound parameters. Neither tokens nor original source bytes are written to application logs.
- An attacker with only an opaque bearer token cannot read or write an ungranted workspace. An attacker with a stolen token *can* exercise its granted capabilities until revoked/expired; use TLS, secure storage and short durations outside the POC.
- **Historical v0.7 gap, closed for the current secure runtime role:** `niccolo.workspace_id` is still transaction-local and mutable, but it is no longer sufficient to open rows through the network-facing database role. The restrictive `niccolo_api` policies additionally require a valid transaction-local bearer digest whose live membership authorizes the same workspace and capability. A leaked database password without a valid bearer credential therefore fails closed. A stolen bearer token or a compromised application process that can observe valid bearer material remains security-sensitive and is **not** claimed to be solved by RLS.
- Authorization is evaluated just before starting a separate data transaction. An access revocation racing this boundary can permit one in-flight request. A production design requires an atomic decision + data-access boundary or explicit revocation guarantees.
- No TLS, distributed rate limiting, key vault, production secrets management, audit pipeline, object store for source bytes, account recovery, email/WhatsApp integrations or real user consent flow. No CORS policy enables browser cross-origin requests by default.
- The PostgreSQL role implementing `SECURITY DEFINER` is a trusted schema owner; do not delegate DDL to an application role. No raw token hash list is exposed to the HTTP service.
- Do not use the helper provisioning CLI against production. It is a privileged operator utility, not a publicly reachable endpoint.

## Real API routes

| Method | Route | Access | Semantics |
|---|---|---|---|
| GET | `/healthz` | public / loopback | Process liveness only |
| GET | `/readyz` | public / loopback | DB reachability; no connection details |
| GET | `/v1/workspaces/{workspace}/graph` | read | Revision, node/edge counts; not raw evidence |
| POST | `/v1/workspaces/{workspace}/assess` | read | `{"report_ids": [uuid, ...]}` (1–64 unique IDs); roots, shared-source context; **no truth/independence claims** |
| POST | `/v1/workspaces/{workspace}/sources/{document}` | write | Raw `.eml` (`message/rfc822`) or WhatsApp text (`text/plain`), `X-Source-Locator`, **≤4 MiB**; `201` newly imported, `200` idempotent retry, `409` conflicting reused ID |
| GET | `/v1/workspaces/{workspace}/sources` | read | Page of metadata only: `limit` 1–100 (default 25), UUID `after` cursor, `next_after`; stable UUID keyset order |

The server does not serve source bytes. Unauthorized data operations fail with generic JSON `401` (missing/malformed token) or `403` (no grant / unknown token), not stack traces. JSON data responses include `Cache-Control: no-store`; the raw parser input is never echoed to the response. `DefaultBodyLimit` rejects overly large upload bodies.

## Reproduce locally (disposable DB only)

For the **current head**, prerequisites are Rust 1.99.1, PostgreSQL 17, Node.js/npm for the web UI, and the repository's locked dependency caches. Use a new dedicated database whose setup owner can create the restricted runtime role. The setup owner need not be `SUPERUSER`.

The supported first-run path is:

```bash
export NICCOLO_PG_ADMIN_URL='postgres://.../new_empty_database'
export NICCOLO_PG_SECURE_PASSWORD='...'
bash scripts/bootstrap-local-mvp.sh
```

That script applies **all current migrations in filename order**, creates the dedicated `niccolo_secure` login, grants it `niccolo_api` membership without `SET ROLE` access to the legacy app role, and writes the local secure runtime URL separately from the administrator URL.

For historical v0.7 regression work only, `NICCOLO_PG_SERVICE_URL` and `scripts/run-v007-validation.sh` still exercise the legacy checkpoint boundary. They are not the connection used by the current HTTP binary.

To run the current server after bootstrap:

```bash
source .niccolo-local/local.env
cargo run --offline --bin niccolo-api
```

Set `NICCOLO_API_PORT` to change the default `38080`; the process deliberately refuses to bind a public interface or start under a PostgreSQL `SUPERUSER`/`BYPASSRLS` login.

To issue one credential, use the **trusted operator console** with a synthetic, existing workspace UUID and principal UUID:

```bash
cargo run --offline --bin niccolo-provision -- mint WORKSPACE_UUID PRINCIPAL_UUID read 7
```

The token is emitted once on stdout; do not put it into source files or logs. Rotate or revoke through the same binary via stdin, not an argument:

```bash
cargo run --offline --bin niccolo-provision -- rotate < /secure/path/to/old-token-file
cargo run --offline --bin niccolo-provision -- revoke < /secure/path/to/token-file
```

These commands require the admin URL. The HTTP process must never receive it.

## Tests executed on disposable Linux/PostgreSQL

- Separate authenticated app role denied direct access to authorization tables.
- Valid, invalid, absent, expired and revoked tokens; disabled principals; revoked membership.
- Read vs. write, cross-workspace UUID collisions, missing documents, duplicate report IDs.
- 32 independent concurrent uploads of the same document (exactly one initial insert).
- Transactionally idempotent source ingestion; rejected altered content; fragment provenance and shared-document markers, with no independence claims.
- Paginated source metadata, no cross-workspace page leaks, bounded page size and payload limits.
- Existing PostgreSQL integration and source parser/adversarial suites from v0.6 remain enabled.
- Live TCP loopback smoke: `401`, `200`, `201`, metadata listing, token revoked after test.

## Follow-on engineering priorities

1. Keep authorization/revocation semantics explicit at transaction boundaries, continue auditing the secure runtime role for least privilege, and test fail-closed behavior whenever new workspace-scoped tables are added.
2. Durable encrypted object storage with deletions, retention limits, explicit user permissions and provenance-preserving re-ingestion.
3. Human-driven identity mapping across sources (no automatic inference of person identity), contradiction discovery as hypotheses, and correction histories.
4. API abuse controls and structured audit events with redaction; fault injection, soak/load benchmarks; production TLS/auth gateway and secret storage.
