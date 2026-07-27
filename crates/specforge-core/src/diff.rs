//! Spec diff: compare two OpenAPI documents and report breaking changes.

use crate::ir::{Document, Model, Operation, Type};

/// Severity of a diff finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffSeverity {
    /// A breaking change that will cause existing clients to fail.
    Breaking,
    /// A non-breaking addition or improvement.
    Info,
}

impl std::fmt::Display for DiffSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiffSeverity::Breaking => write!(f, "breaking"),
            DiffSeverity::Info => write!(f, "info"),
        }
    }
}

/// A single diff finding.
#[derive(Debug, Clone)]
pub struct DiffFinding {
    pub severity: DiffSeverity,
    pub message: String,
    pub path: String,
}

impl std::fmt::Display for DiffFinding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {} [{}]", self.severity, self.message, self.path)
    }
}

/// Compare two documents and return findings.
pub fn diff(old: &Document, new: &Document) -> Vec<DiffFinding> {
    let mut findings = Vec::new();

    diff_operations(old, new, &mut findings);
    diff_schemas(old, new, &mut findings);

    findings
}

fn diff_operations(old: &Document, new: &Document, findings: &mut Vec<DiffFinding>) {
    let old_ops: std::collections::HashMap<String, &Operation> =
        old.operations.iter().map(|op| (op.operation_id.clone(), op)).collect();
    let new_ops: std::collections::HashMap<String, &Operation> =
        new.operations.iter().map(|op| (op.operation_id.clone(), op)).collect();

    // Removed operations are breaking.
    for id in old_ops.keys() {
        if !new_ops.contains_key(id) {
            findings.push(DiffFinding {
                severity: DiffSeverity::Breaking,
                message: format!("operation removed: {id}"),
                path: format!("operations.{id}"),
            });
        }
    }

    // Added operations are info.
    for id in new_ops.keys() {
        if !old_ops.contains_key(id) {
            findings.push(DiffFinding {
                severity: DiffSeverity::Info,
                message: format!("operation added: {id}"),
                path: format!("operations.{id}"),
            });
        }
    }

    // Changed operations.
    for id in old_ops.keys() {
        if let (Some(old_op), Some(new_op)) = (old_ops.get(id), new_ops.get(id)) {
            diff_operation(old_op, new_op, findings);
        }
    }
}

fn diff_operation(old: &Operation, new: &Operation, findings: &mut Vec<DiffFinding>) {
    let path = format!("operations.{}", old.operation_id);

    // New required parameters are breaking.
    for new_param in &new.parameters {
        if new_param.required {
            let existed = old.parameters.iter().any(|p| p.name == new_param.name);
            if !existed {
                findings.push(DiffFinding {
                    severity: DiffSeverity::Breaking,
                    message: format!("new required parameter: {}", new_param.name),
                    path: format!("{path}.params.{}", new_param.name),
                });
            }
        }
    }

    // Removed parameters are breaking.
    for old_param in &old.parameters {
        let exists = new.parameters.iter().any(|p| p.name == old_param.name);
        if !exists {
            findings.push(DiffFinding {
                severity: DiffSeverity::Breaking,
                message: format!("parameter removed: {}", old_param.name),
                path: format!("{path}.params.{}", old_param.name),
            });
        }
    }

    // New required request body is breaking.
    if new.request_body.is_some() && old.request_body.is_none() {
        if new.request_body.as_ref().map(|rb| rb.required).unwrap_or(false) {
            findings.push(DiffFinding {
                severity: DiffSeverity::Breaking,
                message: "new required request body".to_string(),
                path: format!("{path}.requestBody"),
            });
        }
    }

    // Removed responses are breaking.
    for old_resp in &old.responses {
        let exists = new.responses.iter().any(|r| r.status == old_resp.status);
        if !exists {
            findings.push(DiffFinding {
                severity: DiffSeverity::Breaking,
                message: format!("response removed: {}", old_resp.status),
                path: format!("{path}.responses.{}", old_resp.status),
            });
        }
    }
}

fn diff_schemas(old: &Document, new: &Document, findings: &mut Vec<DiffFinding>) {
    let old_schemas: std::collections::HashSet<String> =
        old.schemas.iter().map(|(k, _)| k.clone()).collect();
    let new_schemas: std::collections::HashSet<String> =
        new.schemas.iter().map(|(k, _)| k.clone()).collect();

    // Removed schemas are breaking (other schemas or operations may reference them).
    for name in &old_schemas {
        if !new_schemas.contains(name) {
            findings.push(DiffFinding {
                severity: DiffSeverity::Breaking,
                message: format!("schema removed: {name}"),
                path: format!("schemas.{name}"),
            });
        }
    }

    // Added schemas are info.
    for name in &new_schemas {
        if !old_schemas.contains(name) {
            findings.push(DiffFinding {
                severity: DiffSeverity::Info,
                message: format!("schema added: {name}"),
                path: format!("schemas.{name}"),
            });
        }
    }

    // Changed schemas — check for new required properties (breaking).
    for name in old_schemas.intersection(&new_schemas) {
        let old_model = old.schemas.get(name).unwrap();
        let new_model = new.schemas.get(name).unwrap();
        diff_schema(name, old_model, new_model, findings);
    }
}

