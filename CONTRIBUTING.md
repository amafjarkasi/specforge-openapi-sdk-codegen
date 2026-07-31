# Contributing to specforge

## Getting started

```bash
git clone https://github.com/amafjarkasi/specforge-openapi-sdk-codegen
cd specforge
cargo build --workspace
./scripts/ci.sh quick    # before you start
# ... hack ...
./scripts/ci.sh          # before you push
```

## PR checklist

Before opening a PR, run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

CI enforces `fmt`, `clippy -D warnings`, and the full test suite on every push.

## Architecture rules

These are non-negotiable — CI will catch violations:

1. **Emitters only see the IR.** Never import `openapiv3` types outside `crates/specforge-core`.
2. **Keep output deterministic.** Prefer `BTreeMap` / `IndexMap` over `HashMap` for emission order.
3. **No `/* */` inside doc comments / templates.** It breaks generated TypeScript and Go.
4. **Large-spec gates are sacred.** If GitHub/Stripe stop compiling in regression tests, fix the emitter, don't skip the test.
5. **Dedupe must hand each waiter a fresh body.** Never share one consumed `Response`.

## Adding a new emitter feature

When adding a new generated module (e.g. a new runtime file emitted into the SDK):

1. Add the emit function in the emitter's `lib.rs`.
2. Add it to the file-collection list in `collect` / `collect_index`.
3. If the module exports public types, add a re-export in the barrel (`emit_lib` / `collect_index`).
4. Regenerate the example SDK: `./scripts/generate-examples.sh` (it always rebuilds the CLI first).
5. Verify the generated example compiles.
6. Add a test that the new module exists in the generated output.

## Reserved names

All three emitters guard against spec model names colliding with built-in SDK types. If you add a new built-in type to the runtime, add it to the emitter's `is_*_sdk_builtin_type` / `is_*_builtin_type` list in the same crate.

## Documentation

Every PR that changes crate behavior must include a `CHANGELOG.md` entry under `## [Unreleased]`:

- Use subsections: `### Added`, `### Changed`, `### Fixed`, `### Removed`
- One bullet per logical change
- Pure formatting, CI, docs-only, or internal refactors with no behavior change don't need entries

## CI

[`.github/workflows/ci.yml`](.github/workflows/ci.yml) runs on every push/PR:

| Job | What it checks |
|-----|----------------|
| **test** (Linux/macOS/Windows) | Build, `cargo fmt --check`, `cargo clippy -D warnings`, unit tests, regression (GitHub + Stripe specs compile in Rust/Go/TS), e2e smoke + advanced, TS typecheck |
| **quick** | Build + unit + petstore regression (fast signal) |

## License

By contributing, you agree your code is licensed under the [MIT License](LICENSE).
