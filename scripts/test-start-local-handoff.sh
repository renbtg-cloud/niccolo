#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/repo/scripts" "$TMP/repo/target/release" "$TMP/repo/.niccolo-local"
chmod 700 "$TMP/repo/.niccolo-local"
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
[[ -z "${NICCOLO_PG_ADMIN_URL:-}" ]] || exit 89
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
printf '%s\n' "export NICCOLO_PG_SECURE_URL='postgres://runtime'" "export NICCOLO_API_PORT=38080" > "$TMP/repo/.niccolo-local/local.env"
printf '%s\n' "export WORKSPACE_UUID='11111111-1111-4111-8111-111111111111'" "export SITUATION_UUID='22222222-2222-4222-8222-222222222222'" > "$TMP/repo/.niccolo-local/identifiers.txt"
printf '%s\n' 'ncl_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa' > "$TMP/repo/.niccolo-local/token.txt"
chmod 600 "$TMP/repo/.niccolo-local/local.env" "$TMP/repo/.niccolo-local/identifiers.txt" "$TMP/repo/.niccolo-local/token.txt"

cd "$TMP/repo"

occupied_port_file="$TMP/occupied-port"
node - "$occupied_port_file" <<'NODE' &
const fs = require('node:fs');
const net = require('node:net');
const server = net.createServer();
server.listen(0, '127.0.0.1', () => {
  fs.writeFileSync(process.argv[2], String(server.address().port));
});
setTimeout(() => server.close(), 10000);
NODE
occupied_pid=$!
for _ in $(seq 1 40); do [[ -s "$occupied_port_file" ]] && break; sleep 0.05; done
[[ -s "$occupied_port_file" ]]
occupied_port="$(cat "$occupied_port_file")"
set +e
occupied_output="$(NICCOLO_API_PORT="$occupied_port" bash scripts/start-local-mvp.sh --no-browser 2>&1)"
occupied_status=$?
set -e
[[ "$occupied_status" == 1 ]]
grep -Fq "Local API port $occupied_port is already in use" <<< "$occupied_output"
kill "$occupied_pid" 2>/dev/null || true
wait "$occupied_pid" 2>/dev/null || true

export NICCOLO_TEST_EXPECT_HANDOFF=1
export NICCOLO_TEST_BROWSER_CAPTURE="$TMP/browser-url"
export NICCOLO_TEST_SERVER_CODE="$TMP/server-code"
output="$(NICCOLO_PG_ADMIN_URL='postgres://owner:must-not-reach-runtime@127.0.0.1/admin' bash scripts/start-local-mvp.sh)"
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

port_override="$(NICCOLO_API_PORT=39001 bash scripts/start-local-mvp.sh --print-only)"
grep -Fq 'Open: http://127.0.0.1:39001/' <<< "$port_override"
grep -Fq 'http://127.0.0.1:39001/' .niccolo-local/prepared-url.txt

leading_zero_port="$(NICCOLO_API_PORT=03808 bash scripts/start-local-mvp.sh --print-only)"
grep -Fq 'Open: http://127.0.0.1:3808/' <<< "$leading_zero_port"
grep -Fq 'http://127.0.0.1:3808/' .niccolo-local/prepared-url.txt

set +e
huge_port="$(NICCOLO_API_PORT=999999999999999999999999 bash scripts/start-local-mvp.sh --print-only 2>&1)"
huge_port_status=$?
set -e
[[ "$huge_port_status" == 2 ]]
grep -Fq 'NICCOLO_API_PORT must be an integer from 1 to 65535.' <<< "$huge_port"

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
chmod 600 .niccolo-local/token.txt

