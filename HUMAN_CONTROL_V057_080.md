# Human control and inspectability — v0.57–v0.80

This experimental slice makes day-to-day reasoning easier to inspect without taking ownership of the user's project.

Highlights:

- bounded keyset pagination for analysis and Situation activity;
- explainable attention reasons with **no automatic priority rank**;
- append-only human pins/unpins and immutable review checkpoints;
- explicit reanalysis requests tied to a concrete steering event; requests and suggestions never start analysis by themselves;
- action proposal, human decision, explicit authorization, revocation and execution kept as separate concepts; this slice contains **no executor**;
- deterministic Situation review export/hash;
- explanations for human context present in frozen input but not considered by one analysis, without calling that context false or remembering a rejection policy;
- byte-bounded links from structured claims to visible answer text;
- traceability/inspection reports that explicitly do not claim correctness;
- acknowledgement of an exact review-package hash without implying agreement;
- deterministic Situation control ledger/hash with explicit governance boundaries.

`MVP-HYP-001` remains open. Nothing here turns human dismissal, exclusion, review, pinning or attention into remembered automatic rejection/deprioritization policy.
