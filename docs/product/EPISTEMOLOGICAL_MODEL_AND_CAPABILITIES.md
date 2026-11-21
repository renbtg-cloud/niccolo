# Niccolò: An Epistemology for the Real World

*Investigating information, human behavior, power and deception - including the integrity of the investigation itself.*

**A product and conceptual architecture overview.** This document describes Niccolò's approved epistemological design and MVP target. It is deliberately didactic: its purpose is to explain what kind of product Niccolò is trying to become, why the distinctions matter, and what capabilities follow from them. It is not a claim that every capability described here is already implemented in the shipping software. The detailed architecture review, implementation contracts, adversarial cases and repository evidence remain separately traceable.

Niccolò is designed for situations in which people can be honest, dishonest, mistaken, coerced, loyal, resentful, afraid, curious, strategically silent, selectively attentive, competent in one domain and ignorant in another - sometimes all within the same investigation.

It does not begin from the assumption that a clean dataset exists somewhere waiting to be summarized.

It begins from a harsher premise:

> Reality reaches the investigator through people, records, institutions, artifacts, interpretations and processes that can all be incomplete, dependent, contested or manipulated.

And then it asks a harder question:

> Can software help humans reason rigorously in that environment without pretending to become the sovereign judge of truth?

That is the product.

---

## Executive perspective

**What if an impeccably documented investigation was designed never to discover the truth?** What if an apparent leak was actually independent discovery? What if a witness was honest, every quotation accurate, and the resulting narrative nevertheless misleading? What if the investigator's own questions created the behavior later treated as evidence? What if the most useful next step were not another interview, but a boring log query that could separate three competing explanations at once?

Niccolò is designed for precisely these circumstances.

Its purpose is to give investigators, intelligence analysts, executives, legal teams, auditors and independent reviewers an explicit, revisable model of reality as they encounter it through information.

It is intended to help with problems such as:

- **Counterintelligence:** compromised information, hidden loyalties, coercion, misleading appearances, infiltration hypotheses, circular dissemination, strategic omission, and the possible compromise of counterintelligence itself.
- **Independent oversight and internal affairs:** investigators investigating investigators, selective inquiry, evidence handling, closure under pressure, competing institutional duties, hidden influence, and the integrity of the oversight process itself.
- **Corporate and legal investigations:** actual evidence versus repeated allegations, semantic claims versus communicative occurrences, provenance, circular corroboration, alternative explanations, contested testimony, evolving case theories and selective revision when foundations change.
- **Strategic judgment:** power, leverage, dependence, loyalty, trust, epistemic asymmetry, negotiation, due diligence, organizational politics, and recommendations that remain explicitly grounded in uncertainty.
- **Human-context reasoning:** who may know what; who believes what; who thinks others know what; who wants to know; who prefers not to know; who can influence attention; who may be protecting ego, identity, status, career, relationships or institutional interests.

The distinguishing proposition is that **deception, human knowledge, provenance, investigative direction and investigative integrity are not exceptional modules**. They permeate the same epistemological universe.

A finding is not a terminal answer. It can be challenged, revised, reused under explicit conditions, become a premise in another line of inquiry, become the subject of another investigation, or contribute to a recommendation without losing its original grounding.

Niccolò is therefore not primarily a document retrieval system with a persuasive language model on top. Retrieval and synthesis are useful infrastructure. The product ambition lies elsewhere: preserving the semantics of the relationships among information, reasoning, people and institutions so that the system can distinguish *why* something appears convincing, *what* it depends on, *who* knew or believed what, *which alternatives remain alive*, and *what should be reviewed when a foundation changes*.

Its target is a thinking companion that can disagree with the investigator without appropriating the investigator's authority.

```text
              A: Information and provenance
                       |           |
                       v           v
              B: Investigation <-> D: Human dynamics
                       |           |
                       v           v
                C: Reasoning and revision
                       |
                       v
                 E: Recommendations
                       |
                       v
              Human decision / new information
                       |
                       +----> A, B, C, D

    F: Integrity and governance constrain every layer.
    Deception and uncertainty can occur in every layer.
```

The diagram describes dependency and feedback, not a mandatory processing sequence. Human dynamics can suggest a new question. A new question can change behavior. That behavior can create new evidence. A recommendation can alter incentives. An intervention can invalidate an earlier scenario. An integrity review can force a previously closed investigation back into uncertainty.

The same core can therefore reason about a board decision, a police complaint, a suspected information leak, an executive negotiation, a disputed witness statement, a vulnerable employee, and the integrity of the people performing the analysis.

---

## 1. Reality does not submit a clean dataset

A company president says something. A subordinate remembers only part of it. A witness has a reason to omit a name. A police officer has access to evidence another officer cannot see. A respected expert is wrong. A junior analyst notices an inconsistency everyone else has learned not to question. A manager never asks how an impossible target is being achieved. An investigator closes a line because a superior wants it closed, not because the question was answered. A source tells the truth while carefully selecting which truths to reveal.

Ordinary information systems can preserve records of these events. A language model can summarize those records. Neither operation, by itself, constitutes an investigation.

Niccolò is designed around a more ambitious problem: constructing and continuously revising an explicit, inspectable representation of what can be known about a human situation, how that knowledge was formed, who had access to it, what remains uncertain, what alternative explanations remain viable, and how the informational environment may have been manipulated.

Its epistemological model is not a database of truths. It is a universe of investigable information: propositions, communications, observations, evidence artifacts, relationships, reasoning, uncertainty, competing explanations, human intentions, institutional decisions and the processes that generated them.

The origin and status of every consequential piece of knowledge matter.

**An authority can decide. It cannot decide reality.**

An untrusted person can tell the truth. An honest person can transmit a lie. A highly credible expert can be outside their domain. A genuine original artifact can contain false assertions. A fake artifact can nevertheless be evidence that someone attempted deception. A sophisticated deception may consist entirely of true sentences. A legitimate investigation can reach the wrong answer because it never asked the right question.

These are not edge cases. They are the environment in which Niccolò must work.

---

## 2. The informational universe

Niccolò's universe is informational. People, organizations, objects, physical events and relationships are represented through information about them. The system does not possess the physical object, witness the historical event or magically know the state of the world.

The central families include:

- **Actors and identities:** persons, organizations, informal groups, institutional roles, pseudonyms, aliases, software actors, disputed identities and mistaken identities.
- **Occurrences:** events, actions, meetings, communications, interviews, investigative acts, disclosures, decisions, authorizations, plans, executions and outcomes.
- **Evidence artifacts:** documents, messages, recordings, records, observations, descriptions of physical evidence, analytical results and transformations of those materials.
- **Semantic propositions and claims:** what is asserted or proposed to be true, independently from the particular occurrence in which it was expressed.
- **Communicative acts:** declarations, questions, answers, refusals, requests, warnings, denials and other occurrences with speakers, recipients, time and circumstances.
- **Interpretations and hypotheses:** possible meanings and possible explanations, which may coexist and compete.
- **Relationships:** qualified, investigable connections among objects, actors, propositions, occurrences and even other relationships.
- **Reasoning objects:** premises, premise adoptions, scenarios, arguments, inferences, challenges, refutations, assessments and findings.
- **Human epistemic and strategic states:** exposure, attention, belief, knowledge, curiosity, aversion, trust, loyalty, dependency, coercion, influence, leverage, autonomy and deception.
- **Governance and action objects:** authority, authorization, delegation, obligation, responsibility, recommendations, decisions, plans, execution and retrospective evaluation.

This is intentionally richer than a flat graph of “entities” and “facts”. The richness is not decoration. The distinctions prevent specific classes of reasoning error.

### 2.1 Identity must survive analysis runs

Epistemically significant objects need persistent identity independent of a particular analysis execution.

A new analysis must not automatically recreate a claim, question, information gap, hypothesis, finding or relationship as though it had never existed before.

Conceptually, Niccolò distinguishes at least:

- **Object identity:** the persistent epistemic object.
- **Revision identity:** a particular revision or representation of that object.
- **Occurrence identity:** a concrete historical occurrence - for example, someone actually saying something at a particular time.

A textual correction that preserves semantic meaning is not the same as a new proposition. A changed proposition is not merely a new spelling of the old one. A repeated declaration is a new occurrence even when it expresses the same proposition. A new credibility judgment is a new assessment, not a mutation of the proposition itself.

History is fundamentally append-oriented:

- revision does not erase prior state;
- retraction does not mean an assertion never occurred;
- rejection of a hypothesis does not erase its historical influence;
- consolidation does not destroy original identities, provenance or genealogy;
- disagreement between investigators does not require one globally privileged epistemic version.

Append-oriented semantics do **not** mean that personal or legally regulated content must be retained physically forever. A lawful erasure, controlled redaction, anonymization or pseudonymization operation may remove or transform protected content while preserving, where legally permitted and operationally necessary, a tombstone describing that an object existed, what kind of legal/governance action occurred, which dependencies were affected, and what must be reassessed. A tombstone must never retain the very content the erasure obligation requires the system to remove.

### 2.2 Proposition is not occurrence

Suppose three people state that João received a bribe.

Niccolò may represent:

- one semantic proposition;
- three communicative occurrences;
- three provenance chains;
- three potentially different credibility assessments;
- a possible common origin behind two of the accounts;
- a possible genuinely independent origin behind the third;
- different wording, timing, audiences and motives.

Semantic consolidation must never silently consolidate evidence independence or credibility.

A claim can be repeated many times and still have one informational origin. Conversely, two independent observations can express the same proposition without becoming one evidentiary event.

### 2.3 Physical evidence remains informationally represented

Consider a forensic sample. Niccolò may contain information concerning its collection, custody, storage, examination, disappearance, replacement, contamination and laboratory results.

It does not need a separate “physical-object epistemology”.

The same core can reason about:

- a blood sample;
- a substituted attachment;
- an altered spreadsheet;
- a corrupted database export;
- a forged memo;
- a photograph described by a human;
- a laboratory result;
- an original document whose contents are themselves false.

The important distinctions are among identity, origin, representation, authenticity, integrity, custody, transformation, provenance, accessibility, interpretation and evidentiary force.

A genuine original can be false. An authentic intact artifact can lose confidentiality. A transformed representation can be faithful. A forged document can be useful evidence of forgery.

### 2.4 Identity is itself investigable

Two reports may concern the same person. Two different people may share a name. A pseudonym may refer to several actors. One actor may use several identities. Two propositions may appear equivalent but differ in scope or modality.

Therefore:

> Similarity is not identity, and consolidation must be reversible.

Original references, representations, provenance and genealogy survive semantic merging, splitting or reinterpretation.

---

## 3. Time is part of the epistemology

Niccolò must distinguish several times that conventional systems often collapse:

- when an external event occurred;
- when it was recorded;
- when information became available;
- when a particular actor was exposed to it;
- when that actor may have perceived, examined or understood it;
- when Niccolò acquired it;
- when an investigator formed an interpretation;
- when an assessment was recorded;
- when a later revision changed the understanding of the earlier material.

This matters because hindsight is epistemically dangerous.

A conclusion reached today must not silently become information a witness supposedly knew yesterday. A report created after an incident cannot prove that a manager had its contents before the incident. An investigator who now knows a fact must still be able to reconstruct what was reasonably knowable at the earlier decision point.

Historical reconstruction and current reconstruction are separate views over the same evolving universe.

---

## 4. Investigation is a living epistemic process

Investigations are not merely searches for confirmation. They have objectives, lines of inquiry, relevant unknowns, questions, evolving hypotheses, coverage limitations, resource constraints, interventions and decisions.

