# Niccolò v0.21 — visible analysis / reanalysis history

## Goal

Expose the immutable reasoning history of one Situation without reconstructing it from separate
snapshot and output calls, while preserving the temporal boundary established by v0.17-v0.20.

## Implemented boundary

- `GET /v1/workspaces/{workspace}/situations/{situation}/analysis-history` is read-authorized.
- Entries are ordered newest first and include the frozen input revision/fingerprint, goal,
  requesting principal, explicit `reanalysis_of` parent, optional completed answer/coverage note,
  publisher, current Situation revision, and whether the frozen input is still current.
- Historical output remains attached to the analysis that produced it; a later reanalysis does not
  rewrite the earlier answer.
- Pagination is bounded to 1..100 entries and uses an opaque-to-the-client analysis UUID cursor
  (`before`). Unknown or cross-Situation cursors are not accepted as valid pagination anchors.
- Listing a nonexistent/invisible Situation returns the same not-visible boundary as other Situation
  reads instead of presenting an empty history that could blur authorization with absence.

## Deliberate non-goals

This milestone does not rank analyses, declare one answer "correct", infer that stale input means a
wrong answer, or implement the unresolved `MVP-HYP-001` hypothesis-rejection semantics. It does not
call a model. It only exposes already-persisted immutable history in one bounded read model.
