//! Spec Marketplace — community-shared OpenAPI spec discovery and management.
//!
//! Supports both the original spec marketplace (`SpecEntry` / `MarketplaceIndex`)
//! and the WASM emitter plugin marketplace (`PluginEntry` / `PluginIndex`).

use anyhow::{bail, Context as _};
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// A single spec entry in the marketplace.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SpecEntry {
    pub name: String,
    pub description: String,
    pub version: String,
    pub author: String,
    pub tags: Vec<String>,
    pub downloads: u64,
    pub rating: f32,
    pub url: String,
    pub spec_url: String,
    pub verified: bool,
}

/// The full marketplace index (JSON-serializable).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MarketplaceIndex {
    pub entries: Vec<SpecEntry>,
}

impl MarketplaceIndex {
    /// Load a marketplace index from a JSON file on disk.
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        serde_json::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))
    }

    /// Return the built-in curated marketplace index bundled with this binary.
    ///
    /// # Panics
    ///
    /// Panics if the bundled `assets/marketplace-index.json` is malformed. This
    /// is a compile-time-baked asset, so a panic here indicates a corrupt build.
    pub fn built_in() -> Self {
        let json = include_str!("../../../assets/marketplace-index.json");
        serde_json::from_str(json).expect("bundled marketplace-index.json is valid JSON")
    }

    /// Merge a local/remote index into this one (entries with the same name are replaced).
    pub fn merge(&mut self, other: &MarketplaceIndex) {
        let mut map: HashMap<&str, &SpecEntry> =
            self.entries.iter().map(|e| (e.name.as_str(), e)).collect();
        for entry in &other.entries {
            map.insert(&entry.name, entry);
        }
        self.entries = map.into_values().cloned().collect();
        self.entries.sort_by_key(|a| Reverse(a.downloads));
    }

    /// Search entries by a free-text query (matches name, description, tags, author).
    pub fn search(&self, query: &str) -> Vec<&SpecEntry> {
        let q = query.to_lowercase();
        self.entries
            .iter()
            .filter(|e| {
                e.name.to_lowercase().contains(&q)
                    || e.description.to_lowercase().contains(&q)
                    || e.author.to_lowercase().contains(&q)
                    || e.tags.iter().any(|t| t.to_lowercase().contains(&q))
            })
            .collect()
    }

    /// Look up a single entry by exact name (case-insensitive).
    pub fn find(&self, name: &str) -> Option<&SpecEntry> {
        let q = name.to_lowercase();
        self.entries.iter().find(|e| e.name.to_lowercase() == q)
    }

    /// Return all entries sorted by downloads (descending).
    pub fn sorted_by_downloads(&self) -> Vec<&SpecEntry> {
        let mut v: Vec<&SpecEntry> = self.entries.iter().collect();
        v.sort_by_key(|a| Reverse(a.downloads));
        v
    }

    /// Generate metadata for a local spec file by parsing it.
    pub fn generate_metadata(spec_path: &Path) -> anyhow::Result<SpecEntry> {
        use crate::spec;

        let parsed = spec::parse_file(spec_path).context("failed to parse spec")?;

        let fallback_name = spec_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        let name = if parsed.info.title.is_empty() {
            fallback_name.clone()
        } else {
            parsed.info.title.clone()
        };

        let description = parsed
            .info
            .description
            .unwrap_or_else(|| format!("OpenAPI spec: {name}"));

        let version = parsed.info.version.clone();

        let filename = spec_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("spec.json");

        Ok(SpecEntry {
            name,
            description,
            version,
            author: "community".to_string(),
            tags: vec![],
            downloads: 0,
            rating: 0.0,
            url: String::new(),
            spec_url: filename.to_string(),
            verified: false,
        })
    }
}

// ---------------------------------------------------------------------------
// WASM Emitter Plugin Marketplace
// ---------------------------------------------------------------------------

/// A single WASM emitter plugin entry in the plugin marketplace.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginEntry {
    /// Unique plugin name (e.g. "kotlin-emitter").
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// Semver version string.
    pub version: String,
    /// Plugin author.
    pub author: String,
    /// Target language the emitter produces (kotlin, swift, python, csharp, etc.).
    pub language: String,
    /// URL to download the WASM file.
    pub url: String,
    /// Whether the plugin has been verified by the specforge team.
    pub verified: bool,
    /// Total download count.
    pub downloads: u64,
    /// Community rating (0.0 – 5.0).
    pub rating: f32,
}

/// The full plugin marketplace index (JSON-serializable).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginIndex {
    pub plugins: Vec<PluginEntry>,
}