New information can change the significance of material collected months earlier. An apparently resolved question can reopen when its foundation changes. A question can itself alter the behavior being investigated.

### 4.1 Unknown, InformationGap, Question and QuestionAct

Niccolò does not require a rigid pipeline such as:

```text
UNKNOWN -> InformationGap -> Question -> Response -> Answer -> Truth
```

That pipeline is too simple for human investigations.

- **UNKNOWN** is primarily an epistemic state of something already represented, not a mandatory entity.
- **InformationGap** is a first-class contextual absence of information relevant to an investigation, hypothesis, objective or decision. Its semantic identity concerns what is missing, not the wording used to ask about it.
- **Question** is a formulated investigative problem.
- **QuestionAct** is the concrete occurrence of someone actually asking a question.

A gap can exist without anyone formulating a question. One question can address several gaps. Several questions can refine the same gap. A question can exist for strategic reasons even when no prior gap was explicitly modeled.

### 4.2 Response is not truth

A **Response** is a communicative occurrence. It may include assertions, refusals, uncertainty, evasion, claimed ignorance, counterquestions or several propositions.

If someone says:

> “I do not know.”

Niccolò initially knows:

```text
Actor asserted: "I do not know."
```

It does **not** automatically know:

```text
Actor does not know.
```

Likewise, receiving an answer does not automatically resolve the underlying gap.

Responses and semantic claims are many-to-many. One response can express several claims; the same claim can be expressed by several responses. A relationship such as `extracted_from` records provenance of an interpretation or proposition. It does **not** establish evidentiary support or truth.

A gap may be:

- open;
- partially addressed;
- answered by a claim;
- supported;
- contested;
- resolved for a particular purpose;
- unresolved;
- practically unresolvable.

Operational closure and epistemic resolution are different.

### 4.3 Relevance is not truth

A fabricated confession can be highly relevant to an investigation of deception while being worthless as direct support for the alleged offense.

Relevance is contextual and graded. It requires:

- what the material is relevant to;
- the degree of relevance;
- the rationale;
- confidence in the assessment;
- context and time;
- the assessor.

Relevance is not generally transitive. A is relevant to B and B is relevant to C does not imply A is relevant to C.

### 4.4 The most useful information is discriminating information

A large quantity of relevant information may still fail to separate competing explanations.

Suppose the live hypotheses are:

```text
H1 = insider leak
H2 = compromised credentials
H3 = accidental publication
```

Question A:

> “Was Antônio dissatisfied with the company?”

may affect H1.

Question B:

> “Do the access records show legitimate authentication, compromised credentials or public access?”

may separate all three.

Niccolò therefore treats **discriminatory power** as a first-class assessment dimension.

Initial qualitative values may be sufficient:

```text
negligible / low / moderate / high / very_high
```

provided the rationale, alternatives and context remain visible.

This is not a claim that every investigative choice should be reduced to a numerical information-gain optimizer. The point is semantic: the system should ask what a proposed inquiry could actually distinguish.

### 4.5 Questions can contain hidden premises

Consider:

> “Why did João lie?”

The question may presuppose:

```text
P1: João made the statement.
P2: the statement was false.
P3: João knew it was false.
P4: João intended to mislead.
Q : what motivated him?
```

Perhaps only P1 is established.

Niccolò should preserve the original question while making those presuppositions explicit and challengeable.

The same applies to institutional framing. An investigation that begins with “Which employee leaked the document?” may have silently excluded the possibility that the document became public through configuration error.

### 4.6 A question is also an intervention

Questions do not merely extract information. They can:

- reveal what the investigator knows;
- reveal what the investigator suspects;
- plant a premise;
- prime memory;
- create fear;
- alter trust;
- warn a target;
- change coordination among actors;
- trigger evidence destruction;
- alter later testimony.

The investigation changes the world it investigates.

That fact is part of the epistemology, not merely operational trivia.

### 4.7 Questions can be strategic acts

A question such as:

> “You still speak with supplier X, correct?”

may genuinely seek information. It may also attempt to:

- observe reaction;
- signal surveillance;
- induce fear;
- test whether the subject warns someone else;
- plant a belief about the investigator's knowledge;
- influence future conduct.

The semantic content of a question and an attributed strategic purpose are therefore separate.

### 4.8 Canary information and differentiated disclosure

An investigator may deliberately provide different variants of information to different actors and later observe which variant propagates.

```text
Actor A receives alpha
Actor B receives beta
Actor C receives gamma

Later an external source reports beta
```

That does not prove B leaked it.

It changes the map of possible propagation and becomes one item in a larger causal and epistemic reconstruction.

The same observation can mean different things depending on the original purpose. Information returning to its originator may indicate successful dissemination, a confidentiality breach, independent discovery, copying, coordination or an artifact of a shared source.

A return path is therefore an investigative lead, never self-authenticating proof of the path that produced it.

### 4.9 Choosing the next investigative step

Niccolò should be able to recommend a next question or investigative act by considering:

- current hypotheses;
- information gaps;
- expected discriminatory power;
- cost;
- accessibility;
- authorization;
- contamination risk;
- urgency;
- coverage;
- remaining resource budget.

For example:

> Before interviewing Antônio again, authentication logs appear more useful. They can separate several live hypotheses without alerting an actor under investigation and may change which interview questions are worth asking later.

The user remains in control of priorities and resource allocation in the MVP. Niccolò may expose underexamined lines, explain tradeoffs and recommend reallocations. It must not silently redirect investigative budgets.

### 4.10 Investigation can legitimately stop before truth is reached

Further inquiry may cease because:

- remaining uncertainty is irrelevant to the decision;
- evidence is prohibitively expensive to obtain;
- remaining questions have poor discriminatory power;
- continued inquiry would create unacceptable contamination or danger;
- accessible sources are exhausted;
- an institutional deadline arrives;
- authorization expires.

Therefore:

```text
investigation_status = CLOSED
epistemic_status      = UNRESOLVED
```

can be perfectly legitimate.

Closure is an operational act. Resolution is an epistemic assessment.

---

## 5. Knowledge is constructed, challenged and revised

Niccolò distinguishes the material being reasoned about, the meaning assigned to it, the assumptions adopted, the reasoning performed and the formal result.

### 5.1 Interpretation

An **Interpretation** assigns contextual meaning to material or to another interpretation.

For human language, understanding is not exhausted by dictionary meaning. Niccolò may need to distinguish:

- local lexical and compositional meaning;
- reference and scope - who or what pronouns, time expressions and qualifiers concern;
- pragmatic meaning - accusation, praise, irony, exaggeration, insult, in-group language, warning, justification or another discourse act;
- the situational content the speaker or user intended to communicate.

The original utterance remains primary evidence of what was said. A derived interpretation remains attributable and revisable.

In the MVP, a human statement that Niccolò has interpreted but the user has not yet confirmed as correctly understood remains explicitly marked as **unconfirmed understanding** in both the reasoning state and the user interface. Niccolò may reason from that current interpretation when useful, but every downstream dependency must retain the fact that the interpretation is provisional so that later clarification can trigger selective reassessment.

For example:

> "He was an asshole."

safely supports an initial understanding such as "the speaker expresses a strongly negative evaluation of him". It does not, without context, establish that he humiliated subordinates, complained loudly, acted selfishly or behaved in any other specific way.

Likewise:

> "He's a fag."

can support the local referential understanding that the speaker used a strongly context-dependent label referring to him as a queer person, while leaving pragmatic meaning open. Depending on speaker, audience, relationship and context, the utterance may function as hostility, derogation, reclaimed in-group language, teasing, affection, solidarity or something else.

User confirmation means **"yes, that is what I meant / that is how the utterance should be understood"**. It does not mean that the proposition expressed by the user or speaker is true.

Competing interpretations can coexist. The original material survives. A later preferred interpretation does not rewrite the historical artifact. Revision of an interpretation must preserve its history and identify consequential downstream reasoning that may need review.

### 5.2 Hypothesis

A **Hypothesis** is a proposition or structured explanation to investigate.

Hypotheses may:

- coexist;
- overlap;
- be mutually incompatible;
- be partially compatible;
- refine one another;
- depend on one another;
- form an incomplete set.

They are not required to sum to 100 percent confidence or form a neat mutually exclusive partition.

### 5.3 Premise and PremiseAdoption

A **Premise** is a proposition used in reasoning.

A **PremiseAdoption** records that an actor or process adopts it for a purpose, under conditions, with rationale and possibly duration.

Adoption is not truth.

An investigator may say:

> “For this scenario, assume the invoice is genuine.”

without asserting that the invoice has been established as genuine in the actual world.

### 5.4 Scenario

A **Scenario** has identity, purpose, circumstances, assumptions, evidence, conditions, results, provenance and genealogy.

A scenario may be actual-world reconstruction, counterfactual, prospective or exploratory.

Its conclusions remain conditional on its assumptions when reused.

Hypothetical material must not silently contaminate actual-world evidence.

### 5.5 Inference

An **Inference** is an attributable reasoning act with:

- available inputs;
- method or rule;
- premises;
- context;
- applicability conditions;
- output;
- available rationale.

Niccolò does not pretend to capture hidden chain-of-thought from a language model. It records the reasoning artifacts that are intentionally available for review.

The validity of an inferential form, the truth of its premises, the strength of the conclusion and the applicability of the rule are separate questions.

### 5.6 Argument

An **Argument** groups grounding structures.

It may contain:

- claims;
- evidence;
- premises;
- inferences;
- subarguments;
- counterarguments;
- defeaters;
- dependencies.

A challenge can attack a premise, source reliability, interpretation, inferential applicability or the conclusion itself.

Undermining an argument is not automatically refuting the conclusion.

### 5.7 Assessment

**Assessment** is a transversal abstraction: an attributable, dimension-specific evaluation of some object or relationship.

An assessment can concern:

- credibility;
- relevance;
- expertise;
- independence;
- discriminatory power;
- loyalty;
- vulnerability;
- coercion susceptibility;
- investigation integrity;
- semantic equivalence;
- inferential applicability;
- confidence;
- practical feasibility;
- many other dimensions.

Each assessment has its own dimension semantics, criterion, result, confidence, rationale, scope, time, grounding and assessor.

There is no mandatory universal epistemic score.

Assessments are themselves assessable and revisable.

### 5.8 Finding

A **Finding** is a formal investigative result.

It may be:

- partial;
- conditional;
- contested;
- inconclusive;
- superseded;
- institutionally adopted;
- rejected by another investigator.

Institutional endorsement is not truth.

A finding preserves its dependencies, supporting and opposing grounds, conditions, uncertainty and genealogy.

---

## 6. Provenance is not optional

Suppose one interview is summarized in a memorandum. An analyst cites the memorandum in a report. An AI system summarizes the report. A second investigator cites all three as independent confirmation.

```text
 Original interview (A)       Independent observation (B)
          |                               |
          v                               |
     Memorandum                           |
          |                               |
          v                               |
       Report                             |
          |                               |
          v                               |
      AI summary                          |
          |                               |
          +-----------+-------------------+
                      v
          Assess the actual grounds.
          A's descendants are not B.
```

Three representations of one account can be three historical artifacts without being three independent origins.

Conversely, two independently obtained accounts of the same event must not disappear through semantic deduplication.

Provenance can include:

- acquisition;
- extraction;
- derivation;
- transformation;
- transmission;
- human processing;
- computational processing.

Direct derivation and distant ancestry are different.

Provenance does not establish epistemic support.

