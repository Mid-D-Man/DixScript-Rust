// mdix-wasm/src/dlm.rs
//
// DLM (compress / encrypt / audit) support for JS/TS — thin bindings over
// dixscript::Runtime::DixLoader::compile_with_dlm_from_str /
// decompile_with_dlm_from_bytes.
//
// Everything here is in-memory, same reasoning as merge.rs: wasm32-
// unknown-unknown has no real filesystem, so `.mdix.enc`/`.mdix.key`
// content never touches disk from inside wasm. The audit trail
// (`@DLM(DAuditor...)`) is backed by browser `localStorage` instead of a
// file when running on wasm32 — same pattern as the cloud-import cache —
// so audit history still persists across compiles within the same
// origin, it's just keyed by `sourceLabel` instead of a path on disk.
//
// ```js
// const source = `
//   @DLM(DCompressor.xz, DEncryptor.aes256)
//   @DATA(secret = "shh")
// `;
//
// const outcome = compileWithDlm(source, "my-config");
// if (!outcome.isSuccess()) throw new Error(outcome.errors().join("; "));
//
// const encryptedBytes = outcome.processedData();  // Uint8Array
// const keyFileContent = outcome.keyFileContent();  // string | undefined
//
// // ... store/send encryptedBytes + keyFileContent however you like ...
//
// const db = decompileWithDlm(encryptedBytes, keyFileContent, "my-config");
// db.getString("secret"); // "shh"
// ```
//
// If `source` has no `@DLM(...)` section at all, `compileWithDlm` still
// succeeds — `processedData()` is just the plain binary-packed AST with
// no compression/encryption applied, `keyFileContent()` is `undefined`,
// and `decompileWithDlm` should be called with an empty string for
// `keyFileContent` to match (it unpacks directly rather than attempting
// decryption when given one).

use wasm_bindgen::prelude::*;
use dixscript::Runtime::DixLoader;

use crate::database::MdixDatabase;
use crate::error::runtime_err;

#[wasm_bindgen]
pub struct MdixDlmOutcome {
    is_success:        bool,
    processed_data:     Vec<u8>,
    key_file_content:  Option<String>,
    executed_modules:  Vec<String>,
    errors:            Vec<String>,
    warnings:          Vec<String>,
}

#[wasm_bindgen]
impl MdixDlmOutcome {
    pub fn isSuccess(&self) -> bool {
        self.is_success
    }

    /// The compressed/encrypted (or, with no `@DLM` modules, plain
    /// binary-packed) bytes — always populated in memory regardless of
    /// whether any on-disk artifact could be written (never possible on
    /// wasm32 in the first place).
    pub fn processedData(&self) -> Vec<u8> {
        self.processed_data.clone()
    }

    /// The `.mdix.key` file's content as a plain string, ready to hand
    /// straight to `decompileWithDlm`. `undefined` when `source` had no
    /// `@DLM` modules to apply (nothing to decrypt on the way back
    /// either — see the module doc comment above).
    pub fn keyFileContent(&self) -> Option<String> {
        self.key_file_content.clone()
    }

    /// Which DLM modules actually ran, e.g. `["DCompressor.xz",
    /// "DEncryptor.aes256"]` — empty when `source` had no `@DLM` section.
    pub fn executedModules(&self) -> Vec<String> {
        self.executed_modules.clone()
    }

    pub fn errors(&self) -> Vec<String> {
        self.errors.clone()
    }

    pub fn warnings(&self) -> Vec<String> {
        self.warnings.clone()
    }
}

/// Compiles `source` and, if it declares an `@DLM(DCompressor...
/// DEncryptor...)` section, runs compression/encryption on the result —
/// entirely in memory. `sourceLabel` is just an identifier used for
/// error messages and (if `@DLM` includes `DAuditor`) as the audit
/// trail's localStorage key — it doesn't need to be a real file name,
/// though using one consistently is what makes the audit trail track a
/// given config's history across compiles.
#[wasm_bindgen(js_name = compileWithDlm)]
pub fn compile_with_dlm(source: &str, source_label: &str) -> Result<MdixDlmOutcome, JsValue> {
    if source.trim().is_empty() {
        return Err(runtime_err("compileWithDlm", "source is empty"));
    }

    let loader = DixLoader::new();
    let result = loader
        .compile_with_dlm_from_str(source, source_label)
        .map_err(|e| runtime_err("compileWithDlm", e))?;

    Ok(MdixDlmOutcome {
        is_success:       result.is_success,
        processed_data:   result.processed_data,
        key_file_content: result.key_file_content,
        executed_modules: result.executed_modules,
        errors:           result.errors,
        warnings:         result.warnings,
    })
}

/// Reverse of `compileWithDlm`: takes the bytes from `processedData()`
/// and the string from `keyFileContent()` and returns a normal
/// `MdixDatabase`, exactly as if you'd `loadStr()`'d the original source.
///
/// Pass `""` for `keyFileContent` when the original `compileWithDlm` call
/// returned `undefined` for it (source had no `@DLM` modules) — this then
/// unpacks `data` directly rather than attempting decryption.
#[wasm_bindgen(js_name = decompileWithDlm)]
pub fn decompile_with_dlm(
    data:              Vec<u8>,
    key_file_content:  &str,
    source_label:      &str,
) -> Result<MdixDatabase, JsValue> {
    if data.is_empty() {
        return Err(runtime_err("decompileWithDlm", "data is empty"));
    }

    let loader = DixLoader::new();
    let dix_data = loader
        .decompile_with_dlm_from_bytes(data, key_file_content, source_label)
        .map_err(|e| runtime_err("decompileWithDlm", e))?;

    Ok(MdixDatabase::from_data(dix_data))
}
