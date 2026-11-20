# Niccolò: An Epistemology for the Real World

*Investigating information, human behavior, power and deception — including the integrity of the investigation itself.*

**A product and conceptual architecture overview.** This document describes Niccolò's approved epistemological design and MVP target. The model is not a claim that every capability already exists in the shipping software. The architecture review, conceptual adversarial cases and implementation evidence remain separately traceable.

## Executive perspective

**What if an impeccably documented investigation was designed never to discover the truth?** What if an apparent leak was actually independent discovery? What if a witness was honest, every quotation accurate, and the resulting narrative nevertheless misleading?

Niccolò is designed for precisely these circumstances. Its purpose is to give investigators, intelligence analysts, executives, legal teams and independent reviewers an explicit, revisable model of reality as they encounter it through information.

- **Counterintelligence:** examine compromised information, hidden loyalties, coercion, misleading appearances and the possible compromise of counterintelligence itself.
- **Independent oversight and police internal affairs:** investigate the investigators, their choices, their sources, their dependencies and the integrity of the oversight process.
- **Corporate and legal investigations:** separate actual evidence from repeated allegations, reconstruct who knew what and when, preserve competing arguments and reassess conclusions when their foundations change.
- **Strategic judgment:** investigate power, influence, dependence and human relationships; identify discriminating questions and recommend proportionate actions under explicit uncertainty.

The distinguishing proposition is that deception, human knowledge and investigative integrity are not exceptional modules. They permeate the same epistemological universe. A finding is not a terminal answer: it can become the subject of another investigation, be challenged, be revised or contribute to a recommendation without losing its original foundations.

The intended capability goes beyond document retrieval and fluent synthesis. Niccolò must preserve the semantics of the relationships among information, reasoning and human activity, including the circumstances in which apparently convincing evidence or reasoning becomes unreliable. Its target is a thinking companion that can disagree with the investigator, without appropriating the investigator's authority.

```text
              A: Information and provenance
                       |           |
                       v           v
              B: Investigation <--> D: Human dynamics
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

The diagram summarizes dependencies and feedback, not a mandatory processing sequence. Section 14 describes the six slices in more detail. The same core makes it possible to investigate a board decision, a compromised source and the integrity of an oversight body.

## 1. Reality does not submit a clean dataset

A company president says something. A subordinate remembers only part of it. A witness has a reason to omit a name. A police officer has access to evidence that another officer cannot see. A respected expert is wrong. A junior analyst notices an inconsistency that everyone else has learned not to question. An investigator closes a line of inquiry because a superior wants it closed, not because the question has been answered.

Ordinary information systems can preserve records of these events. A language model can summarize those records. Neither operation, by itself, constitutes an investigation.

Niccolò is designed around a more ambitious problem: constructing and continuously revising an explicit, inspectable representation of what can be known about a human situation, how that knowledge was formed, who had access to it, what remains uncertain, and how the situation may have been manipulated.

Its epistemological model is not a database of truths. It is a universe of investigable information: propositions, observations, relationships, reasoning, uncertainties, competing explanations, human intentions and institutional decisions. The origin and status of every consequential piece of knowledge matter.

**An authority can decide. It cannot decide reality.** An untrusted person can tell the truth. An honest person can transmit a lie. A sophisticated deception may consist entirely of true sentences. A perfectly legitimate investigation can reach the wrong answer because it never asked the right question.

These are not edge cases. They are the environment in which Niccolò must work.

## 2. The informational universe

Niccolò's universe is informational. People, organizations, objects, physical events and relationships are represented through information about them. The system does not possess the physical object, witness the event or magically know the state of the world.

- **Actors and identities:** persons, organizations, informal groups, roles, pseudonyms and possibly mistaken or disputed identities. Formal membership, effective affiliation and perceived affiliation are distinct.
- **Occurrences and artifacts:** events, actions, documents, messages, interviews, recordings, records, observations and descriptions of physical evidence. A record of a laboratory sample is information about the sample, not the sample itself.
- **Claims:** semantic propositions whose scope, modality, conditions and reference determine what they actually assert. A claim may be derived from several communications, or arise independently of any communication.
- **Communications:** declarations, requests, answers and other acts are occurrences, with speakers, recipients, timing and circumstances. The act of making a statement and the truth of its content are separate facts to investigate.
- **Relationships:** qualified, investigable connections among objects, assertions and even other relationships. A relationship may have its own evidence, context, assessor, temporal interval and competing interpretations.

Consider a police evidence sample. Information concerning its collection, chain of custody, storage, examinations, disappearance, replacement and laboratory results can be related without pretending the sample is a native digital object. The same epistemic mechanisms apply to an altered spreadsheet, a substituted attachment or a forged report. Integrity of an artifact, authenticity of its origin, completeness of a representation and truth of its contents are different questions.

Identity is itself fallible. Two reports may concern the same person; two different persons may share a name. Two statements may express the same proposition without sharing an origin. Consolidation must be reversible and must preserve original representations, occurrences and provenance. Similarity is not identity.

Niccolò separates the time an event occurred, the time it was recorded, the time a particular actor learned about it and the time the investigator reconstructed it. A conclusion reached today must not silently become information that a witness supposedly knew yesterday.

## 3. Investigation is a living epistemic process

Investigations are not merely searches for confirmation. They have objectives, lines of inquiry, relevant unknowns, questions, evolving hypotheses and decisions. New information can modify the significance of material that was collected months earlier.

An **InformationGap** identifies a missing semantic aspect of a situation. A **Question** formulates a problem to investigate. A **QuestionAct** records someone actually asking a question. A **Response** is a communicative occurrence, not an automatic solution. One response can address several questions; several responses can support, challenge or merely appear to address the same proposition. A response may yield no extractable claim at all.

There is no mandatory pipeline from unknown to gap to question. A gap can be recognized before anyone formulates a question. A question can reveal several gaps. A response can answer a question that nobody had yet thought to ask. Questions themselves can contain incorrect assumptions: asking why an employee destroyed a file presupposes that the employee destroyed it.

```text
 Information and events       Investigator's objective
           |                            |
           v                            v
      Claims, context <-------- Gaps and questions
           |                            ^
           v                            |
   Competing explanations ---> Missing discriminators
           |                            |
           v                            v
    Arguments and findings <--- New information sought
           |                            ^
           +--- review / challenge -----+
