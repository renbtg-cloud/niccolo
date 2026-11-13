#!/usr/bin/env bash
# Start an already-bootstrapped single-PC Niccolo MVP with a prepared local URL.
# The bearer token is deliberately NOT placed in the URL or printed to stdout.
set -Eeuo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

env_file=".niccolo-local/local.env"
model_env_file=".niccolo-local/model.env"
ids_file=".niccolo-local/identifiers.txt"
token_file=".niccolo-local/token.txt"
url_file=".niccolo-local/prepared-url.txt"

for file in "$env_file" "$ids_file" "$token_file"; do
  [[ -s "$file" ]] || {
    echo "Missing local bootstrap file: $file" >&2
    echo "Run: bash scripts/bootstrap-local-mvp.sh" >&2
    exit 1
  }
done

case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*) ;;
  *)
    [[ -d .niccolo-local && ! -L .niccolo-local && -O .niccolo-local ]] || {
      echo 'Unsafe .niccolo-local directory; it must be an owner-controlled real directory.' >&2
      exit 2
    }
    local_mode="$(stat -c '%a' .niccolo-local 2>/dev/null || stat -f '%Lp' .niccolo-local 2>/dev/null || true)"
    [[ "$local_mode" =~ ^[0-7]{3,4}$ ]] || {
      echo 'Unable to verify .niccolo-local directory permissions.' >&2
      exit 2
    }
    (( (8#$local_mode & 077) == 0 )) || {
      echo '.niccolo-local must not be accessible by group or other users.' >&2
      exit 2
    }
    ;;
esac

# On Unix, bootstrap/model files containing database, bearer or provider
# credentials must remain ordinary, owner-only files. Refuse symlinks and
# group/world-readable secrets before sourcing any shell assignments.
case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*) ;;
  *)
    assert_private_file(){
      local file="$1" mode=''
      [[ -f "$file" && ! -L "$file" && -O "$file" ]] || {
        echo "Unsafe private local file: $file" >&2
        exit 2
      }
      mode="$(stat -c '%a' "$file" 2>/dev/null || stat -f '%Lp' "$file" 2>/dev/null || true)"
      [[ "$mode" =~ ^[0-7]{3,4}$ ]] || {
        echo "Unable to verify private permissions for: $file" >&2
        exit 2
      }
      (( (8#$mode & 077) == 0 )) || {
        echo "Private local file is readable or writable by other users: $file" >&2
        exit 2
      }
    }
    assert_private_file "$env_file"
    assert_private_file "$token_file"
    [[ ! -e "$model_env_file" ]] || assert_private_file "$model_env_file"
    ;;
esac

# Explicit process environment is an operator override for the listening port;
# local.env supplies the bootstrap default.
explicit_api_port_set="${NICCOLO_API_PORT+x}"
explicit_api_port="${NICCOLO_API_PORT:-}"
# Generated locally by bootstrap from UUIDs and shell-escaped values.
# shellcheck disable=SC1091
source "$env_file"
[[ -z "$explicit_api_port_set" ]] || NICCOLO_API_PORT="$explicit_api_port"
# Explicit process environment is an operator override; model.env supplies only
# defaults for values that were not exported for this start invocation.
explicit_model_base_set="${NICCOLO_LLM_BASE_URL+x}"; explicit_model_base="${NICCOLO_LLM_BASE_URL:-}"
explicit_model_name_set="${NICCOLO_LLM_MODEL+x}"; explicit_model_name="${NICCOLO_LLM_MODEL:-}"
explicit_model_provider_set="${NICCOLO_LLM_PROVIDER+x}"; explicit_model_provider="${NICCOLO_LLM_PROVIDER:-}"
explicit_model_key_set="${NICCOLO_LLM_API_KEY+x}"; explicit_model_key="${NICCOLO_LLM_API_KEY:-}"
explicit_model_json_set="${NICCOLO_LLM_JSON_MODE+x}"; explicit_model_json="${NICCOLO_LLM_JSON_MODE:-}"
if [[ -s "$model_env_file" ]]; then
  # Optional locally-owned model configuration. Never generated with secrets by bootstrap.
  # shellcheck disable=SC1091
  source "$model_env_file"
fi
[[ -z "$explicit_model_base_set" ]] || NICCOLO_LLM_BASE_URL="$explicit_model_base"
[[ -z "$explicit_model_name_set" ]] || NICCOLO_LLM_MODEL="$explicit_model_name"
[[ -z "$explicit_model_provider_set" ]] || NICCOLO_LLM_PROVIDER="$explicit_model_provider"
[[ -z "$explicit_model_key_set" ]] || NICCOLO_LLM_API_KEY="$explicit_model_key"
[[ -z "$explicit_model_json_set" ]] || NICCOLO_LLM_JSON_MODE="$explicit_model_json"
# shellcheck disable=SC1091
source "$ids_file"

# Database-owner credentials are never needed by the restricted runtime API.
# Strip an inherited bootstrap URL before any long-lived child process starts.
unset NICCOLO_PG_ADMIN_URL

: "${WORKSPACE_UUID:?missing WORKSPACE_UUID}"
: "${SITUATION_UUID:?missing SITUATION_UUID}"
: "${NICCOLO_API_PORT:=38080}"

uuid_re='^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$'
if ! [[ "$WORKSPACE_UUID" =~ $uuid_re ]]; then
  echo "Invalid WORKSPACE_UUID in $ids_file" >&2
  exit 2
fi
if ! [[ "$SITUATION_UUID" =~ $uuid_re ]]; then
  echo "Invalid SITUATION_UUID in $ids_file" >&2
  exit 2
fi
if ! [[ "$NICCOLO_API_PORT" =~ ^[0-9]+$ ]] || (( ${#NICCOLO_API_PORT} > 5 )); then
  echo 'NICCOLO_API_PORT must be an integer from 1 to 65535.' >&2
  exit 2
fi
api_port_value=$((10#$NICCOLO_API_PORT))
if (( api_port_value < 1 || api_port_value > 65535 )); then
  echo 'NICCOLO_API_PORT must be an integer from 1 to 65535.' >&2
  exit 2
fi
NICCOLO_API_PORT="$api_port_value"
export NICCOLO_API_PORT
if [[ "$(wc -l < "$token_file")" -ne 1 ]] || ! grep -Eq '^ncl_[0-9a-f]{64}\r?$' "$token_file"; then
  echo "Invalid local bearer token file: $token_file" >&2
  echo 'Re-bootstrap a fresh dedicated local installation instead of guessing or repairing the token by hand.' >&2
  exit 2
fi

prepared_url="http://127.0.0.1:${NICCOLO_API_PORT}/?workspace=${WORKSPACE_UUID}&situation=${SITUATION_UUID}"
printf '%s\n' "$prepared_url" > "$url_file"
chmod 600 "$url_file" 2>/dev/null || true

echo "Niccolo local session"
echo "Open: $prepared_url"
echo "Bearer token fallback: $token_file"
llm_base="${NICCOLO_LLM_BASE_URL:-}"
llm_model="${NICCOLO_LLM_MODEL:-}"
llm_key="${NICCOLO_LLM_API_KEY:-}"
llm_provider="${NICCOLO_LLM_PROVIDER:-}"
llm_json_mode="${NICCOLO_LLM_JSON_MODE:-}"
if [[ -n "$llm_base" || -n "$llm_model" || -n "$llm_key" || -n "$llm_provider" || -n "$llm_json_mode" ]]; then
  command -v node >/dev/null 2>&1 || {
    echo 'Node.js is required to validate the reasoning-model configuration.' >&2
    exit 1
  }
  if NICCOLO_START_BASE_URL="$llm_base" NICCOLO_START_MODEL="$llm_model" NICCOLO_START_KEY="$llm_key" \
      NICCOLO_START_PROVIDER="$llm_provider" NICCOLO_START_JSON_MODE="$llm_json_mode" node - <<'NODE'
const base = process.env.NICCOLO_START_BASE_URL ?? '';
const model = process.env.NICCOLO_START_MODEL ?? '';
const key = process.env.NICCOLO_START_KEY ?? '';
const provider = process.env.NICCOLO_START_PROVIDER ?? '';
const jsonMode = process.env.NICCOLO_START_JSON_MODE ?? '';
const controls = value => [...value].some(ch => {
  const code = ch.codePointAt(0);
  return code < 0x20 || code === 0x7f;
});
if (!base || !model.trim() || controls(model) || model.length > 512 ||
    controls(key) || key.length > 4096 ||
    (provider && !provider.trim()) || controls(provider) || provider.length > 128 ||
    (jsonMode && !/^(1|0|true|false|TRUE|FALSE|yes|no|YES|NO)$/.test(jsonMode))) process.exit(2);
try {
  if (/\s/.test(base) || base.length > 2048) process.exit(2);
  const url = new URL(base);
  if (url.username || url.password || url.search || url.hash) process.exit(2);
  const loopback = url.protocol === 'http:' &&
    (url.hostname === '127.0.0.1' || url.hostname === 'localhost');
  const remote = url.protocol === 'https:' && Boolean(url.hostname);
  if (!loopback && !remote) process.exit(2);
  if (remote && !key.trim()) process.exit(3);
} catch {
  process.exit(2);
}
NODE
  then
    :
  else
    status=$?
    echo 'Reasoning model: INVALID CONFIGURATION' >&2
    if [[ "$status" == 3 ]]; then
      echo 'Remote HTTPS model providers require NICCOLO_LLM_API_KEY.' >&2
    else
      echo 'Model configuration must use a credential-free HTTPS URL or loopback HTTP; model/provider/key values must be bounded and control-free, and JSON mode must be a supported boolean.' >&2
    fi
    exit 2
  fi
fi
if [[ -z "$llm_base" && -z "$llm_model" && -z "$llm_key" ]]; then
  echo "Reasoning model: NOT CONFIGURED"
  echo "To enable Analyze, run: bash scripts/configure-local-model.sh"
  echo "You may also export NICCOLO_LLM_BASE_URL and NICCOLO_LLM_MODEL before starting."
elif [[ -z "$llm_base" || -z "$llm_model" ]]; then
  echo "Reasoning model: INCOMPLETE CONFIGURATION" >&2
  echo "Both NICCOLO_LLM_BASE_URL and NICCOLO_LLM_MODEL are required." >&2
  exit 2
else
  echo "Reasoning model: configured (${NICCOLO_LLM_MODEL})"
fi

if [[ "${1:-}" == "--print-only" ]]; then
  exit 0
fi

ext=''
case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*) ext='.exe' ;;
esac
binary="./target/release/niccolo-api${ext}"
[[ -x "$binary" || -f "$binary" ]] || {
  echo "Missing API binary: $binary" >&2
  echo "Run the bootstrap/build first." >&2
  exit 1
}

# Refuse to let the browser opener mistake an unrelated/stale service for this
# Niccolò instance. Probe the exact loopback bind before minting a handoff.
command -v node >/dev/null 2>&1 || {
  echo 'Node.js is required to verify the local API port before startup.' >&2
  exit 1
}
if ! NICCOLO_START_PORT="$NICCOLO_API_PORT" node - <<'NODE'
const net = require('node:net');
const port = Number(process.env.NICCOLO_START_PORT);
const server = net.createServer();
server.once('error', () => process.exit(1));
server.listen(port, '127.0.0.1', () => server.close(() => process.exit(0)));
NODE
then
  echo "Local API port $NICCOLO_API_PORT is already in use; choose another NICCOLO_API_PORT." >&2
  exit 1
fi

open_browser=1
if [[ "${NICCOLO_OPEN_BROWSER:-1}" == "0" || "${NICCOLO_OPEN_BROWSER:-1}" == "false" || "${1:-}" == "--no-browser" ]]; then
  open_browser=0
fi
# Never inherit a stale handoff capability supplied by the surrounding shell.
unset NICCOLO_LOCAL_HANDOFF_CODE NICCOLO_LOCAL_HANDOFF_TOKEN_FILE
browser_url="$prepared_url"
if (( open_browser == 1 )); then
  if command -v od >/dev/null 2>&1 && [[ -r /dev/urandom ]]; then
    handoff_code="$(LC_ALL=C od -An -N32 -tx1 /dev/urandom | tr -d '[:space:]')"
    if [[ "$handoff_code" =~ ^[0-9a-f]{64}$ ]]; then
      export NICCOLO_LOCAL_HANDOFF_CODE="$handoff_code"
      export NICCOLO_LOCAL_HANDOFF_TOKEN_FILE="$ROOT/$token_file"
      browser_url="${prepared_url}#handoff=${handoff_code}"
      echo "Browser: automatic open uses a short-lived one-shot local handoff; the bearer token stays in $token_file."
    else
      echo "Browser: secure handoff generation failed; opening the prepared URL for manual token entry." >&2
    fi
  else
    echo "Browser: secure handoff generator unavailable; opening the prepared URL for manual token entry." >&2
  fi
  (
    # The opener receives only the prepared loopback URL plus an optional
    # one-shot capability. It never receives the bearer or database/model secrets.
    unset NICCOLO_LLM_API_KEY NICCOLO_PG_SECURE_URL NICCOLO_PG_SECURE_PASSWORD NICCOLO_PG_ADMIN_URL NICCOLO_NEXT_ADMIN_URL NICCOLO_NEXT_APP_URL PGPASSWORD
    unset NICCOLO_LOCAL_HANDOFF_CODE NICCOLO_LOCAL_HANDOFF_TOKEN_FILE
    exec bash scripts/open-local-mvp-browser.sh "$browser_url" "$NICCOLO_API_PORT"
  ) >/dev/null 2>&1 &
else
  echo "Browser: automatic open disabled; use the Open URL above and paste the bearer token from $token_file."
fi

exec "$binary"
