//! The page the executable carries.
//!
//! `cargo xtask package onecalc` builds OneCalc's page, packs it ([`calc_platform_desktop::assets`])
//! and names the block to the build (`DNACALC_ONECALC_PAGE`, read by `build.rs`), which embeds it.
//! An everyday build carries no page and is given one with `--assets` or `--url`.

use calc_platform_desktop::assets::Assets;

#[cfg(embedded_page)]
static PAGE: &[u8] = include_bytes!(env!("ONECALC_PAGE_BLOCK"));
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
        "DNA OneCalc shows its window with Microsoft Edge WebView2, and the WebView2 Runtime is not installed on this computer.\n\nInstall the Evergreen runtime from\n{}\nand start DNA OneCalc again.",
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
        assert!(missing_runtime_message().contains("https://developer.microsoft.com/microsoft-edge/webview2"));
        if has_page() {
            let page = page().unwrap();
            assert!(page.get("index.html").is_some() && page.get("boot.js").is_some());
        } else {
            assert!(page().is_err());
        }
    }
}
