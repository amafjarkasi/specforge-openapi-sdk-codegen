//! Static HTML documentation emitter.
//!
//! Generates a self-contained documentation site from the IR, including
//! an `index.html` page and a `style.css` stylesheet.

use crate::ir::{Document, Model};

/// Options for the documentation generator.
pub struct DocsOptions {
    pub out_dir: std::path::PathBuf,
}

/// Generate a static HTML documentation site from the resolved IR.
///
/// Returns the list of files written (relative names like `"index.html"`).
pub fn generate_docs(doc: &Document, opts: &DocsOptions) -> std::io::Result<Vec<String>> {
    let mut written = Vec::new();
    std::fs::create_dir_all(&opts.out_dir)?;

    // Generate index.html
    let index = render_index(doc);
    let path = opts.out_dir.join("index.html");
    std::fs::write(&path, &index)?;
    written.push("index.html".to_string());

    // Generate CSS
    let css = render_css();
    let path = opts.out_dir.join("style.css");
    std::fs::write(&path, &css)?;
    written.push("style.css".to_string());

    Ok(written)
}

fn render_index(doc: &Document) -> String {
    let title = &doc.title;
    let version = &doc.version;
    let base_url = doc.base_url.as_deref().unwrap_or("");

    let mut operations_html = String::new();
    for op in &doc.operations {
        let method = op.method.upper();
        let path = &op.path;
        let id = &op.operation_id;
        let summary = op.summary.as_deref().unwrap_or("");
        let tag = op.tag.as_deref().unwrap_or("Other");

        operations_html.push_str(&format!(
            r#"<div class="endpoint" data-tag="{tag}">
                <div class="endpoint-header">
                    <span class="method method-{method_lower}">{method}</span>
                    <span class="path">{path}</span>
                    <span class="op-id">{id}</span>
                </div>
                <div class="endpoint-summary">{summary}</div>
            </div>"#,
            tag = tag,
            method_lower = method.to_lowercase(),
            method = method,
            path = path,
            id = id,
            summary = summary,
        ));
    }

    let mut schemas_html = String::new();
    for (name, model) in doc.schemas.iter() {
        let kind = match model {
            Model::Object(_) => "object",
            Model::Enum(_) => "enum",
        };
        schemas_html.push_str(&format!(
            r#"<div class="schema">
                <div class="schema-header">
                    <span class="schema-name">{name}</span>
                    <span class="schema-kind">{kind}</span>
                </div>
            </div>"#,
            name = name,
            kind = kind,
        ));
    }

    let base_url_html = if base_url.is_empty() {
        String::new()
    } else {
        format!(r#"<span class="base-url">{}</span>"#, base_url)
    };

    format!(
        r##"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{title} — API Documentation</title>
    <link rel="stylesheet" href="style.css">
</head>
<body>
    <header>
        <h1>🔥 {title}</h1>
        <span class="version">v{version}</span>
        {base_url_html}
    </header>
    <nav>
        <a href="#operations">Operations</a>
        <a href="#schemas">Schemas</a>
    </nav>
    <main>
        <section id="operations">
            <h2>Operations</h2>
            {operations_html}
        </section>
        <section id="schemas">
            <h2>Schemas</h2>
            {schemas_html}
        </section>
    </main>
</body>
</html>"##,
        title = title,
        version = version,
        base_url_html = base_url_html,
        operations_html = operations_html,
        schemas_html = schemas_html,
    )
}

fn render_css() -> String {
    r#"
:root {
    --bg: #1a0f0a;
    --surface: #1c1008;
    --border: #92400e;
    --text: #fef3c7;
    --muted: #d97706;
    --accent: #f97316;
    --green: #22c55e;
    --red: #ef4444;
    --blue: #38bdf8;
}
* { margin: 0; padding: 0; box-sizing: border-box; }
body {
    font-family: ui-sans-serif, system-ui, -apple-system, sans-serif;
    background: var(--bg);
    color: var(--text);
    line-height: 1.6;
}
header {
    padding: 2rem;
    border-bottom: 1px solid var(--border);
    display: flex;
    align-items: center;
    gap: 1rem;
}
header h1 { font-size: 1.8rem; }
.version { color: var(--muted); font-size: 0.9rem; }
.base-url { color: var(--muted); font-size: 0.85rem; margin-left: auto; }
nav {
    padding: 1rem 2rem;
    border-bottom: 1px solid var(--border);
    display: flex;
    gap: 2rem;
}
nav a { color: var(--accent); text-decoration: none; font-weight: 500; }
nav a:hover { text-decoration: underline; }
main { padding: 2rem; max-width: 1200px; }
h2 { margin-bottom: 1.5rem; color: var(--accent); }
.endpoint {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 1rem;
    margin-bottom: 0.75rem;
}
.endpoint-header { display: flex; align-items: center; gap: 1rem; }
.method {
    font-weight: 700;
    font-size: 0.8rem;
    padding: 0.2rem 0.6rem;
    border-radius: 4px;
    text-transform: uppercase;
}
.method-get { background: #166534; color: #86efac; }
.method-post { background: #1e40af; color: #93c5fd; }
.method-put { background: #92400e; color: #fdba74; }
.method-patch { background: #7c2d12; color: #fed7aa; }
.method-delete { background: #991b1b; color: #fca5a5; }
.path { font-family: monospace; font-weight: 500; }
.op-id { color: var(--muted); font-size: 0.85rem; margin-left: auto; }
.endpoint-summary { color: var(--muted); margin-top: 0.5rem; font-size: 0.9rem; }
.schema {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 1rem;
    margin-bottom: 0.75rem;
}
.schema-header { display: flex; align-items: center; gap: 1rem; }
.schema-name { font-weight: 600; }
.schema-kind { color: var(--muted); font-size: 0.8rem; }
"#
    .to_string()
}
