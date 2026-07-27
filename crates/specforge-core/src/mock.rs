//! Mock HTTP server that serves example responses from an OpenAPI spec's IR.
//!
//! Useful for local development and testing without a real backend.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::thread;

use crate::ir::{Document, Model, ObjectModel, Operation, Scalar, Type};

/// A single route that the mock server will respond to.
#[derive(Debug, Clone)]
pub struct Route {
    pub method: String,
    pub path: String,
    pub status: u16,
    pub body: String,
}

/// A running mock server.
pub struct MockServer {
    pub port: u16,
    host: String,
    routes: Vec<Route>,
    listener: Option<TcpListener>,
    title: String,
    version: String,
}

impl MockServer {
    /// Build a mock server from a resolved IR `Document`.
    ///
    /// Each operation becomes a route that responds with a generated example JSON
    /// body matching the response schema type.
    pub fn from_doc(doc: &Document) -> Self {
        let mut routes = Vec::new();
        for op in &doc.operations {
            let method = op.method.upper().to_string();
            let path = op.path.clone();

            // Pick the first success response (2xx) to generate an example body.
            let (status, body) = pick_success_response(doc, op);

            routes.push(Route {
                method,
                path,
                status,
                body,
            });
        }
        Self {
            port: 0,
            host: "127.0.0.1".to_string(),
            routes,
            listener: None,
            title: doc.title.clone(),
            version: doc.version.clone(),
        }
    }

    /// Set the host to bind to.
    pub fn host(mut self, host: &str) -> Self {
        self.host = host.to_string();
        self
    }

    /// Set the port (0 = random available port).
    pub fn port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    /// Bind and start the mock server. Returns the actual port.
    pub fn start(&mut self) -> Result<u16, std::io::Error> {
        let addr = format!("{}:{}", self.host, self.port);
        let listener = TcpListener::bind(&addr)?;
        self.port = listener.local_addr()?.port();
        self.listener = Some(listener);

        Ok(self.port)
    }

    /// Accept connections in a loop (blocking). Call this after `start()`.
    ///
    /// To run this in the background, wrap it in a `std::thread::spawn`.
    pub fn serve(&self) {
        let routes: Arc<Vec<Route>> = Arc::new(self.routes.clone());
        let title = self.title.clone();
        let version = self.version.clone();

        let listener = match &self.listener {
            Some(l) => l,
            None => return,
        };

        // Make listener non-blocking so we can check for shutdown.
        listener.set_nonblocking(false).ok();

        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let routes = Arc::clone(&routes);
                    let title = title.clone();
                    let version = version.clone();
                    thread::spawn(move || {
                        handle_connection(stream, &routes, &title, &version);
                    });
                }
                Err(e) => {
                    eprintln!("error accepting connection: {e}");
                }
            }
        }
    }
}

/// Parse an HTTP request and return (method, path).
fn parse_request_line(
    reader: &mut BufReader<&mut TcpStream>,
) -> Option<(String, String)> {
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    let mut parts = line.trim().split_whitespace();
    let method = parts.next()?.to_uppercase();
    let path = parts.next()?.to_string();
    Some((method, path))
}

/// Drain remaining headers (we don't need them for the mock).
fn drain_headers(reader: &mut BufReader<&mut TcpStream>) {
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line).is_err() {
            break;
        }
        if line.trim().is_empty() {
            break;
        }
    }
}

