# Epistemic implementation reconciliation — V1

**Status:** First code/schema reconciliation against the approved epistemological model. This document classifies the current Rust/PostgreSQL implementation as **PRESERVE / GENERALIZE / REPLACE / CREATE**. It records evidence from the actual repository and disposable PostgreSQL schema; it is not a claim that the approved model is already implemented.

Read with:

- [Epistemological model and capabilities](../product/EPISTEMOLOGICAL_MODEL_AND_CAPABILITIES.md)
- [Epistemic implementation contracts — pre-code checkpoint](EPISTEMIC_IMPLEMENTATION_CONTRACTS_CHECKPOINT.md)
- [Epistemic architecture revamp](EPISTEMIC_ARCHITECTURE_REVAMP_IN_PROGRESS.md)
- [MVP composite evidence and human-guided circumstances](MVP_COMPOSITE_EVIDENCE_AND_HUMAN_GUIDED_CONTEXT.md)

## 1. Baseline inspected

The current code baseline was exercised before this reconciliation: all 98 SQL migrations applied to a disposable PostgreSQL 17 database, the Rust test corpus completed successfully, and the frontend test/build baseline completed successfully. Subsequent repository changes before this document were documentation/PDF-generation changes; no Rust, SQL or frontend source architecture changed.

The live disposable schema contains 122 tables in schema `niccolo`.

This reconciliation inspected, among others:

- `src/analysis_claims.rs`
- `src/analysis_structure.rs`
- `src/analysis_assumptions.rs`
- `src/reasoning_updates.rs`
- `src/hypothesis_portfolio.rs`
- `src/identity.rs`
- `src/temporal_state.rs`
- `src/provenance_dag.rs`
- `src/collection_provenance.rs`
- `src/analysis_snapshot.rs`
- `src/clarification.rs`
- `src/clarification_answer.rs`
- migrations `0001`, `0019`–`0028`, `0055`–`0059`, `0080`–`0097`

## 2. Executive result

The implementation does **not** need a general rewrite.

The strongest reusable foundations are:

- immutable Situation revisions and analysis snapshots;
- source/evidence provenance and derivation;
- append-oriented human review and lineage;
- deterministic workspace authorization/RLS;
- clarification answers as attributed user context rather than automatic truth;
- explicit separation of action proposal, human decision, authorization and execution;
- temporal assertions that already distinguish observation/report/attribution/inference;
- bounded reasoning/controller mechanics.

The principal architectural mismatch is that most modern reasoning objects are **scoped to one analysis execution**, while the approved model requires durable semantic identity across analyses.

A second mismatch is semantic collision: current `candidate_interpretations` are effectively **candidate hypotheses/explanations**, while the approved model now also requires **interpretation of human language itself** with confirmation/revision state. These must not be the same abstraction.

A third mismatch is that model-generated structured updates are validated but then written directly into durable reasoning tables. “Not a fact” is already preserved, which is good; however, the new contract also requires **proposal state, progressive formalization and proposal parsimony** before speculative structure becomes canonical durable reasoning state.

## 3. Concrete classification

