//! `specforge-rust` — Rust SDK emitter for the `specforge-core` IR.
//!
//! Emits a `reqwest` + `serde` client crate that typechecks with `cargo check`.
//! Layout:
//!
//! ```text
//! Cargo.toml
//! src/lib.rs          // re-exports
//! src/client.rs       // HTTP client
//! src/error.rs        // typed errors
//! src/models.rs       // all schema types
//! src/api/<tag>.rs    // one module per tag
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use rayon::prelude::*;
use specforge_core::{
    CompositionKind, Document, EnumModel, Model, ObjectModel, Operation, ParamLocation, Scalar,
    Type,
};

/// Options controlling Rust emission.
pub struct GeneratorOptions {
    /// Output directory; created if missing.
    pub out_dir: PathBuf,
    /// Crate name written into Cargo.toml. Defaults to a derived slug.
    pub crate_name: Option<String>,
    /// Optional i18n configuration for localized error messages.
    pub i18n: Option<specforge_core::I18nConfig>,
}

/// Generate a Rust SDK crate into `opts.out_dir`. Returns relative paths written.
/// Files are written in parallel using rayon.
pub fn generate(doc: &Document, opts: &GeneratorOptions) -> std::io::Result<Vec<String>> {
    // IR version compatibility check.
    if doc.ir_version != specforge_core::IR_VERSION {
        eprintln!(
            "Warning: IR version {} may not be fully supported by this emitter (expected {}).",
            doc.ir_version,
            specforge_core::IR_VERSION
        );
    }

    let src = opts.out_dir.join("src");
    let api = src.join("api");
    std::fs::create_dir_all(&api)?;

    let crate_name = crate_name(doc, opts);

    // Collect all (relative_path, absolute_path, content) triples.
    let mut files: Vec<(String, PathBuf, String)> = Vec::new();

    let cargo = opts.out_dir.join("Cargo.toml");
    files.push((rel(&cargo, &opts.out_dir), cargo, emit_cargo_toml(doc, &crate_name)));

    let error = src.join("error.rs");
    files.push((rel(&error, &opts.out_dir), error, emit_error()));

    let client = src.join("client.rs");
    files.push((rel(&client, &opts.out_dir), client, emit_client(doc)));

    let retry = src.join("retry.rs");
    files.push((rel(&retry, &opts.out_dir), retry, emit_retry()));

    let paginate = src.join("paginate.rs");
    files.push((rel(&paginate, &opts.out_dir), paginate, emit_paginate()));

    let concurrency = src.join("concurrency.rs");
    files.push((rel(&concurrency, &opts.out_dir), concurrency, emit_concurrency()));

    let dedup = src.join("dedup.rs");
    files.push((rel(&dedup, &opts.out_dir), dedup, emit_dedup()));

    let middleware = src.join("middleware.rs");
    files.push((rel(&middleware, &opts.out_dir), middleware, emit_middleware()));
    let interceptors = src.join("interceptors.rs");
    files.push((rel(&interceptors, &opts.out_dir), interceptors, emit_interceptors()));

    let idempotency = src.join("idempotency.rs");
    files.push((rel(&idempotency, &opts.out_dir), idempotency, emit_idempotency()));

    let streaming = src.join("streaming.rs");
    files.push((rel(&streaming, &opts.out_dir), streaming, emit_streaming()));

    let cache = src.join("cache.rs");
    files.push((rel(&cache, &opts.out_dir), cache, emit_cache()));

    let ratelimit = src.join("ratelimit.rs");
    files.push((rel(&ratelimit, &opts.out_dir), ratelimit, emit_ratelimit()));

    let telemetry = src.join("telemetry.rs");
    files.push((rel(&telemetry, &opts.out_dir), telemetry, emit_telemetry()));

    let logging = src.join("logging.rs");
    files.push((rel(&logging, &opts.out_dir), logging, emit_logging()));

    let validate = src.join("validate.rs");
    files.push((rel(&validate, &opts.out_dir), validate, emit_validate(doc)));

    let validation_middleware = src.join("validation_middleware.rs");
    files.push((rel(&validation_middleware, &opts.out_dir), validation_middleware, emit_validation_middleware()));

    let models = src.join("models.rs");
    files.push((rel(&models, &opts.out_dir), models, emit_models(doc)));

    // Webhooks — handler types (only if webhooks are present).
    if !doc.webhooks.is_empty() {
        let webhooks = src.join("webhooks.rs");
        files.push((rel(&webhooks, &opts.out_dir), webhooks, emit_webhooks(doc)));
    }

    // Group ops by tag.
    let mut by_tag: BTreeMap<String, Vec<&Operation>> = BTreeMap::new();
    for op in &doc.operations {
        let tag = op.tag.clone().unwrap_or_else(|| "Default".into());
        by_tag.entry(tag).or_default().push(op);
    }

    let mut tag_mods: Vec<String> = Vec::new();
    for (tag, ops) in &by_tag {
        let mod_name = snake(&pascal(tag));
        tag_mods.push(mod_name.clone());
        let path = api.join(format!("{mod_name}.rs"));
        let content = emit_tag_file(tag, ops);
        files.push((rel(&path, &opts.out_dir), path, content));
    }

    // api/mod.rs
    let api_mod = api.join("mod.rs");
    let mut api_body = String::from("// Code generated by specforge. DO NOT EDIT.\n\n");
    for m in &tag_mods {
        api_body.push_str(&format!("pub mod {m};\n"));
        api_body.push_str(&format!("pub use {m}::*;\n"));
    }
    files.push((rel(&api_mod, &opts.out_dir), api_mod, api_body));

    // lib.rs
    let lib = src.join("lib.rs");
    let has_webhooks = !doc.webhooks.is_empty();
    files.push((rel(&lib, &opts.out_dir), lib, emit_lib(&tag_mods, has_webhooks)));

    let readme = opts.out_dir.join("README.md");
    files.push((rel(&readme, &opts.out_dir), readme, emit_readme(doc, &crate_name)));

    // specforge-version.json — version metadata for the generated SDK.
    files.push(collect_version_file(doc, &opts.out_dir));

    // Write all files in parallel.
    let written: Vec<String> = files
        .par_iter()
        .map(|(rel, abs, content)| {
            if let Some(parent) = abs.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(abs, content);
            rel.clone()
        })
        .collect();

    let mut written = written;
    written.sort();
    Ok(written)
}

fn crate_name(doc: &Document, opts: &GeneratorOptions) -> String {
    opts.crate_name.clone().unwrap_or_else(|| {
        let slug = doc
            .title
            .to_ascii_lowercase()
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c
                } else {
                    '_'
                }
            })
            .collect::<String>()
            .trim_matches('_')
            .to_string();
        let slug = if slug.is_empty() {
            "generated_sdk".into()
        } else {
            slug
        };
        // Crate names can't start with a digit.
        if slug.chars().next().unwrap().is_ascii_digit() {
            format!("sdk_{slug}")
        } else {
            format!("{slug}_sdk")
        }
    })
}

fn rel(abs: &Path, base: &Path) -> String {
    abs.strip_prefix(base)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| abs.to_string_lossy().into_owned())
}

// ─── Naming ──────────────────────────────────────────────────────────────────

fn pascal(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut next_upper = true;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            if next_upper {
                out.extend(ch.to_uppercase());
                next_upper = false;
            } else {
                out.push(ch);
            }
        } else {
            next_upper = true;
        }
    }
    if out.is_empty() {
        return "X".to_string();
    }
    if out.chars().next().unwrap().is_ascii_digit() {
        out.insert(0, 'X');
    }
    out
}

fn snake(input: &str) -> String {
    // Expand punctuation first so "+1"/"-1" don't both collapse to "1".
    let mut expanded = String::new();
    for ch in input.chars() {
        match ch {
            '+' => expanded.push_str("plus_"),
            '-' | '−' => expanded.push_str("minus_"),
            '.' => expanded.push_str("_dot_"),
            '/' => expanded.push_str("_slash_"),
            '@' => expanded.push_str("_at_"),
            '#' => expanded.push_str("_hash_"),
            '$' => expanded.push_str("_dollar_"),
            '%' => expanded.push_str("_pct_"),
            '&' => expanded.push_str("_and_"),
            '*' => expanded.push_str("_star_"),
            _ => expanded.push(ch),
        }
    }
    let mut out = String::new();
    for (i, ch) in expanded.chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.extend(ch.to_lowercase());
        } else if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if ch == '_'
            && !out.ends_with('_') {
                out.push('_');
            }
    }
    let out = out.trim_matches('_').to_string();
    let out = if out.is_empty() {
        "x".into()
    } else if out.chars().next().unwrap().is_ascii_digit() {
        format!("n_{out}")
    } else {
        out
    };
    if is_rust_reserved(&out) {
        format!("{out}_")
    } else {
        out
    }
}

fn unique_field_name(base: &str, used: &mut BTreeSet<String>) -> String {
    let mut candidate = base.to_string();
    let mut n = 2;
    while !used.insert(candidate.clone()) {
        candidate = format!("{base}_{n}");
        n += 1;
    }
    candidate
}

fn is_rust_reserved(s: &str) -> bool {
    matches!(
        s,
        "as" | "break"
            | "const"
            | "continue"
            | "crate"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "fn"
            | "for"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "Self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "type"
            | "unsafe"
            | "use"
            | "where"
            | "while"
            | "async"
            | "await"
            | "dyn"
            | "abstract"
            | "become"
            | "box"
            | "do"
            | "final"
            | "macro"
            | "override"
            | "priv"
            | "typeof"
            | "unsized"
            | "virtual"
            | "yield"
            | "try"
            | "gen"
            // Locals used by generated method bodies / signatures.
            | "client"
            | "path"
            | "query"
            | "body"
            | "result"
            | "ok"
            | "err"
            | "some"
            | "none"
            | "vec"
            | "string"
            | "option"
    )
}

// ─── Type rendering ──────────────────────────────────────────────────────────

fn render_type(ty: &Type) -> String {
    match ty {
        Type::Scalar(s) => match s {
            Scalar::String | Scalar::DateTime | Scalar::Uuid => "String".into(),
            Scalar::Integer => "i32".into(),
            Scalar::Integer64 => "i64".into(),
            Scalar::Float => "f64".into(),
            Scalar::Boolean | Scalar::Base64 | Scalar::Binary => "bool".into(),
        },
        Type::StringEnum { .. } => "String".into(),
        Type::Array { item, .. } => format!("Vec<{}>", render_type(item)),
        Type::Map { value } => format!(
            "::std::collections::HashMap<String, {}>",
            render_type(value)
        ),
        Type::Reference { name, nullable, .. } => {
            let n = pascal(name);
            if *nullable {
                format!("Option<{n}>")
            } else {
                n
            }
        }
        Type::Composition(c) => match c.kind {
            CompositionKind::AllOf => {
                // Approximate allOf as the first member when possible.
                c.members
                    .first()
                    .map(render_type)
                    .unwrap_or_else(|| "serde_json::Value".into())
            }
            CompositionKind::OneOf | CompositionKind::AnyOf => {
                // Named unions become enums at the model level; inline → Value.
                "serde_json::Value".into()
            }
        },
        Type::Any | Type::Unknown => "serde_json::Value".into(),
    }
}

// ─── Cargo.toml / lib ────────────────────────────────────────────────────────

fn emit_cargo_toml(doc: &Document, crate_name: &str) -> String {
    format!(
        r#"[package]
name = "{crate_name}"
version = "{version}"
edition = "2021"
description = "Generated Rust SDK for {title}"
license = "MIT"
publish = false

[dependencies]
reqwest = {{ version = "0.12", default-features = false, features = ["json", "rustls-tls", "stream"] }}
serde = {{ version = "1", features = ["derive"] }}
serde_json = "1"
thiserror = "1"
async-trait = "0.1"
tokio = {{ version = "1", features = ["rt-multi-thread", "macros", "time", "sync"] }}
futures-util = "0.3"
bytes = "1"
"#,
        crate_name = crate_name,
        version = sanitize_semver(&doc.version),
        title = doc.title.replace('"', "\\\""),
    )
}

fn sanitize_semver(v: &str) -> String {
    // Cargo wants x.y.z; specs often ship "1.0.0" already.
    let parts: Vec<&str> = v.split(|c: char| !c.is_ascii_digit() && c != '.').collect();
    let cleaned = parts.concat();
    let nums: Vec<&str> = cleaned.split('.').filter(|s| !s.is_empty()).collect();
    match nums.as_slice() {
        [a, b, c, ..] => format!("{a}.{b}.{c}"),
        [a, b] => format!("{a}.{b}.0"),
        [a] => format!("{a}.0.0"),
        _ => "0.1.0".into(),
    }
}

fn emit_lib(tag_mods: &[String], has_webhooks: bool) -> String {
    let _ = tag_mods;
    let mut out = String::from(
        r#"// Code generated by specforge. DO NOT EDIT.

//! Generated OpenAPI client.

pub mod api;
pub mod cache;
pub mod client;
pub mod concurrency;
pub mod dedup;
pub mod error;
pub mod idempotency;
pub mod interceptors;
pub mod logging;
pub mod middleware;
pub mod models;
pub mod paginate;
pub mod ratelimit;
pub mod retry;
pub mod streaming;
pub mod telemetry;
pub mod validate;
pub mod validation_middleware;
"#,
    );

    if has_webhooks {
        out.push_str("pub mod webhooks;
");
    }

    out.push_str(
        r#"
pub use client::{Auth, Client, ClientBuilder, ResponseTransformer, ServiceContainer};
pub use interceptors::{RequestInterceptor, ResponseInterceptor};
pub use concurrency::Semaphore;
pub use dedup::RequestDeduper;
pub use error::{Error, Result};
pub use idempotency::{is_idempotency_candidate, new_idempotency_key, IDEMPOTENCY_HEADER};
pub use logging::{ConsoleLogger, Logger, NoopLogger};
pub use middleware::{Middleware, MiddlewareRequest, MiddlewareResponse, StreamMiddleware};
pub use paginate::{cursor_paginate, offset_paginate, CursorPage, OffsetPage};
pub use ratelimit::{RateLimiter, SlidingWindow, TokenBucket};
pub use retry::RetryOptions;
pub use telemetry::{MetricsCollector, TelemetryHooks};
pub use streaming::{ServerSentEvent, SseStream};
pub use validation_middleware::{validation_middleware, EndpointSchema, RouteSchemaMap};
"#,
    );

    out
}

fn emit_error() -> String {
    r#"// Code generated by specforge. DO NOT EDIT.

use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("HTTP {status}: {body}")]
    Http {
        status: u16,
        body: String,
        url: String,
    },
    #[error("request timed out after {timeout_ms}ms")]
    Timeout { timeout_ms: u64 },
    #[error(transparent)]
    Transport(#[from] reqwest::Error),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    #[error("{0}")]
    Message(String),
}

impl Error {
    /// HTTP status when this is an [`Error::Http`], else `None`.
    pub fn status(&self) -> Option<u16> {
        match self {
            Error::Http { status, .. } => Some(*status),
            _ => None,
        }
    }
}
"#
    .to_string()
}

