#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PRIVATE_DIR="$ROOT/.niccolo-local"
LOCAL_ENV="$PRIVATE_DIR/local.env"
MODEL_ENV="$PRIVATE_DIR/model.env"
SKIP_MODEL_CONFIG=0
START_ARGS=()

for arg in "$@"; do
  case "$arg" in
    --skip-model-config)
      SKIP_MODEL_CONFIG=1
      ;;
    --no-browser)
      START_ARGS+=("$arg")
      ;;
    *)
      printf 'Unknown first-run option: %s\n' "$arg" >&2
      printf 'Usage: bash scripts/first-run-local-mvp.sh [--skip-model-config] [--no-browser]\n' >&2
      exit 2
      ;;
  esac
done

if [[ ! -f "$LOCAL_ENV" ]]; then
  if [[ -z "${NICCOLO_PG_ADMIN_URL:-}" ]]; then
    cat >&2 <<'MSG'
Niccolò has not been bootstrapped yet.
Set NICCOLO_PG_ADMIN_URL to a PostgreSQL database-owner connection, then run this command again.
Example:
  export NICCOLO_PG_ADMIN_URL='postgres://owner:password@127.0.0.1:5432/niccolo'
MSG
    exit 2
  fi
  printf '%s\n' '[1/3] Bootstrapping the local MVP...'
  bash "$ROOT/scripts/bootstrap-local-mvp.sh"
else
  printf '%s\n' '[1/3] Existing local bootstrap found; leaving it unchanged.'
fi

model_from_process=0
if [[ -n "${NICCOLO_LLM_BASE_URL:-}" && -n "${NICCOLO_LLM_MODEL:-}" ]]; then
  model_from_process=1
fi

if [[ "$SKIP_MODEL_CONFIG" == 0 && ! -f "$MODEL_ENV" && "$model_from_process" == 0 ]]; then
  if [[ -t 0 && -t 1 ]]; then
    printf '%s\n' '[2/3] Configure the reasoning model used by Analyze.'
    bash "$ROOT/scripts/configure-local-model.sh"
  else
    cat >&2 <<'MSG'
A reasoning model is not configured yet, and this run is non-interactive.
Configure it once with:
  bash scripts/configure-local-model.sh
or export NICCOLO_LLM_BASE_URL and NICCOLO_LLM_MODEL before running this command.
Use --skip-model-config only when you intentionally want to prepare/import data without Analyze.
MSG
    exit 2
  fi
elif [[ "$SKIP_MODEL_CONFIG" == 1 && ! -f "$MODEL_ENV" && "$model_from_process" == 0 ]]; then
  printf '%s\n' '[2/3] Reasoning-model setup intentionally skipped; Analyze will remain unavailable.'
else
  printf '%s\n' '[2/3] Reasoning-model configuration is available.'
fi

printf '%s\n' '[3/3] Starting Niccolò...'
exec bash "$ROOT/scripts/start-local-mvp.sh" "${START_ARGS[@]}"