A document can be derived from another without supporting the proposition for which it is later cited. A source can preserve a false assertion perfectly. A computational transformation can preserve or distort content. A report can combine original observation with recycled allegations.

The system must distinguish **where information came from** from **why a conclusion should be believed**.

A previous Niccolò output is investigable material, not privileged truth. Cycles of repetition cannot manufacture external grounding.

---

## 7. Grounding, dependence and selective revision

A conclusion must not merely have “evidence attached”. Niccolò needs to represent *how* the evidence participates in the reasoning.

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

Consider:

```text
E1 --\
      >-- A1: jointly required --> H1
E2 --/

E3 ---- A2: independently sufficient --> H1

E4 ---- A3: weak support ------------> H1
```

If E1 is discredited, Argument A1 may fail. H1 does not necessarily fail because A2 may remain valid.

Grounding roles can include:

- jointly necessary grounds;
- alternative independently sufficient grounds;
- complementary grounds;
- conditional grounds;
- aspect-specific grounds.

A premise necessary for one argument is not necessarily necessary for the conclusion.

### 7.1 Support and opposition do not simply cancel

Suppose:

```text
E1 supports H
E2 challenges H
```

Niccolò should not reduce this to:

```text
E1 + E2 = zero
```

It should preserve:

```text
support_for(H)
support_against(H)
```

A hypothesis can remain contested with serious arguments on both sides.

### 7.2 Selective revision

When an epistemically significant input changes, Niccolò should identify the affected region rather than blindly rerun or invalidate everything.

```text
changed object
     |
     v
direct dependents
     |
     v
affected arguments / inferences
     |
     v
potentially affected findings
     |
     v
recommendations / scenarios / later investigations
```

But dependency does not imply automatic invalidation.

A change creates an obligation to reassess according to the semantics of the dependency.

### 7.3 Contamination does not imply universal falsity

Suppose Source X supplied Evidence A and B. B appears in Report C. Claim D later depends on C.

If Source X becomes suspected of deliberate manipulation, Niccolò should ask:

- Which descendants depend directly on X?
- Which merely share provenance ancestry?
- Which were independently corroborated?
- Which transformations changed the relevant content?
- Which arguments relied on X as necessary rather than supplementary input?
- Which downstream findings or recommendations need review?

The invariant is:

```text
contaminated ancestor != automatically false descendant
```

A compromised source may still have told the truth on one matter. A forged artifact may reveal a real attempt to deceive. A derivative may later gain independent corroboration.

### 7.4 Revision is itself revisable

Suppose Investigation Y concludes that Source X manipulated Evidence E, so E is downgraded. Later Y itself is shown to have relied on manipulated information.

The downgrade of E may need review.

```text
revision
   can itself be revised
```

Niccolò therefore preserves:

- what was believed;
- why it was believed;
- by whom;
- at what time;
- using which available information;
- what later changed;
- why the assessment changed.

This supports questions such as:

> “Why did we believe in March that Antônio was involved, but by June consider that hypothesis weak?”

That explanation should be reconstructable from stored dependencies and history, not invented after the fact.

### 7.5 Cycles in reality are allowed; self-corroboration is not

Real-world relationships can be cyclic. Information can circulate and return to its origin. People influence one another recursively. Investigations can investigate prior investigations.

But epistemic support cycles must not amplify themselves indefinitely.

A claim repeated through five descendants of one source is not five independent confirmations. Circular grounding without external anchors must not manufacture confidence.

---

## 8. Human beings are not Boolean containers of knowledge

One of Niccolò's core product differentiators is the explicit modeling of the human relationship with knowledge.

The fact that information was available to someone does not mean they accessed it. Access does not imply perception. Perception does not imply attention. Attention does not imply examination. Examination does not imply correct comprehension. Comprehension does not imply acceptance. Acceptance does not imply later recall.

Relevant states include:

- information availability;
- potential exposure;
- actual exposure;
- access;
- perception;
- attention;
- examination;
- partial comprehension;
- correct comprehension;
- mistaken comprehension;
- retention;
- recall;
- suspicion;
- inclination;
- doubt;
- belief;
- disbelief;
- suspended judgment;
- claimed knowledge;
- perceived knowledge;
- attributed knowledge;
- knowledge supported by available evidence.

These are contextual, temporal and attributable. They do not form one rigid monotonic ladder.

An actor may understand something and later forget it. They may believe something without understanding its technical basis. They may claim knowledge they do not possess. They may possess knowledge without recognizing its significance.

### 8.1 Expertise is not awareness

An expert in cybersecurity may have no knowledge that a particular server was exposed last Tuesday.

An inexperienced employee may nevertheless be the only person who witnessed a decisive event.

Expertise in a domain and awareness of a particular fact are independent dimensions.

### 8.2 Interest, curiosity, indifference and aversion

Humans do not merely know or fail to know.

They can:

- want to know;
- be indifferent;
- prefer not to know;
- want to know only enough to act;
- avoid knowledge to preserve plausible deniability;
- delegate knowing to someone else;
- avoid formal documentation of something they informally want done;
- be curious for personal, professional, strategic or emotional reasons.

Curiosity is not simply “high interest”. It can have a specific object, purpose and context.

### 8.3 Persistent dispositions versus current reactions

A person may generally avoid conflict but confront someone in one exceptional situation. A naturally curious person may avoid a topic that threatens their self-image. A normally skeptical executive may uncritically accept information from one trusted subordinate.

Niccolò distinguishes persistent dispositions from immediate states and reactions.

Observed behavior can update a hypothesis about disposition. It does not prove personality.

### 8.4 Epistemic acts

People act in relation to knowledge.

Niccolò can represent acts such as:

- investigate;
- disregard;
- deepen;
- defer;
- seek clarification;
- avoid acquiring;
- delegate;
- reconsider;
- challenge;
- preserve ambiguity;
- request independent verification.

This permits a more useful question than merely “What did the director know?”

It can ask:

> “When warning signs appeared, what did the director do in relation to them?”

That distinction matters in governance, negligence, accountability and strategy.

---

## 9. Knowledge about knowledge

Human interaction often depends less on first-order facts than on beliefs about what others know.

For example:

```text
Ana knows F.
Bruno believes Ana knows F.
Carlos believes Bruno thinks Ana does not know F.
```

Niccolò does not require fixed classes such as FirstOrderKnowledge, SecondOrderKnowledge and ThirdOrderKnowledge.

An epistemic state can itself become the object of another epistemic state.

Conceptually:

```text
EpistemicState
    subject = Bruno
    object  = EpistemicState(
                  subject = Ana,
                  object  = F,
                  stance  = KNOWS
              )
    stance = BELIEVES
```

The semantic model has no arbitrary depth ceiling. Computation is bounded by relevance, purpose and resource budget.

This enables strategically important asymmetries:

```text
A knows secret S.
B knows that A knows S.
A does not know that B knows this.
```

B may possess an advantage not merely because B knows S, but because B knows that A misjudges B's knowledge.

Niccolò must keep separate:

- reality;
- an actor's belief about reality;
- one actor's belief about another actor's belief;
- the investigator's attribution of those states.

No truth transfer occurs merely because the nesting is deep.

---

## 10. Trust, reputation, loyalty, ego and identity

Human relationships produce epistemic effects, but the same outward behavior can have very different causes.

### 10.1 Trust is directed and multidimensional

A person can trust someone to report technical facts accurately while distrusting them to reveal conflicts of interest.

Trust can concern dimensions such as:

- truthfulness;
- competence;
- confidentiality;
- judgment;
- loyalty;
- procedural reliability.

Trust has a truster, trustee, dimension, context, scope, basis, confidence and time.

Trust is not generally symmetric or transitive.

Trust is not the same as dependence. Dependence is not the same as reliability. Reliability is not proof of benevolent intention.

### 10.2 Epistemic reputation

Reputation is community-dependent and historically formed. Several reputation reports may share one origin and should not automatically count as independent evidence.

A person's reputation for honesty in one community may differ sharply from another community's experience, and both reports can be genuine.

### 10.3 Loyalty is not obedience

Loyalty can be directed toward:

- a person;
- group;
- institution;
- family;
- cause;
- principle;
- professional role.

Its nature may be:

- affective;
- ideological;
- economic;
- professional;
- institutional;
- familial;
- grateful;
- reciprocal;
- dependency-based.

Its degree, scope, conditions, limits, reciprocity and time matter.

Loyalty is distinct from:

- obedience;
- submission;
- agreement;
- compliance;
- cooperation;
- dependency;
- aligned interests.

Multiple loyalties can conflict.

A subordinate can disobey a manager while acting loyally toward the institution. A whistleblower can violate a local command while complying with a higher obligation. Apparent disloyalty to a person can coexist with loyalty to a principle.

### 10.4 Ego protection and identity protection are distinct

Human conduct is often driven by motives more subtle than money or ideology.

**Ego protection** concerns personal self-image, status and face. It can include:

- avoiding humiliation;
- avoiding appearing incompetent;
- saving face;
- preserving status;
- preserving a sense of importance;
- defending one's self-image;
- seeking recognition;
- resisting admission of error.

**Identity protection** is broader and concerns a social, professional, political, familial, institutional or group identity.

Someone may resist evidence because it threatens the identity “competent surgeon”, “good police officer”, “loyal executive”, “good parent” or “member of the right side”, even when no immediate status loss is involved.

The two mechanisms can overlap, but they should not be collapsed.

### 10.5 Ego can be exploited strategically

Consider:

> “You are probably the only person here capable of understanding this.”

The statement may be sincere, false, manipulative or some mixture.

Its function may include:

- flattery;
- status reinforcement;
- trust manipulation;
- information elicitation;
- recruitment into a coalition;
- encouraging disclosure.

No false proposition is required for the influence attempt to be strategically deceptive.

Deceptive structures can also be compositional. Acts, omissions, motives, targets and messages may `part_of`, `sustain`, `conceal`, `derive_from`, `coordinate_with` or `reinforce` a larger deceptive process. Grouping several acts into one deception hypothesis does not prove common intent; that coordination must itself be grounded.

---

## 11. Power over what others know

Formal authority is only one kind of power.

A person with no senior title may control:

- who receives information;
- who receives it first;
- who can access specialists;
- which version reaches the executive team;
- which anomalies are treated as important;
- which questions are considered respectable;
- what gets summarized away.

Niccolò therefore distinguishes **epistemic influence**, **epistemic power** and **formal authority**.

### 11.1 EpistemicInfluence

Influence may target:

- attention;
- interest;
- curiosity;
- trust;
- belief;
- acceptance;
- interpretation;
- investigative priority;
- willingness to examine an issue.

Separate:

- capacity to influence;
- the influence act;
- target actor;
- target epistemic dimension;
- intended effect;
- actual effect;
- perceived success.

A true statement can be used to direct attention away from something more important.

### 11.2 EpistemicPower

Epistemic power is contextual capacity to alter availability, attention, interpretation, trust, acceptance or investigation.

Capacity, exercise and outcome are different.

Someone may have power and never use it. Someone may attempt influence and fail. Someone may create an effect without intending it.

### 11.3 Epistemic asymmetry

An **EpistemicAsymmetry** can concern:

- information;
- access;
- understanding;
- meta-knowledge;
- influence.

For example:

```text
A knows F.
B does not know F.
A knows B does not know F.
B believes A also does not know F.
```

That asymmetry is strategically richer than “A knows more than B”.

---

## 12. Pervasive deception: the foundational design principle

Niccolò does not treat deception as a special tag attached to obviously false statements.

