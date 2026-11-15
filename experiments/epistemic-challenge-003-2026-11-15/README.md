# Epistemic Challenge 003 — prospective deterministic-first routing

Research only; no production changes. This resumes the benchmark series after
the optional pseudonymization TODO was recorded. Challenge 002's 84/100 gated
score was post-hoc and is not incorporated into the following score.

## Frozen protocol (before inference)

40 fresh synthetic cases, 20 narrative pairs with two pivotal records each,
five families (conjunction, provenance, competing explanations, nested
epistemic states, temporal/personal-context updates). Public actor, time,
payload, rules and givens are controlled/delimited; the relation is natural
language with fresh paraphrases and ambiguity traps. Gold outcomes are
author-stipulated and validated against the unchanged Challenge 002 verifier.
These are not independent human research cases; provenance and explanatory
competition are simplified public rules, not actual source forensics.

Conditions:

1. Direct Qwen: whole case, four verdict choices.
2. Old lexical: Challenge 002 substring parser plus verifier; abstention adds no premise.
3. Lexical: anchored full-phrase recognition plus verifier, abstaining on unfamiliar wording.
4. Gated: lexical first, Qwen only for abstentions, with a fifth explicit ambiguous/abstain option.
   Stable record classifications are reused across the two revisions. The verifier recomputes each time.
5. Gated stable-only: same calls, but report ABSTAIN rather than a verdict if any of the five
   candidate relations on any model-routed record would change the verdict. This is deliberately
   conservative and does not measure correctness of lexical recognitions. ABSTAIN is not UNRESOLVED;
   it counts as incorrect for four-label accuracy, and is measured separately.
6. Budget-matched direct: in frozen shuffled order use direct model results up to the gated input-token
   budget, then lexical for remaining cases. This is a preregistered recombination of actually measured
   calls, not a separately timed run. The boundary is determined by observed token counts, not gold labels.

Primary comparison: gated vs lexical exact match on all 40 cases; secondary direct comparison,
false certainty (definitive verdict on UNRESOLVED/CONFLICT gold), both-revision correctness,
coverage, sensitivity, per-family score, actual model calls/tokens/forward seconds. No claim about
population-level performance. Risk of author/verifier co-design and template dependence remains.
No tuning of cases, prompts, parser or verifier after inference. The code and cases are committed
before the first forward pass. No hidden gold is passed to inference; outcome letter assignments
are frozen and approximately balanced per gold class. This is a local experimental GGUF runtime,
not reference llama.cpp; constrained initial-letter logits, not calibrated probabilities.

DeepSeek is a first-class participant when permitted, available and budget-authorized. The API
responded HTTP 401 and no configured DeepSeek API credential was present in this environment;
therefore no authenticated DeepSeek run is claimed. The same direct and parser tasks should be
repeated with DeepSeek once credentials are configured. No user-PC execution or generated Rust.

## Reproduce

```bash
python -m unittest -v test_protocol.py
OMP_NUM_THREADS=4 OPENBLAS_NUM_THREADS=4 python run.py \
  --model /private/Qwen3-1.7B-Q4_K_M.gguf --cache /private/converted \
  --out results/observations.json
```

Model SHA-256 is checked; every individual call and completed row is atomically persisted.
Do not commit weights or converted tensor caches.

## Completed results

All 40 cases evaluated across the six specified conditions (240 case-condition
outcomes), using **59 actual Qwen forward passes**: 40 direct and 19 routed
semantic classifications. The stable-only and budget-matched conditions reuse
observations under the frozen rules. See [observations](results/observations.json)
and [audit](results/audit.json). Seven protocol/result tests passed, including
rejection of tampered prompts and verdicts.

| Condition | Correct / 40 | Both revisions correct / 20 | False certainty / 20 | Explicit verdict abstentions | Model calls | Input tokens | Forward seconds |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Direct Qwen | 9 | 2 | 11 | 0 | 40 | 8,343 | 116.722 |
| Old lexical | 26 | 13 | 6 | 0 | 0 | 0 | 0 |
| Anchored lexical | 24 | 12 | 5 | 0 | 0 | 0 | 0 |
| Gated | 26 | 13 | 4 | 0 | 19 | 2,256 | 41.192 |
| Gated stable-only | 13 | 5 | 0 | 27 | 19 | 2,256 | 41.192 |
| Budget-matched direct | 23 | 9 | 4 | 0 | 10 | 2,076 | 29.491 |

The predeclared primary comparison was a modest **+5 percentage points**
over anchored lexical (two corrections, zero regressions), not a reproduction
of Challenge 002's exploratory 84% accuracy. Gated matched the old lexical
baseline's 65% accuracy while reducing its false-certainty count from six to
four. Against direct Qwen, gated improved accuracy by 42.5 points with 73.0%
fewer input tokens and 64.7% less measured forward time. The token-budget
matched direct policy scored 23/40 with slightly fewer input tokens than gated.
These are descriptive results for this controlled sample, not independently
replicated generalization or end-to-end latency measurements.

The model parser was correct on only **5/19** routed records, including three
of four deliberately ambiguous records. It abstained on 13/19. Of six
knowledge records and two explicit absence-of-knowledge records, it correctly
classified only one of each. This conservative classifier
mostly fails by withholding useful premises. The modest accuracy improvement
was confined to the nested family. Of six unchanged-gold pairs, direct Qwen
changed its answer on four; both lexical and gated changed on none.

The exhaustive sensitivity policy delivered 13 verdicts, all correct here,
and abstained on 27/40. This follows from the constructed candidate space
containing the stipulated gold and from correct anchored lexical recognitions
in these fixtures. It does not guarantee safety against incorrect lexical
recognition, missed entities, missing relations, or bad rules in real inputs.
The conservative policy is useful as a flag for clarification or selective
escalation rather than necessarily suppressing every uncertain result.

## Interpretation and next research step

The new run supports decomposition and selective model expenditure versus
direct use of this tiny model on bounded records. It does not establish that
the tiny-model fallback is materially superior to a reasonable deterministic
baseline. Relation extraction remains the bottleneck. Do not promote the
experimental parser to production. Preserve the two deterministic baselines
and the sensitivity flag, then test a more capable parser (including DeepSeek
when authorized) and compare against a stronger lexical front end on fresh,
less structured cases. Keep the pseudonymization TODO non-blocking.

Re-audit without inference:

```bash
python audit_results.py --report results/observations.json --out results/audit.json
python -m unittest -v test_protocol.py test_results.py
```
