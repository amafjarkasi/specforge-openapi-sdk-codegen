use crate::ir::{Document, SecurityScheme};
use openapiv3::{OpenAPI, SecurityRequirement, SecurityScheme as OApiSecurityScheme};
use std::collections::BTreeMap;

#[derive(Debug, Clone, serde::Serialize)]
pub struct SecurityReport {
    pub schemes: Vec<SecuritySchemeInfo>,
    pub operations: Vec<OperationSecurity>,
    pub issues: Vec<SecurityIssue>,
}
#[derive(Debug, Clone, serde::Serialize)]
pub struct SecuritySchemeInfo {
    pub name: String,
    pub kind: String,
    pub header: Option<String>,
    pub bearer_format: Option<String>,
}
#[derive(Debug, Clone, serde::Serialize)]
pub struct OperationSecurity {
    pub operation_id: String,
    pub method: String,
    pub path: String,
    pub requires_auth: bool,
    pub scheme: Option<String>,
    pub has_override: bool,
}
#[derive(Debug, Clone, serde::Serialize)]
pub struct SecurityIssue {
    pub severity: String,
    pub message: String,
    pub path: String,
}

pub fn analyze_security(doc: &Document) -> SecurityReport {
    let schemes: Vec<SecuritySchemeInfo> = doc.security.iter().map(scheme_info_from_ir).collect();
    let has_global_auth = !doc.security.is_empty();
    let operations = doc
        .operations
        .iter()
        .map(|op| OperationSecurity {
            operation_id: op.operation_id.clone(),
            method: op.method.as_str().to_string(),
            path: op.path.clone(),
            requires_auth: has_global_auth,
            scheme: doc.security.first().map(scheme_label),
            has_override: false,
        })
        .collect::<Vec<_>>();
    let mut issues = Vec::new();
    analyze_common_issues(&schemes, &operations, &mut issues);
    SecurityReport {
        schemes,
        operations,
        issues,
    }
}

pub fn analyze_security_detailed(doc: &Document, spec: &OpenAPI) -> SecurityReport {
    let mut scheme_map: BTreeMap<String, OApiSecurityScheme> = BTreeMap::new();
    if let Some(components) = &spec.components {
        for (name, scheme_or) in &components.security_schemes {
            if let openapiv3::ReferenceOr::Item(scheme) = scheme_or {
                scheme_map.insert(name.clone(), scheme.clone());
            }
        }
    }
    let schemes: Vec<SecuritySchemeInfo> = scheme_map
        .iter()
        .map(|(n, s)| scheme_info_from_raw(n, s))
        .collect();
    let global_names = extract_scheme_names(&spec.security);
    let has_global_auth = !global_names.is_empty();
    let op_map = build_op_security_map(spec);
    let mut operations = Vec::new();
    for ir_op in &doc.operations {
        let key = op_key(ir_op.method.as_str(), &ir_op.path);
        let (req_auth, sch, ov) = match op_map.get(&key) {
            Some(Some(r)) if r.is_empty() => (false, None, true),
            Some(Some(r)) => {
                let n: Vec<String> = r.iter().flat_map(|x| x.keys().cloned()).collect();
                (true, n.first().cloned(), true)
            }
            _ => (has_global_auth, global_names.first().cloned(), false),
        };
        operations.push(OperationSecurity {
            operation_id: ir_op.operation_id.clone(),
            method: ir_op.method.as_str().to_string(),
            path: ir_op.path.clone(),
            requires_auth: req_auth,
            scheme: sch,
            has_override: ov,
        });
    }
    let mut issues = Vec::new();
    analyze_common_issues(&schemes, &operations, &mut issues);
    analyze_detailed_issues(spec, &operations, &global_names, &mut issues);
    SecurityReport {
        schemes,
        operations,
        issues,
    }
}

