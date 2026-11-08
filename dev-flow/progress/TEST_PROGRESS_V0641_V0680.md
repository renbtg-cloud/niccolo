# Test progress — v0.641-v0.680

Checkpoint: real OpenAI-compatible provider transport.

Validated on the exact exported main snapshot:

- 0/3 provider-focused tests failed; 3/3 passed.
- Loopback TCP mock received a real HTTP POST through `curl`.
- Authorization was sent as a header; the provider URL did not contain the API key.
- JSON response mode was requested.
- Provider envelope parsing preserved prompt/completion usage and effective model.
- Niccolò structured synthesis content was parsed from `choices[0].message.content`.

Latest checkpoint:

- provider-focused tests: 0/3 failed, 3/3 passed;
- rustfmt check: passed;
- strict Clippy: passed.

Still required before DONE:

- `cargo fmt --all -- --check` — PASSED;
- strict Clippy (`--all-targets --features pg-integration -- -D warnings`) — PASSED;
- full Rust/PostgreSQL regression;
- frontend regression/build;
- API startup/configuration validation with provider configured and unconfigured.

This is a progress checkpoint, not a product-readiness claim.

Additional regression findings:

- An existing cancellation test exposed a version race by attempting cancellation while the operation could still be QUEUED. The permanent test now waits for the stable WAITING_FOR_HUMAN boundary before cancelling.
- Focused cancellation regression: 0/1 failed; 1/1 passed.
- Frontend dependencies were restored from the offline npm cache in the VM.
- Frontend tests: 0/16 failed; 16/16 passed.
- Frontend production build: passed.

The full Rust/PostgreSQL regression is being rerun from the corrected test suite.

Harness correction before the next full run:

- The first full regression attempt omitted `NICCOLO_NEXT_ADMIN_URL` and `NICCOLO_NEXT_APP_URL`.
- Historical milestone suites therefore fell back to the obsolete local default on port 55432 and failed for environment reasons.
- The corrected harness binds `NICCOLO_NEXT_ADMIN_URL` to the disposable admin database and `NICCOLO_NEXT_APP_URL` to the existing restricted `niccolo_next_app` role on PostgreSQL 17.
- This checkpoint does not count those harness failures as product/test failures.

A corrected full regression is starting from the same provider code plus the permanent cancellation-race test fix.

Corrected full regression:

- Rust/PostgreSQL: 0/391 failed; 391/391 passed across 42 test targets/results.
- The historical milestone suites used the explicit `NICCOLO_NEXT_*` URLs and the restricted `niccolo_next_app` role.
- Provider-focused TCP transport tests remained green inside the complete run.

Remaining before DONE: API startup/configuration behavior with no provider, incomplete provider configuration, and a configured mock-compatible provider.

FINAL STATUS: DONE

- API without provider configuration starts successfully and remains explicit about unavailable reasoning.
- Partial provider configuration is rejected at startup.
- Complete OpenAI-compatible configuration starts successfully.
- Rust/PostgreSQL: 0/391 failed; 391/391 passed.
- Frontend: 0/16 failed; 16/16 passed; production build passed.
- Strict Clippy and rustfmt checks passed.