```

The investigator can pursue a hypothesis without adopting it. Hypotheses may coexist, overlap, conflict, refine or depend on one another. The set of hypotheses need not be exhaustive or mutually exclusive. An explanation that includes both incompetence and concealment need not be forced into a choice between them.

### Relevance is not truth

The relationship `relevant_to` is contextual and graded. A seemingly insignificant message might be critical to one hypothesis and irrelevant to another. Niccolò distinguishes the degree of relevance, the rationale for that assessment and confidence in the assessment itself.

Relevance is also not identical to evidentiary strength. A fabricated confession may be highly relevant to an investigation of deception and worthless as direct support for the alleged offense.

### Information that discriminates between explanations

The most useful information is not always the information that produces the most findings. It may be a small missing fact that distinguishes two otherwise plausible explanations.

Niccolò is designed to examine whether proposed evidence would change a hypothesis assessment; which explanations could survive either answer; whether a question is answerable; what assumptions it embeds; what alternative questions would be more informative; and where the observed material suggests possibilities not yet formulated. A poorly structured inquiry can collect an enormous quantity of information and learn almost nothing about the issue that matters.

This is particularly important when reviewing another investigator's work. The questions that were never asked, the alternatives that were never entertained, and the investigative choices that prevented discovery can be as important as the answers that appear in the file.

**A rigorous investigation can be incapable of discovering its own subject.** Consider a hypothetical inquiry into the disappearance of a forensic record. The team verifies every recorded transfer and interviews every officer named on those records. The unexplored alternative is that the recording process itself was incomplete. A perfectly consistent chain of recorded transfers does not establish that all relevant transfers were recorded. The discriminating gap concerns the completeness of the record, rather than the accuracy of its entries. This is a different investigative question, not just another request for more of the same evidence.

Niccolò's intended contribution is to keep that distinction explicit: describe the scope of observation, identify the assumption on which the original inference depends, propose alternative hypotheses and questions, and preserve the investigator's decision about whether to pursue them. It must also be able to recognize when a proposed question cannot discriminate, or when the available material warrants no further conclusion.

### Closure is an operational act

An active investigation may be epistemically close to resolution; a closed investigation may leave its essential question unanswered. Operational state (active, suspended, closed) is distinct from epistemic resolution (unresolved, partial, resolved). Resolution assessments identify the aspects clarified, remaining gaps, evidence and applicable purpose; they do not require an artificial percentage.

Even a sole investigator with complete authority sees who decided to close the matter, why and on what basis. In collaborative work, subordinate investigators can formulate their own lines and submit findings or proposed changes to a lead. Partial adoption, dissent, rejection and later reconsideration preserve their intellectual history. Formal acceptance is not an epistemic property of the proposal.

## 4. How knowledge is constructed, challenged and revised

The reasoning model makes a distinction among the material being reasoned about, the meaning assigned to it, the reasoning performed, and its formal conclusions.

- **Interpretation:** a contextual assignment of meaning to material or to another interpretation. Competing readings can coexist. The original content must survive.
- **Hypothesis:** a potential explanation or proposition to investigate. Its relevance, support, adoption and operational investigation status are separate.
- **Premise and PremiseAdoption:** a proposition used as an assumption for a purpose; who adopted it, under what conditions and why. Using a premise is not declaring it true.
- **Scenario:** a real or hypothetical perspective with its own assumptions, scope and consequences. Results remain conditional on those assumptions when reused. Hypothetical material must not become actual-world evidence.
- **Inference:** a reasoning act with attributable inputs, method, conditions, result and available rationale. Validity, applicability and the truth of its premises are separate.
- **Argument:** a composable body of grounding, including claims, evidence, premises, inferences and other arguments. Counterarguments and challenges can target a specific dependency, inference or conclusion. Undermining support is not identical to refuting the conclusion.
- **Assessment:** an attributable, dimension-specific evaluation with criterion, value, confidence, rationale, scope, time and grounding. Its subject can be a proposition, source, relationship, interpretation, hypothesis, argument, another assessment or other investigable object.
- **Finding:** an identifiable investigative result, potentially partial or inconclusive, with supporting and competing grounds, conditions, uncertainties and genealogy. Findings are not compressed into untraceable assertions.

These are distinct semantic roles, not a demand for an isolated software subsystem for every noun. A single proposition, for example, may be a hypothesis in one context and an adopted premise in another.

### Provenance is not optional

Suppose one interview is summarized in a memorandum. An analyst cites the memorandum in a report. An AI system summarizes that report. A second investigator cites all three documents as independent confirmation.

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
          Assess the actual grounds
          A's descendants are not B.
```

Three representations of one account can be three historical artifacts without being three independent origins. Conversely, two independently obtained accounts of the same event must not disappear through semantic deduplication. Independence is assessed for a specific aspect of the information; a document might contain original observations and recycled allegations simultaneously.

