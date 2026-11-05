# Niccolò v0.23 — truthful per-source analysis coverage

## Goal

Make source coverage visible without turning source presence into an analysis claim.

## Contract

For one immutable analysis snapshot, the API can summarize every document that belonged to that
frozen Situation revision. For each document it reports the number of catalogued text fragments and
the number of those fragments explicitly recorded as examined by v0.22.

`all_catalogued_fragments_examined=true` means exactly that: all fragments present in Niccolò's
source-fragment catalogue for that document were recorded as examined. It does **not** mean the
original source is complete, that unsupported media was interpreted, that deleted content was known,
or that every byte or semantic implication of the source was examined. Consequently
`proves_full_source_examined` remains false. A document with zero catalogued fragments is never
reported as fully examined.

The summary uses the analysis snapshot's frozen source set. A document added to the Situation later
does not appear retroactively in an older summary; a later reanalysis can include it by freezing the
newer Situation revision. Reads remain authorization-scoped and UUID-keyset pagination is bounded.

## Non-goals

This milestone does not infer semantic completeness, truth, relevance, absence from unexamined
content, or unsupported-media coverage. It does not perform retrieval or an LLM call.
