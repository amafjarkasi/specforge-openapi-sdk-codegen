# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.2.0] — 2026-07-27

### Added

#### SDK Features
- **Mock server** — `specforge mock` starts local HTTP mock server from spec examples
- **Logging hooks** — `Logger` interface/trait for all 3 SDKs (ConsoleLogger, NoopLogger)
- **i18n support** — `--locale en,es` flag generates localized error messages (8 locales)
- **Request/response interceptors** — Transform request body and response data
- **Response transformers** — Post-processing hooks on deserialized responses

#### Core
- **Swagger Editor export** — `specforge export` inlines all `$ref` for editor compatibility
- **Demo spec** — `specforge demo` generates realistic Petstore spec with examples
- **Schema evolution tracker** — `specforge evolution` tracks changes over git commits
- **Reverse inference** — `specforge infer` generates OpenAPI spec from sample JSON
- **Compatibility checker** — `specforge verify` validates running APIs against spec

#### Testing
- **208 tests** across core, emitters, and CLI
- Swagger Editor lint rules (`--profile swagger-editor`)

## [1.1.0] — 2026-07-27

### Added

#### Core
- **Observability dashboard** — `specforge dashboard` generates HTML dashboard with Chart.js for SDK metrics visualization.
- **OpenAPI 3.1 native parser** — `Schema31` model, `parse_31()`, `warn_unsupported_features()` for unsupported 3.1 features.
- **Security scheme analysis** — `specforge security` with text/JSON/markdown output.
- **Dependency graph** — `specforge graph` generates Mermaid/DOT diagrams.
- **Bundle analyzer** — `specforge analyze` detects unused schemas, duplicates, large models.
- **contentMediaType/contentEncoding** — `Scalar::Base64` and `Scalar::Binary` for binary data.
- **Per-operation retry policies** — `x-retry` extensions on operations.
- **Diff improvements** — Markdown/JSON/color output, inline schema diffs.
- **JSDoc/docstrings** — Documentation comments from spec descriptions in generated code.

#### Ecosystem
- **5 large test fixtures** — GitHub Enterprise (14MB), Atlassian, LaunchDarkly, OpenAI (3.1.0)
- **Performance benchmarks** — Criterion benchmarks + 13 perf tests
- **VS Code extension** — Complete rewrite: 15 commands, icons, auto-validate, progress, output channel
- **Website** — Landing page at specforge.deepwhaleai.com

## [1.0.0] — 2026-07-27

### Added

#### SDK runtimes
- **Rate limiting** — Token bucket and sliding window rate limiters for all 3 SDKs. TS: `rateLimiter` option. Go: `WithRateLimiter()`. Rust: `.rate_limiter()`.
- **Telemetry hooks** — Request metrics, error tracking, retry counting, cache hit/miss. TS/Go/Rust: `TelemetryHooks` interface/trait + `MetricsCollector`.

#### Core
- **Spec deprecation tracking** — `find_deprecations()` scans IR for deprecated operations/schemas. `generate_migration_guide()` compares two specs.
- **`specforge migrate`** — Generate Markdown migration guides between spec versions. Deprecation comments in generated code (`@deprecated`, `// Deprecated:`, `#[deprecated]`).

#### Fixes
- **Go pipeline** — Fixed unused `obj` variable in validate.go, fixed hyphenated field names

### Changed
- **v1.0.0** — First stable release. All roadmap items from README completed.
- Website live at `specforge.deepwhaleai.com`
- 6 external test fixtures (petstore, GitHub, Stripe, Kubernetes, Twilio)
- 176 tests across core, emitters, and CLI

## [0.9.0] — 2026-07-27

### Added

#### SDK runtimes
- **Response caching** — All three SDKs now support ETag-based caching. `ResponseCache` with TTL expiry, `If-None-Match` headers, 304 handling. TS: `cache` option on client. Go: `WithCache(ttl)`. Rust: `.cache_ttl(Duration)`.

#### Core
- **Webhooks support** — OpenAPI 3.1 `webhooks` section parsed from raw JSON (bypasses `openapiv3` crate limitation). `Webhook` IR type with method, path, request body, responses.
- **`specforge workspace`** — Generate SDKs for multiple specs from `.specforge-workspace.yaml` config. `--only` filter, `--dry-run` mode.
- **`specforge workspace-init`** — Scan a directory and generate a starter workspace config.

