#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

: "${NICCOLO_PG_ADMIN_URL:?disposable owner DB URL required}"
: "${NICCOLO_PG_SERVICE_URL:?disposable service DB URL required}"
: "${CARGO:=cargo}"
: "${NPM:=npm}"

export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-4}"
export CARGO_PROFILE_DEV_DEBUG="${CARGO_PROFILE_DEV_DEBUG:-0}"
export CARGO_PROFILE_TEST_DEBUG="${CARGO_PROFILE_TEST_DEBUG:-0}"
export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-0}"

echo '[1/7] Prove the normal API build excludes compatibility parsers'
"$CARGO" clippy --locked --offline --bin niccolo-api -- -D warnings

echo '[2/7] Run parser-free library tests'
"$CARGO" test --locked --offline --lib

echo '[3/7] Verify the parser-free artifact persistence contract'
"$CARGO" test --locked --offline --features pg-integration \
  --test parser_free_artifacts_integration -- --nocapture

echo '[4/7] Verify the parser-free HTTP boundary and frozen reasoning input'
"$CARGO" test --locked --offline --features pg-integration \
  --test parser_free_artifacts_http_integration -- --nocapture

echo '[5/7] Reject accidental restoration of retired HTTP byte routes'
if rg -n 'sources/pdf-preview|sources/\{document\}/pdf-preview|sources/\{document\}/original|post\(ingest\)|post\(add_situation_source\)' src/api.rs; then
  echo 'A retired upload, download, attachment, or parser route returned.' >&2
  exit 1
fi

echo '[6/7] Run the browser regression suite'
(cd frontend && "$NPM" test -- --run)

echo '[7/7] Build the parser-free production browser bundle'
(cd frontend && "$NPM" run build)

echo 'Parser-free restricted MVP validation passed.'
