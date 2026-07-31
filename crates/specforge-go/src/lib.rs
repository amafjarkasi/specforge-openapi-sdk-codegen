//! `specforge-go` — Go SDK emitter for the `specforge-core` IR.
//!
//! Emits a stdlib-only Go module (`net/http` + `encoding/json`) that typechecks
//! with `go build`. Layout mirrors the TypeScript emitter at a high level:
//!
//! ```text
//! go.mod
//! client.go          // HTTP client + options
//! models.go          // all schema types
//! api_<tag>.go       // one file per tag, methods per operation
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use rayon::prelude::*;
use specforge_core::{
    CompositionKind, Discriminator, Document, EnumModel, HttpMethod, Model, ObjectModel, Operation,
    ParamLocation, Scalar, Type,
};

/// Options controlling Go emission.
pub struct GeneratorOptions {
    /// Output directory; created if missing.
    pub out_dir: PathBuf,
    /// Go module path written into `go.mod`. Defaults to a derived slug.
    pub module_path: Option<String>,
    /// Go package name for generated sources. Defaults to `sdk`.
    pub package_name: Option<String>,
    /// Optional i18n configuration for localized error messages.
    pub i18n: Option<specforge_core::I18nConfig>,
}

/// Generate a Go SDK into `opts.out_dir`. Returns relative paths written.
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

    std::fs::create_dir_all(&opts.out_dir)?;

    let pkg = package_name(opts);
    let module = module_path(doc, opts);

    // Collect all (relative_path, absolute_path, content) triples.
    let mut files: Vec<(String, PathBuf, String)> = Vec::new();

    let go_mod = opts.out_dir.join("go.mod");
    files.push((
        rel(&go_mod, &opts.out_dir),
        go_mod,
        format!("module {module}\n\ngo 1.22\n"),
    ));

    let client = opts.out_dir.join("client.go");
    files.push((rel(&client, &opts.out_dir), client, emit_client(&pkg, doc)));

    let retry = opts.out_dir.join("retry.go");
    files.push((rel(&retry, &opts.out_dir), retry, emit_retry(&pkg)));

    let paginate = opts.out_dir.join("paginate.go");
    files.push((rel(&paginate, &opts.out_dir), paginate, emit_paginate(&pkg)));

    let concurrency = opts.out_dir.join("concurrency.go");
    files.push((
        rel(&concurrency, &opts.out_dir),
        concurrency,
        emit_concurrency(&pkg),
    ));

    let dedup = opts.out_dir.join("dedup.go");
    files.push((rel(&dedup, &opts.out_dir), dedup, emit_dedup(&pkg)));

    let middleware = opts.out_dir.join("middleware.go");
    files.push((
        rel(&middleware, &opts.out_dir),
        middleware,
        emit_middleware(&pkg),
    ));
    let interceptors = opts.out_dir.join("interceptors.go");
    files.push((
        rel(&interceptors, &opts.out_dir),
        interceptors,
        emit_interceptors(&pkg),
    ));

    let idempotency = opts.out_dir.join("idempotency.go");
    files.push((
        rel(&idempotency, &opts.out_dir),
        idempotency,
        emit_idempotency(&pkg),
    ));

    let streaming = opts.out_dir.join("streaming.go");
    files.push((
        rel(&streaming, &opts.out_dir),
        streaming,
        emit_streaming(&pkg),
    ));

    let cache = opts.out_dir.join("cache.go");
    files.push((rel(&cache, &opts.out_dir), cache, emit_cache(&pkg)));

    let ratelimit = opts.out_dir.join("ratelimit.go");
    files.push((
        rel(&ratelimit, &opts.out_dir),
        ratelimit,
        emit_ratelimit(&pkg),
    ));

    let telemetry = opts.out_dir.join("telemetry.go");
    files.push((
        rel(&telemetry, &opts.out_dir),
        telemetry,
        emit_telemetry(&pkg),
    ));

    let logging = opts.out_dir.join("logging.go");
    files.push((rel(&logging, &opts.out_dir), logging, emit_logging(&pkg)));

    let validate = opts.out_dir.join("validate.go");
    files.push((
        rel(&validate, &opts.out_dir),
        validate,
        emit_validate(&pkg, doc),
    ));

    let validation_middleware = opts.out_dir.join("validation_middleware.go");
    files.push((
        rel(&validation_middleware, &opts.out_dir),
        validation_middleware,
        emit_validation_middleware(&pkg),
    ));

    let models = opts.out_dir.join("models.go");
    files.push((rel(&models, &opts.out_dir), models, emit_models(&pkg, doc)));

    // Webhooks — handler types (only if webhooks are present).
    if !doc.webhooks.is_empty() {
        let webhooks = opts.out_dir.join("webhooks.go");
        files.push((
            rel(&webhooks, &opts.out_dir),
            webhooks,
            emit_webhooks(&pkg, doc),
        ));
    }

    // Group ops by tag.
    let mut by_tag: BTreeMap<String, Vec<&Operation>> = BTreeMap::new();
    for op in &doc.operations {
        let tag = op.tag.clone().unwrap_or_else(|| "Default".into());
        by_tag.entry(tag).or_default().push(op);
    }
    for (tag, ops) in &by_tag {
        let stem = format!("api_{}.go", snake(&pascal(tag)));
        let path = opts.out_dir.join(&stem);
        let content = emit_tag_file(&pkg, tag, ops);
        files.push((rel(&path, &opts.out_dir), path, content));
    }

    let readme = opts.out_dir.join("README.md");
    files.push((
        rel(&readme, &opts.out_dir),
        readme,
        emit_readme(doc, &module),
    ));

    // specforge-version.json — version metadata for the generated SDK.
    files.push(collect_version_file(doc, &opts.out_dir));

    // Write all files in parallel.
    let written: Vec<String> = files
        .par_iter()
        .map(|(rel, abs, content)| {
            let _ = std::fs::write(abs, content);
            rel.clone()
        })
        .collect();

    let mut written = written;
    written.sort();
    Ok(written)
}

fn package_name(opts: &GeneratorOptions) -> String {
    opts.package_name
        .clone()
        .unwrap_or_else(|| "sdk".to_string())
}

fn module_path(doc: &Document, opts: &GeneratorOptions) -> String {
    opts.module_path.clone().unwrap_or_else(|| {
        let slug = doc
            .title
            .to_ascii_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect::<String>()
            .trim_matches('-')
            .to_string();
        format!("github.com/example/{slug}-go")
    })
}

fn rel(abs: &Path, base: &Path) -> String {
    abs.strip_prefix(base)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| abs.to_string_lossy().into_owned())
        // Normalize Windows backslashes to forward slashes so the returned
        // relative paths are portable (they're the public generate() return
        // value; consumers match against forward slashes like "client.go").
        .replace('\\', "/")
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
    if out.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        out.insert(0, 'X');
    }
    out
}

fn snake(input: &str) -> String {
    let mut out = String::new();
    for (i, ch) in input.chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.extend(ch.to_lowercase());
        } else if ch.is_ascii_alphanumeric() {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    out
}

fn export_ident(input: &str) -> String {
    // Go requires exported names to start with uppercase.
    let name = pascal(input);
    // Avoid colliding with built-in SDK types emitted into the same package
    // (e.g. a spec that defines its own `ValidationError` model would otherwise
    // redeclare the emitter's validation type and break compilation).
    if is_sdk_builtin_type(&name) {
        format!("{name}Model")
    } else {
        name
    }
}

/// Whether `name` matches a type the Go SDK emitter hardcodes into the
/// generated package. User models with these names get a `Model` suffix to
/// avoid redeclaration errors.
fn is_sdk_builtin_type(name: &str) -> bool {
    matches!(
        name,
        "ValidationError"
            | "ValidationErrors"
            | "TokenBucket"
            | "TelemetryHooks"
            | "StreamMiddleware"
            | "SseIterator"
            | "SlidingWindow"
            | "ServiceContainer"
            | "ServerSentEvent"
            | "Semaphore"
            | "RouteSchemaMap"
            | "RetryOptions"
            | "ResponseTransformer"
            | "ResponseInterceptor"
            | "ResponseCache"
            | "RequestInterceptor"
            | "RequestDeduper"
            | "RateLimiter"
            | "OffsetPage"
            | "MiddlewareResponse"
            | "MiddlewareRequest"
            | "Middleware"
            | "MetricsCollector"
            | "Metrics"
            | "CursorPage"
            | "Client"
            | "Auth"
            | "Logger"
            | "WebhookHandler"
    )
}

fn field_name(input: &str) -> String {
    // Preserve sign/punctuation that pascal() would collapse, so "+1" and "-1"
    // don't both become "X1".
    let mut out = String::new();
    for ch in input.chars() {
        match ch {
            '+' => out.push_str("Plus"),
            '-' | '−' => out.push_str("Minus"),
            '.' => out.push_str("Dot"),
            '/' => out.push_str("Slash"),
            '@' => out.push_str("At"),
            '#' => out.push_str("Hash"),
            '$' => out.push_str("Dollar"),
            '%' => out.push_str("Pct"),
            '&' => out.push_str("And"),
            '*' => out.push_str("Star"),
            _ if ch.is_ascii_alphanumeric() || ch == '_' => out.push(ch),
            _ => out.push('_'),
        }
    }
    let name = pascal(&out);
    if is_go_reserved(&name.to_ascii_lowercase()) {
        format!("{name}Field")
    } else {
        name
    }
}

/// Dedup field names within a single struct (after sanitization collisions).
fn unique_field_name(base: &str, used: &mut BTreeSet<String>) -> String {
    let mut candidate = base.to_string();
    let mut n = 2;
    while !used.insert(candidate.clone()) {
        candidate = format!("{base}{n}");
        n += 1;
    }
    candidate
}

// ─── Type rendering ──────────────────────────────────────────────────────────

fn render_type(ty: &Type) -> String {
    match ty {
        Type::Scalar(s) => match s {
            Scalar::String | Scalar::DateTime | Scalar::Uuid => "string".into(),
            Scalar::Integer => "int".into(),
            Scalar::Integer64 => "int64".into(),
            Scalar::Float => "float64".into(),
            Scalar::Boolean | Scalar::Base64 | Scalar::Binary => "bool".into(),
        },
        Type::StringEnum { .. } => "string".into(),
        Type::Array { item, .. } => format!("[]{}", render_type(item)),
        Type::Map { value } => format!("map[string]{}", render_type(value)),
        Type::Reference { name, nullable, .. } => {
            let n = export_ident(name);
            if *nullable {
                format!("*{n}")
            } else {
                n
            }
        }
        Type::Composition(c) => match c.kind {
            // Go has no native union/intersection — fall back to a loose map for
            // compositions at the type-expression level. Named oneOf models get
            // a dedicated interface + concrete structs instead (see emit_models).
            CompositionKind::OneOf | CompositionKind::AnyOf | CompositionKind::AllOf => {
                "map[string]any".into()
            }
        },
        Type::Any | Type::Unknown => "any".into(),
    }
}

/// Whether a model should be emitted as a Go `struct` (true) or a type alias /
/// interface (false — compositions).
fn is_struct_model(o: &ObjectModel) -> bool {
    !o.properties.is_empty() || !matches!(&o.shape_type, Some(Type::Composition(_)))
}

// ─── models.go ───────────────────────────────────────────────────────────────

fn emit_models(pkg: &str, doc: &Document) -> String {
    let mut out = String::new();
    out.push_str("// Code generated by specforge. DO NOT EDIT.\n\n");
    out.push_str(&format!("package {pkg}\n\n"));

    // Check if any model needs json + fmt imports: discriminated unions use
    // json.Marshal/Unmarshal + fmt.Errorf; non-discriminated oneOf unions use
    // json.RawMessage in New{Union}FromJSON helpers.
    let needs_json_fmt = doc.schemas.iter().any(|(_, m)| {
        matches!(
            m,
            Model::Object(o) if matches!(
                &o.shape_type,
                Some(Type::Composition(c))
                    if matches!(c.kind, CompositionKind::OneOf | CompositionKind::AnyOf)
                        && o.properties.is_empty()
            )
        )
    });
    if needs_json_fmt {
        out.push_str("import (\n");
        out.push_str("\t\"encoding/json\"\n");
        out.push_str("\t\"fmt\"\n");
        out.push_str(")\n\n");
    }

    for (_, model) in doc.schemas.iter() {
        match model {
            Model::Enum(e) => out.push_str(&emit_enum(e)),
            Model::Object(o) => out.push_str(&emit_object(o, doc)),
        }
        out.push('\n');
    }
    out
}

fn emit_enum(e: &EnumModel) -> String {
    let name = export_ident(&e.name);
    let mut out = String::new();
    if let Some(d) = &e.description {
        out.push_str(&go_doc(d, ""));
        if d.to_lowercase().contains("deprecated") {
            if let Some(alt) = go_schema_deprecation_alternative(d) {
                out.push_str(&format!(
                    "// Deprecated: Use {} instead.\n",
                    go_inline(&alt)
                ));
            } else {
                out.push_str(&format!("// Deprecated: {name} is deprecated.\n"));
            }
        }
    }
    out.push_str(&format!("type {name} string\n\n"));
    out.push_str("const (\n");
    for v in &e.variants {
        let const_name = format!("{name}{}", pascal(&v.value));
        out.push_str(&format!(
            "\t{const_name} {name} = \"{}\"\n",
            escape_go_string(&v.value)
        ));
    }
    out.push_str(")\n");
    out
}

