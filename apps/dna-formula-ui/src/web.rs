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
    // Where the wasm shadow stack started: a local's address in linear memory lies just below the
    // stack pointer the module began with, and the stack grows down from there. The page's start
    // script (boot.js) reads how deep the mount went from it (dc-0f1k.9).
    let marker = 0u8;
    let stack_top = std::hint::black_box(&marker) as *const u8 as usize;
    if let Some(root) = document.document_element() {
        let _ = root.set_attribute("data-dna-stack-top", &stack_top.to_string());
    }
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
/// Offer bytes as a browser download. A successful start does not confirm durable storage.
#[cfg(target_arch = "wasm32")]
pub fn download_bytes(name: &str, mime: &str, bytes: &[u8]) -> Result<(), String> {
    let parts = js_sys::Array::of1(&js_sys::Uint8Array::from(bytes));
    let options = web_sys::BlobPropertyBag::new();
    options.set_type(mime);
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &options)
        .map_err(|e| format!("{e:?}"))?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(|e| format!("{e:?}"))?;
    let document = web_sys::window()
        .and_then(|w| w.document())
        .ok_or("The browser document is unavailable")?;
    let anchor: web_sys::HtmlAnchorElement = document
        .create_element("a")
        .map_err(|e| format!("{e:?}"))?
        .dyn_into()
        .map_err(|_| "No download anchor".to_owned())?;
    anchor.set_href(&url);
    anchor.set_download(name);
    anchor.click();
    let _ = web_sys::Url::revoke_object_url(&url);
    Ok(())
}
#[cfg(not(target_arch = "wasm32"))]
pub fn download_bytes(_name: &str, _mime: &str, _bytes: &[u8]) -> Result<(), String> {
    Err("Browser downloads are unavailable on this target".into())
}

/// Read the selected local file; callers own validation and the decision to replace current work.
#[cfg(target_arch = "wasm32")]
pub fn read_chosen(
    input: web_sys::HtmlInputElement,
    limit: Option<usize>,
    chosen: impl FnOnce(Result<(String, Vec<u8>), String>) + 'static,
) {
    let Some(file) = input.files().and_then(|f| f.get(0)) else {
        return;
    };
    read_file(file, limit, move |read| {
        chosen(read);
        input.set_value("");
    });
}
#[cfg(not(target_arch = "wasm32"))]
pub fn read_chosen(
    _input: web_sys::HtmlInputElement,
    _limit: Option<usize>,
    _chosen: impl FnOnce(Result<(String, Vec<u8>), String>) + 'static,
) {
}

/// Read a file the page holds (chosen in an input, or dropped on the page).
#[cfg(target_arch = "wasm32")]
pub fn read_file(
    file: web_sys::File,
    limit: Option<usize>,
    chosen: impl FnOnce(Result<(String, Vec<u8>), String>) + 'static,
) {
    let name = file.name();
    if limit.is_some_and(|limit| file.size() > limit as f64) {
        chosen(Err("The selected file exceeds the supported size".into()));
        return;
    }
    wasm_bindgen_futures::spawn_local(async move {
        let bytes = wasm_bindgen_futures::JsFuture::from(file.array_buffer())
            .await
            .map(|b| js_sys::Uint8Array::new(&b).to_vec())
            .map_err(|e| format!("{name} could not be read: {e:?}"));
        chosen(bytes.map(|bytes| (name, bytes)));
    });
}
#[cfg(not(target_arch = "wasm32"))]
pub fn read_file(
    _file: web_sys::File,
    _limit: Option<usize>,
    _chosen: impl FnOnce(Result<(String, Vec<u8>), String>) + 'static,
) {
}

/// The bytes of the wasm instance's linear memory; `None` off the browser.
pub fn memory_bytes() -> Option<f64> {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;
        let memory: js_sys::WebAssembly::Memory = wasm_bindgen::memory().dyn_into().ok()?;
        let buffer: js_sys::ArrayBuffer = memory.buffer().dyn_into().ok()?;
        Some(f64::from(buffer.byte_length()))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        None
    }
}

/// Milliseconds on a clock: the browser's, or off the browser the time since the first call.
pub fn now_ms() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::now()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
        START
            .get_or_init(std::time::Instant::now)
            .elapsed()
            .as_secs_f64()
            * 1000.0
    }
}

/// The browser's local wall clock as a 1900-system Excel serial. The evaluator converts it
/// to the workbook's declared date system. Native embeddings supply their own clock.
pub fn serial_clock() -> Option<f64> {
    #[cfg(target_arch = "wasm32")]
    {
        let now = js_sys::Date::new_0();
        serial_from_local_ms(now.get_time() - now.get_timezone_offset() * 60_000.0)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        None
    }
}

#[cfg(any(test, target_arch = "wasm32"))]
fn serial_from_local_ms(local_ms: f64) -> Option<f64> {
    let serial = local_ms / 86_400_000.0 + 25_569.0;
    serial.is_finite().then_some(serial)
}

#[cfg(test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn local_clock_serial_retains_the_shared_epoch_and_refuses_missing_values() {
    for (ms, fraction) in [(0.0, 0.0), (43_200_000.0, 0.5)] {
        assert_eq!(serial_from_local_ms(ms), Some(25_569.0 + fraction));
    }
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(serial_from_local_ms(invalid), None);
    }
    #[cfg(not(target_arch = "wasm32"))]
    assert_eq!(serial_clock(), None);
}
