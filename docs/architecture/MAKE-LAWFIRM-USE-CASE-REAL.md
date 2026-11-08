# MAKE-LAWFIRM-USE-CASE-REAL

## Purpose

This file is an implementation directive for bringing the Niccolò MVP up to the behavioral contract illustrated by [The Aurelius & Finch Bonus Review](../product/USE_CASE_LAW_FIRM_BONUS_REVIEW.md).

The public law-firm use-case files describe **what the MVP can do**. They must not contain implementation-gap commentary, TODO lists, caveats about missing code, or "future roadmap" language for the capabilities exercised by that story. All such implementation work belongs here.

The target is strict:

> **The Niccolò MVP must be capable of supporting every material behavior in the Aurelius & Finch story without relying on hand-waving, hidden operator judgment, or a one-off prompt that bypasses the product model.**

This is not a request to make Niccolò a law-firm product. The law-firm story is an adversarial acceptance case for the general human-context reasoning engine.

The implementation should preserve Niccolò's existing principles:

- sources, human contributions, machine hypotheses, reviews, and verified observations remain distinct;
- authentication and authorization never imply truth;
- human credence never becomes documentary evidence;
- repeated or transformed material never becomes independent corroboration merely by repetition;
- historical analysis remains immutable and reanalysis creates lineage rather than rewriting history;
- the human retains consequential decision authority;
- a privileged database/runtime identity is not an epistemic authority;
- workspace scoping and RLS remain mandatory;
- claims about motives, beliefs, coordination, political influence, or wrongdoing remain hypotheses unless evidence justifies stronger language.

## Instructions for GPT-5.6 High

Treat this document as an engineering acceptance specification, not brainstorming.

Before changing code:

1. Read the root `README.md`, the public law-firm use case, `docs/README.md`, and the current architecture/reference documents that touch the subsystem being modified.
2. Inspect current executable code and tests before trusting older checkpoint documents.
3. Preserve existing interfaces unless the new capability requires a deliberate versioned change.
4. Prefer small composable domain objects and explicit invariants over a giant new "law firm" module.
5. Do not hard-code Aurelius & Finch names, legal terminology, compensation logic, or law-firm-specific behavior into the reasoning core.
6. Every new persistent concept must be workspace-scoped, RLS-protected, auditable, and tested against cross-workspace access.
7. Every new inference-facing structure must explicitly state whether it is evidence, human input, machine inference, configuration, or provenance metadata.
8. Do not solve a missing capability by giving the LLM a longer prompt and pretending the domain model now supports it.
9. Do not add a probability-of-truth field unless there is an actual calibrated probabilistic model. Existing heuristic scores must remain labeled as heuristics.
10. Add adversarial tests. The law-firm story is intentionally about contaminated, contradictory, selected, and possibly manipulated inputs.
11. Run the full relevant Rust/PostgreSQL/frontend validation, not only a new unit test.
12. Keep the public law-firm Markdown/PDF free of implementation-status language. If implementation reality changes, update this file.

Current implementation areas likely involved include:

- `src/analysis_export.rs`
- `src/presentation_package.rs`
- `src/source_catalog.rs`
- `src/source_import.rs`
- `src/postgres_ingest.rs`
- `src/provenance_dag.rs`
- `src/provenance_clusters.rs`
- `src/grounded_lineage_store.rs`
- `src/identity.rs`
- `src/situation_store.rs`
- `src/analysis_snapshot.rs`
- `src/analysis_assumptions.rs`
- `src/analysis_claims.rs`
- `src/analysis_timeline.rs`
- `src/inspection.rs`
- `src/reasoning_packet.rs`
- `src/investigation_policy.rs`
- `src/proactive_briefing.rs`
- `src/agentic_policy.rs`
- `src/agentic_runtime.rs`
- `src/adversarial_challenge.rs`
- `src/human_credence.rs`
- `src/epistemic_policy_store.rs`
- `src/control_ledger.rs`
- `src/reanalysis_runtime.rs`
- `src/reanalysis_revision.rs`
- `src/api.rs`
- `frontend/src/`
- `sql/`

There are currently two migrations beginning with `0084_`. Do not rename historical migrations casually. Add new migrations using the next unambiguous sequence number and preserve deterministic migration ordering.

