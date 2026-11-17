# Epistemic Challenge 004 — scale, complexity, selective reasoning

Research only. Challenge 004 tests whether the deterministic-first effect observed
in Challenge 003 survives a much larger and structurally harder frozen corpus.

Challenge 003 produced the following internal signal on 40 synthetic cases:

- DeepSeek Flash direct: 22/40 = 55%
- deterministic lexical baseline: 24/40 = 60%
- deterministic-first + selective DeepSeek Flash: 32/40 = 80%
- gated versus lexical: 8 corrections, 0 regressions
- false certainty: 2 gated versus 9 direct

Those are not population estimates. Challenge 004 exists specifically to stress
that result.

## Production intent

DeepSeek is a **surrogate for strong-model capability**, not the intended
production dependency of Niccolò.

For sensitive investigations, the target deployment is local inference in an
air-gapped environment. The product is intended to sell both analytical quality
and architectural confidentiality. API convenience, current workstation RAM and
latency are therefore not product assumptions.

Challenge 004 optimizes for inference quality and epistemic behavior first.
Performance engineering comes later.

## Frozen corpus

The committed generator and seed define exactly:

- 250 base situations
- 4 revisions/context states per base
- 1,000 evaluated cases
- 200 holdout cases from 50 complete base situations
- 250 cases of each verdict: SUPPORTED, REFUTED, UNRESOLVED, CONFLICT
- 10 structural families, 100 cases each
- 200 frozen high-reasoning cases
- 30 frozen max-reasoning cases

The raw generated corpus is reproducible and intentionally not committed as a
large redundant JSON file. Its canonical SHA-256 and the escalation-set hashes
are pinned in `FROZEN_MANIFEST.json`.

No authenticated model inference may be run unless the generated corpus matches
that manifest.

## Structural families

The corpus includes:

1. long inference chains;
2. conjunction requirements;
3. nested epistemic facts;
4. provenance-like source structure;
5. competing positive and negative explanations;
6. stale versus current temporal evidence;
7. personal/context-baseline changes;
8. irrelevant and contradictory noise;
9. deliberate deterministic-parser false-positive traps;
10. mixed high-entropy cases.

Across those families the generator varies actor count, record count, chain
depth, nested depth, noise density, contradiction density, semantic paraphrases,
stale evidence and parser difficulty.

The deterministic parser is intentionally not perfect. A small frozen set of
phrases causes deterministic false positives rather than abstentions. This is
important: the experiment must measure failure modes of the architecture itself,
not only failure modes of the LLM.

## Main conditions

### A — deterministic only

No model call.

### B — DeepSeek Flash direct

Whole-case inference using `deepseek-flash`, thinking disabled.

### C — deterministic-first + Flash

The deterministic parser/verifier runs first. Flash is called only for records
where the deterministic parser abstains. Identical record classifications are
cached across revisions.

### D — selective high

A frozen structural policy selects 200 cases without inspecting model output or
gold labels. Model-routed semantic records in those cases are reclassified using
thinking effort `high`, then the deterministic verifier recomputes the verdict.

A paired direct-high observation is also recorded on the same selected cases so
we can separate "more reasoning" from "better routing".

### E — sparse max

Thirty of the high cases are selected by the same pre-model structural ranking
for effort `max`.

Both parser-level max and paired direct-max observations are recorded.

Outside the escalation cohorts, the adaptive policy retains the cheaper result.
Thus the full-corpus policies are normal -> high -> max rather than three
unrelated benchmarks.

## Routing freeze

Escalation depends only on committed structure:

- chain depth;
- nested depth;
- record count;
- noise density;
- contradiction presence;
- double semantic gaps;
- deterministic parser traps;
- structural family;
- a stable case-id hash for tie breaking.

It never inspects gold labels or model responses.

## Holdout

Fifty complete base situations (200 cases) are frozen as holdout. Prompt,
parser, verifier and escalation rules must not be tuned against holdout results.

The first complete run should report development and holdout metrics separately.

## Cost discipline

The runner is resumable and atomically persists every completed API call.

Current experimental budget is intentionally limited. A run may be stopped at
any time and resumed without repeating completed calls. Expensive reasoning is
therefore selective by design, but cost is not allowed to redefine the
production architecture.

The runner must record prompt, completion and reasoning token usage per call.
Reasoning output is capped explicitly instead of using the API's much larger
default thinking budgets.

## Reproduce the frozen corpus

From this directory:

```bash
python -m unittest -v test_protocol.py
python protocol.py
```

`protocol.py` may generate `cases.generated.json` for local inspection.
That generated file is redundant and should not be committed unless a later
research decision explicitly changes this policy.

## Experimental integrity

Before the first authenticated DeepSeek call:

- generator, seed and manifest are frozen;
- routing policy is frozen;
- prompts are frozen;
- output parsing is frozen;
- holdout membership is frozen;
- high/max cohorts are frozen.

After inference starts, changes require a new experiment version rather than
silently mutating Challenge 004.

Raw API observations, audits and analysis outputs are experimental artifacts and
must be committed to this experimental branch. API keys and model secrets must
never be committed.
