//! Native OpenAPI 3.1 JSON Schema model.
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Schema31 {
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "type")]
    pub ty: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "$schema")]
    pub schema: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub all_of: Option<Vec<Schema31>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub one_of: Option<Vec<Schema31>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub any_of: Option<Vec<Schema31>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub not: Option<Box<Schema31>>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "if")]
    pub if_: Option<Box<Schema31>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub then: Option<Box<Schema31>>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "else")]
    pub else_: Option<Box<Schema31>>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "$ref")]
    pub r#ref: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "$dynamicRef"
    )]
    pub dynamic_ref: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "$dynamicAnchor"
    )]
    pub dynamic_anchor: Option<String>,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub properties: IndexMap<String, Box<Schema31>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub items: Option<Box<Schema31>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix_items: Option<Vec<Schema31>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exclusive_minimum: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exclusive_maximum: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty", rename = "enum")]
    pub enum_values: Vec<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#const: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unevaluated_items: Option<Box<Schema31>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unevaluated_properties: Option<Box<Schema31>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dependent_required: Option<IndexMap<String, Vec<String>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dependent_schemas: Option<IndexMap<String, Box<Schema31>>>,
    #[serde(flatten)]
    pub extensions: IndexMap<String, serde_json::Value>,
}

pub fn type_list(schema: &Schema31) -> Vec<String> {
    match &schema.ty {
        None => Vec::new(),
        Some(serde_json::Value::String(s)) => vec![s.clone()],
        Some(serde_json::Value::Array(arr)) => arr
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect(),
        _ => Vec::new(),
    }
}
pub fn is_nullable(schema: &Schema31) -> bool {
    type_list(schema).iter().any(|t| t == "null")
}
impl Schema31 {
    pub fn from_json(value: &serde_json::Value) -> Result<Self, serde_json::Error> {
        serde_json::from_value(value.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deserialize_type_array() {
        let json = serde_json::json!({"type": ["string", "null"]});
        assert!(is_nullable(&Schema31::from_json(&json).unwrap()));
    }
    #[test]
    fn roundtrip() {
        let json =
            serde_json::json!({"type": ["string", "null"], "const": "yes", "x-custom": "value"});
        let s = serde_json::to_value(Schema31::from_json(&json).unwrap()).unwrap();
        assert_eq!(s["type"], serde_json::json!(["string", "null"]));
    }
}
