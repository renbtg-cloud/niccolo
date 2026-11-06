#!/usr/bin/env bash
set -euo pipefail
cargo test --offline --test milestones_031_038
cargo test --offline --test milestones_057_070
cargo test --offline --test milestones_071_080
cargo test --offline --test milestones_081_090
cargo test --offline --test milestones_091_100
cargo test --offline --test milestones_101_140
cargo test --offline --doc
cargo clippy --offline --all-targets -- -D warnings
cargo fmt --all -- --check
