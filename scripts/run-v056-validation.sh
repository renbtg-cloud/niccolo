#!/usr/bin/env bash
set -euo pipefail
: "${NICCOLO_PG_ADMIN_URL:?set NICCOLO_PG_ADMIN_URL}"
: "${NICCOLO_PG_SERVICE_URL:?set NICCOLO_PG_SERVICE_URL}"
cargo test --offline --features pg-integration --test milestones_031_056
cargo clippy --offline --all-targets -- -D warnings
cargo fmt --all -- --check
if command -v node >/dev/null 2>&1 && [ -f web/package.json ]; then
  (cd web && node --test)
fi
