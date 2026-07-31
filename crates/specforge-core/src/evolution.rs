//! Schema evolution: track how OpenAPI schemas change over git history.

use std::process::Command;

use crate::diff::{self, DiffSeverity};
use crate::ir::Document;
use crate::resolve;
use crate::spec::parse_str;
use serde::Serialize;

/// A snapshot of the schema at a single git commit.
#[derive(Debug, Clone, Serialize)]
pub struct VersionSnapshot {
    /// The full git commit hash.
    pub commit: String,
    /// ISO 8601 date/time of the commit.
    pub date: String,
    /// Number of schemas defined in this version.
    pub schema_count: usize,
    /// Number of operations defined in this version.
    pub operation_count: usize,
    /// Number of breaking changes introduced in this version (vs. the
    /// previous version in the timeline).
    pub breaking_changes: usize,
}

/// The full evolution timeline for a spec file.
#[derive(Debug, Clone, Serialize)]
pub struct SchemaEvolution {
    /// Ordered list of snapshots (oldest first).
    pub versions: Vec<VersionSnapshot>,
}

/// Output format for the evolution timeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvolutionFormat {
    Text,
    Json,
    Markdown,
}

/// Track how a spec file has evolved across git commits.
///
/// Uses `git log` to discover commits that touched `spec_path`, then parses
/// the spec at each commit and compares adjacent versions to count breaking
/// changes. The `spec_path` should be relative to the git repository root
/// (or an absolute path that git can follow).
pub fn track_evolution(
    spec_path: &str,
) -> Result<SchemaEvolution, Box<dyn std::error::Error + Send + Sync>> {
    // Discover commits that changed this file, oldest first.
    let commits = git_log_for_file(spec_path)?;

    if commits.is_empty() {
        return Ok(SchemaEvolution { versions: vec![] });
    }

    // Parse the spec at each commit and build snapshots.
    let mut snapshots: Vec<(String, String, Document)> = Vec::new();

    for (hash, date) in &commits {
        match git_show_file(spec_path, hash) {
            Ok(content) => match parse_str(&content) {
                Ok(spec) => match resolve::resolve(&spec) {
                    Ok(doc) => {
                        snapshots.push((hash.clone(), date.clone(), doc));
                    }
                    Err(_) => {
                        // Skip commits where the spec doesn't resolve (e.g.,
                        // partial edits, broken intermediate states).
                    }
                },
                Err(_) => {
                    // Skip commits where the spec can't be parsed.
                }
            },
            Err(_) => {
                // The file didn't exist at this commit (e.g., it was added
                // later and this commit is before the add).
            }
        }
    }

    // Build version snapshots with breaking change counts.
    let mut versions = Vec::with_capacity(snapshots.len());

    for i in 0..snapshots.len() {
        let (ref commit, ref date, ref doc) = snapshots[i];

        let breaking_changes = if i > 0 {
            let prev_doc = &snapshots[i - 1].2;
            let findings = diff::diff(prev_doc, doc);
            findings
                .iter()
                .filter(|f| f.severity == DiffSeverity::Breaking)
                .count()
        } else {
            0
        };

        versions.push(VersionSnapshot {
            commit: commit.clone(),
            date: date.clone(),
            schema_count: doc.schemas.models.len(),
            operation_count: doc.operations.len(),
            breaking_changes,
        });
    }

    Ok(SchemaEvolution { versions })
}

/// Run `git log` to get the commit hash and date for every commit that
/// touched `spec_path`, ordered oldest-first.
fn git_log_for_file(
    spec_path: &str,
) -> Result<Vec<(String, String)>, Box<dyn std::error::Error + Send + Sync>> {
    let output = Command::new("git")
        .args([
            "log",
            "--format=%H,%aI",
            "--follow",
            "--diff-filter=ACMR",
            "--",
            spec_path,
        ])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git log failed: {stderr}").into());
    }

    let stdout = String::from_utf8(output.stdout)?;
    let mut commits = Vec::new();

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some((hash, date)) = line.split_once(',') {
            commits.push((hash.to_string(), date.to_string()));
        }
    }

    // git log returns newest-first; reverse to get oldest-first.
    commits.reverse();
    Ok(commits)
}

/// Get the contents of `spec_path` at a specific git commit using
/// `git show <commit>:<path>`.
fn git_show_file(
    spec_path: &str,
    commit: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let refspec = format!("{commit}:{spec_path}");
    let output = Command::new("git").args(["show", &refspec]).output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git show failed: {stderr}").into());
    }

    Ok(String::from_utf8(output.stdout)?)
}

