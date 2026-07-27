//! Configurable lint rule settings.
//!
//! Lint rules can be enabled/disabled and assigned a severity level via a
//! [`LintConfig`]. Configuration can be loaded from a YAML file
//! (`.specforge.yaml`) or built programmatically.

use serde::{Deserialize, Serialize};

/// Severity override for a lint rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleSeverity {
    Error,
    Warning,
    Off,
}

impl std::fmt::Display for RuleSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuleSeverity::Error => write!(f, "error"),
            RuleSeverity::Warning => write!(f, "warning"),
            RuleSeverity::Off => write!(f, "off"),
        }
    }
}

/// A single configurable lint rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintRule {
    pub name: String,
    pub enabled: bool,
    pub severity: RuleSeverity,
}

/// Top-level lint configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintConfig {
    pub rules: Vec<LintRule>,
}

impl Default for LintConfig {
    fn default() -> Self {
        Self {
            rules: vec![
                LintRule {
                    name: "duplicate-operation-ids".into(),
                    enabled: true,
                    severity: RuleSeverity::Error,
                },
                LintRule {
                    name: "missing-response-description".into(),
                    enabled: true,
                    severity: RuleSeverity::Warning,
                },
                LintRule {
                    name: "missing-operation-summary".into(),
                    enabled: true,
                    severity: RuleSeverity::Warning,
                },
                LintRule {
                    name: "unused-schema".into(),
                    enabled: true,
                    severity: RuleSeverity::Warning,
                },
                LintRule {
                    name: "missing-operation-id".into(),
                    enabled: true,
                    severity: RuleSeverity::Warning,
                },
                LintRule {
                    name: "missing-schema-description".into(),
                    enabled: false,
                    severity: RuleSeverity::Warning,
                },
                LintRule {
                    name: "path-trailing-slash".into(),
                    enabled: true,
                    severity: RuleSeverity::Warning,
                },
                LintRule {
                    name: "deprecated-operation".into(),
                    enabled: true,
                    severity: RuleSeverity::Warning,
                },
            ],
        }
    }
}

impl LintConfig {
    /// Check whether a rule is enabled. Unknown rules default to enabled.
    pub fn is_enabled(&self, rule_name: &str) -> bool {
        self.rules
            .iter()
            .find(|r| r.name == rule_name)
            .map(|r| r.enabled && r.severity != RuleSeverity::Off)
            .unwrap_or(true)
    }

    /// Get the severity for a rule. Unknown rules default to Warning.
    pub fn severity(&self, rule_name: &str) -> crate::lint::Severity {
        match self
            .rules
            .iter()
            .find(|r| r.name == rule_name)
            .map(|r| r.severity)
            .unwrap_or(RuleSeverity::Warning)
        {
            RuleSeverity::Error => crate::lint::Severity::Error,
            RuleSeverity::Warning => crate::lint::Severity::Warning,
            RuleSeverity::Off => crate::lint::Severity::Warning,
        }
    }

    /// Override a single rule's enabled state. If the rule does not exist yet,
    /// it is added with the default severity of `Warning`.
    pub fn set_enabled(&mut self, rule_name: &str, enabled: bool) {
        if let Some(rule) = self.rules.iter_mut().find(|r| r.name == rule_name) {
            rule.enabled = enabled;
        } else {
            self.rules.push(LintRule {
                name: rule_name.to_string(),
                enabled,
                severity: RuleSeverity::Warning,
            });
        }
    }

    /// Override a single rule's severity. If the rule does not exist yet,
    /// it is added as enabled.
    pub fn set_severity(&mut self, rule_name: &str, severity: RuleSeverity) {
        if let Some(rule) = self.rules.iter_mut().find(|r| r.name == rule_name) {
            rule.severity = severity;
        } else {
            self.rules.push(LintRule {
                name: rule_name.to_string(),
                enabled: severity != RuleSeverity::Off,
                severity,
            });
        }
    }

