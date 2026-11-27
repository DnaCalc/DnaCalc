//! The pages of dnacalc.com the applications link to.
//!
//! An application names a page here and never spells a site URL itself. The site build
//! (`cargo xtask site pack content`) refuses to pack when a page listed here is not built, so a
//! link in a shipped application cannot point at a page the site lacks.

/// The public content host.
pub const SITE: &str = "https://dnacalc.com";
/// The Handbook of Excel Functions.
pub const HANDBOOK: &str = "https://handbook.dnacalc.com/catalogue/index.html";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SitePage {
    Home,
    /// The five views and what each shows today.
    Views,
    /// VBA, XLL, COM and Office.js hosting.
    Automation,
    /// The calculation engine, the runner and the crates.
    Engine,
    /// Windows, the browser and Linux, with what each host can do.
    Platforms,
    /// What an agent can read and change.
    Agents,
    /// The Formula Explorer product page.
    Explorer,
    /// The workbook application in the browser.
    App,
    Download,
    /// The measurements against Excel.
    Status,
    ClassicGridGuide,
}

impl SitePage {
    pub const ALL: [Self; 11] = [
        Self::Home,
        Self::Views,
        Self::Automation,
        Self::Engine,
        Self::Platforms,
        Self::Agents,
        Self::Explorer,
        Self::App,
        Self::Download,
        Self::Status,
        Self::ClassicGridGuide,
    ];

    /// The path on the content host, starting and ending with `/`.
    pub fn route(self) -> &'static str {
        match self {
            Self::Home => "/",
            Self::Views => "/views/",
            Self::Automation => "/automation/",
            Self::Engine => "/engine/",
            Self::Platforms => "/platforms/",
            Self::Agents => "/agents/",
            Self::Explorer => "/onecalc/",
            Self::App => "/app/",
            Self::Download => "/download/",
            Self::Status => "/status/",
            Self::ClassicGridGuide => "/help/classic-grid/",
        }
    }

    pub fn url(self) -> String {
        format!("{SITE}{}", self.route())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_arch = "wasm32")]
    use wasm_bindgen_test::wasm_bindgen_test;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    fn routes_are_distinct_site_paths() {
        let mut seen = std::collections::BTreeSet::new();
        for page in SitePage::ALL {
            let route = page.route();
            assert!(route.starts_with('/') && route.ends_with('/'), "{route}");
            assert!(seen.insert(route), "{route} listed twice");
            assert_eq!(page.url(), format!("https://dnacalc.com{route}"));
        }
    }
}
