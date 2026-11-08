# Aurelius & Finch — operator-assisted MVP acceptance protocol

Status: **acceptance contract**, not a claim of end-to-end product readiness.
Companion specifications: [public use case](../product/USE_CASE_LAW_FIRM_BONUS_REVIEW.md)
and [full technical acceptance map](MAKE-LAWFIRM-USE-CASE-REAL.md).

## Objective and non-goals

Demonstrate the law-firm investigation's **functional reasoning path** with an
MVP Niccolò, a named external operator, and ordinary external tools when
necessary. Native implementation of every LF-001–LF-016 automation is not
required for this demonstration; the epistemic and evidentiary requirements
**cannot** be outsourced away.

Do not hard-code a guilty/innocent outcome or instruct a model to reproduce
the fictional ending. The scenario should remain recoverable from observations
and competing explanations. Allow both exculpatory and incriminating evidence
to change the investigator's conclusions.

Do not portray operator-prepared data as system-generated verified facts.
Do not confuse intact export bytes with authentic source records, or
corroborating reports with multiple independent witnesses.

## Actor policy

All named participants, including the old Niccolò, may be subjects of
observations, claims, questions, influence hypotheses and counterevidence.
Whether the subject is human, software, mixed, or unknown is a potentially
useful observed/inferred attribute, not a gate for investigation.

Internal state, configuration and machine reasoning traces are **optional
additional observations**. External prompts, outputs, actions and times suffice
to begin investigating the old Niccolò. The MVP must not require introspection
into how any observed actor works.

## Operator boundary

An external operator may collect records, copy exact bytes, annotate origin,
convert files into importable formats, perform explicit tool searches, construct
tables and prepare a dossier. The operator must not silently change a source,
convert conjecture into fact or falsely present an operator judgment as a
Niccolò output.

Every intervention has:
- an identifiable human operator and task;
- the original or declared upstream source;
- exact input and output artifact references and hashes where available;
- the transformation performed and any omissions;
- a record of unanswered collection or provenance questions;
- the Niccolò analysis revision that received the material.

A spreadsheet or signed intake log is acceptable as an external ledger for the
MVP if its contents are supplied to Niccolò as **attributed claims** and remain
inspectable outside it. This is a manual substitute for native metadata, **not**
a reason to pretend that Niccolò has implemented the metadata.

### Intake distinction (do not collapse these)

| Class | Example | Required treatment |
| --- | --- | --- |
| Original artifact available | contemporaneous email, chat export | preserve exact bytes, hash, locator and collection history |
| Attributed report | Elias's notes about an interview | retain the reporter and source-of-knowledge distinction |
| Prior machine output | old Niccolò declares Kessler was justified | treat as behavior/output of that actor, not proof that Kessler was justified |
| Historical operator assertion | Julian labeled an identity or declared a source complete | preserve as Julian's claim, not as authenticated identity/completeness |
| Current investigator assertion | Sofia or Helena offers an interpretation | preserve human attribution; do not upgrade because the author is an admin or CEO |
| Niccolò inference | model proposes coordination or innocence | retain hypotheses and provenance, never label as observed documentary fact |
| Missingness | expected folder was not collected | record expected family, scope, reason/status and custodian independently |

## End-to-end execution

1. **Create a fresh Situation.** The CEO's request defines the investigation,
   not its expected verdict. Freeze each analysis input as usual.
2. **Prepare an external source manifest.** Preserve the original old-Niccolò
   export/interaction logs as available. When conversion to .eml or chat text
   is necessary, log the conversion and retain the original separately.
3. **Import and attribute.** Include all source classes above. A conversation
   can contain the old Niccolò as a participant without requiring a new
   native software-identity ontology. Confirm or dispute identity mappings.
4. **Describe collection scope.** List missing periods, custodians, excluded
   source families, extraction decisions and uncertain authenticity as operator
   assertions. An empty retrieval result cannot establish a source never existed.
5. **Form an investigation portfolio.** Maintain at least H1–H8 (or equivalent
   explicit alternatives), preserving competing and coexisting explanations.
   Include innocent, configuration-error, selection-bias, malice, tacit
   incentive alignment and explicit-coordination alternatives.
6. **Trace documentary dependencies.** Treat three reports descending from a
   single interview or document as one known lineage, not three independent
   witnesses. Distinct known roots do not prove statistical independence.
7. **Reconstruct bounded timelines.** Make separate qualified tables for
   observed events, attributed awareness/belief, formal authority, claimed
   authority and inferred practical influence. Include time bounds and sources.
8. **Investigate discriminators.** Use native frozen-revision observation search
   or an authorized operator-controlled search. Record which hypotheses each
   search might support/challenge; preserve zero-result limitations. Prefer
   observations that change real decisions over merely interesting details.
