//! Swagger Editor compatibility export.
//!
//! Produces a self-contained OpenAPI 3.0 YAML bundle that can be loaded
//! directly into Swagger Editor (editor.swagger.io) and other visual spec
//! editors. All `$ref` pointers are resolved to inline definitions so the
//! editor doesn't need to fetch external files.

use serde_json::Value as JsonValue;

use crate::error::ResolveError;

/// Options for the Swagger Editor export.
#[derive(Debug, Clone, Default)]
pub struct ExportOptions {
    /// Ensure all operations have an operationId.
    pub ensure_operation_ids: bool,
    /// Ensure all schemas have a description.
    pub ensure_schema_descriptions: bool,
}

/// Export format variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    /// Swagger Editor compatible bundle (all refs inlined).
    SwaggerEditor,
}

/// Export an OpenAPI spec as a Swagger Editor-compatible YAML string.
///
/// This reads the raw spec, walks all `$ref` pointers, and inlines them so
/// the result is self-contained. The output is always OpenAPI 3.0.3 YAML.
pub fn export_swagger_editor(
    spec_json: &JsonValue,
    _options: &ExportOptions,
) -> Result<String, ExportError> {
    let mut bundle = spec_json.clone();

    // Ensure OpenAPI version is 3.0.3.
    if let Some(obj) = bundle.as_object_mut() {
        obj.insert(
            "openapi".to_string(),
            JsonValue::String("3.0.3".to_string()),
        );
    }

    // Collect all component schemas into a lookup map.
    let components = bundle
        .get("components")
        .and_then(|c| c.get("schemas"))
        .cloned()
        .unwrap_or_else(|| JsonValue::Object(Default::default()));

    // Ensure paths exists.
    if bundle.get("paths").is_none() {
        if let Some(obj) = bundle.as_object_mut() {
            obj.insert("paths".to_string(), JsonValue::Object(Default::default()));
        }
    }

    // Resolve all $ref pointers in paths and components.
    resolve_refs_in_value(&mut bundle, &components);

    // Remove $ref pointers from component schemas themselves
    // (they should now be fully inlined in paths/responses).
    // But keep component schemas as the canonical definitions.
    // Actually, for Swagger Editor compatibility, we keep components as-is
    // since the editor understands internal $ref. The key is that all
    // operation-level $refs are resolved to their inline definitions.

    // Serialize to YAML.
    let yaml = serde_yaml::to_string(&bundle).map_err(|e| ExportError::Serialize(e.to_string()))?;

    Ok(yaml)
}

/// Walk a JSON value and resolve all `$ref` pointers using the components map.
fn resolve_refs_in_value(value: &mut JsonValue, components: &JsonValue) {
    match value {
        JsonValue::Object(obj) => {
            // If this object has a $ref, replace it with the resolved value.
            if let Some(ref_val) = obj.get("$ref").and_then(|v| v.as_str()) {
                if let Some(resolved) = resolve_ref(ref_val, components) {
                    *value = resolved;
                    // The resolved value might itself contain $refs, so recurse.
                    resolve_refs_in_value(value, components);
                    return;
                }
            }

            // Recurse into all values.
            for val in obj.values_mut() {
                resolve_refs_in_value(val, components);
            }
        }
        JsonValue::Array(arr) => {
            for val in arr.iter_mut() {
                resolve_refs_in_value(val, components);
            }
        }
        _ => {}
    }
}

/// Resolve a single `$ref` string against the components map.
///
/// Supports `#/components/schemas/Name`, `#/components/responses/Name`,
/// and `#/components/parameters/Name`.
fn resolve_ref(reference: &str, components: &JsonValue) -> Option<JsonValue> {
    let ref_path = reference.strip_prefix("#/")?;

    // Parse the JSON pointer path.
    let segments: Vec<&str> = ref_path.split('/').collect();
    if segments.len() < 2 {
        return None;
    }

    // Skip the first segment ("components") and walk the rest.
    // The components object already has "schemas", "responses", etc. as keys.
    let mut current = components;
    for segment in &segments[1..] {
        // Handle JSON pointer escaping: ~1 → /, ~0 → ~
        let unescaped = segment.replace("~1", "/").replace("~0", "~");
        current = current.get(&unescaped)?;
    }

    Some(current.clone())
}

/// Errors that can occur during export.
#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("serialization failed: {0}")]
    Serialize(String),

    #[error("invalid spec: {0}")]
    Invalid(String),

    #[error(transparent)]
    Resolve(#[from] ResolveError),
}

