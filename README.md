<p align="center">
  <img src="assets/logo-banner.svg" alt="specforge" width="860"/>
</p>

<p align="center">
  <strong>Forge production-ready client SDKs from OpenAPI 3.x specs.</strong><br/>
  One language-neutral IR. Emitters for <b>TypeScript</b>, <b>Go</b>, and <b>Rust</b>.
</p>

<p align="center">
  <a href="#quick-start"><img src="https://img.shields.io/badge/quick%20start-2%20min-f97316?style=for-the-badge&labelColor=1a0f0a" alt="Quick start"/></a>
  <a href="#features"><img src="https://img.shields.io/badge/languages-TS%20%7C%20Go%20%7C%20Rust-ef4444?style=for-the-badge&labelColor=1a0f0a" alt="Languages"/></a>
  <a href="#testing--ci"><img src="https://img.shields.io/badge/tests-unit%20%2B%20regression%20%2B%20e2e-fbbf24?style=for-the-badge&labelColor=1a0f0a" alt="Tests"/></a>
  <a href="CHANGELOG.md"><img src="https://img.shields.io/badge/version-0.4.0-dc2626?style=for-the-badge&labelColor=1a0f0a" alt="Version"/></a>
  <a href="#license"><img src="https://img.shields.io/badge/license-MIT-fbbf24?style=for-the-badge&labelColor=1a0f0a" alt="License"/></a>
</p>

<p align="center">
  <img src="assets/logo.svg" alt="specforge mark" width="72"/>
</p>

---

## Why specforge?

Most OpenAPI generators dump incomplete types, ignore runtime concerns, or lock you into one language. **specforge** is built the other way around:

1. **Parse once** — OpenAPI YAML/JSON → resolved, language-neutral IR  
2. **Emit many** — walk the same IR for TypeScript, Go, or Rust  
3. **Ship clients that run** — auth, retry, timeouts, pagination, concurrency, dedupe, middleware, idempotency, streaming  

The goal is not “types that might compile.” It’s **SDKs you can point at a real API today**.

```mermaid
graph LR
    A["📄 OpenAPI 3.x<br/>YAML / JSON"] --> B["⚙️ specforge-core<br/>parse · resolve<br/>language-neutral IR"]
    B --> C["🔧 emitters<br/>TS · Go · Rust<br/>typed client + runtime"]

    style A fill:#1a0f0a,stroke:#f97316,color:#fef3c7
    style B fill:#1a0f0a,stroke:#ef4444,color:#fef3c7
    style C fill:#1a0f0a,stroke:#fbbf24,color:#fef3c7
```

---

## Features

### Generator
- OpenAPI **3.0 / 3.x** via `openapiv3` (YAML + JSON)
- Full **`$ref` resolution** without inlining blow-ups (named types stay named)
- **Composition**: `allOf` / `oneOf` / `anyOf`, plus discriminator metadata
- Inline **string enums** preserved for discriminant guards
- Deterministic output (stable ordering from the spec)
- Multi-language CLI: `-l ts|go|rust`

### Generated runtimes (all three languages)

| Capability | TypeScript | Go | Rust |
|---|:---:|:---:|:---:|
| Typed models + operations | ✅ | ✅ | ✅ |
| Bearer / API-key auth providers | ✅ | ✅ | ✅ |
| Retry + full-jitter exponential backoff | ✅ | ✅ | ✅ |
| Per-attempt timeouts | ✅ | ✅ | ✅ |
| Cursor & offset pagination helpers | ✅ | ✅ | ✅ |
| Concurrency semaphore (`maxConcurrent`) | ✅ | ✅ | ✅ |
| In-flight dedupe (GET/HEAD/OPTIONS) | ✅ | ✅ | ✅ |
| Middleware chain | ✅ | ✅ | ✅ |
| Idempotency-Key on POST/PUT/PATCH/DELETE | ✅ | ✅ | ✅ |
| SSE / chunk streaming helpers | ✅ | ✅ | ✅ |
| oneOf runtime type guards | ✅ | — | untagged enums |
| Tree-shakeable multi-file package | ✅ ESM/CJS | stdlib module | cargo crate |

### Quality gates
- **Unit tests** for IR + TypeScript emitter  
- **Regression** on petstore (vendored) + GitHub / Stripe (cached large specs)  
- **Compile gates**: generated Go must `go build`, generated Rust must `cargo check`  
- **E2E smoke**: mock × list/show/create + auth/retry/pagination (all 3 langs)  
- **E2E advanced**: concurrency serialisation, dedupe single-flight, middleware rewrite, idempotency-key on POST, SSE parse (all 3 langs)

