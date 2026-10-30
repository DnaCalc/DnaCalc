//! The browser start both apps' entries run, each with its own [`Page`] (DNA OneCalc's
//! `dna_onecalc::web`, DNA Calc's `dna_calc::web`), in a page laid out like the app's
//! `web/index.html`.
//!
//! [`start_page`] installs a panic hook and mounts an application. The hook writes the panic
//! message to the page's root element (the page's [`Page::panic_attribute`]) and to the console
//! before the wasm instance traps. A panic in a reactive memo, effect or view surfaces in the
//! browser only as a trap (`RuntimeError: unreachable`), which a page can miss; with the message on
//! the root element, and the page's own `error` and `unhandledrejection` listeners
//! (`web/boot.js`) recording any trap, the browser smoke test (`cargo xtask app smoke`) fails on
//! either.
//!
//! It compiles on every target and runs only in a browser.

use wasm_bindgen::{JsCast, JsValue};
use web_sys::HtmlElement;

/// How an application's page reports itself.
pub struct Page {
    /// The application's name in the console.
    pub name: &'static str,
    /// The attribute of the page's root element that records a panic's message.
    pub panic_attribute: &'static str,
    /// The attribute of the page's root element set once the application is mounted.
    pub ready_attribute: &'static str,
    /// The id of the element the application mounts in; the page's body when there is none.
    pub host_id: &'static str,
}

/// Installs `page`'s panic hook, then mounts the application in its host element with `mount`.
pub fn start_page(page: &'static Page, mount: impl FnOnce(HtmlElement)) {
    std::panic::set_hook(Box::new(move |info| {
        let message = info.to_string();
        if let Some(root) = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.document_element())
        {
            let _ = root.set_attribute(page.panic_attribute, &message);
        }
        web_sys::console::error_1(&JsValue::from_str(&format!(
            "{} panicked: {message}",
            page.name
        )));
    }));
    let document = web_sys::window()
        .and_then(|window| window.document())
        .expect("the page has a document");
    let host: HtmlElement = match document.get_element_by_id(page.host_id) {
        Some(element) => element
            .dyn_into()
            .expect("the host element is an HTML element"),
        None => document.body().expect("the page has a body"),
    };
    mount(host);
    if let Some(root) = document.document_element() {
        let _ = root.set_attribute(page.ready_attribute, "true");
    }
}
