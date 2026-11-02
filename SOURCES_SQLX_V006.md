# v0.6 — source import, original-byte provenance, graph revisions, document overlap

Experimental Rust + PostgreSQL backend. Tests use **only synthetic fixtures**.
This is NOT production-ready ingestion or a complete Duhme human-context model.

## What is implemented

1. `src/source_import.rs` parses **bounded UTF-8, plain-text** `.eml` and two common WhatsApp text-export headers. It attributes messages to **unverified labels**, keeps original half-open byte offsets (including UTF-8/CRLF), and computes a SHA-256 of the original raw bytes. Continuation lines stay with their message; unknown preamble is rejected. No LLM is involved.
2. Parsers *reject* MIME multipart, HTML, base64, quoted-printable, folded email headers, unsupported MIME parameters and non-UTF-8 content. This is intentional fail-closed behavior, NOT complete MIME/RFC or WhatsApp-format coverage.
3. `src/postgres_ingest.rs` inserts source metadata, evidence and source fragments in **one PostgreSQL transaction**, scoped using transaction-local RLS. It rejects privileged DB logins, verifies exact replay contents when `document_id` already exists, and locks the workspace record during concurrent imports. Replaying the same `(workspace_id,document_id)` cannot duplicate fragments or increment the graph revision. A conflicting reuse of the same document ID fails closed.
4. `sql/0003_sources_and_revisions.sql` adds `source_documents`, `source_fragments` and `graph_revisions`. Evidence and derivation changes increment the revision using triggers, even for SQL changes made outside Rust. Every detached graph snapshot reports the revision captured in its read-only, repeatable-read transaction; it must NOT be treated as automatically refreshed.
5. `sql/0004_source_integrity.sql` adds database-side byte-range bounds, overlap rejection, and immutable document metadata. Its scoped `FOR UPDATE` lock requires a narrowly granted `UPDATE(byte_length)` privilege; the immutability trigger rejects actual value changes. This was validated using the restricted DB role, including malicious direct SQL attempts.
6. Graph assessment now returns both **shared provenance roots** and **shared source documents** independently. Distinct messages imported from the same chat are separate roots but show a common uploaded source. Copies derived from a message inherit its source-document association. Neither common documents nor separate roots prove collusion, independence, identities, or factual accuracy.
7. `niccolo-source-demo` prints digest, byte count and offsets from a local file, **not** the message contents or names.

## Limits and security requirements

- No HTTP endpoints, authentication, verified user identity, attachment parsing, MIME nesting, general character encodings, real WhatsApp account verification, streaming import, object storage integration, privacy lifecycle, or production deployment.
- The POC stores `evidence.content` as plain-text fragments in PostgreSQL. `source_documents` stores only source **metadata and SHA-256**, not a copy of the original source artifact. The original byte stream must be retained separately with controlled access if byte-range verification is required later.
- The app login can set the PostgreSQL custom setting `niccolo.workspace_id`; this **is not a secure client-to-tenant authorization boundary**. Caller must derive the workspace from authenticated, authorized server state and prevent arbitrary SQL access. The POC rejects superuser/BYPASSRLS but is not safe as a directly exposed public service.
- Max original file 4 MiB, max 10,000 extracted fragments, max fragment 64 KiB. The import writes fragments individually, not optimized for very large source ingestion. The graph adapter loads the whole visible workspace subject to resource ceilings, not a paged closure.
- Mail `From` and WhatsApp speaker labels are **claims in the source**, not verified identities or timestamps. A syntactically valid export can be forged.
- No cross-workspace document linkages are inferred. Two document IDs with identical SHA are not automatically coalesced; the supplied `document_id` is the idempotency key within the workspace. SQL checks and triggers enforce boundaries but do not attest the truth of source bytes or third-party provenance.
- `graph_revisions` bumps on evidence/derivation mutations. Source metadata-only writes do not change the graph and therefore do not bump this revision. Reusing a detached snapshot after revisions or authorization changes is unsafe.

## Reproduce in a disposable PostgreSQL 17 database

With the database-owner `psql` pointed at a **fresh, disposable** database, apply in order **exactly once**:

```bash
psql -v ON_ERROR_STOP=1 -f sql/0001_init.sql
psql -v ON_ERROR_STOP=1 -f sql/0002_benchmark_evolution.sql
psql -v ON_ERROR_STOP=1 -f sql/0003_sources_and_revisions.sql
psql -v ON_ERROR_STOP=1 -f sql/0004_source_integrity.sql
```

Have a separate **non-superuser, non-BYPASSRLS** restricted service login with `niccolo_app` membership. Create/grant this login only in a disposable test setup and authorize it according to local database administration policy. Set `NICCOLO_PG_ADMIN_URL` and `NICCOLO_PG_SERVICE_URL` to the **same disposable database** with the owner and restricted logins, respectively.

```bash
bash scripts/run-v006-validation.sh
```

For offline Rust operation, configure Cargo's vendored sources to the AA package from the dependency courier (see `.cargo/config.toml.example`), and use the Linux Rust 1.99.1 toolchain from AC.

## Measured on the Linux development VM (2026-11-02 UTC−3)

- **0/89 tests failed; 89/89 passed**: 65 library/unit cases, 10 earlier PostgreSQL graph cases, 14 new ingestion/DB cases.
- 4,096 deterministic adversarial input mutations × 2 text parser formats (8,192 parse attempts) with panic-safe byte-range assertions. This is NOT exhaustive fuzzing.
- 24 competing replays of the same document: one first insert, 23 identical idempotent replays.
- 96 parallel distinct source imports across eight workspaces on a 3-connection service pool.
- An entirely fresh PostgreSQL 17.11 test database was migrated through 0001–0004; all 89 tests passed again.
- Rustfmt and Clippy with `-D warnings` passed. Synthetic `.eml` and WhatsApp fixture parsing was run in real binaries.

## What comes next

Trusted identity and authorization binding, original-source storage lifecycle, mature format parsers, evidence correction/audit, egress/permissions integration, scalable closure traversals, and independent reality-grounded evaluation. Do not market this as completed production human-context reasoning.