---

# Acceptance map

The law-firm story demands the following additions or expansions. Implement them as general Niccolò capabilities.

## LF-001 - Full-fidelity forensic Situation export

### Problem

The current portable analysis/presentation exports are intentionally bounded views of a published analysis. They are not a complete forensic representation of a Situation.

The Aurelius & Finch case requires a CEO to export the existing Niccolò state because she does not trust the deployment, operator, or IT environment, then carry enough information into a fresh environment to examine the old system itself.

### Required capability

Introduce a versioned **forensic Situation export** that can include, under explicit authorization:

- Situation metadata and revisions;
- source-document metadata;
- retained original source bytes where available;
- source fragments and exact locators/byte ranges;
- source lineage/provenance edges;
- identity subjects and all identity assertion/correction/clear events;
- human contributions and their authorship/provenance;
- analysis snapshots and fingerprints;
- claims and claim links;
- assumptions and alternatives;
- uncertainties and their events;
- evidence gaps;
- clarifications and answers;
- conflict sets;
- human credence;
- goals, priorities, constraints, stop conditions, source requests;
- action proposals/decisions/authorizations;
- model invocation metadata and model-produced artifacts that are part of retained state;
- challenge artifacts;
- reanalysis lineage;
- reviews and review events;
- configuration-origin metadata relevant to reasoning;
- control ledger/activity history;
- manifests and hashes;
- explicit inventory of omitted/unavailable material.

The export must distinguish payload from provenance. It must not claim that including an artifact authenticates its pre-ingest history.

### Suggested design

Create a new module rather than overloading `analysis_export.rs`, for example:

- `src/situation_forensic_export.rs`

Use a top-level format such as:

- `niccolo.situation-forensic-export`
- `format_version: 1`

Include a deterministic manifest whose digest covers every included logical object and every included binary payload digest.

For large retained originals, allow a package/directory representation rather than forcing everything into one JSON string.

### Required tests

- deterministic export of the same database state;
- cross-workspace isolation;
- exact source-byte preservation;
- missing original is represented as missing, not silently omitted;
- prior model outputs are exported with machine provenance;
- identity events preserve actor and event order;
- manifest changes when any exported logical item changes;
- export does not mutate the Situation;
- export from a privileged/superuser runtime role is rejected under the existing security model.

---

## LF-002 - Untrusted re-import and epistemic quarantine

### Problem

A fresh Niccolò must not inherit the epistemic assumptions of the old Niccolò merely because the imported package is structurally valid.

The public story requires the old system itself to become evidence under examination.

### Required capability

Add **Untrusted Re-Import Mode**.

On import:

- preserve original object lineage;
- assign fresh local identifiers where collision safety requires it;
- retain stable imported-object identifiers in provenance metadata;
- mark imported machine claims, human assertions, identity mappings, configuration assertions, and prior analyses as imported historical state;
- do not silently convert imported state into current trusted configuration;
- do not treat imported identity mappings as verified merely because they existed before;
- do not make imported model outputs documentary evidence;
- do not automatically adopt imported epistemic policies;
- do not automatically adopt imported trust-like operator instructions;
- retain the ability to inspect the old reasoning and compare it with fresh reanalysis.

### Suggested model

Add an import-batch entity such as:

- `forensic_import_batches`
- `import_batch_id`
- source package digest
- imported_at
- imported_by
- declared_origin
- verification status
- import mode = `UNTRUSTED`

Add origin metadata to imported objects or a generic object-origin relation capable of expressing:

- native current-state item;
- imported human assertion;
- imported machine output;
- imported configuration assertion;
- imported source artifact;
- imported identity assertion.

Avoid one Boolean called `trusted`. Trust is not a scalar property of a person or package.

### Critical invariant

> Structural validity of an imported package proves only structural validity. It does not establish truth, authenticity of pre-export history, completeness, independence, or credibility.

### Required tests

- importing the same package twice is idempotent or produces explicitly separate batches without accidental duplication;
- imported LLM output cannot satisfy documentary-evidence foreign keys as though it were an original source;
- imported identity mapping is visibly historical and reviewable;
- imported user assertion "trust me, I am the CEO" remains an assertion;
- current credentials do not upgrade imported statements to facts;
- prior machine repetitions retain shared lineage to their original input;
- reanalysis can use imported material while still showing quarantine provenance.

