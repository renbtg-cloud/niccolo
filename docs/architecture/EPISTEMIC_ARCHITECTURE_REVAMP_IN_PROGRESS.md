# Epistemic architecture revamp — work in progress

**Status:** Exploratory architecture work in progress. This document is **not** a normative product contract, not a declaration of implemented behavior, and not a replacement for the existing MVP architecture documents.

**Purpose:** Preserve the current epistemic-architecture review as a deliberate design track, make its method explicit, and prevent future implementation work from either (a) treating the current model as unquestionably complete or (b) assuming that a concept discovered during research is absent from Niccolò before checking the existing specifications and code.

Read this with:

- [MVP composite evidence and human-guided circumstances](MVP_COMPOSITE_EVIDENCE_AND_HUMAN_GUIDED_CONTEXT.md)
- [Qualified temporal actor state](TEMPORAL_ACTOR_STATE_MVP.md)
- [Independent interdisciplinary epistemic-domain review (66 sources)](EPISTEMIC_DOMAIN_INTERDISCIPLINARY_REVIEW_V1.md)
- [Epistemic weighting API](EPISTEMIC_WEIGHTING_API.md)
- [System-integrator boundaries and near-term hardening](SYSTEM_INTEGRATOR_BOUNDARIES_AND_HARDENING.md)
- [Architecture index](README.md)

## 1. Why this review exists

Niccolò is becoming a richer epistemic system than a collection of document parsers, prompts, claims and scores.

Its core problem is human conflict and human-context reasoning: people, groups and software actors can disagree, cooperate, deceive, misunderstand, possess partial information, act under authority, delegate work, influence one another, revise positions, investigate one another, and intentionally or accidentally alter the informational environment in which later reasoning occurs.

The architectural risk is therefore not only that an individual abstraction is missing. The larger risk is that apparently adequate abstractions fail when combined.

The working assumption for this review is:

> **The devil is often in the interrelationships between epistemic abstractions, and sometimes in relations between those relations.**

This does **not** imply that every relationship must become a heavyweight entity, every academic distinction must become a Rust type, or every conceivable capability must enter the MVP.

## 2. Method: study the epistemic domain before judging the current implementation

The review intentionally uses a two-pass method.

### Pass A — clean-room epistemic study

Study the domain as if Niccolò did not yet exist.

Use relevant work from areas such as:

- social epistemology;
- argumentation theory;
- belief revision;
- dynamic and multi-agent epistemic logic;
- philosophy of action and intention;
- institutional and normative reasoning;
- temporal reasoning;
- causality where it matters to human explanations;
- adversarial reasoning;
- forensic and investigative methodology;
- provenance and information-governance models.

The objective is **not** to accumulate academic vocabulary. The objective is to identify distinctions that matter when reasoning about difficult human situations.

The result of this pass is a candidate conceptual model, not a list of missing Niccolò features.

### Pass B — reconcile with the real Niccolò

Only after the independent pass should the candidate concepts be compared systematically with current documentation, code, schemas, tests and user workflows.

For each candidate concept or relationship, classify it as one of:

- already represented adequately;
- represented but worth refining;
- specified but not yet implemented;
- implemented under a different abstraction or name;
- intentionally out of scope;
- genuinely missing;
- in conflict with an existing design decision that needs explicit resolution.

Do not call something a Niccolò gap merely because it appeared in the academic review.

For example, temporal actor knowledge, authority, influence and historical epistemic state already have explicit treatment in [Qualified temporal actor state](TEMPORAL_ACTOR_STATE_MVP.md). The clean-room review may independently rediscover those needs; that is evidence that the distinction matters, not evidence that Niccolò lacks it.

## 3. Current candidate families

The following families are **research candidates**, not a frozen ontology and not a mandate that each item become its own class, table or service.

- situations and investigations;
- actors, groups and software actors;
- relationships and qualified relationships;
- events, activities and actions;
- communicative acts and investigative acts;
- propositions, claims and circumstances;
- evidence and evidentiary relationships;
- provenance, derivation, collection history and chain of custody;
- source credibility, relevance and inferential force;
- premises, arguments, inference rules, justifications and defeaters;
- hypotheses, findings and belief revision;
- human-declared confidence and operational premises;
- intentions, goals and plans;
- perspectives, beliefs, knowledge and information exposure;
- temporal validity and historical epistemic state;
- institutional roles, authority, delegation and mandates;
- access, purpose and investigation policies;
- contradictions, competing explanations and unresolved states;
- causal and intentional explanations where relevant to human reasoning.

This list is deliberately redundant in places. The review must discover which distinctions are genuinely independent and which should collapse into simpler structures.

## 4. Relationships are not merely secondary plumbing

A simple hierarchy is only one possible projection of the domain.

For example, a chain such as:

```text
authority
  -> commander
    -> investigation lead
      -> team
        -> investigator
```

is a useful happy path, but real investigations may form graphs:

- one investigator participates in several investigations;
- one finding is reused by another investigation;
- one investigation reviews another;
- two investigations share evidence but not access rights;
- one actor formally supervises another while practical influence flows elsewhere;
- a policy is authored by one authority, applied by another and later investigated by a third;
- dependencies can be cyclic in the real-world domain even when a particular computation must use an acyclic projection.

Therefore:

> **Hierarchies are useful views. They are not assumed to be the fundamental structure of the epistemic domain.**

Some relationships may require identity, provenance, temporal scope, authority, qualifications, evidence, contestation and revision. Others should remain lightweight. The distinction must follow semantic need, not a desire to turn every edge into an object.

## 5. Reflexivity without automatic recursion

Niccolò should be capable of investigating investigators, investigative acts, investigations and governing policies using the same general epistemic machinery used for ordinary investigations.

That does **not** mean Niccolò automatically investigates whoever defined a policy, a senior executive, an investigator, or itself.

The governing distinction is:

> **Epistemic representability does not imply operational authority.**

Investigation scope, access, purpose, authorization, delegation and stop conditions remain ordinary enterprise-software constraints.

A policy author can be relevant context without automatically becoming an investigation target. An investigation may become the subject of another investigation only when the applicable authority and scope permit it.

If investigating an investigator or a policy requires a parallel epistemic engine, that is evidence that the general abstractions may still be too narrow.

## 6. Interactions may matter more than isolated abstractions

The review must explicitly search for capabilities that emerge only from combinations of concepts.

Four abstractions may each be simple while their useful behavior appears only when all four participate. In another case, the same four abstractions may be independently useful and called by unrelated parts of the system.

Do not confuse:

- epistemic dependency;
- semantic dependency;
- implementation dependency;
- architectural encapsulation;
- value dependency.

A group of abstractions that must work together does not automatically require a new epistemic abstraction above them. It may need only a cohesive software entry point, a context object, a trait boundary, or an integration test.

Conversely, a combination may reveal a genuinely new domain concept that deserves explicit representation.

## 7. MVP decisions must consider dependency and option value

Complexity alone does not determine MVP inclusion.

A sophisticated capability may be postponable if it is relatively isolated and its absence does not distort other reasoning. A chain-of-custody validator is one hypothetical example to test, not an assumption: if its principal role were to qualify evidence with little cross-domain coupling, a minimal or deferred implementation might be acceptable.

The opposite case is equally important. Several individually modest abstractions may need to be implemented together because partial implementation produces little value or, worse, contaminates the rest of the model with misleading shortcuts.

For every candidate capability, ask:

1. What value does it provide alone?
2. What value appears only when combined with other abstractions?
3. What information or invariant must be captured now even if richer processing is deferred?
4. Can a simple default implementation preserve the semantics?
5. Can an external/custom implementation safely replace or extend it later?
6. What is the migration cost if the abstraction is introduced later?
7. Does a partial implementation mislead users or other reasoning components?

The goal is to identify coherent minimum slices rather than selecting MVP work merely by feature size.

## 8. Adversarial cases are architecture probes

The review should use demanding fictional cases to expose weak abstractions.

Useful probes include:

- coordinated deception;
- truthful information supplied for deceptive purposes;
- colluding sources that appear independent;
- contaminated investigators;
- investigators investigating investigators;
- policy-defined investigations;
- overlapping chains of command;
- conflicting authority and practical influence;
- incomplete information exposure;
- evidence intentionally created to manipulate the investigator;
- changing objectives during an investigation;
- multiple human and software actors with different information and incentives;
- findings that become inputs to later investigations;
- disputed relationships and disputed delegations.

These scenarios are not evidence that Niccolò should become an unlimited surveillance or forensic platform. They are tests of whether the epistemic abstractions remain coherent under difficult human behavior.

## 9. Human authority remains fundamental

A richer epistemic architecture must not silently turn Niccolò into a sovereign judge of reality.

Preserve the established distinction between:

- source material;
- human assertions;
- machine observations where explicitly supported;
- hypotheses;
- interpretations;
- operational premises;
- heuristic weighting;
- findings;
- consequential human decisions.

Human policy may legitimately define how a user or organization chooses to proceed under uncertainty. That policy and its provenance should remain explicit rather than being confused with independently verified fact.

## 10. Extensibility should follow domain decomposition

Customizability is important, but it should not dictate the ontology.

As the domain is decomposed into stable responsibilities, those responsibilities may expose progressively finer extension points. A customizer might eventually replace or extend only a custody validator, an inferential rule evaluator, a credibility assessment, or another narrow behavior without replacing the entire reasoning system.

Possible execution mechanisms may include local IPC/RPC workers, REST, gRPC, native libraries or language-specific runtimes. These are infrastructure choices behind semantic contracts.

The immediate rule is:

> **First identify the real epistemic variation point. Then choose the simplest extension mechanism that preserves its contract and invariants.**

## 11. Ordered next steps

1. Continue the independent academic review of the epistemic domain.
2. Consolidate a candidate ontology without committing prematurely to Rust representation.
3. Map interactions, cross-dependencies and higher-order combinations between abstractions.
4. Classify dependencies: independent, complementary, foundational, transversal, substitutable, or meaningful only as a set.
5. Build adversarial conceptual cases designed to break the candidate model.
6. Use those cases to identify missing or incorrectly separated abstractions.
7. Define semantic invariants for important abstractions and combinations.
8. Identify coherent minimum functional slices for the MVP.
9. Compare the resulting model systematically with existing Niccolò specifications, code, schemas and tests.
10. Produce the actual architectural delta: preserve, refine, add, merge, split or explicitly defer.
11. Implement the delta incrementally with contract, integration and adversarial tests.
12. Deepen customization points only where real domain variation justifies them.

In short:

> **epistemic study -> interactions -> adversarial cases -> coherent minimum slices -> comparison with real Niccolò -> architectural delta -> incremental implementation**

## 12. How future implementers should use this document

This document is a warning against two opposite mistakes:

1. **Current-architecture bias:** assuming the existing model defines the epistemic domain simply because code already exists.
2. **Clean-room arrogance:** assuming every independently rediscovered concept is absent or incorrectly modeled before inspecting the actual Niccolò architecture.

Do not perform broad refactors merely because this review exists.

Do not freeze existing abstractions merely because the MVP is underway.

When the review identifies a concrete improvement, evaluate whether now is the technically opportune moment under the rules already established in [MVP composite evidence and human-guided circumstances](MVP_COMPOSITE_EVIDENCE_AND_HUMAN_GUIDED_CONTEXT.md).

The intended outcome is not the largest ontology. It is the smallest sufficiently expressive epistemic architecture that continues to behave coherently when difficult human situations make the graph, the evidence and the reasoning messy.


