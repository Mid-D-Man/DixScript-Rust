// mdix-ffi/build.rs
//
// Runs at compile time via csbindgen to generate the C# P/Invoke bindings.
// Every `cargo build -p mdix-ffi` regenerates MdixNative.cs automatically.
// The generated file goes into unity-package/Runtime/Generated/ — never edit it by hand.
//
// IMPORTANT: handle.rs is intentionally NOT passed to csbindgen.
// csbindgen does not support opaque types — if it sees a #[repr(C)] struct
// with fields it cannot map to C#, it emits broken field declarations.
// MdixHandle and MdixBuilderHandle are opaque on the C# side (void*).
// lib.rs uses *mut c_void for all handle parameters and return values,
// casting internally. This is the documented csbindgen pattern for opaque types.

fn main() {
    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=src/handle.rs");
    println!("cargo:rerun-if-changed=src/error.rs");
    println!("cargo:rerun-if-changed=src/string_utils.rs");

    let out_path = "../unity-package/Runtime/Generated/MdixNative.cs";

    if let Some(parent) = std::path::Path::new(out_path).parent() {
        std::fs::create_dir_all(parent).expect("failed to create Generated/ directory");
    }

    csbindgen::Builder::default()
        // Only lib.rs — handle.rs is excluded because csbindgen cannot emit
        // opaque struct types. MdixHandle/MdixBuilderHandle are void* on the
        // C# side. See: https://github.com/Cysharp/csbindgen — Opaque Type section.
        .input_extern_file("src/lib.rs")
        .csharp_dll_name("mdix_ffi")
        .csharp_dll_name_if("UNITY_IOS && !UNITY_EDITOR", "__Internal")
        .csharp_namespace("MidManStudio.DixScript.Native")
        .csharp_class_name("MdixNative")
        // false = MonoPInvokeCallback delegates, NOT delegate* (required for Unity IL2CPP)
        .csharp_use_function_pointer(false)
        .generate_csharp_file(out_path)
        .unwrap_or_else(|e| panic!("csbindgen failed: {}", e));
}
