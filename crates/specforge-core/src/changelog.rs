//! SDK changelog generation from OpenAPI specs.
//!
//! Generates a `CHANGELOG.md` that documents:
//! - SDK version (from spec `info.version`)
//! - Operations available
//! - Schemas/models available
//! - Authentication requirements
//! - Breaking changes when comparing to a previous version

use crate::diff::{self, DiffSeverity};
use crate::ir::Document;

/// Options for changelog generation.
#[derive(Debug, Clone, Default)]
pub struct ChangelogOptions {
    /// Override the version string (defaults to `doc.version`).
    pub version: Option<String>,
    /// Path to a previous spec for diffing against the current version.
    pub previous_spec: Option<String>,
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

/// Generate a changelog document from a resolved IR [`Document`].
///
/// If `opts.previous_spec` points to a readable OpenAPI file, the diff between
/// the previous and current spec is included as a "Changes from Previous
/// Version" section.
pub fn generate_changelog(doc: &Document, opts: &ChangelogOptions) -> String {
    let version = opts.version.as_deref().unwrap_or(&doc.version);
    let today = current_date();

    let mut out = String::new();

    // Header
    out.push_str(&format!("# Changelog\n\n"));
    out.push_str(&format!("## [{version}] -- {today}\n\n"));

    // ── Operations ───────────────────────────────────────────────────────
    out.push_str("### Operations\n\n");
    if doc.operations.is_empty() {
        out.push_str("_No operations._\n");
    } else {
        for op in &doc.operations {
            let summary = op.summary.as_deref().unwrap_or(&op.operation_id);
            let method = op.method.upper();
            out.push_str(&format!("- `{method}` {} -- {}\n", op.path, summary));
        }
    }

    // ── Schemas ──────────────────────────────────────────────────────────
    out.push_str("\n### Schemas\n\n");
    if doc.schemas.models.is_empty() {
        out.push_str("_No schemas._\n");
    } else {
        for (name, _) in doc.schemas.iter() {
            out.push_str(&format!("- `{name}`\n"));
        }
    }

    // ── Authentication ───────────────────────────────────────────────────
    if !doc.security.is_empty() {
        out.push_str("\n### Authentication\n\n");
        for scheme in &doc.security {
            match scheme {
                crate::ir::SecurityScheme::HttpBearer => {
                    out.push_str("- Bearer token (HTTP Authorization header)\n");
                }
                crate::ir::SecurityScheme::ApiKey { header } => {
                    out.push_str(&format!("- API key via `{header}` header\n"));
                }
            }
        }
    }

    // ── Changes from Previous Version ────────────────────────────────────
    if let Some(prev_path) = &opts.previous_spec {
        match load_and_diff(prev_path, doc) {
            Ok(findings) if !findings.is_empty() => {
                out.push_str("\n### Changes from Previous Version\n\n");
                for finding in &findings {
                    let prefix = if finding.severity == DiffSeverity::Breaking {
                        "BREAKING"
                    } else {
                        "Added"
                    };
                    out.push_str(&format!("- **{prefix}**: {}\n", finding.message));
                }
            }
            Ok(_) => {
                out.push_str("\n### Changes from Previous Version\n\n");
                out.push_str("_No changes._\n");
            }
            Err(e) => {
                out.push_str(&format!(
                    "\n<!-- Warning: could not diff against previous spec: {e} -->\n"
                ));
            }
        }
    }

    out
}

/// Parse the previous spec, resolve it, and diff against the current document.
fn load_and_diff(
    prev_path: &str,
    current_doc: &Document,
) -> Result<Vec<diff::DiffFinding>, String> {
    let prev_spec = crate::spec::parse_file(prev_path)
        .map_err(|e| format!("failed to parse previous spec: {e}"))?;
    let prev_doc =
        crate::resolve::resolve(&prev_spec).map_err(|e| format!("failed to resolve previous spec: {e}"))?;
    Ok(diff::diff(&prev_doc, current_doc))
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
        assert!(out.contains("`GET` /pets"));
        assert!(out.contains("`POST` /pets"));
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
        assert!(out.contains("- `Pet`"));
        assert!(out.contains("- `Error`"));
    }

    #[test]
    fn changelog_shows_bearer_auth() {
        let doc = Document {
            ir_version: crate::ir::IR_VERSION.to_string(),
            title: "T".into(),
            version: "1.0".into(),
            base_url: None,
            security: vec![SecurityScheme::HttpBearer],
            schemas: SchemaRegistry::default(),
            operations: vec![],
            webhooks: vec![],
        };
        let out = generate_changelog(&doc, &ChangelogOptions::default());
        assert!(out.contains("Bearer token"));
    }

    #[test]
    fn changelog_shows_api_key_auth() {
        let doc = Document {
            ir_version: crate::ir::IR_VERSION.to_string(),
            title: "T".into(),
            version: "1.0".into(),
            base_url: None,
            security: vec![SecurityScheme::ApiKey {
                header: "X-API-Key".into(),
            }],
            schemas: SchemaRegistry::default(),
            operations: vec![],
            webhooks: vec![],
        };
        let out = generate_changelog(&doc, &ChangelogOptions::default());
        assert!(out.contains("`X-API-Key` header"));
    }

    #[test]
    fn changelog_uses_overridden_version() {
        let doc = make_doc(vec![], vec![]);
        let opts = ChangelogOptions {
            version: Some("3.0.0-rc1".into()),
            previous_spec: None,
        };
        let out = generate_changelog(&doc, &opts);
        assert!(out.contains("## [3.0.0-rc1]"));
    }

    #[test]
    fn changelog_empty_doc() {
        let doc = make_doc(vec![], vec![]);
        let out = generate_changelog(&doc, &ChangelogOptions::default());
        assert!(out.contains("_No operations._"));
        assert!(out.contains("_No schemas._"));
    }

    #[test]
    fn current_date_format() {
        let d = current_date();
        // Expect YYYY-MM-DD format
        assert_eq!(d.len(), 10);
        assert_eq!(d.chars().nth(4), Some('-'));
        assert_eq!(d.chars().nth(7), Some('-'));
    }
}
