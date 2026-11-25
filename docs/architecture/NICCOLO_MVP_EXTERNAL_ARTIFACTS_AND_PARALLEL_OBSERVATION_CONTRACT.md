# Niccolò MVP — External Artifacts, Storage Independence, and Parallel Observation

## Status

**Normative MVP architecture contract.**

This document captures product and architecture decisions made for Niccolò's MVP concerning:

- source artifacts and storage;
- PDFs, video, images, audio, and arbitrary binary artifacts;
- human descriptions and transcriptions;
- future machine interpretation;
- provenance, access, representations, observations, and derived material;
- separation between the investigated world and Niccolò's own operational state.

It is the authoritative boundary for reconciling existing documentation, schema, APIs, UI, tests, and code.

This contract is deliberately stronger than the current PDF implementation. Current code may retain original source bytes, impose small upload limits, preview PDF text with `pdftotext`, and treat PDFs specially. Those behaviors should be reviewed against this contract and changed where necessary.

---

# 1. Core decision

## 1.1 Niccolò does not own source artifacts

The fundamental rules are:

> **Niccolò owns reasoning state. Niccolò does not own source artifacts.**
>
> **The MVP does not inspect source artifacts. It has no artifact parsers, upload pipeline, automatic fragment extraction, or modality-specific interpretation path.**

Niccolò's database is authoritative for its own epistemic and operational state:

- Situations;
- artifact identities and references;
- source provenance;
- human contributions;
- observations;
- representations;
- claims;
- interpretations;
- hypotheses;
- questions;
- analyses;
- revisions;
- decisions;
- relationships;
- access state;
- investigation history.

It is **not** the primary blob store for the real-world artifacts being investigated.

Large or small source artifacts may live elsewhere:

- local filesystem;
- shared/network filesystem;
- S3-compatible storage;
- AWS S3;
- Azure Blob Storage;
- Google Cloud Storage;
- another object store;
- removable/offline storage;
- an external system;
- an unknown or currently inaccessible location.

Niccolò must remain useful even when it has no direct access to the artifact.

---

# 2. Architectural separation

Keep these concepts distinct:

```text
WORLD / EXTERNAL STORAGE
        |
        |  zero or more storage references
        v
+----------------------+
|       Artifact       |
+----------------------+
        |
        | described / transcribed / extracted / observed
        v
+----------------------+
| Observation /        |
| Representation       |
+----------------------+
        |
        | supports / contradicts / contextualizes / is interpreted as
        v
+----------------------+
| Claim / Hypothesis / |
| Interpretation /     |
| Reasoning object     |
+----------------------+
        |
        v
     Analysis
```

The artifact is not its transcript.

The artifact is not its extracted text.

The artifact is not a user's description.

The artifact is not an LLM interpretation.

The artifact is not a `pdftotext` result.

These are related but epistemically distinct objects.

---

# 3. Artifact

## 3.1 Definition

An `Artifact` is a real or alleged external object relevant to the investigation.

Examples:

- PDF;
- video;
- audio recording;
- image;
- screenshot;
- email file;
- WhatsApp export;
- ZIP/archive;
- spreadsheet;
- CAD file;
- database dump;
- arbitrary binary;
- paper document represented only by metadata;
- physical object for which no digital file is available.

The core epistemic model should not depend on the modality.

Avoid making `PdfDocument`, `Video`, `Image`, etc. privileged epistemic roots. Modality-specific adapters may exist, but the deeper model should remain generic.

## 3.2 Suggested artifact metadata

An artifact may record, when known:

- `artifact_id`;
- media/type declaration;
- filename or human label;
- MIME type, if known;
- byte length, if known;
- hashes, if known;
- creation time, if known or claimed;
- collection time;
- provenance;
- relationship to other artifacts;
- authenticity/completeness qualifications;
- zero or more storage references.

Unknown values must remain unknown rather than invented.

---

# 4. Storage is external to the epistemic object

## 4.1 StorageReference

An artifact may have zero, one, or many `StorageReference` objects.

Examples:

```text
file:///home/renato/cases/17/video.mp4
file:///Z:/shared/case17/video.mp4
s3://acme-evidence/case17/video.mp4
gs://bucket/path/file.pdf
azure://container/path/file.tiff
```

