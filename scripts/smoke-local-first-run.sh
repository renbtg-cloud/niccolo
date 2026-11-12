#!/usr/bin/env bash
# CI-only validation: disposable PostgreSQL database and freshly generated credentials.
# Never point this script at existing Niccolo data or a production database.
set -Eeuo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
: "${NICCOLO_PG_ADMIN_URL:?new disposable DB required}"
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
test -s .niccolo-local/prepared-url.txt
grep -Fq 'NICCOLO_PG_SECURE_URL=' .niccolo-local/local.env
# Both files are generated locally with UUIDs / shell-escaped values, never with arbitrary user input.
# shellcheck disable=SC1091
source .niccolo-local/local.env
runtime_password="$(NICCOLO_RUNTIME_URL="$NICCOLO_PG_SECURE_URL" node -e 'process.stdout.write(new URL(process.env.NICCOLO_RUNTIME_URL).password)')"
admin_password="$(NICCOLO_ADMIN_URL="$NICCOLO_PG_ADMIN_URL" node -e 'process.stdout.write(new URL(process.env.NICCOLO_ADMIN_URL).password)')"
[[ -n "$runtime_password" && "$runtime_password" != "$admin_password" ]]
unset runtime_password admin_password
# shellcheck disable=SC1091
source .niccolo-local/identifiers.txt
# Persist the already-exported loopback model configuration through the same
# helper an ordinary local user can run, then prove start reads it back.
bash scripts/configure-local-model.sh --from-env > .niccolo-local/model-config.log
test -s .niccolo-local/model.env
grep -Fq 'API key: not required for this loopback HTTP endpoint' .niccolo-local/model-config.log
unset NICCOLO_LLM_BASE_URL NICCOLO_LLM_MODEL NICCOLO_LLM_PROVIDER NICCOLO_LLM_API_KEY NICCOLO_LLM_JSON_MODE
prepared_output="$(bash scripts/start-local-mvp.sh --print-only)"
grep -Fq 'Reasoning model: configured (niccolo-ci-protocol-smoke)' <<< "$prepared_output"
grep -Fq "workspace=${WORKSPACE_UUID}&situation=${SITUATION_UUID}" <<< "$prepared_output"
grep -Fq "Bearer token: .niccolo-local/token.txt" <<< "$prepared_output"
! grep -Fq "$(cat .niccolo-local/token.txt)" <<< "$prepared_output"
# The browser opener must wait for readiness and pass only the non-secret prepared URL.
opener_dir="$(mktemp -d)"
opener_capture="$(mktemp)"
cat > "$opener_dir/uname" <<'SH'
#!/usr/bin/env bash
printf 'Linux\n'
SH
cat > "$opener_dir/curl" <<'SH'
#!/usr/bin/env bash
exit 0
SH
cat > "$opener_dir/xdg-open" <<'SH'
#!/usr/bin/env bash
[[ -z "${NICCOLO_LLM_API_KEY:-}" && -z "${NICCOLO_PG_SECURE_PASSWORD:-}" && -z "${PGPASSWORD:-}" ]] || exit 90
printf '%s\n' "$1" > "$NICCOLO_BROWSER_CAPTURE"
SH
chmod +x "$opener_dir/uname" "$opener_dir/curl" "$opener_dir/xdg-open"
expected_prepared_url="http://127.0.0.1:38080/?workspace=${WORKSPACE_UUID}&situation=${SITUATION_UUID}"
DISPLAY=:99 NICCOLO_BROWSER_CAPTURE="$opener_capture" PATH="$opener_dir:$PATH" \
  NICCOLO_LLM_API_KEY='must-not-reach-browser' NICCOLO_PG_SECURE_PASSWORD='must-not-reach-browser' \
  PGPASSWORD='must-not-reach-browser' \
  bash scripts/open-local-mvp-browser.sh "$expected_prepared_url" 38080
grep -Fxq "$expected_prepared_url" "$opener_capture"
! grep -Fq "$(cat .niccolo-local/token.txt)" "$opener_capture" 2>/dev/null || {
  echo 'Browser opener leaked the bearer token' >&2
  exit 1
}
rm -rf "$opener_dir" "$opener_capture"
if NICCOLO_LLM_BASE_URL='https://provider.example/v1' NICCOLO_LLM_MODEL='example-model' NICCOLO_LLM_API_KEY='' \
    bash scripts/start-local-mvp.sh --print-only > .niccolo-local/incomplete-model.log 2>&1; then
  echo 'Start helper accepted an incomplete remote model configuration' >&2
  exit 1
fi
grep -Fq 'Remote HTTPS model providers require NICCOLO_LLM_API_KEY.' .niccolo-local/incomplete-model.log
if NICCOLO_LLM_BASE_URL='https://provider.example/v1' NICCOLO_LLM_MODEL='example-model' NICCOLO_LLM_API_KEY='' \
    bash scripts/configure-local-model.sh --from-env > .niccolo-local/incomplete-config.log 2>&1; then
  echo 'Model configuration helper accepted a remote provider without an API key' >&2
  exit 1
