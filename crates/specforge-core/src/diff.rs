//! Spec diff: compare two OpenAPI documents and report breaking changes.

use crate::ir::{Document, Model, Operation, Type};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum DiffSeverity { Breaking, Info }
impl std::fmt::Display for DiffSeverity { fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { match self { DiffSeverity::Breaking => write!(f, "breaking"), DiffSeverity::Info => write!(f, "info") } } }

#[derive(Debug, Clone, Serialize)]
pub struct DiffFinding { pub severity: DiffSeverity, pub message: String, pub path: String }
impl std::fmt::Display for DiffFinding { fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}: {} [{}]", self.severity, self.message, self.path) } }

#[derive(Debug, Clone, Serialize)]
pub struct PropertyChange { pub property: String, pub change: PropertyChangeKind }
impl std::fmt::Display for PropertyChange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.change {
            PropertyChangeKind::Added { ty } => {
                write!(f, "+ {}: {} (new property)", self.property, ty)
            }
            PropertyChangeKind::AddedRequired { ty } => {
                write!(f, "+ {}: {} (new property, required)", self.property, ty)
            }
            PropertyChangeKind::Removed { ty } => {
                write!(f, "- {}: {} (removed)", self.property, ty)
            }
            PropertyChangeKind::TypeChanged { old_type, new_type } => {
                write!(f, "~ {}: {} → {}", self.property, old_type, new_type)
            }
            PropertyChangeKind::RequiredChanged { old_required, new_required } => {
                let o = if *old_required { "required" } else { "optional" };
                let n = if *new_required { "required" } else { "optional" };
                write!(f, "~ {}: {} → {} (required changed)", self.property, o, n)
            }
        }
    }
}
#[derive(Debug, Clone, Serialize)]
pub enum PropertyChangeKind { Added { ty: String }, AddedRequired { ty: String }, Removed { ty: String }, TypeChanged { old_type: String, new_type: String }, RequiredChanged { old_required: bool, new_required: bool } }
#[derive(Debug, Clone, Serialize)]
pub struct SchemaDiffDetail { pub name: String, pub changes: Vec<PropertyChange> }
impl std::fmt::Display for SchemaDiffDetail { fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { writeln!(f, "Schema {}:", self.name)?; for c in &self.changes { writeln!(f, "  {c}")?; } Ok(()) } }
#[derive(Debug, Clone, Serialize)]
pub struct DiffResult { pub findings: Vec<DiffFinding>, pub schema_diffs: Vec<SchemaDiffDetail> }
#[derive(Debug, Clone, Serialize)]
pub struct DiffJsonOutput { pub breaking: Vec<DiffFinding>, pub info: Vec<DiffFinding>, pub schema_diffs: Vec<SchemaDiffDetail>, pub summary: DiffSummary }
#[derive(Debug, Clone, Serialize)]
pub struct DiffSummary { pub breaking_count: usize, pub info_count: usize }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffFormat { Text, Markdown, Json }

fn type_to_string(ty: &Type) -> String {
    match ty {
        Type::Scalar(s) => match s {
            crate::ir::Scalar::String => "string".into(),
            crate::ir::Scalar::DateTime => "date-time".into(),
            crate::ir::Scalar::Uuid => "uuid".into(),
            crate::ir::Scalar::Integer => "integer".into(),
            crate::ir::Scalar::Integer64 => "integer (int64)".into(),
            crate::ir::Scalar::Float => "number".into(),
            crate::ir::Scalar::Boolean => "boolean".into(),
            _ => "other".into(),
        },
        Type::Reference { name, .. } => name.clone(),
        Type::StringEnum { .. } => "string (enum)".into(),
        Type::Array { .. } => "array".into(),
        Type::Map { .. } => "object".into(),
        Type::Composition(_) => "composition".into(),
        Type::Any => "any".into(),
        Type::Unknown => "unknown".into(),
    }
}
pub fn diff(old: &Document, new: &Document) -> Vec<DiffFinding> { diff_detailed(old, new).findings }
pub fn diff_detailed(old: &Document, new: &Document) -> DiffResult { let mut findings = Vec::new(); let mut schema_diffs = Vec::new(); diff_ir_version(old, new, &mut findings); diff_operations(old, new, &mut findings); diff_schemas(old, new, &mut findings, &mut schema_diffs); DiffResult { findings, schema_diffs } }