fn emit_object(o: &ObjectModel, doc: &Document) -> String {
    let name = export_ident(&o.name);
    let mut out = String::new();
    if let Some(d) = &o.description {
        out.push_str(&go_doc(d, ""));
        if d.to_lowercase().contains("deprecated") {
            if let Some(alt) = go_schema_deprecation_alternative(d) {
                out.push_str(&format!(
                    "// Deprecated: Use {} instead.\n",
                    go_inline(&alt)
                ));
            } else {
                out.push_str(&format!("// Deprecated: {name} is deprecated.\n"));
            }
        }
    }

    // oneOf/anyOf root → interface + marker method, plus discriminator helpers.
    if let Some(Type::Composition(c)) = &o.shape_type {
        if matches!(c.kind, CompositionKind::OneOf | CompositionKind::AnyOf)
            && o.properties.is_empty()
        {
            out.push_str(&format!("// {name} is a oneOf/anyOf union.\n"));
            out.push_str(&format!("type {name} interface {{\n"));
            out.push_str(&format!("\tis{name}()\n"));
            out.push_str("}\n\n");
            // Emit marker methods on each reference arm (declared here so arms
            // that are structs get the method in this file via a companion).
            for m in &c.members {
                if let Type::Reference { name: arm, .. } = m {
                    let arm_ty = export_ident(arm);
                    out.push_str(&format!("func ({arm_ty}) is{name}() {{}}\n"));
                }
            }
            out.push('\n');

            let arms: Vec<&str> = c
                .members
                .iter()
                .filter_map(|m| match m {
                    Type::Reference { name, .. } => Some(name.as_str()),
                    _ => None,
                })
                .collect();

            if let Some(disc) = &c.discriminator {
                // Emit discriminator helpers when a discriminator is present.
                if !arms.is_empty() {
                    out.push_str(&emit_union_from_map(&name, &arms, disc, doc));
                    out.push('\n');
                    out.push_str(&emit_union_discriminant(&name, &arms, disc, doc));
                    out.push('\n');
                }
            } else if !arms.is_empty() {
                // No discriminator — emit a json.RawMessage-based constructor
                // that tries each arm in order.
                out.push_str(&emit_union_from_json(&name, &arms));
                out.push('\n');
            }

            return out;
        }
    }

    if is_struct_model(o) && !o.properties.is_empty() {
        out.push_str(&format!("type {name} struct {{\n"));
        // Embed the base type if allOf has a single $ref member.
        if let Some(Type::Reference { name: base, .. }) = &o.base_type {
            out.push_str(&format!("\t{}\n", export_ident(base)));
        }
        let mut used_fields = BTreeSet::new();
        for p in &o.properties {
            // Skip properties that come from the embedded base type.
            if let Some(Type::Reference { name: base, .. }) = &o.base_type {
                if let Some(Model::Object(base_obj)) = doc.schemas.get(base) {
                    if base_obj.properties.iter().any(|bp| bp.name == p.name) {
                        continue;
                    }
                }
            }
            // Property description comment.
            if let Some(desc) = &p.description {
                out.push_str(&go_doc(desc, "\t"));
            }
            let field = unique_field_name(&field_name(&p.name), &mut used_fields);
            let ty = render_type(&p.ty);
            let omit = if p.required { "" } else { ",omitempty" };
            out.push_str(&format!(
                "\t{field} {ty} `json:\"{}{omit}\"`\n",
                escape_go_string(&p.name)
            ));
        }
        if let Some(addl) = &o.additional_properties {
            out.push_str(&format!(
                "\tAdditionalProperties map[string]{} `json:\"-\"`\n",
                render_type(addl)
            ));
        }
        out.push_str("}\n");
    } else if let Some(shape) = &o.shape_type {
        // Scalar / array alias.
        out.push_str(&format!("type {name} = {}\n", render_type(shape)));
    } else {
        out.push_str(&format!("type {name} struct {{}}\n"));
    }
    out
}

// ─── client.go ───────────────────────────────────────────────────────────────

fn emit_client(pkg: &str, doc: &Document) -> String {
    let base = doc
        .base_url
        .as_deref()
        .unwrap_or("http://localhost")
        .trim_end_matches('/');

    // Seed API-key header from the first ApiKey security scheme, if any.
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

package {pkg}

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"math/rand"
	"net/http"
	"net/url"
	"strconv"
	"strings"
	"time"
)

// AuthProvider injects credentials into an outgoing request.
// Use BearerAuth / APIKeyAuth helpers, or supply your own.
type AuthProvider interface {{
	Apply(req *http.Request) error
}}

// BearerAuth sends Authorization: Bearer <token>. getToken is called per request.
type BearerAuth struct {{
	GetToken func(ctx context.Context) (string, error)
}}

func (a BearerAuth) Apply(req *http.Request) error {{
	if a.GetToken == nil {{
		return nil
	}}
	tok, err := a.GetToken(req.Context())
	if err != nil {{
		return err
	}}
	if tok != "" {{
		req.Header.Set("Authorization", "Bearer "+tok)
	}}
	return nil
}}

// StaticBearer is a convenience AuthProvider for a fixed token.
func StaticBearer(token string) AuthProvider {{
	return BearerAuth{{GetToken: func(context.Context) (string, error) {{ return token, nil }}}}
}}

// APIKeyAuth sends the key in a named header (e.g. "X-API-Key").
type APIKeyAuth struct {{
	Header string
	GetKey func(ctx context.Context) (string, error)
}}

func (a APIKeyAuth) Apply(req *http.Request) error {{
	if a.GetKey == nil || a.Header == "" {{
		return nil
	}}
	key, err := a.GetKey(req.Context())
	if err != nil {{
		return err
	}}
	if key != "" {{
		req.Header.Set(a.Header, key)
	}}
	return nil
}}

// ResponseTransformer transforms response data before it reaches the application.
// Transformers are applied in order after JSON decoding.
type ResponseTransformer interface {{
	Transform(response any) any
}}

// ServiceContainer groups all DI-able components for easy swapping and testing.
// Create one with NewServiceContainer, configure the fields you want to override,
// then pass it to Client.WithServiceContainer to apply them all at once.
type ServiceContainer struct {{
	HTTPClient    *http.Client
	Cache         *ResponseCache
	RateLimiter   RateLimiter
	Logger        Logger
	Telemetry     TelemetryHooks
}}

// NewServiceContainer creates a ServiceContainer with sensible defaults
// (default HTTP client, console logger). Override individual fields as needed.
func NewServiceContainer() *ServiceContainer {{
	return &ServiceContainer{{
		HTTPClient: &http.Client{{Timeout: 30 * time.Second}},
		Logger:     NewConsoleLogger(),
	}}
}}

// Client is the generated HTTP client for {title}.
type Client struct {{
	BaseURL    string
	HTTPClient *http.Client
	Header     http.Header
	Auth       AuthProvider
	// Retry controls automatic retries on transient failures. Zero value = defaults.
	Retry RetryOptions
	// Timeout is the per-attempt request timeout. Zero uses HTTPClient.Timeout
	// (or 30s if that is also zero). Context deadlines still win.
	Timeout time.Duration
	// MaxConcurrent bounds in-flight requests. 0 = unlimited.
	MaxConcurrent int
	// Dedupe coalesces identical in-flight safe (GET/HEAD/OPTIONS) requests. Default true.
	Dedupe bool
	// Idempotency auto-attaches Idempotency-Key on POST/PUT/PATCH/DELETE. Default true.
	Idempotency bool
	// Validation enables runtime request/response validation against the OpenAPI schema.
	Validation bool
	// Cache enables GET response caching with ETag/conditional requests. Nil = disabled.
	Cache *ResponseCache
	// RateLimiter controls request throughput. Nil = no rate limiting.
	RateLimiter RateLimiter
	// Logger provides structured logging for SDK lifecycle events.
	Logger Logger
	// Telemetry provides hooks for request lifecycle observability.
	Telemetry TelemetryHooks
	// Middleware runs around each HTTP attempt (after auth headers are applied).
	Middleware []Middleware
	// StreamMiddleware runs before streaming requests (header-only modifications).
	StreamMiddleware []StreamMiddleware
	// ResponseTransformers are applied in order after JSON decoding of successful responses.
	ResponseTransformers []ResponseTransformer
	// RequestInterceptors are applied in order before each request body is serialized.
	RequestInterceptors []RequestInterceptor
	// ResponseInterceptors are applied in order after each response body is deserialized.
	ResponseInterceptors []ResponseInterceptor

	sem     *Semaphore
	deduper *RequestDeduper
	// Deprecated simple fields kept for back-compat; prefer Auth.
	BearerToken  string
	APIKey       string
	APIKeyHeader string
}}

// NewClient constructs a Client pointed at the spec's default base URL.
func NewClient() *Client {{
	return &Client{{
		BaseURL:      {base_lit},
		HTTPClient:   &http.Client{{Timeout: 30 * time.Second}},
		Header:       make(http.Header),
		Retry:        DefaultRetryOptions(),
		Timeout:      30 * time.Second,
		Dedupe:       true,
		Idempotency:  true,
		deduper:      NewRequestDeduper(),
		APIKeyHeader: {api_key_header_lit},
	}}
}}

// WithBaseURL overrides the default base URL.
func (c *Client) WithBaseURL(u string) *Client {{
	c.BaseURL = strings.TrimRight(u, "/")
	return c
}}

// WithBearerToken configures a static HTTP bearer token (sets Auth).
func (c *Client) WithBearerToken(token string) *Client {{
	c.BearerToken = token
	c.Auth = StaticBearer(token)
	return c
}}

// WithAuth sets a custom auth provider (called on every request).
func (c *Client) WithAuth(a AuthProvider) *Client {{
	c.Auth = a
	return c
}}

// WithAPIKey configures a static API key header.
func (c *Client) WithAPIKey(header, key string) *Client {{
	c.APIKeyHeader = header
	c.APIKey = key
	c.Auth = APIKeyAuth{{
		Header: header,
		GetKey: func(context.Context) (string, error) {{ return key, nil }},
	}}
	return c
}}

// WithRetry overrides retry policy.
func (c *Client) WithRetry(r RetryOptions) *Client {{
	c.Retry = r
	return c
}}

// WithTimeout sets the per-attempt timeout.
func (c *Client) WithTimeout(d time.Duration) *Client {{
	c.Timeout = d
	return c
}}

// WithMaxConcurrent bounds in-flight requests (0 = unlimited).
func (c *Client) WithMaxConcurrent(n int) *Client {{
	c.MaxConcurrent = n
	if n > 0 {{
		c.sem = NewSemaphore(n)
	}} else {{
		c.sem = nil
	}}
	return c
}}

// WithDedupe enables/disables in-flight GET/HEAD/OPTIONS coalescing.
func (c *Client) WithDedupe(on bool) *Client {{
	c.Dedupe = on
	return c
}}

// Use appends middleware (applied in registration order).
func (c *Client) Use(mw ...Middleware) *Client {{
	c.Middleware = append(c.Middleware, mw...)
	return c
}}

// UseStream appends stream middleware (header-only modifications before streaming).
func (c *Client) UseStream(mw ...StreamMiddleware) *Client {{
	c.StreamMiddleware = append(c.StreamMiddleware, mw...)
	return c
}}

// WithIdempotency enables/disables auto Idempotency-Key on unsafe methods.
func (c *Client) WithIdempotency(on bool) *Client {{
	c.Idempotency = on
	return c
}}

// WithValidation enables/disables runtime request/response validation against the OpenAPI schema.
func (c *Client) WithValidation(on bool) *Client {{
	c.Validation = on
	return c
}}

// WithCache enables GET response caching with ETag/conditional requests.
// Pass a TTL of 0 to use the default (60s).
func (c *Client) WithCache(ttl time.Duration) *Client {{
	if ttl <= 0 {{
		ttl = 60 * time.Second
	}}
	c.Cache = NewResponseCache(ttl)
	return c
}}

// WithRateLimiter sets a rate limiter applied before each request.
func (c *Client) WithRateLimiter(limiter RateLimiter) *Client {{
	c.RateLimiter = limiter
	return c
}}

// WithTelemetry sets telemetry hooks for request lifecycle observability.
func (c *Client) WithTelemetry(hooks TelemetryHooks) *Client {{
	c.Telemetry = hooks
	return c
}}

// WithHTTPClient replaces the underlying *http.Client.
func (c *Client) WithHTTPClient(hc *http.Client) *Client {{
	c.HTTPClient = hc
	return c
}}

// WithServiceContainer applies all non-nil fields from a ServiceContainer to the client.
// Nil fields are left as their current value, so you only need to set what you want to override.
func (c *Client) WithServiceContainer(sc *ServiceContainer) *Client {{
	if sc == nil {{
		return c
	}}
	if sc.HTTPClient != nil {{
		c.HTTPClient = sc.HTTPClient
	}}
	if sc.Cache != nil {{
		c.Cache = sc.Cache
	}}
	if sc.RateLimiter != nil {{
		c.RateLimiter = sc.RateLimiter
	}}
	if sc.Logger != nil {{
		c.Logger = sc.Logger
	}}
	if sc.Telemetry != nil {{
		c.Telemetry = sc.Telemetry
	}}
	return c
}}

// WithResponseTransformers sets response transformers applied in order after JSON decoding.
func (c *Client) WithResponseTransformers(transformers ...ResponseTransformer) *Client {{
	c.ResponseTransformers = append(c.ResponseTransformers, transformers...)
	return c
}}

// WithRequestInterceptors sets request interceptors applied in order before each request body is serialized.
func (c *Client) WithRequestInterceptors(interceptors ...RequestInterceptor) *Client {{
	c.RequestInterceptors = append(c.RequestInterceptors, interceptors...)
	return c
}}

