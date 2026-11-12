#!/usr/bin/env bash
# First-run single-PC MVP setup against a NEW PostgreSQL 17 database.
# Prerequisite: psql, Node.js, npm, Rust/Cargo, an admin DB URL and a password
# for the dedicated niccolo_secure login. Does not install PostgreSQL itself.
set -Eeuo pipefail
umask 077
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

: "${NICCOLO_PG_ADMIN_URL:?Set an admin URL for a new dedicated PostgreSQL database}"
for program in psql node; do
  command -v "$program" >/dev/null || {
    printf 'Missing executable: %s\n' "$program" >&2
    exit 1
  }
done

# The runtime database credential is an implementation secret, not something a
# first user should have to invent. Generate a high-entropy value unless an
# operator deliberately supplied one (for example in deployment automation).
generated_secure_password=0
if [[ -z "${NICCOLO_PG_SECURE_PASSWORD:-}" ]]; then
  NICCOLO_PG_SECURE_PASSWORD="$(node -e "process.stdout.write(require('node:crypto').randomBytes(32).toString('hex'))")"
  export NICCOLO_PG_SECURE_PASSWORD
  generated_secure_password=1
fi
if [[ "$NICCOLO_PG_SECURE_PASSWORD" =~ [[:cntrl:]] ]]; then
  echo 'NICCOLO_PG_SECURE_PASSWORD contains unsupported control characters.' >&2
  exit 2
fi
if [[ -e .niccolo-local/local.env || -e .niccolo-local/token.txt ]]; then
  echo 'Local credentials already exist; refusing to overwrite them.' >&2
  exit 1
fi
if [[ "$(psql -X -At -v ON_ERROR_STOP=1 "$NICCOLO_PG_ADMIN_URL" -c "SELECT to_regclass('niccolo.workspaces') IS NOT NULL")" != "f" ]]; then
  echo 'Database already contains Niccolo tables; use a new empty database.' >&2
  exit 1
fi
# A first-run installer must never run migrations over someone else's data.
# The service is intended for an empty dedicated database, not just a new
# niccolo schema inside an unrelated application database.
if [[ "$(psql -X -At -v ON_ERROR_STOP=1 "$NICCOLO_PG_ADMIN_URL" -c "SELECT NOT EXISTS (SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE c.relkind IN ('r','p','v','m','f','S') AND n.nspname NOT LIKE 'pg_%' AND n.nspname <> 'information_schema')")" != "t" ]]; then
  echo 'Database contains existing user relations; refusing first-run migrations. Use an empty dedicated database.' >&2
  exit 1
fi

# PostgreSQL roles are cluster-global, not per-database. A fresh bootstrap
# creates niccolo_app/niccolo_api in migrations and niccolo_secure afterward.
# Reusing any of them can fail halfway through migrations or, worse, couple
# two installations through shared grants/passwords. Refuse before the build.
cluster_role_conflicts="$(psql -X -At -v ON_ERROR_STOP=1 "$NICCOLO_PG_ADMIN_URL" -c \
  "SELECT COALESCE(string_agg(rolname,',' ORDER BY rolname),'') FROM pg_roles WHERE rolname IN ('niccolo_app','niccolo_api','niccolo_secure')")"
if [[ -n "$cluster_role_conflicts" ]]; then
  printf 'PostgreSQL Niccolo cluster role(s) already exist: %s; use a dedicated cluster or a supported migration.\n' \
    "$cluster_role_conflicts" >&2
  exit 1
fi

if [[ "$(psql -X -At -v ON_ERROR_STOP=1 "$NICCOLO_PG_ADMIN_URL" -c "SELECT rolsuper OR rolcreaterole FROM pg_roles WHERE rolname=current_user")" != "t" ]]; then
  echo 'Bootstrap database owner must have CREATEROLE so Niccolo can create its restricted runtime login; SUPERUSER is not required.' >&2
  exit 1
fi

