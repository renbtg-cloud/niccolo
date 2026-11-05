# Niccolò v0.19 — immutable completed analysis outputs

A completed answer can now be persisted against one frozen analysis-input snapshot. The stored
answer and its coverage note are immutable through the application role: only SELECT and INSERT are
granted. Reanalysis uses a new `analysis_id`; an older answer is retained exactly as produced.

`POST /v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/output`
publishes one answer for an existing frozen snapshot. An identical replay by the same authenticated
principal is idempotent; a different answer, coverage note or publisher for the same analysis ID is a
conflict rather than a rewrite. Read-only members may read the historical output but cannot publish.

Responses expose both the frozen `input_revision` and the Situation's current revision. When later
human context advances the Situation, `input_is_current=false`; the earlier answer remains historical
rather than being silently presented as though it used the new context.

The `answer_text` and `coverage_note` storage bounds are POC transport-safety limits, not approved
model token ceilings or mature product quotas. No LLM call is introduced by this milestone.