Provenance covers acquisition, extraction, transmission, transformation and human or computational processing. It is not an endorsement of the contents. A previous Niccolò report or generated summary is investigable material, not privileged truth. Cycles of repetition cannot manufacture external grounding.

### Revision without epistemic amnesia

Discovering forged evidence, a compromised source, an incorrect identity, an inapplicable inference or a previously unknown deception can require the review of conclusions that depend upon it. Niccolò must be able to identify the affected dependencies and distinguish what is impaired from what remains independently supported.

```text
       Source A ---\                 Source C
       Source B ----> Argument 1         |
           [B compromised]              v
                         |          Argument 2
                         v               |
                   Reassessment          |
                         |               |
                         +-------+-------+
                                 v
                    Finding: revised grounds,
                    confidence and conditions
```

The failure of an input that was jointly necessary to one argument is not necessarily the failure of the conclusion. Independent or conditional alternatives may survive. A compromised source may be reliable on another aspect; a forged document may itself become evidence of a separate event. A discovery does not automatically invalidate every descendant, and old findings do not vanish when superseded.

This principle extends to a chain of investigations, not just a single report. Reuse must preserve conditions, original grounding and genealogy so an authorized later investigator can determine why an earlier conclusion changed.

## 5. The human relationship with knowledge

One of Niccolò's central design commitments is that **humans are not Boolean containers of beliefs and knowledge**. The fact that information was available to a person does not mean they received it. Receipt does not imply attention. Attention does not imply comprehension, acceptance or remembrance.

The model distinguishes, among other things:

- Information available to an actor; exposure; actual access; perception and attention; examination; correct, partial or mistaken comprehension; retention and recall.
- Suspicion, doubt, inclination to believe or disbelieve, belief, suspended judgment, claimed knowledge and attributed knowledge. These are not equivalent to the truth of the proposition.
- Interest, curiosity as a specialization of interest, indifference, perceived significance and aversion to acquiring knowledge. A person can actively wish not to know something without believing it false.
- Immediate reactions to information versus persistent epistemic dispositions: tendencies to attend, investigate, seek confirmation, distrust a source, or reconsider an accepted explanation.
- Deliberation about knowledge: choosing to disregard, defer, investigate, deepen, delegate, avoid or revisit. A deliberate decision to disregard has a context and potentially a meaningful reason. It is not synonymous with simple inattention.

Knowledge relationships have a holder and an object, context, time, intensity, origin of the attribution and grounds. An actor's description of their own state, a documented manifestation and the investigator's inference are different. An expert in a subject may be completely unaware of a specific event. An executive may genuinely believe that all employees know a policy that nobody has read.

Actors may also have desires about the epistemic states of others: wanting another person to know, remain unaware, become curious, examine a question, accept a claim or reconsider a belief.

### Knowledge about knowledge

Humans reason about other humans' knowledge. A thinks B knows something; B suspects A has discovered that B knows it; C mistakenly believes both were informed by D. Each layer is a distinct proposition about an attributed epistemic state, not proof of the underlying proposition.

Higher-order knowledge, beliefs, perceptions and suspicions can therefore be represented recursively. The architecture does not decree a fixed semantic depth. Actual processing is bounded by the objective, relevance, available information and user-controlled resource budget.

An organization is not a single mind. Information known to three people is not necessarily information known by the company, its board, its CEO or its other employees. Collective claims must preserve the basis on which collective knowledge or agreement was attributed.

## 6. Politics: power over what others know

Influence is not an incidental side effect of human communication. It is a subject of investigation in its own right. Humans influence what others attend to, trust, question, believe, investigate and ultimately decide. Their capacity to do so, their intention to do so, the act itself and its actual or perceived effect can all differ.

**Epistemic power** concerns the capacity to influence another's informational and epistemic environment. It need not be accompanied by a formal title. A secretary who controls access to a decision maker, a technically indispensable engineer, a trusted intermediary and a senior official may possess very different kinds of power.

**Epistemic asymmetry** concerns differences in access, understanding, knowledge about others' knowledge and influence. Having an informational advantage is different from recognizing it, being perceived to have it or using it.

**Epistemic trust** is directed, contextual and multidimensional. Competence, sincerity, impartiality, accuracy, completeness and transparency may vary independently. A person may trust a colleague's technical ability but not their account of a political dispute. Trust is not transitive, and reputation in one community need not apply to another. Reputation may itself depend on common-source repetition or strategic influence.

Political configurations emerge from formal and informal relationships: alliances, coalitions, rivalries, interests, institutional loyalties, intermediation, dependence and competing definitions of success. Convergence of behavior need not indicate coordination. Cooperation need not imply shared ultimate interests. An outcome that benefits a person does not establish that they caused it.

Authorities, policies, mandates and obligations are also investigable. Distinguish formal authority from authorization, delegation, practical influence, responsibility and accountability. Distinguish a stated rule from its validity, actual observance, interpretation, exceptions and private use. Distinguish a decision from its execution and consequences; a goal from an intention or plan; a cause from a contributing factor, condition or coincidental sequence.

Niccolò can reason about how a decision was shaped, not merely who signed it.

## 7. Pervasive deception — the foundational design principle

Deception is not an isolated module activated when somebody is suspected of lying. It is a possibility that must remain open throughout the model, at every layer of information, communication, knowledge, attribution, influence and investigation.

A false utterance is not necessarily a lie. A misleading conclusion can be constructed from true statements. A person may be sincere, accurately quote another, and still transmit misinformation. A withholding of information can be motivated by survival, fear of retaliation, confidentiality, embarrassment, convenience, economic interest or a calculated strategy. It can also be unintentional or entirely justified.