9. **Question the investigator.** Evaluate Elias's collection choices and
   Sofia's interventions with the same evidentiary humility used for Julian.
   Neither provenance signatures nor external employment proves neutrality.
10. **Keep politics contextual.** Political affiliation is neither culpability,
    credibility nor motive without case-specific connecting observations.
11. **Challenge the preferred story.** Seek complete threads, contrary dates,
    independent records, missing families, alternative incentive mechanisms and
    self-implicating operator choices.
12. **Prepare divergent decision support.** Distinguish the outcome for Kessler,
    Julian, Camille, Tomas, Nadine and the bonus system. Report why different
    consequences may follow from different evidence. Humans own final actions.
13. **Assemble an adversarial dossier.** The operator can format a portable
    package while Niccolò provides inspectable claims, links, counterevidence,
    unknowns and provisional reasoning. Do not falsely call a manual dossier a
    native LF-015 exporter.
14. **Stop when decision-ready.** Document unresolved uncertainties and whether
    a plausible answer could materially reverse each contemplated decision.
    The human explicitly authorizes or refuses each consequential action.

## LF capability mapping for the operator-assisted demonstration

The third column names an **acceptable manual substitute for a functional
demonstration**, not native feature completion.

| LF | Required behavior | Operator-assisted route |
| --- | --- | --- |
| 001 | Forensic Situation export | operator gathers API exports, retained originals, manifests and an external completeness ledger |
| 002 | Re-import without trust upgrade | convert each imported historical machine/human assertion to an explicitly attributed source, with external object ID map |
| 003 | Collection-chain provenance | external step ledger (author/custodian/collector/transformer/importer) supplied as attributed source |
| 004 | Missingness and scope | separate inventory of expected/collected/excluded/unavailable families and selection decisions |
| 005 | Time-specific knowledge and relationships | qualified, sourced timeline with explicit known/unknown time ranges |
| 006 | Formal authority vs influence | two separate sourced tables, with influence expressly hypothesis-like |
| 007 | Durable competing hypotheses | existing candidate interpretations and assumptions plus operator-maintained H1–H8 portfolio if needed |
| 008 | Retrieval driven by hypotheses | model-guided or operator-issued frozen-revision search; log query, purpose and resulting fragment IDs |
| 009 | Discriminating question value | external comparison of possible answers, impacted hypotheses, cost and possible decision change |
| 010 | Historical machine-output lineage | external dependency manifest linking old run/output/derived summaries to new uses |
| 011 | Integrity vs authenticity | explicitly scoped hash/byte continuity statements and unknown pre-ingest authenticity |
| 012 | Operator self-investigation | include investigator/CEO/IT claims as attributable material, never privileged truth |
| 013 | Political neutrality | test swapped affiliations without altering evidential interpretation absent connecting facts |
| 014 | High-stakes handoff | operator verifies all required epistemic categories appear separately in the deliverable |
| 015 | Adversarial dossier | generate from persisted claims/source exports and a transparently attributed operator assembly log |
| 016 | Decision-relevant stopping | explicit decision matrix, possible reversals and unresolved uncertainties reviewed by Helena |

## Minimum adversarial acceptance fixture

A reproducible fixture must include the sixteen cases listed in
[MAKE-LAWFIRM-USE-CASE-REAL.md](MAKE-LAWFIRM-USE-CASE-REAL.md#end-to-end-synthetic-acceptance-fixture):
the misleading short email, shared interview ancestor, independent source,
wrong CEO statement, skewed collection, missing family, dependent machine
outputs, disputed identity, temporal change of authority, separate influence,
irrelevant political affiliations, decisive late fragment, cheap and costly
questions, and a valid hash without proven prior authenticity.

For each case, log **expected behavior**, **actual output**, cited evidence IDs,
the revision, operator acts, whether a contradiction was considered, and whether
a materially different outcome is still plausible. A mocked model can verify
plumbing but does not establish reasoning quality; use separate blind/adversarial
evaluations for the latter.

## Definition of functional acceptance

The demonstration is complete only when an independent reviewer can follow
the evidence-to-hypothesis-to-question-to-decision chain across all story-critical
episodes, inspect source/counterevidence and operator interventions, and
understand remaining uncertainties. Any missing critical behavior is recorded
as a failure, even if the operator produced the desired ending.

**Not acceptable**: a model merely recites the script; a correct verdict with
fabricated provenance; a chain of copied reports counted as independent proof;
the old Niccolò's output declared true on import; a plausible dossier that
cannot withstand source-by-source review; or a claimed 100% acceptance score
without actually exercising the path.

The **public narrative** ends precisely with:
“Economic crisis, huh, hotshot?”
Legal/bar consequences are implied by the earlier dossier and referral intent,
not dramatized afterward. That artistic boundary does not reduce dossier and
external-review acceptance requirements.