fn scheme_info_from_ir(s: &SecurityScheme) -> SecuritySchemeInfo {
    match s {
        SecurityScheme::HttpBearer => SecuritySchemeInfo {
            name: "BearerAuth".into(),
            kind: "bearer".into(),
            header: Some("Authorization".into()),
            bearer_format: None,
        },
        SecurityScheme::ApiKey { header } => SecuritySchemeInfo {
            name: format!("ApiKey({})", header),
            kind: "apikey".into(),
            header: Some(header.clone()),
            bearer_format: None,
        },
    }
}
fn scheme_info_from_raw(name: &str, s: &OApiSecurityScheme) -> SecuritySchemeInfo {
    match s {
        OApiSecurityScheme::HTTP {
            scheme,
            bearer_format,
            ..
        } => SecuritySchemeInfo {
            name: name.into(),
            kind: scheme.to_lowercase(),
            header: if scheme.eq_ignore_ascii_case("bearer") {
                Some("Authorization".into())
            } else {
                None
            },
            bearer_format: bearer_format.clone(),
        },
        OApiSecurityScheme::APIKey { name: kn, .. } => SecuritySchemeInfo {
            name: name.into(),
            kind: "apikey".into(),
            header: Some(kn.clone()),
            bearer_format: None,
        },
        OApiSecurityScheme::OAuth2 { .. } => SecuritySchemeInfo {
            name: name.into(),
            kind: "oauth2".into(),
            header: None,
            bearer_format: None,
        },
        OApiSecurityScheme::OpenIDConnect { .. } => SecuritySchemeInfo {
            name: name.into(),
            kind: "openidconnect".into(),
            header: None,
            bearer_format: None,
        },
    }
}
fn scheme_label(s: &SecurityScheme) -> String {
    match s {
        SecurityScheme::HttpBearer => "bearer".into(),
        SecurityScheme::ApiKey { header } => format!("apikey ({})", header),
    }
}

fn extract_scheme_names(security: &Option<Vec<SecurityRequirement>>) -> Vec<String> {
    let Some(reqs) = security else {
        return Vec::new();
    };
    reqs.iter().flat_map(|r| r.keys().cloned()).collect()
}

fn op_key(method: &str, path: &str) -> String {
    format!("{} {}", method.to_uppercase(), path)
}

