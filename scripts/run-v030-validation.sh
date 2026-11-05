#!/usr/bin/env bash
set -Eeuo pipefail
cd "$(dirname "$0")/.."
: "${NICCOLO_PG_ADMIN_URL:?disposable owner DB URL required}"
: "${NICCOLO_PG_SERVICE_URL:?disposable legacy DB URL required}"
: "${NICCOLO_PG_SECURE_URL:?disposable dedicated API DB URL required}"
NODE_BIN="${NICCOLO_NODE_BIN:-node}"
cargo fmt --all -- --check
cargo clippy --offline --all-targets --features pg-integration -- -D warnings
cargo test --offline --all-targets --features pg-integration
"$NODE_BIN" --test web/state.test.mjs web/api.test.mjs
