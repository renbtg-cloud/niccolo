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
# Authentication and workspace authorization must fail closed on the same
# freshly provisioned server, not just in unit-level fixtures.
unauthenticated="$(curl --max-time 10 --silent --output /dev/null --write-out '%{http_code}' "$path")"
[[ "$unauthenticated" == 401 ]] || {
  echo "Expected 401 without token, got $unauthenticated" >&2
  exit 1
}
malformed="$(curl --max-time 10 --silent --output /dev/null --write-out '%{http_code}' \
  -H 'Authorization: Bearer invalid-token' "$path")"
[[ "$malformed" == 401 ]] || {
  echo "Expected 401 for invalid token, got $malformed" >&2
  exit 1
}
other_workspace="$(node -p "require('node:crypto').randomUUID()")"
other_path="http://127.0.0.1:38080/v1/workspaces/${other_workspace}/situations/${SITUATION_UUID}"
denied="$(curl --max-time 10 --silent --output /dev/null --write-out '%{http_code}' \
  -H "Authorization: Bearer ${token}" "$other_path")"
[[ "$denied" == 403 ]] || {
  echo "Expected 403 across workspaces, got $denied" >&2
  exit 1
}
# Import a WhatsApp export through the same API used by React. Verify byte-exact
# retention, two attributed fragments, safe replay, and Situation attachment.
source_file=".niccolo-local/ci-whatsapp.txt"
printf '[08/11/2026, 09:00] Ana: Vou chegar ao meio-dia.\n[08/11/2026, 09:01] Beto: Combinado.\n' > "$source_file"
document_id="$(node -p "require('node:crypto').randomUUID()")"
source_path="http://127.0.0.1:38080/v1/workspaces/${WORKSPACE_UUID}/sources/${document_id}"
source_result="$(curl --max-time 10 --silent --show-error --fail \
  -H "Authorization: Bearer ${token}" \
  -H 'Content-Type: text/plain' -H 'X-Source-Locator: ci-whatsapp.txt' \
  --data-binary @"$source_file" "$source_path")"
printf '%s' "$source_result" | node -e '
let s="";process.stdin.on("data",x=>s+=x);process.stdin.on("end",()=>{
 const j=JSON.parse(s);
 if(j.replayed!==false||j.fragment_ids?.length!==2)process.exit(1);
});'
replay="$(curl --max-time 10 --silent --show-error --fail \
  -H "Authorization: Bearer ${token}" \
  -H 'Content-Type: text/plain' -H 'X-Source-Locator: ci-whatsapp.txt' \
  --data-binary @"$source_file" "$source_path")"
printf '%s' "$replay" | node -e '
let s="";process.stdin.on("data",x=>s+=x);process.stdin.on("end",()=>{
 const j=JSON.parse(s);
 if(j.replayed!==true||j.fragment_ids?.length!==2)process.exit(1);
});'
speakers="$(curl --max-time 10 --silent --show-error --fail \
  -H "Authorization: Bearer ${token}" "${source_path}/speakers")"
printf '%s' "$speakers" | node -e '
let s="";process.stdin.on("data",x=>s+=x);process.stdin.on("end",()=>{
 const j=JSON.parse(s);
 if(j.speaker_labels_are_verified_identities!==false ||
    !j.speakers?.some(x=>x.speaker_label==="Ana") ||
    !j.speakers?.some(x=>x.speaker_label==="Beto"))process.exit(1);
});'
curl --max-time 10 --silent --show-error --fail \
  -H "Authorization: Bearer ${token}" \
  "${source_path}/original" -o ".niccolo-local/ci-original.txt"
cmp "$source_file" ".niccolo-local/ci-original.txt"
catalog="$(curl --max-time 10 --silent --show-error --fail \
  -H "Authorization: Bearer ${token}" \
  "http://127.0.0.1:38080/v1/workspaces/${WORKSPACE_UUID}/sources")"
DOCUMENT_ID="$document_id" node -e '
let s="";process.stdin.on("data",x=>s+=x);process.stdin.on("end",()=>{
 const j=JSON.parse(s);
 const d=j.documents?.find(x=>x.document_id===process.env.DOCUMENT_ID);
 if(!d||!d.original_bytes_available||d.fragment_count!==2)process.exit(1);
});' <<< "$catalog"
attach="$(curl --max-time 10 --silent --show-error --fail \
  -X POST -H "Authorization: Bearer ${token}" \
  "${path}/sources/${document_id}")"
DOCUMENT_ID="$document_id" node -e '
let s="";process.stdin.on("data",x=>s+=x);process.stdin.on("end",()=>{
 const j=JSON.parse(s);
 if(!j.source_document_ids?.includes(process.env.DOCUMENT_ID))process.exit(1);
});' <<< "$attach"
denied_source="$(curl --max-time 10 --silent --output /dev/null --write-out '%{http_code}' \
  -H "Authorization: Bearer ${token}" \
  "http://127.0.0.1:38080/v1/workspaces/${other_workspace}/sources/${document_id}/original")"
[[ "$denied_source" == 403 ]] || {
  echo "Expected 403 for cross-workspace source download, got $denied_source" >&2
  exit 1
}
echo 'PASS: source upload/replay/attribution/original bytes/attachment'

echo 'PASS: bootstrap, embedded UI, Situation lifecycle, and auth isolation'