Do not assume that multiple locations mean multiple independent pieces of evidence.

Three copies of one video are still one artifact unless there is evidence that they are materially different artifacts.

## 4.2 Suggested StorageReference fields

Possible fields:

```text
storage_reference_id
artifact_id
storage_type
locator
provider
account_or_environment_label
region
bucket_or_container
path_or_object_key
declared_by
recorded_at
access_status
access_method
last_access_attempt
last_access_result
notes
```

Not every backend needs all fields. Prefer a bounded extensible structure over a universal cloud ontology.

## 4.3 Storage type

Initial values might include:

```text
LOCAL_FS
NETWORK_FS
S3
AZURE_BLOB
GCS
HTTP
HTTPS
REMOVABLE_MEDIA
OFFLINE
OTHER
UNKNOWN
```

These values describe location/mechanism, not truth or accessibility.

---

# 5. Location and access are different concepts

A known location does not imply that Niccolò can read the artifact.

Do not collapse storage location and runtime access into one boolean.

At minimum distinguish states equivalent to:

```text
UNKNOWN
NONE
AVAILABLE
TEMPORARILY_UNAVAILABLE
ACCESS_DENIED
NOT_FOUND
UNVERIFIED
```

Examples:

- User says the video is in S3; Niccolò has no credentials.
- Niccolò accessed the object yesterday but credentials have expired.
- A path is known but the drive is offline.
- A bucket/key was supplied but never verified.
- Two storage references exist and only one is currently accessible.

Access state is itself temporal and revisable.

---

# 6. Credentials are not epistemic data

Do not store cloud credentials, access keys, passwords, bearer tokens, or equivalent secrets inside artifact/storage epistemic objects.

If runtime integration needs credentials, reference an external configuration/capability profile, for example:

```text
resolver_profile = "client-legal-s3-readonly"
```

The resolver/runtime decides whether access is possible.

The investigation should remain intelligible if the resolver disappears permanently.

---

# 7. Niccolò must operate without artifact access

This is a product requirement, not a tolerated failure mode.

Niccolò must be able to reason about an artifact that it has never opened.

Example:

```text
Artifact V1:
  type: VIDEO
  locator: s3://client-bucket/case17/meeting.mp4
  access_status: NONE

Human observation H1:
  Renato reports that V1 shows João entering the room at 14:32.

Human observation H2:
  Ana reports that the same video does not show João entering until 14:41.

Representation T1:
  external transcript allegedly derived from V1.
```

Niccolò can investigate the conflict among H1, H2, and T1 without pretending that it watched V1.

This is epistemically preferable to inventing machine perception.

---

# 8. No automatic PDF interpretation in the MVP

## 8.1 MVP rule

For MVP purposes, PDF should follow the same deep rule as video and image:

> **Niccolò does not automatically interpret the contents of a PDF.**

No MVP requirement for:

- `pdftotext`;
- automatic section extraction;
- chunking;
- OCR;
- table extraction;
- figure interpretation;
- semantic classification;
- automatic document summarization;
- machine claims about what the PDF contains.

A PDF is an artifact.

The user may tell Niccolò what matters about it.

## 8.2 User-supplied PDF information

The user may record:

- page number;
- page range;
- section title;
- exact quoted/transcribed text;
- summary;
- description;
- allegation;
- interpretation;
- relationship to another artifact;
- relationship to a claim/hypothesis;
- reason the material matters;
- uncertainty;
- omissions;
- whether the contribution is believed complete or partial.

Example:

```text
Artifact P1 = board-presentation.pdf

Human representation R1:
  scope: pages 7-9
  type: transcription
  content: "Revenue is expected to increase by 40%..."
  producer: Renato
  completeness: partial

Human interpretation I1:
  "I believe the graph on page 7 exaggerates projected revenue."

Relation:
  I1 challenges Claim C7.
```

Niccolò records that Renato said these things.

It does not silently upgrade them to machine-verified PDF contents.

---

# 9. Same rule for video, image, audio, and unsupported modalities

For an unsupported modality, the MVP may know:

- the artifact exists or is alleged to exist;
- where it is or was stored;
- who supplied the reference;
- its hash/size if known;
- how it relates to other artifacts/events;
- what humans or external tools claim about it.

It does not need to perceive the artifact.

Examples:

### Video

```text
Video V1
Human report: "At 00:13:24 João leaves the room."
```

### Image

```text
Image I1
Human report: "The screenshot shows a message attributed to Carlos saying X."
```

### Audio

```text
Audio A1
External transcript T1
Human correction C1: "The speaker says 'fifteen', not 'fifty'."
```

All remain attributable and challengeable.

---

# 10. Representation

## 10.1 Definition

A `Representation` is content that purports to represent some portion or aspect of an artifact.

Examples:

- transcript;
- transcription;
- extracted text;
- OCR result;
- copied passage;
- manually typed table;
- human description of a figure;
- frame description;
- summary;
- converted file;
- normalized text;
- externally produced Markdown;
- externally produced TXT.

A representation is not automatically a fact.

## 10.2 Representation may itself be external or internal

Both should be supported conceptually.

### External representation

Example:

```text
meeting.mp4
meeting-transcript.md
```

The `.md` is an artifact in its own right and is related to the video:

```text
meeting-transcript.md
    DERIVED_FROM
meeting.mp4
```

It may have its own storage reference and hash.

### Internal representation

The user pastes a transcript directly into Niccolò.

Niccolò may persist this text as part of reasoning state without requiring a separate externally stored file.

The important distinction is:

> Niccolò should not become a general-purpose blob repository, but it may store textual epistemic material that is part of its reasoning state.

---

# 11. Suggested Representation metadata

When relevant:

```text
representation_id
source_artifact_id
representation_type
producer_type
producer_identity
producer_version
production_method
created_at
scope
locator_within_artifact
content
completeness_status
epistemic_qualification
limitations
derived_from
```

Possible `representation_type` values:

```text
HUMAN_DESCRIPTION
HUMAN_TRANSCRIPTION
TRANSCRIPT
EXTRACTED_TEXT
OCR
SUMMARY
TABLE_TRANSCRIPTION
FRAME_DESCRIPTION
CONVERSION
OTHER
```

Do not make the enum artificially closed if the architecture can support an extensible qualified type.

---

# 12. Observation

`Observation` should represent something asserted to have been observed about an artifact, representation, event, or relationship.

An observation must identify its observer or producer.

Possible observers include:

- user;
- third-party human;
- deterministic parser;
- OCR engine;
- transcription service;
- LLM;
- multimodal model;
- another Niccolò instance;
- external software;
- unknown observer.

The observer's authority must never be silently elevated.

---

# 13. Human reports and machine observations share the same epistemic universe

The architecture should not create one privileged path for machine-derived material and another for humans.

Example future state:

```text
Artifact P1: contract.pdf

Observation H1:
  observer: Renato
  scope: page 17
  "The termination clause requires 30 days."

Observation M1:
  observer: pdftotext 24.02
  scope: page 17
  extracted text: "termination ... thirty (30) days ..."

Observation M2:
  observer: OCR engine X
  scope: page 17
  extracted text: "termination ... sixty (60) days ..."

Interpretation L1:
  observer: multimodal model Y
  "The scanned correction appears to overwrite '30' with '60'."
```

Niccolò may compare them.

None is the artifact itself.

---

# 14. Future machine interpretation as a parallel layer

This architecture must explicitly support adding machine perception later without redesigning the epistemic core.

Future adapters may include:

- PDF text extraction;
- OCR;
- layout analysis;
- table extraction;
- image understanding;
- video understanding;
- audio transcription;
- speaker diarization;
- spreadsheet parsing;
- specialized legal/financial parsers;
- multimodal LLMs.

These adapters produce representations/observations.

They do not mutate the artifact into "understood content".

## 14.1 Architectural invariant

> **No artifact modality may become a privileged epistemic path. Machine interpretation of an artifact is an attributed observation over that artifact, not the artifact itself.**

---

# 15. ArtifactObserver / observation execution

A future optional abstraction may describe the component performing machine observation.

Example:

```text
ArtifactObserver:
  observer_id
  type
  implementation
  version
  deterministic_or_probabilistic
  supported_modalities
  declared_capabilities
```