fn emit_retry() -> String {
    r#"// Code generated by specforge. DO NOT EDIT.

use std::time::Duration;

/// Retry policy applied around each request.
#[derive(Debug, Clone)]
pub struct RetryOptions {
    /// Attempts AFTER the first try. Default 2 (3 total).
    pub max_retries: u32,
    /// Exponential base delay. Default 500ms.
    pub base_delay: Duration,
    /// Cap on per-attempt sleep. Default 8s.
    pub max_delay: Duration,
    /// HTTP statuses that should be retried. Default: 408, 429, 502, 503, 504.
    pub retry_on_statuses: Vec<u16>,
}

impl Default for RetryOptions {
    fn default() -> Self {
        Self {
            max_retries: 2,
            base_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(8),
            retry_on_statuses: vec![408, 429, 502, 503, 504],
        }
    }
}

/// True for methods safe to replay by default.
pub fn is_retriable_method(method: &reqwest::Method) -> bool {
    matches!(
        *method,
        reqwest::Method::GET
            | reqwest::Method::HEAD
            | reqwest::Method::PUT
            | reqwest::Method::DELETE
            | reqwest::Method::OPTIONS
    )
}

/// Full-jitter backoff for a zero-based retry attempt index.
pub fn backoff_delay(attempt: u32, opts: &RetryOptions) -> Duration {
    let mut ceiling = opts.base_delay;
    for _ in 0..attempt {
        if ceiling >= opts.max_delay / 2 {
            ceiling = opts.max_delay;
            break;
        }
        ceiling = ceiling.saturating_mul(2);
    }
    if ceiling > opts.max_delay {
        ceiling = opts.max_delay;
    }
    if ceiling.is_zero() {
        return Duration::ZERO;
    }
    let nanos = ceiling.as_nanos() as u64;
    // Cheap LCG-ish jitter without pulling in a rand crate.
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(1)
        .wrapping_mul(6364136223846793005)
        .wrapping_add(attempt as u64);
    let jitter = if nanos == 0 { 0 } else { seed % nanos };
    Duration::from_nanos(jitter)
}
"#
    .to_string()
}

fn emit_concurrency() -> String {
    r#"// Code generated by specforge. DO NOT EDIT.

use std::sync::Arc;
use tokio::sync::{Semaphore as TokioSemaphore, SemaphorePermit};

/// Async semaphore bounding in-flight requests.
#[derive(Clone, Debug)]
pub struct Semaphore {
    inner: Arc<TokioSemaphore>,
}

impl Semaphore {
    pub fn new(max: usize) -> Self {
        assert!(max > 0, "max concurrent must be > 0");
        Self {
            inner: Arc::new(TokioSemaphore::new(max)),
        }
    }

    pub async fn acquire(&self) -> Result<SemaphorePermit<'_>, tokio::sync::AcquireError> {
        self.inner.acquire().await
    }
}
"#
    .to_string()
}

fn emit_dedup() -> String {
    r#"// Code generated by specforge. DO NOT EDIT.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use tokio::sync::{Mutex, broadcast};

use crate::error::{Error, Result};

/// A deduplicated response: body, status code, and ETag.
pub type DedupeResponse = (Vec<u8>, u16, String);

type BoxFut = Pin<Box<dyn Future<Output = Result<DedupeResponse>> + Send>>;

/// Coalesces identical in-flight safe requests.
#[derive(Clone, Default)]
pub struct RequestDeduper {
    inflight: Arc<Mutex<HashMap<String, broadcast::Sender<std::result::Result<DedupeResponse, String>>>>>,
}

impl std::fmt::Debug for RequestDeduper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RequestDeduper")
    }
}

impl RequestDeduper {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_safe_method(method: &reqwest::Method) -> bool {
        matches!(
            *method,
            reqwest::Method::GET | reqwest::Method::HEAD | reqwest::Method::OPTIONS
        )
    }

    /// Run `f`, sharing the result with concurrent callers of the same key.
    pub async fn dedupe<F, Fut>(&self, key: String, f: F) -> Result<DedupeResponse>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<DedupeResponse>> + Send + 'static,
    {
        let mut map = self.inflight.lock().await;
        if let Some(tx) = map.get(&key) {
            let mut rx = tx.subscribe();
            drop(map);
            match rx.recv().await {
                Ok(Ok(v)) => Ok(v),
                Ok(Err(msg)) => Err(Error::Message(msg)),
                Err(_) => Err(Error::Message("dedupe channel closed".into())),
            }
        } else {
            let (tx, _rx) = broadcast::channel(1);
            map.insert(key.clone(), tx.clone());
            drop(map);

            let result = f().await;
            let wire = match &result {
                Ok(v) => Ok(v.clone()),
                Err(e) => Err(e.to_string()),
            };
            let _ = tx.send(wire);

            let mut map = self.inflight.lock().await;
            map.remove(&key);
            result
        }
    }
}

// Silence unused when generated crates don't call the alias.
#[allow(dead_code)]
type _BoxFut = BoxFut;
"#
    .to_string()
}

fn emit_middleware() -> String {
    r#"// Code generated by specforge. DO NOT EDIT.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use reqwest::header::HeaderMap;

use crate::error::Result;

/// Mutable request descriptor passed through middleware.
#[derive(Debug, Clone)]
pub struct MiddlewareRequest {
    pub method: reqwest::Method,
    pub url: String,
    pub headers: HeaderMap,
    pub body: Option<Vec<u8>>,
}

/// Raw response after the transport round-trip.
#[derive(Debug, Clone)]
pub struct MiddlewareResponse {
    pub status: u16,
    pub headers: HeaderMap,
    pub body: Vec<u8>,
}

pub type BoxFuture = Pin<Box<dyn Future<Output = Result<MiddlewareResponse>> + Send>>;
pub type NextFn = Arc<dyn Fn(MiddlewareRequest) -> BoxFuture + Send + Sync>;

/// Middleware observes or rewrites a request/response. Call `next(req)` to continue.
pub type Middleware = Arc<dyn Fn(MiddlewareRequest, NextFn) -> BoxFuture + Send + Sync>;

/// Compose middlewares (registration order = outer-to-inner) around `dispatch`.
pub fn compose(middlewares: &[Middleware], dispatch: NextFn) -> NextFn {
    let mut h = dispatch;
    for mw in middlewares.iter().rev() {
        let mw = Arc::clone(mw);
        let next = Arc::clone(&h);
        h = Arc::new(move |req| mw(req, Arc::clone(&next)));
    }
    h
}

/// Streaming middleware can observe or rewrite request headers before streaming.
/// It cannot read the response body.
pub type StreamMiddleware = Arc<dyn Fn(MiddlewareRequest) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> + Send + Sync>;
"#
    .to_string()
}

fn emit_interceptors() -> String {
    r#"// Code generated by specforge. DO NOT EDIT.

/// Request interceptor: transform the request body before it is serialized
/// and sent. Return the (possibly modified) body.
pub trait RequestInterceptor: Send + Sync {
    /// Transform the request body. Return the modified body.
    fn transform(&self, body: serde_json::Value) -> serde_json::Value;
}

/// Response interceptor: transform the response body after it is deserialized.
/// Return the (possibly modified) body.
pub trait ResponseInterceptor: Send + Sync {
    /// Transform the response body. Return the modified body.
    fn transform(&self, body: serde_json::Value) -> serde_json::Value;
}
"#
    .to_string()
}

fn emit_idempotency() -> String {
    r#"// Code generated by specforge. DO NOT EDIT.

use std::time::{SystemTime, UNIX_EPOCH};

/// Header name used for unsafe-method idempotency keys.
pub const IDEMPOTENCY_HEADER: &str = "Idempotency-Key";

/// True for methods that benefit from an idempotency key on retries.
pub fn is_idempotency_candidate(method: &reqwest::Method) -> bool {
    matches!(
        *method,
        reqwest::Method::POST
            | reqwest::Method::PUT
            | reqwest::Method::PATCH
            | reqwest::Method::DELETE
    )
}

/// Generate a fresh RFC-4122 v4 UUID string without extra deps.
pub fn new_idempotency_key() -> String {
    let mut bytes = [0u8; 16];
    // Mix time + address entropy; good enough for collision avoidance.
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut state = t as u64
        ^ ((t >> 64) as u64)
        ^ std::process::id() as u64
        ^ ((&bytes as *const _) as u64);
    for b in &mut bytes {
        // xorshift64*
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        *b = (state >> 8) as u8;
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3],
        bytes[4], bytes[5],
        bytes[6], bytes[7],
        bytes[8], bytes[9],
        bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]
    )
}
"#
    .to_string()
}

fn emit_streaming() -> String {
    r#"// Code generated by specforge. DO NOT EDIT.

use std::pin::Pin;

use futures_util::Stream;
use futures_util::StreamExt;

use crate::error::{Error, Result};

/// One Server-Sent Event message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerSentEvent {
    pub event: String,
    pub data: String,
    pub id: Option<String>,
}

/// Async SSE parser over a byte stream (e.g. `response.bytes_stream()`).
pub struct SseStream<S> {
    inner: Pin<Box<S>>,
    buffer: String,
    pending: Option<ServerSentEvent>,
    done: bool,
}

impl<S> SseStream<S>
where
    S: Stream<Item = std::result::Result<bytes::Bytes, reqwest::Error>> + Send + Unpin,
{
    pub fn new(inner: S) -> Self {
        Self {
            inner: Box::pin(inner),
            buffer: String::new(),
            pending: None,
            done: false,
        }
    }

    /// Pull the next event, or `None` on end-of-stream.
    pub async fn next_event(&mut self) -> Result<Option<ServerSentEvent>> {
        if let Some(ev) = self.pending.take() {
            return Ok(Some(ev));
        }
        if self.done {
            return Ok(None);
        }
        loop {
            if let Some(ev) = self.drain_one() {
                return Ok(Some(ev));
            }
            match self.inner.next().await {
                Some(Ok(chunk)) => {
                    self.buffer
                        .push_str(&String::from_utf8_lossy(&chunk));
                }
                Some(Err(e)) => return Err(Error::Transport(e)),
                None => {
                    self.done = true;
                    // Flush trailing event without blank-line terminator.
                    if let Some(ev) = self.drain_one_force() {
                        return Ok(Some(ev));
                    }
                    return Ok(None);
                }
            }
        }
    }

    fn drain_one(&mut self) -> Option<ServerSentEvent> {
        if let Some(idx) = self.buffer.find("\n\n") {
            let block = self.buffer[..idx].to_string();
            self.buffer = self.buffer[idx + 2..].to_string();
            return parse_sse_block(&block);
        }
        // Also handle CRLF blank lines.
        if let Some(idx) = self.buffer.find("\r\n\r\n") {
            let block = self.buffer[..idx].to_string();
            self.buffer = self.buffer[idx + 4..].to_string();
            return parse_sse_block(&block);
        }
        None
    }

    fn drain_one_force(&mut self) -> Option<ServerSentEvent> {
        if self.buffer.trim().is_empty() {
            return None;
        }
        let block = std::mem::take(&mut self.buffer);
        parse_sse_block(&block)
    }
}

fn parse_sse_block(block: &str) -> Option<ServerSentEvent> {
    let mut event = "message".to_string();
    let mut data: Vec<String> = Vec::new();
    let mut id: Option<String> = None;
    for line in block.lines() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.is_empty() || line.starts_with(':') {
            continue;
        }
        let (field, value) = match line.split_once(':') {
            Some((f, v)) => (f, v.strip_prefix(' ').unwrap_or(v)),
            None => (line, ""),
        };
        match field {
            "event" => event = value.to_string(),
            "data" => data.push(value.to_string()),
            "id" => id = Some(value.to_string()),
            "retry" => {}
            _ => {}
        }
    }
    if data.is_empty() && event == "message" {
        return None;
    }
    Some(ServerSentEvent {
        event,
        data: data.join("\n"),
        id,
    })
}
"#
    .to_string()
}

fn emit_paginate() -> String {
    r#"// Code generated by specforge. DO NOT EDIT.

use crate::error::Result;

/// One page of a cursor-based list.
#[derive(Debug, Clone)]
pub struct CursorPage<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
}

/// Walk a cursor list until `next_cursor` is `None`/empty.
///
/// ```ignore
/// cursor_paginate(
///     |cursor| async move { /* call generated list method */ Ok(page) },
///     |items| { /* handle */ Ok(()) },
/// ).await?;
/// ```
pub async fn cursor_paginate<T, Fut, F, H>(mut fetch: F, mut handle: H) -> Result<()>
where
    F: FnMut(Option<String>) -> Fut,
    Fut: std::future::Future<Output = Result<CursorPage<T>>>,
    H: FnMut(Vec<T>) -> Result<()>,
{
    let mut cursor: Option<String> = None;
    loop {
        let page = fetch(cursor).await?;
        handle(page.items)?;
        match page.next_cursor {
            Some(c) if !c.is_empty() => cursor = Some(c),
            _ => return Ok(()),
        }
    }
}

/// One page of an offset/limit list.
#[derive(Debug, Clone)]
pub struct OffsetPage<T> {
    pub items: Vec<T>,
    pub total: Option<u64>,
}

/// Walk an offset/limit list until a short page is returned.
pub async fn offset_paginate<T, Fut, F, H>(
    limit: u32,
    mut fetch: F,
    mut handle: H,
) -> Result<()>
where
    F: FnMut(u32, u32) -> Fut,
    Fut: std::future::Future<Output = Result<OffsetPage<T>>>,
    H: FnMut(Vec<T>) -> Result<()>,
{
    let limit = if limit == 0 { 50 } else { limit };
    let mut offset = 0u32;
    loop {
        let page = fetch(offset, limit).await?;
        let n = page.items.len() as u32;
        handle(page.items)?;
        if n < limit {
            return Ok(());
        }
        offset = offset.saturating_add(n);
    }
}
"#
    .to_string()
}

