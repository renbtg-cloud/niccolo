# Experiment 001 — Qwen under epistemic test-case framing

**Date:** 2026-11-15. **Scope:** research-only, fully local/offline CPU execution. **Not a Niccolò MVP implementation.**

## Question

Would presenting epistemic questions as programming-style test challenges improve a small model's results compared with conventional question answering? Can a model that identifies only an utterance's outermost operator feed a narrow deterministic epistemic evaluator?

This is a **Qwen-only preparatory experiment** for a possible later DeepSeek test-report workflow. We do not have DeepSeek API access in this experiment. We did not generate Rust source code, compile LLM-generated programs, or use the user's PC.

## Inputs and mathematical boundaries

Five synthetic actors recur across **15 independent cases**: Cheng, Mara, Nia, Omar and Priya. The cases involve the launch of an audit, contradictory beliefs, messages, nested knowledge claims, and changing knowledge across time. They are simplified analogues of an investigation-awareness scenario, **not** a verified inference about who knows Cheng is specifically being investigated.

We use a deliberately small language with factive knowledge `K(a,p,t)`, non-factive belief `B(a,p,t)`, non-factive report `R(sender,receiver,p,t)`, and explicit negation `NOT(p)`. `K(a,p,t)` entails `p`, including nested statements. Neither belief nor a report establishes `p` or the recipient's understanding. We never infer unknown facts or persistence of knowledge across times. Answers: `SUPPORTED`, `REFUTED`, `UNRESOLVED`.

An independent Python oracle determines the correct outcome. Fifteen test queries were **preselected before inference** and balanced: five supported, five refuted, five unresolved. The oracle initially had a defect when refuting negated queries; it was caught **before** the reported final run, fixed, and covered by regression tests. The initial run was discarded.

## Three treatments

1. **Direct**: Qwen receives the stipulated facts and asks for the case's epistemic outcome.
2. **Challenge**: the same case is framed as a computational challenge and accompanied by three *separate illustrative test examples*, formal reasoning boundaries, and the requirement to select a result code.
3. **Parser-plus-oracle**: twelve different simple English statements are classified by their outermost operator `KNOWLEDGE`, `BELIEF`, or `COMMUNICATION`. The Python oracle then evaluates a **single narrow factivity query**, using the Qwen-predicted outermost operator and **gold** agent, proposition, time, and nested-content scaffolding. This is **not** full semantic parsing and **not** the same set of fifteen questions.

Each model selection used constrained first-token logits for counterbalanced `X/Y/Z` answer labels. The model did **not** freely generate test explanations or JSON reports. The laboratory itself recorded every test definition, input, code mapping, model output, oracle result, and factivity trace. `CHALLENGE_OUTPUT_CONTRACT.md` proposes a **future** free-form structured report for DeepSeek, not an existing capability.

## Measured results

| Treatment | Correct | Total | Notes |
| --- | ---: | ---: | --- |
| Direct Qwen | **6** | 15 | 40.0%; baseline on paired cases |
| Test-challenge Qwen | **7** | 15 | 46.7%; two direct failures corrected, one direct success lost |
| Qwen outermost-operator classification | **10** | 12 | Knowledge 4/4; belief 2/4; communication 4/4 |
| Narrow Qwen + deterministic oracle composition | **12** | 12 | The two operator errors both mapped non-factive categories to other non-factive categories |
| Plain lexical-rule comparator on parser statements | **12** | 12 | Simple overt English verbs make this task easy for hard-coded rules |

The difference **7/15 vs 6/15 is not persuasive evidence of improvement**. Conditional exact paired test for the three discordant cases: two-sided p-value = 1.0; this is a tiny, synthetic, exploratory sample, not a power-calibrated superiority experiment. Direct and challenge have different prompt lengths (mean 212 vs 350 tokens) and different examples, so framing alone is not isolated causally.

The narrow hybrid's **12/12 is not evidence that Qwen adds value over simple code**; the lexical baseline also achieves 12/12. Importantly, a higher downstream score despite two operator errors illustrates that not all intermediate interpretation errors necessarily affect a given downstream question. Other questions would be sensitive to the distinctions that were lost.

## Reproducibility

- Real local model: `Qwen3-1.7B-Q4_K_M.gguf`, size **1,282,439,264 bytes**; SHA-256 `d2387ca2dbfee2ffabce7120d3770dadca0b293052bc2f0e138fdc940d9bc7b5` verified before running.
- Uses the previous laboratory's **experimental non-reference** Python/PyTorch GGUF runtime in `../epistemic-qwen-local-2026-11-14/runtime/`. No numerical equivalence to llama.cpp/ggml has been independently established.
- Linux CPU only; cgroup memory limit **4 GiB**; the runtime dequantizes a temporary approximately **3.8 GiB** FP16 cache to disk. Python requires `torch`, `numpy`, and `regex`. **Do not commit or share model weights or temporary caches.**
- Deterministic oracle contract tests: **7 passed**.
- Cases, constraints, prompts, first-token logits, expected and model-observed outcomes, per-case durations, and oracle steps are preserved in `results/challenge-001.json`.
- Recomputed statistics and the deliberately trivial lexical comparator are in `summarize.py` and `results/summary.json`.

From this folder in the GitHub checkout, after placing the same model in a local accessible location:

```bash
python -m unittest -v test_challenge_001.py
OPENBLAS_NUM_THREADS=4 OMP_NUM_THREADS=4 python challenge_001.py \
  --model /path/to/Qwen3-1.7B-Q4_K_M.gguf \
  --cache /path/to/private/temporary/cache \
  --out /tmp/challenge-001.json
python summarize.py /tmp/challenge-001.json --out /tmp/challenge-001-summary.json
```

## Scientific limitations and decisions

- **No claim that test-style framing improves Qwen:** only a 1/15 net increase, confounded by prompt length and examples.
- **No claim that the LLM writes reliable logs:** the challenge-001 harness, not Qwen, generates the persisted test report.
- **No claim of general language understanding or human investigative quality:** artificial, direct, English statements, no blind holdout set, no natural evidence with uncertain provenance.
- **No claim of end-to-end Qwen + oracle correctness:** the parser experiment relies on oracle-given inner structure.
- **The numerical model runtime is experimental:** replicate using a reference inference implementation before concluding anything about Qwen's actual capability.
- **Rust-generated algorithms remain deferred**, possibly post-MVP. Prefer test definitions, result reports, preserved explanations, and human-controlled epistemic boundaries.

**Next research priority:** a true structured-output experiment with a capable model (when permitted), separate independent input parser and verifier, a reserved blind holdout, and realistic ambiguous-language cases; compare against trivial parsing and well-designed direct prompting. This pilot supports further research, not architecture promotion.
