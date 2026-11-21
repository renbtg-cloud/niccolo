# Epistemic implementation contracts — pre-code checkpoint

**Status:** Product-owner-approved conceptual direction, recorded immediately before implementation reconciliation. This document does **not** claim that the contracts below are already implemented. It translates the approved epistemological model into implementation-facing semantic commitments that must be reconciled with the existing Rust/PostgreSQL architecture before code changes begin.

Read together with:

- [Epistemic architecture revamp](EPISTEMIC_ARCHITECTURE_REVAMP_IN_PROGRESS.md)
- [Independent interdisciplinary epistemic-domain review](EPISTEMIC_DOMAIN_INTERDISCIPLINARY_REVIEW_V1.md)
- [MVP composite evidence and human-guided circumstances](MVP_COMPOSITE_EVIDENCE_AND_HUMAN_GUIDED_CONTEXT.md)
- [Qualified temporal actor state](TEMPORAL_ACTOR_STATE_MVP.md)

## 1. Why this checkpoint exists

The epistemological model changed substantially, but that does not imply that the entire implementation must be discarded. The revolution is mainly semantic: richer identity, qualified relationships, explicit grounding, revision, higher-order human epistemic states, pervasive deception, investigative integrity and action under uncertainty. Existing mechanisms for provenance, historical state, auditing, permissions, ingestion, persistence and analysis may remain reusable if they satisfy the stronger contracts below.

The reconciliation rule is therefore:

> **Preserve engineering where the semantics survive; generalize or replace it where they do not. Never keep an old shortcut merely because code already exists.**

Before modifying code, classify existing structures as **preserve / generalize / replace / create**.

## 2. Persistent identity, revision and occurrence

Epistemically significant objects need identity independent of a particular analysis run. A new analysis must not automatically recreate a claim, question, gap, hypothesis, finding or other durable epistemic object as if it had never existed.

The implementation must distinguish at least these semantic notions:

- **Object identity:** the persistent epistemic object.
- **Revision identity:** a particular revision or representation of that object.
- **Occurrence identity:** a concrete communicative, investigative or observational occurrence.

A textual correction that leaves semantics intact is a new revision. A changed proposition is a new object related genealogically to the previous one. A repeated declaration is a new occurrence even if it expresses the same proposition. A new credibility judgment is a new assessment, not a mutation of the proposition.

History is fundamentally append-oriented:

- revision does not erase prior state;
- retraction does not mean an assertion never occurred;
- rejection of a hypothesis does not erase its historical influence;
- consolidation does not destroy original identities, provenance or genealogy;
- disagreement between investigators does not require a single globally current epistemic version.

This is a semantic/history contract, not an instruction to retain personal content forever. The implementation must permit lawful erasure, controlled redaction, anonymization or pseudonymization while preserving, where lawful, a content-free tombstone sufficient to identify affected dependencies and trigger selective reassessment. A tombstone must not preserve prohibited content indirectly.

## 3. Semantic proposition versus occurrence

The same semantic proposition may be asserted in several independent or non-independent occurrences.

For example, three people may assert the same proposition while preserving:

- three communicative occurrences;
- three provenance chains;
- potentially one shared semantic proposition;
- distinct credibility assessments;
- possible common-origin contamination;
- different contexts and interpretations.

Consolidating a proposition must never silently consolidate its evidence, independence, credibility or provenance.

## 4. Qualified relationships and relation contracts

A generic graph edge is not sufficient as the semantic contract.

Each relationship family must define, where applicable:

- admissible endpoint kinds;
- directionality;
- symmetry;
- transitivity;
- scope and context;
- temporal validity;
- grounding;
- attribution;
- propagation rules;
- revision/genealogy;
- confidentiality implications.

Examples:

| Relationship | Symmetric? | Transitive? |
| --- | --- | --- |
| exact semantic equivalence under matching scope | yes | yes |
| derived_from | no | ancestry may be traversed, but derivation semantics are not generic transitivity |
| supports | no | generally no |
| trusts | no | no |
| relevant_to | no | no |
| contradiction under the same interpretation/scope | yes | no |