Consider the junior employee who omits that the COO convened a meeting in which disastrous decisions were made. The omission may reflect fear of dismissal; the employee's account may otherwise be accurate. Neither the omission nor the fear alone establishes guilt, intent or a moral judgment.

And consider ego. A momentary exaggeration of expertise to avoid embarrassment, a refusal to admit that a prediction failed, or a manager's reorganization of facts to preserve their self-image need not be motivated by money or hierarchy. Self-deception, rationalization, projected image, desired image and deliberate deception of others are distinct possibilities, not diagnoses that can be asserted without grounds.

The architecture treats deception as a pervasive, contestable composition of human and informational circumstances. Six dimensions illustrate its reach:

1. **Composition and representation:** selection, omission, ordering, emphasis, aggregation, decontextualization and associations can change the meaning of truthful fragments. Synthetic or manipulated artifacts can be authentic records of a deception while containing false information.
2. **Communication and transmission:** the originating actor, intermediary, recipient, knowledge of falsity, stated purpose, actual purpose and interpretations may diverge. Intentional deception can be retransmitted sincerely.
3. **Epistemic states and action:** a person may deceive about what they know, believe, understand, value, investigate or deliberately disregard. They may also be mistaken about themselves.
4. **Relationships and strategy:** declarations or appearances of trust, loyalty, independence, authority, affiliation, intention, capability and influence may be false, incomplete or strategically ambiguous.
5. **Investigation and institutional processes:** questions, selection of sources, standards, interpretations, reports, closure decisions and oversight can be manipulated or simply erroneous. The investigator is not outside the model.
6. **Higher-order deception:** deception about deception, false accusations, apparent detection, mistaken attribution, fabricated independence and misleading impressions of what another person knows or suspects.

Distinguish the suspected deceptive intention, attempt, means, target, intended effect, actual effect and perceived success. An attempt can fail. An unintended communication can have a misleading effect. An allegation of deception can itself be erroneous or deceptive. No actor must be treated as uniformly honest or dishonest.

When deception is discovered, the same dependency-aware machinery used for other compromised information can identify what needs reassessment. Crucially, the system does not merely flag a liar. It traces the informational consequences: who heard what, who transformed it, what claims or decisions depended on it, and which alternative grounds remain.

## 8. The deeper structure of human agency

Niccolò's model of strategic relations is deliberately general. These concepts are relevant to businesses, public agencies, criminal investigations, litigation, personal relationships and any other situation in which humans have competing interests and uneven freedom of action.

- **Vulnerability:** contextual exposure to adverse consequences — financial, professional, emotional, reputational, institutional, legal or informational. It may be unknown, perceived, temporary or dependent on circumstances. It is not necessarily an intrinsic weakness or evidence of misconduct.
- **Dependency:** attainment of a goal or preservation of a condition depends on another actor, resource or circumstance. Dependency may be mutual, asymmetric, voluntary or unwanted. Actual alternatives and perceived alternatives may differ.
- **Leverage:** capacity for influence or advantage arising from information, competence, authority, resources, dependence or vulnerability. Possessing leverage does not prove intent to exercise it, actual use or success.
- **Threat:** an adverse possibility or communicated threat. Distinguish capability, intention, conditionality, credibility, perception and actual consequences. A perceived threat can have real effects even if the threat is not credible.
- **Coercion:** influence compromising actual or perceived freedom of decision. Compliance, submission, resistance, partial compliance and seeking protection are distinct; submission is not consent or loyalty.
- **Incentive:** a condition encouraging or discouraging behavior, including recognition, belonging, ideology, ego, material benefit or advancement. Its existence is distinct from its perception and behavioral effect.
- **Pressure:** hierarchical, social, family, institutional, economic or self-imposed pressures may be cumulative, intermittent, incidental or deliberate. Pressure does not necessarily rise to coercion.
- **Constraint:** a real or perceived limitation on alternatives, whether imposed, material, legal, temporal, informational, moral or voluntarily accepted.
- **Autonomy:** the actual and perceived ability to decide, act, refuse, disagree, obtain knowledge, reconsider commitments or exit a dependency. Autonomy is contextual, not an all-or-nothing characteristic.

These relations can reinforce one another over time. An official who becomes compromised under pressure may face increasing dependence. An indispensable expert may have no formal authority yet effectively determine an institution's options. A subordinate may seemingly cooperate with a superior while privately opposing them. Similar conduct can emerge from radically different circumstances.

Niccolò investigates both the behavior and the alternative explanations for it. Causation, awareness, motivation, intent, responsibility and benefit are distinct dimensions.

## 9. Counterintelligence: reasoning against an adaptive adversary

Counterintelligence is a major application of the same epistemological core, not an incompatible second engine. It stresses the model because an adversary may be knowledgeable, strategically adaptive, institutionally powerful, capable of influencing investigators and aware of the processes used to discover them.

### 9.1 Informational exposure and compromise

What information exists? Who is authorized to access it? Who technically can access it? Who actually gained access, what did they perceive or understand, and who else may know about that access? These are independent questions.

The object exposed need not be a document. It may be the existence of an inquiry, a source's identity, an investigator's suspicion, an unanswered question, a relationship, a proposed recommendation or the fact that a specific person has been excluded from an information compartment.

Niccolò's target capability is to reason about suspected compromise, its extent and timing, affected material, paths of transmission, potentially independent grounds and introduced vulnerabilities. A detected incident does not automatically reveal every affected actor, route, document or consequence. Nor does suspicion alone establish an incident.

### 9.2 Leaks, origin, propagation and return

