# v0.101–v0.180 — human-owned review, decisions, and agency gates

This experimental slice extends Niccolò's inspectable Situation workflow without turning model output into authority.

## Included contracts

- v0.101–v0.105: human-owned review checklist with append-only DONE/REOPENED/WAIVED history and deterministic digest.
- v0.106–v0.111: explicit decision points and ordered options. Display order is not automatic ranking; selection is not execution or recommendation.
- v0.112–v0.120: explicit assumptions, assumption events, competing interpretations, and a deterministic human briefing. Assumptions are not facts and alternatives are not ranked automatically.
- v0.121–v0.140: attributed decision rationales, evidence gaps, human-owned Situation goals, and unranked next steps. Satisfying an evidence gap does not make a claim true; accepting a next step does not execute it.
- v0.141–v0.160: human-declared priorities, constraints with waiver/reinstatement history, projected option consequences, and stop conditions. A triggered stop condition blocks automation and requires human handling.
- v0.161–v0.180: source requests, purpose-bound claim assessments, explicit unresolved conflict sets, and deterministic handback to the human. Accepting a source request never fetches automatically; assessments do not establish truth; conflict sets remain unresolved until explicit human/model work changes them; handback returns control rather than delegating authority.

## Deliberate boundary

This slice does not implement the open `MVP-HYP-001` remembered rejection/deprioritization semantics. Human choices in one review do not silently become global future policy. Nothing here auto-starts analysis, fetches sources, ranks alternatives as truth, or executes user actions.
