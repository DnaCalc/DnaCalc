//! The page's way to the workbook's server (dc-p96): [`call`] sends a [`Request`] and hands the
//! [`Reply`] to a callback; [`set_events`] takes the [`Event`]s the server pushes.
//!
//! In a browser the server runs in the calculation worker ([`crate::worker`]), started from
//! `worker.js` in the bundle's asset folder (a module worker from its own script, so it runs under
//! the site's `worker-src 'self'`): a call is posted to it and answered when the worker's reply
//! comes, so the page's main thread never waits for an open, an edit or a recalculation. Calls
//! made before the worker reports [`FromServer::Started`] wait for it. Off the browser (the
//! tests), or when the worker cannot start, the server runs in process on the page, and a call is
//! answered at once. Either way every message crosses as its JSON text ([`crate::wire`]); a
//! workbook's bytes travel beside it (in a worker, transferred, not copied), and the server's
//! events come before the reply they followed.
//!
//! The page records which way it took on the root element: `data-dnacalc-worker` is `starting`,
//! `ready` (with the first round trip in `data-dnacalc-worker-ms`), or `in process: <why>`.
//!
//! Following DnaTreeCalc's worker client (bf6dfd7, `src/dnatreecalc-web/src/worker_client.rs`:
//! the worker and its handler in a thread-local, an id per request, held until the worker reports
//! it started).

use crate::server::{self, Server};
use crate::wire::{self, Event, FromServer, Reply, Request, ToServer};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

type Answer = Box<dyn FnOnce(Result<Reply, String>, Option<Vec<u8>>)>;

#[cfg(target_arch = "wasm32")]
struct WorkerLink {
    worker: web_sys::Worker,
    started: bool,
    /// Messages sent before the worker started, in order, with the bytes beside them.
    waiting: Vec<(String, Option<Vec<u8>>)>,
    _on_message: wasm_bindgen::prelude::Closure<dyn FnMut(web_sys::MessageEvent)>,
}

enum Transport {
    InProcess(Box<Server>),
    #[cfg(target_arch = "wasm32")]
    Worker(WorkerLink),
}

struct Port {
    transport: Option<Transport>,
    next_id: u64,
    /// The callbacks waiting for the worker's replies, by request id.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    answers: BTreeMap<u64, Answer>,
    events: Option<Rc<dyn Fn(Event)>>,
}

thread_local! {
    static PORT: RefCell<Port> = RefCell::new(Port {
        transport: None,
        next_id: 1,
        answers: BTreeMap::new(),
        events: None,
    });
}

#[cfg(target_arch = "wasm32")]
fn mark(name: &str, value: &str) {
    if let Some(root) = leptos::prelude::document().document_element() {
        let _ = root.set_attribute(name, value);
    }
}

/// Starts the server once for the page: in the calculation worker in a browser, in process
/// otherwise.
pub fn start() {
    let started = PORT.with(|port| port.borrow().transport.is_some());
    if started {
        return;
    }
    #[cfg(target_arch = "wasm32")]
    {
        match start_worker() {
            Ok(link) => {
                PORT.with(|port| port.borrow_mut().transport = Some(Transport::Worker(link)));
                // A first round trip, timed, so the page can show the worker answers.
                let asked = crate::controller::now_ms();
                call(Request::Ping, None, move |reply, _| match reply {
                    Ok(Reply::Pong { .. }) => {
                        mark(
                            "data-dnacalc-worker-ms",
                            &format!("{:.1}", crate::controller::now_ms() - asked),
                        );
                        mark("data-dnacalc-worker", "ready");
                    }
                    Ok(_) => mark(
                        "data-dnacalc-worker",
                        "failed: a ping answered with something else",
                    ),
                    Err(e) => mark("data-dnacalc-worker", &format!("failed: {e}")),
                });
                return;
            }
            Err(why) => mark("data-dnacalc-worker", &format!("in process: {why}")),
        }
    }
    PORT.with(|port| {
        port.borrow_mut().transport = Some(Transport::InProcess(Box::default()));
    });
}

/// Whether the server runs in the calculation worker.
pub fn in_worker() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        PORT.with(|port| matches!(port.borrow().transport, Some(Transport::Worker(_))))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        false
    }
}

/// Where the server's events go (the controller's handler).
pub fn set_events(handler: impl Fn(Event) + 'static) {
    PORT.with(|port| port.borrow_mut().events = Some(Rc::new(handler)));
}