## 13. Discussion checkpoint — investigative questions, gaps, responses and claims

**Status:** Product-owner-confirmed conceptual directions below; not yet a normative schema or implementation claim. This checkpoint records the discussion so it can be resumed without reconstructing it from chat. The review/reconciliation method above still applies. Names, example enum values and persistence choices are illustrative unless explicitly promoted into a contract.

### Agreed conceptual directions

#### Unknown, InformationGap and Question

- UNKNOWN is an epistemic state of something already represented, not a mandatory first-class Unknown entity.
- InformationGap is a first-class contextual absence of information relevant to an investigation, hypothesis, objective or decision. Its identity concerns the missing semantic aspect, not the wording of a question. The existence of a prior relationship and the nature of that relationship are distinct gaps.
- Question is a first-class formulated investigative problem. No mandatory UNKNOWN → InformationGap → Question pipeline: questions may arise independently, gaps may exist without questions, and associations may be discovered retrospectively.
- A gap can matter to multiple objects without duplicating the gap. Refining the missing semantic aspect can create a new gap with its own identity and genealogical relationship, rather than overwriting the original.
- Distinguish the semantic Question from QuestionAct (a concrete occurrence of asking it, including actor, recipient, time and context); repeated asking does not necessarily create a new semantic question. Different formulations may express the same question. The exact first-class status and representation of QuestionFormulation remain open.

#### Relevance

- relevant_to is contextual and graded, not merely Boolean. An object may be highly relevant to one hypothesis and scarcely relevant to another.
- Distinguish degree of relevance, rationale and confidence in the relevance assessment. Relevance may change as an investigation evolves even when the endpoints do not.
- Preserve attribution, grounding and history of relevance assessments; the exact ordinal vocabulary, comparison semantics and whether every assessment needs independent identity are design details. Illustrative degrees: negligible, low, moderate, high, critical.

#### Resolution and investigative lifecycle

- Separate epistemic resolution (unresolved, partially resolved, resolved) from operational investigation status (active, suspended, closed). Neither entails the other.
- ResolutionAssessment is attributable, contextual, grounded and versionable rather than a sovereign status asserted by Niccolò. Different investigators may disagree.
- Assess coverage, evidence/inferences/assumptions, confidence, remaining contradictions and sufficiency for the investigative purpose. No universal threshold; the investigator may choose criteria appropriate to the particular gap.
- New evidence can motivate a new assessment without erasing prior assessments. Reactivating investigation is a separate operational decision.
- Partial resolution identifies clarified versus unresolved aspects rather than requiring a numeric percentage. Remaining aspects may become distinct genealogically related gaps; decomposition is optional.
- Question lifecycle separates investigative status, answerability and received responses. Multiple responses can conflict; receipt of an answer does not close the question or resolve a linked gap.
- Even a sole investigator with full operational authority sees that *they* made the closure or assessment. Authority is not epistemic infallibility. Preserve authorship, rationale and past decisions.
- Multi-investigator collaboration should accommodate autonomous investigative lines and proposed incorporation by an authorized lead, including partial adoption, rejection and simultaneous competing assessments. The Git pull-request analogy is illustrative only; it is not a requirement to implement literal branches or PRs. Rejected proposals must not silently erase intellectual provenance.

#### Response, Claim and Interpretation

- Response is an identifiable communicative event; distinguish its source content from derived claims and interpretations. Preserve original content and ambiguity. No mandatory extraction of claims from every response.
- A Response may address several Questions, including ones formulated later. Receiving a Response is not proof of epistemic resolution.
- Response–Claim associations are many-to-many: a response may yield zero or more claims; multiple responses may relate to one claim; claims may arise independently of responses.
- Distinguish extracted_from (provenance of a claim extraction) from evidentiary supported_by / contradicted_by relationships. These are different semantic assertions; one must not automatically imply the other.
- Preserve who made each extraction or evidentiary association, when, and the rationale. Such associations are contestable.
- The fact that Jane declared that she never spoke to Joe is distinct from the proposition that she never spoke to Joe. Evidence of a declaration does not establish the truth of its contents.
- Interpretations can compete and should remain attributable, grounded and independently assessable. Preserve the distinction between what someone said and what another actor understood.

### Pending decisions and reconciliation

- [ ] **Next discussion:** semantic identity of Claim: when apparently equivalent statements from different sources refer to the same semantic claim versus when distinct claims or occurrences must remain separate. Consider scope, time, modality, attribution, precision, source independence and contested equivalence without presupposing a deduplication algorithm.
- [ ] Refine semantic identity, equivalence, formulation and genealogy rules across Question, InformationGap and Claim; separate semantic identity from occurrence and representation where justified.
- [ ] Clarify Response/QuestionAct/communicative-act relationships and original content locators, including responses that address several questions or no previously formulated question.
- [ ] Refine Interpretation identity and its relationships to claims, content, communicative acts and other objects; clarify overlap with existing hypothesis/assessment abstractions.
- [ ] Refine relevance scales, rationale, confidence, temporal applicability and contestability; clarify exact representation rather than freezing example labels.
- [ ] Define lifecycle transition semantics, assessment revision/supersession and how authorized operational decisions are projected for single- and multi-investigator situations.
- [ ] Compare these directions against existing domain code, schema, contracts and tests before naming actual implementation gaps or deciding MVP slices. Identify adversarial cross-abstraction tests and migration precautions.

This checklist is non-exhaustive and records unresolved design work, not a mandate to implement everything immediately. Continue the epistemic review and improve the technical abstractions where evidence warrants it. Preserve confirmed semantic distinctions and human authority; do not silently convert open questions into settled requirements.

## 14. Discussion checkpoint — semantic reasoning, findings and revision

**Status:** Continuation of the owner-confirmed conceptual review in section 13. Sections 14–23 capture the directions agreed in subsequent discussion, not implemented behavior, a frozen ontology, or a commitment to implement every candidate in the MVP. Where section 13 records a pending discussion, the later sections resolve its conceptual aspects and leave technical representation/reconciliation open. Names are working names unless already contracted elsewhere.

### 14.1 Claim identity and lifecycle

- Distinguish semantic proposition, concrete occurrence, original representation, and attributed interpretation. Repetition does not establish independent evidence; independently originating statements must not disappear through semantic consolidation.
- Semantic equivalence is itself contestable. Scope, modality, temporal applicability, precision and attribution matter. Relationships may include equivalent_to, more_specific_than, entails, contradicts and partially_overlaps. Preserve the rationale, author, context and uncertainty of these judgments.
- Consolidation is reversible and must retain original IDs, occurrences, provenance and genealogy. Operational deduplication is not a license to collapse epistemic dependencies.
- Separate evidentiary support, actor stance, operational adoption and relevance. A claim may be retracted, rejected or superseded without erasing its historical occurrence or influence.
- Question formulation, original content locators and exact identity/representation mechanisms remain technical design questions; do not infer that every semantic distinction needs its own persistence entity.

### 14.2 Interpretation and Hypothesis

- Interpretation assigns meaning to an object, relationship or another interpretation; preserve the original material, interpreted object, author, context, rationale and alternatives. Its epistemic support, actor stance and operational adoption are separate.
- A substantial semantic change may warrant a new genealogically related interpretation. Abandoning it does not undo its historical consequences.
- Hypothesis expresses a proposition or composition to investigate or explain. Interpretation and hypothesis may overlap in semantic roles without requiring duplicate objects.
- Hypotheses may compete, complement, contradict, depend on, refine or overlap one another. Track support, stance, adoption, investigative status and contextual relevance independently. An improbable hypothesis can be investigated without being endorsed.

### 14.3 Premise, scenario, inference and argument

- Premise is a contextual role. PremiseAdoption records who adopts what, for which purpose/scope/context, under which conditions, with what rationale and duration. Adoption is not a declaration of truth. Different scenarios may adopt incompatible premises.
- Scenario has identity, purpose, relevant circumstances/evidence, conditions, assumptions, results, provenance and genealogy. It may model a real-world perspective rather than a counterfactual. Results carry their validity conditions when reused.
- Inference is an attributable reasoning act with available inputs, method/rule, premises, context, outputs and rationale. Distinguish validity, truth of premises, strength and contextual applicability; every dependency may be challenged. Record the reasoning artifacts actually available, not fictional claims of access to hidden model reasoning.
- Argument is an identifiable, composable grouping of claims, premises, evidence, inferences, conclusions or other arguments. Parts can be reused or challenged specifically. CounterArgument is a contextual argumentative role, not necessarily a separate ontology.
- Challenge targets an object or particular aspect with author, rationale and context; it need not supply a counterargument. Refutation is an attributable, contestable assessment, not an automatic global truth assignment.

### 14.4 Assessment and Finding

- Assessment is a transversal concept: assessor, object or relationship, assessed dimension, criterion, result, confidence, rationale, scope, time, grounding and genealogy. Different kinds use appropriate scales; no mandatory global score. Confidence in a judgment is not the judgment itself. Assessments themselves can be assessed.
- Finding is an identifiable formal result of an investigation, including partial or inconclusive results. Preserve source dependencies, supporting and opposing arguments, conditions, divergence, uncertainty and genealogy. Institutional endorsement is not truth.
- Findings reused in another investigation must retain traceable dependency on their original material and applicable contexts. Summaries or projections are allowed; irreversible compression that removes the ability to trace or reassess the finding is not.
- Discovery of a forged artifact, contradiction, mistaken identity, deceptive actor or contaminated inference can trigger the same dependency-based review. A potentially compromised dependency creates a need for reassessment, not an automatic conclusion of falsehood. Independent support and the earlier epistemic history remain visible to authorized users.

## 15. Decisions, agency, temporal and institutional reasoning

- Decision, Authorization and Execution are distinct acts/states. Record the responsible decision maker, alternatives, reasons, risks, conditions, scope and genealogy. A decision need not lead to an action.
- Authority, Authorization and Delegation are distinct. Represent scope, temporal validity, constraints and revocation. Formal authority and practical influence may diverge. Real-world authority represented in an investigation does not automatically confer software privileges.
- Goal, Intention and Plan are distinct and can have many qualified associations; each can be declared, attributed, inferred or hypothesized. Do not infer an intention merely from an outcome.
- Event, Action and Outcome: outcome can be a contextual role of an event or change of state. Sequence is not causality. Distinguish action, omission, consequence, responsibility and intent.
- CausalRelationship is a qualified, contestable relationship where needed: causes, contributes, enables, prevents, necessary/sufficient factor, multiple joint causes, alternatives, context, temporal scope, evidence and attribution. Causation is independent of moral, legal or institutional responsibility.
- Responsibility, Obligation and Accountability have actor, scope, governing source, time and contestability. Authority, participation and responsibility are not interchangeable.
- Norm, Policy and Commitment have origin, conditions, exceptions and validity. Existence, formal validity, applicability, compliance, perceived legitimacy and practiced versus stated policy are distinct. Their authors, interpretation and application may themselves be investigated within authorized scope.

## 16. Human relationship with knowledge — a core product differentiator

Belief and Knowledge alone are too strong and too narrow to capture the ways humans relate to information. Use a contextual family (working name ActorEpistemicRelationship) rather than a single binary or universal score.

