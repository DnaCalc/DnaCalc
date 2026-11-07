//! Dedicated workbook worker entry, loaded from its separate wasm module by worker.js.
//! JSON controls, binary tile fragments and transferred package bytes travel independently.
//!
//! Follows DnaTreeCalc bf6dfd7 `src/dnatreecalc-web/src/worker_runtime.rs`.

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

#[cfg(target_arch = "wasm32")]
fn post_event(event: wire::Event) -> bool {
    if let wire::Event::Tiles(batch) = event {
        match wire::encode_tiles(&batch) {
            Ok(bytes) => {
                let array = js_sys::Uint8Array::from(bytes.as_slice());
                let buffer = array.buffer();
                let pair = js_sys::Array::of2(&wire::TILE_TAG.into(), &buffer);
                if scope()
                    .post_message_with_transfer(&pair, &js_sys::Array::of1(&buffer))
                    .is_err()
                {
                    post(&FromServer::Failed {
                        id: None,
                        message: "Could not transfer tile fragment".into(),
                    });
                    return false;
                }
            }
            Err(message) => {
                post(&FromServer::Failed { id: None, message });
                return false;
            }
        }
    } else {
        post(&FromServer::Event(event));
    }
    true
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
    let Some((text, bytes)) = dna_calc_protocol::unpack(event.data()) else {
        post(&FromServer::Failed {
            id: None,
            message: "a message that is not text".to_owned(),
        });
        return;
    };
    let (reply, out, events) = SERVER.with(|server| {
        let mut server = server.borrow_mut();
        let server = server.get_or_insert_with(|| {
            let mut server = crate::server::Server::new();
            server.set_calculation_budget(calc_host::WorkBudget::evaluations(64));
            server.set_fetch_route(std::rc::Rc::new(BrowserFetch::default()));
            server
        });
        let (reply, out) = crate::server::answer_text(server, &text, bytes);
        (reply, out, server.take_events())
    });
    // The events first, as the in-process transport delivers them: a reply finds its
    // subscriptions' cells already applied.
    for event in events {
        if !post_event(event) {
            return;
        }
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
    schedule_calculation();
}

/// A timer queues a worker task. A chain of resolved promises would keep input behind the
/// calculation's microtasks and would not provide this handoff.
#[cfg(target_arch = "wasm32")]
fn schedule_calculation() {
    use wasm_bindgen::{JsCast, prelude::Closure};
    let pending = SERVER.with(|server| {
        server
            .borrow()
            .as_ref()
            .is_some_and(|server| server.has_pending_calculation())
    });
    if !pending || DRIVE_PENDING.with(|scheduled| scheduled.replace(true)) {
        return;
    }
    let callback = Closure::once_into_js(|| {
        DRIVE_PENDING.with(|scheduled| scheduled.set(false));
        let events = SERVER.with(|server| {
            let mut server = server.borrow_mut();
            let Some(server) = server.as_mut() else {
                return Vec::new();
            };
            server.advance_calculation();
            server.take_events()
        });
        for event in events {
            if !post_event(event) {
                return;
            }
        }
        schedule_calculation();
    });
    if scope()
        .set_timeout_with_callback_and_timeout_and_arguments_0(callback.unchecked_ref(), 0)
        .is_err()
    {
        DRIVE_PENDING.with(|scheduled| scheduled.set(false));
        post(&FromServer::Failed {
            id: None,
            message: "could not schedule the calculation continuation".into(),
        });
    }
}

/// WEBSERVICE and IMAGE through the worker's `fetch`. A cross-origin URL answers only when its
/// server allows it (CORS); anything the browser refuses is a failed fetch.
#[cfg(target_arch = "wasm32")]
#[derive(Debug, Default)]
struct BrowserFetch {
    next: std::cell::Cell<u64>,
}

#[cfg(target_arch = "wasm32")]
impl calc_func::FetchRoute for BrowserFetch {
    fn begin(&self, request: calc_func::FetchRequest) -> u64 {
        let id = self.next.get() + 1;
        self.next.set(id);
        wasm_bindgen_futures::spawn_local(async move {
            let reply = fetch_url(&request.url).await;
            let value = reply.into_value(request.kind);
            let events = SERVER.with(|server| {
                let mut server = server.borrow_mut();
                let Some(server) = server.as_mut() else {
                    return Vec::new();
                };
                server.complete_fetch(id, value);
                server.take_events()
            });
            for event in events {
                if !post_event(event) {
                    return;
                }
            }
            schedule_calculation();
        });
        id
    }
}

#[cfg(target_arch = "wasm32")]
async fn fetch_url(url: &str) -> calc_func::FetchReply {
    use calc_func::FetchReply;
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;
    let Ok(response) = JsFuture::from(scope().fetch_with_str(url)).await else {
        return FetchReply::Failed;
    };
    let Ok(response) = response.dyn_into::<web_sys::Response>() else {
        return FetchReply::Failed;
    };
    if response.status() >= 400 {
        return FetchReply::Status(response.status());
    }
    let Ok(promise) = response.array_buffer() else {
        return FetchReply::Failed;
    };
    match JsFuture::from(promise).await {
        Ok(buffer) => FetchReply::Body(js_sys::Uint8Array::new(&buffer).to_vec()),
        Err(_) => FetchReply::Failed,
    }
}

#[cfg(target_arch = "wasm32")]
thread_local! {
    static SERVER: std::cell::RefCell<Option<crate::server::Server>> = const { std::cell::RefCell::new(None) };
    static DRIVE_PENDING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
