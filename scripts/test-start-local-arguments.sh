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

# Reject mistyped browser-policy values before inspecting private files.
status=0
NICCOLO_OPEN_BROWSER='sometimes' bash scripts/start-local-mvp.sh >"$TMP/stdout" 2>"$TMP/stderr" || status=$?
[[ "$status" == 2 ]]
grep -Fq 'NICCOLO_OPEN_BROWSER must be' "$TMP/stderr"
[[ ! -e .niccolo-local ]]

# Explicit headless mode overrides the environment without interpreting it.
status=0
NICCOLO_OPEN_BROWSER='sometimes' bash scripts/start-local-mvp.sh --no-browser >"$TMP/stdout" 2>"$TMP/stderr" || status=$?
[[ "$status" == 1 ]]
grep -Fq 'Missing local bootstrap file:' "$TMP/stderr"

# A supported option must get past argument validation and reach the
# ordinary local-bootstrap preflight, which is intentionally absent here.
status=0
bash scripts/start-local-mvp.sh --print-only >"$TMP/stdout" 2>"$TMP/stderr" || status=$?
[[ "$status" == 1 ]]
grep -Fq 'Missing local bootstrap file: .niccolo-local/local.env' "$TMP/stderr"

# A failed model configuration must never replace a previously prepared link.
mkdir -p .niccolo-local
chmod 700 .niccolo-local
printf '%s\n' 'export NICCOLO_PG_SECURE_URL=postgres://runtime' > .niccolo-local/local.env
printf '%s\n' 'WORKSPACE_UUID=11111111-1111-4111-8111-111111111111' 'SITUATION_UUID=22222222-2222-4222-8222-222222222222' > .niccolo-local/identifiers.txt
printf 'ncl_%064d\n' 0 > .niccolo-local/token.txt
printf '%s\n' 'export NICCOLO_LLM_BASE_URL=https://models.example.test/v1' 'export NICCOLO_LLM_MODEL=test-model' > .niccolo-local/model.env
chmod 600 .niccolo-local/*
printf '%s\n' 'previous-valid-url' > .niccolo-local/prepared-url.txt

status=0
bash scripts/start-local-mvp.sh --print-only >"$TMP/stdout" 2>"$TMP/stderr" || status=$?
[[ "$status" == 2 ]]
grep -Fq 'Remote HTTPS model providers require NICCOLO_LLM_API_KEY' "$TMP/stderr"
[[ "$(cat .niccolo-local/prepared-url.txt)" == 'previous-valid-url' ]]

# After removing the invalid model settings, printing the prepared URL succeeds.
rm .niccolo-local/model.env
bash scripts/start-local-mvp.sh --print-only >"$TMP/stdout" 2>"$TMP/stderr"
grep -Fq 'Open: http://127.0.0.1:38080/?workspace=' "$TMP/stdout"
grep -Fq 'http://127.0.0.1:38080/?workspace=' .niccolo-local/prepared-url.txt

# An existing symlink must be replaced, not followed. Never overwrite the
# symlink target even if the caller has ordinary filesystem permissions.
printf '%s\n' 'do-not-touch' > "$TMP/unrelated"
rm .niccolo-local/prepared-url.txt
ln -s "$TMP/unrelated" .niccolo-local/prepared-url.txt
bash scripts/start-local-mvp.sh --print-only >"$TMP/stdout" 2>"$TMP/stderr"
[[ "$(cat "$TMP/unrelated")" == 'do-not-touch' ]]
[[ -f .niccolo-local/prepared-url.txt && ! -L .niccolo-local/prepared-url.txt ]]
grep -Fq 'http://127.0.0.1:38080/?workspace=' .niccolo-local/prepared-url.txt
if [[ "$(uname -s)" != MINGW* && "$(uname -s)" != MSYS* && "$(uname -s)" != CYGWIN* ]]; then
  [[ "$(stat -c '%a' .niccolo-local/prepared-url.txt)" == '600' ]]
fi
[[ -z "$(find .niccolo-local -maxdepth 1 -name 'prepared-url.txt.tmp.*' -print -quit)" ]]

# Without a compiled API the normal startup must not replace the URL that
# was already prepared for a previous instance, even when settings are valid.
printf '%s\n' 'last-working-url' > .niccolo-local/prepared-url.txt
status=0
bash scripts/start-local-mvp.sh --no-browser >"$TMP/stdout" 2>"$TMP/stderr" || status=$?
[[ "$status" == 1 ]]
grep -Fq 'Missing API binary:' "$TMP/stderr"
[[ "$(cat .niccolo-local/prepared-url.txt)" == 'last-working-url' ]]

echo 'PASS: startup validates arguments, model and binary before publishing a local URL.'
