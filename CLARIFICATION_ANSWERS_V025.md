# v0.25 — human clarification answers as new Situation context

A clarification answer is persisted as an attributed `USER_REPORTED` Situation contribution. It creates a new immutable Situation revision and never rewrites the frozen analysis input that motivated the question. It is not evidence.

Retries with the same answer identifier are idempotent only when actor and text match. Later corrections/retractions use the existing Situation contribution lifecycle, preserving the original answer event.
