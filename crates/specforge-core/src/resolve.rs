//! Resolve `$ref` pointers and composition keywords, producing a flat
//! [`SchemaRegistry`](crate::ir::SchemaRegistry) and walking operations into
//! the IR.
//!
//! References are represented by *name* (never inlined) — this keeps generated
//! output readable (named types stay named) and avoids exponential blow-up on
//! deeply composed schemas. Cycles between models become plain `Reference`s
//! and are therefore safe by construction.

use indexmap::IndexMap;
use openapiv3::{
    AdditionalProperties, APIKeyLocation, Components, MediaType, ObjectType, OpenAPI,
    Operation as OApiOperation, Parameter as OApiParameter, PathItem, ReferenceOr, Responses,
    Schema, SchemaData, SchemaKind, Server, StatusCode, StringType, StringFormat, Type as OApiType,
    VariantOrUnknownOrEmpty,
};

use crate::error::ResolveError;
use crate::ir::{
    Composition, CompositionKind, Discriminator, Document, EnumModel, EnumVariant, HttpMethod,
    Model, ObjectModel, Operation, Parameter as IrParameter, ParamLocation, Property, RequestBody,
    Response, Scalar, SchemaRegistry, SecurityScheme, Type, Webhook, IR_VERSION,
};

/// Load and fully resolve a parsed OpenAPI document into the IR.
pub fn resolve(spec: &OpenAPI) -> Result<Document, ResolveError> {
    resolve_with_webhooks(spec, None)
}

/// Load and fully resolve a parsed OpenAPI document into the IR, including
/// any webhooks extracted from the raw spec (OpenAPI 3.1 `webhooks` field).
///
/// The `webhooks_json` parameter is the raw JSON value of the `webhooks`
/// top-level key, if present. Each key is a webhook name and each value is
/// a PathItem-like object with HTTP method keys.
pub fn resolve_with_webhooks(
    spec: &OpenAPI,
    webhooks_json: Option<&serde_json::Value>,
) -> Result<Document, ResolveError> {
    let components = spec.components.as_ref();

    let schemas = resolve_schemas(components)?;
    let operations = resolve_operations(&spec.paths, components)?;
    let security = resolve_security(spec, components);
    let base_url = pick_base_url(&spec.servers);
    let webhooks = match webhooks_json {
        Some(wh) => resolve_webhooks(wh, components)?,
        None => Vec::new(),
    };

    Ok(Document {
        ir_version: IR_VERSION.to_string(),
        title: spec.info.title.clone(),
        version: spec.info.version.clone(),
        base_url,
        security,
        schemas,
        operations,
        webhooks,
    })
}

// ─── Base URL ───────────────────────────────────────────────────────────────

fn pick_base_url(servers: &[Server]) -> Option<String> {
    servers.first().map(|s| s.url.clone())
}

// ─── Security ───────────────────────────────────────────────────────────────

fn resolve_security(
    spec: &OpenAPI,
    components: Option<&Components>,
) -> Vec<SecurityScheme> {
    // Global security requirement names → look up in components.securitySchemes.
    let mut out = Vec::new();
    let Some(reqs) = spec.security.as_ref() else {
        return out;
    };
    let Some(components) = components else {
        return out;
    };

    for req in reqs {
        for name in req.keys() {
            if let Some(ReferenceOr::Item(scheme)) =
                components.security_schemes.get(name)
            {
                if let Some(ir) = map_security_scheme(scheme) {
                    out.push(ir);
                }
            }
        }
    }
    out
}

fn map_security_scheme(scheme: &openapiv3::SecurityScheme) -> Option<SecurityScheme> {
    match scheme {
        openapiv3::SecurityScheme::HTTP { scheme, .. }
            if scheme.eq_ignore_ascii_case("bearer") =>
        {
            Some(SecurityScheme::HttpBearer)
        }
        openapiv3::SecurityScheme::APIKey { name, location, .. } => {
            // We expose API keys as a header for the fetch SDK regardless of
            // `in` (query/cookie) — query keys are added to the URL by the
            // runtime. The header name carries the original `name`.
            let header = match location {
                APIKeyLocation::Query | APIKeyLocation::Cookie | APIKeyLocation::Header => {
                    name.clone()
                }
            };
            Some(SecurityScheme::ApiKey { header })
        }
        _ => None,
    }
}

