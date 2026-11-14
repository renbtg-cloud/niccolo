# Epistemic architecture revamp — work in progress

**Status:** Exploratory architecture work in progress. This document is **not** a normative product contract, not a declaration of implemented behavior, and not a replacement for the existing MVP architecture documents.

**Purpose:** Preserve the current epistemic-architecture review as a deliberate design track, make its method explicit, and prevent future implementation work from either (a) treating the current model as unquestionably complete or (b) assuming that a concept discovered during research is absent from Niccolò before checking the existing specifications and code.

Read this with:

- [MVP composite evidence and human-guided circumstances](MVP_COMPOSITE_EVIDENCE_AND_HUMAN_GUIDED_CONTEXT.md)
- [Qualified temporal actor state](TEMPORAL_ACTOR_STATE_MVP.md)
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