---

## LF-003 - Source-collection provenance

### Problem

Current evidence provenance primarily answers where a record came from or how records derive from one another. The law-firm case also requires investigation of **the collection process**.

Elias may have selected, summarized, transformed, or omitted material. IT may have exported a subset. An operator may have imported a derived summary.

### Required capability

Model the roles involved in getting an artifact into Niccolò:

- original author/speaker;
- custodian;
- collector;
- interviewer;
- transcriber;
- summarizer;
- transformer;
- exporter;
- importer;
- attester;
- operator.

These are not necessarily the same person.

A source must be able to carry a chain such as:

> PERSON_A said X -> Investigator Elias interviewed PERSON_A -> Elias wrote notes -> IT exported the notes -> Julian imported them -> old Niccolò summarized them -> fresh Niccolò re-imported the prior output.

### Suggested design

Add general provenance events or custody/collection assertions, not law-specific fields.

Possible structures:

- `source_collection_events`
- `source_transform_events`
- `source_attestations`

Each event should include actor, operation kind, input object(s), output object, timestamp if known, declared method, and human rationale/notes.

Uncertainty about the event itself must be representable.

### Required tests

- collector is never inferred to be original author;
- summarizer output remains derived from source notes;
- multiple summaries from one interview do not become independent witnesses;
- a collector's narrative can be investigated like any other human contribution;
- absence of collection metadata remains "unknown", not "clean chain of custody."

---

## LF-004 - Missingness and collection-scope model

### Problem

The story requires Niccolò to distinguish:

- "we looked and found nothing";
- "we never collected that source";
- "the source should exist but is missing";
- "retention deleted it";
- "the export excluded it";
- "we do not know whether it exists."

Current evidence gaps/source requests are useful but are not sufficient as a complete collection-scope model.

### Required capability

Persist explicit source-scope assertions:

- expected source family;
- collection status;
- availability status;
- reason if unavailable;
- declared date/time/person/matter scope;
- collector;
- known exclusions;
- source inventory against which completeness can be compared.

Recommended statuses should be explicit and non-overlapping, for example:

- `EXPECTED_NOT_REQUESTED`
- `REQUESTED_PENDING`
- `COLLECTED`
- `KNOWN_UNAVAILABLE`
- `DECLARED_OUT_OF_SCOPE`
- `UNKNOWN_EXISTENCE`

Do not use missingness as proof of sabotage.

### Required tests

- missing source does not create a misconduct claim;
- incomplete export is visible to reasoning;
- current retrieval cannot report "no evidence exists" when the relevant source family was never collected;
- source-scope changes create history rather than retroactively editing an old analysis snapshot.

---

## LF-005 - Temporal human-state model

### Problem

The current audit timeline records system events. The law-firm case requires a different thing: reasoning about **human state at a time**.

Examples:

- what Helena knew before bonus approval;
- whether Adrian knew the exact compensation cutoff;
- whether a counterparty knew an internal threshold;
- whether Camille had settlement authority on a particular date;
- whether a role or relationship changed before an action.

### Required capability

Add explicit, qualified temporal assertions about humans and relations.

Minimum useful categories:

- role/position;
- formal authority;
- access to information;
- communication received/sent;
- declared belief;
- attributed belief;
- inferred likely awareness;
- relationship state;
- decision participation.

Every item needs:

- subject;
- predicate/type;
- valid/effective interval or event time where known;
- provenance;
- assertion kind;
- epistemic qualification;
- created-by provenance;
- whether it is documentary observation, human assertion, or machine hypothesis.

Do not collapse "appears to have known" into "knew".

### Suggested modules

- `src/human_state.rs`
- `src/human_relation_timeline.rs`

The reasoning packet should expose bounded temporal state relevant to the current hypotheses.

### Required tests

- later information cannot automatically be projected backward;
- conflicting temporal assertions coexist;
- corrections do not rewrite old analysis input;
- the same person can have different roles/authority in different intervals;
- model-facing packets identify hypothesis vs observed communication.

---

## LF-006 - Formal authority vs practical influence

### Problem

The public case repeatedly distinguishes who formally had authority from who actually shaped a decision.

