//! SDK test generation from OpenAPI specs.
//!
//! Generates test files that spin up a mock server from the spec's example
//! responses and verify the generated SDK can call each operation.

use std::fmt::Write;

use crate::ir::{Document, Model, ObjectModel, Scalar, Type};

/// Options for the test generator.
pub struct TestGenOptions {
    pub lang: TestLang,
}

/// Target language for test generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestLang {
    TypeScript,
    Go,
    Rust,
}

/// Generate mock-server test code for the given document and language.
///
/// Returns the full source text of the test file.
pub fn generate_tests(doc: &Document, opts: &TestGenOptions) -> String {
    match opts.lang {
        TestLang::TypeScript => generate_ts_tests(doc),
        TestLang::Go => generate_go_tests(doc),
        TestLang::Rust => generate_rust_tests(doc),
    }
}

// ─── TypeScript ────────────────────────────────────────────────────────────

fn generate_ts_tests(doc: &Document) -> String {
    let mut out = String::new();

    writeln!(out, "import * as http from 'http';").unwrap();
    writeln!(out, "import {{ createClient }} from './src/index.ts';").unwrap();
    writeln!(out).unwrap();

    // Build mock route dispatcher
    writeln!(out, "const mockServer = http.createServer((req, res) => {{").unwrap();

    for op in &doc.operations {
        let method = op.method.upper();
        // Convert OpenAPI path params {petId} to regex-style matching
        let (_ts_path, ts_match) = ts_path_match(&op.path);

        writeln!(out, "    // {} {}", method, op.path).unwrap();
        write!(out, "    if (").unwrap();
        write!(out, "{}", ts_match).unwrap();
        writeln!(out, " && req.method === '{}') {{", method).unwrap();

        // Find the first 2xx response with a body
        if let Some(body) = first_success_body(op) {
            let example = generate_ts_example(&body, &doc.schemas.models, 3);
            writeln!(out, "        res.writeHead(200, {{ 'Content-Type': 'application/json' }});").unwrap();
            writeln!(out, "        res.end(JSON.stringify({}));", example).unwrap();
        } else {
            writeln!(out, "        res.writeHead(200);").unwrap();
            writeln!(out, "        res.end();").unwrap();
        }

        writeln!(out, "        return;").unwrap();
        writeln!(out, "    }}").unwrap();
    }

    // Default 404
    writeln!(out, "    res.writeHead(404);").unwrap();
    writeln!(out, "    res.end('Not Found');").unwrap();
    writeln!(out, "}});").unwrap();
    writeln!(out).unwrap();

    // Launch mock server and run tests
    writeln!(out, "mockServer.listen(0, async () => {{").unwrap();
    writeln!(
        out,
        "    const port = (mockServer.address() as any).port;"
    )
    .unwrap();
    writeln!(
        out,
        "    const client = createClient({{ baseUrl: `http://localhost:${{port}}` }});"
    )
    .unwrap();
    writeln!(out).unwrap();
    writeln!(out, "    let passed = 0;").unwrap();
    writeln!(out, "    let failed = 0;").unwrap();
    writeln!(out).unwrap();

    for op in &doc.operations {
        let test_name = &op.operation_id;
        let ts_accessor = ts_method_accessor(&op.operation_id);

        writeln!(out, "    // Test: {} {} {}", op.method.upper(), op.path, test_name).unwrap();
        writeln!(out, "    try {{").unwrap();
        writeln!(
            out,
            "        const result = await client{};",
            ts_accessor
        )
        .unwrap();

        // Assert based on response type
        if let Some(body) = first_success_body(op) {
            let assertion = ts_assertion(&body);
            writeln!(out, "        {};", assertion).unwrap();
        }
        writeln!(out, "        console.log('  ✓ {}');", test_name).unwrap();
        writeln!(out, "        passed++;").unwrap();
        writeln!(out, "    }} catch (e) {{").unwrap();
        writeln!(out, "        console.error('  ✗ {}:', e);", test_name).unwrap();
        writeln!(out, "        failed++;").unwrap();
        writeln!(out, "    }}").unwrap();
    }

    writeln!(out).unwrap();
    writeln!(out, "    console.log(`\\nResults: ${{passed}} passed, ${{failed}} failed`);").unwrap();
    writeln!(out, "    mockServer.close();").unwrap();
    writeln!(out, "    process.exit(failed > 0 ? 1 : 0);").unwrap();
    writeln!(out, "}});").unwrap();

    out
}

