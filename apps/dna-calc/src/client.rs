//! The page's side of the calculation worker (dc-p96): starts the worker from `worker.js` in the
//! bundle's asset folder, holds requests until it reports [`FromWorker::Started`], sends each as a
//! [`ToWorker`] and hands each answer to the callback that asked.
//!
//! Following DnaTreeCalc's worker client (bf6dfd7, `src/dnatreecalc-web/src/worker_client.rs`:
//! the worker and its handler in a thread-local, an id per request); the worker is a module
//! worker from its own script file, not a blob, so it runs under the site's `worker-src 'self'`.
//! The page records the worker's state on the root element (`data-dnacalc-worker`: `starting`,
//! `ready` or `failed: ...`, and the first round trip in `data-dnacalc-worker-ms`).

#[cfg(target_arch = "wasm32")]
use crate::wire::{self, FromWorker, Reply, Request, ToWorker};
#[cfg(target_arch = "wasm32")]
use std::cell::RefCell;
#[cfg(target_arch = "wasm32")]
use std::collections::BTreeMap;

#[cfg(target_arch = "wasm32")]
type Answer = Box<dyn FnOnce(Result<Reply, String>)>;

#[cfg(target_arch = "wasm32")]
struct Client {
    worker: web_sys::Worker,
    started: bool,
    next_id: u64,
    /// Requests made before the worker started, in order.
    waiting: Vec<ToWorker>,
    answers: BTreeMap<u64, Answer>,
    _on_message: wasm_bindgen::prelude::Closure<dyn FnMut(web_sys::MessageEvent)>,
}

#[cfg(target_arch = "wasm32")]
thread_local! {
    static CLIENT: RefCell<Option<Client>> = const { RefCell::new(None) };
}

#[cfg(target_arch = "wasm32")]
fn mark(state: &str) {
    if let Some(root) = leptos::prelude::document().document_element() {
        let _ = root.set_attribute("data-dnacalc-worker", state);
    }
}

/// Starts the calculation worker and times a first round trip to it.
pub fn spawn() {
    #[cfg(target_arch = "wasm32")]
    {
        if let Err(e) = start_worker() {
            mark(&format!("failed: {e}"));
            return;
        }
        let started = crate::controller::now_ms();
        request(Request::Ping, move |answer| match answer {
            Ok(Reply::Pong { .. }) => {
                if let Some(root) = leptos::prelude::document().document_element() {
                    let _ = root.set_attribute(
                        "data-dnacalc-worker-ms",
                        &format!("{:.1}", crate::controller::now_ms() - started),
                    );
                }
                mark("ready");
            }
            Err(e) => mark(&format!("failed: {e}")),
        });
    }
}

#[cfg(target_arch = "wasm32")]
fn start_worker() -> Result<(), String> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen::prelude::Closure;
    mark("starting");
    // boot.js records the folder its assets came from; worker.js is beside it.
    let assets = js_sys::Reflect::get(&js_sys::global(), &"__dnacalcAssets".into())
        .ok()
        .and_then(|value| value.as_string())
        .ok_or("the page did not record its asset folder")?;
    let url = web_sys::Url::new_with_base("worker.js", &assets)
        .map_err(|e| format!("no worker URL: {e:?}"))?
        .href();
    let options = web_sys::WorkerOptions::new();
    options.set_type(web_sys::WorkerType::Module);
    let worker = web_sys::Worker::new_with_options(&url, &options)
        .map_err(|e| format!("the worker did not start: {e:?}"))?;
    let on_message = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(receive);
    worker.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    let on_error = Closure::<dyn FnMut(web_sys::ErrorEvent)>::new(|event: web_sys::ErrorEvent| {
        mark(&format!("failed: {}", event.message()));
    });
    worker.set_onerror(Some(on_error.as_ref().unchecked_ref()));
    on_error.forget();
    CLIENT.with(|client| {
        *client.borrow_mut() = Some(Client {
            worker,
            started: false,
            next_id: 1,
            waiting: Vec::new(),
            answers: BTreeMap::new(),
            _on_message: on_message,
        });
    });
    Ok(())
}

/// Sends `request` to the worker; `answer` gets its reply (or why there is none).
#[cfg(target_arch = "wasm32")]
pub fn request(request: Request, answer: impl FnOnce(Result<Reply, String>) + 'static) {
    let refused = CLIENT.with(|client| {
        let mut client = client.borrow_mut();
        let Some(client) = client.as_mut() else {
            return Some(answer);
        };
        let id = client.next_id;
        client.next_id += 1;
        client.answers.insert(id, Box::new(answer));
        let message = ToWorker { id, request };
        if client.started {
            let _ = client
                .worker
                .post_message(&wasm_bindgen::JsValue::from_str(&wire::encode(&message)));
        } else {
            client.waiting.push(message);
        }
        None
    });
    if let Some(answer) = refused {
        answer(Err("the worker is not running".to_owned()));
    }
}

#[cfg(target_arch = "wasm32")]
fn receive(event: web_sys::MessageEvent) {
    let Some(text) = event.data().as_string() else {
        return;
    };
    let message = match wire::decode::<FromWorker>(&text) {
        Ok(message) => message,
        Err(e) => {
            mark(&format!("failed: {e}"));
            return;
        }
    };
    let answer = CLIENT.with(|client| {
        let mut client = client.borrow_mut();
        let client = client.as_mut()?;
        match message {
            FromWorker::Started => {
                client.started = true;
                for waiting in client.waiting.drain(..) {
                    let _ = client
                        .worker
                        .post_message(&wasm_bindgen::JsValue::from_str(&wire::encode(&waiting)));
                }
                None
            }
            FromWorker::Reply { id, reply } => client.answers.remove(&id).map(|a| (a, Ok(reply))),
            FromWorker::Failed {
                id: Some(id),
                message,
            } => client.answers.remove(&id).map(|a| (a, Err(message))),
            FromWorker::Failed { id: None, message } => {
                mark(&format!("failed: {message}"));
                None
            }
        }
    });
    // The callback runs with the client released, so it may send another request.
    if let Some((answer, result)) = answer {
        answer(result);
    }
}
