# Niccolò MVP next execution plan

**Status:** Active. The first phase is API and reasoning validation.
**Product name:** **Niccolò**. Naming alternatives are outside this plan and must not be introduced into product code, UI, documentation, commits, fixtures, or examples while this plan is active.
**Purpose:** Verify the reasoning and evidence-handling workflow through the existing HTTP API and internal tests before resuming browser work.

Source code, migrations and executable tests establish implementation of the first phase. A completed browser workflow with an investigator is a separate acceptance milestone.

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

## 2. Active phase — API and reasoning only

React, the browser bundle, and frontend development are deferred for the
entire phase. Preserve the existing frontend code and tests in the repository
as a separate, opt-in path. Omit frontend tests, browser E2E, Node/npm, and
asset builds from default local and CI validation and from the API-only build and
local startup path. The API-only service must start without building,
embedding, or serving the React bundle. Document the commands for building,
starting, stopping, and validating it from a clean checkout with Rust and
PostgreSQL, with a configured model needed only for the live model evaluation.

Use the existing HTTP API for replay and internal tests for the reasoning
rules. Limit HTTP/BFF changes to what these paths require for correct input,
authorization, persistence, provenance, human review, and output. Keep
presentation-oriented BFF work with the browser phase.

### Acceptance journey

1. Create a Situation and attribute a description to an external artifact.
2. Author a typed graph manually, or submit prose and explicitly review the
   proposed objects and relations before adopting them.
3. Correct a source representation and evaluate a separate what-if. Inspect
   downstream effects, surviving support, and earlier revisions.
4. Request discriminating questions and traceable prose from reviewed material.

Replay this journey over authenticated HTTP. The replay supplies investigator
actions explicitly and checks the returned and persisted state. A later
investigator session will test whether the browser makes that same journey usable.

### Execution order and exit criteria

1. Establish an API-only build and verification path (A and I). It must
   run from a clean checkout without Node/npm or a browser build. The
   default reasoning checks must run without provider credentials.
2. Define the fictional county case with explicit expected results and turn
   it into an executable replay (H). Exercise the existing capabilities
   (C–G) and fix the gaps that prevent the replay. Reconcile conflicting
   documentation as those paths are changed (B).
3. Run two complementary checks on the case:
   - **Deterministic motor and HTTP replay.** Start with a reviewed graph.
     Record an attributed witness correction and confirm which specific
     claims lose or weaken support, which routes require review, and which
     independent support survives. Confirm that the prior state remains
     inspectable. Evaluate a separate what-if and verify that the
     canonical state is unchanged. Use controlled model responses where
     needed to test the proposal and report I/O contracts reproducibly.
   - **Live model evaluation.** Use Qwen via OpenRouter on the synthetic
     notes. Check its proposed structure, discriminating questions and
     revised prose against a rubric defined before the run. Record the
     model and concrete errors, including treating a report as fact,
     treating repeated testimony as independent corroboration, and
     overstating a conclusion after its support weakens. Record latency
     and available usage with the results. Compare DeepSeek later on the
     same fixture (K).
4. Resume browser work after the canonical replay passes from clean setup,
   the live model evaluation meets the case rubric, and the result shows
   what changed, what survived, and why. Use the existing interface for
   the first investigator session and prioritize fixes from that session.
   Revisit MCP after the corresponding application semantics are stable (J).

## 3. Ordered execution blocks

The status below is based on the shipped paths at the end of milestone 1.616.
Checked items indicate a bounded implementation with a corresponding code or
test path. The sequence in section 2 governs the active phase. The blocks
below track requirements and retain distinct exit evidence.

### Block A — close the parser-free boundary

The default runtime and first-user guide use external references and attributed
representations. The [parser-free validation gate](../../scripts/run-parser-free-mvp-validation.sh)
currently combines backend and frontend checks. Use its backend coverage in
the default path and separate out the browser checks. The walkthrough
already includes an attributed witness correction as a smaller pilot exercise.

- [x] Ensure the default shipped MVP API does not expose artifact parsing or PDF text-extraction workflows. Preserve historical schema compatibility without presenting legacy parser behavior as MVP capability.
- [ ] Ensure frozen reasoning input uses explicit artifact representations and preserves their attribution, scope, limitations, and non-truth semantics.
- [x] Rewrite the public README and first-user walkthrough so they contain no instructions to upload or parse WhatsApp, email, PDF, image, video, or audio artifacts.
- [ ] Validate Rust formatting, lints and tests, PostgreSQL migrations and integration tests, and the API-only bootstrap without frontend tooling.

**Exit evidence:** an API caller can reference an inaccessible external artifact, describe or transcribe relevant material, analyze it, and later retrieve exactly which attributed representation entered reasoning. The default API has no parsing surface.

### Block B — reconcile normative documentation with current owner decisions