Relationships can themselves be epistemically significant and investigable. “A trusts B”, “C believes A trusts B”, and “D challenges C's attribution” are distinct representable objects or qualified assertions. There is no semantic depth limit imposed by ontology; computation is bounded by purpose, relevance and budget.

N-ary grounding must be representable. Three premises may jointly support a conclusion even when no premise supports it independently.

## 5. Assessment as a transversal abstraction

Assessment is reused across distinct dimensions without creating a universal epistemic score.

An assessment may address, for example:

- credibility;
- relevance;
- expertise;
- independence;
- discriminatory power;
- loyalty;
- vulnerability;
- coercion susceptibility;
- investigation integrity;
- equivalence;
- inferential applicability.

Each dimension keeps its own semantics, qualitative values where appropriate, criteria, context, time, assessor, rationale, grounding and confidence.

Ordinal enums do not imply equal intervals, ratios, additive arithmetic, probabilities or global comparability.

Initial structured MVP implementation (migration 0105): credibility, relevance, reliability, independence and discriminatory power have distinct closed result vocabularies, plus explicit unknown and contested results. An investigator authors a judgment about a Situation-visible object (including an argument), evidence fragment, relation or grounding route. Each append-only event preserves criterion, context, rationale, confidence in the judgment, assessor, recorded time and optional ordered object/evidence grounds. A revision identifies its predecessor, retains the same assessor, target, dimension, criterion and context, and never overwrites history; competing judgments coexist. The workbench exposes the history and revisions without averaging results or inferring truth, independence or consensus. This is not yet a general assessment-dimension registry or automated propagation of assessment results through inference.

Assessment of assessment (migration 0108): another already-recorded assessment event in the same Situation can be the exact target of a separate judgment. Assessing the reliability or credibility of a judgment does not change the target judgment, its underlying proposition, or its route dependencies. Self-targeting and cross-Situation targeting are rejected. A revision of the target does not retarget the meta-assessment; reassessing the successor is a distinct judgment, not an overwrite of the earlier evaluation.

The Workbench identifies a pinned historical assessment target and its latest explicit revision when one exists. The diagnostic does not substitute a competing judgment, retarget the original assessment, or declare it invalid.

Explicit assessment reliance (migration 0106): an investigator may pin an exact assessment event to a grounding route with an attributed rationale. A revision in that same judgment chain triggers review of the relying route; a separate competing judgment does not silently replace the pinned event. Review events identify the exact successor assessed, preserve the earlier decisions, and do not satisfy review obligations created by later revisions. A retained disposition clears the direct review obligation for that successor; deferred and revision-required dispositions do not. Potential downstream grounding routes are exposed through explicit object/condition dependencies with cycle protection; they are not declared invalid or automatically reviewed. Alternate grounding routes remain unchanged unless they themselves depend on an impacted object or condition. This does not automatically infer reliance from a shared target, aggregate judges, change an assessment's values, or recalculate truth or confidence.

Downstream assessment review (migration 0107): each currently reachable downstream grounding route has a separate attributed review history pinned to the dependency and exact assessment successor. Retaining the directly relying route or a sibling downstream route does not clear its obligation. Later assessment revisions reopen the corresponding obligations without discarding historical reviews. Unreachable routes cannot be reviewed through this mechanism. The reachability diagnostic reflects the current explicitly authored graph; it is not a historical graph reconstruction or a determination that any conclusion is false.

Route-level reasoning diagnostics distinguish ordinary object-genealogy reassessment from assessment-revision attention and distinguish directly relying routes from downstream routes. Counts enumerate current review obligations by disposition, not independent evidence, truth, probability or confidence. A single route can have multiple distinct obligations and retaining one cannot clear another.

## 6. Temporal separation

The implementation must be able to distinguish, when relevant:

