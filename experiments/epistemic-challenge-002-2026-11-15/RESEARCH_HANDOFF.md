# Research checkpoint and priority change

## Pause the epistemic benchmark series

Challenge 002 is complete. Its frozen protocol, all 400 evaluations (200
actual local Qwen calls), audit, and validation tests are committed and pushed.
The pull request remains separate from the MVP implementation.

Challenge 003 has resumed in its own [experiment directory](../epistemic-challenge-003-2026-11-15/README.md).
Preserve the proposed deterministic-first routing experiment and its limitations.
Do not treat Challenge 002's exploratory 84/100 score as an independently replicated result.

## DeepSeek participation

Include DeepSeek in future model-assisted research whenever the provider is
permitted, available, and within the authorized budget. It is a first-class
participant rather than an afterthought or a fallback reserved for failure.
The exact role, call allocation, and comparison design remain experiment
specific. No provider receives data outside the user's authorized disclosure
boundary.

## Optional research direction: pervasive pseudonymization

Investigate a reversible, semantics-preserving pseudonymous representation
of Situation data independently of the resumed epistemic benchmarks. The motivating
example is replacing a real name such as John Doe with an invented, natural-
looking alias such as Cornelius Stevenson, rather than deleting the reference.
This is a user-proposed design hypothesis, not a finalized specification.

**Product scope: optional MVP TODO, not a release blocker.** Favor the
low-hanging fruit: consistent aliases for readily recognized identifiers,
with minimal cost and loss of reasoning utility. For example, Chen may become
Tang rather than receiving an alias that introduces unrelated cultural cues.
Exhaustive coverage and perfect anonymization are not acceptance criteria.
Research priority is independent of the MVP release schedule: implementation
may be partial or deferred entirely without blocking the MVP. See the
[planning TODO](../../dev-flow/planning/README.md#optional-mvp-todo-pragmatic-pervasive-pseudonymization).

Potential scope includes people, organizations, projects, locations, unique
identifiers, filenames, email addresses, domains, quoted passages, document
metadata, model prompts and outputs, and text-bearing derived artifacts. The
original evidence should remain immutable and available to authorized humans.
Relations, repeated references, chronology, attribution, provenance,
negation, and relevant context must survive the transformation.

Research questions:

* Stable identity mappings within an authorized Situation, including nicknames,
  pronouns, spelling variants, and aliases; prevent accidental collisions
  and cross-Situation linkage. Explore provider-specific alias namespaces
  where consistent with the intended reasoning workflow.
* Keep reversible mappings and original evidence on the trusted side of
  a disclosure boundary. Ensure raw names and other identifiers do not leak
  through prompts, retrieval, metadata, logs, intermediate artifacts, or
  model-generated requests. Treat secrets/credentials separately.
* Preserve cultural, linguistic, professional, and organizational context
  when legitimately relevant. Do not assign demographic or cultural meaning
  based on the invented alias; a plausible alias must not introduce a new
  personality, ancestry, or social status into the reasoning model.
* Distinguish categorical identity replacement from values whose exact
  magnitude, formatting, spelling, timing, or wording is evidence. Explore
  consistent transformations for dates and quantities only when their
  required relationships remain intact; do not silently change facts.
* Evaluate the privacy/utility tradeoff, including identifying combinations
  of circumstances, distinctive quotations, and uncommon events. Replacement
  of direct identifiers alone does not make data anonymous. Test against
  realistic re-identification attempts and accidental disclosure.
* Evaluate deterministic recognition first, local model-assisted entity
  discovery when needed, human correction, and safe failure/abstention before
  any external model call. No automatic transmission of original sensitive
  text to an external model for the purpose of discovering what to protect.

Suggested experiments: compare original and pseudonymized reasoning results
on wholly synthetic Situations; measure entity consistency, leakage,
relationship and evidence preservation, alias-induced reasoning bias, and
cost. Determine what transformations are safe and when a local-only path
or explicit human decision is necessary.

References for terminology and threat modeling:

* NIST SP 800-188, *De-Identifying Government Datasets: Techniques and
  Governance*, https://doi.org/10.6028/NIST.SP.800-188
* European Data Protection Board, *Guidelines 01/2025 on Pseudonymisation*
  (consultation text), https://www.edpb.europa.eu/documents/guideline/guidelines-012025-on-pseudonymisation_en

No implementation or promise of anonymization is implied by this handoff.
