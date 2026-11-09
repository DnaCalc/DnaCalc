//! The page the executable carries.
//!
//! `cargo xtask package onecalc` builds DNA Calc's page too, packs it ([`calc_platform_desktop::assets`])
//! and names the block to the build (`DNACALC_CALC_PAGE`, read by `build.rs`), which embeds it: the
//! companion then runs from the packaged folder with no `--assets`, `--url` or `page` folder, which is how
//! OneCalc's "Open in DNA Calc" starts it. An everyday build carries no page and is given one with
//! `--assets` or `--url`, or finds a `page` folder beside the executable.

use calc_platform_desktop::assets::Assets;

#[cfg(embedded_page)]
static PAGE: &[u8] = include_bytes!(env!("CALC_PAGE_BLOCK"));
#[cfg(not(embedded_page))]
static PAGE: &[u8] = &[];

/// Whether this build carries its page.
pub fn has_page() -> bool {
    !PAGE.is_empty()
}

/// The embedded page's files.
pub fn page() -> Result<Assets, String> {
    if !has_page() {
        return Err("this build carries no page; give --assets DIR or --url URL".to_owned());
    }
    Assets::parse(PAGE)
}

/// What the person is told when WebView2 is not installed: what it is, why it is needed and where
/// to get it.
pub fn missing_runtime_message() -> String {
    format!(
        "DNA Calc shows its window with Microsoft Edge WebView2, and the WebView2 Runtime is not installed on this computer.\n\nInstall the Evergreen runtime from\n{}\nand start DNA Calc again.",
        calc_platform_desktop::RUNTIME_DOWNLOAD
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Native-only: the message names the download page; and a build that embeds a page can read
    /// it back (an everyday build has none).
    #[test]
    fn the_message_has_the_link_and_an_embedded_page_reads() {
        assert!(
            missing_runtime_message()
                .contains("https://developer.microsoft.com/microsoft-edge/webview2")
        );
        if has_page() {
            let page = page().unwrap();
            for name in [
                "index.html",
                "boot.js",
                "dna-calc-web.js",
                "dna-calc-web_bg.wasm",
                "worker.js",
                "dna-calc-worker-web.js",
                "dna-calc-worker-web_bg.wasm",
            ] {
                assert!(page.get(name).is_some(), "{name}");
            }
        } else {
            assert!(page().is_err());
        }
    }
}