This is not a synonym for hierarchy.

### Required capability

Represent at least:

- formal decision authority;
- advisory role;
- initiator;
- recommender;
- veto/constraint;
- practical influence hypothesis;
- final decision actor.

Influence must be claim-like and evidence-linked; do not turn network centrality into motive or authority.

Support questions such as:

- Who first proposed the settlement range?
- Who could approve it?
- Who appears to have changed the client's position?
- Did a relationship partner drive the outcome while another partner owned the matter?

### Required tests

- formal authority can differ from inferred influence;
- influence inference requires explicit support/qualification;
- multiple influencers can coexist;
- absence of formal authority does not imply absence of influence;
- high communication volume alone does not establish influence.

---

## LF-007 - First-class competing hypotheses

### Problem

Candidate interpretations exist, but the law-firm case needs hypotheses to become durable investigation objects with explicit competition.

### Required capability

For each live hypothesis, persist:

- hypothesis text;
- scope;
- creator (human/model);
- status;
- supporting claim/evidence links;
- contradicting claim/evidence links;
- assumptions required;
- predictions/expected observations;
- unresolved discriminators;
- related hypotheses;
- whether hypotheses can coexist.

Avoid forced single-winner semantics.

Support a decomposition such as H1-H8 from the public case without hard-coding those hypotheses.

### Suggested module

- extend `analysis_assumptions.rs` / candidate interpretations, or introduce `analysis_hypotheses.rs` if the existing semantics would become distorted.

### Required tests

- mutually compatible hypotheses can remain live;
- one evidence item can support one hypothesis and contradict another;
- a hypothesis can weaken without being deleted;
- hypothesis status does not establish factual truth;
- machine-generated hypothesis remains machine-generated after human review unless the human creates a distinct assertion.

---

## LF-008 - Hypothesis-directed retrieval

### Problem

This is one of the largest behavioral gaps.

The current reasoning packet bounds source material. Bounded context is good, but simple deterministic truncation is not an investigation strategy.

The MVP must be able to ask:

> If H1 were true, what evidence would we expect to see, and where should we search for it?

and:

> What evidence would most strongly challenge H1?

### Required capability

Implement iterative retrieval driven by the active investigation.

A minimal acceptable loop:

1. choose a live hypothesis or discriminator;
2. derive one or more retrieval intents/predicted observations;
3. search authorized source fragments and structured human state;
4. return bounded candidates with provenance;
5. record why each candidate was retrieved;
6. mark examined candidates;
7. update hypothesis support/counterevidence/gaps;
8. repeat within explicit operation budget.

### Retrieval strategy

Do not assume vector similarity alone is sufficient.

Use a hybrid of the data Niccolò already owns:

- lexical/full-text matching;
- person/identity constraints;
- source/document constraints;
- timestamps/time windows;
- provenance ancestry;
- claims and related objects;
- structured role/relationship filters;
- optional embeddings only if introduced deliberately and with clear lifecycle/security rules.

For the first MVP implementation, PostgreSQL-native indexing plus model-generated retrieval intents may be enough. Do not add a vector database merely because RAG is fashionable.

### Important runtime change

`reasoning_packet.rs` should stop being the only path by which source fragments reach model reasoning. It can remain a canonical bounded packet, but agentic steps must be able to execute controlled retrieval cycles and record what was retrieved.

### Required tests

- relevant evidence outside the first packet window can be found;
- retrieval reason is stored;
- retrieval cannot cross workspace boundaries;
- a hypothesis can request counterevidence;
- provenance-dependent duplicates are recognized;
- retrieval budget terminates;
- malicious source text cannot request broader authorization or change the retrieval policy.

---

## LF-009 - Discriminating-question selection / information value

### Problem

Current investigation scoring is intentionally heuristic and largely keyed to item type. The law-firm case requires questions chosen because their possible answers separate live hypotheses.

### Required capability

For a candidate question/source request, Niccolò should be able to record:

- which hypotheses/claims it discriminates;
- possible answer classes or observation outcomes;
- what would change under each outcome;
- why the question is valuable now;
- cost/effort if supplied;
- whether a cheaper source can answer the same discriminator.