// WithResponseInterceptors sets response interceptors applied in order after each response body is deserialized.
func (c *Client) WithResponseInterceptors(interceptors ...ResponseInterceptor) *Client {{
	c.ResponseInterceptors = append(c.ResponseInterceptors, interceptors...)
	return c
}}

// DoJSON issues an HTTP request (concurrency + dedupe + retry + middleware),
// JSON-decodes a successful response into out (when non-nil), and returns a
// non-nil error for non-2xx statuses.
func (c *Client) DoJSON(ctx context.Context, method, path string, query url.Values, body any, out any) error {{
	u := c.BaseURL + path
	if len(query) > 0 {{
		u = u + "?" + query.Encode()
	}}

	var bodyBytes []byte
	if body != nil {{
	// Apply request interceptors before serializing the body.
	for _, i := range c.RequestInterceptors {{
		body = i.Transform(body)
	}}
		var err error
		bodyBytes, err = json.Marshal(body)
		if err != nil {{
			return fmt.Errorf("marshal body: %w", err)
		}}
	}}

	// Concurrency gate (optional).
	if c.MaxConcurrent > 0 {{
		if c.sem == nil {{
			c.sem = NewSemaphore(c.MaxConcurrent)
		}}
		if err := c.sem.Acquire(ctx); err != nil {{
			return err
		}}
		defer c.sem.Release()
	}}

	// Rate limiting: wait for permission before proceeding.
	if c.RateLimiter != nil {{
		if err := c.RateLimiter.Acquire(ctx); err != nil {{
			return err
		}}
	}}

	// Logger: request start.
	if c.Logger != nil {{
		c.Logger.Infof("[request] %s %s", method, path)
	}}

	// Telemetry: request start.
	startTime := time.Now()
	if c.Telemetry != nil {{
		c.Telemetry.OnRequestStart(method, path)
	}}

	// --- ETag cache: check for GET requests ---
	isGet := strings.ToUpper(method) == "GET"
	var cachedEntry CacheEntry
	var hasCache bool
	if isGet && c.Cache != nil {{
		cachedEntry, hasCache = c.Cache.Get(u)
		if hasCache {{
			c.Header.Set("If-None-Match", cachedEntry.ETag)
			if c.Logger != nil {{
				c.Logger.Debugf("[cache] HIT %s %s", method, path)
			}}
			if c.Telemetry != nil {{
				c.Telemetry.OnCacheHit(method, path)
			}}
		}} else {{
			if c.Logger != nil {{
				c.Logger.Debugf("[cache] MISS %s %s", method, path)
			}}
			if c.Telemetry != nil {{
				c.Telemetry.OnCacheMiss(method, path)
			}}
		}}
	}}

	// Stable idempotency key for the whole retry loop (generated once).
	var idemKey string
	if c.Idempotency && IsIdempotencyCandidate(method) {{
		idemKey = NewIdempotencyKey()
	}}

	var respETag string

	run := func() ([]byte, int, error) {{
		d, s, e, err := c.doWithRetry(ctx, method, u, bodyBytes, body != nil, idemKey)
		if err == nil {{
			respETag = e
		}}
		return d, s, err
	}}

	var data []byte
	var status int
	var err error
	if c.Dedupe && isDedupeMethod(method) {{
		if c.deduper == nil {{
			c.deduper = NewRequestDeduper()
		}}
		data, status, err = c.deduper.Do(method+" "+u, run)
	}} else {{
		data, status, err = run()
	}}

	// Clean up the If-None-Match header we may have set.
	if hasCache {{
		c.Header.Del("If-None-Match")
	}}

	if err != nil {{
		if c.Telemetry != nil {{
			c.Telemetry.OnRequestError(method, path, time.Since(startTime).Milliseconds(), err)
		}}
		return err
	}}

	// Logger: response received.
	if c.Logger != nil {{
		c.Logger.Infof("[response] %s %s -> %d", method, path, status)
	}}

	// Telemetry: successful response.
	if c.Telemetry != nil {{
		c.Telemetry.OnRequestEnd(method, path, time.Since(startTime).Milliseconds(), status)
	}}

	// --- ETag cache: handle 304 Not Modified and update on 200 ---
	if isGet && c.Cache != nil {{
		if status == http.StatusNotModified && hasCache {{
			// Return cached data.
			data = cachedEntry.Data
			status = http.StatusOK
		}} else if status >= 200 && status < 300 && respETag != "" {{
			// Store response in cache for future conditional requests.
			c.Cache.Set(u, respETag, data)
		}}
	}}

	if out == nil || status == http.StatusNoContent || len(data) == 0 {{
		return nil
	}}
	// Apply response transformers to raw data before decoding.
	for _, t := range c.ResponseTransformers {{
		transformed := t.Transform(data)
		if b, ok := transformed.([]byte); ok {{
			data = b
		}}
	}}
	// Apply response interceptors before decoding.
	for _, i := range c.ResponseInterceptors {{
		transformed := i.Transform(data)
		if b, ok := transformed.([]byte); ok {{
			data = b
		}}
	}}
	if err := json.Unmarshal(data, out); err != nil {{
		return fmt.Errorf("decode response: %w", err)
	}}
	return nil
}}

// DoStream performs a single request and returns the raw *http.Response without
// draining the body — use for SSE / chunked endpoints with StreamSse / StreamLines.
// The caller must close res.Body. Retries are not applied (streaming is not replayable).
func (c *Client) DoStream(ctx context.Context, method, path string, query url.Values, body any) (*http.Response, error) {{
	u := c.BaseURL + path
	if len(query) > 0 {{
		u = u + "?" + query.Encode()
	}}
	var bodyBytes []byte
	if body != nil {{
		var err error
		bodyBytes, err = json.Marshal(body)
		if err != nil {{
			return nil, fmt.Errorf("marshal body: %w", err)
		}}
	}}
	if c.MaxConcurrent > 0 {{
		if c.sem == nil {{
			c.sem = NewSemaphore(c.MaxConcurrent)
		}}
		if err := c.sem.Acquire(ctx); err != nil {{
			return nil, err
		}}
		// Release when body is closed — wrap via helper below is complex; release
		// immediately after headers for stream start. Callers with high fan-out
		// should set MaxConcurrent carefully for streams.
		defer c.sem.Release()
	}}
	// Rate limiting: wait for permission before proceeding.
	if c.RateLimiter != nil {{
		if err := c.RateLimiter.Acquire(ctx); err != nil {{
			return nil, err
		}}
	}}
	var idemKey string
	if c.Idempotency && IsIdempotencyCandidate(method) {{
		idemKey = NewIdempotencyKey()
	}}
	return c.doOnceStream(ctx, method, u, bodyBytes, body != nil, idemKey)
}}

func (c *Client) doWithRetry(ctx context.Context, method, u string, bodyBytes []byte, hasBody bool, idemKey string) ([]byte, int, string, error) {{
	retry := c.Retry
	if retry.MaxRetries == 0 && retry.BaseDelay == 0 && len(retry.RetryOnStatuses) == 0 {{
		retry = DefaultRetryOptions()
	}}

	var lastErr error
	for attempt := 0; attempt <= retry.MaxRetries; attempt++ {{
		if attempt > 0 {{
			delay := backoffDelay(attempt-1, retry)
			timer := time.NewTimer(delay)
			select {{
			case <-ctx.Done():
				timer.Stop()
				return nil, 0, "", ctx.Err()
			case <-timer.C:
			}}
		}}

		data, status, etag, err := c.doOnce(ctx, method, u, bodyBytes, hasBody, idemKey)
		if err == nil {{
			return data, status, etag, nil
		}}
		lastErr = err
		if !isRetriable(method, err, retry) || attempt == retry.MaxRetries {{
			return nil, 0, "", err
		}}
		// Logger: retry notification.
		if c.Logger != nil {{
			c.Logger.Warnf("[retry] %s %s error=%v, attempt %d/%d", method, u, err, attempt+2, retry.MaxRetries+1)
		}}
		// Telemetry: retry notification.
		if c.Telemetry != nil {{
			c.Telemetry.OnRetry(method, u, attempt+1, err)
		}}
	}}
	return nil, 0, "", lastErr
}}

func (c *Client) buildHeaders(ctx context.Context, method, u string, hasBody bool, idemKey string) (http.Header, error) {{
	headers := make(http.Header)
	headers.Set("Accept", "application/json")
	if hasBody {{
		headers.Set("Content-Type", "application/json")
	}}
	for k, vs := range c.Header {{
		for _, v := range vs {{
			headers.Add(k, v)
		}}
	}}
	tmp, _ := http.NewRequestWithContext(ctx, method, u, nil)
	tmp.Header = headers
	if c.Auth != nil {{
		if err := c.Auth.Apply(tmp); err != nil {{
			return nil, err
		}}
	}} else {{
		if c.BearerToken != "" {{
			tmp.Header.Set("Authorization", "Bearer "+c.BearerToken)
		}}
		if c.APIKey != "" && c.APIKeyHeader != "" {{
			tmp.Header.Set(c.APIKeyHeader, c.APIKey)
		}}
	}}
	if idemKey != "" && tmp.Header.Get(IdempotencyHeader) == "" {{
		tmp.Header.Set(IdempotencyHeader, idemKey)
	}}
	return tmp.Header, nil
}}

func (c *Client) doOnce(ctx context.Context, method, u string, bodyBytes []byte, hasBody bool, idemKey string) ([]byte, int, string, error) {{
	reqCtx := ctx
	var cancel context.CancelFunc
	timeout := c.Timeout
	if timeout <= 0 && c.HTTPClient != nil {{
		timeout = c.HTTPClient.Timeout
	}}
	if timeout <= 0 {{
		timeout = 30 * time.Second
	}}
	if timeout > 0 {{
		reqCtx, cancel = context.WithTimeout(ctx, timeout)
		defer cancel()
	}}

	headers, err := c.buildHeaders(reqCtx, method, u, hasBody, idemKey)
	if err != nil {{
		return nil, 0, "", err
	}}

	mwReq := &MiddlewareRequest{{
		Method:  method,
		URL:     u,
		Headers: headers,
		Body:    bodyBytes,
	}}

	handler := ComposeMiddleware(c.Middleware, func(ctx context.Context, req *MiddlewareRequest) (*MiddlewareResponse, error) {{
		var rdr io.Reader
		if len(req.Body) > 0 {{
			rdr = bytes.NewReader(req.Body)
		}}
		httpReq, err := http.NewRequestWithContext(ctx, req.Method, req.URL, rdr)
		if err != nil {{
			return nil, err
		}}
		httpReq.Header = req.Headers.Clone()
		res, err := c.HTTPClient.Do(httpReq)
		if err != nil {{
			return nil, err
		}}
		defer res.Body.Close()
		data, err := io.ReadAll(res.Body)
		if err != nil {{
			return nil, err
		}}
		return &MiddlewareResponse{{StatusCode: res.StatusCode, Header: res.Header.Clone(), Body: data}}, nil
	}})

	mwRes, err := handler(reqCtx, mwReq)
	if err != nil {{
		return nil, 0, "", err
	}}
	etag := mwRes.Header.Get("Etag")
	if mwRes.StatusCode < 200 || mwRes.StatusCode >= 300 {{
		// Pass through 304 as-is so DoJSON can handle it.
		if mwRes.StatusCode == http.StatusNotModified {{
			return mwRes.Body, mwRes.StatusCode, etag, nil
		}}
		return nil, 0, "", &APIError{{StatusCode: mwRes.StatusCode, Body: mwRes.Body, URL: u}}
	}}
	return mwRes.Body, mwRes.StatusCode, etag, nil
}}

func (c *Client) doOnceStream(ctx context.Context, method, u string, bodyBytes []byte, hasBody bool, idemKey string) (*http.Response, error) {{
	headers, err := c.buildHeaders(ctx, method, u, hasBody, idemKey)
	if err != nil {{
		return nil, err
	}}
	// Apply stream middleware.
	for _, mw := range c.StreamMiddleware {{
		mwReq := &MiddlewareRequest{{Method: method, URL: u, Headers: headers.Clone(), Body: bodyBytes}}
		if err := ctx.Err(); err != nil {{
			return nil, err
		}}
		if err := mw(ctx, mwReq); err != nil {{
			return nil, err
		}}
		headers = mwReq.Headers
	}}
	if headers.Get("Accept") == "application/json" {{
		headers.Set("Accept", "text/event-stream, application/json")
	}}
	var rdr io.Reader
	if len(bodyBytes) > 0 {{
		rdr = bytes.NewReader(bodyBytes)
	}}
	httpReq, err := http.NewRequestWithContext(ctx, method, u, rdr)
	if err != nil {{
		return nil, err
	}}
	httpReq.Header = headers
	res, err := c.HTTPClient.Do(httpReq)
	if err != nil {{
		return nil, err
	}}
	if res.StatusCode < 200 || res.StatusCode >= 300 {{
		defer res.Body.Close()
		data, _ := io.ReadAll(res.Body)
		return nil, &APIError{{StatusCode: res.StatusCode, Body: data, URL: u}}
	}}
	return res, nil
}}

// APIError is returned for non-2xx HTTP responses.
type APIError struct {{
	StatusCode int
	Body       []byte
	URL        string
}}

func (e *APIError) Error() string {{
	return fmt.Sprintf("HTTP %d from %s: %s", e.StatusCode, e.URL, truncate(string(e.Body), 200))
}}

// Status returns the HTTP status for errors that carry one.
func (e *APIError) Status() int {{ return e.StatusCode }}

