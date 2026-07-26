//! Parsing raw OpenAPI YAML/JSON into `openapiv3` types.
//!
//! JSON and YAML are both supported; detection is content-based (parse-fallback)
//! rather than by extension so callers don't have to guess.
//!
//! OpenAPI 3.1 specs are transparently downgraded to 3.0 before parsing so the
//! `openapiv3` crate (which only supports 3.0) can consume them. The key
//! conversions are: `type: ["X", "null"]` → `type: X` + `nullable: true`, and
//! numeric `exclusive_minimum`/`exclusive_maximum` → boolean.

use std::path::Path;

use openapiv3::OpenAPI;
use serde_json::Value as JsonValue;

use crate::error::SpecError;

/// Read and parse an OpenAPI document from a file path (JSON or YAML).
pub fn parse_file(path: impl AsRef<Path>) -> Result<OpenAPI, SpecError> {
    let path = path.as_ref();
    let bytes = std::fs::read(path).map_err(|source| SpecError::Io {
        path: path.display().to_string(),
        source,
    })?;
    parse_bytes(&bytes)
}

/// Parse an OpenAPI document from bytes (JSON or YAML, auto-detected).
pub fn parse_bytes(bytes: &[u8]) -> Result<OpenAPI, SpecError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|e| SpecError::Invalid(format!("spec is not valid UTF-8: {e}")))?;
    parse_str(text)
}

/// Parse an OpenAPI document from a string (JSON or YAML, auto-detected).
pub fn parse_str(text: &str) -> Result<OpenAPI, SpecError> {
    // JSON parses faster and stricter; try it first, fall back to YAML.
    let mut json: JsonValue = if let Ok(v) = serde_json::from_str::<JsonValue>(text.trim_start())
    {
        v
    } else {
        serde_yaml::from_str::<JsonValue>(text).map_err(SpecError::Yaml)?
    };

    // Downgrade 3.1 → 3.0 if needed.
    if is_31(&json) {
        downgrade_31_to_30(&mut json);
    }

    serde_json::from_value::<OpenAPI>(json).map_err(SpecError::Json)
}

/// Check if the spec declares OpenAPI 3.1.x.
fn is_31(json: &JsonValue) -> bool {
    json.get("openapi")
        .and_then(|v| v.as_str())
        .map(|v| v.starts_with("3.1"))
        .unwrap_or(false)
}

/// Recursively downgrade OpenAPI 3.1 constructs to 3.0 equivalents.
fn downgrade_31_to_30(json: &mut JsonValue) {
    // Ensure `paths` exists (optional in 3.1, required in 3.0).
    if json.get("paths").is_none() {
        if let Some(obj) = json.as_object_mut() {
            obj.insert("paths".to_string(), JsonValue::Object(Default::default()));
        }
    }
    downgrade_value(json);
}

/// Recursively walk and transform a JSON value.
fn downgrade_value(json: &mut JsonValue) {
    match json {
        JsonValue::Object(obj) => {
            // Convert `type: ["X", "null"]` → `type: X` + `nullable: true`.
            if let Some(type_val) = obj.get("type").cloned() {
                if let Some(arr) = type_val.as_array() {
                    let has_null = arr.iter().any(|v| v.as_str() == Some("null"));
                    let non_null: Vec<&JsonValue> =
                        arr.iter().filter(|v| v.as_str() != Some("null")).collect();
                    if has_null && non_null.len() == 1 {
                        obj.insert("type".to_string(), non_null[0].clone());
                        obj.insert("nullable".to_string(), JsonValue::Bool(true));
                    } else if has_null && non_null.is_empty() {
                        // `type: ["null"]` → treat as nullable any
                        obj.remove("type");
                        obj.insert("nullable".to_string(), JsonValue::Bool(true));
                    }
                }
            }

            // Convert numeric `exclusive_minimum` / `exclusive_maximum` → boolean.
            // In 3.0 these are booleans; in 3.1 they are numbers (the actual value).
            for field in &["exclusiveMinimum", "exclusiveMaximum"] {
                if let Some(val) = obj.get(*field).cloned() {
                    if val.is_number() {
                        obj.insert(field.to_string(), JsonValue::Bool(true));
                    }
                }
            }

            // Recurse into all values.
            for val in obj.values_mut() {
                downgrade_value(val);
            }
        }
        JsonValue::Array(arr) => {
            for val in arr.iter_mut() {
                downgrade_value(val);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_array_nullable_string() {
        let mut json = serde_json::json!({
            "openapi": "3.1.0",
            "info": { "title": "T", "version": "1.0.0" },
            "paths": {},
            "components": {
                "schemas": {
                    "Name": {
                        "type": ["string", "null"]
                    }
                }
            }
        });
        downgrade_31_to_30(&mut json);
        let name = &json["components"]["schemas"]["Name"];
        assert_eq!(name["type"], "string");
        assert_eq!(name["nullable"], true);
    }

    #[test]
    fn type_array_nullable_object() {
        let mut json = serde_json::json!({
            "openapi": "3.1.0",
            "info": { "title": "T", "version": "1.0.0" },
            "paths": {},
            "components": {
                "schemas": {
                    "Thing": {
                        "type": ["object", "null"],
                        "properties": {
                            "name": { "type": "string" }
                        }
                    }
                }
            }
        });
        downgrade_31_to_30(&mut json);
        let thing = &json["components"]["schemas"]["Thing"];
        assert_eq!(thing["type"], "object");
        assert_eq!(thing["nullable"], true);
    }

    #[test]
    fn exclusive_minimum_numeric_to_bool() {
        let mut json = serde_json::json!({
            "type": "number",
            "exclusiveMinimum": 5
        });
        downgrade_value(&mut json);
        assert_eq!(json["exclusiveMinimum"], true);
    }

    #[test]
    fn missing_paths_added() {
        let mut json = serde_json::json!({
            "openapi": "3.1.0",
            "info": { "title": "T", "version": "1.0.0" }
        });
        downgrade_31_to_30(&mut json);
        assert!(json.get("paths").is_some());
    }

    #[test]
    fn parse_31_spec_with_nullable() {
        let yaml = r#"
openapi: "3.1.0"
info:
  title: Test
  version: "1.0.0"
paths: {}
components:
  schemas:
    Pet:
      type: object
      properties:
        name:
          type: ["string", "null"]
"#;
        let result = parse_str(yaml);
        assert!(result.is_ok(), "should parse 3.1 spec: {:?}", result.err());
    }
}