- when an external event occurred;
- when information became available;
- when an actor was exposed to it;
- when an actor may have perceived or examined it;
- when Niccolò acquired it;
- when an investigator formed an interpretation;
- when a later assessment or revision was recorded.

Historical reconstruction and current reconstruction must not be conflated.

## 7. Explicit grounding, argument and inference

A conclusion must not merely have “evidence attached”. The structure by which information supports a conclusion must be representable.

Conceptually:

```text
Evidence / Premises
        |
        v
    Argument
        |
        v
    Inference
        |
        v
    Conclusion
```

An argument records participating grounds and dependency structure. An inference records the attributed reasoning step, rule/method, applicability conditions, assumptions, rationale and output.

The system must be able to challenge an inference without destroying its premises. A true payment does not, by itself, establish that the payment was a bribe.

Grounding can include:

- jointly required grounds;
- alternative independently sufficient grounds;
- complementary grounds;
- conditional grounds;
- aspect-specific grounds.

Necessity for one argument is not necessity for the conclusion itself.

### Structural cycle diagnostics

Explicit supporting routes can form cycles. Context members provide circumstances for interpretation but do not establish a supporting edge, an external anchor, or directional grounding by themselves. A route with grounds outside a strongly connected component may still depend jointly on members inside it. Distinguish those mixed routes from routes with only external non-context grounds. Neither distinction establishes independence, reliability, or sufficiency; the cycle itself supplies no additional corroboration.

Exact reuse of the same object or evidence fragment across authored routes must be visible, using the composite argument and grounding-set identity. This is a structural diagnostic, not a complete provenance-independence test: different grounds may still share ancestry. Support and opposition remain separate, and no numerical confidence or truth verdict is inferred from route counts.

The structured Workbench now also projects recorded dependence components for directly selected documentary grounds of each conclusion. It reuses the bounded, workspace-scoped evidence derivation loader and connects fragments sharing a recorded root or a source document anywhere in their ancestry. The components retain the selected fragment and authored route identities; they do not infer independent corroboration even when distinct. Withdrawn, missing, cyclic or excessive lineage is reported as unresolved rather than supplying a misleading component count. Indirect ancestry through epistemic object grounds and unrecorded collection dependencies are not covered by this projection.

### Inferential applicability — initial expert-authored implementation

A particular grounding route may have explicitly authored required conditions referring to Situation objects. The investigator records an applicability assessment (`UNASSESSED`, `SATISFIED`, `NOT_SATISFIED`, `CONTESTED`, or `UNKNOWN`) with rationale, actor and append-only event identity. The read model retains the latest assessment per condition and assessor, exposing disagreement rather than manufacturing consensus. The absence of a condition is not certification of applicability. The assessment does not establish proposition truth, deductive validity or actual conclusion strength.

Revision of a required condition creates a selective reassessment obligation for its route and downstream dependencies without silently erasing the authored support or alternative routes. The implementation does not yet constitute a separately versioned inference-rule registry, automatic condition evaluation, or a general assessment language.

## 8. Selective epistemic revision

Changes propagate as **obligations to reassess**, not as automatic global falsification.

If one ground is defeated, an argument depending jointly on it may fail while the conclusion remains supported by an independent argument.

Contradictory support is preserved rather than numerically cancelled:

```text
support_for(H)
support_against(H)
```

A hypothesis may remain contested with strong arguments on both sides.

If a source becomes suspected of manipulation, descendants are not automatically false. The system should identify:

- direct dependence;
- derivation ancestry;
- independent corroboration;
- transformed material;
- arguments relying on the source;
- downstream findings/recommendations requiring reassessment.

A revision can itself later be revised. The grounds for degrading prior evidence may themselves become contested.

The central invariant is:

> **Every revisable conclusion must be able to explain what it depends on, and every relevant change must be able to locate what potentially needs reassessment without destroying independently grounded knowledge.**

## 9. Directed investigation: gaps, questions and discriminatory power

`InformationGap` represents a contextual missing semantic aspect, not merely a sentence phrased as a question.

