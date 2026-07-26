//! Emit package scaffolding: package.json (dual ESM/CJS exports), tsconfig,
//! tsup config, and a README. These make the generated SDK publishable and
//! tree-shakeable out of the box.

use std::path::Path;

use specforge_core::Document;

use crate::util::path_str;
use crate::GeneratorOptions;

/// Emit all package-level files at the output root.
pub fn emit(
    doc: &Document,
    opts: &GeneratorOptions,
    out_dir: &Path,
) -> std::io::Result<Vec<String>> {
    let mut written = Vec::new();

    let pkg = out_dir.join("package.json");
    std::fs::write(&pkg, package_json(doc, opts))?;
    written.push(path_str(&pkg, out_dir));

    let tsconfig = out_dir.join("tsconfig.json");
    std::fs::write(&tsconfig, tsconfig_json())?;
    written.push(path_str(&tsconfig, out_dir));

    let tsup = out_dir.join("tsup.config.ts");
    std::fs::write(&tsup, tsup_config())?;
    written.push(path_str(&tsup, out_dir));

    let readme = out_dir.join("README.md");
    std::fs::write(&readme, readme_md(doc, opts))?;
    written.push(path_str(&readme, out_dir));

    let gitignore = out_dir.join(".gitignore");
    std::fs::write(&gitignore, "dist/\nnode_modules/\n*.tsbuildinfo\n")?;
    written.push(path_str(&gitignore, out_dir));

    Ok(written)
}

fn slug(s: &str) -> String {
    s.to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

fn package_name(doc: &Document, opts: &GeneratorOptions) -> String {
    opts.package_name
        .clone()
        .unwrap_or_else(|| format!("@{}sdk", slug(&doc.title)))
}

fn package_json(doc: &Document, opts: &GeneratorOptions) -> String {
    let name = package_name(doc, opts);
    format!(
        r#"{{
  "name": "{name}",
  "version": "{version}",
  "description": "Generated TypeScript SDK for {title}.",
  "type": "module",
  "license": "MIT",
  "sideEffects": false,
  "main": "./dist/index.cjs",
  "module": "./dist/index.js",
  "types": "./dist/index.d.ts",
  "exports": {{
    ".": {{
      "types": "./dist/index.d.ts",
      "import": "./dist/index.js",
      "require": "./dist/index.cjs"
    }},
    "./package.json": "./package.json"
  }},
  "files": ["dist", "src"],
  "engines": {{ "node": ">=18" }},
  "scripts": {{
    "build": "tsup",
    "typecheck": "tsc --noEmit"
  }},
  "devDependencies": {{
    "tsup": "^8.3.5",
    "typescript": "^5.6.0"
  }}
}}
"#,
        name = name,
        version = doc.version,
        title = doc.title.replace('"', "\\\""),
    )
}

fn tsconfig_json() -> String {
    r#"{
  "compilerOptions": {
    "target": "ES2022",
    "lib": ["ES2022", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "moduleResolution": "bundler",
    "strict": true,
    "noUncheckedIndexedAccess": true,
    "exactOptionalPropertyTypes": false,
    "noImplicitOverride": true,
    "noFallthroughCasesInSwitch": true,
    "esModuleInterop": true,
    "isolatedModules": true,
    "skipLibCheck": true,
    "declaration": true,
    "declarationMap": true,
    "sourceMap": true,
    "outDir": "dist",
    "rootDir": "src"
  },
  "include": ["src/**/*.ts"],
  "exclude": ["node_modules", "dist"]
}
"#
    .to_string()
}

fn tsup_config() -> String {
    r#"import { defineConfig } from "tsup";

// Dual ESM/CJS output with type declarations. ESM is the primary format for
// tree-shaking; CJS keeps older consumers working.
export default defineConfig({
  entry: ["src/index.ts"],
  format: ["esm", "cjs"],
  dts: true,
  sourcemap: true,
  clean: true,
  treeshake: true,
});
"#
    .to_string()
}

