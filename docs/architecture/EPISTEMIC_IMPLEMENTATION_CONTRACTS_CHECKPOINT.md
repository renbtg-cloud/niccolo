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

Migration 0117 exposes changes to the exact cited assessment basis of a recorded inquiry decision. A human may retain the decision, require reconsideration, or defer review, with an attributed note attached to both the original and current assessment events. Retention clears attention only for that exact current successor; any subsequent revision reopens attention. Unrelated judgments do not trigger a review, and neither review nor revision rewrites the decision, changes allocation or executes an inquiry. PostgreSQL validates the cited original and current successor, and the Workbench retains the review history.

Checkpoint 1.26 validation: PostgreSQL integration verified exact original/current identity, retry safety, refusal of unrelated and stale reviews, reopening after a second revision, and preservation of the original decision. The full suite passed (588 Rust, 132 React, 22 Python, 8 standalone JavaScript tests), including the embedded frontend build.

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

Migration 0118 implements an explicit actor-state judgment ledger over the existing actor registry and Situation-scoped expert objects. Fifteen independent dimensions represent exposure, access, perception, attention, examination, comprehension, recall, suspicion, inclination, doubt, belief, suspended judgment, claimed knowledge, perceived knowledge and evidence-supported knowledge. Each event retains context, temporal scope, judgment (`INDICATED`, `NOT_INDICATED`, `UNKNOWN`, `CONTESTED`), rationale, recorder, and optional exact source fragment; indicating evidence-supported knowledge requires such a fragment, but is still a human judgment, not a system-certified truth. Revisions preserve the dimension, actor, target, context, time and recorder. Independent or competing judgments remain visible, absence is not UNKNOWN, and no dimension implies another or changes an argument. The actor's registered kind does not gate or prove mental states.

Checkpoint 1.27 validation: real PostgreSQL integration verified independence of exposure and belief, explicit unknown versus absence, exact revision identity, evidence requirements, Situation isolation, direct SQL scope enforcement and bearer read/write separation. Full regression passed (590 Rust, 135 React, 22 Python, 8 standalone JavaScript tests).

## 13. Interest, curiosity, aversion and epistemic acts

Humans can want to know, not care, prefer not to know, seek only enough knowledge to act, avoid knowledge to preserve deniability, delegate knowing or avoid formal documentation.

Curiosity is not merely “high interest”; it can have an object, goal, context and inferred purpose.

Persistent dispositions must be distinguished from immediate reactions. Observed conduct can update hypotheses about dispositions without proving personality traits.

Migration 0120 extends the existing actor-state judgment ledger with independent orientation dimensions (`INTEREST`, `CURIOSITY`, `INDIFFERENCE`, `AVERSION`, `LIMITED_INQUIRY`, `DENIABILITY_PRESERVATION`, `DELEGATED_KNOWING`, `DOCUMENTATION_AVOIDANCE`). These require an explicit attributed goal and either `IMMEDIATE_REACTION` or `PERSISTENT_DISPOSITION`; ordinary epistemic states forbid the orientation fields. The scope and goal are part of the identity preserved by revisions. The investigator may mark a disposition unknown or contested, and no orientation infers a personality trait, executes an act or proves knowledge.

Checkpoint 1.29 validation: PostgreSQL verified scope/goal requirements, refusal to attach orientation metadata to ordinary states, exact revision identity, isolation from inquiry execution and direct SQL constraints. Full regression passed (592 Rust, 137 React, 22 Python, 8 standalone JavaScript tests).

Epistemic acts include investigating, ignoring, deepening, deferring, avoiding acquisition, delegating, reconsidering, challenging and preserving ambiguity.

Migration 0121 records independent attributed descriptions of those nine acts against an existing actor and Situation object. Each description distinguishes `REPORTED`, `OBSERVED`, `INFERRED`, or `CONTESTED`, includes explicit context, temporal scope, rationale and optional source fragment; an observed description requires a visible source fragment. A delegation can name an independently registered recipient, without executing any work or granting authority. Revisions preserve actor, target, act, context, time, delegate and recorder. The ledger never changes an orientation, proves a personality trait, executes an inquiry or resolves a gap.

