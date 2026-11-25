# First-user MVP walkthrough

This is the shortest supported path from a fresh local installation to a first useful Niccolò result.

It is intentionally not an architecture guide.

## 1. Bootstrap once

Use a new dedicated PostgreSQL 17 database and set the two required setup variables:

    export NICCOLO_PG_ADMIN_URL='...'
    export NICCOLO_PG_SECURE_PASSWORD='...'
    bash scripts/bootstrap-local-mvp.sh

Bootstrap builds the first-party UI and Rust binaries, applies migrations, creates a restricted
runtime login, creates one workspace plus one prepared Situation identifier, and mints a private
bearer token.

Expected result:

    Local setup complete.
    Start: bash scripts/start-local-mvp.sh

Private local files are written under `.niccolo-local/`.

## 2. Configure a reasoning model

Niccolò can prepare Situations and retain explicit human material without a reasoning model, but **Analyze**
requires an OpenAI-compatible model.

For a loopback model runtime, either export the settings before starting:

    export NICCOLO_LLM_BASE_URL='http://127.0.0.1:11434/v1'
    export NICCOLO_LLM_MODEL='your-model'

or create the ignored private file `.niccolo-local/model.env` containing the corresponding
`export` commands.

Remote HTTPS providers additionally require `NICCOLO_LLM_API_KEY`.

Do not put a bearer token or model API key in the prepared browser URL.

## 3. Start Niccolò

Run:

    bash scripts/start-local-mvp.sh

Expected output includes:

- a prepared loopback URL with workspace and Situation identifiers;
- the path `.niccolo-local/token.txt`;
- whether a reasoning model is configured.

Open the prepared URL.

The browser receives only the non-secret identifiers. Copy the bearer token from
`.niccolo-local/token.txt` into the password field and choose **Connect**.

## 4. Create the first Situation

On the Situation screen, enter a focus in ordinary language.

Example:

> I have two accounts of the same event and want to understand what is actually supported,
> what remains uncertain, and what I should ask next.

Choose **Save**.

This creates the Situation. It does not create a verdict.

## 5. Add material

A first pass can run from the focus alone, but it is usually more useful to add explicit material.

The restricted MVP accepts **no file upload and invokes no artifact parser**. It does not read text
files, email, WhatsApp exports, PDFs, images, audio, or video. Keep every original wherever you already
store it; Niccolò does not become its storage owner.

Under **External artifacts**, create a reference with:

- an artifact type and human-readable label;
- optional declared MIME type, byte length, or SHA-256;
- an optional external location such as an S3 URI or filesystem path;
- an honest declared access status, including `NONE`, `UNKNOWN`, or `UNVERIFIED`.

A location is metadata only. Niccolò does not dereference it, and `AVAILABLE` is a human claim
rather than proof that the service opened anything. Do not paste durable credentials or signed
access URLs into a locator.

To make something in the artifact available for reasoning, choose **Describe or transcribe
something in this artifact**. Supply the exact text yourself, identify who produced the
representation, bound its scope (for example `pages 7-9` or `00:13:20-00:13:45`), and record
its completeness, qualification, and limitations. This representation remains attributable and
challengeable; it is not silently promoted to truth and does not imply that Niccolò inspected the
original.

For something remembered, hypothetical, or an investigative preference, use **Human context**.
Keep this separate from an artifact representation so the provenance boundary stays visible.

To structure an investigator note, open **Workbench → Draft from investigator text**. The draft
request sends only the note to the configured model. The response shows candidate objects,
connections, model identity and input excerpts. Edit or remove items and choose
**Record reviewed structure** to record what you approved as a single operation. Open the draft
provenance from a recorded object or connection to compare the input and model's proposal with
what the investigator accepted. The manual forms work when no model is configured.
The review panel has a one-draft option to request questions and what-ifs after recording. It
starts unchecked and sends only that draft's newly recorded objects and their connections.

In **Investigative graph**, choose an object to see its incoming and outgoing connections. To add
a connection, choose the starting object and select **Start a relation here**. Focus the destination
and select **Connect to focused object**. This opens the relation form with both endpoints selected.
Choose the appropriate relation and give your rationale before recording it.

