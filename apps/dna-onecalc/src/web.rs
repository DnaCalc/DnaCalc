//! The browser entry: what the bundle's start function runs (`src/bin/dna-onecalc-web.rs`), in a
//! page laid out like `web/index.html`. It is the start both apps share
//! ([`dna_formula_ui::web::start_page`]: the panic hook that writes the message on the root
//! element, then the mount) with DNA OneCalc's [`ONECALC`] page, so the browser smoke test
//! (`cargo xtask app smoke onecalc`) fails on any panic.
//!
//! It compiles on every target and runs only in a browser.

use crate::{skin0, surface};
use dna_formula_ui::web::{Page, start_page};

/// DNA OneCalc's page.
pub const ONECALC: Page = Page {
    name: "DNA Calc - Formula Explorer",
    panic_attribute: "data-onecalc-panic",
    ready_attribute: "data-onecalc-ready",
    host_id: "onecalc",
};

/// Starts DNA OneCalc: its surface lives as long as the page.
pub fn start() {
    start_page(&ONECALC, |host| {
        let legacy = web_sys::window()
            .and_then(|window| window.location().search().ok())
            .is_some_and(|search| {
                search
                    .trim_start_matches('?')
                    .split('&')
                    .any(|part| part == "skin=legacy")
            });
        if legacy {
            std::mem::forget(surface::mount(host));
        } else {
            skin0::mount(host);
        }
    });
}
