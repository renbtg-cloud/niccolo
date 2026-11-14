#!/usr/bin/env bash
# Configure the OpenAI-compatible reasoning model used by the local MVP.
# Secrets are written only to the ignored private .niccolo-local/model.env file.
set -Eeuo pipefail
umask 077

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

if (( $# > 1 )); then
  usage >&2
  exit 2
fi
mode="${1:-}"
case "$mode" in
  ''|--from-env) ;;
  --help) usage; exit 0 ;;
  *) usage >&2; exit 2 ;;
esac

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

model_env_file=".niccolo-local/model.env"
mkdir -p .niccolo-local

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

if [[ -e "$model_env_file" ]]; then
  case "$(uname -s)" in
    MINGW*|MSYS*|CYGWIN*) ;;
    *)
      [[ -f "$model_env_file" && ! -L "$model_env_file" && -O "$model_env_file" ]] || {
        echo "Unsafe existing model configuration file: $model_env_file" >&2
        exit 2
      }
      model_mode="$(stat -c '%a' "$model_env_file" 2>/dev/null || stat -f '%Lp' "$model_env_file" 2>/dev/null || true)"
      [[ "$model_mode" =~ ^[0-7]{3,4}$ ]] || {
        echo "Unable to verify private permissions for: $model_env_file" >&2
        exit 2
      }
      (( (8#$model_mode & 077) == 0 )) || {
        echo "Existing model configuration is readable or writable by other users: $model_env_file" >&2
        exit 2
      }
      ;;
  esac
fi

# Explicit environment values win over an existing private configuration file;
# the file is only a source of defaults for values the caller did not export.
explicit_base_set="${NICCOLO_LLM_BASE_URL+x}"; explicit_base="${NICCOLO_LLM_BASE_URL:-}"
explicit_model_set="${NICCOLO_LLM_MODEL+x}"; explicit_model="${NICCOLO_LLM_MODEL:-}"
explicit_provider_set="${NICCOLO_LLM_PROVIDER+x}"; explicit_provider="${NICCOLO_LLM_PROVIDER:-}"
explicit_key_set="${NICCOLO_LLM_API_KEY+x}"; explicit_key="${NICCOLO_LLM_API_KEY:-}"
explicit_json_set="${NICCOLO_LLM_JSON_MODE+x}"; explicit_json="${NICCOLO_LLM_JSON_MODE:-}"
stored_base_url=''
if [[ -s "$model_env_file" ]]; then
  # Existing private values are defaults for an interactive reconfiguration.
  # shellcheck disable=SC1090
  source "$model_env_file"
  stored_base_url="${NICCOLO_LLM_BASE_URL:-}"
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

# Never silently reuse a stored provider secret after changing the endpoint. A
# new endpoint requires an explicitly supplied key (or an interactive re-entry).
if [[ "$base_url" != "$stored_base_url" && -z "$explicit_key_set" ]]; then
  api_key=''
fi

valid_text(){
  local value="$1" max="$2"
  [[ -n "$value" && ${#value} -le $max && "$value" =~ [^[:space:]] ]] || return 1
  [[ "$value" =~ [[:cntrl:]] ]] && return 1
  return 0
}

valid_text "$base_url" 2048 || { echo 'A bounded model base URL is required.' >&2; exit 2; }
valid_text "$model" 512 || { echo 'A bounded model name is required.' >&2; exit 2; }
if [[ -n "$provider" ]]; then
  valid_text "$provider" 128 || { echo 'Provider label is invalid.' >&2; exit 2; }
fi

command -v node >/dev/null 2>&1 || { echo 'Node.js is required to validate the model endpoint.' >&2; exit 1; }
endpoint_kind="$(NICCOLO_CONFIG_BASE_URL="$base_url" node - <<'NODE'
const raw = process.env.NICCOLO_CONFIG_BASE_URL;
try {
  if (/\s/.test(raw)) process.exit(2);
  const u = new URL(raw);
  if (u.username || u.password || u.search || u.hash) process.exit(2);
  if (u.protocol === 'http:') {
    if (u.hostname !== '127.0.0.1' && u.hostname !== 'localhost') process.exit(3);
    process.stdout.write('loopback');
  } else if (u.protocol === 'https:' && u.hostname) {
    process.stdout.write('remote');
  } else {
    process.exit(2);
  }
} catch {
  process.exit(2);
}
NODE
)" || {
  status=$?
  if [[ "$status" == 3 ]]; then
    echo 'Plain HTTP model endpoints are allowed only on loopback.' >&2
  else
    echo 'Model base URL must be a credential-free HTTPS URL, or HTTP on 127.0.0.1/localhost.' >&2
  fi
  exit 2
}
loopback_http=0
[[ "$endpoint_kind" != "loopback" ]] || loopback_http=1

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
if [[ -n "$json_mode" ]] && ! [[ "$json_mode" =~ ^(1|0|true|false|TRUE|FALSE|yes|no|YES|NO)$ ]]; then
  echo 'NICCOLO_LLM_JSON_MODE must be a supported boolean value when set.' >&2
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
