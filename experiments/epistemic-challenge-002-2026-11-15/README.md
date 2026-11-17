# Epistemic Challenge 002 — paired semantic interpretation and verification

Research only. This directory does not change Niccolò's MVP behavior.

## Preregistered protocol

Building on [Challenge 001](../epistemic-challenge-001-2026-11-15/README.md), compare
four systems on the same 100 frozen, synthetic, semi-structured records and
queries. The dataset comprises 50 narratives with an original and updated
evidence revision, in five families (nested epistemic states, contradictions,
competing explanatory evidence, personal contextual baselines, and time).
Ten reasoning patterns are reused across families. This is an exploratory
benchmark, not 100 statistically independent natural-language investigations.

* **Direct:** Qwen receives the entire public record, rules, query and four
  outcome choices.
* **Lexical:** fixed regex relation extraction, with abstention when an
  expression is unfamiliar, plus the deterministic verifier.
* **Hybrid:** Qwen classifies the relation of the public pivotal record,
  then the same deterministic verifier receives the parsed assertion.
* **Hybrid challenge:** identical hybrid with a test-style parser prompt
  containing explicit examples. This tests a prompt bundle, not the isolated
  effect of framing or model-generated tests.

The actor, timestamp and content are extracted from the public record's
delimited fields; no oracle-provided hidden scaffolding is used. These fields
are intentionally controlled, so this is not full free-text semantic parsing.
The record is stable across revisions; the hybrid reuses its relation
classification, but recomputes closure for each revision. Direct Qwen sees a
different complete input on each revision. Model calls and runtime are counted
accordingly. No model can see gold relations or expected outcomes.

The verifier implements forward-only factivity, explicit negation,
stipulated conjunction rules, and paraconsistent four-valued query outcomes.
Absence of knowledge does not negate the content. No persistence across times,
closed-world inference, or explosion from an unrelated contradiction.
Hypothesis support and anomaly indicators are stipulated rule conclusions,
not factual psychological diagnoses. The contextual baseline is personal and
human-approved; demographic membership is not a premise.

Outcome codes and parser codes are shuffled and approximately balanced within
each expected class. Each pair's parser mapping is held fixed over revisions
for cache reuse. Dataset and prompts are frozen in the initial Git checkpoint.
Declared outcomes are checked against the executable verifier before inference.
This is not a genuinely blinded human-annotated benchmark: the fixture author
also built the verifier and parser prompts.

Metrics: per-case accuracy, false certainty on unresolved/conflicting cases,
both-original-and-revised correctness, correct handling of changed versus
unchanged outcomes, false revisions, per-family breakdown, parser accuracy and
abstentions, model calls/tokens/time, paired discordance, and descriptive
pair-cluster bootstrap intervals. Comparisons are descriptive only given the
reused patterns and synthetic sample.

Real Qwen3-1.7B-Q4_K_M GGUF, SHA-256
`d2387ca2dbfee2ffabce7120d3770dadca0b293052bc2f0e138fdc940d9bc7b5`.
Experimental non-reference CPU GGUF runtime; constrained first-token W/X/Y/Z
logits, not calibrated probabilities nor free-form generated test reports.
No DeepSeek, generated Rust, real investigation, or user-PC execution.

## Run

```bash
python protocol.py
python -m unittest -v test_protocol.py
OMP_NUM_THREADS=4 OPENBLAS_NUM_THREADS=4 python run.py \
    --model /private/Qwen3-1.7B-Q4_K_M.gguf \
    --cache /private/converted \
    --out results/observations.json
```

Do not commit model weights or converted caches. The run saves each completed
row atomically and can resume using the same frozen file and model. Raw model
choices and verifier derivations are persisted for audit. The test definitions
and expected outputs are produced by this harness, not by Qwen.

## Results (completed)

All 400 case-treatment evaluations were executed, including **200 actual
Qwen forward passes**. The frozen cases SHA-256 is
`561440e8a2d3d37e10f2e2d6a64e9e7ad8a45ec3bc99d9565539a454aaa96e85`.
PyTorch 2.9.1+cpu; four CPU threads; model hash checked. The raw
[observations](results/observations.json) and recomputed
[audit](results/audit.json) contain all results and trace details.

| Preregistered system | Correct / 100 | Both revisions correct / 50 | False certainty / 40 | Model calls | Model seconds | Input tokens |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Direct Qwen | 29 | 5 | 14 | 100 | 306.691 | 21,171 |
| Lexical + verifier | 68 | 34 | 9 | 0 | 0 | 0 |
| Qwen parser + verifier | 57 | 28 | 18 | 50 | 110.173 | 6,230 |
| Challenge parser + verifier | 38 | 17 | 22 | 50 | 137.114 | 9,030 |

