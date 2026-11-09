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

Niccolò can prepare Situations and retain sources without a reasoning model, but **Analyze**
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

A first pass can run from the focus alone, but the useful path is to add at least one source or
piece of human context.

Supported MVP source files include:

- `.eml` email;
- `.txt` WhatsApp/plain-text export.

Choose **Import and attach**.

For information that is remembered, hypothetical, or an investigative preference rather than a
source record, use **Human context** instead. Niccolò keeps that separate from verified evidence.

## 6. Check readiness before Analyze

The bottom of the Situation screen shows whether the local service has a reasoning model.

If no model is configured, stop here, configure it, and restart Niccolò. Do not debug the
Situation or re-upload files: the problem is runtime configuration.

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
- source/claim coverage;
- competing interpretations;
- unresolved clarifications;
- useful next leads;
- whether the Situation changed after the frozen input;
- a portable analysis export.

The answer is provisional reasoning over recorded input, not a declaration of truth.

## What to record during a human usability test

Do not teach the tester the architecture first. Record where they hesitate or ask for help:

1. Did they understand what a Situation is?
2. Did they understand the prepared URL / token split?
3. Did they know whether the model was configured before clicking Analyze?
4. Did they distinguish uploaded records from Human context?
5. Did they know what to type as the focus?
6. Did they understand when the first answer was ready?
7. Did they naturally open Review?
8. Could they tell evidence, inference, uncertainty, and human authority apart?
9. Did any technical vocabulary become necessary to complete the basic path?
10. At what point, if any, did they stop seeing value?

A failure here is product evidence. Do not repair the user's behavior by explaining the system
unless the test itself has already ended.
