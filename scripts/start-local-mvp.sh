#!/usr/bin/env bash
# Start an already-bootstrapped single-PC Niccolo MVP with a prepared local URL.
# The bearer token is deliberately NOT placed in the URL or printed to stdout.
set -Eeuo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

env_file=".niccolo-local/local.env"
ids_file=".niccolo-local/identifiers.txt"
token_file=".niccolo-local/token.txt"
url_file=".niccolo-local/prepared-url.txt"

for file in "$env_file" "$ids_file" "$token_file"; do
  [[ -s "$file" ]] || {
    echo "Missing local bootstrap file: $file" >&2
    echo "Run: bash scripts/bootstrap-local-mvp.sh" >&2
    exit 1
  }
done

# Generated locally by bootstrap from UUIDs and shell-escaped values.
# shellcheck disable=SC1091
source "$env_file"
# shellcheck disable=SC1091
source "$ids_file"

: "${WORKSPACE_UUID:?missing WORKSPACE_UUID}"
: "${SITUATION_UUID:?missing SITUATION_UUID}"
: "${NICCOLO_API_PORT:=38080}"

prepared_url="http://127.0.0.1:${NICCOLO_API_PORT}/?workspace=${WORKSPACE_UUID}&situation=${SITUATION_UUID}"
printf '%s\n' "$prepared_url" > "$url_file"
chmod 600 "$url_file" 2>/dev/null || true

echo "Niccolo local session"
echo "Open: $prepared_url"
echo "Bearer token: $token_file"
echo "Paste that token into the connection panel; it is intentionally not embedded in the URL."

if [[ "${1:-}" == "--print-only" ]]; then
  exit 0
fi

ext=''
case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*) ext='.exe' ;;
esac
binary="./target/release/niccolo-api${ext}"
[[ -x "$binary" || -f "$binary" ]] || {
  echo "Missing API binary: $binary" >&2
  echo "Run the bootstrap/build first." >&2
  exit 1
}

exec "$binary"
