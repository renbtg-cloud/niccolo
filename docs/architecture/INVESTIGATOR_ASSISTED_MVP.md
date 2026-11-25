# Investigator-assisted MVP

This is the product target for the first assisted pilots. It updates the older exclusion of semantic interpretation of investigator text. The [external artifact contract](NICCOLO_MVP_EXTERNAL_ARTIFACTS_AND_PARALLEL_OBSERVATION_CONTRACT.md) still governs source files and their locators.

## Investigator workflow

1. An investigator writes a case input or authors structured objects and relationships manually. On request, a configured model proposes a draft graph from the written input. The draft retains model identity, exact excerpts, and qualifiers when present. The investigator can edit, discard, or adopt the proposed structure. The same canonical operations must be available without a model.
2. The investigator asks a question or requests analysis. Niccolò distinguishes what a source says, what other authors or models propose, and which conclusions the current support paths make available. A scoped tool call may retrieve provenance or compute the structural effect of a what-if when a model needs it. A question generated during analysis points to the uncertainty it would resolve. A question or model suggestion is never an observation.
3. The investigator can revise a claim or source assessment, inspect the affected and remaining support paths, and keep earlier analysis results tied to what was available at the time. The graph can be described in prose on request. The investigator can edit and export a report based on the revised graph.

An invocation requires a user request or an applicable explicit setting. Analysis after input and automatic draft acceptance are independent settings, off by default. The user may set them for one input, a batch, or a Situation. An opted-in automatic acceptance must be attributable to its rule and reversible.

## Models and deployment

Support one model first. Allow the investigator to choose roles for multiple models, including independent analysis, self-review, a separate critic, or cross-review. Keep contributions and critiques attributable to their model, and retain the original and its revision. Agreement among models does not add evidentiary support. Evaluation of which configuration helps most is future work.

Use a configured local or remote endpoint. The first adapter uses OpenAI-compatible chat completions. A confined deployment can provide its own model endpoint and run without external providers. Remote destinations must be explicit, and a local-only operation must not fall back to a remote model. Send only the material specified for the requested operation. Automatic redaction, artifact uploads, and machine search or extraction across external PDFs, messages, audio, or video are outside the pilot.

## Pilot readiness

Validate with a synthetic or explicitly authorized case. Ask a tester to record a qualified statement, check its source, request a new question, change the support for one conclusion, inspect the impact, and review the resulting prose. Record what the tester can do without operator intervention and where the effort is justified. The working software and walkthrough determine which steps can be offered to a pilot; this document records the target.
