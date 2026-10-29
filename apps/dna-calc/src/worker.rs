//! The calculation worker (dc-p96): the same wasm build as the page, started in a dedicated module
//! worker by `web/worker.js`. The bundle's start function sees it has no window and calls
//! [`start`], which listens for [`ToWorker`] messages and answers each with a
//! [`FromWorker`] (see [`crate::wire`]).
//!
//! Following DnaTreeCalc's worker runtime (bf6dfd7, `src/dnatreecalc-web/src/worker_runtime.rs`:
//! context detection in the start function, a message handler kept for the worker's life, JSON
//! messages); the worker is started from its own script file rather than a blob, so it runs under
//! the site's `worker-src 'self'`.

#[cfg(target_arch = "wasm32")]
use crate::wire::{self, FromWorker, ToWorker};

/// Whether this wasm instance runs in a dedicated worker rather than a page.
pub fn in_worker() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;
        js_sys::global()
            .dyn_into::<web_sys::DedicatedWorkerGlobalScope>()
            .is_ok()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        false
    }
}

#[cfg(target_arch = "wasm32")]
fn scope() -> web_sys::DedicatedWorkerGlobalScope {
    use wasm_bindgen::JsCast;
    js_sys::global().unchecked_into()
}

#[cfg(target_arch = "wasm32")]
fn post(message: &FromWorker) {
    let _ = scope().post_message(&wasm_bindgen::JsValue::from_str(&wire::encode(message)));
}

/// Starts the worker: installs the message handler, then tells the page it is listening.
pub fn start() {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;
        use wasm_bindgen::prelude::Closure;
        std::panic::set_hook(Box::new(|info| {
            post(&FromWorker::Failed {
                id: None,
                message: format!("the worker panicked: {info}"),
            });
        }));
        let on_message = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(handle);
        scope().set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        on_message.forget();
        post(&FromWorker::Started);
    }
}

#[cfg(target_arch = "wasm32")]
fn handle(event: web_sys::MessageEvent) {
    let Some(text) = event.data().as_string() else {
        post(&FromWorker::Failed {
            id: None,
            message: "a message that is not text".to_owned(),
        });
        return;
    };
    match wire::decode::<ToWorker>(&text) {
        Ok(ToWorker { id, request }) => post(&FromWorker::Reply {
            id,
            reply: wire::serve(&request),
        }),
        Err(message) => post(&FromWorker::Failed { id: None, message }),
    }
}
