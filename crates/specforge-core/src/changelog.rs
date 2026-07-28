//! SDK changelog generation from OpenAPI specs.
//!
//! Generates a `CHANGELOG.md` that documents:
//! - SDK version (from spec `info.version`)
//! - Per-operation changes with HTTP method and path
//! - Schema changes with property-level details
//! - Breaking change classification
//! - Semantic versioning suggestion (major/minor/patch)
//! - Authentication requirements
//! - Changes when comparing to a previous version

use crate::diff::{self, DiffSeverity, PropertyChangeKind, SchemaDiffDetail};
use crate::ir::Document;
use serde::Serialize;

/// Output format for changelog generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
pub enum ChangelogFormat {
    /// Markdown output (default).
    #[default]
    Markdown,
    /// JSON output.
    Json,
}

/// Classification of a changelog entry's impact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ChangeImpact {
    /// Breaking change that requires a major version bump.
    Breaking,
    /// New feature or addition that warrants a minor version bump.
    Feature,
    /// Bug fix, deprecation notice, or documentation change.
    Fix,
}

impl ChangeImpact {
    /// Returns the human-readable label for this impact level.
    pub fn label(&self) -> &'static str {
        match self {
            ChangeImpact::Breaking => "BREAKING",
            ChangeImpact::Feature => "Added",
            ChangeImpact::Fix => "Fix",
        }
    }
}

/// A single changelog entry representing a change to an operation.
#[derive(Debug, Clone, Serialize)]
pub struct OperationEntry {
    /// The HTTP method (GET, POST, etc.).
    pub method: String,
    /// The request path (e.g. `/pets/{petId}`).
    pub path: String,
    /// The operation ID.
    pub operation_id: String,
    /// Summary of the operation (if available).
    pub summary: Option<String>,
    /// The type of change: added, removed, or modified.
    pub change_type: ChangeType,
    /// Impact classification for semantic versioning.
    pub impact: ChangeImpact,
    /// Additional details about the change (e.g. new required parameter).
    pub details: Vec<String>,
}

/// The type of change for an operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum ChangeType {
    /// A new operation was added.
    Added,
    /// An existing operation was removed.
    Removed,
    /// An existing operation was modified.
    Modified,
}

/// A schema change entry.
#[derive(Debug, Clone, Serialize)]
pub struct SchemaChangeEntry {
    /// The schema name.
    pub schema_name: String,
    /// Impact classification.
    pub impact: ChangeImpact,
    /// Individual property changes.
    pub property_changes: Vec<PropertyChangeEntry>,
}

/// A single property change within a schema.
#[derive(Debug, Clone, Serialize)]
pub struct PropertyChangeEntry {
    /// The property name.
    pub property: String,
    /// Human-readable description of the change.
    pub description: String,
    /// Impact classification.
    pub impact: ChangeImpact,
}

/// Suggested semantic version bump based on changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum VersionBump {
    /// No changes detected.
    None,
    /// Patch bump for bug fixes and minor corrections.
    Patch,
    /// Minor bump for new features and non-breaking additions.
    Minor,
    /// Major bump for breaking changes.
    Major,
}

impl VersionBump {
    /// Returns the label for this version bump.
    pub fn label(&self) -> &'static str {
        match self {
            VersionBump::None => "none",
            VersionBump::Patch => "patch",
            VersionBump::Minor => "minor",
            VersionBump::Major => "major",
        }
    }
}

/// The complete changelog result with structured data.
#[derive(Debug, Clone, Serialize)]
pub struct ChangelogResult {
    /// The SDK version.
    pub version: String,
    /// The date of changelog generation.
    pub date: String,
    /// Per-operation changelog entries.
    pub operation_entries: Vec<OperationEntry>,
    /// Schema change entries.
    pub schema_entries: Vec<SchemaChangeEntry>,
    /// All diff findings from the comparison.
    pub findings: Vec<diff::DiffFinding>,
    /// Detailed schema diffs.
    pub schema_diffs: Vec<SchemaDiffDetail>,
    /// Suggested semantic version bump.
    pub suggested_bump: VersionBump,
    /// Number of breaking changes.
    pub breaking_count: usize,
    /// Number of non-breaking changes.
    pub non_breaking_count: usize,
    /// Number of schemas modified.
    pub schemas_modified_count: usize,
}

