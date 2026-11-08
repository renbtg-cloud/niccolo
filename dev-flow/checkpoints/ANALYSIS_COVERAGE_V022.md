# Niccolò v0.22 — frozen examined-fragment coverage

## Goal

Make truthful source coverage an explicit persistence contract. A source being attached to a
Situation is not equivalent to an analysis having examined it.

## Implemented boundary

- `situation_analysis_examined_fragments` records exact imported evidence fragments that an analysis
  explicitly says it examined.
- Every coverage row is bound to one immutable analysis snapshot and one authenticated recorder.
- A fragment is eligible only when its source document belonged to the analysis's **frozen Situation
  revision**. A source added later cannot be backdated into an older analysis's examined scope.
- `POST .../analysis-snapshots/{analysis}/examined-fragments/{evidence}` is idempotent for the same
  recorder and conflicts if a different authenticated principal tries to claim the same row.
- `GET .../analysis-snapshots/{analysis}/examined-fragments` is bounded and cursor-paginated.
- Returned fragment metadata exposes the document, ordinal and byte range, but does not claim that
  the full document was examined.
- The new table has forced RLS and the same restrictive read/write gates as the existing API tables.

## Deliberate non-goals

This milestone does not infer examination from retrieval, indexing, file attachment, or model
availability. It does not claim that an examined fragment is true, sufficient, representative, or
independently corroborated. Empty coverage means only that no examined fragment was recorded. It
also does not implement unsupported-media coverage states or the unresolved `MVP-HYP-001` semantics.
