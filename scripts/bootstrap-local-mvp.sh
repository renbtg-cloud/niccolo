#!/usr/bin/env bash
# First-run single-PC MVP setup against a NEW PostgreSQL 17 database.
# Prerequisite: psql, Node.js, npm, Rust/Cargo, an admin DB URL and a password
# for the dedicated niccolo_secure login. Does not install PostgreSQL itself.
set -Eeuo pipefail
umask 077
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

: "${NICCOLO_PG_ADMIN_URL:?Set an admin URL for a new dedicated PostgreSQL database}"
: "${NICCOLO_PG_SECURE_PASSWORD:?Set the dedicated non-admin runtime password}"
for program in psql node; do
  command -v "$program" >/dev/null || {
    printf 'Missing executable: %s\n' "$program" >&2
    exit 1
  }
done
if [[ -e .niccolo-local/local.env || -e .niccolo-local/token.txt ]]; then
  echo 'Local credentials already exist; refusing to overwrite them.' >&2
  exit 1
fi
if [[ "$(psql -X -At -v ON_ERROR_STOP=1 "$NICCOLO_PG_ADMIN_URL" -c "SELECT to_regclass('niccolo.workspaces') IS NOT NULL")" != "f" ]]; then
  echo 'Database already contains Niccolo tables; use a new empty database.' >&2
  exit 1
fi

echo '[1/4] Build MVP before changing the database'
bash scripts/build-mvp.sh
ext=''
case "$(uname -s)" in MINGW*|MSYS*|CYGWIN*) ext='.exe';; esac

echo '[2/4] Apply all migrations to the fresh database'
for migration in sql/[0-9][0-9][0-9][0-9]_*.sql; do
  psql -X -q -v ON_ERROR_STOP=1 "$NICCOLO_PG_ADMIN_URL" -f "$migration" >/dev/null
done

echo '[3/4] Create restricted runtime PostgreSQL login'
psql -X -q -v ON_ERROR_STOP=1 "$NICCOLO_PG_ADMIN_URL" <<'SQL'
\getenv secure_password NICCOLO_PG_SECURE_PASSWORD
SELECT format('CREATE ROLE niccolo_secure LOGIN PASSWORD %L', :'secure_password')
 WHERE NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname='niccolo_secure') \gexec
ALTER ROLE niccolo_secure LOGIN PASSWORD :'secure_password';
GRANT niccolo_api TO niccolo_secure WITH INHERIT TRUE, SET FALSE;
SQL

workspace="$(node -p "require('node:crypto').randomUUID()")"
principal="$(node -p "require('node:crypto').randomUUID()")"
psql -X -q -v ON_ERROR_STOP=1 -v workspace="$workspace" "$NICCOLO_PG_ADMIN_URL" <<'SQL'
INSERT INTO niccolo.workspaces(workspace_id,label)
  VALUES (:'workspace'::uuid, 'Local Niccolo MVP');
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
printf 'WORKSPACE_UUID=%s\nSITUATION_UUID=%s\n' "$workspace" \
  "$(node -p "require('node:crypto').randomUUID()")" > .niccolo-local/identifiers.txt
token="$(./target/release/niccolo-provision"${ext} mint "$workspace" "$principal" write 30)"
printf '%s\n' "$token" > .niccolo-local/token.txt
chmod 600 .niccolo-local/local.env .niccolo-local/token.txt .niccolo-local/identifiers.txt 2>/dev/null || true

echo 'Local setup complete. Credentials are in .niccolo-local/ (never commit or share).'
echo 'Set NICCOLO_LLM_BASE_URL and NICCOLO_LLM_MODEL to enable local inference.'
echo "Start: source .niccolo-local/local.env && ./target/release/niccolo-api${ext}"
echo 'Open: http://127.0.0.1:38080'
echo 'Use workspace and Situation UUIDs from identifiers.txt and bearer from token.txt.'
echo 'The first Situation is created through the web UI, not automatically here.'
