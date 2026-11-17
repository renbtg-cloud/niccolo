# Domain model checkpoint — 2026-11-17

**Status:** exploratory domain-model checkpoint. This captures the current design
position before continuing into explicit hypothesis support/attack relations and,
after that, Question / Unknown / InformationGap.

It is intentionally stronger than loose brainstorming but is **not yet a frozen
implementation contract**. Future implementation work may complete, correct,
split or deepen these abstractions when the richer model makes that necessary.

Read this with:

- [Epistemic architecture revamp — work in progress](EPISTEMIC_ARCHITECTURE_REVAMP_IN_PROGRESS.md)
- [MVP roadmap](../product/MVP_ROADMAP.md)
- [Qualified temporal actor state](TEMPORAL_ACTOR_STATE_MVP.md)
- [MVP composite evidence and human-guided circumstances](MVP_COMPOSITE_EVIDENCE_AND_HUMAN_GUIDED_CONTEXT.md)

## 1. Architectural stance

The near-term rule is deliberately anti-generic:

> **Build a large, explicit, hard-coded, typed and rationalized domain model
> first. Generalize only after the domain has taught us what is actually
> general.**

For the MVP:

- semantic concepts should be represented explicitly rather than hidden behind
  generic plugin payloads;
- static crates/modules are welcome for ownership and dependency control;
- dynamic plugin systems, generic ontology DSLs, schema-driven reasoning and
  prematurely configurable abstractions are not goals;
- semantic coupling is allowed when it preserves reasoning quality;
- reuse should emerge later through deliberate refactoring, not through early
  abstraction pressure.

A concept may live in a specialized crate without becoming epistemically
isolated. For example, counterintelligence-oriented concepts may be statically
compiled above shared human-systems/core abstractions while still participating
directly in the same Situation graph and reasoning engine.

A possible dependency direction remains:

```text
niccolo-core-domain
        ^
niccolo-human-systems
        ^
niccolo-counterintelligence
```

This is code ownership, not a promise of independent reasoning engines.

## 2. Domain vocabulary versus UX vocabulary

Domain specialization and presentation terminology are different concerns.

A law-enforcement UI may render an actor as "Person of Interest", a legal UI may
render another role, and a corporate-investigation UI may use different labels.
That does **not** imply separate domain engines.

At the same time, a meaningful role such as an investigation target or source of
interest may be more than a cosmetic label. Such concepts should attach to the
underlying Actor through explicit contextual/temporal structures rather than by
turning Actor itself into a flat record or by relying only on UI text.

The product may therefore look strongly domain-specific while the reasoning
model remains shared.

## 3. A recurring structural pattern: identity anchor plus semantic network

Several important abstractions are converging on the same shape:

> **a relatively stable identity anchor surrounded by explicit, typed,
> contextual and often temporal relations.**

This is preferred over both extremes:

- giant structs containing every possible interpretation as fields;
- deep inheritance trees such as `SuspectActor extends Actor`,
  `RumorClaim extends Claim`, or `CrimeEvent extends Event`.

The network remains hard-coded and semantically typed. This is not a generic
property graph.

## 4. Addressables

Any domain element that can be referred to stably by another reasoning object is
conceptually **Addressable**.

This is primarily an identity/reference capability, not a generic object model.

Hypotheses, claims and higher-order relations may refer to Addressables such as:

- Actor;
- Claim;
- Proposition;
- Event;
- Relationship;
- Observation;
- SourceArtifact / Fragment where appropriate;
- EvidenceRelation;
- other Hypotheses;
- reified semantic relations where identity matters.

The implementation should prefer a closed typed representation (for example a
Rust enum of reference kinds) over an unbounded `Object` / `Any` mechanism
unless later evidence proves that insufficient.

## 5. Actor

Actor should not be a flat entity carrying one permanent role.

An Actor is an identity anchor around which contextual structures may describe:

- roles;
- status;
- relationships;
- authority;
- practical influence;
- epistemic position;
- operational position;
- suspicion / investigative interest;
- information access;
- temporal state.

These may be simultaneous, directional, contested, perspective-dependent and
temporary.

"Person of Interest", "source", "intermediary", "decision maker", "victim" and
similar notions therefore need not become subclasses of Actor. When semantically
real, they may be explicit contextual structures attached to Actor; when merely
presentation terminology, they stay in UX.

## 6. Proposition and Claim

A crucial distinction is:

> **Proposition is semantic content. Claim is an attributed act/record of
> asserting, reporting or otherwise putting forward semantic content.**

Multiple people may make different Claims about the same Proposition.

Claims are first-class Addressables and may themselves be the subjects of new
Claims or Hypotheses.

Examples:

- Joe claims `Foo(Jane)`;
- Jane claims Joe's claim is arguable;
- another person claims Joe lacked evidence for the claim;
- another claim disputes whether Joe ever made the original claim.

These do not collapse into a single truth-status for `Foo(Jane)`.

Claim-related structures may need to represent:

- attribution;
- epistemic position of the claimant (knows, believes, suspects, reports, etc.);
- provenance;
- temporal scope;
- derivation;
- source independence;
- support / refutation / qualification;
- claims about claims;
- relations concerning intention, justification, authenticity or evidential
  basis of a claim.