---

## Quick start

### 1. Build the CLI

```bash
cargo build -p specforge-cli
# → target/debug/specforge
```

### 2. Generate an SDK

```bash
# TypeScript (default) — native fetch, dual ESM/CJS package
./target/debug/specforge generate openapi.yaml -o ./sdk-ts -l ts

# Go — stdlib net/http only
./target/debug/specforge generate openapi.yaml -o ./sdk-go -l go \
  -n github.com/acme/widget-go

# Rust — reqwest + serde
./target/debug/specforge generate openapi.yaml -o ./sdk-rs -l rust \
  -n widget_sdk

# Lint a spec without generating
./target/debug/specforge check openapi.yaml
./target/debug/specforge check openapi.yaml --strict
```

### 3. Call your API

<details open>
<summary><b>TypeScript</b></summary>

```ts
import { createClient, bearerAuth, streamSse } from "./sdk-ts/src/index.ts";

const client = createClient({
  baseUrl: "https://api.example.com",
  auth: bearerAuth(() => process.env.API_TOKEN!),
  maxConcurrent: 8,
  dedupe: true,
  idempotency: true,
  retry: { maxRetries: 3 },
});

const page = await client.pets.listPets({ limit: 20 });

// Streaming (SSE)
const res = await client.request("GET", "/events");
for await (const ev of streamSse(res)) {
  console.log(ev.event, ev.data);
}
```

</details>

<details>
<summary><b>Go</b></summary>

```go
c := sdk.NewClient().
    WithBaseURL("https://api.example.com").
    WithBearerToken(os.Getenv("API_TOKEN")).
    WithTimeout(10 * time.Second).
    WithMaxConcurrent(8).
    WithDedupe(true).
    WithIdempotency(true).
    WithRetry(sdk.DefaultRetryOptions())

c.Use(func(ctx context.Context, req *sdk.MiddlewareRequest, next func(context.Context, *sdk.MiddlewareRequest) (*sdk.MiddlewareResponse, error)) (*sdk.MiddlewareResponse, error) {
    start := time.Now()
    res, err := next(ctx, req)
    log.Printf("%s %s %v", req.Method, req.URL, time.Since(start))
    return res, err
})

pets, err := c.ListPets(ctx, 20)

// Streaming SSE
res, err := c.DoStream(ctx, "GET", "/events", nil, nil)
defer sdk.DrainAndClose(res)
it := sdk.NewSseIterator(res)
for it.Next() {
    ev := it.Event()
    fmt.Println(ev.Event, ev.Data)
}
```

</details>

<details>
<summary><b>Rust</b></summary>

```rust
use std::time::Duration;
use widget_sdk::{api, Client};
use widget_sdk::streaming::SseStream;

let client = Client::builder()
    .base_url("https://api.example.com")
    .bearer_token(std::env::var("API_TOKEN")?)
    .timeout(Duration::from_secs(10))
    .max_concurrent(8)
    .dedupe(true)
    .idempotency(true)
    .build()?;

let pets = api::list_pets(&client, Some(20)).await?;

// Streaming SSE
let res = client.request_stream(reqwest::Method::GET, "/events", &[], None).await?;
let mut sse = SseStream::new(res.bytes_stream());
while let Some(ev) = sse.next_event().await? {
    println!("{}: {}", ev.event, ev.data);
}
```

</details>

---

## CLI reference

```text
specforge <COMMAND>

Commands:
  generate  Generate an SDK from an OpenAPI spec
  check     Lint and validate an OpenAPI spec without generating
  diff      Compare two OpenAPI specs and report breaking changes
  emit      Emit the resolved IR as JSON (for external emitters / plugins)
  help      Print this message or the help of the given subcommand
```

### `specforge generate`

```text
specforge generate [OPTIONS] <SPEC>

Arguments:
  <SPEC>   Path to OpenAPI YAML or JSON

Options:
  -o, --out <DIR>           Output directory            [default: ./generated]
  -l, --lang <LANG>         ts | go | rust              [default: ts]
  -n, --name <NAME>         Package / module / crate name override
  -v, --log-level <LEVEL>   error|warn|info|debug|trace [default: info]
  -h, --help
  -V, --version
```

