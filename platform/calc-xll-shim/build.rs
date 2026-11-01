fn main() {
    println!("cargo:rerun-if-changed=src/variadic.c");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        cc::Build::new()
            .file("src/variadic.c")
            .warnings(true)
            .compile("xlcall_variadic");
        // Pull the C variadic exports into the cdylib; stable Rust cannot define C varargs.
        for export in ["Excel12", "Excel4", "Excel4v"] {
            println!("cargo:rustc-cdylib-link-arg=/EXPORT:{export}");
            println!("cargo:rustc-cdylib-link-arg=/INCLUDE:{export}");
        }
    }
}