fn build_op_security_map(spec: &OpenAPI) -> BTreeMap<String, Option<Vec<SecurityRequirement>>> {
    let mut map = BTreeMap::new();
    let paths = &spec.paths;
    for (path, item_or) in &paths.paths {
        let item = match item_or {
            openapiv3::ReferenceOr::Item(i) => i,
            _ => continue,
        };
        for (m, op_opt) in [
            ("get", &item.get),
            ("post", &item.post),
            ("put", &item.put),
            ("patch", &item.patch),
            ("delete", &item.delete),
            ("head", &item.head),
            ("options", &item.options),
        ] {
            if let Some(op) = op_opt {
                map.insert(
                    format!("{} {}", m.to_uppercase(), path),
                    op.security.clone(),
                );
            }
        }
    }
    map
}
fn analyze_common_issues(
    schemes: &[SecuritySchemeInfo],
    operations: &[OperationSecurity],
    issues: &mut Vec<SecurityIssue>,
) {
    if schemes.is_empty() {
        issues.push(SecurityIssue {
            severity: "warning".into(),
            message: "No security schemes defined in the specification".into(),
            path: "/".into(),
        });
    }
    for op in operations {
        if matches!(op.method.as_str(), "POST" | "PUT" | "PATCH" | "DELETE") && !op.requires_auth {
            issues.push(SecurityIssue {
                severity: "warning".into(),
                message: format!(
                    "Write operation {} {} does not require authentication",
                    op.method, op.path
                ),
                path: op.path.clone(),
            });
        }
    }
    let auth: Vec<Option<&str>> = operations
        .iter()
        .filter(|o| o.requires_auth)
        .map(|o| o.scheme.as_deref())
        .collect();
    if auth.iter().collect::<std::collections::HashSet<_>>().len() > 1 {
        issues.push(SecurityIssue {
            severity: "info".into(),
            message: "Operations use mixed authentication schemes".into(),
            path: "/".into(),
        });
    }
}
fn analyze_detailed_issues(
    spec: &OpenAPI,
    operations: &[OperationSecurity],
    global_names: &[String],
    issues: &mut Vec<SecurityIssue>,
) {
    // Operations that explicitly turn off auth.
    for op in operations {
        if op.has_override && !op.requires_auth {
            issues.push(SecurityIssue {
                severity: "info".into(),
                message: format!(
                    "Operation {} {} explicitly disables authentication",
                    op.method, op.path
                ),
                path: op.path.clone(),
            });
        }
    }
    // Schemes referenced by at least one authenticated operation.
    let used: std::collections::HashSet<&str> = operations
        .iter()
        .filter(|o| o.requires_auth)
        .filter_map(|o| o.scheme.as_deref())
        .collect();
    if let Some(components) = &spec.components {
        for (name, scheme_or) in &components.security_schemes {
            // Defined-but-unreferenced schemes.
            if let openapiv3::ReferenceOr::Item(_s) = scheme_or {
                if !used.contains(name.as_str()) && !global_names.contains(name) {
                    issues.push(SecurityIssue {
                        severity: "info".into(),
                        message: format!(
                            "Security scheme '{}' is defined but not referenced by any operation",
                            name
                        ),
                        path: format!("/components/securitySchemes/{}", name),
                    });
                }
            }
            // OAuth2 schemes with no configured flows.
            if let openapiv3::ReferenceOr::Item(OApiSecurityScheme::OAuth2 { flows, .. }) =
                scheme_or
            {
                if flows.implicit.is_none()
                    && flows.password.is_none()
                    && flows.client_credentials.is_none()
                    && flows.authorization_code.is_none()
                {
                    issues.push(SecurityIssue {
                        severity: "warning".into(),
                        message: format!("OAuth2 scheme '{}' has no configured flows", name),
                        path: format!("/components/securitySchemes/{}", name),
                    });
                }
            }
        }
    }
    // Global security referencing an undefined scheme.
    for name in global_names {
        if let Some(components) = &spec.components {
            if !components.security_schemes.contains_key(name) {
                issues.push(SecurityIssue {
                    severity: "error".into(),
                    message: format!("Global security references undefined scheme '{}'", name),
                    path: "/security".into(),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn analyze_empty_doc() {
        let doc = Document {
            ir_version: crate::ir::IR_VERSION.to_string(),
            title: "T".into(),
            version: "1".into(),
            base_url: None,
            security: vec![],
            schemas: Default::default(),
            operations: vec![],
            webhooks: vec![],
        };
        let r = analyze_security(&doc);
        assert!(r.schemes.is_empty());
        assert_eq!(r.issues.len(), 1);
    }
    #[test]
    fn analyze_bearer() {
        use crate::ir::Operation;
        let doc = Document {
            ir_version: crate::ir::IR_VERSION.to_string(),
            title: "T".into(),
            version: "1".into(),
            base_url: None,
            security: vec![SecurityScheme::HttpBearer],
            schemas: Default::default(),
            operations: vec![Operation {
                operation_id: "list".into(),
                method: crate::ir::HttpMethod::Get,
                path: "/items".into(),
                tag: None,
                summary: None,
                description: None,
                parameters: vec![],
                request_body: None,
                responses: vec![],
                retry_policy: None,
            }],
            webhooks: vec![],
        };
        let r = analyze_security(&doc);
        assert_eq!(r.schemes[0].kind, "bearer");
    }

    #[test]
    fn detailed_detects_no_global_auth() {
        // A spec with no security at all should warn.
        let doc = Document {
            ir_version: crate::ir::IR_VERSION.to_string(),
            title: "T".into(),
            version: "1".into(),
            base_url: None,
            security: vec![],
            schemas: Default::default(),
            operations: vec![],
            webhooks: vec![],
        };
        // analyze_security_detailed needs an OpenAPI spec; build a minimal one.
        let spec = openapiv3::OpenAPI {
            openapi: "3.0.3".into(),
            info: openapiv3::Info {
                title: "T".into(),
                version: "1".into(),
                ..Default::default()
            },
            paths: Default::default(),
            components: None,
            security: None,
            ..Default::default()
        };
        let r = analyze_security_detailed(&doc, &spec);
        // Should have at least the "no security schemes" warning.
        assert!(r.issues.iter().any(|i| i.message.contains("No security schemes")));
    }
}
