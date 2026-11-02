#!/usr/bin/env bash
# Real TCP/HTTP smoke using disposable DB, not a production security test.
set -Eeuo pipefail
cd "$(dirname "$0")/.."
: "${NICCOLO_PG_ADMIN_URL:?must name a DISPOSABLE test database}"
: "${NICCOLO_PG_SERVICE_URL:?must name the same DB with restricted login}"
: "${PSQL_BIN:=psql}"
: "${NICCOLO_API_PORT:=38732}"
tmp="$(mktemp -d)"; chmod 700 "$tmp"
server_pid=''
cleanup(){
  [[ -z "$server_pid" ]] || { kill "$server_pid" 2>/dev/null || true; wait "$server_pid" 2>/dev/null || true; }
  rm -rf -- "$tmp"
}
trap cleanup EXIT
uuid(){ python3 -c 'import uuid;print(uuid.uuid4())'; }
w="$(uuid)"; p="$(uuid)"; q="$(uuid)"; e="$(uuid)"; f="$(uuid)"
"$PSQL_BIN" "$NICCOLO_PG_ADMIN_URL" -v ON_ERROR_STOP=1 -q -c "INSERT INTO niccolo.workspaces(workspace_id,label) VALUES ('$w','loopback-smoke');" >/dev/null
"$PSQL_BIN" "$NICCOLO_PG_ADMIN_URL" -v ON_ERROR_STOP=1 -q -c "INSERT INTO niccolo.evidence(workspace_id,evidence_id,source_locator,content,source_class) VALUES ('$w','$e','synthetic://smoke','attributed sample','REPORTED_SPEECH');" >/dev/null
cargo run --offline --quiet --bin niccolo-provision -- mint "$w" "$p" write 1 > "$tmp/token"
chmod 600 "$tmp/token"; token="$(cat "$tmp/token")"
printf 'header = "Authorization: Bearer %s"\n' "$token" > "$tmp/curl-auth.conf"
chmod 600 "$tmp/curl-auth.conf"
NICCOLO_API_PORT="$NICCOLO_API_PORT" cargo run --offline --quiet --bin niccolo-api > "$tmp/server.log" 2>&1 & server_pid=$!
base="http://127.0.0.1:$NICCOLO_API_PORT"
ready=0
for _ in $(seq 1 60);do
 if curl -fsS --max-time 1 "$base/readyz" > /dev/null 2>&1;then ready=1;break;fi
 sleep .1
done
[[ "$ready" == 1 ]] || { cat "$tmp/server.log" >&2; exit 1; }
checks=0
check(){
 local expected="$1";shift
 local actual
 actual="$(curl --max-time 5 -sS -o "$tmp/reply" -w '%{http_code}' "$@")"
 if [[ "$actual" != "$expected" ]];then
  echo "FAIL requested $expected received $actual" >&2; cat "$tmp/reply" >&2;exit 1
 fi
 checks=$((checks+1))
}
check 401 -X GET "$base/v1/workspaces/$w/questions"
check 201 --config "$tmp/curl-auth.conf" -H 'Content-Type: application/json' \
  --data '{"text":"What original record would help?"}' "$base/v1/workspaces/$w/questions/$q"
check 200 --config "$tmp/curl-auth.conf" "$base/v1/workspaces/$w/questions"
check 201 --config "$tmp/curl-auth.conf" -H 'Content-Type: application/json' \
  --data '{"response":"insufficient_evidence","human_reason":"source not yet confirmed"}' "$base/v1/workspaces/$w/questions/$q/feedback/$f"
check 200 --config "$tmp/curl-auth.conf" "$base/v1/workspaces/$w/questions/$q/feedback"
check 200 --config "$tmp/curl-auth.conf" "$base/v1/workspaces/$w/questions/$q/positions"
check 201 --config "$tmp/curl-auth.conf" -H 'Content-Type: application/json' \
  --data "{\"evidence_id\":\"$e\",\"relation\":\"challenges\",\"human_reason\":\"manual doubt\"}" "$base/v1/workspaces/$w/questions/$q/references/$(uuid)"
check 200 --config "$tmp/curl-auth.conf" "$base/v1/workspaces/$w/questions/$q/references"
check 403 --config "$tmp/curl-auth.conf" "$base/v1/workspaces/$(uuid)/questions"
printf '%s\n' "$token" | cargo run --offline --quiet --bin niccolo-provision -- revoke > /dev/null
check 403 --config "$tmp/curl-auth.conf" "$base/v1/workspaces/$w/questions"
echo "LOOPBACK_OK 0/$checks failures; $checks/$checks checks passed"
