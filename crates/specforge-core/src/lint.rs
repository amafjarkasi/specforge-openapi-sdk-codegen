//! Spec linting: static analysis of a resolved [`Document`] for common issues.
//!
//! Lints check for structural problems in the spec that are valid OpenAPI but
//! likely mistakes: duplicate operation IDs, missing descriptions, unused
//! schemas, etc. Results are reported as [`Diagnostic`]s with a severity level.
//!
//! Use [`lint_with_config`] to run lints with a [`LintConfig`] that controls
//! which rules are active and their severity. The convenience function [`lint`]
//! runs all rules with their defaults.

use std::collections::{HashMap, HashSet};

use crate::ir::{Document, Model, Operation, Type};
use crate::lint_config::LintConfig;

/// Diagnostic severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
pub enum Severity {
    Warning,
    Error,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Warning => write!(f, "warning"),
            Severity::Error => write!(f, "error"),
        }
    }
}

/// A single lint finding.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    /// Dot-separated path to the problematic element (e.g. `operations.listPets`).
    pub path: String,
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {} [{}]", self.severity, self.message, self.path)
    }
}

/// Run all lints on a resolved document using default configuration.
pub fn lint(doc: &Document) -> Vec<Diagnostic> {
    lint_with_config(doc, &LintConfig::default())
}

/// Run lints on a resolved document using the provided configuration.
///
/// Each rule is checked against the config to determine whether it is enabled
/// and what severity to assign to its diagnostics.
pub fn lint_with_config(doc: &Document, config: &LintConfig) -> Vec<Diagnostic> {
    let mut diags = Vec::new();

    if config.is_enabled("duplicate-operation-ids") {
        check_duplicate_operation_ids(doc, config, &mut diags);
    }
    if config.is_enabled("missing-response-description") {
        check_missing_response_descriptions(doc, config, &mut diags);
    }
    if config.is_enabled("missing-operation-summary") {
        check_missing_operation_summaries(doc, config, &mut diags);
    }
    if config.is_enabled("unused-schema") {
        check_unused_schemas(doc, config, &mut diags);
    }
    if config.is_enabled("missing-operation-id") {
        check_missing_operation_ids(doc, config, &mut diags);
    }
    if config.is_enabled("missing-schema-description") {
        check_missing_schema_descriptions(doc, config, &mut diags);
    }
    if config.is_enabled("path-trailing-slash") {
        check_path_trailing_slash(doc, config, &mut diags);
    }
    if config.is_enabled("deprecated-operation") {
        check_deprecated_operations(doc, config, &mut diags);
    }

    diags
}

// ─── Existing rules (updated to use config severity) ─────────────────────────

/// Error if two operations share the same `operation_id`.
fn check_duplicate_operation_ids(doc: &Document, config: &LintConfig, diags: &mut Vec<Diagnostic>) {
    let severity = config.severity("duplicate-operation-ids");
    let mut seen: HashMap<&str, &str> = HashMap::new(); // op_id -> first path
    for op in &doc.operations {
        if let Some(first_path) = seen.get(op.operation_id.as_str()) {
            diags.push(Diagnostic {
                severity,
                message: format!(
                    "duplicate operationId {:?} (first seen at {})",
                    op.operation_id, first_path
                ),
                path: format!("operations.{}", op.operation_id),
            });
        } else {
            seen.insert(&op.operation_id, &op.path);
        }
    }
}

/// Warn if a response has no description.
fn check_missing_response_descriptions(doc: &Document, config: &LintConfig, diags: &mut Vec<Diagnostic>) {
    let severity = config.severity("missing-response-description");
    for op in &doc.operations {
        for resp in &op.responses {
            if resp.description.is_none() {
                diags.push(Diagnostic {
                    severity,
                    message: format!("response {} has no description", resp.status),
                    path: format!("operations.{}.responses.{}", op.operation_id, resp.status),
                });
            }
        }
    }
}

/// Warn if an operation has no summary.
fn check_missing_operation_summaries(doc: &Document, config: &LintConfig, diags: &mut Vec<Diagnostic>) {
    let severity = config.severity("missing-operation-summary");
    for op in &doc.operations {
        if op.summary.is_none() {
            diags.push(Diagnostic {
                severity,
                message: "operation has no summary".to_string(),
                path: format!("operations.{}", op.operation_id),
            });
        }
    }
}

