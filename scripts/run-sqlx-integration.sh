#!/usr/bin/env bash
set -Eeuo pipefail
cd "$(dirname "$0")/.."
: "${NICCOLO_PG_ADMIN_URL:?point to disposable PostgreSQL database with schema applied}"
: "${NICCOLO_PG_SERVICE_URL:?point to a NONSUPERUSER, NOBYPASSRLS login with niccolo_app privileges}"
cargo fmt --all -- --check
cargo clippy --offline --all-targets --features pg-integration -- -D warnings
cargo test --offline --all-targets --features pg-integration