An execution may record:

```text
artifact_id
observer_id
scope
started_at
finished_at
parameters
result_representation_id
coverage
limitations
errors
```

Example:

```text
artifact: contract.pdf
observer: pdftotext 24.02
scope: pages 1-84
coverage: embedded text layer only
limitations:
  - no OCR
  - figures not interpreted
  - reading order may be wrong
```

---

# 16. Access-aware future investigation

Future Niccolò may propose obtaining a new observation without assuming it can do so.

Example:

```text
We only have a human description of page 17.
The PDF is accessible to a configured deterministic text extractor.
Extracting page 17 may discriminate H1 from H2 at low cost.
```

Or:

```text
The relevant video is referenced in S3 but access_status = NONE.
Automatic transcription is currently unavailable.
Ask the user for a transcript or for access if obtaining it is worth the cost.
```

Thus artifact interpretation becomes an investigatory capability subject to:

- access;
- cost;
- relevance;
- authorization;
- scope;
- tool availability;
- privacy;
- resource limits.

It is not an unconditional ingestion step.

---

# 17. Artifact identity versus copies

Multiple storage references or copies must not be counted automatically as independent evidence.

Examples:

```text
local copy
S3 backup
NAS backup
```

may all point to the same artifact.

If hashes match, this supports byte equality among those copies, not independent evidentiary origin.

If hashes differ, Niccolò must not silently choose one as canonical truth.

Differences may themselves become investigable.

---

# 18. Hashes and integrity

A hash may support claims such as:

- these bytes match previously recorded bytes;
- these two accessible copies have the same digest.

A hash does not establish:

- historical authenticity;
- authorship;
- truth;
- completeness;
- absence of prior manipulation;
- independent origin.

Preserve the existing Niccolò distinction between integrity and authenticity.

---

# 19. Artifact relationships

Artifacts and representations may participate in relations such as:

```text
COPY_OF
DERIVED_FROM
TRANSCRIPTION_OF
REPRESENTATION_OF
EXCERPT_OF
CONTAINS
ATTACHED_TO
QUOTES
REPLIES_TO
FORWARDS
REFERENCES
DESCRIBES
TRANSFORMED_FROM
CLAIMED_TO_BE_COPY_OF
```

Do not assume every relation is verified.

Record attribution and qualification when needed.

---

# 20. Scope within an artifact

A representation or observation may target a bounded region:

```text
page 7
pages 7-9
section "Commitments"
paragraph 4
time 00:13:20-00:13:45
frame around 00:13:24
sheet "Forecast"
cells B12:F17
audio channel 2
byte range
logical section
user-defined region
```

The core should support locators without requiring a universal locator grammar for every modality in the MVP.

---

# 21. User interface direction

The GUI should avoid implying that Niccolò stores or understands every artifact.

Prefer language such as:

- **Add artifact reference**
- **Describe where this artifact is stored**
- **Niccolò access: none / available / unknown**
- **Describe what matters in this artifact**
- **Add a transcription or representation**
- **Link this description to pages 7-9**
- **Explain why this matters**
- **Relate this artifact to another source or claim**

Avoid misleading labels such as:

- "Niccolò read this PDF"
- "PDF analyzed"
- "Video understood"
- "Verified contents"

unless a future observation capability actually ran and the UI identifies that observer and scope.

---

# 22. Storage-independent lifecycle

Niccolò must not assume that artifact accessibility is permanent.

Possible lifecycle:

```text
reference recorded
        |
artifact available
        |
artifact temporarily unavailable
        |
artifact moved
        |
new storage reference added
        |
old location inaccessible
```

The historical investigation remains valid as a record of what was known and available at the relevant time.

Do not delete epistemic history merely because external storage disappears.

---

# 23. Offlining and air-gapped use

This architecture should naturally support:

- fully local installations;
- disconnected networks;
- air-gapped deployments;
- local/NAS paths;
- object storage;
- artifacts held by a human outside the machine;
- artifacts Niccolò is never authorized to access.

No cloud provider is foundational to the epistemic model.

---

# 24. Implications for current implementation

The implementation agent must inspect current code and reconcile at least the following areas.