fn handle_connection(
    mut stream: TcpStream,
    routes: &[Route],
    title: &str,
    version: &str,
) {
    let mut reader = BufReader::new(&mut stream);

    let (method, path) = match parse_request_line(&mut reader) {
        Some(v) => v,
        None => return,
    };

    drain_headers(&mut reader);

    // Match route.
    let response = routes
        .iter()
        .find(|r| r.method == method && r.path == path);

    let (status_line, body, content_type) = match response {
        Some(route) => {
            let status_line = match route.status {
                200 => "HTTP/1.1 200 OK",
                201 => "HTTP/1.1 201 Created",
                204 => "HTTP/1.1 204 No Content",
                400 => "HTTP/1.1 400 Bad Request",
                404 => "HTTP/1.1 404 Not Found",
                500 => "HTTP/1.1 500 Internal Server Error",
                s => {
                    let _ = s;
                    "HTTP/1.1 200 OK"
                }
            };
            (status_line, route.body.clone(), "application/json")
        }
        None => (
            "HTTP/1.1 404 Not Found",
            serde_json::json!({
                "error": "not_found",
                "message": format!("No route found for {method} {path}"),
            })
            .to_string(),
            "application/json",
        ),
    };

    let body_bytes = body.as_bytes();
    let response = format!(
        "{status_line}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {}\r\n\
         X-Mock-Server: specforge {version}\r\n\
         X-Mock-Title: {title}\r\n\
         Connection: close\r\n\
         \r\n\
         {body}",
        body_bytes.len()
    );

    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

/// Pick the first 2xx success response from an operation and generate an
/// example body for it.
fn pick_success_response(doc: &Document, op: &Operation) -> (u16, String) {
    for resp in &op.responses {
        if let Ok(status) = resp.status.parse::<u16>() {
            if (200..300).contains(&status) {
                if let Some(ref body_type) = resp.body {
                    let json = generate_example_json(doc, body_type);
                    let body = serde_json::to_string_pretty(&json)
                        .unwrap_or_else(|_| "{}".to_string());
                    return (status, body);
                }
                // No body (e.g. 204).
                return (status, "{}".to_string());
            }
        }
    }

    // Fallback: return 200 with empty JSON object.
    (200, "{}".to_string())
}

/// Recursively generate an example `serde_json::Value` from an IR `Type`.
fn generate_example_json(doc: &Document, ty: &Type) -> serde_json::Value {
    match ty {
        Type::Scalar(s) => scalar_example(*s),
        Type::StringEnum { variants, .. } => {
            variants.first().map(|v| serde_json::Value::String(v.clone()))
                .unwrap_or(serde_json::Value::String("value".to_string()))
        }
        Type::Array { item, .. } => {
            let element = generate_example_json(doc, item);
            serde_json::Value::Array(vec![element])
        }
        Type::Map { value } => {
            let inner = generate_example_json(doc, value);
            let mut map = serde_json::Map::new();
            map.insert("key".to_string(), inner);
            serde_json::Value::Object(map)
        }
        Type::Reference { name, .. } => {
            // Resolve the reference from the schema registry.
            if let Some(model) = doc.schemas.get(name) {
                generate_example_from_model(doc, model)
            } else {
                serde_json::Value::Null
            }
        }
        Type::Composition(comp) => {
            // Use the first member for the example.
            comp.members
                .first()
                .map(|m| generate_example_json(doc, m))
                .unwrap_or(serde_json::Value::Null)
        }
        Type::Any => serde_json::json!({}),
        Type::Unknown => serde_json::Value::Null,
    }
}

/// Generate an example JSON value from a resolved model (object or enum).
fn generate_example_from_model(doc: &Document, model: &Model) -> serde_json::Value {
    match model {
        Model::Object(obj) => object_example(doc, obj),
        Model::Enum(e) => {
            e.variants
                .first()
                .map(|v| serde_json::Value::String(v.value.clone()))
                .unwrap_or(serde_json::Value::String("value".to_string()))
        }
    }
}

/// Generate a JSON object from an object model's properties.
fn object_example(doc: &Document, obj: &ObjectModel) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for prop in &obj.properties {
        let value = generate_example_json(doc, &prop.ty);
        map.insert(prop.name.clone(), value);
    }
    serde_json::Value::Object(map)
}

/// Generate a sensible scalar example value.
fn scalar_example(s: Scalar) -> serde_json::Value {
    match s {
        Scalar::String => serde_json::Value::String("string".to_string()),
        Scalar::DateTime => serde_json::Value::String("2024-01-01T00:00:00Z".to_string()),
        Scalar::Uuid => serde_json::Value::String(
            "00000000-0000-0000-0000-000000000000".to_string(),
        ),
        Scalar::Integer => serde_json::json!(0),
        Scalar::Integer64 => serde_json::json!(0),
        Scalar::Float => serde_json::json!(0.0),
        Scalar::Boolean => serde_json::Value::Bool(false),
        Scalar::Base64 => serde_json::Value::String("c2FtcGxl".to_string()),
        Scalar::Binary => serde_json::Value::String("binary".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalar_examples() {
        assert_eq!(scalar_example(Scalar::String), serde_json::json!("string"));
        assert_eq!(scalar_example(Scalar::Integer), serde_json::json!(0));
        assert_eq!(scalar_example(Scalar::Boolean), serde_json::json!(false));
        assert!(scalar_example(Scalar::Uuid).as_str().is_some());
        assert!(scalar_example(Scalar::DateTime).as_str().is_some());
    }

    #[test]
    fn string_enum_picks_first_variant() {
        let ty = Type::StringEnum {
            variants: vec!["active".into(), "inactive".into()],
            nullable: false,
        };
        let val = generate_example_json(&empty_doc(), &ty);
        assert_eq!(val, serde_json::json!("active"));
    }

    #[test]
    fn array_example() {
        let ty = Type::Array {
            item: Box::new(Type::Scalar(Scalar::Integer)),
            nullable: false,
        };
        let val = generate_example_json(&empty_doc(), &ty);
        assert!(val.is_array());
        assert_eq!(val.as_array().unwrap().len(), 1);
    }

    fn empty_doc() -> Document {
        Document {
            title: String::new(),
            version: String::new(),
            base_url: None,
            security: vec![],
            schemas: crate::ir::SchemaRegistry {
                models: indexmap::IndexMap::new(),
            },
            operations: vec![],
            webhooks: vec![],
        }
    }
}