The creator of a piece of information, the first known discloser, an intermediary, the source of a particular report and the ultimate beneficiary may all be different. Attribution is itself an investigable hypothesis. Preserve alternative genealogies and time constraints; distinguish documented, possible, inferred and claimed paths.

Information may return to the person who originally generated it. That return could be evidence that a desired message has propagated, an indication of a possible breach of expected confidentiality, or a source of false corroboration when the originator does not recognize the information. It may also reveal changed interpretation or previously unknown relationships. The same observable event may be success for one actor and a compromise for another.

### 9.3 Infiltration and hidden loyalties

Formal affiliation and actual loyalty are independent. Loyalty can concern another person, family, an institution, a cause, an ideology or a community. It may be affective, professional, economic, grateful, ideological, reciprocal, conditional or ambivalent. Several loyalties may coexist until a decision forces a conflict. They may change through resentment, humiliation, gratitude, recognition, coercion, belonging or new conviction.

Obedience, agreement, compliance, dependence, cooperation, trust and loyalty are not synonyms. A person may obey a leader they despise, criticize an institution to which they are profoundly loyal, or have competence that others trust while concealing a conflicting commitment.

Niccolò can represent suspected hidden ties, dual affiliations, changed allegiance, simulated loyalty, external pressure, access and actual conduct independently; distinguish a preexisting intention from a relationship that developed later; and examine different actors' beliefs about those relationships. Infiltration may itself be authorized, as with an undercover investigator, so apparent membership, actual mandate, affiliation and purpose cannot be collapsed into one label.

It matters who is actually coordinating, who shares an interest without coordination, who is unwittingly assisting someone else and who is being falsely accused. Dissent, friendship, kinship, criticism and shared ideology do not establish operational collaboration. Absence of suspicious conduct cannot be endlessly reinterpreted as proof of an especially competent infiltrator. Such a self-sealing hypothesis must be challenged.

This is sophistication about the possibility of betrayal — not a mechanism for turning suspicion into guilt.

### 9.4 Deception directed at counterintelligence

An adversary may influence the investigator's belief about whether a compromise exists, its origin, timing, severity or scope. An apparent success may be false reassurance; an apparently credible accusation may divert attention toward an uninvolved person. An investigation can be compromised through its agenda, baselines, collection scope or standards of interpretation without the falsification of a single source.

Niccolò distinguishes actual adversary capabilities from attributed capabilities, demonstrated knowledge from suspected knowledge, and centralized coordination from emergent behavior. The same observations can warrant alternative explanations: a hostile actor, an honest error, institutional inertia, resource limits or a legitimate change in priorities.

Overreaction is itself a risk. An institution can damage its own functioning by responding to an inaccurately attributed threat. Recommendations to preserve information, review access or introduce independent review may remain appropriate even if the original suspicion is later discarded.

### 9.5 Compartmentalization and distributed knowledge

Different investigators can possess fragments whose combination yields a hypothesis none could formulate alone. Compartmentalization can protect sources and operations, but can also obscure contradictions, create duplicate work, conceal source dependencies or prevent effective revision.

The epistemological model distinguishes authorization to know, technical possibility of access, actual access, assimilation, retained knowledge, access to an object's existence and access to its contents or provenance. Revocation does not erase information already acquired. The existence, origin and rationale of a restriction can itself be investigable.

Actual Niccolò permissions are deterministic security controls, separate from the authority relationships represented in an investigation. Protection includes derived content, metadata, association graphs, summaries, outputs and review signals. A conclusion formed from authorized fragments must not be represented as a conclusion from the totality of material, and protected material must not be exposed through a supposedly innocuous inference. Where a permitted review route does not exist, the system must respect the limitation.

### 9.6 Vulnerabilities, protection and recovery

Vulnerabilities may arise from combinations of individually ordinary circumstances: a technically essential person with excessive unreviewed privileges, a dependent oversight body, a single source with no alternative source, a supervisor able to close all inquiry, or a recovery process controlled by the same actor whose actions are under review.

Model vulnerability, exposure, awareness by others, incident and actual consequence separately. A vulnerability may exist without any hostile actor. Examine excessive concentration of authority, access and dependency alongside resilience, alternatives, reviewability and recovery.

The target includes protective and containment recommendations: preserving records, independent verification, reviewing access and responsibilities, protecting sources, limiting unnecessary dependencies and reassessing affected knowledge. Containment and recovery must address the integrity of information and reasoning, not only technical assets. A restored system can still contain conclusions derived from compromised material.

## 10. Who investigates the investigators?

Investigation integrity is a first-class cross-cutting capability. It covers the reliability and completeness of collected material, the questions and alternatives considered, assumptions, selection of sources, quality of reasoning, dependence on individuals or compromised material, resource limitations and the provenance of decisions.

The same abstractions that apply to any investigation apply to its investigators, their superiors, their policies, their operational choices and the investigators who review them. Counterintelligence adds the particular stress of an adversary who may understand the review techniques or occupy a position of power, but the underlying epistemology remains the same.

```text
       Investigated situation
                ^
                | investigates
          Investigation A
                ^
                | examines methods, grounds, decisions
       Oversight investigation B
                ^
                | reviews oversight integrity
       Independent review C

 Each layer can produce findings, dissent, decisions and
 new questions concerning another layer. Scope and access
 remain explicit; no mandatory infinite investigation.
```

A police internal affairs body, for example, may possess disciplinary, career, hierarchical and informational power. A corrupt official can privately appropriate legitimate investigative information. Knowledge of alleged misconduct may give that official real or perceived leverage over another officer. Coerced behavior may differ significantly from freely chosen participation, while subsequent wrongdoing can deepen the dependency. Knowing what an officer did does not explain every relationship in which that information subsequently matters.