- Distinguish availability/exposure, actual access, perception, attention, examination, comprehension (partial or mistaken), perceived importance, retention/recall, belief and knowledge.
- Represent weak or intermediate epistemic stances: suspicion, inclination to believe or disbelieve, doubt, belief, claimed/perceived knowledge and suspended judgment. Do not collapse reluctance to attend into disbelief.
- Interest may be personal, instrumental, institutional or other; also distinguish indifference and aversion to acquisition of knowledge. Curiosity is a specialization of Interest.
- Persistent dispositions (e.g. tendencies regarding attention, trust, acceptance criteria, seeking knowledge and revising beliefs) differ from immediate reactions to an information item. Neither should be a global personality diagnosis or ungrounded label.
- Deliberately deciding to disregard something is an epistemic act with context and possible reasons; distinguish it from inattention, simple lack of interest, incomplete comprehension and aversion to knowing. Other acts include investigating, deepening, deferring, avoiding acquisition, delegating and reconsidering.
- Actors may desire that other people know, remain unaware, examine or accept something; these desires can motivate sharing, withholding and influence.
- For every such relation, distinguish self-report, observed manifestation, attribution, inference and hypothesis; capture context, intensity, time, provenance, confidence and alternative interpretations as appropriate. State, transition and explanation of a transition are independent. Motives may be stated or inferred and may themselves be misleading.

## 17. Influence, politics and higher-order knowledge — a core product differentiator

- EpistemicInfluence can concern another actor's attention, interest, curiosity, trust, beliefs, acceptance criteria, investigation of a subject and other epistemic relations. Intention, act, target, actual effect and perceived effect are distinct; influence can be unintentional, indirect, reciprocal, asymmetric or unsuccessful.
- EpistemicPower (capacity to influence) is distinct from exercise and outcome, formal authority and practical power. Informational intermediaries, alliances, coalitions and dependencies can produce collective convergence, polarization, silencing or circular sources; convergence alone is not coordination.
- EpistemicAsymmetry may concern information, access, understanding, knowledge about others' knowledge, or influence. Its existence, perception, intentional use and consequences are distinct. An advantage is not proof it was used.
- EpistemicTrust is directed, contextual and multidimensional (competence, sincerity, accuracy, impartiality, transparency, completeness, etc.). Declared/manifested/attributed trust, dependency, actual support and reliability are distinct.
- EpistemicReputation is community-dependent. Distinguish individual trust, collective reputation and actual support. Reports may share provenance; reputation does not prove reliability.
- MetaEpistemicState represents higher-order knowledge, belief, assumptions, attitudes or perceptions about one's own or others' epistemic states or acts. Support relations to other relations/states without fixed semantic depth; bound actual processing by user-defined purpose, relevance and budget.
- These are subjects of investigation in the modeled reality; representing an actor's influence strategy does not grant Niccolò authority to exercise it. No universal ranking of power, influence, trust or reputation.

## 18. Pervasive Deception — foundational principle of Niccolò

Deception is not a vertical, exceptional event category, or moral presumption. The model must remain open to varying levels and kinds of honesty throughout the epistemic domain. Do not assume honesty or dishonesty as a global default. Describe conduct, possible intention, motivation, context and consequences separately. Legal, institutional and moral assessments, when relevant, are attributable and distinguishable from factual descriptions.

### 18.1 Distinctions and motivations

- Falsity is not intentional deception. True statements, selective omission or truthful fragments may produce a misleading impression; omission can also be accidental, unimportant to the speaker, or a reasonable reaction to context.
- A junior employee may withhold that a COO convened a problematic meeting for fear of hierarchical retaliation or job loss. Such possible reasons do not automatically establish deception, dishonesty, moral wrongdoing or innocence.
- Ego is a crucial potential motivation, including saving face, avoiding momentary embarrassment, exaggerating competence or appearing important, independently of money, position or long-term strategy. Distinguish reactive actions from planned strategies.
- Self-deception is distinct from deceiving others, not necessarily deliberate, and may involve rationalization or a desired/projected self-image (candidate SelfImage / DesiredImage / ProjectedImage roles). Do not diagnose a person's psychology from ambiguous conduct.
- Separate deceptive intention, attempt, means, target, intended effect, actual effect and perceived success; recipients and effects may differ. Sincere transmission does not imply source accuracy or integrity.

### 18.2 Six pervasive dimensions

1. Composition: true elements can be selected, ordered, associated, generalized or framed such that the whole is misleading; component truth is independent of composite meaning.
2. Higher-order deception: accusations of deception, claimed motives, knowledge and confidence about deception can themselves be deceptive. Each layer is independently uncertain; computational depth must remain bounded.
3. Inadvertent propagation: distinguish origin, transmission, transformation and assimilation. Shared ancestry does not provide independent corroboration; the intermediary may be entirely sincere.
4. Contaminated dependencies: discovery can motivate reassessment of downstream findings, hypotheses, decisions and recommendations. Retain original findings, dependency provenance, independent support, and past reasoning. Do not blanket-invalidate or silently compress findings.
5. Reflexive investigation: investigations, their diligence, criteria, apparent independence, scope, actors and institutional adoption are themselves subject to deception. Approval does not erase dissent.
6. Universal representability: actors may misrepresent any relevant epistemic dimension (attention, curiosity, disinterest, suspicion, belief, knowledge, authority, reputation, etc.). Do not duplicate each entity into a specialized deception counterpart or presume every actor deceptive.

### 18.3 Representation and lifecycle

- Deception may be an identifiable composition reusing intentions, acts, omissions, motives, targets, causal links and assessments. Elementary and composite occurrences can have qualified relationships such as part_of, sustains, conceals, derived_from, coordinates_with, reinforces.
- Preserve each component's provenance and contestability; grouping does not prove shared intent or coordination.
- Lifecycle is multidimensional: intention, actual execution, effect per recipient, discovery per actor, stance and evaluation. Use the general revision and dependency mechanisms; no separate deception-specific epistemic engine is required.
- The user is also an actor. Both retrospective and prospective scenario modeling can represent the user's conduct and perceptions; strategic analysis is distinct from execution and subject to product recommendation boundaries. Representation does not imply endorsement or operational authority.

## 19. General strategic relations, information compromise and investigation integrity

- Strategy / StrategicPosition concerns interdependent actors, goals, resources, constraints, power, epistemic states and possible reactions. Strategies depend on perceived circumstances, even when these are mistaken; attribution of another's strategy is contestable. Dynamic revision is required; sophisticated strategic planning is not automatically an MVP requirement.
- Loyalty is a qualified relation to a person, group, institution, cause or principle; its manifestations, declarations and effective existence may differ. Conflict of interests is independent of misconduct. Loyalty does not imply obedience, honesty, coordination or competence.
- Cooperation and Coordination are distinct; coordination may be explicit, tacit, centralized or distributed. Distinguish spontaneous convergence from actual coordination; Collusion may be a qualified form of coordination with relevant normative context. Separate communication, agreement, pattern, shared objective, causation and individual intent. Participants need not know the entire group or goal.
- InformationCompromise is a contextual composition of assessments of confidentiality, integrity, authenticity, availability, reliability and provenance. A genuine original may be false; an authentic, intact artifact can have lost confidentiality. Compromise may be partial, inadvertent or deliberate and is not necessarily deception.
- InvestigationIntegrity is a multidimensional, contestable assessment/composition, not binary or a single score. Include independence, evidence selection and provenance, reasoning, selective attention/exclusions, criteria and their consistent application, consolidation, dissent, historical traceability and the possibility of review. Integrity is a product differentiator, not a claim of infallibility.
- Distinguish investigative lines that were not perceived, deemed irrelevant, suspended/abandoned, prevented or rejected; examine their rationale, authority and effects. Separate authorship, applicability, mutability and authority of protocols. Preserve internal dissent and reasons for adoption/rejection subject to access policy. Proactive fragility suggestions are not accusations.

## 20. Interaction review — formation of knowledge and investigation

- Gaps, questions, responses, claims, hypotheses and findings interact without a mandatory linear pipeline. Revisit missing information as the situation changes.
- Distinguish support for a hypothesis from ability to discriminate among hypotheses; one observation may support several competing explanations equally.
- Distinguish no evidence, evidence of absence, unavailable evidence and evidence not yet collected.
- DiscriminatoryPowerAssessment is contextual, compares existing or potential information with competing hypotheses, and distinguishes apparent/actual and potential/realized value. Combinations can discriminate when individual facts cannot. Reliability, information discrimination, priority and confidence are independent. Qualitative judgments are acceptable.
- Apparent support may reflect unformulated hypotheses. Identify unexplained residues, assumptions shared by all current hypotheses, contrary evidence, alternative perspectives and explanatory failures. Distinguish exploratory questions from discriminating questions; use deterministic structure where possible plus bounded LLM suggestion under human direction.
- Review not just answers but the questions asked, omitted, poorly framed or lacking discrimination. A reviewer/corregedoria can examine investigative imbalance without assuming bad faith.
- Investigators are actors whose actions may change the modeled circumstances. Inquiry can alter trust, fear, ego, information exposure, cooperation and deception. Separate preexisting conduct, conduct induced by the investigation and uncertain origin. Investigative reactions do not establish the original suspicion.
- Formal authority, effective influence, agenda control, dependency, strategic vulnerability and institutional adoption can influence investigation design and reception. The political or institutional environment is part of the modeled reality; do not create a parallel PoliticalInterference engine.
- Strategy depends on the actor's perceived reality, including false beliefs about other strategies; alternative reactions and outcomes are hypothetical, not guaranteed.
- **Explicit MVP constraint:** Niccolò must not automatically allocate or dynamically redistribute investigative resources (including tokens, processing effort or attention budgets) among lines. It may identify promising directions and recommend redistribution with reasons. The user determines and may change the allocation; show the current division and actual use intelligibly (e.g. 70% for one line, 30% for another). Use simple predictable defaults where needed; no autonomous optimizer or hidden certainty-based scheduler. Reconsider dynamic allocation only after demonstrating its value and obtaining explicit product direction. Do not let scheduling sophistication delay the MVP.

### 20.1 Informational universe and evidence artifacts

- Niccolò reasons over information, not the physical world directly. Distinguish a real-world referent, its representations/records, their provenance, and assessments about them.
- The removal of a DNA sample from a physical evidence container is conceptually comparable to removal of a component from a composite digital artifact. Both are known through available information about an artifact, its parts, transformations, custody, availability and integrity.
- Candidate general EvidenceArtifact concept: identity, composition, origin, custody/collection history, transformations, integrity, representations and availability. No separate physical-evidence management system or blanket PhysicalEvidence abstraction is needed. Physical evidence is represented through supplied records, descriptions and analytical results; digital artifacts may be processed where supported. Preserve explicit MVP media limitations.
- The processing universe is informational, but the subject of information and the scope of appropriate recommendations can include real-world actions and outcomes.

### 20.2 ActionRecommendation — proactive strategic advice

ActionRecommendation is an identifiable, attributable recommendation with rationale, purpose, context, intended recipient, conditions and expected consequences. It can be requested or proactively proposed by a human or Niccolò; it remains evaluable, contestable, refinable and replaceable. It is not a Decision, Authorization, Plan or Execution.

