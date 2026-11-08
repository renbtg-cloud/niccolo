# v0.29 — Compact human review

The review endpoint is intentionally a read model, not a new reasoning step. One repeatable-read transaction aggregates the analysis goal and lineage, frozen/current revision relation, output presence, source and fragment coverage counts, human-context present-vs-used counts, clarification counts, and structural delta counts.

No count establishes truth, source completeness, psychological state, or whether reanalysis is mandatory. The user remains the project owner and reviewer.