fn emit_cache() -> String {
    r#"// Code generated by specforge. DO NOT EDIT.

//! Response caching with ETag/conditional request support.
//!
//! GET responses are cached; subsequent requests send `If-None-Match` and
//! return the cached body on 304 Not Modified.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// A cached response entry: the ETag returned by the server, the raw body,
/// and the wall-clock time the entry was stored.
#[derive(Debug, Clone)]
pub struct CacheEntry {
    pub etag: String,
    pub data: Vec<u8>,
    pub timestamp: Instant,
}

/// Thread-safe in-memory response cache keyed by full URL.
///
/// Supports TTL-based expiry and ETag-based conditional requests
/// (`If-None-Match` / 304 Not Modified).
#[derive(Clone, Debug)]
pub struct ResponseCache {
    inner: Arc<Mutex<HashMap<String, CacheEntry>>>,
    ttl: Duration,
}

impl ResponseCache {
    /// Create a new cache with the given time-to-live.
    pub fn new(ttl: Duration) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            ttl,
        }
    }

    /// Return the cached entry for `url` if it exists and has not expired.
    pub async fn get(&self, url: &str) -> Option<CacheEntry> {
        let map = self.inner.lock().await;
        map.get(url).and_then(|entry| {
            if entry.timestamp.elapsed() < self.ttl {
                Some(entry.clone())
            } else {
                None // expired
            }
        })
    }

    /// Store a response under `url` with its ETag and body bytes.
    pub async fn set(&self, url: &str, etag: &str, data: &[u8]) {
        let mut map = self.inner.lock().await;
        // Evict expired entries opportunistically.
        map.retain(|_, e| e.timestamp.elapsed() < self.ttl);
        map.insert(
            url.to_string(),
            CacheEntry {
                etag: etag.to_string(),
                data: data.to_vec(),
                timestamp: Instant::now(),
            },
        );
    }

    /// Remove all cached entries.
    pub async fn clear(&self) {
        self.inner.lock().await.clear();
    }

    /// Number of entries currently in the cache (including possibly expired).
    pub async fn len(&self) -> usize {
        self.inner.lock().await.len()
    }
}
"#
    .to_string()
}