// ─── Schemas → SchemaRegistry ───────────────────────────────────────────────

fn resolve_schemas(components: Option<&Components>) -> Result<SchemaRegistry, ResolveError> {
    let mut registry = SchemaRegistry::default();
    let Some(components) = components else {
        return Ok(registry);
    };

    for (name, schema_or) in &components.schemas {
        let schema = deref_schema(schema_or)?;
        let model = build_model(name, &schema.schema_data, &schema.schema_kind, components, &registry)?;
        registry.models.insert(name.clone(), model);
    }

    Ok(registry)
}

fn deref_schema(schema_or: &ReferenceOr<Schema>) -> Result<&Schema, ResolveError> {
    match schema_or {
        ReferenceOr::Item(s) => Ok(s),
        ReferenceOr::Reference { reference } => Err(ResolveError::InvalidRef(format!(
            "top-level component schema {reference:?} should be inline, not a $ref"
        ))),
    }
}

/// Build an IR model from a resolved schema. String enums → [`Model::Enum`];
/// everything else → [`Model::Object`] whose `shape_type` records the
/// underlying type (used by emitters to emit either an interface or a type
/// alias).
///
/// For `allOf` compositions, properties from all members are merged (later
/// members override on name conflict) and required sets are unioned. The
/// `components` and `registry` parameters enable resolving `$ref` members.
fn build_model(
    name: &str,
    data: &SchemaData,
    kind: &SchemaKind,
    components: &Components,
    registry: &SchemaRegistry,
) -> Result<Model, ResolveError> {
    // String enum → EnumModel.
    if let SchemaKind::Type(OApiType::String(StringType { enumeration, .. })) = kind {
        let variants: Vec<EnumVariant> = enumeration
            .iter()
            .filter_map(|v| v.as_ref().map(|raw| EnumVariant {
                value: raw.clone(),
                description: None,
            }))
            .collect();
        if !variants.is_empty() {
            return Ok(Model::Enum(EnumModel {
                name: name.to_string(),
                description: data.description.clone(),
                variants,
            }));
        }
    }

    // Object body.
    let (properties, additional) = match kind {
        SchemaKind::Type(OApiType::Object(ObjectType {
            properties,
            required,
            additional_properties,
            ..
        })) => {
            let mut out = Vec::with_capacity(properties.len());
            for (prop_name, prop_schema_or) in properties {
                // Properties may be inline schemas OR $refs; handle both.
                let ty = ref_or_boxed_schema_to_type(prop_schema_or);
                let description = match prop_schema_or {
                    ReferenceOr::Item(s) => s.schema_data.description.clone(),
                    ReferenceOr::Reference { .. } => None,
                };
                out.push(Property {
                    name: prop_name.clone(),
                    ty,
                    required: required.contains(prop_name),
                    description,
                });
            }
            let additional = additional_properties
                .as_ref()
                .and_then(|ap| additional_to_type(ap).map(Box::new));
            (out, additional)
        }
        SchemaKind::AllOf { all_of } => {
            let (props, additional) = merge_allof_properties(all_of, components, registry)?;
            (props, additional)
        }
        _ => {
            // Composition or scalar root: emit as a model with no own
            // properties plus a recorded shape type so emitters can render a
            // type alias (e.g. `type PetEvent = PetCreated | PetUpdated`).
            (vec![], None)
        }
    };

    // For allOf: if exactly one member is a $ref, record it as the base type
    // so emitters can use embedding (Go) or flatten (Rust).
    let base_type = if let SchemaKind::AllOf { all_of } = kind {
        allof_base_type(all_of)
    } else {
        None
    };

    let model = ObjectModel {
        name: name.to_string(),
        description: data.description.clone(),
        properties,
        additional_properties: additional,
        shape_type: Some(schema_kind_to_type(kind, data.nullable, data.discriminator.as_ref())),
        base_type,
    };
    Ok(Model::Object(model))
}