#### Emitters
- **TypeScript webhook types** — `webhooks.ts` with payload types, `WebhookHandler<T>`, factory functions.
- **Go webhook types** — `webhooks.go` with payload types and `WebhookHandler` func type.
- **Rust webhook types** — `webhooks.rs` with payload structs and `WebhookHandler` trait.

#### CLI
- **`--include-webhooks`** flag on `generate` — opt-in webhook type generation.

## [0.8.0] — 2026-07-27

### Added

#### Core
- **OpenAPI 3.1 expanded** — `const` → `enum: [value]`, `dependentRequired` merged into `required`, `prefixItems` → `items`. Upgraded 3.0→3.1 now converts `enum: [value]` → `const`. 31 new tests.
- **3.1 feature detection** — `detect_31_features()` reports which 3.1 features are used in a spec.
- **Spec composition** — New `merge` module. `specforge merge` combines multiple spec files (paths, schemas, security schemes). Later specs override on conflict.

#### Rust emitter
- **Dependency injection** — `ClientBuilder::http_client(reqwest::Client)` for injecting custom HTTP clients in tests.

#### CLI
- **`specforge merge`** — Merge multiple OpenAPI spec files into one. `--format json|yaml`, `--out FILE`.

## [0.7.0] — 2026-07-27

### Added

#### TypeScript emitter
- **Tree-shakeable API modules** — `src/index.ts` no longer re-exports API classes. Consumers import individual tags directly (`import { PetsApi } from "./api/Pets"`). New `src/api/index.ts` convenience barrel for callers who want everything. Subpath exports in `package.json` for `./api` and `./api/*`.

#### CLI
- **`specforge versions`** — List all API versions in a spec directory. Supports flat (`v1.yaml`, `v2.yaml`) and nested (`v1/openapi.yaml`) conventions.
- **`--version` flag on `generate`** — Filter by version when spec is a directory. Matches by exact version string, file stem, or directory name.
- **`--profile` flag on `generate` and `emit`** — Outputs detailed timing breakdown (parse, resolve, emit, total) to stderr.

#### Core
- **`scan_versions()`** — Scans a directory for spec files and extracts API versions.
- **`resolve_spec_path()`** — Resolves spec path with optional version filtering.

## [0.6.0] — 2026-07-27

### Added

#### Core
- **Mock server test generation** — New `specforge test` subcommand generates mock server tests for TS/Go/Rust SDKs. Creates mock HTTP servers from spec example responses and verifies SDK calls.
- **3.1 `$ref` sibling support** — `$ref` objects with sibling `description`/`summary`/`deprecated` are now preserved via `allOf` wrapping during preprocessing. IR `Type::Reference` carries optional `description` override.
- **Configurable lint rules** — New `lint_config` module with `LintConfig`, `LintRule`, `RuleSeverity`. Loads from `.specforge.yaml`. CLI flags: `--disable`, `--enable`, `--severity`, `--config`.
- **New lint rules** — `missing-operation-id`, `missing-schema-description` (off by default), `path-trailing-slash`, `deprecated-operation`.

## [0.5.0] — 2026-07-27

### Added

#### TypeScript emitter
- **Runtime validation** — `validate.ts` with per-model schema descriptors and validators. `validateRequest()` / `validateResponse()` called automatically when `validate: true` is set on the client.

#### Go emitter
- **Runtime validation** — `validate.go` with `ValidatePet(v any) error` per model. `WithValidation(true)` option on client.

#### Rust emitter
- **Runtime validation** — `validate.rs` with `fn validate_pet(v: &Value)` per model. `.validation(true)` builder option.

#### WASM
- **`specforge-wasm` crate** — Compiles core parsing/resolving to WASM via `wasm-bindgen`. Exposes `parse_and_resolve()` and `lint()` functions.
- **Web UI WASM integration** — "Parse Locally (WASM)" button for client-side spec parsing without `specforge emit`.

#### CLI
- **`specforge docs`** — Generate static HTML API documentation from a spec. Dark forge-themed with color-coded HTTP method badges.

## [0.4.0] — 2026-07-27

### Added

#### Core
- **Spec validation** — New `validate` module with 51 unit tests. Validates JSON values against IR Type definitions (scalars, enums, arrays, maps, references, compositions). Returns `ValidationError` with JSON-pointer paths.
- **WASM plugin SDK** — `specforge-plugin` crate with `Plugin` trait, `GeneratedFile`/`PluginResult` types, and `export_plugin!` macro for WASM targets.
- **Example plugin** — `examples/plugin-example/` — `ReadmePlugin` that generates a README from the IR.

