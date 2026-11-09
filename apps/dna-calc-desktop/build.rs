include!("version-resource.rs");

fn main() {
    link_executable_identity("dna-calc-desktop");
    // `cargo xtask package onecalc` packs DNA Calc's built page and names the block here: the
    // executable then carries its page. Without it (everyday builds) the page comes from `--assets`,
    // `--url` or a `page` folder beside the executable.
    println!("cargo:rustc-check-cfg=cfg(embedded_page)");
    println!("cargo:rerun-if-env-changed=DNACALC_CALC_PAGE");
    if let Some(block) = std::env::var_os("DNACALC_CALC_PAGE") {
        let block = std::path::absolute(block).expect("the page block path");
        println!("cargo:rerun-if-changed={}", block.display());
        println!("cargo:rustc-cfg=embedded_page");
        println!("cargo:rustc-env=CALC_PAGE_BLOCK={}", block.display());
        // The packaged executable is a window program: no console opens beside it.
        println!("cargo:rustc-link-arg-bin=dna-calc-desktop=/SUBSYSTEM:WINDOWS");
        println!("cargo:rustc-link-arg-bin=dna-calc-desktop=/ENTRY:mainCRTStartup");
    }
}