The standard hybrid improved over direct Qwen by 28 percentage points
(40 corrected cases, 12 regressions; descriptive cluster bootstrap interval
12 to 44 points). It did **not** beat the lexical baseline (11 points worse;
18 corrected, 29 regressions; interval -29 to +8 points). The challenge
prompt bundle reduced hybrid accuracy by 19 points (8 corrected, 27
regressions; interval -34 to -4 points). These intervals are descriptive of
the synthetic sample, not population-level claims of superiority.

### Behavior under changing evidence

| System | Both revisions correct when gold changed / 30 | Both correct when gold stayed unchanged / 20 | Spurious revisions / 20 |
| --- | ---: | ---: | ---: |
| Direct Qwen | 3 | 2 | 17 |
| Lexical + verifier | 21 | 13 | 0 |
| Qwen parser + verifier | 14 | 14 | 0 |
| Challenge parser + verifier | 10 | 7 | 0 |

The zero spurious revisions in these three verifier configurations follows
from stable record interpretation and the constructed irrelevant additions;
it is a property of this bounded protocol, not a claim about arbitrary
revisions in real investigations. False certainty means a definitive
SUPPORTED/REFUTED verdict on an UNRESOLVED/CONFLICT gold case.

### Why the hybrid failed against the lexical baseline

On the 50 unique pivotal records, the lexical parser found 24 correctly and
abstained on 26; its recognized cases were all correct. Qwen correctly
classified only 17/50, and challenge Qwen only 11/50; both were forced to
choose rather than abstain. All correct parses yielded correct downstream
verdicts. Some incorrect parses nevertheless yielded correct verdicts (23
revisions for ordinary hybrid, 16 for challenge hybrid). In 93/100 cases,
changing the assumed relation could alter the final verdict; seven were
invariant across all four relations.

This local Qwen classifier is not reliable enough to override recognized
deterministic relations. In particular, the ordinary parser confused six
belief records and four report records with factive knowledge. The verifier
cannot repair epistemically incorrect premises merely by being logically correct.

## Post-hoc exploratory routing — NOT part of the preregistered comparison

After the run, we recombined already recorded observations with a simple rule:
accept the lexical parser when it recognizes an expression; invoke Qwen only
when it abstains. No gold labels are used to route individual records. This
uses existing model predictions, not a new executed end-to-end treatment.

| Exploratory routing | Correct / 100 | Both revisions correct / 50 | False certainty / 40 | Model calls from existing observations |
| --- | ---: | ---: | ---: | ---: |
| Lexical then ordinary Qwen | **84** | **42** | **7** | **26** |
| Lexical then challenge Qwen | 76 | 37 | 8 | 26 |

The ordinary gated route used 26 distinct cached Qwen calls (56.909 model
seconds; 3,273 prompt tokens). On changed-gold pairs it got both revisions
right in 25/30, versus 21/30 for lexical alone. Its per-family scores were:
context 18/20, contradiction 20/20, hypothesis 18/20, nested 14/20,
temporal 14/20. Its 84/100 is 16 points above lexical alone and 55 points
above direct Qwen. The latter comparative cost is a model-compute estimate
from already measured calls, not a measured gated system wall-clock benchmark.

The route was selected after observing the experiment's data, so this is
**an exploratory hypothesis requiring a fresh frozen benchmark**. No
production promotion is justified. The lexical recognition set is limited,
the classifier has no abstention option, and the direct condition receives the
whole record whereas hybrid inference solves a narrower relation problem.
The experiment establishes an observed benefit from decomposition on this
bounded task, not superior general reasoning or real-world investigative truth.

## Next experiment

**Resumed:** see [Challenge 003](../epistemic-challenge-003-2026-11-15/README.md).
The [research handoff](RESEARCH_HANDOFF.md) preserves the optional, non-blocking
pseudonymization TODO. The following design remains the direction of the series.

Preregister deterministic-first routing with an explicit model abstention
option. Use new paraphrases and narrative patterns, including multiple
ambiguous evidence records, provenance dependence, competing explanations,
and temporal/contextual updates. Compare to a stronger deterministic
baseline and a cost-matched direct model. Record when a conclusion changes
under plausible alternative parses instead of unconditionally accepting a
single prediction. Repeat with DeepSeek when available, and eventually with
a reference model runtime. Keep generated Rust outside the current scope.

The 100 cases are controlled records, not free-form messages; the
hypothesis family evaluates a stipulated support indicator, not genuine
competition between complete explanatory models. The context family tests
explicit personal-baseline rules, not empirical cultural archetypes. Both
are intentional limitations.

Audit again without re-running inference:

```bash
python audit_results.py --report results/observations.json --out results/audit.json
python -m unittest -v test_protocol.py test_results.py
```