| Language | `-n` means | Default if omitted |
|----------|------------|--------------------|
| **ts** | npm package name in `package.json` | `@<title-slug>sdk` |
| **go** | Go module path in `go.mod` | `github.com/example/<title>-go` |
| **rust** | Cargo crate name | `<title>_sdk` |

### `specforge check`

```text
specforge check [OPTIONS] <SPEC>

Arguments:
  <SPEC>   Path to OpenAPI YAML or JSON

Options:
  --strict                   Treat warnings as errors
  -v, --log-level <LEVEL>    error|warn|info|debug|trace [default: info]
```

### `specforge diff`

```text
specforge diff [OPTIONS] <OLD> <NEW>

Arguments:
  <OLD>   Path to the old (baseline) OpenAPI spec
  <NEW>   Path to the new OpenAPI spec

Options:
  --breaking-only              Show only breaking changes
  -v, --log-level <LEVEL>      error|warn|info|debug|trace [default: info]
```

Exit code 1 if breaking changes are found — use in CI to gate releases.

### `specforge emit`

```text
specforge emit [OPTIONS] <SPEC>

Arguments:
  <SPEC>   Path to OpenAPI YAML or JSON

Options:
  -v, --log-level <LEVEL>    error|warn|info|debug|trace [default: warn]
```

Outputs the resolved IR as pretty-printed JSON to stdout. Use this to build external emitters:

```bash
specforge emit openapi.yaml | my-custom-emitter --input - --output ./sdk
```

---

## Architecture

specforge is a **Cargo workspace** with a hard boundary: emitters never touch `openapiv3` types. They only see the IR.

```text
specforge/
├── crates/
│   ├── specforge-core/     # parse · $ref resolve · IR
│   ├── specforge-ts/       # TypeScript emitter + rich runtime templates
│   ├── specforge-go/       # Go emitter (stdlib HTTP client)
│   ├── specforge-rust/     # Rust emitter (reqwest + serde)
│   └── specforge-cli/      # `specforge` binary + regression / e2e tests
├── fixtures/
│   ├── petstore.yaml       # small vendored fixture (always online)
│   └── sample-api.yaml     # auth · oneOf · cursor pagination sample
├── examples/               # consumer stubs (regenerate via script)
├── scripts/
│   ├── ci.sh               # local CI mirror
│   └── generate-examples.sh
├── assets/                 # logo + banner
├── CHANGELOG.md
├── RELEASE.md
└── .github/workflows/ci.yml
```

### Pipeline stages

| Stage | Crate | Responsibility |
|-------|-------|----------------|
| **Parse** | `specforge-core` | YAML/JSON → `openapiv3::OpenAPI` |
| **Resolve** | `specforge-core` | `$ref`s, security schemes, operations → `Document` IR |
| **Emit** | `specforge-{ts,go,rust}` | IR → idiomatic project on disk |
| **Orchestrate** | `specforge-cli` | CLI flags, logging, language dispatch |

### IR highlights

```rust
// Simplified view of crates/specforge-core/src/ir.rs
pub struct Document {
    pub title: String,
    pub version: String,
    pub base_url: Option<String>,
    pub security: Vec<SecurityScheme>,
    pub schemas: SchemaRegistry,   // named models, stable order
    pub operations: Vec<Operation>,
}

pub enum Type {
    Scalar(Scalar),
    StringEnum { variants: Vec<String>, nullable: bool },
    Array { item: Box<Type>, nullable: bool },
    Map { value: Box<Type> },
    Reference { name: String, nullable: bool },
    Composition(Composition),  // allOf | oneOf | anyOf + optional Discriminator
    Any,
    Unknown,
}
```

Design rules that keep large specs tractable (GitHub ~965 schemas / 1209 ops, Stripe ~1431 / 587):

- **References stay references** — no exponential inlining  
- **Cycles are safe** — self-`$ref` becomes a plain name  
- **Determinism** — `IndexMap` preserves spec order for bit-stable output  

---

## Generated project layouts

<details open>
<summary><b>TypeScript</b> <code>-l ts</code></summary>

