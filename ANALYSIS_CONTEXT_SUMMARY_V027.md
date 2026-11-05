# v0.27 — Frozen analysis context: presence versus use

This read model answers a narrow question: which attributed human contributions were present in the exact frozen input, and which of those were explicitly recorded as used by that analysis?

It deliberately preserves the original contribution kind. `USER_REPORTED`, `HYPOTHETICAL`, and `INVESTIGATIVE_PREFERENCE` are not promoted to verified evidence. Presence is not use; use is not truth. Contributions added after the frozen input are excluded by construction.
