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
    /// Optional i18n configuration for localized error messages.
    /// When `Some`, generates `src/i18n.ts` with per-locale translation maps.
    pub i18n: Option<specforge_core::I18nConfig>,
}

/// Generate the full SDK into `opts.out_dir`. Returns the list of files written
/// (relative paths), in deterministic order. Files are written in parallel using rayon.
pub fn generate(doc: &Document, opts: &GeneratorOptions) -> std::io::Result<Vec<String>> {
    // IR version compatibility check.
    if doc.ir_version != specforge_core::IR_VERSION {
        eprintln!(
            "Warning: IR version {} may not be fully supported by this emitter (expected {}).",
            doc.ir_version,
            specforge_core::IR_VERSION
        );
    }

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
        let name = name::safe_model_name(&name::pascal(model.name()));
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

    // i18n — localized error messages (only when locales are provided).
    if let Some(ref i18n) = opts.i18n {
        files.extend(collect_i18n(i18n, &opts.out_dir));
    }

    // Barrel index (root exports client + models only; API tags are tree-shakeable).
    files.extend(collect_index(doc, &opts.out_dir, opts.i18n.is_some())?);

    // API convenience barrel (re-exports all tags for callers who want everything).
    files.extend(collect_api_index(doc, &opts.out_dir)?);

    // specforge-version.json — version metadata for the generated SDK.
    files.push(collect_version_file(doc, &opts.out_dir));

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

/// Collect `src/i18n.ts` — localized error message translations for each locale.
fn collect_i18n(
    i18n: &specforge_core::I18nConfig,
    out_dir: &Path,
) -> Vec<(String, PathBuf, String)> {
    let src = out_dir.join("src");
    let mut body = String::from("/* eslint-disable */\n// Generated i18n translations. DO NOT EDIT.\n\n");

    // Group translations into a nested object per locale: { errors: { key: "value" } }
    for (locale, translations) in &i18n.translations {
        body.push_str(&format!("export const {locale} = {{\n  errors: {{\n"));
        // Sort keys for deterministic output.
        let mut sorted: Vec<(&String, &String)> = translations.iter().collect();
        sorted.sort_by_key(|(k, _)| k.to_string());
        for (key, value) in &sorted {
            // Strip the "errors." prefix for the nested object key.
            let short_key = key.strip_prefix("errors.").unwrap_or(key);
            let escaped_value = value
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\n', "\\n");
            body.push_str(&format!("    {}: \"{}\",\n", short_key, escaped_value));
        }
        body.push_str("  },\n};\n\n");
    }

    // Generate the locale map and helper function.
    body.push_str("/** Available locale codes. */\n");
    let locale_keys: Vec<&str> = i18n.translations.keys().map(|s| s.as_str()).collect();
    body.push_str(&format!(
        "export type Locale = {};\n\n",
        locale_keys
            .iter()
            .map(|l| format!("\"{}\"", l))
            .collect::<Vec<_>>()
            .join(" | ")
    ));

    body.push_str("/** All translation maps keyed by locale code. */\n");
    body.push_str("export const locales: Record<Locale, typeof en> = {\n");
    for locale in &locale_keys {
        body.push_str(&format!("  {},\n", locale));
    }
    body.push_str("};\n\n");

    body.push_str(&format!(
        "/** Default locale code. */\nexport const defaultLocale: Locale = \"{}\";\n\n",
        i18n.default_locale
    ));

    // The translate helper.
    body.push_str(
        r#"/**
 * Look up a translated error message. Falls back to the key itself if the
 * locale or key is missing.
 *
 * @param key    Dot-separated translation key (e.g. "errors.notFound")
 * @param locale Locale code (e.g. "en", "es")
 * @param params Optional interpolation parameters (e.g. { status: 404 })
 */
export function t(
  key: string,
  locale: Locale = defaultLocale,
  params?: Record<string, string | number>,
): string {
  const map = locales[locale] ?? locales[defaultLocale];
  const raw = key in map ? map[key as keyof typeof map] : key;
  if (!params) return raw;
  return Object.entries(params).reduce(
    (s, [k, v]) => s.replace(new RegExp(`\\{${k}\\}`, "g"), String(v)),
    raw,
  );
}
"#,
    );

    let path = src.join("i18n.ts");
    let rel = path_str(&path, out_dir);
    vec![(rel, path, body)]
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
fn collect_index(doc: &Document, out_dir: &Path, has_i18n: bool) -> std::io::Result<Vec<(String, PathBuf, String)>> {
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
    body.push_str("export * from \"./interceptors\";\n");
    body.push_str("export * from \"./errors\";\n");
    body.push_str("export * from \"./auth\";\n");
    body.push_str("export * from \"./retry\";\n");
    body.push_str("export * from \"./paginate\";\n");
    body.push_str("export * from \"./concurrency\";\n");
    body.push_str("export * from \"./dedup\";\n");
    body.push_str("export * from \"./idempotency\";\n");
    body.push_str("export * from \"./middleware\";\n");
    body.push_str("export * from \"./streaming\";\n");
    body.push_str("export * from \"./cache\";\n");
    body.push_str("export * from \"./validate\";\n");
    body.push_str("export * from \"./validation-middleware\";\n");
    body.push_str("export * from \"./ratelimit\";\n");
    body.push_str("export * from \"./telemetry\";\n");
    body.push_str("export * from \"./logging\";\n");
    body.push_str("export * from \"./service_container\";\n\n");

    // Models.
    for (_, model) in doc.schemas.iter() {
        let name = name::safe_model_name(&name::pascal(model.name()));
        body.push_str(&format!("export * from \"./models/{name}\";\n"));
    }
    // Webhooks.
    if !doc.webhooks.is_empty() {
        body.push_str("export * from \"./webhooks\";\n");
    }
    // i18n — localized error messages.
    if has_i18n {
        body.push_str("export * from \"./i18n\";\n");
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

/// Collect `specforge-version.json` — version metadata for the generated SDK.
fn collect_version_file(doc: &Document, out_dir: &Path) -> (String, PathBuf, String) {
    let content = format!(
        r#"{{"specforge_version":"{}","ir_version":"{}","spec_version":"{}","generated_at":"{}"}}"#,
        env!("CARGO_PKG_VERSION"),
        doc.ir_version,
        doc.version,
        chrono_free_timestamp(),
    );
    let path = out_dir.join("specforge-version.json");
    let rel = path_str(&path, out_dir);
    (rel, path, content)
}

/// Generate an ISO 8601 timestamp without pulling in chrono.
fn chrono_free_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Simple UTC date conversion (no leap seconds needed for approx timestamp).
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Civil date from days since epoch.
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
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y, m, d, hours, minutes, seconds
    )
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0)
}