A normative/interpretive relation such as `mitigates` is allowed in the domain.
Its presence does not mean Niccolò has sovereignly judged it true. It may be
asserted by a user, participant, professional, model or later autonomous
reasoner, with explicit provenance and epistemic status.

## 7. Evidence is relational

"Evidence" should not be treated merely as an intrinsic file type.

Current decomposition:

### SourceArtifact

The thing that exists: email, message export, video, document, log, testimony
record, etc.

### Fragment

An addressable portion of a source with lineage back to its origin.

### Observation

A conservative observation extracted from or contributed about material.

Example:

> a video depicts a rifle pointed at Jane.

This is intentionally weaker than:

> Jane was coerced.

### EvidenceRelation

The contextual role that an Observation, Fragment, Claim or other eligible item
plays with respect to a Proposition, Claim, Hypothesis or other target.

The same observation may:

- support one hypothesis;
- weaken another;
- establish that an actor believed something without establishing the believed
  content;
- support an authenticity assessment;
- be irrelevant to another question.

Evidence may itself become the subject of investigation. Useful explicit
structures include authenticity, integrity, completeness, provenance,
independence and evidence-production/manipulation hypotheses.

A scene compatible with coercion may support a coercion hypothesis while a
separate staging hypothesis questions how that evidence was produced.

## 8. Event

Event is not a complete authoritative description of "what happened".

It is an identity anchor for a candidate occurrence around which we can attach:

- occurrence assertions;
- observations;
- participation;
- temporal placement;
- spatial placement;
- causal and enabling relations;
- interpretations;
- competing reconstructions;
- identity relations such as same-event / possible-same-event / distinct-event.

Different sources may describe what is perhaps the same Event differently.
Conversely, one account may collapse several distinct Events into one episode.

### Event versus Episode

An Episode groups events for human/reasoning convenience without destroying
their temporal and causal granularity.

### Participation

Participation deserves explicit identity/semantics rather than an unqualified
array of actor IDs. The same Actor may be described or hypothesized as employee,
victim, accomplice, coerced participant or another role under different
epistemic statuses.

### Causality

Causality should be represented as an explicit relation rather than an Event
field. Competing causal explanations must be able to coexist.

For example:

- threat -> handing over keys;
- prior agreement -> handing over keys;
- prior agreement + genuine later threat -> handing over keys.

## 9. Relationship

Relationship is not merely `Actor A --type--> Actor B`.

It is an identity anchor for a relatively persistent human/system connection
whose dimensions and states may change over time.

Relevant dimensions may include:

- kinship;
- organizational role;
- trust;
- conflict;
- loyalty;
- economic dependence;
- obligation;
- affection;
- fear;
- authority;
- practical influence;
- information flow;
- competing interests.

Many dimensions are directional. Formal authority and practical influence are
explicitly different and may point in opposite directions.

Useful surrounding concepts include:

- Relationship;
- RelationshipState;
- RelationshipDimension;
- RelationshipEvent.

Claims and hypotheses may be made about any of these rather than turning an
interpretation into a permanent field on the Actor pair.

## 10. Higher-level human/adversarial abstractions

Counterintelligence and similar investigative practice provide useful human
"prior art": many recurring patterns predate modern software and institutions.

Candidate hard-coded abstractions include:

- InformationAsymmetry;
- SourceIndependence;
- InformationPath / rumor propagation;
- CompetingLoyalty;
- Faction / Coalition;
- Influence;
- Compartment / NeedToKnow;
- CollectionGap;
- Indicator / Counterindicator;
- DeceptionHypothesis;
- FeignedIgnoranceHypothesis;
- Manipulation / StagingHypothesis;
- AdversarialHypothesis;
- StrategicSacrificeHypothesis;
- opportunity / capability / incentive distinctions.

These should not become a separate generic "CI plugin" merely for configurability.
Some may eventually prove transversal and be promoted downward into
human-systems or core-domain abstractions.

## 11. Hypothesis — current design position

Hypothesis is more cerebral than Actor, Claim, Event or Relationship. It belongs
to the reasoning/epistemic layer rather than directly to the world/case layer.

A Hypothesis describes a possible configuration or interpretation of the world.

### 11.1 Arbitrary finite set of Addressables

A Hypothesis may concern any finite combination of Addressables, including mixed
types.

Example participants in one hypothesis:

- Actor Jane;
- Actor Joe;
- Relationship R8;
- Event E17;
- Claim C42;
- Observation O11;
- Hypothesis H3.

The set of participating Addressables is not itself sufficient. The Hypothesis
must also express the candidate semantic structure linking them.

### 11.2 Candidate subgraph

A Hypothesis may therefore be best thought of as a candidate typed subgraph,
possibly including propositions such as:

- a Relationship existed before an Event;
- an Event causally contributed to another;
- a Claim concealed a Relationship;
- an EvidenceRelation is misleading because of a staging hypothesis.

This must remain a typed domain model rather than evolve casually into an
unbounded generic logic language.

