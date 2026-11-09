//! Browser worker transport for the page. The worker has its own module and wasm binary.
//! Calls made before startup wait in order; a failed worker reports a reason and releases
//! pending answers. A failed connection is retained because restarting would lose the workbook.
//! Headless composition is injected through `WorkbookTransport` from dna-calc-worker.
//!
//! Follows DnaTreeCalc bf6dfd7 `src/dnatreecalc-web/src/worker_client.rs`.

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

pub use dna_calc_model::Health;

enum Transport {
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    Failed(String),
    #[cfg(target_arch = "wasm32")]
    Worker(WorkerLink),
}

struct Port {
    transport: Option<Transport>,
    next_id: u64,
    commands: u64,
    /// The callbacks waiting for the worker's replies, by request id.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    answers: BTreeMap<u64, Answer>,
    events: Option<Rc<dyn Fn(Event)>>,
    health: Health,
    observer: Option<Rc<dyn Fn(Health)>>,
}

thread_local! {
    static PORT: RefCell<Port> = RefCell::new(Port {
        transport: None,
        next_id: 1,
        commands: 0,
        answers: BTreeMap::new(),
        events: None,
        health: Health::default(),
        observer: None,
    });
}

/// Observes transport state and outstanding messages, outside the port borrow.
pub fn set_health(handler: impl Fn(Health) + 'static) {
    PORT.with(|port| port.borrow_mut().observer = Some(Rc::new(handler)));
    notify_health(None);
}

fn notify_health(state: Option<String>) {
    let (handler, health) = PORT.with(|port| {
        let mut port = port.borrow_mut();
        if let Some(state) = state {
            port.health.state = state;
        }
        port.health.pending = port.answers.len();
        (port.observer.clone(), port.health.clone())
    });
    if let Some(handler) = handler {
        handler(health);
    }
}

#[cfg(target_arch = "wasm32")]
fn fail(message: String) {
    let answers = PORT.with(|port| {
        let mut port = port.borrow_mut();
        // Keep a failed transport: restarting here would lose the workbook silently.
        if let Some(Transport::Worker(link)) = port.transport.as_ref() {
            link.worker.set_onmessage(None);
            link.worker.set_onerror(None);
            link.worker.terminate();
        }
        port.transport = Some(Transport::Failed(message.clone()));
        std::mem::take(&mut port.answers)
    });
    mark("data-dnacalc-worker", &format!("failed: {message}"));
    notify_health(Some(format!(
        "Worker failed: {message}. Keep this page open; reload starts a new session."
    )));
    for (_, answer) in answers {
        answer(Err(message.clone()), None);
    }
}

#[cfg(target_arch = "wasm32")]
fn mark(name: &str, value: &str) {
    if let Some(root) = leptos::prelude::document().document_element() {
        let _ = root.set_attribute(name, value);
    }
}

