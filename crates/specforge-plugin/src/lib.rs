use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedFile {
    pub path: String,
    pub content: String,
}

/// Result of plugin generation.
#[derive(Debug, Serialize, Deserialize)]
pub struct PluginResult {
    pub files: Vec<GeneratedFile>,
    pub errors: Vec<String>,
}

/// Trait that WASM plugins implement.
pub trait Plugin {
    fn generate(&self, ir_json: &str) -> PluginResult;
}

/// Macro to define a WASM plugin entry point.
/// Usage:
/// ```
/// use specforge_plugin::{Plugin, PluginResult, GeneratedFile};
///
/// struct MyPlugin;
/// impl Plugin for MyPlugin {
///     fn generate(&self, ir_json: &str) -> PluginResult {
///         // ...
///     }
/// }
///
/// specforge_plugin::export_plugin!(MyPlugin);
/// ```
#[macro_export]
macro_rules! export_plugin {
    ($plugin_type:ty) => {
        #[no_mangle]
        pub extern "C" fn generate(ptr: *const u8, len: usize) -> *mut u8 {
            let input = unsafe { std::slice::from_raw_parts(ptr, len) };
            let ir_json = std::str::from_utf8(input).unwrap_or("");
            let plugin = <$plugin_type>::default();
            let result = plugin.generate(ir_json);
            let output = serde_json::to_vec(&result).unwrap_or_default();
            let len = output.len();
            let ptr = output.as_ptr();
            std::mem::forget(output);
            // Return pointer and length packed (simplified)
            // In practice, use a shared memory protocol
            ptr as *mut u8
        }
    };
}