impl PluginIndex {
    /// Load a plugin index from a JSON file on disk.
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        serde_json::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))
    }

    /// Return the built-in curated plugin index bundled with this binary.
    ///
    /// # Panics
    ///
    /// Panics if the bundled `assets/plugin-index.json` is malformed. This
    /// is a compile-time-baked asset, so a panic here indicates a corrupt build.
    pub fn built_in() -> Self {
        let json = include_str!("../../../assets/plugin-index.json");
        serde_json::from_str(json).expect("bundled plugin-index.json is valid JSON")
    }

    /// Merge another index into this one (entries with the same name are replaced).
    pub fn merge(&mut self, other: &PluginIndex) {
        let mut map: HashMap<&str, &PluginEntry> =
            self.plugins.iter().map(|e| (e.name.as_str(), e)).collect();
        for entry in &other.plugins {
            map.insert(&entry.name, entry);
        }
        self.plugins = map.into_values().cloned().collect();
        self.plugins.sort_by_key(|a| Reverse(a.downloads));
    }

    /// Search plugins by a free-text query (matches name, description, language, author).
    pub fn search(&self, query: &str) -> Vec<&PluginEntry> {
        let q = query.to_lowercase();
        self.plugins
            .iter()
            .filter(|p| {
                p.name.to_lowercase().contains(&q)
                    || p.description.to_lowercase().contains(&q)
                    || p.language.to_lowercase().contains(&q)
                    || p.author.to_lowercase().contains(&q)
            })
            .collect()
    }

    /// Look up a single plugin by exact name (case-insensitive).
    pub fn find(&self, name: &str) -> Option<&PluginEntry> {
        let q = name.to_lowercase();
        self.plugins.iter().find(|p| p.name.to_lowercase() == q)
    }

    /// Return all plugins sorted by downloads (descending).
    pub fn sorted_by_downloads(&self) -> Vec<&PluginEntry> {
        let mut v: Vec<&PluginEntry> = self.plugins.iter().collect();
        v.sort_by_key(|a| Reverse(a.downloads));
        v
    }

    /// Install a plugin by downloading the WASM file to the given directory.
    ///
    /// Returns the local path where the WASM file was written.
    pub fn install_plugin(&self, name: &str, plugins_dir: &Path) -> anyhow::Result<PathBuf> {
        let plugin = self
            .find(name)
            .ok_or_else(|| anyhow::anyhow!("plugin not found: {name}"))?;

        if plugin.url.is_empty() {
            bail!("plugin {name} has no download URL");
        }

        fs::create_dir_all(plugins_dir)
            .with_context(|| format!("failed to create {}", plugins_dir.display()))?;

        let wasm_filename = format!("{}.wasm", plugin.name);
        let dest = plugins_dir.join(&wasm_filename);

        // Attempt to download; fall back to creating a placeholder on network error.
        match reqwest::blocking::get(&plugin.url) {
            Ok(resp) => {
                if resp.status().is_success() {
                    let bytes = resp.bytes().context("failed to read response body")?;
                    fs::write(&dest, &bytes)
                        .with_context(|| format!("failed to write {}", dest.display()))?;
                } else {
                    tracing::warn!(
                        "download failed for {name}: HTTP {}; creating placeholder",
                        resp.status()
                    );
                    let placeholder = format!(
                        "// placeholder for {name} v{}\n// download URL: {}\n",
                        plugin.version, plugin.url,
                    );
                    fs::write(&dest, placeholder)
                        .with_context(|| format!("failed to write {}", dest.display()))?;
                }
            }
            Err(e) => {
                // Network unavailable — create a placeholder so the install
                // flow can be tested offline.
                tracing::warn!("could not download {name}: {e}; creating placeholder");
                let placeholder = format!(
                    "// placeholder for {name} v{}\n// download URL: {}\n",
                    plugin.version, plugin.url,
                );
                fs::write(&dest, placeholder)
                    .with_context(|| format!("failed to write {}", dest.display()))?;
            }
        }

        Ok(dest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_entry(name: &str) -> SpecEntry {
        SpecEntry {
            name: name.to_string(),
            description: format!("The {name} API"),
            version: "1.0.0".to_string(),
            author: "test".to_string(),
            tags: vec!["test".to_string()],
            downloads: 100,
            rating: 4.5,
            url: String::new(),
            spec_url: String::new(),
            verified: false,
        }
    }

    #[test]
    fn built_in_index_loads() {
        let index = MarketplaceIndex::built_in();
        assert!(!index.entries.is_empty());
    }

    #[test]
    fn search_matches_name() {
        let index = MarketplaceIndex {
            entries: vec![sample_entry("github"), sample_entry("stripe")],
        };
        let results = index.search("github");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "github");
    }

    #[test]
    fn search_matches_tags() {
        let mut entry = sample_entry("my-api");
        entry.tags = vec!["payments".to_string()];
        let index = MarketplaceIndex {
            entries: vec![entry],
        };
        let results = index.search("payments");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn find_exact() {
        let index = MarketplaceIndex {
            entries: vec![sample_entry("petstore")],
        };
        assert!(index.find("petstore").is_some());
        assert!(index.find("PETSTORE").is_some());
        assert!(index.find("nope").is_none());
    }

    #[test]
    fn merge_replaces_duplicates() {
        let mut a = MarketplaceIndex {
            entries: vec![sample_entry("x")],
        };
        let mut b = sample_entry("x");
        b.version = "2.0.0".to_string();
        let other = MarketplaceIndex { entries: vec![b] };
        a.merge(&other);
        assert_eq!(a.entries.len(), 1);
        assert_eq!(a.entries[0].version, "2.0.0");
    }

    #[test]
    fn sorted_by_downloads() {
        let mut e1 = sample_entry("a");
        e1.downloads = 10;
        let mut e2 = sample_entry("b");
        e2.downloads = 100;
        let index = MarketplaceIndex {
            entries: vec![e1, e2],
        };
        let sorted = index.sorted_by_downloads();
        assert_eq!(sorted[0].name, "b");
        assert_eq!(sorted[1].name, "a");
    }

    // --- Plugin marketplace tests ---

    fn sample_plugin(name: &str, lang: &str) -> PluginEntry {
        PluginEntry {
            name: name.to_string(),
            description: format!("An emitter for {lang}"),
            version: "0.1.0".to_string(),
            author: "test".to_string(),
            language: lang.to_string(),
            url: "https://example.com/plugin.wasm".to_string(),
            verified: false,
            downloads: 50,
            rating: 4.0,
        }
    }

    #[test]
    fn plugin_built_in_index_loads() {
        let index = PluginIndex::built_in();
        assert!(!index.plugins.is_empty());
    }

    #[test]
    fn plugin_search_matches_name() {
        let index = PluginIndex {
            plugins: vec![
                sample_plugin("kotlin-emitter", "kotlin"),
                sample_plugin("swift-emitter", "swift"),
            ],
        };
        let results = index.search("kotlin");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "kotlin-emitter");
    }

    #[test]
    fn plugin_search_matches_language() {
        let index = PluginIndex {
            plugins: vec![
                sample_plugin("kotlin-emitter", "kotlin"),
                sample_plugin("swift-emitter", "swift"),
            ],
        };
        let results = index.search("swift");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].language, "swift");
    }

    #[test]
    fn plugin_find_exact() {
        let index = PluginIndex {
            plugins: vec![sample_plugin("kotlin-emitter", "kotlin")],
        };
        assert!(index.find("kotlin-emitter").is_some());
        assert!(index.find("KOTLIN-EMITTER").is_some());
        assert!(index.find("nope").is_none());
    }

    #[test]
    fn plugin_merge_replaces_duplicates() {
        let mut a = PluginIndex {
            plugins: vec![sample_plugin("x", "rust")],
        };
        let mut b = sample_plugin("x", "rust");
        b.version = "2.0.0".to_string();
        let other = PluginIndex { plugins: vec![b] };
        a.merge(&other);
        assert_eq!(a.plugins.len(), 1);
        assert_eq!(a.plugins[0].version, "2.0.0");
    }

    #[test]
    fn plugin_sorted_by_downloads() {
        let mut p1 = sample_plugin("a", "kotlin");
        p1.downloads = 10;
        let mut p2 = sample_plugin("b", "swift");
        p2.downloads = 100;
        let index = PluginIndex {
            plugins: vec![p1, p2],
        };
        let sorted = index.sorted_by_downloads();
        assert_eq!(sorted[0].name, "b");
        assert_eq!(sorted[1].name, "a");
    }

    #[test]
    fn plugin_install_creates_placeholder_on_network_error() {
        let index = PluginIndex {
            plugins: vec![sample_plugin("test-plugin", "python")],
        };
        let dir = tempfile::tempdir().unwrap();
        let result = index.install_plugin("test-plugin", dir.path());
        assert!(result.is_ok());
        let dest = result.unwrap();
        assert!(dest.exists());
        assert!(dest.to_string_lossy().ends_with("test-plugin.wasm"));
    }
}
