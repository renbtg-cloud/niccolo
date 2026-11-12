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

# Generated locally by bootstrap from UUIDs and shell-escaped values.
# shellcheck disable=SC1091
source "$env_file"
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

: "${WORKSPACE_UUID:?missing WORKSPACE_UUID}"
: "${SITUATION_UUID:?missing SITUATION_UUID}"
: "${NICCOLO_API_PORT:=38080}"

prepared_url="http://127.0.0.1:${NICCOLO_API_PORT}/?workspace=${WORKSPACE_UUID}&situation=${SITUATION_UUID}"
printf '%s\n' "$prepared_url" > "$url_file"
chmod 600 "$url_file" 2>/dev/null || true

echo "Niccolo local session"
echo "Open: $prepared_url"
echo "Bearer token: $token_file"
echo "Paste that token into the connection panel; it is intentionally not embedded in the URL."
llm_base="${NICCOLO_LLM_BASE_URL:-}"
llm_model="${NICCOLO_LLM_MODEL:-}"
llm_key="${NICCOLO_LLM_API_KEY:-}"
if [[ -z "$llm_base" && -z "$llm_model" && -z "$llm_key" ]]; then
  echo "Reasoning model: NOT CONFIGURED"
  echo "To enable Analyze, run: bash scripts/configure-local-model.sh"
  echo "You may also export NICCOLO_LLM_BASE_URL and NICCOLO_LLM_MODEL before starting."
elif [[ -z "$llm_base" || -z "$llm_model" ]]; then
  echo "Reasoning model: INCOMPLETE CONFIGURATION" >&2
  echo "Both NICCOLO_LLM_BASE_URL and NICCOLO_LLM_MODEL are required." >&2
  exit 2
elif [[ "$llm_base" == https://* && -z "$llm_key" ]]; then
  echo "Reasoning model: INCOMPLETE CONFIGURATION" >&2
  echo "Remote HTTPS model providers require NICCOLO_LLM_API_KEY." >&2
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

open_browser=1
if [[ "${NICCOLO_OPEN_BROWSER:-1}" == "0" || "${NICCOLO_OPEN_BROWSER:-1}" == "false" || "${1:-}" == "--no-browser" ]]; then
  open_browser=0
fi
if (( open_browser == 1 )); then
  echo "Browser: best-effort automatic open is enabled (set NICCOLO_OPEN_BROWSER=0 to disable)."
  (
    # The opener needs only the generated loopback URL and port. Do not expose
    # database or model credentials to the short-lived helper process.
    unset NICCOLO_LLM_API_KEY NICCOLO_PG_SECURE_URL NICCOLO_PG_SECURE_PASSWORD NICCOLO_PG_ADMIN_URL NICCOLO_NEXT_ADMIN_URL NICCOLO_NEXT_APP_URL PGPASSWORD
    exec bash scripts/open-local-mvp-browser.sh "$prepared_url" "$NICCOLO_API_PORT"
  ) >/dev/null 2>&1 &
else
  echo "Browser: automatic open disabled; use the Open URL above."
fi

exec "$binary"