/// Extract properties from a single allOf member (inline object or $ref).
/// Returns `(properties, required_set)`.
fn properties_from_allof_member(
    member: &ReferenceOr<Schema>,
    components: &Components,
    registry: &SchemaRegistry,
) -> Result<(Vec<Property>, std::collections::HashSet<String>), ResolveError> {
    match member {
        ReferenceOr::Reference { reference } => {
            let ref_name = ref_name(reference);
            // First, try to find the schema in components and extract inline.
            if let Some(schema_or) = components.schemas.get(&ref_name) {
                if let ReferenceOr::Item(schema) = schema_or {
                    if let SchemaKind::Type(OApiType::Object(ObjectType {
                        properties,
                        required,
                        ..
                    })) = &schema.schema_kind
                    {
                        let required_set: std::collections::HashSet<String> =
                            required.iter().cloned().collect();
                        let props = properties
                            .iter()
                            .map(|(prop_name, prop_schema_or)| {
                                let ty = ref_or_boxed_schema_to_type(prop_schema_or);
                                let description = match prop_schema_or {
                                    ReferenceOr::Item(s) => s.schema_data.description.clone(),
                                    ReferenceOr::Reference { .. } => None,
                                };
                                Property {
                                    name: prop_name.clone(),
                                    ty,
                                    required: required_set.contains(prop_name),
                                    description,
                                }
                            })
                            .collect();
                        return Ok((props, required_set));
                    }
                }
            }
            // Fallback: check the already-built registry.
            if let Some(Model::Object(obj)) = registry.get(&ref_name) {
                return Ok((obj.properties.clone(), obj.properties.iter().filter(|p| p.required).map(|p| p.name.clone()).collect()));
            }
            Ok((vec![], std::collections::HashSet::new()))
        }
        ReferenceOr::Item(schema) => {
            if let SchemaKind::Type(OApiType::Object(ObjectType {
                properties,
                required,
                ..
            })) = &schema.schema_kind
            {
                let required_set: std::collections::HashSet<String> =
                    required.iter().cloned().collect();
                let props = properties
                    .iter()
                    .map(|(prop_name, prop_schema_or)| {
                        let ty = ref_or_boxed_schema_to_type(prop_schema_or);
                        let description = match prop_schema_or {
                            ReferenceOr::Item(s) => s.schema_data.description.clone(),
                            ReferenceOr::Reference { .. } => None,
                        };
                        Property {
                            name: prop_name.clone(),
                            ty,
                            required: required_set.contains(prop_name),
                            description,
                        }
                    })
                    .collect();
                return Ok((props, required_set));
            }
            Ok((vec![], std::collections::HashSet::new()))
        }
    }
}

/// Merge properties from all allOf members. Later members override earlier on
/// name conflict. Required sets are unioned.
fn merge_allof_properties(
    all_of: &[ReferenceOr<Schema>],
    components: &Components,
    registry: &SchemaRegistry,
) -> Result<(Vec<Property>, Option<Box<Type>>), ResolveError> {
    use indexmap::IndexMap;

    // IndexMap preserves insertion order; later inserts replace earlier entries.
    let mut merged: IndexMap<String, Property> = IndexMap::new();
    let mut all_required: std::collections::HashSet<String> = std::collections::HashSet::new();

    for member in all_of {
        let (props, required) = properties_from_allof_member(member, components, registry)?;
        all_required.extend(required);
        for prop in props {
            merged.insert(prop.name.clone(), prop);
        }
    }

    // Re-mark required based on the unioned set.
    let mut out: Vec<Property> = merged.into_values().collect();
    for prop in &mut out {
        prop.required = all_required.contains(&prop.name);
    }

    Ok((out, None))
}

