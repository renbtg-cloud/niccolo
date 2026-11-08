# v0.16 — persistent Situation revisions

v0.16 moves the bounded Situation semantics introduced in v0.14 into PostgreSQL without expanding the unresolved hypothesis-rejection policy.

## What is persisted

- one workspace-scoped Situation with a current focus;
- source documents explicitly added to that Situation;
- exact human contributions classified as `USER_REPORTED`, `HYPOTHETICAL`, or `INVESTIGATIVE_PREFERENCE`;
- authenticated author principal for every contribution;
- optional relationship from a contribution to one existing evidence item;
- correction and retraction state without rewriting original contribution text;
- immutable revision rows containing the focus, active contribution set, source-document set, actor that caused the revision, and a SHA-256 fingerprint over the exact active state.

Historical revisions keep their original source/contribution membership even after later additions, corrections or retractions. The API therefore can show what an earlier reasoning step could have seen instead of silently projecting the current state backward.

## HTTP boundary

All routes remain loopback-only POC routes behind the existing bearer-token and restrictive RLS boundary.

- `POST /v1/workspaces/{workspace}/situations/{situation}` — create idempotently.
- `GET /v1/workspaces/{workspace}/situations/{situation}` — current frozen snapshot.
- `POST .../focus` — change current focus and create a revision.
- `POST .../sources/{document}` — add an already-imported source document and create a revision.
- `POST .../contributions/{contribution}` — append human context / WHAT-IF / investigative preference.
- `POST .../contributions/{old}/corrections/{replacement}` — preserve the old contribution and replace it in later snapshots.
- `POST .../contributions/{contribution}/retract` — withdraw it from later snapshots without erasing history.
- `GET .../revisions` and `GET .../revisions/{revision}` — bounded historical access.

Every contribution returned inside a revision explicitly carries `is_verified_evidence=false`. `HYPOTHETICAL` remains a separate kind; snapshotting never promotes it to fact.

## Database boundary

Migration `sql/0011_situations.sql` creates six workspace-scoped tables. Every one has `FORCE ROW LEVEL SECURITY` and the four restrictive `niccolo_api` policies for SELECT/INSERT/UPDATE/DELETE, even where SQL grants intentionally do not permit the operation. Original contribution text, author, kind and related evidence are insert-only for the application role; only status/correction metadata can be updated.

## Test evidence

Dedicated PostgreSQL tests cover idempotent replay/conflict, incremental sources, historical snapshots, user-reported versus hypothetical context, correction/retraction, focus changes, read-only membership, 24 concurrent identical writes, bounded revision pagination, actor attribution and RLS policy completeness.

This does not implement model reasoning, automatic hypothesis lifecycle, calibrated probabilities or mature dependency blast-radius repair.
