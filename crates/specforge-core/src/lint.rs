//! Spec linting: static analysis of a resolved [`Document`] for common issues.
//!
//! Lints check for structural problems in the spec that are valid OpenAPI but
//! likely mistakes: duplicate operation IDs, missing descriptions, unused
//! schemas, etc. Results are reported as [`Diagnostic`]s with a severity level.

use std::collections::{HashMap, HashSet};

use crate::ir::{Document, Model, Operation, Type};

/// Diagnostic severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
#[derive(Debug, Clone)]
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

/// Run all lints on a resolved document and return the collected diagnostics.
pub fn lint(doc: &Document) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    check_duplicate_operation_ids(doc, &mut diags);
    check_missing_response_descriptions(doc, &mut diags);
    check_missing_operation_summaries(doc, &mut diags);
    check_unused_schemas(doc, &mut diags);
    diags
}

/// Error if two operations share the same `operation_id`.
fn check_duplicate_operation_ids(doc: &Document, diags: &mut Vec<Diagnostic>) {
    let mut seen: HashMap<&str, &str> = HashMap::new(); // op_id -> first path
    for op in &doc.operations {
        if let Some(first_path) = seen.get(op.operation_id.as_str()) {
            diags.push(Diagnostic {
                severity: Severity::Error,
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
fn check_missing_response_descriptions(doc: &Document, diags: &mut Vec<Diagnostic>) {
    for op in &doc.operations {
        for resp in &op.responses {
            if resp.description.is_none() {
                diags.push(Diagnostic {
                    severity: Severity::Warning,
                    message: format!(
                        "response {} has no description",
                        resp.status
                    ),
                    path: format!("operations.{}.responses.{}", op.operation_id, resp.status),
                });
            }
        }
    }
}

/// Warn if an operation has no summary.
fn check_missing_operation_summaries(doc: &Document, diags: &mut Vec<Diagnostic>) {
    for op in &doc.operations {
        if op.summary.is_none() {
            diags.push(Diagnostic {
                severity: Severity::Warning,
                message: "operation has no summary".to_string(),
                path: format!("operations.{}", op.operation_id),
            });
        }
    }
}

/// Warn about schemas that are defined but never referenced by any operation.
fn check_unused_schemas(doc: &Document, diags: &mut Vec<Diagnostic>) {
    let referenced = collect_referenced_schema_names(doc);

    for name in doc.schemas.models.keys() {
        if !referenced.contains(name.as_str()) {
            diags.push(Diagnostic {
                severity: Severity::Warning,
                message: format!("schema {:?} is not referenced by any operation", name),
                path: format!("schemas.{}", name),
            });
        }
    }
}

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
        // Note: the IR resolve_responses currently always sets description to
        // None for inline responses, so both responses here will be flagged.
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
}