fi
grep -Fq 'Remote HTTPS model providers require an API key.' .niccolo-local/incomplete-config.log
# Failed reconfiguration must leave the last valid private configuration intact.
grep -Fq 'niccolo-ci-protocol-smoke' .niccolo-local/model.env
token="$(cat .niccolo-local/token.txt)"
[[ "$token" == ncl_* ]]
probe_principal="$(node -p "require('node:crypto').randomUUID()")"
if NICCOLO_PG_ADMIN_URL="$NICCOLO_PG_SECURE_URL" \
    ./target/release/niccolo-provision mint "$WORKSPACE_UUID" "$probe_principal" read 1 \
    > .niccolo-local/runtime-provision.log 2>&1; then
  echo 'Restricted runtime login unexpectedly provisioned a bearer token' >&2
  exit 1
fi
grep -Fq 'database-owner or BYPASSRLS login' .niccolo-local/runtime-provision.log
NICCOLO_OPEN_BROWSER=0 bash scripts/start-local-mvp.sh > .niccolo-local/ci-server.log 2>&1 &
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

# A second source format must coexist with the chat in one Situation.
eml_file=".niccolo-local/ci-mail.eml"
printf 'From: Ana <ana@example.test>\r\nContent-Type: text/plain; charset=UTF-8\r\n\r\nO encontro está confirmado.\r\n' > "$eml_file"
email_id="$(node -p "require('node:crypto').randomUUID()")"
email_path="http://127.0.0.1:38080/v1/workspaces/${WORKSPACE_UUID}/sources/${email_id}"
email_result="$(curl --max-time 10 --silent --show-error --fail \
  -H "Authorization: Bearer ${token}" \
  -H 'Content-Type: message/rfc822' -H 'X-Source-Locator: ci-mail.eml' \
  --data-binary @"$eml_file" "$email_path")"
printf '%s' "$email_result" | node -e '
let s="";process.stdin.on("data",x=>s+=x);process.stdin.on("end",()=>{
 const j=JSON.parse(s);
 if(j.replayed!==false||j.fragment_ids?.length!==1)process.exit(1);
});'
curl --max-time 10 --silent --show-error --fail \
  -H "Authorization: Bearer ${token}" \
  "${email_path}/original" -o ".niccolo-local/ci-original.eml"
cmp "$eml_file" ".niccolo-local/ci-original.eml"
curl --max-time 10 --silent --show-error --fail -X POST \
  -H "Authorization: Bearer ${token}" \
  "${path}/sources/${email_id}" >/dev/null
sources_attached="$(curl --max-time 10 --silent --show-error --fail \
  -H "Authorization: Bearer ${token}" "$path")"
WHATSAPP_ID="$document_id" EMAIL_ID="$email_id" node -e '
let s="";process.stdin.on("data",x=>s+=x);process.stdin.on("end",()=>{
 const j=JSON.parse(s);
 if(!j.source_document_ids?.includes(process.env.WHATSAPP_ID) ||
    !j.source_document_ids?.includes(process.env.EMAIL_ID))process.exit(1);
});' <<< "$sources_attached"
echo 'PASS: two source formats attached to one Situation'

# Human context has distinct provenance from source evidence and must be
# correctable/retractable without rewriting previous Situation revisions.
context_id="$(node -p "require('node:crypto').randomUUID()")"
context_path="${path}/contributions/${context_id}"
context_result="$(curl --max-time 10 --silent --show-error --fail \
  -H "Authorization: Bearer ${token}" \
  -H 'Content-Type: application/json' \
  --data '{"kind":"user_reported","text":"I remember a phone call; not verified by records.","related_evidence_id":null}' \
  "$context_path")"
before_correction="$(CONTRIBUTION_ID="$context_id" node -e '
let s="";process.stdin.on("data",x=>s+=x);process.stdin.on("end",()=>{
 const j=JSON.parse(s);
 const c=j.contributions?.find(x=>x.contribution_id===process.env.CONTRIBUTION_ID);
 if(!c||c.is_verified_evidence!==false||j.replayed!==false ||
    !Number.isInteger(j.revision))process.exit(1);
 console.log(j.revision);
});' <<< "$context_result")"
replayed_context="$(curl --max-time 10 --silent --show-error --fail \
  -H "Authorization: Bearer ${token}" \
  -H 'Content-Type: application/json' \
  --data '{"kind":"user_reported","text":"I remember a phone call; not verified by records.","related_evidence_id":null}' \
  "$context_path")"
printf '%s' "$replayed_context" | node -e '
let s="";process.stdin.on("data",x=>s+=x);process.stdin.on("end",()=>{
 const j=JSON.parse(s);if(j.replayed!==true||j.contributions?.length!==1)process.exit(1);
});'
replacement_id="$(node -p "require('node:crypto').randomUUID()")"
corrected="$(curl --max-time 10 --silent --show-error --fail \
  -H "Authorization: Bearer ${token}" \
  -H 'Content-Type: application/json' \
  --data '{"text":"Correction: this was only a guess, not a remembered call.","related_evidence_id":null}' \
  "${context_path}/corrections/${replacement_id}")"
CONTRIBUTION_ID="$replacement_id" node -e '
let s="";process.stdin.on("data",x=>s+=x);process.stdin.on("end",()=>{
 const j=JSON.parse(s);
 const c=j.contributions?.find(x=>x.contribution_id===process.env.CONTRIBUTION_ID);
 if(!c||!c.text.startsWith("Correction:")||c.is_verified_evidence!==false ||
    j.contributions.length!==1)process.exit(1);
});' <<< "$corrected"
old_revision="$(curl --max-time 10 --silent --show-error --fail \
  -H "Authorization: Bearer ${token}" "${path}/revisions/${before_correction}")"
