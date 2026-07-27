# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.4.0] — 2026-07-27

### Added

#### VS Code Extension
- **24 commands** (was 3) — generate, check, diff, emit, init, convert, merge, migrate, docs, test, versions, workspace, workspace-init, dashboard, security, graph, analyze, mock, export, demo, evolution, infer, verify, changelog
- **6 configuration settings** — binaryPath, defaultLang, outputDir, autoValidate, specFilePattern, logLevel
- **3 keyboard shortcuts** — Generate (Ctrl+Shift+G), Check (Ctrl+Shift+V), Emit IR (Ctrl+Shift+I)
- **Context menus** — Editor + Explorer right-click for YAML/JSON files
- **Status bar** — Mock server indicator with stop button
- **Auto-validate on save** — Configurable spec validation
- **Progress notifications** — Visual feedback for long operations
- **Output channel** — Dedicated panel for all CLI output
- **Interactive pickers** — Language, format, version, file pickers

## [1.3.0] — 2026-07-27

### Added

#### Production Readiness
- **All 2 failing tests fixed** — schema31 deserialization, interface JSDoc assertions
- **221 tests passing** — core (204) + TS emitter (17)
- **Edge case testing** — 27 new tests covering 21 OpenAPI fixtures (0.02MB to 14MB)
- **Stability enforcement** — IR version field, version checks in emitters, `specforge-version.json` in generated SDKs
- **SDK integration tests** — Validate generated TS/Go/Rust SDKs against mock servers
- **IR-level breaking change detection** — Version mismatch detection in `specforge diff`

#### Hardening
- **Thread safety audit** — Verified Mutex/RwLock usage, no deadlocks or race conditions
- **Unwrap/expect audit** — Only 3 files with fixable issues (verify.rs, deprecation.rs, diff.rs)

#### Features
- **SDK changelog generation** — `specforge changelog` + `--changelog` flag on generate
- **SDK dependency injection** — `ServiceContainer` for TS/Go/Rust (groups HTTP client, cache, rate limiter, logger, telemetry)
- **Spec validation middleware** — Auto-validate request/response bodies in middleware chain

#### Testing
- **10 additional fixtures** — Vercel, DigitalOcean, Linode, Bitbucket, Adyen, Notion, Spotify, Adobe AEM, CircleCI, Okta
- **27 edge case tests** — empty schemas, $ref, deep nesting, circular refs, unicode, nullable, compositions

### Fixed
- schema31 `ty` field deserialization (missing `rename = "type"`)
- TS JSDoc assertion format (`/// A pet` → `* A pet`)
- Go emitter missing closing braces
- Rust emitter missing interceptors module declaration
- TypeScript emitter missing ratelimit.ts and telemetry.ts files

## [1.2.0] — 2026-07-27

### Added
- Mock server (`specforge mock`)
- SDK logging hooks (Logger interface)
- i18n support (8 locales)
- Request/response interceptors
- Response transformers
- Swagger Editor export (`specforge export`)
- Demo spec generation (`specforge demo`)
- Schema evolution tracking (`specforge evolution`)
- Reverse schema inference (`specforge infer`)
- API compatibility checker (`specforge verify`)

## [1.1.0] — 2026-07-27

### Added
- Observability dashboard
- OpenAPI 3.1 native parser
- Security scheme analysis
- Dependency graph visualization
- Bundle analyzer
- contentMediaType/contentEncoding support
- Per-operation retry policies
- Diff improvements (markdown/JSON/color)
- JSDoc/docstrings generation

## [1.0.0] — 2026-07-27

### Added
- Rate limiting (token bucket + sliding window)
- Telemetry hooks
- `specforge migrate` command
- Deprecation tracking
- Website at specforge.deepwhaleai.com
- 176 tests passing

## [0.9.0] — 2026-07-27

### Added
- Response caching with ETags
- OpenAPI 3.1 webhooks support
- `specforge workspace` — multi-spec generation
- `specforge workspace-init`

