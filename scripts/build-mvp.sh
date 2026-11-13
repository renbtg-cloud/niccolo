#!/usr/bin/env bash
# Build the first-party frontend BEFORE compiling the Rust binary that embeds it.
# Works with Bash on Linux and Git Bash on Windows when Node/npm and Cargo exist.
set -Eeuo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

: "${NPM:=npm}"
: "${CARGO:=cargo}"

# Fail before touching frontend dependencies if the selected build tools are absent.
for tool in "$NPM" "$CARGO"; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    printf 'Missing build tool: %s\n' "$tool" >&2
    exit 127
  fi
done

# Offline mode uses a preloaded npm cache and Cargo registry/vendor dependencies.
# Set NICCOLO_OFFLINE=1 to forbid network dependency resolution during builds.
npm_flags=()
cargo_flags=()
if [[ "${NICCOLO_OFFLINE:-0}" == "1" ]]; then
  npm_flags+=(--offline)
  cargo_flags+=(--offline)
fi

echo "[1/5] Install locked frontend dependencies"
(cd frontend && "$NPM" ci "${npm_flags[@]}")
echo "[2/5] Verify frontend regression tests"
(cd frontend && "$NPM" test)
echo "[3/5] Build the frontend that Rust embeds"
(cd frontend && "$NPM" run build)
for file in frontend/dist/index.html frontend/dist/assets/app.js frontend/dist/assets/app.css; do
  if [[ ! -s "$file" ]]; then
    echo "Missing built frontend artifact: $file" >&2
    exit 1
  fi
done
echo "[4/5] Verify Rust unit tests"
"$CARGO" test --locked "${cargo_flags[@]}" --lib
echo "[5/5] Compile the Niccolo API and provisioning CLI"
"$CARGO" build --locked "${cargo_flags[@]}" --release --bin niccolo-api --bin niccolo-provision

ext=''
case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*) ext='.exe' ;;
esac
for bin in niccolo-api niccolo-provision; do
  test -s "target/release/${bin}${ext}" || {
    echo "Expected executable missing: target/release/${bin}${ext}" >&2
    exit 1
  }
done
echo "MVP build complete. The Rust API binary embeds the freshly built React UI."
echo "Binaries: target/release/niccolo-api${ext} and niccolo-provision${ext}"
echo "PostgreSQL setup and service credentials are still required to run the API."