/// If allOf has exactly one `$ref` member, return it as the base type for
/// embedding (Go) or flatten (Rust). Returns `None` if there are zero or
/// multiple `$ref` members, or if there are only inline members.
fn allof_base_type(all_of: &[ReferenceOr<Schema>]) -> Option<Type> {
    let ref_names: Vec<String> = all_of
        .iter()
        .filter_map(|m| match m {
            ReferenceOr::Reference { reference } => Some(ref_name(reference)),
            _ => None,
        })
        .collect();
    if ref_names.len() == 1 {
        Some(Type::Reference {
            name: ref_names.into_iter().next().unwrap(),
            nullable: false,
            description: None,
        })
    } else {
        None
    }
}

/// Detect the `$ref` + metadata-sibling pattern produced by the 3.1→3.0
/// preprocessing step. When an `allOf` has exactly two members — one `$ref`
/// and one inline schema that carries only metadata (`description`,
/// `summary`, `deprecated`, and no `type`/`properties`/etc.) — collapse
/// them into a single `Type::Reference` with a description override.
///
/// Returns `None` if the pattern doesn't match (caller should fall through
/// to normal allOf handling).
fn allof_ref_sibling_description(all_of: &[ReferenceOr<Schema>]) -> Option<Type> {
    if all_of.len() != 2 {
        return None;
    }

    // Find the $ref and the inline member.
    let (ref_ref, inline) = match (&all_of[0], &all_of[1]) {
        (ReferenceOr::Reference { reference }, ReferenceOr::Item(s)) => (reference, s),
        (ReferenceOr::Item(s), ReferenceOr::Reference { reference }) => (reference, s),
        _ => return None,
    };

    // The inline member must be metadata-only: no `type`, no `properties`,
    // no `allOf`/`oneOf`/`anyOf`, no `items`, no `additional_properties`.
    // `SchemaKind::Any(...)` represents the `{}` / empty schema.
    let is_metadata_only = matches!(inline.schema_kind, SchemaKind::Any(_))
        && (inline.schema_data.description.is_some()
            || inline.schema_data.title.is_some()
            || inline.schema_data.deprecated);

    if !is_metadata_only {
        return None;
    }

    let description = inline
        .schema_data
        .description
        .clone()
        .or_else(|| inline.schema_data.title.clone());

    Some(Type::Reference {
        name: ref_name(ref_ref),
        nullable: false,
        description,
    })
}

fn additional_to_type(ap: &AdditionalProperties) -> Option<Type> {
    match ap {
        AdditionalProperties::Any(true) => Some(Type::Any),
        AdditionalProperties::Any(false) => None,
        AdditionalProperties::Schema(boxed) => Some(boxed_ref_or_schema_to_type(boxed)),
    }
}

/// Convert a resolved schema's *shape* into an IR [`Type`].
fn schema_to_type(schema: &Schema) -> Option<Type> {
    Some(schema_kind_to_type(
        &schema.schema_kind,
        schema.schema_data.nullable,
        schema.schema_data.discriminator.as_ref(),
    ))
}

fn schema_kind_to_type(
    kind: &SchemaKind,
    nullable: bool,
    discriminator: Option<&openapiv3::Discriminator>,
) -> Type {
    match kind {
        SchemaKind::Type(t) => type_to_ir(t, nullable),
        SchemaKind::OneOf { one_of } => {
            composition(CompositionKind::OneOf, one_of, discriminator)
        }
        SchemaKind::AnyOf { any_of } => {
            composition(CompositionKind::AnyOf, any_of, discriminator)
        }
        SchemaKind::AllOf { all_of } => {
            // Check if this is a $ref with metadata siblings (produced by
            // the 3.1→3.0 preprocessing step). When an allOf has exactly
            // one $ref and one inline schema that carries only metadata
            // (description/summary/deprecated), collapse it back to a
            // Reference with a description override.
            if let Some(desc) = allof_ref_sibling_description(all_of) {
                return desc;
            }
            composition(CompositionKind::AllOf, all_of, discriminator)
        }
        // OpenAPI `{}` / `Any(...)` → any.
        SchemaKind::Any(_) => Type::Any,
        SchemaKind::Not { .. } => Type::Any, // TS has no negation; approximate.
    }
}

