#!/usr/bin/env bash
set -euo pipefail
: "${NICCOLO_PG_ADMIN_URL:?set NICCOLO_PG_ADMIN_URL}"
: "${NICCOLO_PG_SERVICE_URL:?set NICCOLO_PG_SERVICE_URL}"
export NICCOLO_PG_ADMIN_URL NICCOLO_PG_SERVICE_URL
cargo test --offline --test milestones_057_070 -- --test-threads=1
cargo test --offline --test milestones_071_080 -- --test-threads=1
cargo clippy --offline --all-targets -- -D warnings
cargo fmt --all -- --check
