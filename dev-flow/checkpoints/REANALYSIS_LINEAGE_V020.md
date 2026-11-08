# Niccolò v0.20 — explicit reanalysis lineage

v0.20 makes visible reanalysis explicit without rewriting history. A new analysis snapshot may name
one earlier analysis in the same Situation as `reanalysis_of`. The new analysis receives the
Situation revision that is current when it starts; the earlier analysis and any completed output
remain immutable historical artifacts bound to their original inputs.

## Contract

- Reanalysis always uses a new `analysis_id`; replaying the same ID remains idempotency, not reanalysis.
- `reanalysis_of` is optional and immutable once the analysis snapshot is created.
- The parent must already exist in the same workspace and Situation.
- An analysis cannot name itself as its parent.
- Changing the parent on an idempotent retry is a conflict.
- Reanalysis lineage does not declare the earlier answer false, rejected or superseded as a fact.
- No remembered machine-hypothesis rejection/deprioritization semantics are introduced here.

Migration `sql/0014_reanalysis_lineage.sql` adds the nullable self-reference to the existing
forced-RLS analysis-snapshot table. The existing workspace and authorization boundary therefore
continues to apply to lineage reads and writes.