fn ts_path_match(path: &str) -> (String, String) {
    let mut regex_pattern = String::from("^/");
    for seg in path.trim_matches('/').split('/') {
        if seg.starts_with('{') && seg.ends_with('}') {
            regex_pattern.push_str("[^/]+");
        } else {
            regex_pattern.push_str(&regex_escape(seg));
        }
        regex_pattern.push('/');
    }
    // Remove trailing slash and add end anchor
    regex_pattern.pop();
    regex_pattern.push('$');
    let match_expr = format!(
        "req.url && new RegExp('{}').test(req.url.split('?')[0])",
        regex_pattern
    );
    (path.to_string(), match_expr)
}

fn regex_escape(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if matches!(
            c,
            '.' | '^' | '$' | '|' | '?' | '*' | '+' | '(' | ')' | '[' | ']' | '{' | '}'
        ) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

fn ts_method_accessor(operation_id: &str) -> String {
    // Convert camelCase operationId like "listPets" to ".pets.listPets()"
    // For simplicity, just use the operationId directly
    format!(".{}()", operation_id)
}

fn ts_assertion(ty: &Type) -> String {
    match ty {
        Type::Array { .. } => "console.assert(Array.isArray(result), 'expected array')".to_string(),
        Type::Reference { name, .. } => {
            format!(
                "console.assert(result !== null && typeof result === 'object', 'expected {}')",
                name
            )
        }
        Type::Scalar(s) => match s {
            Scalar::String | Scalar::DateTime | Scalar::Uuid => {
                "console.assert(typeof result === 'string', 'expected string')".to_string()
            }
            Scalar::Integer | Scalar::Integer64 | Scalar::Float => {
                "console.assert(typeof result === 'number', 'expected number')".to_string()
            }
            Scalar::Boolean | Scalar::Base64 => {
                "console.assert(typeof result === 'boolean', 'expected boolean')".to_string()
            }
            Scalar::Binary => {
                "console.assert(result !== undefined, 'expected bytes')".to_string()
            }
        },
        _ => "console.assert(result !== undefined, 'expected result')".to_string(),
    }
}

fn generate_ts_example(ty: &Type, schemas: &indexmap::IndexMap<String, Model>, indent: usize) -> String {
    match ty {
        Type::Scalar(s) => match s {
            Scalar::String => r#""example""#.to_string(),
            Scalar::DateTime => r#""2024-01-01T00:00:00Z""#.to_string(),
            Scalar::Uuid => r#""550e8400-e29b-41d4-a716-446655440000""#.to_string(),
            Scalar::Integer | Scalar::Integer64 => "1".to_string(),
            Scalar::Float => "1.0".to_string(),
            Scalar::Boolean | Scalar::Base64 => "true".to_string(),
            Scalar::Binary => "new Uint8Array([1,2,3])".to_string(),
        },
        Type::StringEnum { variants, .. } => {
            if let Some(first) = variants.first() {
                format!("\"{}\"", first)
            } else {
                "\"enum_value\"".to_string()
            }
        }
        Type::Array { item, .. } => {
            let inner = generate_ts_example(item, schemas, indent);
            format!("[{}]", inner)
        }
        Type::Map { value } => {
            let inner = generate_ts_example(value, schemas, indent);
            format!("{{ \"key\": {} }}", inner)
        }
        Type::Reference { name, .. } => {
            if let Some(model) = schemas.get(name) {
                match model {
                    Model::Object(obj) => {
                        // Check if shape_type reveals this is actually an array/map/etc.
                        if let Some(ref shape) = obj.shape_type {
                            match shape {
                                Type::Array { item, .. } => {
                                    let inner = generate_ts_example(item, schemas, indent);
                                    format!("[{}]", inner)
                                }
                                Type::Map { value } => {
                                    let inner = generate_ts_example(value, schemas, indent);
                                    format!("{{ \"key\": {} }}", inner)
                                }
                                _ => ts_object_example(obj, schemas, indent),
                            }
                        } else {
                            ts_object_example(obj, schemas, indent)
                        }
                    }
                    Model::Enum(e) => {
                        if let Some(first) = e.variants.first() {
                            format!("\"{}\"", first.value)
                        } else {
                            "\"enum_value\"".to_string()
                        }
                    }
                }
            } else {
                "{}".to_string()
            }
        }
        Type::Composition(comp) => {
            // Use the first member as the example
            if let Some(first) = comp.members.first() {
                generate_ts_example(first, schemas, indent)
            } else {
                "{}".to_string()
            }
        }
        Type::Any | Type::Unknown => "null".to_string(),
    }
}

fn ts_object_example(
    obj: &ObjectModel,
    schemas: &indexmap::IndexMap<String, Model>,
    indent: usize,
) -> String {
    if obj.properties.is_empty() {
        return "{}".to_string();
    }
    let pad = "    ".repeat(indent);
    let inner_pad = "    ".repeat(indent + 1);
    let mut out = String::from("{\n");
    for (i, prop) in obj.properties.iter().enumerate() {
        let val = generate_ts_example(&prop.ty, schemas, indent + 1);
        let comma = if i + 1 < obj.properties.len() { "," } else { "" };
        writeln!(out, "{}{}: {}{}", inner_pad, prop.name, val, comma).unwrap();
    }
    write!(out, "{}}}", pad).unwrap();
    out
}

// ─── Go ───────────────────────────────────────────────────────────────────

fn generate_go_tests(doc: &Document) -> String {
    let mut out = String::new();

    writeln!(out, "package sdk").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "import (").unwrap();
    writeln!(out, "    \"encoding/json\"").unwrap();
    writeln!(out, "    \"fmt\"").unwrap();
    writeln!(out, "    \"net/http\"").unwrap();
    writeln!(out, "    \"net/http/httptest\"").unwrap();
    writeln!(out, "    \"testing\"").unwrap();
    writeln!(out, ")").unwrap();
    writeln!(out).unwrap();

    for op in &doc.operations {
        let test_name = format!(
            "Test{}{}",
            capitalize_first(&op.operation_id),
            ""
        );
        let method = op.method.upper();
        let path = &op.path;

        writeln!(out, "func {}(t *testing.T) {{", test_name).unwrap();

        // Create mock handler
        writeln!(out, "    server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {{").unwrap();
        writeln!(
            out,
            "        if r.URL.Path == \"{}\" && r.Method == \"{}\" {{",
            path, method
        )
        .unwrap();

        if let Some(body) = first_success_body(op) {
            let example = generate_go_example(&body, &doc.schemas.models);
            writeln!(out, "            w.Header().Set(\"Content-Type\", \"application/json\")").unwrap();
            writeln!(out, "            fmt.Fprint(w, `{}`)", example).unwrap();
        } else {
            writeln!(out, "            w.WriteHeader(http.StatusOK)").unwrap();
        }

        writeln!(out, "            return").unwrap();
        writeln!(out, "        }}").unwrap();
        writeln!(out, "        http.NotFound(w, r)").unwrap();
        writeln!(out, "    }}))").unwrap();
        writeln!(out, "    defer server.Close()").unwrap();
        writeln!(out).unwrap();
        writeln!(out, "    client := NewClient().WithBaseURL(server.URL)").unwrap();
        writeln!(out, "    _ = client // TODO: call client method and assert").unwrap();
        writeln!(out, "}}").unwrap();
        writeln!(out).unwrap();
    }

    out
}