| Current construct | Classification | Why |
| --- | --- | --- |
| Situation revisions / immutable analysis snapshots | **PRESERVE** | Strong match for temporal separation, frozen inputs, replay and historical reconstruction. |
| Workspace RLS/authz boundary | **PRESERVE** | Correctly deterministic and independent from modeled-world authority/trust. |
| `evidence`, `evidence_derivation`, source fragments, provenance DAG | **PRESERVE + GENERALIZE** | Multi-parent ancestry, withdrawal/cycle handling and provenance are strong. Future grounding must not misuse provenance as epistemic support. |
| Collection/custody/transformation provenance | **PRESERVE** | Good substrate for EvidenceArtifact history. |
| Speaker identity mapping in `identity.rs` | **PRESERVE** | Correct human-confirmed identity mapping with append-only corrections. It is **not** the persistent epistemic-object identity substrate. |
| Legacy workspace-level `propositions` table from migration 0001 | **REPLACE / REHABILITATE** | It already has persistent IDs, but Rust does not use it and semantics are too narrow: `ATOMIC/SAYS/BELIEVES` plus text/actor labels. It is useful evidence that workspace-level semantic identity was anticipated, not a sufficient current implementation. |
| `situation_analysis_claims` | **GENERALIZE** | Good immutable record of what one analysis said, but `claim_id` is analysis-scoped. It should become an occurrence/representation that can link to a persistent semantic proposition/object. |
| Claim reviews | **PRESERVE + GENERALIZE** | Append-only challenge/correction semantics are useful; review dimensions need broader typed Assessment semantics. |
| Claim lineage across direct reanalysis | **PRESERVE + GENERALIZE** | Valuable genealogy, but currently limited to claims and direct reanalysis. It should project onto general object genealogy/revision. |
| Claim-to-claim relations | **REPLACE as primary relation substrate** | Current relation set is only `SUPPORTS/QUALIFIES/TENSIONS_WITH/ALTERNATIVE_TO/DEPENDS_ON`, endpoints are claims only, relation is a string, and everything is analysis-scoped. Preserve data, replace the semantic substrate. |
| Claim-fragment support links | **GENERALIZE** | Useful explicit “support as used / contradicts as used / context only”, but binary evidence-to-claim links cannot express n-ary or alternative grounding. |
| Candidate interpretations | **SPLIT SEMANTICALLY** | Current objects are model-generated competing hypotheses and are correctly marked non-factual/unranked. Keep that role, but do **not** use them as the new “what did this sentence mean?” interpretation abstraction. |
| Hypothesis portfolio | **PRESERVE + GENERALIZE** | Strong non-fact, non-single-winner behavior, status history, predictions and relations. Needs persistent identity beyond one analysis plus richer relation/grounding contracts. |
| Assumptions and alternatives | **PRESERVE + GENERALIZE** | Explicit and non-factual; need persistent contextual roles and adoption semantics. |
| Clarification questions/answers | **PRESERVE + GENERALIZE** | Very useful human steering substrate. Extend it to confirm/correct human-language interpretation without equating confirmation of meaning with truth. |
| Reasoning update ingestion | **REPLACE promotion semantics** | Structured output is bounded and validated, but model proposals are written directly into durable assumptions/gaps/hypotheses/questions. Add proposal/staging semantics and parsimony gates. |
| Temporal actor assertions | **GENERALIZE** | Already separates documentary observation, human assertion and machine hypothesis. But subject/object are text labels, types are limited, and records are analysis-scoped. Link to persistent actors/objects and richer qualified relations. |
| Uncertainty / evidence gaps | **GENERALIZE** | Good explicit unknown/gap history. Needs broader contextual InformationGap identity and resolution assessment. |
| Action proposals / decisions / authorization | **PRESERVE** | Already reflects the approved Recommendation != Decision != Authorization != Execution boundary. |
| Quantitative evidence | **CREATE typed support where needed** | Current reasoning objects are mostly text. Genuine measurements/amounts/log frequencies/statistics need typed values, units, methods and uncertainty without becoming universal epistemic scores. |
| Lawful erasure / redaction / tombstoning | **CREATE** | Current append-oriented semantics and RLS do not provide the approved legal-erasure workflow or dependency-aware tombstones. |
| MVP English-only semantic boundary | **CREATE / GENERALIZE intake** | Storage may retain opaque non-English evidence; structured semantic interpretation must require English in MVP. Current reasoning path does not enforce this boundary. |
| Progressive formalization | **CREATE orchestration contract, not a giant ontology table** | Current model can materialize rich structures, but lacks an explicit demand-driven rule for when to deepen representation. |
| AT-26 weaponized formalism | **GENERALIZE attention/triage** | Existing hard caps and bounded controllers help operationally, but there is no semantic triage of low-value formal objections based on novelty, relevance, discriminatory value and consequence. |

## 4. Important semantic collision: “interpretation” currently means hypothesis

Migration `0081_candidate_interpretations.sql` explicitly says these are “durable pre-synthesis competing interpretations” and that they are not facts, rankings or decisions.

The later `hypothesis_portfolio.rs` then treats those same `interpretation_id` values as hypotheses, with evidence links, predictions, relations and status events.

That behavior is coherent for **candidate explanations of a situation**.

It is not the same object as:

> The current understanding of what a particular human utterance meant.

The new language contract therefore requires a separate abstraction, tentatively:

- original utterance/source occurrence;
- interpretation object/revision;
- interpreter and rationale;
- confirmation state;
- confirmation/correction events;
- downstream dependencies;
- original language preserved;
- MVP semantic parsing allowed only for English.

No migration should simply add `confirmed=true` to `situation_analysis_candidate_interpretations`; that would fuse two meanings of “interpretation” that the approved model deliberately separates.

## 5. Persistent semantic identity: current state

The repository has three distinct identity mechanisms today:

1. **Source/person identity** — `identity.rs`: user-confirmed mapping from source speaker labels to people.
2. **Analysis object IDs** — claims, hypotheses, assumptions, gaps, etc. scoped to a particular `analysis_id`.
3. **Legacy workspace proposition IDs** — migration `0001` `propositions`, presently unused by Rust.

The approved architecture needs a fourth concept to become the shared substrate:

> durable epistemic object identity independent of a particular analysis execution.

The least disruptive direction is a small persistent object registry plus typed tables/projections, **not** a generic JSON/EAV universe.

A plausible shape for implementation reconciliation is:

```text
epistemic_object
  object_id
  workspace_id
  object_kind
  created_by
  created_at

typed semantic table
  object_id -> proposition / interpretation / hypothesis / gap / ...

analysis occurrence/projection
  analysis_id
  local id
  object_id
  representation / role used in that analysis
```

This lets current analysis tables remain useful while semantic identity begins surviving analysis runs.

## 6. Relations: current code is too narrow

`analysis_structure.rs` validates only:

```text
SUPPORTS
QUALIFIES
TENSIONS_WITH
ALTERNATIVE_TO
DEPENDS_ON
```

and the endpoints are two claims in one analysis.

The approved relation model needs typed families with distinct contracts:

- provenance;
- epistemic support;
- dependence;
- semantics;
- causality;
- context;
- actor epistemics;
- institutional relations;
- genealogy;
- composition.

The first replacement should therefore not be “add 40 more strings”.

The new substrate needs relation identity plus typed endpoint constraints, attribution, context, time, rationale/grounding and relation-family validation. Existing claim/hypothesis relation tables can remain as compatibility projections during migration.

## 7. Grounding and selective revision

Current code already contains useful ingredients:

- source links to claims;
- contribution links;
- `DEPENDS_ON`;
- claim lineage;
- clarification impact;
- hypothesis evidence;
- provenance ancestry;
- reanalysis lineage.

What is missing is an explicit structure for:

- jointly necessary grounds;
- alternative independently sufficient grounds;
- complementary or conditional grounds;
- inference identity and applicability;
- selective downstream re-evaluation from a changed dependency.

Therefore **do not discard the existing links**. Generalize them under a grounding/dependency substrate and use current links as migration inputs.

A future invalidated source must weaken only the arguments that actually depend on it; it must not automatically erase a conclusion that also has independent grounds.

## 8. LLM proposal boundary and progressive formalization

`reasoning_updates.rs` is already strong in several ways:

- structured JSON;
- `deny_unknown_fields`;
- bounded counts;
- explicit allowed relation kinds;
- candidate interpretations are not truth;
- assumptions/gaps/questions remain distinct.

The conflict is promotion semantics.

After validation, model output is persisted directly into durable candidate interpretations, assumptions, evidence gaps and clarification questions.

The approved contract now requires:

```text
model output
    -> proposal / intermediate representation
    -> deterministic validation
    -> parsimony / materiality decision
    -> durable canonical reasoning state
```

Not every plausible structure deserves persistence or human review.

Progressive formalization should use concrete triggers such as:

- a live conflict;
- a consequential conclusion;
- an ambiguity that affects reasoning;
- a revision dependency;
- discriminatory value;
- a user question that requires the finer distinction.

The absence of one of those triggers should usually favor a shallower representation.

## 9. Human-language interpretation state

For a human statement, the MVP needs to preserve at least:

```text
original utterance
current interpretation
interpretation status
confirmation status
interpreter / producer
rationale / context
revision history
downstream dependencies
```

Niccolò may reason from an unconfirmed interpretation, but that dependency must remain explicit.

Confirmation means:

> “You understood what I meant.”

It does not mean:

> “What I said is true.”

This capability should reuse the existing clarification/human-feedback patterns rather than inventing a separate approval subsystem.

## 10. First implementation slice

The technically useful first slice is **not** to implement every new noun.

It is to create the shared substrate required by everything else:

1. persistent epistemic object identity;
2. persistent semantic proposition identity or rehabilitation/replacement of the legacy `propositions` table;
3. explicit mapping from analysis-scoped claims/hypotheses to persistent objects;
4. a separate human-utterance interpretation object with confirmation/revision events;
5. typed relation registry/contracts;
6. grounding/dependency primitives sufficient for n-ary and alternative support;
7. selective revision hooks;
8. proposal-state boundary for LLM-generated structures.

Existing provenance, snapshots, RLS, clarifications and action-governance mechanisms should be reused.

## 11. Migration strategy

Prefer additive migrations and compatibility projections.

Do not rewrite the first 98 migrations.

The next migration sequence should begin at `0098` and:

- introduce new substrate without deleting existing tables;
- backfill only where semantics are unambiguous;
- leave ambiguous legacy analysis claims analysis-scoped rather than inventing equivalence;
- preserve all existing IDs and provenance;
- add explicit mapping tables before replacing read paths;
- migrate behavior in Rust incrementally;
- keep old projections until contract tests demonstrate replacement behavior.

## 12. First executable contracts to derive

Before or together with implementation, convert these into tests:

1. two analysis claims may map to one persistent proposition without merging their occurrences, provenance or credibility;
2. repeated wording does not imply semantic identity;
3. a user-confirmed interpretation confirms meaning, not truth;
4. an unconfirmed interpretation may support reasoning but keeps a visible provisional dependency;
5. correcting an interpretation identifies downstream conclusions requiring review;
6. a model proposal cannot silently become a human-confirmed interpretation;
7. receiving a document does not imply reading/comprehension/belief;
8. n-ary joint support cannot be reconstructed as several independently sufficient binary edges;
9. provenance ancestry does not become epistemic support;
10. a lawful tombstone can force downstream reassessment without retaining erased protected content;
11. a non-English source may be stored but is not semantically parsed by the MVP;
12. AT-26: a formally valid low-value objection remains representable without automatically blocking consequential progress.

## 13. Decision

The architecture is not “wrong and due for replacement”.

It contains substantial durable engineering that should survive.

The necessary change is a **semantic substrate refactor around persistent identity, typed relations, grounding and interpretation state**, followed by incremental migration of the existing analysis-scoped objects.

The next code step should therefore start with migration `0098` and Rust types for persistent epistemic identity and interpretation state, while preserving the current 98-migration baseline and existing external behavior until new contract tests are green.