## 24.1 Current original-byte storage

Current code may store uploaded originals in database tables such as `source_payloads`.

This conflicts with the new direction when used as Niccolò's general artifact repository.

Review and redesign so that:

- artifact metadata can exist without original bytes stored by Niccolò;
- storage references are first-class;
- reasoning state does not depend on byte ownership;
- existing source history can be migrated or compatibility-preserved if necessary.

Do not delete data or migrations casually. Design a forward migration.

## 24.2 Current 4 MiB limit

A single global source-size limit such as 4 MiB is inappropriate as a conceptual artifact limit.

For externally stored artifacts, Niccolò may record metadata for arbitrarily large artifacts.

Any future runtime limit should be local to a particular operation:

- upload of a textual representation;
- temporary tool input;
- extraction job;
- preview;
- API request.

Do not equate an operation budget with the maximum size of a real-world artifact.

## 24.3 Current PDF preview / `pdftotext`

The MVP should no longer require built-in PDF reading to make PDFs useful.

Review whether the current `pdftotext` preview should be:

1. removed from the MVP;
2. disabled behind an experimental/non-MVP capability; or
3. reintroduced later as an explicit `ArtifactObserver`.

If retained later, its result must be modeled as an attributed machine representation/observation with:

- tool identity/version;
- scope;
- limitations;
- provenance.

Do not silently insert extracted text as if it were the PDF itself.

## 24.4 Current PDF-as-special-source behavior

Avoid making PDF a privileged branch in the epistemic core.

PDF-specific UI/adapters may exist, but source/artifact identity and representations should generalize to arbitrary modalities.

## 24.5 Current source fragments

Existing automatically parsed fragments are legacy historical records. Preserve their identifiers, provenance, revision history, and use in earlier analyses, but do not produce new automatic fragments in the MVP.

The MVP does not parse `.eml`, WhatsApp exports, `.txt`, `.md`, PDF, image, audio, video, archives, spreadsheets, or any other source-artifact format. A human or external system may explicitly submit a textual representation or observation, but Niccolò records that contribution as attributed epistemic material rather than parsing an artifact.

No modality-specific parser is part of the MVP.

---

# 25. Migration strategy principle

Prefer a migration that generalizes the model rather than patching PDF/video individually.

Desired direction:

```text
source_document
      ↓ generalize
artifact

source_payload / original bytes
      ↓ decouple
storage_reference / external ownership

parsed text / transcript / user report
      ↓ generalize
representation / observation

PDF-specific "passage"
      ↓ generalize
scoped representation / scoped human contribution
```

Exact table and type names are implementation choices.

Preserve existing stable IDs and history where feasible.

---

# 26. MVP acceptance examples

## 26.1 Large inaccessible video

Given:

```text
Video: 18 GB
Location: s3://client/video/meeting.mp4
Niccolò access: NONE
```

User can still:

- register it;
- describe its relevance;
- link it to an event;
- add a transcript;
- add a human observation;
- relate a claim to a timestamp;
- investigate disagreements about its contents.

Niccolò does not need the 18 GB object.

## 26.2 PDF with no machine reading

Given:

```text
PDF artifact
Location: shared filesystem
Niccolò access: NONE
```

User can add:

```text
pages 12-14
section: Commitments
human transcription: ...
why it matters: contradicts email E4
```

Niccolò can reason over that contribution without claiming it read the PDF.

## 26.3 External transcript

Given:

```text
Video V1
Transcript T1.md
```

T1 is recorded as derived from V1.

Niccolò can reason over T1 while preserving:

- producer;
- method;
- completeness;
- uncertainty;
- storage;
- artifact relationship.

## 26.4 Future automatic PDF extraction

Later:

```text
P1 -> pdftotext observer -> R1
P1 -> OCR observer -> R2
P1 -> human transcription -> R3
```

R1, R2, and R3 coexist.

Niccolò may compare disagreement without altering the core artifact model.

## 26.5 Copies do not multiply evidence

Given:

```text
file:///case/foo.pdf
s3://backup/foo.pdf
nas://archive/foo.pdf
```

All have the same hash.

Niccolò records three storage references for one artifact, not three independent sources.

---

# 27. Explicit non-goals for the MVP