- Purposes are open and non-exclusive: epistemic (acquire knowledge, test hypotheses, verify claims), protective, preventive, corrective, strategic and exploratory. An action can serve several purposes.
- It may be grounded in evidence, hypotheses, suspicion, gaps, risk assessments, perceived intentions or scenarios. Confidence in a motivating hypothesis differs from strength of recommendation: reviewing privileged access may be justified by structural risk without concluding that an employee is dishonest. An authorized forensic laboratory analysis of a suspected substance may distinguish claimed from actual composition; Niccolò reasons about the information need and the result rather than physically handling evidence.
- Include feasibility, likely authorized executors, conditions, expected benefits, primary/secondary and intended/unintended consequences, relevant uncertainties, possible reactions, and effects on other investigations.
- Consider alternatives, including inaction. Compare cost, urgency, expected value, risk, reversibility, dependencies and discriminatory power. Do not impose a universal score or autonomously choose competing human objectives.
- Conditional recommendations can depend on another result, confirmation, event or change in context; include delay, suspension and conditional reassessment.
- Distinguish recommendation, assessment of it, decision, authorization, planning, execution, actual outcome and retrospective evaluation. Preserve their independent provenance and lineage.
- Interventions change the very circumstances investigated: objective conditions, information distribution, beliefs, relationships, resources, power, incentives, opportunities and investigative evidence. Distinguish spontaneous behavior from behavior potentially induced by an intervention.
- Actors adapt based on perceived reality, with errors, fear, ego, resentment, self-preservation, changing intentions and possibly simulated adaptation. Do not assume rationality, honesty or predictability.
- A recommendation may become inappropriate even if all its original supporting facts are still true: circumstances, goals, strategy, opportunity or evidence interpretation can change. Recommend reevaluation, suspension or replacement, including during execution, retaining the original rationale and history. Reuse general dependency and revision mechanisms.
- Recommendations may complement, depend on, conflict with, interfere with, duplicate or replace one another. Ordering and conditionality matter; use Plan for a proposed composition or sequence rather than create parallel planning machinery. Identify conflicts between objectives (e.g. operational safety, productivity, investigation opportunity, reputation) and let the user decide.
- MVP: identify and explain material dependencies and alternatives without an autonomous strategic optimizer.
- **Explicit product boundary including air-gapped deployments:** Niccolò may represent, investigate and analyze kinetic attacks and the consequences of death, injury or incapacitation, including protective and preventive scenarios. It must not recommend or develop operational plans for kinetic attacks against people or installations, directly or through euphemisms such as removal/neutralization of an actor. Apply the restriction semantically in recommendation policy, not by weakening the epistemic ontology. Consequence analysis does not imply permission to advise causing the harm. Human decision and authorization remain separate from system recommendations.

## 21. Counterintelligence — a vertical and stress test of the general epistemic core

The counterintelligence study demonstrates capabilities that should enrich the universal core rather than introduce a parallel subsystem. The following nine areas were explored and approved conceptually. Their boundaries are contextual and many overlap with ordinary investigations.

### 21.1 Informational exposure

- InformationAccess distinguishes formal authorization, practical/technical possibility, indirect access, conditional access and formerly available/revoked access from actual access. Exposure means availability or receipt, not necessarily assimilation.
- Distinguish receipt, perception, attention, comprehension, appreciation of significance and retention. Revoking access does not erase acquired knowledge.
- Disclosure distinguishes intended/unintended, authorized/unauthorized, intended/actual recipients and outcomes. Restriction may be legitimate or not. Propagation preserves transformation and provenance.
- A person may independently infer sensitive information from combined public information, context or specialist understanding without ever accessing the originating artifact. Seemingly harmless facts may become sensitive in combination or after changed circumstances.
- Separate actual from potential and presumed knowledge; actor expectations regarding secrecy may be wrong. Consider simulated knowledge, ignorance, comprehension or access capability.
- Confidentiality of an information object and knowledge held by other actors are separate.

### 21.2 Detection of compromise

- Anomaly examples: apparent unexpected knowledge, information in an unexplained context, apparent anticipation of confidential decisions, changes to supposedly preserved information.
- The reference baseline (roles, privileges, ordinary communication, inference capabilities) is itself contextual, incomplete, contestable and investigable.
- Distinguish confidentiality, integrity, authenticity, availability and provenance, and assess scope and affected dependencies/actors/decisions separately.
- Competing explanations include intentional leakage, inadvertent exposure, access-control failure, legitimate access unknown to the investigator, independent inference, coincidence, investigator error and deceptive appearance of compromise.
- An anomaly invites investigation; it is not proof of compromise. Reasonable protective review can be warranted before confirmation.

### 21.3 Reconstruction of leaks and attribution of origin

- Distinguish originator, earliest known holder, first unauthorized discloser, conscious/unconscious intermediaries, intended and actual recipients, beneficiaries and alleged organizers. These roles do not imply one another.
- Propagation reconstructions can be incomplete and competing; separate distinct routes, common ancestry, distributed fragments and independent inference. Preserve transformations (simplification, distortion, omission, interpretation, recombination); a later expression need not be identical to the original.
- A disclosure may occur without awareness of confidentiality, strategic importance, authorization boundaries or the recipient's existing knowledge. Causal participation does not establish intent.
- Possible motives include finance, hidden loyalty, revenge, ego, recognition, ideology, protection of others, whistleblowing or coercion. Claimed motives are subject to deception.
- Consider erroneous or deceptive attribution: manipulated evidence, false accusations/confessions, incomplete admissions, apparent use of a channel never used. Investigate the attribution process itself.
- Separate creation time, acquisition per actor, disclosure, discovery and attribution; a past disclosure may become strategically important later. Finding one leak does not establish its full recipients, content, duration, other routes or consequences.
- Responsibility dimensions: causal participation, awareness, intent, motive, authorization, institutional responsibility and benefit. Benefit is not proof of participation.
- Returning information (circularity) is not necessarily bad. A recipient may fail to recognize their own original information and count it as independent confirmation; or recognize it as evidence of intended dissemination/influence; or as possible breach of expected secrecy; or as a signal of transformation, reinterpretation or connections between actors.
- Distinguish intended, expected, authorized, actual and perceived propagation; the same returned information can be a strategic success for one actor and a compromise for another. A return is an indication, not proof of the full path or effect.
- Model reconstructions using existing events, causal relationships, hypotheses and assessments; no dedicated leak-specific epistemic engine.

### 21.4 Infiltration and hidden loyalties — core strategic capability

**Loyalty refinement**
- Objects may include persons, family, groups, organizations, institutions, causes, communities, ideology, religion, principles or relationships. Loyalty to an institution can coexist with opposition to its current leadership.
- Nature may be affective, ideological, economic, professional, institutional, familial, grateful, reciprocal or dependency-related; types can coexist. Degree, scope, conditions, personal limits, reciprocity and temporal change matter.
- Loyalty is distinct from obedience, submission, agreement, compliance, cooperation, dependency and aligned interests. One can obey someone one despises or challenge someone one is loyal to.
- Multiple concurrent loyalties may be compatible, independent, conflicting or latent until a choice becomes necessary. Relative importance is context-specific and may be declared, inferred, unknown to the actor or change with circumstances.
- Distinguish declared, manifested, perceived, attributed, suspected, hidden, simulated and misattributed loyalty. Reciprocity need not exist and may be wrongly assumed. Trust (including competence and predictability) is independent of loyalty.
- Loyalty can arise, strengthen, weaken, disappear, recover, transfer or become ambivalent; perception of change may lag. Humiliation, resentment, abandonment, belonging, admiration, recognition, ego or emotional injury may matter independently of economic reward.
- Self-perceived loyalty can diverge from others' evaluations and one's own past commitments. Avoid diagnosing motives as fact.

**Infiltration refinement**
- Formal affiliation, actual cooperation, convergent interests, independent conduct and hidden relations are different. No single factor establishes infiltration.
- Distinguish preexisting intention upon entry from later-developed ties, occasional cooperation, gradual compromise, coercion, opportunism and changes of conviction.
- Privileged access, legitimate work responsibilities, actual access, influence over access holders, purpose, capability, intent and behavior are independent.
- Investigable possible purposes include acquisition of knowledge, observation, influencing decisions, protecting interests, informational compromise, interference with investigation and benefit to others. Purpose is a hypothesis, not inferred simply from outcome.
- Distinguish affinity, spontaneous alignment, cooperation, coordination, subordination, dependency, coercion and external control. An actor may benefit another organization without any contact. Participants may be unaware of ultimate beneficiaries or larger strategy, or misunderstand the purpose of their activity.
- Dual or conflicting engagements can have different commitments and perceptions per party, changing over time. Infiltration may be authorized (e.g. lawfully authorized undercover investigators); preserve the distinction among apparent membership, actual affiliation, mandate, purpose and authority, without intrinsic moral judgment.
- Loyalty or disloyalty may be simulated or wrongly perceived. Accusations may arise from investigative error, personal/political disputes, bias, manipulated evidence or intentional deception.
- An accusation itself has strategic effects independent of truth: trust, exclusion, authority, investigations, alliances and reputation.
- Divergence, dissent, criticism, noncompliance, conflicting interests, whistleblowing, personal relationships, kinship, professional communications and shared convictions do not establish sabotage, disloyalty or clandestine coordination. Conduct may arise from fear, obligation, incompetence, ego, pressure, interest or coincidence. Independent evidence, common ancestry, and alternative explanations must be traceable.
- Absence of observed suspicious activity is not automatically proof of a particularly competent infiltrator; identify self-sealing hypotheses that cannot be contradicted. Distinguish insufficient observation from actual counterevidence and consider hypothesis discriminability.
- Assess competence, trust, loyalty, access, influence, conflict of interests, exposure to coercion and observed behavior separately, contextual and attributable; no global loyalty or suspicion score.
- Multi-actor hypotheses preserve alternatives concerning actual participation, intermediaries, beneficiaries, organizers, unwitting actors and unjustly implicated people.
- Protective recommendations may address structural risk with proportionate access review, segregation of duties, evidence preservation, independent review and reduced excessive dependencies without declaring someone an infiltrator.
- Investigation itself can change loyalty, fear, resentment, solidarity and relationships; distinguish preexisting from induced conduct. Investigators may also have hidden loyalties, dependencies and imperfect knowledge.

Working abstractions: Loyalty (qualified relation), Affiliation (membership or relationship), StrategicAlignment (convergence of goals/interests), Infiltration (investigable composition reusing intentions, access, actions, influence, deception, coordination, loyalty, knowledge and assessments). Exact persistence choices await reconciliation. Sophistication about possible betrayal must never convert distrust into knowledge automatically.

### 21.5 Deception against counterintelligence

- Counterintelligence is itself a potential epistemic target. False confidence in security, false perception of compromise and erroneous extent or timing may occur independently.
- Distinguish actual participation, appearance, accusation, attributed origin and benefit. Even a detected incident can be misattributed or misunderstood.
- Agenda compromise: apparently rigorous investigations may pursue wrong questions, overvalue secondary lines, exclude hypotheses or fail to challenge assumptions. Alternative explanations include deception, incompetence, resource limits and legitimate choices.
- Normality baselines may themselves be incorrect, including normalized misconduct or legitimate behavior interpreted as suspicious. Reassess source trust/reputation, hierarchy-based credibility and information asymmetry.
- Separate actual, demonstrated and attributed adversary capabilities, knowledge, resources, influence and limits. Distinguish apparent/declared/attributed/actual goals; distinguish coordination from spontaneous patterns.
- Institutional summaries can misrepresent technically correct findings through omission, decontextualization, selection or exaggerated confidence; preserve provenance. Operational closure may reflect resources, policy or politics rather than epistemic resolution.
- Self-deception of investigators (ego, attachment to their own hypotheses, resistance to public correction) is investigable, not automatically presumed.
- Overreaction to an actual or imagined threat may cause greater damage than the original threat; consider introduced risks.
- Recursive deception (including deception about an allegation of deception) remains representable without mandatory unlimited computation; use purpose, relevance and user budget. Self-sealing arguments (all evidence and counterevidence supposedly confirm the same hypothesis) merit explicit challenge.
- No CounterintelligenceDeception engine. Investigators, investigations, policies, methods and findings, including work aided by Niccolò, can themselves be investigated within authorized scope.