/// Check for IR version mismatches between old and new documents.
/// A major version difference indicates breaking IR schema changes.
fn diff_ir_version(old: &Document, new: &Document, findings: &mut Vec<DiffFinding>) {
    if old.ir_version != new.ir_version {
        let old_major = ir_major_version(&old.ir_version);
        let new_major = ir_major_version(&new.ir_version);
        if old_major != new_major {
            findings.push(DiffFinding {
                severity: DiffSeverity::Breaking,
                message: format!(
                    "IR version changed: {} → {} (major version bump indicates breaking IR schema changes)",
                    old.ir_version, new.ir_version
                ),
                path: "ir_version".into(),
            });
        } else {
            findings.push(DiffFinding {
                severity: DiffSeverity::Info,
                message: format!(
                    "IR version changed: {} → {}",
                    old.ir_version, new.ir_version
                ),
                path: "ir_version".into(),
            });
        }
    }
}

/// Parse the major version from a semver-like string (e.g. "1.0" → 1, "2.1" → 2).
fn ir_major_version(version: &str) -> u64 {
    version
        .split('.')
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
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
    // Added operations are informational.
    for id in new_ops.keys() {
        if !old_ops.contains_key(id) {
            findings.push(DiffFinding {
                severity: DiffSeverity::Info,
                message: format!("operation added: {id}"),
                path: format!("operations.{id}"),
            });
        }
    }
    // Operations present in both may have changed.
    for id in old_ops.keys() {
        if let (Some(o), Some(n)) = (old_ops.get(id), new_ops.get(id)) {
            diff_operation(o, n, findings);
        }
    }
}

fn diff_operation(old: &Operation, new: &Operation, findings: &mut Vec<DiffFinding>) {
    let path = format!("operations.{}", old.operation_id);

    // New required parameters are breaking.
    for p in &new.parameters {
        if p.required && !old.parameters.iter().any(|x| x.name == p.name) {
            findings.push(DiffFinding {
                severity: DiffSeverity::Breaking,
                message: format!("new required parameter: {}", p.name),
                path: format!("{path}.params.{}", p.name),
            });
        }
    }
    // Removed parameters are breaking.
    for p in &old.parameters {
        if !new.parameters.iter().any(|x| x.name == p.name) {
            findings.push(DiffFinding {
                severity: DiffSeverity::Breaking,
                message: format!("parameter removed: {}", p.name),
                path: format!("{path}.params.{}", p.name),
            });
        }
    }
    // A newly required request body is breaking.
    if new.request_body.is_some()
        && old.request_body.is_none()
        && new.request_body.as_ref().map(|rb| rb.required).unwrap_or(false)
    {
        findings.push(DiffFinding {
            severity: DiffSeverity::Breaking,
            message: "new required request body".into(),
            path: format!("{path}.requestBody"),
        });
    }
    // Removed response codes are breaking.
    for r in &old.responses {
        if !new.responses.iter().any(|x| x.status == r.status) {
            findings.push(DiffFinding {
                severity: DiffSeverity::Breaking,
                message: format!("response removed: {}", r.status),
                path: format!("{path}.responses.{}", r.status),
            });
        }
    }
}
fn diff_schemas(
    old: &Document,
    new: &Document,
    findings: &mut Vec<DiffFinding>,
    schema_diffs: &mut Vec<SchemaDiffDetail>,
) {
    let old_s: std::collections::HashSet<String> =
        old.schemas.iter().map(|(k, _)| k.clone()).collect();
    let new_s: std::collections::HashSet<String> =
        new.schemas.iter().map(|(k, _)| k.clone()).collect();

    // Removed schemas are breaking.
    for n in &old_s {
        if !new_s.contains(n) {
            findings.push(DiffFinding {
                severity: DiffSeverity::Breaking,
                message: format!("schema removed: {n}"),
                path: format!("schemas.{n}"),
            });
        }
    }
    // Added schemas are informational.
    for n in &new_s {
        if !old_s.contains(n) {
            findings.push(DiffFinding {
                severity: DiffSeverity::Info,
                message: format!("schema added: {n}"),
                path: format!("schemas.{n}"),
            });
        }
    }
    // Schemas present in both may have property-level changes.
    for n in old_s.intersection(&new_s) {
        if let Some(d) = diff_schema(
            n,
            old.schemas.get(n).unwrap(),
            new.schemas.get(n).unwrap(),
            findings,
        ) {
            schema_diffs.push(d);
        }
    }
}

