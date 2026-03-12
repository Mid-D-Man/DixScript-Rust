// mdix-ffi/build.rs
//
// Generates MdixNative.cs directly into the Core NuGet project's source tree.
// Core is self-contained — it has no dependency on the Unity package.
// The Unity package is a downstream consumer that receives a pre-built Core.dll.
//
// Output: csharp/src/MidManStudio.Mdix.Core/Generated/MdixNative.cs
//
// Run `cargo build -p mdix-ffi` before opening the .sln in Rider.
// Without it, Rider will show a compile error because Generated/MdixNative.cs
// does not exist yet.
//
// IMPORTANT: handle.rs is intentionally NOT passed to csbindgen.
// csbindgen cannot emit opaque struct types. MdixHandle and MdixBuilderHandle
// are void* on the C# side. lib.rs uses *mut c_void for all handle parameters,
// casting internally — the documented csbindgen pattern for opaque types.

fn main() {
    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=src/handle.rs");
    println!("cargo:rerun-if-changed=src/error.rs");
    println!("cargo:rerun-if-changed=src/string_utils.rs");

    let out_path = "../csharp/src/MidManStudio.Mdix.Core/Generated/MdixNative.cs";

    if let Some(parent) = std::path::Path::new(out_path).parent() {
        std::fs::create_dir_all(parent).expect("failed to create Generated/ directory");
    }

    csbindgen::Builder::default()
        .input_extern_file("src/lib.rs")
        .csharp_dll_name("mdix_ffi")
        .csharp_dll_name_if("UNITY_IOS && !UNITY_EDITOR", "__Internal")
        .csharp_namespace("MidManStudio.DixScript.Native")
        .csharp_class_name("MdixNative")
        .csharp_use_function_pointer(false)
        .generate_csharp_file(out_path)
        .unwrap_or_else(|e| panic!("csbindgen failed: {}", e));
}