CONTRIBUTION_ID="$context_id" node -e '
let s="";process.stdin.on("data",x=>s+=x);process.stdin.on("end",()=>{
 const j=JSON.parse(s);
 if(!j.contributions?.some(x=>x.contribution_id===process.env.CONTRIBUTION_ID))
    process.exit(1);
});' <<< "$old_revision"
retracted="$(curl --max-time 10 --silent --show-error --fail -X POST \
  -H "Authorization: Bearer ${token}" \
  -H 'Content-Type: application/json' \
  --data '{}' "${path}/contributions/${replacement_id}/retract")"
printf '%s' "$retracted" | node -e '
let s="";process.stdin.on("data",x=>s+=x);process.stdin.on("end",()=>{
 const j=JSON.parse(s);
 if(j.contributions?.length!==0||j.source_document_ids?.length!==2)
    process.exit(1);
});'
echo 'PASS: context provenance, idempotent replay, correction, frozen history, retraction'

# With the CI-only OpenAI-compatible fixture configured, prove the same Analyze
# path used by the React UI: structured updates -> synthesis -> material challenge ->
# explicit human boundary -> automatic continuation -> provenance-bearing handoff.
readiness="$(curl --max-time 10 --silent --show-error --fail http://127.0.0.1:38080/readyz)"
printf '%s' "$readiness" | node -e '
let s="";process.stdin.on("data",x=>s+=x);process.stdin.on("end",()=>{
 const j=JSON.parse(s);if(j.status!=="ready"||j.model_configured!==true)process.exit(1);
});'
operation_id="$(node -p "require('node:crypto').randomUUID()")"
analysis_id="$(node -p "require('node:crypto').randomUUID()")"
intent_body="$(OPERATION_ID="$operation_id" ANALYSIS_ID="$analysis_id" node -e '
process.stdout.write(JSON.stringify({
 analysis_id:process.env.ANALYSIS_ID,intent:"UNDERSTAND",
 goal:"What do the imported records establish, what remains uncertain, and which ordinary alternative should stay alive?",
 reanalysis_of:null,idempotency_key:`ci-intent-${process.env.OPERATION_ID}`,step_budget:32,
 context:{surface:"ci-first-run",human_owns_project:true}
}));')"
intent_result="$(curl --max-time 60 --silent --show-error --fail \
  -H "Authorization: Bearer ${token}" -H 'Content-Type: application/json' \
  --data-binary "$intent_body" "${path}/bff/intents/${operation_id}/sync")"
ANALYSIS_ID="$analysis_id" OPERATION_ID="$operation_id" node -e '
let s="";process.stdin.on("data",x=>s+=x);process.stdin.on("end",()=>{
 const j=JSON.parse(s);
 if(j.analysis_id!==process.env.ANALYSIS_ID||j.operation?.operation_id!==process.env.OPERATION_ID||
    j.operation?.state!=="WAITING_FOR_HUMAN"||j.operation?.terminal!==false)process.exit(1);
});' <<< "$intent_result"

controller="$(curl --max-time 10 --silent --show-error --fail \
  -H "Authorization: Bearer ${token}" "${path}/bff/operations/${operation_id}/agentic")"
waiting_step="$(printf '%s' "$controller" | node -e '
let s="";process.stdin.on("data",x=>s+=x);process.stdin.on("end",()=>{
 const j=JSON.parse(s),step=j.steps?.find(x=>x.state==="WAITING_FOR_HUMAN");
 if(!j.waiting_for_human||!step||step.kind!=="REQUEST_HUMAN_DECISION")process.exit(1);
 process.stdout.write(step.step_id);
});')"
response_id="$(node -p "require('node:crypto').randomUUID()")"
human_body="$(RESPONSE_ID="$response_id" node -e 'process.stdout.write(JSON.stringify({response_id:process.env.RESPONSE_ID,decision:"CONTINUE",response_text:"Continue, but preserve the unresolved explanation and material coverage challenge."}))')"
human_result="$(curl --max-time 10 --silent --show-error --fail \
  -H "Authorization: Bearer ${token}" -H 'Content-Type: application/json' \
  --data-binary "$human_body" \
  "${path}/bff/operations/${operation_id}/agentic/steps/${waiting_step}/human-response")"
printf '%s' "$human_result" | node -e '
let s="";process.stdin.on("data",x=>s+=x);process.stdin.on("end",()=>{
 const j=JSON.parse(s);
 if(j.resolution?.operation_state!=="RUNNING"||j.resolution?.human_response_persisted!==true||
    j.resolution?.system_inferred_consent!==false||
    j.resolution?.executor_resume_required!==true)process.exit(1);
});'

# Human-response submission itself must resume execution; the client does not
# make a hidden second "run" request.
operation_state=""
for _ in $(seq 1 80); do
  operation_json="$(curl --max-time 10 --silent --show-error --fail \
    -H "Authorization: Bearer ${token}" "${path}/bff/operations/${operation_id}")"
  operation_state="$(printf '%s' "$operation_json" | node -e '