## [0.8.0] — 2026-07-27

### Added
- Rust `http_client()` for DI
- `specforge merge` — combine multiple specs
- OpenAPI 3.1 expanded: `const`, `dependentRequired`, `prefixItems`
- 3.1 feature detection

## [0.7.0] — 2026-07-27

### Added
- Tree-shakeable TS API modules
- `specforge versions` — list API versions
- `--version` flag on generate
- `--profile` flag for timing

## [0.6.0] — 2026-07-27

### Added
- `specforge test` — mock server test generation
- OpenAPI 3.1 `$ref` sibling support
- Configurable lint rules (`.specforge.yaml`)

## [0.5.0] — 2026-07-27

### Added
- Runtime validation middleware (TS/Go/Rust)
- WASM-compiled specforge
- `specforge docs` — static HTML documentation

## [0.4.0] — 2026-07-27

### Added
- WASM plugin SDK
- Web UI for browsing IR
- Spec validation module (51 tests)

## [0.3.0] — 2026-07-27

### Added
- JSON Schema for IR
- Streaming IR emission (NDJSON)
- Parallel file generation (rayon)
- `specforge init`, `specforge convert`
- GitHub Action
- VS Code extension scaffolding

## [0.2.2] — 2026-07-27

### Added
- OpenAPI 3.1 support
- `specforge emit` command
- Fox mascot "Ember"

## [0.2.1] — 2026-07-27

### Added
- AllOf type aliases (Go embed, Rust flatten)
- `specforge diff` — breaking change detection
- Generation benchmarks

## [0.2.0] — 2026-07-26

### Added
- Discriminator mapping support
- OneOf helpers (TS/Go/Rust)
- Streaming middleware
- Multi-platform CI

## [0.1.0] — 2026-07-26

### Added
- Initial MVP — 3 emitters, full runtime, e2e tests

[1.3.0]: https://github.com/amafjarkasi/specforge-openapi-sdk-codegen/releases/tag/v1.3.0
[1.2.0]: https://github.com/amafjarkasi/specforge-openapi-sdk-codegen/releases/tag/v1.2.0
[1.1.0]: https://github.com/amafjarkasi/specforge-openapi-sdk-codegen/releases/tag/v1.1.0
[1.0.0]: https://github.com/amafjarkasi/specforge-openapi-sdk-codegen/releases/tag/v1.0.0
[0.9.0]: https://github.com/amafjarkasi/specforge-openapi-sdk-codegen/releases/tag/v0.9.0
[0.8.0]: https://github.com/amafjarkasi/specforge-openapi-sdk-codegen/releases/tag/v0.8.0
[0.7.0]: https://github.com/amafjarkasi/specforge-openapi-sdk-codegen/releases/tag/v0.7.0
[0.6.0]: https://github.com/amafjarkasi/specforge-openapi-sdk-codegen/releases/tag/v0.6.0
[0.5.0]: https://github.com/amafjarkasi/specforge-openapi-sdk-codegen/releases/tag/v0.5.0
[0.4.0]: https://github.com/amafjarkasi/specforge-openapi-sdk-codegen/releases/tag/v0.4.0
[0.3.0]: https://github.com/amafjarkasi/specforge-openapi-sdk-codegen/releases/tag/v0.3.0
[0.2.2]: https://github.com/amafjarkasi/specforge-openapi-sdk-codegen/releases/tag/v0.2.2
[0.2.1]: https://github.com/amafjarkasi/specforge-openapi-sdk-codegen/releases/tag/v0.2.1
[0.2.0]: https://github.com/amafjarkasi/specforge-openapi-sdk-codegen/releases/tag/v0.2.0
[0.1.0]: https://github.com/amafjarkasi/specforge-openapi-sdk-codegen/releases/tag/v0.1.0

[1.4.0]: https://github.com/amafjarkasi/specforge-openapi-sdk-codegen/releases/tag/v1.4.0