`Question` represents a formulated investigative problem or inquiry and may relate to one or several gaps, hypotheses, decisions or objectives.

There is no mandatory UNKNOWN -> InformationGap -> Question pipeline.

Initial structured authoring of investigative questions (migration 0109): a Situation-scoped `QUESTION` object preserves the investigator's wording, rationale and attribution independently of `INFORMATION_GAP` and hypothesis objects. A question may exist without a gap, is linked explicitly by contextual relevance, and can have question-to-question genealogy. The Workbench/API do not allow a question itself to be a proposition-like entailment, causal claim, conclusion, premise, assumption or applicability condition. It may be contextual material or a target of an attributed assessment. No answer, presupposition, evidentiary support or gap resolution is inferred from authoring a question.

Typed investigative links (migration 0110) let an investigator record `ADDRESSES_GAP` from a question to one or several information gaps, `TESTS_HYPOTHESIS` to one or several candidate explanations, and `PRESUPPOSES` to explicitly identified propositions, assumptions or hypotheses. Endpoint types, direction and Situation visibility are validated by the API and by a caller-scoped SQL trigger. These links preserve the original question, do not resolve a gap or endorse a hidden premise, and do not imply that testing several hypotheses discriminates between them.

The Workbench snapshot's `question_inquiries` is a read-only projection for every visible question, including questions with no links. It groups the exact typed relation and target identities, and separately exposes unrevised direct relevance and discriminatory-power assessment event IDs. The UI resolves these identities to the original wording, rationale, criterion, context and attribution. Independent judgment chains remain separate; neither link counts nor the absence of an assessment imply discriminatory power, truth, priority or resolution.

A question should be evaluable in terms of what it could change. Discriminatory power is a core dimension: information can be relevant yet fail to distinguish between competing hypotheses.

Initial qualitative scales are acceptable, for example negligible / low / moderate / high / very_high, provided rationale and context are preserved.

Questions may contain hidden premises. “Why did João lie?” can presuppose that João made the statement, that it was false, that João knew it was false and that he intended deception. Niccolò should preserve the original question while making those presuppositions challengeable.

Questions are also real-world acts. Asking can reveal information, alter trust, create fear, prime a witness, change behavior or contaminate later evidence.

## 10. Response is not truth

A response is a communicative occurrence. It may contain assertions, refusals, uncertainty, evasion, claimed ignorance or several propositions.

“Actor said ‘I do not know’” is not equivalent to “actor does not know”.

Migration 0112 records already-received question responses as explicit links to immutable human utterance occurrences. One utterance can respond to several questions, and two occurrences with identical wording remain distinct identities. The question and utterance must belong to the same Situation; the write is atomic and supports exact replay without duplicating the occurrence. The original English wording, optional actor and source reference, link rationale and recorder are retained. The Workbench shows responses separately from semantic abstractions and assessments, and does not infer a proposition, actor mental state, answer correctness or gap resolution; it does not ask or send a question.

Receiving an answer does not automatically resolve the underlying gap. A gap may remain open, partially addressed, supported, contested, unresolved or effectively unresolvable depending on the available grounding.

Migration 0111 implements gap-resolution judgments as the separate `GAP_RESOLUTION` assessment dimension, restricted to a Situation-visible `INFORMATION_GAP`. Its qualitative results are `OPEN`, `PARTIALLY_ADDRESSED`, `SUPPORTED`, `CONTESTED`, `UNRESOLVED`, `UNRESOLVABLE` and `UNKNOWN`. The API validates the visibility of explicit grounds; both the API and SQL require at least one ground other than a question or information gap for `PARTIALLY_ADDRESSED` or `SUPPORTED`. This is an attributed contextual assessment, not a certified resolution or truth value. The Workbench keeps gaps with no judgment visible, displays competing unrevised judgment chains independently, and preserves exact revision history and grounds. A question link, an utterance or a new proposition does not automatically produce a judgment.

For human-language input, preserve separately:

- the original utterance;
- the current derived interpretation;
- whether the user has confirmed that Niccolò understood the intended meaning;
- who or what produced the interpretation;
- rationale/context where material;
- interpretation revision history;
- downstream reasoning that depends on the interpretation.

Niccolò may reason from an unconfirmed interpretation, but it must remain explicitly provisional in persistence and UI. Later clarification must be able to trigger selective reassessment.

User confirmation of an interpretation means that the meaning was understood correctly. It does **not** establish the truth of the resulting proposition.

The Workbench now exposes the existing persistent utterance-interpretation substrate (migration 0098) for response occurrences. A trained investigator explicitly authors each human interpretation revision; the API scopes the utterance, interpretation and predecessor to the exact Situation and uses optimistic revision identity. Meaning confirmation or rejection targets an exact revision and preserves recorder, note and time. The response snapshot presents original wording, current and historical interpretations, producer, rationale, and all confirmation events. A new revision is provisional even when the preceding revision was confirmed; a later confirmation of a historical revision cannot confirm its successor. No proposition, truth, gap resolution or inference is automatically authored by this operation.

Migration 0113 permits a route to explicitly pin an interpretation revision and its exact confirmation event, if any, with an attributed rationale. The pinned meaning may be provisional. A later interpretation revision, or changed confirmation context for the still-current revision, creates a structural reassessment obligation for the directly relying route and potentially dependent downstream routes. The snapshot reports these attention counts separately from assessment and relation reassessment, and does not mark unrelated or alternative routes solely because they share a conclusion. The original dependency and reasoning content remain intact; confirmation of meaning is not validation of the route or truth of its conclusion.

Migration 0114 adds append-only interpretation-reliance reviews for the directly relying route and each structurally reachable downstream route separately. Each review identifies the exact revised meaning and current confirmation event (or absence), disposition (`RETAINED`, `REVISION_REQUIRED`, `DEFERRED`), recorder and note. Only retention for the current meaning context clears that route's attention; it does not clear downstream attention, alter its grounds or validate its conclusion. A subsequent meaning revision or confirmation event opens new review obligations and never overwrites earlier reviews.

Interpretation should be conservative. Parse what was said and what the context supports; do not complete an implied chain such as received -> read -> understood -> believed without evidence. Unknown is preferable to unsupported semantic completion.

## 11. Choosing the next investigative step

Niccolò may recommend next investigative questions/actions by considering, qualitatively or with justified richer methods:

- competing hypotheses;
- gaps;
- expected discriminatory power;
- cost;
- availability/accessibility;
- contamination risk;
- urgency;
- authorization;
- investigation coverage.

This is not an autonomous universal optimizer. The user controls investigative resource allocation in the MVP. Niccolò may recommend redistribution but must not silently reallocate budgets.

Migration 0115 introduces separate, attributed question-context assessments for acquisition cost, accessibility, contamination risk, urgency, reported authorization status and investigation coverage. The existing relevance and discriminatory-power dimensions remain independent. Each assessment retains its criterion, context, rationale, confidence, grounds and revision chain. Reported authorization is not an authorization grant; no dimension permits an external act, reallocates resources or establishes epistemic resolution. Missing or contested judgments must not be converted into numerical defaults.

The Workbench comparison panel projects up to three explicitly selected questions side by side, retaining linked gaps, tested hypotheses and presuppositions alongside attributed current criterion judgments. It distinguishes no assessment from an explicit UNKNOWN or an incomplete projection, retains disagreements, and makes no automatic selection or ranking. The panel is read-only: comparison does not create an investigative act or authorize one.

Migration 0116 adds a human-authored inquiry decision ledger. An explicit disposition (`PURSUE_NEXT`, `DEFER`, `DO_NOT_PURSUE`) names one considered question, the set of up to three considered questions, an optional list of exact selection-assessment events, and the recorder's rationale. The scoped write API and database both validate the question and basis identities. History remains append-only; a later assessment revision does not rewrite the consulted basis or silently change the decision. Recording a choice does not execute it, allocate a budget, grant authority, produce an answer or resolve a gap.

