// mdix-wasm/src/error.rs
use wasm_bindgen::JsValue;

/// Converts any string message into a proper JS Error object.
/// This ensures the browser devtools show a real stack trace
/// rather than a plain string when a WASM call fails.
pub fn into_js_error(msg: &str) -> JsValue {
    let error = js_sys::Error::new(msg);
    error.into()
}

/// Converts a dixscript runtime error into a JS Error.
pub fn runtime_err(context: &str, detail: impl std::fmt::Display) -> JsValue {
    into_js_error(&format!("[mdix] {}: {}", context, detail))
}

/// Returned when a method is called on a database handle that has
/// already been freed (dropped via `free()`).
pub fn freed_err(type_name: &str) -> JsValue {
    into_js_error(&format!(
        "[mdix] {} has been freed and cannot be used.",
        type_name
    ))
}

/// Returned when a required path argument is null or empty.
pub fn invalid_path_err(path: &str) -> JsValue {
    into_js_error(&format!("[mdix] Path is null or empty: '{}'", path))
}
