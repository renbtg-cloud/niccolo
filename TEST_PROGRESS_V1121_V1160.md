# Test progress — v1.121-v1.160

Status: full regression in progress.

Implemented and committed before validation:
- backend reasoning-input observability endpoint;
- frozen revision, included/total source fragment counts, included source bytes, truncation flag and reasoning-packet SHA-256;
- first-party Human Review presentation with explicit truncation warning;
- permanent backend and frontend tests.

Validated so far:
- focused frozen-source/reasoning-input integration: 0/2 failed; 2/2 passed;
- frontend: 0/32 failed; 32/32 passed across 8 test files;
- production frontend build: passed;
- rustfmt check: passed;
- strict Clippy with pg-integration: passed;
- full Rust/PostgreSQL regression has reported no failure through the integration suites reached before the execution window cut the command.

This is a five-minute progress checkpoint, not DONE.
