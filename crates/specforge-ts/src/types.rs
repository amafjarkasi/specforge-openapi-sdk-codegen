//! Map the language-neutral IR [`Type`](specforge_core::Type) into TypeScript type
//! strings.
//!
//! Mapping (reflects the 2026 consensus — see openapi-typescript):
//! - `string` → `string`; `date-time`/`date` → `string`; `uuid` → `string`
//! - `integer`/`number` → `number`; `boolean` → `boolean`
//! - `array<T>` → `T[]`
//! - `map<V>` → `Record<string, V>`
//! - `Reference` → the referenced model's name
//! - `allOf` → `A & B` (intersection)
//! - `oneOf`/`anyOf` → `A | B` (union)
//! - `Any` → `unknown`; `Unknown` → `unknown`
//! - nullable types → `T | null`

use specforge_core::{CompositionKind, Scalar, Type};

use crate::name::pascal;

/// Render a [`Type`] as a TS type expression.
pub fn render(ty: &Type) -> String {
    match ty {
        Type::Scalar(s) => match s {
            Scalar::String | Scalar::DateTime | Scalar::Uuid | Scalar::Integer64 => {
                "string".to_string()
            }
            Scalar::Integer | Scalar::Float => "number".to_string(),
            Scalar::Boolean => "boolean".to_string(),
        },
        Type::StringEnum { variants, nullable } => {
            let arms: Vec<String> = variants
                .iter()
                .map(|v| format!("\"{}\"", v.replace('\\', "\\\\").replace('"', "\\\"")))
                .collect();
            let union = if arms.is_empty() {
                "string".to_string()
            } else {
                arms.join(" | ")
            };
            if *nullable {
                format!("{union} | null")
            } else {
                union
            }
        }
        Type::Array { item, .. } => {
            // Wrap intersections/unions so `A | B[]` doesn't become `(A|B)[]`.
            let inner = render(item);
            if needs_parens(item) {
                format!("({})[]", inner)
            } else {
                format!("{}[]", inner)
            }
        }
        Type::Map { value } => format!("Record<string, {}>", render(value)),
        Type::Reference { name, nullable, .. } => {
            let rendered = pascal(name);
            if *nullable {
                format!("{rendered} | null")
            } else {
                rendered
            }
        }
        Type::Composition(c) => {
            let sep = match c.kind {
                CompositionKind::AllOf => " & ",
                CompositionKind::OneOf | CompositionKind::AnyOf => " | ",
            };
            let members: Vec<String> =
                c.members.iter().map(render).collect();
            let joined = members.join(sep);
            if c.members.len() > 1 && matches!(c.kind, CompositionKind::AllOf) {
                // Intersections read more clearly parenthesized when nested,
                // but at top level TS doesn't require it. Leave bare for now.
                joined
            } else {
                joined
            }
        }
        Type::Any => "unknown".to_string(),
        Type::Unknown => "unknown".to_string(),
    }
}

/// Some types must be parenthesized when used as an array element or union
/// arm, because TS operator precedence would otherwise change the meaning.
fn needs_parens(ty: &Type) -> bool {
    matches!(ty, Type::Composition(_))
}

#[cfg(test)]
mod tests {
    use super::*;
    use specforge_core::{Composition, Type};

    #[test]
    fn scalars() {
        assert_eq!(render(&Type::Scalar(Scalar::String)), "string");
        assert_eq!(render(&Type::Scalar(Scalar::Integer)), "number");
        assert_eq!(render(&Type::Scalar(Scalar::Uuid)), "string");
        assert_eq!(render(&Type::Scalar(Scalar::DateTime)), "string");
        assert_eq!(render(&Type::Scalar(Scalar::Boolean)), "boolean");
    }

    #[test]
    fn arrays_and_maps() {
        assert_eq!(
            render(&Type::Array {
                item: Box::new(Type::Scalar(Scalar::String)),
                nullable: false
            }),
            "string[]"
        );
        assert_eq!(
            render(&Type::Map {
                value: Box::new(Type::Scalar(Scalar::Integer))
            }),
            "Record<string, number>"
        );
    }

    #[test]
    fn references() {
        assert_eq!(
            render(&Type::Reference {
                name: "pet".into(),
                nullable: false,
                description: None,
            }),
            "Pet"
        );
        assert_eq!(
            render(&Type::Reference {
                name: "pet".into(),
                nullable: true,
                description: None,
            }),
            "Pet | null"
        );
    }

    #[test]
    fn unions_parenthesize_inside_arrays() {
        let union = Type::Composition(Composition {
            kind: CompositionKind::OneOf,
            members: vec![
                Type::Reference {
                    name: "a".into(),
                    nullable: false,
                    description: None,
                },
                Type::Reference {
                    name: "b".into(),
                    nullable: false,
                    description: None,
                },
            ],
            discriminator: None,
        });
        assert_eq!(
            render(&Type::Array {
                item: Box::new(union),
                nullable: false
            }),
            "(A | B)[]"
        );
    }

    #[test]
    fn intersections_render_with_ampersand() {
        let all = Type::Composition(Composition {
            kind: CompositionKind::AllOf,
            members: vec![
                Type::Reference {
                    name: "base".into(),
                    nullable: false,
                    description: None,
                },
                Type::Reference {
                    name: "extra".into(),
                    nullable: false,
                    description: None,
                },
            ],
            discriminator: None,
        });
        assert_eq!(render(&all), "Base & Extra");
    }
}
