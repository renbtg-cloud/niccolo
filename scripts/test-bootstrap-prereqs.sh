#!/bin/bash
set -Eeuo pipefail
trap 'status=$?; printf "bootstrap prerequisite test failed at line %s (exit %s): %s\n" "$LINENO" "$status" "$BASH_COMMAND" >&2; exit "$status"' ERR

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/repo/scripts" "$TMP/bin"
cp "$ROOT/scripts/bootstrap-local-mvp.sh" "$TMP/repo/scripts/bootstrap-local-mvp.sh"
for tool in dirname node mkdir uname stat bash; do
  cp "$(command -v "$tool")" "$TMP/bin/$tool"
  chmod +x "$TMP/bin/$tool"
done

stub_ok(){
  local name="$1"
  cat > "$TMP/bin/$name" <<'SH'
#!/bin/bash
exit 0
SH
  chmod +x "$TMP/bin/$name"
}

for name in psql npm rustc; do stub_ok "$name"; done

set +e
PATH="$TMP/bin" NICCOLO_PG_ADMIN_URL='postgres://owner@127.0.0.1/db' /bin/bash "$TMP/repo/scripts/bootstrap-local-mvp.sh" >"$TMP/missing-out" 2>"$TMP/missing-err"
missing_status=$?
set -e
[[ "$missing_status" == 1 ]]
grep -Fq 'Missing executable: cargo' "$TMP/missing-err"

stub_ok cargo

# Reject connection-string shapes that psql might accept but runtime URL
# derivation cannot safely transform, before contacting PostgreSQL at all.
cat > "$TMP/bin/psql" <<'SH'
#!/bin/bash
echo 'psql must not run for invalid owner URLs' >&2
exit 92
SH
chmod +x "$TMP/bin/psql"
for bad_admin_url in \
  'host=127.0.0.1 dbname=niccolo' \
  'https://127.0.0.1/niccolo' \
  'postgresql://127.0.0.1' \
  'postgresql:///niccolo' \
  'postgresql://127.0.0.1/niccolo#fragment'; do
  set +e
  PATH="$TMP/bin" NICCOLO_PG_ADMIN_URL="$bad_admin_url" /bin/bash "$TMP/repo/scripts/bootstrap-local-mvp.sh" \
    >"$TMP/url-out" 2>"$TMP/url-err"
  url_status=$?
  set -e
  [[ "$url_status" == 2 ]]
  grep -Fq 'must be a PostgreSQL URL with an explicit host and database name' "$TMP/url-err"
  ! grep -Fq 'psql must not run for invalid owner URLs' "$TMP/url-err"
done

cat > "$TMP/bin/psql" <<'SH'
#!/bin/bash
if [[ " $* " == *' SHOW server_version_num '* ]]; then
  if [[ -n "${NICCOLO_TEST_PSQL_CAPTURE:-}" ]]; then
    printf 'argv=%s\npgpassword=%s\n' "$*" "${PGPASSWORD:-}" > "$NICCOLO_TEST_PSQL_CAPTURE"
  fi
  printf '%s\n' '160012'
  exit 0
fi
printf '%s\n' 'unexpected psql call' >&2
exit 90
SH
chmod +x "$TMP/bin/psql"

set +e
PATH="$TMP/bin" NICCOLO_TEST_PSQL_CAPTURE="$TMP/psql-capture" \
  NICCOLO_PG_ADMIN_URL='postgres://owner:p%40ss%3Aword@127.0.0.1/db' \
  /bin/bash "$TMP/repo/scripts/bootstrap-local-mvp.sh" >"$TMP/version-out" 2>"$TMP/version-err"
version_status=$?
set -e
[[ "$version_status" == 2 ]]
grep -Fq 'requires PostgreSQL 17' "$TMP/version-err"
grep -Fq 'version_num=160012' "$TMP/version-err"
grep -Fq 'pgpassword=p@ss:word' "$TMP/psql-capture"
! grep -Fq 'p%40ss%3Aword' "$TMP/psql-capture"
! grep -Fq 'p@ss:word@' "$TMP/psql-capture"

# Local state must be private before bootstrap is allowed to touch a fresh DB.
cat > "$TMP/bin/psql" <<'SH'
#!/bin/bash
if [[ " $* " == *' SHOW server_version_num '* ]]; then
  printf '%s\n' '170011'
  exit 0