### 21.6 Integrity of counterintelligence and public or corporate oversight

Counterintelligence integrity is a contextual application/refinement of InvestigationIntegrity, not a separate fundamental abstraction. A specialized intelligence adversary may know investigative methods, possess institutional authority/resources, investigate investigators and adapt to their activity. The complexity of the adversarial environment, not its institutional label, determines the necessary sophistication. A corporate investigation can be more adversarially complex than a formal counterintelligence case.

- An oversight body can accumulate informational, investigative, disciplinary, hierarchical, reputational and career-related power; individuals can appropriate institutional powers and information for private purposes.
- Knowledge of a person's actual or alleged misconduct can create real or perceived leverage, even absent formal authority or a completed disclosure. Coercion may persist, and subsequent misconduct under pressure can deepen dependence. Identical observed conduct can originate from loyalty, money, fear, coercion, dependence or combinations, with distinct responsibilities.
- Independence is formal, operational, financial, informational, decisional and perceived; formally independent bodies can depend on those they investigate.
- Selective oversight can affect subjects, depth, resources, evidence criteria, interpretation, closure or referral. It can arise from legitimate priorities and limitations, interest, discrimination or misconduct; compare context and rationale before concluding wrongdoing.
- Institutional capture can be partial, temporary or persistent, by individuals or groups, economically, hierarchically or politically driven, recognized or unrecognized. Not every investigator or every case is compromised.
- Legitimate acquisition of institutional information does not imply legitimacy of subsequent use. Investigate private appropriation, selective enforcement, concealment, coercion, favoritism and protection networks. Such networks can arise from loyalty, reciprocal interests, fear, dependence, convenience or coercion, without centralized coordination or full participant knowledge.
- Compare allegedly similar cases using relevant differences in conduct, evidence, circumstances, criteria, decisions and rationale; unequal treatment is not automatically unjustified.
- Handling of a complaint (including non-investigation, superficial examination, misinterpretation, closure or inappropriate disclosure) is itself investigable.
- Recursively represent investigation of investigators, supervisors, oversight processes and their policies without requiring automatic investigations of every layer. Scope and access remain explicit.
- Compromise may be localized to an individual, team, phase, source or decision. An honest investigator may reason from contaminated information. Do not propagate negative assessments indiscriminately across individuals, cases or institutions.
- Examine actual autonomy to disagree, formulate alternatives, preserve assessments, challenge evidence, request review, and refuse improper interference. Formal authority does not guarantee actual autonomy.
- Preserve original evidence, provenance, dissent, alternative interpretations, decision rationale, changes in scope and conclusion genealogy so an authorized independent reviewer can reassess. A senior investigator's rejection cannot erase the underlying assessment.
- Protective recommendations include independent review, segregation of responsibilities, reduction of excessive privileges/dependencies, decision audit, record preservation and protection of people who report misconduct; no autonomous execution.
- These refinements enrich general InvestigationIntegrity. The same mechanisms apply to independent audits, internal corporate investigations, police oversight and other institutions.

#### Universal family of power, vulnerability and dependence

The oversight example exposed a broader family of concepts, all owner-approved as universal rather than specific to police or counterintelligence. Context, intensity, temporal qualification, evidence, assessment authorship, uncertainty, competing perceptions, deception and genealogy apply throughout. Names and exact entity/relationship choices remain working design.

| Working concept | Core distinction |
| --- | --- |
| Vulnerability | A contextual exposure to possible adverse consequences, not necessarily a personal deficiency. May be material/economic, professional, institutional/hierarchical, legal, reputational, emotional or informational; actual, potential, perceived, unknown or misattributed; transient, conditional or persistent. |
| Dependency | Attainment of a goal or maintenance of a condition depends on an actor, resource or circumstance. May be unilateral, reciprocal, asymmetric or distributed; material, affective, professional, informational, institutional or strategic; distinguish real alternatives from alternatives known/perceived to the actor. Voluntary or desired dependency is not intrinsically submission. |
| Leverage | Capacity for influence or advantage arising from resources, competence, authority, knowledge, dependence or vulnerability (not exclusively a negative vulnerability). Distinguish actual/potential/perceived/simulated capacity, intent to exercise, exercise, perceived exercise and outcome, including resistance or unintended effects. Holding leverage does not establish intent to use it. |
| Threat | Distinguish a condition threatening an interest from a communicated threat. Separate conditionality, sincerity, actual capability, intent, communication, perceived credibility and consequence. A perceived or simulated threat can produce real effects. |
| Coercion | Influence that compromises actual or perceived freedom of decision; explicit/implicit, direct/mediated, episodic/recurring/continuing. Distinguish submission, apparent or partial submission, resistance and seeking protection. Consequences may include behavior change, worsened dependency, altered loyalty and diminished autonomy; submission is not consent or loyalty. |
| Incentive | A condition that encourages/discourages behavior; economic, affective, reputational, professional, ideological, recognition, belonging or ego-related. May arise naturally, institutionally, through deliberate offering or only in the actor's perception. Perception and behavioral effect differ; may be irrelevant or counterproductive. |
| Pressure | Broader than coercion; hierarchical, social, economic, familial, institutional, psychological or self-imposed, deliberate or incidental, from individuals, groups or circumstances. May be cumulative, intermittent, increasing or decreasing. |
| Constraint | A real or perceived limitation on alternatives: physical, material, legal, institutional, informational, temporal, moral or voluntarily accepted. Perceived lack of alternatives can differ from actual options. |
| Autonomy | Contextual actual and perceived ability to decide, act, refuse, obtain knowledge, disagree, exit a dependency or revise commitments. Degrees and scope matter, not a binary or global trait. |

Separate conduct and causal consequences from inferred motives, responsibility and moral/legal judgments. Understand what people believe they can do, what they fear and what they consider impossible to refuse, as well as what they desire. These concepts enable investigation of coercive relationships, protection, governance and risk; representability is not authority to use another person's vulnerabilities for coercion.

### 21.7 Informational and epistemic compartmentalization

- Distinguish compartmentalization in the modeled world from actual Niccolò product access control. Actual software permissions are deterministic security controls, not inferred from modeled authority or delegated to LLM judgment.
- Any epistemic object may be compartmentalized: source identity, evidence, claims, hypotheses, questions, interpretations, findings, intent, recommendation, investigative lines and even existence of an investigation.
- Fine-grained access to existence, content, partial content, source identity or origin, conclusion versus rationale; distinguish authorized access, technical possibility, actual access, assimilation and retained knowledge. Revocation does not erase acquired knowledge.
- Restrictions may protect people, sources, operations, confidentiality, parallel investigations or legal obligations, or reflect improper private interests. Existence of a restriction does not prove legitimacy.
- Distributed knowledge: different investigators may hold fragments whose combination yields a relevant hypothesis that none can formulate independently. Do not confuse an investigator's limited view with the totality of known or existing information.
- Distinguish knowing an inaccessible object exists from not knowing of it and incorrectly believing it absent. When existence is itself protected, Niccolò must not leak it indirectly.
- Changes in rights may be temporal, conditional or role-related; preserve authorization change provenance. Authorization to access does not imply authorization for all uses or disclosures; purpose matters.
- Derived content, graphs, summaries, metadata, associations, inferences and LLM processing must respect effective authorization, including restrictions on derived outputs. Confidential inputs cannot be exposed through outputs or unjustified existence hints.
- Compartmentalization can obscure contradictions, source independence, hypotheses, relevant combinations and review, cause duplication or impair discriminatory power. A conclusion may be justified relative to the available subset but inadequate given broader knowledge.
- The reason and application of a restriction can itself be investigated (legitimate, unnecessary, mistaken, abusive, or deceptively founded). Preservation and authorization are distinct operations: access restriction must not imply evidence destruction or erased dissent.
- Candidate InformationCompartment represents overlapping, dynamic information groups and access/use/sharing conditions in the epistemic model, reusing authority, authorization, access, knowledge, dependency and assessment. Avoid assuming the MVP must operationally implement all fine-grained variants. Where the product does support protected compartments, enforce actual isolation for users and LLMs deterministically and do not leak through unsupported reasoning paths.

### 21.8 Informational, human and institutional vulnerabilities

- Separate existence of vulnerability, exposure, others' knowledge, incident, actual consequence and discovery. Vulnerability need not imply an adversary or malicious intent.
- Emergent vulnerabilities arise from combinations (responsibility concentration, dependency, inadequate independent review, excessive confidence, poor recovery) that may be innocuous alone. Relational vulnerabilities reside in asymmetry, conflicting obligations, concentrated authority or lack of alternatives, not necessarily intrinsic personal traits.
- Individual competence does not guarantee collective competence. Consider conformity, absent challenge, fragmented knowledge, institutional resistance to correction and common-source dependency.
- Epistemic vulnerabilities include unquestioned premises, interpretation lock-in, mistaken source independence, authority confused with credibility, omitted hypotheses, resistance to revision and confusing absence of evidence with evidence of absence.
- Success can itself create fragility (excessive confidence, reduced oversight, reputation insulated from challenge, overgeneralized successful methods). Ego/self-deception remain possible explanations rather than diagnoses.
- Epistemic single points of failure: multiple findings, investigations or decisions depend on one originator, unverified premise, interpretation or unique knowledge holder.
- Protection and oversight are themselves investigable and may have second-order weaknesses: record verification, source assessment, exceptions, review and reviewer independence.
- Protection can introduce new vulnerabilities: excessive compartmentalization, centralization, excessive scrutiny, operational dependencies, impaired contradiction detection. Separate residual and introduced risk.
- Distinguish actual, perceived, attributed, unknown, simulated and mistakenly assumed vulnerability; temporal/contextual variation matters. Severity, likelihood, scope and reversibility are independent rather than a global absolute score.
- Actual exposure does not establish an adversary's knowledge, capability, interest, resources or intent.
- Proactive structural warnings can be grounded without alleging wrongdoing, e.g. three seemingly separate findings all depend on a common information origin. Explain basis and uncertainty.
- Candidate Exposure (conditions in which a vulnerability can have consequences), Mitigation (measures to reduce vulnerability or impact), and Resilience (capacity to absorb events, preserve essentials, recognize compromise, recover information/reasoning/operational ability) are universal concepts. Disambiguate Exposure in a risk context from informational exposure and reconcile naming with existing semantics. Vulnerability and resilience are independent.

### 21.9 Protective and containment recommendations

Reuse general ActionRecommendation, Vulnerability, Mitigation, Resilience, InformationCompromise, InvestigationIntegrity, Assessment, Plan and Scenario; no parallel recommendation engine.

