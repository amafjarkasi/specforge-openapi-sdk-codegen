//! Emit one TypeScript file per IR model (interface / type alias / string
//! union). Output is deterministic and import-light: models reference each
//! other by relative path within the generated `models/` directory.
//!
//! For `oneOf`/`anyOf` compositions, also emit runtime type-guard helpers so
//! response unions can be narrowed at the call site (e.g. `isPetCreated(e)`).

use std::collections::BTreeSet;

use specforge_core::{
    Composition, CompositionKind, Discriminator, EnumModel, Model, ObjectModel, Property,
    SchemaRegistry, Type,
};

use crate::name::{pascal, property_key, string_literal};
use crate::types::render;
use crate::util::file_header;

/// The full text of `models/<Name>.ts` for a single model.
///
/// When `registry` is provided, oneOf/anyOf type aliases also get runtime
/// type-guard helpers that inspect discriminant properties on sibling models.
pub fn emit_model_file(model: &Model) -> String {
    emit_model_file_with_registry(model, None)
}

/// Like [`emit_model_file`], but with access to the full schema registry so
/// oneOf guards can read sibling model properties (discriminators / required
/// fields).
pub fn emit_model_file_with_registry(model: &Model, registry: Option<&SchemaRegistry>) -> String {
    let (name, body, refs) = match model {
        Model::Object(o) => emit_object(o, registry),
        Model::Enum(e) => emit_enum(e),
    };
    let imports = build_imports(&refs, &name);
    let mut out = String::new();
    out.push_str(&file_header());
    if !imports.is_empty() {
        out.push('\n');
        out.push_str(&imports);
        out.push('\n');
    }
    out.push('\n');
    out.push_str(&body);
    out.push('\n');
    out
}

/// (filename_stem, body, referenced-model-names)
fn emit_object(o: &ObjectModel, registry: Option<&SchemaRegistry>) -> (String, String, BTreeSet<String>) {
    let name = crate::name::safe_model_name(&pascal(&o.name));
    let mut refs = BTreeSet::new();
    let mut body = String::new();

    if let Some(desc) = &o.description {
        body.push_str(&format!("/**\n * {desc}\n"));
        for prop in &o.properties {
            if let Some(pd) = &prop.description {
                let key = property_key(&prop.name);
                let suffix = if prop.required { String::new() } else { " (optional)".to_string() };
                body.push_str(&format!(" * @property {key} - {pd}{suffix}\n"));
            }
        }
        body.push_str(" */\n");
    }

    // If the root is a composition or scalar alias (no own properties), emit a
    // type alias instead of an interface.
    let is_pure_object = o.properties.is_empty()
        && !matches!(&o.shape_type, Some(Type::Composition(_)));
    let has_alias_shape = !o.properties.is_empty() || matches!(&o.shape_type, Some(Type::Composition(_)));

    if !o.properties.is_empty() {
        // Interface for a true object.
        body.push_str(&format!("export interface {name} {{\n"));
        for prop in &o.properties {
            collect_refs(&prop.ty, &mut refs);
            body.push_str(&render_property(prop));
        }
        // additionalProperties → index signature
        if let Some(addl) = &o.additional_properties {
            collect_refs(addl, &mut refs);
            body.push_str(&format!(
                "  [key: string]: {};\n",
                render(addl)
            ));
        }
        body.push_str("}\n");
    } else if let Some(shape) = &o.shape_type {
        // Type alias for unions/intersections/scalars.
        collect_refs(shape, &mut refs);
        body.push_str(&format!("export type {name} = {};\n", render(shape)));
        // Runtime narrowing helpers for oneOf/anyOf unions.
        if let Type::Composition(comp) = shape {
            if matches!(comp.kind, CompositionKind::OneOf | CompositionKind::AnyOf) {
                if let Some(guards) = emit_union_guards(&name, comp, registry) {
                    body.push('\n');
                    body.push_str(&guards);
                }
            }
        }
    } else if is_pure_object {
        // Empty `{}`.
        body.push_str(&format!("export interface {name} {{}}\n"));
    } else if has_alias_shape {
        // Fallback (shouldn't happen given the above branches).
        body.push_str(&format!("export type {name} = unknown;\n"));
    }

    (name, body, refs)
}