Checkpoint 1.25 validation: real PostgreSQL authoring/replay, incompatible replay refusal, invisible and non-considered basis rejection, direct SQL constraint, bearer read/write separation, and retention of exact historical assessment references. Full regression passed (588 Rust, 130 React, 22 Python, 8 standalone JavaScript tests), including the rebuilt embedded frontend.

Administrative closure remains independent from epistemic resolution. An investigation can legitimately be closed while epistemically unresolved.

## 12. Human epistemic state is not a boolean

“Knowing” must not collapse exposure, access, attention, comprehension, belief and assertion into one flag.

Relevant distinguishable notions include:

- availability/exposure;
- access;
- perception;
- attention;
- examination;
- comprehension;
- retention/recall;
- suspicion;
- inclination;
- doubt;
- belief;
- suspended judgment;
- claimed knowledge;
- perceived knowledge;
- knowledge supported by available evidence.

These notions are contextual, temporal, attributable and independently assessable. They do not form one rigid monotonic ladder.

## 13. Interest, curiosity, aversion and epistemic acts

Humans can want to know, not care, prefer not to know, seek only enough knowledge to act, avoid knowledge to preserve deniability, delegate knowing or avoid formal documentation.

Curiosity is not merely “high interest”; it can have an object, goal, context and inferred purpose.

Persistent dispositions must be distinguished from immediate reactions. Observed conduct can update hypotheses about dispositions without proving personality traits.

Epistemic acts include investigating, ignoring, deepening, deferring, avoiding acquisition, delegating, reconsidering, challenging and preserving ambiguity.

## 14. Higher-order epistemic state

Niccolò must represent beliefs or knowledge about another actor's epistemic state without hard-coded “first/second/third-order” classes.

Conceptually, an epistemic state can itself become the object of another epistemic state.

Computation is bounded by purpose, relevance and budget, not by an ontological depth ceiling.

This enables representation of asymmetries such as:

- A knows fact F;
- B knows that A knows F;
- A does not know that B knows this.

## 15. Trust, loyalty, ego and identity

Trust is directed, contextual and multidimensional. A person may trust another's technical competence while distrusting their confidentiality or truthfulness.

Trust is distinct from dependence, reliability, support and benevolence.

Loyalty is a qualified relation to a person, group, institution, cause or principle and is distinct from obedience, agreement, compliance, fear, dependency and aligned interests. Multiple loyalties can conflict.

Motives involving ego must be representable explicitly:

- saving face;
- avoiding humiliation;
- avoiding appearing incompetent;
- protecting self-image;
- preserving status or importance;
- seeking recognition.

The product owner explicitly prefers **ego protection** for this personal self-image/status mechanism.

**Identity protection** remains a separate, broader possibility: protection of a social, professional, political, familial or group identity. The two must not be collapsed.

## 16. Pressure, coercion, dependency, leverage and autonomy

Do not collapse all motivation into one field.

Distinguish at least:

- incentive;
- pressure;
- communicated threat;
- adverse condition;
- coercion;
- constraint;
- dependency;
- leverage;
- autonomy.

Capability, intent, communication, perceived credibility and outcome are different.

Compliance is not consent. Submission is not loyalty.

Autonomy is contextual: ability to decide, refuse, disagree, exit, seek review, acquire information and revise commitments may differ independently.

Vulnerability is contextual exposure to an adverse consequence, not a personal defect.

## 17. Epistemic influence and power

Epistemic influence can target more than belief. It can target:

- attention;
- interest;
- curiosity;
- trust;
- acceptance;
- interpretation;
- investigation priority;
- willingness to examine an issue.

Separate:

- influence capacity;
- influence act;
- intended target/effect;
- actual effect;
- perceived success.

Epistemic power is not formal authority. A person without formal rank may control who receives information, who understands it, which experts are consulted or which version reaches decision-makers.

Epistemic asymmetry can concern information, access, understanding, meta-knowledge or influence.

## 18. Pervasive deception