### 11.3 Hypotheses about hypotheses

A Hypothesis is itself Addressable.

Therefore meta-hypotheses require no separate ontological class merely because
their subject is another hypothesis.

Examples:

- H2: H1 was induced by staged evidence;
- H3: Actor Joe promotes H1 because H1 benefits him;
- H4: H1 and H5 only appear independent because they derive from the same source.

### 11.4 Assumptions

Hypotheses should expose assumptions explicitly.

If an identity assumption or timestamp assumption fails, the system should know
which part of the hypothesis lost support rather than merely decrementing an
opaque confidence score.

### 11.5 Predictions and discriminators

A Hypothesis may generate predictions or expectations.

Those may later yield discriminating Questions and InvestigationActions.

This preserves the important separation between cheap creative hypothesis
generation and expensive investigation.

### 11.6 Separate axes

Do not collapse these into one `confidence` field:

- evidential support / plausibility;
- investigative priority;
- importance / consequence if true.

An unlikely but consequential hypothesis may deserve high investigative
priority. A strongly supported hypothesis may deserve little additional
investigative effort.

### 11.7 Temporal/contextual scope

A Hypothesis must be able to scope its proposition narrowly in time and context.

"Jane voluntarily cooperated during E17" and "Jane was coerced during E22" may
both be representable without turning either into an essence of Jane.

### 11.8 Counterfactuals

Counterfactual hypotheses are stronger than ordinary causal descriptions and
should remain distinguishable where represented.

"Threat E4 contributed to action E5" is not identical to:

"If E4 had not occurred, E5 would not have occurred."

### 11.9 Hypothesis space versus investigation agenda

Creative generation is comparatively cheap. Pursuing hypotheses is expensive.

The model should permit a broad HypothesisSpace containing speculative,
combinatorial or dormant candidates.

The human currently controls which candidates are promoted into active
investigation and which InvestigationActions deserve resources.

The expensive unit may be the investigative action rather than the hypothesis:
one action may discriminate several competing hypotheses at once.

Future Niccolò versions may gain greater autonomy in prioritization, but the
domain should not be impoverished to match today's trust level.

## 12. Hypothesis identity and genealogy — agreed invariant

This is the strongest current Hypothesis invariant:

> **Any semantically meaningful change to what a hypothesis asserts creates a
> new Hypothesis related explicitly to the previous one.**

Do not rewrite H1 until it silently becomes a different hypothesis.

Examples of relations between distinct hypotheses may include:

- specializes;
- generalizes;
- refines;
- weakens;
- branches_from;
- contradicts;
- compatible_with;
- depends_on;
- subsumes;
- preserves_part_of.

The exact vocabulary may evolve, but the identity rule should remain.

Small editorial wording changes or metadata changes need not create a new
Hypothesis. Changes in current support also do not create a new Hypothesis.

Therefore distinguish conceptually:

- **Hypothesis** — the candidate semantic commitment;
- **HypothesisAssessment** — how the current Situation state evaluates it;
- **HypothesisRelation** — its genealogy/semantic relationship to other
  hypotheses;
- optional editorial/history metadata where wording changes without semantic
  change.

A specialization is not a new version of the same semantic object. It is another
Hypothesis with lineage.

This preserves the intellectual history of an investigation: later we can ask
not only what is currently suspected, but how that hypothesis emerged.

## 13. Layers currently emerging

A useful conceptual separation is:

```text
WORLD / CASE MODEL
  Actor
  Proposition / Claim
  SourceArtifact / Fragment / Observation
  Event / Episode / Participation
  Relationship / RelationshipState
  typed semantic relations
        |
        v
EPISTEMIC / REASONING MODEL
  Hypothesis
  HypothesisAssessment
  HypothesisRelation
  Explanation / Interpretation
  Discriminator
  Question / Unknown / InformationGap
  Finding
  InvestigationAgenda / InvestigationAction
```

This is not a demand for two runtime engines. It is a semantic distinction
between representing the material/situation and representing reasoning about
that material.

## 14. Immediate next design work

Resume from here in this order:

1. **Hypothesis support / attack**
   - model support and attack as explicit typed relations rather than one score;
   - determine eligible source/target kinds;
   - preserve provenance, independence, scope and defeaters;
   - allow support/attack relations themselves to be questioned when needed.

2. **Question / Unknown / InformationGap**
   - determine whether these are distinct abstractions;
   - connect them to discriminators and hypothesis competition;
   - distinguish "not known", "not asked", "missing from available sources",
     "known inaccessible", and "worth investigating".

3. Continue through the remaining base/reasoning abstractions before freezing
   implementation representation.

## 15. Non-goals of this checkpoint

This document does not yet freeze:

- Rust structs/enums;
- database schema;
- exact crate boundaries;
- exact relation vocabulary;
- confidence mathematics;
- automatic hypothesis-generation policy;
- autonomous promotion/investigation policy;
- collaboration/selective-disclosure protocol.

Those follow the semantic model rather than driving it.

The working principle remains:

> **model the human reasoning problem richly first; optimize, generalize and
> configure only after we understand what must not be lost.**
