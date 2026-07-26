#!/usr/bin/env bash
# Regenerate examples/petstore-{ts,go,rust}/sdk from fixtures/petstore.yaml.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
export PATH="${HOME}/.cargo/bin:/usr/local/go/bin:${PATH}"

BIN="${ROOT}/target/debug/specforge"
if [[ ! -x "$BIN" ]]; then
  echo "==> building specforge"
  cargo build -p specforge-cli -q
fi

gen() {
  local lang="$1" dest="$2" name="$3"
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
