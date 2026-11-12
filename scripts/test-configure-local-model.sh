#!/usr/bin/env bash
# Deterministic regression checks for the private local-model configuration helper.
set -Eeuo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
fixture_root="$(mktemp -d)"
cleanup(){ rm -rf "$fixture_root"; }
trap cleanup EXIT
mkdir -p "$fixture_root/scripts"
cp "$ROOT/scripts/configure-local-model.sh" "$fixture_root/scripts/configure-local-model.sh"
cd "$fixture_root"

run_cfg(){ bash scripts/configure-local-model.sh --from-env; }

# Keyless loopback is the only HTTP provider shape accepted.
NICCOLO_LLM_BASE_URL='http://127.0.0.1:18080/v1' \
NICCOLO_LLM_MODEL='local model' \
NICCOLO_LLM_PROVIDER='fixture' \
NICCOLO_LLM_API_KEY='' \
NICCOLO_LLM_JSON_MODE='YES' \
  run_cfg > loopback.log
source .niccolo-local/model.env
[[ "$NICCOLO_LLM_BASE_URL" == 'http://127.0.0.1:18080/v1' ]]
[[ "$NICCOLO_LLM_MODEL" == 'local model' ]]
[[ "$NICCOLO_LLM_JSON_MODE" == 'YES' ]]
grep -Fq 'API key: not required for this loopback HTTP endpoint' loopback.log

# Explicit environment must override an existing private default.
cat > .niccolo-local/model.env <<'ENV'
export NICCOLO_LLM_BASE_URL=https://old.example/v1
export NICCOLO_LLM_MODEL=old-model
export NICCOLO_LLM_API_KEY=old-secret
ENV
NICCOLO_LLM_BASE_URL='http://localhost:18080/v1' \
NICCOLO_LLM_MODEL='new-model' NICCOLO_LLM_API_KEY='' \
  run_cfg > override.log
source .niccolo-local/model.env
[[ "$NICCOLO_LLM_BASE_URL" == 'http://localhost:18080/v1' ]]
[[ "$NICCOLO_LLM_MODEL" == 'new-model' ]]
[[ -z "${NICCOLO_LLM_API_KEY:-}" ]]

# Remote HTTPS requires a key, but a key must never be echoed.
if NICCOLO_LLM_BASE_URL='https://provider.example/v1' NICCOLO_LLM_MODEL='remote' NICCOLO_LLM_API_KEY='' \
    run_cfg > missing-key.log 2>&1; then
  echo 'remote provider without API key unexpectedly accepted' >&2
  exit 1
fi
grep -Fq 'Remote HTTPS model providers require an API key.' missing-key.log
secret='sp ace$";[]'
NICCOLO_LLM_BASE_URL='https://provider.example/v1' NICCOLO_LLM_MODEL='remote' NICCOLO_LLM_API_KEY="$secret" \
  run_cfg > remote.log
! grep -Fq "$secret" remote.log
unset NICCOLO_LLM_API_KEY
source .niccolo-local/model.env
[[ "$NICCOLO_LLM_API_KEY" == "$secret" ]]

# Fail closed on endpoints that could leak credentials or smuggle URL material.
for bad_url in \
  'http://provider.example/v1' \
  'https://user:pass@provider.example/v1' \
  'https://provider.example/v1?key=secret' \
  'https://provider.example/v1#fragment' \
  'https://'; do
  if NICCOLO_LLM_BASE_URL="$bad_url" NICCOLO_LLM_MODEL='bad' NICCOLO_LLM_API_KEY='secret' \
      run_cfg > bad-url.log 2>&1; then
    echo "invalid model URL unexpectedly accepted: $bad_url" >&2
    exit 1
  fi
done

# Failed reconfiguration never replaces the last valid private file.
valid_digest="$(sha256sum .niccolo-local/model.env | awk '{print $1}')"
if NICCOLO_LLM_BASE_URL='https://provider.example/v1?key=bad' NICCOLO_LLM_MODEL='bad' NICCOLO_LLM_API_KEY='secret' \
    run_cfg >/dev/null 2>&1; then
  exit 1
fi
[[ "$(sha256sum .niccolo-local/model.env | awk '{print $1}')" == "$valid_digest" ]]

# CLI secret arguments are deliberately unsupported.
if bash scripts/configure-local-model.sh --api-key should-never-be-accepted >/dev/null 2>&1; then
  echo 'API key command-line argument unexpectedly accepted' >&2
  exit 1
fi

echo 'PASS: private model configuration helper boundaries'