func truncate(s string, n int) string {{
	if len(s) <= n {{
		return s
	}}
	return s[:n] + "…"
}}

// pathEscape escapes a single path segment.
func pathEscape(s string) string {{
	return url.PathEscape(s)
}}

func queryValues(pairs ...string) url.Values {{
	q := url.Values{{}}
	for i := 0; i+1 < len(pairs); i += 2 {{
		if pairs[i+1] != "" {{
			q.Set(pairs[i], pairs[i+1])
		}}
	}}
	return q
}}

func fmtInt(v int) string       {{ return strconv.Itoa(v) }}
func fmtInt64(v int64) string   {{ return strconv.FormatInt(v, 10) }}
func fmtBool(v bool) string     {{ return strconv.FormatBool(v) }}
func fmtFloat(v float64) string {{ return strconv.FormatFloat(v, 'f', -1, 64) }}

// anyString stringifies query/path values that aren't plain scalars.
func anyString(v any) string {{
	switch t := v.(type) {{
	case string:
		return t
	case fmt.Stringer:
		return t.String()
	default:
		return fmt.Sprint(v)
	}}
}}

// silence unused-import guards for packages used only in some generated shapes.
var (
	_ = rand.Float64
	_ = time.Now
)
"#,
        pkg = pkg,
        title = doc.title.replace('"', "\\\""),
        base_lit = go_string_lit(base),
        api_key_header_lit = go_string_lit(default_api_key_header),
    )
}

fn emit_retry(pkg: &str) -> String {
    format!(
        r#"// Code generated by specforge. DO NOT EDIT.

package {pkg}

import (
	"math/rand"
	"net"
	"net/http"
	"strings"
	"time"
)

// RetryOptions controls automatic retries around DoJSON.
type RetryOptions struct {{
	// MaxRetries is attempts AFTER the first try. Default 2 (3 total tries).
	MaxRetries int
	// BaseDelay is the exponential base. Default 500ms.
	BaseDelay time.Duration
	// MaxDelay caps the per-attempt sleep. Default 8s.
	MaxDelay time.Duration
	// RetryOnStatuses lists HTTP statuses that should be retried.
	// Default: 408, 429, 502, 503, 504.
	RetryOnStatuses []int
}}

// DefaultRetryOptions returns the recommended defaults.
func DefaultRetryOptions() RetryOptions {{
	return RetryOptions{{
		MaxRetries:      2,
		BaseDelay:       500 * time.Millisecond,
		MaxDelay:        8 * time.Second,
		RetryOnStatuses: []int{{408, 429, 502, 503, 504}},
	}}
}}

// isRetriableMethod is true for methods that are safe to replay by default.
func isRetriableMethod(method string) bool {{
	switch strings.ToUpper(method) {{
	case http.MethodGet, http.MethodHead, http.MethodPut, http.MethodDelete, http.MethodOptions:
		return true
	default:
		return false
	}}
}}

func isRetriable(method string, err error, opts RetryOptions) bool {{
	if err == nil {{
		return false
	}}
	// Transport / timeout / temporary network errors: always retriable.
	if _, ok := err.(net.Error); ok {{
		return true
	}}
	if strings.Contains(err.Error(), "timeout") || strings.Contains(err.Error(), "connection reset") {{
		return true
	}}
	// HTTP status errors: only if method is safe and status is listed.
	if ae, ok := err.(*APIError); ok {{
		if !isRetriableMethod(method) {{
			return false
		}}
		for _, s := range opts.RetryOnStatuses {{
			if ae.StatusCode == s {{
				return true
			}}
		}}
		return false
	}}
	return false
}}

// backoffDelay returns a full-jitter sleep for the given zero-based retry attempt.
func backoffDelay(attempt int, opts RetryOptions) time.Duration {{
	base := opts.BaseDelay
	if base <= 0 {{
		base = 500 * time.Millisecond
	}}
	max := opts.MaxDelay
	if max <= 0 {{
		max = 8 * time.Second
	}}
	// ceiling = min(base * 2^attempt, max)
	ceiling := base
	for i := 0; i < attempt; i++ {{
		if ceiling >= max/2 {{
			ceiling = max
			break
		}}
		ceiling *= 2
	}}
	if ceiling > max {{
		ceiling = max
	}}
	if ceiling <= 0 {{
		return 0
	}}
	// Full jitter: random in [0, ceiling).
	return time.Duration(rand.Int63n(int64(ceiling)))
}}
"#,
        pkg = pkg,
    )
}

fn emit_paginate(pkg: &str) -> String {
    format!(
        r#"// Code generated by specforge. DO NOT EDIT.

package {pkg}

import "context"

// CursorPage is the shape expected by CursorPaginate.
type CursorPage[T any] struct {{
	Items      []T
	NextCursor *string
}}

// CursorPaginate walks a cursor-based list endpoint until NextCursor is nil/empty.
//
//	err := CursorPaginate(ctx, func(ctx context.Context, cursor *string) (CursorPage[Pet], error) {{
//	    // call a generated list method, mapping its response into CursorPage
//	    return page, nil
//	}}, func(items []Pet) error {{
//	    // handle each page
//	    return nil
//	}})
func CursorPaginate[T any](
	ctx context.Context,
	fetch func(ctx context.Context, cursor *string) (CursorPage[T], error),
	handle func(items []T) error,
) error {{
	var cursor *string
	for {{
		if err := ctx.Err(); err != nil {{
			return err
		}}
		page, err := fetch(ctx, cursor)
		if err != nil {{
			return err
		}}
		if err := handle(page.Items); err != nil {{
			return err
		}}
		if page.NextCursor == nil || *page.NextCursor == "" {{
			return nil
		}}
		cursor = page.NextCursor
	}}
}}

// OffsetPage is the shape expected by OffsetPaginate.
type OffsetPage[T any] struct {{
	Items []T
	Total *int
}}

// OffsetPaginate walks an offset/limit list endpoint until a short page is returned.
func OffsetPaginate[T any](
	ctx context.Context,
	limit int,
	fetch func(ctx context.Context, offset, limit int) (OffsetPage[T], error),
	handle func(items []T) error,
) error {{
	if limit <= 0 {{
		limit = 50
	}}
	offset := 0
	for {{
		if err := ctx.Err(); err != nil {{
			return err
		}}
		page, err := fetch(ctx, offset, limit)
		if err != nil {{
			return err
		}}
		if err := handle(page.Items); err != nil {{
			return err
		}}
		if len(page.Items) < limit {{
			return nil
		}}
		offset += len(page.Items)
	}}
}}
"#,
        pkg = pkg,
    )
}

fn emit_concurrency(pkg: &str) -> String {
    format!(
        r#"// Code generated by specforge. DO NOT EDIT.

package {pkg}

import "context"

// Semaphore bounds the number of in-flight requests.
type Semaphore struct {{
	ch chan struct{{}}
}}

// NewSemaphore creates a semaphore with the given maximum concurrency.
// panics if max <= 0.
func NewSemaphore(max int) *Semaphore {{
	if max <= 0 {{
		panic("max concurrent must be > 0")
	}}
	return &Semaphore{{ch: make(chan struct{{}}, max)}}
}}

// Acquire blocks until a slot is free or ctx is cancelled.
func (s *Semaphore) Acquire(ctx context.Context) error {{
	select {{
	case s.ch <- struct{{}}{{}}:
		return nil
	case <-ctx.Done():
		return ctx.Err()
	}}
}}

// Release frees a slot. Safe to call even if Acquire failed (no-op when empty).
func (s *Semaphore) Release() {{
	select {{
	case <-s.ch:
	default:
	}}
}}
"#,
        pkg = pkg,
    )
}

fn emit_dedup(pkg: &str) -> String {
    format!(
        r#"// Code generated by specforge. DO NOT EDIT.

package {pkg}

import (
	"strings"
	"sync"
)

// RequestDeduper coalesces identical in-flight safe requests so concurrent
// callers of the same GET share one round-trip.
type RequestDeduper struct {{
	mu   sync.Mutex
	inflight map[string]*dedupeCall
}}

type dedupeCall struct {{
	wg     sync.WaitGroup
	data   []byte
	status int
	err    error
}}

// NewRequestDeduper constructs an empty deduper.
func NewRequestDeduper() *RequestDeduper {{
	return &RequestDeduper{{inflight: make(map[string]*dedupeCall)}}
}}

func isDedupeMethod(method string) bool {{
	switch strings.ToUpper(method) {{
	case "GET", "HEAD", "OPTIONS":
		return true
	default:
		return false
	}}
}}

// Do runs fn, coalescing concurrent calls with the same key.
func (d *RequestDeduper) Do(key string, fn func() ([]byte, int, error)) ([]byte, int, error) {{
	d.mu.Lock()
	if c, ok := d.inflight[key]; ok {{
		d.mu.Unlock()
		c.wg.Wait()
		return c.data, c.status, c.err
	}}
	c := &dedupeCall{{}}
	c.wg.Add(1)
	d.inflight[key] = c
	d.mu.Unlock()

	data, status, err := fn()
	c.data, c.status, c.err = data, status, err
	c.wg.Done()

	d.mu.Lock()
	delete(d.inflight, key)
	d.mu.Unlock()
	return data, status, err
}}
"#,
        pkg = pkg,
    )
}

fn emit_middleware(pkg: &str) -> String {
    format!(
        r#"// Code generated by specforge. DO NOT EDIT.

package {pkg}

import (
	"context"
	"net/http"
)

// MiddlewareRequest is the mutable request descriptor passed through middleware.
type MiddlewareRequest struct {{
	Method  string
	URL     string
	Headers http.Header
	Body    []byte
}}

// MiddlewareResponse is the raw response after the transport round-trip.
type MiddlewareResponse struct {{
	StatusCode int
	Header     http.Header
	Body       []byte
}}

// Middleware observes or rewrites a request/response. Call next to continue.
type Middleware func(ctx context.Context, req *MiddlewareRequest, next func(context.Context, *MiddlewareRequest) (*MiddlewareResponse, error)) (*MiddlewareResponse, error)

// ComposeMiddleware chains middlewares around a terminal dispatch function.
func ComposeMiddleware(
	middlewares []Middleware,
	dispatch func(context.Context, *MiddlewareRequest) (*MiddlewareResponse, error),
	) func(context.Context, *MiddlewareRequest) (*MiddlewareResponse, error) {{
	h := dispatch
	// Apply in reverse so registration order is outer-to-inner left-to-right.
	for i := len(middlewares) - 1; i >= 0; i-- {{
		mw := middlewares[i]
		next := h
		h = func(ctx context.Context, req *MiddlewareRequest) (*MiddlewareResponse, error) {{
			return mw(ctx, req, next)
		}}
	}}
	return h
}}

// StreamMiddleware can observe or modify a request before streaming begins.
// It cannot read the response body. Return a non-nil error to abort.
type StreamMiddleware func(ctx context.Context, req *MiddlewareRequest) error
"#,
        pkg = pkg,
    )
}

fn emit_interceptors(pkg: &str) -> String {
    format!(
        r#"// Code generated by specforge. DO NOT EDIT.

package {pkg}

// RequestInterceptor transforms the request body before it is serialized and sent.
// Return the (possibly modified) body.
type RequestInterceptor interface {{
	Transform(body interface{{}}) interface{{}}
}}

// ResponseInterceptor transforms the response body after it is deserialized.
// Return the (possibly modified) body.
type ResponseInterceptor interface {{
	Transform(body interface{{}}) interface{{}}
}}
"#,
        pkg = pkg,
    )
}

fn emit_idempotency(pkg: &str) -> String {
    format!(
        r#"// Code generated by specforge. DO NOT EDIT.

package {pkg}

import (
	"crypto/rand"
	"fmt"
	"strings"
)

// IdempotencyHeader is the header name used for unsafe-method keys.
const IdempotencyHeader = "Idempotency-Key"

// IsIdempotencyCandidate reports whether method benefits from an idempotency key.
func IsIdempotencyCandidate(method string) bool {{
	switch strings.ToUpper(method) {{
	case "POST", "PUT", "PATCH", "DELETE":
		return true
	default:
		return false
	}}
}}

// NewIdempotencyKey returns a random RFC-4122 version-4 UUID string.
func NewIdempotencyKey() string {{
	var b [16]byte
	if _, err := rand.Read(b[:]); err != nil {{
		// Extremely unlikely; fall back to a timestamp-ish unique string.
		return fmt.Sprintf("key-%d", nonceFallback())
	}}
	b[6] = (b[6] & 0x0f) | 0x40
	b[8] = (b[8] & 0x3f) | 0x80
	return fmt.Sprintf("%x-%x-%x-%x-%x", b[0:4], b[4:6], b[6:8], b[8:10], b[10:16])
}}

var nonceCounter uint64

func nonceFallback() uint64 {{
	nonceCounter++
	return nonceCounter
}}
"#,
        pkg = pkg,
    )
}

