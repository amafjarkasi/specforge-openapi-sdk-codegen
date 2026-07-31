//! API verification against an OpenAPI spec.
//!
//! Hits live endpoints described in the spec, compares response status codes,
//! and validates response bodies against the declared schemas.

use crate::ir::{Document, HttpMethod, Type};
use serde_json::Value as JsonValue;

/// Result of verifying a single endpoint.
#[derive(Debug, Clone, serde::Serialize)]
pub struct VerifyResult {
    /// The endpoint path template (e.g. `/pets/{petId}`).
    pub endpoint: String,
    /// HTTP method (e.g. `GET`).
    pub method: String,
    /// The actual HTTP status code returned by the server, or `"error"` if the
    /// request itself failed.
    pub status: String,
    /// Whether the response body matched the declared schema.
    pub schema_match: bool,
    /// Any issues found during verification.
    pub issues: Vec<String>,
}

/// Options that control verification behaviour.
pub struct VerifyOptions {
    /// Base URL of the running API (e.g. `http://localhost:3000`).
    pub base_url: String,
    /// Optional `Authorization` header value.
    pub auth: Option<String>,
    /// Per-request timeout in milliseconds.
    pub timeout_ms: u64,
}

/// Verify every operation declared in `doc` against a live API at `base_url`.
///
/// For each operation the function:
/// 1. Makes an HTTP request, substituting path parameters with placeholder values.
/// 2. Compares the returned status code with the first declared response.
/// 3. Attempts to validate the response body against the declared schema.
/// 4. Collects any mismatches into [`VerifyResult::issues`].
pub fn verify_api(doc: &Document, opts: &VerifyOptions) -> Vec<VerifyResult> {
    let mut results = Vec::with_capacity(doc.operations.len());

    let client = build_client(opts);

    for op in &doc.operations {
        let result = verify_operation(&client, doc, op, opts);
        results.push(result);
    }

    results
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn build_client(opts: &VerifyOptions) -> reqwest::blocking::Client {
    let timeout = std::time::Duration::from_millis(opts.timeout_ms);
    let mut builder = reqwest::blocking::Client::builder()
        .timeout(timeout)
        .danger_accept_invalid_certs(true);

    // Default user-agent.
    builder = builder.user_agent("specforge-verify/0.1");

    builder.build().expect("failed to build HTTP client")
}

fn verify_operation(
    client: &reqwest::blocking::Client,
    doc: &Document,
    op: &crate::ir::Operation,
    opts: &VerifyOptions,
) -> VerifyResult {
    let method_str = op.method.upper();
    let path = substitute_path_params(&op.path);
    let url = format!("{}{}", opts.base_url.trim_end_matches('/'), path);

    let mut request_builder = match op.method {
        HttpMethod::Get => client.get(&url),
        HttpMethod::Post => client.post(&url),
        HttpMethod::Put => client.put(&url),
        HttpMethod::Patch => client.patch(&url),
        HttpMethod::Delete => client.delete(&url),
        HttpMethod::Head => client.head(&url),
        HttpMethod::Options => client.request(reqwest::Method::OPTIONS, &url),
    };

    if let Some(ref auth) = opts.auth {
        request_builder = request_builder.header("Authorization", auth);
    }

    let response = match request_builder.send() {
        Ok(r) => r,
        Err(e) => {
            return VerifyResult {
                endpoint: op.path.clone(),
                method: method_str.to_string(),
                status: "error".to_string(),
                schema_match: false,
                issues: vec![format!("request failed: {e}")],
            };
        }
    };

    let actual_status = response.status().as_u16();
    let mut issues: Vec<String> = Vec::new();

    // --- Status code check ---
    let expected_status = op
        .responses
        .first()
        .map(|r| r.status.as_str())
        .unwrap_or("200");

    let expected_status_code: u16 = expected_status.parse().unwrap_or(200);
    if actual_status != expected_status_code {
        issues.push(format!(
            "status mismatch: expected {expected_status_code}, got {actual_status}"
        ));
    }

    // --- Schema validation ---
    let mut schema_match = true;

    // Attempt to read body as JSON.
    let body_text = response.text().unwrap_or_default();

    let response_body: JsonValue = match serde_json::from_str(&body_text) {
        Ok(v) => v,
        Err(_) => {
            // Non-JSON body – we cannot schema-validate but treat as info.
            if (200..300).contains(&expected_status_code) {
                // Expected JSON but got something else.
                issues.push("response body is not valid JSON".to_string());
                schema_match = false;
            }
            return VerifyResult {
                endpoint: op.path.clone(),
                method: method_str.to_string(),
                status: actual_status.to_string(),
                schema_match,
                issues,
            };
        }
    };

    // Find the expected response body type from the IR.
    let expected_body_type = op
        .responses
        .iter()
        .find(|r| r.status == expected_status || r.status == actual_status.to_string())
        .and_then(|r| r.body.as_ref());

    if let Some(expected_type) = expected_body_type {
        if !validate_json_against_type(&response_body, expected_type, doc) {
            schema_match = false;
            issues.push("response body does not match declared schema".to_string());
        }
    }

    VerifyResult {
        endpoint: op.path.clone(),
        method: method_str.to_string(),
        status: actual_status.to_string(),
        schema_match,
        issues,
    }
}

/// Replace path parameter placeholders (e.g. `{petId}`) with dummy values
/// so that the URL is valid enough for a probe request.
fn substitute_path_params(path: &str) -> String {
    let mut result = path.to_string();
    // Replace {paramName} with a dummy integer.
    while let Some(start) = result.find('{') {
        if let Some(end) = result[start..].find('}') {
            let param = &result[start + 1..start + end];
            // Use a simple dummy: numeric params get 1, others get "test".
            let dummy = if param.contains("id") || param.contains("Id") {
                "1".to_string()
            } else {
                "test".to_string()
            };
            result = format!(
                "{}{}{}",
                &result[..start],
                dummy,
                &result[start + end + 1..]
            );
        } else {
            break;
        }
    }
    result
}

/// Perform a lightweight structural validation of a JSON value against an IR
/// [`Type`]. This is a best-effort check: it verifies that required fields exist,
/// that array items have the right kind, and so on. It does **not** perform full
/// JSON Schema validation (that would require converting the IR type back to a
/// JSON Schema).
fn validate_json_against_type(value: &JsonValue, ty: &Type, doc: &Document) -> bool {
    match ty {
        Type::Scalar(scalar) => match scalar {
            crate::ir::Scalar::String
            | crate::ir::Scalar::DateTime
            | crate::ir::Scalar::Uuid
            | crate::ir::Scalar::Base64 => value.is_string(),
            crate::ir::Scalar::Integer | crate::ir::Scalar::Integer64 => {
                value.is_i64() || value.is_u64()
            }
            crate::ir::Scalar::Float => value.is_f64() || value.is_i64(),
            crate::ir::Scalar::Boolean => value.is_boolean(),
            crate::ir::Scalar::Binary => value.is_string() || value.is_object(),
        },
        Type::StringEnum { .. } => value.is_string(),
        Type::Array { item, .. } => {
            if let Some(arr) = value.as_array() {
                arr.iter().all(|v| validate_json_against_type(v, item, doc))
            } else {
                false
            }
        }
        Type::Map { value: map_val } => {
            if let Some(obj) = value.as_object() {
                obj.values()
                    .all(|v| validate_json_against_type(v, map_val, doc))
            } else {
                false
            }
        }
        Type::Reference { name, .. } => {
            // Resolve the reference to its model and recurse.
            if let Some(model) = doc.schemas.get(name) {
                match model {
                    crate::ir::Model::Object(obj_model) => {
                        validate_json_against_object(value, obj_model, doc)
                    }
                    crate::ir::Model::Enum(enum_model) => {
                        if let Some(s) = value.as_str() {
                            enum_model.variants.iter().any(|v| v.value == s)
                        } else {
                            false
                        }
                    }
                }
            } else {
                // Unknown reference – accept anything.
                true
            }
        }
        Type::Composition(comp) => {
            // For allOf / oneOf / anyOf, at least one member must match.
            comp.members
                .iter()
                .any(|m| validate_json_against_type(value, m, doc))
        }
        Type::Any | Type::Unknown => true,
    }
}

/// Validate a JSON object against an IR ObjectModel.
fn validate_json_against_object(
    value: &JsonValue,
    obj: &crate::ir::ObjectModel,
    doc: &Document,
) -> bool {
    let map = match value.as_object() {
        Some(m) => m,
        None => return false,
    };

    // Check that all declared required properties are present.
    for prop in &obj.properties {
        if prop.required && !map.contains_key(&prop.name) {
            return false;
        }
    }

    // Validate each present property's value.
    for prop in &obj.properties {
        if let Some(val) = map.get(&prop.name) {
            if !validate_json_against_type(val, &prop.ty, doc) {
                return false;
            }
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitute_path_params_simple() {
        assert_eq!(substitute_path_params("/pets"), "/pets");
        assert_eq!(substitute_path_params("/pets/{petId}"), "/pets/1");
        assert_eq!(
            substitute_path_params("/orgs/{orgId}/repos/{repoId}"),
            "/orgs/1/repos/1"
        );
        assert_eq!(substitute_path_params("/search/{query}"), "/search/test");
    }
}
