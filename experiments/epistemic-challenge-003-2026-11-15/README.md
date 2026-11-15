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
