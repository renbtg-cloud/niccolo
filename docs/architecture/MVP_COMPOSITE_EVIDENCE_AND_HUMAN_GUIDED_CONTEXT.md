# MVP contract — composite evidence and human-guided circumstances

**Status:** Normative product/architecture contract for the **MVP**, not a claim of implemented functionality and not a post-MVP proposal.
**Scope:** Evidence intake, relationships, attribution, document structure, human interpretation, and investigative context.
**Read with:** [architecture index](README.md), [collection provenance](COLLECTION_PROVENANCE_MVP.md), [source scope/missingness](SOURCE_SCOPE_MISSINGNESS_MVP.md), [integrity versus authenticity](INTEGRITY_SCOPE_MVP.md), and [first-user walkthrough](../product/FIRST_USER_MVP_WALKTHROUGH.md).

## 0. How implementers must use this contract

This specification is **deliberately non-exhaustive**. Its examples are illustrative, not a complete enumeration of inputs, behaviors, relationship types, failure modes, user journeys, schemas, or tests. An implementing AI/engineer **may and probably should complete, correct, extend, reorganize, and improve** the technical specification; discover missing cases; design better abstractions; and propose or perform appropriate refactorings. Preserve the core product principles, epistemic distinctions, privacy boundaries, and explicit MVP exclusions. If a proposed change alters those principles or materially expands the MVP, flag the tradeoff and obtain a product decision rather than silently redefining the contract.

**This belongs to the MVP now; its implementation is not required immediately unless now is the most technically opportune moment.** At *every relevant* development cycle, consider the contract and evaluate dependencies, migration cost, risk, testing, and whether an early refactoring would prevent avoidable rework. If now is best, implement/refactor now. If later is better, record the reason, dependencies, suggested insertion point, and any architectural precautions. Do not postpone simply because the MVP is unfinished; do not force work prematurely merely because it appears in this file. Milestone counts or existing code do not justify preserving a poor abstraction.

Before changing code, inspect current executable interfaces, source parsers, schema/migrations, UI, tests and pending work. Keep design/intent, implemented behavior, and demonstrated/tested behavior explicitly separate. Historical checkpoint documentation can be stale.

## 1. Human epistemic authority and bounded circumstances

**Investigation is collaborative, not a sovereign interpretation of reality by Niccolò and/or any supporting AI.**

The user must **define or at least guide the domain of circumstances** considered relevant to a Situation: participants, relationships, recent events, organizational power, relevant time frames, incentives, disputed assumptions, and other possible context. Niccolò can analyze supplied/retrieved material, ask questions, expose missing context, and **propose** new contextual dimensions or competing explanations. It must not quietly assume they exist, declare them true, or autonomously redefine the relevant circumstance domain without visible human guidance/authorization.

Distinguish at minimum:
- Original artifact and directly extractable recorded content;
- User/third-party descriptions and claims about an artifact or its meaning;
- Communicative acts, relationships, and context supplied or evidenced;
- Model-generated observations (if any), hypotheses, interpretations, and suggestions;
- Unknown, missing, excluded, unretrieved, and contradicted material.

A user's confident description is an **attributed human assertion**, not a machine-verified observation. A model's interpretation is also **not evidence** merely because it came from a model. User choice of what to investigate does not establish truth. No hidden elevation of model, privileged operator, system-integrator, or administrator authority.

## 2. Evidence can be composite, recursive, and relational

Treat emails, WhatsApp conversations, documents, messages, attachments, links, quotations, replies, forwards, and excerpts as potentially **nested or related evidence**, like Russian nesting dolls, not simply independent flat files.

An email may contain a body, quoted prior messages, attachments (PDFs, images, audio/video or other files), URLs, and references to other records. A PDF may have pages, sections, paragraphs, tables, figures and user-identified regions. A WhatsApp message may have text, media, documents, quoted/replied-to messages, or links. A forwarded item has both its original artifact/content and the **new communicative act** that forwarded it. A link alone is a reference, not proof that the remote contents were fetched, retained, or unchanged.

Required semantic abilities (implementation-specific representation):
- Preserve original artifact identity/bytes when supplied and permitted, digest, format, collection origin, accessible parent/child relationships, and provenance.
- Permit nested objects and **relationships between objects** to receive human descriptions, claims, investigative requests, or competing hypotheses—not just individual documents.
- Represent relevant relationships such as *contains*, *attached to*, *quotes*, *replies to*, *forwards*, *references*, *derived from*, *described by*, and *examined in context*. The list is open.
- Distinguish original author/speaker, forwarder, recipient, importer, describer, claimant, operator and model. These roles may differ and identities may be unresolved.
- Preserve events and the relevant order/time/scope, including uncertainty and discrepancies, without claiming exact timestamps or identities are authenticated.
- Keep shared ancestry/dependence visible; multiple attachments, re-shares, excerpts and derived reports are not automatically independent corroborations.

