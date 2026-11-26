# Niccolò MVP next execution plan

**Status:** Active execution plan.
**Product name:** **Niccolò**. Naming alternatives are outside this plan and must not be introduced into product code, UI, documentation, commits, fixtures, or examples while this plan is active.
**Purpose:** Turn the existing epistemological, persistence, authorization, reasoning, and artifact foundations into one coherent investigator-facing workflow that can be exercised by a real user.

This document is a TODO-like execution sequence, not a claim that the listed behavior is already implemented. Source code, migrations, executable tests, and a completed first-user workflow remain the evidence of implementation.

For execution sequencing, this plan records the latest owner decisions and therefore supersedes older planning statements that place all interpretation of investigator prose, external model use, or model-assisted structure proposals after the MVP. Block B must reconcile those older documents rather than leaving permanent contradictory specifications.

## 1. Governing product decisions

The following decisions constrain all work below:

- The MVP has **no artifact parser** for WhatsApp, email, PDF, image, video, audio, or other artifact formats.
- An artifact may remain outside Niccolò. The user records its identity, location, accessibility, and explicitly attributed descriptions, transcriptions, summaries, or other representations.
- Niccolò does not need its own store for original artifact bytes in this MVP.
- Redaction/anonymization assistance is not an MVP feature. Ordinary application security, authorization, secret handling, and workspace isolation remain required.
- A reasoning model may be local, air-gapped, LAN-hosted, or remote. Air-gapped deployment is an option, not a universal assumption.
- OpenAI-compatible endpoints are the initial provider boundary. Operator configuration is sufficient for the first MVP; a polished end-user provider-management console is not required first.
- LLM assistance is optional but first-class. A trained investigator must be able to author and edit the epistemic structure manually.
- Ordinary investigator prose may be converted into **proposed** epistemic objects and relations during the MVP. This supersedes earlier planning text that placed all investigator prose interpretation after the MVP.
- Model output does not become canonical merely because it is syntactically valid. Provider/model provenance, proposal state, human review, edits, rejection, and promotion history remain inspectable.
- Niccolò may generate investigative questions. Question generation occurs on request by default; explicitly configured automatic behavior may be added without silently enabling it for every Situation.
- Consequential hypotheses, conclusions, decisions, and actions remain attributable and revisable. Human authority is not delegated to a model.
- Niccolò remains the sole current product name throughout implementation.

## 2. Primary MVP acceptance journey

The critical path is:

```text
investigator statement or manual authoring
        |
        v
proposed typed objects and relations
        |
        v
inspectable graph/draft
        |
        v
accept / edit / reject / add manually
        |
        v
canonical, provenance-preserving investigation model
        |
        +--> investigator-generated what-if branch
        |          |
        |          v
        |    downstream impact and surviving support
        |
        +--> investigative questions and information gaps
        |
        v
fluent, editable, traceable prose
```

The MVP is not ready merely because each box exists separately. A real user must be able to complete this journey without learning the internal architecture first.

## 3. Ordered execution blocks

### Block A — close the parser-free boundary

- [ ] Finish the in-progress Situation UI transition to external artifact references and attributed representations.
- [ ] Remove obsolete source-upload, parser, catalog, PDF-preview, and identity-review assumptions from primary frontend tests.
- [ ] Ensure the default shipped MVP API does not expose artifact parsing or PDF text-extraction workflows. Preserve historical schema compatibility without presenting legacy parser behavior as MVP capability.
- [ ] Remove or isolate unused browser client methods and UI modules that imply supported parsing.
- [ ] Ensure frozen reasoning input uses explicit artifact representations and preserves their attribution, scope, limitations, and non-truth semantics.
- [ ] Rewrite the public README and first-user walkthrough so they contain no instructions to upload or parse WhatsApp, email, PDF, image, video, or audio artifacts.
- [ ] Validate frontend tests/build, Rust formatting/lints/tests, PostgreSQL migrations/integration tests, and the local bootstrap path.

**Exit evidence:** a first user can reference an inaccessible external artifact, describe or transcribe relevant material, analyze it, and later see exactly which attributed representation entered reasoning. No default MVP surface offers parsing.

### Block B — reconcile normative documentation with current owner decisions

- [ ] Update architecture and product documents that still declare investigator prose interpretation post-MVP.
- [ ] Record the optional-local-or-remote LLM boundary and the initial OpenAI-compatible endpoint contract.
- [ ] Record that manual graph authoring and LLM-assisted proposals are two entry paths into the same epistemic model.
- [ ] Record proposal provenance requirements: provider, model, invocation, source text/input revision, output contract version, timestamps, and human disposition history.
- [ ] Record opt-in automation scopes: per action, per Situation, or broader user configuration; default behavior remains non-automatic.
- [ ] Move redaction assistance and artifact parsing explicitly to post-MVP scope.
- [ ] Reconcile any conflicting README, walkthrough, architecture, planning, and product statements.

