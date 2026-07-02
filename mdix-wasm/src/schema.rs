// mdix-wasm/src/schema.rs
//
// MdixSchema / MdixValidationReport — schema validation for JS/TS.
//
// Thin bindings over dixscript::Runtime::schema, the same core module
// mdix-lua's and mdix-python's schema.rs wrap. Custom validators
// (`require_with` / `optional_with` in the Rust core) are not exposed
// here: they take a `Fn(&DixData) -> Result<(), String> + Send + Sync +
// 'static` closure, and a JS function crossing that boundary safely is
// its own separate piece of work. The named require_*/optional_* methods
// cover the overwhelming majority of real schema use.
//
// Simpler than the Lua/Python equivalents: those need an
// `Option<SchemaBuilder>` "take, mutate, put back" dance because their
// host languages don't have Rust's by-value `self` consuming-builder
// semantics. wasm-bindgen DOES — `mut self -> Self` here is the exact
// same pattern MdixBuilder already uses in builder.rs, so MdixSchema
// just stores `SchemaBuilder` directly with no wrapper and no "already
// consumed" error case to handle.
//
// ```js
// const schema = new MdixSchema()
//   .requireString("app_name")
//   .requireInt("port")
//   .requireLong("created_at_ms")
//   .optionalBool("debug");
//
// const report = db.validateSchema(schema);
// if (!report.isValid) {
//   console.log(report.toString());
//   for (const path of report.failedPaths()) console.log("failed:", path);
// }
// ```

use wasm_bindgen::prelude::*;
use dixscript::Runtime::{SchemaBuilder, ValidationReport};

// ── MdixSchema ─────────────────────────────────────────────────────────────────

#[wasm_bindgen]
pub struct MdixSchema {
    pub(crate) inner: SchemaBuilder,
}

#[wasm_bindgen]
impl MdixSchema {
    #[wasm_bindgen(constructor)]
    pub fn new() -> MdixSchema {
        MdixSchema { inner: SchemaBuilder::new() }
    }

    // ── required ─────────────────────────────────────────────────────────

    #[wasm_bindgen(js_name = requireString)]
    pub fn require_string(mut self, path: &str) -> MdixSchema {
        self.inner = self.inner.require_string(path);
        self
    }
    #[wasm_bindgen(js_name = requireInt)]
    pub fn require_int(mut self, path: &str) -> MdixSchema {
        self.inner = self.inner.require_int(path);
        self
    }
    /// Requires a 64-bit integer field. Also accepts Int values (an i32
    /// widens into the i64 field with no precision loss).
    #[wasm_bindgen(js_name = requireLong)]
    pub fn require_long(mut self, path: &str) -> MdixSchema {
        self.inner = self.inner.require_long(path);
        self
    }
    #[wasm_bindgen(js_name = requireFloat)]
    pub fn require_float(mut self, path: &str) -> MdixSchema {
        self.inner = self.inner.require_float(path);
        self
    }
    #[wasm_bindgen(js_name = requireDouble)]
    pub fn require_double(mut self, path: &str) -> MdixSchema {
        self.inner = self.inner.require_double(path);
        self
    }
    #[wasm_bindgen(js_name = requireBool)]
    pub fn require_bool(mut self, path: &str) -> MdixSchema {
        self.inner = self.inner.require_bool(path);
        self
    }
    #[wasm_bindgen(js_name = requireArray)]
    pub fn require_array(mut self, path: &str) -> MdixSchema {
        self.inner = self.inner.require_array(path);
        self
    }
    #[wasm_bindgen(js_name = requireObject)]
    pub fn require_object(mut self, path: &str) -> MdixSchema {
        self.inner = self.inner.require_object(path);
        self
    }
    #[wasm_bindgen(js_name = requireEnum)]
    pub fn require_enum(mut self, path: &str) -> MdixSchema {
        self.inner = self.inner.require_enum(path);
        self
    }

    // ── optional ─────────────────────────────────────────────────────────

