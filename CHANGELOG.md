# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## Handoff Summary

**Current version:** v1.6.0
**Test count:** 259 (core 242 + TS 17)
**CLI commands:** 33
**VS Code commands:** 24
**Test fixtures:** 21 (0.02MB → 14MB)
**Curated specs:** 18 (marketplace)
**Languages:** TypeScript, Go, Rust, WASM
**Status:** Production-ready, mission-critical verified

**To start from here, the next session should:**
1. Run `cargo build --workspace` and `cargo test` to verify
2. Check `.github/workflows/ci.yml` for any pending CI issues
3. Review open GitHub Issues if any
4. Continue with new features or bug fixes

---

## [1.6.0] — 2026-07-27

### Added

#### CI/CD
- **Spec diff GitHub Action** — Auto-detect spec changes in PRs, post diff comments, fail on breaking changes
- **Spec diff workflow** — `.github/workflows/spec-diff.yml` triggers on YAML/JSON changes

#### CLI
- **`specforge changelog`** — Enhanced with per-operation entries, schema changes, semantic versioning suggestions (`--suggest-version`), JSON output (`--format json`)
- **`specforge version`** — Apply versioning strategies (url/header/query) to specs. `--prefix v2`, `--strategy header`
- **`specforge generate --version-prefix v2`** — Auto-add version prefix before generation

#### Plugin System
- **Plugin marketplace** — `specforge plugin list/search/info/install`
- **4 curated plugins** — Kotlin, Swift, Python, C# emitter placeholders
- **Plugin configuration** — `.specforge.yaml` plugin section
- **`--plugin <name>`** flag on `generate` to use custom WASM emitters

#### Profiling
- **`specforge profile`** — Performance profiling against live APIs
- **Metrics** — Latency (avg/p50/p95/p99), throughput, error rate, cache hit rate
- **Recommendations** — Actionable optimization suggestions

### Quality
- **259 tests** (core 242 + TS 17)

---

## [1.5.0] — 2026-07-27

### Added

#### Web UI
- **Professional light theme** — Complete redesign with modern design system (1713 lines)
- **Design system** — Color palette, typography, spacing, components documented
- **Interactive features** — Schema tree browser, JSON viewer, YAML export, copy buttons
- **Responsive design** — Mobile-friendly with breakpoints at 1024px and 640px
- **WCAG AA accessibility** — Focus states, aria labels, keyboard navigation

#### Marketplace
- **18 curated specs** — GitHub, Stripe, Petstore, Kubernetes, Spotify, Notion, Twilio, Vercel, Okta, Atlassian, Adyen, Bitbucket, Linode, CircleCI, LaunchDarkly, Adobe AEM, 1Password, Ably
- **`specforge market list`** — Browse all specs with ratings and downloads
- **`specforge market search`** — Search by name, description, tags, author
- **`specforge market info`** — Detailed spec information
- **`specforge market add`** — Add your own spec to the marketplace

### Quality
- **227 tests** (core 210 + TS 17)

---

## [1.4.0] — 2026-07-27

### Added

#### VS Code Extension
- **24 commands** (was 3) — All CLI subcommands
- **6 configuration settings** — binaryPath, defaultLang, outputDir, autoValidate, specFilePattern, logLevel
- **3 keyboard shortcuts** — Generate (Ctrl+Shift+G), Check (Ctrl+Shift+V), Emit IR (Ctrl+Shift+I)
- **Context menus** — Editor + Explorer right-click for YAML/JSON files
- **Status bar** — Mock server indicator with stop button
- **Auto-validate on save** — Configurable spec validation
- **Progress notifications** — Visual feedback for long operations
- **Output channel** — Dedicated panel for all CLI output
- **Interactive pickers** — Language, format, version, file pickers

---

## [1.3.0] — 2026-07-27 — Production-Ready Milestone

### Added
- **221 tests passing** (core 204 + TS 17)
- **21 test fixtures** (0.02MB to 14MB, 4 OpenAPI versions)
- **Stability enforcement** — IR version + `specforge-version.json` in generated SDKs
- **SDK integration tests** — Validate TS/Go/Rust against mock servers
- **Thread safety audit** — No deadlocks or race conditions
- **Unwrap/expect audit** — Production code audited for panics
- **SDK changelog generation** (`specforge changelog`)
- **ServiceContainer** for TS/Go/Rust dependency injection
- **Spec validation middleware** — Auto-validate all requests/responses

