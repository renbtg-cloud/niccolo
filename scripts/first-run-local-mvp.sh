#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PRIVATE_DIR="$ROOT/.niccolo-local"
LOCAL_ENV="$PRIVATE_DIR/local.env"
IDENTIFIERS="$PRIVATE_DIR/identifiers.txt"
TOKEN_FILE="$PRIVATE_DIR/token.txt"
MODEL_ENV="$PRIVATE_DIR/model.env"
SKIP_MODEL_CONFIG=0
NO_BROWSER=0
START_ARGS=()

for arg in "$@"; do
  case "$arg" in
    --skip-model-config)
      if (( SKIP_MODEL_CONFIG )); then
        echo 'Duplicate first-run option: --skip-model-config' >&2
        exit 2
      fi
      SKIP_MODEL_CONFIG=1
      ;;
    --no-browser)
      if (( NO_BROWSER )); then
        echo 'Duplicate first-run option: --no-browser' >&2
        exit 2
      fi
      NO_BROWSER=1
      START_ARGS+=("$arg")
      ;;
    *)
      printf 'Unknown first-run option: %s\n' "$arg" >&2
      printf 'Usage: bash scripts/first-run-local-mvp.sh [--skip-model-config] [--no-browser]\n' >&2
      exit 2
      ;;
  esac
done

bootstrap_files=("$LOCAL_ENV" "$IDENTIFIERS" "$TOKEN_FILE")
bootstrap_present=0
for file in "${bootstrap_files[@]}"; do
  [[ ! -f "$file" ]] || ((bootstrap_present += 1))
done
if (( bootstrap_present != 0 && bootstrap_present != ${#bootstrap_files[@]} )); then
  cat >&2 <<'MSG'
Niccolò local bootstrap state is incomplete. Refusing to rerun migrations or rotate credentials over a partial installation.
Restore the matching .niccolo-local/local.env, identifiers.txt and token.txt set, or bootstrap a new dedicated database.
MSG
  exit 2
fi

if (( bootstrap_present == 0 )); then
  for program in psql node npm cargo rustc; do
    command -v "$program" >/dev/null || {
      printf 'Missing executable: %s\n' "$program" >&2
      exit 1
    }
  done
  if [[ -z "${NICCOLO_PG_ADMIN_URL:-}" ]]; then
    if [[ -t 0 && -t 1 ]]; then
      printf '%s' 'PostgreSQL database-owner URL (input hidden): ' >&2
      IFS= read -r -s NICCOLO_PG_ADMIN_URL
      printf '\n' >&2
      if [[ -z "$NICCOLO_PG_ADMIN_URL" ]]; then
        echo 'A PostgreSQL database-owner URL is required for first bootstrap.' >&2
        exit 2
      fi
      export NICCOLO_PG_ADMIN_URL
    else
      cat >&2 <<'MSG'
Niccolò has not been bootstrapped yet, and this run is non-interactive.
Export NICCOLO_PG_ADMIN_URL with a PostgreSQL database-owner connection, then run this command again.
MSG
      exit 2
    fi
  fi
  printf '%s\n' '[1/3] Bootstrapping the local MVP...'
  bash "$ROOT/scripts/bootstrap-local-mvp.sh"
else
  printf '%s\n' '[1/3] Existing local bootstrap found; leaving it unchanged.'
fi

# The database-owner credential is bootstrap-only. Do not leak it into model
# configuration helpers or the long-lived API process on this first-run path.
unset NICCOLO_PG_ADMIN_URL

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