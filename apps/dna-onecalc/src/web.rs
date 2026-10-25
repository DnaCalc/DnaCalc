//! The browser entry: what the bundle's start function runs (`src/bin/dna-onecalc-web.rs`), in a
//! page laid out like `web/index.html`.
//!
//! [`start`] installs a panic hook and mounts the surface. The hook writes the panic message to the
//! page's root element (`data-onecalc-panic`) and to the console before the wasm instance traps.
//! A panic in a reactive memo, effect or view surfaces in the browser only as a trap
//! (`RuntimeError: unreachable`), which a page can miss; with the message on the root element, and
//! the page's own `error` and `unhandledrejection` listeners (`web/index.html`) recording any trap,
//! the browser smoke test (`cargo xtask app smoke onecalc`) fails on either.
//!
//! It compiles on every target and runs only in a browser.

use crate::surface;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::HtmlElement;

/// The attribute of the page's root element that records a panic's message.
pub const PANIC_ATTRIBUTE: &str = "data-onecalc-panic";

/// The attribute of the page's root element set once the surface is mounted.
pub const READY_ATTRIBUTE: &str = "data-onecalc-ready";

/// The id of the element the surface mounts in; the page's body when there is none.
pub const HOST_ID: &str = "onecalc";

/// Installs the panic hook, then mounts the surface in the host element for the life of the page.
pub fn start() {
    std::panic::set_hook(Box::new(|info| {
        let message = info.to_string();
        if let Some(root) = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.document_element())
        {
            let _ = root.set_attribute(PANIC_ATTRIBUTE, &message);
        }
        web_sys::console::error_1(&JsValue::from_str(&format!(
            "DNA OneCalc panicked: {message}"
        )));
    }));
    let document = web_sys::window()
        .and_then(|window| window.document())
        .expect("the page has a document");
    let host: HtmlElement = match document.get_element_by_id(HOST_ID) {
        Some(element) => element
            .dyn_into()
            .expect("the host element is an HTML element"),
        None => document.body().expect("the page has a body"),
    };
    // The surface lives as long as the page.
    std::mem::forget(surface::mount(host));
    if let Some(root) = document.document_element() {
        let _ = root.set_attribute(READY_ATTRIBUTE, "true");
    }
}