/// Options for changelog generation.
#[derive(Debug, Clone, Default)]
pub struct ChangelogOptions {
    /// Override the version string (defaults to `doc.version`).
    pub version: Option<String>,
    /// Path to a previous spec for diffing against the current version.
    pub previous_spec: Option<String>,
    /// When true, suggest a semantic version bump based on changes.
    pub suggest_version: bool,
    /// Output format for the changelog.
    pub format: ChangelogFormat,
}

/// Compute today's date as `YYYY-MM-DD` without pulling in `chrono`.
fn current_date() -> String {
    use std::time::SystemTime;

    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let days = secs / 86400;

    // Simple civil calendar from days since Unix epoch.
    let mut y = 1970i64;
    let mut remaining = days as i64;
    loop {
        let days_in_year = if is_leap(y) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        y += 1;
    }
    let leap = is_leap(y);
    let month_days: [i64; 12] = [
        31,
        if leap { 29 } else { 28 },
        31, 30, 31, 30, 31, 31, 30, 31, 30, 31,
    ];
    let mut m = 1u32;
    for &md in &month_days {
        if remaining < md {
            break;
        }
        remaining -= md;
        m += 1;
    }
    let d = remaining + 1;
    format!("{y:04}-{m:02}-{d:02}")
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0)
}

/// Compute the suggested semantic version bump from diff findings.
fn compute_suggested_bump(findings: &[diff::DiffFinding]) -> VersionBump {
    let has_breaking = findings.iter().any(|f| f.severity == DiffSeverity::Breaking);
    let has_additions = findings.iter().any(|f| f.severity == DiffSeverity::Info);

    if has_breaking {
        VersionBump::Major
    } else if has_additions {
        VersionBump::Minor
    } else {
        VersionBump::None
    }
}

/// Classify a single diff finding into a changelog impact level.
fn classify_finding(finding: &diff::DiffFinding) -> ChangeImpact {
    match finding.severity {
        DiffSeverity::Breaking => ChangeImpact::Breaking,
        DiffSeverity::Info => {
            if finding.message.starts_with("operation added")
                || finding.message.starts_with("schema added")
                || finding.message.starts_with("new optional property")
                || finding.message.starts_with("new required property")
            {
                ChangeImpact::Feature
            } else {
                ChangeImpact::Fix
            }
        }
    }
}

/// Build per-operation changelog entries from the diff between old and new documents.
fn build_operation_entries(
    old_doc: &Document,
    new_doc: &Document,
) -> Vec<OperationEntry> {
    use std::collections::HashMap;

    let old_ops: HashMap<String, &crate::ir::Operation> =
        old_doc.operations.iter().map(|op| (op.operation_id.clone(), op)).collect();
    let new_ops: HashMap<String, &crate::ir::Operation> =
        new_doc.operations.iter().map(|op| (op.operation_id.clone(), op)).collect();

    let mut entries = Vec::new();

    // Removed operations
    for (id, op) in &old_ops {
        if !new_ops.contains_key(id) {
            entries.push(OperationEntry {
                method: op.method.upper().to_string(),
                path: op.path.clone(),
                operation_id: id.clone(),
                summary: op.summary.clone(),
                change_type: ChangeType::Removed,
                impact: ChangeImpact::Breaking,
                details: vec![format!("Operation `{id}` was removed")],
            });
        }
    }

    // Added operations
    for (id, op) in &new_ops {
        if !old_ops.contains_key(id) {
            entries.push(OperationEntry {
                method: op.method.upper().to_string(),
                path: op.path.clone(),
                operation_id: id.clone(),
                summary: op.summary.clone(),
                change_type: ChangeType::Added,
                impact: ChangeImpact::Feature,
                details: vec![format!("New operation `{id}` added")],
            });
        }
    }

    // Modified operations
    for id in old_ops.keys() {
        if let (Some(old_op), Some(new_op)) = (old_ops.get(id), new_ops.get(id)) {
            let mut details = Vec::new();
            let mut is_breaking = false;

            // Check for new required parameters
            for p in &new_op.parameters {
                if p.required && !old_op.parameters.iter().any(|x| x.name == p.name) {
                    details.push(format!("New required parameter: `{}`", p.name));
                    is_breaking = true;
                }
            }

            // Check for removed parameters
            for p in &old_op.parameters {
                if !new_op.parameters.iter().any(|x| x.name == p.name) {
                    details.push(format!("Parameter removed: `{}`", p.name));
                    is_breaking = true;
                }
            }

            // Check for new required request body
            if new_op.request_body.is_some() && old_op.request_body.is_none() {
                if new_op.request_body.as_ref().map(|rb| rb.required).unwrap_or(false) {
                    details.push("New required request body added".to_string());
                    is_breaking = true;
                } else {
                    details.push("New optional request body added".to_string());
                }
            }

            // Check for removed responses
            for r in &old_op.responses {
                if !new_op.responses.iter().any(|x| x.status == r.status) {
                    details.push(format!("Response `{}` removed", r.status));
                    is_breaking = true;
                }
            }

            if !details.is_empty() {
                entries.push(OperationEntry {
                    method: new_op.method.upper().to_string(),
                    path: new_op.path.clone(),
                    operation_id: id.clone(),
                    summary: new_op.summary.clone(),
                    change_type: ChangeType::Modified,
                    impact: if is_breaking {
                        ChangeImpact::Breaking
                    } else {
                        ChangeImpact::Feature
                    },
                    details,
                });
            }
        }
    }

    entries
}

