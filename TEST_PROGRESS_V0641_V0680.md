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