fn readme_md(doc: &Document, opts: &GeneratorOptions) -> String {
    let name = package_name(doc, opts);
    let base = doc.base_url.as_deref().unwrap_or("<base-url>");

    // Pick the first GET operation for examples, falling back to generic names.
    let get_op = doc.operations.iter().find(|op| op.method == specforge_core::HttpMethod::Get);
    let (example_tag, example_call, example_list_call) = if let Some(op) = get_op {
        let tag = op.tag.as_deref().unwrap_or("default");
        let tag_ident = tag.to_ascii_lowercase();
        let fn_name = &op.operation_id;
        // Build a plausible call with required path params.
        let mut call_args = String::new();
        for p in &op.parameters {
            if p.location == specforge_core::ParamLocation::Path {
                call_args.push_str(&format!(" {}: \"...\"", p.name));
            }
        }
        if !call_args.is_empty() {
            call_args = format!("{{ {} }}", call_args.trim_start());
        }
        let is_list = fn_name.starts_with("list") || fn_name.starts_with("get") && op.responses.iter().any(|r| r.body.is_some());
        if is_list {
            (tag_ident.clone(), format!("client.{tag_ident}.{fn_name}({call_args})"), format!("client.{tag_ident}.{fn_name}()"))
        } else {
            (tag_ident.clone(), format!("client.{tag_ident}.{fn_name}({call_args})"), "client.pets.listPets()".into())
        }
    } else {
        ("pets".into(), "client.pets.getPet({ petId: \"abc\" })".into(), "client.pets.listPets()".into())
    };

    let list_fn = if let Some(op) = get_op {
        let tag = op.tag.as_deref().unwrap_or("default").to_ascii_lowercase();
        format!("client.{tag}.{}", op.operation_id)
    } else {
        "client.pets.listPets".into()
    };

    format!(
        r#"# {title} SDK

Generated TypeScript SDK for **{title}** (`{name}`). Targets `{base}`.

## Install

```bash
npm install {name}
```

## Quick start

```ts
import {{ createClient, bearerAuth }} from "{name}";

const client = createClient({{ baseUrl: "{base}", auth: bearerAuth(() => process.env.API_TOKEN!) }});

// Each tag is a namespaced group of operations.
const result = await {example_call};
```

## Errors

All non-2xx responses (and network/timeout failures) throw a typed `ApiError`.
Narrow on `.type`:

```ts
import {{ isApiError }} from "{name}";
try {{
  await {example_list_call};
}} catch (e) {{
  if (isApiError(e)) {{
    if (e.type === "http") console.error(e.status, e.body);
  }}
}}
```

## Pagination

```ts
import {{ cursorPaginator }} from "{name}";
for await (const page of cursorPaginator((cursor) => {list_fn}({{ cursor }}))) {{
  for (const item of page.items) console.log(item);
}}
```

## Concurrency

Limit in-flight requests with `maxConcurrent`:

```ts
const client = createClient({{
  baseUrl: "{base}",
  auth: bearerAuth(() => process.env.API_TOKEN!),
  maxConcurrent: 10,
}});
```

## Dedupe

Coalesce identical in-flight safe (GET/HEAD/OPTIONS) requests so concurrent callers share one round-trip:

```ts
const client = createClient({{
  baseUrl: "{base}",
  auth: bearerAuth(() => process.env.API_TOKEN!),
  dedupe: true,
}});
```

## Middleware

Add request/response middleware using `client.use`:

```ts
client.use(async (req, next) => {{
  console.log(`${{req.method}} ${{req.url}}`);
  const res = await next(req);
  console.log(`-> ${{res.status}}`);
  return res;
}});
```

## Streaming / SSE

Consume server-sent events with `streamSse`:

```ts
import {{ streamSse }} from "{name}";

const res = await client.{example_tag}.streamEvents();
for await (const event of streamSse(res)) {{
  console.log(event.event, event.data);
}}
```

## Idempotency

Auto-attach `Idempotency-Key` headers on unsafe methods (POST/PUT/PATCH/DELETE) for safe retries:

```ts
const client = createClient({{
  baseUrl: "{base}",
  auth: bearerAuth(() => process.env.API_TOKEN!),
  idempotency: true,
}});
```

---

_Generated by `specforge`. Do not edit generated files directly._
"#,
        title = doc.title,
        name = name,
        base = base,
        example_tag = example_tag,
        example_call = example_call,
        example_list_call = example_list_call,
        list_fn = list_fn,
    )
}