/// Format the evolution timeline as human-readable text.
pub fn format_text(evo: &SchemaEvolution) -> String {
    if evo.versions.is_empty() {
        return "No evolution history found.\n".to_string();
    }

    let mut out = String::new();
    out.push_str("Schema Evolution Timeline\n");
    out.push_str("========================\n\n");

    for v in &evo.versions {
        let short = &v.commit[..v.commit.len().min(8)];
        let marker = if v.breaking_changes > 0 {
            format!(" [{} BREAKING]", v.breaking_changes)
        } else {
            String::new()
        };
        out.push_str(&format!(
            "  {} {} schemas, {} ops{}\n",
            short, v.schema_count, v.operation_count, marker,
        ));
        out.push_str(&format!("  date: {}\n\n", v.date));
    }

    let total = evo.versions.len();
    let total_breaking: usize = evo.versions.iter().map(|v| v.breaking_changes).sum();
    out.push_str(&format!(
        "{total} version(s), {total_breaking} total breaking change(s)\n"
    ));

    out
}

/// Format the evolution timeline as a Markdown table.
pub fn format_markdown(evo: &SchemaEvolution) -> String {
    if evo.versions.is_empty() {
        return "_No evolution history found._\n".to_string();
    }

    let mut out = String::new();
    out.push_str("# Schema Evolution\n\n");
    out.push_str("| Commit | Date | Schemas | Operations | Breaking Changes |\n");
    out.push_str("|--------|------|---------|------------|------------------|\n");

    for v in &evo.versions {
        let short = &v.commit[..v.commit.len().min(8)];
        out.push_str(&format!(
            "| `{short}` | {} | {} | {} | {} |\n",
            v.date, v.schema_count, v.operation_count, v.breaking_changes,
        ));
    }

    let total = evo.versions.len();
    let total_breaking: usize = evo.versions.iter().map(|v| v.breaking_changes).sum();
    out.push_str(&format!(
        "\n**{total}** version(s), **{total_breaking}** total breaking change(s)\n"
    ));

    out
}

/// Format the evolution timeline as JSON.
pub fn format_json(evo: &SchemaEvolution) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(evo)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_evolution_text() {
        let evo = SchemaEvolution { versions: vec![] };
        let t = format_text(&evo);
        assert!(t.contains("No evolution history"));
    }

    #[test]
    fn empty_evolution_markdown() {
        let evo = SchemaEvolution { versions: vec![] };
        let m = format_markdown(&evo);
        assert!(m.contains("No evolution history"));
    }

    #[test]
    fn single_version_text() {
        let evo = SchemaEvolution {
            versions: vec![VersionSnapshot {
                commit: "abc123def456".to_string(),
                date: "2025-01-01T00:00:00+00:00".to_string(),
                schema_count: 5,
                operation_count: 10,
                breaking_changes: 0,
            }],
        };
        let t = format_text(&evo);
        assert!(t.contains("5 schemas"));
        assert!(t.contains("10 ops"));
        assert!(t.contains("1 version(s)"));
    }

    #[test]
    fn single_version_markdown() {
        let evo = SchemaEvolution {
            versions: vec![VersionSnapshot {
                commit: "abc123def456".to_string(),
                date: "2025-01-01T00:00:00+00:00".to_string(),
                schema_count: 5,
                operation_count: 10,
                breaking_changes: 0,
            }],
        };
        let m = format_markdown(&evo);
        assert!(m.contains("| `abc123de` |"));
        assert!(m.contains("| 5 |"));
        assert!(m.contains("| 10 |"));
    }

    #[test]
    fn breaking_changes_highlighted() {
        let evo = SchemaEvolution {
            versions: vec![
                VersionSnapshot {
                    commit: "aaa".to_string(),
                    date: "2025-01-01T00:00:00+00:00".to_string(),
                    schema_count: 3,
                    operation_count: 5,
                    breaking_changes: 0,
                },
                VersionSnapshot {
                    commit: "bbb".to_string(),
                    date: "2025-02-01T00:00:00+00:00".to_string(),
                    schema_count: 4,
                    operation_count: 6,
                    breaking_changes: 2,
                },
            ],
        };
        let t = format_text(&evo);
        assert!(t.contains("[2 BREAKING]"));
        assert!(t.contains("2 total breaking change(s)"));
    }

    #[test]
    fn json_serialization() {
        let evo = SchemaEvolution {
            versions: vec![VersionSnapshot {
                commit: "abc".to_string(),
                date: "2025-01-01T00:00:00+00:00".to_string(),
                schema_count: 1,
                operation_count: 2,
                breaking_changes: 0,
            }],
        };
        let j = format_json(&evo).unwrap();
        let v: serde_json::Value = serde_json::from_str(&j).unwrap();
        assert_eq!(v["versions"][0]["commit"], "abc");
        assert_eq!(v["versions"][0]["schema_count"], 1);
    }
}
