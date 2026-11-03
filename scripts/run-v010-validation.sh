#!/usr/bin/env bash
set -Eeuo pipefail
cd "$(dirname "$0")/.."
: "${NICCOLO_PG_ADMIN_URL:?disposable test DB owner URL}"
: "${NICCOLO_PG_SERVICE_URL:?legacy restricted test role URL}"
: "${NICCOLO_PG_SECURE_URL:?dedicated niccolo_api inherited restricted role URL}"
cargo fmt --all -- --check
cargo clippy --offline --all-targets --features pg-integration -- -D warnings
cargo test --offline --all-targets --features pg-integration
cargo run --offline --quiet --bin niccolo-source-demo -- eml fixtures/v006/simple.eml
cargo run --offline --quiet --bin niccolo-source-demo -- whatsapp fixtures/v006/chat.txt