fn diff_schema(
    name: &str,
    old: &Model,
    new: &Model,
    findings: &mut Vec<DiffFinding>,
) -> Option<SchemaDiffDetail> {
    let (op, np) = match (old, new) {
        (Model::Object(o), Model::Object(n)) => (&o.properties, &n.properties),
        _ => return None,
    };
    let path = format!("schemas.{name}");
    let mut changes = Vec::new();

    // New required properties are breaking.
    for p in np {
        if p.required && !op.iter().any(|x| x.name == p.name) {
            findings.push(DiffFinding {
                severity: DiffSeverity::Breaking,
                message: format!("new required property: {}", p.name),
                path: format!("{path}.{}", p.name),
            });
            changes.push(PropertyChange {
                property: p.name.clone(),
                change: PropertyChangeKind::AddedRequired { ty: type_to_string(&p.ty) },
            });
        }
    }
    // New optional properties are informational.
    for p in np {
        if !p.required && !op.iter().any(|x| x.name == p.name) {
            findings.push(DiffFinding {
                severity: DiffSeverity::Info,
                message: format!("new optional property: {}", p.name),
                path: format!("{path}.{}", p.name),
            });
            changes.push(PropertyChange {
                property: p.name.clone(),
                change: PropertyChangeKind::Added { ty: type_to_string(&p.ty) },
            });
        }
    }
    // Removed properties are breaking.
    for p in op {
        if !np.iter().any(|x| x.name == p.name) {
            findings.push(DiffFinding {
                severity: DiffSeverity::Breaking,
                message: format!("property removed: {}", p.name),
                path: format!("{path}.{}", p.name),
            });
            changes.push(PropertyChange {
                property: p.name.clone(),
                change: PropertyChangeKind::Removed { ty: type_to_string(&p.ty) },
            });
        }
    }
    // Type or required-status changes for shared properties.
    for p in op {
        if let Some(np) = np.iter().find(|x| x.name == p.name) {
            if type_changed(&p.ty, &np.ty) {
                findings.push(DiffFinding {
                    severity: DiffSeverity::Breaking,
                    message: format!("type changed for property: {}", p.name),
                    path: format!("{path}.{}", p.name),
                });
                changes.push(PropertyChange {
                    property: p.name.clone(),
                    change: PropertyChangeKind::TypeChanged {
                        old_type: type_to_string(&p.ty),
                        new_type: type_to_string(&np.ty),
                    },
                });
            } else if p.required != np.required {
                findings.push(DiffFinding {
                    severity: if np.required { DiffSeverity::Breaking } else { DiffSeverity::Info },
                    message: format!(
                        "required changed for property: {} ({} → {})",
                        p.name,
                        if p.required { "required" } else { "optional" },
                        if np.required { "required" } else { "optional" }
                    ),
                    path: format!("{path}.{}", p.name),
                });
                changes.push(PropertyChange {
                    property: p.name.clone(),
                    change: PropertyChangeKind::RequiredChanged {
                        old_required: p.required,
                        new_required: np.required,
                    },
                });
            }
        }
    }

    if changes.is_empty() { None } else { Some(SchemaDiffDetail { name: name.to_string(), changes }) }
}
fn type_changed(old: &Type, new: &Type) -> bool {
    match (old, new) {
        (Type::Scalar(a), Type::Scalar(b)) => a != b,
        (Type::Reference { name: a, .. }, Type::Reference { name: b, .. }) => a != b,
        (Type::StringEnum { variants: a, .. }, Type::StringEnum { variants: b, .. }) => a != b,
        (Type::Array { item: a, .. }, Type::Array { item: b, .. }) => type_changed(a, b),
        _ => std::mem::discriminant(old) != std::mem::discriminant(new),
    }
}