fn emit_streaming(pkg: &str) -> String {
    format!(
        r#"// Code generated by specforge. DO NOT EDIT.

package {pkg}

import (
	"bufio"
	"bytes"
	"io"
	"net/http"
	"strings"
)

// ServerSentEvent is one SSE message (blank-line delimited).
type ServerSentEvent struct {{
	Event string
	Data  string
	ID    string
}}

// StreamLines yields text lines from an HTTP response body.
// The caller must close res.Body (StreamLines does not).
func StreamLines(res *http.Response) *bufio.Scanner {{
	return bufio.NewScanner(res.Body)
}}

// StreamSse parses a text/event-stream body into ServerSentEvent values.
// Call next() until false; then check Err(). Closes nothing — caller closes Body.
type SseIterator struct {{
	sc   *bufio.Scanner
	cur  ServerSentEvent
	err  error
	done bool
}}

// NewSseIterator starts an SSE iterator over res.Body.
func NewSseIterator(res *http.Response) *SseIterator {{
	sc := bufio.NewScanner(res.Body)
	// Allow large events (1 MiB).
	buf := make([]byte, 0, 64*1024)
	sc.Buffer(buf, 1024*1024)
	return &SseIterator{{sc: sc}}
}}

// Next advances to the next event. Returns false on EOF or error.
func (it *SseIterator) Next() bool {{
	if it.done {{
		return false
	}}
	event := "message"
	var data []string
	var id string
	for it.sc.Scan() {{
		line := it.sc.Text()
		if line == "" {{
			if len(data) > 0 || event != "message" {{
				it.cur = ServerSentEvent{{
					Event: event,
					Data:  strings.Join(data, "\n"),
					ID:    id,
				}}
				return true
			}}
			event = "message"
			data = nil
			id = ""
			continue
		}}
		if strings.HasPrefix(line, ":") {{
			continue // comment
		}}
		field, value, _ := strings.Cut(line, ":")
		value = strings.TrimPrefix(value, " ")
		switch field {{
		case "event":
			event = value
		case "data":
			data = append(data, value)
		case "id":
			id = value
		case "retry":
			// ignored — caller owns reconnect policy
		}}
	}}
	if err := it.sc.Err(); err != nil {{
		it.err = err
		it.done = true
		return false
	}}
	// Flush trailing event without blank line terminator.
	if len(data) > 0 || event != "message" {{
		it.cur = ServerSentEvent{{
			Event: event,
			Data:  strings.Join(data, "\n"),
			ID:    id,
		}}
		it.done = true
		return true
	}}
	it.done = true
	return false
}}

// Event returns the current event after a successful Next.
func (it *SseIterator) Event() ServerSentEvent {{ return it.cur }}

// Err returns any scanner error.
func (it *SseIterator) Err() error {{ return it.err }}

// DrainAndClose reads any remaining body and closes it.
func DrainAndClose(res *http.Response) {{
	if res == nil || res.Body == nil {{
		return
	}}
	_, _ = io.Copy(io.Discard, res.Body)
	_ = res.Body.Close()
}}

// ReadAllBody is a small helper for tests.
func ReadAllBody(r io.Reader) ([]byte, error) {{
	var buf bytes.Buffer
	_, err := buf.ReadFrom(r)
	return buf.Bytes(), err
}}
"#,
        pkg = pkg,
    )
}

// ─── cache.go ────────────────────────────────────────────────────────────────

fn emit_cache(pkg: &str) -> String {
    format!(
        r#"// Code generated by specforge. DO NOT EDIT.

package {pkg}

import (
	"sync"
	"time"
)

// CacheEntry holds a cached HTTP response: the ETag returned by the server,
// the raw response body, and the wall-clock time the entry was stored.
type CacheEntry struct {{
	ETag      string
	Data      []byte
	Timestamp time.Time
}}

// ResponseCache is a thread-safe in-memory cache for GET responses.
// Entries are keyed by full URL and expire after the configured TTL.
type ResponseCache struct {{
	mu      sync.RWMutex
	entries map[string]CacheEntry
	ttl     time.Duration
}}

// NewResponseCache creates a cache with the given time-to-live.
func NewResponseCache(ttl time.Duration) *ResponseCache {{
	return &ResponseCache{{
		entries: make(map[string]CacheEntry),
		ttl:     ttl,
	}}
}}

// Get returns the cached entry for url if it exists and has not expired.
// The second return value reports whether a valid entry was found.
func (c *ResponseCache) Get(url string) (CacheEntry, bool) {{
	c.mu.RLock()
	entry, ok := c.entries[url]
	c.mu.RUnlock()
	if !ok {{
		return CacheEntry{{}}, false
	}}
	if time.Since(entry.Timestamp) >= c.ttl {{
		// Expired — evict lazily.
		c.mu.Lock()
		delete(c.entries, url)
		c.mu.Unlock()
		return CacheEntry{{}}, false
	}}
	return entry, true
}}

// Set stores a response body and its ETag under the given URL key.
func (c *ResponseCache) Set(url string, etag string, data []byte) {{
	c.mu.Lock()
	c.entries[url] = CacheEntry{{
		ETag:      etag,
		Data:      data,
		Timestamp: time.Now(),
	}}
	c.mu.Unlock()
}}

// Clear removes all cached entries.
func (c *ResponseCache) Clear() {{
	c.mu.Lock()
	c.entries = make(map[string]CacheEntry)
	c.mu.Unlock()
}}

// Len returns the number of entries currently in the cache (including expired).
func (c *ResponseCache) Len() int {{
	c.mu.RLock()
	defer c.mu.RUnlock()
	return len(c.entries)
}}
"#,
        pkg = pkg,
    )
}

// ─── ratelimit.go ────────────────────────────────────────────────────────────

fn emit_ratelimit(pkg: &str) -> String {
    format!(
        r#"// Code generated by specforge. DO NOT EDIT.

package {pkg}

import (
	"context"
	"sync"
	"time"
)

// RateLimiter controls request throughput. The client calls Acquire before each
// request; the call blocks until the request is allowed to proceed.
type RateLimiter interface {{
	Acquire(ctx context.Context) error
}}

// TokenBucket implements a token-bucket rate limiter. Tokens refill at a constant
// rate up to a maximum. Each request consumes one token; when empty, Acquire
// blocks until a token is available.
type TokenBucket struct {{
	mu         sync.Mutex
	tokens     float64
	maxTokens  float64
	refillRate float64 // tokens per second
	lastRefill time.Time
}}

// NewTokenBucket creates a token bucket with the given burst capacity and refill rate (tokens/sec).
func NewTokenBucket(maxTokens int, refillRate float64) *TokenBucket {{
	return &TokenBucket{{
		tokens:     float64(maxTokens),
		maxTokens:  float64(maxTokens),
		refillRate: refillRate,
		lastRefill: time.Now(),
	}}
}}

// Acquire blocks until a token is available or ctx is cancelled.
func (tb *TokenBucket) Acquire(ctx context.Context) error {{
	tb.mu.Lock()
	defer tb.mu.Unlock()
	tb.refill()
	for tb.tokens < 1 {{
		// Wait for a refill.
		tb.mu.Unlock()
		select {{
		case <-ctx.Done():
			return ctx.Err()
		case <-time.After(100 * time.Millisecond):
		}}
		tb.mu.Lock()
		tb.refill()
	}}
	tb.tokens -= 1
	return nil
}}

func (tb *TokenBucket) refill() {{
	now := time.Now()
	elapsed := now.Sub(tb.lastRefill).Seconds()
	tb.tokens = minFloat(tb.maxTokens, tb.tokens+elapsed*tb.refillRate)
	tb.lastRefill = now
}}

// SlidingWindow implements a sliding-window rate limiter. Allows at most
// maxRequests within any rolling window. When the limit is reached, Acquire
// blocks until the oldest request in the window expires.
type SlidingWindow struct {{
	mu          sync.Mutex
	requests    []time.Time
	maxRequests int
	window      time.Duration
}}

// NewSlidingWindow creates a sliding window limiter with the given max requests and window duration.
func NewSlidingWindow(maxRequests int, window time.Duration) *SlidingWindow {{
	return &SlidingWindow{{
		maxRequests: maxRequests,
		window:      window,
	}}
}}

// Acquire blocks until a request slot is available or ctx is cancelled.
func (sw *SlidingWindow) Acquire(ctx context.Context) error {{
	sw.mu.Lock()
	defer sw.mu.Unlock()
	sw.evict()
	for len(sw.requests) >= sw.maxRequests {{
		// Wait until the oldest request expires.
		oldest := sw.requests[0]
		wait := sw.window - time.Since(oldest)
		sw.mu.Unlock()
		select {{
		case <-ctx.Done():
			return ctx.Err()
		case <-time.After(wait):
		}}
		sw.mu.Lock()
		sw.evict()
	}}
	sw.requests = append(sw.requests, time.Now())
	return nil
}}

func (sw *SlidingWindow) evict() {{
	cutoff := time.Now().Add(-sw.window)
	n := 0
	for _, t := range sw.requests {{
		if t.After(cutoff) {{
			break
		}}
		n++
	}}
	sw.requests = sw.requests[n:]
}}

func minFloat(a, b float64) float64 {{
	if a < b {{
		return a
	}}
	return b
}}
"#,
        pkg = pkg,
    )
}

// ─── logging.go ─────────────────────────────────────────────────────────────

fn emit_logging(pkg: &str) -> String {
    format!(
        r#"// Code generated by specforge. DO NOT EDIT.

package {pkg}

import (
	"fmt"
	"log"
	"os"
)

// Logger is the structured logging interface for SDK lifecycle events.
// Implement this to plug in your own logger; the SDK logs requests,
// responses, retries, and cache hits/misses.
type Logger interface {{
	Debugf(format string, args ...any)
	Infof(format string, args ...any)
	Warnf(format string, args ...any)
	Errorf(format string, args ...any)
}}

// ConsoleLogger delegates to the standard log package.
type ConsoleLogger struct {{
	DebugLogger *log.Logger
	InfoLogger  *log.Logger
	WarnLogger  *log.Logger
	ErrorLogger *log.Logger
}}

// NewConsoleLogger creates a Logger that writes to stderr via the standard log package.
func NewConsoleLogger() *ConsoleLogger {{
	return &ConsoleLogger{{
		DebugLogger: log.New(os.Stderr, "[DEBUG] ", log.LstdFlags),
		InfoLogger:  log.New(os.Stderr, "[INFO]  ", log.LstdFlags),
		WarnLogger:  log.New(os.Stderr, "[WARN]  ", log.LstdFlags),
		ErrorLogger: log.New(os.Stderr, "[ERROR] ", log.LstdFlags),
	}}
}}

func (l *ConsoleLogger) Debugf(format string, args ...any) {{
	l.DebugLogger.Output(2, fmt.Sprintf(format, args...))
}}

func (l *ConsoleLogger) Infof(format string, args ...any) {{
	l.InfoLogger.Output(2, fmt.Sprintf(format, args...))
}}

func (l *ConsoleLogger) Warnf(format string, args ...any) {{
	l.WarnLogger.Output(2, fmt.Sprintf(format, args...))
}}

func (l *ConsoleLogger) Errorf(format string, args ...any) {{
	l.ErrorLogger.Output(2, fmt.Sprintf(format, args...))
}}

// noopLogger discards all log messages.
type noopLogger struct{{}}

func (noopLogger) Debugf(string, ...any) {{}}
func (noopLogger) Infof(string, ...any)  {{}}
func (noopLogger) Warnf(string, ...any)  {{}}
func (noopLogger) Errorf(string, ...any) {{}}

// NoopLogger is a Logger that discards all messages.
var NoopLogger Logger = noopLogger{{}}
"#,
        pkg = pkg,
    )
}

// ─── telemetry.go ────────────────────────────────────────────────────────────

fn emit_telemetry(pkg: &str) -> String {
    format!(
        r#"// Code generated by specforge. DO NOT EDIT.

package {pkg}

import (
	"sync"
	"time"
)

// TelemetryHooks provides callbacks for observing the SDK request lifecycle.
// Implement any subset; nil hooks are never called.
type TelemetryHooks interface {{
	// OnRequestStart fires just before the request is dispatched.
	OnRequestStart(method, path string)
	// OnRequestEnd fires after a successful response (2xx) with the elapsed duration.
	OnRequestEnd(method, path string, durationMs int64, status int)
	// OnRequestError fires when a request fails (network, timeout, non-retriable HTTP error).
	OnRequestError(method, path string, durationMs int64, err error)
	// OnRetry fires before a retry attempt (after backoff sleep).
	OnRetry(method, path string, attempt int, err error)
	// OnCacheHit fires when a GET request is served from the response cache.
	OnCacheHit(method, path string)
	// OnCacheMiss fires when a GET request misses the cache and goes to the network.
	OnCacheMiss(method, path string)
}}

// MetricsCollector is a built-in TelemetryHooks implementation that tracks
// request counts, errors, durations, and retries.
type MetricsCollector struct {{
	RequestCount  int64
	ErrorCount    int64
	TotalDuration time.Duration
	RetryCount    int64
	mu            sync.Mutex
}}

// NewMetricsCollector creates an empty MetricsCollector.
func NewMetricsCollector() *MetricsCollector {{
	return &MetricsCollector{{}}
}}

func (m *MetricsCollector) OnRequestStart(method, path string) {{
	m.mu.Lock()
	m.RequestCount++
	m.mu.Unlock()
}}

func (m *MetricsCollector) OnRequestEnd(method, path string, durationMs int64, status int) {{
	m.mu.Lock()
	m.TotalDuration += time.Duration(durationMs) * time.Millisecond
	if status >= 400 {{
		m.ErrorCount++
	}}
	m.mu.Unlock()
}}

func (m *MetricsCollector) OnRequestError(method, path string, durationMs int64, err error) {{
	m.mu.Lock()
	m.ErrorCount++
	m.TotalDuration += time.Duration(durationMs) * time.Millisecond
	m.mu.Unlock()
}}

func (m *MetricsCollector) OnRetry(method, path string, attempt int, err error) {{
	m.mu.Lock()
	m.RetryCount++
	m.mu.Unlock()
}}

func (m *MetricsCollector) OnCacheHit(method, path string)  {{}}
func (m *MetricsCollector) OnCacheMiss(method, path string) {{}}

// Metrics is a snapshot of collected metrics.
type Metrics struct {{
	RequestCount  int64
	ErrorCount    int64
	TotalDuration time.Duration
	AvgDuration   time.Duration
	RetryCount    int64
}}

// GetMetrics returns a snapshot of the collected metrics.
func (m *MetricsCollector) GetMetrics() Metrics {{
	m.mu.Lock()
	defer m.mu.Unlock()
	avg := time.Duration(0)
	if m.RequestCount > 0 {{
		avg = m.TotalDuration / time.Duration(m.RequestCount)
	}}
	return Metrics{{
		RequestCount:  m.RequestCount,
		ErrorCount:    m.ErrorCount,
		TotalDuration: m.TotalDuration,
		AvgDuration:   avg,
		RetryCount:    m.RetryCount,
	}}
}}
"#,
        pkg = pkg,
    )
}

