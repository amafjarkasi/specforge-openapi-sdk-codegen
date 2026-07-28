# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

## [1.5.0] — 2026-07-27

### Added
- Professional light theme web UI (1713 lines, design system)
- Spec marketplace (18 curated specs, 4 CLI commands)
- 227 tests passing

## [1.4.0] — 2026-07-27

### Added
- VS Code extension: 24 commands, full IDE integration

## [1.3.0] — 2026-07-27

### Added
- Production-ready: 221 tests, 21 fixtures, stability, integration tests

## [1.2.0] — 2026-07-27

### Added
- Mock server, logging, i18n, interceptors, transformers, export, demo, evolution, infer, verify

## [1.1.0] — 2026-07-27

### Added
- Dashboard, 3.1 parser, security, graph, analyzer, contentMediaType, retry, diff++, JSDoc

## [1.0.0] — 2026-07-27

### Added
- Rate limiting, telemetry, migration, deprecation, website, 176 tests

## [0.9.0] — 2026-07-27

### Added
- Caching, webhooks, workspace

## [0.8.0] — 2026-07-27

### Added
- DI, merge, 3.1 expanded

## [0.7.0] — 2026-07-27

### Added
- Tree-shake modules, versions, profile flag

## [0.6.0] — 2026-07-27

### Added
- Mock test gen, 3.1 `$ref` siblings, lint config

## [0.5.0] — 2026-07-27

### Added
- Validation middleware, WASM specforge, `specforge docs`

## [0.4.0] — 2026-07-27

### Added
- WASM plugins, web UI, spec validation module

## [0.3.0] — 2026-07-27

### Added
- JSON Schema, streaming emit, parallel gen, init, convert, GitHub Action, VS Code

## [0.2.2] — 2026-07-27

### Added
- OpenAPI 3.1, emit command, fox mascot

## [0.2.1] — 2026-07-27

### Added
- AllOf embed/flatten, diff command, benchmarks

## [0.2.0] — 2026-07-26

### Added
- Discriminator mapping, oneOf helpers, streaming middleware, multi-platform CI

## [0.1.0] — 2026-07-26

### Added
- Initial MVP — 3 emitters, full runtime, e2e tests

[1.6.0]: https://github.com/amafjarkasi/specforge-openapi-sdk-codegen/releases/tag/v1.6.0
[1.5.0]: https://github.com/amafjarkasi/specforge-openapi-sdk-codegen/releases/tag/v1.5.0
[1.4.0]: https://github.com/amafjarkasi/specforge-openapi-sdk-codegen/releases/tag/v1.4.0
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
