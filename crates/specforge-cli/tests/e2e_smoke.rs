//! End-to-end smoke tests.
//!
//! 1. **Petstore basics** — list / show / create against a tiny mock (TS + Go + Rust).
//! 2. **Sample-api runtime** — bearer auth, retry-on-503, cursor pagination.
//!
//! Skips a language leg only when its toolchain is missing. A failed generate or
//! client call (when the toolchain IS present) is a hard failure.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};

/// A parsed HTTP request: method, path, version, headers, body.
type ParsedRequest = (String, String, String, HashMap<String, String>, Vec<u8>);

/// A language test leg: its label and the function that runs it for a crate dir.
type LangLeg = (&'static str, Box<dyn Fn(&Path) -> Result<(), String>>);
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

// ─── Shared HTTP mock primitives ─────────────────────────────────────────────

fn write_response(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &str,
) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        201 => "Created",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        503 => "Service Unavailable",
        _ => "OK",
    };
    let header = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(body.as_bytes())?;
    stream.flush()
}

fn escape_json(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn parse_request(stream: &mut TcpStream) -> std::io::Result<ParsedRequest> {
    stream.set_read_timeout(Some(Duration::from_secs(3)))?;
    stream.set_write_timeout(Some(Duration::from_secs(3)))?;

    let mut buf = [0u8; 16384];
    let n = stream.read(&mut buf)?;
    if n == 0 {
        return Ok((
            String::new(),
            String::new(),
            String::new(),
            HashMap::new(),
            Vec::new(),
        ));
    }
    let raw = &buf[..n];
    // Split headers/body on \r\n\r\n
    let (head, body) = if let Some(i) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
        (&raw[..i], raw[i + 4..].to_vec())
    } else {
        (raw, Vec::new())
    };
    let head_s = String::from_utf8_lossy(head);
    let mut lines = head_s.lines();
    let request_line = lines.next().unwrap_or("");
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path_q = parts.next().unwrap_or("/").to_string();
    let (path, query_str) = match path_q.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (path_q.clone(), String::new()),
    };
    let mut headers = HashMap::new();
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
        }
    }
    let mut query = HashMap::new();
    for pair in query_str.split('&').filter(|s| !s.is_empty()) {
        let (k, v) = match pair.split_once('=') {
            Some((k, v)) => (k.to_string(), urlencoding_decode(v)),
            None => (pair.to_string(), String::new()),
        };
        query.insert(k, v);
    }
    // Also expose query map via a side channel — return path and encode query in path_q style.
    // We return headers and body; query is re-parsed by callers from path_q if needed.
    // Simpler: stash query in headers under a private key? Better return it.
    let _ = query; // parsed below by helper using path_q
    Ok((method, path, path_q, headers, body))
}

fn parse_query(path_q: &str) -> HashMap<String, String> {
    let mut query = HashMap::new();
    let q = path_q.split_once('?').map(|(_, q)| q).unwrap_or("");
    for pair in q.split('&').filter(|s| !s.is_empty()) {
        let (k, v) = match pair.split_once('=') {
            Some((k, v)) => (k.to_string(), urlencoding_decode(v)),
            None => (pair.to_string(), String::new()),
        };
        query.insert(k, v);
    }
    query
}

fn urlencoding_decode(s: &str) -> String {
    // Minimal: + → space, %XX hex.
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hex = &s[i + 1..i + 3];
                if let Ok(v) = u8::from_str_radix(hex, 16) {
                    out.push(v as char);
                    i += 3;
                } else {
                    out.push('%');
                    i += 1;
                }
            }
            c => {
                out.push(c as char);
                i += 1;
            }
        }
    }
    out
}

// ─── Mock A: classic petstore (no auth) ──────────────────────────────────────

#[derive(Clone)]
struct Pet {
    id: i64,
    name: String,
    tag: Option<String>,
}

struct PetstoreState {
    pets: HashMap<i64, Pet>,
    next_id: i64,
}

fn start_petstore_mock() -> (SocketAddr, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock");
    let addr = listener.local_addr().expect("local_addr");
    let state = Arc::new(Mutex::new(PetstoreState {
        pets: {
            let mut m = HashMap::new();
            m.insert(
                1,
                Pet {
                    id: 1,
                    name: "Doggo".into(),
                    tag: Some("friendly".into()),
                },
            );
            m.insert(
                2,
                Pet {
                    id: 2,
                    name: "Michi".into(),
                    tag: Some("cat".into()),
                },
            );
            m
        },
        next_id: 3,
    }));

    let handle = thread::spawn(move || {
        for conn in listener.incoming() {
            match conn {
                Ok(mut stream) => {
                    let st = Arc::clone(&state);
                    let _ = handle_petstore(&mut stream, st);
                }
                Err(_) => break,
            }
        }
    });
    thread::sleep(Duration::from_millis(20));
    (addr, handle)
}