The [investigator-assisted MVP contract](../../docs/architecture/INVESTIGATOR_ASSISTED_MVP.md)
records the manual and model-assisted entry paths, the endpoint boundary and
the default opt-in policy. Review older planning material for conflicting guidance.

- [ ] Update architecture and product documents that still declare investigator prose interpretation post-MVP.
- [x] Record the optional-local-or-remote LLM boundary and the initial OpenAI-compatible endpoint contract.
- [x] Record that manual graph authoring and LLM-assisted proposals are two entry paths into the same epistemic model.
- [ ] Record proposal provenance requirements: provider, model, invocation, source text/input revision, output contract version, timestamps, and human disposition history.
- [ ] Record opt-in automation scopes: per action, per Situation, or broader user configuration; default behavior remains non-automatic.
- [ ] Move redaction assistance and artifact parsing explicitly to post-MVP scope.
- [ ] Reconcile any conflicting README, walkthrough, architecture, planning, and product statements.

**Exit evidence:** a new engineer can determine the intended MVP behavior without relying on conversation history or encountering mutually contradictory scope documents.

### Block C — prose-to-graph proposal slice

The Workbench has a draft panel and an adoption operation. The
[Workbench integration tests](../../tests/epistemic_workbench_integration.rs)
exercise attribution, exact input excerpts and adoption of an edited draft.

- [ ] Verify that an authorized API caller can submit ordinary investigator prose and obtain a provisional structure proposal.
- [x] Preserve the exact original user text independently from every interpretation or normalized representation.
- [ ] Define a bounded structured-output contract for proposed objects, relations, grounding, unresolved references, ambiguity, and clarification needs.
- [ ] Reuse or generalize the reasoning proposal ledger rather than creating an untracked model-output path.
- [ ] Require each proposed object and relation to explain the source phrase/context that motivated it.
- [ ] Prevent ungrounded enrichment: the proposal path must not silently invent motive, deception, knowledge, comprehension, loyalty, causation, or identity.
- [ ] Add adversarial tests for over-interpretation, missing provenance, unsupported certainty, unknown references, duplicate proposals, and provider retries.

**Exit evidence:** a qualified statement such as “Apparently, the witness heard someone ask for a ride” produces a bounded, attributable draft that the API identifies as provisional.

### Block D — human graph approval and manual authoring

Draft review and manual authoring work through the Workbench. Validate their
API contracts in this phase and defer visual presentation to the browser phase.

- [ ] Return typed proposed objects and relations with their labels, qualifications, and provenance.
- [ ] Support accept, edit, reject, and bounded bulk acceptance while recording the exact human action.
- [ ] Support manual creation of the same object and relation types without requiring an LLM.
- [ ] Make unconfirmed, accepted, revised, rejected, superseded, and retracted states distinguishable in the API or recorded history.
- [ ] Preserve the distinction between confirming intended meaning and asserting truth.
- [ ] Ensure bulk acceptance does not fabricate evidence that every proposal was carefully read or cognitively understood.

**Exit evidence:** the replay exercises both manual authoring without a model and reviewer adoption of edited model proposals, with explicit attribution for what enters the canonical model.

### Block E — what-if dependency impact

The Workbench integration suite checks that a human-authored what-if identifies
affected support routes, preserves unrelated routes and leaves the case intact.
Use the canonical case to check that the HTTP response and stored revisions preserve those distinctions.

- [ ] Let the investigator create a branch in which a selected claim, assumption, relation, source representation, or interpretation is treated as false, unsupported, withdrawn, or differently interpreted.
- [ ] Keep the branch separate from canonical history unless the investigator explicitly promotes a revision.
- [ ] Traverse explicit grounding, dependency, genealogy, and qualification structures without confusing provenance ancestry with epistemic support.
- [ ] Classify downstream effects at minimum as: collapses, weakens, requires review, remains supported, or currently unknown.
- [ ] Expose evidence and reasoning that survive the changed premise.
- [ ] Surface newly material information gaps and discriminating questions.
- [ ] Test cycles, joint support, alternative support routes, partial grounding, historical revisions, and changes that should have no downstream effect.

**Exit evidence:** changing one consequential premise produces an intelligible and reproducible account of what changes, what does not, and why—without rewriting the original investigation.

### Block F — investigator questions

The Workbench supports on-demand suggestions from selected objects and a
one-draft opt-in after review. Suggestions show model attribution and can be
opened in the manual form. The full question disposition and automation scope
still need separate verification.

- [ ] Generate a bounded number of questions on explicit request.
- [ ] Explain which hypotheses, assumptions, gaps, or decisions each question could discriminate.
- [ ] Preserve model provenance and distinguish generated questions from investigator-authored questions.
- [ ] Let the investigator accept, rewrite, prioritize, defer, or reject each question.
- [ ] Support explicitly configured automatic question generation after relevant input or revision without making it the universal default.
- [ ] Protect human attention against low-value question floods and weaponized formalism.

