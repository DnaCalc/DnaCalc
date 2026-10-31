//! Headless composition using the same server and JSON route as the browser worker.
use crate::{
    server::{self, Server},
    wire,
};
use dna_calc_model::{Health, TransportAnswer, WorkbookTransport};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use wire::{Event, FromServer, Request, ToServer};
#[derive(Default)]
pub struct InProcessTransport {
    server: RefCell<Server>,
    next: Cell<u64>,
    events: RefCell<Option<Rc<dyn Fn(Event)>>>,
    health: RefCell<Option<Rc<dyn Fn(Health)>>>,
}
impl WorkbookTransport for InProcessTransport {
    fn start(&self) {}
    fn set_events(&self, handler: Rc<dyn Fn(Event)>) {
        *self.events.borrow_mut() = Some(handler);
    }
    fn set_health(&self, handler: Rc<dyn Fn(Health)>) {
        *self.health.borrow_mut() = Some(handler.clone());
        handler(Health {
            state: "Headless workbook transport".into(),
            pending: 0,
        });
    }
    fn call(&self, request: Request, bytes: Option<Vec<u8>>, answer: TransportAnswer) {
        let id = self
            .next
            .get()
            .checked_add(1)
            .expect("transport identities exhausted");
        self.next.set(id);
        let text = wire::encode(&ToServer { id, request });
        let (reply, bytes, events) = {
            let mut server = self.server.borrow_mut();
            let (reply, bytes) = server::answer_text(&mut server, &text, bytes);
            (reply, bytes, server.take_events())
        };
        for event in events {
            let encoded = wire::encode(&FromServer::Event(event));
            let handler = self.events.borrow().clone();
            if let (Some(handler), Ok(FromServer::Event(event))) = (handler, wire::decode(&encoded))
            {
                handler(event);
            }
        }
        match wire::decode(&reply) {
            Ok(FromServer::Reply {
                id: received,
                reply,
            }) if received == id => answer(Ok(reply), bytes),
            Ok(FromServer::Failed { message, .. }) => answer(Err(message), None),
            Ok(_) => answer(
                Err("The workbook transport returned an unexpected message".into()),
                None,
            ),
            Err(error) => answer(Err(error), None),
        }
    }
}