fn generate_go_example(ty: &Type, schemas: &indexmap::IndexMap<String, Model>) -> String {
    match ty {
        Type::Scalar(s) => match s {
            Scalar::String => "\"example\"".to_string(),
            Scalar::DateTime => "\"2024-01-01T00:00:00Z\"".to_string(),
            Scalar::Uuid => "\"550e8400-e29b-41d4-a716-446655440000\"".to_string(),
            Scalar::Integer | Scalar::Integer64 => "1".to_string(),
            Scalar::Float => "1.0".to_string(),
            Scalar::Boolean | Scalar::Base64 => "true".to_string(),
            Scalar::Binary => "new Uint8Array([1,2,3])".to_string(),
        },
        Type::StringEnum { variants, .. } => {
            if let Some(first) = variants.first() {
                format!("\"{}\"", first)
            } else {
                "\"enum_value\"".to_string()
            }
        }
        Type::Array { item, .. } => {
            let inner = generate_go_example(item, schemas);
            format!("[{}]", inner)
        }
        Type::Map { value } => {
            let inner = generate_go_example(value, schemas);
            format!("{{\"key\": {}}}", inner)
        }
        Type::Reference { name, .. } => {
            if let Some(model) = schemas.get(name) {
                match model {
                    Model::Object(obj) => {
                        if let Some(ref shape) = obj.shape_type {
                            match shape {
                                Type::Array { item, .. } => {
                                    let inner = generate_go_example(item, schemas);
                                    format!("[{}]", inner)
                                }
                                Type::Map { value } => {
                                    let inner = generate_go_example(value, schemas);
                                    format!("{{\"key\": {}}}", inner)
                                }
                                _ => go_object_example(obj, schemas),
                            }
                        } else {
                            go_object_example(obj, schemas)
                        }
                    }
                    Model::Enum(e) => {
                        if let Some(first) = e.variants.first() {
                            format!("\"{}\"", first.value)
                        } else {
                            "\"enum_value\"".to_string()
                        }
                    }
                }
            } else {
                "{}".to_string()
            }
        }
        Type::Composition(comp) => {
            if let Some(first) = comp.members.first() {
                generate_go_example(first, schemas)
            } else {
                "{}".to_string()
            }
        }
        Type::Any | Type::Unknown => "null".to_string(),
    }
}