- Objects may include people, sources, identities, information, investigations, infrastructure, resources, decision capacity, autonomy, institutional integrity and continuity of operations.
- Distinguish prevention (avoid incidents), mitigation (reduce harm), containment (limit ongoing spread/impact), correction (address contributing conditions), recovery (restore capacities), and adaptation (prepare for changed circumstances); actions can serve multiple purposes.
- Recommend reasonable proportional protective measures based on structural risk even under uncertainty about whether a threat, leak, infiltration, hostile identity or malicious intention exists. State the uncertainty and basis. Do not convert suspicion into accusation.
- Proportionality involves potential consequences, quality of information, urgency, cost, reversibility, impact on possibly innocent people and legitimate activities. Structural measures such as general privilege review, segregation of duties, independent review, record preservation and reducing excessive dependencies may be preferable to personal confrontation.
- Source protection distinguishes identity, content, acquisition circumstances, relationships, consequences of identification and disclosure authorization; one exposure can compromise several investigations.
- Informational containment assesses affected information, potential recipients, channels, downstream decisions/investigations and whether compromise continues. Containment includes consequences, not just circulation.
- **Epistemic containment:** potentially adulterated evidence or another compromised dependency can prompt recommending a pause on reliance on relevant findings, review of downstream inferences and decision consequences, independent verification and investigation of the extent of contamination. Never blanket-invalidate, delete, or destroy provenance; retain independent support and history. This is a general dependency/revision capability.
- Preserve investigation capacity, source availability, evidence, independence, review and competing hypotheses; explain conflicts between protection now and continued investigation.
- Consider technical, material, social and reputational reversibility. Restoring access may be easier than restoring trust after an unjust accusation. Conditions, urgency, duration and review/expiry matter.
- Other actors may react by changing behavior, perceptions, loyalty, trust, resistance or cooperation; those reactions do not automatically confirm the initial suspicion.
- Protective measures may themselves conceal wrongdoing, concentrate power, suppress dissent, exclude reviewers, restrict access unfairly or retaliate. Motives, legitimacy and effects remain investigable.
- Analyze residual and newly introduced risks; a measure can reduce, eliminate, transfer or create risk, or have no material effect. Distinguish intended, implemented, perceived and effective protection. Absence of a subsequent incident is not proof of effectiveness.
- Epistemic recovery rebuilds capacity for reliable reasoning by identifying still usable material, recovering independent sources, revising affected conclusions, restoring controls, identifying remaining gaps and reassessing integrity. Preserve history throughout.
- Product supports advice before, during and after compromise even if adversary identity/motive/existence is unknown. Decision, authorization and execution remain human-governed; kinetic attack recommendation/planning prohibition still applies.

## 22. Cross-cutting invariants and MVP boundaries from this checkpoint

These are the principal decisions to carry forward when reviewing scenarios and existing code. They complement the existing MVP contracts and earlier sections; no implication that the implementation already satisfies them.

1. Human-guided circumstance domain; Niccolò and supporting LLMs do not become sovereign arbiters of truth, guilt, loyalty, morality, investigation closure or action. Preserve attribution of consequential decisions, even with a sole all-powerful investigator.
2. Treat observed conduct, attributed motives, hypotheses, declarations, beliefs, epistemic assessments, operational adoption, authority and actual outcomes as different dimensions. No automatic transition from suspicion, association, anomaly or benefit to culpability or knowledge.
3. Deception is pervasive and potentially recursive; maintain alternate explanations and attribution without assuming universal dishonesty or confusing sincerity with truth.
4. Preserve provenance, dependency, source independence, genealogy, dissent and revision. Derived findings and summaries must retain inspectable grounding for authorized review; preservation differs from broad visibility.
5. Relations can be qualified and investigable; context, time, uncertainty and differing perspectives matter. No fixed semantic depth, but computation has practical and user-controlled bounds.
6. Counterintelligence is a demanding vertical using and enriching the general core; avoid duplicated epistemic engines for oversight, deception, sources, leaks or protection.
7. All input and reasoning material is informational; real-world objects are referents of information, not a separate physical evidence management system. Real-world action advice is a distinct permitted output capability, within product boundaries.
8. Protect against undue informational exposure using actual deterministic authorization and its applicable derived-output constraints; represented organizational authority must not grant software access. Do not send unauthorized information to an LLM. Keep explicit MVP media understanding exclusions.
9. Proactive suggestions, warnings and ActionRecommendation are welcome and important, not automatic interventions. Keep authority, recommendation, decision, authorization and execution separate. No recommendations or operational plans for kinetic attacks, including in air-gapped variants and disguised by euphemism.
10. **MVP resource allocation remains explicitly user-controlled, inspectable and predictable**, including investigation-line token/processing budgets. The system can recommend allocation but not autonomously reallocate. Avoid elaborate autonomous strategic optimizers for MVP.
11. Candidate names, examples, grades/enums and conceptual groupings do not mandate classes, tables, services, algorithms or global scalar scores. Reuse existing semantic mechanisms, identify coherent minimum slices, and defer implementation richness only when safe and technically appropriate.

## 23. Pending work and next checkpoint

The independent conceptual discovery phase, major interaction review and nine-area counterintelligence exploration are complete **for now**. This is an exploratory design checkpoint with owner-confirmed directions; not scientific validation, implementation verification, or a declaration that the ontology is complete. The earlier interdisciplinary review is linked in this document; the current synthesis should not be represented as a source-by-source academically validated result without checking its sources.

Next:
- [ ] Conduct fictional adversarial conceptual cases (design/desk tests, not builds at this point). Combine ordinary and sophisticated adversaries, truthful yet misleading information, multiple actors, circular/common-source evidence, forged/partially compromised evidence, hidden/conflicting loyalties, coercion and abuse of oversight, investigator dissent, constrained access, misattribution, self-sealing hypotheses, and risky or obsolete recommendations.
- [ ] Probe for representational failures, undesirable conflation, missing relationships or invalid implications. Check alternatives, attribution, provenance, dependency reassessment, human authority and security constraints. Refine the smallest necessary abstractions; retain useful uncertainties and avoid duplicating engines.
- [ ] Exercise combined cases of discovery and correction, contamination and recovery, action/reaction feedback, and legitimate restrictions versus abusive concealment. Test whether useful protection advice is possible without treating an unconfirmed suspicion as fact.
- [ ] Resolve remaining representation questions (including QuestionFormulation, content locators, relation identity, relevance grades and scale semantics, assessment/lifecycle projections, names that overlap e.g. Exposure, and candidate compound conceptual roles) as scenarios and existing code warrant.
- [ ] Define semantic invariants and coherent minimum slices for MVP, preserving required information/invariants now where later migration would otherwise be costly. Separate conceptual breadth from necessary immediate operational support.
- [ ] Reconcile against actual specifications, source code, schema, tests and workflows. Identify what already exists, needs refinement, is absent, is already specified, conflicts or should be deferred. Do not assume missing implementation based on the clean-room model.
- [ ] Produce an architectural delta and incrementally implement/refactor at the most technically opportune point, including early structural work where beneficial, under AGENTS.md and the existing MVP contracts. Record reasons/dependencies/precautions for deferrals. Verify implemented capabilities before representing them as available.

The checklist in section 13 is retained as historical context: its subsequent Claim, Interpretation and interaction discussions have advanced conceptually in sections 14–22; exact implementation decisions and reconciliation remain open. The future implementer is expected to correct, extend and improve the design where evidence warrants rather than follow these examples mechanically.

## 24. Adversarial conceptual validation — first round (25 cases)

**Status:** The following fictional desk tests and proposed refinements were discussed and accepted by the product owner. The user's repeated "E?" explicitly means "I agree; what next?". These cases probe the candidate architecture in sections 13–22; they are neither executed software tests nor proof of code coverage or logical completeness. A refinement may already exist in the real implementation or its specifications. Continue to distinguish conceptual validation from implementation verification.

### AT-01 — False independence of evidence
**Scenario:** In a corporate fraud inquiry, Antônio alleges that a CFO authorized an irregular payment. Bernardo repeats Antônio, Carlos repeats Bernardo without knowing the origin, Daniela directly observed the authorization but learned the interpretation of fraud from Antônio, and Eduardo has an authentic record of the payment. Alternatives include conscious fraud, legitimate payment, a misled director, misinterpretation and deliberate false accusation.
**Expected:** Preserve all five occurrences; do not count them as five independent confirmations of fraud. Daniela is observationally independent with respect to authorization but not necessarily interpretatively independent regarding fraud. Eduardo's record establishes a different aspect from illicit intent.
**Refinement:** Independence is an attributable, contextual assessment of specified informational aspects, observation, observation method, origin, propagation, interpretation, premises and inferential basis; never a global property of a person or source. Distinguish independent observation of the same real-world event from independent corroboration of an allegation.

### AT-02 — Partial contamination of knowledge
**Scenario:** Three findings rely on an adulterated document: payment occurred (also independently confirmed by a bank record), a director authorized it (partially corroborated by testimony), and the director received a clandestine commission (no other known support).
**Expected:** Reassess the affected scope without invalidating all findings indiscriminately. The first may remain sufficiently grounded; the second requires review; the third loses its known grounding, which does not make it false.
**Refinement:** A graph of unqualified dependencies is insufficient. Capture the role of each input in a particular argument/inference: jointly necessary, complementary, independently sufficient alternative or aspect-specific support. Preserve provenance and alternative arguments; propagate reassessment needs, not automatic falsity. Reuse Argument, Inference, Premise, Finding and Assessment.

### AT-03 — Hindsight contamination
**Scenario:** In January an investigator recommends retaining an employee in a trusted role. In March hidden ties dating from December are discovered. In April a reviewer challenges the January recommendation.
**Expected:** Separate whether the decision was reasonable given information then available, inadequate given actual circumstances, or negligent given information reasonably obtainable at the time. Bad outcomes do not prove poor original grounding.
**Refinement:** Preserve event time, actor-specific acquisition/awareness time, system record time, interpretation time and decision time; reconstruct current understanding of past reality separately from contemporaneous knowledge and reasonably obtainable information. Actual and ought-to-know are independently assessed, not inferred from later knowledge.

### AT-04 — Deception without false assertions
**Scenario:** A report truthfully says a director met a supplier before a tender, the supplier won, and the director authorized the contract. It omits that the director met all suppliers and selection followed independent criteria.
**Expected:** The three claims can be true while the composite communication suggests a misleading interpretation. Do not infer deceptive intent merely from omission.
**Refinement:** Distinguish explicitly communicated content, meaning suggested by composition, recipient-specific interpretation, relevant omitted context and possible intent. Assess omitted context against communication purpose and reasonable completeness criteria. Integrity of a communication is not the conjunction of the truth of its parts.

### AT-05 — Mistaken identity contamination
**Scenario:** Three records concern someone named Roberto: suspicious transfer, confidential meeting and kinship to a subject. Later it becomes evident there are two people with that name.
**Expected:** Restore original references, separate the mistaken consolidation, trace and reassess affected relationships, inferences, findings and recommendations, preserving historical error and innocent alternatives.
**Refinement:** Distinguish actor, source reference, identification hypothesis and equivalence assessment. Identity association is a contestable epistemic dependency. Consolidation must be reversible without losing original reference, evidence or genealogy; authentic source records do not establish the association.

### AT-06 — Compartmented knowledge and conflicting findings
**Scenario:** Antônio sees evidence of a police officer's cooperation with a criminal organization; Bernardo has protected information that the officer is authorized undercover. Antônio is not authorized even to know the operation exists.
**Expected:** Preserve each evaluation with its own grounds. Do not disclose protected information or existence through a hint, unexplained disqualification, graph, recommendation or other derived output. An appropriately authorized reviewer may compare them.
**Refinement:** Assessment scope includes accessible information, information actually considered, permissions, context and premises. Distinguish reasonableness under each informational universe from correctness given wider information. No automatic global visibility or omniscient reasoning.

