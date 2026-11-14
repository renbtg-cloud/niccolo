#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

mkdir -p "$TMP/repo/scripts"
cp "$ROOT/scripts/start-local-mvp.sh" "$TMP/repo/scripts/start-local-mvp.sh"
cd "$TMP/repo"

assert_bad_option() {
  local expected="$1"
  shift
  local status=0
  bash scripts/start-local-mvp.sh "$@" >"$TMP/stdout" 2>"$TMP/stderr" || status=$?
  [[ "$status" == 2 ]] || {
    printf 'Expected exit 2; got %s for option %s\n' "$status" "$expected" >&2
    exit 1
  }
  grep -Fq "$expected" "$TMP/stderr"
  grep -Fq 'Usage: bash scripts/start-local-mvp.sh' "$TMP/stderr"
  [[ ! -e .niccolo-local ]] || {
    echo 'Invalid argument caused local state to be written.' >&2
    exit 1
  }
}

assert_bad_option 'Unknown startup option: --no-broswer' --no-broswer
assert_bad_option 'Usage: bash scripts/start-local-mvp.sh' --no-browser --print-only

# A supported option must get past argument validation and reach the
# ordinary local-bootstrap preflight, which is intentionally absent here.
status=0
bash scripts/start-local-mvp.sh --print-only >"$TMP/stdout" 2>"$TMP/stderr" || status=$?
[[ "$status" == 1 ]]
grep -Fq 'Missing local bootstrap file: .niccolo-local/local.env' "$TMP/stderr"

echo 'PASS: local MVP startup rejects bad/conflicting options before touching private state.'