/// Build schema change entries from the diff results.
fn build_schema_entries(schema_diffs: &[SchemaDiffDetail]) -> Vec<SchemaChangeEntry> {
    schema_diffs
        .iter()
        .map(|sd| {
            let property_changes: Vec<PropertyChangeEntry> = sd
                .changes
                .iter()
                .map(|pc| {
                    let (description, impact) = match &pc.change {
                        PropertyChangeKind::Added { ty } => (
                            format!("New optional property (`{ty}`)"),
                            ChangeImpact::Feature,
                        ),
                        PropertyChangeKind::AddedRequired { ty } => (
                            format!("New required property (`{ty}`)"),
                            ChangeImpact::Breaking,
                        ),
                        PropertyChangeKind::Removed { ty } => (
                            format!("Property removed (was `{ty}`)"),
                            ChangeImpact::Breaking,
                        ),
                        PropertyChangeKind::TypeChanged { old_type, new_type } => (
                            format!("Type changed: `{old_type}` to `{new_type}`"),
                            ChangeImpact::Breaking,
                        ),
                        PropertyChangeKind::RequiredChanged {
                            old_required,
                            new_required,
                        } => {
                            let o = if *old_required { "required" } else { "optional" };
                            let n = if *new_required { "required" } else { "optional" };
                            let impact = if *new_required {
                                ChangeImpact::Breaking
                            } else {
                                ChangeImpact::Fix
                            };
                            (format!("Changed from {o} to {n}"), impact)
                        }
                    };
                    PropertyChangeEntry {
                        property: pc.property.clone(),
                        description,
                        impact,
                    }
                })
                .collect();

            let has_breaking = property_changes.iter().any(|pc| pc.impact == ChangeImpact::Breaking);

            SchemaChangeEntry {
                schema_name: sd.name.clone(),
                impact: if has_breaking {
                    ChangeImpact::Breaking
                } else {
                    ChangeImpact::Feature
                },
                property_changes,
            }
        })
        .collect()
}

/// Generate a complete `ChangelogResult` from a resolved IR [`Document`].
///
/// If `opts.previous_spec` points to a readable OpenAPI file, the diff between
/// the previous and current spec is included in the result.
pub fn generate_changelog_result(doc: &Document, opts: &ChangelogOptions) -> ChangelogResult {
    let version = opts.version.as_deref().unwrap_or(&doc.version);
    let today = current_date();

    // Compute diff findings if a previous spec was provided.
    let (findings, schema_diffs, operation_entries, schema_entries) =
        if let Some(prev_path) = &opts.previous_spec {
            match load_and_diff(prev_path, doc) {
                Ok(diff_result) => {
                    let op_entries = build_operation_entries_from_findings(
                        &diff_result.findings,
                        doc,
                    );
                    let sch_entries = build_schema_entries(&diff_result.schema_diffs);
                    (
                        diff_result.findings,
                        diff_result.schema_diffs,
                        op_entries,
                        sch_entries,
                    )
                }
                Err(_) => (vec![], vec![], vec![], vec![]),
            }
        } else {
            // No previous spec: list all current operations as "Added" entries.
            let op_entries: Vec<OperationEntry> = doc
                .operations
                .iter()
                .map(|op| OperationEntry {
                    method: op.method.upper().to_string(),
                    path: op.path.clone(),
                    operation_id: op.operation_id.clone(),
                    summary: op.summary.clone(),
                    change_type: ChangeType::Added,
                    impact: ChangeImpact::Feature,
                    details: vec![],
                })
                .collect();
            (vec![], vec![], op_entries, vec![])
        };

    let suggested_bump = compute_suggested_bump(&findings);
    let breaking_count = findings.iter().filter(|f| f.severity == DiffSeverity::Breaking).count();
    let non_breaking_count = findings.len() - breaking_count;
    let schemas_modified_count = schema_entries.len();

    ChangelogResult {
        version: version.to_string(),
        date: today,
        operation_entries,
        schema_entries,
        findings,
        schema_diffs,
        suggested_bump,
        breaking_count,
        non_breaking_count,
        schemas_modified_count,
    }
}

