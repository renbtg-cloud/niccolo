#!/usr/bin/env bash
set -Eeuo pipefail
cd "$(dirname "$0")/.."
: "${NICCOLO_PG_ADMIN_URL:?Set owner connection for DISPOSABLE test database}"
: "${NICCOLO_PG_SERVICE_URL:?Set restricted application connection for same database}"
cargo fmt --all -- --check
cargo clippy --offline --all-targets --features pg-integration -- -D warnings
cargo test --offline --all-targets --features pg-integration
cargo run --offline --quiet --bin niccolo-source-demo -- eml fixtures/v006/simple.eml
cargo run --offline --quiet --bin niccolo-source-demo -- whatsapp fixtures/v006/chat.txt