pub fn format_text(findings: &[DiffFinding]) -> String {
    let mut out = String::new();
    for finding in findings {
        match finding.severity {
            DiffSeverity::Breaking => out.push_str(&format!("breaking: {finding}\n")),
            DiffSeverity::Info => out.push_str(&format!("info: {finding}\n")),
        }
    }
    let bc = findings.iter().filter(|f| f.severity == DiffSeverity::Breaking).count();
    let ic = findings.len() - bc;
    if !findings.is_empty() {
        out.push_str(&format!("\n{bc} breaking change(s), {ic} info finding(s)\n"));
    }
    out
}

pub fn format_colored(findings: &[DiffFinding], schema_diffs: &[SchemaDiffDetail]) -> String {
    const RED: &str = "\x1b[31m";
    const GREEN: &str = "\x1b[32m";
    const YELLOW: &str = "\x1b[33m";
    const BOLD: &str = "\x1b[1m";
    const DIM: &str = "\x1b[2m";
    const RESET: &str = "\x1b[0m";

    let mut out = String::new();
    for f in findings {
        match f.severity {
            DiffSeverity::Breaking => {
                out.push_str(&format!("{RED}{BOLD}breaking{RESET} {RED}{}\n{RESET}", f.message))
            }
            DiffSeverity::Info => {
                out.push_str(&format!("{GREEN}{BOLD}info{RESET} {GREEN}{}\n{RESET}", f.message))
            }
        }
    }
    if !schema_diffs.is_empty() {
        out.push('\n');
        for d in schema_diffs {
            out.push_str(&format!("{BOLD}Schema {}:{RESET}\n", d.name));
            for c in &d.changes {
                let (col, pre) = match &c.change {
                    PropertyChangeKind::Added { .. } | PropertyChangeKind::AddedRequired { .. } => (GREEN, "+"),
                    PropertyChangeKind::Removed { .. } => (RED, "-"),
                    _ => (YELLOW, "~"),
                };
                out.push_str(&format!("  {col}{pre} {c}{RESET}\n"));
            }
        }
    }
    let bc = findings.iter().filter(|f| f.severity == DiffSeverity::Breaking).count();
    let ic = findings.len() - bc;
    if !findings.is_empty() {
        out.push_str(&format!("\n{DIM}{bc} breaking change(s), {ic} info finding(s){RESET}\n"));
    }
    out
}

pub fn format_markdown(
    findings: &[DiffFinding],
    schema_diffs: &[SchemaDiffDetail],
    old_version: &str,
    new_version: &str,
) -> String {
    let mut out = String::new();
    out.push_str(&format!("# API Changes: {old_version} → {new_version}\n\n"));

    let brk: Vec<_> = findings.iter().filter(|f| f.severity == DiffSeverity::Breaking).collect();
    let inf: Vec<_> = findings.iter().filter(|f| f.severity == DiffSeverity::Info).collect();

    // Breaking changes section.
    out.push_str("## Breaking Changes\n\n");
    if brk.is_empty() {
        out.push_str("_None._\n\n");
    } else {
        for f in &brk {
            out.push_str(&format!("- ❌ {}\n", fmt_md(f)));
        }
        out.push('\n');
    }

    // Non-breaking changes section.
    out.push_str("## Non-Breaking Changes\n\n");
    if inf.is_empty() {
        out.push_str("_None._\n\n");
    } else {
        for f in &inf {
            out.push_str(&format!("- ✅ {}\n", fmt_md(f)));
        }
        out.push('\n');
    }

    // Schema changes section (table per schema).
    if !schema_diffs.is_empty() {
        out.push_str("## Schema Changes\n\n");
        for d in schema_diffs {
            out.push_str(&format!(
                "### `{}`\n\n| Change | Property | Details |\n|--------|----------|--------|\n",
                d.name
            ));
            for c in &d.changes {
                let (icon, det) = match &c.change {
                    PropertyChangeKind::Added { ty } => ("Added", format!("new property (`{ty}`)")),
                    PropertyChangeKind::AddedRequired { ty } => {
                        ("Added", format!("new required property (`{ty}`)"))
                    }
                    PropertyChangeKind::Removed { ty } => ("Removed", format!("was `{ty}`")),
                    PropertyChangeKind::TypeChanged { old_type, new_type } => {
                        ("Changed", format!("`{old_type}` → `{new_type}`"))
                    }
                    PropertyChangeKind::RequiredChanged { old_required, new_required } => {
                        let o = if *old_required { "required" } else { "optional" };
                        let n = if *new_required { "required" } else { "optional" };
                        ("Changed", format!("{o} → {n}"))
                    }
                };
                out.push_str(&format!("| {icon} | `{}` | {det} |\n", c.property));
            }
            out.push('\n');
        }
    }

    // Summary section.
    out.push_str("## Summary\n\n");
    out.push_str(&format!(
        "- **{}** breaking change(s)\n- **{}** non-breaking change(s)\n",
        brk.len(),
        inf.len()
    ));
    if !schema_diffs.is_empty() {
        out.push_str(&format!("- **{}** schema(s) modified\n", schema_diffs.len()));
    }
    out
}

