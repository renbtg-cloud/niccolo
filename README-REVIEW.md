# README review — editorial notes and engineering-documentation follow-ups

This document records the editorial problems corrected in `README.md`, the treatment of historical implementation commentary, and verifications still needed before the README is considered authoritative. It is **not** a status banner for product users or an alternative product introduction.

## Editorial diagnosis of the previous README

1. **The opening described an internal Rust proof-of-concept, not Niccolò.** The former title included an internal version number and led with a narrow disclaimer, obscuring the user problem and the product's intended value.
2. **A changelog displaced the onboarding journey.** Sequential `v0.3`, `v0.4`, ..., `v0.250` sections filled the document with implementation chronology. Someone trying to understand the product, start it, or program against it had to excavate that information.
3. **Negative inventory dominated positive explanation.** Repeated “does not yet” and “still experimental” remarks interrupted capabilities and examples. Necessary distinctions about availability were not organized into a coherent coverage section.
4. **Developer guidance was scattered or obsolete.** The document offered milestone-specific validation commands without one clear dependency, database setup, API startup, frontend startup, and extension workflow.
5. **Planned product identity was underrepresented.** The product's wider intentions — contextual understanding, human-guided inquiry, proactive leads, multi-source reasoning, bounded controller, model evaluation — matter even when not complete. Omitting them creates an artificially small description.
6. **Chronology risked misleading the reader.** The title ended at `v0.250` while repository commits already contain further agentic development. A README should not rely on a static internal checkpoint for its product identity.
7. **Technical risk and test history were placed in the wrong document.** They need dedicated engineering/test/security records and should be linked only when operationally relevant.

## Intended README policy

`README.md` should let a prospective user, contributor or integrator answer:

- What is Niccolò and why would I use it?
- How do I work through a Situation, and what remains in my control?
- What exists today, and what major product capabilities are planned?
- What components are involved, how do I install and run them, and how do I test a change?
- Where do I add functionality, and what invariants must I preserve?

**Planned features are welcome and important**, provided the wording makes their status distinguishable from implemented capabilities. This distinction should be concise and structural rather than a repeated apology.

## What moved out of the README

The original README included detailed paragraphs about the implementation of each historical release (v0.3 through v0.250), per-checkpoint SQL changes, individual adversarial tests, and all of the following as interspersed disclaimers: lack of external frontier calls, no production-ready ingestion, lack of a complete security model, no full trust guarantee, absence of automatic actions, and synthetic-only test evidence.

These topics have a legitimate place in:

- focused `*_V*.md` technical records;
- migration specifications under `sql/`;
- reproducible scripts and test evidence;
- a future dedicated security/deployment guide;
- release notes / `CHANGELOG.md` if chronological narration is needed.

No technical risk should be concealed: critical operational boundaries remain visible in README setup and principles, with links to deeper explanations. The change is **placement and emphasis**, not erasure of the underlying facts.

## Follow-up verification before publishing

These are **documentation review tasks**, not claims that the features are broken.

1. **Database bootstrap:** confirm there is a single documented script to create owner/service/API roles and apply all migrations. If none exists, add or document one; the README currently describes the process at a high level rather than inventing a turnkey command.
2. **Newest implementation:** inspect the latest `main` source and confirm whether controller/model integration has advanced beyond the `MVP_ARCHITECTURE_V250.md` checkpoint; update coverage terminology without automatically assigning user-visible readiness.
3. **Frontend connectivity:** verify the development-server API URL configuration/proxy and document the exact supported path; do not imply `npm run dev` automatically connects to the API.
4. **Full test command:** verify the canonical all-checkpoints validation flow for the *current* repository head. Historical `run-v030-validation.sh` is a reproducible example, not necessarily the latest comprehensive suite.
5. **Real ingestion path:** document the supported first-party UI upload flow separately from backend source-upload endpoints; maintain a clear distinction between browser file selection and completed server ingestion.
6. **Operational deployment:** if deployment beyond loopback is planned, provide a dedicated guide for authentication, TLS edge, database role guarantees, audit, privacy, and retention instead of crowding the README.
7. **Product identity:** confirm the final public naming/positioning language. Avoid stating launch readiness or advertising an untested competitive advantage.

## Review acceptance criteria

- A newcomer understands Niccolò before encountering implementation caveats.
- The README includes major planned capabilities as well as current foundations.
- A developer can find prerequisites, backend startup, frontend startup, database roles/migrations, and test entry points.
- Technical statements do not claim unverified implementation or product readiness.
- Historic versions and elaborate validation evidence are discoverable by links, not repeated in the product narrative.
- This change set contains only `README.md` and the new `README-REVIEW.md`; no source, SQL, test or other file is modified or committed as part of the review.