let s="";process.stdin.on("data",x=>s+=x);process.stdin.on("end",()=>process.stdout.write(JSON.parse(s).state));')"
  [[ "$operation_state" == "SUCCEEDED" || "$operation_state" == "FAILED" ]] && break
  sleep .125
done
[[ "$operation_state" == "SUCCEEDED" ]] || {
  echo "Expected automatic post-human continuation to SUCCEEDED, got $operation_state" >&2
  exit 1
}
printf '%s' "$operation_json" | node -e '
let s="";process.stdin.on("data",x=>s+=x);process.stdin.on("end",()=>{
 const j=JSON.parse(s),q=j.result?.quality;
 if(j.terminal!==true||j.result?.explicit_human_review_occurred!==true||
    q?.claim_count!==1||q?.alternative_count!==1||q?.assumption_count!==2||
    q?.clarification_count!==1||q?.open_evidence_gap_count!==1||
    q?.adversarial_challenge_count!==1||q?.open_material_challenge_count!==1||
    q?.provisional!==true||q?.human_attention_recommended!==true)process.exit(1);
});'

events_before_replay="$(psql -X -qAt -v ON_ERROR_STOP=1 \
  -v workspace="$WORKSPACE_UUID" -v operation="$operation_id" -v step="$waiting_step" \
  "$NICCOLO_PG_ADMIN_URL" <<'SQL'
BEGIN;
SELECT set_config('niccolo.workspace_id', :'workspace', true) \gset
SELECT count(*) FROM niccolo.agentic_step_events
WHERE operation_id=:'operation'::uuid AND step_id=:'step'::uuid;
ROLLBACK;
SQL
)"
replay_file="$(mktemp)"
replay_status="$(curl --max-time 10 --silent --show-error -o "$replay_file" -w '%{http_code}' \
  -H "Authorization: Bearer ${token}" -H 'Content-Type: application/json' \
  --data-binary "$human_body" \
  "${path}/bff/operations/${operation_id}/agentic/steps/${waiting_step}/human-response")"
[[ "$replay_status" == "200" ]] || {
  echo "Expected exact human-response replay HTTP 200, got $replay_status" >&2
  cat "$replay_file" >&2
  exit 1
}
node -e '
const fs=require("fs"),j=JSON.parse(fs.readFileSync(process.argv[1],"utf8"));
if(j.resolution?.operation_state!=="SUCCEEDED"||
   j.resolution?.human_response_persisted!==true||
   j.resolution?.system_inferred_consent!==false||
   j.resolution?.executor_resume_required!==false)process.exit(1);
' "$replay_file"
events_after_replay="$(psql -X -qAt -v ON_ERROR_STOP=1 \
  -v workspace="$WORKSPACE_UUID" -v operation="$operation_id" -v step="$waiting_step" \
  "$NICCOLO_PG_ADMIN_URL" <<'SQL'
BEGIN;
SELECT set_config('niccolo.workspace_id', :'workspace', true) \gset
SELECT count(*) FROM niccolo.agentic_step_events
WHERE operation_id=:'operation'::uuid AND step_id=:'step'::uuid;
ROLLBACK;
SQL
)"
[[ "$events_before_replay" == "$events_after_replay" ]] || {
  echo "Exact human-response replay duplicated step events: $events_before_replay -> $events_after_replay" >&2
  exit 1
}
different_response_id="$(node -p "require('node:crypto').randomUUID()")"
different_body="$(RESPONSE_ID="$different_response_id" node -e 'process.stdout.write(JSON.stringify({response_id:process.env.RESPONSE_ID,decision:"CONTINUE",response_text:"Continue, but preserve the unresolved explanation and material coverage challenge."}))')"
different_status="$(curl --max-time 10 --silent --output /dev/null --write-out '%{http_code}' \
  -H "Authorization: Bearer ${token}" -H 'Content-Type: application/json' \
  --data-binary "$different_body" \
  "${path}/bff/operations/${operation_id}/agentic/steps/${waiting_step}/human-response")"
[[ "$different_status" == "409" ]] || {
  echo "Expected different human response_id to conflict, got $different_status" >&2
  exit 1
}
rm -f "$replay_file"
echo 'PASS: human response replay is exact-idempotent and side-effect free'

analysis_output="$(curl --max-time 10 --silent --show-error --fail \
  -H "Authorization: Bearer ${token}" "${path}/analysis-snapshots/${analysis_id}/output")"
printf '%s' "$analysis_output" | node -e '
let s="";process.stdin.on("data",x=>s+=x);process.stdin.on("end",()=>{
 const j=JSON.parse(s);
 if(j.answer_text!=="The imported records contain attributed statements, but this fixture does not infer motive or truth beyond those records."||
    j.coverage_note!=="Protocol validation used the frozen imported records; unverified context remains explicitly unresolved."||
    j.input_is_current!==true||j.historical_output_preserved!==true)process.exit(1);
});'

coverage="$(curl --max-time 10 --silent --show-error --fail \
  -H "Authorization: Bearer ${token}" "${path}/analysis-snapshots/${analysis_id}/coverage")"