fn emit_ratelimit() -> String {
    r#"// Code generated by specforge. DO NOT EDIT.

//! Rate limiting for controlling request throughput.

use async_trait::async_trait;
use tokio::sync::Mutex;

/// Rate limiter interface. The client calls `acquire()` before each request.
#[async_trait]
pub trait RateLimiter: Send + Sync {
    async fn acquire(&self) -> crate::error::Result<()>;
}

/// Token bucket rate limiter. Tokens refill at a constant rate up to a maximum.
pub struct TokenBucket {
    inner: Mutex<TokenBucketInner>,
}

struct TokenBucketInner {
    tokens: f64,
    max_tokens: f64,
    refill_rate: f64,
    last_refill: std::time::Instant,
}

impl TokenBucket {
    pub fn new(max_tokens: usize, refill_rate: f64) -> Self {
        Self {
            inner: Mutex::new(TokenBucketInner {
                tokens: max_tokens as f64,
                max_tokens: max_tokens as f64,
                refill_rate,
                last_refill: std::time::Instant::now(),
            }),
        }
    }

    fn refill(inner: &mut TokenBucketInner) {
        let now = std::time::Instant::now();
        let elapsed = now.duration_since(inner.last_refill).as_secs_f64();
        inner.tokens = (inner.max_tokens).min(inner.tokens + elapsed * inner.refill_rate);
        inner.last_refill = now;
    }
}

#[async_trait]
impl RateLimiter for TokenBucket {
    async fn acquire(&self) -> crate::error::Result<()> {
        loop {
            {
                let mut inner = self.inner.lock().await;
                Self::refill(&mut inner);
                if inner.tokens >= 1.0 {
                    inner.tokens -= 1.0;
                    return Ok(());
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }
}

/// Sliding window rate limiter.
pub struct SlidingWindow {
    inner: Mutex<SlidingWindowInner>,
}

struct SlidingWindowInner {
    requests: Vec<std::time::Instant>,
    max_requests: usize,
    window: std::time::Duration,
}

impl SlidingWindow {
    pub fn new(max_requests: usize, window: std::time::Duration) -> Self {
        Self {
            inner: Mutex::new(SlidingWindowInner {
                requests: Vec::new(),
                max_requests,
                window,
            }),
        }
    }
}

#[async_trait]
impl RateLimiter for SlidingWindow {
    async fn acquire(&self) -> crate::error::Result<()> {
        loop {
            {
                let mut inner = self.inner.lock().await;
                let cutoff = std::time::Instant::now() - inner.window;
                inner.requests.retain(|t| *t > cutoff);
                if inner.requests.len() < inner.max_requests {
                    inner.requests.push(std::time::Instant::now());
                    return Ok(());
                }
                if let Some(oldest) = inner.requests.first() {
                    let wait = inner.window - std::time::Instant::now().duration_since(*oldest);
                    drop(inner);
                    tokio::time::sleep(wait).await;
                }
            }
        }
    }
}
"#
    .to_string()
}

fn emit_logging() -> String {
    r#"// Code generated by specforge. DO NOT EDIT.

//! Structured logging abstraction. Consumers plug in their own logger;
//! the SDK logs requests, responses, retries, and cache hits/misses.

use std::fmt;

/// Structured logging trait. Implement this to plug in your own logger.
pub trait Logger: Send + Sync + fmt::Debug {
    fn debug(&self, message: &str);
    fn info(&self, message: &str);
    fn warn(&self, message: &str);
    fn error(&self, message: &str);
}

/// Default logger that writes to stderr via `eprintln!`.
#[derive(Debug, Clone, Copy)]
pub struct ConsoleLogger;

impl Logger for ConsoleLogger {
    fn debug(&self, message: &str) {
        eprintln!("[DEBUG] {}", message);
    }
    fn info(&self, message: &str) {
        eprintln!("[INFO]  {}", message);
    }
    fn warn(&self, message: &str) {
        eprintln!("[WARN]  {}", message);
    }
    fn error(&self, message: &str) {
        eprintln!("[ERROR] {}", message);
    }
}

/// A no-op logger that discards all messages.
#[derive(Debug, Clone, Copy)]
pub struct NoopLogger;

impl Logger for NoopLogger {
    fn debug(&self, _: &str) {}
    fn info(&self, _: &str) {}
    fn warn(&self, _: &str) {}
    fn error(&self, _: &str) {}
}
"#
    .to_string()
}

fn emit_telemetry() -> String {
    r#"// Code generated by specforge. DO NOT EDIT.

//! Telemetry hooks for request lifecycle observability.

/// Telemetry hooks for observing the SDK request lifecycle.
pub trait TelemetryHooks: Send + Sync + std::fmt::Debug {
    fn on_request_start(&self, _method: &str, _path: &str) {}
    fn on_request_end(&self, _method: &str, _path: &str, _duration_ms: u128, _status: u16) {}
    fn on_request_error(&self, _method: &str, _path: &str, _duration_ms: u128, _error: &crate::error::Error) {}
    fn on_retry(&self, _method: &str, _path: &str, _attempt: u32, _error: &crate::error::Error) {}
    fn on_cache_hit(&self, _method: &str, _path: &str) {}
    fn on_cache_miss(&self, _method: &str, _path: &str) {}
}

/// Built-in metrics collector.
#[derive(Debug, Clone, Default)]
pub struct MetricsCollector {
    inner: std::sync::Arc<tokio::sync::Mutex<MetricsInner>>,
}

#[derive(Debug, Clone, Default)]
struct MetricsInner {
    request_count: u64,
    error_count: u64,
    total_duration_ms: u64,
    retry_count: u64,
}

#[derive(Debug, Clone)]
pub struct Metrics {
    pub request_count: u64,
    pub error_count: u64,
    pub total_duration_ms: u64,
    pub avg_duration_ms: f64,
    pub retry_count: u64,
}

impl MetricsCollector {
    pub fn new() -> Self { Self::default() }
    pub async fn get_metrics(&self) -> Metrics {
        let m = self.inner.lock().await;
        Metrics {
            request_count: m.request_count,
            error_count: m.error_count,
            total_duration_ms: m.total_duration_ms,
            avg_duration_ms: if m.request_count > 0 { m.total_duration_ms as f64 / m.request_count as f64 } else { 0.0 },
            retry_count: m.retry_count,
        }
    }
}

impl TelemetryHooks for MetricsCollector {
    fn on_request_start(&self, _: &str, _: &str) {
        let inner = self.inner.clone();
        tokio::spawn(async move { inner.lock().await.request_count += 1; });
    }
    fn on_request_end(&self, _: &str, _: &str, dur_ms: u128, status: u16) {
        let inner = self.inner.clone();
        tokio::spawn(async move {
            let mut m = inner.lock().await;
            m.total_duration_ms += dur_ms as u64;
            if status >= 400 { m.error_count += 1; }
        });
    }
    fn on_request_error(&self, _: &str, _: &str, dur_ms: u128, _: &crate::error::Error) {
        let inner = self.inner.clone();
        tokio::spawn(async move {
            let mut m = inner.lock().await;
            m.error_count += 1;
            m.total_duration_ms += dur_ms as u64;
        });
    }
    fn on_retry(&self, _: &str, _: &str, _: u32, _: &crate::error::Error) {
        let inner = self.inner.clone();
        tokio::spawn(async move { inner.lock().await.retry_count += 1; });
    }
}
"#
    .to_string()
}
fn emit_client(doc: &Document) -> String {
    let base = doc
        .base_url
        .as_deref()
        .unwrap_or("http://localhost")
        .trim_end_matches('/');

    let default_api_key_header = doc
        .security
        .iter()
        .find_map(|s| match s {
            specforge_core::SecurityScheme::ApiKey { header } => Some(header.as_str()),
            _ => None,
        })
        .unwrap_or("");

    format!(
        r#"// Code generated by specforge. DO NOT EDIT.

use std::sync::Arc;
use std::time::Duration;

use reqwest::header::{{HeaderMap, HeaderName, HeaderValue, AUTHORIZATION}};
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::cache::ResponseCache;
		use crate::concurrency::Semaphore;
		use crate::interceptors::{{RequestInterceptor, ResponseInterceptor}};
		use crate::ratelimit::RateLimiter;
		use crate::telemetry::TelemetryHooks;
		use crate::dedup::RequestDeduper;
		use crate::error::{{Error, Result}};
		use crate::idempotency::{{is_idempotency_candidate, new_idempotency_key, IDEMPOTENCY_HEADER}};
			use crate::middleware::{{
			    compose, BoxFuture, Middleware, MiddlewareRequest, MiddlewareResponse, NextFn, StreamMiddleware,
			}};
		use crate::retry::{{backoff_delay, is_retriable_method, RetryOptions}};
		use crate::streaming::SseStream;
	
	/// Typed service container grouping all DI-able SDK dependencies.
	/// Create one with `ServiceContainer::new()`, override fields, then pass
	/// to `ClientBuilder::service_container()` to apply them all at once.
	#[derive(Clone)]
	pub struct ServiceContainer {{
	    pub http_client: reqwest::Client,
	    pub cache: Option<ResponseCache>,
	    pub rate_limiter: Option<Arc<dyn RateLimiter>>,
	    pub logger: Option<Arc<dyn crate::logging::Logger>>,
	    pub telemetry: Option<Arc<dyn TelemetryHooks>>,
	}}

	impl ServiceContainer {{
	    /// Create a new container with sensible defaults (default reqwest client,
	    /// console logger, no cache/rate limiter/telemetry).
	    pub fn new() -> Self {{
	        Self {{
	            http_client: reqwest::Client::builder()
	                .user_agent("specforge-rust-sdk")
	                .build()
	                .expect("failed to build default HTTP client"),
	            cache: None,
	            rate_limiter: None,
	            logger: None,
	            telemetry: None,
	        }}
	    }}

	    /// Set a custom HTTP client, replacing the default.
	    pub fn http_client(mut self, client: reqwest::Client) -> Self {{
	        self.http_client = client;
	        self
	    }}

	    /// Enable response caching with the given TTL.
	    pub fn cache_ttl(mut self, ttl: std::time::Duration) -> Self {{
	        self.cache = Some(ResponseCache::new(ttl));
	        self
	    }}

	    /// Set a pre-configured response cache.
	    pub fn cache(mut self, cache: ResponseCache) -> Self {{
	        self.cache = Some(cache);
	        self
	    }}

	    /// Set a rate limiter.
	    pub fn rate_limiter(mut self, limiter: impl RateLimiter + 'static) -> Self {{
	        self.rate_limiter = Some(Arc::new(limiter));
	        self
	    }}

	    /// Set a structured logger.
	    pub fn logger(mut self, logger: impl crate::logging::Logger + 'static) -> Self {{
	        self.logger = Some(Arc::new(logger));
	        self
	    }}

	    /// Set telemetry hooks.
	    pub fn telemetry(mut self, hooks: impl TelemetryHooks + 'static) -> Self {{
	        self.telemetry = Some(Arc::new(hooks));
	        self
	    }}
	}}

	impl Default for ServiceContainer {{
	    fn default() -> Self {{
	        Self::new()
	    }}
	}}

	/// Credential provider applied on every request.
	#[derive(Clone)]
	pub enum Auth {{
	    /// `Authorization: Bearer <token>` with a fixed token.
	    Bearer(String),
	    /// `Authorization: Bearer <token>` where the token is produced per call.
	    BearerFn(Arc<dyn Fn() -> std::result::Result<String, String> + Send + Sync>),
	    /// Named header API key with a fixed value.
	    ApiKey {{ header: String, key: String }},
	    /// Named header API key produced per call.
	    ApiKeyFn {{
	        header: String,
	        get_key: Arc<dyn Fn() -> std::result::Result<String, String> + Send + Sync>,
	    }},
	    /// No auth.
	    None,
	}}
	
	impl std::fmt::Debug for Auth {{
	    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {{
	        match self {{
	            Auth::Bearer(_) => write!(f, "Auth::Bearer(***)"),
	            Auth::BearerFn(_) => write!(f, "Auth::BearerFn(..)"),
	            Auth::ApiKey {{ header, .. }} => write!(f, "Auth::ApiKey({{header}}, ***)"),
	            Auth::ApiKeyFn {{ header, .. }} => write!(f, "Auth::ApiKeyFn({{header}}, ..)"),
	            Auth::None => write!(f, "Auth::None"),
	        }}
	    }}
	}}
	
	impl Auth {{
	    fn apply(&self, headers: &mut HeaderMap) -> Result<()> {{
	        match self {{
	            Auth::None => Ok(()),
	            Auth::Bearer(tok) => {{
	                let value = HeaderValue::from_str(&format!("Bearer {{tok}}"))
	                    .map_err(|e| Error::Message(e.to_string()))?;
	                headers.insert(AUTHORIZATION, value);
	                Ok(())
	            }}
	            Auth::BearerFn(f) => {{
	                let tok = f().map_err(Error::Message)?;
	                let value = HeaderValue::from_str(&format!("Bearer {{tok}}"))
	                    .map_err(|e| Error::Message(e.to_string()))?;
	                headers.insert(AUTHORIZATION, value);
	                Ok(())
	            }}
	            Auth::ApiKey {{ header, key }} => {{
	                let name = HeaderName::from_bytes(header.as_bytes())
	                    .map_err(|e| Error::Message(e.to_string()))?;
	                let value =
	                    HeaderValue::from_str(key).map_err(|e| Error::Message(e.to_string()))?;
	                headers.insert(name, value);
	                Ok(())
	            }}
	            Auth::ApiKeyFn {{ header, get_key }} => {{
	                let key = get_key().map_err(Error::Message)?;
	                let name = HeaderName::from_bytes(header.as_bytes())
	                    .map_err(|e| Error::Message(e.to_string()))?;
	                let value =
	                    HeaderValue::from_str(&key).map_err(|e| Error::Message(e.to_string()))?;
	                headers.insert(name, value);
	                Ok(())
	            }}
	        }}
	    }}
	}}
	
	/// Transform response data before it reaches the application.
	pub trait ResponseTransformer: Send + Sync {{
	    fn transform(&self, response: serde_json::Value) -> serde_json::Value;
	}}

	/// HTTP client for {title}.
	#[derive(Clone)]
	pub struct Client {{
	    base_url: String,
	    http: reqwest::Client,
	    auth: Auth,
	    default_headers: HeaderMap,
	    retry: RetryOptions,
	    /// Per-attempt timeout. `None` uses the underlying reqwest client default.
	    timeout: Option<Duration>,
	    semaphore: Option<Semaphore>,
		    dedupe: bool,
		    idempotency: bool,
		    validation: bool,
		    cache: Option<ResponseCache>,
		    rate_limiter: Option<Arc<dyn RateLimiter>>,
		    telemetry: Option<Arc<dyn TelemetryHooks>>,
			    logger: Option<Arc<dyn crate::logging::Logger>>,
			    deduper: RequestDeduper,
			    middlewares: Vec<Middleware>,
			    stream_middlewares: Vec<StreamMiddleware>,
			    response_transformers: Vec<Arc<dyn ResponseTransformer>>,
			    request_interceptors: Vec<Arc<dyn RequestInterceptor>>,
			    response_interceptors: Vec<Arc<dyn ResponseInterceptor>>,
			}}

	impl std::fmt::Debug for Client {{
	    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {{
	        f.debug_struct("Client")
	            .field("base_url", &self.base_url)
	            .field("auth", &self.auth)
	            .field("retry", &self.retry)
	            .field("timeout", &self.timeout)
		            .field("dedupe", &self.dedupe)
			            .field("idempotency", &self.idempotency)
			            .field("validation", &self.validation)
			            .field("cache", &self.cache.is_some())
			            .field("max_concurrent", &self.semaphore.as_ref().map(|_| "set"))
			            .field("middlewares", &self.middlewares.len())
			            .field("stream_middlewares", &self.stream_middlewares.len())
			            .finish()
			    }}
		}}
		
		impl Client {{
		    /// Build a client with the spec's default base URL.
		    pub fn new() -> Result<Self> {{
	        ClientBuilder::new().build()
	    }}
	
	    /// Start a builder.
	    pub fn builder() -> ClientBuilder {{
	        ClientBuilder::new()
	    }}
	
	    pub fn base_url(&self) -> &str {{
	        &self.base_url
	    }}
	
		    /// Register middleware (applied in registration order).
		    pub fn use_middleware(&mut self, mw: Middleware) -> &mut Self {{
		        self.middlewares.push(mw);
		        self
		    }}

		    /// Register stream middleware (header-only modifications before streaming).
		    pub fn use_stream_middleware(&mut self, mw: StreamMiddleware) -> &mut Self {{
		        self.stream_middlewares.push(mw);
		        self
		    }}
	
	    /// Issue a JSON request with concurrency + dedupe + retry + middleware.
	    /// `body` is serialized when `Some`. On 2xx, the response is deserialized
	    /// into `T` (use `()` for empty bodies).
	    pub async fn request_json<T, B>(
	        &self,
	        method: reqwest::Method,
	        path: &str,
	        query: &[(&str, String)],
	        body: Option<&B>,
	    ) -> Result<T>
	    where
	        T: DeserializeOwned,
	        B: Serialize + ?Sized,
	    {{
	        let url = format!("{{}}{{}}", self.base_url, path);

	        // Apply request interceptors before serializing the body.
	        let body = if let Some(b) = body {{
	            let mut val = serde_json::to_value(b)?;
	            for i in &self.request_interceptors {{
	                val = i.transform(val);
	            }}
	            Some(val)
	        }} else {{
	            None
	        }};
	        let body_bytes = match body {{
	            Some(b) => Some(serde_json::to_vec(&b)?),
	            None => None,
	        }};
	
	        // Concurrency gate.
	        let _permit = if let Some(sem) = &self.semaphore {{
	            Some(sem.acquire().await.map_err(|e| Error::Message(e.to_string()))?)
	        }} else {{
	            None
	        }};
	
	        // Logger: request start.
	        if let Some(l) = &self.logger {{
	            l.info(&format!("[request] {{}} {{}}", method.as_str(), path));
	        }}

	        // Telemetry: request start.
	        if let Some(t) = &self.telemetry {{
	            t.on_request_start(method.as_str(), path);
	        }}

	        // Rate limiting: wait for permission before proceeding.
	        if let Some(limiter) = &self.rate_limiter {{
	            limiter.acquire().await?;
	        }}
	
	        let query_owned: Vec<(String, String)> =
	            query.iter().map(|(k, v)| (k.to_string(), v.clone())).collect();
	
	        // --- ETag cache: check for GET requests ---
	        let is_get = method == reqwest::Method::GET;
	        let cached_entry = if is_get {{
	            if let Some(cache) = &self.cache {{
	                let entry = cache.get(&url).await;
	                if let Some(t) = &self.telemetry {{
	                    if entry.is_some() {{
	                        if let Some(l) = &self.logger {{
	                            l.debug(&format!("[cache] HIT {{}} {{}}", method.as_str(), path));
	                        }}
	                        t.on_cache_hit(method.as_str(), path);
	                    }} else {{
	                        if let Some(l) = &self.logger {{
	                            l.debug(&format!("[cache] MISS {{}} {{}}", method.as_str(), path));
	                        }}
	                        t.on_cache_miss(method.as_str(), path);
	                    }}
	                }}
	                entry
	            }} else {{
	                None
	            }}
	        }} else {{
	            None
	        }};
	
	        // One idempotency key for the whole retry loop.
	        let idem_key = if self.idempotency && is_idempotency_candidate(&method) {{
	            Some(new_idempotency_key())
	        }} else {{
	            None
	        }};
	
	        let cached_etag = cached_entry.as_ref().map(|e| e.etag.clone());

	        let (data, status, resp_etag) = if self.dedupe && RequestDeduper::is_safe_method(&method) {{
	            let key = if query_owned.is_empty() {{
	                format!("{{method}} {{url}}")
	            }} else {{
	                format!("{{method}} {{url}}?{{query_owned:?}}")
	            }};
	            let this = self.clone();
	            let method_c = method.clone();
	            let url_c = url.clone();
	            let query_c = query_owned.clone();
	            let body_c = body_bytes.clone();
	            let idem_c = idem_key.clone();
	            let inm = cached_etag.clone();
	            self.deduper
	                .dedupe(key, move || {{
	                    let this = this;
	                    let method = method_c;
	                    let url = url_c;
	                    let query = query_c;
	                    let body_bytes = body_c;
	                    let idem_key = idem_c;
		            let if_none_match = inm;
	                    async move {{
	                        this.do_with_retry(
	                            &method,
	                            &url,
	                            &query,
	                            body_bytes.as_deref(),
	                            idem_key.as_deref(),
	                            if_none_match.as_deref(),
	                        )
	                        .await
	                    }}
	                }})
	                .await?
	        }} else {{
	            self.do_with_retry(
	                &method,
	                &url,
	                &query_owned,
	                body_bytes.as_deref(),
	                idem_key.as_deref(),
	                cached_etag.as_deref(),
	            )
	            .await?
	        }};
	
        // --- ETag cache: handle 304 Not Modified and update on 200 ---
        let data = if is_get {{
            if let Some(cache) = &self.cache {{
                if status == 304 {{
                    if let Some(ref entry) = cached_entry {{
                        entry.data.clone()
                    }} else {{
                        data
                    }}
                }} else if (200..300).contains(&status) && !resp_etag.is_empty() {{
                    cache.set(&url, &resp_etag, &data).await;
                    data
                }} else {{
                    data
                }}
            }} else {{
                data
            }}
        }} else {{
            data
        }};
	
	        if status == 204 || data.is_empty() {{
	            return serde_json::from_str("null")
	                .or_else(|_| serde_json::from_str("{{}}"))
	                .map_err(Error::from);
	        }}
	        // Apply response transformers.
	        let mut value: serde_json::Value = serde_json::from_slice(&data)?;
        for t in &self.response_transformers {{
            value = t.transform(value);
        }}

        // Apply response interceptors.
        for i in &self.response_interceptors {{
            value = i.transform(value);
        }}
        Ok(serde_json::from_value(value)?)
	    }}
	
	    /// Issue a streaming request (no retry). Returns the raw Response; use
	    /// [`SseStream`] for SSE parsing. Caller owns the body.
	    pub async fn request_stream(
	        &self,
	        method: reqwest::Method,
	        path: &str,
	        query: &[(&str, String)],
	        body: Option<&[u8]>,
	    ) -> Result<reqwest::Response> {{
	        // Rate limiting: wait for permission before proceeding.
	        if let Some(limiter) = &self.rate_limiter {{
	            limiter.acquire().await?;
	        }}

	        let url = format!("{{}}{{}}", self.base_url, path);
	        let query_owned: Vec<(String, String)> =
	            query.iter().map(|(k, v)| (k.to_string(), v.clone())).collect();
	        let full_url = if query_owned.is_empty() {{
	            url
	        }} else {{
	            let q: String = query_owned
	                .iter()
	                .map(|(k, v)| format!("{{}}={{}}", k, urlencoding_lite(v)))
	                .collect::<Vec<_>>()
	                .join("&");
	            format!("{{}}?{{}}", url, q)
	        }};
	        let mut headers = self.default_headers.clone();
	        self.auth.apply(&mut headers)?;
	        if body.is_some() {{
	            headers.insert(
	                reqwest::header::CONTENT_TYPE,
	                HeaderValue::from_static("application/json"),
	            );
	        }}
	        if !headers.contains_key(reqwest::header::ACCEPT) {{
	            headers.insert(
	                reqwest::header::ACCEPT,
	                HeaderValue::from_static("text/event-stream, application/json"),
	            );
	        }}
	        if self.idempotency && is_idempotency_candidate(&method) {{
	            let name = HeaderName::from_bytes(IDEMPOTENCY_HEADER.as_bytes())
	                .map_err(|e| Error::Message(e.to_string()))?;
	            if !headers.contains_key(&name) {{
	                headers.insert(
	                    name,
	                    HeaderValue::from_str(&new_idempotency_key())
	                        .map_err(|e| Error::Message(e.to_string()))?,
	                );
	            }}
		        }}
		        // Apply stream middleware.
		        let mut mw_req = MiddlewareRequest {{
		            method: method.clone(),
		            url: full_url.clone(),
		            headers: headers.clone(),
		            body: body.map(|b| b.to_vec()),
		        }};
		        for mw in &self.stream_middlewares {{
		            mw(mw_req.clone()).await?;
		            headers = mw_req.headers.clone();
		        }}
		        let mut builder = self.http.request(method, &full_url).headers(headers);
	        if let Some(b) = body {{
	            builder = builder.body(b.to_vec());
	        }}
	        if let Some(t) = self.timeout {{
	            builder = builder.timeout(t);
	        }}
	        let res = builder.send().await.map_err(Error::from)?;
	        if !res.status().is_success() {{
	            let status = res.status().as_u16();
	            let bytes = res.bytes().await?;
	            return Err(Error::Http {{
	                status,
	                body: String::from_utf8_lossy(&bytes).into_owned(),
	                url: full_url,
	            }});
	        }}
	        Ok(res)
	    }}
	
	    /// Convenience: GET a path as an SSE stream.
	    pub async fn stream_sse(
	        &self,
	        path: &str,
	        query: &[(&str, String)],
	    ) -> Result<SseStream<impl futures_util::Stream<Item = std::result::Result<bytes::Bytes, reqwest::Error>> + Send + Unpin>> {{
	        let res = self
	            .request_stream(reqwest::Method::GET, path, query, None)
	            .await?;
	        Ok(SseStream::new(res.bytes_stream()))
	    }}
	
	    async fn do_with_retry(
	        &self,
	        method: &reqwest::Method,
	        url: &str,
	        query: &[(String, String)],
	        body_bytes: Option<&[u8]>,
	        idem_key: Option<&str>,
	        if_none_match: Option<&str>,
	    ) -> Result<(Vec<u8>, u16, String)> {{
	        let mut last_err: Option<Error> = None;
	        let max = self.retry.max_retries;
	        for attempt in 0..=max {{
	            if attempt > 0 {{
		        let delay = backoff_delay(attempt - 1, &self.retry);
		        tokio::time::sleep(delay).await;
		    }}
	            let start = std::time::Instant::now(); match self.do_once(method, url, query, body_bytes, idem_key, if_none_match, start).await {{
	                Ok(v) => return Ok(v),
	                Err(e) => {{
	                    let retriable = is_error_retriable(method, &e, &self.retry);
	                    last_err = Some(e);
	                    if !retriable || attempt == max {{
	                        break;
	                    }}
	                    // Logger: retry notification.
	                    if let Some(l) = &self.logger {{
	                        l.warn(&format!("[retry] {{}} {{}} error={{}}, attempt {{}}/{{}}", method.as_str(), url, last_err.as_ref().unwrap(), attempt + 2, max + 1));
	                    }}
		            // Telemetry: retry notification.
		            if let Some(t) = &self.telemetry {{
		                t.on_retry(method.as_str(), url, attempt + 1, &last_err.as_ref().unwrap());
		            }}
	                }}
	            }}
	        }}
	        Err(last_err.unwrap_or_else(|| Error::Message("request failed".into())))
	    }}
	
    async fn do_once(
        &self,
        method: &reqwest::Method,
        url: &str,
        query: &[(String, String)],
        body_bytes: Option<&[u8]>,
        idem_key: Option<&str>,
        if_none_match: Option<&str>,
        start: std::time::Instant,
    ) -> Result<(Vec<u8>, u16, String)> {{
		        let _start = start;
	
	        // Build full URL with query for middleware visibility.
	        let full_url = if query.is_empty() {{
	            url.to_string()
	        }} else {{
	            let q: String = query
	                .iter()
	                .map(|(k, v)| format!("{{}}={{}}", k, urlencoding_lite(v)))
	                .collect::<Vec<_>>()
	                .join("&");
	            format!("{{}}?{{}}", url, q)
	        }};
	
	        let mut headers = self.default_headers.clone();
	        self.auth.apply(&mut headers)?;
	        if body_bytes.is_some() {{
	            headers.insert(
	                reqwest::header::CONTENT_TYPE,
	                HeaderValue::from_static("application/json"),
	            );
	        }}
		        if let Some(key) = idem_key {{
		            let name = HeaderName::from_bytes(IDEMPOTENCY_HEADER.as_bytes())
		                .map_err(|e| Error::Message(e.to_string()))?;
		            if !headers.contains_key(&name) {{
		                headers.insert(
		                    name,
		                    HeaderValue::from_str(key).map_err(|e| Error::Message(e.to_string()))?,
		                );
		            }}
		        }}
		        if let Some(etag) = if_none_match {{
		            headers.insert(
		                reqwest::header::IF_NONE_MATCH,
		                HeaderValue::from_str(etag).map_err(|e| Error::Message(e.to_string()))?,
		            );
		        }}
	
		        let mw_req = MiddlewareRequest {{
	            method: method.clone(),
	            url: full_url.clone(),
	            headers,
	            body: body_bytes.map(|b| b.to_vec()),
	        }};
	
	        let http = self.http.clone();
	        let timeout = self.timeout;
	        let dispatch: NextFn = Arc::new(move |req: MiddlewareRequest| {{
	            let http = http.clone();
	            Box::pin(async move {{
	                let mut builder = http.request(req.method, &req.url);
	                builder = builder.headers(req.headers);
	                if let Some(bytes) = req.body {{
	                    builder = builder.body(bytes);
	                }}
	                if let Some(t) = timeout {{
	                    builder = builder.timeout(t);
	                }}
	                let res = builder.send().await.map_err(|e| {{
	                    if e.is_timeout() {{
	                        Error::Timeout {{
	                            timeout_ms: timeout.map(|d| d.as_millis() as u64).unwrap_or(0),
	                        }}
	                    }} else {{
	                        Error::Transport(e)
	                    }}
	                }})?;
	                let status = res.status().as_u16();
	                let headers = res.headers().clone();
	                let body = res.bytes().await?.to_vec();
	                Ok(MiddlewareResponse {{ status, headers, body }})
	            }}) as BoxFuture
	        }});
	
	        let handler = compose(&self.middlewares, dispatch);
	        let mw_res = handler(mw_req).await?;
	
	        let etag = mw_res.headers
	            .get("etag")
	            .and_then(|v| v.to_str().ok())
	            .unwrap_or("")
	            .to_string();
	        if !(200..300).contains(&mw_res.status) {{
	            // Pass through 304 for cache handling upstream.
	            if mw_res.status == 304 {{
		        return Ok((mw_res.body, mw_res.status, etag));
		    }}
	            let err = Error::Http {{
	                status: mw_res.status,
	                body: String::from_utf8_lossy(&mw_res.body).into_owned(),
	                url: full_url,
	            }};
	            // Telemetry: request error.
	            if let Some(t) = &self.telemetry {{
	                t.on_request_error(method.as_str(), url, _start.elapsed().as_millis(), &err);
	            }}
	            return Err(err);
	        }}
	        // Logger: response received.
	        if let Some(l) = &self.logger {{
	            l.info(&format!("[response] {{}} {{}} -> {{}}", method.as_str(), url, mw_res.status));
	        }}

	        // Telemetry: successful request.
	        if let Some(t) = &self.telemetry {{
	            t.on_request_end(method.as_str(), url, _start.elapsed().as_millis(), mw_res.status);
	        }}
	        Ok((mw_res.body, mw_res.status, etag))
	    }}
	}}
	
	fn urlencoding_lite(s: &str) -> String {{
	    let mut out = String::with_capacity(s.len());
	    for b in s.bytes() {{
	        match b {{
	            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {{
	                out.push(b as char)
	            }}
	            _ => out.push_str(&format!("%{{:02X}}", b)),
	        }}
	    }}
	    out
	}}
	
	fn is_error_retriable(method: &reqwest::Method, err: &Error, opts: &RetryOptions) -> bool {{
	    match err {{
	        Error::Timeout {{ .. }} => true,
	        Error::Transport(e) => e.is_timeout() || e.is_connect() || e.is_request(),
	        Error::Http {{ status, .. }} => {{
	            is_retriable_method(method) && opts.retry_on_statuses.contains(status)
	        }}
	        Error::Serde(_) | Error::Message(_) => false,
	    }}
	}}
	
	impl Default for Client {{
	    fn default() -> Self {{
	        Self::new().expect("default client")
	    }}
	}}
	
	/// Builder for [`Client`].
	#[derive(Clone)]
	pub struct ClientBuilder {{
	    base_url: String,
	    auth: Auth,
	    default_headers: HeaderMap,
	    retry: RetryOptions,
	    timeout: Option<Duration>,
	    max_concurrent: Option<usize>,
		    dedupe: bool,
		    idempotency: bool,
		    validation: bool,
		    cache: Option<ResponseCache>,
		    rate_limiter: Option<Arc<dyn RateLimiter>>,
		    telemetry: Option<Arc<dyn TelemetryHooks>>,
		    logger: Option<Arc<dyn crate::logging::Logger>>,
			    middlewares: Vec<Middleware>,
			    stream_middlewares: Vec<StreamMiddleware>,
			    /// Seeded from the first API-key security scheme in the spec, if any.
		    #[allow(dead_code)]
		    default_api_key_header: String,
		    response_transformers: Vec<Arc<dyn ResponseTransformer>>,
		    request_interceptors: Vec<Arc<dyn RequestInterceptor>>,
		    response_interceptors: Vec<Arc<dyn ResponseInterceptor>>,
		    /// Injected HTTP client. When `None`, `build()` creates a default one.
		    http_client: Option<reqwest::Client>,
		}}
	
	impl std::fmt::Debug for ClientBuilder {{
	    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {{
	        f.debug_struct("ClientBuilder")
	            .field("base_url", &self.base_url)
	            .field("auth", &self.auth)
	            .field("retry", &self.retry)
	            .field("timeout", &self.timeout)
	            .field("max_concurrent", &self.max_concurrent)
		            .field("dedupe", &self.dedupe)
			            .field("idempotency", &self.idempotency)
			            .field("validation", &self.validation)
			            .field("cache", &self.cache.is_some())
			            .field("middlewares", &self.middlewares.len())
			            .finish()
			    }}
			}}

			impl ClientBuilder {{
		    pub fn new() -> Self {{
	        Self {{
	            base_url: {base_lit}.to_string(),
	            auth: Auth::None,
	            default_headers: HeaderMap::new(),
	            retry: RetryOptions::default(),
	            timeout: Some(Duration::from_secs(30)),
	            max_concurrent: None,
		    dedupe: true,
		    idempotency: true,
		    validation: false,
	            cache: None,
	            rate_limiter: None,
	            telemetry: None,
	            logger: None,
	                    response_transformers: Vec::new(),
			    request_interceptors: Vec::new(),
			    response_interceptors: Vec::new(),
		            middlewares: Vec::new(),
		            stream_middlewares: Vec::new(),
			            default_api_key_header: {api_key_header_lit}.to_string(),
		            http_client: None,
		        }}
	    }}
	
	    pub fn base_url(mut self, url: impl Into<String>) -> Self {{
	        self.base_url = url.into().trim_end_matches('/').to_string();
	        self
	    }}
	
	    pub fn bearer_token(mut self, token: impl Into<String>) -> Self {{
	        self.auth = Auth::Bearer(token.into());
	        self
	    }}
	
	    pub fn auth(mut self, auth: Auth) -> Self {{
	        self.auth = auth;
	        self
	    }}
	
	    pub fn api_key(mut self, header: impl Into<String>, key: impl Into<String>) -> Self {{
	        self.auth = Auth::ApiKey {{
	            header: header.into(),
	            key: key.into(),
	        }};
	        self
	    }}
	
	    pub fn retry(mut self, retry: RetryOptions) -> Self {{
	        self.retry = retry;
	        self
	    }}
	
	    pub fn timeout(mut self, timeout: Duration) -> Self {{
	        self.timeout = Some(timeout);
	        self
	    }}
	
	    pub fn no_timeout(mut self) -> Self {{
	        self.timeout = None;
	        self
	    }}
	
	    pub fn max_concurrent(mut self, n: usize) -> Self {{
	        self.max_concurrent = Some(n);
	        self
	    }}
	
	    pub fn dedupe(mut self, on: bool) -> Self {{
	        self.dedupe = on;
	        self
	    }}
	
		    pub fn idempotency(mut self, on: bool) -> Self {{
		        self.idempotency = on;
		        self
		    }}

		    pub fn validation(mut self, on: bool) -> Self {{
		        self.validation = on;
		        self
		    }}

		    /// Enable GET response caching with ETag/conditional requests.
		    pub fn cache_ttl(mut self, ttl: Duration) -> Self {{
		        self.cache = Some(ResponseCache::new(ttl));
		        self
		    }}

		    /// Set a rate limiter applied before each request.
		    pub fn rate_limiter(mut self, limiter: Arc<dyn RateLimiter>) -> Self {{
		        self.rate_limiter = Some(limiter);
		        self
		    }}

		    /// Set telemetry hooks for request lifecycle observability.
		    pub fn telemetry(mut self, hooks: impl crate::telemetry::TelemetryHooks + 'static) -> Self {{
		        self.telemetry = Some(Arc::new(hooks));
		        self
		    }}

		    /// Set a structured logger for SDK lifecycle events.
		    pub fn logger(mut self, logger: impl crate::logging::Logger + 'static) -> Self {{
		        self.logger = Some(Arc::new(logger));
		        self
		    }}
	
		    pub fn middleware(mut self, mw: Middleware) -> Self {{
		        self.middlewares.push(mw);
		        self
		    }}

		    pub fn stream_middleware(mut self, mw: StreamMiddleware) -> Self {{
		        self.stream_middlewares.push(mw);
		        self
		    }}

		    /// Add a response transformer applied after deserialization.
		    pub fn response_transformer(mut self, t: impl ResponseTransformer + 'static) -> Self {{
		        self.response_transformers.push(Arc::new(t));
		        self
		    }}

	    /// Register a request interceptor applied before each request body is serialized.
	    pub fn request_interceptor(mut self, i: impl RequestInterceptor + 'static) -> Self {{
	        self.request_interceptors.push(Arc::new(i));
	        self
	    }}

		    /// Register a response interceptor applied after each response body is deserialized.
		    pub fn response_interceptor(mut self, i: impl ResponseInterceptor + 'static) -> Self {{
		        self.response_interceptors.push(Arc::new(i));
		        self
		    }}

			    /// Inject a custom `reqwest::Client` (e.g. for testing with `wiremock`).
		    /// When set, `build()` skips creating a default client.
		    pub fn http_client(mut self, client: reqwest::Client) -> Self {{
		        self.http_client = Some(client);
		        self
		    }}

		    /// Apply a [`ServiceContainer`] to configure all DI-able services at once.
		    /// Non-None container values override the builder defaults; individual
		    /// builder methods called afterward take final precedence.
		    pub fn service_container(mut self, sc: ServiceContainer) -> Self {{
		        self.http_client = Some(sc.http_client);
		        if let Some(cache) = sc.cache {{
		            self.cache = Some(cache);
		        }}
		        if let Some(limiter) = sc.rate_limiter {{
		            self.rate_limiter = Some(limiter);
		        }}
		        if let Some(logger) = sc.logger {{
		            self.logger = Some(logger);
		        }}
		        if let Some(telemetry) = sc.telemetry {{
		            self.telemetry = Some(telemetry);
		        }}
		        self
		    }}

		    pub fn build(self) -> Result<Client> {{
	        let http = match self.http_client {{
	            Some(c) => c,
	            None => reqwest::Client::builder()
	                .user_agent("specforge-rust-sdk")
	                .build()?,
	        }};
	        Ok(Client {{
	            base_url: self.base_url,
	            http,
	            auth: self.auth,
	            default_headers: self.default_headers,
	            retry: self.retry,
	            timeout: self.timeout,
	            semaphore: self.max_concurrent.map(Semaphore::new),
		            dedupe: self.dedupe,
		            idempotency: self.idempotency,
		            validation: self.validation,
			            cache: self.cache,
			            rate_limiter: self.rate_limiter,
			    telemetry: self.telemetry,
			    logger: self.logger,
			            deduper: RequestDeduper::new(),
		            middlewares: self.middlewares,
		            stream_middlewares: self.stream_middlewares,
		            response_transformers: self.response_transformers,
			    request_interceptors: self.request_interceptors,
			    response_interceptors: self.response_interceptors,
		        }})
	    }}
	}}
	
	impl Default for ClientBuilder {{
	    fn default() -> Self {{
	        Self::new()
	    }}
	}}
	"#,
	        title = doc.title.replace('"', "\\\""),
	        base_lit = rust_string_lit(base),
	        api_key_header_lit = rust_string_lit(default_api_key_header),
	    )
	}