#### Web UI
- **`web-ui/index.html`** — Self-contained single-page app for browsing IR: schema tree, operations tab, IR JSON viewer, stats dashboard. Dark forge-themed. No build step.

#### Docs
- **`PLUGINS.md`** — How to build WASM emitter plugins (protocol, build steps, example).

## [0.3.0] — 2026-07-26

### Added

#### Core
- **JSON Schema for IR** — `assets/ir-schema.json` (draft 2020-12) describes the full IR output. `specforge emit --schema` prints it.
- **Streaming IR emission** — `specforge emit --stream` outputs newline-delimited JSON (NDJSON) for processing large specs without loading everything into memory.
- **Parallel file generation** — All three emitters now write files in parallel using rayon.

#### CLI
- **`specforge init`** — Scaffold a new OpenAPI spec with a `/health` endpoint and README.
- **`specforge convert`** — Convert between OpenAPI 3.0 and 3.1 (`--to 3.0` or `--to 3.1`). Operates on raw YAML/JSON to preserve full spec structure.

#### Ecosystem
- **GitHub Action** — `action.yml` composite action for CI: `generate`, `check`, `diff`, `emit` commands. Auto-detects platform and installs the correct binary.
- **VS Code extension** — `vscode-extension/` scaffolding with Generate SDK, Check Spec, and Preview IR commands.
- **Stability policy** — `STABILITY.md` documents semver guarantees for the IR schema, CLI, and generated output.
- **Incremental generation guide** — `INCREMENTAL.md` documents CI caching strategies.

## [0.2.2] — 2026-07-26

### Added

#### Core
- **OpenAPI 3.1 support** — Transparent preprocessing layer that converts 3.1 specs to 3.0 before parsing. Handles `type: ["string", "null"]` → `nullable: true`, numeric `exclusiveMinimum`/`exclusiveMaximum` → boolean, and missing `paths` → empty object.
- **IR serialization** — All IR types now derive `serde::Serialize` for JSON emission.

#### CLI
- **`specforge emit`** — New subcommand that dumps the resolved IR as JSON to stdout. Enables external emitters and plugins to consume the IR without embedding specforge.

#### Branding
- **Fox blacksmith mascot** — New logo with a fox character wearing goggles, holding a hammer and document.
- **Forge color scheme** — Warm orange/amber/red palette replacing the previous blue/purple.

## [0.2.1] — 2026-07-26

### Added

#### Core
- **AllOf type aliases** — When `allOf` has exactly one `$ref` member, emitters use Go struct embedding and Rust `#[serde(flatten)]` instead of merging all properties into a flat struct.
- **Spec diff** — New `specforge diff <old> <new>` subcommand that compares two OpenAPI specs and reports breaking changes (removed operations, new required parameters/properties, type changes). Exits non-zero on breaking changes for CI gates.
- **Generation benchmarks** — `scripts/bench.sh` times generation on GitHub and Stripe specs across all 3 languages.

## [0.2.0] — 2026-07-26

### Added

#### Core
- **AllOf property merging** — `build_model()` now merges properties from all `allOf` members (later members override on name conflict, required sets are unioned). Go and Rust emitters get merged properties instead of lossy first-member/`map[string]any` fallbacks.
- **Discriminator `mapping` support** — IR `Discriminator` now carries `mapping: Option<IndexMap<String, String>>`. TS and Rust emitters use explicit mapping values when arm schemas don't embed single-variant string enums.
- **Spec linting** — New `specforge_core::lint` module with 4 checks: duplicate operation IDs (error), missing response descriptions (warning), missing operation summaries (warning), unused schemas (warning).
- **Circular reference tests** — Unit tests for self-referential schemas and mutual A→B→A cycles.

#### CLI
- **`specforge check` subcommand** — Lint and validate a spec without generating. `--strict` flag promotes warnings to errors.
- CLI refactored to subcommand-based: `specforge generate <SPEC>` and `specforge check <SPEC>`.

#### TypeScript emitter
- Discriminator mapping fallback in `discriminant_literal()` when arm schemas lack single-variant enums.