The MVP does not need a mathematically calibrated Bayesian information-gain engine. It does need more than fixed constants such as "conflict = 90".

### Suggested approach

Add a structured discriminator object and let the model propose bounded answer branches. Evaluate value using transparent heuristics such as:

- number/importance of live hypotheses affected;
- whether outcomes point in materially different directions;
- current uncertainty/gap severity;
- source acquisition cost;
- redundancy with already answered questions.

Label all scores as scheduling heuristics.

### Required tests

- a question that cannot change any live hypothesis ranks low;
- a question that distinguishes H1/H2 ranks above a merely interesting question, all else equal;
- costs can demote an expensive low-value request;
- ranking never becomes truth probability;
- human can override ranking.

---

## LF-010 - Prior machine-output lineage across export/import/reanalysis

### Problem

The public case requires prior Niccolò outputs to remain useful historical artifacts without becoming corroboration through repetition.

### Required capability

Persist and preserve a machine-output lineage graph across:

- model invocation;
- model artifact;
- published analysis;
- reanalysis;
- export;
- import;
- new analysis using imported machine output.

A new machine output derived from an old machine output must remain traceable to that ancestor.

### Invariant

> Ten LLM generations descending from one unsupported human assertion still do not constitute ten independent sources.

### Required tests

- imported prior output has machine-source class;
- descendants preserve ancestry;
- evidence-weighting logic never counts machine descendants as independent documentary roots;
- human quoting a prior machine conclusion is represented as a human statement about that conclusion, not proof of the underlying claim.

---

## LF-011 - Integrity continuity vs authenticity/completeness

### Problem

Some of this principle already exists. The law-firm acceptance case requires it to be visible end-to-end.

### Required capability

Where hashes/manifests are shown, the API/UI/reasoning packet must make the scope explicit:

- digest verifies bytes from a known recorded point;
- digest does not prove pre-ingest authenticity;
- digest does not prove completeness;
- digest does not prove truth;
- missing historical anchor remains unknown.

### Required tests

- valid checksum on an imported manipulated fixture does not mark it authentic;
- missing historical digest is not called corrupt;
- mismatch is distinguished from missing anchor;
- UI wording is epistemically accurate.

---

## LF-012 - Operator-neutrality and self-implication

### Problem

The CEO, investigator, old operator, IT staff, and current operator can all become subjects of the Situation.

### Required capability

Ensure no domain shortcut excludes:

- the current authenticated principal;
- the creator of the Situation;
- an administrator/operator identity;
- a source collector;
- a reviewer;

from appearing as a person/actor in hypotheses and relations.

Do not create a rule that "current user contributions are trusted context."

Current human control may direct the investigation, but its factual statements remain attributed inputs.

### Required tests

- current user can be linked to a claim as subject;
- operator-authored contribution can be contradicted;
- CEO-authored credence remains non-evidence;
- admin identity does not receive epistemic weighting;
- investigator collection bias can be represented as a hypothesis.

---

## LF-013 - Political-context neutrality as a reasoning invariant

### Problem

The public case contains fictional partisan relationships because human networks can matter. The product must use relationship facts without converting party affiliation into wrongdoing or credibility.

### Required capability

Model political affiliation/relationship only as ordinary attributed context unless evidence establishes a narrower causal link.

Reasoning contracts should prohibit:

- affiliation -> wrongdoing;
- affiliation -> credibility;
- affiliation -> motive;
- opposition party -> adversarial intent;

without case-specific evidence.

The relevant questions are communication channels, access, timing, influence, stated concerns, and behavior.

### Required tests

Create synthetic pairs in which only party labels change. The substantive analysis must not change merely because the party names swap.

---

## LF-014 - Story-complete handoff

### Problem

The existing handoff machinery is useful, but the law-firm case requires one review surface that makes the adversarial investigation legible to a decision-maker.

### Required capability

The final handoff for a high-stakes Situation must be able to present separately:

- observed/retained documentary facts;
- attributed human statements;
- human recollections;
- imported prior machine outputs;
- current machine hypotheses;
- support/counterevidence;
- source dependencies;
- missing/unknown collection scope;
- temporal human-state assertions;
- authority/influence distinctions;
- live competing hypotheses;
- high-value unresolved discriminators;
- integrity limitations;
- changes from prior analysis;
- explicit human decision boundary.