Deception is not equivalent to a false statement.

Truthful propositions can be composed into a misleading narrative by:

- selection;
- omission;
- framing;
- ordering;
- timing;
- aggregation;
- decontextualization.

Omission is not automatically deceptive. Ignorance, forgetfulness, irrelevance, hierarchy, fear, limited access and time pressure are alternative explanations.

Separate:

- deceptive intention;
- attempt;
- means;
- target;
- intended effect;
- actual effect;
- perceived success.

A deception attempt can fail against its intended target yet mislead an unintended observer.

Self-deception must remain an attributed hypothesis, not casual psychological diagnosis. Candidate mechanisms include rationalization, selective attention, motivated interpretation, ego protection, identity protection and avoidance of conflicting evidence.

## 19. Investigation-directed deception and integrity

A sophisticated adversary need not alter evidence. They may manipulate:

- questions asked;
- hypotheses considered;
- witness priority;
- time windows;
- normalization criteria;
- investigative resource allocation;
- exclusions;
- summaries reaching decision-makers.

A procedurally competent investigation can therefore follow the wrong direction.

`InvestigationIntegrity` should be multidimensional and contestable, considering matters such as independence, evidence selection/provenance, question framing, omitted hypotheses, discriminatory power, reasoning quality, criteria consistency, dissent preservation, historical traceability and reviewability.

Detect self-sealing reasoning: a hypothesis for which supporting evidence, contrary evidence and absence of evidence are all reinterpreted as support is epistemically dangerous even if the hypothesis might happen to be true.

Counterintelligence remains a vertical/stress-test over the general epistemic core, not a second epistemic engine.

## 20. Compartmentalization and security

Modeled-world secrecy is not product access control.

The model may represent that an actor should not know that an investigation exists. Actual software permissions are enforced deterministically by product security and are never inferred automatically from modeled authority or secrecy.

Compartmentalization may protect:

- existence;
- content;
- source identity;
- origin;
- rationale;
- conclusions;
- partial content.

If existence is protected, indirect signals through graphs, search results, counts, recommendations, notifications, metadata, summaries or LLM context must not leak it.

Distributed fragments may permit an authorized Niccolò analysis to formulate a hypothesis no individual actor can formulate. That does not justify saying “the organization knew”.

## 21. Vulnerability, containment and recovery

Vulnerability is actor/resource/circumstance/consequence specific and may be actual, potential, perceived, unknown or misattributed.

Vulnerabilities may emerge compositionally. Several individually innocuous data points can jointly expose sensitive information.

Protective actions can introduce new vulnerabilities: centralization can reduce diffusion while creating a single point of failure, dependency or capture risk.

Epistemic containment for potentially compromised material should:

- identify affected dependencies;
- pause reliance where justified;
- preserve provenance;
- seek independent verification;
- review downstream findings;
- avoid blanket deletion or automatic falsification.

Epistemic recovery should identify usable material, independently grounded material, revised conclusions, unresolved gaps and controls that must be restored.

## 22. ActionRecommendation and the human action boundary

`ActionRecommendation` is a first-class, attributable, revisable object with purpose, recipient, context, rationale, grounding, conditions, alternatives, expected consequences, uncertainty, risk and reversibility.

Keep structurally distinct:

```text
Recommendation
    != Decision
    != Authorization
    != Plan
    != Execution
    != ActualOutcome
```

Confidence in a suspicion is not the same as recommendation strength. A low-confidence leak hypothesis can still justify a low-cost, reversible privilege review because the structural vulnerability exists independently.

Recommendations should compare alternatives where meaningful, including inaction, delay and gathering more discriminatory information first.

Protective recommendations under uncertainty should prefer proportionate, reversible and structurally useful measures such as evidence preservation, privilege review, separation of duties, independent review and provenance verification rather than silently converting suspicion into accusation or punishment.

The existing prohibition remains: Niccolò may model and analyze kinetic attacks and consequences but must not recommend or develop operational plans for kinetic attacks against persons or installations.

