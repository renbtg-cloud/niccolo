#!/usr/bin/env bash
# Best-effort local browser opener for the prepared Niccolò loopback URL.
# The URL contains only non-secret workspace/Situation identifiers.
set -Eeuo pipefail

prepared_url="${1:-}"
port="${2:-}"

# This helper never needs Niccolò/provider/database credentials. Strip the known
# secret-bearing variables even when the helper is invoked directly.
unset NICCOLO_LLM_API_KEY NICCOLO_PG_SECURE_URL NICCOLO_PG_SECURE_PASSWORD NICCOLO_PG_ADMIN_URL NICCOLO_NEXT_ADMIN_URL NICCOLO_NEXT_APP_URL PGPASSWORD

[[ "$port" =~ ^[0-9]+$ ]] && (( port >= 1 && port <= 65535 )) || {
  echo 'Invalid Niccolò local port.' >&2
  exit 2
}
# Prefix matching is not an origin check: e.g. 127.0.0.1:38080@host.example
# is a URL whose actual host is host.example. Validate the parsed origin,
# credentials and port before passing anything to a browser opener.
if ! node - "$prepared_url" "$port" <<'JS'
try {
  const url = new URL(process.argv[2]);
  const port = Number(process.argv[3]);
  if (url.protocol !== 'http:' || url.hostname !== '127.0.0.1' ||
      url.username || url.password || Number(url.port || 80) !== port) {
    process.exit(1);
  }
} catch (_) {
  process.exit(1);
}
JS
then
  echo 'Refusing to open a non-loopback Niccolò URL.' >&2
  exit 2
fi

platform="$(uname -s)"
linux_gui=0
if [[ "$platform" == Linux* ]]; then
  if [[ -n "${DISPLAY:-}" || -n "${WAYLAND_DISPLAY:-}" ]] || command -v wslview >/dev/null 2>&1; then
    linux_gui=1
  elif grep -qi microsoft /proc/version 2>/dev/null && command -v cmd.exe >/dev/null 2>&1; then
    linux_gui=1
  fi
fi

has_opener=0
case "$platform" in
  Darwin*) command -v open >/dev/null 2>&1 && has_opener=1 ;;
  MINGW*|MSYS*|CYGWIN*) command -v cmd.exe >/dev/null 2>&1 && has_opener=1 ;;
  Linux*)
    if (( linux_gui == 1 )) && { command -v xdg-open >/dev/null 2>&1 || command -v gio >/dev/null 2>&1 || command -v wslview >/dev/null 2>&1 || command -v cmd.exe >/dev/null 2>&1; }; then
      has_opener=1
    fi
    ;;
esac
(( has_opener == 1 )) || exit 0

ready=0
for (( attempt = 0; attempt < 80; attempt++ )); do
  if command -v curl >/dev/null 2>&1; then
    if curl --max-time 1 --silent --fail "http://127.0.0.1:${port}/healthz" >/dev/null 2>&1; then
      ready=1
      break
    fi
  elif (exec 3<>"/dev/tcp/127.0.0.1/${port}") 2>/dev/null; then
    ready=1
    break
  fi
  sleep 0.25
done
(( ready == 1 )) || exit 0

case "$platform" in
  Darwin*)
    open "$prepared_url" >/dev/null 2>&1 || true
    ;;
  MINGW*|MSYS*|CYGWIN*)
    cmd.exe /c start "" "$prepared_url" >/dev/null 2>&1 || true
    ;;
  Linux*)
    if command -v wslview >/dev/null 2>&1; then
      wslview "$prepared_url" >/dev/null 2>&1 || true
    elif grep -qi microsoft /proc/version 2>/dev/null && command -v cmd.exe >/dev/null 2>&1; then
      cmd.exe /c start "" "$prepared_url" >/dev/null 2>&1 || true
    elif command -v xdg-open >/dev/null 2>&1; then
      xdg-open "$prepared_url" >/dev/null 2>&1 || true
    elif command -v gio >/dev/null 2>&1; then
      gio open "$prepared_url" >/dev/null 2>&1 || true
    fi
    ;;
esac
