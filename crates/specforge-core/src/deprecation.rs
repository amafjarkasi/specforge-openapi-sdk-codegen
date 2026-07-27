//! Deprecation tracking for OpenAPI specs.
//!
//! Scans resolved IR documents for deprecated operations, schemas, parameters,
//! and responses. Deprecation signals include:
//! - OpenAPI `deprecated: true` on operations (propagated via summary/description)
//! - "deprecated" keyword in operation summaries or schema descriptions
//! - `x-deprecated` extension messages

use crate::ir::{Document, Model};

/// The kind of deprecated element found in the spec.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeprecationKind {
    /// An API operation (endpoint) is deprecated.
    Operation,
    /// A named schema/model is deprecated.
    Schema,
    /// A parameter is deprecated.
    Parameter,
    /// A response variant is deprecated.
    Response,
}

impl std::fmt::Display for DeprecationKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeprecationKind::Operation => write!(f, "operation"),
            DeprecationKind::Schema => write!(f, "schema"),
            DeprecationKind::Parameter => write!(f, "parameter"),
            DeprecationKind::Response => write!(f, "response"),
        }
    }
}

/// Information about a single deprecated element.
#[derive(Debug, Clone)]
pub struct DeprecationInfo {
    /// What kind of element is deprecated.
    pub kind: DeprecationKind,
    /// The name or identifier of the deprecated element.
    pub name: String,
    /// A dotted path for locating this element (e.g. `operations.getPets`).
    pub path: String,
    /// Optional human-readable deprecation message (from summary/description).
    pub message: Option<String>,
    /// Optional suggested alternative (extracted from deprecation text).
    pub alternative: Option<String>,
}

impl std::fmt::Display for DeprecationInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.kind, self.name)?;
        if let Some(msg) = &self.message {
            write!(f, " -- {msg}")?;
        }
        if let Some(alt) = &self.alternative {
            write!(f, " (use {alt} instead)")?;
        }
        Ok(())
    }
}

/// Scan a resolved document for all deprecated elements.
///
/// Detection heuristics:
/// - Operations whose summary or description contains "deprecated" (case-insensitive)
/// - Schemas whose description contains "deprecated" (case-insensitive)
/// - Parameters whose description contains "deprecated" (case-insensitive)
/// - Responses whose description contains "deprecated" (case-insensitive)
///
/// When a "use X instead" or "replaced by X" pattern is found in the text,
/// it is extracted as the `alternative`.
pub fn find_deprecations(doc: &Document) -> Vec<DeprecationInfo> {
    let mut deprecations = Vec::new();

    // Scan operations.
    for op in &doc.operations {
        let mut dep_msg: Option<String> = None;

        if let Some(summary) = &op.summary {
            if contains_deprecated(summary) {
                dep_msg = Some(summary.clone());
            }
        }

        if dep_msg.is_none() {
            if let Some(desc) = &op.description {
                if contains_deprecated(desc) {
                    dep_msg = Some(desc.clone());
                }
            }
        }

        if let Some(msg) = dep_msg {
            let alternative = extract_alternative(&msg);
            deprecations.push(DeprecationInfo {
                kind: DeprecationKind::Operation,
                name: op.operation_id.clone(),
                path: format!("operations.{}", op.operation_id),
                message: Some(msg),
                alternative,
            });
        }

        // Scan parameters within operations.
        for param in &op.parameters {
            if let Some(desc) = &param.description {
                if contains_deprecated(desc) {
                    deprecations.push(DeprecationInfo {
                        kind: DeprecationKind::Parameter,
                        name: param.name.clone(),
                        path: format!("operations.{}.params.{}", op.operation_id, param.name),
                        message: Some(desc.clone()),
                        alternative: extract_alternative(desc),
                    });
                }
            }
        }

        // Scan responses within operations.
        for resp in &op.responses {
            if let Some(desc) = &resp.description {
                if contains_deprecated(desc) {
                    deprecations.push(DeprecationInfo {
                        kind: DeprecationKind::Response,
                        name: format!("{} {}", resp.status, op.operation_id),
                        path: format!("operations.{}.responses.{}", op.operation_id, resp.status),
                        message: Some(desc.clone()),
                        alternative: extract_alternative(desc),
                    });
                }
            }
        }
    }

    // Scan schemas.
    for (name, model) in doc.schemas.iter() {
        let desc = match model {
            Model::Object(o) => o.description.as_deref(),
            Model::Enum(e) => e.description.as_deref(),
        };

        if let Some(desc) = desc {
            if contains_deprecated(desc) {
                deprecations.push(DeprecationInfo {
                    kind: DeprecationKind::Schema,
                    name: name.clone(),
                    path: format!("schemas.{name}"),
                    message: Some(desc.to_string()),
                    alternative: extract_alternative(desc),
                });
            }
        }
    }

    deprecations
}