Checkpoint 1.30 validation: PostgreSQL and HTTP integration verified attributed occurrence history, delegation scope, observed-evidence requirement, exact retry and revision identity, read-only bearer refusal, direct SQL constraints and independence from state/decision/response ledgers. Full regression passed (593 Rust, 139 React, 22 Python, 8 standalone JavaScript tests).

## 14. Higher-order epistemic state

Niccolò must represent beliefs or knowledge about another actor's epistemic state without hard-coded “first/second/third-order” classes.

Conceptually, an epistemic state can itself become the object of another epistemic state.

Migration 0119 generalizes the actor-state target from a Situation-scoped expert object to either that object or one exact, previously recorded actor-state event. The target is exclusive, workspace- and Situation-scoped, and immutable; the database requires the referenced state to preexist, preventing recursive cycles without a depth ceiling. The API and Workbench preserve historical targets even when subsequently revised; no automatic inference, transitive knowledge, or promotion of another actor's attributed judgment occurs. The display projects direct links rather than recursively expanding the entire ancestry.

Checkpoint 1.28 validation: a PostgreSQL chain of eight higher-order links preserved exact target identities after root revision; malformed, self-referential and cross-Situation targets were rejected, including direct SQL self-reference. Full regression passed (591 Rust, 136 React, 22 Python, 8 standalone JavaScript tests).

Computation is bounded by purpose, relevance and budget, not by an ontological depth ceiling.

This enables representation of asymmetries such as:

- A knows fact F;
- B knows that A knows F;
- A does not know that B knows this.

## 15. Trust, loyalty, ego and identity

Trust is directed, contextual and multidimensional. A person may trust another's technical competence while distrusting their confidentiality or truthfulness.

Trust is distinct from dependence, reliability, support and benevolence.

Loyalty is a qualified relation to a person, group, institution, cause or principle and is distinct from obedience, agreement, compliance, fear, dependency and aligned interests. Multiple loyalties can conflict.

Migration 0122 generalizes the actor-state target to an exact registered actor for directed trust, and to a registered actor or Situation object for loyalty. Separate trust dimensions represent competence, confidentiality and truthfulness. An explicit `DISTRUST_INDICATED` differs from absent trust, `NOT_INDICATED`, `UNKNOWN` and `CONTESTED`; it is valid only in a trust dimension. Multiple concurrent trust/loyalty judgments are preserved, with context, temporal scope, rationale, basis and attribution; neither reciprocal trust nor authorization, reliability, obedience or truth is inferred. The existing deterministic workspace and bearer access-control rules are unchanged.

Checkpoint 1.31 validation: migration 0122 applied on a fresh PostgreSQL 17.11 database. Directed trust/loyalty integration verified distinct dimensions, exact actor and object targets, explicit distrust, rejection of incompatible targets and revision identity. Regression passed in individually executed Rust suites (594 tests), React (140), Python (22) and standalone JavaScript (8). The parallel full-build command exceeded ephemeral disk space; the affected test binary was rebuilt separately, and the initial environment/concurrency failures passed on retry with the configured dedicated database and bounded test concurrency.

Motives involving ego must be representable explicitly:

- saving face;
- avoiding humiliation;
- avoiding appearing incompetent;
- protecting self-image;
- preserving status or importance;
- seeking recognition.

The product owner explicitly prefers **ego protection** for this personal self-image/status mechanism.

**Identity protection** remains a separate, broader possibility: protection of a social, professional, political, familial or group identity. The two must not be collapsed.

Migration 0123 introduces separate attributed ego-protection and identity-protection judgments about a visible Situation object. Each has its own rationale, temporal scope, judgment and optional evidence; neither indicates a diagnosis, an established motive, nor an automatic inference from behavior. Other actor states can target an exact prior motive judgment, but the motive description itself cannot use an actor or state event instead of its Situation object.

