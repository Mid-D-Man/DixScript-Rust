//! DLM compressor coverage — all three `DCompressor` subtypes (gzip,
//! bzip2, lzma), each round-tripped standalone AND paired with an
//! encryptor, specifically on the wasm32 target.
//!
//! tests/web.rs has exactly one DLM compression test
//! (`compile_with_dlm_round_trips_with_compression_and_encryption`), and
//! it only ever exercises `DCompressor.gzip`. bzip2 and lzma have zero
//! wasm32 coverage, despite `Compiler/DLM/Compressor/mod.rs` explicitly
//! documenting all three as pure Rust and building "on every target:
//! wasm32-unknown-unknown, wasm32-wasip2, Android, iOS, Windows." Both
//! are feature-gated on the dixscript side (`bzip2-support`, `xz-support` -- the Cargo feature is still named xz-support, only the @DLM source keyword is `lzma`)
//! — both happen to be in dixscript's `default` feature set already, so
//! mdix-wasm gets them for free via the plain `dixscript = { path =
//! "../dixscript" }` dependency with no extra feature wiring needed — but
//! "compiles in" and "actually round-trips correctly through the
//! in-memory wasm32 DLM pipeline" are two different claims, and only
//! gzip's had a test proving the second one.

use wasm_bindgen_test::*;
use mdix_wasm::{compile_with_dlm, decompile_with_dlm};

wasm_bindgen_test_configure!(run_in_browser);

/// Repetitive enough that a real compressor should shrink it noticeably —
/// a few bytes of "shh" wouldn't tell you if a compressor silently
/// no-op'd and just passed bytes through unchanged.
const REPETITIVE_PHRASE: &str = "the quick brown fox jumps over the lazy dog. ";

fn repeated_payload(times: usize) -> String {
    REPETITIVE_PHRASE.repeat(times)
}

// ── standalone compression, no encryptor — isolates each compressor ───────

fn assert_compressor_round_trips(module_name: &str, payload: &str) {
    let source = format!(
        "@DLM(DCompressor.{module_name})\n@DATA(\n  secret = \"{payload}\",\n  count  = 42\n)\n"
    );

    let outcome = compile_with_dlm(&source, &format!("dlm-{module_name}-only-test"))
        .unwrap_or_else(|_| panic!("compileWithDlm should succeed with {module_name} alone"));

    assert!(outcome.isSuccess(), "{module_name}: DLM pipeline should report success");
    assert!(!outcome.processedData().is_empty(), "{module_name}: processedData should be non-empty");
    assert!(
        outcome.keyFileContent().is_none(),
        "{module_name}: no DEncryptor ran, keyFileContent should stay undefined"
    );

    let modules = outcome.executedModules();
    assert!(
        modules.iter().any(|m| m.to_lowercase().contains("compressor")),
        "{module_name}: executedModules should mention the compressor: {:?}",
        modules
    );

    // Compression-only round trip decompiles with an empty keyFileContent,
    // per decompile_with_dlm's own documented contract for "no encryptor".
    let db = decompile_with_dlm(outcome.processedData(), "", &format!("dlm-{module_name}-only-test"))
        .unwrap_or_else(|_| panic!("decompileWithDlm should reverse {module_name}-only compression"));

    assert_eq!(db.get_string("secret").unwrap(), payload);
    assert_eq!(db.get_int("count").unwrap(), 42);
}

#[wasm_bindgen_test]
fn gzip_alone_round_trips() {
    assert_compressor_round_trips("gzip", &repeated_payload(50));
}

#[wasm_bindgen_test]
fn bzip2_alone_round_trips() {
    assert_compressor_round_trips("bzip2", &repeated_payload(50));
}

#[wasm_bindgen_test]
fn lzma_alone_round_trips() {
    assert_compressor_round_trips("lzma", &repeated_payload(50));
}

// ── compression + encryption paired — mirrors web.rs's existing gzip
//    coverage, extended to the other two compressors (gzip+aes256 itself
//    is already covered there, not duplicated here) ───────────────────────

