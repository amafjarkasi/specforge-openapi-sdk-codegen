//! Emit `src/api/<Tag>.ts` files — one method per operation, grouped by tag.
//! Each method builds a request through the shared `ApiClient`, narrows the
//! response to the declared success-body type, and throws typed `ApiError`s.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use specforge_core::{Composition, Document, Operation, ParamLocation, Type};

use crate::name::{camel, pascal};
use crate::types::render;
use crate::util::{file_header, path_str};

/// Emit all per-tag operation files plus a barrel. Returns relative paths.
pub fn emit(doc: &Document, out_dir: &Path) -> std::io::Result<Vec<String>> {
    let items = collect(doc, out_dir)?;
    let mut written = Vec::new();
    for (rel, abs, content) in &items {
        if let Some(parent) = abs.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(abs, content)?;
        written.push(rel.clone());
    }
    Ok(written)
}

/// Collect all per-tag operation files (relative path, absolute path, content) for parallel writing.
pub fn collect(doc: &Document, out_dir: &Path) -> std::io::Result<Vec<(String, PathBuf, String)>> {
    let api_dir = out_dir.join("src").join("api");

    // Group operations by tag (default "Default" when none).
    let mut by_tag: BTreeMap<String, Vec<&Operation>> = BTreeMap::new();
    for op in &doc.operations {
        let tag = op.tag.clone().unwrap_or_else(|| "Default".to_string());
        by_tag.entry(tag).or_default().push(op);
    }

    let mut files = Vec::new();
    for (tag, ops) in &by_tag {
        let stem = pascal(tag);
        let file = api_dir.join(format!("{stem}.ts"));
        let rel = path_str(&file, out_dir);
        let content = emit_tag_file(doc, tag, ops);
        files.push((rel, file, content));
    }
    Ok(files)
}

fn emit_tag_file(doc: &Document, tag: &str, ops: &[&Operation]) -> String {
    let class_name = format!("{}Api", pascal(tag));
    let mut refs = TypeRefs::default();
    let mut validate_imports = std::collections::BTreeSet::new();

    let mut params_interfaces = String::new();
    let mut methods = String::new();
    for op in ops {
        let (iface, body, validators) = emit_method(doc, op, &mut refs);
        if let Some(iface) = iface {
            params_interfaces.push_str(&iface);
            params_interfaces.push('\n');
        }
        methods.push_str(&body);
        methods.push('\n');
        for v in validators {
            validate_imports.insert(v);
        }
    }

    let mut out = String::new();
    out.push_str(&file_header());
    out.push('\n');
    // Imports.
    out.push_str("import type { ApiClient, RequestOptions } from \"../client\";\n");
    if !validate_imports.is_empty() {
        let names: Vec<&str> = validate_imports.iter().map(|s| s.as_str()).collect();
        out.push_str(&format!(
            "import {{ {} }} from \"../validate\";\n",
            names.join(", ")
        ));
    }
    if refs.model_imports() {
        // Collect distinct referenced model names and emit one import each.
        // Names are pascalized to match the generated model filenames/identifiers.
        let mut names: Vec<String> = refs
            .names
            .iter()
            .map(|n| crate::name::pascal(n))
            .collect();
        names.sort();
        names.dedup();
        for name in &names {
            out.push_str(&format!(
                "import type {{ {name} }} from \"../models/{name}\";\n"
            ));
        }
    }
    out.push('\n');

    // Params interfaces go BEFORE the class (TS forbids nested interfaces).
    out.push_str(&params_interfaces);

    out.push_str(&format!("export class {class_name} {{\n"));
    out.push_str("  constructor(private readonly client: ApiClient) {}\n\n");
    out.push_str(&methods);
    out.push_str("}\n\n");
    // Factory for ergonomic construction at the top level.
    out.push_str(&format!(
        "/** Construct the {class_name} against an existing client. */\nexport function {ctor}({var}: {{ client: ApiClient }}): {class_name} {{\n  return new {class_name}({var}.client);\n}}\n",
        ctor = camel(&class_name),
        var = "deps"
    ));

    out
}