Formal independence does not establish practical independence. Review may be selective through legitimate priorities, budget constraints, error, favoritism, political influence or capture. Compare the circumstances and reasons for treating superficially similar cases differently. An honest reviewer can reason from compromised material; misconduct by one member does not prove that every case or every colleague is compromised.

The integrity of oversight is particularly important because the same institutional mechanisms intended to discover misconduct can become instruments of concealment, coercion or selective enforcement. An allegation, its handling, its referral or its closure is itself investigable. The absence of inquiry is not evidence that the allegation was false.

Niccolò is designed to expose coverage and dependency problems, hidden premises, self-confirming inquiry, missed alternatives, suppressed dissent, unsupported closure and gaps in actual autonomy to disagree. It preserves original material, contested assessments and decision genealogy for authorized review. The limitations of a computational process are not discoveries about the world: a resource-limited search returning no result does not prove absence.

**Example of the interaction:** An oversight reviewer receives three reports alleging an officer's misconduct. Two reports descend from the same interview; the third is a genuinely independent observation. A later discovery compromises the interview's accuracy. The reviewer must not count its two descendants as independent corroboration, nor automatically discard the independent observation. A recommendation for independent review of the recording procedure may stand on separate grounds. The original decision to close the case, the evidence then available, and any rejected investigative alternatives remain inspectable. This compact hypothetical combines provenance, deception, selective revision, responsibility, investigative integrity and recommendation in one operation.

## 11. From analysis to action recommendation

The model is not confined to describing the past. An **ActionRecommendation** is a grounded, reviewable proposal to an authorized human decision maker. It is distinct from a decision, authorization, implementation or prediction of success.

A recommendation can propose obtaining information, seeking an independent examination, reviewing privileges, preserving evidence, protecting an individual, reducing a dangerous dependency, reviewing a prior decision or redirecting an investigation. It may be relevant because of a structural risk rather than a concluded accusation. Niccolò can compare alternatives, including no action, and describe conditional outcomes, uncertainty, introduced risks and what new information would justify changing the proposal.

Imagine an organization with a brilliant programmer whose unrestricted administrative privileges have become a concentrated institutional risk. The recommendation may be to review and narrow privileges while retaining the person's technical contribution. That recommendation need not assert that the person committed misconduct. Similarly, in a criminal investigation, an independent forensic examination of lawfully obtained material may be recommended to discriminate between competing claims about its composition, rather than accepting a participant's assertion as established fact.

Recommendations preserve their grounds and conditions. A change in the credibility of an incident report need not invalidate a recommendation whose justification is independently supported. An intervention's implementation does not retroactively establish that the suspicion that prompted it was correct.

Niccolò proposes; authorized humans decide and act. Recommendation capability does not include recommending or planning kinetic attacks. This is a product boundary, not a limitation on representing and analyzing information about real-world events and their consequences.

## 12. Semantic discipline instead of false precision

The model is intentionally rich without requiring elaborate mathematics merely to appear sophisticated. Many epistemic degrees are initially qualitative, with domain-specific ordinal values. Illustrative examples include relevance (`negligible`, `low`, `moderate`, `high`, `critical`), confidence (`very_low` through `very_high`) and expertise (`novice`, `basic`, `intermediate`, `advanced`, `expert`). Interest, dependence, influence and other concepts have their own appropriate semantics.

Unknown, none and not applicable mean different things. Ordinal values are not automatically equal-interval measurements, probabilities or additive scores. A confidence level in an assessment of loyalty is not the degree of that loyalty; the degree of loyalty in one relationship need not be comparable to a superficially similar degree elsewhere. Provenance, assessment rationale and historical context remain available for later refinement or empirical calibration.

The architecture also demands qualified semantics for relationships. Provenance is not support; support is not universally transitive; logical contradiction depends on matching scope and interpretation; causality is not mere temporal sequence; institutional authority does not confer truth; and an inferred equivalence does not authorize an irreversible identity merge. Several independently assessable relations can connect the same objects, and a relation may itself be the subject of a further relation or assessment.

Reasoning cycles can reflect real-world feedback, but recursive corroboration without external grounding cannot increase knowledge. The ability to represent recursion does not require uncontrolled computation. Analysis follows relevance, purpose, authorization and the explicit budget.

## 13. Where the epistemology becomes a product

The capabilities below are not separate, disconnected products. They are different applications of the same core model. The examples illustrate the intended scope rather than claim demonstrated operational outcomes.

### Police internal affairs and independent oversight

Examine suspected misconduct, evidence substitution or disappearance, selective investigation, coercive relationships, protection networks and potential institutional capture. Reconstruct who had access to information, who could influence decisions, what was documented, who made the closure decision and what alternatives remained unexamined. Review the integrity of the original investigators and, where appropriate and authorized, the integrity of the oversight process itself.

A reviewer can ask not just whether a file contains evidence of misconduct, but whether its collection and reasoning made it capable of discovering misconduct in the first place. The epistemology is relevant to police internal affairs, inspectorates, independent review bodies and other accountability institutions.

### Criminal investigations and intelligence analysis

Reconstruct timelines, actor relationships, alternative explanations, testimonial inconsistencies, source dependencies and possible deception. Distinguish the original alleged event from subsequent claims and knowledge about the event. Represent possible competing loyalties and conflicting mandates without conflating association with guilt. Identify information that could actually distinguish rival explanations, rather than merely seeking more material.

Counterintelligence and protected investigations make this especially valuable: distinguish genuine insight from planted or recycled information, investigate suspected compromise and assess the integrity of the investigative process under pressure from adaptive actors.

