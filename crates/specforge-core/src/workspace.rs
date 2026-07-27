//! Workspace configuration for generating SDKs from multiple specs in one command.
//!
//! A workspace config (`.specforge-workspace.yaml`) lists multiple API specs and
//! their desired outputs so that a single `specforge workspace` invocation can
//! generate all SDKs at once.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::error::SpecError;

/// Top-level workspace configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    pub specs: Vec<WorkspaceSpec>,
}

/// A single spec entry within a workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceSpec {
    /// Human-friendly name for this spec (used in logs and `--only` filtering).
    pub name: String,
    /// Path to the OpenAPI spec file (relative to the workspace config file).
    pub spec: String,
    /// SDK outputs to generate for this spec.
    pub outputs: Vec<WorkspaceOutput>,
}

/// One SDK output target for a workspace spec.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceOutput {
    /// Target language (`ts`, `go`, `rust`).
    pub lang: String,
    /// Output directory (relative to the workspace config file).
    pub out: String,
    /// Optional package / module / crate name.
    #[serde(default)]
    pub name: Option<String>,
}

/// Summary returned after a workspace run.
#[derive(Debug, Clone)]
pub struct WorkspaceRunResult {
    /// Total number of spec entries processed.
    pub specs_processed: usize,
    /// Total number of output targets generated.
    pub outputs_generated: usize,
    /// Total number of files written across all outputs.
    pub files_written: usize,
}

/// Summary returned after a workspace init scan.
#[derive(Debug, Clone)]
pub struct WorkspaceInitResult {
    /// Number of spec files discovered.
    pub specs_found: usize,
    /// Path to the generated config file.
    pub config_path: PathBuf,
}

impl WorkspaceConfig {
    /// Load a workspace config from a YAML file.
    pub fn load(path: &Path) -> Result<Self, SpecError> {
        let content = std::fs::read_to_string(path).map_err(|e| SpecError::Io {
            path: path.display().to_string(),
            source: e,
        })?;
        serde_yaml::from_str(&content).map_err(SpecError::Yaml)
    }

    /// Resolve a spec or output path relative to the workspace config directory.
    pub fn resolve_path(config_path: &Path, relative: &str) -> PathBuf {
        let base = config_path
            .parent()
            .unwrap_or_else(|| Path::new("."));
        base.join(relative)
    }
}

impl WorkspaceSpec {
    /// Return the fully resolved path to the spec file.
    pub fn resolved_spec_path(&self, config_path: &Path) -> PathBuf {
        WorkspaceConfig::resolve_path(config_path, &self.spec)
    }
}

impl WorkspaceOutput {
    /// Return the fully resolved output directory path.
    pub fn resolved_out_path(&self, config_path: &Path) -> PathBuf {
        WorkspaceConfig::resolve_path(config_path, &self.out)
    }
}

/// Scan a directory for OpenAPI spec files and build a workspace config.
///
/// Looks for `*.yaml`, `*.yml`, and `*.json` files that contain an `openapi` key.
pub fn init_workspace(dir: &Path, out: &Path) -> Result<WorkspaceInitResult, SpecError> {
    let mut specs = Vec::new();

    let entries: Vec<PathBuf> = if dir.is_dir() {
        let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
            .map_err(|e| SpecError::Io {
                path: dir.display().to_string(),
                source: e,
            })?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|p| {
                p.is_file()
                    && matches!(
                        p.extension().and_then(|e| e.to_str()),
                        Some("yaml" | "yml" | "json")
                    )
            })
            .collect();
        files.sort();
        files
    } else {
        return Err(SpecError::Invalid(format!(
            "{} is not a directory",
            dir.display()
        )));
    };

    // Resolve the base for relative paths (relative to the output config file).
    let out_base = out.parent().unwrap_or_else(|| Path::new("."));

    for path in &entries {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(_) => continue,
        };

        // Quick check: does this file contain an "openapi" key?
        let is_openapi = if path.extension().and_then(|e| e.to_str()) == Some("json") {
            text.contains("\"openapi\"")
        } else {
            text.contains("openapi:")
        };

        if !is_openapi {
            continue;
        }

        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("spec")
            .to_string();

        // Try to produce a path relative to the output config location.
        let relative_spec = match path.strip_prefix(out_base) {
            Ok(rel) => rel.display().to_string(),
            Err(_) => path.display().to_string(),
        };

        specs.push(WorkspaceSpec {
            name: name.clone(),
            spec: relative_spec,
            outputs: vec![
                WorkspaceOutput {
                    lang: "ts".to_string(),
                    out: format!("sdks/{}-ts", name),
                    name: None,
                },
                WorkspaceOutput {
                    lang: "go".to_string(),
                    out: format!("sdks/{}-go", name),
                    name: None,
                },
            ],
        });
    }

    let config = WorkspaceConfig {
        specs: specs.clone(),
    };

    let yaml = serde_yaml::to_string(&config).map_err(SpecError::Yaml)?;
    let header = "# Generated by `specforge workspace init`\n";
    std::fs::write(out, format!("{header}{yaml}")).map_err(|e| SpecError::Io {
        path: out.display().to_string(),
        source: e,
    })?;

    Ok(WorkspaceInitResult {
        specs_found: specs.len(),
        config_path: out.to_path_buf(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_workspace_config() {
        let yaml = r#"
specs:
  - name: petstore
    spec: specs/petstore.yaml
    outputs:
      - lang: ts
        out: sdks/petstore-ts
        name: "@acme/petstore"
      - lang: go
        out: sdks/petstore-go
        name: github.com/acme/petstore-go

  - name: payments
    spec: specs/payments.yaml
    outputs:
      - lang: ts
        out: sdks/payments-ts
        name: "@acme/payments"
"#;
        let config: WorkspaceConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.specs.len(), 2);
        assert_eq!(config.specs[0].name, "petstore");
        assert_eq!(config.specs[0].outputs.len(), 2);
        assert_eq!(
            config.specs[0].outputs[0].name.as_deref(),
            Some("@acme/petstore")
        );
        assert_eq!(config.specs[1].name, "payments");
        assert_eq!(config.specs[1].outputs[0].lang, "ts");
    }

    #[test]
    fn workspace_output_name_is_optional() {
        let yaml = r#"
specs:
  - name: test
    spec: test.yaml
    outputs:
      - lang: ts
        out: out/ts
"#;
        let config: WorkspaceConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.specs[0].outputs[0].name.is_none());
    }

    #[test]
    fn resolve_paths() {
        let config_path = Path::new("/project/.specforge-workspace.yaml");
        let resolved =
            WorkspaceConfig::resolve_path(config_path, "specs/petstore.yaml");
        assert_eq!(resolved, PathBuf::from("/project/specs/petstore.yaml"));
    }
}