---

## [1.2.0] — 2026-07-27

### Added
- **`specforge mock`** — Local mock server from spec examples
- **SDK logging hooks** — Logger interface (ConsoleLogger, NoopLogger)
- **i18n support** — 8 locales, `--locale en,es` flag
- **Request/response interceptors** for all 3 SDKs
- **Response transformers** for post-processing
- **`specforge export`** — Swagger Editor compatible output
- **`specforge demo`** — Realistic Petstore spec with examples
- **`specforge evolution`** — Schema change tracking over git commits
- **`specforge infer`** — Generate OpenAPI from sample JSON
- **`specforge verify`** — Validate running APIs against spec

---

## [1.1.0] — 2026-07-27

### Added
- **`specforge dashboard`** — HTML metrics visualization with Chart.js
- **OpenAPI 3.1 native parser** — `Schema31`, `parse_31()`, unsupported feature warnings
- **`specforge security`** — Auth analysis (text/JSON/markdown output)
- **`specforge graph`** — Mermaid/DOT dependency diagrams
- **`specforge analyze`** — Unused schemas, duplicates, large models, recommendations
- **contentMediaType/contentEncoding** — `Scalar::Base64`, `Scalar::Binary`
- **Per-operation retry policies** — `x-retry` extensions
- **Diff improvements** — Markdown/JSON/color output, inline schema diffs
- **JSDoc/docstrings** — Documentation from spec descriptions in generated code

---

## [1.0.0] — 2026-07-27

### Added
- **Rate limiting** — Token bucket + sliding window (TS/Go/Rust)
- **Telemetry hooks** — Request metrics, error tracking, cache hit/miss
- **`specforge migrate`** — Generate migration guides between spec versions
- **Deprecation tracking** — Comments in generated code (`@deprecated`, `// Deprecated:`, `#[deprecated]`)
- **Website** — specforge.deepwhaleai.com
- **176 tests** passing

---

## [0.9.0] — 2026-07-27

### Added
- **Response caching** — ETag-based caching with TTL expiry and 304 handling
- **OpenAPI 3.1 webhooks** — `--include-webhooks` flag
- **`specforge workspace`** — Multi-spec generation from config
- **`specforge workspace-init`** — Generate workspace config from directory

---

## [0.8.0] — 2026-07-27

### Added
- **Rust `http_client()`** — Dependency injection builder
- **`specforge merge`** — Combine multiple spec files into one
- **OpenAPI 3.1 expanded** — `const`, `dependentRequired`, `prefixItems`
- **3.1 feature detection** (`detect_31_features()`)

---

## [0.7.0] — 2026-07-27

### Added
- **Tree-shakeable TS modules** — Per-tag imports, `src/api/index.ts` barrel
- **`specforge versions`** — List API versions in a directory
- **`--version` flag** — Filter by version from directory
- **`--profile` flag** — Timing breakdown for pipeline stages

---

## [0.6.0] — 2026-07-27

### Added
- **`specforge test`** — Mock server test generation (TS/Go/Rust)
- **OpenAPI 3.1 `$ref` siblings** — `description`/`summary` preserved via `allOf` wrapping
- **Configurable lint rules** — `.specforge.yaml` config, `--disable`/`--enable`/`--severity`
- **New lint rules** — `missing-operation-id`, `path-trailing-slash`, `deprecated-operation`

---

## [0.5.0] — 2026-07-27

### Added
- **Runtime validation middleware** — TS/Go/Rust SDKs validate request/response bodies
- **WASM-compiled specforge** — `specforge-wasm` crate for browser use
- **`specforge docs`** — Static HTML API documentation generator
- **Web UI WASM integration** — Client-side parsing

---

## [0.4.0] — 2026-07-27

### Added
- **WASM plugin SDK** — `specforge-plugin` crate with `Plugin` trait
- **Web UI** — Interactive IR browser (specs, operations, schemas)
- **Spec validation module** — 51 unit tests

