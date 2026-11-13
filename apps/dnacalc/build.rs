fn main() {
    // The same page block as WebView2, made by calc_platform_desktop::assets::pack.
    println!("cargo:rustc-check-cfg=cfg(embedded_page)");
    println!("cargo:rerun-if-env-changed=DNACALC_CALC_PAGE");
    if let Some(block) = std::env::var_os("DNACALC_CALC_PAGE") {
        let block = std::path::absolute(block).expect("the page block path");
        println!("cargo:rerun-if-changed={}", block.display());
        println!("cargo:rustc-cfg=embedded_page");
        println!("cargo:rustc-env=CALC_PAGE_BLOCK={}", block.display());
    }
}