fn assert_compressor_round_trips_with_encryption(module_name: &str, payload: &str) {
    let source = format!(
        "@DLM(DCompressor.{module_name}, DEncryptor.aes256)\n@DATA(\n  secret = \"{payload}\",\n  count  = 7\n)\n"
    );

    let outcome = compile_with_dlm(&source, &format!("dlm-{module_name}-aes256-test"))
        .unwrap_or_else(|_| panic!("compileWithDlm should succeed with {module_name} + aes256"));

    assert!(outcome.isSuccess());
    assert!(!outcome.processedData().is_empty());
    assert!(
        outcome.keyFileContent().is_some(),
        "{module_name}: keyFileContent should be populated when DEncryptor ran"
    );

    let modules = outcome.executedModules();
    assert!(modules.iter().any(|m| m.to_lowercase().contains("compressor")));
    assert!(modules.iter().any(|m| m.to_lowercase().contains("encryptor")));

    let key_content = outcome.keyFileContent().unwrap();
    let db = decompile_with_dlm(
        outcome.processedData(),
        &key_content,
        &format!("dlm-{module_name}-aes256-test"),
    )
    .unwrap_or_else(|_| panic!("decompileWithDlm should reverse {module_name} + aes256"));

    assert_eq!(db.get_string("secret").unwrap(), payload);
    assert_eq!(db.get_int("count").unwrap(), 7);
}

#[wasm_bindgen_test]
fn bzip2_plus_encryption_round_trips() {
    assert_compressor_round_trips_with_encryption("bzip2", &repeated_payload(50));
}

#[wasm_bindgen_test]
fn lzma_plus_encryption_round_trips() {
    assert_compressor_round_trips_with_encryption("lzma", &repeated_payload(50));
}

// ── sanity check: compression should actually shrink the data, not just
//    round-trip a no-op passthrough ────────────────────────────────────────

#[wasm_bindgen_test]
fn each_compressor_actually_shrinks_the_packed_data() {
    let payload = repeated_payload(200); // ~9KB, highly repetitive

    let plain_source = format!("@DATA(secret = \"{payload}\")\n");
    let baseline = compile_with_dlm(&plain_source, "dlm-shrink-baseline")
        .expect("compileWithDlm should succeed with no @DLM section");
    let baseline_len = baseline.processedData().len();

    for module_name in ["gzip", "bzip2", "lzma"] {
        let source = format!("@DLM(DCompressor.{module_name})\n@DATA(secret = \"{payload}\")\n");
        let outcome = compile_with_dlm(&source, &format!("dlm-{module_name}-shrink-test"))
            .unwrap_or_else(|_| panic!("compileWithDlm should succeed with {module_name}"));

        let compressed_len = outcome.processedData().len();
        assert!(
            compressed_len < baseline_len,
            "{module_name}: compressed ({compressed_len} bytes) should be smaller than \
             the uncompressed packed baseline ({baseline_len} bytes) for a highly \
             repetitive payload — a no-op passthrough would fail this"
        );
    }
}

#[wasm_bindgen_test]
fn top_level_and_nested_fields_still_resolve_correctly_under_compression() {
    // Regression guard: compression sits between the AST and the bytes
    // that get shipped around, so this is the spot a subtle
    // compress/decompress bug (off-by-one in a length prefix, a corrupted
    // dictionary reset, etc.) would surface as silently wrong data rather
    // than a hard error.
    let source = r#"
@DLM(DCompressor.bzip2)
@DATA(
  user:
    name = "Alice",
    status<enum> = Status.ACTIVE

  user.permissions::
    { role<enum> = Role.EDITOR, scope = "team" },
    { role<enum> = Role.ADMIN,  scope = "global" }
)

@ENUMS(
  Status { ACTIVE = 1, INACTIVE = 0 }
  Role   { ADMIN = 0, EDITOR = 1, VIEWER = 2 }
)
"#;

    let outcome = compile_with_dlm(source, "dlm-bzip2-nested-test")
        .expect("compileWithDlm should succeed with bzip2 over nested + enum data");
    let db = decompile_with_dlm(outcome.processedData(), "", "dlm-bzip2-nested-test")
        .expect("decompileWithDlm should reverse bzip2 compression over nested + enum data");

    assert_eq!(db.get_string("user.name").unwrap(), "Alice");
    assert_eq!(db.get_enum_field("user.status").unwrap(), "ACTIVE");
    assert_eq!(db.get_int("user.status").unwrap(), 1);
    assert_eq!(db.get_enum_field("user.permissions[0].role").unwrap(), "EDITOR");
    assert_eq!(db.get_string("user.permissions[1].scope").unwrap(), "global");
}