fi
printf '%s\n' 'unexpected psql call after local-directory preflight' >&2
exit 91
SH
chmod +x "$TMP/bin/psql"
mkdir -p "$TMP/repo/.niccolo-local"
chmod 755 "$TMP/repo/.niccolo-local"
set +e
PATH="$TMP/bin" NICCOLO_PG_ADMIN_URL='postgres://owner@127.0.0.1/db' NICCOLO_PG_SECURE_PASSWORD='fixture-secret' \
  /bin/bash "$TMP/repo/scripts/bootstrap-local-mvp.sh" >"$TMP/dir-out" 2>"$TMP/dir-err"
dir_status=$?
set -e
[[ "$dir_status" == 2 ]]
grep -Fq '.niccolo-local must not be accessible by group or other users.' "$TMP/dir-err"
! grep -Fq 'unexpected psql call after local-directory preflight' "$TMP/dir-err"

# The setup login must own the dedicated database; elevated access alone is
# insufficient because object ownership is part of the runtime migration model.
rm -rf "$TMP/repo/.niccolo-local"
mkdir -p "$TMP/repo/.niccolo-local"
chmod 700 "$TMP/repo/.niccolo-local"
cat > "$TMP/bin/psql" <<'SH'
#!/bin/bash
case " $* " in
  *' SHOW server_version_num '*) printf '%s\n' '170011' ;;
  *' SELECT pg_get_userbyid(datdba)=current_user FROM pg_database WHERE datname=current_database() '*)
    printf '%s\n' 'f'
    ;;
  *) printf 'unexpected psql call after owner mismatch: %s\n' "$*" >&2; exit 97 ;;
esac
SH
chmod +x "$TMP/bin/psql"
set +e
PATH="$TMP/bin" NICCOLO_PG_ADMIN_URL='postgres://operator@127.0.0.1/db' NICCOLO_PG_SECURE_PASSWORD='fixture-secret' \
  /bin/bash "$TMP/repo/scripts/bootstrap-local-mvp.sh" >"$TMP/owner-out" 2>"$TMP/owner-err"
owner_status=$?
set -e
[[ "$owner_status" == 1 ]]
grep -Fq 'Bootstrap login must own the dedicated Niccolò database.' "$TMP/owner-err"
! grep -Fq 'unexpected psql call after owner mismatch' "$TMP/owner-err"

# Once DB preflight passes, build tools still must not inherit the database
# owner URL or password that were used only for psql.
rm -rf "$TMP/repo/.niccolo-local"
mkdir -p "$TMP/repo/.niccolo-local"
chmod 700 "$TMP/repo/.niccolo-local"
cat > "$TMP/bin/psql" <<'SH'
#!/bin/bash
case " $* " in
  *' SHOW server_version_num '*) printf '%s\n' '170011' ;;
  *' SELECT pg_get_userbyid(datdba)=current_user FROM pg_database WHERE datname=current_database() '*) printf '%s\n' 't' ;;
  *" to_regclass('niccolo.workspaces') IS NOT NULL "*) printf '%s\n' 'f' ;;
  *' SELECT NOT EXISTS (SELECT 1 FROM pg_class '* ) printf '%s\n' 't' ;;
  *" string_agg(rolname,',' ORDER BY rolname) "* ) printf '%s\n' '' ;;
  *' SELECT rolsuper OR rolcreaterole FROM pg_roles WHERE rolname=current_user '* ) printf '%s\n' 't' ;;
  *) printf 'unexpected psql preflight: %s\n' "$*" >&2; exit 96 ;;
esac
SH
chmod +x "$TMP/bin/psql"
cat > "$TMP/repo/scripts/build-mvp.sh" <<'SH'
#!/bin/bash
set -euo pipefail
[[ -z "${PGPASSWORD:-}" ]] || exit 93
[[ -z "${NICCOLO_PG_ADMIN_URL:-}" ]] || exit 94
printf 'credentials stripped\n' > "${NICCOLO_TEST_BUILD_MARKER:?}"
exit 95
SH
chmod +x "$TMP/repo/scripts/build-mvp.sh"
set +e
PATH="$TMP/bin" NICCOLO_TEST_BUILD_MARKER="$TMP/build-marker" \
  NICCOLO_PG_ADMIN_URL='postgres://owner:p%40ss%3Aword@127.0.0.1/db' \
  /bin/bash "$TMP/repo/scripts/bootstrap-local-mvp.sh" >"$TMP/build-out" 2>"$TMP/build-err"
build_status=$?
set -e
[[ "$build_status" == 95 ]]
grep -Fq 'credentials stripped' "$TMP/build-marker"

printf '%s\n' 'PASS: bootstrap preflight protects prerequisites, owner credentials, and local credential storage before mutation.'
