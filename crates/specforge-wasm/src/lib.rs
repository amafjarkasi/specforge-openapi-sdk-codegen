use specforge_core::{parse_str, resolve};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn parse_and_resolve(yaml_or_json: &str) -> Result<String, JsValue> {
    let spec =
        parse_str(yaml_or_json).map_err(|e| JsValue::from_str(&format!("parse error: {e}")))?;
    let doc = resolve(&spec).map_err(|e| JsValue::from_str(&format!("resolve error: {e}")))?;
    serde_json::to_string_pretty(&doc)
        .map_err(|e| JsValue::from_str(&format!("serialize error: {e}")))
}

#[wasm_bindgen]
pub fn lint(yaml_or_json: &str) -> Result<String, JsValue> {
    let spec =
        parse_str(yaml_or_json).map_err(|e| JsValue::from_str(&format!("parse error: {e}")))?;
    let doc = resolve(&spec).map_err(|e| JsValue::from_str(&format!("resolve error: {e}")))?;
    let diagnostics = specforge_core::lint::lint(&doc);
    serde_json::to_string(&diagnostics)
        .map_err(|e| JsValue::from_str(&format!("serialize error: {e}")))
}