fn type_to_ir(t: &OApiType, nullable: bool) -> Type {
    match t {
        OApiType::String(s) => {
            // Inline string enums keep their literals so emitters can build
            // discriminant guards (e.g. `type: "pet.created"`).
            let variants: Vec<String> = s
                .enumeration
                .iter()
                .filter_map(|v| v.clone())
                .collect();
            if !variants.is_empty() {
                return Type::StringEnum {
                    variants,
                    nullable,
                };
            }
            let scalar = match &s.format {
                VariantOrUnknownOrEmpty::Item(StringFormat::Date)
                | VariantOrUnknownOrEmpty::Item(StringFormat::DateTime) => Scalar::DateTime,
                VariantOrUnknownOrEmpty::Unknown(fmt)
                    if fmt.eq_ignore_ascii_case("uuid") =>
                {
                    Scalar::Uuid
                }
                _ => Scalar::String,
            };
            Type::Scalar(scalar)
        }
        OApiType::Integer(_) => Type::Scalar(Scalar::Integer),
        OApiType::Number(_) => Type::Scalar(Scalar::Float),
        OApiType::Boolean(_) => Type::Scalar(Scalar::Boolean),
        OApiType::Array(arr) => {
            let item = arr
                .items
                .as_ref()
                .map(ref_or_boxed_schema_to_type)
                .unwrap_or(Type::Unknown);
            Type::Array {
                item: Box::new(item),
                nullable,
            }
        }
        OApiType::Object(ObjectType { additional_properties, .. }) => match additional_properties {
            Some(AdditionalProperties::Any(true)) => Type::Map {
                value: Box::new(Type::Any),
            },
            Some(AdditionalProperties::Any(false)) | None => Type::Any,
            Some(AdditionalProperties::Schema(boxed)) => Type::Map {
                value: Box::new(boxed_ref_or_schema_to_type(boxed)),
            },
        },
    }
}

/// Handle a `ReferenceOr<Box<Schema>>` — used for object *properties*.
fn ref_or_boxed_schema_to_type(schema_or: &ReferenceOr<Box<Schema>>) -> Type {
    match schema_or {
        ReferenceOr::Item(s) => schema_to_type(s).unwrap_or(Type::Unknown),
        ReferenceOr::Reference { reference } => Type::Reference {
            name: ref_name(reference),
            nullable: false,
            description: None,
        },
    }
}

/// Handle a `Box<ReferenceOr<Schema>>` — used for `additionalProperties` and
/// array `items`. (openapiv3 is inconsistent about which side the Box is on.)
fn boxed_ref_or_schema_to_type(boxed: &ReferenceOr<Schema>) -> Type {
    match boxed {
        ReferenceOr::Item(s) => schema_to_type(s).unwrap_or(Type::Unknown),
        ReferenceOr::Reference { reference } => Type::Reference {
            name: ref_name(reference),
            nullable: false,
            description: None,
        },
    }
}

fn composition(
    kind: CompositionKind,
    members: &[ReferenceOr<Schema>],
    discriminator: Option<&openapiv3::Discriminator>,
) -> Type {
    let members: Vec<Type> = members
        .iter()
        .map(|m| match m {
            ReferenceOr::Item(s) => schema_to_type(s).unwrap_or(Type::Unknown),
            ReferenceOr::Reference { reference } => Type::Reference {
                name: ref_name(reference),
                nullable: false,
                description: None,
            },
        })
        .collect();
    let discriminator = discriminator.map(|d| {
        let mapping = if d.mapping.is_empty() {
            None
        } else {
            Some(
                d.mapping
                    .iter()
                    .map(|(k, v)| (k.clone(), ref_name(v)))
                    .collect(),
            )
        };
        Discriminator {
            property_name: d.property_name.clone(),
            mapping,
        }
    });
    Type::Composition(Composition {
        kind,
        members,
        discriminator,
    })
}

