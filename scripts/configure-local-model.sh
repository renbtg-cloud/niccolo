#!/usr/bin/env bash
# Configure the OpenAI-compatible reasoning model used by the local MVP.
# Secrets are written only to the ignored private .niccolo-local/model.env file.
set -Eeuo pipefail
umask 077

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

model_env_file=".niccolo-local/model.env"
mkdir -p .niccolo-local

# Explicit environment values win over an existing private configuration file;
# the file is only a source of defaults for values the caller did not export.
explicit_base_set="${NICCOLO_LLM_BASE_URL+x}"; explicit_base="${NICCOLO_LLM_BASE_URL:-}"
explicit_model_set="${NICCOLO_LLM_MODEL+x}"; explicit_model="${NICCOLO_LLM_MODEL:-}"
explicit_provider_set="${NICCOLO_LLM_PROVIDER+x}"; explicit_provider="${NICCOLO_LLM_PROVIDER:-}"
explicit_key_set="${NICCOLO_LLM_API_KEY+x}"; explicit_key="${NICCOLO_LLM_API_KEY:-}"
explicit_json_set="${NICCOLO_LLM_JSON_MODE+x}"; explicit_json="${NICCOLO_LLM_JSON_MODE:-}"
if [[ -s "$model_env_file" ]]; then
  # Existing private values are defaults for an interactive reconfiguration.
  # shellcheck disable=SC1090
  source "$model_env_file"
fi
[[ -z "$explicit_base_set" ]] || NICCOLO_LLM_BASE_URL="$explicit_base"
[[ -z "$explicit_model_set" ]] || NICCOLO_LLM_MODEL="$explicit_model"
[[ -z "$explicit_provider_set" ]] || NICCOLO_LLM_PROVIDER="$explicit_provider"
[[ -z "$explicit_key_set" ]] || NICCOLO_LLM_API_KEY="$explicit_key"
[[ -z "$explicit_json_set" ]] || NICCOLO_LLM_JSON_MODE="$explicit_json"

base_url="${NICCOLO_LLM_BASE_URL:-}"
model="${NICCOLO_LLM_MODEL:-}"
provider="${NICCOLO_LLM_PROVIDER:-}"
api_key="${NICCOLO_LLM_API_KEY:-}"
json_mode="${NICCOLO_LLM_JSON_MODE:-}"

usage(){
  cat <<'USAGE'
Usage:
  bash scripts/configure-local-model.sh
  bash scripts/configure-local-model.sh --from-env

Interactive mode prompts for the base URL/model and hides API-key input.
--from-env writes the already-exported NICCOLO_LLM_* values without prompting.
Never pass an API key as a command-line argument.
USAGE
}

mode="${1:-}"
if [[ -n "$mode" && "$mode" != "--from-env" && "$mode" != "--help" ]]; then
  usage >&2
  exit 2
fi
if [[ "$mode" == "--help" ]]; then
  usage
  exit 0
fi

if [[ "$mode" != "--from-env" ]]; then
  [[ -t 0 ]] || {
    echo 'Interactive model configuration requires a terminal; use --from-env for scripts.' >&2
    exit 2
  }
  printf 'OpenAI-compatible base URL%s: ' "${base_url:+ [$base_url]}"
  read -r entered
  [[ -z "$entered" ]] || base_url="$entered"
  printf 'Model name%s: ' "${model:+ [$model]}"
  read -r entered
  [[ -z "$entered" ]] || model="$entered"
  printf 'Provider label (optional)%s: ' "${provider:+ [$provider]}"
  read -r entered
  [[ -z "$entered" ]] || provider="$entered"
fi

valid_text(){
  local value="$1" max="$2"
  [[ -n "$value" && ${#value} -le $max && "$value" != *$'\n'* && "$value" != *$'\r'* ]]
}

valid_text "$base_url" 2048 || { echo 'A bounded model base URL is required.' >&2; exit 2; }
valid_text "$model" 512 || { echo 'A bounded model name is required.' >&2; exit 2; }
if [[ -n "$provider" ]]; then
  valid_text "$provider" 256 || { echo 'Provider label is invalid.' >&2; exit 2; }
fi

loopback_http=0
case "$base_url" in
  http://127.0.0.1:*|http://localhost:*|http://\[::1\]:*) loopback_http=1 ;;
  https://*) ;;
  http://*)
    echo 'Plain HTTP model endpoints are allowed only on loopback.' >&2
    exit 2
    ;;
  *)
    echo 'Model base URL must use HTTPS, or HTTP on loopback.' >&2
    exit 2
    ;;
esac

if (( loopback_http == 0 )) && [[ -z "$api_key" && "$mode" != "--from-env" ]]; then
  printf 'API key (input hidden): '
  IFS= read -r -s api_key
  printf '\n'
fi
if (( loopback_http == 0 )) && ! valid_text "$api_key" 4096; then
  echo 'Remote HTTPS model providers require an API key.' >&2
  exit 2
fi
if [[ -n "$api_key" ]] && ! valid_text "$api_key" 4096; then
  echo 'API key is invalid.' >&2
  exit 2
fi
if [[ -n "$json_mode" && "$json_mode" != "true" && "$json_mode" != "false" ]]; then
  echo 'NICCOLO_LLM_JSON_MODE must be true or false when set.' >&2
  exit 2
fi

model_env_tmp="${model_env_file}.tmp.$$"
cleanup(){ rm -f "$model_env_tmp"; }
trap cleanup EXIT
{
  printf 'export NICCOLO_LLM_BASE_URL=%q\n' "$base_url"
  printf 'export NICCOLO_LLM_MODEL=%q\n' "$model"
  [[ -z "$provider" ]] || printf 'export NICCOLO_LLM_PROVIDER=%q\n' "$provider"
  [[ -z "$api_key" ]] || printf 'export NICCOLO_LLM_API_KEY=%q\n' "$api_key"
  [[ -z "$json_mode" ]] || printf 'export NICCOLO_LLM_JSON_MODE=%q\n' "$json_mode"
} > "$model_env_tmp"
chmod 600 "$model_env_tmp" 2>/dev/null || true
mv -f "$model_env_tmp" "$model_env_file"
trap - EXIT

echo "Model configuration saved privately to $model_env_file"
echo "Base URL: $base_url"
echo "Model: $model"
[[ -z "$provider" ]] || echo "Provider: $provider"
if [[ -n "$api_key" ]]; then
  echo 'API key: stored privately (value not displayed)'
else
  echo 'API key: not required for this loopback HTTP endpoint'
fi