Avoid unnecessary graph-model complexity or universal schemas if a simpler bounded composition is sufficient for the MVP. Design with clean extension points rather than locking evidence into a single message/file abstraction.

## 3. User-driven interpretations of content and *relationships*

The user may explicitly associate an interpretation, accusation, concern, remembered circumstance, investigative goal, or uncertainty with an **artifact, internal element, communicative act, or relation between several elements**. Keep the human claim separate from the cited source and record what supports the attribution.

Examples:

- An email says **"Espero agora ter sido claro."**, contains several PDFs and images; the recipient answers only **"OK"**. A user may interpret the first text as irritation or an insult and the brief reply as avoidance or annoyance. Those are **hypotheses about the interaction**, not facts contained in either isolated text. "Ghosting" would require subsequent communication history and appropriate collection scope; it cannot be inferred from "OK" alone.
- The **same CTO directive PDF** is forwarded by one person allied with the CTO and by someone whose bonus was just denied. The forwards are separate communicative acts. One *might* express endorsement or appeal to authority; the other *might* express sarcasm, irreverence, protest, neutrality or something else. The alleged alliance/bonus denial must themselves be sourced, human-attributed, or marked unknown. Neither forwarding act has a uniquely determined meaning.
- A user may say: **"O gráfico de pizza da página 7 deve ser usado como evidência de projeção exagerada de aumento de receita."** The original PDF and page 7 should remain locatable. Record the user's claim/interpretation; do **not** assert that Niccolò saw the graph or independently verified the projection. The user can later add "o gráfico mostra 40%, mas a diretoria falava em 12%" as another attributable, challengeable claim.

Support **alternatives and disagreement**: the user may revise or reject an interpretation; Niccolò may propose counterhypotheses or request discriminating evidence without overriding the user or rewriting the earlier record.

## 4. MVP handling by input type

### Email and WhatsApp

Email and WhatsApp are high-priority starting channels, **not evidence-completeness guarantees**. A message with attachments or links may compose a larger evidence object. A chat export without accompanying media is an incomplete subset of a conversation; it must not imply full coverage.

Current native parsers are bounded: UTF-8 plain-text `.eml` and WhatsApp `.txt` exports, with limitations described in the [first-user walkthrough](../product/FIRST_USER_MVP_WALKTHROUGH.md) and actual code. Do not imply existing MIME attachment parsing, automatic resolution of URLs, or embedded-media extraction. The MVP contract calls for a way to preserve, relate and describe supporting material and mark omitted/unavailable children. The implementation may use explicitly associated user uploads and/or feasible native attachment unpacking where justified. Reassess technical timing rather than presuming that a complete email MIME engine or universal WhatsApp parser must be built first.

### Images and video

**Automatic image/video interpretation is OUT OF MVP scope.** An image, screenshot, video or visual portion may be preserved as **supporting evidence** where authorized, linked to its parent/email/chat/PDF/event and to user-created descriptions/claims. The user supplies and may revise the description and relevance. Niccolò reasons over these attributed descriptions and other recorded context, not unseen visual contents. Do not present such text as machine-perceived visual observation.

Automatic OCR, computer vision, video understanding and media-derived sentiment/intent detection are not prerequisites. A description may be incomplete or wrong; keep it challengeable.

### PDFs

Target **bounded, deterministic text extraction**, retaining the PDF original and page/offset/locator references as far as reliable. Where feasible, recognize **candidate section structure** from layout/text clues (headings, page breaks, typographic cues); label structure uncertain if ambiguous. No initial semantic classification or presumption that a heading represents validated fact, obligation, complaint, etc.

Show the user intelligible extracted text/sections (e.g. **"DÚVIDAS"** and **"COMPROMISSOS"**), permit selecting a page/section/paragraph or identified element and choosing what Niccolò should investigate: suspected allegations, whether commitments were honored, comparison with other sources, or context only. The system need not ask the user to classify every section; free-form investigative instructions are acceptable.

**Scanned or image-only PDFs** cannot be interpreted automatically by the MVP. A user may identify a figure/chart/page and describe its meaning while the source remains attached and locatable. A PDF can itself be an attachment inside an email or WhatsApp export. Complex layouts, extraction uncertainty and missing text must be disclosed; never pretend extraction was complete or accurate when it was not.

