fn main() {
    println!("cargo:rerun-if-changed=src/projection.cpp");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        cc::Build::new()
            .cpp(true)
            .std("c++17")
            .flag_if_supported("/EHsc")
            .file("src/projection.cpp")
            .compile("dna_com_projection");
        for library in ["ole32", "oleaut32", "oleacc", "uuid"] {
            println!("cargo:rustc-link-lib={library}");
        }
    }
}