/// Emit one operation. Returns `(params_interface, method_body, validator_imports)` so the caller
/// can place the interface before the class definition.
fn emit_method(_doc: &Document, op: &Operation, refs: &mut TypeRefs) -> (Option<String>, String, Vec<String>) {
    let method_name = camel(&op.operation_id);

    // Partition parameters by location.
    let mut path_params = Vec::new();
    let mut query_params = Vec::new();
    let mut header_params = Vec::new();
    for p in &op.parameters {
        match p.location {
            ParamLocation::Path => path_params.push(p),
            ParamLocation::Query => query_params.push(p),
            ParamLocation::Header => header_params.push(p),
        }
    }

    let has_body = op.request_body.is_some();
    let has_params = !path_params.is_empty()
        || !query_params.is_empty()
        || !header_params.is_empty()
        || has_body;

    // Build the params interface (if any) and rewrite path placeholders.
    let params_name = format!("{}Params", pascal(&op.operation_id));
    let mut path_subst = op.path.clone();
    let iface = if has_params {
        let mut s = String::new();
        s.push_str(&format!("export interface {params_name} {{\n"));
        for p in path_params
            .iter()
            .chain(query_params.iter())
            .chain(header_params.iter())
        {
            let optional = if p.required { "" } else { "?" };
            refs.add(&p.ty);
            s.push_str(&format!(
                "  {}{}: {};\n",
                crate::name::property_key(&p.name),
                optional,
                render(&p.ty)
            ));
        }
        if let Some(body) = &op.request_body {
            let optional = if body.required { "" } else { "?" };
            refs.add(&body.ty);
            s.push_str(&format!("  body{optional}: {};\n", render(&body.ty)));
        }
        s.push_str("}\n");
        Some(s)
    } else {
        None
    };

    // Rewrite path params from {petId} to ${params.petId} (or ${params["x"]}).
    for p in &path_params {
        path_subst = path_subst.replace(
            &format!("{{{}}}", p.name),
            &format!("${{{}}}", crate::name::member_access("params", &p.name)),
        );
    }

    // Success body type.
    let success = success_body(op);
    if let Some(t) = &success {
        refs.add(t);
    }
    let returns = success
        .as_ref()
        .map(render)
        .unwrap_or_else(|| "void".to_string());
    let is_void = success.is_none();

    // Build the RequestOptions object literal passed to the client. Keys are
    // always quoted strings; values use member_access() so reserved/hyphenated
    // param names resolve to bracket notation (e.g. params["package"]).
    let mut opts_parts: Vec<String> = Vec::new();
    if !header_params.is_empty() {
        let entries: Vec<String> = header_params
            .iter()
            .map(|p| {
                format!(
                    "\"{}\": {}",
                    p.name,
                    crate::name::member_access("params", &p.name)
                )
            })
            .collect();
        opts_parts.push(format!("headers: {{ {} }}", entries.join(", ")));
    }
    if !query_params.is_empty() {
        let entries: Vec<String> = query_params
            .iter()
            .map(|p| {
                format!(
                    "\"{}\": {}",
                    p.name,
                    crate::name::member_access("params", &p.name)
                )
            })
            .collect();
        opts_parts.push(format!("query: {{ {} }}", entries.join(", ")));
    }
    if has_body {
        opts_parts.push("body: params.body".to_string());
    }
    // Per-operation retry policy override.
    if let Some(retry) = &op.retry_policy {
        let retry_fields = match (retry.max_retries, retry.retryable) {
            (Some(max), false) => format!("{{ maxRetries: {max}, retryable: false }}"),
            (Some(max), true) => format!("{{ maxRetries: {max} }}"),
            (None, false) => "{{ retryable: false }}".to_string(),
            (None, true) => String::new(),
        };
        if !retry_fields.is_empty() {
            opts_parts.push(format!("retry: {retry_fields}"));
        }
    }
    let opts_literal = if opts_parts.is_empty() {
        String::new()
    } else {
        format!(", {{ {} }}", opts_parts.join(", "))
    };

    // Determine validator names for request body and response body.
    let req_validator = op.request_body.as_ref().and_then(|b| validator_name(&b.ty));
    let resp_validator = success.as_ref().and_then(|t| validator_name(t));

    // Schema names for the generated code (e.g. "PetSchema", "StatusSchema").
    let req_schema_name = op.request_body.as_ref().and_then(|b| schema_name(&b.ty));
    let resp_schema_name = success.as_ref().and_then(|t| schema_name(t));

    let mut used_validators = Vec::new();
    if let Some(v) = &req_validator {
        used_validators.push(v.clone());
    }
    if let Some(v) = &resp_validator {
        used_validators.push(v.clone());
    }
    // Also import the schema constants and validateRequest/validateResponse functions.
    if let Some(s) = &req_schema_name {
        used_validators.push(s.clone());
    }
    if let Some(s) = &resp_schema_name {
        used_validators.push(s.clone());
    }
    // When any validation is needed, import the validateRequest/validateResponse functions.
    if req_schema_name.is_some() {
        used_validators.push("validateRequest".to_string());
    }
    if resp_schema_name.is_some() {
        used_validators.push("validateResponse".to_string());
    }

    let call = if is_void {
        let mut lines = String::new();
        if let (Some(v), Some(s)) = (&req_validator, &req_schema_name) {
            lines.push_str(&format!(
                "    if (this.client.validate) {{ validateRequest(\"{}\", `{}`, params.body, {}); }}\n",
                op.method.upper(),
                path_subst,
                s,
            ));
            let _ = v;
        }
        lines.push_str(&format!(
            "    await this.client.request(\"{}\", `{}`{});",
            op.method.upper(),
            path_subst,
            opts_literal
        ));
        lines
    } else {
        let mut lines = String::new();
        if let (Some(v), Some(s)) = (&req_validator, &req_schema_name) {
            lines.push_str(&format!(
                "    if (this.client.validate) {{ validateRequest(\"{}\", `{}`, params.body, {}); }}\n",
                op.method.upper(),
                path_subst,
                s,
            ));
            let _ = v;
        }
        if let (Some(v), Some(s)) = (&resp_validator, &resp_schema_name) {
            lines.push_str(&format!(
                "    const result = await this.client.requestJson<{}>(\"{}\", `{}`{});\n",
                returns,
                op.method.upper(),
                path_subst,
                opts_literal
            ));
            lines.push_str(&format!(
                "    if (this.client.validate) {{ validateResponse(\"{}\", `{}`, 200, result, {}); }}\n",
                op.method.upper(),
                path_subst,
                s,
            ));
            lines.push_str("    return result;");
            let _ = v;
        } else {
            lines.push_str(&format!(
                "    return await this.client.requestJson<{}>(\"{}\", `{}`{});",
                returns,
                op.method.upper(),
                path_subst,
                opts_literal
            ));
        }
        lines
    };

    // Doc comment.
    let mut doc = String::new();
    if let Some(s) = &op.summary {
        doc.push_str(&format!("/**\n * {s}\n"));
    } else {
        doc.push_str(&format!("/**\n * {} {}\n", op.method.upper(), op.path));
    }
    if let Some(d) = &op.description {
        for line in d.lines() {
            doc.push_str(&format!(" * {line}\n"));
        }
    }
    // @param tags for parameters.
    for p in &op.parameters {
        let pname = crate::name::member_access("params", &p.name);
        let suffix = if p.required { String::new() } else { " (optional)".to_string() };
        if let Some(desc) = &p.description {
            doc.push_str(&format!(" * @param {pname} - {desc}{suffix}\n"));
        } else {
            doc.push_str(&format!(" * @param {pname}{suffix}\n"));
        }
    }
    // @param tag for request body.
    if let Some(body) = &op.request_body {
        let suffix = if body.required { String::new() } else { " (optional)".to_string() };
        if let Some(desc) = &body.description {
            doc.push_str(&format!(" * @param params.body - {desc}{suffix}\n"));
        } else {
            doc.push_str(&format!(" * @param params.body{suffix}\n"));
        }
    }
    // @returns tag from success response description.
    let success_desc = op.responses.iter()
        .filter(|r| r.status.starts_with('2'))
        .min_by_key(|r| r.status.clone())
        .and_then(|r| r.description.clone());
    if let Some(desc) = &success_desc {
        doc.push_str(&format!(" * @returns {desc}\n"));
    } else if success_body(op).is_some() {
        doc.push_str(" * @returns The response body\n");
    }
    // @throws tags for non-success responses.
    for r in &op.responses {
        if !r.status.starts_with('2') && r.status != "*" {
            if let Some(desc) = &r.description {
                doc.push_str(&format!(" * @throws {{ApiError}} {} - {desc}\n", r.status));
            } else {
                doc.push_str(&format!(" * @throws {{ApiError}} {}\n", r.status));
            }
        }
    }
    doc.push_str(" */\n");

    let sig_params = if has_params {
        format!("params: {params_name}")
    } else {
        "options?: RequestOptions".to_string()
    };

    let mut method = String::new();
    method.push_str(&doc);
    let ret: String = if is_void {
        ": Promise<void>".to_string()
    } else {
        format!(": Promise<{returns}>")
    };
    method.push_str(&format!(
        "  async {method_name}({sig_params}){ret} {{\n    {call}\n  }}\n"
    ));

    (iface, method, used_validators)
}