### Corporate counterintelligence and insider-risk investigations

Investigate suspicious information flows, disputed origins of disclosures, hidden conflicts of interest, inappropriate privileges, misleading explanations and organizational dependencies. Evaluate competing explanations for anomalous conduct instead of equating anomalies with disloyalty. Preserve the genealogy of analytical conclusions when new evidence challenges an earlier source.

A security report may be technically correct but misleading in its interpretation; an analyst may be honest yet depend on a manipulated source; an access restriction may address structural risk without proving misconduct. Niccolò's informational and human models are designed to make these distinctions actionable.

### Law firms, litigation and dispute resolution

Develop alternative case theories, map what testimony establishes versus what it alleges, discover circular corroboration, challenge the scope of assertions and trace conclusions to primary material. Model who knew what at the relevant time rather than letting later discoveries contaminate earlier intentions. Analyze credibility dimensions, disputed interpretations, conflicting institutional obligations and missing discriminators.

A legal team can examine competing interpretations without erasing their provenance, track dissent and revisit material when a witness, document or assumption is undermined. The fictional [Aurelius & Finch bonus review](USE_CASE_LAW_FIRM_BONUS_REVIEW.md) illustrates the richness of an adversarial human-context investigation.

### Corporate investigations, fraud, audits and governance

Follow decisions through their actual informational basis and organizational influence. Investigate who convened a meeting, who had access to a material risk, who withheld information, which accounts originated independently, which decision records were changed, and why a conclusion was adopted. Model the potential influence of fear of retaliation, incentives, loyalty, reputation and ego without converting inferred motives into facts.

Boards, auditors and independent investigators can scrutinize the inquiry itself: its collection scope, exceptions, contested conclusions, rejected alternatives and reasons for closure. In an investigation of a whistleblower's allegation, the handling of the allegation is also an investigable event.

### Strategic decisions, due diligence and negotiation preparation

Represent stated and inferred interests, dependencies, practical leverage, competing commitments, authority, actual alternatives, informational asymmetries and the evidence for each. Understand what the parties may know, what they may mistakenly believe and what remains uncertain. Evaluate scenarios under explicitly adopted premises and compare recommendations without assuming that a preferred scenario describes actual reality.

The ability to distinguish stated intent from inferred intent and actual behavior matters whether the subject is an acquisition, a contractual dispute, a strategic partnership or an internal reorganization.

### Sensitive and isolated deployments

The model has clear relevance to organizations that cannot expose investigations or source material to externally hosted services. An isolated or air-gapped deployment is an important product direction for sensitive contexts; the actual deployment, integrations, model availability and isolation controls must be verified for the specific environment. The epistemological model does not assume that a cloud provider possesses the organization's information.

## 14. One model, six interacting functional slices

The architecture can be implemented and reviewed through coherent slices without weakening its conceptual scope:

- **A — Investigable universe:** actors, identity, events, artifacts, claims, relationships and provenance.
- **B — Directed investigation:** gaps, questions, responses, hypotheses, relevance, answerability and discriminatory information.
- **C — Reasoning and review:** interpretations, premises, arguments, inferences, assessments, findings and their dependencies.
- **D — Human and strategic dynamics:** epistemic relationships, dispositions, influence, trust, loyalty, power, dependence, coercion and pervasive deception.
- **E — Proactive advice:** scenarios, vulnerabilities, recommendations, plans, risks and conditional consequences.
- **F — Integrity and governance:** permissions, human decisions, dissent, coverage, compartmentalization, genealogy and review of investigations.

These slices are not sequential stages. Human dynamics can suggest a new investigative question; an inference can expose a vulnerability; a recommendation can reveal a previously unexamined assumption; integrity review can require new information or force revision of a finding. A provides the substrate, B and D shape inquiry, C builds and revises knowledge, E offers actionable proposals, and F constrains and enriches all of them.

The model's value comes from these combinations. Counterintelligence is not merely a graph of relationships; nor is investigation integrity merely an audit log. They emerge from the intersection of explicit epistemic grounding, human agency, recursive review, qualified relationships and the possibility of deception.

## 15. The MVP commitment

The approved MVP scope is the **complete epistemological model**, not a rudimentary model surrounded by promises of later intelligence. Its implementation may be incremental, but the required distinctions must support meaningful behavior rather than exist only as vocabulary. Sophisticated algorithms are warranted wherever the reasoning actually requires them: provenance and independent origins, selective revision, meaningful alternatives, discriminatory information, layered deception, strategic relationships and investigative integrity.

Initial simplifications are directed toward implementation machinery and unnecessary mathematical complexity, not the essence of the product. Complex probability calibration, autonomous resource optimization and formal strategic utility models are not requirements for their own sake. Qualitative scales can be refined later without erasing historical meaning.

The investigator controls investigative priorities and resource allocation. Niccolò may expose resource usage, identify underexamined lines, explain its proposed priorities and suggest a different allocation; it must not silently redirect the user's budget or decide that an investigation is unworthy of attention. Real permissions are enforced independently of any model's opinion. Information supplied about images and video can be investigated; autonomous image and video understanding is outside the initial scope.

The conceptual architecture has undergone a first round of 25 adversarial desk cases, including false source independence, partial contamination, hindsight, deceptive truth, mistaken identity, compartmentation, self-generated evidence, selection bias, ambiguous contradiction, authority, circularity, counterfactual contamination, incomplete hypothesis sets, attributed and collective knowledge, self-sealing explanations, earlier AI outputs, conditional recommendations, silence, conflicting obligations, confidential revision, processing limitations, hidden premises, invalid support propagation and combined multi-layer compromise. These cases are architectural probes, not evidence of implemented coverage or measured accuracy.