```text
sdk-ts/
├── package.json          # dual ESM/CJS, sideEffects: false
├── tsconfig.json         # strict TS 5.6+
├── tsup.config.ts
├── README.md
└── src/
    ├── index.ts          # createClient() + re-exports
    ├── client.ts         # fetch core
    ├── auth.ts
    ├── retry.ts
    ├── paginate.ts
    ├── concurrency.ts    # async semaphore
    ├── dedup.ts          # in-flight GET coalescing (buffered bodies)
    ├── middleware.ts
    ├── idempotency.ts    # Idempotency-Key generation
    ├── streaming.ts      # streamBytes / streamLines / streamSse
    ├── errors.ts         # discriminated-union ApiError
    ├── models/<Name>.ts  # one file per schema (+ oneOf guards)
    └── api/<Tag>.ts      # one class per tag
```

</details>

<details>
<summary><b>Go</b> <code>-l go</code></summary>

```text
sdk-go/
├── go.mod
├── client.go             # Client, auth, DoJSON + DoStream pipeline
├── retry.go
├── paginate.go           # CursorPaginate / OffsetPaginate generics
├── concurrency.go        # Semaphore
├── dedup.go              # RequestDeduper
├── middleware.go
├── idempotency.go        # Idempotency-Key UUID
├── streaming.go          # NewSseIterator / StreamLines
├── models.go
├── api_<tag>.go          # methods on *Client
└── README.md
```

Zero third-party deps for the happy path (`net/http`, `encoding/json`).

</details>

<details>
<summary><b>Rust</b> <code>-l rust</code></summary>

```text
sdk-rs/
├── Cargo.toml            # reqwest · serde · tokio · futures-util · bytes
├── README.md
└── src/
    ├── lib.rs
    ├── client.rs         # Client + ClientBuilder + request_stream
    ├── error.rs
    ├── retry.rs
    ├── paginate.rs
    ├── concurrency.rs
    ├── dedup.rs
    ├── middleware.rs
    ├── idempotency.rs
    ├── streaming.rs      # SseStream
    ├── models.rs
    └── api/<tag>.rs
```

</details>

---

## Runtime deep dive

All three clients share the same conceptual request pipeline:

```mermaid
graph TD
    A["🚀 operation call"] --> B["build URL / query / body"]
    B --> C["acquire concurrency permit<br/>(optional)"]
    C --> D{"dedupe in-flight?<br/>GET/HEAD/OPTIONS"}
    D -->|yes| E["share result with<br/>concurrent callers"]
    D -->|no| F["retry loop"]
    F --> G["apply auth"]
    G --> H["attach Idempotency-Key<br/>(unsafe methods, once per loop)"]
    H --> I["per-attempt timeout"]
    I --> J["middleware chain"]
    J --> K["transport<br/>fetch / net/http / reqwest"]
    K --> L{"classify error"}
    L -->|retriable| F
    L -->|success| M["decode JSON → typed model"]
    L -->|non-retriable| N["throw"]
    E --> M

    style A fill:#1a0f0a,stroke:#f97316,color:#fef3c7
    style M fill:#1a0f0a,stroke:#22c55e,color:#fef3c7
    style N fill:#1a0f0a,stroke:#ef4444,color:#fef3c7
```

### Auth

| Language | Static bearer | Dynamic token | API key header |
|----------|---------------|---------------|----------------|
| TS | `bearerAuth(() => token)` | async getter supported | `apiKeyAuth(header, getKey)` |
| Go | `WithBearerToken(tok)` | `BearerAuth{GetToken: …}` | `WithAPIKey(header, key)` |
| Rust | `.bearer_token(tok)` | `Auth::BearerFn(…)` | `.api_key(header, key)` |

### Retry defaults

- **Max retries:** 2 (3 total attempts)  
- **Backoff:** full jitter, base 500ms, cap 8s  
- **Retriable statuses:** `408`, `429`, `502`, `503`, `504`  
- **Retriable methods:** `GET`, `HEAD`, `PUT`, `DELETE`, `(OPTIONS)` + transport/timeouts  

### Idempotency

Unsafe methods (`POST` / `PUT` / `PATCH` / `DELETE`) automatically receive a stable `Idempotency-Key` for the **entire retry loop** (one key generated, reused on retries). Disable with `idempotency: false` / `WithIdempotency(false)` / `.idempotency(false)`.

### Streaming (SSE)

| Language | Entry point | Parser |
|----------|-------------|--------|
| TS | `client.request("GET", "/events")` | `streamSse(res)` / `streamLines` / `streamBytes` |
| Go | `client.DoStream(ctx, "GET", "/events", …)` | `NewSseIterator(res)` |
| Rust | `client.request_stream(GET, "/events", …)` | `SseStream::new(res.bytes_stream())` |