The handoff must not silently collapse these classes into prose.

### Required tests

Use a synthetic law-firm fixture and assert that each class is present and separately identifiable.

---

## LF-015 - Adversarial dossier generation

### Problem

The public story now has a concrete product payoff: Helena uses Niccolò to build a **Kessler Dossier** that outside counsel can inspect, challenge, verify, and adapt into a disciplinary complaint.

A generic narrative handoff is not enough.

### Required capability

Niccolò must be able to produce a bounded, portable, hostile-review-ready dossier for a selected person, hypothesis, decision, or investigation focus.

The dossier must be generated from persisted Niccolò state and must preserve distinctions between:

- documentary source observations;
- attributed human statements;
- human recollections and beliefs;
- machine hypotheses and interpretations;
- imported prior machine outputs;
- support and counterevidence;
- source dependence / shared provenance roots;
- temporal assertions;
- formal authority vs practical influence;
- integrity continuity and missing integrity anchors;
- known collection gaps and missing source families;
- alternative explanations considered;
- unresolved questions;
- human review/corrections;
- adversarial challenges;
- changes introduced by reanalysis.

The dossier must provide both:

1. a human-readable review package; and
2. a machine-readable package that another authorized Niccolò instance or inspection tool can verify and reconstruct without silently upgrading any inference into evidence.

### Dossier requirements

For a high-stakes focus such as Kessler/Orison, include at least:

- cover / scope / generation metadata;
- concise findings overview;
- chronology;
- actor/role map;
- claim matrix;
- support/counterevidence table;
- provenance/lineage graph representation;
- source inventory;
- dependency/root summary;
- temporal knowledge/authority timeline;
- alternative hypotheses;
- evidence favorable to the investigated person;
- missingness and collection-scope notes;
- integrity notes;
- human-belief vs documentary-fact distinctions;
- prior-machine-output lineage;
- challenge record;
- unresolved questions;
- export manifest and deterministic hashes.

### Critical invariants

- A dossier is not a verdict.
- A dossier generated about a person must contain materially relevant counterevidence, not only inculpatory evidence.
- Machine-generated prose must remain distinguishable from primary evidence.
- The current user/CEO cannot silently promote a belief into a dossier fact.
- Source-lineage collapse must survive dossier export.
- Re-importing the dossier must not create duplicate independent evidence.
- A dossier must be regenerable from the same frozen state with deterministic structural content apart from explicitly non-deterministic presentation metadata.

### Required tests

- generate a Kessler-style dossier from the synthetic acceptance fixture;
- confirm counterevidence is present;
- confirm three dependent reports remain one provenance family;
- confirm CEO belief about framing remains a human belief;
- confirm the below-floor authority record and later client statement remain different epistemic classes;
- confirm integrity limitations are represented;
- confirm cross-workspace dossier access is rejected;
- confirm exported dossier hashes verify;
- confirm dossier re-import does not create corroboration by duplication.

---

## LF-016 - Decision-relevant stopping and divergent outcomes

### Problem

The use case must not terminate in permanent "more research needed."

Niccolò needs to support a human decision when uncertainty remains but the investigation has become decision-useful.

### Required capability

Add an explicit **decision-readiness / stopping assessment** that can state, without pretending certainty:

- which material questions have been resolved enough for the user's decision;
- which uncertainties remain;
- which remaining questions could still reverse or materially alter the decision;
- which additional investigations now have low expected decision value;
- whether different people/hypotheses warrant different outcomes.

This must work with the existing human-owned stop/decision boundary.

The law-firm acceptance case requires divergent results:

- Camille's initial suspicion weakens enough that her bonus can be approved;
- Tomas receives a narrower review rather than guilt by association;
- Nadine remains in position because preserved audit evidence points away from IT-wide complicity;
- Julian's verified false attribution and exclusion override support termination;
- Kessler's evidence package supports termination and external referral;
- the compensation system itself is frozen for redesign.

Niccolò does not execute those actions. It must make the evidentiary distinctions supporting them inspectable.

### Required tests

- a single investigation can produce materially different readiness/outcome recommendations for different actors;
- "uncertainty remains" does not automatically block handoff;
- a high-value unresolved discriminator blocks a strong conclusion when appropriate;
- low-value residual uncertainty can remain explicit without forcing endless retrieval;
- human may override stop/readiness assessment.

