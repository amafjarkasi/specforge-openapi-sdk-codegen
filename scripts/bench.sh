#!/usr/bin/env bash
# Benchmark specforge generation on large specs.
# Usage: ./scripts/bench.sh
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

BIN="target/release/specforge"
if [[ ! -x "$BIN" ]]; then
  echo "==> building release binary"
  cargo build --release -p specforge-cli -q
fi

SPEC_CACHE="target/spec-cache"
GITHUB="${SPEC_CACHE}/github.yaml"
STRIPE="${SPEC_CACHE}/stripe.json"

bench() {
  local spec="$1" lang="$2" name="$3" label="$4"
  local out
  out=$(mktemp -d)
  local start end elapsed
  start=$(date +%s%N)
  "$BIN" generate "$spec" -o "$out" -l "$lang" -n "$name" -v error 2>/dev/null
  end=$(date +%s%N)
  elapsed=$(( (end - start) / 1000000 ))
  local files
  files=$(find "$out" -type f | wc -l)
  printf "  %-12s %-6s %5d ms  (%d files)\n" "$label" "$lang" "$elapsed" "$files"
  rm -rf "$out"
}

echo "=== specforge generation benchmarks ==="
echo ""

if [[ -f "$GITHUB" ]]; then
  echo "GitHub API spec (~965 schemas, ~1209 ops):"
  bench "$GITHUB" ts   "@bench/github" "github"
  bench "$GITHUB" go   "github.com/bench/github-go" "github"
  bench "$GITHUB" rust "bench_github_sdk" "github"
  echo ""
fi

if [[ -f "$STRIPE" ]]; then
  echo "Stripe API spec (~1431 schemas, ~587 ops):"
  bench "$STRIPE" ts   "@bench/stripe" "stripe"
  bench "$STRIPE" go   "github.com/bench/stripe-go" "stripe"
  bench "$STRIPE" rust "bench_stripe_sdk" "stripe"
  echo ""
fi

echo "done."