// ─── validate.go ─────────────────────────────────────────────────────────────

fn emit_validate(pkg: &str, doc: &Document) -> String {
    let mut out = String::new();
    out.push_str("// Code generated by specforge. DO NOT EDIT.\n\n");
    out.push_str(&format!("package {pkg}\n\n"));
    out.push_str("import (\n");
    out.push_str("\t\"encoding/json\"\n");
    out.push_str("\t\"fmt\"\n");
    out.push_str(")\n\n");

    // ValidationError type.
    out.push_str(
        r#"// ValidationError describes a single validation failure with a path and message.
type ValidationError struct {
	Path    string
	Message string
}

func (e *ValidationError) Error() string {
	return fmt.Sprintf("%s: %s", e.Path, e.Message)
}

// ValidationErrors is a slice of ValidationError that implements the error interface.
type ValidationErrors []ValidationError

func (e ValidationErrors) Error() string {
	if len(e) == 0 {
		return ""
	}
	msgs := make([]string, len(e))
	for i, v := range e {
		msgs[i] = v.Error()
	}
	out := ""
	for i, m := range msgs {
		if i > 0 {
			out += "; "
		}
		out += m
	}
	return out
}

"#,
    );

    // Per-model validators.
    for (_, model) in doc.schemas.iter() {
        match model {
            Model::Object(o) => {
                let name = export_ident(&o.name);
                out.push_str(&format!(
                    "// Validate{model} validates v against the {model} schema.\n",
                    model = name
                ));
                out.push_str(&format!(
                    "func Validate{model}(v any) error {{\n",
                    model = name
                ));
                out.push_str("\terrs := validateObject(v, \"\")\n");
                // Check required fields and types.
                // Only declare obj when at least one property check actually uses it
                // (required field existence check or enum value check).
                let needs_obj = o
                    .properties
                    .iter()
                    .any(|p| p.required || matches!(&p.ty, Type::StringEnum { .. }));
                if needs_obj {
                    out.push_str("\tobj, ok := v.(map[string]any)\n");
                    out.push_str("\tif !ok {\n");
                    out.push_str("\t\tif b, err := json.Marshal(v); err == nil {\n");
                    out.push_str("\t\t\tvar m map[string]any\n");
                    out.push_str("\t\t\tif err := json.Unmarshal(b, &m); err == nil {\n");
                    out.push_str("\t\t\t\tobj = m\n");
                    out.push_str("\t\t\t\tok = true\n");
                    out.push_str("\t\t\t}\n");
                    out.push_str("\t\t}\n");
                    out.push_str("\t}\n");
                    out.push_str("\tif !ok {\n");
                    out.push_str("\t\terrs = append(errs, ValidationError{Path: \"\", Message: \"expected object\"})\n");
                    out.push_str("\t} else {\n");
                    for prop in &o.properties {
                        let field_lit = go_string_lit(&prop.name);
                        if prop.required {
                            out.push_str(&format!(
                                "\t\tif _, exists := obj[{field_lit}]; !exists {{\n"
                            ));
                            out.push_str(&format!(
                                "\t\t\terrs = append(errs, ValidationError{{Path: {field_lit}, Message: \"missing required field\"}})\n"
                            ));
                            out.push_str("\t\t}\n");
                        }
                        // Type check for enums.
                        if let Type::StringEnum { variants, .. } = &prop.ty {
                            let vals: Vec<String> = variants
                                .iter()
                                .map(|v| format!("\"{}\"", escape_go_string(v)))
                                .collect();
                            out.push_str(&format!(
                                "\t\tif val, exists := obj[{field_lit}]; exists {{\n"
                            ));
                            out.push_str("\t\t\tif s, ok := val.(string); ok {\n");
                            out.push_str("\t\t\t\tvalid := false\n");
                            for allowed in &vals {
                                out.push_str(&format!(
                                    "\t\t\t\tif s == {allowed} {{ valid = true }}\n"
                                ));
                            }
                            out.push_str("\t\t\t\tif !valid {\n");
                            out.push_str(&format!(
                                "\t\t\t\t\terrs = append(errs, ValidationError{{Path: {field_lit}, Message: fmt.Sprintf(\"invalid enum value: %s\", s)}})\n"
                            ));
                            out.push_str("\t\t\t\t}\n");
                            out.push_str("\t\t\t}\n");
                            out.push_str("\t\t}\n");
                        }
                    }
                    out.push_str("\t}\n");
                }
                out.push_str("\tif len(errs) > 0 {\n");
                out.push_str("\t\treturn ValidationErrors(errs)\n");
                out.push_str("\t}\n");
                out.push_str("\treturn nil\n");
                out.push_str("}\n\n");
            }
            Model::Enum(e) => {
                let name = export_ident(&e.name);
                let vals: Vec<String> = e
                    .variants
                    .iter()
                    .map(|v| format!("\"{}\"", escape_go_string(&v.value)))
                    .collect();
                out.push_str(&format!(
                    "// Validate{model} validates v is a valid {model} enum value.\n",
                    model = name
                ));
                out.push_str(&format!(
                    "func Validate{model}(v any) error {{\n",
                    model = name
                ));
                out.push_str("\ts, ok := v.(string)\n");
                out.push_str("\tif !ok {\n");
                out.push_str(&format!(
                    "\t\treturn &ValidationError{{Message: fmt.Sprintf(\"expected string for {model}, got %T\", v)}}\n",
                    model = name
                ));
                out.push_str("\t}\n");
                for allowed in &vals {
                    out.push_str(&format!("\tif s == {allowed} {{ return nil }}\n"));
                }
                out.push_str(&format!(
                    "\treturn &ValidationError{{Message: fmt.Sprintf(\"invalid {model} value: %s\", s)}}\n",
                    model = name
                ));
                out.push_str("}\n\n");
            }
        }
    }

    // Generic validateObject helper.
    out.push_str(
        r#"// validateObject performs basic structural validation on a value expected to be an object.
func validateObject(v any, path string) []ValidationError {
	var errs []ValidationError
	if v == nil {
		errs = append(errs, ValidationError{Path: path, Message: "expected object, got nil"})
		return errs
	}
	// Try to ensure it's a map.
	switch v.(type) {
	case map[string]any:
		// ok
	default:
		// Try JSON round-trip.
		if b, err := json.Marshal(v); err == nil {
			var m map[string]any
			if err := json.Unmarshal(b, &m); err != nil {
				errs = append(errs, ValidationError{Path: path, Message: fmt.Sprintf("expected object, got %T", v)})
			}
		} else {
			errs = append(errs, ValidationError{Path: path, Message: fmt.Sprintf("expected object, got %T", v)})
		}
	}
	return errs
}

"#,
    );

    out
}

// ─── validation_middleware.go ──────────────────────────────────────────────

fn emit_validation_middleware(pkg: &str) -> String {
    format!(
        r#"// Code generated by specforge. DO NOT EDIT.

package {pkg}

import (
	"context"
	"encoding/json"
	"fmt"
	"strings"
)

// EndpointSchema describes the request and/or response schema for an endpoint.
type EndpointSchema struct {{
	// RequestBody is the schema for the request body. Nil means no validation.
	RequestBody func(v any) error
	// ResponseBody is the schema for the response body. Nil means no validation.
	ResponseBody func(v any) error
}}

// RouteSchemaMap maps "METHOD /path/pattern" to an EndpointSchema.
// Patterns use {{param}} placeholders for path segments.
type RouteSchemaMap map[string]*EndpointSchema

// matchesPath checks if actualPath matches the pattern.
// Pattern segments in {{...}} are treated as wildcards.
func matchesPath(pattern, actualPath string) bool {{
	patternSegments := strings.Split(strings.Trim(pattern, "/"), "/")
	actualSegments := strings.Split(strings.Trim(actualPath, "/"), "/")
	if len(patternSegments) != len(actualSegments) {{
		return false
	}}
	for i, ps := range patternSegments {{
		if strings.HasPrefix(ps, "{{") && strings.HasSuffix(ps, "}}") {{
			continue
		}}
		if ps != actualSegments[i] {{
			return false
		}}
	}}
	return true
}}

// ValidationMiddleware creates a middleware that validates request/response
// bodies against the OpenAPI schema. Validation errors are returned as errors.
//
// Usage:
//
//	schemas := RouteSchemaMap{{
//		"POST /pets": {{RequestBody: ValidatePet, ResponseBody: ValidatePet}},
//		"GET /pets/{{petId}}": {{ResponseBody: ValidatePet}},
//	}}
//	client.Use(ValidationMiddleware(schemas))
func ValidationMiddleware(schemas RouteSchemaMap) Middleware {{
	return func(ctx context.Context, req *MiddlewareRequest, next func(context.Context, *MiddlewareRequest) (*MiddlewareResponse, error)) (*MiddlewareResponse, error) {{
		method := strings.ToUpper(req.Method)
		// Strip query string for route matching.
		path := req.URL
		if idx := strings.Index(path, "?"); idx >= 0 {{
			path = path[:idx]
		}}

		// Find a matching route schema.
		routeKey := method + " " + path
		endpointSchema := schemas[routeKey]

		// Try pattern matching if exact match fails.
		if endpointSchema == nil {{
			for pattern, schema := range schemas {{
				spaceIdx := strings.Index(pattern, " ")
				if spaceIdx < 0 {{
					continue
				}}
				patternMethod := strings.ToUpper(pattern[:spaceIdx])
				patternPath := pattern[spaceIdx+1:]
				if patternMethod == method && matchesPath(patternPath, path) {{
					endpointSchema = schema
					break
				}}
			}}
		}}

		// Validate request body if a schema is defined.
		if endpointSchema != nil && endpointSchema.RequestBody != nil && len(req.Body) > 0 {{
			var body any
			if err := json.Unmarshal(req.Body, &body); err == nil {{
				if err := endpointSchema.RequestBody(body); err != nil {{
					return nil, fmt.Errorf("[validation] %s %s request body: %w", method, path, err)
				}}
			}}
		}}

		// Proceed with the request.
		resp, err := next(ctx, req)
		if err != nil {{
			return nil, err
		}}

		// Validate response body if a schema is defined.
		if endpointSchema != nil && endpointSchema.ResponseBody != nil && resp.StatusCode >= 200 && resp.StatusCode < 300 && len(resp.Body) > 0 {{
			contentType := resp.Header.Get("Content-Type")
			if strings.Contains(contentType, "application/json") {{
				var body any
				if err := json.Unmarshal(resp.Body, &body); err == nil {{
					if err := endpointSchema.ResponseBody(body); err != nil {{
						return nil, fmt.Errorf("[validation] %s %s response %d: %w", method, path, resp.StatusCode, err)
					}}
				}}
			}}
		}}

		return resp, nil
	}}
}}
"#,
        pkg = pkg,
    )
}

// ─── api_<tag>.go ────────────────────────────────────────────────────────────

fn emit_tag_file(pkg: &str, tag: &str, ops: &[&Operation]) -> String {
    let mut out = String::new();
    out.push_str("// Code generated by specforge. DO NOT EDIT.\n\n");
    out.push_str(&format!("package {pkg}\n\n"));
    out.push_str("import (\n");
    out.push_str("\t\"context\"\n");
    out.push_str("\t\"net/url\"\n");
    out.push_str(")\n\n");

    let _ = tag;

    for op in ops {
        out.push_str(&emit_method(op));
        out.push('\n');
    }
    out
}

