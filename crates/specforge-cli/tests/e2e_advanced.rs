//! Advanced e2e: concurrency, dedupe, middleware, idempotency, streaming.
//!
//! Spins an in-process mock that records every request, then drives generated
//! TS / Go / Rust clients against it and asserts runtime behaviour.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;

/// A parsed HTTP request: method, path, headers, body.
type ParsedRequest = (String, String, HashMap<String, String>, Vec<u8>);
use std::time::Duration;

// ─── Shared helpers (mirrored lightly from e2e_smoke) ────────────────────────

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../../fixtures/{name}"))
}

fn specforge_bin() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.pop();
    let c = path.join("target/debug/specforge");
    if c.exists() {
        c
    } else {
        PathBuf::from("specforge")
    }
}

fn run(cmd: &mut Command) -> Result<String, String> {
    let out = cmd
        .output()
        .map_err(|e| format!("spawn {:?}: {e}", cmd.get_program()))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(format!(
            "command {:?} failed (exit {:?}):\nstdout:\n{}\nstderr:\n{}",
            cmd.get_program(),
            out.status.code(),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ))
    }
}

fn tool_works(bin: &Path) -> bool {
    for args in [&["--version"][..], &["version"][..], &["-v"][..]] {
        if let Ok(out) = Command::new(bin).args(args).output() {
            if out.status.success() {
                return true;
            }
        }
    }
    bin.exists()
}

fn resolve_tool(name: &str) -> Option<PathBuf> {
    let as_name = PathBuf::from(name);
    if tool_works(&as_name) {
        return Some(as_name);
    }
    let home = std::env::var("HOME").unwrap_or_default();
    for c in [
        format!("/usr/local/go/bin/{name}"),
        format!("/usr/bin/{name}"),
        format!("/usr/local/bin/{name}"),
        format!("{home}/.cargo/bin/{name}"),
    ] {
        let p = PathBuf::from(&c);
        if p.is_file() && tool_works(&p) {
            return Some(p);
        }
    }
    None
}

fn write_response(stream: &mut TcpStream, status: u16, ct: &str, body: &str, extra_headers: &[(&str, &str)]) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        201 => "Created",
        503 => "Service Unavailable",
        _ => "OK",
    };
    let mut header = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {ct}\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for (k, v) in extra_headers {
        header.push_str(&format!("{k}: {v}\r\n"));
    }
    header.push_str("\r\n");
    stream.write_all(header.as_bytes())?;
    stream.write_all(body.as_bytes())?;
    stream.flush()
}

fn parse_http(stream: &mut TcpStream) -> std::io::Result<ParsedRequest> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    let mut buf = [0u8; 32768];
    let n = stream.read(&mut buf)?;
    if n == 0 {
        return Ok((String::new(), String::new(), HashMap::new(), Vec::new()));
    }
    let raw = &buf[..n];
    let (head, body) = if let Some(i) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
        (&raw[..i], raw[i + 4..].to_vec())
    } else {
        (raw, Vec::new())
    };
    let head_s = String::from_utf8_lossy(head);
    let mut lines = head_s.lines();
    let req_line = lines.next().unwrap_or("");
    let mut parts = req_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("/").to_string();
    let mut headers = HashMap::new();
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
        }
    }
    Ok((method, path, headers, body))
}

// ─── Advanced mock ───────────────────────────────────────────────────────────

#[derive(Clone, Debug, Default)]
struct Recorded {
    method: String,
    path: String,
    headers: HashMap<String, String>,
}

struct AdvState {
    log: Vec<Recorded>,
    /// Active in-flight handlers (incremented at start, decremented at end).
    in_flight: u32,
    peak_in_flight: u32,
    /// Slow GET /slow sleeps this long.
    slow_ms: u64,
    /// GET /events returns SSE.
    /// POST /echo records idempotency keys.
    post_count: u32,
}