printf '%s' "$coverage" | node -e '
let s="";process.stdin.on("data",x=>s+=x);process.stdin.on("end",()=>{
 const j=JSON.parse(s),docs=j.documents??[];
 if(j.coverage_scope!=="FROZEN_ANALYSIS_INPUT"||docs.length!==2||
    j.source_present_does_not_mean_examined!==true||
    j.all_catalogued_fragments_examined_does_not_prove_unsupported_media_examined!==true||
    docs.some(d=>!(d.catalogued_fragment_count>0)||
                 d.examined_fragment_count!==d.catalogued_fragment_count||
                 d.all_catalogued_fragments_examined!==true||
                 d.proves_full_source_examined!==false))process.exit(1);
});'
echo 'PASS: frozen-input coverage distinguishes examined fragments from whole-source proof'

handoff="$(curl --max-time 10 --silent --show-error --fail \
  -H "Authorization: Bearer ${token}" "${path}/analysis-snapshots/${analysis_id}/bff/handoff")"
printf '%s' "$handoff" | node -e '
let s="";process.stdin.on("data",x=>s+=x);process.stdin.on("end",()=>{
 const j=JSON.parse(s).handoff;
 if(!j||j.candidate_interpretations?.length!==1||j.claims?.length!==1||
    j.clarifications?.length!==1||j.hypothesis_portfolio?.hypotheses?.length!==1||
    j.candidate_interpretations[0].is_fact!==false||
    j.hypothesis_portfolio.portfolio_establishes_truth!==false||
    j.decisions?.human_owns_decision!==true||j.handoff_executes_actions!==false)process.exit(1);
});'

ledger_summary="$(psql -X -qAt -v ON_ERROR_STOP=1 \
  -v workspace="$WORKSPACE_UUID" -v operation="$operation_id" -v analysis="$analysis_id" \
  "$NICCOLO_PG_ADMIN_URL" <<'SQL'
BEGIN;
SELECT set_config('niccolo.workspace_id', :'workspace', true) \gset
SELECT concat_ws('|',
  (SELECT count(*) FROM niccolo.model_invocations WHERE operation_id=:'operation'::uuid),
  (SELECT count(*) FROM niccolo.model_invocations WHERE operation_id=:'operation'::uuid AND status='SUCCEEDED'),
  (SELECT count(*) FROM niccolo.model_invocations WHERE operation_id=:'operation'::uuid AND response_sha256 IS NOT NULL),
  (SELECT count(DISTINCT (request_json::jsonb)->>'purpose') FROM niccolo.model_invocations WHERE operation_id=:'operation'::uuid),
  (SELECT count(*) FROM niccolo.situation_analysis_claims WHERE analysis_id=:'analysis'::uuid),
  (SELECT count(*) FROM niccolo.situation_analysis_claim_fragments WHERE analysis_id=:'analysis'::uuid),
  (SELECT count(*) FROM niccolo.situation_analysis_assumptions WHERE analysis_id=:'analysis'::uuid),
  (SELECT count(*) FROM niccolo.situation_analysis_candidate_interpretations WHERE analysis_id=:'analysis'::uuid),
  (SELECT count(*) FROM niccolo.situation_analysis_hypothesis_evidence WHERE analysis_id=:'analysis'::uuid),
  (SELECT count(*) FROM niccolo.situation_analysis_evidence_gaps WHERE analysis_id=:'analysis'::uuid),
  (SELECT count(*) FROM niccolo.situation_analysis_clarification_questions WHERE analysis_id=:'analysis'::uuid),
  (SELECT count(*) FROM niccolo.situation_analysis_challenges WHERE analysis_id=:'analysis'::uuid),
  (SELECT count(*) FROM niccolo.agentic_human_responses WHERE operation_id=:'operation'::uuid)
);
ROLLBACK;
SQL
)"
[[ "$ledger_summary" == '6|6|6|6|1|1|2|1|1|1|1|1|1' ]] || {
  echo "Unexpected rich Analyze persistence summary: $ledger_summary" >&2
  exit 1
}
echo 'PASS: rich Analyze provenance, human boundary, automatic resume, and handoff'

# A forensic export may come back into Niccolò only through quarantine. Prove
# the first-run service can round-trip its own package without turning historical
# rows into current evidence, and that a human must explicitly bind one item as
# non-evidence context for a chosen analysis.
forensic_file=".niccolo-local/ci-forensic-export.json"
curl --max-time 30 --silent --show-error --fail \
  -H "Authorization: Bearer ${token}" \
  "${path}/forensic-export" -o "$forensic_file"
node -e '
const fs=require("node:fs"),j=JSON.parse(fs.readFileSync(process.argv[1],"utf8"));
if(j.format!=="niccolo.situation-forensic-export"||j.format_version!==1||
   j.structural_validity_establishes_truth!==false||
   j.included_model_output_is_documentary_evidence!==false||
   j.imported_state_should_be_trusted_automatically!==false)process.exit(1);
' "$forensic_file"
forensic_batch="$(node -p "require('node:crypto').randomUUID()")"
forensic_request=".niccolo-local/ci-forensic-import-request.json"
FORENSIC_FILE="$forensic_file" node -e '
const fs=require("node:fs");
const pkg=JSON.parse(fs.readFileSync(process.env.FORENSIC_FILE,"utf8"));
process.stdout.write(JSON.stringify({declared_origin:"ci://first-run-self-export",package:pkg}));
' > "$forensic_request"
forensic_import="$(curl --max-time 60 --silent --show-error --fail \
  -H "Authorization: Bearer ${token}" -H 'Content-Type: application/json' \
  --data-binary @"$forensic_request" \
  "http://127.0.0.1:38080/v1/workspaces/${WORKSPACE_UUID}/forensic-imports/${forensic_batch}")"
