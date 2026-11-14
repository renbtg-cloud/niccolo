# Global WIP/DONE lifecycle audit — 2026-11-14

**Scope:** all 108 remote heads fetched with full history on GitHub Actions, including `main`, `ci/mvp-checkpoint`, archives, and historical `tmp/*` refs. The audit uses first-parent histories and deduplicates by identical WIP commit SHA sequences. The original heuristic reported 315 branch-level sequences; these represent **six unique sequences**, including the audit workflow's own transient `WIP 001` which was subsequently reworded to `DONE:` (new SHA `a0d285e5d913`, lease-checked force ref update). **Five historical sequences remain** as of the reword.

A first-parent `WIP 001` restart is *not* conclusive evidence that the previous task was never finished; legacy work sometimes received its real `DONE` after unrelated checkpoints. The following is a semantic review, not a claim that 315 tasks are unfinished.

| Historical WIP sequence | Heads referencing it in initial snapshot | Verified later closure | Classification |
| --- | ---: | --- | --- |
| `d099bef50746` — epistemic read model v0.561–v0.600 | 79 | `e232dd162ea6` — **DONE: human-readable epistemic reasoning v0.561–v0.600** | Closed; numbering restarted prematurely |
| `52f4c978c20b` — register epistemic read model | 79 | Same `e232dd162ea6`, a descendant of this WIP | Closed; second `WIP 1` within the same logical unit |
| `6d9212fcbde9` … `d536a493b782` — human feedback v0.961–v1.000 | 79 | `02b0fb46bb7` — **DONE: explicit human feedback v0.961–v1.000** | Closed; unrelated work intervened before DONE |
| `fc148133314b` … `0d8d5b48d30d` — provider claim provenance v1.201–v1.240 | 75 initially | `93ac6134691` — **docs: close v1.201–v1.240 validation**, six commits after the last WIP | Implementation/validation closure recorded, but **no matching `DONE:`** — historical commit-message irregularity |
| `0993ae7ca93c` … `a261a6627c78` — human-intent BFF v0.391–v0.430 | One historical ref (`tmp-wip430-sync`) | Mainline descendant `d978de5f5a26` — **DONE: human-intent BFF launch flow v0.391–v0.430** | Historical mid-task snapshot; task completed on another line |

**Verified ancestry:** `d099bef5` and `52f4c978` precede `e232dd16`; `d536a493` precedes `02b0fb46`; `0d8d5b48` precedes `93ac6134`; `a261a662` precedes `d978de5f`. In every comparison, the latter is ahead and not behind.

## Safe remediation performed

- Reworded the audit's own single `WIP 001` into `DONE: Audit WIP/DONE lifecycle across all Git branches`, **without changing the tree or parent**; updated the branch using a force-with-lease equivalent (`expected_sha`).
- Did **not** reword immutable/shared historical commits: the first four sequences are inherited by `main`, `ci/mvp-checkpoint`, and dozens of archive/feature refs. Rewriting them would change thousands of descendant SHAs and disrupt open PRs, active CI and historical checkpoints.
- Did **not** mark the mid-task `tmp-wip430-sync` snapshot as `DONE`, because that snapshot predates the WIP4 work and its genuine DONE. Claiming it was complete then would falsify history.

## Going forward

The lifecycle invariant is per logical unit: `WIP 001`, `WIP 002` … `DONE: title`; the next unit restarts at `WIP 001`. Checkpoints are pushed approximately every five minutes. When the **only** commit of a fully completed unit is `WIP 001`, reword that exact commit to `DONE:` and lease-check any necessary force-push **before merge or distribution**. Preserve shared/merged history; prefer explicit audit annotations to destructive rewriting.

Audit evidence: GitHub Actions artifacts `all-branches-wip-audit` generated on the audit branch (including per-branch raw scan, deduplicated unique sets, and nearby mainline commit messages).
