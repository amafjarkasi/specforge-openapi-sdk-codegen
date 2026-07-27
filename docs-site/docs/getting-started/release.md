---
title: Release Notes
sidebar_position: 1
description: Latest release information for specforge
---


# Release process

specforge is versioned as a **Cargo workspace** (`[workspace.package] version`).
All crates share the same version.

## Versioning

- **MAJOR** — breaking IR or CLI changes that break existing generate scripts  
- **MINOR** — new languages, runtime features, backward-compatible IR additions  
- **PATCH** — bug fixes, template fixes, docs  

Current version: see `Cargo.toml` → `[workspace.package].version`.

## Pre-release checklist

```bash
# 1. Full local CI
./scripts/ci.sh full

# 2. Explicit advanced e2e (included in full once wired; run directly too)
cargo test -p specforge-cli --test e2e_advanced -- --nocapture
cargo test -p specforge-cli --test e2e_smoke -- --nocapture

# 3. Smoke examples
./scripts/generate-examples.sh
```

Confirm:

- [ ] `CHANGELOG.md` has a section for the new version with date  
- [ ] Workspace version bumped in root `Cargo.toml`  
- [ ] `cargo build --release -p specforge-cli` succeeds  
- [ ] README feature matrix matches reality  
- [ ] GitHub Actions green on the release branch  

## Bump version

```bash
# Edit root Cargo.toml [workspace.package] version = "X.Y.Z"
# Then refresh lockfile:
cargo build --workspace
```

## Tag & GitHub release

```bash
VERSION=0.1.0
git add -A
git commit -m "release: v${VERSION}"
git tag -a "v${VERSION}" -m "specforge v${VERSION}"
git push origin HEAD
git push origin "v${VERSION}"
```

Pushing the tag triggers `.github/workflows/release.yml`, which automatically:

1. Builds cross-compiled binaries for **5 targets**:
   - `x86_64-unknown-linux-gnu` (Linux AMD64)
   - `aarch64-unknown-linux-gnu` (Linux ARM64, via `cross`)
   - `x86_64-apple-darwin` (macOS Intel)
   - `aarch64-apple-darwin` (macOS Apple Silicon)
   - `x86_64-pc-windows-msvc` (Windows)
2. Packages each as `.tar.gz` (unix) or `.zip` (Windows)
3. Generates `checksums-sha256.txt`
4. Creates a **GitHub Release** with all artifacts attached

The release notes are auto-generated from commits since the last tag. Paste the matching `CHANGELOG.md` section into the release description if you want richer notes.

## Publish binary (optional / manual)

If you need a local binary:

```bash
cargo build --release -p specforge-cli
# artifact: target/release/specforge
```

Crates.io publish is optional for the generator crates; generated SDKs are **not** published by this project (consumers generate their own).

## Generate example consumers

```bash
./scripts/generate-examples.sh
```

This regenerates `examples/petstore-{ts,go,rust}/sdk` from `fixtures/petstore.yaml` so docs stay in sync with the emitter.

## Hotfix releases

For template-only bugs (broken JSDoc, missing import):

1. Patch the emitter  
2. Bump **patch** version  
3. Add a `### Fixed` entry under the new changelog section  
4. Tag as usual  