Streaming calls **do not retry** (bodies are not replayable).

### oneOf narrowing (TypeScript)

```ts
import { isPetCreated, narrowPetEvent, type PetEvent } from "./models/PetEvent";

function handle(event: PetEvent) {
  if (isPetCreated(event)) {
    console.log(event.pet.name); // narrowed
    return;
  }
  switch (narrowPetEvent(event)) {
    case "PetUpdated": /* … */ break;
    case "PetDeleted": /* … */ break;
  }
}
```

---

## Examples

Ready-to-run consumer stubs live under [`examples/`](./examples):

```bash
./scripts/generate-examples.sh   # regenerate sdk/ trees from fixtures/petstore.yaml

cd examples/petstore-ts   && npm i && npx tsx main.mts
cd examples/petstore-go   && go run .
cd examples/petstore-rust && cargo run
```

Point them at your own server with `PETSTORE_URL=…`.

Repo fixtures for generator development:

| Fixture | What it stresses |
|---------|------------------|
| `fixtures/petstore.yaml` | Small happy path — list / show / create |
| `fixtures/sample-api.yaml` | Bearer auth, enums, `oneOf` + discriminator, cursor pages |

---

## Testing & CI

### Local

```bash
# Full suite — build, unit, large-spec compile gates, e2e (smoke + advanced), tsc
./scripts/ci.sh

# Faster loops
./scripts/ci.sh quick         # unit + petstore generate
./scripts/ci.sh regression    # includes GitHub/Stripe go build + cargo check
./scripts/ci.sh e2e           # smoke + advanced e2e
```

### What each suite proves

| Suite | Command | Coverage |
|-------|---------|----------|
| **Unit** | `cargo test -p specforge-core --all-targets`<br/>`cargo test -p specforge-ts --lib` | IR construction, TS naming/types/models/ops |
| **Regression** | `cargo test -p specforge-cli --test regression` | Petstore generate (TS/Go/Rust); GitHub + Stripe **resolve**; Go/Rust **generate + compile** on large specs |
| **E2E smoke** | `cargo test -p specforge-cli --test e2e_smoke` | Petstore basics + sample-api **auth / 503-retry / cursor pagination** (3 langs) |
| **E2E advanced** | `cargo test -p specforge-cli --test e2e_advanced` | **Concurrency** serialisation, **dedupe** single-flight, **middleware** header rewrite, **idempotency-key** on POST, **SSE** parse (3 langs) |

Large specs download on demand into `target/spec-cache/` and are **skipped (not failed)** when offline. Compile steps skip only if `go` / `cargo` are missing; a failed compile when the tool **is** present fails CI.

### GitHub Actions

[`.github/workflows/ci.yml`](.github/workflows/ci.yml) runs on every push/PR:

- **`test`** — full suite (Rust stable + Go 1.23 + Node 20, cached cargo + spec-cache)  
- **`quick`** — build + unit + petstore-only regression for a fast signal  

---

## Release

See **[RELEASE.md](./RELEASE.md)** for the full checklist. Short version:

```bash
# 1. bump [workspace.package].version in Cargo.toml
# 2. update CHANGELOG.md
./scripts/ci.sh full
./scripts/generate-examples.sh
git tag -a v0.1.0 -m "specforge v0.1.0"
git push origin v0.1.0
```

Binary: `cargo build --release -p specforge-cli` → `target/release/specforge`.

---

## Requirements

| Tool | Required for |
|------|----------------|
| **Rust 1.75+** (rustc/cargo) | Building specforge itself; Rust SDK `cargo check` |
| **Go 1.22+** | Go SDK `go build` + e2e (optional but recommended) |
| **Node 20+ / npm** | TypeScript `tsc` + e2e (optional but recommended) |

---

## Status & roadmap

**v0.4.0** — plugins, web UI, validation:

- [x] WASM plugin SDK (`specforge-plugin` crate + `export_plugin!` macro)  
- [x] Example WASM plugin (`examples/plugin-example/`)  
- [x] Web UI for browsing IR (`web-ui/index.html`)  
- [x] Spec validation middleware (`validate` module, 51 tests)  
- [x] Plugin documentation (`PLUGINS.md`)  

**v0.3.0** — ecosystem, DX, and stability:

- [x] JSON Schema for IR (`assets/ir-schema.json`, `emit --schema`)  
- [x] Streaming IR emission (`emit --stream` — NDJSON for large specs)  
- [x] Parallel file generation (rayon)  
- [x] `specforge init` — scaffold new OpenAPI specs  
- [x] `specforge convert` — 3.0 ↔ 3.1 conversion  
- [x] GitHub Action (`action.yml` — generate/check/diff/emit)  
- [x] VS Code extension scaffolding  
- [x] Stability policy (`STABILITY.md`)  
- [x] Incremental generation guide (`INCREMENTAL.md`)  

**v0.2.2** — OpenAPI 3.1, plugin system, mascot:

- [x] OpenAPI 3.1 support — transparent `type` array → nullable conversion, numeric `exclusiveMinimum` → boolean  
- [x] `specforge emit` — dump resolved IR as JSON for external emitters / plugins  
- [x] Fox blacksmith mascot + forge color scheme  

**v0.2.1** — allOf composition, spec diff, and benchmarks:

- [x] AllOf type aliases — Go embed, Rust `#[serde(flatten)]`  
- [x] `specforge diff <old> <new>` — breaking-change detection for CI  
- [x] Generation benchmarks (`scripts/bench.sh`) — GitHub/Stripe specs  

**v0.2.0** — composition, ergonomics, and release infrastructure:

- [x] Discriminator `mapping` support (all 3 emitters)  
- [x] Go oneOf helpers (`New{Union}`, `{Union}Discriminant`, `New{Union}FromJSON`)  
- [x] Rust oneOf helpers (`discriminant()`, `is_*()`, `into_*()`, `as_*()`)  
- [x] AllOf property merging (last-wins, required union)  
- [x] Spec linting (`specforge check [--strict]`)  
- [x] Go + Rust streaming middleware (`StreamMiddleware`)  
- [x] Cross-compiled release binaries (5 targets)  
- [x] Crates.io publish workflow  
- [x] Multi-platform CI (Linux, macOS, Windows)  
- [x] Richer generated READMEs (errors, pagination, concurrency, middleware, streaming)  

**Next up:**

- [ ] Request/response validation middleware in generated SDKs  
- [ ] WASM-compiled specforge for browser use  
- [ ] Spec documentation generator  

---

## Ecosystem

### GitHub Action

Use specforge in CI with the official GitHub Action:

```yaml
- uses: amafjarkasi/specforge-openapi-sdk-codegen@v0.2.2
  with:
    command: generate
    spec: openapi.yaml
    lang: ts
    output: ./sdk
```

Commands: `generate`, `check`, `diff`, `emit`

### VS Code Extension

Install the [specforge VS Code extension](vscode-extension/) for:
- One-click SDK generation
- Spec validation
- IR preview

### External Emitters

Build custom emitters using `specforge emit`:

```bash
# Pipe the IR to your custom emitter
specforge emit openapi.yaml | my-emitter --lang kotlin --output ./sdk

# Stream mode for large specs
specforge emit openapi.yaml --stream | while read -r line; do
  echo "$line" | process-schema
done
```

See [assets/ir-schema.json](assets/ir-schema.json) for the IR JSON Schema.

### Deterministic Output

specforge guarantees deterministic output — the same spec + version always produces identical files. Use this for:
- CI caching (hash spec + version as cache key)
- Reproducible builds
- Diff-friendly generated code

See [INCREMENTAL.md](INCREMENTAL.md) for CI caching strategies.

---

## Contributing

```bash
git clone <repo-url> && cd specforge
cargo build --workspace
./scripts/ci.sh quick     # before you start
# … hack …
./scripts/ci.sh           # before you push
```

Guidelines:

1. **Emitters only see the IR** — never import `openapiv3` outside `specforge-core`.  
2. **Keep output deterministic** — prefer `BTreeMap` / `IndexMap` over `HashMap` for emission order.  
3. **Don’t nest `/* */` inside JSDoc/templates** — it breaks generated TypeScript.  
4. **Large-spec gates are sacred** — if GitHub/Stripe stop compiling, fix the emitter, don’t skip.  
5. **Dedupe must hand each waiter a fresh body** — never share one consumed `Response`.  

---

## License

MIT © specforge contributors

---

<p align="center">
  <img src="assets/logo.svg" width="40" alt=""/>
  <br/>
  <sub>OpenAPI in. Typed clients out. Ship faster.</sub>
</p>