Checkpoint 1.32 validation: migration 0123, independent concurrent motive judgments, cross-dimension revision refusal and direct SQL target constraint verified. Full regression passed (595 Rust, 141 React, 22 Python, 8 standalone JavaScript tests), running test binaries in bounded batches to respect the transient VM's disk limit.

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

Migration 0125 adds independent attributed judgments of incentive, pressure, communicated threat, adverse condition, coercion, constraint, dependency and leverage concerning a registered actor and a visible Situation object. The same object may receive conflicting or independently qualified judgments, none of which proves the others, establishes consent or supplies a legal determination. This slice captures contextual conditions on the affected actor; it does not yet model a structured directed source actor or independently attribute threat capability, intent, credibility and outcome.

Checkpoint 1.34 validation: migration 0125, eight independent conditions, unknown coercion despite indicated threat, revision identity and direct SQL qualification verified. Full regression passed (597 Rust, 143 React, 22 Python, 8 standalone JavaScript tests).

Migration 0126 permits an optional exact registered counterparty for these contextual pressure judgments while retaining the affected actor and Situation object. It expresses an attributed possible contribution or counterparty relation, not established causation, intent or responsibility. Unknown counterparties remain valid; the system does not invent identities. A revision cannot change the counterparty, and incompatible dimensions, self-counterparties and out-of-workspace identities are rejected.

Checkpoint 1.35 validation: migration 0126, optional and exact counterparty, conflicting retry and revision, invisible identity, self-counterparty, incompatible dimension and direct SQL revision constraint verified. Full regression passed (598 Rust, 144 React, 22 Python, 8 standalone JavaScript tests).

Migration 0127 separately attributes threat capability, intent, perceived credibility and outcome. Capability and intent concern an explicitly identified alleged issuer; perceived credibility and outcome concern the affected actor and may leave the issuer unknown. All share the same visible Situation object addressing the alleged threat, while judgments remain independent, contextual and revisable only within their exact identity. Neither communication, capability nor perceived credibility proves intent, actual outcome, consent or causation.

Checkpoint 1.36 validation: migration 0127, independent communication/capability/intent/perceived credibility/outcome, required issuer, cross-dimension revision refusal and direct SQL qualification verified. Full regression passed (599 Rust, 145 React, 22 Python, 8 standalone JavaScript tests).

Compliance is not consent. Submission is not loyalty.

Autonomy is contextual: ability to decide, refuse, disagree, exit, seek review, acquire information and revise commitments may differ independently.

Migration 0124 represents those seven capabilities as independent attributed contextual autonomy judgments about a visible Situation object. Each may be indicated, not indicated, contested or unknown, with its own rationale, time and optional evidence. One capability does not imply another; reporting capacity does not establish consent, execute an act, confer authorization or make a system-level permission change.

Checkpoint 1.33 validation: migration 0124, all seven independent autonomy judgments, cross-dimension revision and incorrect-target refusal, direct SQL constraint and absence of inferred acts/decisions verified. Full regression passed (596 Rust, 142 React, 22 Python, 8 standalone JavaScript tests).

Vulnerability is contextual exposure to an adverse consequence, not a personal defect.

Migration 0128 adds independent contextual vulnerability, coercion susceptibility, compliance, consent and submission judgments about a registered actor and visible Situation object, with optional exact counterparty. They distinguish exposure or susceptibility to an adverse consequence from a response to pressure; no judgment derives a personal defect, consent from compliance, loyalty from submission, or deterministic authorization for Niccolò. These are attributed descriptions and do not determine legal validity.

Checkpoint 1.37 validation: migration 0128, independent vulnerability/susceptibility and compliance/consent/submission, cross-dimension revision refusal, direct SQL target qualification and absence of inferred loyalty or execution verified. Full regression passed (600 Rust, 146 React, 22 Python, 8 standalone JavaScript tests).

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

