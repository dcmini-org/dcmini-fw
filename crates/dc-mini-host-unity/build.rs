fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/lib.rs");

    csbindgen::Builder::default()
        .input_extern_file("src/lib.rs")
        .csharp_dll_name("dc_mini_host_unity")
        .csharp_dll_name_if("UNITY_IOS && !UNITY_EDITOR", "__Internal")
        .csharp_namespace("DcMini.Generated")
        .csharp_class_name("DcMiniNativeMethods")
        .csharp_class_accessibility("internal")
        .csharp_use_function_pointer(false)
        .csharp_use_nint_types(false)
        .csharp_generate_const_filter(|name| name.starts_with("DCMINI_"))
        .generate_csharp_file(
            "Assets/Scripts/DcMini/Generated/DcMiniNativeMethods.g.cs",
        )
        .expect("failed to generate C# bindings");
}