    #[wasm_bindgen(js_name = optionalString)]
    pub fn optional_string(mut self, path: &str) -> MdixSchema {
        self.inner = self.inner.optional_string(path);
        self
    }
    #[wasm_bindgen(js_name = optionalInt)]
    pub fn optional_int(mut self, path: &str) -> MdixSchema {
        self.inner = self.inner.optional_int(path);
        self
    }
    #[wasm_bindgen(js_name = optionalLong)]
    pub fn optional_long(mut self, path: &str) -> MdixSchema {
        self.inner = self.inner.optional_long(path);
        self
    }
    #[wasm_bindgen(js_name = optionalFloat)]
    pub fn optional_float(mut self, path: &str) -> MdixSchema {
        self.inner = self.inner.optional_float(path);
        self
    }
    #[wasm_bindgen(js_name = optionalDouble)]
    pub fn optional_double(mut self, path: &str) -> MdixSchema {
        self.inner = self.inner.optional_double(path);
        self
    }
    #[wasm_bindgen(js_name = optionalBool)]
    pub fn optional_bool(mut self, path: &str) -> MdixSchema {
        self.inner = self.inner.optional_bool(path);
        self
    }
    #[wasm_bindgen(js_name = optionalArray)]
    pub fn optional_array(mut self, path: &str) -> MdixSchema {
        self.inner = self.inner.optional_array(path);
        self
    }
    #[wasm_bindgen(js_name = optionalObject)]
    pub fn optional_object(mut self, path: &str) -> MdixSchema {
        self.inner = self.inner.optional_object(path);
        self
    }

    // ── metadata ─────────────────────────────────────────────────────────

    /// Annotates the most recently added field with a description.
    #[wasm_bindgen(js_name = withDescription)]
    pub fn with_description(mut self, description: &str) -> MdixSchema {
        self.inner = self.inner.with_description(description);
        self
    }

    #[wasm_bindgen(getter, js_name = fieldCount)]
    pub fn field_count(&self) -> i32 {
        self.inner.field_count() as i32
    }

    pub fn paths(&self) -> Vec<String> {
        self.inner.paths().into_iter().map(String::from).collect()
    }
}

impl Default for MdixSchema {
    fn default() -> Self {
        MdixSchema::new()
    }
}

// ── MdixValidationReport ──────────────────────────────────────────────────────

/// Returned by `MdixDatabase.validateSchema`.
#[wasm_bindgen]
pub struct MdixValidationReport {
    inner: ValidationReport,
}

impl MdixValidationReport {
    pub(crate) fn new(report: ValidationReport) -> Self {
        MdixValidationReport { inner: report }
    }
}

#[wasm_bindgen]
impl MdixValidationReport {
    #[wasm_bindgen(getter, js_name = isValid)]
    pub fn is_valid(&self) -> bool {
        self.inner.is_valid()
    }

    #[wasm_bindgen(getter, js_name = errorCount)]
    pub fn error_count(&self) -> i32 {
        self.inner.error_count() as i32
    }

    /// Dotted paths that failed validation, in order.
    #[wasm_bindgen(js_name = failedPaths)]
    pub fn failed_paths(&self) -> Vec<String> {
        self.inner.failed_paths().into_iter().map(String::from).collect()
    }

    /// All errors as a real JS array of plain objects:
    /// `{path, expected, actual, kind}` where kind is one of
    /// "Missing" | "WrongType" | "InvalidValue". Built via
    /// `js_sys::JSON::parse` over a hand-built JSON string rather than
    /// requiring `ValidationError`/`ValidationErrorKind` to derive
    /// `Serialize` in the core (they don't, and adding that derive purely
    /// for this one binding's convenience isn't worth the core-wide change).
    pub fn errors(&self) -> Result<JsValue, JsValue> {
        let arr: Vec<serde_json::Value> = self.inner.errors.iter().map(|e| {
            serde_json::json!({
                "path":     e.path,
                "expected": e.expected,
                "actual":   e.actual,
                "kind":     e.kind.to_string(),
            })
        }).collect();
        let json_str = serde_json::to_string(&arr)
            .map_err(|e| JsValue::from_str(&format!("[mdix] errors serialize failed: {}", e)))?;
        js_sys::JSON::parse(&json_str)
    }

    /// Human-readable multi-line summary. Mapped to `toString` so
    /// `String(report)` / template-literal interpolation work naturally.
    #[wasm_bindgen(js_name = toString)]
    pub fn to_js_string(&self) -> String {
        self.inner.to_string()
    }
}