Migration 0129 records those five independent attributed influence facets with an explicit epistemic target: belief, attention, interest, curiosity, trust, acceptance, interpretation, investigation priority or willingness to examine. The affected actor and Situation object remain exact; capacity, act and intended effect also require an identified influencing actor, whereas actual effect and perceived success can retain an unknown source. A revision preserves the facet, target and counterparty. No recorded influence act changes the investigation, proves the actual effect or grants formal authority.

Checkpoint 1.38: Migration 0129 applied successfully; 601 Rust tests, 147 frontend tests, 22 Python tests and 8 standalone JavaScript tests passed.

Epistemic power is not formal authority. A person without formal rank may control who receives information, who understands it, which experts are consulted or which version reaches decision-makers.

Epistemic asymmetry can concern information, access, understanding, meta-knowledge or influence.

Migration 0130 preserves epistemic power and formal authority as independent contextual judgments. Each of the five asymmetries records an attributed comparative advantage of the subject relative to an exact, distinct registered counterparty concerning a visible Situation object. Neither a reverse comparison nor a global ranking is generated; formal authority is not an application permission. Revision retains the exact participants, object, scope and dimension.

Checkpoint 1.39: Migration 0130 applied successfully; 602 Rust tests, 148 frontend tests, 22 Python tests and 8 standalone JavaScript tests passed.

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

Migration 0131 distinguishes eight attributed narrative means, including a false statement and each of the seven composition methods. A judgment is contextual to a registered narrator and visible Situation object, with an optional exact audience member. No deceptive intention, attempt or effect is inferred; competing explanations remain open. A contested omission is not an accusation.

Checkpoint 1.40: Migration 0131 applied successfully; 603 Rust tests, 149 frontend tests, 22 Python tests and 8 standalone JavaScript tests passed.

Separate:

- deceptive intention;
- attempt;
- means;
- target;
- intended effect;
- actual effect;
- perceived success.

A deception attempt can fail against its intended target yet mislead an unintended observer.

Migration 0132 records the alleged narrator's deception intention, attempt, intended target, intended effect, actual effect and perceived success as six independent facets. Each requires a visible Situation object and contextual rationale. Intended target, intended effect and actual effect require an exact registered audience member, allowing conflicting or different outcomes for distinct actors. Intention, attempt and perceived success can retain an unidentified audience; means remain independent as in migration 0131. No event automatically establishes deception, falsity, intent or success.

Checkpoint 1.41: Migration 0132 verified in PostgreSQL; 604 Rust tests, 150 frontend tests, 22 Python tests and 8 standalone JavaScript tests passed.

Self-deception must remain an attributed hypothesis, not casual psychological diagnosis. Candidate mechanisms include rationalization, selective attention, motivated interpretation, ego protection, identity protection and avoidance of conflicting evidence.

Migration 0133 adds independent attributed contextual judgments for self-deception, rationalization, selective attention, motivated interpretation and avoidance of conflicting evidence. Ego and identity protection retain their existing independent dimensions. A possible mechanism does not establish self-deception, knowledge of a contradiction, or a general psychological disposition. No distinct counterparty is attached; each judgment requires an explicit visible Situation object, time, context and rationale.

Checkpoint 1.42: Migration 0133 applied successfully; 605 Rust tests, 151 frontend tests, 22 Python tests and 8 standalone JavaScript tests passed.

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

Migration 0134 generalizes the existing exact-target assessment ledger with ten independent investigation-integrity dimensions: investigative independence, evidence selection, provenance, question framing, hypothesis coverage, reasoning quality, criteria consistency, dissent preservation, historical traceability and reviewability. Discriminatory power remains its existing independent dimension. Each judgment is scoped to a visible object, relation, evidence fragment, grounding route or exact prior assessment, with criterion, context, confidence, rationale and optional grounds. No concern indicated is not proof of integrity; a concern or indicated compromise is not a general truth verdict, permission change, or automatic investigation veto. Explicit competing judgments and revisions retain their history.

Checkpoint 1.43: Migration 0134 applied successfully; 606 Rust tests, 152 frontend tests, 22 Python tests and 8 standalone JavaScript tests passed.

