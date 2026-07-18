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

    // NOT `.is_none()` -- easy to assume, wrong. `dlm_pipeline_executor.rs`'s
    // `generate_output_files()` only early-returns when `result.metadata` is
    // completely empty, and Phase 2 (compress) populates
    // `result.metadata["compressor"]` on its own, with no dependency on
    // whether an encryptor also ran. So `keyFileContent()` is `Some(...)`
    // for compression alone too -- it's really "DLM pipeline metadata
    // sidecar content", not strictly "the encryption key", despite the
    // name. The wasm `dlm.rs` doc comment's "`undefined` when source had no
    // `@DLM` modules" is about the `@DLM` section being absent entirely,
    // not about "`@DLM` present but no `DEncryptor`" -- two different
    // scenarios. Passing `""` here when a real, non-empty `keyFileContent`
    // is expected means the reverse pipeline's `instantiate_modules()` sees
    // no compression config in the (empty) key data, skips decompression
    // entirely, and tries to checksum-validate still-compressed bytes as if
    // they were the final AST bytes -- a checksum-mismatch failure that
    // only stays hidden for payloads with no enum fields to expose it.
    let key_content = outcome.keyFileContent().unwrap_or_default();
    assert!(
        !key_content.is_empty(),
        "{module_name}: a compressor ran, so keyFileContent should be populated \
         (with compression metadata, even though no DEncryptor ran)"
    );

    let modules = outcome.executedModules();
    assert!(
        modules.iter().any(|m| m.to_lowercase().contains("compressor")),
        "{module_name}: executedModules should mention the compressor: {:?}",
        modules
    );

    let db = decompile_with_dlm(outcome.processedData(), &key_content, &format!("dlm-{module_name}-only-test"))
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
    // Regression guard for a real bug found and fixed while adding this
    // test suite: this fixture has @DATA fields typed <enum> but zero
    // QuickFuncs anywhere in scope (no @QUICKFUNCS section, no imports).
    //
    // DixLoader::compile_source's Stage 7 (Runtime/loader.rs) used to only
    // run ValueResolver::resolve() -- which pre-resolves every
    // Value::EnumValue node to a plain integer -- when the AST had
    // QuickFuncs (local or imported). A file using enums with no QuickFuncs
    // anywhere skipped that stage entirely, so every enum field reached
    // BinaryPacker::pack() still as a raw, unresolved Value::EnumValue.
    // Compiler/Core/BinarySerialization/value_encoder.rs had no enums table
    // to resolve against at that point, so it logged a warning and silently
    // wrote a hardcoded Int32(0) -- no enum tag, indistinguishable from a
    // real value of 0 -- and get_enum_field() below would have correctly
    // rejected the round-tripped data, because by that point it genuinely
    // wasn't an enum anymore.
    //
    // Fixed in two places:
    //  - Runtime/loader.rs: Stage 7 now also gates on "has any enum in
    //    scope" (local or imported), not just "has any function in scope",
    //    so resolve_all_enum_values() always runs before packing whenever
    //    there's an enum to resolve.
    //  - Compiler/Core/BinarySerialization/value_encoder.rs: the
    //    EnumValue-reaches-the-encoder-unresolved case is now a hard
    //    error instead of a silent Int32(0) fallback, as defense-in-depth
    //    against this ever silently recurring through some other caller.
    let source = r#"
@ENUMS(
  Status { ACTIVE = 1, INACTIVE = 0 }
  Role   { ADMIN = 0, EDITOR = 1, VIEWER = 2 }
)
@DLM(DCompressor.bzip2)
@DATA(
  user:
    name = "Alice",
    status<enum> = Status.ACTIVE

  user.permissions::
    { role<enum> = Role.EDITOR, scope = "team" },
    { role<enum> = Role.ADMIN,  scope = "global" }
)
"#;

    let outcome = compile_with_dlm(source, "dlm-bzip2-nested-test")
        .expect("compileWithDlm should succeed with bzip2 over nested + enum data");

    // See the long comment in assert_compressor_round_trips() above --
    // compression alone still populates keyFileContent (it's pipeline
    // metadata, not strictly an encryption key), and decompile_with_dlm
    // needs that real value here, not "".
    let key_content = outcome.keyFileContent().unwrap_or_default();
    let db = decompile_with_dlm(outcome.processedData(), &key_content, "dlm-bzip2-nested-test")
        .expect("decompileWithDlm should reverse bzip2 compression over nested + enum data");

    assert_eq!(db.get_string("user.name").unwrap(), "Alice");
    assert_eq!(db.get_enum_field("user.status").unwrap(), "ACTIVE");
    assert_eq!(db.get_int("user.status").unwrap(), 1);
    assert_eq!(db.get_enum_field("user.permissions[0].role").unwrap(), "EDITOR");
    assert_eq!(db.get_string("user.permissions[1].scope").unwrap(), "global");
}