### AT-07 — Investigation creates its own apparent evidence
**Scenario:** Suspecting an employee's disloyalty, an executive restricts admin rights, excludes the employee from meetings and increases audits. The employee becomes resentful and seeks other work; the executive interprets this as confirming original disloyalty.
**Expected:** Consider behavior that predated, was induced or intensified by the intervention, or has indeterminate origin. A subsequent reaction is not automatic evidence of an antecedent intention.
**Refinement:** Trace both acquisition provenance (how information was obtained) and relevant causal provenance of the observed event (whether inquiry/intervention contributed to it). Model feedback between investigative acts, perception, behavior, evidence and revision; do not assert causation merely from sequence.

### AT-08 — Investigative selection bias
**Scenario:** A police oversight inquiry identifies 30 irregularities in unit A and three in unit B, but A received hundreds of checks and B superficial investigation.
**Expected:** Raw counts do not establish relative underlying prevalence or integrity; consider coverage, detection opportunities and differences in methods and conditions.
**Refinement:** InvestigationIntegrity must support attributable coverage assessments: examined and unexamined subjects/aspects, depth, selection criteria, observation opportunities and limitations. Coverage is itself contestable; no universal numeric score is mandatory.

### AT-09 — Apparent contradictions
**Scenario:** One witness says a director authorized an operation; another says the director never authorized payment; a third says the director did not know the financial terms.
**Expected:** These can coexist. Clarify scope and meaning before treating them as contradiction or dishonesty.
**Refinement:** Qualify equivalence and contradiction by referent, proposition aspect, scope, temporal validity, conditions, modality, precision and interpretation. Preserve distinction among actual incompatibility, partial/contextual incompatibility, apparent contradiction and undetermined equivalence. Assessments of contradiction are contestable, and an apparent conflict may indicate an InformationGap.

### AT-10 — Authority versus epistemic resolution
**Scenario:** Two investigators disagree on an evidentiary hypothesis. An authorized lead closes the case.
**Expected:** Show closure and official adoption, where applicable, without erasing contrary assessments or claiming epistemic resolution or consensus. Same rules apply when powers reside in one investigator.
**Refinement:** Precisely identify the target and scope of each institutional decision (close activity, adopt finding, reject recommendation, conclude sufficient for a specific purpose). Operational status, epistemic resolution, institutional adoption and individual agreement are distinct.

### AT-11 — Circular epistemic grounding
**Scenario:** Antônio's credibility is grounded in Bernardo's evaluation; Bernardo's credibility is grounded in a finding based exclusively on Antônio's declarations.
**Expected:** Identify circular grounding with no independent anchor; do not create extra confidence merely by traversing a cycle. Cycles may have external support and are not necessarily invalid.
**Refinement:** Distinguish ordinary relational and causal cycles from epistemic support cycles, including those with independent external grounding. No prohibition of cyclic domain graphs; prevent recursive self-corroboration, repeated support inflation and nonterminating evaluation.

### AT-12 — Hypothetical scenarios contaminate actual-world claims
**Scenario:** A researcher assumes that Antônio coerces Bernardo for exploratory reasoning. Explanations derived in that scenario are later reused elsewhere as if coercion were an established event.
**Expected:** Retain conditional nature and original assumptions; assess applicability before reuse.
**Refinement:** Carry scenario, necessary premises, validity conditions and unverified dependencies with derived results. Validity within a hypothetical scenario does not establish its premises or applicability in another context; avoid epistemic promotion through reuse.

### AT-13 — Nonexclusive and nonexhaustive hypotheses
**Scenario:** A police officer favors a criminal organization; motives considered are money, coercion and personal loyalty. All may coexist.
**Expected:** Confirmation of one need not weaken another; unknown alternatives remain possible.
**Refinement:** Make mutual exclusion, compatibility, complementarity, conditional dependence, overlap and joint causation qualified relations. Distinguish complete partitions from open or overlapping hypothesis sets. Never automatically normalize confidence across arbitrary hypotheses to sum to 100%.

### AT-14 — Attributed knowledge about another's knowledge
**Scenario:** Antônio believes Bernardo knows compromising information, although Bernardo does not. Antônio changes behavior because he feels exposed. Carlos interprets that reaction as proof Bernardo knows.
**Expected:** Keep actual knowledge, Antônio's belief about Bernardo's knowledge, Carlos's inference and subsequent effects separate. A mistaken belief can cause real behavioral consequences.
**Refinement:** MetaEpistemicState retains holder, target/object (including a nested attributed epistemic relation), subject actor, context, grounding and effects. No automatic transfer of support across levels. Real perceived vulnerability or influence may arise without the underlying capability actually existing.

### AT-15 — Self-sealing hypothesis
**Scenario:** An alleged infiltrator is deemed confirmed by suspicious communications, absence of communications is deemed evidence of competence, and contrary evidence is deemed sophisticated deception.
**Expected:** Identify inability to discriminate and lack of conditions that would weaken the hypothesis; not automatically declare it false solely because it is hard to test.
**Refinement:** Contextual testability assessment: expected confirming, disconfirming and inconclusive observations, conditions of observation/detectability, competing hypotheses and auxiliary premises used to explain contrary results. Possibility of deception is not universal proof of deception.

### AT-16 — Niccolò contaminates its own investigation
**Scenario:** One LLM misinterprets a document, a second derives a hypothesis, a third produces a finding from it, and retrieval returns all three as apparent confirmations.
**Expected:** Preserve derivation from one error; no independent corroboration merely from output count, repetition, summary or successive inference. Independent models reviewing the same source may help assess the interpretation, not independently verify its real-world contents.
**Refinement:** Distinguish source provenance, computational processing provenance, observation/extraction, interpretation, inference and human validation. Model-generated material and human adoption remain attributable and inspectable. Evaluate Niccolò-assisted processing with the same integrity principles.

### AT-17 — Recommendation remains appropriate after a suspicion is rejected
**Scenario:** Recommend removing root privileges due to suspicion of collaboration with a competitor and independently identified excessive privilege concentration. The collaboration suspicion is subsequently ruled out.
**Expected:** Reevaluate whether structural safeguards remain appropriate; implementation of a measure is not institutional proof of the original suspicion.
**Refinement:** Apply AT-02 dependency semantics to ActionRecommendation: necessary, complementary and alternative grounds, goals, applicability and suspension conditions. Epistemic correctness and strategic appropriateness are independently evaluated.

### AT-18 — Improper attribution of collective knowledge
**Scenario:** Legal knows about an irregularity, Finance knows the economic consequences and the CEO knows neither. A review claims the administration was fully aware.
**Expected:** Do not automatically transfer distributed information to individual actors, or individual knowledge, loyalty, intent or responsibility to an organization or vice versa.
**Refinement:** Distinguish individual, shared, distributed, institutionally recorded, formally communicated knowledge/information and obligation to acquire it. Formal notice, duty to know and actual comprehension are distinct. A group may hold fragments collectively without any member integrating them.

### AT-19 — Interpretation of silence
**Scenario:** An executive sends an irregular proposal to five subordinates. One agrees, one dissents, one is silent, one has not read it, and one read it but fears retaliation. The executive claims four agreed.
**Expected:** Lack of objection is not automatically assent, complicity or disloyalty; silence can have competing interpretations including consent, fear, indifference, lack of comprehension or awareness and deliberate nonresponse.
**Refinement:** Separate observed absence of a response from actual nonresponse, contextual expectation, ability/opportunity to respond, attributed interpretation and potential motives. Preserve uncertainty and original communications; do not manufacture implicit agreement.

### AT-20 — Conflicting authorizations and obligations
**Scenario:** A subordinate gives confidential information to a formally authorized auditor despite a supervisor's prohibition, under an institutional policy requiring cooperation and fear of retaliation.
**Expected:** The same action can be evaluated as hierarchical disobedience, compliance with an institutional obligation, authorized cooperation and loyalty to institution versus apparent disloyalty to supervisor. No automatic collapse into a single moral, legal or institutional classification.
**Refinement:** Each normative assessment identifies the norm/obligation, issuing authority, scope and applicability, any claimed precedence or exception and assessor's interpretation. Norm conflicts and disputed precedence are themselves investigable. Do not conflate obedience, legality, legitimacy and loyalty.

### AT-21 — Epistemic revision under confidentiality
**Scenario:** A finding is relied upon for an imminent decision. Another protected investigation discovers the supporting source supplied altered information. The first investigator is not authorized to know this; even an unexpected suspension alert could disclose the secret.
**Expected:** Do not leak protected information or existence through revision behavior; if authorized, route the need for review to an appropriate authorized reviewer. If no permitted route exists, the system cannot guarantee cross-compartment revision without violating restrictions.
**Refinement:** Separate epistemic need for review from authorization to conduct it, know its grounds, or communicate its existence/necessity. Derived review signals, routing and notifications are protected as applicable. Respect deterministic access policy; no unilateral LLM exception.

### AT-22 — Computational limits misrepresented as epistemic conclusions
**Scenario:** User sets 90% of resources to A and 10% to B; many relevant items are found in A while B's budget expires before analysis is complete.
**Expected:** Do not infer A is more promising merely because it received more processing. Show limitations, coverage and incomplete analysis; optionally recommend resource redistribution, never autonomously redistribute in MVP.
**Refinement:** Distinguish absence, not yet found, accessible but unexamined, partially processed and resource-truncated analysis. Processing coverage and constraints qualify results and comparisons, reusing general investigative coverage concepts (AT-08).

### AT-23 — Hidden premises in questions
**Scenario:** A researcher asks why Antônio disclosed restricted information to a competitor although disclosure and participation are not established.
**Expected:** Flag presuppositions and suggest asking whether a disclosure happened, whether Antônio participated, and only conditionally why.
**Refinement:** Questions/formulations may carry explicit or implicit premises, established or contested/unverified assumptions, excluded alternatives and links to hypotheses/gaps. Identify and challenge these without erasing original wording or preventing authorized exploratory premise adoption. Assess the question's discriminatory usefulness.

### AT-24 — Improper propagation of epistemic support
**Scenario:** Evidence shows an employee had information access and personally knew a competitor; neither establishes an actual disclosure.
**Expected:** Distinguish opportunity or capability from performance of the alleged act; association of true statements does not automatically prove a conclusion.
**Refinement:** Qualify inferential roles (capability, opportunity, intent, preparation, execution, consequence); support is not generally transitive and necessary conditions are not sufficient conditions. Explicit inference rules, assumptions and competing explanations determine evidentiary force; a role-specific support relationship is not a proof of its target.

### AT-25 — Simultaneous compromise across layers
**Scenario:** In a leak inquiry three accusations share one origin, an authentic artifact was misinterpreted, an investigator omits information out of fear, another has a hidden loyalty, favorable evidence is compartmented, administration demands rapid closure, and genuine access-control vulnerability exists independently of authorship.
**Expected:** Simultaneously question independence, preserve competing interpretations and motives, respect information restrictions, distinguish closure from resolution, retain dissent and recommend proportionate structural protection without declaring the accused guilty. Represent loyalty and motive contestably, not as proof of specific misconduct. Review and route any protected dependency concerns only under applicable authority.
**Result:** No demonstrated need for a new fundamental abstraction in this combined case. The established distinctions appear composable; this is a conceptual assessment, not a verification of completeness or implemented behavior.