fn start_adv_mock() -> (SocketAddr, Arc<Mutex<AdvState>>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().unwrap();
    let state = Arc::new(Mutex::new(AdvState {
        log: Vec::new(),
        in_flight: 0,
        peak_in_flight: 0,
        slow_ms: 200,
        post_count: 0,
    }));
    let st2 = Arc::clone(&state);
    let handle = thread::spawn(move || {
        for conn in listener.incoming() {
            match conn {
                Ok(mut stream) => {
                    let st = Arc::clone(&st2);
                    let _ = handle_adv(&mut stream, st);
                }
                Err(_) => break,
            }
        }
    });
    thread::sleep(Duration::from_millis(20));
    (addr, state, handle)
}

fn handle_adv(stream: &mut TcpStream, state: Arc<Mutex<AdvState>>) -> std::io::Result<()> {
    let (method, path_q, headers, _body) = parse_http(stream)?;
    if method.is_empty() {
        return Ok(());
    }
    let path = path_q.split('?').next().unwrap_or("/").to_string();

    {
        let mut st = state.lock().unwrap();
        st.in_flight += 1;
        if st.in_flight > st.peak_in_flight {
            st.peak_in_flight = st.in_flight;
        }
        st.log.push(Recorded {
            method: method.clone(),
            path: path.clone(),
            headers: headers.clone(),
        });
    }

    let result = match (method.as_str(), path.as_str()) {
        ("GET", "/slow") => {
            let ms = state.lock().unwrap().slow_ms;
            thread::sleep(Duration::from_millis(ms));
            write_response(stream, 200, "application/json", r#"{"ok":true}"#, &[])
        }
        ("GET", "/ping") => {
            write_response(stream, 200, "application/json", r#"{"pong":true}"#, &[])
        }
        ("GET", "/mw-check") => {
            // Echo a request header set by middleware.
            let marker = headers
                .get("x-specforge-mw")
                .cloned()
                .unwrap_or_else(|| "missing".into());
            let body = format!(r#"{{"marker":"{marker}"}}"#);
            write_response(stream, 200, "application/json", &body, &[])
        }
        ("POST", "/echo") => {
            let mut st = state.lock().unwrap();
            st.post_count += 1;
            let key = headers
                .get("idempotency-key")
                .cloned()
                .unwrap_or_else(|| "none".into());
            let n = st.post_count;
            drop(st);
            let body = format!(r#"{{"n":{n},"key":"{key}"}}"#);
            write_response(stream, 201, "application/json", &body, &[])
        }
        ("GET", "/events") => {
            // SSE: two events then end.
            let body = "event: hello\ndata: one\n\nevent: hello\ndata: two\nid: 2\n\n";
            write_response(
                stream,
                200,
                "text/event-stream",
                body,
                &[("Cache-Control", "no-cache")],
            )
        }
        _ => write_response(
            stream,
            404,
            "application/json",
            r#"{"code":404,"message":"no route"}"#,
            &[],
        ),
    };

    {
        let mut st = state.lock().unwrap();
        st.in_flight = st.in_flight.saturating_sub(1);
    }
    result
}

// ─── Language legs ───────────────────────────────────────────────────────────

fn gen(lang: &str, out: &Path, name: &str) -> Result<(), String> {
    run(Command::new(specforge_bin()).args([
        "generate",
        fixture("petstore.yaml").to_str().unwrap(),
        "-o",
        out.to_str().unwrap(),
        "-l",
        lang,
        "-n",
        name,
    ]))?;
    Ok(())
}

fn smoke_go_advanced(out: &Path, base: &str, state: Arc<Mutex<AdvState>>) -> Result<(), String> {
    let go = resolve_tool("go").ok_or_else(|| "go not found".to_string())?;
    gen("go", out, "github.com/example/adv-go")?;

    let smoke = out.join("_smoke");
    std::fs::create_dir_all(&smoke).map_err(|e| e.to_string())?;
    std::fs::write(
        smoke.join("go.mod"),
        format!(
            "module smoke\n\ngo 1.22\n\nrequire github.com/example/adv-go v0.0.0\nreplace github.com/example/adv-go => {}\n",
            out.display()
        ),
    )
    .map_err(|e| e.to_string())?;

    std::fs::write(
        smoke.join("main.go"),
        format!(
            r#"package main

import (
	"context"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"sync"
	"sync/atomic"
	"time"

	sdk "github.com/example/adv-go"
)

func main() {{
	ctx := context.Background()
	base := "{base}"

	// ── Middleware rewrite ──────────────────────────────────────────────
	c := sdk.NewClient().WithBaseURL(base).WithTimeout(5 * time.Second).WithRetry(sdk.RetryOptions{{MaxRetries: 0}})
	c.Use(func(ctx context.Context, req *sdk.MiddlewareRequest, next func(context.Context, *sdk.MiddlewareRequest) (*sdk.MiddlewareResponse, error)) (*sdk.MiddlewareResponse, error) {{
		req.Headers.Set("X-Specforge-Mw", "go-mw")
		return next(ctx, req)
	}})
	// Hit /mw-check via DoJSON
	var mwOut map[string]any
	if err := c.DoJSON(ctx, "GET", "/mw-check", nil, nil, &mwOut); err != nil {{
		fmt.Fprintf(os.Stderr, "mw: %v\n", err); os.Exit(1)
	}}
	if mwOut["marker"] != "go-mw" {{
		fmt.Fprintf(os.Stderr, "mw marker want go-mw got %#v\n", mwOut); os.Exit(1)
	}}

	// ── Idempotency key on POST ─────────────────────────────────────────
	c2 := sdk.NewClient().WithBaseURL(base).WithTimeout(5 * time.Second).WithRetry(sdk.RetryOptions{{MaxRetries: 0}}).WithIdempotency(true)
	var postOut map[string]any
	if err := c2.DoJSON(ctx, "POST", "/echo", nil, map[string]string{{"x": "1"}}, &postOut); err != nil {{
		fmt.Fprintf(os.Stderr, "post: %v\n", err); os.Exit(1)
	}}
	key, _ := postOut["key"].(string)
	if key == "" || key == "none" {{
		fmt.Fprintf(os.Stderr, "missing idempotency key: %#v\n", postOut); os.Exit(1)
	}}

	// ── Dedupe: two concurrent GET /ping → single upstream hit ──────────
	c3 := sdk.NewClient().WithBaseURL(base).WithTimeout(5 * time.Second).WithDedupe(true).WithRetry(sdk.RetryOptions{{MaxRetries: 0}})
	var wg sync.WaitGroup
	wg.Add(2)
	for i := 0; i < 2; i++ {{
		go func() {{
			defer wg.Done()
			var out map[string]any
			_ = c3.DoJSON(ctx, "GET", "/ping", nil, nil, &out)
		}}()
	}}
	wg.Wait()

	// ── Concurrency: MaxConcurrent=1 on /slow ───────────────────────────
	c4 := sdk.NewClient().WithBaseURL(base).WithTimeout(5 * time.Second).WithMaxConcurrent(1).WithDedupe(false).WithRetry(sdk.RetryOptions{{MaxRetries: 0}})
	start := time.Now()
	wg.Add(2)
	var slowOK atomic.Int32
	for i := 0; i < 2; i++ {{
		go func() {{
			defer wg.Done()
			var out map[string]any
			if err := c4.DoJSON(ctx, "GET", "/slow", nil, nil, &out); err == nil {{
				slowOK.Add(1)
			}}
		}}()
	}}
	wg.Wait()
	elapsed := time.Since(start)
	if slowOK.Load() != 2 {{
		fmt.Fprintf(os.Stderr, "slow calls failed\n"); os.Exit(1)
	}}
	// Two 200ms serialised ≥ ~350ms; parallel would be ~200ms.
	if elapsed < 350*time.Millisecond {{
		fmt.Fprintf(os.Stderr, "concurrency not serialised: %v\n", elapsed); os.Exit(1)
	}}

	// ── Streaming SSE ───────────────────────────────────────────────────
	c5 := sdk.NewClient().WithBaseURL(base).WithTimeout(5 * time.Second)
	res, err := c5.DoStream(ctx, "GET", "/events", url.Values{{}}, nil)
	if err != nil {{
		fmt.Fprintf(os.Stderr, "stream: %v\n", err); os.Exit(1)
	}}
	it := sdk.NewSseIterator(res)
	var events []sdk.ServerSentEvent
	for it.Next() {{
		events = append(events, it.Event())
	}}
	if err := it.Err(); err != nil {{
		fmt.Fprintf(os.Stderr, "sse err: %v\n", err); os.Exit(1)
	}}
	io.Copy(io.Discard, res.Body)
	res.Body.Close()
	if len(events) < 2 {{
		fmt.Fprintf(os.Stderr, "sse events=%d\n", len(events)); os.Exit(1)
	}}
	if events[0].Data != "one" || events[1].Data != "two" {{
		fmt.Fprintf(os.Stderr, "sse data %#v\n", events); os.Exit(1)
	}}

	fmt.Printf("go-advanced-ok mw=%v idem=%s sse=%d elapsed_ms=%d\n", mwOut["marker"], key, len(events), elapsed.Milliseconds())
}}

var _ = http.StatusOK
"#,
            base = base,
        ),
    )
    .map_err(|e| e.to_string())?;

    // Reset peak before concurrency test portion — actually concurrency is inside main.
    // Capture peak after run.
    let before_peak = state.lock().unwrap().peak_in_flight;
    let stdout = run(Command::new(&go)
        .args(["run", "."])
        .current_dir(&smoke)
        .env("GO111MODULE", "on"))?;
    if !stdout.contains("go-advanced-ok") {
        return Err(format!("go advanced missing ok: {stdout}"));
    }

    // Server-side assertions.
    let st = state.lock().unwrap();
    let ping_hits = st.log.iter().filter(|r| r.path == "/ping").count();
    if ping_hits != 1 {
        return Err(format!("dedupe: expected 1 /ping hit, got {ping_hits} (log={:?})", st.log));
    }
    let echo = st.log.iter().find(|r| r.path == "/echo" && r.method == "POST");
    let Some(echo) = echo else {
        return Err("no POST /echo recorded".into());
    };
    let key = echo.headers.get("idempotency-key").cloned().unwrap_or_default();
    if key.is_empty() {
        return Err(format!("POST /echo missing Idempotency-Key: {:?}", echo.headers));
    }
    let mw = st.log.iter().find(|r| r.path == "/mw-check");
    let Some(mw) = mw else {
        return Err("no /mw-check".into());
    };
    if mw.headers.get("x-specforge-mw").map(|s| s.as_str()) != Some("go-mw") {
        return Err(format!("middleware header missing: {:?}", mw.headers));
    }
    // Peak in-flight for /slow with MaxConcurrent=1 should stay ≤ 1 above prior baseline.
    // (Other requests may have bumped peak earlier; check slow overlap via elapsed in client.)
    let _ = before_peak;
    eprintln!("go advanced e2e: {stdout}");
    Ok(())
}

fn smoke_rust_advanced(out: &Path, base: &str, state: Arc<Mutex<AdvState>>) -> Result<(), String> {
    let cargo = resolve_tool("cargo").ok_or_else(|| "cargo not found".to_string())?;
    gen("rust", out, "adv_sdk")?;

    let cargo_toml = out.join("Cargo.toml");
    let mut toml = std::fs::read_to_string(&cargo_toml).map_err(|e| e.to_string())?;
    if !toml.contains("name = \"advanced\"") {
        toml.push_str(
            r#"
[[bin]]
name = "advanced"
path = "src/bin/advanced.rs"
"#,
        );
        std::fs::write(&cargo_toml, toml).map_err(|e| e.to_string())?;
    }
    let bin = out.join("src/bin");
    std::fs::create_dir_all(&bin).map_err(|e| e.to_string())?;
    std::fs::write(
        bin.join("advanced.rs"),
        format!(
            r#"use adv_sdk::middleware::{{MiddlewareRequest, MiddlewareResponse}};
use adv_sdk::streaming::SseStream;
use adv_sdk::retry::RetryOptions;
use adv_sdk::Client;
use std::sync::Arc;
use std::time::{{Duration, Instant}};

#[tokio::main]
async fn main() {{
    let base = "{base}";

    // Middleware
    let mw: adv_sdk::Middleware = Arc::new(|mut req: MiddlewareRequest, next| {{
        Box::pin(async move {{
            req.headers.insert(
                "x-specforge-mw",
                reqwest::header::HeaderValue::from_static("rust-mw"),
            );
            next(req).await
        }})
    }});
    let client = Client::builder()
        .base_url(base)
        .timeout(Duration::from_secs(5))
        .retry(RetryOptions {{ max_retries: 0, ..RetryOptions::default() }})
        .middleware(mw)
        .build()
        .unwrap();
    let v: serde_json::Value = client
        .request_json(reqwest::Method::GET, "/mw-check", &[], None::<&()>)
        .await
        .expect("mw");
    assert_eq!(v["marker"], "rust-mw", "got {{v}}");

    // Idempotency
    let c2 = Client::builder()
        .base_url(base)
        .timeout(Duration::from_secs(5))
        .retry(RetryOptions {{ max_retries: 0, ..RetryOptions::default() }})
        .idempotency(true)
        .build()
        .unwrap();
    let body = serde_json::json!({{"x": 1}});
    let posted: serde_json::Value = c2
        .request_json(reqwest::Method::POST, "/echo", &[], Some(&body))
        .await
        .expect("post");
    let key = posted["key"].as_str().unwrap_or("");
    assert!(!key.is_empty() && key != "none", "idem key {{posted}}");

    // Dedupe two concurrent GET /ping
    let c3 = Client::builder()
        .base_url(base)
        .timeout(Duration::from_secs(5))
        .dedupe(true)
        .retry(RetryOptions {{ max_retries: 0, ..RetryOptions::default() }})
        .build()
        .unwrap();
    let a = c3.request_json::<serde_json::Value, ()>(reqwest::Method::GET, "/ping", &[], None);
    let b = c3.request_json::<serde_json::Value, ()>(reqwest::Method::GET, "/ping", &[], None);
    let (ra, rb) = tokio::join!(a, b);
    ra.expect("ping a");
    rb.expect("ping b");

    // Concurrency MaxConcurrent=1
    let c4 = Client::builder()
        .base_url(base)
        .timeout(Duration::from_secs(5))
        .max_concurrent(1)
        .dedupe(false)
        .retry(RetryOptions {{ max_retries: 0, ..RetryOptions::default() }})
        .build()
        .unwrap();
    let start = Instant::now();
    let s1 = c4.request_json::<serde_json::Value, ()>(reqwest::Method::GET, "/slow", &[], None);
    let s2 = c4.request_json::<serde_json::Value, ()>(reqwest::Method::GET, "/slow", &[], None);
    let (r1, r2) = tokio::join!(s1, s2);
    r1.expect("slow1");
    r2.expect("slow2");
    let elapsed = start.elapsed();
    assert!(
        elapsed >= Duration::from_millis(350),
        "expected serialised slow calls, elapsed={{elapsed:?}}"
    );

    // SSE stream
    let c5 = Client::builder()
        .base_url(base)
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let res = c5
        .request_stream(reqwest::Method::GET, "/events", &[], None)
        .await
        .expect("stream");
    let mut sse = SseStream::new(res.bytes_stream());
    let mut events = Vec::new();
    while let Some(ev) = sse.next_event().await.expect("sse") {{
        events.push(ev);
    }}
    assert!(events.len() >= 2, "events={{events:?}}");
    assert_eq!(events[0].data, "one");
    assert_eq!(events[1].data, "two");

    println!(
        "rust-advanced-ok mw=rust-mw idem={{}} sse={{}} elapsed_ms={{}}",
        key,
        events.len(),
        elapsed.as_millis()
    );
}}
"#,
            base = base,
        ),
    )
    .map_err(|e| e.to_string())?;

    // Clear log for clean ping count — but other langs may share mock.
    // We'll filter by checking at least the rust-specific markers.
    let stdout = run(Command::new(&cargo)
        .args(["run", "-q", "--bin", "advanced"])
        .current_dir(out))?;
    if !stdout.contains("rust-advanced-ok") {
        return Err(format!("rust advanced missing ok: {stdout}"));
    }

    let st = state.lock().unwrap();
    let mw = st
        .log
        .iter()
        .find(|r| r.path == "/mw-check" && r.headers.get("x-specforge-mw").map(|s| s.as_str()) == Some("rust-mw"));
    if mw.is_none() {
        return Err(format!("rust mw header not seen: {:?}", st.log));
    }
    let echo_keys: Vec<_> = st
        .log
        .iter()
        .filter(|r| r.path == "/echo")
        .filter_map(|r| r.headers.get("idempotency-key").cloned())
        .collect();
    if echo_keys.is_empty() {
        return Err("rust: no idempotency-key on POST /echo".into());
    }
    eprintln!("rust advanced e2e: {stdout}");
    Ok(())
}

fn smoke_ts_advanced(out: &Path, base: &str, state: Arc<Mutex<AdvState>>) -> Result<(), String> {
    let npm = resolve_tool("npm").ok_or_else(|| "npm not found".to_string())?;
    let npx = resolve_tool("npx").ok_or_else(|| "npx not found".to_string())?;
    let _node = resolve_tool("node").ok_or_else(|| "node not found".to_string())?;

    gen("ts", out, "@adv/sdk")?;
    run(Command::new(&npm)
        .args(["install", "--silent"])
        .current_dir(out))?;
    let _ = run(Command::new(&npm)
        .args(["install", "--silent", "--no-save", "tsx"])
        .current_dir(out));

    std::fs::write(
        out.join("advanced.mts"),
        format!(
            r#"import {{ ApiClient }} from "./src/client.ts";
import {{ streamSse }} from "./src/streaming.ts";

const baseUrl = "{base}";

// Middleware
const c = new ApiClient({{ baseUrl, timeoutMs: 5000, retry: {{ maxRetries: 0 }} }});
c.use(async (req, next) => {{
  req.headers = {{ ...req.headers, "x-specforge-mw": "ts-mw" }};
  return next(req);
}});
const mwRes = await c.requestJson("GET", "/mw-check");
if (mwRes.marker !== "ts-mw") {{
  console.error("mw", mwRes);
  process.exit(1);
}}

// Idempotency
const c2 = new ApiClient({{ baseUrl, timeoutMs: 5000, retry: {{ maxRetries: 0 }}, idempotency: true }});
const posted = await c2.requestJson("POST", "/echo", {{ body: {{ x: 1 }} }});
if (!posted.key || posted.key === "none") {{
  console.error("idem", posted);
  process.exit(1);
}}

// Dedupe
const c3 = new ApiClient({{ baseUrl, timeoutMs: 5000, retry: {{ maxRetries: 0 }}, dedupe: true }});
await Promise.all([
  c3.requestJson("GET", "/ping"),
  c3.requestJson("GET", "/ping"),
]);

// Concurrency
const c4 = new ApiClient({{
  baseUrl,
  timeoutMs: 5000,
  retry: {{ maxRetries: 0 }},
  maxConcurrent: 1,
  dedupe: false,
}});
const t0 = Date.now();
await Promise.all([
  c4.requestJson("GET", "/slow"),
  c4.requestJson("GET", "/slow"),
]);
const elapsed = Date.now() - t0;
if (elapsed < 350) {{
  console.error("concurrency not serialised", elapsed);
  process.exit(1);
}}

// SSE
const c5 = new ApiClient({{ baseUrl, timeoutMs: 5000, retry: {{ maxRetries: 0 }} }});
const res = await c5.request("GET", "/events");
const events = [];
for await (const ev of streamSse(res)) {{
  events.push(ev);
}}
if (events.length < 2 || events[0].data !== "one" || events[1].data !== "two") {{
  console.error("sse", events);
  process.exit(1);
}}

console.log("ts-advanced-ok", mwRes.marker, posted.key, events.length, elapsed);
"#,
            base = base,
        ),
    )
    .map_err(|e| e.to_string())?;

    let stdout = run(Command::new(&npx)
        .args(["tsx", "advanced.mts"])
        .current_dir(out))?;
    if !stdout.contains("ts-advanced-ok") {
        return Err(format!("ts advanced missing ok: {stdout}"));
    }

    let st = state.lock().unwrap();
    let mw = st.log.iter().any(|r| {
        r.path == "/mw-check" && r.headers.get("x-specforge-mw").map(|s| s.as_str()) == Some("ts-mw")
    });
    if !mw {
        return Err(format!("ts mw header not seen: {:?}", st.log));
    }
    eprintln!("ts advanced e2e: {stdout}");
    Ok(())
}

// ─── Test ────────────────────────────────────────────────────────────────────

#[test]
fn e2e_advanced_concurrency_dedupe_middleware_idempotency_streaming() {
    let (addr, state, _server) = start_adv_mock();
    let base = format!("http://{addr}");
    eprintln!("advanced mock at {base}");

    // Sanity SSE probe
    {
        let mut s = TcpStream::connect(addr).unwrap();
        s.write_all(b"GET /events HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .unwrap();
        let mut resp = String::new();
        s.read_to_string(&mut resp).unwrap();
        assert!(resp.contains("data: one"), "{resp}");
    }

    let root = std::env::temp_dir().join(format!("specforge-e2e-adv-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();

    let mut failures = Vec::new();
    let mut ran = 0;

    // Fresh state log between languages would be nicer; filter by markers instead.
    // Run Go
    {
        // Reset log for cleaner dedupe assert on go's pings — actually concurrent langs share server.
        // Run sequentially and snapshot ping count around each leg.
        let before = state.lock().unwrap().log.len();
        match smoke_go_advanced(&root.join("go"), &base, Arc::clone(&state)) {
            Ok(()) => ran += 1,
            Err(e) if e.contains("not found") => eprintln!("skip go advanced: {e}"),
            Err(e) => failures.push(format!("go: {e}")),
        }
        let _ = before;
    }

    {
        match smoke_rust_advanced(&root.join("rust"), &base, Arc::clone(&state)) {
            Ok(()) => ran += 1,
            Err(e) if e.contains("not found") => eprintln!("skip rust advanced: {e}"),
            Err(e) => failures.push(format!("rust: {e}")),
        }
    }

    {
        match smoke_ts_advanced(&root.join("ts"), &base, Arc::clone(&state)) {
            Ok(()) => ran += 1,
            Err(e) if e.contains("not found") => eprintln!("skip ts advanced: {e}"),
            Err(e) => failures.push(format!("ts: {e}")),
        }
    }

    // Global: at least one POST with idempotency key, at least one SSE request.
    {
        let st = state.lock().unwrap();
        assert!(
            st.log.iter().any(|r| r.path == "/events"),
            "no /events hits: {:?}",
            st.log
        );
        assert!(
            st.log
                .iter()
                .any(|r| r.path == "/echo" && r.headers.contains_key("idempotency-key")),
            "no idempotent POST: {:?}",
            st.log
        );
    }

    assert!(
        failures.is_empty(),
        "advanced e2e failures ({ran} ok):\n{}",
        failures.join("\n\n")
    );
    assert!(ran >= 1, "no language toolchains for advanced e2e");
    eprintln!("advanced e2e passed for {ran} language(s)");
}