/// Warn about schemas that are defined but never referenced by any operation.
fn check_unused_schemas(doc: &Document, config: &LintConfig, diags: &mut Vec<Diagnostic>) {
    let severity = config.severity("unused-schema");
    let referenced = collect_referenced_schema_names(doc);

    for name in doc.schemas.models.keys() {
        if !referenced.contains(name.as_str()) {
            diags.push(Diagnostic {
                severity,
                message: format!("schema {:?} is not referenced by any operation", name),
                path: format!("schemas.{}", name),
            });
        }
    }
}

// ─── New rules ───────────────────────────────────────────────────────────────

/// Warn if an operation is missing an `operationId`.
fn check_missing_operation_ids(doc: &Document, config: &LintConfig, diags: &mut Vec<Diagnostic>) {
    let severity = config.severity("missing-operation-id");
    for op in &doc.operations {
        if op.operation_id.is_empty() {
            diags.push(Diagnostic {
                severity,
                message: format!("operation {} {} has no operationId", op.method.as_str(), op.path),
                path: format!("operations.{}.{}", op.method.as_str(), op.path),
            });
        }
    }
}

/// Warn if a schema has no description.
fn check_missing_schema_descriptions(doc: &Document, config: &LintConfig, diags: &mut Vec<Diagnostic>) {
    let severity = config.severity("missing-schema-description");
    for (name, model) in doc.schemas.iter() {
        let has_desc = match model {
            Model::Object(o) => o.description.is_some(),
            Model::Enum(e) => e.description.is_some(),
        };
        if !has_desc {
            diags.push(Diagnostic {
                severity,
                message: format!("schema {:?} has no description", name),
                path: format!("schemas.{}", name),
            });
        }
    }
}

/// Warn about inconsistent trailing slashes on paths.
fn check_path_trailing_slash(doc: &Document, config: &LintConfig, diags: &mut Vec<Diagnostic>) {
    let severity = config.severity("path-trailing-slash");
    for op in &doc.operations {
        if op.path.len() > 1 && op.path.ends_with('/') {
            diags.push(Diagnostic {
                severity,
                message: format!("path {:?} has a trailing slash", op.path),
                path: format!("operations.{}", op.operation_id),
            });
        }
    }
}

/// Warn about operations marked as deprecated.
fn check_deprecated_operations(_doc: &Document, _config: &LintConfig, _diags: &mut Vec<Diagnostic>) {
    // The current IR does not carry a `deprecated` flag on operations.
    // This is a placeholder for when the IR is extended. For now, we cannot
    // detect deprecated operations from the resolved IR alone.
    //
    // When the Operation struct gains a `deprecated: bool` field, the
    // implementation would be:
    //
    //   let severity = config.severity("deprecated-operation");
    //   for op in &doc.operations {
    //       if op.deprecated {
    //           diags.push(Diagnostic {
    //               severity,
    //               message: format!("operation {:?} is deprecated", op.operation_id),
    //               path: format!("operations.{}", op.operation_id),
    //           });
    //       }
    //   }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Walk all operations and collect every schema name reachable via types.
fn collect_referenced_schema_names(doc: &Document) -> HashSet<&str> {
    let mut names = HashSet::new();
    for op in &doc.operations {
        collect_op_refs(op, &mut names);
    }
    // Also mark schemas that are referenced by other schemas' properties
    // as "used" — they're part of the API surface even if no operation
    // directly uses them. We do a simple transitive walk.
    let mut changed = true;
    while changed {
        changed = false;
        for (sname, model) in doc.schemas.iter() {
            if names.contains(sname.as_str()) {
                // Already marked; check if it references others.
                if let Model::Object(obj) = model {
                    for prop in &obj.properties {
                        if let Type::Reference { name, .. } = &prop.ty {
                            if names.insert(name.as_str()) {
                                changed = true;
                            }
                        }
                    }
                    if let Some(shape) = &obj.shape_type {
                        collect_type_refs(shape, &mut names);
                    }
                }
            }
        }
    }
    names
}

fn collect_op_refs<'a>(op: &'a Operation, names: &mut HashSet<&'a str>) {
    if let Some(body) = &op.request_body {
        collect_type_refs(&body.ty, names);
    }
    for param in &op.parameters {
        collect_type_refs(&param.ty, names);
    }
    for resp in &op.responses {
        if let Some(body) = &resp.body {
            collect_type_refs(body, names);
        }
    }
}

