# v0.17 — frozen analysis-input snapshots

v0.17 adds one small but important concurrency invariant: when a reasoning operation is admitted, it can be bound to one immutable Situation revision before any model or reasoning implementation runs.

`sql/0012_analysis_snapshots.sql` stores an `analysis_id`, Situation, exact input revision, that revision's fingerprint, the human goal, and the authenticated requesting principal. The row is immutable to the application role.

## Why

If an analysis starts at Situation revision S1 and the human adds or corrects context while that operation is in flight, the operation must remain attributable to S1. A retry with the same `analysis_id` also returns the original S1 binding even if the Situation is now at S2 or S3. The retry cannot silently drift to newer context.

## API

`POST /v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}` with `{ "goal": "..." }` creates the binding. An identical replay returns HTTP 200 and the original binding. Reusing the identifier with another goal or another actor returns HTTP 409.

`GET` on the same path returns the frozen binding to readers authorized for that workspace. The response states `frozen_input=true` and `incorporates_future_context=false`.

The endpoint does **not** run a model, store a fabricated model result, claim that a source was analyzed, or decide the still-open durable semantics of rejected machine hypotheses. It establishes the persistence primitive that a later admitted reasoning invocation can safely use.

## Test evidence

Dedicated PostgreSQL tests verify that later human contributions do not modify the frozen input, retries after a Situation change retain the old revision/fingerprint, conflicting replays are rejected, read-only members cannot start an analysis, and 16 concurrent identical starts create exactly one immutable snapshot.