/// Emit `isFoo(value): value is Foo` helpers for each reference arm of a union,
/// plus a `narrowName(value)` switcher when a discriminator is available.
fn emit_union_guards(
    union_name: &str,
    comp: &Composition,
    registry: Option<&SchemaRegistry>,
) -> Option<String> {
    // Only reference arms can be narrowed to named types.
    let arms: Vec<&str> = comp
        .members
        .iter()
        .filter_map(|m| match m {
            Type::Reference { name, .. } => Some(name.as_str()),
            _ => None,
        })
        .collect();
    if arms.is_empty() {
        return None;
    }

    let disc_prop = comp
        .discriminator
        .as_ref()
        .map(|d| d.property_name.as_str());

    let mut out = String::new();
    out.push_str(&format!(
        "/** Runtime type guards for the `{union_name}` union. */\n"
    ));

    for arm in &arms {
        let arm_ty = pascal(arm);
        // Guard names are union-scoped (`is{Union}{Arm}`) because top-level
        // functions named purely after the arm type collide when two unions
        // share an arm (e.g. Stripe's DeletedExternalAccount and
        // DeletedPaymentSource both contain DeletedBankAccount). The
        // `narrow{Union}` helper below is the ergonomic entry point.
        let guard_name = format!("is{union_name}{arm_ty}");
        let check = arm_type_check(arm, disc_prop, registry, comp.discriminator.as_ref());
        out.push_str(&format!(
            "export function {guard_name}(value: {union_name}): value is {arm_ty} {{\n"
        ));
        out.push_str(&format!("  return {check};\n"));
        out.push_str("}\n");
    }

    // Convenience: try each guard in order and return the narrowed arm name.
    out.push_str(&format!(
        "/** Return which `{union_name}` arm `value` matches, or `undefined`. */\n"
    ));
    out.push_str(&format!(
        "export function narrow{union_name}(value: {union_name}): {arm_names} | undefined {{\n",
        arm_names = arms
            .iter()
            .map(|a| format!("\"{}\"", pascal(a)))
            .collect::<Vec<_>>()
            .join(" | ")
    ));
    for arm in &arms {
        let arm_ty = pascal(arm);
        out.push_str(&format!(
            "  if (is{union_name}{arm_ty}(value)) return \"{arm_ty}\";\n"
        ));
    }
    out.push_str("  return undefined;\n");
    out.push_str("}\n");

    Some(out)
}

/// Build a TS expression that is true when `value` matches the named arm.
fn arm_type_check(
    arm_name: &str,
    disc_prop: Option<&str>,
    registry: Option<&SchemaRegistry>,
    disc: Option<&Discriminator>,
) -> String {
    // Prefer discriminator enum value when we can resolve the arm's property.
    if let (Some(prop), Some(reg)) = (disc_prop, registry) {
        if let Some(lit) = discriminant_literal(arm_name, prop, reg, disc) {
            let key = property_key(prop);
            // value is object-like and the discriminant matches.
            return format!(
                "typeof value === \"object\" && value !== null && {access} === {lit}",
                access = if key.starts_with('"') {
                    format!("(value as unknown as Record<string, unknown>)[{key}]")
                } else {
                    format!("(value as unknown as Record<string, unknown>).{key}")
                }
            );
        }
        // Discriminator property present but no enum literal — just check presence.
        let key = property_key(prop);
        let access = if key.starts_with('"') {
            format!("(value as unknown as Record<string, unknown>)[{key}]")
        } else {
            format!("(value as unknown as Record<string, unknown>).{key}")
        };
        // Fall through to required-field check, but require the disc prop exists.
        if let Some(req) = required_field_check(arm_name, reg, Some(prop)) {
            return format!(
                "typeof value === \"object\" && value !== null && {access} !== undefined && {req}"
            );
        }
        return format!(
            "typeof value === \"object\" && value !== null && {access} !== undefined"
        );
    }

    // No discriminator: narrow by presence of required fields unique-ish to the arm.
    if let Some(reg) = registry {
        if let Some(req) = required_field_check(arm_name, reg, None) {
            return format!("typeof value === \"object\" && value !== null && {req}");
        }
    }

    // Last resort: structural object check (still useful as a type predicate seed).
    "typeof value === \"object\" && value !== null".to_string()
}