fn emit_method(op: &Operation) -> String {
    let name = export_ident(&op.operation_id);
    let mut path_params = Vec::new();
    let mut query_params = Vec::new();
    let mut header_params = Vec::new();
    for p in &op.parameters {
        match p.location {
            ParamLocation::Path => path_params.push(p),
            ParamLocation::Query => query_params.push(p),
            ParamLocation::Header => header_params.push(p),
        }
    }

    let success = success_body(op);
    let ret_ty = success.as_ref().map(render_type);
    let has_body = op.request_body.is_some();

    // Signature params.
    let mut args: Vec<String> = vec!["ctx context.Context".into()];
    for p in &path_params {
        args.push(format!(
            "{} {}",
            go_param_ident(&p.name),
            render_type(&p.ty)
        ));
    }
    for p in &query_params {
        // Optional query params still take the value type; empty/zero omitted.
        args.push(format!(
            "{} {}",
            go_param_ident(&p.name),
            render_type(&p.ty)
        ));
    }
    if has_body {
        let body_ty = op
            .request_body
            .as_ref()
            .map(|b| render_type(&b.ty))
            .unwrap_or_else(|| "any".into());
        args.push(format!("body {body_ty}"));
    }

    let (ret_sig, zero_ret) = match &ret_ty {
        Some(t) if t.starts_with('[') || t.starts_with("map[") || t == "any" => {
            (format!("({t}, error)"), "nil, ".to_string())
        }
        Some(t) if t.starts_with('*') => (format!("({t}, error)"), "nil, ".into()),
        Some(t) => (format!("(*{t}, error)"), "nil, ".into()),
        None => ("error".into(), String::new()),
    };

    let mut body = String::new();
    if let Some(s) = &op.summary {
        body.push_str(&go_doc(s, ""));
    } else {
        body.push_str(&format!("// {name} — {} {}.\n", op.method.upper(), op.path));
    }
    // Operation description (multi-line).
    if let Some(d) = &op.description {
        if !body.trim().ends_with(d.trim()) {
            body.push('\n');
            body.push_str(&go_doc(d, ""));
        }
    }
    // Parameter descriptions.
    let all_params: Vec<&specforge_core::Parameter> = path_params
        .iter()
        .chain(query_params.iter())
        .chain(header_params.iter())
        .copied()
        .collect();
    if all_params.iter().any(|p| p.description.is_some()) {
        body.push('\n');
        for p in &all_params {
            let pname = go_param_ident(&p.name);
            if let Some(desc) = &p.description {
                body.push_str(&go_doc_label(desc, &format!("{pname}: ")));
            }
        }
    }
    // Request body description.
    if let Some(rb) = &op.request_body {
        if let Some(desc) = &rb.description {
            body.push_str(&go_doc_label(desc, "body: "));
        }
    }
    // Return description from success response.
    let success_desc = op
        .responses
        .iter()
        .filter(|r| r.status.starts_with('2'))
        .min_by_key(|r| r.status.clone())
        .and_then(|r| r.description.clone());
    if let Some(desc) = &success_desc {
        body.push('\n');
        body.push_str(&go_doc_label(desc, "Returns "));
    }
    // Error response descriptions.
    let error_responses: Vec<&specforge_core::Response> = op
        .responses
        .iter()
        .filter(|r| !r.status.starts_with('2') && r.status != "*")
        .collect();
    if !error_responses.is_empty() {
        for r in &error_responses {
            if let Some(desc) = &r.description {
                body.push_str(&format!(
                    "// Returns {} for {}.\n",
                    go_inline(desc),
                    r.status
                ));
            } else {
                body.push_str(&format!("// Returns an error for {}.\n", r.status));
            }
        }
    }
    // Deprecation notice.
    if is_operation_deprecated(op) {
        if let Some(alt) = go_deprecation_alternative(op) {
            body.push_str(&format!(
                "// Deprecated: Use {} instead.\n",
                go_inline(&alt)
            ));
        } else {
            body.push_str(&format!("// Deprecated: {name} is deprecated.\n"));
        }
    }
    body.push_str(&format!(
        "func (c *Client) {name}({}) {} {{\n",
        args.join(", "),
        ret_sig
    ));

    // Path substitution.
    let mut path_expr = format!("\"{}\"", escape_go_string(&op.path));
    for p in &path_params {
        let placeholder = format!("{{{}}}", p.name);
        let ident = go_param_ident(&p.name);
        let as_string = coerce_to_string(&p.ty, &ident);
        path_expr =
            format!("strings.Replace({path_expr}, \"{placeholder}\", pathEscape({as_string}), 1)");
    }
    // Ensure strings import when we substitute path params.
    if !path_params.is_empty() {
        // path uses strings.Replace — import already in file? We'll use a simpler approach:
        // build path with fmt.Sprintf-style via manual concat.
        // Rebuild with simple concat instead of strings.Replace to avoid extra import issues.
        path_expr = build_path_concat(&op.path, &path_params);
    } else {
        path_expr = format!("\"{}\"", escape_go_string(&op.path));
    }

    // Query builder uses a fixed local name that params are barred from.
    body.push_str("\tquery := url.Values{}\n");
    for p in &query_params {
        let ident = go_param_ident(&p.name);
        let as_string = coerce_to_string(&p.ty, &ident);
        // Skip zero values for optional params.
        if p.required {
            body.push_str(&format!(
                "\tquery.Set(\"{}\", {as_string})\n",
                escape_go_string(&p.name)
            ));
        } else {
            let zero_check = zero_check(&p.ty, &ident);
            body.push_str(&format!(
                "\tif {zero_check} {{\n\t\tquery.Set(\"{}\", {as_string})\n\t}}\n",
                escape_go_string(&p.name)
            ));
        }
    }

    let body_arg = if has_body { "body" } else { "nil" };
    let method = op.method.upper();

    match &ret_ty {
        Some(t) if t.starts_with('[') || t.starts_with("map[") => {
            body.push_str(&format!("\tvar out {t}\n"));
            body.push_str(&format!(
                "\tif err := c.DoJSON(ctx, \"{method}\", {path_expr}, query, {body_arg}, &out); err != nil {{\n\t\treturn {zero_ret}err\n\t}}\n"
            ));
            body.push_str("\treturn out, nil\n");
        }
        Some(t) if t.starts_with('*') => {
            let inner = t.trim_start_matches('*');
            body.push_str(&format!("\tvar out {inner}\n"));
            body.push_str(&format!(
                "\tif err := c.DoJSON(ctx, \"{method}\", {path_expr}, query, {body_arg}, &out); err != nil {{\n\t\treturn {zero_ret}err\n\t}}\n"
            ));
            body.push_str("\treturn &out, nil\n");
        }
        Some(t) => {
            body.push_str(&format!("\tvar out {t}\n"));
            body.push_str(&format!(
                "\tif err := c.DoJSON(ctx, \"{method}\", {path_expr}, query, {body_arg}, &out); err != nil {{\n\t\treturn {zero_ret}err\n\t}}\n"
            ));
            body.push_str("\treturn &out, nil\n");
        }
        None => {
            body.push_str(&format!(
                "\treturn c.DoJSON(ctx, \"{method}\", {path_expr}, query, {body_arg}, nil)\n"
            ));
        }
    }
    body.push_str("}\n");

    // headers currently unused in v1 signature — keep quiet.
    let _ = header_params;
    let _ = HttpMethod::Get;

    body
}

fn build_path_concat(path: &str, path_params: &[&specforge_core::Parameter]) -> String {
    // Split path on `{name}` placeholders and concat.
    let mut parts: Vec<String> = Vec::new();
    let mut rest = path;
    loop {
        if let Some(start) = rest.find('{') {
            let (lit, after) = rest.split_at(start);
            if !lit.is_empty() {
                parts.push(format!("\"{}\"", escape_go_string(lit)));
            }
            let Some(end) = after.find('}') else {
                parts.push(format!("\"{}\"", escape_go_string(after)));
                break;
            };
            let name = &after[1..end];
            rest = &after[end + 1..];
            if let Some(p) = path_params.iter().find(|p| p.name == name) {
                let ident = go_param_ident(&p.name);
                let as_string = coerce_to_string(&p.ty, &ident);
                parts.push(format!("pathEscape({as_string})"));
            } else {
                parts.push(format!("\"{{{}}}\"", escape_go_string(name)));
            }
        } else {
            if !rest.is_empty() {
                parts.push(format!("\"{}\"", escape_go_string(rest)));
            }
            break;
        }
    }
    if parts.is_empty() {
        "\"/\"".into()
    } else {
        parts.join(" + ")
    }
}

fn go_param_ident(name: &str) -> String {
    let mut s = String::new();
    let mut next_upper = false;
    for (i, ch) in name.chars().enumerate() {
        if ch.is_ascii_alphanumeric() {
            if i == 0 {
                s.extend(ch.to_lowercase());
            } else if next_upper {
                s.extend(ch.to_uppercase());
                next_upper = false;
            } else {
                s.push(ch);
            }
        } else {
            next_upper = true;
        }
    }
    if s.is_empty() {
        "p".into()
    } else if is_go_reserved(&s) {
        format!("{s}Val")
    } else {
        s
    }
}

fn is_go_reserved(s: &str) -> bool {
    matches!(
        s,
        "break"
            | "default"
            | "func"
            | "interface"
            | "select"
            | "case"
            | "defer"
            | "go"
            | "map"
            | "struct"
            | "chan"
            | "else"
            | "goto"
            | "package"
            | "switch"
            | "const"
            | "fallthrough"
            | "if"
            | "range"
            | "type"
            | "continue"
            | "for"
            | "import"
            | "return"
            | "var"
            | "error"
            | "string"
            | "bool"
            | "int"
            | "any"
            | "true"
            | "false"
            | "nil"
            | "iota"
            | "append"
            | "cap"
            | "close"
            | "complex"
            | "copy"
            | "delete"
            | "imag"
            | "len"
            | "make"
            | "new"
            | "panic"
            | "print"
            | "println"
            | "real"
            | "recover"
            // Locals / imports used by generated method bodies.
            | "context"
            | "body"
            | "ctx"
            | "c"
            | "q"
            | "query"
            | "out"
            | "err"
            | "url"
            | "http"
            | "json"
            | "fmt"
            | "io"
            | "strings"
            | "strconv"
            | "bytes"
            | "time"
            | "pathEscape"
            | "anyString"
            | "fmtInt"
            | "fmtInt64"
            | "fmtBool"
            | "fmtFloat"
    )
}

fn coerce_to_string(ty: &Type, ident: &str) -> String {
    match ty {
        Type::Scalar(Scalar::String)
        | Type::Scalar(Scalar::DateTime)
        | Type::Scalar(Scalar::Uuid)
        | Type::StringEnum { .. } => ident.to_string(),
        Type::Reference { .. } => format!("anyString({ident})"),
        Type::Scalar(Scalar::Integer) => format!("fmtInt({ident})"),
        Type::Scalar(Scalar::Integer64) => format!("fmtInt64({ident})"),
        Type::Scalar(Scalar::Boolean | Scalar::Base64 | Scalar::Binary) => {
            format!("fmtBool({ident})")
        }
        Type::Scalar(Scalar::Float) => format!("fmtFloat({ident})"),
        // arrays/maps/any — stringify via helper (lives in client.go, no fmt import needed here)
        _ => format!("anyString({ident})"),
    }
}

fn zero_check(ty: &Type, ident: &str) -> String {
    match ty {
        Type::Scalar(Scalar::String)
        | Type::Scalar(Scalar::DateTime)
        | Type::Scalar(Scalar::Uuid)
        | Type::StringEnum { .. } => format!("{ident} != \"\""),
        Type::Scalar(Scalar::Integer)
        | Type::Scalar(Scalar::Integer64)
        | Type::Scalar(Scalar::Float) => {
            format!("{ident} != 0")
        }
        Type::Scalar(Scalar::Boolean | Scalar::Base64 | Scalar::Binary) => ident.to_string(), // only send when true
        Type::Reference { .. } => "true /* always send ref */".to_string(),
        Type::Array { .. } | Type::Map { .. } => format!("len({ident}) > 0"),
        _ => "true".into(),
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

// ─── Discriminator helpers ──────────────────────────────────────────────────

/// Look up the discriminant value for an arm, checking the arm's own property
/// first (single-variant string enum), then falling back to the mapping.
fn go_discriminant_value(
    arm_name: &str,
    prop: &str,
    doc: &Document,
    disc: &Discriminator,
) -> Option<String> {
    // 1. Arm's own property.
    if let Some(Model::Object(obj)) = doc.schemas.get(arm_name) {
        if let Some(p) = obj.properties.iter().find(|p| p.name == prop) {
            if let Type::StringEnum { variants, .. } = &p.ty {
                if variants.len() == 1 {
                    return Some(variants[0].clone());
                }
            }
        }
    }
    // 2. Explicit mapping.
    if let Some(mapping) = &disc.mapping {
        for (val, schema) in mapping {
            if schema == arm_name {
                return Some(val.clone());
            }
        }
    }
    None
}

/// Emit `func New{Union}(m map[string]any) ({Union}, error)` that inspects the
/// discriminator field and returns the concrete arm.
fn emit_union_from_map(
    union_name: &str,
    arms: &[&str],
    disc: &Discriminator,
    doc: &Document,
) -> String {
    let disc_prop = &disc.property_name;
    let mut out = String::new();
    out.push_str(&format!(
        "// New{union_name} constructs a {union_name} from a map (e.g. after JSON unmarshal).\n"
    ));
    out.push_str(&format!(
        "func New{union_name}(m map[string]any) ({union_name}, error) {{\n"
    ));
    out.push_str(&format!(
        "\tdisc, _ := m[{}].(string)\n",
        go_string_lit(disc_prop)
    ));
    out.push_str("\tswitch disc {\n");
    for arm in arms {
        let arm_ty = export_ident(arm);
        if let Some(val) = go_discriminant_value(arm, disc_prop, doc, disc) {
            out.push_str(&format!("\tcase {}:\n", go_string_lit(&val)));
            out.push_str(&format!("\t\tvar v {arm_ty}\n"));
            out.push_str("\t\tif b, err := json.Marshal(m); err == nil {\n");
            out.push_str("\t\t\tif err := json.Unmarshal(b, &v); err == nil {\n");
            out.push_str("\t\t\t\treturn v, nil\n");
            out.push_str("\t\t\t}\n");
            out.push_str("\t\t}\n");
            out.push_str(&format!(
                "\t\treturn v, fmt.Errorf(\"failed to unmarshal {arm_ty}\")\n"
            ));
        }
    }
    out.push_str("\t}\n");
    out.push_str(&format!(
        "\treturn nil, fmt.Errorf(\"unknown {union_name} discriminator: %v\", disc)\n"
    ));
    out.push_str("}\n");
    out
}

/// Emit `func {Union}Discriminant(v {Union}) string` that returns the
/// discriminator value for a concrete arm.
fn emit_union_discriminant(
    union_name: &str,
    arms: &[&str],
    disc: &Discriminator,
    doc: &Document,
) -> String {
    let disc_prop = &disc.property_name;
    let mut out = String::new();
    out.push_str(&format!(
        "// {union_name}Discriminant returns the discriminator value for a {union_name} arm.\n"
    ));
    out.push_str(&format!(
        "func {union_name}Discriminant(v {union_name}) string {{\n"
    ));
    out.push_str("\tswitch v.(type) {\n");
    for arm in arms {
        let arm_ty = export_ident(arm);
        if let Some(val) = go_discriminant_value(arm, disc_prop, doc, disc) {
            out.push_str(&format!("\tcase {arm_ty}:\n"));
            out.push_str(&format!("\t\treturn {}\n", go_string_lit(&val)));
        }
    }
    out.push_str("\t}\n");
    out.push_str("\treturn \"\"\n");
    out.push_str("}\n");
    out
}

/// Emit `func New{Union}FromJSON(raw json.RawMessage) ({Union}, error)` that
/// tries to unmarshal raw JSON into each arm in order, returning the first match.
fn emit_union_from_json(union_name: &str, arms: &[&str]) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "// New{union_name}FromJSON attempts to unmarshal raw JSON into the first matching arm.\n"
    ));
    out.push_str(&format!(
        "func New{union_name}FromJSON(raw json.RawMessage) ({union_name}, error) {{\n"
    ));
    for arm in arms {
        let arm_ty = export_ident(arm);
        let arm_var = go_param_ident(arm);
        out.push_str(&format!("\tvar {arm_var} {arm_ty}\n"));
        out.push_str(&format!(
            "\tif err := json.Unmarshal(raw, &{arm_var}); err == nil {{\n"
        ));
        out.push_str(&format!("\t\treturn {arm_var}, nil\n"));
        out.push_str("\t}\n");
    }
    out.push_str(&format!(
        "\treturn nil, fmt.Errorf(\"no matching {union_name} arm\")\n"
    ));
    out.push_str("}\n");
    out
}