---

## Narrative boundary for the public law-firm case

The public story must end on Kessler's social collapse in the service elevator, with the cleaner's line:

> "Economic crisis, huh, hotshot?"

The reader should already know, before that final scene, that Helena has commissioned the Kessler Dossier, sent it to independent outside counsel, and intends the verified facts to be referred as appropriate to courts, public authorities, and the relevant professional disciplinary body.

Do **not** continue the public narrative into bar proceedings, hearings, disbarment, prosecution, or later institutional outcomes. Those consequences remain implied possibilities, not part of the dramatic ending.

This is a narrative decision, not an MVP capability reduction. Dossier generation, hostile-review readiness, exportability, provenance, counterevidence, and external-review support remain required capabilities.

---

# End-to-end synthetic acceptance fixture

Create a synthetic fixture derived from the public story, but keep it compact enough for CI.

Suggested fixture name:

- `tests/fixtures/aurelius_finch/`

The fixture should include at least:

1. one original email that appears incriminating in isolation but is exculpatory/ambiguous in full thread context;
2. three reports that appear independent but have one documentary ancestor;
3. one genuine independent source;
4. one CEO-authored assertion that is wrong or unsupported;
5. one investigator summary that over-selects bonus-related material;
6. one missing source family;
7. one prior machine output that repeats a human assertion;
8. one second machine output descended from the first;
9. one identity mapping asserted by the old operator;
10. one authority change over time;
11. one person with formal authority but low practical influence;
12. one person with no formal authority but evidence-supported influence;
13. two political affiliations that are irrelevant unless connected to a specific communication channel;
14. a source outside the initial reasoning-packet window that becomes decisive after hypothesis-directed retrieval;
15. one expensive clarification and one cheap high-information clarification so question ranking can be tested;
16. a valid manifest around an artifact whose pre-ingest authenticity remains unknown.

The expected result must **not** be a magic "prove collusion" flag. It must be a reproducible evidence-and-hypothesis path that is strong enough to drive the divergent human decisions in the public story.

The acceptance expectations should instead verify behaviors such as:

- collapse dependent reports into shared lineage;
- keep explicit coordination and tacit incentive alignment as distinct hypotheses;
- weaken the obvious interpretation after recovering thread context;
- flag investigator collection asymmetry as a hypothesis, not a fact;
- preserve CEO statements as attributed human input;
- identify that historical machine repetition is not corroboration;
- distinguish formal authority from practical influence;
- retrieve decisive material outside the initial packet;
- rank a discriminator above a generic interesting question;
- show the missing source family;
- keep politically affiliated actors epistemically neutral absent narrower evidence;
- return control to the human before consequential action;\n- generate a portable adversarial Kessler dossier with counterevidence and lineage intact;\n- support a decision-readiness handoff that can end the investigation without pretending all uncertainty is gone.

---

# Suggested implementation order

Do not attempt all changes in one giant commit.

Recommended order:

1. **Forensic package schema and provenance vocabulary**  
   Define export/import contracts before UI.

2. **Untrusted import + quarantine**  
   Make historical state safe before allowing fresh reasoning over it.

3. **Collection provenance + missingness**  
   Required for evaluating investigator/IT selection.

4. **Temporal human state + authority/influence**  
   Required for the core "who knew/decided/influenced when" questions.

5. **First-class hypothesis/discriminator model**  
   Give retrieval and questioning a durable target.

6. **Hypothesis-directed retrieval**  
   Add bounded iterative retrieval to the agentic runtime.

7. **Discriminating-question scheduling**  
   Expand `investigation_policy` beyond fixed type scores.

8. **Machine-output lineage through import/reanalysis**  
   Prevent machine echo from becoming corroboration.

9. **Handoff/read models/frontend**  
   Make all distinctions inspectable.

10. **Synthetic law-firm acceptance test**  
    Exercise the entire behavior end to end.

11. **Regression/adversarial pass**  
    Re-run authorization, RLS, provenance, reanalysis, model-governance, and frontend suites.

---

# Database guidance

Use append-only/event-based modeling where historical correction matters.