fn diff_schema(
    name: &str,
    old: &Model,
    new: &Model,
    findings: &mut Vec<DiffFinding>,
) {
    let (old_props, new_props) = match (old, new) {
        (Model::Object(o), Model::Object(n)) => (&o.properties, &n.properties),
        _ => return,
    };

    let path = format!("schemas.{name}");

    // New required properties are breaking.
    for new_prop in new_props {
        if new_prop.required {
            let existed = old_props.iter().any(|p| p.name == new_prop.name);
            if !existed {
                findings.push(DiffFinding {
                    severity: DiffSeverity::Breaking,
                    message: format!("new required property: {}", new_prop.name),
                    path: format!("{path}.{}", new_prop.name),
                });
            }
        }
    }

    // Removed properties are breaking.
    for old_prop in old_props {
        let exists = new_props.iter().any(|p| p.name == old_prop.name);
        if !exists {
            findings.push(DiffFinding {
                severity: DiffSeverity::Breaking,
                message: format!("property removed: {}", old_prop.name),
                path: format!("{path}.{}", old_prop.name),
            });
        }
    }

    // Type changes on existing properties are breaking.
    for old_prop in old_props {
        if let Some(new_prop) = new_props.iter().find(|p| p.name == old_prop.name) {
            if type_changed(&old_prop.ty, &new_prop.ty) {
                findings.push(DiffFinding {
                    severity: DiffSeverity::Breaking,
                    message: format!("type changed for property: {}", old_prop.name),
                    path: format!("{path}.{}", old_prop.name),
                });
            }
        }
    }
}

/// Rough type compatibility check. Returns true if the type changed in a
/// potentially breaking way.
fn type_changed(old: &Type, new: &Type) -> bool {
    match (old, new) {
        (Type::Scalar(a), Type::Scalar(b)) => a != b,
        (Type::Reference { name: a, .. }, Type::Reference { name: b, .. }) => a != b,
        (Type::StringEnum { variants: a, .. }, Type::StringEnum { variants: b, .. }) => a != b,
        (Type::Array { item: a, .. }, Type::Array { item: b, .. }) => type_changed(a, b),
        _ => std::mem::discriminant(old) != std::mem::discriminant(new),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::*;

    fn make_doc(ops: Vec<Operation>, schemas: Vec<(String, Model)>) -> Document {
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

    fn make_op(id: &str) -> Operation {
        Operation {
            operation_id: id.into(),
            method: crate::ir::HttpMethod::Get,
            path: "/test".into(),
            tag: None,
            summary: None,
            description: None,
            parameters: vec![],
            request_body: None,
            responses: vec![],
        }
    }

    #[test]
    fn removed_operation_is_breaking() {
        let old = make_doc(vec![make_op("getPet")], vec![]);
        let new = make_doc(vec![], vec![]);
        let findings = diff(&old, &new);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, DiffSeverity::Breaking);
        assert!(findings[0].message.contains("getPet"));
    }

    #[test]
    fn added_operation_is_info() {
        let old = make_doc(vec![], vec![]);
        let new = make_doc(vec![make_op("createPet")], vec![]);
        let findings = diff(&old, &new);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, DiffSeverity::Info);
    }

    #[test]
    fn removed_schema_is_breaking() {
        let old = make_doc(vec![], vec![("Pet".into(), Model::Object(ObjectModel {
            name: "Pet".into(),
            description: None,
            properties: vec![],
            additional_properties: None,
            shape_type: None,
            base_type: None,
        }))]);
        let new = make_doc(vec![], vec![]);
        let findings = diff(&old, &new);
        assert!(findings.iter().any(|f| f.severity == DiffSeverity::Breaking
            && f.message.contains("Pet")));
    }

    #[test]
    fn new_required_property_is_breaking() {
        let old_model = Model::Object(ObjectModel {
            name: "Pet".into(),
            description: None,
            properties: vec![],
            additional_properties: None,
            shape_type: None,
            base_type: None,
        });
        let new_model = Model::Object(ObjectModel {
            name: "Pet".into(),
            description: None,
            properties: vec![Property {
                name: "name".into(),
                ty: Type::Scalar(Scalar::String),
                required: true,
                description: None,
            }],
            additional_properties: None,
            shape_type: None,
            base_type: None,
        });
        let old = make_doc(vec![], vec![("Pet".into(), old_model)]);
        let new = make_doc(vec![], vec![("Pet".into(), new_model)]);
        let findings = diff(&old, &new);
        assert!(findings.iter().any(|f| f.severity == DiffSeverity::Breaking
            && f.message.contains("name")));
    }

    #[test]
    fn no_changes_no_findings() {
        let doc = make_doc(vec![make_op("getPet")], vec![]);
        let findings = diff(&doc, &doc);
        assert!(findings.is_empty());
    }
}
