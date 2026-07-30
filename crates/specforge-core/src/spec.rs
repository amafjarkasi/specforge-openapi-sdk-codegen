//! Parsing raw OpenAPI YAML/JSON into `openapiv3` types.
//!
//! JSON and YAML are both supported; detection is content-based (parse-fallback)
//! rather than by extension so callers don't have to guess.
//!
//! OpenAPI 3.1 specs are transparently downgraded to 3.0 before parsing so the
//! `openapiv3` crate (which only supports 3.0) can consume them. The key
//! conversions are: `type: ["X", "null"]` → `type: X` + `nullable: true`, and
//! numeric `exclusive_minimum`/`exclusive_maximum` → boolean.

use std::path::{Path, PathBuf};

use openapiv3::OpenAPI;
use serde_json::Value as JsonValue;

use crate::error::SpecError;

/// Information about a discovered API version in a spec directory.
#[derive(Debug, Clone)]
pub struct VersionInfo {
    /// The API version string extracted from `info.version`.
    pub version: String,
    /// The file path where this version's spec lives.
    pub path: PathBuf,
}

/// A parsed OpenAPI document along with any webhooks extracted from the raw
/// spec. The `openapiv3` crate does not support the `webhooks` field (OpenAPI
/// 3.1), so we extract it from the raw JSON before parsing and carry it
/// alongside the parsed spec.
#[derive(Debug, Clone)]
pub struct ParsedSpec {
    /// The parsed OpenAPI document (3.0-compatible after any downgrade).
    pub spec: OpenAPI,
    /// The raw `webhooks` JSON map, if present. Each key is a webhook name
    /// and each value is a PathItem object.
    pub webhooks: Option<serde_json::Value>,
}

/// Read and parse an OpenAPI document from a file path (JSON or YAML).
pub fn parse_file(path: impl AsRef<Path>) -> Result<OpenAPI, SpecError> {
    let path = path.as_ref();
    let bytes = std::fs::read(path).map_err(|source| SpecError::Io {
        path: path.display().to_string(),
        source,
    })?;
    parse_bytes(&bytes)
}

/// Read and parse an OpenAPI document from a file path, preserving webhooks.
pub fn parse_file_full(path: impl AsRef<Path>) -> Result<ParsedSpec, SpecError> {
    let path = path.as_ref();
    let bytes = std::fs::read(path).map_err(|source| SpecError::Io {
        path: path.display().to_string(),
        source,
    })?;
    parse_bytes_full(&bytes)
}

/// Parse an OpenAPI document from bytes (JSON or YAML, auto-detected).
pub fn parse_bytes(bytes: &[u8]) -> Result<OpenAPI, SpecError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|e| SpecError::Invalid(format!("spec is not valid UTF-8: {e}")))?;
    parse_str(text)
}

/// Parse an OpenAPI document from bytes, preserving webhooks.
pub fn parse_bytes_full(bytes: &[u8]) -> Result<ParsedSpec, SpecError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|e| SpecError::Invalid(format!("spec is not valid UTF-8: {e}")))?;
    parse_str_full(text)
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

/// Parse an OpenAPI document from a string, preserving the `webhooks` section.
///
/// OpenAPI 3.1 introduced a top-level `webhooks` field that maps webhook names
/// to PathItem objects. The `openapiv3` crate does not support this field, so
/// we extract it from the raw JSON before parsing and return it separately in
/// the [`ParsedSpec`] struct.
pub fn parse_str_full(text: &str) -> Result<ParsedSpec, SpecError> {
    // JSON parses faster and stricter; try it first, fall back to YAML.
    let mut json: JsonValue = if let Ok(v) = serde_json::from_str::<JsonValue>(text.trim_start())
    {
        v
    } else {
        serde_yaml::from_str::<JsonValue>(text).map_err(SpecError::Yaml)?
    };

    // Extract webhooks before downgrading (they exist only in 3.1).
    let webhooks = extract_webhooks(&mut json);

    // Downgrade 3.1 → 3.0 if needed.
    if is_31(&json) {
        downgrade_31_to_30(&mut json);
    }

    let spec = serde_json::from_value::<OpenAPI>(json).map_err(SpecError::Json)?;
    Ok(ParsedSpec { spec, webhooks })
}