// ─── webhooks.go ─────────────────────────────────────────────────────────────

fn emit_webhooks(pkg: &str, doc: &Document) -> String {
    let mut out = String::new();
    out.push_str("// Code generated by specforge. DO NOT EDIT.\n\n");
    out.push_str(&format!("package {pkg}\n\n"));

    // Payload structs for each webhook.
    for wh in &doc.webhooks {
        let name = export_ident(&format!("{}WebhookPayload", pascal(&wh.name)));
        if let Some(d) = &wh.description {
            out.push_str(&go_doc(d, ""));
        } else if let Some(s) = &wh.summary {
            out.push_str(&go_doc(s, ""));
        }
        if let Some(rb) = &wh.request_body {
            let go_ty = render_type(&rb.ty);
            out.push_str(&format!("type {name} = {go_ty}\n\n"));
        } else {
            out.push_str(&format!("type {name} = any\n\n"));
        }
    }

    // WebhookHandler function type.
    out.push_str("// WebhookHandler is a function that handles a webhook payload.\n");
    if doc.webhooks.len() == 1 {
        let wh = &doc.webhooks[0];
        let payload_ty = export_ident(&format!("{}WebhookPayload", pascal(&wh.name)));
        out.push_str(&format!(
            "type WebhookHandler func(payload {payload_ty}) error\n"
        ));
    } else {
        // Multiple webhooks: use a generic any-based handler.
        out.push_str("type WebhookHandler func(payload any) error\n");
    }
    out.push('\n');

    out
}

// ─── helpers ─────────────────────────────────────────────────────────────────

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
fn go_deprecation_alternative(op: &Operation) -> Option<String> {
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
            let end = after.find(['.', ',', '\n', ';']).unwrap_or(after.len());
            let alt = after[..end].trim();
            if !alt.is_empty() {
                return Some(alt.to_string());
            }
        }
    }
    None
}

/// Extract a suggested alternative from schema deprecation text.
fn go_schema_deprecation_alternative(desc: &str) -> Option<String> {
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
            let end = after.find(['.', ',', '\n', ';']).unwrap_or(after.len());
            let alt = after[..end].trim();
            if !alt.is_empty() {
                return Some(alt.to_string());
            }
        }
    }
    None
}

fn go_doc(text: &str, pad: &str) -> String {
    // Go `//` line comments are safe from `/* */` block-comment tokenizing, but
    // a stray `*/` can still close an enclosing block comment in some tooling,
    // and we want one `//` per line so multi-paragraph descriptions don't leak
    // bare prose into the source (which breaks parsing). Escape `*/` and prefix
    // every line.
    text.lines()
        .map(|l| {
            let escaped = l.replace("*/", "*&#47;");
            format!("{pad}// {escaped}\n")
        })
        .collect()
}

/// Like [`go_doc`], but prefixes the first line with `label` (e.g. `name: `)
/// and `// ` on continuation lines. Used for per-field doc paragraphs.
fn go_doc_label(text: &str, label: &str) -> String {
    let mut lines = text.lines();
    let mut out = String::new();
    if let Some(first) = lines.next() {
        out.push_str(&format!("// {label}{}\n", first.replace("*/", "*&#47;")));
    }
    for l in lines {
        out.push_str(&format!("// {}\n", l.replace("*/", "*&#47;")));
    }
    out
}

/// Sanitize a spec-derived string for safe inlining into a single Go `//`
/// comment line: escape `*/` (closes block comments) and collapse embedded
/// newlines to spaces so the comment stays on one line.
fn go_inline(text: &str) -> String {
    text.replace("*/", "*&#47;").replace(['\n', '\r'], " ")
}

fn escape_go_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn go_string_lit(s: &str) -> String {
    format!("\"{}\"", escape_go_string(s))
}

fn emit_readme(doc: &Document, module: &str) -> String {
    // Pick the first GET operation for examples.
    let get_op = doc
        .operations
        .iter()
        .find(|op| op.method == HttpMethod::Get);
    let example_method = if let Some(op) = get_op {
        export_ident(&op.operation_id)
    } else {
        "GetPet".to_string()
    };

    // Build call arguments for the example (ctx + path params).
    let example_args = if let Some(op) = get_op {
        let mut args = vec!["ctx".to_string()];
        for p in &op.parameters {
            if p.location == ParamLocation::Path {
                args.push("\"abc\"".to_string());
            }
        }
        args.join(", ")
    } else {
        "ctx, \"abc\"".to_string()
    };

    let list_method = doc
        .operations
        .iter()
        .find(|op| op.method == HttpMethod::Get && op.operation_id.starts_with("list"))
        .map(|op| export_ident(&op.operation_id))
        .unwrap_or_else(|| {
            get_op
                .map(|op| export_ident(&op.operation_id))
                .unwrap_or_else(|| "ListPets".to_string())
        });

    format!(
        r#"# {title} Go SDK

Generated by [specforge](https://github.com/example/specforge). Stdlib only (`net/http`, `encoding/json`).

## Install

```bash
go get {module}
```

## Quick start

```go
package main

import (
    "context"
    "fmt"
    "log"
    sdk "{module}"
)

func main() {{
    c := sdk.NewClient().WithBearerToken("…")
    result, err := c.{example_method}({example_args})
    if err != nil {{
        log.Fatal(err)
    }}
    fmt.Println(result)
}}
```

## Errors

Non-2xx responses return an `*sdk.APIError`:

```go
result, err := c.{list_method}(ctx)
if err != nil {{
    var apiErr *sdk.APIError
    if errors.As(err, &apiErr) {{
        fmt.Println("status:", apiErr.StatusCode)
        fmt.Println("body:", string(apiErr.Body))
    }}
}}
```

## Pagination

Walk cursor-based or offset-based list endpoints:

```go
err := sdk.CursorPaginate(ctx,
    func(ctx context.Context, cursor *string) (sdk.CursorPage[sdk.Pet], error) {{
        // call your generated list method and map to CursorPage
        return page, nil
    }},
    func(items []sdk.Pet) error {{
        for _, pet := range items {{
            fmt.Println(pet.Name)
        }}
        return nil
    }},
)
```

Or use `sdk.OffsetPaginate` for offset/limit pagination.

## Concurrency

Bound in-flight requests with `WithMaxConcurrent`:

```go
c := sdk.NewClient().
    WithBearerToken("…").
    WithMaxConcurrent(10)
```

## Dedupe

Coalesce identical in-flight safe requests (GET/HEAD/OPTIONS) so concurrent callers share one round-trip:

```go
c := sdk.NewClient().
    WithBearerToken("…").
    WithDedupe(true)
```

## Middleware

Add request/response middleware:

```go
c := sdk.NewClient().WithBearerToken("…")
c.Use(func(ctx context.Context, req *sdk.MiddlewareRequest,
    next func(context.Context, *sdk.MiddlewareRequest) (*sdk.MiddlewareResponse, error),
) (*sdk.MiddlewareResponse, error) {{
    log.Printf("%s %s", req.Method, req.URL)
    res, err := next(ctx, req)
    if err == nil {{
        log.Printf("-> %d", res.StatusCode)
    }}
    return res, err
}})
```

## Streaming / SSE

Consume server-sent events:

```go
res, err := c.DoStream(ctx, "GET", "/events", nil, nil)
if err != nil {{
    log.Fatal(err)
}}
defer sdk.DrainAndClose(res)

it := sdk.NewSseIterator(res)
for it.Next() {{
    ev := it.Event()
    fmt.Println(ev.Event, ev.Data)
}}
if err := it.Err(); err != nil {{
    log.Fatal(err)
}}
```

## Idempotency

Auto-attach `Idempotency-Key` headers on unsafe methods (POST/PUT/PATCH/DELETE) for safe retries:

```go
c := sdk.NewClient().
    WithBearerToken("…").
    WithIdempotency(true)
```

_Do not edit generated files directly._
"#,
        title = doc.title,
        module = module,
        example_method = example_method,
        example_args = example_args,
        list_method = list_method,
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
    let rel_str = rel(&path, out_dir);
    (rel_str, path, content)
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
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
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

#[cfg(test)]
mod tests {
    use super::*;

    // ── pascal ─────────────────────────────────────────────────────────

    #[test]
    fn pascal_casifies() {
        assert_eq!(pascal("pet"), "Pet");
        assert_eq!(pascal("pet_store"), "PetStore");
        assert_eq!(pascal("pet-store"), "PetStore");
    }

    #[test]
    fn pascal_handles_digits_and_empty() {
        assert_eq!(pascal("42"), "X42");
        assert_eq!(pascal(""), "X");
    }

    // ── snake ──────────────────────────────────────────────────────────

    #[test]
    fn snake_lowercases_and_separates() {
        assert_eq!(snake("PetStore"), "pet_store");
        assert_eq!(snake("pet_store"), "pet_store");
        assert_eq!(snake("HTTPClient"), "h_t_t_p_client");
    }

    #[test]
    fn snake_handles_digits_and_empty() {
        assert_eq!(snake("42"), "42");
        assert_eq!(snake(""), "");
    }

    // ── export_ident ───────────────────────────────────────────────────

    #[test]
    fn export_ident_caps_and_avoids_collisions() {
        assert_eq!(export_ident("pet"), "Pet");
        assert_eq!(export_ident("error"), "Error");
        // collision avoidance via "Model" suffix
        assert_eq!(export_ident("validation_error"), "ValidationErrorModel");
    }

    // ── field_name ─────────────────────────────────────────────────────

    #[test]
    fn field_name_avoids_reserved_keywords() {
        assert_eq!(field_name("name"), "Name"); // pascal-cased, not reserved
        assert_eq!(field_name("type"), "TypeField"); // reserved → "Field" suffix
        assert_eq!(field_name("for"), "ForField"); // reserved → "Field" suffix
    }

    // ── is_go_reserved ─────────────────────────────────────────────────

    #[test]
    fn reserved_keywords_recognized() {
        assert!(is_go_reserved("func"));
        assert!(is_go_reserved("type"));
        assert!(is_go_reserved("select"));
        assert!(!is_go_reserved("name"));
        assert!(!is_go_reserved("client"));
    }
}

// ── edge cases ────────────────────────────────────────────────────

#[test]
fn pascal_unicode_passthrough() {
    // Unicode chars that don't change under lowercasing should pass through.
    assert_eq!(pascal("cafe"), "Cafe"); // non-ASCII chars are dropped by pascal
}

#[test]
fn snake_preserves_numbers() {
    assert_eq!(snake("v2beta1"), "v2beta1");
    assert_eq!(snake("HTTP2"), "h_t_t_p2");
}

#[test]
fn export_ident_safe_model_name_collision() {
    // Built-in SDK types get "Model" suffix.
    assert_eq!(export_ident("middleware"), "MiddlewareModel");
    assert_eq!(export_ident("auth"), "AuthModel");
    // Non-colliding names pass through.
    assert_eq!(export_ident("pet_store"), "PetStore");
}

    // ── go_inline ─────────────────────────────────────────────────────

    #[test]
    fn go_inline_escapes_closes_block_comment() {
        assert_eq!(go_inline("use FooApi*/ instead"), "use FooApi*&#47; instead");
        assert_eq!(go_inline("no issues"), "no issues");
    }

    #[test]
    fn go_inline_collapses_newlines() {
        // Each newline individually becomes a space.
        assert_eq!(go_inline("line1\nline2"), "line1 line2");
        assert_eq!(go_inline("a\nb\nc"), "a b c");
    }