### Checkpoint outcome and next step

The first round of 25 conceptual adversarial cases is complete and owner-approved as a direction of refinement. They identified cross-cutting semantic invariants, several relationships needing more precise semantics, and candidate overlap rather than establishing 25 new domain entities. The next step is to consolidate refinements, remove unnecessary duplication, and check tensions/conflicts across tests and sections 13–22. Only after that should the review define coherent minimum slices and reconcile with the actual implementation, schema, specifications and executable tests. Retain all AT identifiers and cases as traceable candidates for subsequent regression tests. Do not infer implementation coverage or claim exhaustive validation from this checkpoint.

## 25. Post-adversarial consolidation: refinement families and relationship semantics

**Status:** Owner-approved direction of conceptual architecture, including explicit product/MVP scope decisions in section 26. The 25 test cases remain individually traceable. This consolidation removes redundant mechanisms, not distinctions necessary for reasoning. It does not claim implementation coverage.

### 25.1 Consolidation of the 25 cases

| Cross-cutting family | Cases |
| --- | --- |
| Semantic identity, equivalence, qualified incompatibility and hypothesis relations | AT-05, AT-09, AT-13 |
| Grounding, provenance, independence and epistemic dependencies | AT-01, AT-02, AT-11, AT-16, AT-24 |
| Temporal and hypothetical context | AT-03, AT-12 |
| Communication, interpretation and presupposition | AT-04, AT-19, AT-23 |
| Individual, collective and attributed epistemic state | AT-14, AT-18 |
| Coverage, testability and investigation integrity | AT-07, AT-08, AT-15, AT-22 |
| Compartmentalization and permitted revision | AT-06, AT-21 |
| Institutional authority and conflicting norms | AT-10, AT-20 |
| Strategic appropriateness and compound cases | AT-17, AT-25 |

Reuse Assessment for distinct dimensions and relationships (including independence, credibility, relevance, testability, integrity and equivalence); Argument and Inference for the structure of grounding; InvestigationIntegrity for coverage, critical dependencies, bias and processing limitations; ActionRecommendation for separately grounded advice and its revision; MetaEpistemicState for higher-order attribution. Do not build separate epistemic engines per dimension.

Resolve apparent tensions explicitly:
- Preservation versus confidentiality: retaining provenance and historical dissent does not imply access to them. Protect content and existence, including derived outputs, metadata and review signals. Route cross-compartment review only through currently authorized mechanisms; absence of a permitted route limits the system's ability to intervene.
- Cyclic graphs versus circular grounding: allow real-world and relational cycles; prevent unanchored recursive corroboration and counting repetition as additional evidence. Traverse with bounded algorithms appropriate to the relation, preserving external grounding.
- Uncertainty versus action: proportionate protection can be warranted independently of guilt or confirmed hostile intent; implementing a measure is not evidence of the original suspicion. A conclusion may survive loss of one argument if separately supported.
- Proactivity versus autonomy: offer questions, warnings, recommendations and alternative allocations; do not autonomously reallocate investigative resources in MVP or execute real-world interventions.
- Expressivity versus complexity: preserve distinctions; do not demand that each become a separate entity, service or heavy edge. The operational representation must nevertheless support meaningful use of the model.

No fundamental incompatibility was identified by this round; this is not an exhaustive proof.

### 25.2 Semantic families of relationships

A generic graph edge is insufficient as a semantic contract. Two objects can participate simultaneously in multiple independently assessable relations. Relation families are extensible; terminology below is illustrative and must be reconciled with existing code.

- **Provenance:** acquisition, extraction, derivation, transformation, transmission, human/computational processing. Distinguish direct derivation from ancestry. Original content, observations, interpretations and processing outputs have their own identity/provenance. Derivation does not establish epistemic support. Causal circumstances that produce the observed event are distinct from acquisition of its record.
- **Epistemic support:** supports, weakens, challenges a premise, contests inferential applicability, challenges source reliability, offers an alternative. A challenge to grounding need not contradict the conclusion; distinguish rebuttal from weakening an inference. Support is not generally symmetric or transitive; a necessary condition is not generally sufficient.
- **Dependence:** qualify input roles within an Argument/Inference or recommendation: jointly necessary, alternative independently sufficient grounds, complementary, conditional, aspect-specific. Necessity for a particular argument does not establish necessity for the conclusion itself. A compromised input motivates scoped reassessment; never automatic global falsity or deletion. Compositions may represent n-ary grounding without requiring a specialized hypergraph engine.
- **Semantics:** exact equivalence, contextual equivalence, more-specific-than, implication, partial overlap, incompatibility or apparent contradiction. Exact equivalence under the same scope is symmetric and transitive; uncertain assessments of equivalence are not established equivalence and may not silently authorize a merge or transitive closure. Logical contradiction is symmetric under matching interpretation and scope, not generally transitive. Hypotheses may overlap, coexist, exclude one another or form incomplete sets; no forced normalization. Original references and genealogy must survive consolidation/separation.
- **Causality:** causes, contributes, enables, prevents, intensifies, attenuates, jointly contributing factors and conditions. No blanket transitivity, inference from mere sequence, or inference from causation to intent or responsibility.
- **Contextualization:** relevant_to, applicable_under, scope, time, scenario, adopted premise, actor perspective and purpose. Graded relevance, credibility and confidence are independent; relevance is not generally transitive.
- **Actor epistemic relations:** belief, knowledge, suspicion, trust, interest, aversion, perception, attention, etc. Preserve holder, object, scope/time, degree, assessor and grounding. Higher-order attributions are separately identified; no transfer of truth or support across nested levels or between individuals and organizations. Trust is not generally symmetric or transitive.
- **Institutional:** authority, authorization, delegation, obligation, responsibility, adoption, influence. Scope, source, time, conflict, interpretation and exceptions matter. These imply neither truth nor trust, knowledge, loyalty or agreement. Represented real-world relationships cannot grant software permissions.
- **Genealogy:** refines, supersedes, retracts, consolidates, separates, reinterprets. Revision does not erase previous assessments, decisions, original representations or historical consequences.
- **Composition:** part_of, composed_of, scoped subsets and alternative groupings. Preserve component identity, relationships and provenance; do not infer truth of a composite from truth of fragments or shared intention from membership.

A relation and an assertion/assessment about that relation are different. For example, A trusts B, C believes that A trusts B, and D challenges C's attribution. Relevant relationships can have identity, author, evidence, context, interval, competing assessments, genealogy and links to other relationships. Lightweight representation is acceptable where those properties are irrelevant; a rich relationship must remain investigable. Logical properties apply to the defined relation under its conditions, not automatically to fallible assessments assigning it. Do not assume closure across uncertain, differently scoped or unauthorized edges.

### 25.3 Core semantic contracts

Identity (reversible, grounded consolidation); provenance (traceable origin and derivation); attribution (distinguish originator/assessor/actor); context (preserve conditions); temporal separation (current reconstruction vs historical actor knowledge); grounding (structure and alternatives, scope); revision (preserve historical state); uncertainty (no automatic promotion); authority (adoption isn't truth); confidentiality (retention doesn't confer access). These are transverse invariants, not separate services.

### 25.4 Coherent functional slices, not a linear pipeline

- A — Investigable universe: actors, identity, events, evidence/artifacts, claims, provenance and qualified relations.
- B — Directed investigation: InformationGap, Question, Response, Hypothesis, relevance, discriminatory power.
- C — Reasoning and review: Premise, Interpretation, Argument, Inference, Assessment, Finding, explicit dependency semantics.
- D — Human dynamics and strategy: situated epistemic relationships and dispositions, intentions, motives, trust, loyalty, power, influence, dependency, coercion and pervasive Deception.
- E — Proactive advice: Scenario, Vulnerability, ActionRecommendation, Plan, expected outcomes, risks and alternatives.
- F — Integrity and governance: real permissions, decisions, dissent, history, coverage, compartmentalization and InvestigationIntegrity.

A supports B and D; both inform C; C informs E; F constrains and enriches them all. This is a dependency-oriented decomposition, not a mandatory order of implementation or processing. Individual abstractions may be reused across slices. Counterintelligence, oversight and other verticals emerge from their composition.

## 26. Explicit owner decision: complete epistemological model in the MVP

**This section supersedes any earlier suggestion in this review of simply deferring core epistemic distinctions or capabilities until after MVP.** The owner wants the entire approved epistemological model in the MVP, including all the conceptual families discussed in sections 13–25, because exceptional epistemological capability is essential to Niccolò's product identity. The implementation may be incremental and technically pragmatic, but must not make the model representationally rich while its actual behavior is rudimentary. No core epistemic distinction or capability should be dropped merely for convenience or deferred on the assumption it is a premium feature.

- Use algorithms as sophisticated as necessary for meaningful epistemic behavior. In particular, ancestry and independence, selective review across alternative/partial grounding, pervasive and multi-layer deception, investigative gaps and presuppositions, discriminatory questions and qualified strategic/loyalty/vulnerability reasoning are not to be made rudimentary.
- Simplify implementation mechanisms only when semantic correctness and useful behavior remain intact. Avoid unnecessary mathematical sophistication (complex probability calibration, autonomous resource optimization, mathematical strategic utility models) unless demonstrable benefit justifies it. Respect user-set investigation-line budgets, human circumstance-domain control, real deterministic access policy and the recommendation boundary, including the ban on kinetic attack recommendations/planning.
- Initial qualitative scales can be implemented with domain-specific enums: e.g. expertise (novice/basic/intermediate/advanced/expert), relevance (negligible/low/moderate/high/critical), confidence (very_low/low/moderate/high/very_high), interest and dependence with their own meaningful values. Do not conflate expertise in a subject with awareness of a particular event; actor state, degree and assessment confidence are independent. Distinguish unknown, none and not_applicable, including cases where a sentinel is represented outside the ordinal set.
- Order enum values only where semantics justify it. Ordinality does not imply equal intervals, ratio properties, additive scores, normalized probabilities or globally comparable values. Preserve context, author, rationale, evidence and time so later metric calibration/quantification can be added without rewriting semantic history.
- Bounded processing, simple UI projections, and simpler operational compartment structures are acceptable if they preserve semantic distinctions and don't mislead users or leak protected content. If existing MVP security contracts impose stronger requirements, preserve them. Full representability is not authorization to execute every modeled act. Existing exclusions of autonomous image/video understanding remain.
- Compare the complete model with real code, schema, contracts and executable tests before naming implementation gaps. Identify what can be preserved, refined, added, shared or simplified while maintaining full epistemic capability. Implement/refactor at technically opportune points; do not impose a giant rewrite or wait for an almost finished MVP. For any proposed delay or narrowing of an approved core capability, explicitly surface rationale, affected invariants, interim behavior, dependencies and migration precautions for owner review. Do not silently defer core model coverage.
- Product claims should differentiate approved scope/target behavior from verified implemented behavior. The 25 desk tests and consolidation are conceptual design material, not assurance of current implementation or empirical accuracy.

Next: reconcile the approved model and semantic contracts with the actual Niccolò architecture, identify the concrete delta, review planned implementation sequencing and derive executable regression/contract tests from AT-01–AT-25. The architecture review is evolvable and non-exhaustive; improve it where evidence warrants it. 