Migration 0135 allows the investigator to explicitly cite an exact integrity assessment event about a considered question in a recorded question decision. The comparison presents current integrity judgments separately from selection criteria; no assessment is preselected or converted into a veto or rank. Revising a cited judgment creates the existing selective decision-basis review obligation while leaving the historical decision and its original grounds intact.

Checkpoint 1.44: Migration 0135 applied successfully; 607 Rust tests, 153 frontend tests, 22 Python tests and 8 standalone JavaScript tests passed.

Detect self-sealing reasoning: a hypothesis for which supporting evidence, contrary evidence and absence of evidence are all reinterpreted as support is epistemically dangerous even if the hypothesis might happen to be true.

The structural reasoning projection now surfaces a narrow review candidate: an explicitly authored `CONTRADICTS` relation whose two endpoints occur as non-contextual grounds in supporting routes for the same conclusion. Exact relation and route identities remain inspectable; unrelated conclusions, opposition routes, contextual-only grounds and textually similar but unlinked objects do not trigger the candidate. This is not a detector of motive, a falsifiability proof, or an automatic rejection. Different conditional interpretations can legitimately support a conclusion under alternative circumstances. Missing evidence and absence claims are never inferred from silence; a complete self-sealing analysis still requires an explicitly modeled absence and investigation of competing explanations.

Checkpoint 1.45: 608 Rust tests, 154 frontend tests, 22 Python tests and 8 standalone JavaScript tests passed.

The projection also exposes an explicitly authored `INFORMATION_GAP` used as a non-contextual supporting ground, with the exact supporting route identities. An information gap is not verified evidence of absence or concealment; it may legitimately support a need for further inquiry. When both structural review candidates coexist, the workbench invites scrutiny of whether contrary outcomes remain meaningful, without inferring self-sealing reasoning, promoting a missingness assertion to evidence, or rejecting a conclusion.

Checkpoint 1.46: 609 Rust tests, 155 frontend tests, 22 Python tests and 8 standalone JavaScript tests passed.

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

Migration 0136 records these seven independent modeled-world withholding expectations as attributed actor judgments. The subject is the actor from whom information is intended to be withheld, not necessarily an actor who actually lacks it; a visible Situation object, context, time and rationale identify the target. This record neither grants nor revokes product access and does not establish actual exposure, access or knowledge. Workspace-scoped product permissions remain separate. Per-object secrecy and its complete non-leakage guarantees are not implemented by this modeled-world record.

Checkpoint 1.47: Migration 0136 applied successfully; 48 focused PostgreSQL workbench tests and 156 frontend tests passed.

If existence is protected, indirect signals through graphs, search results, counts, recommendations, notifications, metadata, summaries or LLM context must not leak it.

Distributed fragments may permit an authorized Niccolò analysis to formulate a hypothesis no individual actor can formulate. That does not justify saying “the organization knew”.

The actor ledger now offers a read-only comparison of current attributed state records for different actors sharing an exact Situation object, context and temporal scope. It retains each event's distinct dimension, judgment, recorder, rationale and evidence reference; historical superseded events remain in the ledger but not in this comparison. The view performs no cross-context merging, knowledge inference, evidence aggregation or creation of organizational judgments.

Checkpoint 1.48: 49 focused PostgreSQL workbench tests and 157 frontend tests passed.

## 21. Vulnerability, containment and recovery

Vulnerability is actor/resource/circumstance/consequence specific and may be actual, potential, perceived, unknown or misattributed.

Migration 0137 adds independent actual, potential, perceived and misattributed vulnerability dimensions, all attributed judgments rather than established facts; unknown remains an explicit judgment for any dimension. Each requires an exact resource or other Situation object, actor, circumstance, temporal scope and rationale (including intended consequence), with an optional distinct attributed counterparty. These qualifications do not imply exploitation, compromise, consent or a protective response.

Checkpoint 1.49 validation: full regression passed (612 Rust, 22 Python and 8 standalone JavaScript tests); the tested source snapshot matches the published 1.49 implementation.

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

