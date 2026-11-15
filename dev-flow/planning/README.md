# Planning and deferred scope

Documents here record proposals, deliberate exclusions and older implementation plans. They are not delivered product commitments.

- [Deferred post-MVP capabilities](POST_MVP_TO_SPECIFY.md)
- [Epistemic weights rollout plan](EPISTEMIC_WEIGHTING_MVP.md)

## Optional MVP TODO: pragmatic pervasive pseudonymization

**Desirable for the MVP, but not mandatory and not a release blocker.**
Implement early if technically convenient; otherwise defer in whole or in part.
Prefer low-cost, high-impact coverage of readily recognized identifiers:
personal names, organizations, projects, email addresses, telephone numbers,
addresses, URLs, and document or contract identifiers. Replace references with
consistent, type-appropriate aliases (e.g. Chen becomes Tang), retaining
reasoning-relevant context without inventing cultural attributes. Keep the
mapping within the trusted boundary and apply protection at external model
egress, including derived text and metadata where practical.

Do not pursue exhaustive redaction or perfect anonymization at the expense of
reasoning quality, affordability, or MVP delivery. Measure residual disclosure
risk; pseudonymization does not authorize otherwise prohibited data egress.
Existing privacy, authorization, and credential-handling requirements remain
mandatory. See the [research handoff](../../experiments/epistemic-challenge-002-2026-11-15/RESEARCH_HANDOFF.md)
for the broader investigation.

See the [public README](../../README.md) for implementation status.
