# Niccolò: local Qwen3-1.7B epistemic reasoning pilot (2026-11-14)

## Research status

First *real*, fully offline CPU inference pilot on synthetic epistemic tasks. **Exploratory, developer-visible, not blind and not a claim of generalization.** This is a mathematical research harness, not a Niccolò MVP feature or a proof that any LLM is reliable for human investigations.

The target architecture hypothesis is that a small language model might perform **limited semantic interpretation**, while a deterministic epistemic component maintains and reasons over the resulting structures.

## Model, runtime and environment

- Model: `Qwen3-1.7B-Q4_K_M.gguf` (1,282,439,264 bytes), SHA-256 `d2387ca2dbfee2ffabce7120d3770dadca0b293052bc2f0e138fdc940d9bc7b5`.
- Model was reconstructed from 16 files previously present in the user's ChatGPT Library. No download, DeepSeek API, or user PC activity was required.
- Sandbox: 5 visible CPU cores, **4 GiB cgroup memory ceiling**, no GPU, and no external DNS resolution.
- We reused the **Spititout Dodge Viper v0.6 experimental Python/PyTorch GGUF CPU forward**, included under `runtime/` for reproducibility. It is **not** a verified reference implementation of llama.cpp or ggml, and numerical compatibility has not been established.
- Quantized Qwen weights are **NOT included**. The research runtime produces an approximately **3.8 GiB FP16 dequantization cache** on first use.
- A smoke test actually generated `4` for `Quanto é 2+2?` in approximately 25 seconds including cache preparation. That only proves a working local inference path.

## Experiments and raw data

| Experiment | Cases | Result | Limitations |
| --- | ---: | ---: | --- |
| First-token epistemic QA, first prompt design | 10 paired prose/formal | 3/10 prose, 3/10 formal | The model answered `A` in every trial: strong format/label bias. |
| Revised epistemic QA: actual system role, per-case rotating answer labels | 10 paired prose/formal | **5/10 prose, 5/10 formal** | Still 14/20 `A` choices. No advantage found for formal notation. |
| Outermost-operator parsing, initial labels | 15 | 9/15 | The letter `B` was overloaded as both `BELIEF` category notation and answer option, confounding the experiment. |
| Outermost-operator parsing, corrected disjoint `X/Y/Z` options | 15 | **13/15** | Five knowledge and five communication cases correct; three of five belief cases correct. All cases synthetic and prompt-edited. |

Raw per-case prompts, answer-option mappings, forced-choice logits, predicted labels, and elapsed time are in `results/`. The logits are **not calibrated probabilities**.

**Important distinction:** the core epistemic QA task required the model to perform logical deduction and yielded only 5/10. In contrast, a simpler first-stage task—recognizing the main claim's operator—yielded 13/15 after prompt/label correction. These are **different tasks**, not a controlled head-to-head measurement of a complete architecture. The observed improvement between parser versions changes several prompt details simultaneously; it is not an isolated causal estimate of the symbol-collision effect.

## Deterministic mathematical oracle

The Python fixtures use a deliberately small modal-logic fragment:

- `K(a, phi, t)`: actor `a` knows proposition `phi` at time `t`.
- `B(a, phi, t)`: actor `a` believes proposition `phi` at `t`.
- `NOT(phi)`: explicit logical negation.
- Factivity: `K(a, phi, t) -> phi`; nested knowledge is expanded recursively.
- **No** inference from belief to truth, no default closed-world assumption, and no unstated temporal persistence.
- Answer labels: entailed (`YES`), explicitly negated (`NO`), or neither (`UNKNOWN`).

All **10** synthetic fixture labels were checked against that deterministic closure, which is expected by construction. This validates the test oracle's *implementation against its chosen semantics*, **not** the adequacy of those semantics for human investigation.

## Reproduce (offline, given weights and Python dependencies)

The following is an example run from the experiment directory. It requires Python with `torch`, `numpy`, and `regex` already installed. The full GGUF model must be present as a local file; no model download is performed.

```bash
python -m unittest -v runtime/test_gguf_runtime.py

OPENBLAS_NUM_THREADS=4 OMP_NUM_THREADS=4 python epistemic_qwen_pilot.py \
  --model /path/to/Qwen3-1.7B-Q4_K_M.gguf --cache /path/to/fp16-cache \
  --out /tmp/epistemic-pilot-results.json

OPENBLAS_NUM_THREADS=4 OMP_NUM_THREADS=4 python epistemic_qwen_pilot_v2.py \
  --model /path/to/Qwen3-1.7B-Q4_K_M.gguf --cache /path/to/fp16-cache \
  --out /tmp/epistemic-pilot-v2-results.json

OPENBLAS_NUM_THREADS=4 OMP_NUM_THREADS=4 python epistemic_qwen_parser_pilot_v2.py \
  --model /path/to/Qwen3-1.7B-Q4_K_M.gguf --cache /path/to/fp16-cache \
  --out /tmp/epistemic-parser-v2-results.json
```

The original versions of the two parser experiments are included because negative and confounded results are part of the research record.

## Interpretation / go-no-go

1. **Go for the next *small* research slice**: a typed semantic interface where an LLM extracts claims and actors and a verifier either accepts structured output or requests human review.
2. **No-go for unsupervised epistemic judgments**: neither 5/10 formal QA nor 13/15 simple operator classification justifies autonomous interpretation of real people.
3. **Do not fine-tune yet**: first establish a verified inference backend, independent blind cases, operator/agent/time extraction, end-to-end integration, model comparison and robustness under different label encodings.
4. A strong baseline must include trivial deterministic extraction methods (regular expressions or parsers) for synthetic templated statements; the LLM must earn its complexity on linguistically varied real or carefully generated data.
5. Keep the human authorized to reject or correct semantic mappings and downstream hypotheses; preserve provenance and the model/heuristic used at extraction time.

## User-PC activity

**None.** All computation and model-file reconstruction occurred in the assistant's Linux environment. This temporary environment is not guaranteed to persist across conversations, so model weights may need to be reconstructed from the Library again in another session.