fn fmt_md(f: &DiffFinding) -> String { fmt_md_msg(&f.message) }
fn fmt_md_msg(msg: &str) -> String {
    // Recognized diff messages are rewritten into human-readable Markdown.
    // Each prefix maps to a templated sentence; anything else is returned
    // with its first character uppercased.
    if let Some(rest) = msg.strip_prefix("IR version changed: ") {
        return format!("IR version changed: {rest}");
    }
    if let Some(id) = msg.strip_prefix("operation removed: ") {
        return format!("Operation `{id}` removed");
    }
    if let Some(id) = msg.strip_prefix("operation added: ") {
        return format!("Operation `{id}` added");
    }
    if let Some(n) = msg.strip_prefix("schema removed: ") {
        return format!("Schema `{n}` removed");
    }
    if let Some(n) = msg.strip_prefix("schema added: ") {
        return format!("Schema `{n}` added");
    }
    if let Some(p) = msg.strip_prefix("new required property: ") {
        return format!("New required property `{p}`");
    }
    if let Some(p) = msg.strip_prefix("new optional property: ") {
        return format!("New optional property `{p}`");
    }
    if let Some(p) = msg.strip_prefix("property removed: ") {
        return format!("Property `{p}` removed");
    }
    if let Some(p) = msg.strip_prefix("type changed for property: ") {
        return format!("Type changed for property `{p}`");
    }
    if let Some(p) = msg.strip_prefix("new required parameter: ") {
        return format!("New required parameter `{p}`");
    }
    if let Some(p) = msg.strip_prefix("parameter removed: ") {
        return format!("Parameter `{p}` removed");
    }
    if let Some(p) = msg.strip_prefix("response removed: ") {
        return format!("Response `{p}` removed");
    }
    if let Some(p) = msg.strip_prefix("required changed for property: ") {
        return format!("Required status changed for property `{p}`");
    }
    if msg == "new required request body" {
        return "New required request body".into();
    }
    let mut c = msg.chars();
    match c.next() {
        None => String::new(),
        Some(ch) => {
            let mut s = ch.to_uppercase().to_string();
            s.extend(c);
            s
        }
    }
}