    /// Load configuration from a YAML file. Falls back to defaults on any error.
    pub fn load_from_file(path: &std::path::Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(content) => match serde_yaml::from_str::<LintConfig>(&content) {
                Ok(config) => config,
                Err(e) => {
                    tracing::warn!(
                        "failed to parse lint config from {}: {e}, using defaults",
                        path.display()
                    );
                    Self::default()
                }
            },
            Err(e) => {
                tracing::warn!(
                    "failed to read lint config from {}: {e}, using defaults",
                    path.display()
                );
                Self::default()
            }
        }
    }

    /// Try to load configuration from well-known locations in the current
    /// directory. Returns defaults if no config file is found.
    pub fn load() -> Self {
        for name in &[".specforge.yaml", ".specforge.yml"] {
            let path = std::path::Path::new(name);
            if path.exists() {
                return Self::load_from_file(path);
            }
        }
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_expected_rules() {
        let config = LintConfig::default();
        assert_eq!(config.rules.len(), 8);
        assert!(config.is_enabled("duplicate-operation-ids"));
        assert!(config.is_enabled("missing-response-description"));
        assert!(config.is_enabled("missing-operation-summary"));
        assert!(config.is_enabled("unused-schema"));
        assert!(config.is_enabled("missing-operation-id"));
        assert!(!config.is_enabled("missing-schema-description"));
        assert!(config.is_enabled("path-trailing-slash"));
        assert!(config.is_enabled("deprecated-operation"));
    }

    #[test]
    fn unknown_rules_default_to_enabled() {
        let config = LintConfig::default();
        assert!(config.is_enabled("nonexistent-rule"));
    }

    #[test]
    fn disable_rule_via_set_enabled() {
        let mut config = LintConfig::default();
        assert!(config.is_enabled("duplicate-operation-ids"));
        config.set_enabled("duplicate-operation-ids", false);
        assert!(!config.is_enabled("duplicate-operation-ids"));
    }

    #[test]
    fn set_severity_on_existing_rule() {
        let mut config = LintConfig::default();
        config.set_severity("unused-schema", RuleSeverity::Error);
        assert_eq!(
            config.severity("unused-schema"),
            crate::lint::Severity::Error
        );
    }

    #[test]
    fn set_severity_off_disables_rule() {
        let mut config = LintConfig::default();
        config.set_severity("unused-schema", RuleSeverity::Off);
        assert!(!config.is_enabled("unused-schema"));
    }

    #[test]
    fn set_enabled_adds_new_rule() {
        let mut config = LintConfig::default();
        config.set_enabled("custom-rule", true);
        assert!(config.is_enabled("custom-rule"));
        assert_eq!(config.rules.len(), 9);
    }

    #[test]
    fn set_severity_adds_new_rule() {
        let mut config = LintConfig::default();
        config.set_severity("custom-rule", RuleSeverity::Error);
        assert_eq!(
            config.severity("custom-rule"),
            crate::lint::Severity::Error
        );
        assert!(config.is_enabled("custom-rule"));
    }

    #[test]
    fn load_from_yaml_string() {
        let yaml = r#"
rules:
  - name: duplicate-operation-ids
    enabled: false
    severity: warning
  - name: unused-schema
    enabled: true
    severity: error
"#;
        let config: LintConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(!config.is_enabled("duplicate-operation-ids"));
        assert_eq!(
            config.severity("duplicate-operation-ids"),
            crate::lint::Severity::Warning
        );
        assert!(config.is_enabled("unused-schema"));
        assert_eq!(
            config.severity("unused-schema"),
            crate::lint::Severity::Error
        );
    }

    #[test]
    fn severity_display() {
        assert_eq!(RuleSeverity::Error.to_string(), "error");
        assert_eq!(RuleSeverity::Warning.to_string(), "warning");
        assert_eq!(RuleSeverity::Off.to_string(), "off");
    }
}