**Exit evidence:** generated questions are useful because their possible answers could change live reasoning, not merely because they sound investigative.

### Block G — graph-to-prose and report workflow

The Workbench returns prose with basis references. Check those references
against the revised case in the API response. Browser editing and export
controls belong to the resumed browser phase.

- [ ] Add “describe what this graph currently expresses” as an explicit investigator action.
- [ ] Generate fluent prose from the accepted graph, current branch, recorded uncertainty, competing accounts, and provenance.
- [ ] Prevent generated prose from silently adding claims absent from the graph.
- [ ] Make consequential sentences traceable to their supporting objects and relations.
- [ ] Preserve distinctions such as alleged/observed/inferred/hypothetical, corroborated/repeated, and unknown/unsupported/false.
- [ ] Return prose and its cited basis in a form a caller can carry forward into a revised report.
- [ ] Preserve which graph revision and scenario branch produced each document.

**Exit evidence:** the response contains prose suitable for an investigator to revise, with uncertainty preserved and the cited grounds still identifiable.

### Block H — canonical fictional-county investigation

- [ ] Define a fictional county in a fictional state with recurring institutions and actors.
- [ ] Create a bounded case in which an initially convincing account points toward the wrong person.
- [ ] Include conflicting testimony, one materially deceptive account, external artifact references, uncertain identity or timing, and evidence that remains useful after the central theory changes.
- [ ] Replay model proposals, human correction, manual graph authoring, what-if impact, generated questions, selective reanalysis, and final prose generation over the HTTP API.
- [ ] Record expected results before the replay. Use the case as a regression fixture and LLM evaluation scenario now, and as onboarding material and a product demonstration when browser work resumes.
- [ ] Ensure police, prosecution, defense, witnesses, experts, victims, suspects, and Niccolò's own prior analysis can all be challenged.

**Exit evidence:** the canonical case can be replayed from clean setup and demonstrates the full primary acceptance journey with deterministic assertions around provenance, human decisions, revisions, and downstream impact.

### Block I — executable local API pilot

The existing [first-user walkthrough](../../docs/product/FIRST_USER_MVP_WALKTHROUGH.md)
documents the browser-based bootstrap, private model configuration and a short
witness exercise. Document a separate API-only path for this phase.

- [ ] Establish and validate API-only bootstrap on a clean checkout with Rust and PostgreSQL. Apply migrations, provision restricted credentials, start the API and check persistence across restart.
- [ ] Run with one local or remote OpenAI-compatible endpoint using private operator configuration.
- [ ] Run the canonical replay against the local API and record the model-free and live model results separately.
- [ ] Fix reasoning, data-entry, API or persistence failures that prevent the replay from completing.

**Exit evidence:** a clean local API installation runs the replay without frontend dependencies and preserves the investigation and its reasoning history across restart.

### Block J — MCP surface after internal semantics stabilize

- [ ] Define MCP tools over stable application capabilities rather than exposing database tables or unstable internal structures.
- [ ] Candidate tools include Situation creation, artifact-reference recording, structure proposal, graph inspection, what-if evaluation, question generation, and traceable prose generation.
- [ ] Preserve workspace authorization, provider/model provenance, human approval boundaries, idempotency, and operation budgets.
- [ ] Ensure MCP callers cannot bypass proposal review or convert tool output into human authorization.
- [ ] Apply the same application contracts to MCP and HTTP clients.

**Exit evidence:** an authorized MCP client can perform the same bounded operations with the same provenance and human-control semantics as an authorized HTTP client.

### Block K — multi-model comparison and routing

- [ ] Make all model-produced material provider/model attributable from the first proposal implementation.
- [ ] After one-provider correctness is demonstrated, compare Qwen and DeepSeek (or other configured models) on the canonical case.
- [ ] Preserve each model's independent output before synthesis, critique, validation, or human selection.
- [ ] Support role inversion and cross-review without presenting agreement as truth or disagreement as equal evidentiary weight.
- [ ] Defer elaborate automatic routing/topology optimization until reproducible evaluation justifies it.

**Exit evidence:** investigators can determine which model said what, compare performance over repeated work, and stop using models that do not justify their cost or attention.

## 4. Deferred work

### Browser phase

Once the API and reasoning gate in section 2 passes, resume from the existing
frontend. Run the canonical case and a second investigator-authored case in
the browser. Observe where investigators hesitate, mistake evidence for
interpretation, struggle to correct a graph, or miss what changed after a
revision. Use that evidence to prioritize browser fixes and tests.

Carry forward the Situation UI transition, legacy browser surface cleanup,
visual graph and review states, progressive disclosure of ontology detail,
and report editing, bounded regeneration, and export controls.

### Other deferred work

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

## 6. MVP success definition after the browser phase

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