/// Extract the trailing path segment of a `#/components/schemas/Foo` ref.
fn ref_name(reference: &str) -> String {
    reference.rsplit('/').next().unwrap_or(reference).to_string()
}

// ─── Operations → IR ────────────────────────────────────────────────────────

fn resolve_operations(
    paths: &openapiv3::Paths,
    components: Option<&Components>,
) -> Result<Vec<Operation>, ResolveError> {
    let mut out = Vec::new();
    for (path, item_or) in &paths.paths {
        let item = match item_or {
            ReferenceOr::Item(i) => i,
            ReferenceOr::Reference { .. } => continue,
        };
        for (method, op) in iter_methods(item) {
            out.push(resolve_operation(
                method,
                path,
                op,
                &item.parameters,
                components,
            )?);
        }
    }
    Ok(out)
}

fn iter_methods(item: &PathItem) -> Vec<(HttpMethod, &OApiOperation)> {
    let mut out = Vec::new();
    if let Some(op) = &item.get {
        out.push((HttpMethod::Get, op));
    }
    if let Some(op) = &item.post {
        out.push((HttpMethod::Post, op));
    }
    if let Some(op) = &item.put {
        out.push((HttpMethod::Put, op));
    }
    if let Some(op) = &item.patch {
        out.push((HttpMethod::Patch, op));
    }
    if let Some(op) = &item.delete {
        out.push((HttpMethod::Delete, op));
    }
    if let Some(op) = &item.head {
        out.push((HttpMethod::Head, op));
    }
    if let Some(op) = &item.options {
        out.push((HttpMethod::Options, op));
    }
    out
}

fn resolve_operation(
    method: HttpMethod,
    path: &str,
    op: &OApiOperation,
    path_item_params: &[ReferenceOr<OApiParameter>],
    components: Option<&Components>,
) -> Result<Operation, ResolveError> {
    let operation_id = op
        .operation_id
        .clone()
        .unwrap_or_else(|| synthesize_operation_id(method, path));

    // Merge path-level and operation-level parameters. Operation-level wins on
    // name conflicts (per OpenAPI spec). Track seen names to dedupe.
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut parameters = Vec::new();
    for source in [path_item_params, &op.parameters] {
        for p in source {
            if let Some(ir) = resolve_parameter(p, components)? {
                if seen.insert(ir.name.clone()) {
                    parameters.push(ir);
                }
            }
        }
    }

    let request_body = op
        .request_body
        .as_ref()
        .and_then(|rb| match rb {
            ReferenceOr::Item(b) => Some(b),
            ReferenceOr::Reference { .. } => None,
        })
        .and_then(|b| {
            b.content
                .get("application/json")
                .and_then(|m| m.schema.as_ref())
                .map(|s| RequestBody {
                    ty: ref_or_schema_to_type(s),
                    required: b.required,
                    description: b.description.clone(),
                })
        });

    let responses = resolve_responses(&op.responses)?;


    Ok(Operation {
        operation_id,
        method,
        path: path.to_string(),
        tag: op.tags.first().cloned(),
        summary: op.summary.clone(),
        description: op.description.clone(),
        parameters,
        request_body,
        responses,
    
        retry_policy: None,
    })
}