Migration 0138 introduces separate contextual assessments of possible material compromise, the indicated need to pause or limit reliance, and recovery readiness. An assessment may target any exact visible workbench object, fragment, relation, argument route or previous assessment and cite explicit grounds. Neither a compromise nor a containment or recovery judgment changes evidence or reliance automatically. An investigator can explicitly cite such a judgment in a route dependency; a later revision surfaces the existing selective downstream review without rewriting routes or deleting provenance.

## 22. ActionRecommendation and the human action boundary

`ActionRecommendation` is a first-class, attributable, revisable object with purpose, recipient, context, rationale, grounding, conditions, alternatives, expected consequences, uncertainty, risk and reversibility.

The Rust `action_recommendation` module defines a bounded, non-executable draft contract for protective and investigative recommendations, including delay and inaction. It requires explicit grounds, risk, uncertainty and an alternative; its closed kind enum excludes kinetic attack plans. Free-text descriptions still require human review.

Migration 0139 persists attributed, immutable recommendation records in a workspace and Situation. Recipients must be registered actors in the workspace; grounding objects must belong to the exact Situation. Revisions preserve the recipient and original record, have a single successor, and never update a decision or execute anything. Rust and SQL independently validate the payload, scope and revision constraints; RLS and insert-only privileges protect the ledger. Checkpoint 1.52: all 140 migrations applied from scratch, 208 Rust library tests and all 52 expert-workbench integration tests passed, including direct SQL rejection of authority fields, foreign grounds and whitespace-only purpose.

Checkpoint 1.53 exposes explicit recommendation authoring through the authenticated Workbench API and includes the entire attributed revision history in its read-only, consistent snapshot. Only write-capable credentials can record a recommendation, the recorder comes from the authenticated principal, replay returns the same record, and read-only/foreign credentials cannot write. A recommendation does not record a question decision or send an instruction. Verified with 208 Rust library and 53 Workbench integration tests.

Checkpoint 1.54 adds explicit authoring and revision in the React Workbench: registered recipient, visible grounds, conditions, uncertainty, risk, reversibility and bounded alternatives. The original attributed recommendation and its expected (not actual) outcomes remain visible after revision. Recording is not execution or permission. The rebuilt bundle passed 163 frontend tests, 208 Rust library tests, 53 Workbench integration tests and a loopback HTTP smoke test serving the updated Workbench bundle.

Migration 0140 adds a distinct immutable action-decision ledger. A principal explicitly selects the primary option, selects an existing alternative by index, defers, or declines, always pinning the exact recommendation revision and supplying a rationale. The index is valid only for that immutable recommendation. Decision revisions preserve the originally recorded principal and recommendation identity; a decision about a revised recommendation requires a separate explicit record. No decision confers permission, creates an execution plan, changes a recommendation or asserts an actual outcome. Checkpoint 1.55: 141 migrations from scratch, 210 Rust unit tests and 54 Workbench integration tests passed.

Checkpoint 1.56 exposes action decisions through the authenticated Workbench API and consistent read-only snapshot. Read-only or foreign credentials cannot record decisions; authenticated principal identity provides attribution, and another principal cannot revise a decision. Revisions of recommendations never rebind prior decisions. Verified with 210 Rust library tests and 55 Workbench integrations, including the complete authenticated choice/revision/read flow.

The React Workbench now separates recommendation authoring from explicit decision authoring. The decision surface selects an exact recommendation revision and its primary or enumerated alternative, or records deferral or decline. It shows pinned historical recommendations and attributed decision revisions without silently adopting a successor, and distinguishes choosing inaction from deferring the decision. Checkpoint 1.57 ran 622 Rust tests across 47 groups, 167 React tests, 22 Python tests and 8 standalone JavaScript tests. Two Rust security/readiness failures identified incomplete RLS gates on the append-only action ledgers; migration 0141 provides explicit deny-update and deny-delete gates without granting mutation. Both affected suites (13 Rust tests) passed on rerun after applying the migration.

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

