#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
SCRIPT="$ROOT/scripts/first-run-local-mvp.sh"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

mkdir -p "$TMP/repo/scripts"
cp "$SCRIPT" "$TMP/repo/scripts/first-run-local-mvp.sh"

cat > "$TMP/repo/scripts/bootstrap-local-mvp.sh" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
mkdir -p "$ROOT/.niccolo-local"
chmod 700 "$ROOT/.niccolo-local" 2>/dev/null || true
printf 'export NICCOLO_PG_SECURE_URL=%q\n' 'postgres://runtime' > "$ROOT/.niccolo-local/local.env"
printf 'WORKSPACE_UUID=%s\nSITUATION_UUID=%s\n' \
  '11111111-1111-4111-8111-111111111111' '22222222-2222-4222-8222-222222222222' \
  > "$ROOT/.niccolo-local/identifiers.txt"
printf '%s\n' 'ncl_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa' \
  > "$ROOT/.niccolo-local/token.txt"
chmod 600 "$ROOT/.niccolo-local/local.env" "$ROOT/.niccolo-local/identifiers.txt" "$ROOT/.niccolo-local/token.txt" 2>/dev/null || true
printf 'bootstrap\n' >> "${NICCOLO_TEST_TRACE:?}"
SH

cat > "$TMP/repo/scripts/configure-local-model.sh" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
mkdir -p "$ROOT/.niccolo-local"
printf 'export NICCOLO_LLM_BASE_URL=%q\nexport NICCOLO_LLM_MODEL=%q\n' \
  'http://127.0.0.1:11434/v1' 'local-test' > "$ROOT/.niccolo-local/model.env"
chmod 600 "$ROOT/.niccolo-local/model.env" 2>/dev/null || true
printf 'configure\n' >> "${NICCOLO_TEST_TRACE:?}"
SH

cat > "$TMP/repo/scripts/start-local-mvp.sh" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
[[ -z "${NICCOLO_PG_ADMIN_URL:-}" ]] || {
  echo 'bootstrap owner URL leaked into runtime startup' >&2
  exit 93
}
printf 'start:%s\n' "$*" >> "${NICCOLO_TEST_TRACE:?}"
SH
chmod +x "$TMP/repo/scripts/"*.sh

export NICCOLO_TEST_TRACE="$TMP/trace"
: > "$TMP/trace"

# Missing local build tools fail before asking for database-owner credentials.
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
grep -Fq 'Missing executable: cargo' "$TMP/preflight-err"
! grep -Fq 'database-owner URL' "$TMP/preflight-err"

# Non-interactive bootstrap requires the owner URL through the environment.
unset NICCOLO_PG_ADMIN_URL || true
set +e
bash "$TMP/repo/scripts/first-run-local-mvp.sh" --no-browser >"$TMP/no-admin-out" 2>"$TMP/no-admin-err"
no_admin_status=$?
set -e
[[ "$no_admin_status" == 2 ]]
grep -Fq 'run is non-interactive' "$TMP/no-admin-err"
[[ ! -e "$TMP/repo/.niccolo-local/local.env" ]]

export NICCOLO_PG_ADMIN_URL='postgres://owner:test@127.0.0.1:5432/db'

# Partial credential state must never trigger a second bootstrap over a database
# whose migrations or roles may already have been changed.
mkdir -p "$TMP/repo/.niccolo-local"
printf 'export NICCOLO_PG_SECURE_URL=%q\n' 'postgres://runtime' > "$TMP/repo/.niccolo-local/local.env"
: > "$TMP/trace"
set +e
bash "$TMP/repo/scripts/first-run-local-mvp.sh" --skip-model-config --no-browser >"$TMP/partial-out" 2>"$TMP/partial-err"
partial_status=$?
set -e
[[ "$partial_status" == 2 ]]
grep -Fq 'local bootstrap state is incomplete' "$TMP/partial-err"
! grep -Fq '^bootstrap$' "$TMP/trace"
rm -rf "$TMP/repo/.niccolo-local"

# A fresh non-interactive first run may bootstrap, but must not silently launch
# Analyze without a reasoning model.
: > "$TMP/trace"
set +e
bash "$TMP/repo/scripts/first-run-local-mvp.sh" --no-browser >"$TMP/model-missing-out" 2>"$TMP/model-missing-err"
model_missing_status=$?
set -e
[[ "$model_missing_status" == 2 ]]
grep -Fxq 'bootstrap' "$TMP/trace"
! grep -Fq '^start:' "$TMP/trace"
grep -Fq 'reasoning model is not configured' "$TMP/model-missing-err"

# The explicit prepare-only escape hatch reuses bootstrap and starts without
# invoking model configuration.
: > "$TMP/trace"
bash "$TMP/repo/scripts/first-run-local-mvp.sh" --skip-model-config --no-browser
! grep -Fq '^bootstrap$' "$TMP/trace"
! grep -Fq '^configure$' "$TMP/trace"
grep -Fxq 'start:--no-browser' "$TMP/trace"

# Process-environment model configuration is sufficient and stays ephemeral.
: > "$TMP/trace"
export NICCOLO_LLM_BASE_URL='https://models.example.test/v1'
export NICCOLO_LLM_MODEL='test-model'
export NICCOLO_LLM_API_KEY='do-not-write-me'
bash "$TMP/repo/scripts/first-run-local-mvp.sh" --no-browser
grep -Fxq 'start:--no-browser' "$TMP/trace"
[[ ! -f "$TMP/repo/.niccolo-local/model.env" ]]
unset NICCOLO_LLM_BASE_URL NICCOLO_LLM_MODEL NICCOLO_LLM_API_KEY

# A persisted model configuration is reused without re-bootstrap/reconfigure.
printf 'export NICCOLO_LLM_BASE_URL=%q\nexport NICCOLO_LLM_MODEL=%q\n' \
  'http://127.0.0.1:11434/v1' 'stored-model' > "$TMP/repo/.niccolo-local/model.env"
chmod 600 "$TMP/repo/.niccolo-local/model.env" 2>/dev/null || true
: > "$TMP/trace"
bash "$TMP/repo/scripts/first-run-local-mvp.sh" --no-browser
! grep -Fq '^bootstrap$' "$TMP/trace"
! grep -Fq '^configure$' "$TMP/trace"
grep -Fxq 'start:--no-browser' "$TMP/trace"

printf '%s\n' 'PASS: first-run wrapper is bootstrap-idempotent, model-aware, partial-state-safe, and does not leak bootstrap credentials.'