**Exit evidence:** a new engineer can determine the intended MVP behavior without relying on conversation history or encountering mutually contradictory scope documents.

### Block C — prose-to-graph proposal slice

- [ ] Add an investigator-facing ordinary-prose input for proposing epistemic structure.
- [ ] Preserve the exact original user text independently from every interpretation or normalized representation.
- [ ] Define a bounded structured-output contract for proposed objects, relations, grounding, unresolved references, ambiguity, and clarification needs.
- [ ] Reuse or generalize the reasoning proposal ledger rather than creating an untracked model-output path.
- [ ] Require each proposed object and relation to explain the source phrase/context that motivated it.
- [ ] Prevent ungrounded enrichment: the proposal path must not silently invent motive, deception, knowledge, comprehension, loyalty, causation, or identity.
- [ ] Add adversarial tests for over-interpretation, missing provenance, unsupported certainty, unknown references, duplicate proposals, and provider retries.

**Exit evidence:** a sentence such as “Apparently, the victim asked the aggressors for a ride shortly before the crime” produces a bounded, attributable, visibly provisional draft instead of an opaque answer or automatically accepted graph.

### Block D — human graph approval and manual authoring

- [ ] Present proposed objects and relations visually as inspectable nodes, edges, labels, qualifications, and provenance.
- [ ] Support accept, edit, reject, and bounded bulk acceptance while recording the exact human action.
- [ ] Support manual creation of the same object and relation types without requiring an LLM.
- [ ] Make unconfirmed, accepted, revised, rejected, superseded, and retracted states visually distinguishable.
- [ ] Preserve the distinction between confirming intended meaning and asserting truth.
- [ ] Ensure bulk acceptance does not fabricate evidence that every proposal was carefully read or cognitively understood.
- [ ] Keep advanced ontology detail progressively disclosed; internal representational complexity must not become mandatory form-filling complexity.

**Exit evidence:** an expert investigator can construct and revise a useful graph manually, while another user can start from model proposals and retain full control over what enters the canonical model.

### Block E — what-if dependency impact

- [ ] Let the investigator create a branch in which a selected claim, assumption, relation, source representation, or interpretation is treated as false, unsupported, withdrawn, or differently interpreted.
- [ ] Keep the branch separate from canonical history unless the investigator explicitly promotes a revision.
- [ ] Traverse explicit grounding, dependency, genealogy, and qualification structures without confusing provenance ancestry with epistemic support.
- [ ] Classify downstream effects at minimum as: collapses, weakens, requires review, remains supported, or currently unknown.
- [ ] Expose evidence and reasoning that survive the changed premise.
- [ ] Surface newly material information gaps and discriminating questions.
- [ ] Test cycles, joint support, alternative support routes, partial grounding, historical revisions, and changes that should have no downstream effect.

**Exit evidence:** changing one consequential premise produces an intelligible and reproducible account of what changes, what does not, and why—without rewriting the original investigation.

### Block F — investigator questions

- [ ] Generate a bounded number of questions on explicit request.
- [ ] Explain which hypotheses, assumptions, gaps, or decisions each question could discriminate.
- [ ] Preserve model provenance and distinguish generated questions from investigator-authored questions.
- [ ] Let the investigator accept, rewrite, prioritize, defer, or reject each question.
- [ ] Support explicitly configured automatic question generation after relevant input or revision without making it the universal default.
- [ ] Protect human attention against low-value question floods and weaponized formalism.

**Exit evidence:** generated questions are useful because their possible answers could change live reasoning, not merely because they sound investigative.

### Block G — graph-to-prose and report workflow

- [ ] Add “describe what this graph currently expresses” as an explicit investigator action.
- [ ] Generate fluent prose from the accepted graph, current branch, recorded uncertainty, competing accounts, and provenance.
- [ ] Prevent generated prose from silently adding claims absent from the graph.
- [ ] Make consequential sentences traceable to their supporting objects and relations.
- [ ] Preserve distinctions such as alleged/observed/inferred/hypothetical, corroborated/repeated, and unknown/unsupported/false.
- [ ] Let the investigator edit, regenerate bounded sections, and export the result.
- [ ] Preserve which graph revision and scenario branch produced each document.

**Exit evidence:** an investigator can produce a useful, editable document for an attorney, law-enforcement professional, executive, client, or other intended reader without losing epistemic qualification.

### Block H — canonical fictional-county investigation