/// Check if text contains the word "deprecated" (case-insensitive).
fn contains_deprecated(text: &str) -> bool {
    text.to_lowercase().contains("deprecated")
}

/// Try to extract a suggested alternative from deprecation text.
///
/// Looks for patterns like:
/// - "Use X instead"
/// - "use X instead"
/// - "replaced by X"
/// - "replaced with X"
/// - "migrate to X"
fn extract_alternative(text: &str) -> Option<String> {
    let lower = text.to_lowercase();

    // "use <alternative> instead"
    if let Some(pos) = lower.find("use ") {
        let after = &text[pos + 4..];
        if let Some(end) = after.to_lowercase().find(" instead") {
            let alt = after[..end].trim();
            if !alt.is_empty() {
                return Some(alt.to_string());
            }
        }
    }

    // "replaced by <alternative>" or "replaced with <alternative>"
    for pattern in &["replaced by ", "replaced with "] {
        if let Some(pos) = lower.find(pattern) {
            let after = &text[pos + pattern.len()..];
            // Take until end of sentence or end of text.
            let end = after
                .find(|c: char| c == '.' || c == ',' || c == '\n' || c == ';')
                .unwrap_or(after.len());
            let alt = after[..end].trim();
            if !alt.is_empty() {
                return Some(alt.to_string());
            }
        }
    }

    // "migrate to <alternative>"
    if let Some(pos) = lower.find("migrate to ") {
        let after = &text[pos + 11..];
        let end = after
            .find(|c: char| c == '.' || c == ',' || c == '\n' || c == ';')
            .unwrap_or(after.len());
        let alt = after[..end].trim();
        if !alt.is_empty() {
            return Some(alt.to_string());
        }
    }

    None
}