## 23. Intervention changes the investigated world

A recommendation that becomes a decision and execution changes circumstances.

An intervention can alter:

- actors' beliefs;
- incentives;
- loyalties;
- vulnerabilities;
- evidence availability;
- opportunities;
- trust;
- behavior;
- future deception;
- investigation contamination.

Reasoning after intervention must not continue as if the pre-intervention world were unchanged.

## 24. Implementation stance

The approved model does not mandate one Rust type, SQL table or service per conceptual noun.

Use the smallest implementation architecture that preserves the semantic distinctions and permits meaningful behavior.

The working implementation stance is:

- Rust for typed contracts and deterministic semantic validation;
- PostgreSQL for durable relational persistence, referential integrity, history and efficient projections;
- no graph database required merely because the domain is graph-shaped;
- no heavyweight ontology engine required merely because relationships are rich;
- the MVP assumes a cultivated, domain-trained investigator who explicitly authors epistemic abstractions through a structured workbench;
- free text may be entered inside a semantic role explicitly chosen by the user, but automatic interpretation of investigator free text into ontology objects is post-MVP;
- core abstractions such as Hypothesis, Assumption, InformationGap, Finding, Argument, typed Relation and Grounding may therefore be intentional UI primitives in the MVP;
- progressive formalization is human-directed: the complete ontology is representable, while the investigator deepens structure when conflict, consequence, ambiguity, revision or investigative purpose warrants it;
- model-generated reasoning inside Niccolò remains proposal state, not canonical epistemic fact, and never bypasses deterministic contracts;
- the system must record observable user actions and provenance without inferring unobserved attention, comprehension or diligence from interaction speed or bulk actions;
- consequential conclusions require stronger explicit grounding than exploratory material;
- genuinely quantitative evidence preserves numbers, units, method, uncertainty and provenance; the rejection of universal epistemic scores does not discard quantitative evidence;
- access control remains deterministic product security;
- sophisticated algorithms are justified where necessary for the approved epistemic capabilities;
- avoid unjustified universal numerical scoring, probability or autonomous optimization.

## 25. Reconciliation gate before code changes

The first repository-level reconciliation is recorded in [Epistemic implementation reconciliation — V1](EPISTEMIC_IMPLEMENTATION_RECONCILIATION_V1.md). It classifies the actual Rust/PostgreSQL substrate before implementation changes.

The first expert-authoring substrate is now implemented by migration `0100_expert_epistemic_workbench.sql`, `src/epistemic_workbench.rs`, the corresponding API endpoints and the frontend Workbench. This implementation intentionally leaves investigator free-text interpretation out of the MVP.

Before implementing this checkpoint, inspect the current Rust, PostgreSQL schema/migrations, APIs, frontend contracts and executable tests.

Classify each relevant existing construct as:

1. **Preserve** — semantics already satisfy the approved contract.
2. **Generalize** — useful implementation exists but needs broader semantics.
3. **Replace** — current representation conflicts with the approved model.
4. **Create** — approved capability lacks a suitable implementation primitive.

Prioritize shared semantic substrate before proliferating feature-specific objects:

- persistent epistemic identity;
- qualified relationship contracts;
- provenance and attribution;
- contextual and temporal qualification;
- genealogy/revision;
- grounding/dependency semantics;
- scoped assessment;
- confidentiality/visibility enforcement;
- cycle-safe/non-amplifying reasoning.

Then reconcile the six functional slices already defined in the main architecture review.

The AT-01 through AT-26 adversarial cases remain candidates for executable contract, invariant, property and integration tests. AT-26 specifically tests weaponized formalism: formally valid but low-value objections must remain representable without acquiring automatic veto power over investigation progress.

---

**Checkpoint decision:** Conceptual design is sufficiently mature to stop adding ontology merely for completeness. The next legitimate step is reconciliation with the real implementation, not further speculative abstraction. New concepts should be added only when code reconciliation, adversarial testing or evidence demonstrates that an approved distinction cannot be expressed coherently with the current model.
