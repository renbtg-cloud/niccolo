#!/usr/bin/env bash
# Mocked, database-free verification of the bootstrap executable path.
set -Eeuo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/bin"

cat > "$TMP/bin/psql" <<'SH'
#!/usr/bin/env bash
case " $* " in
  *' SHOW server_version_num '*) echo 170011 ;;
  *' SELECT pg_get_userbyid(datdba)=current_user FROM pg_database WHERE datname=current_database() '*) echo t ;;
  *" to_regclass('niccolo.workspaces') IS NOT NULL "*) echo f ;;
  *' SELECT NOT EXISTS (SELECT 1 FROM pg_class '*) echo t ;;
  *"string_agg(rolname,',' ORDER BY rolname)"*) printf '\n' ;;
  *' SELECT rolsuper OR rolcreaterole FROM pg_roles WHERE rolname=current_user '*) echo t ;;
  *' -f '* | *' -q '*) : ;;
  *) echo "Unexpected PostgreSQL operation: $*" >&2; exit 91 ;;
esac
SH
chmod +x "$TMP/bin/psql"
for cmd in cargo rustc npm; do
  printf '#!/usr/bin/env bash\nexit 0\n' > "$TMP/bin/$cmd"
  chmod +x "$TMP/bin/$cmd"
done

for layout in absolute relative; do
  checkout="$TMP/$layout/repo"
  mkdir -p "$checkout/scripts" "$checkout/sql"
  cp "$ROOT/scripts/bootstrap-local-mvp.sh" "$checkout/scripts/"
  printf '%s\n' '-- mock migration --' > "$checkout/sql/0001_mock.sql"
  cat > "$checkout/scripts/build-mvp.sh" <<'SH'
#!/usr/bin/env bash
set -Eeuo pipefail
[[ -z "${NICCOLO_PG_ADMIN_URL:-}" && -z "${PGPASSWORD:-}" ]]
mkdir -p "${CARGO_TARGET_DIR:-target}/release"
cat > "${CARGO_TARGET_DIR:-target}/release/niccolo-provision" <<'PROVISION'
#!/usr/bin/env bash
[[ "$1" == mint ]] || exit 92
printf '%s\n' "$0" > "${NICCOLO_TEST_PROVISION_CAPTURE:?}"
printf 'ncl_%064d\n' 0
PROVISION
chmod +x "${CARGO_TARGET_DIR:-target}/release/niccolo-provision"
SH
  chmod +x "$checkout/scripts/build-mvp.sh"
  if [[ "$layout" == absolute ]]; then
    target_dir="$TMP/external-target"
  else
    target_dir='relative-target'
  fi
  capture="$TMP/$layout/provision-path"
  (
    cd "$checkout"
    PATH="$TMP/bin:$PATH" CARGO_TARGET_DIR="$target_dir" \
      NICCOLO_TEST_PROVISION_CAPTURE="$capture" \
      NICCOLO_PG_ADMIN_URL='postgresql://owner:private@127.0.0.1/niccolo_fixture' \
      NICCOLO_PG_SECURE_PASSWORD='runtime_fixture_password' \
      bash scripts/bootstrap-local-mvp.sh > "$TMP/$layout/stdout" 2> "$TMP/$layout/stderr"
  )
  [[ "$(cat "$capture")" == "$target_dir/release/niccolo-provision" ]]
  grep -Eq '^ncl_[0-9]{64}$' "$checkout/.niccolo-local/token.txt"
  [[ ! -e "$checkout/target/release/niccolo-provision" ]]
  ! grep -Fq 'private' "$TMP/$layout/stdout"
done
echo 'PASS: first-run bootstrap uses Cargo output path for provisioner.'