Deception can occur in every layer:

- source material;
- testimony;
- interpretation;
- provenance;
- question framing;
- investigative priority;
- institutional reporting;
- recommendations;
- knowledge about knowledge;
- even the process intended to detect deception.

### 12.1 Falsity is not deception

A false statement may be an innocent error.

A deceptive act may contain no false statements at all.

Therefore:

```text
false != intentionally deceptive
true  != non-deceptive
```

### 12.2 Truth can compose deception

Suppose A, B and C are all true facts.

Someone deliberately presents only A and B, knowing that C changes the interpretation.

```text
all transmitted propositions = true
overall communicative effect  = misleading
```

Deception can operate through:

- selection;
- omission;
- framing;
- ordering;
- timing;
- aggregation;
- decontextualization;
- selective precision;
- strategic ambiguity.

### 12.3 Omission is not automatically deceptive

Missing information may result from:

- ignorance;
- forgetfulness;
- irrelevance;
- fear;
- hierarchy;
- limited access;
- time pressure;
- confusion;
- deliberate concealment.

The omission is observable. The motive requires assessment.

### 12.4 Intent, attempt, target and effect are separate

A deceptive process can distinguish:

- deceptive intention;
- attempted deception;
- means;
- target;
- intended effect;
- actual effect;
- perceived success.

Example:

```text
A tries to convince B that C is corrupt.
B recognizes the manipulation.
D overhears the conversation and believes it.
```

The attempt against B failed. The unintended effect on D succeeded.

### 12.5 Inadvertent propagation

Someone can honestly repeat manipulated information.

The intermediary may be:

- unwitting;
- negligent;
- careless;
- trusting;
- partially aware;
- consciously cooperative.

The truth or falsity of the propagated content and the intermediary's intent are separate.

### 12.6 Self-deception

Niccolò can represent hypotheses of self-deception without casually diagnosing psychology.

Possible mechanisms include:

- rationalization;
- selective attention;
- motivated interpretation;
- ego protection;
- identity protection;
- avoidance of threatening evidence;
- preserving a desirable self-image.

The system should say, in effect:

> “Observed behavior is consistent with selective avoidance of information that threatens self-image, among other explanations.”

It should not elevate an ambiguous behavioral pattern into a psychiatric fact.

### 12.7 Deception can be recursive

```text
A deceives B.
B investigates A.
A attempts to deceive B's investigation.
C investigates B's investigation.
A attempts to influence C.
```

Niccolò does not need a new epistemic engine at each level.

The same core abstractions - actors, knowledge, questions, evidence, provenance, arguments, assessments, influence and relationships - remain reusable.

---

## 13. Deception can target the investigation itself

A sophisticated adversary does not need to falsify evidence.

They may manipulate:

- which questions are asked;
- which hypotheses enter the shortlist;
- which witnesses are prioritized;
- which period is analyzed;
- which anomalies are considered normal;
- which resources are allocated;
- what is excluded from the final summary;
- which uncertainty is made visible to decision makers.

The result can be a procedurally competent investigation moving in the wrong direction.

### 13.1 Question-space bias

An investigation can be incapable of discovering the true explanation because its question space excludes it.

Example:

> “Which employee leaked the document?”

may exclude:

> “No employee leaked it; the document was externally accessible.”

Collecting more and more employee evidence does not fix the original framing error.

### 13.2 Investigative selection bias

The questions that were not asked, sources not consulted, hypotheses not considered and exclusions not justified are part of the investigation's epistemic record.

### 13.3 The investigation can create apparent evidence

An investigator may reveal information to a witness, alter the witness's understanding, and later interpret the witness's changed behavior as independent evidence.

Niccolò needs to preserve that causal possibility rather than treating every later occurrence as uncontaminated observation.

### 13.4 Self-sealing hypotheses

Suppose the hypothesis is:

> “Pedro is an infiltrator.”

If every possible observation becomes support - suspicious behavior supports it; normal behavior proves sophistication; absence of evidence proves concealment - the method has become self-sealing.

This does not prove the hypothesis false.

It shows the evaluation method is epistemically defective because no practical observation can count against it.

Niccolò should be able to flag the risk of a self-sealing hypothesis.

### 13.5 Resource bias is not epistemic evidence

If 90 percent of processing budget is spent on Investigation Line A and 10 percent on B, finding more items in A does not by itself show A is more promising.

Niccolò must distinguish:

- no evidence found;
- evidence absent;
- evidence unavailable;
- evidence not collected;
- material not yet examined;
- analysis truncated by resource limits.

Processing coverage qualifies conclusions.

---

## 14. InvestigationIntegrity

The integrity of an investigation is itself investigable.

**InvestigationIntegrity** is multidimensional and contestable. There is no need for a single magic score.

Relevant dimensions can include:

- independence;
- source selection;
- provenance quality;
- evidence preservation;
- question framing;
- hypothesis coverage;
- discriminatory quality of inquiry;
- reasoning quality;
- handling of contrary evidence;
- consistency of criteria;
- treatment of dissent;
- consolidation practices;
- historical traceability;
- reviewability;
- resource coverage;
- confidentiality and authorized routing;
- resistance to external interference.

The point is not that Niccolò should automatically accuse investigators of bias.

The point is that the investigative process should not be epistemologically invisible merely because it is the process doing the investigating.

---

## 15. Agency: intention, action, causality and responsibility

Human reasoning becomes dangerous when several distinct questions are collapsed into one.

Niccolò distinguishes:

- **Goal:** a desired state.
- **Intention:** an actor's orientation toward bringing something about.
- **Plan:** an organized proposal for action.
- **Decision:** a choice.
- **Authorization:** permission or mandate to act.
- **Execution:** an act actually carried out.
- **Outcome:** what actually happened.

A decision does not prove execution. Execution does not prove the intended outcome occurred. An outcome does not prove the actor intended it.

### 15.1 Causality

A **CausalRelationship** may distinguish:

- causes;
- contributes to;
- enables;
- prevents;
- intensifies;
- attenuates;
- necessary condition;
- sufficient condition;
- jointly contributing factors.

Causality has context, time, evidence and attribution.

Temporal sequence alone does not establish causation.

Causal relationships are not universally transitive.

### 15.2 Causality is not responsibility

An actor can causally contribute to an outcome without being morally, legally or institutionally responsible for it.

Another actor can be normatively accountable without being the direct physical cause.

Therefore Niccolò keeps separate:

- causal contribution;
- intention;
- responsibility;
- obligation;
- accountability;
- authority;
- authorization.

### 15.3 Norms, policies and commitments

A **Norm**, **Policy** or **Commitment** has origin, scope, conditions, exceptions, temporal validity and interpretation.

Different questions include:

- Does the norm exist?
- Is it formally valid?
- Is it applicable here?
- Was it complied with?
- Is it perceived as legitimate?
- Is actual practice consistent with the stated policy?

These should not be collapsed.

Conflicting norms and disputed precedence are themselves investigable.

---

## 16. Cooperation, coordination, collusion and strategy

Similar behavior can arise through very different social mechanisms.

Niccolò distinguishes:

- **Cooperation:** actors contribute toward compatible or shared outcomes.
- **Coordination:** behavior is organized relative to others, explicitly or tacitly.
- **Collusion:** a qualified form of coordination whose significance depends on the relevant normative context.
- **Convergence:** actors independently reach similar behavior or interests.

Convergence does not establish coordination.

Coordination can be:

- explicit;
- tacit;
- centralized;
- distributed.

Membership in a group does not automatically establish shared intent.

### 16.1 Strategy and strategic position

A **Strategy** or **StrategicPosition** can involve:

- actors;
- goals;
- resources;
- constraints;
- dependencies;
- power;
- information;
- beliefs about other actors;
- expected reactions;
- possible alternative actions.

Strategic behavior is often based on perceived reality, which may itself be wrong.

A strategy can therefore be coherent relative to a mistaken belief.

---

## 17. Vulnerability, dependency, leverage, threat, coercion and autonomy

Niccolò should not collapse all human motivation into a generic “incentive” or “pressure” field.

### 17.1 Vulnerability

Vulnerability is contextual exposure to an adverse consequence, not a deficiency in a person.

It may be:

- material;
- economic;
- professional;
- institutional;
- legal;
- reputational;
- emotional;
- informational.

It may be:

- actual;
- potential;
- perceived;
- unknown;
- misattributed;
- transient;
- conditional;
- persistent.

### 17.2 Dependency

A dependency exists when achieving a goal or maintaining a condition depends on an actor, resource or circumstance.

Dependencies may be:

- unilateral;
- reciprocal;
- asymmetric;
- distributed;
- material;
- affective;
- professional;
- informational;
- institutional;
- strategic.

Actual alternatives, perceived alternatives and known alternatives are different.

Voluntary dependency does not imply submission.

### 17.3 Leverage

Leverage is contextual capacity to influence another actor through resources, competence, authority, knowledge, dependence or vulnerability.

It may be actual, potential, perceived or simulated.

Capacity, intent, exercise, perceived exercise and outcome remain separate.

### 17.4 Threat

An adverse condition is not the same as a communicated threat.

Separate:

- capability;
- intent;
- communication;
- credibility;
- consequence.

### 17.5 Coercion

Coercion concerns compromised actual or perceived freedom to act.

It can be:

- explicit;
- implicit;
- direct;
- mediated;
- episodic;
- recurring;
- continuing.

Responses can include submission, apparent compliance, partial compliance, resistance or protective behavior.

Submission does not establish consent or loyalty.

### 17.6 Incentive and pressure

Incentives can be economic, affective, reputational, professional, ideological, recognition-based, belonging-based or ego-based.

Pressure is broader and can be hierarchical, social, economic, familial, institutional, psychological or self-imposed. It can be deliberate or incidental, cumulative or intermittent.

### 17.7 Constraint

Constraints can be real or perceived and can be physical, material, legal, institutional, informational, temporal, moral or voluntarily adopted.

### 17.8 Autonomy

Autonomy is contextual actual or perceived ability to:

- decide;
- act;
- refuse;
- know;
- disagree;
- exit;
- seek review;
- revise commitments.

A person can have technical autonomy and little political autonomy. They can be free to disagree but unable to exit. They can be formally authorized to refuse while realistically fearing retaliation.

---

## 18. Information access, disclosure and compromise

Counterintelligence and sensitive investigations require careful distinctions around information flow.

### 18.1 InformationAccess

Access is not one boolean.

Separate:

- formal authorization;
- practical possibility;
- technical possibility;
- indirect access;
- conditional access;
- former access;
- actual access.

A person authorized to access information may never have done so. Someone without formal authorization may have gained practical access.

### 18.2 Disclosure

A **Disclosure** can distinguish:

- intended versus unintended;
- authorized versus unauthorized;
- intended recipient versus actual recipient;
- content transmitted;
- outcome.

### 18.3 InformationCompromise

Information compromise can affect different properties independently:

- confidentiality;
- integrity;
- authenticity;
- availability;
- reliability;
- provenance.

An artifact can remain authentic and intact while confidentiality is lost. A document can remain confidential while integrity is corrupted. A source can be authentic and unreliable.

### 18.4 Publicly inferable sensitive information

Sensitive information may be inferable from public or contextual material.

The fact that a person knows something sensitive does not prove they received it through an unauthorized channel.

An investigator should ask whether the information could have been independently inferred by a knowledgeable outsider.

An anomaly is therefore an indication to investigate, not proof of compromise. The baseline against which something looks anomalous may itself be wrong, incomplete or strategically manipulated.

---

