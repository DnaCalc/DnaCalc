//! The browser entry: what the bundle's start function runs (`src/bin/dna-calc-web.rs`), in a page
//! laid out like `web/index.html`. It is DNA OneCalc's entry ([`dna_onecalc::web::start_page`]:
//! the panic hook that writes the message on the root element, then the mount) with DNA Calc's
//! [`PAGE`], so the browser smoke test (`cargo xtask app smoke calc`) fails on any panic.

use crate::app;
use dna_onecalc::web::{Page, start_page};

/// DNA Calc's page.
pub const PAGE: Page = Page {
    name: "DNA Calc",
    panic_attribute: "data-dnacalc-panic",
    ready_attribute: "data-dnacalc-ready",
    host_id: "dnacalc",
};

/// Starts DNA Calc.
pub fn start() {
    start_page(&PAGE, app::mount);
}
