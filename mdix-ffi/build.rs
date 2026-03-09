// mdix-ffi/build.rs
//
// Runs at compile time via csbindgen to generate the C# P/Invoke bindings.
// Every `cargo build -p mdix-ffi` regenerates MdixNative.cs automatically.
// The generated file goes into unity-package/Runtime/Generated/ — never edit it by hand.

fn main() {
    // Tell Cargo to re-run this script if lib.rs changes.
    // Without this, a change to lib.rs won't trigger MdixNative.cs regeneration.
    println!("cargo:rerun-if-changed=src/lib.rs");

    let out_path = "../unity-package/Runtime/Generated/MdixNative.cs";

    // Create the output directory if it does not exist yet.
    if let Some(parent) = std::path::Path::new(out_path).parent() {
        std::fs::create_dir_all(parent).expect("failed to create Generated/ directory");
    }

    csbindgen::Builder::default()
        // Parse our extern "C" functions from lib.rs.
        .input_extern_file("src/lib.rs")
        // The DLL name Unity will load at runtime.
        // On iOS this MUST be "__Internal" because iOS forbids dynamic library loading.
        // The #if directive is emitted verbatim into the generated C# file.
        .csharp_dll_name("mdix_ffi")
        .csharp_dll_name_if("UNITY_IOS && !UNITY_EDITOR", "__Internal")
        // Namespace and class that wrap all the generated P/Invoke declarations.
        .csharp_namespace("MidManStudio.DixScript.Native")
        .csharp_class_name("MdixNative")
        // false = emit delegate callbacks using [MonoPInvokeCallback], NOT C# 9
        // function pointers (delegate*). Unity's IL2CPP does not support the
        // modern delegate* syntax — this must be false for Unity targets.
        .csharp_use_function_pointer(false)
        .generate_csharp_file(out_path)
        .unwrap_or_else(|e| panic!("csbindgen failed: {}", e));
}
