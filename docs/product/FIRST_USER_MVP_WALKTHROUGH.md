# First-user MVP walkthrough

This is the shortest supported path from a fresh local installation to a first useful Niccolò result.

It is intentionally not an architecture guide.

## 1. Bootstrap once

Use a new dedicated PostgreSQL 17 database. The setup login should own that database and have
`CREATEROLE` so bootstrap can create the dedicated restricted runtime login. It does **not** need
`SUPERUSER` or `BYPASSRLS`.

Niccolò generates the restricted runtime-login password by default, so a first user does not need
to invent or manage a second database password. Start with:

    bash scripts/first-run-local-mvp.sh

On an interactive first run, the wrapper asks for the PostgreSQL database-owner URL with hidden
input. Automation may instead export `NICCOLO_PG_ADMIN_URL` before invoking it.

For a brand-new interactive installation, that one command performs the missing bootstrap, prompts
for the reasoning-model settings if needed, and then starts the local service. On subsequent runs it
recognizes the existing private bootstrap and does not rerun migrations or rotate credentials. Use
`--skip-model-config` only when intentionally preparing/importing data without Analyze.

Deployment automation may explicitly set `NICCOLO_PG_SECURE_PASSWORD` before bootstrap when it
needs to control that credential. The value is never printed. The ordinary local path generates a
random password and stores only the resulting runtime connection string in `.niccolo-local/local.env`.
The lower-level `scripts/bootstrap-local-mvp.sh`, `scripts/configure-local-model.sh`, and
`scripts/start-local-mvp.sh` commands remain available for automation or deliberately separated setup.

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

The shortest path is the private configuration helper:

    bash scripts/configure-local-model.sh

It prompts for the OpenAI-compatible base URL and model name, hides API-key input, rejects remote
plain HTTP, and writes `.niccolo-local/model.env` with restrictive permissions where the platform
supports them. A loopback HTTP model may be keyless; remote HTTPS providers require an API key.

For scripted or temporary overrides, export `NICCOLO_LLM_BASE_URL`, `NICCOLO_LLM_MODEL` and any
other `NICCOLO_LLM_*` settings before starting. Explicit process-environment values override the
stored private defaults for that invocation. `--from-env` lets automation persist already-exported
values without putting an API key on a command line.

Do not put a bearer token or model API key in the prepared browser URL.

## 3. Start Niccolò

Run:

    bash scripts/start-local-mvp.sh

Expected output includes:

- a prepared loopback URL with workspace and Situation identifiers;
- the path `.niccolo-local/token.txt`;
- whether a reasoning model is configured.

On a local desktop, Niccolò waits for the service to become healthy and then makes a best-effort
attempt to open that prepared URL in the default browser. Headless systems simply keep the printed
URL as the fallback. Set `NICCOLO_OPEN_BROWSER=0` or pass `--no-browser` to suppress automatic
opening.

If the browser did not open automatically, open the prepared URL shown by the start command.

The browser receives only the non-secret identifiers. Copy the bearer token from
`.niccolo-local/token.txt` into the password field and choose **Connect**. Niccolò first performs
a bounded workspace read to validate the token/workspace pair; a rejected token does not create a
connected browser session or require you to recreate the prepared Situation.

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

Choose one or several files and then **Import and attach**. A browser batch is prevalidated
before the first upload, is bounded to 32 files / 64 MiB total, and each file still keeps the
server's 4 MiB per-source limit. If a later file fails after earlier ones were attached, retrying
the same selection reuses their document identities rather than creating duplicate records.

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

**Advanced execution** also exposes explicit operation-wide model-call, reported-token and
finalization-reserve limits. Their defaults are 32 model calls, 200,000 reported tokens and a
5-second deterministic finalization reserve. They are resource ceilings, not confidence or
quality scores; budget exhaustion preserves a visibly provisional result instead of pretending
that the investigation is complete.

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

The standard portable ZIP contains recorded analysis state and source identifiers but no source
text. Review also offers a separate **Sensitive export** action that can include only source
fragments recorded as examined by that analysis. That action requires write authorization, is bounded, excludes original uploaded files, and warns
that excerpts may contain private communications and do not establish truth. Before the sensitive
download is enabled, Review shows a read-only scope preview with the number of recorded examined
fragments and the UTF-8 byte count that would be included; the preview itself exposes no source
text. Each successful sensitive export also records an immutable local authorization event with the
requesting principal, selected fragment/byte counts and SHA-256 of the exact presentation package.
Retrying an interrupted browser response reuses the same event ID instead of creating duplicate
audit history.

The answer is provisional reasoning over recorded input, not a declaration of truth.

## 9. Optional: bring back a prior Niccolò export

This is an advanced recovery/continuity path, not part of the basic first-run funnel. Open
**Technical → Prior exports** when you need to preserve or inspect historical Niccolò material.

From there you can:

- download a forensic JSON export of the current Situation;
- import a prior Niccolò forensic JSON package into a workspace quarantine;
- inspect the imported batch, its provenance, logical rows and retained original bytes;
- download retained original bytes without promoting them into the current source catalog;
- explicitly select an imported item as **non-evidence historical context** for a current analysis.

The quarantine boundary is intentional. A structurally valid package does **not** make historical
claims true, does not turn old model output into documentary evidence, and does not let current
credentials vouch for old statements. Imported rows are not copied into native evidence, identity,
configuration, source, or analysis tables.

Selecting an imported item for an analysis also does not rewrite the frozen input or any published
answer. When you explicitly approve **Reanalysis**, Niccolò carries the parent analysis's selected
quarantined context into the new analysis before reasoning starts. The copied selection remains
non-evidence historical context, keeps the original human selector/rationale, and still does not
become part of the frozen Situation input. Review shows selected imported context separately so the
provenance boundary remains visible.

## 10. Optional: record a deterministic heuristic score

In **Review**, a claim with recorded source links can be given an explicit, human-directed heuristic weighting. Choose the claim, assign each linked source a bounded strength (0–100) and explain the rationale. Niccolò derives the source stance and recorded dependence from PostgreSQL rather than trusting the browser to declare them.

Recording is a separate write action. It creates an immutable score artifact containing the exact policy, declared strengths, provenance-aware deterministic result, graph revision and result SHA-256. Reusing the same score ID with different contents fails closed.

The recorded score is **not** a probability of truth, proof, documentary evidence or certification of witness independence. It does not rewrite the published analysis. It can later be inspected and included in the portable analysis export.

## 11. Optional: publish the standard package to user-owned GitHub

In **Review**, the GitHub publication panel can record a target repository owner/name/ref and show the exact standard presentation-package file count, byte count and SHA-256 before anything leaves the machine.

Recording a target or publication intent is still local-only: it does not contact GitHub or prove remote preservation. An optional **read-only remote preflight** can use a transient token to verify the repository, declared visibility and requested ref without creating a commit or changing a ref; when the requested branch is absent, the preflight reports the default branch that an explicit publication would use as its creation base. After reviewing the exact package SHA-256 and requested ref, the user may explicitly publish that recorded standard package with a **transient GitHub fine-grained token**.

Niccolò does not persist that token. The remote operation uses a non-force Git ref update, then re-reads the ref and records append-only publication observations in PostgreSQL. The UI says **verified published** only when the observed remote ref points at the exact commit Niccolò just constructed. Failed attempts keep only a bounded failure classification; provider response bodies and credentials are not recorded. PostgreSQL remains the authoritative publication ledger even when the package has been copied to user-owned GitHub.

The current Git planning scope deliberately excludes examined source text and original uploads.

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
