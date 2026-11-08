# Discriminating questions in the MVP

Niccolò can attach structured **investigation-value metadata** to a clarification question. The metadata is deliberately about scheduling an investigation, not about truth.

A clarification may record:

- the live candidate interpretations whose plausible answers it could separate;
- bounded possible answer/observation classes and what each would change;
- a 0–100 decision-relevance heuristic;
- a 0–100 acquisition-cost heuristic;
- an optional cheaper alternative that may answer the same discriminator.

The model-facing reasoning contract can propose this metadata while generating clarifications. It remains machine-generated investigation structure, not evidence. The values are persisted under the same workspace, Situation and frozen analysis as the question.

`investigation_policy` uses the metadata to rank clarification questions. A cheap question with multiple materially different outcomes and high decision relevance can outrank a generic question. A costly low-value question can be demoted. Existing clarifications without metadata keep the older conservative scheduling baseline.

The score is deterministic and inspectable. It is **not** Bayesian information gain, a probability, a confidence in a hypothesis, or a truth score. Humans can still ignore the ranking, supply a cheaper route, or decline to pursue the question.

This capability is intentionally general. It does not know about Aurelius & Finch, lawyers, bonuses or misconduct. In the law-firm acceptance story it supports LF-009: asking a question because its possible answers distinguish live explanations or change a contemplated decision, rather than merely because the question is interesting.