### Other files, audio, links, and external conversions

Files not natively understood may be retained/linked as supporting artifacts with **human-authored descriptions**, provided the implementation explicitly supports that storage/attachment path. Audio transcription, OCR, video analysis, universal document conversion and remote link fetching are **not implied MVP commitments** by this contract. Optional user/operator-supplied transcriptions or conversions must disclose who produced them, their source, transformation and omissions. A linked external resource is not an ingested or authenticated source unless explicitly retrieved and preserved through an authorized workflow.

## 5. Investigation steering and UX

User-facing flows should support the following **intent**, with the least complicated viable interaction:
1. Import a source and, where available, its related/attached artifacts; preserve missing-attachment indicators rather than implying completeness.
2. Inspect or navigate a useful source outline: message/body, attachments, link references, document pages/sections and relevant events.
3. Select any supported object, location, or relation; describe what the user believes, remembers or wants checked. Do not require a formal ontology or exhaustive labeling.
4. Ask Niccolò to compare evidence and challenge a proposition under the user-guided circumstance domain.
5. Surface competing hypotheses, unknowns, source/collection gaps and the precise boundary between extractable source text and attributed human interpretations.
6. Let the user correct, retract, expand context, exclude material, or approve proposed circumstance-domain expansions, while preserving immutable analysis history.

Meaning and authority can be **relation-specific and context-dependent**. Do not infer the sender's endorsement merely from forwarding; do not diagnose emotional states from brevity or punctuation.

## 6. Provenance, trust, and security invariants

- Source-integrity hashes prove continuity with stored bytes, not historical authenticity, completeness, truth or authorship.
- Preserve original bytes when collected; protect access, retention and explicit export. Original binary bytes must not be silently included in public packages or Git publication.
- Authenticated upload permission does not attest to source truth or independent corroboration.
- Never promote a user's description of unprocessed media to verified machine perception.
- Capture collection/sampling/exclusion and transformation context as available; make missing source families and absent attachments visible.
- A frozen analysis must identify what it actually examined and which approved human circumstances shaped it. Reanalysis creates a new lineage, not a retroactive change.
- The system should not autonomously inspect private links, third-party cloud resources, or attachments outside explicit consent and resource/security limits.
- Preserve tenant/workspace boundaries, fail safely on malformed input, bound file sizes and nesting, and guard against zip bombs, parser abuse, unsafe external references and prompt injection within imported content.

## 7. Minimum validation and acceptance directions

Tests should address not only parser correctness but also *epistemic behavior*:
- Original email/chat plus referenced/attached objects remain distinguishable and traversable; absent attachments are not invented.
- Different forwards of an identical PDF have distinct contextual events and can hold incompatible, attributed interpretations.
- A user can attach a supporting image or inaccessible visual portion and describe it; no output asserts Niccolò visually interpreted it.
- A text PDF yields page/section candidates and a human-directed scope selection; ambiguous/failed extraction remains explicit.
- "OK" and the preceding message do not establish insult, agreement, anger or ghosting without qualified evidence.
- Users and models cannot silently turn suspicion into fact or claim completeness from a partial chat/mail/PDF export.
- Relationships are not counted as independent corroborating documents merely because multiple copies or descendants exist.
- Changed circumstances or human interpretations cause reviewable new analysis/revisions, not silent history rewrites.
- Ingestion/structure/authorization limits and cross-workspace isolation are tested.

Add adversarial variations, real-user observations, and independent reasoning-quality evaluation where appropriate. Test contracts must not pre-seed a desired story ending.

## 8. Future development and documentation hygiene

At the outset of a relevant commit, milestone or feature slice, inspect this contract. Ask whether current work is an opportune point for:
- a data-model/API migration/refactoring;
- file attachment composition and source identity;
- user descriptions/claims attached to elements and relationships;
- PDF extraction and selection UX;
- circumstance-domain steering;
- targeted regression/acceptance tests.

Maintain a short trace in the relevant PR/working notes for meaningful implementation, deferral or deviation decisions. **Do not create an obligatory commit per requirement**, or inflate the architecture solely to satisfy an illustrative list. Keep this document current when the design improves; record material scope changes transparently. Update the README's implemented-versus-planned distinction and first-user instructions **only when the behavior is actually working and tested**.

**Not allowed as an excuse for deferral:** "the MVP must first be almost finished." **Also not required:** immediate implementation regardless of technical opportunity.

---

This contract defines desired **MVP capability and epistemic boundaries**, not proof that any particular parser, UI, attachment store, or test already exists.