## 19. Leaks, origin, propagation and return

Information propagation is rarely a clean chain from one guilty originator to one recipient.

Possible roles include:

- original author;
- earliest known holder;
- first unauthorized discloser;
- conscious intermediary;
- unwitting intermediary;
- intended recipient;
- actual recipient;
- beneficiary;
- organizer;
- later redistributor.

These roles do not automatically imply guilt, intent or coordination.

### 19.1 Propagation changes information

As information moves, it may be:

- summarized;
- simplified;
- distorted;
- partially omitted;
- recombined;
- reframed;
- translated;
- decontextualized.

The content received later may not be semantically identical to the content originally disclosed.

### 19.2 Circular return

Information can return to its originator through a propagation network.

That may indicate:

- false independence;
- successful dissemination;
- a possible secrecy breach;
- transformation during propagation;
- a separate independent rediscovery.

It is an indication to investigate, not automatic proof of the pathway.

### 19.3 Motives remain contestable

Potential motives include:

- money;
- loyalty;
- revenge;
- ego;
- recognition;
- ideology;
- protection;
- whistleblowing;
- coercion;
- fear;
- opportunism.

Claimed motives may themselves be deceptive.

---

## 20. Counterintelligence is a vertical, not a second epistemic engine

Counterintelligence is where many of Niccolò's abstractions become simultaneously important: access, secrecy, deception, loyalty, vulnerability, propagation, meta-knowledge, investigation integrity and adaptive behavior.

But it should not require a parallel epistemology.

The same core model should handle ordinary investigations, corporate politics, legal disputes and CI stress cases.

### 20.1 Infiltration and hidden relationships

Niccolò must distinguish:

- formal affiliation;
- cooperation;
- convergent interests;
- independent conduct;
- hidden relationships;
- affinity;
- alignment;
- coordination;
- subordination;
- dependency;
- coercion;
- control.

Preexisting hostile intent is only one possibility.

Relationships can develop through:

- later recruitment;
- gradual compromise;
- coercion;
- opportunism;
- changed conviction;
- personal loyalty;
- dependency.

Access, capability, intent and behavior remain separate.

Dissent, whistleblowing, kinship, shared beliefs or friendship do not establish infiltration. Authorized undercover activity is also conceptually distinct from hostile infiltration even when some observable behaviors overlap.

Likewise, absence of suspicious behavior does not prove “excellent infiltration”. That move risks making the hypothesis self-sealing.

### 20.2 Accusation can itself be strategic

An infiltration allegation may be:

- true;
- mistaken;
- retaliatory;
- politically convenient;
- protective;
- deliberately deceptive.

The accusation is therefore part of the investigable universe, not privileged metadata.

### 20.3 Counterintelligence can be deceived by agenda

An adversary may influence what CI investigates rather than falsify the evidence CI examines.

Wrong priorities or exclusions can arise from:

- deception;
- incompetence;
- resource constraints;
- legitimate strategic choices;
- organizational politics;
- mistaken baselines.

Niccolò should preserve those competing explanations.

### 20.4 Institutional summaries can distort correct findings

An investigation can produce technically correct findings that are later presented misleadingly through omission, confidence inflation, selective quotation or decontextualization.

Therefore the path from evidence to executive summary is itself epistemically relevant.

---

## 21. Who investigates the investigators?

A system designed to reason about manipulation cannot make investigators epistemically untouchable.

Investigators, supervisors, oversight bodies, complaint handlers, policy authors and Niccolò-assisted analyses can themselves become subjects of authorized investigation.

This does **not** mean automatic recursive suspicion.

The governing distinction is:

> Epistemic representability does not imply operational authority.

A relationship can be modeled without granting permission to inspect it. A person can be investigable in principle without being in scope. A policy author can be relevant context without becoming an investigation target.

### 21.1 Oversight creates its own power

Oversight bodies can accumulate:

- informational power;
- investigative power;
- disciplinary power;
- hierarchical influence;
- reputational power;
- career power.

Individuals can sometimes appropriate institutional powers for personal purposes.

Therefore oversight integrity is not guaranteed by the word “oversight”.

### 21.2 Independence is multidimensional

Independence can be:

- formal;
- operational;
- financial;
- informational;
- decisional;
- perceived.

An institution can be formally independent while practically dependent on another institution for information or career advancement.

### 21.3 Institutional capture can be partial

Capture can be:

- localized;
- partial;
- temporary;
- persistent;
- issue-specific.

Evidence that one unit is compromised does not automatically contaminate the entire institution.

### 21.4 Complaint handling is investigable

How complaints are received, categorized, routed, delayed, reframed, closed and reported can itself become a legitimate subject of investigation.

### 21.5 Autonomy of investigators matters

Meaningful integrity may require the ability to:

- disagree;
- preserve dissent;
- challenge assumptions;
- request review;
- refuse improper interference;
- maintain an independent assessment.

Formal hierarchy must not be confused with epistemic authority.

---

## 22. Compartmentalization: secrecy in the world is not software authorization

Niccolò separates two fundamentally different things:

```text
modeled-world secrecy != product access control
```

The model may represent:

> “Ana should not know this investigation exists.”

The product may separately enforce:

> “Ana's user account cannot access this investigation.”

The first is an investigable fact or policy claim. The second is deterministic software security.

Niccolò must never derive product authorization automatically from modeled authority, loyalty, trust or secrecy.

### 22.1 What can be compartmentalized

Any epistemic object may require protection of:

- existence;
- content;
- partial content;
- source identity;
- origin;
- rationale;
- conclusion;
- relationship;
- review signal;
- metadata.

If the existence of an investigation is protected, the system must avoid leaking it indirectly through:

- graph edges;
- notifications;
- search results;
- counts;
- summaries;
- LLM context;
- recommendation hints;
- derived metadata.

### 22.2 Actual access is not assimilation

An actor may technically access a document without reading it, may read without understanding it, or may understand without retaining it.

The model should not collapse authorization, access, exposure and knowledge.

### 22.3 Distributed fragments do not imply collective knowledge

Suppose:

```text
A knows fragment F1.
B knows fragment F2.
C knows fragment F3.
```

An authorized Niccolò analysis may combine them and formulate H.

That does not justify saying:

> “The organization knew H.”

A more accurate statement is:

> “Information distributed across several compartments was sufficient, when combined, to formulate H.”

This distinction can be legally and organizationally critical.

### 22.4 Confidentiality can constrain revision

Suppose Investigation A relies on a finding whose supporting source is later compromised in protected Investigation B. The investigator in A is not authorized to know B exists.

Niccolò must distinguish:

- epistemic need for review;
- authorization to perform the review;
- authorization to know the reason;
- authorization to signal the existence of a problem.

If an authorized routing mechanism exists, the need can be escalated appropriately. If no lawful or permitted route exists, the system cannot guarantee cross-compartment correction without violating the confidentiality rule.

Security constraints are real epistemic constraints on what a particular actor can responsibly know.

---

## 23. Vulnerabilities can emerge from combinations

Not all vulnerabilities belong to one person or one artifact.

They can be relational or compositional.

Examples:

- System A reveals department.
- System B reveals travel.
- System C reveals project dates.
- Together they identify a sensitive project team.

Each disclosure may be individually harmless. The combination is not.

Human vulnerabilities can similarly emerge from:

- conflicting obligations;
- dependence on one decision maker;
- asymmetrical access;
- lack of exit alternatives;
- reputational exposure;
- concentration of knowledge;
- unquestioned premises;
- institutional lock-in.

### 23.1 Epistemic vulnerabilities

Niccolò can reason about vulnerabilities such as:

- unquestioned premises;
- premature semantic consolidation;
- false source independence;
- treating authority as credibility;
- omitted hypotheses;
- resistance to revision;
- confusion between absence of evidence and evidence of absence;
- a single analyst becoming an epistemic single point of failure.

### 23.2 Success can create fragility

A highly trusted expert may become so central that nobody can meaningfully challenge them.

A successful compartmentalization policy may prevent anyone from seeing a dangerous cross-compartment pattern.

A defensive measure can create new dependencies.

Protection is therefore not automatically risk reduction in every dimension.

---

## 24. Protection, containment and epistemic recovery

Protective responses can be:

- preventive;
- mitigating;
- containing;
- corrective;
- restorative;
- adaptive.

Under uncertainty, Niccolò should prefer proportionate measures whose epistemic and operational consequences remain visible.

Examples include:

- privilege review;
- separation of duties;
- independent review;
- evidence preservation;
- reducing unnecessary dependencies;
- preserving dissent;
- strengthening provenance;
- verifying a critical fact independently.

Suspicion must not automatically become accusation.

### 24.1 Protective measures need lifecycle semantics

A protective measure can have:

- purpose;
- trigger;
- conditions;
- duration;
- review date;
- expiry;
- reversibility;
- expected benefit;
- residual risk;
- introduced risk.

A temporary measure should not silently become permanent merely because nobody revisited it.

### 24.2 Epistemic containment

If a dependency becomes potentially compromised, Niccolò should help:

```text
identify affected dependencies
        |
        v
pause reliance where appropriate
        |
        v
preserve the material and provenance
        |
        v
seek independent verification
        |
        v
review downstream findings
```

It should not automatically erase evidence or globally declare every descendant false.

### 24.3 Epistemic recovery

After compromise, the useful question is not merely:

> “Was the investigation contaminated?”

It is also:

> “What can still be saved?”

Recovery includes:

- identifying usable material;
- identifying independently corroborated material;
- revising conclusions;
- preserving unresolved gaps;
- restoring controls;
- documenting residual uncertainty.

---

## 25. From analysis to ActionRecommendation

Niccolò is not confined to describing the past.

An **ActionRecommendation** is a first-class, attributable and revisable proposal to an authorized human decision maker.

A recommendation can include:

- purpose;
- recipient;
- context;
- rationale;
- grounding;
- conditions;
- alternatives;
- expected consequences;
- uncertainty;
- risks;
- reversibility;
- dependencies;
- expected reactions;
- effect on the investigation itself.

The critical boundary is:

```text
Recommendation
    != Decision
    != Authorization
    != Plan
    != Execution
    != ActualOutcome
```

Niccolò recommends.

A human decides.

Someone with appropriate authority authorizes.

A plan may then be developed.

Someone executes.

The outcome is later observed and evaluated.

### 25.1 Recommendation strength is not hypothesis confidence

Suppose:

```text
H = Pedro may be leaking information.
confidence(H) = low
```

A recommendation to review privilege assignments may still be strong because:

- the cost is low;
- reversibility is high;
- the structural access risk exists independently of Pedro's guilt;
- the consequence of continued exposure could be serious.

Therefore:

```text
low-confidence hypothesis != weak recommendation
```

The inverse also holds: strong confidence in a fact does not guarantee a useful action exists.

### 25.2 Alternatives include inaction

Important recommendations should be comparable with alternatives such as:

- alternative action A;
- alternative action B;
- do nothing;
- delay;
- gather more discriminating information first.

Relevant dimensions can include:

- cost;
- urgency;
- expected benefit;
- reversibility;
- risk;
- dependencies;
- possible reactions;
- epistemic contamination;
- discriminatory value.

No universal formula is required to preserve these distinctions.

### 25.3 Structural protection can survive rejection of suspicion

Suppose a recommendation to narrow dangerous privileges was initially triggered by suspicion of insider misconduct.

Later the suspicion is rejected.

The privilege review can remain justified if the access architecture was independently unsafe.

Implementing a protective measure does not retroactively prove the suspicion that originally prompted examination.