// ─── telemetry.rs ────────────────────────────────────────────────────────────


// ─── validate.rs ─────────────────────────────────────────────────────────────

fn emit_validate(doc: &Document) -> String {
    let mut out = String::from(
        r#"// Code generated by specforge. DO NOT EDIT.

//! Runtime validation of values against the OpenAPI schema.
//!
//! Each model gets a `validate_<model>` function that checks required fields,
//! types, enum values, and recursively validates nested objects.

use serde_json::Value;

/// A single validation error with a JSON-path-style location and message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    /// JSON-pointer-style path to the offending value (e.g. `.items[2].name`).
    pub path: String,
    /// Human-readable description of what went wrong.
    pub message: String,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path, self.message)
    }
}

impl std::error::Error for ValidationError {}

/// Validate a JSON value as an object, returning any errors found.
fn validate_object_fields(
    value: &Value,
    required: &[&str],
    path: &str,
) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    let obj = match value.as_object() {
        Some(o) => o,
        None => {
            errors.push(ValidationError {
                path: path.to_string(),
                message: format!("expected object, got {}", value_type_name(value)),
            });
            return errors;
        }
    };
    for field in required {
        if !obj.contains_key(*field) {
            errors.push(ValidationError {
                path: if path.is_empty() {
                    field.to_string()
                } else {
                    format!("{path}.{field}")
                },
                message: "missing required field".to_string(),
            });
        }
    }
    errors
}

/// Validate that a string field value is one of the allowed enum values.
fn validate_enum_field(
    value: &Value,
    field: &str,
    allowed: &[&str],
    path: &str,
) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    let obj = match value.as_object() {
        Some(o) => o,
        None => return errors,
    };
    if let Some(val) = obj.get(field) {
        if let Some(s) = val.as_str() {
            if !allowed.contains(&s) {
                let field_path = if path.is_empty() {
                    field.to_string()
                } else {
                    format!("{path}.{field}")
                };
                errors.push(ValidationError {
                    path: field_path,
                    message: format!("invalid enum value: \"{s}\""),
                });
            }
        }
    }
    errors
}

fn value_type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

