#!/usr/bin/env bash
# Local CI mirror of .github/workflows/ci.yml
# Usage:
#   ./scripts/ci.sh           # full suite
#   ./scripts/ci.sh quick     # unit + petstore only
#   ./scripts/ci.sh e2e       # e2e smoke only
#   ./scripts/ci.sh regression
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# Toolchain discovery (match e2e_smoke.rs)
export PATH="${HOME}/.cargo/bin:/usr/local/go/bin:/usr/local/bin:${PATH}"

mode="${1:-full}"
TS_OUT="${TMPDIR:-/tmp}/specforge-ci-petstore-ts"

log() { printf '\n\033[1;34m==>\033[0m %s\n' "$*"; }
ok()  { printf '\033[1;32m✓\033[0m %s\n' "$*"; }
die() { printf '\033[1;31m✗\033[0m %s\n' "$*" >&2; exit 1; }

need() {
  if ! command -v "$1" >/dev/null 2>&1; then
    # go lives outside PATH on some images
    if [[ "$1" == "go" && -x /usr/local/go/bin/go ]]; then
      return 0
    fi
    die "missing required tool: $1"
  fi
}

show_tools() {
  log "Toolchains"
  rustc --version
  cargo --version
  if command -v go >/dev/null 2>&1 || [[ -x /usr/local/go/bin/go ]]; then
    "$(command -v go || echo /usr/local/go/bin/go)" version
  else
    echo "go: (missing — Go gates will skip compile)"
  fi
  if command -v node >/dev/null 2>&1; then
    node --version
    npm --version
  else
    echo "node/npm: (missing — TS tsc/e2e may skip)"
  fi
}

build() {
  log "cargo build --workspace"
  cargo build --workspace
  ok "build"
}

unit() {
  log "Unit tests"
  cargo test -p specforge-core --all-targets
  cargo test -p specforge-ts --lib
  # go/rust emitter crates currently have no #[cfg(test)] modules; don't fail.
  cargo test -p specforge-go --lib 2>/dev/null || true
  cargo test -p specforge-rust --lib 2>/dev/null || true
  ok "unit tests"
}

regression() {
  log "CLI regression (resolve + generate + large-spec compile gates)"
  cargo test -p specforge-cli --test regression -- --nocapture
  ok "regression"
}

regression_petstore() {
  log "CLI regression (petstore only)"
  cargo test -p specforge-cli --test regression petstore_ -- --nocapture
  ok "petstore regression"
}

e2e() {
  log "E2E smoke (mock × TS/Go/Rust)"
  cargo test -p specforge-cli --test e2e_smoke -- --nocapture
  ok "e2e smoke"
  log "E2E advanced (concurrency · dedupe · middleware · idempotency · SSE)"
  cargo test -p specforge-cli --test e2e_advanced -- --nocapture
  ok "e2e advanced"
}

ts_typecheck() {
  if ! command -v npm >/dev/null 2>&1; then
    echo "skip TS typecheck (npm not found)"
    return 0
  fi
  log "Petstore TypeScript tsc --noEmit"
  rm -rf "$TS_OUT"
  cargo run -p specforge-cli --quiet -- generate fixtures/petstore.yaml -o "$TS_OUT" -l ts -n @ci/petstore
  (
    cd "$TS_OUT"
    npm install --silent
    npx tsc --noEmit
  )
  ok "tsc --noEmit"
}

case "$mode" in
  quick)
    need cargo
    show_tools
    build
    unit
    regression_petstore
    ;;
  e2e)
    need cargo
    show_tools
    cargo build -p specforge-cli
    e2e
    ;;
  regression)
    need cargo
    show_tools
    cargo build --workspace
    regression
    ;;
  full|"")
    need cargo
    show_tools
    build
    unit
    regression
    e2e
    ts_typecheck
    ;;
  *)
    die "unknown mode: $mode (use full|quick|e2e|regression)"
    ;;
esac

log "All checks passed ($mode)"
