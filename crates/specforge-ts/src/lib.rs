//! `specforge-ts` — TypeScript SDK emitter for the `specforge-core` IR.
//!
//! The emitter is split into focused modules:
//! - [`name`] — identifier sanitization (pascal/camel/property keys)
//! - [`types`] — IR [`Type`](specforge_core::Type) → TS type expression
//! - [`models`] — one file per IR [`Model`](specforge_core::Model)
//! - [`runtime`] — static runtime files (client, errors, auth, retry, paginate)
//! - [`operations`] — one file per tag, one method per operation
//! - [`package`] — package.json / tsconfig.json / tsup.config.ts scaffolding
//!
//! The top-level [`generate`] function orchestrates emission to a directory.

pub mod models;
pub mod name;
pub mod operations;
pub mod package;
pub mod runtime;
pub mod types;
pub mod util;

use std::path::{Path, PathBuf};

use rayon::prelude::*;
use specforge_core::Document;

use crate::util::path_str;

/// Everything needed to drive emission from the CLI or tests.
pub struct GeneratorOptions {
    /// Output directory; created if missing. Existing files are overwritten.
    pub out_dir: PathBuf,
    /// Package name written into package.json. Defaults to a derived slug.
    pub package_name: Option<String>,
}

/// Generate the full SDK into `opts.out_dir`. Returns the list of files written
/// (relative paths), in deterministic order. Files are written in parallel using rayon.
pub fn generate(doc: &Document, opts: &GeneratorOptions) -> std::io::Result<Vec<String>> {
    // Ensure the output root exists before any sub-emitter writes into it.
    std::fs::create_dir_all(&opts.out_dir)?;

    // Collect all (relative_path, absolute_path, content) triples.
    let mut files: Vec<(String, PathBuf, String)> = Vec::new();

    // Scaffolding (package.json, tsconfig, tsup, README).
    files.extend(package::collect(doc, opts, &opts.out_dir)?);

    // Runtime (client, errors, auth, retry, paginate) — static files.
    files.extend(runtime::collect(doc, &opts.out_dir)?);

    // Models — one file per schema. Pass the registry so oneOf unions can emit
    // runtime type-guard helpers that inspect sibling model shapes.
    let models_dir = opts.out_dir.join("src").join("models");
    std::fs::create_dir_all(&models_dir)?;
    for (_, model) in doc.schemas.iter() {
        let name = name::pascal(model.name());
        let path = models_dir.join(format!("{name}.ts"));
        let rel = path_str(&path, &opts.out_dir);
        let content = models::emit_model_file_with_registry(model, Some(&doc.schemas));
        files.push((rel, path, content));
    }

    // Operations — one file per tag.
    files.extend(operations::collect(doc, &opts.out_dir)?);

    // Barrel index.
    files.extend(collect_index(doc, &opts.out_dir)?);

    // Write all files in parallel.
    let written: Vec<String> = files
        .par_iter()
        .map(|(rel, abs, content)| {
            if let Some(parent) = abs.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(abs, content);
            rel.clone()
        })
        .collect();

    let mut written = written;
    written.sort();
    Ok(written)
}

/// Collect `src/index.ts` content for parallel writing.
fn collect_index(doc: &Document, out_dir: &Path) -> std::io::Result<Vec<(String, PathBuf, String)>> {
    let src = out_dir.join("src");
    let mut body = String::from("/* eslint-disable */\n// Generated barrel. DO NOT EDIT.\n\n");

    // Runtime exports.
    body.push_str("export * from \"./client\";\n");
    body.push_str("export * from \"./errors\";\n");
    body.push_str("export * from \"./auth\";\n");
    body.push_str("export * from \"./retry\";\n");
    body.push_str("export * from \"./paginate\";\n\n");

    // Models.
    for (_, model) in doc.schemas.iter() {
        let name = name::pascal(model.name());
        body.push_str(&format!("export * from \"./models/{name}\";\n"));
    }
    body.push('\n');

    // Operations grouped by tag file.
    let mut tags: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for op in &doc.operations {
        if let Some(t) = &op.tag {
            tags.insert(name::pascal(t));
        }
    }
    let tag_list: Vec<String> = tags.iter().cloned().collect();

    // Import each tag API class for the factory below.
    for tag in &tag_list {
        body.push_str(&format!(
            "import {{ {}Api }} from \"./api/{tag}\";\n",
            tag
        ));
    }
    if !tag_list.is_empty() {
        body.push('\n');
    }
    // Import ApiClient + its options type for the factory.
    body.push_str("import { ApiClient } from \"./client\";\n");
    body.push_str("import type { ApiClientOptions } from \"./client\";\n\n");
    // Re-export each tag's class and its params interfaces.
    for tag in &tag_list {
        body.push_str(&format!("export {{ {}Api }} from \"./api/{tag}\";\n", tag));
    }
    body.push('\n');

    // The typed top-level client: one property per tag.
    let client_type = if tag_list.is_empty() {
        "export interface SdkClient {}\n\n".to_string()
    } else {
        let fields: Vec<String> = tag_list
            .iter()
            .map(|t| format!("  {}: {}Api;", name::camel(t), t))
            .collect();
        format!("export interface SdkClient {{\n{}\n}}\n\n", fields.join("\n"))
    };
    body.push_str(&client_type);

    // Factory.
    let constructor = if tag_list.is_empty() {
        "  return {};".to_string()
    } else {
        let inits: Vec<String> = tag_list
            .iter()
            .map(|t| format!("    {}: new {}Api(client),", name::camel(t), t))
            .collect();
        format!("  return {{\n{}\n  }};", inits.join("\n"))
    };
    body.push_str(&format!(
        "/**\n * Construct the SDK client. Each tag becomes a namespaced property.\n */\nexport function createClient(options: ApiClientOptions = {{}}): SdkClient {{\n  const client = new ApiClient(options);\n{constructor}\n}}\n"
    ));

    let path = src.join("index.ts");
    let rel = path_str(&path, out_dir);
    Ok(vec![(rel, path, body)])
}