FORENSIC_BATCH="$forensic_batch" node -e '
let s="";process.stdin.on("data",x=>s+=x);process.stdin.on("end",()=>{
 const j=JSON.parse(s);
 if(j.import_batch_id!==process.env.FORENSIC_BATCH||j.import_mode!=="UNTRUSTED"||
    j.verification_status!=="STRUCTURALLY_VERIFIED"||
    j.structural_validity_establishes_truth!==false||
    j.current_credentials_upgrade_imported_statements!==false||
    j.imported_model_output_is_documentary_evidence!==false)process.exit(1);
});' <<< "$forensic_import"
forensic_batches="$(curl --max-time 10 --silent --show-error --fail \
  -H "Authorization: Bearer ${token}" \
  "http://127.0.0.1:38080/v1/workspaces/${WORKSPACE_UUID}/forensic-imports?limit=25")"
FORENSIC_BATCH="$forensic_batch" node -e '
let s="";process.stdin.on("data",x=>s+=x);process.stdin.on("end",()=>{
 const j=JSON.parse(s);
 if(!j.batches?.some(x=>x.import_batch_id===process.env.FORENSIC_BATCH&&x.import_mode==="UNTRUSTED"))process.exit(1);
});' <<< "$forensic_batches"
forensic_items="$(curl --max-time 10 --silent --show-error --fail \
  -H "Authorization: Bearer ${token}" \
  "http://127.0.0.1:38080/v1/workspaces/${WORKSPACE_UUID}/forensic-imports/${forensic_batch}/items?limit=200")"
