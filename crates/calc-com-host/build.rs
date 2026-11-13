#[path = "../calc-object-model/src/descriptor.rs"]
#[allow(dead_code)]
mod descriptor;
mod generate;

fn main() {
    generate::write();
    println!("cargo:rerun-if-changed=src/projection.cpp");
    println!("cargo:rerun-if-changed=src/projection-office.h");
    println!("cargo:rerun-if-changed=src/rtd.cpp");
    println!("cargo:rerun-if-changed=src/value.h");
    println!("cargo:rerun-if-changed=src/rtd-interfaces.h");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        cc::Build::new()
            .cpp(true)
            .std("c++17")
            .flag_if_supported("/EHsc")
            .file("src/projection.cpp")
            .file("src/rtd.cpp")
            .compile("dna_com_projection");
        for library in ["ole32", "oleaut32", "oleacc", "uuid", "advapi32"] {
            println!("cargo:rustc-link-lib={library}");
        }
    }
}
