#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCRIPT="$ROOT/scripts/first-run-local-mvp.sh"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

mkdir -p "$TMP/repo/scripts" "$TMP/bin"
cp "$SCRIPT" "$TMP/repo/scripts/first-run-local-mvp.sh"

cat > "$TMP/repo/scripts/bootstrap-local-mvp.sh" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
mkdir -p "$ROOT/.niccolo-local"
printf 'export NICCOLO_DATABASE_URL=%q\n' 'postgres://runtime' > "$ROOT/.niccolo-local/local.env"
printf 'WORKSPACE_UUID=%s\nSITUATION_UUID=%s\n' \
  '11111111-1111-4111-8111-111111111111' '22222222-2222-4222-8222-222222222222' \
  > "$ROOT/.niccolo-local/identifiers.txt"
printf '%s\n' 'ncl_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa' \
  > "$ROOT/.niccolo-local/token.txt"
chmod 600 "$ROOT/.niccolo-local/local.env" "$ROOT/.niccolo-local/identifiers.txt" "$ROOT/.niccolo-local/token.txt"
printf 'bootstrap\n' >> "${NICCOLO_TEST_TRACE:?}"
SH
cat > "$TMP/repo/scripts/configure-local-model.sh" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
mkdir -p "$ROOT/.niccolo-local"
printf 'export NICCOLO_LLM_BASE_URL=%q\nexport NICCOLO_LLM_MODEL=%q\n' 'http://127.0.0.1:11434/v1' 'local-test' > "$ROOT/.niccolo-local/model.env"
printf 'configure\n' >> "${NICCOLO_TEST_TRACE:?}"
SH
cat > "$TMP/repo/scripts/start-local-mvp.sh" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
printf 'start:%s\n' "$*" >> "${NICCOLO_TEST_TRACE:?}"
SH
chmod +x "$TMP/repo/scripts/"*.sh

export NICCOLO_TEST_TRACE="$TMP/trace"

# Missing local build tools fail before asking for or validating database-owner credentials.
preflight_bin="$TMP/preflight-bin"
mkdir -p "$preflight_bin"
cp "$(command -v dirname)" "$preflight_bin/dirname"
for name in psql node npm rustc; do
  cat > "$preflight_bin/$name" <<'SH'
#!/bin/bash
exit 0
SH
  chmod +x "$preflight_bin/$name"
done
unset NICCOLO_PG_ADMIN_URL || true
set +e
PATH="$preflight_bin" /bin/bash "$TMP/repo/scripts/first-run-local-mvp.sh" --no-browser >"$TMP/preflight-out" 2>"$TMP/preflight-err"
preflight_status=$?
set -e
[[ "$preflight_status" == 1 ]]
grep -q 'Missing executable: cargo' "$TMP/preflight-err"
! grep -q 'database-owner URL' "$TMP/preflight-err"

# Non-interactive bootstrap requires the owner URL through the environment and never prompts into a pipe/log.
unset NICCOLO_PG_ADMIN_URL || true
set +e
bash "$TMP/repo/scripts/first-run-local-mvp.sh" --no-browser >"$TMP/no-admin-out" 2>"$TMP/no-admin-err"
status=$?
set -e
[[ "$status" == 2 ]]
grep -q 'run is non-interactive' "$TMP/no-admin-err"
[[ ! -e "$TMP/repo/.niccolo-local/local.env" ]]

export NICCOLO_PG_ADMIN_URL='postgres://owner:test@127.0.0.1:5432/db'

# Partial credential state must not trigger a second bootstrap over a database
# whose migrations/roles may already have been changed.
mkdir -p "$TMP/repo/.niccolo-local"
printf 'export NICCOLO_DATABASE_URL=%q\n' 'postgres://runtime' > "$TMP/repo/.niccolo-local/local.env"
: > "$TMP/trace"
set +e
bash "$TMP/repo/scripts/first-run-local-mvp.sh" --skip-model-config --no-browser >"$TMP/partial-out" 2>"$TMP/partial-err"
partial_status=$?
set -e
[[ "$partial_status" == 2 ]]
grep -q 'local bootstrap state is incomplete' "$TMP/partial-err"
! grep -q '^bootstrap
set +e
bash "$TMP/repo/scripts/first-run-local-mvp.sh" --no-browser >"$TMP/out" 2>"$TMP/err"
status=$?
set -e
[[ "$status" == 2 ]]
grep -q '^bootstrap$' "$TMP/trace"
! grep -q '^start:' "$TMP/trace"
grep -q 'reasoning model is not configured' "$TMP/err"

# The explicit prepare-only escape hatch starts without invoking model configuration.
: > "$TMP/trace"
bash "$TMP/repo/scripts/first-run-local-mvp.sh" --skip-model-config --no-browser
! grep -q '^bootstrap$' "$TMP/trace"
! grep -q '^configure$' "$TMP/trace"
grep -q '^start:--no-browser$' "$TMP/trace"

# Process-environment model configuration is sufficient and does not persist secrets.
: > "$TMP/trace"
export NICCOLO_LLM_BASE_URL='https://models.example.test/v1'
export NICCOLO_LLM_MODEL='test-model'
export NICCOLO_LLM_API_KEY='do-not-write-me'
bash "$TMP/repo/scripts/first-run-local-mvp.sh" --no-browser
grep -q '^start:--no-browser$' "$TMP/trace"
[[ ! -f "$TMP/repo/.niccolo-local/model.env" ]]
unset NICCOLO_LLM_BASE_URL NICCOLO_LLM_MODEL NICCOLO_LLM_API_KEY

printf '%s\n' 'PASS: first-run wrapper is bootstrap-idempotent, model-aware, non-interactive-safe, and does not persist process secrets.' "$TMP/trace"
rm -rf "$TMP/repo/.niccolo-local"

# Non-interactive first run must not silently start without a reasoning model.
set +e
bash "$TMP/repo/scripts/first-run-local-mvp.sh" --no-browser >"$TMP/out" 2>"$TMP/err"
status=$?
set -e
[[ "$status" == 2 ]]
grep -q '^bootstrap$' "$TMP/trace"
! grep -q '^start:' "$TMP/trace"
grep -q 'reasoning model is not configured' "$TMP/err"

# The explicit prepare-only escape hatch starts without invoking model configuration.
: > "$TMP/trace"
bash "$TMP/repo/scripts/first-run-local-mvp.sh" --skip-model-config --no-browser
! grep -q '^bootstrap$' "$TMP/trace"
! grep -q '^configure$' "$TMP/trace"
grep -q '^start:--no-browser$' "$TMP/trace"

# Process-environment model configuration is sufficient and does not persist secrets.
: > "$TMP/trace"
export NICCOLO_LLM_BASE_URL='https://models.example.test/v1'
export NICCOLO_LLM_MODEL='test-model'
export NICCOLO_LLM_API_KEY='do-not-write-me'
bash "$TMP/repo/scripts/first-run-local-mvp.sh" --no-browser
grep -q '^start:--no-browser$' "$TMP/trace"
[[ ! -f "$TMP/repo/.niccolo-local/model.env" ]]
unset NICCOLO_LLM_BASE_URL NICCOLO_LLM_MODEL NICCOLO_LLM_API_KEY

printf '%s\n' 'PASS: first-run wrapper is bootstrap-idempotent, model-aware, non-interactive-safe, and does not persist process secrets.'