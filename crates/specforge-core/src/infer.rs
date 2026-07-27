//! Reverse schema inference: generate an OpenAPI spec from sample JSON.

use serde_json::Value;

/// Infer a JSON Schema object from a sample JSON value.
///
/// The `name` parameter is used as a hint for nested schema titles but is not
/// emitted into the schema itself -- it only affects debug/tracing output.
pub fn infer_schema(json: &Value, name: &str) -> Value {
    match json {
        Value::Null => serde_json::json!({"type": "string", "nullable": true}),
        Value::Bool(_) => serde_json::json!({"type": "boolean"}),
        Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                serde_json::json!({"type": "integer"})
            } else {
                serde_json::json!({"type": "number"})
            }
        }
        Value::String(s) => infer_string_schema(s),
        Value::Array(arr) => {
            let item = arr
                .first()
                .map(|v| infer_schema(v, &format!("{name}Item")))
                .unwrap_or_else(|| serde_json::json!({"type": "string"}));
            serde_json::json!({"type": "array", "items": item})
        }
        Value::Object(obj) => {
            let mut properties = serde_json::Map::new();
            let mut required = Vec::new();
            for (k, v) in obj {
                properties.insert(k.clone(), infer_schema(v, k));
                required.push(k.clone());
            }
            serde_json::json!({
                "type": "object",
                "properties": properties,
                "required": required
            })
        }
    }
}

/// Infer a schema for a string value, detecting common patterns such as
/// ISO-8601 date-times and UUIDs.
fn infer_string_schema(s: &str) -> Value {
    if s.contains('T') && s.contains('Z') {
        serde_json::json!({"type": "string", "format": "date-time"})
    } else if s.len() == 36 && s.contains('-') {
        serde_json::json!({"type": "string", "format": "uuid"})
    } else {
        serde_json::json!({"type": "string"})
    }
}

/// Options for generating an OpenAPI spec from sample JSON.
pub struct InferOptions {
    /// Schema / model name used in `components.schemas`.
    pub schema_name: String,
    /// API title for the generated spec.
    pub title: String,
    /// API version string.
    pub version: String,
}

impl Default for InferOptions {
    fn default() -> Self {
        Self {
            schema_name: "Inferred".to_string(),
            title: "Inferred API".to_string(),
            version: "1.0.0".to_string(),
        }
    }
}

/// Generate a complete OpenAPI 3.0.3 spec (as a `serde_json::Value`) from a
/// sample JSON request/response body.
///
/// The spec contains:
/// - A single `POST /sample` endpoint whose request body and `200` response
///   share the inferred schema.
/// - The inferred schema placed under `components.schemas`.
pub fn infer_openapi(json: &Value, opts: &InferOptions) -> Value {
    let schema = infer_schema(json, &opts.schema_name);

    let dollar_ref = ["$", "ref"].concat();
    let schema_path = format!("#/components/schemas/{}", opts.schema_name);

    serde_json::json!({
        "openapi": "3.0.3",
        "info": {
            "title": opts.title,
            "version": opts.version,
            "description": "OpenAPI spec inferred from sample JSON by specforge."
        },
        "paths": {
            "/sample": {
                "post": {
                    "operationId": "createSample",
                    "summary": "Create a sample resource",
                    "tags": ["inferred"],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    dollar_ref.clone(): schema_path.clone()
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Successful response",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        dollar_ref.clone(): schema_path.clone()
                                    }
                                }
                            }
                        }
                    }
                }
            }
        },
        "components": {
            "schemas": {
                opts.schema_name.clone(): schema
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn infer_null() {
        let s = infer_schema(&Value::Null, "x");
        assert_eq!(s["type"], "string");
        assert_eq!(s["nullable"], true);
    }

    #[test]
    fn infer_bool() {
        let s = infer_schema(&json!(true), "x");
        assert_eq!(s["type"], "boolean");
    }

    #[test]
    fn infer_integer() {
        let s = infer_schema(&json!(42), "x");
        assert_eq!(s["type"], "integer");
    }

    #[test]
    fn infer_float() {
        let s = infer_schema(&json!(3.14), "x");
        assert_eq!(s["type"], "number");
    }

    #[test]
    fn infer_string() {
        let s = infer_schema(&json!("hello"), "x");
        assert_eq!(s["type"], "string");
    }

    #[test]
    fn infer_datetime() {
        let s = infer_schema(&json!("2024-01-15T10:30:00Z"), "x");
        assert_eq!(s["type"], "string");
        assert_eq!(s["format"], "date-time");
    }

    #[test]
    fn infer_uuid() {
        let s = infer_schema(&json!("550e8400-e29b-41d4-a716-446655440000"), "x");
        assert_eq!(s["type"], "string");
        assert_eq!(s["format"], "uuid");
    }

    #[test]
    fn infer_array() {
        let s = infer_schema(&json!([1, 2, 3]), "x");
        assert_eq!(s["type"], "array");
        assert_eq!(s["items"]["type"], "integer");
    }

    #[test]
    fn infer_empty_array() {
        let s = infer_schema(&json!([]), "x");
        assert_eq!(s["type"], "array");
        assert_eq!(s["items"]["type"], "string");
    }

    #[test]
    fn infer_object() {
        let s = infer_schema(&json!({"name": "Fido", "age": 5}), "x");
        assert_eq!(s["type"], "object");
        assert_eq!(s["properties"]["name"]["type"], "string");
        assert_eq!(s["properties"]["age"]["type"], "integer");
        let required = s["required"].as_array().unwrap();
        assert!(required.contains(&json!("name")));
        assert!(required.contains(&json!("age")));
    }

    #[test]
    fn infer_nested_object() {
        let s = infer_schema(&json!({"user": {"name": "Alice", "email": "a@b.com"}}), "x");
        assert_eq!(s["type"], "object");
        assert_eq!(s["properties"]["user"]["type"], "object");
        assert_eq!(s["properties"]["user"]["properties"]["name"]["type"], "string");
    }

    #[test]
    fn infer_openapi_spec() {
        let sample = json!({"id": 1, "name": "Widget", "created_at": "2024-01-15T10:30:00Z"});
        let opts = InferOptions::default();
        let spec = infer_openapi(&sample, &opts);

        assert_eq!(spec["openapi"], "3.0.3");
        assert_eq!(spec["info"]["title"], "Inferred API");
        assert!(spec["paths"]["/sample"]["post"].is_object());
        assert!(spec["components"]["schemas"]["Inferred"].is_object());
    }
}