/// Generate a migration guide by comparing two document versions.
///
/// Produces a Markdown document listing deprecated operations, removed
/// operations, new required parameters, and schema changes.
pub fn generate_migration_guide(
    old_doc: &Document,
    new_doc: &Document,
    old_version: &str,
    new_version: &str,
) -> String {
    let mut out = String::new();

    out.push_str(&format!(
        "# Migration Guide: {old_version} -> {new_version}\n\n"
    ));

    // Deprecated operations in new spec.
    let new_deprecations = find_deprecations(new_doc);
    let deprecated_ops: Vec<&DeprecationInfo> = new_deprecations
        .iter()
        .filter(|d| d.kind == DeprecationKind::Operation)
        .collect();
    if !deprecated_ops.is_empty() {
        out.push_str("## Deprecated Operations\n\n");
        for dep in &deprecated_ops {
            // Try to find the HTTP method + path from the operation.
            if let Some(op) = new_doc
                .operations
                .iter()
                .find(|o| o.operation_id == dep.name)
            {
                let endpoint = format!("{} {}", op.method.upper(), op.path);
                if let Some(alt) = &dep.alternative {
                    out.push_str(&format!(
                        "- `{}` ({}) -- Use `{alt}` instead\n",
                        dep.name, endpoint
                    ));
                } else {
                    out.push_str(&format!(
                        "- `{}` ({}) -- Deprecated\n",
                        dep.name, endpoint
                    ));
                }
            } else {
                out.push_str(&format!("- `{}` -- Deprecated\n", dep.name));
            }
        }
        out.push('\n');
    }

    // Deprecated schemas in new spec.
    let deprecated_schemas: Vec<&DeprecationInfo> = new_deprecations
        .iter()
        .filter(|d| d.kind == DeprecationKind::Schema)
        .collect();
    if !deprecated_schemas.is_empty() {
        out.push_str("## Deprecated Schemas\n\n");
        for dep in &deprecated_schemas {
            if let Some(alt) = &dep.alternative {
                out.push_str(&format!(
                    "- `{}` -- Use `{alt}` instead\n",
                    dep.name
                ));
            } else {
                out.push_str(&format!("- `{}` -- Deprecated\n", dep.name));
            }
        }
        out.push('\n');
    }

    // Removed operations (in old but not in new).
    let old_op_ids: std::collections::HashSet<&str> = old_doc
        .operations
        .iter()
        .map(|o| o.operation_id.as_str())
        .collect();
    let new_op_ids: std::collections::HashSet<&str> = new_doc
        .operations
        .iter()
        .map(|o| o.operation_id.as_str())
        .collect();
    let removed_ops: Vec<&str> = old_op_ids
        .difference(&new_op_ids)
        .copied()
        .collect::<Vec<_>>();
    if !removed_ops.is_empty() {
        out.push_str("## Removed Operations\n\n");
        for id in &removed_ops {
            if let Some(op) = old_doc
                .operations
                .iter()
                .find(|o| o.operation_id == *id)
            {
                out.push_str(&format!(
                    "- `{}` ({} {}) was removed\n",
                    id,
                    op.method.upper(),
                    op.path
                ));
            } else {
                out.push_str(&format!("- `{}` was removed\n", id));
            }
        }
        out.push('\n');
    }

    // Removed schemas.
    let old_schema_names: std::collections::HashSet<&str> =
        old_doc.schemas.iter().map(|(k, _)| k.as_str()).collect();
    let new_schema_names: std::collections::HashSet<&str> =
        new_doc.schemas.iter().map(|(k, _)| k.as_str()).collect();
    let removed_schemas: Vec<&str> = old_schema_names
        .difference(&new_schema_names)
        .copied()
        .collect();
    if !removed_schemas.is_empty() {
        out.push_str("## Removed Schemas\n\n");
        for name in &removed_schemas {
            out.push_str(&format!("- `{name}` was removed\n"));
        }
        out.push('\n');
    }

    // New required parameters.
    let old_ops_map: std::collections::HashMap<&str, &crate::ir::Operation> = old_doc
        .operations
        .iter()
        .map(|o| (o.operation_id.as_str(), o))
        .collect();
    let new_ops_map: std::collections::HashMap<&str, &crate::ir::Operation> = new_doc
        .operations
        .iter()
        .map(|o| (o.operation_id.as_str(), o))
        .collect();

    let mut new_required_params = Vec::new();
    for (id, new_op) in &new_ops_map {
        if let Some(old_op) = old_ops_map.get(*id) {
            for param in &new_op.parameters {
                if param.required {
                    let existed = old_op.parameters.iter().any(|p| p.name == param.name);
                    if !existed {
                        new_required_params.push((*id, param.name.clone()));
                    }
                }
            }
        }
    }
    if !new_required_params.is_empty() {
        out.push_str("## New Required Parameters\n\n");
        for (op_id, param_name) in &new_required_params {
            if let Some(op) = new_ops_map.get(op_id) {
                out.push_str(&format!(
                    "- `{}` ({} {}) now requires `{}` parameter\n",
                    op_id,
                    op.method.upper(),
                    op.path,
                    param_name
                ));
            }
        }
        out.push('\n');
    }

    // Schema property changes (removed or type-changed).
    let mut schema_changes = Vec::new();
    for name in old_schema_names.intersection(&new_schema_names) {
        let old_model = old_doc.schemas.get(name).unwrap();
        let new_model = new_doc.schemas.get(name).unwrap();
        if let (Model::Object(old_obj), Model::Object(new_obj)) = (old_model, new_model) {
            // Removed properties.
            for old_prop in &old_obj.properties {
                if !new_obj.properties.iter().any(|p| p.name == old_prop.name) {
                    schema_changes.push(format!(
                        "- `{}.{}` was removed",
                        name, old_prop.name
                    ));
                }
            }
            // Type changes.
            for old_prop in &old_obj.properties {
                if let Some(new_prop) = new_obj.properties.iter().find(|p| p.name == old_prop.name) {
                    let old_ty = format!("{:?}", old_prop.ty);
                    let new_ty = format!("{:?}", new_prop.ty);
                    if old_ty != new_ty {
                        schema_changes.push(format!(
                            "- `{}.{}` type changed",
                            name, old_prop.name
                        ));
                    }
                }
            }
            // New required properties.
            for new_prop in &new_obj.properties {
                if new_prop.required && !old_obj.properties.iter().any(|p| p.name == new_prop.name) {
                    schema_changes.push(format!(
                        "- `{}.{}` was added as required",
                        name, new_prop.name
                    ));
                }
            }
        }
    }
    if !schema_changes.is_empty() {
        out.push_str("## Schema Changes\n\n");
        for change in &schema_changes {
            out.push_str(&format!("{change}\n"));
        }
        out.push('\n');
    }

    // New operations.
    let added_ops: Vec<&str> = new_op_ids
        .difference(&old_op_ids)
        .copied()
        .collect();
    if !added_ops.is_empty() {
        out.push_str("## New Operations\n\n");
        for id in &added_ops {
            if let Some(op) = new_doc
                .operations
                .iter()
                .find(|o| o.operation_id == *id)
            {
                out.push_str(&format!(
                    "- `{}` ({} {})\n",
                    id,
                    op.method.upper(),
                    op.path
                ));
            }
        }
        out.push('\n');
    }

    // New schemas.
    let added_schemas: Vec<&str> = new_schema_names
        .difference(&old_schema_names)
        .copied()
        .collect();
    if !added_schemas.is_empty() {
        out.push_str("## New Schemas\n\n");
        for name in &added_schemas {
            out.push_str(&format!("- `{name}`\n"));
        }
        out.push('\n');
    }

    // If nothing was found, add a note.
    if out.lines().count() <= 2 {
        out.push_str("No breaking changes or deprecations detected.\n");
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::*;

    fn make_doc(
        ops: Vec<Operation>,
        schemas: Vec<(String, Model)>,
    ) -> Document {
        let mut schema_map = indexmap::IndexMap::new();
        for (k, v) in schemas {
            schema_map.insert(k, v);
        }
        Document {
            title: "Test".into(),
            version: "1.0.0".into(),
            base_url: None,
            security: vec![],
            schemas: SchemaRegistry { models: schema_map },
            operations: ops,
            webhooks: vec![],
        }
    }

    fn make_op(id: &str, summary: Option<&str>) -> Operation {
        Operation {
            operation_id: id.into(),
            method: HttpMethod::Get,
            path: "/test".into(),
            tag: None,
            summary: summary.map(|s| s.into()),
            description: None,
            parameters: vec![],
            request_body: None,
            responses: vec![],
        }
    }

    #[test]
    fn detects_deprecated_operation_in_summary() {
        let doc = make_doc(
            vec![make_op("oldEndpoint", Some("Deprecated: Use newEndpoint instead."))],
            vec![],
        );
        let deps = find_deprecations(&doc);
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].kind, DeprecationKind::Operation);
        assert_eq!(deps[0].name, "oldEndpoint");
        assert_eq!(deps[0].alternative.as_deref(), Some("newEndpoint"));
    }

    #[test]
    fn detects_deprecated_schema_in_description() {
        let model = Model::Object(ObjectModel {
            name: "OldPet".into(),
            description: Some("This schema is deprecated. Use PetV2 instead.".into()),
            properties: vec![],
            additional_properties: None,
            shape_type: None,
            base_type: None,
        });
        let doc = make_doc(vec![], vec![("OldPet".into(), model)]);
        let deps = find_deprecations(&doc);
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].kind, DeprecationKind::Schema);
        assert_eq!(deps[0].name, "OldPet");
        assert_eq!(deps[0].alternative.as_deref(), Some("PetV2"));
    }

    #[test]
    fn no_deprecations_in_clean_spec() {
        let doc = make_doc(
            vec![make_op("getPets", Some("List all pets"))],
            vec![],
        );
        let deps = find_deprecations(&doc);
        assert!(deps.is_empty());
    }

    #[test]
    fn extract_alternative_use_instead() {
        assert_eq!(
            extract_alternative("Deprecated. Use newApi instead."),
            Some("newApi".into())
        );
    }

    #[test]
    fn extract_alternative_replaced_by() {
        assert_eq!(
            extract_alternative("This is deprecated, replaced by V2Client."),
            Some("V2Client".into())
        );
    }

    #[test]
    fn extract_alternative_migrate_to() {
        assert_eq!(
            extract_alternative("Deprecated: migrate to /v2/endpoint."),
            Some("/v2/endpoint".into())
        );
    }

    #[test]
    fn generate_migration_guide_basic() {
        let old_doc = make_doc(
            vec![make_op("getPet", Some("Get a pet"))],
            vec![],
        );
        let new_doc = make_doc(
            vec![make_op("listPets", Some("List pets"))],
            vec![],
        );
        let guide = generate_migration_guide(&old_doc, &new_doc, "v1.0.0", "v2.0.0");
        assert!(guide.contains("v1.0.0 -> v2.0.0"));
        assert!(guide.contains("Removed Operations"));
        assert!(guide.contains("getPet"));
        assert!(guide.contains("New Operations"));
        assert!(guide.contains("listPets"));
    }
}
