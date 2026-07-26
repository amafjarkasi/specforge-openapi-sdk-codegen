//! Error types for spec parsing, resolution, and IR construction.

use thiserror::Error;

/// Errors produced while loading or parsing an OpenAPI document.
#[derive(Debug, Error)]
pub enum SpecError {
    #[error("failed to read spec file {path:?}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse spec as JSON: {0}")]
    Json(#[from] serde_json::Error),

    #[error("failed to parse spec as YAML: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("document is not a valid OpenAPI 3 document: {0}")]
    Invalid(String),
}

/// Errors produced while resolving `$ref` pointers or walking schemas.
#[derive(Debug, Error)]
pub enum ResolveError {
    #[error("invalid $ref {0:?}: must start with '#/'")]
    InvalidRef(String),

    #[error("unresolved $ref {0:?}: target not found in document")]
    UnresolvedRef(String),

    #[error("maximum reference depth ({max_depth}) exceeded at {ref_path:?}; this usually indicates a cycle")]
    RefDepthExceeded { ref_path: String, max_depth: usize },

    #[error("schema has no resolvable type at {context}")]
    UntypedSchema { context: String },
}
