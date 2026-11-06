# Test progress — v0.921-v0.960

Checkpoint: recorded reanalysis revision explanation.

Permanent backend, client and presentation tests were committed before validation.

Focused validation:

- rustfmt check: passed;
- strict Clippy with pg-integration: passed;
- reanalysis-revision HTTP test: 0/1 failed; 1/1 passed;
- frontend: 0/29 failed; 29/29 passed across 8 test files;
- production frontend build: passed.

Validated boundaries:

- only direct recorded reanalysis lineage is compared;
- Situation input deltas are reported as recorded deltas;
- claim lineage counts are reported from explicit lineage records;
- claims without lineage are not silently classified as change;
- input change does not prove the causal reason for output change.

Full Rust/PostgreSQL regression is starting now. This is a progress checkpoint, not DONE.