"#,
    );

    // Per-model validators.
    for (_, model) in doc.schemas.iter() {
        match model {
            Model::Object(o) => {
                let fn_name = snake(&o.name);
                let model_name = pascal(&o.name);
                let required: Vec<&str> = o
                    .properties
                    .iter()
                    .filter(|p| p.required)
                    .map(|p| p.name.as_str())
                    .collect();

                out.push_str(&format!(
                    "/// Validate a value against the `{model_name}` schema.\n"
                ));
                out.push_str(&format!(
                    "pub fn validate_{fn_name}(v: &Value) -> Result<(), Vec<ValidationError>> {{\n"
                ));
                out.push_str("    let mut errors = Vec::new();\n");

                if required.is_empty() && o.properties.is_empty() {
                    // Empty object - just check it's an object.
                    out.push_str("    if !v.is_object() {\n");
                    out.push_str("        errors.push(ValidationError {\n");
                    out.push_str("            path: String::new(),\n");
                    out.push_str("            message: format!(\"expected object, got {}\", value_type_name(v)),\n");
                    out.push_str("        });\n");
                    out.push_str("    }\n");
                } else {
                    // Check required fields.
                    if !required.is_empty() {
                        let req_lit: Vec<String> = required
                            .iter()
                            .map(|r| format!("\"{}\"", escape_rust_string(r)))
                            .collect();
                        out.push_str(&format!(
                            "    errors.extend(validate_object_fields(v, &[{}], \"\"));\n",
                            req_lit.join(", ")
                        ));
                    }
                    // Check enum fields.
                    for prop in &o.properties {
                        if let Type::StringEnum { variants, .. } = &prop.ty {
                            let vals: Vec<String> = variants
                                .iter()
                                .map(|v| format!("\"{}\"", escape_rust_string(v)))
                                .collect();
                            out.push_str(&format!(
                                "    errors.extend(validate_enum_field(v, \"{}\", &[{}], \"\"));\n",
                                escape_rust_string(&prop.name),
                                vals.join(", ")
                            ));
                        }
                    }
                }

                out.push_str("    if errors.is_empty() { Ok(()) } else { Err(errors) }\n");
                out.push_str("}\n\n");
            }
            Model::Enum(e) => {
                let fn_name = snake(&e.name);
                let model_name = pascal(&e.name);
                let vals: Vec<String> = e
                    .variants
                    .iter()
                    .map(|v| format!("\"{}\"", escape_rust_string(&v.value)))
                    .collect();

                out.push_str(&format!(
                    "/// Validate a value is a valid `{model_name}` enum value.\n"
                ));
                out.push_str(&format!(
                    "pub fn validate_{fn_name}(v: &Value) -> Result<(), Vec<ValidationError>> {{\n"
                ));
                out.push_str("    if let Some(s) = v.as_str() {\n");
                out.push_str(&format!(
                    "        if [{}].contains(&s) {{ return Ok(()); }}\n",
                    vals.join(", ")
                ));
                out.push_str("        Err(vec![ValidationError {\n");
                out.push_str("            path: String::new(),\n");
                out.push_str(&format!(
                    "            message: format!(\"invalid {model_name} value: \\\"{{s}}\\\"\"),\n"
                ));
                out.push_str("        }])\n");
                out.push_str("    } else {\n");
                out.push_str("        Err(vec![ValidationError {\n");
                out.push_str("            path: String::new(),\n");
                out.push_str(&format!(
                    "            message: format!(\"expected string for {model_name}, got {{}}\", value_type_name(v)),\n"
                ));
                out.push_str("        }])\n");
                out.push_str("    }\n");
                out.push_str("}\n\n");
            }
        }
    }

    out
}

// ─── validation_middleware.rs ────────────────────────────────────────────────

fn emit_validation_middleware() -> String {
    r#"// Code generated by specforge. DO NOT EDIT.

//! Automatic request/response validation middleware that intercepts all
//! requests and validates bodies against the OpenAPI schema.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;

use crate::error::{Error, Result};
use crate::middleware::{Middleware, MiddlewareRequest, MiddlewareResponse, NextFn};
use crate::validate::ValidationError;

/// Schema descriptor for an endpoint's request and/or response body validator.
pub struct EndpointSchema {
    /// Validator function for the request body. None means no validation.
    pub request_body: Option<fn(&Value) -> std::result::Result<(), Vec<ValidationError>>>,
    /// Validator function for the response body. None means no validation.
    pub response_body: Option<fn(&Value) -> std::result::Result<(), Vec<ValidationError>>>,
}

/// Route schema map keyed by "METHOD /path/pattern".
/// Patterns use `{param}` placeholders for path segments.
pub type RouteSchemaMap = HashMap<String, EndpointSchema>;

/// Check if `actual_path` matches the `pattern` path.
/// Pattern segments wrapped in `{...}` are treated as wildcards.
fn matches_path(pattern: &str, actual_path: &str) -> bool {
    let pattern_segments: Vec<&str> = pattern.trim_matches('/').split('/').filter(|s| !s.is_empty()).collect();
    let actual_segments: Vec<&str> = actual_path.trim_matches('/').split('/').filter(|s| !s.is_empty()).collect();
    if pattern_segments.len() != actual_segments.len() {
        return false;
    }
    for (ps, as_) in pattern_segments.iter().zip(actual_segments.iter()) {
        if ps.starts_with('{') && ps.ends_with('}') {
            continue; // wildcard
        }
        if ps != as_ {
            return false;
        }
    }
    true
}

/// Create a validation middleware that intercepts all requests and validates
/// request/response bodies against the OpenAPI schema.
///
/// # Usage
///
/// ```ignore
/// use std::collections::HashMap;
/// use sdk::validation_middleware::{validation_middleware, EndpointSchema, RouteSchemaMap};
/// use sdk::validate::validate_pet;
///
/// let mut schemas: RouteSchemaMap = HashMap::new();
/// schemas.insert(
///     "POST /pets".to_string(),
///     EndpointSchema {
///         request_body: Some(validate_pet),
///         response_body: Some(validate_pet),
///     },
/// );
/// schemas.insert(
///     "GET /pets/{petId}".to_string(),
///     EndpointSchema {
///         request_body: None,
///         response_body: Some(validate_pet),
///     },
/// );
///
/// client.use_middleware(validation_middleware(schemas));
/// ```
pub fn validation_middleware(schemas: RouteSchemaMap) -> Middleware {
    let schemas = Arc::new(schemas);
    Arc::new(move |req: MiddlewareRequest, next: NextFn| {
        let schemas = Arc::clone(&schemas);
        Box::pin(async move {
            let method = req.method.as_str().to_uppercase();
            let path = req.url.split('?').next().unwrap_or("").to_string();

            // Find a matching route schema.
            let route_key = format!("{} {}", method, path);
            let endpoint_schema = schemas.get(&route_key).or_else(|| {
                schemas.iter().find_map(|(pattern, schema)| {
                    let space_idx = pattern.find(' ')?;
                    let pattern_method = pattern[..space_idx].to_uppercase();
                    let pattern_path = &pattern[space_idx + 1..];
                    if pattern_method == method && matches_path(pattern_path, &path) {
                        Some(schema)
                    } else {
                        None
                    }
                })
            });

            // Validate request body if a schema is defined.
            if let Some(schema) = endpoint_schema {
                if let Some(validator) = schema.request_body {
                    if let Some(body_bytes) = &req.body {
                        if !body_bytes.is_empty() {
                            if let Ok(body) = serde_json::from_slice::<Value>(body_bytes) {
                                if let Err(errors) = validator(&body) {
                                    let msg: Vec<String> = errors.iter().map(|e| format!("{}: {}", e.path, e.message)).collect();
                                    return Err(Error::Message(format!(
                                        "[validation] {} {} request body: {}",
                                        method, path, msg.join("; ")
                                    )));
                                }
                            }
                        }
                    }
                }
            }

            // Proceed with the request.
            let resp = next(req).await?;

            // Validate response body if a schema is defined.
            if let Some(schema) = endpoint_schema {
                if let Some(validator) = schema.response_body {
                    if resp.status >= 200 && resp.status < 300 && !resp.body.is_empty() {
                        let is_json = resp.headers
                            .get("content-type")
                            .and_then(|v| v.to_str().ok())
                            .map(|ct| ct.contains("application/json"))
                            .unwrap_or(false);
                        if is_json {
                            if let Ok(body) = serde_json::from_slice::<Value>(&resp.body) {
                                if let Err(errors) = validator(&body) {
                                    let msg: Vec<String> = errors.iter().map(|e| format!("{}: {}", e.path, e.message)).collect();
                                    return Err(Error::Message(format!(
                                        "[validation] {} {} response {}: {}",
                                        method, path, resp.status, msg.join("; ")
                                    )));
                                }
                            }
                        }
                    }
                }
            }

            Ok(resp)
        })
    })
}
"#
    .to_string()
}

// ─── webhooks.rs ────────────────────────────────────────────────────────────

fn emit_webhooks(doc: &Document) -> String {
    let mut out = String::from(
        r#"// Code generated by specforge. DO NOT EDIT.

use serde::{Deserialize, Serialize};

"#,
    );

    // Payload structs for each webhook.
    for wh in &doc.webhooks {
        let name = pascal(&format!("{}WebhookPayload", wh.name));
        if let Some(d) = &wh.description {
            out.push_str(&rust_doc(d));
        } else if let Some(s) = &wh.summary {
            out.push_str(&rust_doc(s));
        }
        out.push_str("#[derive(Debug, Clone, Serialize, Deserialize)]\n");
        if let Some(rb) = &wh.request_body {
            let rust_ty = render_type(&rb.ty);
            out.push_str(&format!("pub struct {name} {{\n"));
            out.push_str("    #[serde(flatten)]\n");
            out.push_str(&format!("    pub inner: {rust_ty},\n"));
            out.push_str("}\n\n");
        } else {
            out.push_str(&format!("pub struct {name} {{}}\n\n"));
        }
    }

    // WebhookHandler trait.
    out.push_str("/// Trait for handling webhook events.\n");
    out.push_str("pub trait WebhookHandler: Send + Sync {\n");
    for wh in &doc.webhooks {
        let payload_ty = pascal(&format!("{}WebhookPayload", wh.name));
        let method_name = snake(&format!("handle_{}", wh.name));
        out.push_str(&format!(
            "    /// Handle the `{}` webhook.\n",
            wh.name
        ));
        out.push_str(&format!(
            "    fn {method_name}(&self, payload: {payload_ty}) -> crate::error::Result<()>;\n"
        ));
    }
    out.push_str("}\n");

    out
}

// ─── models.rs ───────────────────────────────────────────────────────────────

fn emit_models(doc: &Document) -> String {
    let mut out = String::from(
        r#"// Code generated by specforge. DO NOT EDIT.

use serde::{Deserialize, Serialize};

"#,
    );
    for (_, model) in doc.schemas.iter() {
        match model {
            Model::Enum(e) => out.push_str(&emit_enum(e)),
            Model::Object(o) => out.push_str(&emit_object(o, &doc.schemas)),
        }
        out.push('\n');
    }
    out
}

fn emit_enum(e: &EnumModel) -> String {
    let name = pascal(&e.name);
    let mut out = String::new();
    if let Some(d) = &e.description {
        out.push_str(&rust_doc(d));
        if d.to_lowercase().contains("deprecated") {
            if let Some(alt) = rust_schema_deprecation_alternative(d) {
                out.push_str(&format!(
                    "#[deprecated(note = \"Use {alt} instead\")]\n"
                ));
            } else {
                out.push_str(&format!(
                    "#[deprecated(note = \"{name} is deprecated\")]\n"
                ));
            }
        }
    }
    out.push_str("#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]\n");
    // Keep original wire values via rename on each variant when needed.
    out.push_str(&format!("pub enum {name} {{\n"));
    let mut seen = BTreeSet::new();
    let mut variants_meta: Vec<(String, String)> = Vec::new();
    for v in &e.variants {
        let mut var = pascal(&v.value);
        if var.is_empty() {
            var = "Empty".into();
        }
        // Dedup variant names.
        let mut candidate = var.clone();
        let mut n = 2;
        while !seen.insert(candidate.clone()) {
            candidate = format!("{var}{n}");
            n += 1;
        }
        out.push_str(&format!(
            "    #[serde(rename = \"{}\")]\n",
            escape_rust_string(&v.value)
        ));
        out.push_str(&format!("    {candidate},\n"));
        variants_meta.push((candidate, v.value.clone()));
    }
    out.push_str("}\n\n");
    // Display so query/path encoding can format enum values as their wire form.
    out.push_str(&format!("impl std::fmt::Display for {name} {{\n"));
    out.push_str("    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {\n");
    out.push_str("        match self {\n");
    for (cand, wire) in &variants_meta {
        out.push_str(&format!(
            "            Self::{cand} => write!(f, \"{}\"),\n",
            escape_rust_string(wire)
        ));
    }
    out.push_str("        }\n");
    out.push_str("    }\n");
    out.push_str("}\n");
    out
}

