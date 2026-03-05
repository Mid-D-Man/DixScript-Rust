// mdix-cli/src/output/json_output.rs
//! Wraps any serializable result in a standard JSON envelope.
//!
//! All `--json` output goes through `print_result` so the shape is
//! consistent: `{ "success": bool, "data": T, "error": null | string }`.

use serde::Serialize;

/// Standard JSON response envelope.
#[derive(Serialize)]
pub struct JsonResponse<T: Serialize> {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Print a successful result as JSON to stdout.
pub fn print_result<T: Serialize>(data: T) {
    let response = JsonResponse {
        success: true,
        data: Some(data),
        error: None,
    };
    match serde_json::to_string_pretty(&response) {
        Ok(json) => println!("{}", json),
        Err(e)   => eprintln!("JSON serialization failed: {}", e),
    }
}

/// Print an error as JSON to stderr.
pub fn print_error(message: &str) {
    let response: JsonResponse<serde_json::Value> = JsonResponse {
        success: false,
        data: None,
        error: Some(message.to_string()),
    };
    match serde_json::to_string_pretty(&response) {
        Ok(json) => eprintln!("{}", json),
        Err(e)   => eprintln!("JSON serialization failed: {}", e),
    }
  }
