# Niccolò v0.18 — materialized frozen analysis input

A stored analysis snapshot can now be materialized back into the exact immutable Situation revision
that the analysis was allowed to see. The API returns the frozen focus, active contributions, source
document IDs and fingerprint together with the Situation's current revision.

This makes the temporal boundary visible rather than implicit: if the human adds context later,
`input_is_current` becomes false while the frozen input remains unchanged. The materialized input
verifies that the historical Situation fingerprint equals the fingerprint copied into the analysis
snapshot. A mismatch fails closed as an integrity error.

`GET /v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/input`
requires read authorization and never claims that future context was incorporated.

This milestone does not invoke a model, infer truth, or decide the still-open `MVP-HYP-001`
hypothesis-rejection semantics.
