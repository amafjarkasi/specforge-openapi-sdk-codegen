//! Runtime validation of JSON values against IR Type definitions.
//!
//! This module provides a validation layer that can be used as middleware in
//! generated SDKs to validate request/response bodies against the OpenAPI
//! schema at runtime. It helps catch API contract violations during
//! development and testing.

use serde_json::Value;

use crate::ir::{CompositionKind, Model, Property, Scalar, SchemaRegistry, Type};

/// A single validation error with a JSON-path-style location and message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    /// JSON-pointer-style path to the offending value (e.g. `.items[2].name`).
    pub path: String,
    /// Human-readable description of what went wrong.
    pub message: String,
}

/// Validate a JSON `value` against the given IR `Type`.
///
/// Returns an empty vec when the value is valid; otherwise one error per
/// violation found.
pub fn validate(value: &Value, ty: &Type, registry: &SchemaRegistry) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    validate_inner(value, ty, registry, "", &mut errors);
    errors
}

// ─── Inner recursive validator ───────────────────────────────────────────────

fn validate_inner(
    value: &Value,
    ty: &Type,
    registry: &SchemaRegistry,
    path: &str,
    errors: &mut Vec<ValidationError>,
) {
    match ty {
        Type::Scalar(s) => validate_scalar(value, s, path, errors),

        Type::StringEnum { variants, nullable } => {
            if *nullable && value.is_null() {
                return;
            }
            if let Some(s) = value.as_str() {
                if !variants.iter().any(|v| v == s) {
                    errors.push(ValidationError {
                        path: path.to_string(),
                        message: format!("expected one of {:?}, got \"{}\"", variants, s),
                    });
                }
            } else {
                errors.push(ValidationError {
                    path: path.to_string(),
                    message: format!("expected string, got {}", value_type_name(value)),
                });
            }
        }

        Type::Array { item, nullable } => {
            if *nullable && value.is_null() {
                return;
            }
            if let Some(arr) = value.as_array() {
                for (i, v) in arr.iter().enumerate() {
                    validate_inner(v, item, registry, &format!("{path}[{i}]"), errors);
                }
            } else {
                errors.push(ValidationError {
                    path: path.to_string(),
                    message: format!("expected array, got {}", value_type_name(value)),
                });
            }
        }

        Type::Map { value: val_ty } => {
            if let Some(obj) = value.as_object() {
                for (k, v) in obj {
                    validate_inner(v, val_ty, registry, &format!("{path}.{k}"), errors);
                }
            }
        }

        Type::Reference { name, nullable, .. } => {
            if *nullable && value.is_null() {
                return;
            }
            if let Some(model) = registry.get(name) {
                match model {
                    Model::Object(obj) => {
                        if let Some(shape) = &obj.shape_type {
                            validate_inner(value, shape, registry, path, errors);
                        } else {
                            validate_object(value, &obj.properties, registry, path, errors);
                        }
                    }
                    Model::Enum(e) => {
                        if let Some(s) = value.as_str() {
                            if !e.variants.iter().any(|v| v.value == s) {
                                errors.push(ValidationError {
                                    path: path.to_string(),
                                    message: format!("invalid enum variant \"{}\"", s),
                                });
                            }
                        } else {
                            errors.push(ValidationError {
                                path: path.to_string(),
                                message: format!(
                                    "expected string for enum \"{}\", got {}",
                                    name,
                                    value_type_name(value)
                                ),
                            });
                        }
                    }
                }
            }
        }

        Type::Composition(c) => match c.kind {
            CompositionKind::AllOf => {
                for member in &c.members {
                    validate_inner(value, member, registry, path, errors);
                }
            }
            CompositionKind::OneOf | CompositionKind::AnyOf => {
                let mut any_valid = false;
                for member in &c.members {
                    let mut sub_errors = Vec::new();
                    validate_inner(value, member, registry, path, &mut sub_errors);
                    if sub_errors.is_empty() {
                        any_valid = true;
                    }
                }
                if !any_valid && !c.members.is_empty() {
                    errors.push(ValidationError {
                        path: path.to_string(),
                        message: "value does not match any oneOf/anyOf variant".to_string(),
                    });
                }
            }
        },

        Type::Any | Type::Unknown => {} // always valid
    }
}