Open **Link external artifact provenance** to connect a graph object to a specific description or
transcription recorded in the Situation. State why the supplied material is relevant. The link
preserves the particular artifact, scope, qualification, supplied words, and who recorded the
link. Refer back to it when reviewing the object's support.

Open **Describe or investigate selected objects** in the Workbench and choose the objects to send
to the configured model. **Describe selection** returns editable, downloadable prose with links to
the graph it cites. **Suggest questions and what-ifs** proposes investigative leads and explains
why each matters. Open a proposal in the manual object form if you want to edit and record it.
Use the Workbench and case records to verify claims and source qualifications.

## 6. Check readiness before Analyze

The bottom of the Situation screen shows whether the local service has a reasoning model.

If no model is configured, stop here, configure it, and restart Niccolò. Do not debug the
Situation or artifact references: the problem is runtime configuration.

If the model is configured, choose:

**Analyze this Situation →**

## 7. Ask for the first pass

The default Analyze settings are intended for an ordinary first run:

- intent: understand;
- delivery: wait for the answer;
- default reasoning step budget.

The optional goal/question can be left blank if the Situation focus already says what matters.

Choose **Analyze this Situation**.

If Niccolò reaches a human boundary, answer, replan, or stop explicitly. The product does not infer
consent for consequential external action.

## 8. First useful result

The first-user success criterion is not merely HTTP 200 or a completed background operation.

A successful first pass displays **Niccolò's answer** and gives the user a path to **Review**.

Review should make it possible to inspect, as available for that analysis:

- what the answer says;
- what input reached reasoning;
- artifact/representation and claim coverage;
- competing interpretations;
- unresolved clarifications;
- useful next leads;
- attributed source-material requests and their decision history;
- whether the Situation changed after the frozen input;
- a portable analysis export.

The answer is provisional reasoning over recorded input, not a declaration of truth. Open **Read the human context in this analysis** to revisit the exact reports, assumptions, and preferences in that analysis. Each entry shows whether it was recorded as used.

Under **Worth looking at next**, check the reason beside each available lead. For a clarification lead, choose **Answer this question** and submit your answer directly. To pursue another kind of lead later, choose **Draft as an investigative preference**, edit it in the Situation, and select **Add context** to save it. Use the link to the Situation to add an artifact reference, attributed representation, or human context. To revisit an earlier answer, choose it from **Analysis history**. A newly launched analysis also appears in that history. Use **Older analyses** to browse earlier entries. In **Clarifications**, use **More questions** when the review has additional questions. Use **More answers** on a question to browse its recorded responses. An answer you save appears as human context in the Situation. While a saved answer or its correction remains active, the question drops out of the current list of suggested leads. If you retract the last active answer, it can return as a lead. The original question and answer history remain available in Clarifications.

Under **Requests for more source material**, accept, defer or decline a proposal and explain why. After accepting it, mark the acquisition complete when you have finished the external step. A settled or completed request can be reopened later. Every decision remains attributed and visible in sequence. Acceptance records human intent only: it does not fetch a message, file or link. Completion is also only your attributed report; it does not prove that material exists, attach it or turn it into evidence. If you choose to obtain something, perform that external action yourself, then add an artifact reference and explicitly attributed representation through the Situation.

## What to record during a human usability test

Do not teach the tester the architecture first. Record where they hesitate or ask for help:

1. Did they understand what a Situation is?
2. Did they understand the prepared URL / token split?
3. Did they know whether the model was configured before clicking Analyze?
4. Did they distinguish an external artifact, an attributed representation, and Human context?
5. Did they know what to type as the focus?
6. Did they understand when the first answer was ready?
7. Did they naturally open Review?
8. Could they tell evidence, inference, uncertainty, and human authority apart?
9. Did they understand that accepting a source request records a decision but fetches nothing?
10. Did any technical vocabulary become necessary to complete the basic path?
11. At what point, if any, did they stop seeing value?

A failure here is product evidence. Do not repair the user's behavior by explaining the system
unless the test itself has already ended.