Checkpoint 1.63 extends the existing recommendation contract with optional, explicitly authored qualified basis groups: jointly necessary, alternatively sufficient, complementary and contextual. Every listed ground must be covered when qualification is supplied; overlap across groups is allowed without claiming independent origin. Legacy recommendations retain their unqualified flat list. Migration 0144 stores this alongside the unchanged historical payload with SQL and Rust validation, preserving immutable decision pins. The AT-17 structural-risk example can now distinguish a motivating suspicion from an independently declared sufficient protective rationale. The 213 unit, 59 Workbench integration and 13 security/readiness tests passed.

Checkpoint 1.64 projects recommendation basis impacts from the existing scoped genealogy and explicit argument reassessment graph. For each triggering revision it identifies the changed grounds and affected group indices, and enumerates declared alternative sufficient groups not linked to that trigger. These are structural attention cues, not confirmation of independence, adequacy, appropriateness or a change to the recommendation or decision. Historical and unqualified bases remain visible. The 213 unit, 60 Workbench integration and 13 security/readiness tests passed.

Checkpoint 1.65 adds qualified basis authoring and selective impact presentation to the React recommendation panel, preserving the historical flat-list contract and displaying declared alternative groups not linked to each trigger without asserting sufficiency or appropriateness. All 145 migrations applied in a new PostgreSQL database; 287 Rust tests including the real HTTP bundle smoke and 175 React tests passed.

Checkpoint 1.66 extends attributed multidimensional assessments to exact recommendation and decision records. Recommendation appropriateness, contemporaneous decision reasonableness, adequacy under actual circumstances and reasonably obtainable information are independent judgments with criterion, context, rationale, uncertainty and optional explicit grounds. Later discoveries do not rewrite a historical decision or automatically establish prior knowledge. Existing assessment revision, scope and authority boundaries apply; migration 0145 additionally enforces exact target and revision identity. The 213 unit, 61 Workbench integration and 13 security/readiness tests passed.

Checkpoint 1.67 exposes the exact action records as assessment targets in the Workbench, restricts choices to compatible perspectives, and visibly identifies explicit successor records without retargeting judgments. The form distinguishes strategic suitability, contemporaneous knowledge, actual circumstances and reasonable information obtainability. All 146 migrations applied on a new PostgreSQL database; 631 Rust, 178 React, 22 Python and 8 standalone JavaScript tests passed (839 total).

Checkpoint 1.68 implements the AT-26 objection-attention contract using existing attributed assessment history: novelty, materiality to consequential conclusions, review cost and explicit attention disposition are distinct from relevance, discriminatory power and epistemic validity. Divergent judgments and deferred objections remain representable without deleting, closing or vetoing the challenged material or a human inquiry decision. Migration 0146 also enforces the existing immutable assessment revision identity contract at the storage boundary for all dimensions. The 213 unit, 62 Workbench integration and 13 security/readiness tests passed.

Checkpoint 1.69 exposes the AT-26 dimensions and a contextual attention view grouped by exact explicitly triaged target, retaining competing unrevised judgments and historic revisions without ranking or automatic closure. All 147 migrations applied on a new PostgreSQL database. Full regression passed: 632 Rust, 181 React, 22 Python and 8 standalone JavaScript tests (843 total).

Checkpoint 1.70 adds [the AT-01–AT-26 executable witness matrix](EPISTEMIC_ADVERSARIAL_TEST_TRACEABILITY.md), distinguishing focused tests, constituent safeguards and open contracts. The matrix is guarded by two tests that require each case to remain individually traceable and every listed witness to refer to a real Rust integration test. All 24 Python tests and the documentation link check passed. This is a scoped implementation audit, not a declaration that all adversarial scenarios have been fully reproduced.

