# Frozen-revision observation retrieval (incremental MVP)

## Purpose

This small increment closes the initial reasoning-packet visibility gap: the
model receives at most 256 source fragments and 384 KiB initially, while a
Situation can contain substantially more. A source outside that prefix must
remain discoverable by query without reclassifying it as fact.

## Current implementation

- Read-only endpoint:
  `GET /v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/observations/search?q=<text>&limit=10`
- Authorization uses the existing bearer read guard and transaction-local
  workspace/RLS controls. No cross-workspace or post-snapshot sources.
- The search includes *all eligible imported fragments of the frozen analysis
  revision*, not just the fixed initial reasoning packet.
- `plainto_tsquery('simple', ...)` uses literal AND-style lexical search.
  Queries are bound SQL parameters, bounded to 160 UTF-8 bytes, with a 1-25
  result limit. A GIN index supports source-text full-text matching.
- Results retain original evidence ID, document ID, fragment ordinal, byte
  boundaries, speaker label, locator, and source class.
- Ranking is lexical relevance **only**; source text and ranking are not facts,
  credibility estimates or independent corroboration. Results explicitly
  include these disclaimers.
- Searches neither mutate an analysis nor assert that their results have been
  examined by a model. Request/response retention is left to the investigator
  or a later integrated controller step.

## Outside the current increment

This endpoint is an authenticated human/tool-facing retrieval primitive.
It does **not** yet authorize the model to issue its own source queries inside
the agentic execution loop. It is not semantic or hybrid retrieval, and it does
not calibrate truth or information value. PostgreSQL lexical matching can miss
paraphrases, translations, dates or names with incompatible spelling. A zero
result is not evidence that no relevant source exists.

The next increment should allow a model reasoning step to **propose** specific
queries, perform them under the same authorization and frozen-input
constraints, and record which original fragments were actually included in
the subsequent reasoning input. This may use existing agentic steps and
model-invocation ledgers; no new global ontology is required.

## Acceptance checks

1. A 300-message Situation, frozen before new sources arrive, can retrieve a
   matching message at ordinal >= 256.
2. Sources attached after freezing never appear in the old analysis search.
3. Results are stable, have inspectable original offsets, and are bounded.
4. Nonexistent analysis, invalid queries, unauthorized workspace access and
   missing bearer credentials fail closed.
5. Querying does not insert evidence, create claims or mark fragments examined.