fn emit_object(o: &ObjectModel, registry: &specforge_core::SchemaRegistry) -> String {
    let name = pascal(&o.name);
    let mut out = String::new();
    let is_deprecated = o.description.as_deref().is_some_and(|d| {
        d.to_lowercase().contains("deprecated")
    });
    if let Some(d) = &o.description {
        out.push_str(&rust_doc(d));
    }

    // oneOf/anyOf → externally-tagged-ish untagged enum over reference arms.
    if let Some(Type::Composition(c)) = &o.shape_type {
        if matches!(c.kind, CompositionKind::OneOf | CompositionKind::AnyOf)
            && o.properties.is_empty()
        {
            let arms: Vec<&str> = c
                .members
                .iter()
                .filter_map(|m| match m {
                    Type::Reference { name, .. } => Some(name.as_str()),
                    _ => None,
                })
                .collect();
            if !arms.is_empty() {
                if is_deprecated {
                    if let Some(alt) = o.description.as_deref().and_then(rust_schema_deprecation_alternative) {
                        out.push_str(&format!("#[deprecated(note = \"Use {alt} instead\")]\n"));
                    } else {
                        out.push_str(&format!("#[deprecated(note = \"{name} is deprecated\")]\n"));
                    }
                }
                out.push_str("#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]\n");
                if let Some(disc) = &c.discriminator {
                    // serde internally tagged.
                    out.push_str(&format!(
                        "#[serde(tag = \"{}\")]\n",
                        escape_rust_string(&disc.property_name)
                    ));
                    out.push_str(&format!("pub enum {name} {{\n"));
                    for arm in &arms {
                        let arm_ty = pascal(arm);
                        // Try to find the discriminant literal for rename.
                        let rename = discriminant_value(arm, &disc.property_name, registry, disc);
                        if let Some(lit) = rename {
                            out.push_str(&format!(
                                "    #[serde(rename = \"{}\")]\n",
                                escape_rust_string(&lit)
                            ));
                            // Internally tagged with a unit-like content binding is awkward
                            // when arms are full structs that ALSO contain the tag field.
                            // Use adjacently/untagged fallback: flatten via newtype + untagged.
                        }
                        let _ = rename;
                        out.push_str(&format!("    {arm_ty}({arm_ty}),\n"));
                    }
                    out.push_str("}\n");
                    // Internally tagged newtype variants require the inner type NOT to
                    // re-declare the tag. Our models do include the tag field, so switch
                    // to untagged which matches on structure.
                    // Rebuild as untagged for correctness with current IR.
                    out.clear();
                    if let Some(d) = &o.description {
                        out.push_str(&rust_doc(d));
                    }
                    if is_deprecated {
                        if let Some(alt) = o.description.as_deref().and_then(rust_schema_deprecation_alternative) {
                            out.push_str(&format!("#[deprecated(note = \"Use {alt} instead\")]\n"));
                        } else {
                            out.push_str(&format!("#[deprecated(note = \"{name} is deprecated\")]\n"));
                        }
                    }
                    out.push_str("#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]\n");
                    out.push_str("#[serde(untagged)]\n");
                    out.push_str(&format!("pub enum {name} {{\n"));
                    for arm in &arms {
                        let arm_ty = pascal(arm);
                        out.push_str(&format!("    {arm_ty}({arm_ty}),\n"));
                    }
                    out.push_str("}\n\n");
                    out.push_str(&emit_union_impl(&name, &arms, Some(disc), registry));
                    return out;
                } else {
                    out.push_str("#[serde(untagged)]\n");
                    out.push_str(&format!("pub enum {name} {{\n"));
                    for arm in &arms {
                        let arm_ty = pascal(arm);
                        out.push_str(&format!("    {arm_ty}({arm_ty}),\n"));
                    }
                    out.push_str("}\n\n");
                    out.push_str(&emit_union_impl(&name, &arms, None, registry));
                    return out;
                }
            }
        }
    }

    if !o.properties.is_empty() {
        if is_deprecated {
            if let Some(alt) = o.description.as_deref().and_then(rust_schema_deprecation_alternative) {
                out.push_str(&format!("#[deprecated(note = \"Use {alt} instead\")]\n"));
            } else {
                out.push_str(&format!("#[deprecated(note = \"{name} is deprecated\")]\n"));
            }
        }
        out.push_str("#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]\n");
        out.push_str(&format!("pub struct {name} {{\n"));
        // Flatten the base type if allOf has a single $ref member.
        if let Some(Type::Reference { name: base, .. }) = &o.base_type {
            let base_ty = pascal(base);
            let base_field = snake(base);
            out.push_str("    #[serde(flatten)]\n");
            out.push_str(&format!("    pub {base_field}: {base_ty},\n"));
        }
        let mut used = BTreeSet::new();
        for p in &o.properties {
            // Skip properties that come from the flattened base type.
            if let Some(Type::Reference { name: base, .. }) = &o.base_type {
                if let Some(Model::Object(base_obj)) = registry.get(base) {
                    if base_obj.properties.iter().any(|bp| bp.name == p.name) {
                        continue;
                    }
                }
            }
            // Property description doc comment.
            if let Some(desc) = &p.description {
                out.push_str(&format!("    /// {desc}\n"));
            }
            let field = unique_field_name(&snake(&p.name), &mut used);
            let mut ty = render_type(&p.ty);
            if !p.required && !ty.starts_with("Option<") {
                ty = format!("Option<{ty}>");
            }
            // Always rename when the Rust field id differs from the wire name.
            if field != p.name {
                out.push_str(&format!(
                    "    #[serde(rename = \"{}\")]\n",
                    escape_rust_string(&p.name)
                ));
            }
            if !p.required {
                out.push_str("    #[serde(default, skip_serializing_if = \"Option::is_none\")]\n");
            }
            out.push_str(&format!("    pub {field}: {ty},\n"));
        }
        out.push_str("}\n");
    } else if let Some(shape) = &o.shape_type {
        out.push_str(&format!("pub type {name} = {};\n", render_type(shape)));
    } else {
        if is_deprecated {
            if let Some(alt) = o.description.as_deref().and_then(rust_schema_deprecation_alternative) {
                out.push_str(&format!("#[deprecated(note = \"Use {alt} instead\")]\n"));
            } else {
                out.push_str(&format!("#[deprecated(note = \"{name} is deprecated\")]\n"));
            }
        }
        out.push_str("#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]\n");
        out.push_str(&format!("pub struct {name} {{}}\n"));
    }
    out
}

fn discriminant_value(
    arm_name: &str,
    prop: &str,
    registry: &specforge_core::SchemaRegistry,
    disc: &specforge_core::Discriminator,
) -> Option<String> {
    // 1. Try the arm's own property (single-variant string enum).
    if let Some(Model::Object(obj)) = registry.get(arm_name) {
        if let Some(p) = obj.properties.iter().find(|p| p.name == prop) {
            if let Type::StringEnum { variants, .. } = &p.ty {
                if variants.len() == 1 {
                    return Some(variants[0].clone());
                }
            }
        }
    }

    // 2. Fall back to explicit discriminator mapping.
    if let Some(mapping) = &disc.mapping {
        for (disc_value, schema_name) in mapping {
            if schema_name == arm_name {
                return Some(disc_value.clone());
            }
        }
    }

    None
}

/// Generate an `impl` block for a oneOf/anyOf enum with `discriminant()` and
/// `is_{arm}()` methods.
fn emit_union_impl(
    name: &str,
    arms: &[&str],
    disc: Option<&specforge_core::Discriminator>,
    registry: &specforge_core::SchemaRegistry,
) -> String {
    let snake_name = snake(name);
    let mut out = String::new();
    out.push_str(&format!("impl {name} {{\n"));

    // discriminant() — returns the discriminator value as a string.
    out.push_str(&format!(
        "    /// Return the discriminator value for this {snake_name} variant.\n"
    ));
    out.push_str("    pub fn discriminant(&self) -> &'static str {\n");
    out.push_str("        match self {\n");
    for arm in arms {
        let arm_ty = pascal(arm);
        let val = disc
            .and_then(|d| discriminant_value(arm, &d.property_name, registry, d))
            .unwrap_or_else(|| arm_ty.clone());
        out.push_str(&format!(
            "            Self::{arm_ty}(_) => \"{}\",\n",
            escape_rust_string(&val)
        ));
    }
    out.push_str("        }\n");
    out.push_str("    }\n\n");

    // is_{arm}() — type guard for each arm.
    for arm in arms {
        let arm_ty = pascal(arm);
        let is_fn = snake(arm);
        out.push_str(&format!(
            "    /// Returns `true` if this is a [`{arm_ty}`] variant.\n"
        ));
        out.push_str(&format!(
            "    pub fn is_{is_fn}(&self) -> bool {{\n"
        ));
        out.push_str(&format!("        matches!(self, Self::{arm_ty}(_))\n"));
        out.push_str("    }\n\n");

        // into_{arm}() — try to extract the inner value.
        out.push_str(&format!(
            "    /// Consume self and return the inner [`{arm_ty}`], if applicable.\n"
        ));
        out.push_str(&format!(
            "    pub fn into_{is_fn}(self) -> Option<{arm_ty}> {{\n"
        ));
        out.push_str(&format!(
            "        match self {{ Self::{arm_ty}(v) => Some(v), _ => None }}\n"
        ));
        out.push_str("    }\n\n");

        // as_{arm}() — borrow the inner value.
        out.push_str(&format!(
            "    /// Borrow the inner [`{arm_ty}`], if applicable.\n"
        ));
        out.push_str(&format!(
            "    pub fn as_{is_fn}(&self) -> Option<&{arm_ty}> {{\n"
        ));
        out.push_str(&format!(
            "        match self {{ Self::{arm_ty}(v) => Some(v), _ => None }}\n"
        ));
        out.push_str("    }\n\n");
    }

    out.push_str("}\n");
    out
}

// ─── api/<tag>.rs ────────────────────────────────────────────────────────────

fn emit_tag_file(tag: &str, ops: &[&Operation]) -> String {
    let mut out = String::from("// Code generated by specforge. DO NOT EDIT.\n\n");
    out.push_str("use crate::client::Client;\n");
    out.push_str("use crate::error::Result;\n");
    out.push_str("use crate::models::*;\n\n");

    let _ = tag;
    for op in ops {
        out.push_str(&emit_method(op));
        out.push('\n');
    }
    out
}

fn emit_method(op: &Operation) -> String {
    let fn_name = snake(&op.operation_id);
    let mut path_params = Vec::new();
    let mut query_params = Vec::new();
    for p in &op.parameters {
        match p.location {
            ParamLocation::Path => path_params.push(p),
            ParamLocation::Query => query_params.push(p),
            ParamLocation::Header => {}
        }
    }
    let success = success_body(op);
    let ret_ty = success
        .as_ref()
        .map(render_type)
        .unwrap_or_else(|| "()".into());
    let has_body = op.request_body.is_some();

    let mut args: Vec<String> = vec!["client: &Client".into()];
    for p in path_params.iter().chain(query_params.iter()) {
        let ident = snake(&p.name);
        let ty = render_type(&p.ty);
        // Pass owned strings by impl Into<String> for ergonomics on path/query strings.
        if ty == "String" {
            if p.required {
                args.push(format!("{ident}: impl Into<String>"));
            } else {
                args.push(format!("{ident}: Option<impl Into<String>>"));
            }
        } else if !p.required {
            args.push(format!("{ident}: Option<{ty}>"));
        } else {
            args.push(format!("{ident}: {ty}"));
        }
    }
    if has_body {
        let body_ty = op
            .request_body
            .as_ref()
            .map(|b| render_type(&b.ty))
            .unwrap_or_else(|| "serde_json::Value".into());
        args.push(format!("body: &{body_ty}"));
    }

    let mut body = String::new();
    if let Some(s) = &op.summary {
        body.push_str(&rust_doc(s));
    } else {
        body.push_str(&format!(
            "/// {} {}.\n",
            op.method.upper(),
            op.path
        ));
    }
    // Deprecation attribute.
    if is_operation_deprecated(op) {
        if let Some(alt) = rust_deprecation_alternative(op) {
            body.push_str(&format!(
                "#[deprecated(note = \"Use {alt} instead\")]\n"
            ));
        } else {
            body.push_str(&format!(
                "#[deprecated(note = \"Use {} instead\")]\n",
                fn_name
            ));
        }
    }
    body.push_str(&format!(
        "pub async fn {fn_name}({}) -> Result<{ret_ty}> {{\n",
        args.join(", ")
    ));

    // Bind Into<String> params.
    for p in path_params.iter().chain(query_params.iter()) {
        let ident = snake(&p.name);
        if render_type(&p.ty) == "String" {
            if p.required {
                body.push_str(&format!("    let {ident} = {ident}.into();\n"));
            } else {
                body.push_str(&format!(
                    "    let {ident} = {ident}.map(|v| v.into());\n"
                ));
            }
        }
    }

    // Path.
    let path_expr = build_rust_path(&op.path, &path_params);
    body.push_str(&format!("    let path = {path_expr};\n"));

    // Query — only mut when we actually push.
    if query_params.is_empty() {
        body.push_str("    let query: Vec<(&str, String)> = Vec::new();\n");
    } else {
        body.push_str("    let mut query: Vec<(&str, String)> = Vec::new();\n");
        for p in &query_params {
            let ident = snake(&p.name);
            let as_string = rust_coerce_string(&p.ty, &ident);
            if p.required {
                body.push_str(&format!(
                    "    query.push((\"{}\", {as_string}));\n",
                    escape_rust_string(&p.name)
                ));
            } else {
                body.push_str(&format!(
                    "    if let Some(ref v) = {ident} {{\n        query.push((\"{}\", {}));\n    }}\n",
                    escape_rust_string(&p.name),
                    rust_coerce_string_ref(&p.ty)
                ));
            }
        }
    }

    let method = match op.method {
        specforge_core::HttpMethod::Get => "GET",
        specforge_core::HttpMethod::Post => "POST",
        specforge_core::HttpMethod::Put => "PUT",
        specforge_core::HttpMethod::Patch => "PATCH",
        specforge_core::HttpMethod::Delete => "DELETE",
        specforge_core::HttpMethod::Head => "HEAD",
        specforge_core::HttpMethod::Options => "OPTIONS",
    };

    let body_arg = if has_body {
        "Some(body)"
    } else {
        "None::<&()>"
    };

    if ret_ty == "()" {
        body.push_str(&format!(
            "    let _: serde_json::Value = client.request_json(reqwest::Method::{method}, &path, &query, {body_arg}).await.or_else(|e| match e {{\n        Error::Http {{ status, .. }} if status == 204 => Ok(serde_json::Value::Null),\n        other => Err(other),\n    }})?;\n"
        ));
        // Simpler: just call and discard via unit decode helper.
        // Actually request_json::<()> is fine with empty/null body handling in client.
        body.clear();
        // rebuild carefully
        if let Some(s) = &op.summary {
            body.push_str(&rust_doc(s));
        } else {
            body.push_str(&format!(
                "/// {} {}.\n",
                op.method.upper(),
                op.path
            ));
        }
        // Deprecation attribute.
        if is_operation_deprecated(op) {
            if let Some(alt) = rust_deprecation_alternative(op) {
                body.push_str(&format!(
                    "#[deprecated(note = \"Use {alt} instead\")]\n"
                ));
            } else {
                body.push_str(&format!(
                    "#[deprecated(note = \"Use {} instead\")]\n",
                    fn_name
                ));
            }
        }
        body.push_str(&format!(
            "pub async fn {fn_name}({}) -> Result<{ret_ty}> {{\n",
            args.join(", ")
        ));
        for p in path_params.iter().chain(query_params.iter()) {
            let ident = snake(&p.name);
            if render_type(&p.ty) == "String" {
                if p.required {
                    body.push_str(&format!("    let {ident} = {ident}.into();\n"));
                } else {
                    body.push_str(&format!(
                        "    let {ident} = {ident}.map(|v| v.into());\n"
                    ));
                }
            }
        }
        body.push_str(&format!("    let path = {path_expr};\n"));
        if query_params.is_empty() {
            body.push_str("    let query: Vec<(&str, String)> = Vec::new();\n");
        } else {
            body.push_str("    let mut query: Vec<(&str, String)> = Vec::new();\n");
            for p in &query_params {
                let ident = snake(&p.name);
                if p.required {
                    let as_string = rust_coerce_string(&p.ty, &ident);
                    body.push_str(&format!(
                        "    query.push((\"{}\", {as_string}));\n",
                        escape_rust_string(&p.name)
                    ));
                } else {
                    body.push_str(&format!(
                        "    if let Some(ref v) = {ident} {{\n        query.push((\"{}\", {}));\n    }}\n",
                        escape_rust_string(&p.name),
                        rust_coerce_string_ref(&p.ty)
                    ));
                }
            }
        }
        body.push_str("    let _: () = client\n");
        body.push_str(&format!(
            "        .request_json::<(), _>(reqwest::Method::{method}, &path, &query, {body_arg})\n"
        ));
        body.push_str("        .await\n");
        body.push_str("        .or_else(|e| match &e {\n");
        body.push_str("            crate::error::Error::Http { status, .. } if *status == 204 => Ok(()),\n");
        body.push_str("            crate::error::Error::Serde(_) => Ok(()),\n");
        body.push_str("            _ => Err(e),\n");
        body.push_str("        })?;\n");
        body.push_str("    Ok(())\n");
        body.push_str("}\n");
        return body;
    }

    body.push_str(&format!(
        "    client.request_json::<{ret_ty}, _>(reqwest::Method::{method}, &path, &query, {body_arg}).await\n"
    ));
    body.push_str("}\n");
    body
}