#### Go emitter
- **OneOf ergonomic helpers** — `New{Union}(m map[string]any)` and `{Union}Discriminant(v)` for discriminated unions.
- **`json.RawMessage` fallback** — `New{Union}FromJSON(raw json.RawMessage)` for non-discriminated oneOf unions.
- **Streaming middleware** — `StreamMiddleware` type + `UseStream()` method. Applied before HTTP call in `doOnceStream`; can modify headers without consuming the response body.
- **Richer README** — Generated README now includes Errors, Pagination, Concurrency, Dedupe, Middleware, Streaming/SSE, and Idempotency sections.

#### Rust emitter
- **OneOf ergonomic helpers** — `impl` block on each oneOf enum with `discriminant()`, `is_{arm}()`, `into_{arm}()`, `as_{arm}()`.
- **Streaming middleware** — `StreamMiddleware` type + `use_stream_middleware()`. Applied in `request_stream` before `builder.send()`.
- **Richer README** — Generated README now includes Errors, Pagination, Concurrency, Dedupe, Middleware, Streaming/SSE, and Idempotency sections.

#### CI & packaging
- **Multi-platform CI** — Test matrix expanded to `ubuntu-latest`, `macos-latest`, `windows-latest`.
- **Cross-compiled release binaries** — `.github/workflows/release.yml` builds for 5 targets (linux amd64/arm64, macOS Intel/Apple Silicon, Windows) on tag push.
- **Crates.io publish workflow** — `.github/workflows/publish.yml` with dry-run support.

### Changed
- CLI is now subcommand-based: `specforge generate <SPEC>` instead of `specforge <SPEC>`. The old flat invocation no longer works.
- Internal crate dependencies now specify version requirements for crates.io compatibility.

### Fixed
- Unmatched brace in Rust `emit_readme()` format string.

## [0.1.0] — 2026-07-26

First public MVP of **specforge**: OpenAPI → typed SDKs for TypeScript, Go, and Rust.

### Added

#### Core
- Language-neutral IR (`specforge-core`) with `$ref` resolution, composition (`allOf`/`oneOf`/`anyOf`), discriminators, and inline string enums
- CLI `specforge` with `-l ts|go|rust`, `-o`, `-n`, `-v`

#### TypeScript emitter (`specforge-ts`)
- Multi-file tree-shakeable package (ESM/CJS via tsup)
- Runtime: auth, retry, timeout, pagination, concurrency semaphore, in-flight dedupe, middleware, idempotency keys, SSE/chunk streaming
- oneOf runtime type guards (`isX` / `narrowX`) from discriminator literals
- Fixed nested JSDoc comment bug in concurrency template
- Dedupe buffers response bodies so concurrent `requestJson` callers each get a readable body

#### Go emitter (`specforge-go`)
- Stdlib-only client (`net/http`, `encoding/json`)
- Runtime: auth providers, retry + full-jitter backoff, timeouts, cursor/offset pagination, semaphore, dedupe, middleware, idempotency keys, SSE iterator (`NewSseIterator`), `DoStream`
- Large-spec hardening: field-name collisions (`+1`/`-1`), reserved param idents, `anyString` coercion

#### Rust emitter (`specforge-rust`)
- `reqwest` + `serde` + `tokio` crate layout
- Runtime: `Auth` enum, retry, timeouts, pagination, semaphore, dedupe, middleware, idempotency keys, `request_stream` / `SseStream`
- Untagged enums for oneOf; `Display` on string enums for query encoding

#### Quality
- Regression suite: petstore generate (TS/Go/Rust); GitHub + Stripe resolve + Go/Rust compile gates
- E2E smoke: petstore basics + sample-api auth/retry/pagination (all 3 langs)
- E2E advanced: concurrency serialisation, dedupe single-flight, middleware header rewrite, idempotency-key on POST, SSE parse (all 3 langs)
- CI: `.github/workflows/ci.yml` + `scripts/ci.sh` (`full` / `quick` / `e2e` / `regression`)

#### Docs & packaging
- Modern README with logo/banner, architecture, runtime matrix, examples
- `CHANGELOG.md`, `RELEASE.md`, `examples/*` consumer stubs
- `assets/logo.svg`, `assets/logo-banner.svg`

### Known limitations
- Go/Rust do not yet emit oneOf runtime type guards (Rust uses `#[serde(untagged)]`)
- Streaming middleware on Go `DoStream` does not rewrite responses (body stays open)
- Large-spec downloads require network on first run (cached under `target/spec-cache/`)

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