Checkpoint 1.71 adds an AT-08/AT-22 regression witness: two analyses of the same frozen catalogue examine reverse 90/10 and 10/90 allocations across two sources. Exact examination counts remain analysis-specific; neither examination completeness nor epistemic quality or prevalence is inferred, and the successor does not inherit the predecessor's examined-fragment claims. All nine coverage integration tests and 24 Python tests passed. The audit retains both cases as constituent coverage pending a compound fixture with actual findings and operation exhaustion.

Checkpoint 1.72 adds six independent contextual response assessments for AT-19, distinguishing observed absence, actual nonresponse, expectation, opportunity, deliberate nonresponse and interpreted assent. Observed absence assertions require an exact evidence fragment; interpreted assent identifies its distinct interpreting actor. Neither establishes consent or actual nonresponse. Migration 0147 and a five-participant fixture passed alongside 289 Rust and 24 Python tests. Completeness of observation remains a human assessment, not a consequence of recording an observation.

Checkpoint 1.73 exposes these six independently attributed response assessments in the React Workbench with exact communication, interpreting actor, observation scope, evidence and revision identity checks. All 148 migrations applied to a fresh PostgreSQL database; 634 Rust tests across 47 groups, 182 React tests, 24 Python tests and eight standalone JavaScript tests passed, including the served bundle smoke test.

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

Checkpoint 1.58 implements an append-only Situation-scoped ledger of independently attributed authorization reports, existing-plan reports, execution occurrences and event observations. These are descriptions only; none authorizes or performs an action. The decision pin is optional and exact, event time is distinct from acquisition and recording time, and observed characterization requires visible source evidence. A separate causal assessment pins an execution occurrence and an event observation (which may have predated the intervention), preserves alternative explanations and uncertainty, and never arises automatically from temporal order. Revisions preserve attribution and exact pins. Migration 0142 and the Rust store passed 212 unit, 56 Workbench integration and 13 security/readiness tests.

Checkpoint 1.59 exposes these ledgers through authenticated, non-executing authoring endpoints and the repeatable-read Workbench snapshot. The causal projection identifies exact superseded occurrence/observation pins and flags current affected assessments for review without changing their recorded judgment, inventing invalidity or flagging unrelated observations. The 212 unit, 57 Workbench integration and 13 security/readiness tests passed.

Checkpoint 1.60 connects both independent authoring forms and their attributed revision histories to the React Workbench. The user explicitly selects the event stage, source and causal pins, and can compare historical records without automatic rebinding. The rebuilt bundle is served by the real HTTP backend; 172 frontend tests and 283 Rust tests passed after applying all 143 migrations to a fresh PostgreSQL database.

Checkpoint 1.61 adds attributed, append-only causal review events with authenticated authoring. Each review pins the current execution and event revisions after a basis change, retaining the original assessment; retention can acknowledge the exact changed basis, while deferral or requested reconsideration keeps review pending. Later changes reopen the review without deleting earlier review history. The database independently rejects stale or unrelated pins. Migration 0143 passed; 212 unit, 58 Workbench integration and 13 security/readiness tests passed.

Checkpoint 1.62 adds the explicit React review surface. It shows original and current pins and attributed review history, and never silently retargets a review after another revision. All 144 migrations applied cleanly to a freshly created PostgreSQL database. The full regression passed: 627 Rust tests in 47 groups (including the real HTTP bundle smoke), 173 React tests, 22 Python tests and 8 standalone JavaScript tests.

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

The AT-01 through AT-26 adversarial cases remain individual regression obligations. See the [executable witness matrix](EPISTEMIC_ADVERSARIAL_TEST_TRACEABILITY.md) for implemented focused and constituent tests, open contracts, and remaining composed-scenario challenges. AT-26 specifically tests weaponized formalism: formally valid but low-value objections must remain representable without acquiring automatic veto power over investigation progress.

---

**Checkpoint decision:** Conceptual design is sufficiently mature to stop adding ontology merely for completeness. The next legitimate step is reconciliation with the real implementation, not further speculative abstraction. New concepts should be added only when code reconciliation, adversarial testing or evidence demonstrates that an approved distinction cannot be expressed coherently with the current model.