/// Extract the `webhooks` field from the raw JSON, removing it so the
/// `openapiv3` parser doesn't choke on the unknown field.
fn extract_webhooks(json: &mut JsonValue) -> Option<JsonValue> {
    json.as_object_mut()
        .and_then(|obj| obj.remove("webhooks"))
        .filter(|v| v.is_object())
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
    // First pass: extract `$ref` siblings into allOf wrappers. This must
    // happen before other downgrades so the allOf structure is in place.
    extract_ref_siblings(json);
    downgrade_value(json);
}

/// Recursively walk the JSON tree and convert any object that has both a
/// `$ref` and sibling properties into an `allOf` wrapper. This preserves
/// sibling data that the `openapiv3` crate would otherwise silently drop.
///
/// ```json
/// // Before:
/// { "$ref": "#/components/schemas/Pet", "description": "A pet" }
///
/// // After:
/// { "allOf": [
///     { "$ref": "#/components/schemas/Pet" },
///     { "description": "A pet" }
/// ]}
/// ```
fn extract_ref_siblings(json: &mut JsonValue) {
    match json {
        JsonValue::Object(obj) => {
            // Check if this object has a `$ref` key alongside other keys.
            if let Some(ref_val) = obj.get("$ref").cloned() {
                let ref_str = ref_val.as_str().unwrap_or("");
                if !ref_str.is_empty() {
                    // Collect sibling keys (everything except `$ref`).
                    let siblings: Vec<(String, JsonValue)> = obj
                        .iter()
                        .filter(|(k, _)| *k != "$ref")
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect();

                    if !siblings.is_empty() {
                        // Build the allOf wrapper.
                        let ref_member =
                            serde_json::json!({ "$ref": ref_val });
                        let mut sibling_obj = serde_json::Map::new();
                        for (k, v) in siblings {
                            sibling_obj.insert(k, v);
                        }
                        *obj = serde_json::Map::new();
                        obj.insert(
                            "allOf".to_string(),
                            serde_json::json!([ref_member, JsonValue::Object(sibling_obj)]),
                        );
                    }
                }
            }

            // Recurse into all values.
            for val in obj.values_mut() {
                extract_ref_siblings(val);
            }
        }
        JsonValue::Array(arr) => {
            for val in arr.iter_mut() {
                extract_ref_siblings(val);
            }
        }
        _ => {}
    }
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

            // Convert `const` → `enum: [value]`.
            // In 3.1, `const` restricts to a single exact value; in 3.0 the
            // equivalent is a single-element `enum`.
            if let Some(const_val) = obj.remove("const") {
                obj.insert("enum".to_string(), JsonValue::Array(vec![const_val]));
            }

            // Convert `dependentRequired` → merge into `required`.
            // In 3.1, `dependentRequired` maps property names to arrays of
            // additional required properties. In 3.0 we flatten all of them
            // into the top-level `required` array (lossy but workable).
            if let Some(dep_req) = obj.remove("dependentRequired") {
                if let Some(dep_map) = dep_req.as_object() {
                    let mut extra: Vec<String> = Vec::new();
                    for deps in dep_map.values() {
                        if let Some(arr) = deps.as_array() {
                            for v in arr {
                                if let Some(s) = v.as_str() {
                                    extra.push(s.to_string());
                                }
                            }
                        }
                    }
                    if !extra.is_empty() {
                        let required = obj
                            .entry("required".to_string())
                            .or_insert_with(|| JsonValue::Array(vec![]));
                        if let Some(req_arr) = required.as_array_mut() {
                            for dep in extra {
                                let dep_json = JsonValue::String(dep);
                                if !req_arr.contains(&dep_json) {
                                    req_arr.push(dep_json);
                                }
                            }
                        }
                    }
                }
            }

            // Convert `prefixItems` → `items` (tuple validation → array items).
            // In 3.1, `prefixItems` is an array of schemas for positional
            // validation. In 3.0, `items` is a single schema (or
            // `items` + `additionalItems`). We take the first prefix item
            // as the `items` schema — a best-effort approximation.
            if let Some(prefix) = obj.remove("prefixItems") {
                if let Some(arr) = prefix.as_array() {
                    if arr.len() == 1 {
                        // Single prefix item: use directly as `items`.
                        obj.insert("items".to_string(), arr[0].clone());
                    } else if arr.len() > 1 {
                        // Multiple prefix items: use `items` as an array
                        // (supported by some validators) or fall back to first.
                        // openapiv3 doesn't support array `items`, so use first.
                        obj.insert("items".to_string(), arr[0].clone());
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

/// Extract the `info.version` field from a raw YAML/JSON value without full
/// OpenAPI parsing. Returns `None` if the field is missing or not a string.
fn extract_info_version(json: &JsonValue) -> Option<String> {
    json.get("info")
        .and_then(|info| info.get("version"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Try to extract the API version from a spec file. This does a lightweight
/// parse (just reads the raw YAML/JSON) rather than a full OpenAPI parse,
/// so it works even for specs that have minor issues.
fn try_extract_version(path: &Path) -> Option<VersionInfo> {
    let bytes = std::fs::read(path).ok()?;
    let text = std::str::from_utf8(&bytes).ok()?;

    // Try JSON first, then YAML.
    let json: JsonValue = if let Ok(v) = serde_json::from_str::<JsonValue>(text.trim_start()) {
        v
    } else {
        serde_yaml::from_str::<JsonValue>(text).ok()?
    };

    let version = extract_info_version(&json)?;
    Some(VersionInfo {
        version,
        path: path.to_path_buf(),
    })
}

/// Scan a directory for OpenAPI spec files (`.yaml`, `.yml`, `.json`) and
/// extract their API versions. Supports two directory conventions:
///
/// 1. Flat files: `specs/v1.yaml`, `specs/v2.yaml`
/// 2. Subdirectories: `specs/v1/openapi.yaml`, `specs/v2/openapi.yaml`
///
/// Returns a list of [`VersionInfo`] sorted by version string.
pub fn scan_versions(dir: &Path) -> Vec<VersionInfo> {
    let mut results = Vec::new();

    let Ok(entries) = std::fs::read_dir(dir) else {
        return results;
    };

    for entry in entries.flatten() {
        let path = entry.path();

        if path.is_file() {
            // Convention 1: flat files like v1.yaml, v2.json, openapi.yaml
            if is_spec_file(&path) {
                if let Some(info) = try_extract_version(&path) {
                    results.push(info);
                }
            }
        } else if path.is_dir() {
            // Convention 2: subdirectories like v1/openapi.yaml
            if let Some(info) = scan_version_dir(&path) {
                results.push(info);
            }
        }
    }

    results.sort_by(|a, b| a.version.cmp(&b.version));
    results
}

/// Check if a file looks like an OpenAPI spec by extension.
fn is_spec_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("yaml" | "yml" | "json")
    )
}

/// Scan a single subdirectory for a spec file and extract its version.
/// Looks for `openapi.yaml`, `openapi.yml`, `openapi.json`, or any
/// `.yaml`/`.json` file in the directory.
fn scan_version_dir(dir: &Path) -> Option<VersionInfo> {
    // Prefer well-known names first.
    let preferred = ["openapi.yaml", "openapi.yml", "openapi.json"];
    for name in &preferred {
        let path = dir.join(name);
        if path.is_file() {
            if let Some(info) = try_extract_version(&path) {
                return Some(info);
            }
        }
    }

    // Fall back to any spec file in the directory.
    let Ok(entries) = std::fs::read_dir(dir) else {
        return None;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && is_spec_file(&path) {
            if let Some(info) = try_extract_version(&path) {
                return Some(info);
            }
        }
    }

    None
}

/// Resolve a spec path from a directory and optional version filter.
///
/// - If `path` is a file, returns it directly.
/// - If `path` is a directory and `version` is `None`, returns an error
///   asking the user to specify `--version`.
/// - If `path` is a directory and `version` is `Some`, scans the directory
///   and returns the matching spec file.
pub fn resolve_spec_path(path: &Path, version: Option<&str>) -> Result<PathBuf, SpecError> {
    if path.is_file() {
        return Ok(path.to_path_buf());
    }

    if !path.is_dir() {
        return Err(SpecError::Io {
            path: path.display().to_string(),
            source: std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "path does not exist or is not a file/directory",
            ),
        });
    }

    let version_str = version.ok_or_else(|| SpecError::Invalid(format!(
        "spec path is a directory; use --version to select an API version (run `specforge versions {}` to see available versions)",
        path.display()
    )))?;

    let versions = scan_versions(path);
    if versions.is_empty() {
        return Err(SpecError::Invalid(format!(
            "no OpenAPI spec files found in {}",
            path.display()
        )));
    }

    // Try exact match first.
    if let Some(info) = versions.iter().find(|v| v.version == version_str) {
        return Ok(info.path.clone());
    }

    // Try matching by directory/file name (e.g., "v1" matches specs/v1.yaml).
    let normalized = if version_str.starts_with('v') {
        version_str.to_string()
    } else {
        format!("v{version_str}")
    };

    for info in &versions {
        let file_stem = info.path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let parent_name = info
            .path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .unwrap_or("");

        if file_stem == normalized || parent_name == normalized {
            return Ok(info.path.clone());
        }
    }

    let available: Vec<&str> = versions.iter().map(|v| v.version.as_str()).collect();
    Err(SpecError::Invalid(format!(
        "version {version_str:?} not found; available versions: {}",
        available.join(", ")
    )))
}

/// Features detected in an OpenAPI 3.1 spec that have no direct 3.0 equivalent.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Spec31Features {
    /// `type: ["string", "null"]` — array type values.
    pub uses_type_array: bool,
    /// `$ref` with sibling properties (description, summary, etc.).
    pub uses_ref_siblings: bool,
    /// `const` keyword for single-value constraints.
    pub uses_const: bool,
    /// `prefixItems` for tuple validation.
    pub uses_prefix_items: bool,
    /// `dependentRequired` for conditional required fields.
    pub uses_dependent_required: bool,
    /// Numeric `exclusiveMinimum` / `exclusiveMaximum` (3.1 style).
    pub uses_numeric_exclusive_bounds: bool,
}

/// Walk a raw JSON value and detect which OpenAPI 3.1-specific features are
/// used. This is a lightweight scan that does not perform full schema parsing.
pub fn detect_31_features(json: &JsonValue) -> Spec31Features {
    let mut features = Spec31Features::default();
    detect_31_features_walk(json, &mut features);
    features
}

fn detect_31_features_walk(json: &JsonValue, features: &mut Spec31Features) {
    match json {
        JsonValue::Object(obj) => {
            // Check for array type values.
            if let Some(type_val) = obj.get("type") {
                if type_val.is_array() {
                    features.uses_type_array = true;
                }
            }

            // Check for $ref with siblings.
            if obj.contains_key("$ref") && obj.len() > 1 {
                features.uses_ref_siblings = true;
            }

            // Check for const keyword.
            if obj.contains_key("const") {
                features.uses_const = true;
            }

            // Check for prefixItems.
            if obj.contains_key("prefixItems") {
                features.uses_prefix_items = true;
            }

            // Check for dependentRequired.
            if obj.contains_key("dependentRequired") {
                features.uses_dependent_required = true;
            }

            // Check for numeric exclusive bounds.
            for field in &["exclusiveMinimum", "exclusiveMaximum"] {
                if let Some(val) = obj.get(*field) {
                    if val.is_number() {
                        features.uses_numeric_exclusive_bounds = true;
                    }
                }
            }

            // Recurse.
            for val in obj.values() {
                detect_31_features_walk(val, features);
            }
        }
        JsonValue::Array(arr) => {
            for val in arr {
                detect_31_features_walk(val, features);
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

    // ── $ref sibling extraction tests ────────────────────────────────────

    #[test]
    fn ref_with_description_sibling_becomes_allof() {
        let mut json = serde_json::json!({
            "$ref": "#/components/schemas/Pet",
            "description": "A pet in the store"
        });
        extract_ref_siblings(&mut json);
        assert!(json.get("allOf").is_some(), "should have allOf");
        let allof = json["allOf"].as_array().unwrap();
        assert_eq!(allof.len(), 2);
        assert_eq!(allof[0]["$ref"], "#/components/schemas/Pet");
        assert_eq!(allof[1]["description"], "A pet in the store");
        // The original $ref should no longer be at the top level.
        assert!(json.get("$ref").is_none());
    }

    #[test]
    fn ref_with_summary_sibling_becomes_allof() {
        let mut json = serde_json::json!({
            "$ref": "#/components/schemas/Pet",
            "summary": "Pet summary"
        });
        extract_ref_siblings(&mut json);
        let allof = json["allOf"].as_array().unwrap();
        assert_eq!(allof.len(), 2);
        assert_eq!(allof[0]["$ref"], "#/components/schemas/Pet");
        assert_eq!(allof[1]["summary"], "Pet summary");
    }

    #[test]
    fn ref_with_multiple_siblings_becomes_allof() {
        let mut json = serde_json::json!({
            "$ref": "#/components/schemas/Pet",
            "description": "A pet",
            "summary": "Pet",
            "deprecated": true
        });
        extract_ref_siblings(&mut json);
        let allof = json["allOf"].as_array().unwrap();
        assert_eq!(allof.len(), 2);
        assert_eq!(allof[0]["$ref"], "#/components/schemas/Pet");
        assert_eq!(allof[1]["description"], "A pet");
        assert_eq!(allof[1]["summary"], "Pet");
        assert_eq!(allof[1]["deprecated"], true);
    }

    #[test]
    fn ref_without_siblings_unchanged() {
        let mut json = serde_json::json!({
            "$ref": "#/components/schemas/Pet"
        });
        extract_ref_siblings(&mut json);
        // Should remain a plain $ref — no allOf wrapper.
        assert!(json.get("allOf").is_none());
        assert_eq!(json["$ref"], "#/components/schemas/Pet");
    }

    #[test]
    fn ref_siblings_in_nested_location() {
        let mut json = serde_json::json!({
            "type": "object",
            "properties": {
                "pet": {
                    "$ref": "#/components/schemas/Pet",
                    "description": "The pet"
                }
            }
        });
        extract_ref_siblings(&mut json);
        let pet = &json["properties"]["pet"];
        assert!(pet.get("allOf").is_some(), "nested $ref should be wrapped");
        let allof = pet["allOf"].as_array().unwrap();
        assert_eq!(allof[0]["$ref"], "#/components/schemas/Pet");
        assert_eq!(allof[1]["description"], "The pet");
    }

    #[test]
    fn ref_siblings_in_array_items() {
        let mut json = serde_json::json!({
            "type": "array",
            "items": {
                "$ref": "#/components/schemas/Pet",
                "description": "A pet item"
            }
        });
        extract_ref_siblings(&mut json);
        let items = &json["items"];
        assert!(items.get("allOf").is_some(), "items $ref should be wrapped");
    }

    #[test]
    fn ref_sibling_full_pipeline_preserves_description() {
        // A 3.1 spec with a $ref sibling inside an allOf member.
        let yaml = r##"
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
          type: string
    NamedPet:
      allOf:
        - $ref: "#/components/schemas/Pet"
          description: "A named pet"
"##;
        let result = parse_str(yaml);
        assert!(result.is_ok(), "should parse: {:?}", result.err());
    }

    #[test]
    fn ref_sibling_at_property_level_full_pipeline() {
        // A 3.1 spec where a property uses $ref with description sibling.
        let yaml = r##"
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
          type: string
    Owner:
      type: object
      properties:
        pet:
          $ref: "#/components/schemas/Pet"
          description: "The owner's pet"
"##;
        let result = parse_str(yaml);
        assert!(result.is_ok(), "should parse: {:?}", result.err());
    }

    // ── version scanning tests ──────────────────────────────────────────

    #[test]
    fn scan_versions_flat_files() {
        let dir = tempfile::tempdir().unwrap();
        let v1 = dir.path().join("v1.yaml");
        let v2 = dir.path().join("v2.yaml");
        std::fs::write(&v1, "openapi: \"3.0.3\"\ninfo:\n  title: T\n  version: \"1.0.0\"\npaths: {}").unwrap();
        std::fs::write(&v2, "openapi: \"3.0.3\"\ninfo:\n  title: T\n  version: \"2.0.0\"\npaths: {}").unwrap();

        let versions = scan_versions(dir.path());
        assert_eq!(versions.len(), 2);
        assert_eq!(versions[0].version, "1.0.0");
        assert_eq!(versions[0].path, v1);
        assert_eq!(versions[1].version, "2.0.0");
        assert_eq!(versions[1].path, v2);
    }

    #[test]
    fn scan_versions_nested_directories() {
        let dir = tempfile::tempdir().unwrap();
        let v1_dir = dir.path().join("v1");
        let v2_dir = dir.path().join("v2");
        std::fs::create_dir_all(&v1_dir).unwrap();
        std::fs::create_dir_all(&v2_dir).unwrap();
        std::fs::write(v1_dir.join("openapi.yaml"), "openapi: \"3.0.3\"\ninfo:\n  title: T\n  version: \"1.0.0\"\npaths: {}").unwrap();
        std::fs::write(v2_dir.join("openapi.yaml"), "openapi: \"3.0.3\"\ninfo:\n  title: T\n  version: \"2.0.0\"\npaths: {}").unwrap();

        let versions = scan_versions(dir.path());
        assert_eq!(versions.len(), 2);
        assert_eq!(versions[0].version, "1.0.0");
        assert_eq!(versions[1].version, "2.0.0");
    }

    #[test]
    fn scan_versions_empty_directory() {
        let dir = tempfile::tempdir().unwrap();
        let versions = scan_versions(dir.path());
        assert!(versions.is_empty());
    }

    #[test]
    fn scan_versions_ignores_non_spec_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("readme.txt"), "not a spec").unwrap();
        std::fs::write(dir.path().join("v1.yaml"), "openapi: \"3.0.3\"\ninfo:\n  title: T\n  version: \"1.0.0\"\npaths: {}").unwrap();

        let versions = scan_versions(dir.path());
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].version, "1.0.0");
    }

    #[test]
    fn scan_versions_json_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("v1.json"),
            r#"{"openapi": "3.0.3", "info": {"title": "T", "version": "1.0.0"}, "paths": {}}"#,
        ).unwrap();

        let versions = scan_versions(dir.path());
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].version, "1.0.0");
    }

    #[test]
    fn resolve_spec_path_file_passthrough() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("api.yaml");
        std::fs::write(&file, "openapi: \"3.0.3\"\ninfo:\n  title: T\n  version: \"1.0.0\"\npaths: {}").unwrap();

        let result = resolve_spec_path(&file, None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), file);
    }

    #[test]
    fn resolve_spec_path_dir_requires_version() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("v1.yaml"), "openapi: \"3.0.3\"\ninfo:\n  title: T\n  version: \"1.0.0\"\npaths: {}").unwrap();

        let result = resolve_spec_path(dir.path(), None);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("--version"), "error should mention --version: {err}");
    }

    #[test]
    fn resolve_spec_path_exact_version_match() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("v1.yaml");
        std::fs::write(&file, "openapi: \"3.0.3\"\ninfo:\n  title: T\n  version: \"1.0.0\"\npaths: {}").unwrap();
        std::fs::write(dir.path().join("v2.yaml"), "openapi: \"3.0.3\"\ninfo:\n  title: T\n  version: \"2.0.0\"\npaths: {}").unwrap();

        let result = resolve_spec_path(dir.path(), Some("1.0.0"));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), file);
    }

    #[test]
    fn resolve_spec_path_file_stem_match() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("v1.yaml");
        std::fs::write(&file, "openapi: \"3.0.3\"\ninfo:\n  title: T\n  version: \"1.0.0\"\npaths: {}").unwrap();

        let result = resolve_spec_path(dir.path(), Some("v1"));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), file);
    }

    #[test]
    fn resolve_spec_path_version_not_found() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("v1.yaml"), "openapi: \"3.0.3\"\ninfo:\n  title: T\n  version: \"1.0.0\"\npaths: {}").unwrap();

        let result = resolve_spec_path(dir.path(), Some("99.0.0"));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not found"), "error should mention not found: {err}");
        assert!(err.contains("1.0.0"), "error should list available versions: {err}");
    }

    #[test]
    fn resolve_spec_path_nested_dir_match() {
        let dir = tempfile::tempdir().unwrap();
        let v1_dir = dir.path().join("v1");
        std::fs::create_dir_all(&v1_dir).unwrap();
        let file = v1_dir.join("openapi.yaml");
        std::fs::write(&file, "openapi: \"3.0.3\"\ninfo:\n  title: T\n  version: \"1.0.0\"\npaths: {}").unwrap();

        let result = resolve_spec_path(dir.path(), Some("v1"));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), file);
    }

    // ── const keyword tests ────────────────────────────────────────────

    #[test]
    fn const_to_enum_single_value() {
        let mut json = serde_json::json!({
            "type": "string",
            "const": "active"
        });
        downgrade_value(&mut json);
        assert!(json.get("const").is_none(), "const should be removed");
        let en = json["enum"].as_array().unwrap();
        assert_eq!(en.len(), 1);
        assert_eq!(en[0], "active");
    }

    #[test]
    fn const_to_enum_numeric_value() {
        let mut json = serde_json::json!({
            "type": "integer",
            "const": 42
        });
        downgrade_value(&mut json);
        let en = json["enum"].as_array().unwrap();
        assert_eq!(en.len(), 1);
        assert_eq!(en[0], 42);
    }

    #[test]
    fn const_to_enum_null_value() {
        let mut json = serde_json::json!({
            "const": null
        });
        downgrade_value(&mut json);
        let en = json["enum"].as_array().unwrap();
        assert_eq!(en.len(), 1);
        assert!(en[0].is_null());
    }

    #[test]
    fn const_to_enum_object_value() {
        let mut json = serde_json::json!({
            "const": { "status": "ok" }
        });
        downgrade_value(&mut json);
        let en = json["enum"].as_array().unwrap();
        assert_eq!(en.len(), 1);
        assert_eq!(en[0], serde_json::json!({ "status": "ok" }));
    }

    #[test]
    fn const_full_pipeline() {
        let yaml = r#"
openapi: "3.1.0"
info:
  title: Test
  version: "1.0.0"
paths: {}
components:
  schemas:
    Status:
      type: string
      const: "active"
"#;
        let result = parse_str(yaml);
        assert!(result.is_ok(), "should parse spec with const: {:?}", result.err());
    }

    // ── dependentRequired tests ────────────────────────────────────────

    #[test]
    fn dependent_required_merges_into_required() {
        let mut json = serde_json::json!({
            "type": "object",
            "required": ["name"],
            "dependentRequired": {
                "name": ["email", "phone"],
                "address": ["zip"]
            }
        });
        downgrade_value(&mut json);
        assert!(json.get("dependentRequired").is_none(), "dependentRequired should be removed");
        let req = json["required"].as_array().unwrap();
        assert!(req.contains(&serde_json::json!("name")));
        assert!(req.contains(&serde_json::json!("email")));
        assert!(req.contains(&serde_json::json!("phone")));
        assert!(req.contains(&serde_json::json!("zip")));
        assert_eq!(req.len(), 4); // no duplicates
    }

    #[test]
    fn dependent_required_no_existing_required() {
        let mut json = serde_json::json!({
            "type": "object",
            "dependentRequired": {
                "name": ["email"]
            }
        });
        downgrade_value(&mut json);
        let req = json["required"].as_array().unwrap();
        assert_eq!(req.len(), 1);
        assert_eq!(req[0], "email");
    }

    #[test]
    fn dependent_required_deduplicates() {
        let mut json = serde_json::json!({
            "type": "object",
            "required": ["email"],
            "dependentRequired": {
                "name": ["email"]
            }
        });
        downgrade_value(&mut json);
        let req = json["required"].as_array().unwrap();
        // "email" should appear only once.
        assert_eq!(req.iter().filter(|v| *v == "email").count(), 1);
    }

    #[test]
    fn dependent_required_full_pipeline() {
        let yaml = r#"
openapi: "3.1.0"
info:
  title: Test
  version: "1.0.0"
paths: {}
components:
  schemas:
    User:
      type: object
      required:
        - name
      dependentRequired:
        name:
          - email
          - phone
"#;
        let result = parse_str(yaml);
        assert!(result.is_ok(), "should parse spec with dependentRequired: {:?}", result.err());
    }

    // ── prefixItems tests ──────────────────────────────────────────────

    #[test]
    fn prefix_items_single_to_items() {
        let mut json = serde_json::json!({
            "type": "array",
            "prefixItems": [
                { "type": "number" }
            ]
        });
        downgrade_value(&mut json);
        assert!(json.get("prefixItems").is_none(), "prefixItems should be removed");
        assert_eq!(json["items"]["type"], "number");
    }

    #[test]
    fn prefix_items_multiple_to_first_items() {
        let mut json = serde_json::json!({
            "type": "array",
            "prefixItems": [
                { "type": "number" },
                { "type": "string" }
            ]
        });
        downgrade_value(&mut json);
        assert!(json.get("prefixItems").is_none());
        // Falls back to first prefix item.
        assert_eq!(json["items"]["type"], "number");
    }

    #[test]
    fn prefix_items_empty_is_noop() {
        let mut json = serde_json::json!({
            "type": "array",
            "prefixItems": []
        });
        downgrade_value(&mut json);
        assert!(json.get("prefixItems").is_none());
        assert!(json.get("items").is_none());
    }

    #[test]
    fn prefix_items_full_pipeline() {
        let yaml = r#"
openapi: "3.1.0"
info:
  title: Test
  version: "1.0.0"
paths: {}
components:
  schemas:
    Coordinate:
      type: array
      prefixItems:
        - type: number
        - type: number
"#;
        let result = parse_str(yaml);
        assert!(result.is_ok(), "should parse spec with prefixItems: {:?}", result.err());
    }

    // ── type array with multiple types test ────────────────────────────

    #[test]
    fn type_array_multiple_types_no_null() {
        let mut json = serde_json::json!({
            "type": ["string", "integer"]
        });
        downgrade_value(&mut json);
        // With multiple non-null types and no null, we leave it as-is
        // (no good 3.0 equivalent).
        assert!(json["type"].is_array());
    }

    #[test]
    fn type_array_multiple_types_with_null() {
        let mut json = serde_json::json!({
            "type": ["string", "integer", "null"]
        });
        downgrade_value(&mut json);
        // Multiple non-null types + null: can't simplify to single type.
        assert!(json["type"].is_array());
    }

    // ── detect_31_features tests ───────────────────────────────────────

    #[test]
    fn detect_features_type_array() {
        let json = serde_json::json!({
            "openapi": "3.1.0",
            "info": { "title": "T", "version": "1" },
            "components": {
                "schemas": {
                    "A": { "type": ["string", "null"] }
                }
            }
        });
        let f = detect_31_features(&json);
        assert!(f.uses_type_array);
        assert!(!f.uses_const);
    }

    #[test]
    fn detect_features_const() {
        let json = serde_json::json!({
            "openapi": "3.1.0",
            "info": { "title": "T", "version": "1" },
            "components": {
                "schemas": {
                    "Status": { "const": "active" }
                }
            }
        });
        let f = detect_31_features(&json);
        assert!(f.uses_const);
        assert!(!f.uses_type_array);
    }

    #[test]
    fn detect_features_ref_siblings() {
        let json = serde_json::json!({
            "$ref": "#/components/schemas/Pet",
            "description": "A pet"
        });
        let f = detect_31_features(&json);
        assert!(f.uses_ref_siblings);
    }

    #[test]
    fn detect_features_prefix_items() {
        let json = serde_json::json!({
            "prefixItems": [{ "type": "number" }]
        });
        let f = detect_31_features(&json);
        assert!(f.uses_prefix_items);
    }

    #[test]
    fn detect_features_dependent_required() {
        let json = serde_json::json!({
            "dependentRequired": { "name": ["email"] }
        });
        let f = detect_31_features(&json);
        assert!(f.uses_dependent_required);
    }

    #[test]
    fn detect_features_numeric_exclusive_bounds() {
        let json = serde_json::json!({
            "exclusiveMinimum": 5
        });
        let f = detect_31_features(&json);
        assert!(f.uses_numeric_exclusive_bounds);
    }

    #[test]
    fn detect_features_none() {
        let json = serde_json::json!({
            "openapi": "3.0.3",
            "info": { "title": "T", "version": "1" },
            "paths": {}
        });
        let f = detect_31_features(&json);
        assert_eq!(f, Spec31Features::default());
    }

    #[test]
    fn detect_features_multiple() {
        let json = serde_json::json!({
            "openapi": "3.1.0",
            "info": { "title": "T", "version": "1" },
            "components": {
                "schemas": {
                    "A": { "type": ["string", "null"], "const": "x" },
                    "B": { "prefixItems": [{ "type": "number" }] }
                }
            }
        });
        let f = detect_31_features(&json);
        assert!(f.uses_type_array);
        assert!(f.uses_const);
        assert!(f.uses_prefix_items);
        assert!(!f.uses_ref_siblings);
        assert!(!f.uses_dependent_required);
    }
}
