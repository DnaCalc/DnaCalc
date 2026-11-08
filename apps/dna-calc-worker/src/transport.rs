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
impl InProcessTransport {
    /// The server the transport runs, for a native owner that installs what a browser worker
    /// cannot (an add-in's registered functions).
    pub fn with_server<R>(&self, f: impl FnOnce(&mut Server) -> R) -> R {
        f(&mut self.server.borrow_mut())
    }
    /// A native owner can drive one cooperative task, releasing its server borrow
    /// before events reenter the transport (for example a save-ready PollSave).
    pub fn advance_calculation(&self) -> Result<calc_host::HostProgress, String> {
        let (progress, events) = {
            let mut server = self.server.borrow_mut();
            let progress = server.advance_calculation();
            (progress, server.take_events())
        };
        self.deliver_events(events)?;
        Ok(progress)
    }
    fn deliver_events(&self, events: Vec<Event>) -> Result<(), String> {
        for event in events {
            let received = match event {
                Event::Tiles(batch) => wire::encode_tiles(&batch)
                    .and_then(|bytes| wire::decode_tiles(&bytes))
                    .map(Event::Tiles),
                control => wire::decode::<FromServer>(&wire::encode(&FromServer::Event(control)))
                    .and_then(|message| match message {
                        FromServer::Event(event) => Ok(event),
                        _ => Err("Unexpected projection control".into()),
                    }),
            };
            let handler = self.events.borrow().clone();
            if let Some(handler) = handler {
                match received {
                    Ok(event) => handler(event),
                    Err(reason) => {
                        return Err(reason);
                    }
                }
            }
        }
        Ok(())
    }
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
        if let Err(reason) = self.deliver_events(events) {
            answer(Err(reason), None);
            return;
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