/// Return the validate-module function name for a type (e.g. "validatePet").
fn validator_name(ty: &Type) -> Option<String> {
    match ty {
        Type::Reference { name, .. } => Some(format!("validate{}", crate::name::pascal(name))),
        Type::Array { item, .. } => validator_name(item),
        Type::Map { value, .. } => validator_name(value),
        _ => None,
    }
}

/// Return the schema constant name for a type (e.g. "PetSchema").
fn schema_name(ty: &Type) -> Option<String> {
    match ty {
        Type::Reference { name, .. } => Some(format!("{}Schema", crate::name::pascal(name))),
        Type::Array { item, .. } => schema_name(item),
        Type::Map { value, .. } => schema_name(value),
        _ => None,
    }
}

/// Pick the success-body type for an operation: the body of the lowest 2xx,
/// else `None` (treated as void).
fn success_body(op: &Operation) -> Option<Type> {
    let mut twos: Vec<&specforge_core::Response> = op
        .responses
        .iter()
        .filter(|r| r.status.starts_with('2'))
        .collect();
    twos.sort_by_key(|r| r.status.clone());
    twos.first().and_then(|r| r.body.clone())
}

#[derive(Default)]
struct TypeRefs {
    names: std::collections::BTreeSet<String>,
}

impl TypeRefs {
    fn add(&mut self, ty: &Type) {
        match ty {
            Type::Reference { name, .. } => {
                self.names.insert(name.clone());
            }
            Type::Array { item, .. } => self.add(item),
            Type::Map { value } => self.add(value),
            Type::Composition(Composition { members, .. }) => {
                for m in members {
                    self.add(m);
                }
            }
            Type::Scalar(_) | Type::StringEnum { .. } | Type::Any | Type::Unknown => {}
        }
    }

