//! Spec composition: merge multiple OpenAPI spec files into one.
//!
//! Later specs override earlier ones on conflict (paths, schemas, etc.).

use serde_json::Value;
use crate::error::SpecError;

/// Merge multiple OpenAPI spec files into one.
///
/// The first spec provides the base structure (info, servers, etc.).
/// Subsequent specs are merged in order: paths, component schemas,
/// security schemes, and other top-level keys are combined, with later
/// values taking precedence on key collision.
pub fn merge_specs(specs: &[Value]) -> Result<Value, SpecError> {
    if specs.is_empty() {
        return Err(SpecError::Invalid("no specs to merge".into()));
    }

    let mut merged = specs[0].clone();

    for spec in &specs[1..] {
        merge_into(&mut merged, spec)?;
    }

    Ok(merged)
}

fn merge_into(target: &mut Value, source: &Value) -> Result<(), SpecError> {
    // Merge paths
    if let (Some(t_paths), Some(s_paths)) = (target.get_mut("paths"), source.get("paths")) {
        if let (Some(t_obj), Some(s_obj)) = (t_paths.as_object_mut(), s_paths.as_object()) {
            for (key, val) in s_obj {
                t_obj.insert(key.clone(), val.clone());
            }
        }
    }

    // Merge components: ensure target has a "components" object if source does
    if source.get("components").is_some() && target.get("components").is_none() {
        if let Some(obj) = target.as_object_mut() {
            obj.insert("components".to_string(), serde_json::json!({}));
        }
    }

    // Merge components.schemas
    if let Some(s_schemas) = source.get("components").and_then(|c| c.get("schemas")) {
        // Ensure target has components.schemas
        if target.get("components").and_then(|c| c.get("schemas")).is_none() {
            if let Some(t_comp) = target.get_mut("components") {
                if let Some(obj) = t_comp.as_object_mut() {
                    obj.insert("schemas".to_string(), serde_json::json!({}));
                }
            }
        }
        if let (Some(t_schemas), Some(s_obj)) = (
            target.get_mut("components").and_then(|c| c.get_mut("schemas")).and_then(|s| s.as_object_mut()),
            s_schemas.as_object(),
        ) {
            for (key, val) in s_obj {
                t_schemas.insert(key.clone(), val.clone());
            }
        }
    }

    // Merge security schemes
    if let Some(s_sec) = source.get("components").and_then(|c| c.get("securitySchemes")) {
        // Ensure target has components.securitySchemes
        if target.get("components").and_then(|c| c.get("securitySchemes")).is_none() {
            if let Some(t_comp) = target.get_mut("components") {
                if let Some(obj) = t_comp.as_object_mut() {
                    obj.insert("securitySchemes".to_string(), serde_json::json!({}));
                }
            }
        }
        if let (Some(t_sec_obj), Some(s_sec_obj)) = (
            target.get_mut("components").and_then(|c| c.get_mut("securitySchemes")).and_then(|s| s.as_object_mut()),
            s_sec.as_object(),
        ) {
            for (key, val) in s_sec_obj {
                t_sec_obj.insert(key.clone(), val.clone());
            }
        }
    }

    // Merge top-level tags (append unique)
    if let (Some(t_tags), Some(s_tags)) = (target.get_mut("tags"), source.get("tags")) {
        if let (Some(t_arr), Some(s_arr)) = (t_tags.as_array_mut(), s_tags.as_array()) {
            let existing_names: std::collections::HashSet<String> = t_arr
                .iter()
                .filter_map(|t| t.get("name").and_then(|n| n.as_str()).map(String::from))
                .collect();
            for tag in s_arr {
                let name = tag.get("name").and_then(|n| n.as_str());
                if name.map_or(true, |n| !existing_names.contains(n)) {
                    t_arr.push(tag.clone());
                }
            }
        }
    }

    // Merge top-level security (append unique entries)
    if let (Some(t_sec), Some(s_sec)) = (target.get_mut("security"), source.get("security")) {
        if let (Some(t_arr), Some(s_arr)) = (t_sec.as_array_mut(), s_sec.as_array()) {
            for entry in s_arr {
                if !t_arr.contains(entry) {
                    t_arr.push(entry.clone());
                }
            }
        }
    }

    // Merge servers (append unique)
    if let (Some(t_servers), Some(s_servers)) = (target.get_mut("servers"), source.get("servers")) {
        if let (Some(t_arr), Some(s_arr)) = (t_servers.as_array_mut(), s_servers.as_array()) {
            for server in s_arr {
                if !t_arr.contains(server) {
                    t_arr.push(server.clone());
                }
            }
        }
    }

    // Keep target's info as-is (first spec wins)

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn merge_empty_specs_errors() {
        let result = merge_specs(&[]);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("no specs to merge"));
    }

    #[test]
    fn merge_single_spec_returns_clone() {
        let spec = json!({
            "openapi": "3.0.0",
            "info": { "title": "A", "version": "1.0" },
            "paths": { "/a": {} }
        });
        let result = merge_specs(std::slice::from_ref(&spec)).unwrap();
        assert_eq!(result, spec);
    }

    #[test]
    fn merge_two_specs_different_paths() {
        let spec1 = json!({
            "openapi": "3.0.0",
            "info": { "title": "A", "version": "1.0" },
            "paths": { "/users": { "get": { "summary": "list users" } } }
        });
        let spec2 = json!({
            "openapi": "3.0.0",
            "info": { "title": "B", "version": "1.0" },
            "paths": { "/orders": { "get": { "summary": "list orders" } } }
        });

        let merged = merge_specs(&[spec1, spec2]).unwrap();
        let paths = merged.get("paths").unwrap().as_object().unwrap();
        assert!(paths.contains_key("/users"));
        assert!(paths.contains_key("/orders"));
        // First spec's info wins
        assert_eq!(merged["info"]["title"], "A");
    }

    #[test]
    fn merge_overlapping_paths_later_wins() {
        let spec1 = json!({
            "openapi": "3.0.0",
            "info": { "title": "A", "version": "1.0" },
            "paths": { "/items": { "get": { "summary": "old" } } }
        });
        let spec2 = json!({
            "openapi": "3.0.0",
            "info": { "title": "B", "version": "1.0" },
            "paths": { "/items": { "get": { "summary": "new" } } }
        });

        let merged = merge_specs(&[spec1, spec2]).unwrap();
        assert_eq!(merged["paths"]["/items"]["get"]["summary"], "new");
    }

    #[test]
    fn merge_schemas_later_wins_on_overlap() {
        let spec1 = json!({
            "openapi": "3.0.0",
            "info": { "title": "A", "version": "1.0" },
            "paths": {},
            "components": {
                "schemas": {
                    "Pet": { "type": "object", "properties": { "name": { "type": "string" } } }
                }
            }
        });
        let spec2 = json!({
            "openapi": "3.0.0",
            "info": { "title": "B", "version": "1.0" },
            "paths": {},
            "components": {
                "schemas": {
                    "Pet": { "type": "object", "properties": { "id": { "type": "integer" }, "name": { "type": "string" } } },
                    "Order": { "type": "object" }
                }
            }
        });

        let merged = merge_specs(&[spec1, spec2]).unwrap();
        let schemas = &merged["components"]["schemas"];
        // Pet from spec2 overrides spec1
        assert!(schemas["Pet"]["properties"].as_object().unwrap().contains_key("id"));
        // Order only in spec2
        assert!(schemas.as_object().unwrap().contains_key("Order"));
    }

    #[test]
    fn merge_security_schemes() {
        let spec1 = json!({
            "openapi": "3.0.0",
            "info": { "title": "A", "version": "1.0" },
            "paths": {},
            "components": {
                "securitySchemes": {
                    "ApiKey": { "type": "apiKey", "name": "X-API-Key", "in": "header" }
                }
            }
        });
        let spec2 = json!({
            "openapi": "3.0.0",
            "info": { "title": "B", "version": "1.0" },
            "paths": {},
            "components": {
                "securitySchemes": {
                    "Bearer": { "type": "http", "scheme": "bearer" }
                }
            }
        });

        let merged = merge_specs(&[spec1, spec2]).unwrap();
        let schemes = &merged["components"]["securitySchemes"];
        assert!(schemes.as_object().unwrap().contains_key("ApiKey"));
        assert!(schemes.as_object().unwrap().contains_key("Bearer"));
    }

    #[test]
    fn merge_creates_components_if_missing_in_target() {
        let spec1 = json!({
            "openapi": "3.0.0",
            "info": { "title": "A", "version": "1.0" },
            "paths": {}
        });
        let spec2 = json!({
            "openapi": "3.0.0",
            "info": { "title": "B", "version": "1.0" },
            "paths": {},
            "components": {
                "schemas": {
                    "Widget": { "type": "object" }
                }
            }
        });

        let merged = merge_specs(&[spec1, spec2]).unwrap();
        assert!(merged["components"]["schemas"].as_object().unwrap().contains_key("Widget"));
    }

    #[test]
    fn merge_three_specs() {
        let spec1 = json!({
            "openapi": "3.0.0",
            "info": { "title": "A", "version": "1.0" },
            "paths": { "/a": {} }
        });
        let spec2 = json!({
            "openapi": "3.0.0",
            "info": { "title": "B", "version": "1.0" },
            "paths": { "/b": {} }
        });
        let spec3 = json!({
            "openapi": "3.0.0",
            "info": { "title": "C", "version": "1.0" },
            "paths": { "/c": {} }
        });

        let merged = merge_specs(&[spec1, spec2, spec3]).unwrap();
        let paths = merged.get("paths").unwrap().as_object().unwrap();
        assert_eq!(paths.len(), 3);
        assert!(paths.contains_key("/a"));
        assert!(paths.contains_key("/b"));
        assert!(paths.contains_key("/c"));
    }

    #[test]
    fn merge_preserves_openapi_version() {
        let spec1 = json!({
            "openapi": "3.0.0",
            "info": { "title": "A", "version": "1.0" },
            "paths": {}
        });
        let spec2 = json!({
            "openapi": "3.1.0",
            "info": { "title": "B", "version": "1.0" },
            "paths": {}
        });

        let merged = merge_specs(&[spec1, spec2]).unwrap();
        // First spec's openapi version is preserved
        assert_eq!(merged["openapi"], "3.0.0");
    }
}
