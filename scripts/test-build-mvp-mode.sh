#!/usr/bin/env bash
# Fast contract test for build-mvp dependency-mode flags; no network or compilation.
set -Eeuo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
fixture="$(mktemp -d)"
cleanup(){ rm -rf "$fixture"; }
trap cleanup EXIT
mkdir -p "$fixture/project/scripts" "$fixture/project/frontend" "$fixture/project/target/release" "$fixture/bin"
cp "$ROOT/scripts/build-mvp.sh" "$fixture/project/scripts/build-mvp.sh"
log="$fixture/calls.log"
cat > "$fixture/bin/npm" <<'EOF'
#!/usr/bin/env bash
printf 'npm' >> "$BUILD_MODE_LOG"; printf ' <%s>' "$@" >> "$BUILD_MODE_LOG"; printf '\n' >> "$BUILD_MODE_LOG"
if [[ " $* " == *' run build '* ]]; then
  mkdir -p dist/assets
  printf '<html></html>\n' > dist/index.html
  printf 'app\n' > dist/assets/app.js
  printf 'css\n' > dist/assets/app.css
  mkdir -p dist/assets/chunks
  for page in AnalysisPage InspectorPage ReasoningPage IdentitiesPage FeedbackPage EpistemicPlaygroundPage ForensicPage; do
    printf '%s\n' "$page" > "dist/assets/chunks/$page.js"
  done
fi
EOF
cat > "$fixture/bin/cargo" <<'EOF'
#!/usr/bin/env bash
printf 'cargo' >> "$BUILD_MODE_LOG"; printf ' <%s>' "$@" >> "$BUILD_MODE_LOG"; printf '\n' >> "$BUILD_MODE_LOG"
if [[ " ${1:-} " == ' build ' ]]; then
  mkdir -p target/release
  printf 'api\n' > target/release/niccolo-api
  printf 'provision\n' > target/release/niccolo-provision
fi
EOF
chmod +x "$fixture/bin/npm" "$fixture/bin/cargo"
export BUILD_MODE_LOG="$log"
cd "$fixture/project"

NPM="$fixture/bin/npm" CARGO="$fixture/bin/cargo" NICCOLO_OFFLINE=1 bash scripts/build-mvp.sh >/dev/null
line="$(grep '^npm <ci>' "$log")"
[[ "$line" == *'<--offline>'* && "$line" == *'<--no-audit>'* && "$line" == *'<--no-fund>'* ]]
grep -Fq 'cargo <test> <--locked> <--offline> <--lib>' "$log"
grep -Fq 'cargo <build> <--locked> <--offline> <--release>' "$log"

: > "$log"
rm -rf frontend/dist
rm -f target/release/niccolo-api target/release/niccolo-provision
NPM="$fixture/bin/npm" CARGO="$fixture/bin/cargo" bash scripts/build-mvp.sh >/dev/null
line="$(grep '^npm <ci>' "$log")"
[[ "$line" != *'<--offline>'* ]]
grep -Fq 'cargo <test> <--locked> <--lib>' "$log"
! grep -Fq '<--offline>' "$log"

: > "$log"
rm -rf frontend/dist
rm -f target/release/niccolo-api target/release/niccolo-provision
NPM="$fixture/bin/npm" CARGO="$fixture/bin/cargo" NICCOLO_BUILD_VERIFY=0 bash scripts/build-mvp.sh >/dev/null
! grep -Fq 'npm <test>' "$log"
! grep -Fq 'cargo <test>' "$log"
grep -Fq 'npm <run> <build>' "$log"
grep -Fq 'cargo <build> <--locked> <--release>' "$log"

if NPM="$fixture/bin/npm" CARGO="$fixture/bin/cargo" NICCOLO_BUILD_VERIFY=maybe bash scripts/build-mvp.sh >/dev/null 2>&1; then
  echo 'Invalid NICCOLO_BUILD_VERIFY unexpectedly accepted' >&2
  exit 1
fi

echo 'PASS: build-mvp online/offline/install dependency mode contract'