pub fn format_json(
    findings: &[DiffFinding],
    schema_diffs: &[SchemaDiffDetail],
) -> Result<String, serde_json::Error> {
    let breaking: Vec<_> = findings
        .iter()
        .filter(|f| f.severity == DiffSeverity::Breaking)
        .cloned()
        .collect();
    let info: Vec<_> = findings
        .iter()
        .filter(|f| f.severity == DiffSeverity::Info)
        .cloned()
        .collect();
    serde_json::to_string_pretty(&DiffJsonOutput {
        breaking,
        info,
        schema_diffs: schema_diffs.to_vec(),
        summary: DiffSummary {
            breaking_count: findings.iter().filter(|f| f.severity == DiffSeverity::Breaking).count(),
            info_count: findings.iter().filter(|f| f.severity == DiffSeverity::Info).count(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::*;
    fn mk_doc(ops: Vec<Operation>, schemas: Vec<(String, Model)>) -> Document { let mut m = indexmap::IndexMap::new(); for (k, v) in schemas { m.insert(k, v); } Document { ir_version: crate::ir::IR_VERSION.to_string(), title: "T".into(), version: "1.0.0".into(), base_url: None, security: vec![], schemas: SchemaRegistry { models: m }, operations: ops, webhooks: vec![] } }
    fn mk_op(id: &str) -> Operation { Operation { operation_id: id.into(), method: HttpMethod::Get, path: "/".into(), tag: None, summary: None, description: None, parameters: vec![], request_body: None, responses: vec![], retry_policy: None } }
    #[test] fn removed_op_is_breaking() { let f = diff(&mk_doc(vec![mk_op("x")], vec![]), &mk_doc(vec![], vec![])); assert_eq!(f.len(), 1); assert_eq!(f[0].severity, DiffSeverity::Breaking); }
    #[test] fn added_op_is_info() { let f = diff(&mk_doc(vec![], vec![]), &mk_doc(vec![mk_op("x")], vec![])); assert_eq!(f.len(), 1); assert_eq!(f[0].severity, DiffSeverity::Info); }
    #[test] fn no_changes() { let d = mk_doc(vec![mk_op("x")], vec![]); assert!(diff(&d, &d).is_empty()); }
    #[test] fn detailed() { let r = diff_detailed(&mk_doc(vec![], vec![]), &mk_doc(vec![], vec![])); assert!(r.schema_diffs.is_empty()); }
    #[test] fn json_fmt() { let f = vec![DiffFinding { severity: DiffSeverity::Breaking, message: "x".into(), path: "y".into() }]; let j = format_json(&f, &[]).unwrap(); let v: serde_json::Value = serde_json::from_str(&j).unwrap(); assert_eq!(v["summary"]["breaking_count"], 1); }
    #[test] fn md_fmt() { let f = vec![DiffFinding { severity: DiffSeverity::Breaking, message: "operation removed: deletePet".into(), path: "x".into() }]; let m = format_markdown(&f, &[], "1.0", "2.0"); assert!(m.contains("Breaking Changes")); }

    #[test]
    fn ir_version_same_no_finding() {
        let old = mk_doc(vec![], vec![]);
        let new = mk_doc(vec![], vec![]);
        let f = diff(&old, &new);
        assert!(f.iter().all(|f| f.path != "ir_version"), "no IR version finding when versions match");
    }

    #[test]
    fn ir_version_minor_change_is_info() {
        let mut old = mk_doc(vec![], vec![]);
        old.ir_version = "1.0".into();
        let mut new = mk_doc(vec![], vec![]);
        new.ir_version = "1.1".into();
        let f = diff(&old, &new);
        let ir_findings: Vec<_> = f.iter().filter(|f| f.path == "ir_version").collect();
        assert_eq!(ir_findings.len(), 1);
        assert_eq!(ir_findings[0].severity, DiffSeverity::Info);
    }

    #[test]
    fn ir_version_major_change_is_breaking() {
        let mut old = mk_doc(vec![], vec![]);
        old.ir_version = "1.0".into();
        let mut new = mk_doc(vec![], vec![]);
        new.ir_version = "2.0".into();
        let f = diff(&old, &new);
        let ir_findings: Vec<_> = f.iter().filter(|f| f.path == "ir_version").collect();
        assert_eq!(ir_findings.len(), 1);
        assert_eq!(ir_findings[0].severity, DiffSeverity::Breaking);
    }

    #[test]
    fn ir_major_version_parser() {
        assert_eq!(ir_major_version("1.0"), 1);
        assert_eq!(ir_major_version("2.3.4"), 2);
        assert_eq!(ir_major_version("0"), 0);
        assert_eq!(ir_major_version("invalid"), 0);
    }
}
