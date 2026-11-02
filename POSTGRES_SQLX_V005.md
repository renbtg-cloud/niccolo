# v0.5 — PostgreSQL-backed provenance graph (SQLx)

The experiment now uses the **real PostgreSQL 17 schema** (the same `sql/0001_init.sql` and `sql/0002_benchmark_evolution.sql` from v0.4) with a fully offline-built SQLx adapter. No `libpq` FFI is used by the Rust repository.

## Actual behavior

- Connect through a *restricted* PostgreSQL login, never a superuser or a `BYPASSRLS` role. The adapter explicitly refuses those connections.
- Read nodes and multi-parent derivation edges in one `REPEATABLE READ`, **READ ONLY** transaction; set `niccolo.workspace_id` **transaction-locally** before either query. Connection reuse must not carry a previous workspace scope.
- Map database UUIDs to graph-local integer identifiers without confusing them across workspaces. All externally returned roots retain their original UUIDs.
- Missing/invisible reports, withdrawn ancestors, cycles and malformed cross-references fail closed. The graph never claims distinct roots imply independent testimony or true statements.
- Abort instead of returning a truncated graph if the configurable node/edge budget is exceeded (defaults: 50,000 nodes, 150,000 edges).
- The adapter **does not take workspace IDs from HTTP/client parameters**. The *calling application* must authorize and bind workspace IDs from trusted authenticated state; this POC deliberately has no authentication layer.

## Repeat locally (requires a disposable PostgreSQL instance)

Apply migrations as the database owner:

```bash
psql -v ON_ERROR_STOP=1 -d niccolo_test -f sql/0001_init.sql
psql -v ON_ERROR_STOP=1 -d niccolo_test -f sql/0002_benchmark_evolution.sql
```

Create a separate, minimally privileged login for tests, in the **disposable database only**:

```sql
CREATE ROLE niccolo_test_service LOGIN NOSUPERUSER NOBYPASSRLS;
GRANT niccolo_app TO niccolo_test_service;
GRANT CONNECT ON DATABASE niccolo_test TO niccolo_test_service;
```

Then set `NICCOLO_PG_ADMIN_URL` for a privileged fixture writer and `NICCOLO_PG_SERVICE_URL` for that restricted role. For the *offline Cargo vendor*, configure Cargo's source replacement to the transferred phase `AA/vendor` and use the Linux Rust 1.99.1 toolchain from phase AC. Run:

```bash
bash scripts/run-sqlx-integration.sh
```

All test records are synthetic. Tests cover cross-tenant ID collisions, 64 independent concurrent readers on a 3-connection pool, transaction-scope cleanup, RLS without a workspace setting, no write privilege via the read path, superuser rejection, multi-parent DAGs, diamond provenance, stale snapshot behavior, cycle/withdrawal fail-closed, a 1,200-node chain, and graph budget enforcement.

## Deliberate limits

- Read-only graph loading is **not** data ingestion, full authorization, authentication or an HTTP service.
- The custom PostgreSQL scope variable itself is settable by the database login. A compromised query path or SQL injection could impersonate another workspace; a production trust boundary needs stronger enforcement (e.g. authorized session binding with restricted SQL access and security review). **RLS alone is not complete tenant security.**
- Loaded graphs are **snapshots**. Subsequent revocations do not invalidate detached snapshots. Never cache them across authorization or evidence changes; a stronger versioning/invalidation model is required.
- Entire visible workspace is loaded and checked against configured limits rather than traversing a bounded closure. Later versions should use indexed, paginated, iterative ancestor traversal with transaction-specific limits.
- The in-memory graph does not independently verify real-world reliability, identities, manipulation, coordination or truthfulness.
- PostgreSQL role DDL and schema migrations require administrator approval; this module never creates a role in production.
