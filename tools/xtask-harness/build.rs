fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
            println!("cargo:rustc-link-arg-bin=xtask-harness=/EXPORT:MdCallBack12");
        } else {
            println!("cargo:rustc-link-arg-bin=xtask-harness=-Wl,--export-all-symbols");
        }
    }
}