    fn model_imports(&self) -> bool {
        !self.names.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use specforge_core::{
        HttpMethod, Operation, Parameter as IrParameter, ParamLocation, Response, Type,
    };

    fn op() -> Operation {
        Operation {
            operation_id: "getPet".into(),
            method: HttpMethod::Get,
            path: "/pets/{petId}".into(),
            tag: Some("Pets".into()),
            summary: Some("Get a pet".into()),
            description: None,
            parameters: vec![IrParameter {
                name: "petId".into(),
                location: ParamLocation::Path,
                ty: Type::Scalar(specforge_core::Scalar::String),
                required: true,
                description: None,
            }],
            request_body: None,
            responses: vec![Response {
                status: "200".into(),
                description: None,
                body: Some(Type::Reference {
                    name: "Pet".into(),
                    nullable: false,
                    description: None,
                }),
            }],
            retry_policy: None,
        }
    }

    #[test]
    fn emits_a_method_per_operation() {
        use specforge_core::SchemaRegistry;
        let doc = Document {
            ir_version: specforge_core::IR_VERSION.to_string(),
            title: "Test".into(),
            version: "1.0.0".into(),
            base_url: None,
            security: vec![],
            schemas: SchemaRegistry::default(),
            operations: vec![],
            webhooks: vec![],
        };
        let file = emit_tag_file(&doc, "Pets", &[&op()]);
        assert!(file.contains("export class PetsApi"));
        assert!(file.contains("async getPet(params: GetPetParams): Promise<Pet>"));
        assert!(file.contains("`/pets/${params.petId}`"));
        assert!(file.contains("import type { Pet } from \"../models/Pet\";"));
    }
}
