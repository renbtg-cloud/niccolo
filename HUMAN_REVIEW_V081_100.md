# Human review workflow — v0.81–v0.100

This experimental slice makes human review explicit and inspectable without transferring project ownership to the system.

## v0.81–v0.90

- explicit human-owned review sessions with append-only end records;
- immutable notes and bookmarks inside a review session;
- deterministic session digests that do not imply approval;
- explicit rationale for human context actually used by an analysis;
- explicit rationale for source fragments actually examined;
- claim-linked clarification questions with answer-state reporting;
- per-claim revision conditions describing what could justify revisiting a claim;
- append-only condition events (`MET`, `NOT_MET`, `REOPENED`);
- reanalysis candidates are suggestions only and never start work automatically;
- deterministic reasoning handoff bundle and SHA-256 with no delegated authority or execution semantics.

## v0.91–v0.100

- append-only human-selected review-scope history and a derived current scope;
- bounded deterministic claim pagination;
- exact-hash handoff receipts that do not mean agreement or delegated authority;
- comparison restricted to actual direct reanalysis relationships;
- explicit `DROPPED` lineage distinguished from missing lineage;
- measurable lineage completeness that explicitly does not mean correctness;
- unranked human-pending read model for questions, revision conditions, uncertainties and action proposals;
- session-local dispositions validated against the analysis actually under review;
- session dispositions never become remembered future policy;
- deterministic review agenda and integrity hash with explicit non-ranking / non-execution boundaries.

## Deliberate boundary

This slice does **not** implement the open `MVP-HYP-001` semantics. A human choice made during one review session is not silently converted into a persistent future rejection, deprioritization, or hidden policy.
