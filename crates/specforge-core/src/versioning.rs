use crate::ir::Document;

/// Configuration for automatic API versioning.
pub struct VersioningConfig {
    /// How the version is conveyed to the client.
    pub strategy: VersionStrategy,
    /// URL path prefix (e.g. "v1", "v2") used with `UrlPath` strategy.
    pub prefix: Option<String>,
    /// Header name used with `Header` strategy (e.g. "API-Version").
    pub header_name: Option<String>,
}

/// Supported versioning strategies.
pub enum VersionStrategy {
    /// URL path prefix (/v1/pets)
    UrlPath,
    /// Header (API-Version: 1)
    Header,
    /// Query parameter (?version=1)
    QueryParam,
    /// None (no versioning)
    None,
}

/// Apply versioning transformations to a resolved IR document.
///
/// For `UrlPath` strategy, each operation path is prefixed with `/<prefix>`
/// unless it already starts with that prefix.
///
/// For `Header` strategy, a version parameter is added to each operation's
/// parameters list (header location).
///
/// For `QueryParam` strategy, a version parameter is added to each operation's
/// parameters list (query location).
pub fn apply_versioning(doc: &mut Document, config: &VersioningConfig) {
    match config.strategy {
        VersionStrategy::UrlPath => {
            if let Some(ref prefix) = config.prefix {
                let prefix_path = format!("/{prefix}");
                for op in &mut doc.operations {
                    if !op.path.starts_with(&prefix_path) {
                        op.path = format!("{prefix_path}{}", op.path);
                    }
                }
            }
        }
        VersionStrategy::Header => {
            let header_name = config
                .header_name
                .clone()
                .unwrap_or_else(|| "API-Version".to_string());
            for op in &mut doc.operations {
                // Skip if this header parameter already exists.
                let has_header = op.parameters.iter().any(|p| {
                    p.name.to_lowercase() == header_name.to_lowercase()
                        && matches!(p.location, crate::ir::ParamLocation::Header)
                });
                if !has_header {
                    op.parameters.push(crate::ir::Parameter {
                        name: header_name.clone(),
                        location: crate::ir::ParamLocation::Header,
                        ty: crate::ir::Type::Scalar(crate::ir::Scalar::String),
                        required: true,
                        description: Some("API version".to_string()),
                    });
                }
            }
        }
        VersionStrategy::QueryParam => {
            let param_name = config
                .prefix
                .clone()
                .unwrap_or_else(|| "version".to_string());
            for op in &mut doc.operations {
                // Skip if this query parameter already exists.
                let has_param = op.parameters.iter().any(|p| {
                    p.name == param_name && matches!(p.location, crate::ir::ParamLocation::Query)
                });
                if !has_param {
                    op.parameters.push(crate::ir::Parameter {
                        name: param_name.clone(),
                        location: crate::ir::ParamLocation::Query,
                        ty: crate::ir::Type::Scalar(crate::ir::Scalar::String),
                        required: false,
                        description: Some("API version".to_string()),
                    });
                }
            }
        }
        VersionStrategy::None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{HttpMethod, Operation};

    fn make_doc() -> Document {
        Document {
            ir_version: crate::ir::IR_VERSION.to_string(),
            title: "Test".to_string(),
            version: "1.0.0".to_string(),
            base_url: None,
            security: vec![],
            schemas: Default::default(),
            operations: vec![
                Operation {
                    operation_id: "listPets".to_string(),
                    method: HttpMethod::Get,
                    path: "/pets".to_string(),
                    tag: None,
                    summary: None,
                    description: None,
                    parameters: vec![],
                    request_body: None,
                    responses: vec![],
                    retry_policy: None,
                },
                Operation {
                    operation_id: "getPet".to_string(),
                    method: HttpMethod::Get,
                    path: "/pets/{petId}".to_string(),
                    tag: None,
                    summary: None,
                    description: None,
                    parameters: vec![],
                    request_body: None,
                    responses: vec![],
                    retry_policy: None,
                },
            ],
            webhooks: vec![],
        }
    }

    #[test]
    fn url_path_prefix() {
        let mut doc = make_doc();
        let config = VersioningConfig {
            strategy: VersionStrategy::UrlPath,
            prefix: Some("v2".to_string()),
            header_name: None,
        };
        apply_versioning(&mut doc, &config);
        assert_eq!(doc.operations[0].path, "/v2/pets");
        assert_eq!(doc.operations[1].path, "/v2/pets/{petId}");
    }

    #[test]
    fn url_path_no_double_prefix() {
        let mut doc = make_doc();
        doc.operations[0].path = "/v1/pets".to_string();
        let config = VersioningConfig {
            strategy: VersionStrategy::UrlPath,
            prefix: Some("v1".to_string()),
            header_name: None,
        };
        apply_versioning(&mut doc, &config);
        assert_eq!(doc.operations[0].path, "/v1/pets");
    }

    #[test]
    fn header_strategy() {
        let mut doc = make_doc();
        let config = VersioningConfig {
            strategy: VersionStrategy::Header,
            prefix: None,
            header_name: Some("X-API-Version".to_string()),
        };
        apply_versioning(&mut doc, &config);
        assert_eq!(doc.operations[0].parameters.len(), 1);
        assert_eq!(doc.operations[0].parameters[0].name, "X-API-Version");
        assert!(matches!(
            doc.operations[0].parameters[0].location,
            crate::ir::ParamLocation::Header
        ));
    }

    #[test]
    fn header_strategy_default_name() {
        let mut doc = make_doc();
        let config = VersioningConfig {
            strategy: VersionStrategy::Header,
            prefix: None,
            header_name: None,
        };
        apply_versioning(&mut doc, &config);
        assert_eq!(doc.operations[0].parameters[0].name, "API-Version");
    }

    #[test]
    fn query_param_strategy() {
        let mut doc = make_doc();
        let config = VersioningConfig {
            strategy: VersionStrategy::QueryParam,
            prefix: Some("api_version".to_string()),
            header_name: None,
        };
        apply_versioning(&mut doc, &config);
        assert_eq!(doc.operations[0].parameters.len(), 1);
        assert_eq!(doc.operations[0].parameters[0].name, "api_version");
        assert!(matches!(
            doc.operations[0].parameters[0].location,
            crate::ir::ParamLocation::Query
        ));
    }

    #[test]
    fn none_strategy() {
        let mut doc = make_doc();
        let original_path = doc.operations[0].path.clone();
        let config = VersioningConfig {
            strategy: VersionStrategy::None,
            prefix: None,
            header_name: None,
        };
        apply_versioning(&mut doc, &config);
        assert_eq!(doc.operations[0].path, original_path);
    }
}