/// Generate the full doc comment block + deprecation attribute for a method.
fn build_rust_path(path: &str, path_params: &[&specforge_core::Parameter]) -> String {
    // Use format! with placeholders; every segment is stringified first so ints
    // and enums path-encode cleanly.
    let mut fmt = String::new();
    let mut args: Vec<String> = Vec::new();
    let mut rest = path;
    loop {
        if let Some(start) = rest.find('{') {
            let (lit, after) = rest.split_at(start);
            fmt.push_str(&escape_fmt(lit));
            let Some(end) = after.find('}') else {
                fmt.push_str(&escape_fmt(after));
                break;
            };
            let name = &after[1..end];
            rest = &after[end + 1..];
            fmt.push_str("{}");
            if let Some(p) = path_params.iter().find(|p| p.name == name) {
                let ident = snake(&p.name);
                // path_seg accepts impl ToString so i32/String/enums all work.
                args.push(format!("path_seg(&{ident})"));
            } else {
                args.push(format!("\"{name}\".to_string()"));
            }
        } else {
            fmt.push_str(&escape_fmt(rest));
            break;
        }
    }
    if args.is_empty() {
        format!("\"{}\".to_string()", escape_rust_string(path))
    } else {
        format!(
            "{{ fn path_seg(v: impl std::fmt::Display) -> String {{ v.to_string().replace('/', \"%2F\") }} format!(\"{fmt}\", {}) }}",
            args.join(", ")
        )
    }
}

fn escape_fmt(s: &str) -> String {
    s.replace('{', "{{").replace('}', "}}")
}

fn rust_coerce_string(ty: &Type, ident: &str) -> String {
    match ty {
        Type::Scalar(Scalar::String)
        | Type::Scalar(Scalar::DateTime)
        | Type::Scalar(Scalar::Uuid)
        | Type::StringEnum { .. } => format!("{ident}.clone()"),
        Type::Reference { .. } => format!("{ident}.to_string()"),
        Type::Scalar(Scalar::Integer)
        | Type::Scalar(Scalar::Integer64)
        | Type::Scalar(Scalar::Float)
        | Type::Scalar(Scalar::Boolean | Scalar::Base64 | Scalar::Binary) => format!("{ident}.to_string()"),
        // Arrays/maps/any: JSON-stringify for query (OpenAPI often uses explode
        // forms; JSON is a safe compile-time default).
        Type::Array { .. } | Type::Map { .. } | Type::Any | Type::Unknown | Type::Composition(_) => {
            format!("serde_json::to_string(&{ident}).unwrap_or_default()")
        }
    }
}

fn rust_coerce_string_ref(ty: &Type) -> String {
    match ty {
        Type::Scalar(Scalar::String)
        | Type::Scalar(Scalar::DateTime)
        | Type::Scalar(Scalar::Uuid)
        | Type::StringEnum { .. } => "v.clone()".into(),
        Type::Reference { .. } => "v.to_string()".into(),
        Type::Scalar(Scalar::Integer)
        | Type::Scalar(Scalar::Integer64)
        | Type::Scalar(Scalar::Float)
        | Type::Scalar(Scalar::Boolean | Scalar::Base64 | Scalar::Binary) => "v.to_string()".into(),
        Type::Array { .. } | Type::Map { .. } | Type::Any | Type::Unknown | Type::Composition(_) => {
            "serde_json::to_string(v).unwrap_or_default()".into()
        }
    }
}

fn success_body(op: &Operation) -> Option<Type> {
    let mut twos: Vec<&specforge_core::Response> = op
        .responses
        .iter()
        .filter(|r| r.status.starts_with('2'))
        .collect();
    twos.sort_by_key(|r| r.status.clone());
    twos.first().and_then(|r| r.body.clone())
}

fn rust_doc(text: &str) -> String {
    text.lines().map(|l| format!("/// {l}\n")).collect()
}

fn escape_rust_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn rust_string_lit(s: &str) -> String {
    format!("\"{}\"", escape_rust_string(s))
}

/// Check if an operation is deprecated (summary or description mentions "deprecated").
fn is_operation_deprecated(op: &Operation) -> bool {
    if let Some(summary) = &op.summary {
        if summary.to_lowercase().contains("deprecated") {
            return true;
        }
    }
    if let Some(desc) = &op.description {
        if desc.to_lowercase().contains("deprecated") {
            return true;
        }
    }
    false
}

/// Extract a suggested alternative from the operation's deprecation text.
fn rust_deprecation_alternative(op: &Operation) -> Option<String> {
    let text = op
        .summary
        .as_deref()
        .or(op.description.as_deref())
        .unwrap_or("");
    let lower = text.to_lowercase();
    if let Some(pos) = lower.find("use ") {
        let after = &text[pos + 4..];
        if let Some(end) = after.to_lowercase().find(" instead") {
            let alt = after[..end].trim();
            if !alt.is_empty() {
                return Some(alt.to_string());
            }
        }
    }
    for pattern in &["replaced by ", "replaced with "] {
        if let Some(pos) = lower.find(pattern) {
            let after = &text[pos + pattern.len()..];
            let end = after
                .find(['.', ',', '\n', ';'])
                .unwrap_or(after.len());
            let alt = after[..end].trim();
            if !alt.is_empty() {
                return Some(alt.to_string());
            }
        }
    }
    None
}

/// Extract a suggested alternative from schema deprecation text.
fn rust_schema_deprecation_alternative(desc: &str) -> Option<String> {
    let lower = desc.to_lowercase();
    if let Some(pos) = lower.find("use ") {
        let after = &desc[pos + 4..];
        if let Some(end) = after.to_lowercase().find(" instead") {
            let alt = after[..end].trim();
            if !alt.is_empty() {
                return Some(alt.to_string());
            }
        }
    }
    for pattern in &["replaced by ", "replaced with "] {
        if let Some(pos) = lower.find(pattern) {
            let after = &desc[pos + pattern.len()..];
            let end = after
                .find(['.', ',', '\n', ';'])
                .unwrap_or(after.len());
            let alt = after[..end].trim();
            if !alt.is_empty() {
                return Some(alt.to_string());
            }
        }
    }
    None
}

fn emit_readme(doc: &Document, crate_name: &str) -> String {
    // Pick the first GET operation for examples.
    let get_op = doc
        .operations
        .iter()
        .find(|op| op.method == specforge_core::HttpMethod::Get);
    let example_fn = if let Some(op) = get_op {
        snake(&op.operation_id)
    } else {
        "get_pet".to_string()
    };

    // Build call arguments (client + path params).
    let example_args = if let Some(op) = get_op {
        let mut args = vec!["&client".to_string()];
        for p in &op.parameters {
            if p.location == ParamLocation::Path {
                args.push("\"abc\"".to_string());
            }
        }
        args.join(", ")
    } else {
        "&client, \"abc\"".to_string()
    };

    let list_fn = doc
        .operations
        .iter()
        .find(|op| {
            op.method == specforge_core::HttpMethod::Get && op.operation_id.starts_with("list")
        })
        .map(|op| snake(&op.operation_id))
        .unwrap_or_else(|| {
            get_op
                .map(|op| snake(&op.operation_id))
                .unwrap_or_else(|| "list_pets".to_string())
        });

    format!(
        r#"# {title} Rust SDK

Generated by specforge. Uses `reqwest` + `serde`.

## Use

```rust
use {crate_name}::Client;
use {crate_name}::api::{example_fn};

#[tokio::main]
async fn main() -> {crate_name}::Result<()> {{
    let client = Client::builder()
        .bearer_token("…")
        .build()?;
    let result = {example_fn}({example_args}).await?;
    println!("{{result:?}}");
    Ok(())
}}
```

## Errors

All errors are typed via `{crate_name}::Error`:

```rust
use {crate_name}::Error;
use {crate_name}::api::{list_fn};


match {list_fn}(&client).await {{
    Ok(items) => println!("{{items:?}}"),
    Err(Error::Http {{ status, body, .. }}) => eprintln!("HTTP {{status}}: {{body}}"),
    Err(Error::Timeout {{ timeout_ms }}) => eprintln!("timed out after {{timeout_ms}}ms"),
    Err(Error::Transport(e)) => eprintln!("transport error: {{e}}"),
    Err(e) => eprintln!("other error: {{e}}"),
}}
```

## Pagination

Walk cursor-based or offset-based list endpoints:

```rust
use {crate_name}::paginate::{{cursor_paginate, CursorPage}};

cursor_paginate(
    |cursor| async move {{
        // call your generated list method and map to CursorPage
        todo!()
    }},
    |items| {{
        for item in items {{
            println!("{{item:?}}");
        }}
        Ok(())
    }},
)
.await?;
```

Or use `offset_paginate` for offset/limit pagination.

## Concurrency

Bound in-flight requests with `.max_concurrent()`:

```rust
let client = Client::builder()
    .bearer_token("…")
    .max_concurrent(10)
    .build()?;
```

## Dedupe

Coalesce identical in-flight safe requests (GET/HEAD/OPTIONS) so concurrent callers share one round-trip:

```rust
let client = Client::builder()
    .bearer_token("…")
    .dedupe(true)
    .build()?;
```

## Middleware

Add request/response middleware:

```rust
use std::sync::Arc;
use {crate_name}::middleware::{{Middleware, MiddlewareRequest, NextFn}};

let mw: Middleware = Arc::new(|req: MiddlewareRequest, next: NextFn| {{
    Box::pin(async move {{
        println!("{{}} {{}}", req.method, req.url);
        let res = next(req).await?;
        println!("-> {{}}", res.status);
        Ok(res)
    }})
}});

let mut client = Client::builder()
    .bearer_token("…")
    .build()?;
client.use_middleware(mw);
```

## Streaming / SSE

Consume server-sent events with `SseStream`:

```rust
use {crate_name}::streaming::SseStream;

let res = client
    .request_stream(reqwest::Method::GET, "/events", &[], None)
    .await?;
let mut sse = SseStream::new(res.bytes_stream());
while let Some(event) = sse.next_event().await? {{
    println!("{{}}: {{}}", event.event, event.data);
}}
```

## Idempotency

Auto-attach `Idempotency-Key` headers on unsafe methods (POST/PUT/PATCH/DELETE) for safe retries:

```rust
let client = Client::builder()
    .bearer_token("…")
    .idempotency(true)
    .build()?;
```

## Interceptors

Transform the request body before it is serialized, or the response body after it
is deserialized, by implementing a trait and registering it on the builder:

```rust
use {crate_name}::{{RequestInterceptor, ResponseInterceptor}};
use serde_json::{{json, Value}};

// Add a field to every outgoing request body.
struct AddTraceId;
impl RequestInterceptor for AddTraceId {{
    fn transform(&self, mut body: Value) -> Value {{
        if let Some(obj) = body.as_object_mut() {{
            obj.insert("trace_id".into(), json!("abc-123"));
        }}
        body
    }}
}}

// Strip a field from every response body.
struct StripMeta;
impl ResponseInterceptor for StripMeta {{
    fn transform(&self, mut body: Value) -> Value {{
        if let Some(obj) = body.as_object_mut() {{
            obj.remove("_meta");
        }}
        body
    }}
}}

let client = Client::builder()
    .bearer_token("…")
    .request_interceptor(AddTraceId)
    .response_interceptor(StripMeta)
    .build()?;
```

## Validation

Validate request/response bodies against the OpenAPI schema at runtime. Toggle the
built-in validator per client, or wire the generated `validation_middleware` into the
middleware chain with per-route schemas for finer control:

```rust
// Whole-client validation (uses the generated validators automatically).
let client = Client::builder()
    .bearer_token("…")
    .validation(true)
    .build()?;

// Or attach validation as a middleware with an explicit route -> schema map.
use std::collections::HashMap;
use {crate_name}::validation_middleware::{{validation_middleware, EndpointSchema, RouteSchemaMap}};
use {crate_name}::models; // generated validators live here, e.g. validate_pet

let mut schemas: RouteSchemaMap = HashMap::new();
schemas.insert(
    "POST /pets".to_string(),
    EndpointSchema {{
        request_body: Some(models::validate_pet),
        response_body: None,
    }},
);
let mut client = Client::builder().bearer_token("…").build()?;
client.use_middleware(validation_middleware(schemas));
```

## Telemetry & dependency injection

Observe the request lifecycle by implementing `TelemetryHooks`, or group injectable
dependencies (HTTP client, cache, rate limiter, logger, telemetry) into a
`ServiceContainer` and apply them in one call:

```rust
use std::time::Duration;
use {crate_name}::{{MetricsCollector, ServiceContainer}};

let metrics = MetricsCollector::default();

let sc = ServiceContainer::new()
    .cache_ttl(Duration::from_secs(60))
    .logger({crate_name}::ConsoleLogger)
    .telemetry(metrics.clone());

let client = Client::builder()
    .bearer_token("…")
    .service_container(sc)
    .build()?;

// `metrics.get_metrics().await` now reflects every request, retry, and error.
println!("{{:?}}", metrics.get_metrics().await);
```

_Do not edit generated files directly._
"#,
        title = doc.title,
        crate_name = crate_name,
    )
}

/// Collect `specforge-version.json` — version metadata for the generated SDK.
fn collect_version_file(doc: &Document, out_dir: &Path) -> (String, PathBuf, String) {
    let content = format!(
        r#"{{"specforge_version":"{}","ir_version":"{}","spec_version":"{}","generated_at":"{}"}}"#,
        env!("CARGO_PKG_VERSION"),
        doc.ir_version,
        doc.version,
        chrono_free_timestamp(),
    );
    let path = out_dir.join("specforge-version.json");
    let rel = rel(&path, out_dir);
    (rel, path, content)
}

/// Generate an ISO 8601 timestamp without pulling in chrono.
fn chrono_free_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    let mut y = 1970i64;
    let mut remaining = days as i64;
    loop {
        let days_in_year = if is_leap(y) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        y += 1;
    }
    let leap = is_leap(y);
    let month_days: [i64; 12] = [
        31,
        if leap { 29 } else { 28 },
        31, 30, 31, 30, 31, 31, 30, 31, 30, 31,
    ];
    let mut m = 1u32;
    for &md in &month_days {
        if remaining < md {
            break;
        }
        remaining -= md;
        m += 1;
    }
    let d = remaining + 1;
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y, m, d, hours, minutes, seconds
    )
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0)
}