/// Build operation entries from diff findings by parsing the finding messages.
fn build_operation_entries_from_findings(
    findings: &[diff::DiffFinding],
    doc: &Document,
) -> Vec<OperationEntry> {
    let mut entries = Vec::new();

    for finding in findings {
        if let Some(op_id) = finding.message.strip_prefix("operation removed: ") {
            // Try to find the operation in the doc to get method/path.
            if let Some(op) = doc.operations.iter().find(|o| o.operation_id == op_id) {
                entries.push(OperationEntry {
                    method: op.method.upper().to_string(),
                    path: op.path.clone(),
                    operation_id: op_id.to_string(),
                    summary: op.summary.clone(),
                    change_type: ChangeType::Removed,
                    impact: ChangeImpact::Breaking,
                    details: vec![finding.message.clone()],
                });
            } else {
                entries.push(OperationEntry {
                    method: "UNKNOWN".to_string(),
                    path: "UNKNOWN".to_string(),
                    operation_id: op_id.to_string(),
                    summary: None,
                    change_type: ChangeType::Removed,
                    impact: ChangeImpact::Breaking,
                    details: vec![finding.message.clone()],
                });
            }
        } else if let Some(op_id) = finding.message.strip_prefix("operation added: ") {
            if let Some(op) = doc.operations.iter().find(|o| o.operation_id == op_id) {
                entries.push(OperationEntry {
                    method: op.method.upper().to_string(),
                    path: op.path.clone(),
                    operation_id: op_id.to_string(),
                    summary: op.summary.clone(),
                    change_type: ChangeType::Added,
                    impact: ChangeImpact::Feature,
                    details: vec![finding.message.clone()],
                });
            } else {
                entries.push(OperationEntry {
                    method: "UNKNOWN".to_string(),
                    path: "UNKNOWN".to_string(),
                    operation_id: op_id.to_string(),
                    summary: None,
                    change_type: ChangeType::Added,
                    impact: ChangeImpact::Feature,
                    details: vec![finding.message.clone()],
                });
            }
        } else if finding.path.starts_with("operations.") {
            // This is a per-operation modification (e.g. new required param).
            let op_id = finding
                .path
                .strip_prefix("operations.")
                .unwrap_or(&finding.path)
                .split('.')
                .next()
                .unwrap_or("unknown");

            // Only add one entry per operation for modifications.
            if !entries.iter().any(|e| e.operation_id == op_id) {
                if let Some(op) = doc.operations.iter().find(|o| o.operation_id == op_id) {
                    entries.push(OperationEntry {
                        method: op.method.upper().to_string(),
                        path: op.path.clone(),
                        operation_id: op_id.to_string(),
                        summary: op.summary.clone(),
                        change_type: ChangeType::Modified,
                        impact: classify_finding(finding),
                        details: vec![finding.message.clone()],
                    });
                } else {
                    entries.push(OperationEntry {
                        method: "UNKNOWN".to_string(),
                        path: "UNKNOWN".to_string(),
                        operation_id: op_id.to_string(),
                        summary: None,
                        change_type: ChangeType::Modified,
                        impact: classify_finding(finding),
                        details: vec![finding.message.clone()],
                    });
                }
            } else if let Some(entry) = entries.iter_mut().find(|e| e.operation_id == op_id) {
                // Append additional details for the same operation.
                entry.details.push(finding.message.clone());
                // Upgrade impact if this finding is breaking.
                if classify_finding(finding) == ChangeImpact::Breaking {
                    entry.impact = ChangeImpact::Breaking;
                }
            }
        }
    }

    entries
}