/// Starts the page's browser worker once. Other targets require an injected transport.
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
                        notify_health(Some("Worker ready".to_owned()));
                    }
                    Ok(_) => mark(
                        "data-dnacalc-worker",
                        "failed: a ping answered with something else",
                    ),
                    Err(e) => mark("data-dnacalc-worker", &format!("failed: {e}")),
                });
                return;
            }
            Err(why) => {
                fail(why);
                return;
            }
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        PORT.with(|port| {
            port.borrow_mut().transport = Some(Transport::Failed(
                "The browser transport needs a worker; headless callers inject a transport".into(),
            ))
        });
        notify_health(Some("Browser worker unavailable on this target".into()));
    }
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
    #[cfg(target_arch = "wasm32")]
    if let Request::TileAck(barrier) = &request {
        let barrier = *barrier;
        let sent = PORT.with(|port| {
            let mut port = port.borrow_mut();
            let id = port.next_id;
            port.next_id = id.checked_add(1).expect("Transport identities exhausted");
            let text = wire::encode(&ToServer {
                id,
                request: Request::TileAck(barrier),
            });
            match port.transport.as_ref() {
                Some(Transport::Worker(link)) if link.started => post(&link.worker, &text, None),
                Some(Transport::Failed(message)) => Err(message.clone()),
                _ => Err("A tile acknowledgement arrived before the worker started".into()),
            }
        });
        // There is at most one ACK per flight/subscription. It cannot wait for an answer
        // slot held by ordinary queries: doing so could strand every later publication.
        // The worker's no-op reply is ignored; local success acknowledges the send only.
        match sent {
            Ok(()) => answer(Ok(Reply::Subscribed), None),
            Err(message) => {
                fail(message.clone());
                answer(Err(message), None);
            }
        }
        return;
    }
    let answer: Answer = Box::new(answer);
    // Build each message while holding the port, then release it before invoking an answer.
    let served = PORT.with(|port| {
        let mut port = port.borrow_mut();
        let control = matches!(
            &request,
            Request::TileAck(_) | Request::ResetTiles { .. } | Request::Unsubscribe { .. }
        );
        // Keep room for acknowledgements and recovery while normal requests fill their budget.
        let limit = wire::MAX_PENDING_REQUESTS
            - if control {
                0
            } else {
                2 * wire::MAX_SUBSCRIPTIONS
            };
        if port.answers.len() >= limit {
            return Some((
                vec![wire::encode(&FromServer::Failed {
                    id: None,
                    message: "The workbook request queue is full; wait for the current operation"
                        .into(),
                })],
                None,
                answer,
            ));
        }
        if matches!(&request, Request::Command(_)) || request.is_command() {
            port.commands = port
                .commands
                .checked_add(1)
                .expect("Transport command count exhausted");
        }
        let id = port.next_id;
        port.next_id = port
            .next_id
            .checked_add(1)
            .expect("Transport identities exhausted");
        let text = wire::encode(&ToServer { id, request });
        let Port {
            transport, answers, ..
        } = &mut *port;
        let _ = &answers;
        match transport.as_mut() {
            #[cfg(target_arch = "wasm32")]
            Some(Transport::Worker(link)) => {
                answers.insert(id, answer);
                if link.started {
                    if let Err(message) = post(&link.worker, &text, bytes) {
                        // Defer until the current port borrow is released.
                        leptos::prelude::set_timeout(
                            move || fail(message),
                            std::time::Duration::ZERO,
                        );
                    }
                } else {
                    link.waiting.push((text, bytes));
                }
                None
            }
            Some(Transport::Failed(message)) => Some((
                vec![wire::encode(&FromServer::Failed {
                    id: Some(id),
                    message: message.clone(),
                })],
                None,
                answer,
            )),
            None => Some((Vec::new(), None, answer)),
        }
    });
    notify_health(None);
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
fn post(worker: &web_sys::Worker, text: &str, bytes: Option<Vec<u8>>) -> Result<(), String> {
    let message = wasm_bindgen::JsValue::from_str(text);
    match bytes {
        None => worker
            .post_message(&message)
            .map_err(|error| format!("Could not send workbook request: {error:?}")),
        Some(bytes) => {
            // The bytes travel beside the text, their buffer transferred rather than copied.
            let array = js_sys::Uint8Array::from(bytes.as_slice());
            let buffer = array.buffer();
            let pair = js_sys::Array::of2(&message, &buffer);
            worker
                .post_message_with_transfer(&pair, &js_sys::Array::of1(&buffer))
                .map_err(|error| format!("Could not send workbook bytes: {error:?}"))
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use dna_calc_protocol::unpack;

#[cfg(target_arch = "wasm32")]
fn start_worker() -> Result<WorkerLink, String> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen::prelude::Closure;
    mark("data-dnacalc-worker", "starting");
    notify_health(Some("Worker starting".to_owned()));
    // boot.js records the folder its assets came from; worker.js is beside it.
    let assets = js_sys::Reflect::get(&js_sys::global(), &"__dnacalcAssets".into())
        .ok()
        .and_then(|value| value.as_string())
        .ok_or("the page did not record its asset folder")?;
    let url = web_sys::Url::new_with_base("worker.js", &assets)
        .map_err(|e| format!("no worker URL: {e:?}"))?;
    // Regional overrides belong to the workbook worker as well as its page.
    url.set_search(
        &leptos::prelude::window()
            .location()
            .search()
            .unwrap_or_default(),
    );
    let url = url.href();
    let options = web_sys::WorkerOptions::new();
    options.set_type(web_sys::WorkerType::Module);
    let worker = web_sys::Worker::new_with_options(&url, &options)
        .map_err(|e| format!("the worker did not start: {e:?}"))?;
    let on_message = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(receive);
    worker.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    // A script that throws gives an ErrorEvent with a message; a script that cannot be fetched
    // gives a plain Event of the same type with none, and reading `message()` on it threw a
    // TypeError inside this handler instead of reporting the failure (found in a Chromium
    // smoke run, dc-0f1k.11.3).
    let on_error = Closure::<dyn FnMut(web_sys::Event)>::new(|event: web_sys::Event| {
        event.prevent_default();
        let message = js_sys::Reflect::get(&event, &"message".into())
            .ok()
            .and_then(|value| value.as_string())
            .filter(|text| !text.is_empty())
            .unwrap_or_else(|| {
                "the worker script did not load or run (the browser gave no message)".to_owned()
            });
        fail(message);
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
        fail("The worker sent an unreadable message".to_owned());
        return;
    };
    if text == wire::TILE_TAG {
        match bytes
            .as_deref()
            .ok_or("A tile fragment has no binary payload".to_owned())
            .and_then(wire::decode_tiles)
        {
            Ok(batch) => deliver(Event::Tiles(batch)),
            Err(message) => fail(message),
        }
        return;
    }
    let message = match wire::decode::<FromServer>(&text) {
        Ok(message) => message,
        Err(e) => {
            fail(e);
            return;
        }
    };
    match message {
        FromServer::Started => PORT.with(|port| {
            let mut port = port.borrow_mut();
            if let Some(Transport::Worker(link)) = port.transport.as_mut() {
                link.started = true;
                for (text, bytes) in link.waiting.drain(..) {
                    if let Err(message) = post(&link.worker, &text, bytes) {
                        leptos::prelude::set_timeout(
                            move || fail(message),
                            std::time::Duration::ZERO,
                        );
                        break;
                    }
                }
            }
        }),
        FromServer::Event(event) => deliver(event),
        FromServer::Reply { id, reply } => {
            let answer = PORT.with(|port| port.borrow_mut().answers.remove(&id));
            if let Some(answer) = answer {
                answer(Ok(reply), bytes);
            }
            notify_health(None);
        }
        FromServer::Failed {
            id: Some(id),
            message,
        } => {
            let answer = PORT.with(|port| port.borrow_mut().answers.remove(&id));
            if let Some(answer) = answer {
                answer(Err(message), None);
            }
            notify_health(None);
        }
        FromServer::Failed { id: None, message } => {
            fail(message);
        }
    }
}

/// Browser worker adapter. It has no in-process workbook fallback or host dependency.
pub struct BrowserTransport;
impl dna_calc_model::WorkbookTransport for BrowserTransport {
    fn start(&self) {
        start();
    }
    fn set_events(&self, handler: Rc<dyn Fn(Event)>) {
        set_events(move |event| handler(event));
    }
    fn set_health(&self, handler: Rc<dyn Fn(Health)>) {
        set_health(move |health| handler(health));
    }
    fn call(
        &self,
        request: Request,
        bytes: Option<Vec<u8>>,
        answer: dna_calc_model::TransportAnswer,
    ) {
        call(request, bytes, answer);
    }
}

/// Commands actually submitted through this page's transport, for read-only diagnostics.
#[cfg(target_arch = "wasm32")]
pub(crate) fn command_count() -> u64 {
    PORT.with(|port| port.borrow().commands)
}
