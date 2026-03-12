// mdix-ffi/build.rs
//
// Runs at compile time via csbindgen to generate the C# P/Invoke bindings.
// Every `cargo build -p mdix-ffi` regenerates MdixNative.cs automatically.
// The generated file goes into unity-package/Runtime/Generated/ — never edit it by hand.

fn main() {
    // Re-run this script whenever any file that contributes to the public
    // FFI surface changes. lib.rs is the primary surface, but error.rs and
    // handle.rs define types referenced by it — include them so a rename or
    // new type in those files also triggers regeneration.
    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=src/handle.rs");
    println!("cargo:rerun-if-changed=src/error.rs");
    println!("cargo:rerun-if-changed=src/string_utils.rs");

    let out_path = "../unity-package/Runtime/Generated/MdixNative.cs";

    if let Some(parent) = std::path::Path::new(out_path).parent() {
        std::fs::create_dir_all(parent).expect("failed to create Generated/ directory");
    }

    csbindgen::Builder::default()
        // lib.rs contains all #[no_mangle] extern "C" functions.
        // handle.rs contains the #[repr(C)] structs that csbindgen must see
        // in order to emit the MdixHandle and MdixBuilderHandle C# partial
        // struct definitions. Without this second input they are silently
        // skipped and the C# side fails to compile.
        .input_extern_file("src/lib.rs")
        .input_extern_file("src/handle.rs")
        // The DLL name Unity will load at runtime.
        // On iOS this MUST be "__Internal" because iOS forbids dynamic library loading.
        .csharp_dll_name("mdix_ffi")
        .csharp_dll_name_if("UNITY_IOS && !UNITY_EDITOR", "__Internal")
        .csharp_namespace("MidManStudio.DixScript.Native")
        .csharp_class_name("MdixNative")
        // false = emit delegate callbacks using [MonoPInvokeCallback], NOT C# 9
        // function pointers (delegate*). Unity's IL2CPP does not support delegate*
        // syntax — this must be false for Unity targets.
        .csharp_use_function_pointer(false)
        .generate_csharp_file(out_path)
        .unwrap_or_else(|e| panic!("csbindgen failed: {}", e));
}
