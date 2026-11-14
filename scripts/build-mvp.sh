#!/usr/bin/env bash
# Build the first-party frontend BEFORE compiling the Rust binary that embeds it.
# Works with Bash on Linux and Git Bash on Windows when Node/npm and Cargo exist.
set -Eeuo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

: "${NPM:=npm}"
: "${CARGO:=cargo}"
: "${NICCOLO_BUILD_VERIFY:=1}"
[[ "$NICCOLO_BUILD_VERIFY" == "0" || "$NICCOLO_BUILD_VERIFY" == "1" ]] || { echo "NICCOLO_BUILD_VERIFY must be 0 or 1" >&2; exit 2; }

npm_ci=(ci)
cargo_locked=(--locked)
if [[ "${NICCOLO_OFFLINE:-0}" == "1" ]]; then
  npm_ci+=(--offline --no-audit --no-fund)
  cargo_locked+=(--offline)
  echo "Offline dependency mode enabled."
fi

echo "[1/5] Install locked frontend dependencies"
(cd frontend && "$NPM" "${npm_ci[@]}")
if [[ "$NICCOLO_BUILD_VERIFY" == "1" ]]; then
  echo "[2/5] Verify frontend regression tests"
  (cd frontend && "$NPM" test)
else
  echo "[2/5] Skip frontend regression tests (installation build)"
fi
echo "[3/5] Build the frontend that Rust embeds"
(cd frontend && "$NPM" run build)
for file in frontend/dist/index.html frontend/dist/assets/app.js frontend/dist/assets/app.css \
    frontend/dist/assets/chunks/AnalysisPage.js \
    frontend/dist/assets/chunks/InspectorPage.js \
    frontend/dist/assets/chunks/ReasoningPage.js \
    frontend/dist/assets/chunks/IdentitiesPage.js \
    frontend/dist/assets/chunks/FeedbackPage.js \
    frontend/dist/assets/chunks/EpistemicPlaygroundPage.js \
    frontend/dist/assets/chunks/ForensicPage.js; do
  if [[ ! -s "$file" ]]; then
    echo "Missing built frontend artifact: $file" >&2
    exit 1
  fi
done
if [[ "$NICCOLO_BUILD_VERIFY" == "1" ]]; then
  echo "[4/5] Verify Rust unit tests"
  "$CARGO" test "${cargo_locked[@]}" --lib
else
  echo "[4/5] Skip Rust unit tests (installation build)"
fi
echo "[5/5] Compile the Niccolo API and provisioning CLI"
"$CARGO" build "${cargo_locked[@]}" --release --bin niccolo-api --bin niccolo-provision

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
