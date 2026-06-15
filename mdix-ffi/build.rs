// mdix-ffi/build.rs
//
// Two code-generation steps run on every `cargo build -p mdix-ffi`:
//
// 1. csbindgen  → mdix-csharp/src/MidManStudio.Mdix.Core/Generated/MdixNative.cs
//    C# P/Invoke bindings consumed by the NuGet package and Unity plugin.
//
// 2. cbindgen   → mdix-go/internal/include/mdix_ffi.h
//    C header consumed by the Go package's cgo layer.
//    Also useful for any C/C++ consumer.
//
// Neither generated file is tracked in git.
// Run `cargo build -p mdix-ffi` before opening the .sln in Rider or
// running `go build ./...`.

fn main() {
    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=src/handle.rs");
    println!("cargo:rerun-if-changed=src/error.rs");
    println!("cargo:rerun-if-changed=src/string_utils.rs");
    println!("cargo:rerun-if-changed=cbindgen.toml");

    // ── Step 1: csbindgen → MdixNative.cs ────────────────────────────────────
    let cs_out = "../mdix-csharp/src/MidManStudio.Mdix.Core/Generated/MdixNative.cs";

    if let Some(parent) = std::path::Path::new(cs_out).parent() {
        std::fs::create_dir_all(parent).expect("failed to create Generated/ directory");
    }

    csbindgen::Builder::default()
        .input_extern_file("src/lib.rs")
        .csharp_dll_name("mdix_ffi")
        // iOS forbids runtime dynamic linking — Unity uses __Internal for static libs.
        .csharp_dll_name_if("UNITY_IOS && !UNITY_EDITOR", "__Internal")
        .csharp_namespace("MidManStudio.DixScript.Native")
        .csharp_class_name("MdixNative")
        .csharp_use_function_pointer(false)
        .generate_csharp_file(cs_out)
        .unwrap_or_else(|e| panic!("csbindgen failed: {}", e));

    // ── Step 2: cbindgen → mdix_ffi.h ────────────────────────────────────────
    let h_out = "../mdix-go/internal/include/mdix_ffi.h";

    if let Some(parent) = std::path::Path::new(h_out).parent() {
        std::fs::create_dir_all(parent).expect("failed to create mdix-go include/ directory");
    }

    let crate_dir = std::env::var("CARGO_MANIFEST_DIR")
        .expect("CARGO_MANIFEST_DIR not set");

    let config = cbindgen::Config::from_file("cbindgen.toml")
        .unwrap_or_else(|e| panic!("failed to read cbindgen.toml: {}", e));

    cbindgen::Builder::new()
        .with_crate(&crate_dir)
        .with_config(config)
        .generate()
        .unwrap_or_else(|e| panic!("cbindgen failed: {}", e))
        .write_to_file(h_out);

    println!("cargo:warning=Generated {}", h_out);
}
