# Validation — v0.681-v0.720

FINAL STATUS: DONE

First-party source import is now a normal product flow rather than a raw API exercise.

Implemented:

- create a Situation from the UI when the configured Situation does not yet exist;
- edit the current Situation focus;
- select bounded .eml or .txt sources;
- upload the exact source body with the backend media type and source locator;
- attach the imported document to the current Situation;
- list the workspace source catalog and show which documents are attached;
- preserve the distinction between speaker labels, human context and verified identity/evidence.

Validation on the exact exported main snapshot:

- frontend: 0/20 failed; 20/20 passed across 5 test files;
- production frontend build: passed;
- source-upload boundary tests cover accepted formats, empty files, unsupported formats and the 4 MiB limit;
- client tests cover raw upload headers/body, source catalog listing and Situation attachment.

The Rust/backend code used by this flow was unchanged after the immediately preceding 0/391 full Rust/PostgreSQL regression.