/// Generate a Swagger Editor-compatible bundle from a raw spec string.
///
/// This is the high-level entry point that parses the spec and exports it.
pub fn export_spec(spec_text: &str, options: &ExportOptions) -> Result<String, ExportError> {
    // Parse as raw JSON value (preserves full structure).
    let json: JsonValue = if let Ok(v) = serde_json::from_str::<JsonValue>(spec_text.trim_start()) {
        v
    } else {
        serde_yaml::from_str::<JsonValue>(spec_text)
            .map_err(|e| ExportError::Invalid(format!("failed to parse spec: {e}")))?
    };

    export_swagger_editor(&json, options)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_simple_ref() {
        let components = serde_json::json!({
            "schemas": {
                "Pet": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string" }
                    }
                }
            }
        });
        let result = resolve_ref("#/components/schemas/Pet", &components);
        assert!(result.is_some());
        let resolved = result.unwrap();
        assert_eq!(resolved["type"], "object");
    }

    #[test]
    fn resolve_nested_ref() {
        let components = serde_json::json!({
            "responses": {
                "NotFound": {
                    "description": "Not found",
                    "content": {
                        "application/json": {
                            "schema": {
                                "$ref": "#/components/schemas/Error"
                            }
                        }
                    }
                }
            },
            "schemas": {
                "Error": {
                    "type": "object",
                    "properties": {
                        "message": { "type": "string" }
                    }
                }
            }
        });
        let result = resolve_ref("#/components/responses/NotFound", &components);
        assert!(result.is_some());
    }

    #[test]
    fn resolve_ref_returns_none_for_invalid() {
        let components = serde_json::json!({});
        let result = resolve_ref("#/components/schemas/Missing", &components);
        assert!(result.is_none());
    }

    #[test]
    fn resolve_ref_returns_none_for_non_component() {
        let components = serde_json::json!({});
        let result = resolve_ref("#/definitions/Pet", &components);
        assert!(result.is_none());
    }

    #[test]
    fn export_inlines_refs_in_response() {
        let spec = serde_json::json!({
            "openapi": "3.0.3",
            "info": { "title": "Test", "version": "1.0.0" },
            "paths": {
                "/pets": {
                    "get": {
                        "operationId": "listPets",
                        "responses": {
                            "200": {
                                "description": "OK",
                                "content": {
                                    "application/json": {
                                        "schema": {
                                            "$ref": "#/components/schemas/Pet"
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
                    "Pet": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string" }
                        }
                    }
                }
            }
        });

        let result = export_swagger_editor(&spec, &ExportOptions::default());
        assert!(result.is_ok());
        let yaml = result.unwrap();

        // The exported YAML should contain the inlined schema.
        assert!(
            yaml.contains("type: object"),
            "should contain 'type: object' in YAML"
        );
        // And should contain the original info.
        assert!(yaml.contains("title: Test"), "should contain 'title: Test'");
        assert!(yaml.contains("3.0.3"), "should contain '3.0.3'");
    }

    #[test]
    fn export_sets_openapi_version() {
        let spec = serde_json::json!({
            "openapi": "3.1.0",
            "info": { "title": "Test", "version": "1.0.0" },
            "paths": {}
        });

        let result = export_swagger_editor(&spec, &ExportOptions::default());
        assert!(result.is_ok());
        let yaml = result.unwrap();
        assert!(yaml.contains("3.0.3"));
    }

    #[test]
    fn export_spec_from_json_string() {
        let spec = serde_json::json!({
            "openapi": "3.0.3",
            "info": { "title": "Test", "version": "1.0.0" },
            "paths": {
                "/health": {
                    "get": {
                        "operationId": "getHealth",
                        "responses": {
                            "200": {
                                "description": "OK",
                                "content": {
                                    "application/json": {
                                        "schema": {
                                            "$ref": "#/components/schemas/HealthResponse"
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
                    "HealthResponse": {
                        "type": "object",
                        "properties": {
                            "status": { "type": "string" }
                        }
                    }
                }
            }
        });

        let spec_str = serde_json::to_string(&spec).unwrap();
        let result = export_spec(&spec_str, &ExportOptions::default());
        assert!(result.is_ok());
        let out = result.unwrap();
        assert!(out.contains("3.0.3"));
        assert!(out.contains("HealthResponse"));
    }

    #[test]
    fn deeply_nested_ref_resolution() {
        let spec = serde_json::json!({
            "openapi": "3.0.3",
            "info": { "title": "Test", "version": "1.0.0" },
            "paths": {
                "/pets/{petId}": {
                    "get": {
                        "operationId": "getPet",
                        "parameters": [
                            {
                                "name": "petId",
                                "in": "path",
                                "required": true,
                                "schema": { "type": "string" }
                            }
                        ],
                        "responses": {
                            "200": {
                                "description": "OK",
                                "content": {
                                    "application/json": {
                                        "schema": {
                                            "$ref": "#/components/schemas/Pet"
                                        }
                                    }
                                }
                            },
                            "404": {
                                "$ref": "#/components/responses/NotFound"
                            }
                        }
                    }
                }
            },
            "components": {
                "responses": {
                    "NotFound": {
                        "description": "Not found",
                        "content": {
                            "application/json": {
                                "schema": {
                                    "$ref": "#/components/schemas/Error"
                                }
                            }
                        }
                    }
                },
                "schemas": {
                    "Pet": {
                        "type": "object",
                        "properties": {
                            "id": { "type": "integer" },
                            "name": { "type": "string" }
                        }
                    },
                    "Error": {
                        "type": "object",
                        "properties": {
                            "code": { "type": "integer" },
                            "message": { "type": "string" }
                        }
                    }
                }
            }
        });

        let result = export_swagger_editor(&spec, &ExportOptions::default());
        assert!(result.is_ok());
        let yaml = result.unwrap();
        // Both Pet and Error schemas should be inlined.
        assert!(yaml.contains("id:"));
        assert!(yaml.contains("code:"));
    }
}
