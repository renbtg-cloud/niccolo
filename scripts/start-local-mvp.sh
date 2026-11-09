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
if [[ -s "$model_env_file" ]]; then
  # Optional locally-owned model configuration. Never generated with secrets by bootstrap.
  # shellcheck disable=SC1091
  source "$model_env_file"
fi
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
  echo "To enable Analyze, set NICCOLO_LLM_BASE_URL and NICCOLO_LLM_MODEL before starting,"
  echo "or store export commands in $model_env_file (keep API keys private)."
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

exec "$binary"