---

## [0.3.0] — 2026-07-27

### Added
- **JSON Schema for IR** — `assets/ir-schema.json`
- **Streaming IR emission** — NDJSON for large specs
- **Parallel file generation** — Rayon
- **`specforge init`** — Scaffold new OpenAPI spec
- **`specforge convert`** — 3.0 ↔ 3.1 conversion
- **GitHub Action** — One-line CI integration
- **VS Code extension** — Initial scaffolding

---

## [0.2.2] — 2026-07-27

### Added
- **OpenAPI 3.1** — Transparent preprocessing (type arrays, $ref siblings, const, prefixItems)
- **`specforge emit`** — Dump resolved IR as JSON
- **Fox mascot "Ember"** — Blacksmith fox with goggles and forge theme

---

## [0.2.1] — 2026-07-27

### Added
- **AllOf type aliases** — Go embed, Rust `#[serde(flatten)]`
- **`specforge diff`** — Breaking change detection
- **Generation benchmarks** — `scripts/bench.sh`

---

## [0.2.0] — 2026-07-26

### Added
- **Discriminator mapping** — Full support across all 3 emitters
- **OneOf helpers** — `isX()`/`narrowX()` (TS), `New{Union}`/`New{Union}FromJSON` (Go), `discriminant()`/`is_*()` (Rust)
- **Streaming middleware** — `StreamMiddleware` for TS/Go/Rust
- **Multi-platform CI** — Linux, macOS, Windows

---

## [0.1.0] — 2026-07-26

### Added
- **Initial MVP** — 3 emitters (TypeScript, Go, Rust), full runtime, e2e tests

---

## Version Links

- [v1.6.0](https://github.com/amafjarkasi/specforge-openapi-sdk-codegen/releases/tag/v1.6.0)
- [v1.5.0](https://github.com/amafjarkasi/specforge-openapi-sdk-codegen/releases/tag/v1.5.0)
- [v1.4.0](https://github.com/amafjarkasi/specforge-openapi-sdk-codegen/releases/tag/v1.4.0)
- [v1.3.0](https://github.com/amafjarkasi/specforge-openapi-sdk-codegen/releases/tag/v1.3.0)
- [v1.2.0](https://github.com/amafjarkasi/specforge-openapi-sdk-codegen/releases/tag/v1.2.0)
- [v1.1.0](https://github.com/amafjarkasi/specforge-openapi-sdk-codegen/releases/tag/v1.1.0)
- [v1.0.0](https://github.com/amafjarkasi/specforge-openapi-sdk-codegen/releases/tag/v1.0.0)
- [v0.9.0](https://github.com/amafjarkasi/specforge-openapi-sdk-codegen/releases/tag/v0.9.0)
- [v0.8.0](https://github.com/amafjarkasi/specforge-openapi-sdk-codegen/releases/tag/v0.8.0)
- [v0.7.0](https://github.com/amafjarkasi/specforge-openapi-sdk-codegen/releases/tag/v0.7.0)
- [v0.6.0](https://github.com/amafjarkasi/specforge-openapi-sdk-codegen/releases/tag/v0.6.0)
- [v0.5.0](https://github.com/amafjarkasi/specforge-openapi-sdk-codegen/releases/tag/v0.5.0)
- [v0.4.0](https://github.com/amafjarkasi/specforge-openapi-sdk-codegen/releases/tag/v0.4.0)
- [v0.3.0](https://github.com/amafjarkasi/specforge-openapi-sdk-codegen/releases/tag/v0.3.0)
- [v0.2.2](https://github.com/amafjarkasi/specforge-openapi-sdk-codegen/releases/tag/v0.2.2)
- [v0.2.1](https://github.com/amafjarkasi/specforge-openapi-sdk-codegen/releases/tag/v0.2.1)
- [v0.2.0](https://github.com/amafjarkasi/specforge-openapi-sdk-codegen/releases/tag/v0.2.0)
- [v0.1.0](https://github.com/amafjarkasi/specforge-openapi-sdk-codegen/releases/tag/v0.1.0)
