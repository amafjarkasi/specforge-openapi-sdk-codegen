//! Parsing raw OpenAPI YAML/JSON into `openapiv3` types.
//!
//! JSON and YAML are both supported; detection is content-based (parse-fallback)
//! rather than by extension so callers don't have to guess.

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
    if let Ok(json) = serde_json::from_str::<JsonValue>(text.trim_start()) {
        return serde_json::from_value::<OpenAPI>(json).map_err(SpecError::Json);
    }
    serde_yaml::from_str::<OpenAPI>(text).map_err(SpecError::Yaml)
}