fn collect_type_refs<'a>(ty: &'a Type, names: &mut HashSet<&'a str>) {
    match ty {
        Type::Reference { name, .. } => {
            names.insert(name.as_str());
        }
        Type::Array { item, .. } => collect_type_refs(item, names),
        Type::Map { value } => collect_type_refs(value, names),
        Type::Composition(comp) => {
            for member in &comp.members {
                collect_type_refs(member, names);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{parse_str, resolve};

    #[test]
    fn duplicate_operation_ids_are_errors() {
        let yaml = concat!(
            "openapi: \"3.0.0\"\n",
            "info:\n",
            "  title: Dup Test\n",
            "  version: \"1.0.0\"\n",
            "paths:\n",
            "  /a:\n",
            "    get:\n",
            "      operationId: listItems\n",
            "      summary: List A\n",
            "      responses:\n",
            "        '200':\n",
            "          description: OK\n",
            "  /b:\n",
            "    get:\n",
            "      operationId: listItems\n",
            "      summary: List B\n",
            "      responses:\n",
            "        '200':\n",
            "          description: OK\n",
        );
        let spec = parse_str(yaml).unwrap();
        let doc = resolve(&spec).unwrap();
        let diags = lint(&doc);
        let dupes: Vec<_> = diags
            .iter()
            .filter(|d| d.severity == Severity::Error && d.message.contains("duplicate"))
            .collect();
        assert_eq!(dupes.len(), 1, "expected one duplicate error");
    }

    #[test]
    fn missing_response_description_is_warning() {
        let yaml = concat!(
            "openapi: \"3.0.0\"\n",
            "info:\n",
            "  title: Desc Test\n",
            "  version: \"1.0.0\"\n",
            "paths:\n",
            "  /items:\n",
            "    get:\n",
            "      operationId: getItems\n",
            "      summary: Get items\n",
            "      responses:\n",
            "        '200':\n",
            "          description: OK\n",
            "        '500':\n",
            "          description: Server Error\n",
        );
        let spec = parse_str(yaml).unwrap();
        let doc = resolve(&spec).unwrap();
        let diags = lint(&doc);
        let missing: Vec<_> = diags
            .iter()
            .filter(|d| {
                d.severity == Severity::Warning && d.message.contains("no description")
            })
            .collect();
        assert!(
            !missing.is_empty(),
            "expected a warning about missing response description"
        );
    }

    #[test]
    fn missing_operation_summary_is_warning() {
        let yaml = concat!(
            "openapi: \"3.0.0\"\n",
            "info:\n",
            "  title: Summary Test\n",
            "  version: \"1.0.0\"\n",
            "paths:\n",
            "  /items:\n",
            "    get:\n",
            "      operationId: getItems\n",
            "      responses:\n",
            "        '200':\n",
            "          description: OK\n",
        );
        let spec = parse_str(yaml).unwrap();
        let doc = resolve(&spec).unwrap();
        let diags = lint(&doc);
        let missing: Vec<_> = diags
            .iter()
            .filter(|d| {
                d.severity == Severity::Warning && d.message.contains("no summary")
            })
            .collect();
        assert_eq!(missing.len(), 1, "expected one missing-summary warning");
    }

    #[test]
    fn unused_schema_is_warning() {
        let yaml = concat!(
            "openapi: \"3.0.0\"\n",
            "info:\n",
            "  title: Unused Test\n",
            "  version: \"1.0.0\"\n",
            "paths:\n",
            "  /items:\n",
            "    get:\n",
            "      operationId: getItems\n",
            "      summary: Get items\n",
            "      responses:\n",
            "        '200':\n",
            "          description: OK\n",
            "          content:\n",
            "            application/json:\n",
            "              schema:\n",
            "                \x24ref: \"#/components/schemas/Item\"\n",
            "components:\n",
            "  schemas:\n",
            "    Item:\n",
            "      type: object\n",
            "      properties:\n",
            "        name:\n",
            "          type: string\n",
            "    Orphan:\n",
            "      type: object\n",
            "      properties:\n",
            "        value:\n",
            "          type: integer\n",
        );
        let spec = parse_str(yaml).unwrap();
        let doc = resolve(&spec).unwrap();
        let diags = lint(&doc);
        let orphan: Vec<_> = diags
            .iter()
            .filter(|d| d.message.contains("Orphan") && d.message.contains("not referenced"))
            .collect();
        assert_eq!(orphan.len(), 1, "expected Orphan to be flagged as unused");

        // Item should NOT be flagged — it's used by getItems.
        let item_unused: Vec<_> = diags
            .iter()
            .filter(|d| d.message.contains("\"Item\"") && d.message.contains("not referenced"))
            .collect();
        assert!(
            item_unused.is_empty(),
            "Item should not be flagged as unused"
        );
    }

    // ─── Tests for lint_with_config ──────────────────────────────────────────

    #[test]
    fn config_disable_rule_skips_check() {
        let yaml = concat!(
            "openapi: \"3.0.0\"\n",
            "info:\n",
            "  title: Config Test\n",
            "  version: \"1.0.0\"\n",
            "paths:\n",
            "  /a:\n",
            "    get:\n",
            "      operationId: listItems\n",
            "      summary: List A\n",
            "      responses:\n",
            "        '200':\n",
            "          description: OK\n",
            "  /b:\n",
            "    get:\n",
            "      operationId: listItems\n",
            "      summary: List B\n",
            "      responses:\n",
            "        '200':\n",
            "          description: OK\n",
        );
        let spec = parse_str(yaml).unwrap();
        let doc = resolve(&spec).unwrap();

        let mut config = LintConfig::default();
        config.set_enabled("duplicate-operation-ids", false);

        let diags = lint_with_config(&doc, &config);
        let dupes: Vec<_> = diags
            .iter()
            .filter(|d| d.message.contains("duplicate"))
            .collect();
        assert_eq!(dupes.len(), 0, "duplicate check should be skipped");
    }

    #[test]
    fn config_severity_override_changes_diagnostic() {
        let yaml = concat!(
            "openapi: \"3.0.0\"\n",
            "info:\n",
            "  title: Severity Test\n",
            "  version: \"1.0.0\"\n",
            "paths:\n",
            "  /a:\n",
            "    get:\n",
            "      operationId: listItems\n",
            "      summary: List A\n",
            "      responses:\n",
            "        '200':\n",
            "          description: OK\n",
            "  /b:\n",
            "    get:\n",
            "      operationId: listItems\n",
            "      summary: List B\n",
            "      responses:\n",
            "        '200':\n",
            "          description: OK\n",
        );
        let spec = parse_str(yaml).unwrap();
        let doc = resolve(&spec).unwrap();

        let mut config = LintConfig::default();
        config.set_severity(
            "duplicate-operation-ids",
            crate::lint_config::RuleSeverity::Warning,
        );

        let diags = lint_with_config(&doc, &config);
        let dupes: Vec<_> = diags
            .iter()
            .filter(|d| d.message.contains("duplicate"))
            .collect();
        assert_eq!(dupes.len(), 1);
        assert_eq!(dupes[0].severity, Severity::Warning);
    }

    #[test]
    fn config_from_yaml_file_content() {
        let yaml = concat!(
            "openapi: \"3.0.0\"\n",
            "info:\n",
            "  title: YAML Config Test\n",
            "  version: \"1.0.0\"\n",
            "paths:\n",
            "  /items:\n",
            "    get:\n",
            "      operationId: getItems\n",
            "      responses:\n",
            "        '200':\n",
            "          description: OK\n",
        );
        let spec = parse_str(yaml).unwrap();
        let doc = resolve(&spec).unwrap();

        let config_yaml = r#"
rules:
  - name: missing-operation-summary
    enabled: false
    severity: warning
"#;
        let config: LintConfig = serde_yaml::from_str(config_yaml).unwrap();
        let diags = lint_with_config(&doc, &config);

        let summary_warnings: Vec<_> = diags
            .iter()
            .filter(|d| d.message.contains("no summary"))
            .collect();
        assert_eq!(
            summary_warnings.len(),
            0,
            "missing-operation-summary should be disabled"
        );
    }

    #[test]
    fn path_trailing_slash_detected() {
        let yaml = concat!(
            "openapi: \"3.0.0\"\n",
            "info:\n",
            "  title: Trailing Slash Test\n",
            "  version: \"1.0.0\"\n",
            "paths:\n",
            "  /items/:\n",
            "    get:\n",
            "      operationId: getItems\n",
            "      summary: Get items\n",
            "      responses:\n",
            "        '200':\n",
            "          description: OK\n",
        );
        let spec = parse_str(yaml).unwrap();
        let doc = resolve(&spec).unwrap();
        let diags = lint(&doc);

        let trailing: Vec<_> = diags
            .iter()
            .filter(|d| d.message.contains("trailing slash"))
            .collect();
        assert_eq!(trailing.len(), 1, "expected trailing slash warning");
    }

    #[test]
    fn missing_schema_description_detected_when_enabled() {
        let yaml = concat!(
            "openapi: \"3.0.0\"\n",
            "info:\n",
            "  title: Schema Desc Test\n",
            "  version: \"1.0.0\"\n",
            "paths:\n",
            "  /items:\n",
            "    get:\n",
            "      operationId: getItems\n",
            "      summary: Get items\n",
            "      responses:\n",
            "        '200':\n",
            "          description: OK\n",
            "          content:\n",
            "            application/json:\n",
            "              schema:\n",
            "                \x24ref: \"#/components/schemas/Item\"\n",
            "components:\n",
            "  schemas:\n",
            "    Item:\n",
            "      type: object\n",
            "      properties:\n",
            "        name:\n",
            "          type: string\n",
        );
        let spec = parse_str(yaml).unwrap();
        let doc = resolve(&spec).unwrap();

        let mut config = LintConfig::default();
        config.set_enabled("missing-schema-description", true);

        let diags = lint_with_config(&doc, &config);
        let schema_desc: Vec<_> = diags
            .iter()
            .filter(|d| d.message.contains("has no description") && d.path.starts_with("schemas."))
            .collect();
        assert!(
            !schema_desc.is_empty(),
            "expected schema description warning when enabled"
        );
    }

    #[test]
    fn missing_schema_description_not_detected_by_default() {
        let yaml = concat!(
            "openapi: \"3.0.0\"\n",
            "info:\n",
            "  title: Schema Desc Default Test\n",
            "  version: \"1.0.0\"\n",
            "paths:\n",
            "  /items:\n",
            "    get:\n",
            "      operationId: getItems\n",
            "      summary: Get items\n",
            "      responses:\n",
            "        '200':\n",
            "          description: OK\n",
            "          content:\n",
            "            application/json:\n",
            "              schema:\n",
            "                \x24ref: \"#/components/schemas/Item\"\n",
            "components:\n",
            "  schemas:\n",
            "    Item:\n",
            "      type: object\n",
            "      properties:\n",
            "        name:\n",
            "          type: string\n",
        );
        let spec = parse_str(yaml).unwrap();
        let doc = resolve(&spec).unwrap();
        let diags = lint(&doc);

        let schema_desc: Vec<_> = diags
            .iter()
            .filter(|d| d.message.contains("has no description") && d.path.starts_with("schemas."))
            .collect();
        assert!(
            schema_desc.is_empty(),
            "missing-schema-description should be off by default"
        );
    }

    #[test]
    fn path_without_trailing_slash_is_clean() {
        let yaml = concat!(
            "openapi: \"3.0.0\"\n",
            "info:\n",
            "  title: No Trailing Slash\n",
            "  version: \"1.0.0\"\n",
            "paths:\n",
            "  /items:\n",
            "    get:\n",
            "      operationId: getItems\n",
            "      summary: Get items\n",
            "      responses:\n",
            "        '200':\n",
            "          description: OK\n",
        );
        let spec = parse_str(yaml).unwrap();
        let doc = resolve(&spec).unwrap();
        let diags = lint(&doc);

        let trailing: Vec<_> = diags
            .iter()
            .filter(|d| d.message.contains("trailing slash"))
            .collect();
        assert_eq!(trailing.len(), 0, "no trailing slash expected");
    }

    #[test]
    fn root_path_no_trailing_slash_warning() {
        // "/" alone should not trigger the trailing slash warning.
        let yaml = concat!(
            "openapi: \"3.0.0\"\n",
            "info:\n",
            "  title: Root Path\n",
            "  version: \"1.0.0\"\n",
            "paths:\n",
            "  /:\n",
            "    get:\n",
            "      operationId: getRoot\n",
            "      summary: Root\n",
            "      responses:\n",
            "        '200':\n",
            "          description: OK\n",
        );
        let spec = parse_str(yaml).unwrap();
        let doc = resolve(&spec).unwrap();
        let diags = lint(&doc);

        let trailing: Vec<_> = diags
            .iter()
            .filter(|d| d.message.contains("trailing slash"))
            .collect();
        assert_eq!(
            trailing.len(),
            0,
            "root path '/' should not trigger trailing slash warning"
        );
    }

    #[test]
    fn severity_off_disables_rule_entirely() {
        let yaml = concat!(
            "openapi: \"3.0.0\"\n",
            "info:\n",
            "  title: Off Test\n",
            "  version: \"1.0.0\"\n",
            "paths:\n",
            "  /items:\n",
            "    get:\n",
            "      operationId: getItems\n",
            "      responses:\n",
            "        '200':\n",
            "          description: OK\n",
        );
        let spec = parse_str(yaml).unwrap();
        let doc = resolve(&spec).unwrap();

        let mut config = LintConfig::default();
        config.set_severity(
            "missing-operation-summary",
            crate::lint_config::RuleSeverity::Off,
        );

        let diags = lint_with_config(&doc, &config);
        let summary: Vec<_> = diags
            .iter()
            .filter(|d| d.message.contains("no summary"))
            .collect();
        assert_eq!(
            summary.len(),
            0,
            "severity Off should suppress the diagnostic"
        );
    }
}
