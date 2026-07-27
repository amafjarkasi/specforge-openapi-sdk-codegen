---
title: Incremental Updates
sidebar_position: 2
description: Incremental development notes
---


# Incremental Generation

For large specs, regenerating SDKs on every commit is wasteful. Use specforge's deterministic output with CI caching for incremental generation.

## GitHub Actions

```yaml
- name: Cache generated SDK
  uses: actions/cache@v4
  with:
    path: ./generated
    key: sdk-${{ hashFiles('openapi.yaml') }}-${{ hashFiles('Cargo.lock') }}

- name: Generate SDK (skip if cached)
  run: |
    if [ -d "./generated" ]; then
      echo "SDK already cached"
    else
      specforge generate openapi.yaml -o ./generated -l ts
    fi
```

## How it works

1. The cache key includes the spec hash AND the specforge version (via Cargo.lock)
2. If neither changes, the cached SDK is reused
3. specforge's output is deterministic — same inputs always produce the same files
4. To force regeneration, bust the cache by changing the key

## CI Pipeline

```yaml
- name: Check spec
  run: specforge check openapi.yaml --strict

- name: Diff against main
  if: github.event_name == 'pull_request'
  run: |
    git show origin/main:openapi.yaml > /tmp/old.yaml
    specforge diff /tmp/old.yaml openapi.yaml
```
