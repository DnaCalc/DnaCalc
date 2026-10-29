//! The page's way to the workbook's server (dc-p96): [`call`] sends a [`Request`] and hands the
//! [`Reply`] to a callback; [`set_events`] takes the [`Event`]s the server pushes.
//!
//! Every message crosses as its JSON text ([`crate::wire`]), whatever the transport, so what the
//! page and the server say to each other is the same in process and across a worker. While some
//! skins still read the workbook directly, the server runs in process, here on the page: a call
//! is answered at once, its events delivered before its reply. [`with_book`] is those skins' way
//! to the workbook; it goes when the server moves into the calculation worker.

use crate::server::{self, Server};
use crate::wire::{self, Event, FromServer, Reply, Request, ToServer};
use crate::workbook::Workbook;
use std::cell::RefCell;
use std::rc::Rc;

type Answer = Box<dyn FnOnce(Result<Reply, String>, Option<Vec<u8>>)>;

struct Port {
    server: Option<Server>,
    next_id: u64,
    events: Option<Rc<dyn Fn(Event)>>,
}

thread_local! {
    static PORT: RefCell<Port> = RefCell::new(Port {
        server: None,
        next_id: 1,
        events: None,
    });
}

/// Starts the server: in process, over a new workbook.
pub fn start() {
    PORT.with(|port| {
        let mut port = port.borrow_mut();
        if port.server.is_none() {
            port.server = Some(Server::new());
        }
    });
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
    let answer: Answer = Box::new(answer);
    // The server answers with the port released, so a callback may call again.
    let served = PORT.with(|port| {
        let mut port = port.borrow_mut();
        let id = port.next_id;
        port.next_id += 1;
        let text = wire::encode(&ToServer { id, request });
        let handler = port.events.clone();
        let server = port.server.get_or_insert_with(Server::new);
        let (reply, bytes) = server::answer_text(server, &text, bytes);
        let events: Vec<String> = server
            .take_events()
            .into_iter()
            .map(|event| wire::encode(&FromServer::Event(event)))
            .collect();
        (reply, bytes, events, handler)
    });
    let (reply, bytes, events, handler) = served;
    if let Some(handler) = handler {
        for text in events {
            if let Ok(FromServer::Event(event)) = wire::decode::<FromServer>(&text) {
                handler(event);
            }
        }
    }
    match wire::decode::<FromServer>(&reply) {
        Ok(FromServer::Reply { reply, .. }) => answer(Ok(reply), bytes),
        Ok(FromServer::Failed { message, .. }) => answer(Err(message), None),
        Ok(_) => answer(Err("the server answered with no reply".to_owned()), None),
        Err(message) => answer(Err(message), None),
    }
}

/// Reads the open workbook directly: for the skins that do not yet read through messages.
pub fn with_book<R>(f: impl FnOnce(&Workbook) -> R) -> R {
    PORT.with(|port| {
        let mut port = port.borrow_mut();
        let server = port.server.get_or_insert_with(Server::new);
        f(server.book())
    })
}

/// Changes the open workbook directly (the atlas's census through short subscriptions).
pub fn with_book_mut<R>(f: impl FnOnce(&mut Workbook) -> R) -> R {
    PORT.with(|port| {
        let mut port = port.borrow_mut();
        let server = port.server.get_or_insert_with(Server::new);
        f(server.book_mut())
    })
}