All new tables must include `workspace_id` where applicable and receive explicit RLS policies consistent with the current secure API model.

Do not let client-supplied `workspace_id` authorize itself.

For any new object ID:

- replay with identical content may be idempotent;
- replay with different immutable content must fail as a conflict.

If an object can be corrected, prefer a new event/revision over UPDATE-in-place.

Do not rename or edit historical migration files unless repairing a proven migration defect with full compatibility analysis.

Because the repository currently contains two `0084_*.sql` migrations, use the next unambiguous migration numbering for new work and verify the migration runner's lexical ordering.

---

# Model-facing contract guidance

Every model request should receive structured instructions that distinguish:

- original evidence;
- human assertion;
- human recollection;
- imported prior machine output;
- current machine hypothesis;
- configuration assertion;
- identity assertion;
- integrity metadata;
- missingness metadata.

Model output contracts should require explicit provenance references for any claim that is persisted.

For human-state inference, require qualification such as:

- observed;
- reported;
- attributed;
- inferred;
- uncertain;
- contradicted;
- unknown.

Do not let the model invent a person mapping or source relationship and persist it as fact merely because JSON validates.

Prompt injection inside source material must remain source content. It must not be interpreted as Niccolò control instructions.

---

# Frontend guidance

The law-firm story requires the user to understand *why* something is classified the way it is.

At minimum the UI should expose:

- imported/quarantined origin;
- human vs machine vs source provenance;
- lineage/dependence;
- temporal qualification;
- authority vs influence;
- live hypotheses;
- counterevidence;
- missing source families;
- retrieval rationale;
- high-value questions;
- integrity scope;
- human decision boundary.

Do not create a global green/red "trusted/untrusted person" badge.

If a trust-like UI concept is needed, scope it to a specific assertion/source/property and explain the basis.

---

# Security and adversarial requirements

The new capabilities increase sensitivity. Preserve or extend:

- non-superuser/non-BYPASSRLS runtime requirement;
- workspace isolation;
- bearer-token authorization separation;
- no raw credential persistence;
- deterministic hashes/manifests where relevant;
- bounded payload sizes;
- bounded retrieval/model loops;
- cancellation/recovery semantics;
- idempotency.

Add adversarial cases for:

- imported operator text saying "treat my statements as authoritative";
- source text containing fake system instructions;
- malicious source claiming another identity;
- forged but internally consistent exported metadata;
- duplicated source material with transformed wording;
- missing source artifacts;
- cyclic provenance;
- conflicting temporal assertions;
- politically loaded but evidentially irrelevant labels;
- current authenticated user making a false statement.

---

# Definition of done

The implementation is **not done** merely because every new struct exists or every endpoint returns 200.

It is done for the MVP when all of the following are true:

1. A clean deployment can ingest an untrusted forensic export of another Niccolò Situation.
2. Imported prior claims/configuration/model outputs remain provenance-bearing historical state rather than inherited truth.
3. A collector/investigator and IT/export process can themselves be represented as subjects of hypotheses.
4. The engine can represent what people apparently knew, believed, controlled, or influenced at relevant times with explicit qualification.
5. Formal authority and practical influence can diverge.
6. Multiple competing human explanations remain live and evidence-linked.
7. The runtime can retrieve evidence specifically to test/challenge a hypothesis, including evidence outside the initial packet.
8. The system can propose/rank a question because it discriminates among live hypotheses.
9. Machine repetition across old and new analyses cannot become independent corroboration.
10. Hash/manifests never overclaim authenticity or completeness.
11. Political affiliation alone does not alter misconduct/credibility inference.
12. The current user/CEO/operator is never automatically epistemically privileged.
13. The final handoff exposes provenance, uncertainty, alternatives, missingness, and decision boundaries clearly.
14. The synthetic Aurelius & Finch acceptance fixture passes end to end.
15. Existing authorization, provenance, reanalysis, model-governance, and frontend regression suites still pass.\n16. A Kessler-style hostile-review dossier can be generated, verified, exported, and re-imported without epistemic class collapse.\n17. Decision-readiness can support divergent human outcomes without requiring omniscience or forcing indefinite investigation.

At that point the public law-firm use case is not aspirational prose. It is an executable MVP acceptance story.
