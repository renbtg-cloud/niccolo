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

Results pending.
