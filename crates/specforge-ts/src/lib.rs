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
use specforge_core::{Document, Webhook};

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

    // Webhooks — handler types (only if webhooks are present).
    if !doc.webhooks.is_empty() {
        files.extend(collect_webhooks(&doc.webhooks, &opts.out_dir)?);
    }

    // Barrel index (root exports client + models only; API tags are tree-shakeable).
    files.extend(collect_index(doc, &opts.out_dir)?);

    // API convenience barrel (re-exports all tags for callers who want everything).
    files.extend(collect_api_index(doc, &opts.out_dir)?);

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

/// Collect `src/webhooks.ts` content for webhook handler types.
fn collect_webhooks(
    webhooks: &[Webhook],
    out_dir: &Path,
) -> std::io::Result<Vec<(String, PathBuf, String)>> {
    let src = out_dir.join("src");
    let mut body = String::from("/* eslint-disable */\n// Generated webhook types. DO NOT EDIT.\n\n");

    // Payload interfaces for each webhook.
    for wh in webhooks {
        let payload_name = format!("{}WebhookPayload", name::pascal(&wh.name));
        if let Some(d) = &wh.description {
            body.push_str(&format!("/**\n * {}\n */\n", d));
        } else if let Some(s) = &wh.summary {
            body.push_str(&format!("/**\n * {}\n */\n", s));
        }
        if let Some(rb) = &wh.request_body {
            let ts_type = types::render(&rb.ty);
            body.push_str(&format!("export type {payload_name} = {ts_type};\n\n"));
        } else {
            body.push_str(&format!("export type {payload_name} = unknown;\n\n"));
        }
    }

    // Generic WebhookHandler type.
    body.push_str("/**\n * A webhook handler receives a typed payload and returns void (or a Promise<void>).\n */\n");
    body.push_str("export type WebhookHandler<T> = (payload: T) => Promise<void> | void;\n\n");

    // Factory function for each webhook.
    for wh in webhooks {
        let payload_name = format!("{}WebhookPayload", name::pascal(&wh.name));
        let factory_name = name::camel(&format!("create_{}_webhook_handler", wh.name));
        body.push_str(&format!(
            "/**\n * Create a typed handler for the `{}` webhook.\n */\n",
            wh.name
        ));
        body.push_str(&format!(
            "export function {factory_name}(handler: WebhookHandler<{payload_name}>): WebhookHandler<{payload_name}> {{\n  return handler;\n}}\n\n"
        ));
    }

    let path = src.join("webhooks.ts");
    let rel = path_str(&path, out_dir);
    Ok(vec![(rel, path, body)])
}

/// Collect `src/index.ts` content for parallel writing.
///
/// The root barrel exports only the client core, runtime helpers, and models.
/// API tag classes are **not** re-exported from the root so that bundlers can
/// tree-shake unused tags. Consumers who need a specific tag import directly:
///
/// ```ts
/// import { PetsApi } from "./api/Pets";
/// import type { ListPetsParams } from "./api/Pets";
/// ```
///
/// A convenience barrel at `src/api/index.ts` re-exports every tag for callers
/// who prefer a single import.
fn collect_index(doc: &Document, out_dir: &Path) -> std::io::Result<Vec<(String, PathBuf, String)>> {
    let src = out_dir.join("src");
    let mut body = String::from("/* eslint-disable */\n// Generated barrel. DO NOT EDIT.\n\n");

    // Tree-shaking comment for consumers.
    body.push_str("// Tree-shaking: this file exports only the client core, runtime helpers,\n");
    body.push_str("// and models. API tag classes are intentionally excluded so bundlers can\n");
    body.push_str("// eliminate unused tags. Import individual tag modules directly:\n");
    body.push_str("//\n");
    body.push_str("//   import { PetsApi } from \"./api/Pets\";\n");
    body.push_str("//   import type { ListPetsParams } from \"./api/Pets\";\n");
    body.push_str("//\n");
    body.push_str("// Or use the convenience barrel that re-exports all tags:\n");
    body.push_str("//\n");
    body.push_str("//   import { PetsApi } from \"./api\";\n");
    body.push_str("//\n");
    body.push_str("// The createClient() factory below pulls all tags into a single object.\n");
    body.push_str("// Prefer direct tag imports when bundle size matters.\n\n");

    // Runtime exports.
    body.push_str("export * from \"./client\";\n");
    body.push_str("export * from \"./errors\";\n");
    body.push_str("export * from \"./auth\";\n");
    body.push_str("export * from \"./retry\";\n");
    body.push_str("export * from \"./paginate\";\n");
    body.push_str("export * from \"./validate\";\n");
    body.push_str("export * from \"./ratelimit\";\n");
    body.push_str("export * from \"./telemetry\";\n\n");

    // Models.
    for (_, model) in doc.schemas.iter() {
        let name = name::pascal(model.name());
        body.push_str(&format!("export * from \"./models/{name}\";\n"));
    }
    // Webhooks.
    if !doc.webhooks.is_empty() {
        body.push_str("export * from \"./webhooks\";\n");
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

    // Import each tag API class for the factory below (internal use only — not re-exported).
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
        "/**\n * Construct the SDK client. Each tag becomes a namespaced property.\n *\n * NOTE: This factory imports all tag modules. For tree-shakeable code,\n * import individual tag modules directly instead of using createClient.\n */\nexport function createClient(options: ApiClientOptions = {{}}): SdkClient {{\n  const client = new ApiClient(options);\n{constructor}\n}}\n"
    ));

    let path = src.join("index.ts");
    let rel = path_str(&path, out_dir);
    Ok(vec![(rel, path, body)])
}

/// Collect `src/api/index.ts` — a convenience barrel that re-exports all tag
/// API classes and their params interfaces. Callers who want everything can
/// import from here; callers who need tree-shaking import individual tags.
fn collect_api_index(doc: &Document, out_dir: &Path) -> std::io::Result<Vec<(String, PathBuf, String)>> {
    let api_dir = out_dir.join("src").join("api");

    let mut tags: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for op in &doc.operations {
        if let Some(t) = &op.tag {
            tags.insert(name::pascal(t));
        }
    }

    let mut body = String::from("/* eslint-disable */\n// Generated barrel. DO NOT EDIT.\n\n");
    body.push_str("// Convenience barrel: re-exports all tag API modules.\n");
    body.push_str("// For tree-shakeable imports, import individual tag files instead:\n");
    body.push_str("//\n");
    body.push_str("//   import { PetsApi } from \"./Pets\";\n");
    body.push_str("//   import type { ListPetsParams } from \"./Pets\";\n\n");

    for tag in &tags {
        body.push_str(&format!("export * from \"./{tag}\";\n"));
    }

    let path = api_dir.join("index.ts");
    let rel = path_str(&path, out_dir);
    Ok(vec![(rel, path, body)])
}
