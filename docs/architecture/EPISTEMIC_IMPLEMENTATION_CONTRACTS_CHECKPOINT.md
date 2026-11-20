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

A question should be evaluable in terms of what it could change. Discriminatory power is a core dimension: information can be relevant yet fail to distinguish between competing hypotheses.

Initial qualitative scales are acceptable, for example negligible / low / moderate / high / very_high, provided rationale and context are preserved.

Questions may contain hidden premises. “Why did João lie?” can presuppose that João made the statement, that it was false, that João knew it was false and that he intended deception. Niccolò should preserve the original question while making those presuppositions challengeable.

Questions are also real-world acts. Asking can reveal information, alter trust, create fear, prime a witness, change behavior or contaminate later evidence.

## 10. Response is not truth

A response is a communicative occurrence. It may contain assertions, refusals, uncertainty, evasion, claimed ignorance or several propositions.

“Actor said ‘I do not know’” is not equivalent to “actor does not know”.

Receiving an answer does not automatically resolve the underlying gap. A gap may remain open, partially addressed, supported, contested, unresolved or effectively unresolvable depending on the available grounding.

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
- the MVP semantic interpretation boundary is English-only;
- a conservative English parser/intermediate representation may structure ordinary statements before richer epistemic materialization; the exact parser/LLM split must be reconciled with the current code rather than assumed;
- LLM output is proposal state, not canonical epistemic fact, and never bypasses deterministic contracts;
- progressive formalization is demand-driven: the complete ontology is representable, but rich decomposition occurs when conflict, consequence, ambiguity or investigative purpose justifies it;
- the ontology is not the investigator's data-entry model, and the graph is not required to be the primary UI;
- low-value speculative LLM proposals should be suppressed rather than creating a human curation queue;
- consequential conclusions require stronger explicit grounding than exploratory material;
- genuinely quantitative evidence preserves numbers, units, method, uncertainty and provenance; the rejection of universal epistemic scores does not discard quantitative evidence;
- access control remains deterministic product security;
- sophisticated algorithms are justified where necessary for the approved epistemic capabilities;
- avoid unjustified universal numerical scoring, probability or autonomous optimization.

## 25. Reconciliation gate before code changes

The first repository-level reconciliation is recorded in [Epistemic implementation reconciliation — V1](EPISTEMIC_IMPLEMENTATION_RECONCILIATION_V1.md). It classifies the actual Rust/PostgreSQL substrate before implementation changes.

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