forensic_item="$(node -e '
let s="";process.stdin.on("data",x=>s+=x);process.stdin.on("end",()=>{
 const j=JSON.parse(s),x=j.items?.find(x=>x.source_kind==="LOGICAL_ROW");
 if(!x||x.documentary_evidence!==false||x.current_credentials_establish_truth!==false)process.exit(1);
 process.stdout.write(x.item_id);
});' <<< "$forensic_items")"
mapfile -t forensic_original_meta < <(node -e '
let s="";process.stdin.on("data",x=>s+=x);process.stdin.on("end",()=>{
 const j=JSON.parse(s),x=j.items?.find(x=>x.source_kind==="ORIGINAL_SOURCE_BYTES");
 if(!x||x.documentary_evidence!==false||x.current_credentials_establish_truth!==false)process.exit(1);
 process.stdout.write(x.item_id+"\n"+x.source_sha256+"\n");
});' <<< "$forensic_items")
forensic_original_item="${forensic_original_meta[0]:-}"
forensic_original_sha="${forensic_original_meta[1]:-}"
[[ -n "$forensic_original_item" && -n "$forensic_original_sha" ]] || {
  echo 'Forensic import did not retain an original-source quarantine item' >&2
  exit 1
}
forensic_original_headers=".niccolo-local/ci-forensic-original.headers"
forensic_original_file=".niccolo-local/ci-forensic-original.bin"
curl --max-time 30 --silent --show-error --fail \
  -D "$forensic_original_headers" -o "$forensic_original_file" \
  -H "Authorization: Bearer ${token}" \
  "http://127.0.0.1:38080/v1/workspaces/${WORKSPACE_UUID}/forensic-imports/${forensic_batch}/items/${forensic_original_item}/original"
grep -qi '^content-type: application/octet-stream' "$forensic_original_headers"
grep -qi '^cache-control: no-store' "$forensic_original_headers"
grep -qi '^x-niccolo-quarantine: true' "$forensic_original_headers"
grep -qi '^x-niccolo-documentary-evidence: false' "$forensic_original_headers"
[[ "$(sha256sum "$forensic_original_file" | awk '{print $1}')" == "$forensic_original_sha" ]] || {
  echo 'Quarantined original bytes do not match imported source digest' >&2
  exit 1
}
forensic_binding="$(node -p "require('node:crypto').randomUUID()")"
bind_body="$(FORENSIC_BATCH="$forensic_batch" FORENSIC_ITEM="$forensic_item" node -e '
process.stdout.write(JSON.stringify({
 import_batch_id:process.env.FORENSIC_BATCH,
 item_id:process.env.FORENSIC_ITEM,
 note:"CI selected historical context; never treat it as fact or documentary evidence."
}));')"
bound="$(curl --max-time 10 --silent --show-error --fail \
  -H "Authorization: Bearer ${token}" -H 'Content-Type: application/json' \
  --data-binary "$bind_body" \
  "${path}/analysis-snapshots/${analysis_id}/forensic-context/${forensic_binding}")"
FORENSIC_BINDING="$forensic_binding" node -e '
let s="";process.stdin.on("data",x=>s+=x);process.stdin.on("end",()=>{
 const j=JSON.parse(s).item;
 if(j.binding_id!==process.env.FORENSIC_BINDING||j.quarantine_provenance!==true||
    j.establishes_truth!==false||j.documentary_evidence!==false||
    j.current_credentials_upgrade_imported_statement!==false)process.exit(1);
});' <<< "$bound"
bound_context="$(curl --max-time 10 --silent --show-error --fail \
  -H "Authorization: Bearer ${token}" \
  "${path}/analysis-snapshots/${analysis_id}/forensic-context")"
FORENSIC_BINDING="$forensic_binding" node -e '
let s="";process.stdin.on("data",x=>s+=x);process.stdin.on("end",()=>{
 const j=JSON.parse(s);
 if(!j.items?.some(x=>x.binding_id===process.env.FORENSIC_BINDING)||
    j.items_are_documentary_evidence!==false||j.part_of_frozen_analysis_input!==false)process.exit(1);
});' <<< "$bound_context"
echo 'PASS: forensic export round-trip stays quarantined and requires explicit non-evidence binding'

# Publication planning must be durable and exact without contacting GitHub or
# accepting a provider secret. The first network-capable publisher is a later
# boundary; this smoke proves we do not confuse intent with remote success.
git_target="$(node -p "require('node:crypto').randomUUID()")"
git_target_body='{"repository_owner":"niccolo-ci-owner","repository_name":"history","ref_name":"main","repository_visibility":"PRIVATE","sensitive_exports_allowed":false}'
git_target_result="$(curl --max-time 10 --silent --show-error --fail \
  -H "Authorization: Bearer ${token}" -H 'Content-Type: application/json' \
  --data-binary "$git_target_body" \
  "http://127.0.0.1:38080/v1/workspaces/${WORKSPACE_UUID}/git-publication-targets/${git_target}")"
node -e '
let s="";process.stdin.on("data",x=>s+=x);process.stdin.on("end",()=>{
 const j=JSON.parse(s);
 if(j.target?.provider!=="GITHUB"||j.provider_connection_verified!==false||
    j.provider_secret_stored!==false||j.registration_publishes_nothing!==true)process.exit(1);
});' <<< "$git_target_result"
git_preview="$(curl --max-time 10 --silent --show-error --fail \
  -H "Authorization: Bearer ${token}" \
  "${path}/analysis-snapshots/${analysis_id}/git-publication-preview?target_id=${git_target}")"
GIT_TARGET="$git_target" node -e '
let s="";process.stdin.on("data",x=>s+=x);process.stdin.on("end",()=>{
 const j=JSON.parse(s);
 if(j.target?.target_id!==process.env.GIT_TARGET||j.scope_kind!=="STANDARD_PRESENTATION"||
    j.package_sha256?.length!==64||!(j.file_count>0)||!(j.total_text_bytes>0)||
    j.includes_examined_source_text!==false||j.includes_original_uploads!==false||
    j.remote_publish_performed!==false||j.requires_transient_provider_authorization!==true||
    j.ready_for_remote_publish!==false)process.exit(1);
});' <<< "$git_preview"
git_publication="$(node -p "require('node:crypto').randomUUID()")"
git_intent="$(curl --max-time 10 --silent --show-error --fail \
  -H "Authorization: Bearer ${token}" -H 'Content-Type: application/json' \
  --data-binary "{\"target_id\":\"${git_target}\"}" \
  "${path}/analysis-snapshots/${analysis_id}/git-publication-intents/${git_publication}")"
GIT_PUBLICATION="$git_publication" node -e '
let s="";process.stdin.on("data",x=>s+=x);process.stdin.on("end",()=>{
 const j=JSON.parse(s);
 if(j.intent?.publication_id!==process.env.GIT_PUBLICATION||
    j.intent?.publication_state!=="AWAITING_REMOTE_AUTHORIZATION"||
    j.remote_publish_performed!==false||
    j.requires_provider_authorization_before_publish!==true)process.exit(1);
});' <<< "$git_intent"
git_intent_replay="$(curl --max-time 10 --silent --show-error --fail \
  -H "Authorization: Bearer ${token}" -H 'Content-Type: application/json' \
  --data-binary "{\"target_id\":\"${git_target}\"}" \
  "${path}/analysis-snapshots/${analysis_id}/git-publication-intents/${git_publication}")"
node -e '
let s="";process.stdin.on("data",x=>s+=x);process.stdin.on("end",()=>{
 const j=JSON.parse(s);if(j.replayed!==true||j.remote_publish_performed!==false)process.exit(1);
});' <<< "$git_intent_replay"
echo 'PASS: GitHub publication target, exact scope preview, and intent remain local and un-published'

# A second, empty database on the SAME PostgreSQL cluster must not
# trigger password rotation for the already-provisioned service role.
psql -X -q -v ON_ERROR_STOP=1 "$NICCOLO_PG_ADMIN_URL" \
  -c 'CREATE DATABASE niccolo_ci_role_probe'
probe_url="$(node -e '
const u = new URL(process.env.NICCOLO_PG_ADMIN_URL);
u.pathname="/niccolo_ci_role_probe";
process.stdout.write(u.toString());
')"
probe_root=".niccolo-local/role-probe"
mkdir -p "$probe_root/scripts"
cp scripts/bootstrap-local-mvp.sh "$probe_root/scripts/bootstrap-local-mvp.sh"
if (cd "$probe_root" && NICCOLO_PG_ADMIN_URL="$probe_url" \
    bash scripts/bootstrap-local-mvp.sh > bootstrap-probe.log 2>&1); then
  echo 'Bootstrapping a second DB unexpectedly reused a cluster-global service role' >&2
  exit 1
fi
grep -q 'Niccolo cluster role(s) already exist:' "$probe_root/bootstrap-probe.log" || {
  echo 'Cluster-role conflict was not the explicit reason for refusing second bootstrap' >&2
  exit 1
}
[[ "$(psql -X -At -v ON_ERROR_STOP=1 "$probe_url" \
  -c "SELECT to_regclass('niccolo.workspaces') IS NULL")" == "t" ]] || {
  echo 'Second bootstrap mutated an empty database before refusing role reuse' >&2
  exit 1
}
psql -X -q -v ON_ERROR_STOP=1 "$NICCOLO_PG_ADMIN_URL" \
  -c 'DROP DATABASE niccolo_ci_role_probe'
echo 'PASS: preflight refuses reuse of cluster-global Niccolo roles'

# The MVP advertises up to 4 MiB per source. Verify real HTTP ingestion
# accepts more than the default Axum 2 MiB body cap, without skipping
# fragment-bound checks or byte-exact original retention.
large_file=".niccolo-local/ci-large-chat.txt"
node -e '
const message="x".repeat(32000);
for(let i=0;i<66;i++)
  process.stdout.write("[08/11/2026, 10:00] Speaker"+i+": "+message+"\n");
' > "$large_file"
large_size="$(wc -c < "$large_file")"
[[ "$large_size" -gt 2097152 && "$large_size" -lt 4194304 ]] || {
  echo "Unexpected bounded large source size: $large_size" >&2
  exit 1
}
large_id="$(node -p "require('node:crypto').randomUUID()")"
large_path="http://127.0.0.1:38080/v1/workspaces/${WORKSPACE_UUID}/sources/${large_id}"
large_result="$(curl --max-time 30 --silent --show-error --fail \
  -H "Authorization: Bearer ${token}" \
  -H 'Content-Type: text/plain' -H 'X-Source-Locator: ci-large-chat.txt' \
  --data-binary @"$large_file" "$large_path")"
printf '%s' "$large_result" | node -e '
let s="";process.stdin.on("data",x=>s+=x);process.stdin.on("end",()=>{
 const j=JSON.parse(s);
 if(j.replayed!==false||j.fragment_ids?.length!==66)process.exit(1);
});'
curl --max-time 30 --silent --show-error --fail \
  -H "Authorization: Bearer ${token}" \
  "${large_path}/original" -o ".niccolo-local/ci-large-original.txt"
cmp "$large_file" ".niccolo-local/ci-large-original.txt"
oversize_file=".niccolo-local/ci-oversized-chat.txt"
node -e 'process.stdout.write("x".repeat(4*1024*1024+1))' > "$oversize_file"
oversize_id="$(node -p "require('node:crypto').randomUUID()")"
oversize_status="$(curl --max-time 30 --silent --show-error \
  --output /dev/null --write-out '%{http_code}' \
  -H "Authorization: Bearer ${token}" \
  -H 'Content-Type: text/plain' -H 'X-Source-Locator: ci-oversized-chat.txt' \
  --data-binary @"$oversize_file" \
  "http://127.0.0.1:38080/v1/workspaces/${WORKSPACE_UUID}/sources/${oversize_id}")"
[[ "$oversize_status" == 413 ]] || {
  echo "Expected HTTP 413 for a source above 4 MiB, got $oversize_status" >&2
  exit 1
}
echo 'PASS: HTTP source limit accepts 2–4 MiB, rejects >4 MiB, retains exact bytes'

# Reject migrations over an unrelated application's tables, even when
# there is no Niccolo schema yet.
psql -X -q -v ON_ERROR_STOP=1 "$NICCOLO_PG_ADMIN_URL" \
  -c 'CREATE DATABASE niccolo_ci_populated_probe'
populated_url="$(node -e '
const u=new URL(process.env.NICCOLO_PG_ADMIN_URL);
u.pathname="/niccolo_ci_populated_probe";
process.stdout.write(u.toString());
')"
psql -X -q -v ON_ERROR_STOP=1 "$populated_url" \
  -c 'CREATE TABLE public.customer_records (record_id integer PRIMARY KEY)'
if (cd "$probe_root" && NICCOLO_PG_ADMIN_URL="$populated_url" \
    bash scripts/bootstrap-local-mvp.sh > populated-probe.log 2>&1); then
  echo 'Bootstrap unexpectedly applied migrations over existing application data' >&2
  exit 1
fi
grep -q 'Database contains existing user relations' "$probe_root/populated-probe.log" || {
  echo 'Unexpected reason for refusing bootstrap over populated database' >&2
  exit 1
}
[[ "$(psql -X -At -v ON_ERROR_STOP=1 "$populated_url" \
  -c "SELECT to_regclass('public.customer_records') IS NOT NULL AND to_regclass('niccolo.workspaces') IS NULL")" == "t" ]] || {
  echo 'Unsafe data mutation while refusing populated-database bootstrap' >&2
  exit 1
}
psql -X -q -v ON_ERROR_STOP=1 "$NICCOLO_PG_ADMIN_URL" \
  -c 'DROP DATABASE niccolo_ci_populated_probe'
echo 'PASS: unrelated application data is protected from first-run migrations'

echo 'PASS: bootstrap, embedded UI, Situation lifecycle, auth isolation, rich Analyze, and human-boundary resume'