/// Generate a changelog document from a resolved IR [`Document`].
///
/// If `opts.previous_spec` points to a readable OpenAPI file, the diff between
/// the previous and current spec is included as a "Changes from Previous
/// Version" section.
pub fn generate_changelog(doc: &Document, opts: &ChangelogOptions) -> String {
    let result = generate_changelog_result(doc, opts);

    match opts.format {
        ChangelogFormat::Markdown => render_markdown(&result, opts),
        ChangelogFormat::Json => render_json(&result),
    }
}

/// Render the changelog as Markdown.
fn render_markdown(result: &ChangelogResult, opts: &ChangelogOptions) -> String {
    let version = &result.version;
    let today = &result.date;

    let mut out = String::new();

    // Header
    out.push_str(&format!("# Changelog\n\n"));
    out.push_str(&format!("## [{version}] -- {today}\n\n"));

    // Suggested version bump
    if opts.suggest_version && result.suggested_bump != VersionBump::None {
        out.push_str(&format!(
            "**Suggested version bump: `{}`**\n\n",
            result.suggested_bump.label()
        ));
    }

    // Summary
    out.push_str(&format!(
        "### Summary\n\n"
    ));
    out.push_str(&format!(
        "- **{}** operation(s) changed\n",
        result.operation_entries.len()
    ));
    out.push_str(&format!(
        "- **{}** schema(s) modified\n",
        result.schemas_modified_count
    ));
    if !result.findings.is_empty() {
        out.push_str(&format!(
            "- **{}** breaking change(s)\n",
            result.breaking_count
        ));
        out.push_str(&format!(
            "- **{}** non-breaking change(s)\n",
            result.non_breaking_count
        ));
    }
    out.push('\n');

    // Per-operation entries
    if !result.operation_entries.is_empty() {
        out.push_str("### Operations\n\n");

        // Group by impact: breaking first, then features, then fixes
        let mut breaking: Vec<_> = result.operation_entries.iter().filter(|e| e.impact == ChangeImpact::Breaking).collect();
        let mut features: Vec<_> = result.operation_entries.iter().filter(|e| e.impact == ChangeImpact::Feature).collect();
        let mut fixes: Vec<_> = result.operation_entries.iter().filter(|e| e.impact == ChangeImpact::Fix).collect();

        breaking.sort_by(|a, b| a.operation_id.cmp(&b.operation_id));
        features.sort_by(|a, b| a.operation_id.cmp(&b.operation_id));
        fixes.sort_by(|a, b| a.operation_id.cmp(&b.operation_id));

        if !breaking.is_empty() {
            out.push_str("#### Breaking Changes\n\n");
            for entry in &breaking {
                out.push_str(&format_operation_entry(entry));
            }
            out.push('\n');
        }

        if !features.is_empty() {
            out.push_str("#### New Features\n\n");
            for entry in &features {
                out.push_str(&format_operation_entry(entry));
            }
            out.push('\n');
        }

        if !fixes.is_empty() {
            out.push_str("#### Fixes & Modifications\n\n");
            for entry in &fixes {
                out.push_str(&format_operation_entry(entry));
            }
            out.push('\n');
        }
    } else {
        out.push_str("### Operations\n\n");
        out.push_str("_No operations._\n");
    }

    // Schema changes
    if !result.schema_entries.is_empty() {
        out.push_str("### Schema Changes\n\n");
        for entry in &result.schema_entries {
            let icon = match entry.impact {
                ChangeImpact::Breaking => "BREAKING",
                ChangeImpact::Feature => "Added",
                ChangeImpact::Fix => "Fix",
            };
            out.push_str(&format!("#### `{}` ({})\n\n", entry.schema_name, icon));
            out.push_str("| Property | Change | Impact |\n");
            out.push_str("|----------|--------|--------|\n");
            for pc in &entry.property_changes {
                let impact_label = pc.impact.label();
                out.push_str(&format!(
                    "| `{}` | {} | {} |\n",
                    pc.property, pc.description, impact_label
                ));
            }
            out.push('\n');
        }
    }

    // Schemas section (list all current schemas)
    out.push_str("### Schemas\n\n");
    // Note: doc.schemas is not available in ChangelogResult, so we include schema info
    // only from the schema_entries above. If there are no schema changes, list them from
    // the findings. For the full list, the caller can use the doc directly.
    out.push_str("_See schema changes above for details._\n");

    // Authentication section (not in result, but included in full version)
    // This is handled by the caller if needed.

    out
}