echo '[1/4] Build MVP before changing the database'
NICCOLO_BUILD_VERIFY=0 bash scripts/build-mvp.sh
ext=''
case "$(uname -s)" in MINGW*|MSYS*|CYGWIN*) ext='.exe';; esac

echo '[2/4] Apply all migrations to the fresh database'
for migration in sql/[0-9][0-9][0-9][0-9]_*.sql; do
  psql -X -q -v ON_ERROR_STOP=1 "$NICCOLO_PG_ADMIN_URL" -f "$migration" >/dev/null
done

echo '[3/4] Create restricted runtime PostgreSQL login'
psql -X -q -v ON_ERROR_STOP=1 "$NICCOLO_PG_ADMIN_URL" <<'SQL'
\getenv secure_password NICCOLO_PG_SECURE_PASSWORD
SELECT format('CREATE ROLE niccolo_secure LOGIN PASSWORD %L', :'secure_password') \gexec
GRANT niccolo_api TO niccolo_secure WITH INHERIT TRUE, SET FALSE;
SQL

workspace="$(node -p "require('node:crypto').randomUUID()")"
principal="$(node -p "require('node:crypto').randomUUID()")"
psql -X -q -v ON_ERROR_STOP=1 -v workspace="$workspace" "$NICCOLO_PG_ADMIN_URL" <<'SQL'
BEGIN;
SELECT set_config('niccolo.workspace_id', :'workspace', true);
INSERT INTO niccolo.workspaces(workspace_id,label)
  VALUES (:'workspace'::uuid, 'Local Niccolo MVP');
COMMIT;
SQL

echo '[4/4] Issue private workspace token and write local runtime configuration'
mkdir -p .niccolo-local
runtime_url="$(node -e '
const u = new URL(process.env.NICCOLO_PG_ADMIN_URL);
u.username = "niccolo_secure";
u.password = process.env.NICCOLO_PG_SECURE_PASSWORD;
process.stdout.write(u.toString());
')"
printf 'export NICCOLO_PG_SECURE_URL=%q\n' "$runtime_url" > .niccolo-local/local.env
printf 'export NICCOLO_API_PORT=38080\n' >> .niccolo-local/local.env
SITUATION_UUID="$(node -p "require('node:crypto').randomUUID()")"
printf 'WORKSPACE_UUID=%s\nSITUATION_UUID=%s\n' "$workspace" "$SITUATION_UUID" > .niccolo-local/identifiers.txt
printf 'http://127.0.0.1:38080/?workspace=%s&situation=%s\n' "$workspace" "$SITUATION_UUID" > .niccolo-local/prepared-url.txt
provision_bin="./target/release/niccolo-provision${ext}"
token="$("$provision_bin" mint "$workspace" "$principal" write 30)"
printf '%s\n' "$token" > .niccolo-local/token.txt
chmod 600 .niccolo-local/local.env .niccolo-local/token.txt .niccolo-local/identifiers.txt .niccolo-local/prepared-url.txt 2>/dev/null || true

echo 'Local setup complete. Credentials are in .niccolo-local/ (never commit or share).'
if (( generated_secure_password == 1 )); then
  echo 'Runtime PostgreSQL password: generated automatically and stored only in .niccolo-local/local.env.'
else
  echo 'Runtime PostgreSQL password: supplied by the operator and not displayed.'
fi
echo 'Configure reasoning: bash scripts/configure-local-model.sh'
echo 'Start: bash scripts/start-local-mvp.sh'
echo 'Prepared browser URL: .niccolo-local/prepared-url.txt'
echo 'Bearer token: .niccolo-local/token.txt'
echo 'The prepared URL fills workspace and Situation identifiers; paste the bearer token separately.'
echo 'On a desktop, start waits for health and best-effort opens that URL; set NICCOLO_OPEN_BROWSER=0 to disable.'
echo 'The first Situation is created through the web UI, not automatically here.'