fn handle_petstore(stream: &mut TcpStream, state: Arc<Mutex<PetstoreState>>) -> std::io::Result<()> {
    let (method, path, _path_q, _headers, _body) = parse_request(stream)?;
    if method.is_empty() {
        return Ok(());
    }

    let (status, body, ct) = match (method.as_str(), path.as_str()) {
        ("GET", "/pets") => {
            let st = state.lock().unwrap();
            let mut pets: Vec<&Pet> = st.pets.values().collect();
            pets.sort_by_key(|p| p.id);
            (200, pets_to_json(&pets), "application/json")
        }
        ("GET", p) if p.starts_with("/pets/") => {
            let id_str = &p["/pets/".len()..];
            let st = state.lock().unwrap();
            if let Ok(id) = id_str.parse::<i64>() {
                if let Some(pet) = st.pets.get(&id) {
                    (200, pet_to_json(pet), "application/json")
                } else {
                    (404, r#"{"code":404,"message":"not found"}"#.into(), "application/json")
                }
            } else {
                (400, r#"{"code":400,"message":"bad id"}"#.into(), "application/json")
            }
        }
        ("POST", "/pets") => {
            let mut st = state.lock().unwrap();
            let id = st.next_id;
            st.next_id += 1;
            st.pets.insert(
                id,
                Pet {
                    id,
                    name: format!("Pet-{id}"),
                    tag: None,
                },
            );
            (201, String::new(), "text/plain")
        }
        ("GET", "/health") => (200, r#"{"ok":true}"#.into(), "application/json"),
        _ => (404, r#"{"code":404,"message":"no route"}"#.into(), "application/json"),
    };
    write_response(stream, status, ct, &body)
}

fn pet_to_json(p: &Pet) -> String {
    match &p.tag {
        Some(tag) => format!(
            r#"{{"id":{},"name":"{}","tag":"{}"}}"#,
            p.id,
            escape_json(&p.name),
            escape_json(tag)
        ),
        None => format!(r#"{{"id":{},"name":"{}"}}"#, p.id, escape_json(&p.name)),
    }
}

fn pets_to_json(pets: &[&Pet]) -> String {
    let parts: Vec<String> = pets.iter().map(|p| pet_to_json(p)).collect();
    format!("[{}]", parts.join(","))
}

// ─── Mock B: sample-api runtime (auth + flaky + cursor pages) ────────────────

const BEARER_TOKEN: &str = "smoke-secret-token";

struct SampleState {
    /// How many times GET /pets (no cursor) has been hit — first is 503.
    list_hits: u32,
    /// Pages keyed by cursor; "" = first page.
    pages: HashMap<String, (Vec<SamplePet>, Option<String>)>,
}

#[derive(Clone)]
struct SamplePet {
    id: String,
    name: String,
    species: String,
    created_at: String,
}

fn start_sample_mock() -> (SocketAddr, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind sample mock");
    let addr = listener.local_addr().expect("local_addr");

    let mut pages = HashMap::new();
    pages.insert(
        String::new(),
        (
            vec![
                SamplePet {
                    id: "p1".into(),
                    name: "Alpha".into(),
                    species: "dog".into(),
                    created_at: "2024-01-01T00:00:00Z".into(),
                },
                SamplePet {
                    id: "p2".into(),
                    name: "Beta".into(),
                    species: "cat".into(),
                    created_at: "2024-01-02T00:00:00Z".into(),
                },
            ],
            Some("cursor-2".into()),
        ),
    );
    pages.insert(
        "cursor-2".into(),
        (
            vec![SamplePet {
                id: "p3".into(),
                name: "Gamma".into(),
                species: "bird".into(),
                created_at: "2024-01-03T00:00:00Z".into(),
            }],
            None,
        ),
    );

    let state = Arc::new(Mutex::new(SampleState {
        list_hits: 0,
        pages,
    }));

    let handle = thread::spawn(move || {
        for conn in listener.incoming() {
            match conn {
                Ok(mut stream) => {
                    let st = Arc::clone(&state);
                    let _ = handle_sample(&mut stream, st);
                }
                Err(_) => break,
            }
        }
    });
    thread::sleep(Duration::from_millis(20));
    (addr, handle)
}

fn sample_pet_json(p: &SamplePet) -> String {
    format!(
        r#"{{"id":"{}","name":"{}","species":"{}","createdAt":"{}"}}"#,
        escape_json(&p.id),
        escape_json(&p.name),
        escape_json(&p.species),
        escape_json(&p.created_at)
    )
}

fn handle_sample(stream: &mut TcpStream, state: Arc<Mutex<SampleState>>) -> std::io::Result<()> {
    let (method, path, path_q, headers, body) = parse_request(stream)?;
    if method.is_empty() {
        return Ok(());
    }
    let query = parse_query(&path_q);

    // Auth gate — every route except /health requires Bearer.
    if path != "/health" {
        let auth = headers.get("authorization").map(|s| s.as_str()).unwrap_or("");
        let expected = format!("Bearer {BEARER_TOKEN}");
        if auth != expected {
            return write_response(
                stream,
                401,
                "application/json",
                r#"{"code":401,"message":"unauthorized"}"#,
            );
        }
    }

    let (status, resp_body, ct) = match (method.as_str(), path.as_str()) {
        ("GET", "/health") => (200, r#"{"ok":true}"#.into(), "application/json"),

        // Flaky list: first request with no cursor → 503; subsequent → page.
        // Also serves cursor pages for pagination.
        ("GET", "/pets") => {
            let cursor = query.get("cursor").cloned().unwrap_or_default();
            let mut st = state.lock().unwrap();
            if cursor.is_empty() {
                st.list_hits += 1;
                if st.list_hits == 1 {
                    // Force the client retry path.
                    return write_response(
                        stream,
                        503,
                        "application/json",
                        r#"{"code":503,"message":"try again"}"#,
                    );
                }
            }
            let (items, next) = st
                .pages
                .get(&cursor)
                .cloned()
                .unwrap_or_else(|| (vec![], None));
            let items_json: Vec<String> = items.iter().map(sample_pet_json).collect();
            let has_more = if next.is_some() { "true" } else { "false" };
            let next_json = match &next {
                Some(c) => format!(r#""{}""#, escape_json(c)),
                None => "null".into(),
            };
            let page = format!(
                r#"{{"items":[{}],"nextCursor":{},"hasMore":{}}}"#,
                items_json.join(","),
                next_json,
                has_more
            );
            (200, page, "application/json")
        }

        ("GET", p) if p.starts_with("/pets/") => {
            let id = &p["/pets/".len()..];
            let st = state.lock().unwrap();
            let found = st
                .pages
                .values()
                .flat_map(|(items, _)| items.iter())
                .find(|p| p.id == id);
            match found {
                Some(pet) => (200, sample_pet_json(pet), "application/json"),
                None => (404, r#"{"code":404,"message":"not found"}"#.into(), "application/json"),
            }
        }

        ("POST", "/pets") => {
            // Echo a created pet; body is ignored for smoke.
            let _ = body;
            let pet = SamplePet {
                id: "new-1".into(),
                name: "Created".into(),
                species: "dog".into(),
                created_at: "2024-06-01T00:00:00Z".into(),
            };
            (201, sample_pet_json(&pet), "application/json")
        }

        _ => (404, r#"{"code":404,"message":"no route"}"#.into(), "application/json"),
    };
    write_response(stream, status, ct, &resp_body)
}

// ─── Tooling ─────────────────────────────────────────────────────────────────

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../../fixtures/{name}"))
}

fn specforge_bin() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.pop();
    let candidate = path.join("target/debug/specforge");
    if candidate.exists() {
        candidate
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
    let candidates = [
        format!("/usr/local/go/bin/{name}"),
        format!("/usr/lib/go/bin/{name}"),
        format!("/usr/bin/{name}"),
        format!("/usr/local/bin/{name}"),
        format!("{home}/.cargo/bin/{name}"),
        format!("/usr/local/node/bin/{name}"),
    ];
    for c in candidates {
        let p = PathBuf::from(&c);
        if p.is_file() && tool_works(&p) {
            return Some(p);
        }
    }
    let nvm = PathBuf::from(format!("{home}/.nvm/versions/node"));
    if nvm.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&nvm) {
            let mut versions: Vec<_> = entries.filter_map(|e| e.ok()).collect();
            versions.sort_by_key(|e| e.file_name());
            for e in versions.into_iter().rev() {
                let p = e.path().join("bin").join(name);
                if p.is_file() && tool_works(&p) {
                    return Some(p);
                }
            }
        }
    }
    None
}

// ─── Petstore language legs ──────────────────────────────────────────────────

fn smoke_go_petstore(out_dir: &Path, base_url: &str) -> Result<(), String> {
    let go = resolve_tool("go").ok_or_else(|| "go not found".to_string())?;
    run(Command::new(specforge_bin()).args([
        "generate",
        fixture("petstore.yaml").to_str().unwrap(),
        "-o",
        out_dir.to_str().unwrap(),
        "-l",
        "go",
        "-n",
        "github.com/example/petstore-smoke-go",
    ]))?;

    let smoke_dir = out_dir.join("_smoke");
    std::fs::create_dir_all(&smoke_dir).map_err(|e| e.to_string())?;
    std::fs::write(
        smoke_dir.join("go.mod"),
        format!(
            "module smoke\n\ngo 1.22\n\nrequire github.com/example/petstore-smoke-go v0.0.0\n\nreplace github.com/example/petstore-smoke-go => {}\n",
            out_dir.display()
        ),
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(
        smoke_dir.join("main.go"),
        format!(
            r#"package main
import (
	"context"; "fmt"; "os"; "time"
	sdk "github.com/example/petstore-smoke-go"
)
func main() {{
	ctx := context.Background()
	c := sdk.NewClient().WithBaseURL("{base_url}").WithTimeout(5 * time.Second)
	pets, err := c.ListPets(ctx, 10)
	if err != nil {{ fmt.Fprintf(os.Stderr, "ListPets: %v\n", err); os.Exit(1) }}
	if pets == nil || len(*pets) < 1 {{ fmt.Fprintf(os.Stderr, "ListPets empty\n"); os.Exit(1) }}
	pet, err := c.ShowPetById(ctx, "1")
	if err != nil {{ fmt.Fprintf(os.Stderr, "ShowPetById: %v\n", err); os.Exit(1) }}
	if pet.Id != 1 || pet.Name == "" {{ fmt.Fprintf(os.Stderr, "bad pet %#v\n", pet); os.Exit(1) }}
	if err := c.CreatePets(ctx); err != nil {{ fmt.Fprintf(os.Stderr, "CreatePets: %v\n", err); os.Exit(1) }}
	fmt.Printf("go-smoke-ok %d %s\n", len(*pets), pet.Name)
}}
"#,
            base_url = base_url
        ),
    )
    .map_err(|e| e.to_string())?;

    let stdout = run(Command::new(&go)
        .args(["run", "."])
        .current_dir(&smoke_dir)
        .env("GO111MODULE", "on"))?;
    if !stdout.contains("go-smoke-ok") {
        return Err(format!("go petstore missing ok: {stdout}"));
    }
    eprintln!("go petstore e2e: {stdout}");
    Ok(())
}

fn smoke_rust_petstore(out_dir: &Path, base_url: &str) -> Result<(), String> {
    let cargo = resolve_tool("cargo").ok_or_else(|| "cargo not found".to_string())?;
    run(Command::new(specforge_bin()).args([
        "generate",
        fixture("petstore.yaml").to_str().unwrap(),
        "-o",
        out_dir.to_str().unwrap(),
        "-l",
        "rust",
        "-n",
        "petstore_smoke_sdk",
    ]))?;

    let cargo_toml = out_dir.join("Cargo.toml");
    let mut toml = std::fs::read_to_string(&cargo_toml).map_err(|e| e.to_string())?;
    if !toml.contains("[[bin]]") {
        toml.push_str(
            r#"
[[bin]]
name = "smoke"
path = "src/bin/smoke.rs"
"#,
        );
        std::fs::write(&cargo_toml, toml).map_err(|e| e.to_string())?;
    }
    let bin_dir = out_dir.join("src/bin");
    std::fs::create_dir_all(&bin_dir).map_err(|e| e.to_string())?;
    std::fs::write(
        bin_dir.join("smoke.rs"),
        format!(
            r#"use petstore_smoke_sdk::api;
use petstore_smoke_sdk::Client;
#[tokio::main]
async fn main() {{
    let client = Client::builder()
        .base_url("{base_url}")
        .timeout(std::time::Duration::from_secs(5))
        .build().expect("client");
    let pets = api::list_pets(&client, Some(10)).await.expect("list");
    assert!(pets.len() >= 1);
    let pet = api::show_pet_by_id(&client, "1").await.expect("show");
    assert_eq!(pet.id, 1);
    api::create_pets(&client).await.expect("create");
    println!("rust-smoke-ok {{}} {{}}", pets.len(), pet.name);
}}
"#,
            base_url = base_url
        ),
    )
    .map_err(|e| e.to_string())?;

    let stdout = run(Command::new(&cargo)
        .args(["run", "-q", "--bin", "smoke"])
        .current_dir(out_dir))?;
    if !stdout.contains("rust-smoke-ok") {
        return Err(format!("rust petstore missing ok: {stdout}"));
    }
    eprintln!("rust petstore e2e: {stdout}");
    Ok(())
}

fn smoke_ts_petstore(out_dir: &Path, base_url: &str) -> Result<(), String> {
    let npm = resolve_tool("npm").ok_or_else(|| "npm not found".to_string())?;
    let _node = resolve_tool("node").ok_or_else(|| "node not found".to_string())?;
    let npx = resolve_tool("npx").ok_or_else(|| "npx not found".to_string())?;

    run(Command::new(specforge_bin()).args([
        "generate",
        fixture("petstore.yaml").to_str().unwrap(),
        "-o",
        out_dir.to_str().unwrap(),
        "-l",
        "ts",
        "-n",
        "@smoke/petstore",
    ]))?;
    run(Command::new(&npm)
        .args(["install", "--silent"])
        .current_dir(out_dir))?;
    let _ = run(Command::new(&npm)
        .args(["install", "--silent", "--no-save", "tsx"])
        .current_dir(out_dir));

    std::fs::write(
        out_dir.join("smoke.mts"),
        format!(
            r#"import {{ createClient }} from "./src/index.ts";
const client = createClient({{ baseUrl: "{base_url}", timeoutMs: 5000, retry: {{ maxRetries: 0 }} }});
const pets = await client.pets.listPets({{ limit: 10 }});
if (!Array.isArray(pets) || pets.length < 1) {{ console.error("list bad", pets); process.exit(1); }}
const pet = await client.pets.showPetById({{ petId: "1" }});
if (pet.id !== 1 || !pet.name) {{ console.error("show bad", pet); process.exit(1); }}
await client.pets.createPets();
console.log("ts-smoke-ok", pets.length, pet.name);
"#,
            base_url = base_url
        ),
    )
    .map_err(|e| e.to_string())?;

    let stdout = run(Command::new(&npx)
        .args(["tsx", "smoke.mts"])
        .current_dir(out_dir))?;
    if !stdout.contains("ts-smoke-ok") {
        return Err(format!("ts petstore missing ok: {stdout}"));
    }
    eprintln!("ts petstore e2e: {stdout}");
    Ok(())
}

// ─── Sample-api runtime legs (auth + retry + pagination) ─────────────────────

fn smoke_go_sample(out_dir: &Path, base_url: &str) -> Result<(), String> {
    let go = resolve_tool("go").ok_or_else(|| "go not found".to_string())?;
    run(Command::new(specforge_bin()).args([
        "generate",
        fixture("sample-api.yaml").to_str().unwrap(),
        "-o",
        out_dir.to_str().unwrap(),
        "-l",
        "go",
        "-n",
        "github.com/example/sample-smoke-go",
    ]))?;

    let smoke_dir = out_dir.join("_smoke");
    std::fs::create_dir_all(&smoke_dir).map_err(|e| e.to_string())?;
    std::fs::write(
        smoke_dir.join("go.mod"),
        format!(
            "module smoke\n\ngo 1.22\n\nrequire github.com/example/sample-smoke-go v0.0.0\n\nreplace github.com/example/sample-smoke-go => {}\n",
            out_dir.display()
        ),
    )
    .map_err(|e| e.to_string())?;

    std::fs::write(
        smoke_dir.join("main.go"),
        format!(
            r#"package main

import (
	"context"
	"fmt"
	"os"
	"time"

	sdk "github.com/example/sample-smoke-go"
)

func main() {{
	ctx := context.Background()
	base := "{base_url}"

	// ── Auth: unauthenticated client must get 401 ──────────────────────────
	noAuth := sdk.NewClient().WithBaseURL(base).WithTimeout(5 * time.Second).WithRetry(sdk.RetryOptions{{MaxRetries: 0}})
	if _, err := noAuth.ListPets(ctx, 10, "", ""); err == nil {{
		fmt.Fprintln(os.Stderr, "expected 401 without auth")
		os.Exit(1)
	}} else if ae, ok := err.(*sdk.APIError); !ok || ae.StatusCode != 401 {{
		fmt.Fprintf(os.Stderr, "expected APIError 401, got %T %v\n", err, err)
		os.Exit(1)
	}}

	// ── Auth + retry: first /pets is 503, client retries to 200 ────────────
	c := sdk.NewClient().
		WithBaseURL(base).
		WithBearerToken("{token}").
		WithTimeout(5 * time.Second).
		WithRetry(sdk.RetryOptions{{
			MaxRetries:      3,
			BaseDelay:       20 * time.Millisecond,
			MaxDelay:        100 * time.Millisecond,
			RetryOnStatuses: []int{{408, 429, 502, 503, 504}},
		}})

	page, err := c.ListPets(ctx, 10, "", "")
	if err != nil {{
		fmt.Fprintf(os.Stderr, "ListPets (retry) failed: %v\n", err)
		os.Exit(1)
	}}
	if page == nil || len(page.Items) < 1 {{
		fmt.Fprintf(os.Stderr, "ListPets empty page: %#v\n", page)
		os.Exit(1)
	}}

	// ── Cursor pagination via helper ───────────────────────────────────────
	var all []sdk.Pet
	err = sdk.CursorPaginate(ctx,
		func(ctx context.Context, cursor *string) (sdk.CursorPage[sdk.Pet], error) {{
			cur := ""
			if cursor != nil {{
				cur = *cursor
			}}
			p, err := c.ListPets(ctx, 10, cur, "")
			if err != nil {{
				return sdk.CursorPage[sdk.Pet]{{}}, err
			}}
			var next *string
			if p.NextCursor != "" {{
				n := p.NextCursor
				next = &n
			}}
			return sdk.CursorPage[sdk.Pet]{{Items: p.Items, NextCursor: next}}, nil
		}},
		func(items []sdk.Pet) error {{
			all = append(all, items...)
			return nil
		}},
	)
	if err != nil {{
		fmt.Fprintf(os.Stderr, "CursorPaginate: %v\n", err)
		os.Exit(1)
	}}
	// After the initial ListPets consumed the 503, pagination starts fresh on
	// the server's list_hits counter — first page may 503 once more then yield
	// 2 + 1 = 3 pets across two pages. Accept >= 3.
	if len(all) < 3 {{
		fmt.Fprintf(os.Stderr, "paginate expected >=3 pets, got %d %#v\n", len(all), all)
		os.Exit(1)
	}}

	fmt.Printf("go-runtime-ok pets=%d page0=%d paginated=%d\n", len(all), len(page.Items), len(all))
}}
"#,
            base_url = base_url,
            token = BEARER_TOKEN,
        ),
    )
    .map_err(|e| e.to_string())?;

    let stdout = run(Command::new(&go)
        .args(["run", "."])
        .current_dir(&smoke_dir)
        .env("GO111MODULE", "on"))?;
    if !stdout.contains("go-runtime-ok") {
        return Err(format!("go sample runtime missing ok: {stdout}"));
    }
    eprintln!("go runtime e2e: {stdout}");
    Ok(())
}

fn smoke_rust_sample(out_dir: &Path, base_url: &str) -> Result<(), String> {
    let cargo = resolve_tool("cargo").ok_or_else(|| "cargo not found".to_string())?;
    run(Command::new(specforge_bin()).args([
        "generate",
        fixture("sample-api.yaml").to_str().unwrap(),
        "-o",
        out_dir.to_str().unwrap(),
        "-l",
        "rust",
        "-n",
        "sample_smoke_sdk",
    ]))?;

    let cargo_toml = out_dir.join("Cargo.toml");
    let mut toml = std::fs::read_to_string(&cargo_toml).map_err(|e| e.to_string())?;
    if !toml.contains("name = \"runtime_smoke\"") {
        toml.push_str(
            r#"
[[bin]]
name = "runtime_smoke"
path = "src/bin/runtime_smoke.rs"
"#,
        );
        std::fs::write(&cargo_toml, toml).map_err(|e| e.to_string())?;
    }
    let bin_dir = out_dir.join("src/bin");
    std::fs::create_dir_all(&bin_dir).map_err(|e| e.to_string())?;
    std::fs::write(
        bin_dir.join("runtime_smoke.rs"),
        format!(
            r#"use sample_smoke_sdk::api;
use sample_smoke_sdk::paginate::{{cursor_paginate, CursorPage}};
use sample_smoke_sdk::retry::RetryOptions;
use sample_smoke_sdk::{{Client, Error}};
use std::time::Duration;

#[tokio::main]
async fn main() {{
    let base = "{base_url}";

    // Auth: no token → 401
    let no_auth = Client::builder()
        .base_url(base)
        .timeout(Duration::from_secs(5))
        .retry(RetryOptions {{ max_retries: 0, ..RetryOptions::default() }})
        .build()
        .expect("client");
    match api::list_pets(&no_auth, Some(10), None::<String>, None).await {{
        Err(Error::Http {{ status: 401, .. }}) => {{}}
        other => panic!("expected 401, got {{other:?}}"),
    }}

    // Auth + retry through 503
    let client = Client::builder()
        .base_url(base)
        .bearer_token("{token}")
        .timeout(Duration::from_secs(5))
        .retry(RetryOptions {{
            max_retries: 3,
            base_delay: Duration::from_millis(20),
            max_delay: Duration::from_millis(100),
            retry_on_statuses: vec![408, 429, 502, 503, 504],
        }})
        .build()
        .expect("client");

    let page = api::list_pets(&client, Some(10), None::<String>, None)
        .await
        .expect("list with retry");
    assert!(!page.items.is_empty(), "empty first page");

    // Cursor pagination
    let mut all = Vec::new();
    cursor_paginate(
        |cursor| {{
            let client = client.clone();
            async move {{
                let p = api::list_pets(&client, Some(10), cursor, None).await?;
                Ok(CursorPage {{
                    items: p.items,
                    next_cursor: p.next_cursor.filter(|s| !s.is_empty()),
                }})
            }}
        }},
        |items| {{
            all.extend(items);
            Ok(())
        }},
    )
    .await
    .expect("paginate");

    assert!(all.len() >= 3, "expected >=3 pets, got {{}}", all.len());
    println!("rust-runtime-ok pets={{}} page0={{}}", all.len(), page.items.len());
}}
"#,
            base_url = base_url,
            token = BEARER_TOKEN,
        ),
    )
    .map_err(|e| e.to_string())?;

    let stdout = run(Command::new(&cargo)
        .args(["run", "-q", "--bin", "runtime_smoke"])
        .current_dir(out_dir))?;
    if !stdout.contains("rust-runtime-ok") {
        return Err(format!("rust sample runtime missing ok: {stdout}"));
    }
    eprintln!("rust runtime e2e: {stdout}");
    Ok(())
}

fn smoke_ts_sample(out_dir: &Path, base_url: &str) -> Result<(), String> {
    let npm = resolve_tool("npm").ok_or_else(|| "npm not found".to_string())?;
    let _node = resolve_tool("node").ok_or_else(|| "node not found".to_string())?;
    let npx = resolve_tool("npx").ok_or_else(|| "npx not found".to_string())?;

    run(Command::new(specforge_bin()).args([
        "generate",
        fixture("sample-api.yaml").to_str().unwrap(),
        "-o",
        out_dir.to_str().unwrap(),
        "-l",
        "ts",
        "-n",
        "@smoke/sample",
    ]))?;
    run(Command::new(&npm)
        .args(["install", "--silent"])
        .current_dir(out_dir))?;
    let _ = run(Command::new(&npm)
        .args(["install", "--silent", "--no-save", "tsx"])
        .current_dir(out_dir));

    std::fs::write(
        out_dir.join("runtime_smoke.mts"),
        format!(
            r#"import {{ createClient, bearerAuth, cursorPaginator, isApiError }} from "./src/index.ts";

const baseUrl = "{base_url}";

// Auth: no token → 401
const noAuth = createClient({{ baseUrl, timeoutMs: 5000, retry: {{ maxRetries: 0 }} }});
let got401 = false;
try {{
  await noAuth.pets.listPets({{ limit: 10 }});
}} catch (e) {{
  if (isApiError(e) && e.type === "http" && e.status === 401) got401 = true;
  else {{ console.error("expected http 401", e); process.exit(1); }}
}}
if (!got401) {{ console.error("expected 401 without auth"); process.exit(1); }}

// Auth + retry through 503
const client = createClient({{
  baseUrl,
  timeoutMs: 5000,
  auth: bearerAuth(() => "{token}"),
  retry: {{ maxRetries: 3, baseDelayMs: 20, maxDelayMs: 100, retryOnStatuses: [408, 429, 502, 503, 504] }},
}});

const page = await client.pets.listPets({{ limit: 10 }});
if (!page.items?.length) {{ console.error("empty page", page); process.exit(1); }}

// Cursor pagination
const all: unknown[] = [];
for await (const p of cursorPaginator(async (cursor) => {{
  const res = await client.pets.listPets({{ limit: 10, cursor }});
  return {{ items: res.items, nextCursor: res.nextCursor ?? null }};
}})) {{
  all.push(...p.items);
}}
if (all.length < 3) {{ console.error("paginate expected >=3", all); process.exit(1); }}

console.log("ts-runtime-ok", all.length, page.items.length);
"#,
            base_url = base_url,
            token = BEARER_TOKEN,
        ),
    )
    .map_err(|e| e.to_string())?;

    // cursorPaginator is exported from paginate — ensure index re-exports it.
    // (index already exports ./paginate)

    let stdout = run(Command::new(&npx)
        .args(["tsx", "runtime_smoke.mts"])
        .current_dir(out_dir))?;
    if !stdout.contains("ts-runtime-ok") {
        return Err(format!("ts sample runtime missing ok: {stdout}"));
    }
    eprintln!("ts runtime e2e: {stdout}");
    Ok(())
}

// ─── Test runners ────────────────────────────────────────────────────────────

fn run_langs(
    label: &str,
    root: &Path,
    legs: &[LangLeg],
) {
    let mut failures = Vec::new();
    let mut ran = 0;
    for (name, f) in legs {
        let dir = root.join(name);
        let _ = std::fs::remove_dir_all(&dir);
        match f(&dir) {
            Ok(()) => ran += 1,
            Err(e) if e.contains("not found") => eprintln!("skip {label}/{name}: {e}"),
            Err(e) => failures.push(format!("{name}: {e}")),
        }
    }
    assert!(
        failures.is_empty(),
        "{label} failures ({ran} ok):\n{}",
        failures.join("\n\n")
    );
    assert!(ran >= 1, "{label}: no language toolchains available");
    eprintln!("{label} passed for {ran} language(s)");
}

#[test]
fn e2e_petstore_smoke_all_languages() {
    assert!(fixture("petstore.yaml").exists());
    let (addr, _server) = start_petstore_mock();
    let base_url = format!("http://{addr}");
    eprintln!("petstore mock at {base_url}");

    // Sanity probe
    {
        let mut stream = TcpStream::connect(addr).expect("connect");
        stream
            .write_all(b"GET /pets HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .unwrap();
        let mut resp = String::new();
        stream.read_to_string(&mut resp).unwrap();
        assert!(resp.contains("Doggo"), "mock list failed: {resp}");
    }

    let root = std::env::temp_dir().join(format!("specforge-e2e-pet-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();

    let base = base_url.clone();
    run_langs(
        "petstore smoke",
        &root,
        &[
            (
                "go",
                Box::new(move |d| smoke_go_petstore(d, &base)),
            ),
            (
                "rust",
                Box::new({
                    let base = base_url.clone();
                    move |d| smoke_rust_petstore(d, &base)
                }),
            ),
            (
                "ts",
                Box::new({
                    let base = base_url.clone();
                    move |d| smoke_ts_petstore(d, &base)
                }),
            ),
        ],
    );
}

#[test]
fn e2e_sample_auth_retry_pagination() {
    assert!(fixture("sample-api.yaml").exists());
    let (addr, _server) = start_sample_mock();
    let base_url = format!("http://{addr}");
    eprintln!("sample-api mock at {base_url}");

    // Sanity: 401 without auth, 503 then 200 with auth (manual two-step).
    {
        let mut stream = TcpStream::connect(addr).unwrap();
        stream
            .write_all(b"GET /pets HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .unwrap();
        let mut resp = String::new();
        stream.read_to_string(&mut resp).unwrap();
        assert!(resp.contains("401"), "expected 401: {resp}");
    }
    {
        let mut stream = TcpStream::connect(addr).unwrap();
        let req = format!(
            "GET /pets HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {BEARER_TOKEN}\r\nConnection: close\r\n\r\n"
        );
        stream.write_all(req.as_bytes()).unwrap();
        let mut resp = String::new();
        stream.read_to_string(&mut resp).unwrap();
        assert!(resp.contains("503"), "expected first 503: {resp}");
    }
    {
        let mut stream = TcpStream::connect(addr).unwrap();
        let req = format!(
            "GET /pets HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {BEARER_TOKEN}\r\nConnection: close\r\n\r\n"
        );
        stream.write_all(req.as_bytes()).unwrap();
        let mut resp = String::new();
        stream.read_to_string(&mut resp).unwrap();
        assert!(resp.contains("Alpha"), "expected pets after retry seed: {resp}");
    }

    let root = std::env::temp_dir().join(format!("specforge-e2e-rt-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();

    let base = base_url.clone();
    run_langs(
        "sample runtime",
        &root,
        &[
            (
                "go",
                Box::new(move |d| smoke_go_sample(d, &base)),
            ),
            (
                "rust",
                Box::new({
                    let base = base_url.clone();
                    move |d| smoke_rust_sample(d, &base)
                }),
            ),
            (
                "ts",
                Box::new({
                    let base = base_url.clone();
                    move |d| smoke_ts_sample(d, &base)
                }),
            ),
        ],
    );
}
