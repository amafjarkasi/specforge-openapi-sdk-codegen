//! Spec Marketplace — community-shared OpenAPI spec discovery and management.

use anyhow::Context as _;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

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
        serde_json::from_str(&text)
            .with_context(|| format!("failed to parse {}", path.display()))
    }

    /// Return the built-in curated marketplace index bundled with this binary.
    pub fn built_in() -> Self {
        let json = include_str!("../../../assets/marketplace-index.json");
        serde_json::from_str(json).expect("bundled marketplace-index.json is valid JSON")
    }

    /// Merge a local/remote index into this one (entries with the same name are replaced).
    pub fn merge(&mut self, other: &MarketplaceIndex) {
        let mut map: HashMap<&str, &SpecEntry> = self
            .entries
            .iter()
            .map(|e| (e.name.as_str(), e))
            .collect();
        for entry in &other.entries {
            map.insert(&entry.name, entry);
        }
        self.entries = map.into_values().cloned().collect();
        self.entries.sort_by(|a, b| b.downloads.cmp(&a.downloads));
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
        v.sort_by(|a, b| b.downloads.cmp(&a.downloads));
        v
    }

    /// Generate metadata for a local spec file by parsing it.
    pub fn generate_metadata(spec_path: &Path) -> anyhow::Result<SpecEntry> {
        use crate::spec;

        let parsed = spec::parse_file(spec_path)
            .context("failed to parse spec")?;

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
        let mut index = MarketplaceIndex {
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
        let other = MarketplaceIndex {
            entries: vec![b],
        };
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
}
