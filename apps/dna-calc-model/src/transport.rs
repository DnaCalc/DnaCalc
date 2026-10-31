//! Workbook transport seam. Implementations own delivery and browser/native resources;
//! application commands and projections contain no implementation handles.
use crate::{
    Health,
    projection::{Event, Reply, Request},
};
use std::rc::Rc;
pub type TransportAnswer = Box<dyn FnOnce(Result<Reply, String>, Option<Vec<u8>>)>;
pub trait WorkbookTransport {
    fn start(&self);
    fn set_events(&self, handler: Rc<dyn Fn(Event)>);
    fn set_health(&self, handler: Rc<dyn Fn(Health)>);
    fn call(&self, request: Request, bytes: Option<Vec<u8>>, answer: TransportAnswer);
}
