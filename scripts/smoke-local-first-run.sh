#!/usr/bin/env bash
# CI-only validation: disposable PostgreSQL database and freshly generated credentials.
# Never point this script at existing Niccolo data or a production database.
set -Eeuo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
: "${NICCOLO_PG_ADMIN_URL:?new disposable DB required}"
: "${NICCOLO_PG_SECURE_PASSWORD:?disposable service password required}"
command -v curl >/dev/null
command -v node >/dev/null
if [[ -e .niccolo-local/local.env ]]; then
  echo 'Refusing to run fresh bootstrap smoke over existing local credentials' >&2
  exit 1
fi
bash scripts/bootstrap-local-mvp.sh
test -s .niccolo-local/local.env
test -s .niccolo-local/identifiers.txt
test -s .niccolo-local/token.txt
# Both files are generated locally with UUIDs / shell-escaped values, never with arbitrary user input.
# shellcheck disable=SC1091
source .niccolo-local/local.env
# shellcheck disable=SC1091
source .niccolo-local/identifiers.txt
token="$(cat .niccolo-local/token.txt)"
[[ "$token" == ncl_* ]]
./target/release/niccolo-api > .niccolo-local/ci-server.log 2>&1 &
pid=$!
cleanup(){ kill "$pid" 2>/dev/null || true; wait "$pid" 2>/dev/null || true; }
trap cleanup EXIT
started=0
for _ in $(seq 1 40); do
  if curl --max-time 2 --silent --fail http://127.0.0.1:38080/healthz >/dev/null; then
    started=1
    break
  fi
  if ! kill -0 "$pid" 2>/dev/null; then
    echo 'Niccolo server exited during bootstrap smoke' >&2
    tail -n 30 .niccolo-local/ci-server.log >&2
    exit 1
  fi
  sleep 1
done
if [[ "$started" != 1 ]]; then
  echo 'Niccolo server failed to become healthy in bootstrap smoke' >&2
  tail -n 30 .niccolo-local/ci-server.log >&2
  exit 1
fi
curl --max-time 5 --silent --show-error --fail http://127.0.0.1:38080/ >/dev/null
path="http://127.0.0.1:38080/v1/workspaces/${WORKSPACE_UUID}/situations/${SITUATION_UUID}"
curl --max-time 10 --silent --show-error --fail \
  -H "Authorization: Bearer ${token}" \
  -H 'Content-Type: application/json' \
  --data '{"focus":"First-run MVP bootstrap smoke"}' "$path" >/dev/null
curl --max-time 10 --silent --show-error --fail \
  -H "Authorization: Bearer ${token}" "$path" | \
  node -e 'let s="";process.stdin.on("data",c=>s+=c);process.stdin.on("end",()=>{const v=JSON.parse(s);if(v.focus!=="First-run MVP bootstrap smoke")process.exit(1);});'
echo 'PASS: fresh local bootstrap, embedded web UI, bearer-authenticated Situation creation/read'