A recommendation can also become obsolete while the underlying facts remain unchanged - for example because the decision deadline passed, a safer alternative appeared, costs changed, authorization changed or the intervention would now create disproportionate new risk. Recommendation validity is therefore contextual and temporal, not a permanent property of its factual premises.

### 25.4 Intervention changes the world

A recommendation that becomes a decision and execution changes circumstances.

It can:

- alert actors;
- change incentives;
- change loyalties;
- alter vulnerability;
- destroy opportunities;
- create or destroy evidence;
- modify trust;
- provoke deception;
- reduce or increase autonomy;
- alter future investigative options.

Reasoning after intervention must not continue as if the pre-intervention world were unchanged.

### 25.5 Human action boundary

Niccolò can recommend epistemic, protective, preventive, corrective, strategic and exploratory actions.

It can represent and analyze information about violence, attacks and consequences where they are relevant to an investigation.

It does **not** recommend or develop operational plans for kinetic attacks against persons or installations, including euphemistic “neutralization”.

---

## 26. Semantic discipline instead of false precision

Niccolò is intentionally rich without requiring complicated mathematics merely to look sophisticated.

Many dimensions can initially use domain-specific qualitative enums.

Examples:

```text
expertise: novice / basic / intermediate / advanced / expert
relevance: negligible / low / moderate / high / critical
confidence: very_low / low / moderate / high / very_high
```

Interest, dependence, trust, influence, loyalty and other concepts have their own appropriate semantics.

Three rules are essential:

1. **Unknown, none and not applicable are different.**
2. **Ordinal does not mean interval or ratio.** “High” is not mathematically twice “moderate”.
3. **Different dimensions are not interchangeable.** Confidence in an assessment of loyalty is not the degree of loyalty itself.

Qualitative representation should preserve context, author, rationale, evidence and time so later calibration can be introduced without rewriting semantic history.

Rejecting false universal precision does not mean rejecting quantitative evidence. Amounts, timestamps, durations, access frequencies, measured error rates, statistical estimates, laboratory measurements and other genuinely quantitative observations should preserve their numeric values, units, methods, populations/samples, uncertainty and provenance where applicable. The prohibition is against pretending that qualitative human judgments become scientifically meaningful merely because software assigns them a universal number.

### 26.1 Relationship families have different contracts

A generic graph edge is not enough.

Relationship families include:

| Family | Examples | Important semantic cautions |
| --- | --- | --- |
| Provenance | acquired_from, extracted_from, derived_from, transformed_from, transmitted_from | provenance is not epistemic support |
| Epistemic support | supports, weakens, challenges premise, challenges source reliability | support is generally not symmetric or transitive |
| Dependence | jointly required, alternative sufficient, complementary, conditional | necessity for one argument is not necessity for the conclusion |
| Semantics | exact equivalence, contextual equivalence, more-specific-than, entails, partial overlap, contradiction | uncertain equivalence must not silently merge identity |
| Causality | causes, contributes, enables, prevents | sequence is not causality; causality is not responsibility |
| Context | relevant_to, applicable_under, scoped_to, valid_at | relevance is not generally transitive |
| Actor epistemics | believes, knows, suspects, trusts, attends_to, is_interested_in | no truth transfer across actors or nested levels |
| Institutional | authority, authorization, delegation, obligation, responsibility | authority does not imply truth or software permission |
| Genealogy | refines, supersedes, retracts, consolidates, separates, reinterprets | history must survive |
| Composition | part_of, composed_of, grouped_with | membership does not imply shared intent |

Logical properties apply to the defined relationship under matching conditions, not automatically to fallible assessments asserting that relationship.

### 26.2 Relationships can themselves be investigated

Suppose:

```text
A trusts B.
C believes A trusts B.
D challenges C's attribution.
```

These are not the same statement.

A relationship can have identity, provenance, evidence, context, temporal scope, competing assessments and genealogy.

This does not mean every edge must become a heavyweight object. Lightweight representation is acceptable when the richer properties are irrelevant.

The rule is semantic, not aesthetic:

> A relationship that matters to reasoning must be representable with the qualifications needed to investigate it.

### 26.3 N-ary relationships matter

Some reasoning cannot be represented faithfully by independent binary edges.

Three premises may jointly support a conclusion while none supports it alone.

A composition or argument structure can represent this without requiring a specialized hypergraph database.

---

## 27. Core semantic invariants

Across all domains, Niccolò preserves several transversal contracts.

### Identity

Consolidation is reversible and grounded. Similarity does not silently erase distinct objects or occurrences.

### Provenance

Origin, acquisition, derivation and transformation remain traceable.

### Attribution

Originator, actor, assessor, interpreter and decision maker remain distinct.

### Context

Meaning, relevance, applicability and relationships preserve the conditions under which they hold.

### Temporal separation

Current reconstruction does not overwrite historical actor knowledge.

### Grounding

Consequential conclusions expose what they depend on, including alternatives and conditions.

### Revision

Historical state survives later correction.

### Interpretation state

The original human utterance, current interpretation, confirmation status, interpreter, rationale, history and consequential downstream dependencies remain distinguishable. Confirmation of meaning does not confirm truth.

### Progressive formalization

The complete epistemological model must be representable without requiring every item to be eagerly decomposed into its richest possible form. Niccolò formalizes more deeply when conflict, consequence, ambiguity, revision or investigative purpose makes the distinction useful. Unknown is preferable to unsupported completion.

### Human interaction

The complete ontology remains richer than any single screen, but the MVP deliberately exposes core epistemic abstractions as an **expert workbench**. The intended user is a cultivated, domain-trained professional who can explicitly author hypotheses, assumptions, information gaps, findings, arguments, typed relations and grounding structures.

This is not a promise that every internal distinction becomes a form field. The UI should expose abstractions when explicit human authorship is useful and hide machinery that does not improve the investigation.

### AI proposal boundary

Model-generated reasoning inside Niccolò may propose hypotheses, challenges, questions or other structure. Proposal is not canonical epistemic fact. Proposed structure should be materialized only when it preserves a necessary distinction or has identifiable investigative value; plausible but operationally useless elaboration is epistemic spam.

Automatic interpretation of the investigator's own free text into epistemic abstractions is a separate capability and is **outside the MVP**.

### Uncertainty

Uncertain material is not automatically promoted into fact because it was repeated or institutionally adopted.

### Authority

Adoption, authorization and institutional endorsement do not create truth.

### Confidentiality

Retention of information does not confer permission to access it. Conversely, append-oriented epistemic history does not override lawful erasure, redaction, anonymization or retention limits; controlled tombstones may preserve revision semantics without preserving prohibited content.

### Human authority

Niccolò can recommend, challenge, warn and explain. Consequential human decisions remain attributable to humans.

---

## 28. Where the epistemology becomes a product

The capabilities below are not separate products. They are different applications of one epistemic core.

### 28.1 Police internal affairs and independent oversight

Niccolò can help examine suspected misconduct, evidence substitution or disappearance, selective investigation, coercive relationships, protection networks and possible institutional capture.

It can reconstruct:

- who had access;
- who had authority;
- who actually exercised influence;
- who knew what and when;
- what alternatives were not pursued;
- why a case was closed;
- what dissent existed;
- whether the investigative process itself deserves scrutiny.

A reviewer can ask not only whether a file contains evidence of misconduct, but whether the original inquiry was capable of discovering misconduct at all.

### 28.2 Criminal investigations and intelligence analysis

Niccolò can reconstruct timelines, actor relationships, source dependencies, alternative explanations, testimonial inconsistencies and possible deception.

It can distinguish:

- an alleged event;
- claims about the event;
- knowledge about those claims;
- later interpretations;
- institutional adoption;
- causal responsibility;
- authority to act.

It can identify information that actually discriminates among explanations rather than simply rewarding accumulation of material.

### 28.3 Corporate counterintelligence and insider risk

Niccolò can reason about suspicious information flows, disputed disclosure origins, inappropriate privileges, hidden dependencies, loyalty, coercion, reputation, ego, strategic omission and access anomalies.

It can preserve the difference between:

- opportunity and action;
- capability and intent;
- anomaly and proof;
- structural vulnerability and personal guilt;
- repeated allegations and independent origins.

### 28.4 Law firms, litigation and dispute resolution

Legal teams can use the model to:

- maintain alternative case theories;
- distinguish testimony from what testimony proves;
- expose circular corroboration;
- preserve historical knowledge states;
- model conflicting institutional duties;
- identify hidden assumptions;
- track dissent and revision;
- distinguish factual confidence from action strategy.

The fictional [Aurelius & Finch bonus review](USE_CASE_LAW_FIRM_BONUS_REVIEW.md) illustrates an adversarial human-context investigation.

### 28.5 Corporate investigations, fraud, audit and governance

Niccolò can help trace a decision to the information actually available when it was made, the influence surrounding it, the provenance of assertions and the reasons a conclusion was adopted.

It can model fear of retaliation, professional dependence, loyalty, reputation, ego protection and institutional incentives without converting inferred motives into established fact.

A whistleblower allegation and the organization's handling of the allegation can both be investigated.

### 28.6 Strategy, due diligence and negotiation

Niccolò can represent:

- stated and inferred interests;
- practical leverage;
- dependencies;
- commitments;
- authority;
- alternatives;
- informational asymmetries;
- beliefs about the counterparty;
- scenarios under explicit premises.

A party may act rationally relative to a false belief. A negotiation can be shaped by what each side thinks the other side knows, not merely by the underlying facts.

### 28.7 Sensitive and isolated deployments

Organizations may need the reasoning model without exposing investigation material to externally hosted services.

Isolated or air-gapped deployment is therefore an important product direction. Actual deployment controls, model availability, integrations and security must be verified for the specific environment.

The epistemological model itself does not assume cloud possession of the organization's information.

---

## 29. One model, six interacting functional slices

The approved architecture can be implemented and reviewed through six coherent slices.

### A - Investigable universe

Actors, identity, events, evidence artifacts, claims, provenance and qualified relationships.

### B - Directed investigation

InformationGap, Question, QuestionAct, Response, Hypothesis, relevance, answerability and discriminatory power.

### C - Reasoning and review

Premise, PremiseAdoption, Interpretation, Scenario, Argument, Inference, Assessment, Challenge, Refutation, Finding and explicit dependency semantics.

### D - Human dynamics and strategy

Epistemic relationships and dispositions, intentions, motives, trust, reputation, loyalty, ego, identity, power, influence, strategy, dependency, leverage, coercion, autonomy and pervasive deception.

### E - Proactive advice

Scenario, Vulnerability, ActionRecommendation, alternatives, expected outcomes, risks, conditions and retrospective evaluation.

### F - Integrity and governance

Deterministic permissions, decisions, authorizations, dissent, history, coverage, compartmentalization, InvestigationIntegrity and investigation of investigations.

These are not sequential stages.

A supports B and D. B and D inform C. C informs E. F constrains and enriches every slice. New actions feed back into the universe and can alter every other slice.

Counterintelligence, oversight and other verticals emerge from composition rather than requiring duplicate epistemic engines.

---

## 30. The complete epistemological model belongs in the MVP

The approved MVP scope is the **complete epistemological model**, not a rudimentary model surrounded by promises of later intelligence.

Implementation can be incremental and pragmatic, but the required distinctions must support meaningful behavior rather than exist only as vocabulary.

Capabilities that cannot be made rudimentary without damaging the product include:

- provenance and independent-origin analysis;
- selective revision across alternative and partial grounding;
- competing hypotheses that need not be exclusive or exhaustive;
- hidden premises and investigative gaps;
- discriminatory questions;
- higher-order human epistemic state;
- pervasive, multi-layer deception;
- strategic loyalty, vulnerability, dependence and coercion reasoning;
- investigation integrity;
- compartment-aware behavior;
- grounded recommendations and their revision.

Implementation should avoid unnecessary mathematical sophistication where it adds little value. Complex probability calibration, universal numerical epistemic scoring, autonomous resource optimization and formal strategic utility models are not goals in themselves.

Qualitative semantics can be excellent engineering when they preserve distinctions and traceable reasoning.

### 30.1 Human-guided investigative control

The investigator controls the domain of circumstances, priorities and investigative budgets.

Niccolò may:

- propose leads;
- show underexamined lines;
- recommend resource redistribution;
- expose actual resource usage;
- explain tradeoffs;
- warn about coverage limits.

It must not silently optimize away the human's investigative priorities.

### 30.2 Real permissions remain deterministic product security

No modeled authority, loyalty, trust or inferred relationship grants software access.

Authorization is enforced outside LLM judgment.

### 30.3 Representation is not operational authority

Niccolò may represent acts, intentions, vulnerabilities, violence, illegal conduct and institutional abuse when relevant to understanding a case.

Representability does not mean the software is authorized to perform, facilitate or recommend every represented act.

### 30.4 Current modality boundary

Information supplied by users about images or video can participate as evidence. Autonomous image and video understanding remains outside the initial product scope.

### 30.5 MVP interaction model: structured expert authoring

The MVP assumes a **cultivated, domain-trained investigator** who understands the reasoning distinctions relevant to the professional domain and explicitly authors the epistemic structure through the UI.

The investigator can create and connect objects such as:

- hypotheses;
- assumptions;
- information gaps;
- findings;
- arguments;
- typed semantic, causal, contextual and genealogical relations;
- grounding structures, including joint and alternative routes.

Free text may of course appear **inside an explicitly selected abstraction** - for example, the text of a Hypothesis or Finding. The important point is that the user selected the semantic role. Niccolò does not infer that role from an unstructured investigator narrative in the MVP.

This makes the MVP an expert epistemic workbench rather than a general-purpose natural-language front end.

### 30.6 Complete model, progressive expert formalization

"Complete model in the MVP" means **complete semantic capability**, not eager maximal decomposition of every fact, person or relationship.

The trained investigator decides when a richer distinction is useful. A simple representation may remain simple until conflict, consequence, ambiguity, revision or investigative purpose requires a deeper structure. The system validates the resulting contracts, preserves provenance and dependencies, and helps reason over the explicit model.

Unknown remains preferable to invented detail. Receiving a document, for example, does not imply reading, comprehension or belief unless the investigator explicitly represents or grounds those additional states.

The graph is a data structure, not necessarily the primary visual interface. The workbench may expose important ontology concepts directly without forcing the user to manipulate every internal implementation detail.

### 30.7 Post-MVP: investigator free-text interpretation

Automatic interpretation of the investigator's free text into epistemic abstractions is **outside the MVP**.

A later capability may act as a compiler from ordinary language into proposed structured objects and relations. It should explain why each mapping was proposed, preserve ambiguity, distinguish observable text from inferred structure, and allow the user to accept, modify or reject proposals - including bulk acceptance as an observable UI action.

That future layer must never pretend to know whether a user carefully read a proposal, understood it instantly or accepted it mechanically. It can record only observable actions and their provenance.

The current MVP is intentionally useful without this compiler: **structured reasoning first; natural-language interpretation later**.

---

## 31. Implementation stance: preserve engineering, not old shortcuts

The epistemological revolution does not imply a rewrite of everything.

Many existing mechanisms can remain valuable:

- provenance storage;
- historical state;
- auditability;
- authorization infrastructure;
- ingestion;
- asynchronous processing;
- PostgreSQL persistence;
- analysis orchestration;
- existing qualified temporal actor-state concepts.

The rule is:

> Preserve engineering where the semantics survive; generalize or replace it where they do not.

Before major code changes, existing Rust types, PostgreSQL schema and migrations, APIs, frontend contracts and executable tests should be classified as:

1. **Preserve** - existing semantics already satisfy the approved contract.
2. **Generalize** - useful implementation exists but needs broader semantics.
3. **Replace** - current representation conflicts with the approved model.
4. **Create** - approved capability lacks a suitable implementation primitive.

Shared substrate should be prioritized before proliferating feature-specific types:

- persistent epistemic identity;
- qualified relationship contracts;
- provenance and attribution;
- contextual and temporal qualification;
- genealogy and revision;
- grounding and dependency semantics;
- scoped assessment;
- confidentiality and visibility enforcement;
- cycle-safe, non-amplifying reasoning.

Rust is a natural place for typed contracts and deterministic semantic validation. PostgreSQL remains appropriate for durable relational persistence, referential integrity, history and efficient projections.

A graph-shaped domain does not automatically require a graph database. Hierarchies remain useful projections for command, delegation and navigation, but they are not assumed to be the fundamental structure of the epistemic domain.

Rich relationships do not automatically require a heavyweight ontology engine.

LLMs can propose interpretations, relationships and questions. They do not bypass deterministic contracts or authorization.

Extensibility should follow semantic boundaries rather than dictate them. Where a real variation point exists - for example provenance validation, a narrow assessment policy or a custody checker - implementation can later expose a replaceable interface through the simplest mechanism that preserves the contract. The ontology should not be distorted merely to make plug-ins convenient.

---

## 32. Twenty-six adversarial cases used to attack the model

The architecture was deliberately tested with difficult conceptual cases. These are not benchmark results and do not prove completeness. Their purpose is to expose contradictions, missing distinctions and unsafe propagation rules.

### AT-01 - False independence of evidence

Several apparently independent reports descend from one source. Expected behavior: preserve separate artifacts while identifying shared origin; do not count descendants as independent corroboration.

### AT-02 - Partial contamination of knowledge

A source is compromised on one aspect but independently reliable on another. Expected behavior: scoped reassessment, not blanket invalidation.

### AT-03 - Hindsight contamination

Later knowledge is incorrectly attributed to an actor at an earlier time. Expected behavior: separate historical actor knowledge from current reconstruction.

### AT-04 - Deception without false assertions

Every statement is true, but selection and omission deliberately induce a false interpretation. Expected behavior: deception remains representable without requiring a false proposition.

### AT-05 - Mistaken identity contamination

Two actors or propositions are incorrectly consolidated. Expected behavior: reversible identity repair preserving historical references, provenance and consequences.

### AT-06 - Compartmented knowledge and conflicting findings

Two authorized teams see different evidence and reach different findings. Expected behavior: preserve both findings and confidentiality; do not pretend a single omniscient investigator existed.

### AT-07 - Investigation creates its own apparent evidence

An investigative act changes behavior that is later interpreted as independent evidence. Expected behavior: model the investigative intervention and possible causal contamination.

### AT-08 - Investigative selection bias

A conclusion reflects what was selected for examination rather than the full available universe. Expected behavior: coverage and exclusions qualify the result.

### AT-09 - Apparent contradictions

Two statements appear contradictory but differ in time, scope, reference or interpretation. Expected behavior: test semantic compatibility before declaring contradiction.

### AT-10 - Authority versus epistemic resolution

An authorized leader declares a matter closed despite unresolved central questions. Expected behavior: distinguish operational closure from epistemic resolution.

### AT-11 - Circular epistemic grounding

A claim returns through several derivative reports and appears independently corroborated. Expected behavior: detect ancestry and prevent self-corroboration.

### AT-12 - Hypothetical scenarios contaminate actual-world claims

A scenario assumption is later treated as established fact. Expected behavior: preserve scenario context and conditionality.

### AT-13 - Nonexclusive and nonexhaustive hypotheses

Several explanations can coexist and the real explanation may not yet be listed. Expected behavior: no forced probability partition or exhaustive set.

### AT-14 - Attributed knowledge about another's knowledge

One actor believes another actor knows something. Expected behavior: preserve the nested attribution rather than promoting it to first-order knowledge.

### AT-15 - Self-sealing hypothesis

Evidence for, against and absence of evidence are all interpreted as support. Expected behavior: identify testability failure without asserting the hypothesis false.

### AT-16 - Niccolò contaminates its own investigation

A generated summary or prior model output later becomes evidence without proper provenance. Expected behavior: treat Niccolò outputs as derivative investigable artifacts, never privileged independent sources.

### AT-17 - Recommendation remains appropriate after a suspicion is rejected

A protective recommendation was initially prompted by suspicion but has independent structural justification. Expected behavior: revise the suspicion without automatically discarding the recommendation.

### AT-18 - Improper attribution of collective knowledge

Different people hold different fragments that only together imply a conclusion. Expected behavior: do not say “the organization knew” unless an appropriate collective-knowledge claim is actually grounded.

### AT-19 - Interpretation of silence

A person does not respond. Expected behavior: preserve competing interpretations such as consent, fear, indifference, lack of awareness, inability to respond or strategic silence; do not manufacture assent.

### AT-20 - Conflicting authorizations and obligations

A subordinate disobeys a supervisor while complying with an authorized auditor and institutional policy. Expected behavior: represent multiple normative relationships without collapsing loyalty, legality, obedience and legitimacy.

### AT-21 - Epistemic revision under confidentiality

A protected investigation discovers that another investigation relies on compromised information, but the latter team is not authorized to know why. Expected behavior: respect access rules, route review only through authorized mechanisms and acknowledge limits when no route exists.

### AT-22 - Computational limits misrepresented as epistemic conclusions

One line receives much more processing budget than another. Expected behavior: do not infer that the first is more promising merely because more material was found there; expose coverage and truncation.

### AT-23 - Hidden premises in questions

A question presupposes disclosure, participation or guilt that has not been established. Expected behavior: surface presuppositions and propose more discriminating prior questions.

### AT-24 - Improper propagation of epistemic support

Evidence establishes access and acquaintance but not disclosure. Expected behavior: distinguish capability, opportunity, intent, execution and consequence; support is not generally transitive.

### AT-25 - Simultaneous compromise across layers

A complex case combines shared source origin, misinterpreted authentic evidence, fear-driven omission, hidden loyalty, compartmented favorable evidence, pressure for closure and an independent structural vulnerability. Expected behavior: preserve all distinctions simultaneously, resist premature guilt, respect confidentiality, retain dissent and still recommend proportionate structural protection where independently justified.

### AT-26 - Weaponized formalism

An interested actor continuously injects marginally plausible alternative hypotheses, low-value provenance objections and formally valid challenges in order to consume investigative attention, delay closure beyond a deadline or prescription period, and create the appearance that nothing can ever be concluded. Expected behavior: preserve legitimate objections without granting veto power merely because an objection is formally representable; assess novelty, grounding, relevance, discriminatory value, cost, deadline impact and whether the challenge can materially change a consequential conclusion. Human attention is a scarce investigative resource.

The combined cases did not demonstrate a need for a separate new epistemic engine. The approved abstractions appear composable, which is exactly the architectural objective.

---

## 33. What meaningful validation should look like

Validation should test behavior across interactions, not merely whether class names or database tables exist.

Representative acceptance targets include:

