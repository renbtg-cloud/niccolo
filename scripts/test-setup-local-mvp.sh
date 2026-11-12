#!/usr/bin/env bash
set -Eeuo pipefail

SOURCE="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/setup-local-mvp.sh"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

make_case(){
  local name="$1" root="$TMP/$1"
  mkdir -p "$root/scripts"
  cp "$SOURCE" "$root/scripts/setup-local-mvp.sh"
  cat > "$root/scripts/bootstrap-local-mvp.sh" <<'STUB'
#!/usr/bin/env bash
set -e
printf 'bootstrap\n' >> "$TEST_TRACE"
mkdir -p .niccolo-local
printf 'x\n' > .niccolo-local/local.env
printf 'x\n' > .niccolo-local/identifiers.txt
printf 'x\n' > .niccolo-local/token.txt
STUB
  cat > "$root/scripts/configure-local-model.sh" <<'STUB'
#!/usr/bin/env bash
printf 'configure\n' >> "$TEST_TRACE"
mkdir -p .niccolo-local
cat > .niccolo-local/model.env <<'ENV'
export NICCOLO_LLM_BASE_URL=http://127.0.0.1:18080
export NICCOLO_LLM_MODEL=mock
ENV
STUB
  cat > "$root/scripts/start-local-mvp.sh" <<'STUB'
#!/usr/bin/env bash
printf 'start:%s\n' "$*" >> "$TEST_TRACE"
STUB
  chmod +x "$root/scripts/"*.sh
  printf '%s\n' "$root"
}

# Fresh non-interactive automation can bootstrap and start when a complete
# explicit model configuration is already supplied.
root="$(make_case fresh)"; trace="$root/trace"
(
  cd "$root"
  TEST_TRACE="$trace" NICCOLO_LLM_BASE_URL=http://127.0.0.1:18080 NICCOLO_LLM_MODEL=mock \
    bash scripts/setup-local-mvp.sh --no-browser
)
grep -Fx 'bootstrap' "$trace" >/dev/null
grep -Fx 'start:--no-browser' "$trace" >/dev/null
! grep -Fx 'configure' "$trace" >/dev/null 2>&1

# Existing complete local/model state skips bootstrap and model reconfiguration.
root="$(make_case existing)"; trace="$root/trace"; mkdir -p "$root/.niccolo-local"
printf 'x\n' > "$root/.niccolo-local/local.env"; printf 'x\n' > "$root/.niccolo-local/identifiers.txt"; printf 'x\n' > "$root/.niccolo-local/token.txt"
cat > "$root/.niccolo-local/model.env" <<'ENV'
export NICCOLO_LLM_BASE_URL=http://127.0.0.1:18080
export NICCOLO_LLM_MODEL=mock
ENV
(cd "$root" && TEST_TRACE="$trace" bash scripts/setup-local-mvp.sh)
grep -Fx 'start:' "$trace" >/dev/null
! grep -Fx 'bootstrap' "$trace" >/dev/null 2>&1
! grep -Fx 'configure' "$trace" >/dev/null 2>&1

# Partial private state is never overwritten automatically.
root="$(make_case partial)"; trace="$root/trace"; mkdir -p "$root/.niccolo-local"; printf 'x\n' > "$root/.niccolo-local/local.env"
if (cd "$root" && TEST_TRACE="$trace" NICCOLO_LLM_BASE_URL=http://127.0.0.1:18080 NICCOLO_LLM_MODEL=mock bash scripts/setup-local-mvp.sh) >/dev/null 2>&1; then
  echo 'partial bootstrap unexpectedly accepted' >&2; exit 1
fi
[[ ! -e "$trace" ]]

# Non-interactive sessions fail clearly when model configuration is absent.
root="$(make_case nomodel)"; trace="$root/trace"; mkdir -p "$root/.niccolo-local"
printf 'x\n' > "$root/.niccolo-local/local.env"; printf 'x\n' > "$root/.niccolo-local/identifiers.txt"; printf 'x\n' > "$root/.niccolo-local/token.txt"
if (cd "$root" && TEST_TRACE="$trace" bash scripts/setup-local-mvp.sh) >/dev/null 2>&1; then
  echo 'missing model unexpectedly accepted non-interactively' >&2; exit 1
fi
[[ ! -e "$trace" ]]

# An explicit partial override must not borrow a stored secret/configuration.
root="$(make_case override)"; trace="$root/trace"; mkdir -p "$root/.niccolo-local"
printf 'x\n' > "$root/.niccolo-local/local.env"; printf 'x\n' > "$root/.niccolo-local/identifiers.txt"; printf 'x\n' > "$root/.niccolo-local/token.txt"
cat > "$root/.niccolo-local/model.env" <<'ENV'
export NICCOLO_LLM_BASE_URL=https://example.invalid/v1
export NICCOLO_LLM_MODEL=stored
export NICCOLO_LLM_API_KEY=stored-secret
ENV
if (cd "$root" && TEST_TRACE="$trace" NICCOLO_LLM_MODEL=override-only bash scripts/setup-local-mvp.sh) >/dev/null 2>&1; then
  echo 'partial explicit model override unexpectedly accepted' >&2; exit 1
fi
[[ ! -e "$trace" ]]

printf '%s\n' 'PASS: one-command local setup orchestration contract'