The MVP does not need to provide:

- managed object storage;
- S3 upload service;
- Azure/GCS upload service;
- automatic copying of external artifacts;
- universal file conversion;
- OCR;
- PDF understanding;
- video understanding;
- image understanding;
- automatic audio transcription;
- universal document parser;
- credentials manager for arbitrary external stores;
- automatic fetching from every recorded locator;
- automatic parsing of raw-text artifacts;
- email parsing;
- WhatsApp or chat-export parsing;
- automatic speaker extraction;
- automatic fragment generation from any artifact;
- any artifact upload or transient artifact-processing endpoint.

These can be added as parallel capabilities later.

---

# 28. Security and privacy

External-artifact references can themselves be sensitive.

Treat locators and storage metadata as workspace-scoped protected information.

Do not:

- expose external paths across workspaces;
- log secrets embedded in URLs;
- store presigned URLs as durable credentials without explicit policy;
- treat access to metadata as access to artifact contents;
- infer authorization from the fact that an artifact is mentioned.

External access must use explicit runtime authorization.

---

# 29. Epistemic invariants

The implementation must preserve these invariants.

1. **Artifact != representation.**
2. **Representation != interpretation.**
3. **Interpretation != fact.**
4. **Storage location != access.**
5. **Access != examination.**
6. **Examination != comprehension.**
7. **Hash continuity != authenticity.**
8. **Copy != independent source.**
9. **Human description != machine perception.**
10. **Machine extraction != artifact truth.**
11. **Unavailable artifact != nonexistent artifact.**
12. **Unknown location != nonexistent artifact.**
13. **Niccolò may reason about inaccessible artifacts.**
14. **No modality is epistemically privileged.**
15. **Future machine perception enters as attributed observation/representation.**
16. **Artifact storage ownership is not required for Niccolò reasoning.**
17. **Niccolò's database owns reasoning state, not necessarily source bytes.**

---

# 30. Guiding formulation

The intended architecture can be summarized as:

> **Artifacts belong to the world being investigated. Representations of them belong to the epistemic process. Niccolò owns the latter, not necessarily the former.**

And for future machine interpretation:

> **No artifact modality may become a privileged epistemic path. Machine interpretation of an artifact is an attributed observation over that artifact, not the artifact itself.**

And operationally:

> **Niccolò must remain capable of investigating an artifact it has never accessed.**

---

# 31. Implementation instruction

An implementation agent using this document should:

1. inspect current schema, legacy source ingestion, `source_payloads`, source catalog, PDF preview, GUI, and tests;
2. identify where current behavior assumes Niccolò owns source bytes;
3. propose/generalize the artifact/storage/representation model;
4. migrate incrementally without destroying history;
5. remove artifact upload, parsing, preview, and automatic fragment-generation behavior from the MVP while preserving legacy history;
6. make external storage references and access state representable;
7. make human descriptions/transcriptions first-class and scoped to artifacts/regions/relationships;
8. preserve provenance and attribution;
9. add adversarial tests for inaccessible artifacts, duplicate copies, conflicting human representations, and the absence of parser/upload behavior;
10. update product/architecture documentation so implemented behavior and intended MVP capability remain explicitly distinct.

Do not solve this by adding special cases only for PDF and video.

The goal is a modality-neutral epistemic substrate on which future artifact observers can be attached in parallel.


---

# 32. Strict MVP sequencing

The parser-free MVP must become stable, functional, and epistemologically precise before artifact-input experiments begin.

Only after that boundary is met may a post-MVP program consider artifact inspection. Raw text files are the first candidate modality. Deterministic NLP, an attributed LLM observer, and hybrid approaches must be evaluated as competing implementations rather than assumed capabilities. Email, WhatsApp exports, PDF, image, audio, and video follow only through later explicit decisions.

Language-model drafting from an investigator's directly submitted note is in scope for the [investigator-assisted MVP](INVESTIGATOR_ASSISTED_MVP.md). The input remains an investigator statement, with the model's interpretation kept separate from the source artifact.

Future observers must produce attributed, scoped, revisable representations or observations. They never become privileged truth paths and their future possibility does not authorize parser infrastructure in the MVP.