/// If the arm model has `prop` as a required string-enum with a single variant,
/// return that literal (e.g. `"pet.created"`). Falls back to the discriminator
/// `mapping` when the arm's own property doesn't yield a literal.
fn discriminant_literal(
    arm_name: &str,
    prop: &str,
    registry: &SchemaRegistry,
    disc: Option<&Discriminator>,
) -> Option<String> {
    // 1. Try the arm's own property (single-variant string enum).
    if let Some(Model::Object(obj)) = registry.get(arm_name) {
        if let Some(p) = obj.properties.iter().find(|p| p.name == prop) {
            match &p.ty {
                Type::StringEnum { variants, .. } if variants.len() == 1 => {
                    return Some(string_literal(&variants[0]));
                }
                Type::Reference { name, .. } => {
                    if let Some(Model::Enum(e)) = registry.get(name) {
                        if e.variants.len() == 1 {
                            return Some(string_literal(&e.variants[0].value));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // 2. Fall back to explicit discriminator mapping.
    if let Some(d) = disc {
        if let Some(mapping) = &d.mapping {
            for (disc_value, schema_name) in mapping {
                if schema_name == arm_name {
                    return Some(string_literal(disc_value));
                }
            }
        }
    }

    None
}

/// Build ` "a" in value && "b" in value ` for the arm's required properties.
/// `extra_exclude` skips a property already checked (e.g. the discriminator).
fn required_field_check(
    arm_name: &str,
    registry: &SchemaRegistry,
    extra_exclude: Option<&str>,
) -> Option<String> {
    let model = registry.get(arm_name)?;
    let Model::Object(obj) = model else {
        return None;
    };
    let reqs: Vec<&str> = obj
        .properties
        .iter()
        .filter(|p| p.required)
        .map(|p| p.name.as_str())
        .filter(|n| extra_exclude != Some(*n))
        .collect();
    // Always include at least the required props; if none, no structural signal.
    let mut names = reqs;
    if names.is_empty() {
        // Fall back to all property names so empty-required objects still check shape.
        names = obj.properties.iter().map(|p| p.name.as_str()).collect();
    }
    if names.is_empty() {
        return None;
    }
    let parts: Vec<String> = names
        .iter()
        .map(|n| format!("\"{}\" in (value as object)", escape_str(n)))
        .collect();
    Some(parts.join(" && "))
}

fn escape_str(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn emit_enum(e: &EnumModel) -> (String, String, BTreeSet<String>) {
    let name = crate::name::safe_model_name(&pascal(&e.name));
    let mut body = String::new();
    if let Some(desc) = &e.description {
        body.push_str(&format_doc_comment(desc, ""));
    }
    // String union — the modern, tree-shakeable representation. (A real TS
    // `enum` adds runtime overhead and is discouraged in 2026.)
    let arms: Vec<String> = e.variants.iter().map(|v| string_literal(&v.value)).collect();
    body.push_str(&format!("export type {name} = {};\n", arms.join(" | ")));
    // Also export a const tuple of the values for runtime iteration/validation.
    body.push_str(&format!(
        "export const {name}Values = [{}] as const;\n",
        arms.join(", ")
    ));
    (name, body, BTreeSet::new())
}

fn render_property(p: &Property) -> String {
    let key = property_key(&p.name);
    let optional = if p.required { "" } else { "?" };
    let ty = render(&p.ty);
    let mut out = String::new();
    if let Some(desc) = &p.description {
        out.push_str(&format_doc_comment(desc, "  "));
    }
    out.push_str(&format!("  {key}{optional}: {ty};\n"));
    out
}

fn format_doc_comment(text: &str, pad: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = String::new();
    for line in &lines {
        out.push_str(&format!("{pad}/// {line}\n"));
    }
    out
}

/// Walk a type and record every `Reference` name it transitively mentions.
fn collect_refs(ty: &Type, out: &mut BTreeSet<String>) {
    match ty {
        Type::Reference { name, .. } => {
            out.insert(name.clone());
        }
        Type::Array { item, .. } => collect_refs(item, out),
        Type::Map { value } => collect_refs(value, out),
        Type::Composition(c) => {
            for m in &c.members {
                collect_refs(m, out);
            }
        }
        Type::Scalar(_) | Type::StringEnum { .. } | Type::Any | Type::Unknown => {}
    }
}

/// Build the relative `import` block. Same-directory imports omit the path
/// extension (TS resolves `.ts` automatically under bundler/nodenext). A model
/// never imports itself (some specs have self-referential `$ref`s).
fn build_imports(refs: &BTreeSet<String>, self_name: &str) -> String {
    if refs.is_empty() {
        return String::new();
    }
    let self_pascal = pascal(self_name);
    let mut lines: Vec<String> = refs
        .iter()
        .filter_map(|r| {
            let p = pascal(r);
            if p == self_pascal {
                None
            } else {
                Some(format!("import {{ {p} }} from \"./{p}\";"))
            }
        })
        .collect();
    lines.sort();
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use specforge_core::{
        Composition, CompositionKind, EnumModel, EnumVariant, ObjectModel, Property, Scalar, Type,
    };

    #[test]
    fn interface_with_required_and_optional() {
        let o = ObjectModel {
            name: "Pet".into(),
            description: Some("A pet".into()),
            properties: vec![
                Property {
                    name: "id".into(),
                    ty: Type::Scalar(Scalar::Uuid),
                    required: true,
                    description: None,
                },
                Property {
                    name: "age".into(),
                    ty: Type::Scalar(Scalar::Integer),
                    required: false,
                    description: None,
                },
            ],
            additional_properties: None,
            shape_type: None,
            base_type: None,
        };
        let file = emit_model_file(&Model::Object(o));
        assert!(file.contains("export interface Pet {"));
        assert!(file.contains("id: string;"));
        assert!(file.contains("age?: number;"));
        assert!(file.contains("* A pet"));
    }

    #[test]
    fn enum_emits_string_union_and_values() {
        let e = EnumModel {
            name: "Species".into(),
            description: None,
            variants: vec![
                EnumVariant {
                    value: "dog".into(),
                    description: None,
                },
                EnumVariant {
                    value: "cat".into(),
                    description: None,
                },
            ],
        };
        let file = emit_model_file(&Model::Enum(e));
        assert!(file.contains("export type Species = \"dog\" | \"cat\";"));
        assert!(file.contains("export const SpeciesValues = [\"dog\", \"cat\"] as const;"));
    }

    #[test]
    fn union_root_becomes_type_alias() {
        let o = ObjectModel {
            name: "PetEvent".into(),
            description: None,
            properties: vec![],
            additional_properties: None,
            shape_type: Some(Type::Composition(specforge_core::Composition {
                kind: CompositionKind::OneOf,
                members: vec![
                    Type::Reference {
                        name: "PetCreated".into(),
                        nullable: false,
                        description: None,
                    },
                    Type::Reference {
                        name: "PetUpdated".into(),
                        nullable: false,
                        description: None,
                    },
                ],
                discriminator: Some(specforge_core::Discriminator {
                    property_name: "type".into(),
                    mapping: None,
                }),
            })),
            base_type: None,
        };
        let file = emit_model_file(&Model::Object(o));
        assert!(file.contains("export type PetEvent = PetCreated | PetUpdated;"));
        assert!(file.contains("import { PetCreated } from \"./PetCreated\";"));
        assert!(file.contains("import { PetUpdated } from \"./PetUpdated\";"));
        // Guards are emitted even without a registry (structural fallback).
        assert!(file.contains("export function isPetEventPetCreated(value: PetEvent): value is PetCreated"));
        assert!(file.contains("export function narrowPetEvent(value: PetEvent)"));
    }

    #[test]
    fn oneof_guards_use_discriminator_literals_from_registry() {
        use specforge_core::{Discriminator, SchemaRegistry};

        let mut registry = SchemaRegistry::default();
        registry.models.insert(
            "PetCreated".into(),
            Model::Object(ObjectModel {
                name: "PetCreated".into(),
                description: None,
                properties: vec![Property {
                    name: "type".into(),
                    ty: Type::StringEnum {
                        variants: vec!["pet.created".into()],
                        nullable: false,
                    },
                    required: true,
                    description: None,
                }],
                additional_properties: None,
                shape_type: None,
                base_type: None,
            }),
        );
        registry.models.insert(
            "PetUpdated".into(),
            Model::Object(ObjectModel {
                name: "PetUpdated".into(),
                description: None,
                properties: vec![Property {
                    name: "type".into(),
                    ty: Type::StringEnum {
                        variants: vec!["pet.updated".into()],
                        nullable: false,
                    },
                    required: true,
                    description: None,
                }],
                additional_properties: None,
                shape_type: None,
                base_type: None,
            }),
        );

        let o = ObjectModel {
            name: "PetEvent".into(),
            description: None,
            properties: vec![],
            additional_properties: None,
            shape_type: Some(Type::Composition(Composition {
                kind: CompositionKind::OneOf,
                members: vec![
                    Type::Reference {
                        name: "PetCreated".into(),
                        nullable: false,
                        description: None,
                    },
                    Type::Reference {
                        name: "PetUpdated".into(),
                        nullable: false,
                        description: None,
                    },
                ],
                discriminator: Some(Discriminator {
                    property_name: "type".into(),
                    mapping: None,
                }),
            })),
            base_type: None,
        };
        let file = emit_model_file_with_registry(&Model::Object(o), Some(&registry));
        assert!(file.contains("=== \"pet.created\""));
        assert!(file.contains("=== \"pet.updated\""));
        assert!(file.contains("export function isPetEventPetCreated"));
        assert!(file.contains("export function isPetEventPetUpdated"));
        assert!(file.contains("export function narrowPetEvent"));
    }

    #[test]
    fn oneof_guards_use_discriminator_mapping_when_no_enum_literals() {
        use indexmap::IndexMap;
        use specforge_core::Discriminator;

        // Arms have a `petType` property but it's a plain string, not a
        // single-variant string enum. The mapping provides the discriminant values.
        let mut registry = SchemaRegistry::default();
        registry.models.insert(
            "Dog".into(),
            Model::Object(ObjectModel {
                name: "Dog".into(),
                description: None,
                properties: vec![Property {
                    name: "petType".into(),
                    ty: Type::Scalar(Scalar::String),
                    required: true,
                    description: None,
                }],
                additional_properties: None,
                shape_type: None,
                base_type: None,
            }),
        );
        registry.models.insert(
            "Cat".into(),
            Model::Object(ObjectModel {
                name: "Cat".into(),
                description: None,
                properties: vec![Property {
                    name: "petType".into(),
                    ty: Type::Scalar(Scalar::String),
                    required: true,
                    description: None,
                }],
                additional_properties: None,
                shape_type: None,
                base_type: None,
            }),
        );

        let mut mapping = IndexMap::new();
        mapping.insert("dog".to_string(), "Dog".to_string());
        mapping.insert("cat".to_string(), "Cat".to_string());

        let o = ObjectModel {
            name: "Pet".into(),
            description: None,
            properties: vec![],
            additional_properties: None,
            shape_type: Some(Type::Composition(Composition {
                kind: CompositionKind::OneOf,
                members: vec![
                    Type::Reference {
                        name: "Dog".into(),
                        nullable: false,
                        description: None,
                    },
                    Type::Reference {
                        name: "Cat".into(),
                        nullable: false,
                        description: None,
                    },
                ],
                discriminator: Some(Discriminator {
                    property_name: "petType".into(),
                    mapping: Some(mapping),
                }),
            })),
            base_type: None,
        };
        let file = emit_model_file_with_registry(&Model::Object(o), Some(&registry));
        assert!(file.contains("=== \"dog\""), "should use mapping value 'dog'");
        assert!(file.contains("=== \"cat\""), "should use mapping value 'cat'");
        assert!(file.contains("export function isPetDog"));
        assert!(file.contains("export function isPetCat"));
        assert!(file.contains("export function narrowPet"));
    }

    #[test]
    fn additional_properties_emit_index_signature() {
        let o = ObjectModel {
            name: "Error".into(),
            description: None,
            properties: vec![Property {
                name: "code".into(),
                ty: Type::Scalar(Scalar::String),
                required: true,
                description: None,
            }],
            additional_properties: Some(Box::new(Type::Any)),
            shape_type: None,
            base_type: None,
        };
        let file = emit_model_file(&Model::Object(o));
        assert!(file.contains("[key: string]: unknown;"));
    }
}