/// Sends `request` (with `bytes` beside it); `answer` gets the reply and the bytes beside it.
pub fn call(
    request: Request,
    bytes: Option<Vec<u8>>,
    answer: impl FnOnce(Result<Reply, String>, Option<Vec<u8>>) + 'static,
) {
    if PORT.with(|port| port.borrow().transport.is_none()) {
        start();
    }
    let answer: Answer = Box::new(answer);
    // The in-process server answers here, with the port released afterwards, so a callback may
    // call again.
    let served = PORT.with(|port| {
        let mut port = port.borrow_mut();
        let id = port.next_id;
        port.next_id += 1;
        let text = wire::encode(&ToServer { id, request });
        let Port {
            transport, answers, ..
        } = &mut *port;
        let _ = &answers;
        match transport.as_mut() {
            Some(Transport::InProcess(server)) => {
                let (reply, bytes) = server::answer_text(server, &text, bytes);
                let events: Vec<String> = server
                    .take_events()
                    .into_iter()
                    .map(|event| wire::encode(&FromServer::Event(event)))
                    .collect();
                let mut messages = events;
                messages.push(reply);
                Some((messages, bytes, answer))
            }
            #[cfg(target_arch = "wasm32")]
            Some(Transport::Worker(link)) => {
                answers.insert(id, answer);
                if link.started {
                    post(&link.worker, &text, bytes);
                } else {
                    link.waiting.push((text, bytes));
                }
                None
            }
            None => Some((Vec::new(), None, answer)),
        }
    });
    if let Some((messages, bytes, answer)) = served {
        let mut answer = Some(answer);
        let mut bytes = bytes;
        for text in messages {
            match wire::decode::<FromServer>(&text) {
                Ok(FromServer::Event(event)) => deliver(event),
                Ok(FromServer::Reply { reply, .. }) => {
                    if let Some(answer) = answer.take() {
                        answer(Ok(reply), bytes.take());
                    }
                }
                Ok(FromServer::Failed { message, .. }) => {
                    if let Some(answer) = answer.take() {
                        answer(Err(message), None);
                    }
                }
                Ok(FromServer::Started) => {}
                Err(message) => {
                    if let Some(answer) = answer.take() {
                        answer(Err(message), None);
                    }
                }
            }
        }
        if let Some(answer) = answer {
            answer(Err("the server is not running".to_owned()), None);
        }
    }
}

/// Hands an event to the handler, with the port released.
fn deliver(event: Event) {
    let handler = PORT.with(|port| port.borrow().events.clone());
    if let Some(handler) = handler {
        handler(event);
    }
}

#[cfg(target_arch = "wasm32")]
fn post(worker: &web_sys::Worker, text: &str, bytes: Option<Vec<u8>>) {
    let message = wasm_bindgen::JsValue::from_str(text);
    match bytes {
        None => {
            let _ = worker.post_message(&message);
        }
        Some(bytes) => {
            // The bytes travel beside the text, their buffer transferred rather than copied.
            let array = js_sys::Uint8Array::from(bytes.as_slice());
            let buffer = array.buffer();
            let pair = js_sys::Array::of2(&message, &buffer);
            let _ = worker.post_message_with_transfer(&pair, &js_sys::Array::of1(&buffer));
        }
    }
}

/// A message's text, and the bytes beside it (`[text, ArrayBuffer]`).
#[cfg(target_arch = "wasm32")]
pub fn unpack(data: wasm_bindgen::JsValue) -> Option<(String, Option<Vec<u8>>)> {
    if let Some(text) = data.as_string() {
        return Some((text, None));
    }
    let pair: js_sys::Array = wasm_bindgen::JsCast::dyn_into(data).ok()?;
    let text = pair.get(0).as_string()?;
    let bytes = js_sys::Uint8Array::new(&pair.get(1)).to_vec();
    Some((text, Some(bytes)))
}

#[cfg(target_arch = "wasm32")]
fn start_worker() -> Result<WorkerLink, String> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen::prelude::Closure;
    mark("data-dnacalc-worker", "starting");
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
        mark(
            "data-dnacalc-worker",
            &format!("failed: {}", event.message()),
        );
    });
    worker.set_onerror(Some(on_error.as_ref().unchecked_ref()));
    on_error.forget();
    Ok(WorkerLink {
        worker,
        started: false,
        waiting: Vec::new(),
        _on_message: on_message,
    })
}

#[cfg(target_arch = "wasm32")]
fn receive(event: web_sys::MessageEvent) {
    let Some((text, bytes)) = unpack(event.data()) else {
        return;
    };
    let message = match wire::decode::<FromServer>(&text) {
        Ok(message) => message,
        Err(e) => {
            mark("data-dnacalc-worker", &format!("failed: {e}"));
            return;
        }
    };
    match message {
        FromServer::Started => PORT.with(|port| {
            let mut port = port.borrow_mut();
            if let Some(Transport::Worker(link)) = port.transport.as_mut() {
                link.started = true;
                for (text, bytes) in link.waiting.drain(..) {
                    post(&link.worker, &text, bytes);
                }
            }
        }),
        FromServer::Event(event) => deliver(event),
        FromServer::Reply { id, reply } => {
            let answer = PORT.with(|port| port.borrow_mut().answers.remove(&id));
            if let Some(answer) = answer {
                answer(Ok(reply), bytes);
            }
        }
        FromServer::Failed {
            id: Some(id),
            message,
        } => {
            let answer = PORT.with(|port| port.borrow_mut().answers.remove(&id));
            if let Some(answer) = answer {
                answer(Err(message), None);
            }
        }
        FromServer::Failed { id: None, message } => {
            mark("data-dnacalc-worker", &format!("failed: {message}"));
        }
    }
}
