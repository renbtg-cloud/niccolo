#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --all -- --check
cargo test --offline
cargo clippy --offline --all-targets -- -D warnings
cargo run --offline --bin niccolo-dag-evolution-demo -- --processes "${1:-8}" --reps "${2:-15}"
