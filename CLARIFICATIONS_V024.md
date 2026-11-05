# Niccolò v0.24 — targeted clarification provenance

## Goal

Preserve *why and from which frozen context* a clarification question was proposed.

## Contract

An authorized writer may propose a clarification question against one immutable analysis snapshot.
The persisted binding records the question, the explicit reason it could materially distinguish
interpretations, the proposing principal and the analysis ID. The analysis already freezes the exact
Situation revision/fingerprint it saw, so a later correction or focus change cannot make the old
question appear to have been motivated by future context.

Questions remain ordinary `investigative_questions` and therefore can use the existing feedback and
evidence-reference APIs. The clarification binding does not make the question, its premise, or a human
response into evidence. Identical retries are idempotent; changing text, reason or attribution under
the same binding is a conflict. Reads are bounded, authorization-scoped and cursor-paginated.

## Non-goals

This milestone does not decide when a model should ask a question, score information gain, infer the
answer, launch an investigation, or implement unresolved hypothesis-rejection semantics. No LLM call
is made.