- [ ] Define a fictional county in a fictional state with recurring institutions and actors.
- [ ] Create a bounded case in which an initially convincing account points toward the wrong person.
- [ ] Include conflicting testimony, one materially deceptive account, external artifact references, uncertain identity or timing, and evidence that remains useful after the central theory changes.
- [ ] Exercise model proposals, human correction, manual graph editing, what-if impact, generated questions, selective reanalysis, and final prose generation.
- [ ] Use the case as onboarding material, a product demonstration, a regression fixture, and an LLM evaluation scenario.
- [ ] Ensure police, prosecution, defense, witnesses, experts, victims, suspects, and Niccolò's own prior analysis can all be challenged.

**Exit evidence:** the canonical case can be replayed from clean setup and demonstrates the full primary acceptance journey with deterministic assertions around provenance, human decisions, revisions, and downstream impact.

### Block I — executable local pilot

- [ ] Revalidate bootstrap on a clean machine with PostgreSQL and the documented Rust/Node requirements.
- [ ] Run with one local or remote OpenAI-compatible endpoint using private operator configuration.
- [ ] Exercise the canonical case and one investigator-authored case through the browser.
- [ ] Record hesitation, vocabulary failures, accidental overclaiming, excessive form burden, and places where architecture leaks into the primary experience.
- [ ] Fix failures that prevent the investigator from completing the core journey before expanding surface area.

**Exit evidence:** a new user can configure, start, use, stop, and restart Niccolò locally while preserving the investigation and its reasoning history.

### Block J — MCP surface after internal semantics stabilize

- [ ] Define MCP tools over stable application capabilities rather than exposing database tables or unstable internal structures.
- [ ] Candidate tools include Situation creation, artifact-reference recording, structure proposal, graph inspection, what-if evaluation, question generation, and traceable prose generation.
- [ ] Preserve workspace authorization, provider/model provenance, human approval boundaries, idempotency, and operation budgets.
- [ ] Ensure MCP callers cannot bypass proposal review or convert tool output into human authorization.
- [ ] Keep the browser workflow complete; MCP is an additional surface, not a replacement for investigator usability.

**Exit evidence:** an authorized MCP client can perform the same bounded operations with the same provenance and human-control semantics as the first-party UI.

### Block K — multi-model comparison and routing

- [ ] Make all model-produced material provider/model attributable from the first proposal implementation.
- [ ] After one-provider correctness is demonstrated, compare Qwen and DeepSeek (or other configured models) on the canonical case.
- [ ] Preserve each model's independent output before synthesis, critique, validation, or human selection.
- [ ] Support role inversion and cross-review without presenting agreement as truth or disagreement as equal evidentiary weight.
- [ ] Defer elaborate automatic routing/topology optimization until reproducible evaluation justifies it.

**Exit evidence:** investigators can determine which model said what, compare performance over repeated work, and stop using models that do not justify their cost or attention.

## 4. Work deliberately deferred while the primary journey is incomplete

- Artifact parsing or autonomous interpretation of WhatsApp, email, PDF, image, video, or audio originals.
- Redaction/anonymization assistance.
- A polished provider marketplace or end-user credential-management console.
- Sophisticated capability-aware model routing.
- Large-scale ingestion optimized for hundreds of thousands of people.
- Enterprise procurement presentation, installer polish, or broad organizational administration that does not unblock the first investigator pilot.
- Product renaming. The implemented and documented product remains Niccolò.

Deferral does not prohibit small structural precautions that prevent expensive rework, especially provider/model provenance, stable application boundaries, workspace isolation, and revision identity.

## 5. Milestone and commit discipline

Each execution block may contain several milestones. For every milestone:

1. Start commit numbering again at `WIP 01`.
2. If the milestone remains in progress for approximately five minutes, create and immediately push the next useful `WIP nn: ...` checkpoint.
3. Push immediately after every commit.
4. The final commit for the milestone is `DONE: ...`.
5. If the latest WIP commit turns out to contain the completed milestone and no additional commit is useful, amend that latest message to `DONE: ...` and update the remote safely, using force-with-lease when history replacement is required.
6. Do not mix unrelated user changes into milestone commits.
7. Do not treat a successful commit as a reason to stop when the next in-scope milestone is clear.

## 6. MVP success definition

The MVP succeeds when an investigator can:

1. start a Situation;
2. enter ordinary prose or manually create structured epistemic material;
3. inspect, edit, accept, or reject model-proposed structure;
4. reference external artifacts without asking Niccolò to parse them;
5. construct and inspect the evolving reasoning graph;
6. change a material premise in an explicit what-if branch;
7. see what collapses, weakens, survives, or requires review;
8. request useful discriminating questions;
9. generate fluent, traceable prose from the approved graph;
10. revisit the provenance, human decisions, model contributions, and prior revisions that produced the current state.

Everything else is subordinate to demonstrating this journey honestly and usefully.
