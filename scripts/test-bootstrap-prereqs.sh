#!/bin/bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/repo/scripts" "$TMP/bin"
cp "$ROOT/scripts/bootstrap-local-mvp.sh" "$TMP/repo/scripts/bootstrap-local-mvp.sh"
cp "$(command -v dirname)" "$TMP/bin/dirname"

stub_ok(){
  local name="$1"
  cat > "$TMP/bin/$name" <<'SH'
#!/bin/bash
exit 0
SH
  chmod +x "$TMP/bin/$name"
}

for name in psql node npm rustc; do stub_ok "$name"; done

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

printf '%s\n' 'PASS: bootstrap fails before build when required tools, PostgreSQL 17, or local credential-directory safety are unavailable.'