- semantic consolidation preserves independent origins;
- circular corroboration does not amplify confidence;
- hypothetical scenarios cannot leak into actual-world grounding;
- support and opposition remain separately visible;
- hidden question premises can be identified;
- only affected dependencies are marked for reassessment;
- a conclusion can survive the failure of one argument when separately grounded;
- resource-limited analysis is shown as incomplete rather than negative evidence;
- protected existence does not leak through derived UI or LLM context;
- collective knowledge is not inferred from distributed fragments without justification;
- recommendations preserve their own grounds and can outlive rejected suspicions;
- intervention changes subsequent reasoning context;
- higher-order beliefs remain attributed rather than promoted to fact;
- provenance remains distinct from evidentiary support;
- authority does not become truth;
- modeled secrecy does not become product authorization;
- real-world cycles do not become epistemic self-corroboration.

The 25 desk cases are therefore candidates for executable contract, property, integration and regression tests during implementation reconciliation.

---

## 34. The point

For Niccolò, information about an investigation is information.

Information about the integrity of the investigators is information.

A source's path to a belief is information. A decision to avoid learning is information. The fact that an executive believes a subordinate does not know something is information. The influence behind an inquiry's agenda is information. The false impression created by true statements is information. The fear behind an omission may be information. The provenance of a conclusion is information. The fact that a case was closed without resolving its central question is information.

But none of those facts should silently become more than the evidence warrants.

The aim is not omniscience. It is not automated guilt. It is not a machine declaring whose motives are pure or whose loyalty is real.

The aim is to make human reasoning in difficult, adversarial circumstances more explicit, discriminating, revisable and accountable - to investigate reality without surrendering to the authority, reputation, volume or confidence with which a version of it is presented.

**Insolence to authority. Humility to evidence.**

---

## Appendix A. Compact semantic reference

This appendix condenses the approved model for readers who need the vocabulary without rereading the full narrative.

### Informational and evidentiary core

- Actor
- Identity / disputed identity / alias
- Event / Action / Outcome
- EvidenceArtifact
- Communication / QuestionAct / Response / Disclosure
- Claim / semantic proposition
- Provenance / derivation / ancestry / transformation
- InformationAccess
- InformationCompromise
- QualifiedRelationship

### Investigation core

- InformationGap
- Question
- Hypothesis
- RelevanceAssessment
- DiscriminatoryPowerAssessment
- ResolutionAssessment
- Investigation status versus epistemic resolution
- Investigation coverage

### Reasoning core

- Interpretation / interpretation confirmation state / revision history
- Premise
- PremiseAdoption
- Scenario
- Argument
- Inference
- Challenge
- Refutation
- Assessment
- Finding
- explicit grounding/dependency roles
- genealogy / supersession / retraction / consolidation / separation

### Human epistemic core

- availability / exposure / access
- perception / attention / examination
- comprehension / retention / recall
- suspicion / doubt / belief / disbelief / suspended judgment
- claimed / perceived / attributed knowledge
- Interest / Curiosity / Indifference / KnowledgeAversion
- epistemic acts such as investigate, defer, avoid, delegate, reconsider
- MetaEpistemicState
- EpistemicTrust
- EpistemicReputation
- EpistemicInfluence
- EpistemicPower
- EpistemicAsymmetry

### Human strategic and institutional core

- Goal / Intention / Plan
- Decision / Authorization / Execution
- Authority / Delegation
- Responsibility / Obligation / Accountability
- Norm / Policy / Commitment
- Strategy / StrategicPosition
- Loyalty
- Cooperation / Coordination / Collusion / Convergence
- Vulnerability
- Dependency
- Leverage
- Threat
- Coercion
- Incentive
- Pressure
- Constraint
- Autonomy

### Deception core

- deceptive intention
- deceptive attempt
- means
- target
- intended effect
- actual effect
- perceived success
- omission / selection / framing / ordering / timing / aggregation / decontextualization
- inadvertent propagation
- self-deception hypotheses
- ego protection
- identity protection
- deception directed at investigations
- self-sealing hypothesis risk

### Governance and action core

- Compartmentalization
- lawful erasure / controlled redaction / tombstone
- InvestigationIntegrity
- ActionRecommendation
- alternative actions including inaction/delay
- protective measure
- containment
- recovery
- residual and introduced risk
- human decision boundary

---

## Appendix B. Relationship semantics at a glance

### Provenance relationships

Describe acquisition, extraction, derivation, transformation and transmission. They answer “where did this come from?” They do not by themselves answer “why should this conclusion be believed?”

### Epistemic support relationships

Describe how material supports, weakens or challenges a proposition, premise, inference or source. They are generally directional, non-symmetric and non-transitive.

### Dependence relationships

Describe how reasoning relies on inputs: jointly necessary, alternative sufficient, complementary, conditional or aspect-specific.

### Semantic relationships

Describe exact equivalence, contextual equivalence, specialization, implication, partial overlap, incompatibility and contradiction. Exact equivalence requires matching scope. Apparent contradiction is not automatically logical contradiction.

### Causal relationships

Describe causes, contribution, enabling, prevention and joint conditions. Causality does not automatically imply intention, guilt or responsibility.

### Contextual relationships

Describe relevance, applicability, scenario, scope, time, purpose and perspective.

### Actor-epistemic relationships

Describe belief, knowledge, suspicion, trust, interest, attention and other actor-relative epistemic states. Truth does not propagate across actors or nested states.

### Institutional relationships

Describe authority, authorization, delegation, obligation and responsibility. These do not imply epistemic truth, trust, loyalty or product permission.

### Genealogical relationships

Describe refinement, supersession, retraction, consolidation, separation and reinterpretation. History remains preserved.

### Composition relationships

Describe part-whole and grouping structures. Membership does not establish shared intent or shared truth.

---

## Appendix C. Selected research foundations

Niccolò's conceptual design draws on an interdisciplinary review of epistemology, human behavior, strategic communication, argumentation, evidence, causality and professional investigations. The detailed annotated review contains 66 sources and records inspection levels, competing findings, limitations and design implications.

These works inform the distinctions in the design. They do not establish the validity, accuracy or operational maturity of Niccolò, nor imply that it implements any published formalism verbatim.

### Knowledge, testimony and human relationships

- Sperber et al. (2010), *Epistemic Vigilance*. Evaluation of information and communicators. https://doi.org/10.1111/j.1468-0017.2010.01394.x
- Fricker (2007), *Epistemic Injustice*. Credibility attribution, power and interpretation. https://academic.oup.com/book/32817
- Perner & Wimmer (1985), *John Thinks That Mary Thinks That...*. Second-order belief representation. https://www.sciencedirect.com/science/article/pii/0022096585900517
- Wellman & Liu (2004), *Scaling of Theory-of-Mind Tasks*. Knowledge access, desires and beliefs. https://pubmed.ncbi.nlm.nih.gov/15056204/
- Clark & Brennan (1991), *Grounding in Communication*. Shared understanding and communicative coordination.

### Deception, strategic communication and human judgment

- Crawford & Sobel (1982), *Strategic Information Transmission*. Communication under divergent preferences. https://www.jstor.org/stable/1913390
- Kuran (1995), *Private Truths, Public Lies*. Private preferences and public positions. https://sites.duke.edu/timurkuran/research/books/private-truths-public-lies-the-social-consequences-of-preference-falsification/
- DePaulo et al. (2003), *Cues to Deception*. Evidence and limitations of deception cues. https://pubmed.ncbi.nlm.nih.gov/12555795/
- Bond & DePaulo (2006), *Accuracy of Deception Judgments*. Limits of human deception judgment. https://pubmed.ncbi.nlm.nih.gov/16859438/
- Levine, Truth-Default Theory. Human default assumptions and deception detection.
- Tversky & Kahneman (1974), *Judgment Under Uncertainty*. Heuristics and biases. https://doi.org/10.1126/science.185.4157.1124
- Gigerenzer & Gaissmaier (2011), *Heuristic Decision Making*. Ecological and adaptive perspectives on heuristics. https://doi.org/10.1146/annurev-psych-120709-145346
- Simon, work on bounded rationality. Decision making under limited information and resources.

### Reasoning, provenance and revision

- Dung (1995), *On the Acceptability of Arguments and Its Fundamental Role in Nonmonotonic Reasoning, Logic Programming and n-Person Games*. Abstract argumentation.
- Prakken (2010), *An Abstract Framework for Argumentation with Structured Arguments*. Different targets of argumentative challenge. https://journals.sagepub.com/doi/10.1080/19462160903564592
- Alchourrón, Gärdenfors & Makinson (1985), *On the Logic of Theory Change*. Belief revision and contraction. https://doi.org/10.2307/2274239
- de Kleer (1986), *An Assumption-Based TMS*. Assumption contexts and dependency management. https://doi.org/10.1016/0004-3702(86)90080-9
- Belnap, four-valued logic and reasoning with inconsistent information.
- W3C (2013), *PROV-O: The PROV Ontology*. Provenance of entities, activities and agents. https://www.w3.org/TR/prov-o/
- Lamport, work on distributed systems and ordering, relevant to distinctions among event occurrence and observation.

### Causality, institutions and authority

- Rubin (1976), *Inference and Missing Data*. Missingness and the mechanism producing absence. https://doi.org/10.1093/biomet/63.3.581
- Lagnado and related work on causal reasoning and responsibility attribution.
- Jones & Sergot (1996), *A Formal Characterisation of Institutionalised Power*. Institutional power and authorization. https://doi.org/10.1093/jigpal/4.3.427
- NIST work on attribute-based access control, informing the separation of modeled authority from deterministic product authorization.

### Professional investigation and investigative integrity

- Heuer (1999), *Psychology of Intelligence Analysis*. Competing hypotheses and analytic bias. https://www.cia.gov/resources/csi/books-monographs/psychology-of-intelligence-analysis-2/
- National Academies (2014), *Identifying the Culprit: Assessing Eyewitness Identification*. Witness memory and investigative procedure. https://www.nationalacademies.org/read/18891/chapter/2
- Cooper & Meterko (2019), *Cognitive Bias Research in Forensic Science: A Systematic Review*. Bias and forensic practice. https://doi.org/10.1016/j.forsciint.2019.01.016
- NIST human-factors work relevant to evidence interpretation, procedure and bias.

For the complete research trace, see [EPISTEMIC_DOMAIN_INTERDISCIPLINARY_REVIEW_V1.md](../architecture/EPISTEMIC_DOMAIN_INTERDISCIPLINARY_REVIEW_V1.md).

---

## Appendix D. Architecture and implementation trace

This product document explains the approved epistemological model. Detailed engineering and research trace lives in:

- [Epistemic architecture revamp](../architecture/EPISTEMIC_ARCHITECTURE_REVAMP_IN_PROGRESS.md)
- [Epistemic implementation contracts - pre-code checkpoint](../architecture/EPISTEMIC_IMPLEMENTATION_CONTRACTS_CHECKPOINT.md)
- [Independent interdisciplinary epistemic-domain review](../architecture/EPISTEMIC_DOMAIN_INTERDISCIPLINARY_REVIEW_V1.md)
- [Qualified temporal actor state](../architecture/TEMPORAL_ACTOR_STATE_MVP.md)
- [MVP composite evidence and human-guided circumstances](../architecture/MVP_COMPOSITE_EVIDENCE_AND_HUMAN_GUIDED_CONTEXT.md)
- [Epistemic weighting API](../architecture/EPISTEMIC_WEIGHTING_API.md)
- [System integrator boundaries and hardening](../architecture/SYSTEM_INTEGRATOR_BOUNDARIES_AND_HARDENING.md)

The next engineering obligation is not more speculative ontology. It is reconciliation with the real Rust/PostgreSQL implementation, followed by explicit **preserve / generalize / replace / create** decisions and executable tests derived from the approved semantic contracts.
