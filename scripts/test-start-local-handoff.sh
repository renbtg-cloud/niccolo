#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/repo/scripts" "$TMP/repo/target/release" "$TMP/repo/.niccolo-local"
cp "$ROOT/scripts/start-local-mvp.sh" "$TMP/repo/scripts/start-local-mvp.sh"
cat > "$TMP/repo/scripts/open-local-mvp-browser.sh" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
[[ -z "${NICCOLO_LOCAL_HANDOFF_CODE:-}" ]]
[[ -z "${NICCOLO_LOCAL_HANDOFF_TOKEN_FILE:-}" ]]
printf '%s\n' "$1" > "${NICCOLO_TEST_BROWSER_CAPTURE:?}"
SH
cat > "$TMP/repo/target/release/niccolo-api" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
if [[ "${NICCOLO_TEST_EXPECT_HANDOFF:-0}" == 1 ]]; then
  [[ "${NICCOLO_LOCAL_HANDOFF_CODE:-}" =~ ^[0-9a-f]{64}$ ]]
  [[ "${NICCOLO_LOCAL_HANDOFF_TOKEN_FILE:-}" == "$PWD/.niccolo-local/token.txt" ]]
  printf '%s\n' "$NICCOLO_LOCAL_HANDOFF_CODE" > "${NICCOLO_TEST_SERVER_CODE:?}"
else
  [[ -z "${NICCOLO_LOCAL_HANDOFF_CODE:-}" ]]
  [[ -z "${NICCOLO_LOCAL_HANDOFF_TOKEN_FILE:-}" ]]
fi
SH
chmod +x "$TMP/repo/scripts/"*.sh "$TMP/repo/target/release/niccolo-api"
printf '%s\n' "export NICCOLO_PG_SECURE_URL='postgres://runtime'" > "$TMP/repo/.niccolo-local/local.env"
printf '%s\n' "export WORKSPACE_UUID='11111111-1111-4111-8111-111111111111'" "export SITUATION_UUID='22222222-2222-4222-8222-222222222222'" > "$TMP/repo/.niccolo-local/identifiers.txt"
printf '%s\n' 'ncl_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa' > "$TMP/repo/.niccolo-local/token.txt"

cd "$TMP/repo"
export NICCOLO_TEST_EXPECT_HANDOFF=1
export NICCOLO_TEST_BROWSER_CAPTURE="$TMP/browser-url"
export NICCOLO_TEST_SERVER_CODE="$TMP/server-code"
output="$(bash scripts/start-local-mvp.sh)"
for _ in $(seq 1 40); do [[ -s "$TMP/browser-url" ]] && break; sleep 0.05; done
[[ -s "$TMP/browser-url" && -s "$TMP/server-code" ]]
code="$(cat "$TMP/server-code")"
url="$(cat "$TMP/browser-url")"
[[ "$url" == "http://127.0.0.1:38080/?workspace=11111111-1111-4111-8111-111111111111&situation=22222222-2222-4222-8222-222222222222#handoff=$code" ]]
prepared="$(cat .niccolo-local/prepared-url.txt)"
[[ "$prepared" == "http://127.0.0.1:38080/?workspace=11111111-1111-4111-8111-111111111111&situation=22222222-2222-4222-8222-222222222222" ]]
[[ "$prepared" != *'#handoff='* ]]
! grep -Fq 'ncl_aaaaaaaa' <<< "$output"
! grep -Fq "$code" <<< "$output"
grep -Fq 'one-shot local handoff' <<< "$output"
grep -Fq 'Bearer token fallback: .niccolo-local/token.txt' <<< "$output"

export NICCOLO_TEST_EXPECT_HANDOFF=0
rm -f "$TMP/browser-url" "$TMP/server-code"
manual="$(NICCOLO_OPEN_BROWSER=0 NICCOLO_LOCAL_HANDOFF_CODE="$(printf 'f%.0s' {1..64})" NICCOLO_LOCAL_HANDOFF_TOKEN_FILE='/tmp/stale' bash scripts/start-local-mvp.sh)"
grep -Fq 'paste the bearer token from .niccolo-local/token.txt' <<< "$manual"

# Corrupted local bootstrap state must fail before browser/server startup.
cp .niccolo-local/identifiers.txt "$TMP/identifiers.good"
cp .niccolo-local/token.txt "$TMP/token.good"

set +e
bad_port="$(NICCOLO_API_PORT=70000 bash scripts/start-local-mvp.sh --print-only 2>&1)"
bad_port_status=$?
set -e
[[ "$bad_port_status" == 2 ]]
grep -Fq 'NICCOLO_API_PORT must be an integer from 1 to 65535.' <<< "$bad_port"

printf '%s\n' "export WORKSPACE_UUID='not-a-uuid'" "export SITUATION_UUID='22222222-2222-4222-8222-222222222222'" > .niccolo-local/identifiers.txt
set +e
bad_id="$(bash scripts/start-local-mvp.sh --print-only 2>&1)"
bad_id_status=$?
set -e
[[ "$bad_id_status" == 2 ]]
grep -Fq 'Invalid WORKSPACE_UUID' <<< "$bad_id"
cp "$TMP/identifiers.good" .niccolo-local/identifiers.txt

printf '%s\n' 'not-a-token' > .niccolo-local/token.txt
set +e
bad_token="$(bash scripts/start-local-mvp.sh --print-only 2>&1)"
bad_token_status=$?
set -e
[[ "$bad_token_status" == 2 ]]
grep -Fq 'Invalid local bearer token file' <<< "$bad_token"
cp "$TMP/token.good" .niccolo-local/token.txt

printf '%s\n' 'PASS: local browser start uses a non-bearer one-shot capability, keeps manual fallback, and rejects corrupted local bootstrap state.'
