#!/usr/bin/env bash
# Regenerate examples/petstore-{ts,go,rust}/sdk from fixtures/petstore.yaml.
#
# Usage:
#   ./scripts/generate-examples.sh           # regenerate all three
#   ./scripts/generate-examples.sh --only ts  # regenerate only TypeScript
#   ./scripts/generate-examples.sh --only go  # regenerate only Go
#   ./scripts/generate-examples.sh --only rust
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
export PATH="${HOME}/.cargo/bin:/usr/local/go/bin:${PATH}"

ONLY="${1:-}"
case "$ONLY" in
  --only) ONLY="${2:-}";;
esac

BIN="${ROOT}/target/debug/specforge"
# Always rebuild: a stale binary silently regenerates examples with old
# emitter logic (this has bitten us before). The incremental build is fast.
echo "==> building specforge"
cargo build -p specforge-cli -q

gen() {
  local lang="$1" dest="$2" name="$3"
  if [[ -n "$ONLY" && "$ONLY" != "$lang" ]]; then return; fi
  echo "==> $lang → $dest"
  rm -rf "$dest"
  mkdir -p "$(dirname "$dest")"
  "$BIN" generate fixtures/petstore.yaml -o "$dest" -l "$lang" -n "$name"
}

gen ts   examples/petstore-ts/sdk   "@examples/petstore"
gen go   examples/petstore-go/sdk   "github.com/example/petstore-example-go"
gen rust examples/petstore-rust/sdk "petstore_example_sdk"

# Drop heavy lock/target if any leaked
rm -rf examples/petstore-rust/sdk/target examples/petstore-ts/sdk/node_modules

echo "✓ examples regenerated"
