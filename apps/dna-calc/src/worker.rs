//! The calculation worker (dc-p96): the same wasm build as the page, started in a dedicated module
//! worker by `web/worker.js`. The bundle's start function sees it has no window and calls
//! [`start`], which listens for [`crate::wire::ToServer`] messages and answers each with a
//! [`FromServer`] through its own [`crate::server::Server`] (see [`crate::wire`]).
//!
//! Following DnaTreeCalc's worker runtime (bf6dfd7, `src/dnatreecalc-web/src/worker_runtime.rs`:
//! context detection in the start function, a message handler kept for the worker's life, JSON
//! messages); the worker is started from its own script file rather than a blob, so it runs under
//! the site's `worker-src 'self'`.

#[cfg(target_arch = "wasm32")]
use crate::wire::{self, FromServer};

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
fn post(message: &FromServer) {
    let _ = scope().post_message(&wasm_bindgen::JsValue::from_str(&wire::encode(message)));
}

/// Starts the worker: installs the message handler, then tells the page it is listening.
pub fn start() {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;
        use wasm_bindgen::prelude::Closure;
        std::panic::set_hook(Box::new(|info| {
            post(&FromServer::Failed {
                id: None,
                message: format!("the worker panicked: {info}"),
            });
        }));
        let on_message = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(handle);
        scope().set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        on_message.forget();
        post(&FromServer::Started);
    }
}

#[cfg(target_arch = "wasm32")]
fn handle(event: web_sys::MessageEvent) {
    let Some((text, bytes)) = crate::port::unpack(event.data()) else {
        post(&FromServer::Failed {
            id: None,
            message: "a message that is not text".to_owned(),
        });
        return;
    };
    let (reply, out, events) = SERVER.with(|server| {
        let mut server = server.borrow_mut();
        let server = server.get_or_insert_with(crate::server::Server::new);
        let (reply, out) = crate::server::answer_text(server, &text, bytes);
        (reply, out, server.take_events())
    });
    // The events first, as the in-process transport delivers them: a reply finds its
    // subscriptions' cells already applied.
    for event in events {
        post(&FromServer::Event(event));
    }
    let message = wasm_bindgen::JsValue::from_str(&reply);
    match out {
        None => {
            let _ = scope().post_message(&message);
        }
        Some(bytes) => {
            // A saved package travels beside the reply, its buffer transferred.
            let array = js_sys::Uint8Array::from(bytes.as_slice());
            let buffer = array.buffer();
            let pair = js_sys::Array::of2(&message, &buffer);
            let _ = scope().post_message_with_transfer(&pair, &js_sys::Array::of1(&buffer));
        }
    }
}

#[cfg(target_arch = "wasm32")]
thread_local! {
    static SERVER: std::cell::RefCell<Option<crate::server::Server>> = const { std::cell::RefCell::new(None) };
}