The next engineering obligation is to reconcile the approved model with the existing implementation, determine its concrete delta, and derive executable regression and contract tests. See the [epistemic architecture review](../architecture/EPISTEMIC_ARCHITECTURE_REVAMP_IN_PROGRESS.md) for the detailed conceptual decisions, relationship semantics, validation cases and technical reconciliation work. See the [repository README](../../README.md) for verified implementation status.

Meaningful validation should evaluate behavior across the interactions, not merely the presence of abstractions in a schema. Representative acceptance targets include preserving independent sources after consolidation, rejecting circular corroboration, keeping hypothetical scenarios separate from actual-world evidence, displaying both supporting and opposing grounds, recognizing hidden presuppositions, and reassessing only affected dependencies. Recommendations should expose their conditions and survive the loss of a suspicion when independently justified. Resource-limited results and compartment-limited views must not be represented as comprehensive knowledge. These are testable targets for the upcoming implementation reconciliation, not reported benchmark results.

## 16. The point

For Niccolò, information about an investigation is information. Information about the integrity of its investigators is information. The way a source came to believe an assertion, the influence behind a decision, the false impression created by a true statement, the fear that explains an omission, the provenance of a conclusion, the fact that an investigation has been closed without resolving its central question — all belong to the same universe.

The aim is not omniscience. It is to make human reasoning in difficult, adversarial circumstances more explicit, discriminating, revisable and accountable — to investigate reality without surrendering to the authority, reputation, volume or confidence with which a version of it is presented.

**Insolence to authority. Humility to evidence.**

## Appendix. Selected research foundations

Niccolò's conceptual design draws on an interdisciplinary examination of epistemology, human behavior, strategic communication, argumentation, evidence and professional investigations. The following works are a selected reading path from the [66-source annotated interdisciplinary review](../architecture/EPISTEMIC_DOMAIN_INTERDISCIPLINARY_REVIEW_V1.md), which records inspection levels, competing findings, limitations and design implications. These references inform distinctions in the design; they do not establish the validity, accuracy or operational maturity of Niccolò itself, nor imply that it implements any particular published formalism.

### Knowledge, testimony and human relationships

- Sperber et al. (2010), [*Epistemic Vigilance*](https://doi.org/10.1111/j.1468-0017.2010.01394.x). Evaluation of information and communicators.
- Fricker (2007), [*Epistemic Injustice*](https://academic.oup.com/book/32817). Credibility attribution, power and interpretation.
- Perner & Wimmer (1985), [*John Thinks That Mary Thinks That…*](https://www.sciencedirect.com/science/article/pii/0022096585900517). Second-order belief representation.
- Wellman & Liu (2004), [*Scaling of Theory-of-Mind Tasks*](https://pubmed.ncbi.nlm.nih.gov/15056204/). Distinctions involving knowledge access, desires and beliefs.

### Deception, strategic communication and human judgment

- Crawford & Sobel (1982), [*Strategic Information Transmission*](https://www.jstor.org/stable/1913390). Communication under divergent preferences.
- Kuran (1995), [*Private Truths, Public Lies*](https://sites.duke.edu/timurkuran/research/books/private-truths-public-lies-the-social-consequences-of-preference-falsification/). Private preferences and publicly expressed positions.
- DePaulo et al. (2003), [*Cues to Deception*](https://pubmed.ncbi.nlm.nih.gov/12555795/), and Bond & DePaulo (2006), [*Accuracy of Deception Judgments*](https://pubmed.ncbi.nlm.nih.gov/16859438/). Evidence and limitations of deception judgments.
- Tversky & Kahneman (1974), [*Judgment Under Uncertainty*](https://doi.org/10.1126/science.185.4157.1124), and Gigerenzer & Gaissmaier (2011), [*Heuristic Decision Making*](https://doi.org/10.1146/annurev-psych-120709-145346). Complementary perspectives on judgment and heuristics.

### Reasoning, provenance and institutional authority

- Prakken (2010), [*An Abstract Framework for Argumentation with Structured Arguments*](https://journals.sagepub.com/doi/10.1080/19462160903564592). Distinct targets of argumentative challenges.
- Alchourrón, Gärdenfors & Makinson (1985), [*On the Logic of Theory Change*](https://doi.org/10.2307/2274239). Belief revision and contraction.
- de Kleer (1986), [*An Assumption-Based TMS*](https://doi.org/10.1016/0004-3702(86)90080-9). Assumption contexts and dependencies.
- W3C (2013), [*PROV-O: The PROV Ontology*](https://www.w3.org/TR/prov-o/). Provenance of entities, activities and agents.
- Jones & Sergot (1996), [*A Formal Characterisation of Institutionalised Power*](https://doi.org/10.1093/jigpal/4.3.427). Institutional power and authorization.

### Professional investigation and investigative integrity

- Heuer (1999), [*Psychology of Intelligence Analysis*](https://www.cia.gov/resources/csi/books-monographs/psychology-of-intelligence-analysis-2/). Competing hypotheses and analytic bias.
- National Academies (2014), [*Identifying the Culprit: Assessing Eyewitness Identification*](https://www.nationalacademies.org/read/18891/chapter/2). Witness memory and investigative procedures.
- Cooper & Meterko (2019), [*Cognitive Bias Research in Forensic Science: A Systematic Review*](https://doi.org/10.1016/j.forsciint.2019.01.016). Bias, prior exposure and forensic practice.
- Rubin (1976), [*Inference and Missing Data*](https://doi.org/10.1093/biomet/63.3.581). Missing information and the mechanism producing its absence.