fn synthesize_operation_id(method: HttpMethod, path: &str) -> String {
    let cleaned = path
        .trim_matches('/')
        .split('/')
        .map(|seg| {
            seg.trim_start_matches('{')
                .trim_end_matches('}')
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("_");
    format!("{}{}", method.as_str(), cleaned)
}

fn resolve_parameter(
    p: &ReferenceOr<OApiParameter>,
    components: Option<&Components>,
) -> Result<Option<IrParameter>, ResolveError> {
    // Dereference component parameters (#/components/parameters/{name}).
    let param = match p {
        ReferenceOr::Item(p) => p,
        ReferenceOr::Reference { reference } => {
            let name = ref_name(reference);
            let Some(Some(components)) = components.map(|c| c.parameters.get(&name)) else {
                return Ok(None);
            };
            return match components {
                ReferenceOr::Item(p) => resolve_parameter_item(p),
                ReferenceOr::Reference { .. } => Ok(None),
            };
        }
    };
    resolve_parameter_item(param)
}

fn resolve_parameter_item(param: &OApiParameter) -> Result<Option<IrParameter>, ResolveError> {
    let (location, data) = match param {
        OApiParameter::Query { parameter_data, .. } => (ParamLocation::Query, parameter_data),
        OApiParameter::Header { parameter_data, .. } => (ParamLocation::Header, parameter_data),
        OApiParameter::Path { parameter_data, .. } => (ParamLocation::Path, parameter_data),
        OApiParameter::Cookie { parameter_data, .. } => (ParamLocation::Header, parameter_data),
    };
    let ty = match &data.format {
        openapiv3::ParameterSchemaOrContent::Schema(s) => ref_or_schema_to_type(s),
        openapiv3::ParameterSchemaOrContent::Content(_) => Type::Unknown,
    };
    Ok(Some(IrParameter {
        name: data.name.clone(),
        location,
        ty,
        required: data.required,
        description: data.description.clone(),
    }))
}

fn resolve_responses(
    responses: &Responses,
) -> Result<Vec<Response>, ResolveError> {
    let mut out = Vec::new();
    for (status, resp_or) in &responses.responses {
        let body = match resp_or {
            ReferenceOr::Item(r) => json_body(&r.content),
            // Component response refs aren't inlined here; emit a bodyless
            // response so the operation still resolves. (v1 scope.)
            ReferenceOr::Reference { .. } => None,
        };
        out.push(Response {
            status: status_code_str(status),
            description: None,
            body,
        });
    }
    if let Some(default) = &responses.default {
        if let ReferenceOr::Item(r) = default {
            out.push(Response {
                status: "default".to_string(),
                description: Some(r.description.clone()),
                body: json_body(&r.content),
            });
        }
    }
    Ok(out)
}

fn status_code_str(code: &StatusCode) -> String {
    match code {
        StatusCode::Code(n) => n.to_string(),
        StatusCode::Range(n) => format!("{n}XX"),
    }
}

fn json_body(content: &IndexMap<String, MediaType>) -> Option<Type> {
    content
        .get("application/json")
        .and_then(|m| m.schema.as_ref())
        .map(ref_or_schema_to_type)
}

fn ref_or_schema_to_type(schema_or: &ReferenceOr<Schema>) -> Type {
    match schema_or {
        ReferenceOr::Item(s) => schema_to_type(s).unwrap_or(Type::Unknown),
        ReferenceOr::Reference { reference } => Type::Reference {
            name: ref_name(reference),
            nullable: false,
            description: None,
        },
    }
}

// ─── Webhooks → IR ──────────────────────────────────────────────────────────

/// Resolve the raw `webhooks` JSON map into IR [`Webhook`] values.
///
/// In OpenAPI 3.1, webhooks are a map of `name -> PathItem` where each
/// PathItem contains HTTP method keys (post, get, etc.). Since the
/// `openapiv3` crate doesn't support webhooks, we parse from raw JSON.
fn resolve_webhooks(
    webhooks_json: &serde_json::Value,
    components: Option<&Components>,
) -> Result<Vec<Webhook>, ResolveError> {
    let Some(obj) = webhooks_json.as_object() else {
        return Ok(Vec::new());
    };

    let mut webhooks = Vec::new();

    for (name, path_item) in obj {
        let Some(path_obj) = path_item.as_object() else {
            continue;
        };

        // Each key in the PathItem is an HTTP method.
        let methods = [
            ("get", HttpMethod::Get),
            ("post", HttpMethod::Post),
            ("put", HttpMethod::Put),
            ("patch", HttpMethod::Patch),
            ("delete", HttpMethod::Delete),
            ("head", HttpMethod::Head),
            ("options", HttpMethod::Options),
        ];

        for (method_str, method) in &methods {
            let Some(op_json) = path_obj.get(*method_str) else {
                continue;
            };
            let Some(op_obj) = op_json.as_object() else {
                continue;
            };

            let path = format!("/webhooks/{}", name);

            let summary = op_obj
                .get("summary")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let description = op_obj
                .get("description")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let request_body = resolve_webhook_request_body(op_obj, components);

            let responses = resolve_webhook_responses(op_obj, components);

            webhooks.push(Webhook {
                name: name.clone(),
                method: *method,
                path,
                summary,
                description,
                request_body,
                responses,
            });
        }
    }

    Ok(webhooks)
}

/// Extract the request body from a webhook operation JSON object.
fn resolve_webhook_request_body(
    op_obj: &serde_json::Map<String, serde_json::Value>,
    _components: Option<&Components>,
) -> Option<RequestBody> {
    let rb = op_obj.get("requestBody")?;
    let rb_obj = rb.as_object()?;

    let content = rb_obj.get("content")?.as_object()?;
    let json_content = content.get("application/json")?.as_object()?;
    let schema_json = json_content.get("schema")?;

    let ty = webhook_json_to_type(schema_json);
    let required = rb_obj
        .get("required")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let description = rb_obj
        .get("description")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Some(RequestBody {
        ty,
        required,
        description,
    })
}

/// Extract responses from a webhook operation JSON object.
fn resolve_webhook_responses(
    op_obj: &serde_json::Map<String, serde_json::Value>,
    _components: Option<&Components>,
) -> Vec<Response> {
    let Some(responses_json) = op_obj.get("responses") else {
        return Vec::new();
    };
    let Some(responses_obj) = responses_json.as_object() else {
        return Vec::new();
    };

    let mut responses = Vec::new();

    for (status, resp_json) in responses_obj {
        let body = resp_json
            .as_object()
            .and_then(|r| r.get("content"))
            .and_then(|c| c.as_object())
            .and_then(|c| c.get("application/json"))
            .and_then(|m| m.as_object())
            .and_then(|m| m.get("schema"))
            .map(webhook_json_to_type);

        let description = resp_json
            .as_object()
            .and_then(|r| r.get("description"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        responses.push(Response {
            status: status.clone(),
            description,
            body,
        });
    }

    responses
}

/// Convert a raw JSON schema value into an IR Type. This handles `$ref`
/// references and inline types from webhook definitions.
fn webhook_json_to_type(schema: &serde_json::Value) -> Type {
    let Some(obj) = schema.as_object() else {
        return Type::Unknown;
    };

    // Handle $ref.
    if let Some(ref_val) = obj.get("$ref").and_then(|v| v.as_str()) {
        return Type::Reference {
            name: ref_name(ref_val),
            nullable: false,
            description: None,
        };
    }

    // Handle inline types.
    match obj.get("type").and_then(|v| v.as_str()) {
        Some("string") => Type::Scalar(Scalar::String),
        Some("integer") => Type::Scalar(Scalar::Integer),
        Some("number") => Type::Scalar(Scalar::Float),
        Some("boolean") => Type::Scalar(Scalar::Boolean),
        Some("array") => {
            let item = obj
                .get("items")
                .map(webhook_json_to_type)
                .unwrap_or(Type::Unknown);
            Type::Array {
                item: Box::new(item),
                nullable: false,
            }
        }
        Some("object") => Type::Any,
        _ => Type::Unknown,
    }
}