// ─── Scalar validator ────────────────────────────────────────────────────────

fn validate_scalar(value: &Value, scalar: &Scalar, path: &str, errors: &mut Vec<ValidationError>) {
    match scalar {
        Scalar::String | Scalar::DateTime | Scalar::Uuid | Scalar::Base64 | Scalar::Binary => {
            if !value.is_string() {
                errors.push(ValidationError {
                    path: path.to_string(),
                    message: format!("expected string, got {}", value_type_name(value)),
                });
            }
        }
        Scalar::Integer | Scalar::Integer64 => {
            if !value.is_i64() && !value.is_u64() {
                errors.push(ValidationError {
                    path: path.to_string(),
                    message: format!("expected integer, got {}", value_type_name(value)),
                });
            }
        }
        Scalar::Float => {
            if !value.is_number() {
                errors.push(ValidationError {
                    path: path.to_string(),
                    message: format!("expected number, got {}", value_type_name(value)),
                });
            }
        }
        Scalar::Boolean if !value.is_boolean() => {
            errors.push(ValidationError {
                path: path.to_string(),
                message: format!("expected boolean, got {}", value_type_name(value)),
            });
        }
        _ => {}
    }
}

// ─── Object validator ────────────────────────────────────────────────────────

fn validate_object(
    value: &Value,
    properties: &[Property],
    registry: &SchemaRegistry,
    path: &str,
    errors: &mut Vec<ValidationError>,
) {
    let obj = match value.as_object() {
        Some(o) => o,
        None => {
            errors.push(ValidationError {
                path: path.to_string(),
                message: format!("expected object, got {}", value_type_name(value)),
            });
            return;
        }
    };
    for prop in properties {
        let prop_path = format!("{path}.{}", prop.name);
        if let Some(val) = obj.get(&prop.name) {
            validate_inner(val, &prop.ty, registry, &prop_path, errors);
        } else if prop.required {
            errors.push(ValidationError {
                path: prop_path,
                message: "missing required property".to_string(),
            });
        }
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn value_type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::*;
    use serde_json::json;

    fn string_type() -> Type {
        Type::Scalar(Scalar::String)
    }

    fn int_type() -> Type {
        Type::Scalar(Scalar::Integer)
    }

    fn float_type() -> Type {
        Type::Scalar(Scalar::Float)
    }

    fn bool_type() -> Type {
        Type::Scalar(Scalar::Boolean)
    }

    fn uuid_type() -> Type {
        Type::Scalar(Scalar::Uuid)
    }

    fn datetime_type() -> Type {
        Type::Scalar(Scalar::DateTime)
    }

    // ── Scalar tests ─────────────────────────────────────────────────────

    #[test]
    fn valid_string() {
        let errors = validate(&json!("hello"), &string_type(), &SchemaRegistry::default());
        assert!(errors.is_empty());
    }

    #[test]
    fn invalid_string() {
        let errors = validate(&json!(42), &string_type(), &SchemaRegistry::default());
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("expected string"));
    }

    #[test]
    fn valid_integer() {
        let errors = validate(&json!(42), &int_type(), &SchemaRegistry::default());
        assert!(errors.is_empty());
    }

    #[test]
    fn invalid_integer() {
        let errors = validate(&json!("hello"), &int_type(), &SchemaRegistry::default());
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("expected integer"));
    }

    #[test]
    fn integer_accepts_negative() {
        let errors = validate(&json!(-7), &int_type(), &SchemaRegistry::default());
        assert!(errors.is_empty());
    }

    #[test]
    fn integer_accepts_zero() {
        let errors = validate(&json!(0), &int_type(), &SchemaRegistry::default());
        assert!(errors.is_empty());
    }

    #[test]
    fn valid_float() {
        let errors = validate(&json!(2.5), &float_type(), &SchemaRegistry::default());
        assert!(errors.is_empty());
    }

    #[test]
    fn integer_is_also_valid_float() {
        // serde_json numbers: integers are valid numbers
        let errors = validate(&json!(42), &float_type(), &SchemaRegistry::default());
        assert!(errors.is_empty());
    }

    #[test]
    fn invalid_float() {
        let errors = validate(&json!("pi"), &float_type(), &SchemaRegistry::default());
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("expected number"));
    }

    #[test]
    fn valid_boolean() {
        let errors = validate(&json!(true), &bool_type(), &SchemaRegistry::default());
        assert!(errors.is_empty());
    }

    #[test]
    fn invalid_boolean() {
        let errors = validate(&json!("yes"), &bool_type(), &SchemaRegistry::default());
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("expected boolean"));
    }

    #[test]
    fn valid_uuid_string() {
        let errors = validate(
            &json!("550e8400-e29b-41d4-a716-446655440000"),
            &uuid_type(),
            &SchemaRegistry::default(),
        );
        assert!(errors.is_empty());
    }

    #[test]
    fn valid_datetime_string() {
        let errors = validate(
            &json!("2024-01-15T10:30:00Z"),
            &datetime_type(),
            &SchemaRegistry::default(),
        );
        assert!(errors.is_empty());
    }

    // ── StringEnum tests ─────────────────────────────────────────────────

    #[test]
    fn valid_enum() {
        let ty = Type::StringEnum {
            variants: vec!["a".into(), "b".into()],
            nullable: false,
        };
        let errors = validate(&json!("a"), &ty, &SchemaRegistry::default());
        assert!(errors.is_empty());
    }

    #[test]
    fn invalid_enum_variant() {
        let ty = Type::StringEnum {
            variants: vec!["a".into(), "b".into()],
            nullable: false,
        };
        let errors = validate(&json!("c"), &ty, &SchemaRegistry::default());
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("expected one of"));
    }

    #[test]
    fn enum_expects_string_not_number() {
        let ty = Type::StringEnum {
            variants: vec!["a".into()],
            nullable: false,
        };
        let errors = validate(&json!(42), &ty, &SchemaRegistry::default());
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("expected string"));
    }

    #[test]
    fn nullable_enum_accepts_null() {
        let ty = Type::StringEnum {
            variants: vec!["a".into()],
            nullable: true,
        };
        let errors = validate(&json!(null), &ty, &SchemaRegistry::default());
        assert!(errors.is_empty());
    }

    // ── Array tests ──────────────────────────────────────────────────────

    #[test]
    fn valid_array() {
        let ty = Type::Array {
            item: Box::new(string_type()),
            nullable: false,
        };
        let errors = validate(&json!(["a", "b"]), &ty, &SchemaRegistry::default());
        assert!(errors.is_empty());
    }

    #[test]
    fn empty_array() {
        let ty = Type::Array {
            item: Box::new(string_type()),
            nullable: false,
        };
        let errors = validate(&json!([]), &ty, &SchemaRegistry::default());
        assert!(errors.is_empty());
    }

    #[test]
    fn invalid_array_item() {
        let ty = Type::Array {
            item: Box::new(string_type()),
            nullable: false,
        };
        let errors = validate(&json!(["a", 42]), &ty, &SchemaRegistry::default());
        assert_eq!(errors.len(), 1);
        assert!(errors[0].path.ends_with("[1]"));
    }

    #[test]
    fn multiple_invalid_array_items() {
        let ty = Type::Array {
            item: Box::new(string_type()),
            nullable: false,
        };
        let errors = validate(&json!([42, 99, true]), &ty, &SchemaRegistry::default());
        assert_eq!(errors.len(), 3);
    }

    #[test]
    fn array_expects_array_not_object() {
        let ty = Type::Array {
            item: Box::new(string_type()),
            nullable: false,
        };
        let errors = validate(&json!({"not": "array"}), &ty, &SchemaRegistry::default());
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("expected array"));
    }

    #[test]
    fn nullable_array_accepts_null() {
        let ty = Type::Array {
            item: Box::new(string_type()),
            nullable: true,
        };
        let errors = validate(&json!(null), &ty, &SchemaRegistry::default());
        assert!(errors.is_empty());
    }

    #[test]
    fn nested_array() {
        let ty = Type::Array {
            item: Box::new(Type::Array {
                item: Box::new(int_type()),
                nullable: false,
            }),
            nullable: false,
        };
        let errors = validate(
            &json!([[1, 2], [3, "bad"]]),
            &ty,
            &SchemaRegistry::default(),
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].path.contains("[1][1]"));
    }

    // ── Map tests ────────────────────────────────────────────────────────

    #[test]
    fn valid_map() {
        let ty = Type::Map {
            value: Box::new(int_type()),
        };
        let errors = validate(&json!({"a": 1, "b": 2}), &ty, &SchemaRegistry::default());
        assert!(errors.is_empty());
    }

    #[test]
    fn invalid_map_value() {
        let ty = Type::Map {
            value: Box::new(int_type()),
        };
        let errors = validate(
            &json!({"a": 1, "b": "wrong"}),
            &ty,
            &SchemaRegistry::default(),
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].path.contains(".b"));
    }

    #[test]
    fn empty_map() {
        let ty = Type::Map {
            value: Box::new(string_type()),
        };
        let errors = validate(&json!({}), &ty, &SchemaRegistry::default());
        assert!(errors.is_empty());
    }

    // ── Object / Reference tests ─────────────────────────────────────────

    #[test]
    fn valid_object() {
        let props = vec![
            Property {
                name: "name".into(),
                ty: string_type(),
                required: true,
                description: None,
            },
            Property {
                name: "id".into(),
                ty: int_type(),
                required: true,
                description: None,
            },
        ];
        let mut errors = Vec::new();
        validate_object(
            &json!({"name": "Fido", "id": 1}),
            &props,
            &SchemaRegistry::default(),
            "",
            &mut errors,
        );
        assert!(errors.is_empty());
    }

    #[test]
    fn missing_required_property() {
        let props = vec![Property {
            name: "name".into(),
            ty: string_type(),
            required: true,
            description: None,
        }];
        let mut errors = Vec::new();
        validate_object(
            &json!({}),
            &props,
            &SchemaRegistry::default(),
            "",
            &mut errors,
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("missing required"));
    }

    #[test]
    fn optional_property_can_be_absent() {
        let props = vec![Property {
            name: "nickname".into(),
            ty: string_type(),
            required: false,
            description: None,
        }];
        let mut errors = Vec::new();
        validate_object(
            &json!({}),
            &props,
            &SchemaRegistry::default(),
            "",
            &mut errors,
        );
        assert!(errors.is_empty());
    }

    #[test]
    fn wrong_property_type() {
        let props = vec![Property {
            name: "age".into(),
            ty: int_type(),
            required: true,
            description: None,
        }];
        let mut errors = Vec::new();
        validate_object(
            &json!({"age": "old"}),
            &props,
            &SchemaRegistry::default(),
            "",
            &mut errors,
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("expected integer"));
    }

    #[test]
    fn object_expects_object_not_array() {
        let props = vec![Property {
            name: "x".into(),
            ty: int_type(),
            required: false,
            description: None,
        }];
        let mut errors = Vec::new();
        validate_object(
            &json!([1, 2, 3]),
            &props,
            &SchemaRegistry::default(),
            "",
            &mut errors,
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("expected object"));
    }

    #[test]
    fn reference_to_object_model() {
        let mut registry = SchemaRegistry::default();
        registry.models.insert(
            "Pet".to_string(),
            Model::Object(ObjectModel {
                name: "Pet".into(),
                description: None,
                properties: vec![
                    Property {
                        name: "name".into(),
                        ty: string_type(),
                        required: true,
                        description: None,
                    },
                    Property {
                        name: "id".into(),
                        ty: int_type(),
                        required: true,
                        description: None,
                    },
                ],
                additional_properties: None,
                shape_type: None,
                base_type: None,
            }),
        );

        let ty = Type::Reference {
            name: "Pet".into(),
            nullable: false,
            description: None,
        };

        let valid = validate(&json!({"name": "Fido", "id": 1}), &ty, &registry);
        assert!(valid.is_empty());

        let invalid = validate(&json!({"name": "Fido"}), &ty, &registry);
        assert_eq!(invalid.len(), 1);
        assert!(invalid[0].message.contains("missing required"));
    }

    #[test]
    fn nullable_reference_accepts_null() {
        let ty = Type::Reference {
            name: "Pet".into(),
            nullable: true,
            description: None,
        };
        let errors = validate(&json!(null), &ty, &SchemaRegistry::default());
        assert!(errors.is_empty());
    }

    #[test]
    fn reference_to_enum_model() {
        let mut registry = SchemaRegistry::default();
        registry.models.insert(
            "Status".to_string(),
            Model::Enum(EnumModel {
                name: "Status".into(),
                description: None,
                variants: vec![
                    EnumVariant {
                        value: "active".into(),
                        description: None,
                    },
                    EnumVariant {
                        value: "inactive".into(),
                        description: None,
                    },
                ],
            }),
        );

        let ty = Type::Reference {
            name: "Status".into(),
            nullable: false,
            description: None,
        };

        let valid = validate(&json!("active"), &ty, &registry);
        assert!(valid.is_empty());

        let invalid = validate(&json!("deleted"), &ty, &registry);
        assert_eq!(invalid.len(), 1);
        assert!(invalid[0].message.contains("invalid enum variant"));
    }

    #[test]
    fn reference_to_enum_expects_string() {
        let mut registry = SchemaRegistry::default();
        registry.models.insert(
            "Status".to_string(),
            Model::Enum(EnumModel {
                name: "Status".into(),
                description: None,
                variants: vec![EnumVariant {
                    value: "active".into(),
                    description: None,
                }],
            }),
        );

        let ty = Type::Reference {
            name: "Status".into(),
            nullable: false,
            description: None,
        };

        let errors = validate(&json!(42), &ty, &registry);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("expected string for enum"));
    }

    // ── Composition tests ────────────────────────────────────────────────

    #[test]
    fn allof_validates_all_members() {
        let ty = Type::Composition(Composition {
            kind: CompositionKind::AllOf,
            members: vec![string_type(), string_type()],
            discriminator: None,
        });
        let errors = validate(&json!("hello"), &ty, &SchemaRegistry::default());
        assert!(errors.is_empty());
    }

    #[test]
    fn allof_reports_errors_from_all_members() {
        let ty = Type::Composition(Composition {
            kind: CompositionKind::AllOf,
            members: vec![int_type(), bool_type()],
            discriminator: None,
        });
        // "hello" is neither an integer nor a boolean
        let errors = validate(&json!("hello"), &ty, &SchemaRegistry::default());
        assert_eq!(errors.len(), 2);
    }

    #[test]
    fn oneof_valid_when_one_matches() {
        let ty = Type::Composition(Composition {
            kind: CompositionKind::OneOf,
            members: vec![int_type(), string_type()],
            discriminator: None,
        });
        let errors = validate(&json!("hello"), &ty, &SchemaRegistry::default());
        assert!(errors.is_empty());
    }

    #[test]
    fn oneof_invalid_when_none_match() {
        let ty = Type::Composition(Composition {
            kind: CompositionKind::OneOf,
            members: vec![int_type(), bool_type()],
            discriminator: None,
        });
        let errors = validate(&json!("hello"), &ty, &SchemaRegistry::default());
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("oneOf/anyOf"));
    }

    #[test]
    fn oneof_empty_members_is_always_valid() {
        let ty = Type::Composition(Composition {
            kind: CompositionKind::OneOf,
            members: vec![],
            discriminator: None,
        });
        let errors = validate(&json!("anything"), &ty, &SchemaRegistry::default());
        assert!(errors.is_empty());
    }

    #[test]
    fn anyof_valid_when_one_matches() {
        let ty = Type::Composition(Composition {
            kind: CompositionKind::AnyOf,
            members: vec![int_type(), bool_type()],
            discriminator: None,
        });
        let errors = validate(&json!(42), &ty, &SchemaRegistry::default());
        assert!(errors.is_empty());
    }

    #[test]
    fn anyof_invalid_when_none_match() {
        let ty = Type::Composition(Composition {
            kind: CompositionKind::AnyOf,
            members: vec![int_type(), bool_type()],
            discriminator: None,
        });
        let errors = validate(&json!("hello"), &ty, &SchemaRegistry::default());
        assert_eq!(errors.len(), 1);
    }

    // ── Any / Unknown tests ──────────────────────────────────────────────

    #[test]
    fn any_accepts_everything() {
        for val in &[json!(null), json!(42), json!("s"), json!(true), json!([1])] {
            let errors = validate(val, &Type::Any, &SchemaRegistry::default());
            assert!(errors.is_empty(), "Any should accept {:?}", val);
        }
    }

    #[test]
    fn unknown_accepts_everything() {
        for val in &[json!(null), json!(42), json!("s"), json!(true), json!({})] {
            let errors = validate(val, &Type::Unknown, &SchemaRegistry::default());
            assert!(errors.is_empty(), "Unknown should accept {:?}", val);
        }
    }

    // ── Nested / deep path tests ─────────────────────────────────────────

    #[test]
    fn deep_path_reported_correctly() {
        let mut registry = SchemaRegistry::default();
        registry.models.insert(
            "Inner".to_string(),
            Model::Object(ObjectModel {
                name: "Inner".into(),
                description: None,
                properties: vec![Property {
                    name: "value".into(),
                    ty: int_type(),
                    required: true,
                    description: None,
                }],
                additional_properties: None,
                shape_type: None,
                base_type: None,
            }),
        );

        let ty = Type::Array {
            item: Box::new(Type::Reference {
                name: "Inner".into(),
                nullable: false,
                description: None,
            }),
            nullable: false,
        };

        let errors = validate(&json!([{"value": 1}, {"value": "bad"}]), &ty, &registry);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].path, "[1].value");
    }

    #[test]
    fn object_with_nested_array() {
        let props = vec![Property {
            name: "items".into(),
            ty: Type::Array {
                item: Box::new(string_type()),
                nullable: false,
            },
            required: true,
            description: None,
        }];
        let mut errors = Vec::new();
        validate_object(
            &json!({"items": ["a", 42]}),
            &props,
            &SchemaRegistry::default(),
            "root",
            &mut errors,
        );
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].path, "root.items[1]");
    }

    #[test]
    fn object_with_shape_type_delegates_to_shape() {
        let mut registry = SchemaRegistry::default();
        registry.models.insert(
            "StringAlias".to_string(),
            Model::Object(ObjectModel {
                name: "StringAlias".into(),
                description: None,
                properties: vec![],
                additional_properties: None,
                shape_type: Some(string_type()),
                base_type: None,
            }),
        );

        let ty = Type::Reference {
            name: "StringAlias".into(),
            nullable: false,
            description: None,
        };

        let valid = validate(&json!("hello"), &ty, &registry);
        assert!(valid.is_empty());

        let invalid = validate(&json!(42), &ty, &registry);
        assert_eq!(invalid.len(), 1);
    }

    // ── Multiple errors accumulation ─────────────────────────────────────

    #[test]
    fn multiple_properties_invalid() {
        let props = vec![
            Property {
                name: "name".into(),
                ty: string_type(),
                required: true,
                description: None,
            },
            Property {
                name: "age".into(),
                ty: int_type(),
                required: true,
                description: None,
            },
        ];
        let mut errors = Vec::new();
        validate_object(
            &json!({"name": 123, "age": "old"}),
            &props,
            &SchemaRegistry::default(),
            "",
            &mut errors,
        );
        assert_eq!(errors.len(), 2);
    }

    // ── Edge cases ───────────────────────────────────────────────────────

    #[test]
    fn null_value_against_non_nullable_scalar() {
        let errors = validate(&json!(null), &string_type(), &SchemaRegistry::default());
        // Scalars don't have nullable flag directly; null is not a string
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn extra_properties_ignored() {
        let props = vec![Property {
            name: "name".into(),
            ty: string_type(),
            required: true,
            description: None,
        }];
        let mut errors = Vec::new();
        validate_object(
            &json!({"name": "ok", "extra": "ignored"}),
            &props,
            &SchemaRegistry::default(),
            "",
            &mut errors,
        );
        assert!(errors.is_empty());
    }
}