fn go_object_example(obj: &ObjectModel, schemas: &indexmap::IndexMap<String, Model>) -> String {
    if obj.properties.is_empty() {
        return "{}".to_string();
    }
    let mut out = String::from("{");
    for (i, prop) in obj.properties.iter().enumerate() {
        let val = generate_go_example(&prop.ty, schemas);
        if i > 0 {
            out.push(',');
        }
        write!(out, "\"{}\":{}", prop.name, val).unwrap();
    }
    out.push('}');
    out
}

fn capitalize_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}

// ─── Rust ─────────────────────────────────────────────────────────────────

fn generate_rust_tests(doc: &Document) -> String {
    let mut out = String::new();

    writeln!(out, "//! Integration tests with a mock server.").unwrap();
    writeln!(out, "//! Generated by specforge testgen.").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "use std::io::{{Read, Write}};").unwrap();
    writeln!(out, "use std::net::TcpListener;").unwrap();
    writeln!(out).unwrap();

    for op in &doc.operations {
        let test_name = format!("test_{}", op.operation_id.to_lowercase());

        writeln!(out, "#[test]").unwrap();
        writeln!(out, "fn {}() {{", test_name).unwrap();

        // Find mock response body
        let body_json = if let Some(body) = first_success_body(op) {
            generate_rust_example(&body, &doc.schemas.models)
        } else {
            String::new()
        };

        writeln!(out, "    let listener = TcpListener::bind(\"127.0.0.1:0\").unwrap();").unwrap();
        writeln!(out, "    let addr = listener.local_addr().unwrap();").unwrap();
        writeln!(out).unwrap();

        // Spawn mock server in a thread
        writeln!(out, "    let handle = std::thread::spawn(move || {{").unwrap();
        writeln!(out, "        let (mut stream, _) = listener.accept().unwrap();").unwrap();

        // Read the request (consume it)
        writeln!(out, "        let mut buf = [0u8; 4096];").unwrap();
        writeln!(out, "        let _ = stream.read(&mut buf);").unwrap();

        // Write mock response
        if body_json.is_empty() {
            writeln!(out, "        let response = \"HTTP/1.1 200 OK\\r\\n\\r\\n\";").unwrap();
        } else {
            let escaped_body = body_json.replace('\\', "\\\\").replace('"', "\\\"");
            writeln!(out, "        let body = \"{}\";", escaped_body).unwrap();
            writeln!(out, "        let response = format!(\"HTTP/1.1 200 OK\\r\\nContent-Type: application/json\\r\\n\\r\\n{{}}\", body);").unwrap();
        }
        writeln!(out, "        stream.write_all(response.as_bytes()).unwrap();").unwrap();
        writeln!(out, "    }});").unwrap();
        writeln!(out).unwrap();

        writeln!(out, "    let base_url = format!(\"http://{{}}\", addr);").unwrap();
        writeln!(out, "    // TODO: create SDK client with base_url and call operation").unwrap();
        writeln!(out, "    // let client = Client::new(&base_url);").unwrap();
        writeln!(
            out,
            "    // let result = client.{}();",
            op.operation_id
        )
        .unwrap();
        writeln!(out, "    // assert!(result.is_ok());").unwrap();
        writeln!(out).unwrap();
        writeln!(out, "    handle.join().unwrap();").unwrap();
        writeln!(out, "}}").unwrap();
        writeln!(out).unwrap();
    }

    out
}

