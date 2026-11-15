# Epistemic challenge output contract — research draft

This is a **research protocol**, not an implemented Niccolò MVP interface. It is meant to be given to a more capable reasoning LLM such as DeepSeek in subsequent experiments.

## Input boundary

Niccolò supplies a `challenge_id`, semantic rules, relevant source records with provenance, context boundaries, a question, an evaluation budget, and (when feasible) tests with independent expected results. The LLM may choose its mathematical procedure. It cannot invent source records or change the stipulated semantics.

## Proposed LLM report

Each response should contain:

- `challenge_id`, `method_id`, `method_summary`: what was attempted and why.
- `assumptions`: explicitly distinguishing **stipulated**, **inferred**, and **hypothetical**.
- `test_cases[]`: stable `test_id`, `case_description`, `input_refs`, `question`, and `expected` **only if independently provided**.
- `test_cases[].proposed_result`: one of `SUPPORTED`, `REFUTED`, `UNRESOLVED`, or `CONTRADICTORY`.
- `test_cases[].explanation`: a concise, checkable account of premises, steps, and unresolved alternatives.
- `test_cases[].execution`: `NOT_EXECUTED` until an independent tool or oracle has actually run the test, then `EXECUTED` with tool provenance.
- `test_cases[].verification`: `NOT_VERIFIED`, `AGREES`, or `DISAGREES`, assigned **by Niccolò's independent evaluator**, not by the LLM.
- `counterexamples` and `next_discriminating_tests`.

Outputs are hypotheses and candidate derivations unless verified. Passing formal tests certifies derivation against the supplied semantics; it does **not** certify that real-world premises or psychological interpretations are true.

## Persistence and iterative review

Niccolò, not the model, stores versions of the challenge, statements, result rows, explanations, oracle traces, and model/runtime identifiers. A subsequent call gets the minimum relevant, provenance-preserving subset and can revise an earlier result using its stable `test_id`. Old outcomes remain inspectable. A revised result is never silently substituted for a prior result.

## Reuse and cost

At the current stage, prefer model-produced method descriptions, test results and traceable explanations; **source-code generation, Rust compilation and program synthesis are deferred** and may fall outside the MVP. A human remains responsible for authorizing investigations and selecting operational policies.

## Note about experiment 001

The Qwen pilot here does **not** generate this report. It is limited to constrained first-token outcome selection, and the Python harness independently writes case definitions, observed choices, reference outcomes and oracle explanations. The contract above is a **future DeepSeek evaluation target** and should not be mistaken for an accomplished Qwen ability.