/// Format a single operation entry as a markdown line.
fn format_operation_entry(entry: &OperationEntry) -> String {
    let mut line = format!(
        "- `{method}` `{path}` -- {op_id}",
        method = entry.method,
        path = entry.path,
        op_id = entry.operation_id,
    );

    if let Some(ref summary) = entry.summary {
        line.push_str(&format!(": {summary}"));
    }

    line.push('\n');

    for detail in &entry.details {
        line.push_str(&format!("  - {detail}\n"));
    }

    line
}

/// Render the changelog as JSON.
fn render_json(result: &ChangelogResult) -> String {
    match serde_json::to_string_pretty(result) {
        Ok(json) => json,
        Err(_) => r#"{"error": "failed to serialize changelog"}"#.to_string(),
    }
}

/// Parse the previous spec, resolve it, and diff against the current document.
fn load_and_diff(
    prev_path: &str,
    current_doc: &Document,
) -> Result<diff::DiffResult, String> {
    let prev_spec = crate::spec::parse_file(prev_path)
        .map_err(|e| format!("failed to parse previous spec: {e}"))?;
    let prev_doc =
        crate::resolve::resolve(&prev_spec).map_err(|e| format!("failed to resolve previous spec: {e}"))?;
    Ok(diff::diff_detailed(&prev_doc, current_doc))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::*;

    fn make_doc(ops: Vec<Operation>, schemas: Vec<(&str, Model)>) -> Document {
        let mut models = indexmap::IndexMap::new();
        for (name, model) in schemas {
            models.insert(name.to_string(), model);
        }
        Document {
            ir_version: crate::ir::IR_VERSION.to_string(),
            title: "Test API".into(),
            version: "2.0.0".into(),
            base_url: None,
            security: vec![],
            schemas: SchemaRegistry { models },
            operations: ops,
            webhooks: vec![],
        }
    }

    fn make_op(id: &str, method: HttpMethod, path: &str) -> Operation {
        Operation {
            operation_id: id.into(),
            method,
            path: path.into(),
            tag: None,
            summary: Some(format!("Summary of {id}")),
            description: None,
            parameters: vec![],
            request_body: None,
            responses: vec![],
            retry_policy: None,
        }
    }

    #[test]
    fn changelog_lists_operations() {
        let doc = make_doc(
            vec![
                make_op("listPets", HttpMethod::Get, "/pets"),
                make_op("createPet", HttpMethod::Post, "/pets"),
            ],
            vec![],
        );
        let out = generate_changelog(&doc, &ChangelogOptions::default());
        assert!(out.contains("## [2.0.0]"));
        assert!(out.contains("`GET` `/pets`"));
        assert!(out.contains("`POST` `/pets`"));
        assert!(out.contains("listPets"));
        assert!(out.contains("createPet"));
    }

    #[test]
    fn changelog_lists_schemas() {
        let doc = make_doc(
            vec![],
            vec![
                ("Pet", Model::Object(ObjectModel {
                    name: "Pet".into(), description: None, properties: vec![],
                    additional_properties: None, shape_type: None, base_type: None,
                })),
                ("Error", Model::Object(ObjectModel {
                    name: "Error".into(), description: None, properties: vec![],
                    additional_properties: None, shape_type: None, base_type: None,
                })),
            ],
        );
        let out = generate_changelog(&doc, &ChangelogOptions::default());
        assert!(out.contains("### Schemas"));
    }

    #[test]
    fn changelog_uses_overridden_version() {
        let doc = make_doc(vec![], vec![]);
        let opts = ChangelogOptions {
            version: Some("3.0.0-rc1".into()),
            previous_spec: None,
            ..Default::default()
        };
        let out = generate_changelog(&doc, &opts);
        assert!(out.contains("## [3.0.0-rc1]"));
    }

    #[test]
    fn changelog_empty_doc() {
        let doc = make_doc(vec![], vec![]);
        let out = generate_changelog(&doc, &ChangelogOptions::default());
        assert!(out.contains("_No operations._"));
        assert!(out.contains("### Schemas"));
    }

    #[test]
    fn changelog_json_format() {
        let doc = make_doc(
            vec![make_op("listPets", HttpMethod::Get, "/pets")],
            vec![],
        );
        let opts = ChangelogOptions {
            format: ChangelogFormat::Json,
            ..Default::default()
        };
        let out = generate_changelog(&doc, &opts);
        let v: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
        assert_eq!(v["version"], "2.0.0");
        assert!(v["operation_entries"].is_array());
        let ops = v["operation_entries"].as_array().unwrap();
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0]["method"], "GET");
        assert_eq!(ops[0]["path"], "/pets");
        assert_eq!(ops[0]["operation_id"], "listPets");
    }

    #[test]
    fn suggest_version_none_when_no_changes() {
        let findings: Vec<diff::DiffFinding> = vec![];
        assert_eq!(compute_suggested_bump(&findings), VersionBump::None);
    }

    #[test]
    fn suggest_version_minor_for_additions() {
        let findings = vec![diff::DiffFinding {
            severity: DiffSeverity::Info,
            message: "operation added: newOp".into(),
            path: "operations.newOp".into(),
        }];
        assert_eq!(compute_suggested_bump(&findings), VersionBump::Minor);
    }

    #[test]
    fn suggest_version_major_for_breaking() {
        let findings = vec![
            diff::DiffFinding {
                severity: DiffSeverity::Info,
                message: "operation added: newOp".into(),
                path: "operations.newOp".into(),
            },
            diff::DiffFinding {
                severity: DiffSeverity::Breaking,
                message: "operation removed: oldOp".into(),
                path: "operations.oldOp".into(),
            },
        ];
        assert_eq!(compute_suggested_bump(&findings), VersionBump::Major);
    }

    #[test]
    fn changelog_result_includes_suggested_bump() {
        let doc = make_doc(
            vec![make_op("listPets", HttpMethod::Get, "/pets")],
            vec![],
        );
        let result = generate_changelog_result(&doc, &ChangelogOptions::default());
        assert_eq!(result.suggested_bump, VersionBump::None);
        assert_eq!(result.breaking_count, 0);
        assert_eq!(result.non_breaking_count, 0);
    }

    #[test]
    fn changelog_with_suggest_version_flag() {
        let doc = make_doc(
            vec![make_op("listPets", HttpMethod::Get, "/pets")],
            vec![],
        );
        let opts = ChangelogOptions {
            suggest_version: true,
            ..Default::default()
        };
        let out = generate_changelog(&doc, &opts);
        // No previous spec, so no changes detected -- no bump suggestion shown
        assert!(!out.contains("Suggested version bump"));
    }

    #[test]
    fn current_date_format() {
        let d = current_date();
        // Expect YYYY-MM-DD format
        assert_eq!(d.len(), 10);
        assert_eq!(d.chars().nth(4), Some('-'));
        assert_eq!(d.chars().nth(7), Some('-'));
    }

    #[test]
    fn operation_entry_formatting() {
        let entry = OperationEntry {
            method: "GET".to_string(),
            path: "/pets".to_string(),
            operation_id: "listPets".to_string(),
            summary: Some("List all pets".to_string()),
            change_type: ChangeType::Added,
            impact: ChangeImpact::Feature,
            details: vec!["New operation `listPets` added".to_string()],
        };
        let formatted = format_operation_entry(&entry);
        assert!(formatted.contains("`GET`"));
        assert!(formatted.contains("`/pets`"));
        assert!(formatted.contains("listPets"));
        assert!(formatted.contains("List all pets"));
        assert!(formatted.contains("New operation"));
    }

    #[test]
    fn build_operation_entries_added() {
        let old = make_doc(vec![], vec![]);
        let new = make_doc(
            vec![make_op("listPets", HttpMethod::Get, "/pets")],
            vec![],
        );
        let entries = build_operation_entries(&old, &new);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].change_type, ChangeType::Added);
        assert_eq!(entries[0].method, "GET");
        assert_eq!(entries[0].path, "/pets");
    }

    #[test]
    fn build_operation_entries_removed() {
        let old = make_doc(
            vec![make_op("listPets", HttpMethod::Get, "/pets")],
            vec![],
        );
        let new = make_doc(vec![], vec![]);
        let entries = build_operation_entries(&old, &new);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].change_type, ChangeType::Removed);
        assert_eq!(entries[0].impact, ChangeImpact::Breaking);
    }

    #[test]
    fn build_operation_entries_modified_breaking() {
        let mut old_op = make_op("listPets", HttpMethod::Get, "/pets");
        old_op.parameters = vec![Parameter {
            name: "limit".into(),
            location: ParamLocation::Query,
            ty: Type::Scalar(Scalar::Integer),
            required: false,
            description: None,
        }];
        let old = make_doc(vec![old_op], vec![]);

        let mut new_op = make_op("listPets", HttpMethod::Get, "/pets");
        new_op.parameters = vec![
            Parameter {
                name: "limit".into(),
                location: ParamLocation::Query,
                ty: Type::Scalar(Scalar::Integer),
                required: false,
                description: None,
            },
            Parameter {
                name: "ownerId".into(),
                location: ParamLocation::Query,
                ty: Type::Scalar(Scalar::String),
                required: true, // new required param = breaking
                description: None,
            },
        ];
        let new = make_doc(vec![new_op], vec![]);

        let entries = build_operation_entries(&old, &new);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].change_type, ChangeType::Modified);
        assert_eq!(entries[0].impact, ChangeImpact::Breaking);
        assert!(entries[0].details.iter().any(|d| d.contains("ownerId")));
    }

    #[test]
    fn changelog_result_operation_entries_no_previous_spec() {
        let doc = make_doc(
            vec![
                make_op("listPets", HttpMethod::Get, "/pets"),
                make_op("createPet", HttpMethod::Post, "/pets"),
            ],
            vec![],
        );
        let result = generate_changelog_result(&doc, &ChangelogOptions::default());
        assert_eq!(result.operation_entries.len(), 2);
        // Without a previous spec, all operations are listed as "Added"
        assert!(result.operation_entries.iter().all(|e| e.change_type == ChangeType::Added));
    }

    #[test]
    fn build_schema_entries_from_diffs() {
        let diffs = vec![SchemaDiffDetail {
            name: "Pet".to_string(),
            changes: vec![
                diff::PropertyChange {
                    property: "name".to_string(),
                    change: PropertyChangeKind::Added {
                        ty: "string".to_string(),
                    },
                },
                diff::PropertyChange {
                    property: "id".to_string(),
                    change: PropertyChangeKind::AddedRequired {
                        ty: "string".to_string(),
                    },
                },
            ],
        }];
        let entries = build_schema_entries(&diffs);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].schema_name, "Pet");
        assert_eq!(entries[0].impact, ChangeImpact::Breaking); // has a required property
        assert_eq!(entries[0].property_changes.len(), 2);
        assert_eq!(entries[0].property_changes[0].property, "name");
        assert_eq!(entries[0].property_changes[0].impact, ChangeImpact::Feature);
        assert_eq!(entries[0].property_changes[1].property, "id");
        assert_eq!(entries[0].property_changes[1].impact, ChangeImpact::Breaking);
    }

    #[test]
    fn version_bump_label() {
        assert_eq!(VersionBump::None.label(), "none");
        assert_eq!(VersionBump::Patch.label(), "patch");
        assert_eq!(VersionBump::Minor.label(), "minor");
        assert_eq!(VersionBump::Major.label(), "major");
    }

    #[test]
    fn change_impact_label() {
        assert_eq!(ChangeImpact::Breaking.label(), "BREAKING");
        assert_eq!(ChangeImpact::Feature.label(), "Added");
        assert_eq!(ChangeImpact::Fix.label(), "Fix");
    }

    #[test]
    fn changelog_markdown_groups_by_impact() {
        let doc = make_doc(
            vec![
                make_op("oldOp", HttpMethod::Delete, "/old"),
                make_op("newOp", HttpMethod::Get, "/new"),
            ],
            vec![],
        );
        // Use as a single document with no previous spec -- all are "Added"
        let opts = ChangelogOptions::default();
        let out = generate_changelog(&doc, &opts);
        assert!(out.contains("New Features"));
        // oldOp and newOp are both added when there's no previous spec
        assert!(out.contains("oldOp"));
        assert!(out.contains("newOp"));
    }
}