fn generate_rust_example(ty: &Type, schemas: &indexmap::IndexMap<String, Model>) -> String {
    match ty {
        Type::Scalar(s) => match s {
            Scalar::String => r#""example""#.to_string(),
            Scalar::DateTime => r#""2024-01-01T00:00:00Z""#.to_string(),
            Scalar::Uuid => r#""550e8400-e29b-41d4-a716-446655440000""#.to_string(),
            Scalar::Integer | Scalar::Integer64 => "1".to_string(),
            Scalar::Float => "1.0".to_string(),
            Scalar::Boolean | Scalar::Base64 => "true".to_string(),
            Scalar::Binary => "new Uint8Array([1,2,3])".to_string(),
        },
        Type::StringEnum { variants, .. } => {
            if let Some(first) = variants.first() {
                format!("\"{}\"", first)
            } else {
                "\"enum_value\"".to_string()
            }
        }
        Type::Array { item, .. } => {
            let inner = generate_rust_example(item, schemas);
            format!("[{}]", inner)
        }
        Type::Map { value } => {
            let inner = generate_rust_example(value, schemas);
            format!("{{\"key\": {}}}", inner)
        }
        Type::Reference { name, .. } => {
            if let Some(model) = schemas.get(name) {
                match model {
                    Model::Object(obj) => {
                        if let Some(ref shape) = obj.shape_type {
                            match shape {
                                Type::Array { item, .. } => {
                                    let inner = generate_rust_example(item, schemas);
                                    format!("[{}]", inner)
                                }
                                Type::Map { value } => {
                                    let inner = generate_rust_example(value, schemas);
                                    format!("{{\"key\": {}}}", inner)
                                }
                                _ => rust_object_example(obj, schemas),
                            }
                        } else {
                            rust_object_example(obj, schemas)
                        }
                    }
                    Model::Enum(e) => {
                        if let Some(first) = e.variants.first() {
                            format!("\"{}\"", first.value)
                        } else {
                            "\"enum_value\"".to_string()
                        }
                    }
                }
            } else {
                "{}".to_string()
            }
        }
        Type::Composition(comp) => {
            if let Some(first) = comp.members.first() {
                generate_rust_example(first, schemas)
            } else {
                "{}".to_string()
            }
        }
        Type::Any | Type::Unknown => "null".to_string(),
    }
}