# Unix startup refuses the state directory itself if it is exposed or redirected.
if [[ "$(uname -s)" == Linux* || "$(uname -s)" == Darwin* ]]; then
  chmod 755 .niccolo-local
  set +e
  unsafe_dir="$(bash scripts/start-local-mvp.sh --print-only 2>&1)"
  unsafe_dir_status=$?
  set -e
  [[ "$unsafe_dir_status" == 2 ]]
  grep -Fq '.niccolo-local must not be accessible by group or other users.' <<< "$unsafe_dir"
  chmod 700 .niccolo-local

  mv .niccolo-local "$TMP/local-real"
  ln -s "$TMP/local-real" .niccolo-local
  set +e
  unsafe_dir_link="$(bash scripts/start-local-mvp.sh --print-only 2>&1)"
  unsafe_dir_link_status=$?
  set -e
  [[ "$unsafe_dir_link_status" == 2 ]]
  grep -Fq 'Unsafe .niccolo-local directory' <<< "$unsafe_dir_link"
  rm .niccolo-local
  mv "$TMP/local-real" .niccolo-local
  chmod 700 .niccolo-local
fi

# Unix startup refuses local secrets made readable by another user/group.
if [[ "$(uname -s)" == Linux* || "$(uname -s)" == Darwin* ]]; then
  chmod 644 .niccolo-local/token.txt
  set +e
  unsafe_mode="$(bash scripts/start-local-mvp.sh --print-only 2>&1)"
  unsafe_mode_status=$?
  set -e
  [[ "$unsafe_mode_status" == 2 ]]
  grep -Fq 'Private local file is readable or writable by other users' <<< "$unsafe_mode"
  chmod 600 .niccolo-local/token.txt

  mv .niccolo-local/token.txt "$TMP/token.real"
  ln -s "$TMP/token.real" .niccolo-local/token.txt
  set +e
  unsafe_link="$(bash scripts/start-local-mvp.sh --print-only 2>&1)"
  unsafe_link_status=$?
  set -e
  [[ "$unsafe_link_status" == 2 ]]
  grep -Fq 'Unsafe private local file' <<< "$unsafe_link"
  rm .niccolo-local/token.txt
  mv "$TMP/token.real" .niccolo-local/token.txt
  chmod 600 .niccolo-local/token.txt
fi

# The opener must validate the parsed URL origin, not just a textual prefix.
# An authority with userinfo can appear to start with 127.0.0.1 yet target
# an attacker-controlled host after the @ sign.
for malicious_url in \
  'http://127.0.0.1:38080@attacker.example/' \
  'http://127.0.0.1:38081/' \
  'https://127.0.0.1:38080/' \
  'http://user@127.0.0.1:38080/'; do
  if bash "$ROOT/scripts/open-local-mvp-browser.sh" "$malicious_url" 38080 >/dev/null 2>&1; then
    echo "Browser opener accepted an invalid origin: $malicious_url" >&2
    exit 1
  fi
done

# Startup must reject process-level model overrides that runtime would refuse,
# rather than launching successfully and failing only when Analyze is clicked.
for invalid_base in \
  'http://provider.example/v1' \
  'https://user:pass@provider.example/v1' \
  'https://provider.example/v1?secret=x' \
  'https://provider.example/v1#fragment'; do
  set +e
  invalid_model="$(
    NICCOLO_LLM_BASE_URL="$invalid_base" NICCOLO_LLM_MODEL='model' NICCOLO_LLM_API_KEY='test-key' \
      bash scripts/start-local-mvp.sh --print-only 2>&1
  )"
  invalid_model_status=$?
  set -e
  [[ "$invalid_model_status" == 2 ]]
  grep -Fq 'Reasoning model: INVALID CONFIGURATION' <<< "$invalid_model"
done

set +e
missing_remote_key="$(
  NICCOLO_LLM_BASE_URL='https://provider.example/v1' NICCOLO_LLM_MODEL='model' NICCOLO_LLM_API_KEY='' \
    bash scripts/start-local-mvp.sh --print-only 2>&1
)"
missing_remote_key_status=$?
set -e
[[ "$missing_remote_key_status" == 2 ]]
grep -Fq 'Remote HTTPS model providers require NICCOLO_LLM_API_KEY.' <<< "$missing_remote_key"

set +e
bad_json_mode="$(
  NICCOLO_LLM_BASE_URL='http://127.0.0.1:11434/v1' NICCOLO_LLM_MODEL='model' \
  NICCOLO_LLM_JSON_MODE='sometimes' bash scripts/start-local-mvp.sh --print-only 2>&1
)"
bad_json_mode_status=$?
set -e
[[ "$bad_json_mode_status" == 2 ]]
grep -Fq 'Reasoning model: INVALID CONFIGURATION' <<< "$bad_json_mode"

