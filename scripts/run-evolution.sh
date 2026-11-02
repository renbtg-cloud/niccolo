#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
processes="${1:-8}"
reps="${2:-31}"
cargo test --offline
cargo clippy --offline --all-targets -- -D warnings
cargo run --offline --bin niccolo-evolution-demo -- --processes "$processes" --reps "$reps"