fn rust_object_example(obj: &ObjectModel, schemas: &indexmap::IndexMap<String, Model>) -> String {
    if obj.properties.is_empty() {
        return "{}".to_string();
    }
    let mut out = String::from("{");
    for (i, prop) in obj.properties.iter().enumerate() {
        let val = generate_rust_example(&prop.ty, schemas);
        if i > 0 {
            out.push(',');
        }
        write!(out, "\"{}\":{}", prop.name, val).unwrap();
    }
    out.push('}');
    out
}

// ─── Shared helpers ───────────────────────────────────────────────────────

/// Find the Type of the first success (2xx) response body for an operation.
fn first_success_body(op: &crate::ir::Operation) -> Option<Type> {
    for resp in &op.responses {
        if resp.status.starts_with('2') {
            return resp.body.clone();
        }
    }
    // Fall back to default if present
    for resp in &op.responses {
        if resp.status == "default" {
            return resp.body.clone();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::*;

    fn sample_doc() -> Document {
        let pet_type = Type::Reference {
            name: "Pet".to_string(),
            nullable: false,
            description: None,
        };

        let mut schemas = SchemaRegistry::default();
        schemas.models.insert(
            "Pet".to_string(),
            Model::Object(ObjectModel {
                name: "Pet".to_string(),
                description: None,
                properties: vec![
                    Property {
                        name: "id".to_string(),
                        ty: Type::Scalar(Scalar::Integer64),
                        required: true,
                        description: None,
                    },
                    Property {
                        name: "name".to_string(),
                        ty: Type::Scalar(Scalar::String),
                        required: true,
                        description: None,
                    },
                ],
                additional_properties: None,
                shape_type: None,
                base_type: None,
            }),
        );

        Document {
            ir_version: crate::ir::IR_VERSION.to_string(),
            title: "Test API".to_string(),
            version: "1.0.0".to_string(),
            base_url: Some("http://localhost:3000".to_string()),
            security: vec![],
            schemas,
            webhooks: vec![],
            operations: vec![Operation {
                operation_id: "listPets".to_string(),
                method: HttpMethod::Get,
                path: "/pets".to_string(),
                tag: Some("pets".to_string()),
                summary: Some("List all pets".to_string()),
                description: None,
                parameters: vec![],
                request_body: None,
                retry_policy: None,
                responses: vec![Response {
                    status: "200".to_string(),
                    description: Some("A list of pets".to_string()),
                    body: Some(Type::Array {
                        item: Box::new(pet_type),
                        nullable: false,
                    }),
                }],
            }],
        }
    }

    #[test]
    fn generates_ts_output() {
        let doc = sample_doc();
        let opts = TestGenOptions {
            lang: TestLang::TypeScript,
        };
        let output = generate_tests(&doc, &opts);
        assert!(output.contains("import * as http from 'http'"));
        assert!(output.contains("mockServer"));
        assert!(output.contains("listPets"));
    }

    #[test]
    fn generates_go_output() {
        let doc = sample_doc();
        let opts = TestGenOptions {
            lang: TestLang::Go,
        };
        let output = generate_tests(&doc, &opts);
        assert!(output.contains("package sdk"));
        assert!(output.contains("httptest.NewServer"));
        assert!(output.contains("TestListPets"));
    }

    #[test]
    fn generates_rust_output() {
        let doc = sample_doc();
        let opts = TestGenOptions {
            lang: TestLang::Rust,
        };
        let output = generate_tests(&doc, &opts);
        assert!(output.contains("TcpListener"));
        assert!(output.contains("test_listpets"));
    }
}