set +e
bad_provider_label="$(
  NICCOLO_LLM_BASE_URL='http://127.0.0.1:11434/v1' NICCOLO_LLM_MODEL='model' \
  NICCOLO_LLM_PROVIDER=
)"
bad_provider_label_status=$?
set -e
[[ "$bad_provider_label_status" == 2 ]]
grep -Fq 'Reasoning model: INVALID CONFIGURATION' <<< "$bad_provider_label"

set +e
blank_model="$(
  NICCOLO_LLM_BASE_URL='http://127.0.0.1:11434/v1' NICCOLO_LLM_MODEL='   ' \
    bash scripts/start-local-mvp.sh --print-only 2>&1
)"
blank_model_status=$?
set -e
[[ "$blank_model_status" == 2 ]]
grep -Fq 'Reasoning model: INVALID CONFIGURATION' <<< "$blank_model"

set +e
blank_provider="$(
  NICCOLO_LLM_BASE_URL='http://127.0.0.1:11434/v1' NICCOLO_LLM_MODEL='model' \
  NICCOLO_LLM_PROVIDER='   ' bash scripts/start-local-mvp.sh --print-only 2>&1
)"
blank_provider_status=$?
set -e
[[ "$blank_provider_status" == 2 ]]
grep -Fq 'Reasoning model: INVALID CONFIGURATION' <<< "$blank_provider"

set +e
blank_remote_key="$(
  NICCOLO_LLM_BASE_URL='https://provider.example/v1' NICCOLO_LLM_MODEL='model' NICCOLO_LLM_API_KEY='   ' \
    bash scripts/start-local-mvp.sh --print-only 2>&1
)"
blank_remote_key_status=$?
set -e
[[ "$blank_remote_key_status" == 2 ]]
grep -Fq 'Remote HTTPS model providers require NICCOLO_LLM_API_KEY.' <<< "$blank_remote_key"

printf '%s\n' 'PASS: local browser start uses a non-bearer one-shot capability, keeps manual fallback, and rejects corrupted local bootstrap state.'
provider\033[2J' bash scripts/start-local-mvp.sh --print-only 2>&1
)"
bad_provider_label_status=$?
set -e
[[ "$bad_provider_label_status" == 2 ]]
grep -Fq 'Reasoning model: INVALID CONFIGURATION' <<< "$bad_provider_label"

set +e
blank_model="$(
  NICCOLO_LLM_BASE_URL='http://127.0.0.1:11434/v1' NICCOLO_LLM_MODEL='   ' \
    bash scripts/start-local-mvp.sh --print-only 2>&1
)"
blank_model_status=$?
set -e
[[ "$blank_model_status" == 2 ]]
grep -Fq 'Reasoning model: INVALID CONFIGURATION' <<< "$blank_model"

set +e
blank_provider="$(
  NICCOLO_LLM_BASE_URL='http://127.0.0.1:11434/v1' NICCOLO_LLM_MODEL='model' \
  NICCOLO_LLM_PROVIDER='   ' bash scripts/start-local-mvp.sh --print-only 2>&1
)"
blank_provider_status=$?
set -e
[[ "$blank_provider_status" == 2 ]]
grep -Fq 'Reasoning model: INVALID CONFIGURATION' <<< "$blank_provider"

set +e
blank_remote_key="$(
  NICCOLO_LLM_BASE_URL='https://provider.example/v1' NICCOLO_LLM_MODEL='model' NICCOLO_LLM_API_KEY='   ' \
    bash scripts/start-local-mvp.sh --print-only 2>&1
)"
blank_remote_key_status=$?
set -e
[[ "$blank_remote_key_status" == 2 ]]
grep -Fq 'Remote HTTPS model providers require NICCOLO_LLM_API_KEY.' <<< "$blank_remote_key"

printf '%s\n' 'PASS: local browser start uses a non-bearer one-shot capability, keeps manual fallback, and rejects corrupted local bootstrap state.'
