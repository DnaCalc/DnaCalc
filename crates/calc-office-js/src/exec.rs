//! Batch execution: ops run in order on the owner lane; the first failure stops the batch.
//! Earlier ops stay committed, later ones do not run (API_FACES.md 5.2).

use crate::map::{self, Operation};
use crate::wire::*;
use calc_object_model::{AutoError, AutoValue, Call, Descriptor, ObjectModel, ObjectRef};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

const OWNER: &str = "dc-17spk.5";

pub struct Executor<M: ObjectModel> {
    model: M,
    handles: RefCell<HashMap<(u64, u64), ObjectRef>>,
    tracked: RefCell<HashSet<(u64, u64)>>,
    /// Batch ids already answered: a repeated id is refused, never re-run.
    seen: RefCell<HashSet<u64>>,
}

impl<M: ObjectModel> Executor<M> {
    pub fn new(model: M) -> Self {
        Executor { model, handles: Default::default(), tracked: Default::default(), seen: Default::default() }
    }
    pub fn model(&self) -> &M {
        &self.model
    }
    pub fn live_handles(&self) -> usize {
        self.handles.borrow().len()
    }

    pub fn run(&self, request: &BatchRequest) -> BatchReply {
        let mut reply = BatchReply { batch_id: request.batch_id, results: Vec::new(), error: None };
        if !self.seen.borrow_mut().insert(request.batch_id) {
            reply.error = Some(error("GeneralException", "batch id already used; batches are never replayed", None, 0));
            return reply;
        }
        for (ordinal, op) in request.ops.iter().enumerate() {
            match self.step(request.context, op) {
                Ok(mut result) => {
                    result.ordinal = ordinal;
                    reply.results.push(result);
                }
                Err(mut failure) => {
                    failure.ordinal = ordinal;
                    reply.error = Some(failure);
                    break;
                }
            }
        }
        if request.final_release {
            self.release(request.context);
        }
        reply
    }

    /// Release every untracked handle of `context` (end of `Excel.run`).
    pub fn release(&self, context: u64) {
        let mut handles = self.handles.borrow_mut();
        let tracked = self.tracked.borrow();
        let gone: Vec<_> = handles.keys().filter(|k| k.0 == context && !tracked.contains(k)).copied().collect();
        for key in gone {
            if let Some(object) = handles.remove(&key) {
                self.model.released(&object);
            }
        }
    }

    fn object(&self, context: u64, handle: u64) -> Result<ObjectRef, BatchError> {
        self.handles
            .borrow()
            .get(&(context, handle))
            .cloned()
            .ok_or_else(|| error("InvalidObjectPath", &format!("handle {handle} is not live"), None, 0))
    }

    fn bind(&self, context: u64, handle: u64, object: ObjectRef) {
        if let Some(old) = self.handles.borrow_mut().insert((context, handle), object) {
            self.model.released(&old);
        }
    }

    fn step(&self, context: u64, op: &Op) -> Result<OpResult, BatchError> {
        let mut result = OpResult { ordinal: 0, loaded: Vec::new(), null_object: false };
        match op {
            Op::Root { name, out } => {
                let application = self.model.application();
                let object = match name.as_str() {
                    "application" => application,
                    "workbook" => {
                        let value = self.invoke(&application, "ActiveWorkbook", Operation::Get, vec![])?;
                        self.as_object(value)?
                    }
                    other => return Err(error("ApiNotFound", &format!("context root {other} is not modelled"), Some(OWNER), 0)),
                };
                self.bind(context, *out, object);
            }
            Op::Get { target, name, out } => {
                let target = self.object(context, *target)?;
                match self.invoke(&target, name, Operation::Get, vec![]) {
                    Ok(value) => {
                        let object = self.as_object(value)?;
                        self.bind(context, *out, object);
                    }
                    Err(failure) => return self.null_or(name, failure, &mut result),
                }
            }
            Op::Call { target, name, args, out } => {
                let target = self.object(context, *target)?;
                let args = self.args(context, args)?;
                match self.invoke(&target, name, Operation::Call, args) {
                    Ok(value) => {
                        if let Some(out) = out {
                            let object = self.as_object(value)?;
                            self.bind(context, *out, object);
                        } else if let AutoValue::Object(object) = value {
                            self.model.released(&object);
                        }
                    }
                    Err(failure) => return self.null_or(name, failure, &mut result),
                }
            }
            Op::Set { target, name, value } => {
                let target = self.object(context, *target)?;
                let value = self.args(context, std::slice::from_ref(value))?;
                self.invoke(&target, name, Operation::Set, value)?;
            }
            Op::Load { target, props } => {
                let target = self.object(context, *target)?;
                for prop in props {
                    let mapped = map::lookup(prop);
                    let value = self.invoke(&target, prop, Operation::Get, vec![])?;
                    let offset = mapped.map(|m| m.offset).unwrap_or(0);
                    let value = match (&value, offset) {
                        (AutoValue::I4(n), o) if o != 0 => AutoValue::I4(n + o),
                        (AutoValue::R8(n), o) if o != 0 => AutoValue::R8(n + f64::from(o)),
                        _ => value,
                    };
                    let wire = self.to_wire(context, &value);
                    result.loaded.push((prop.clone(), wire));
                }
            }
            Op::Track { target } => {
                self.object(context, *target)?;
                self.tracked.borrow_mut().insert((context, *target));
            }
            Op::Untrack { target } => {
                self.tracked.borrow_mut().remove(&(context, *target));
            }
        }
        Ok(result)
    }

    /// `getXOrNullObject` that finds nothing yields a null proxy, not a failed batch.
    fn null_or(&self, name: &str, failure: BatchError, result: &mut OpResult) -> Result<OpResult, BatchError> {
        if name.ends_with("OrNullObject") && failure.code != "ApiNotFound" {
            result.null_object = true;
            return Ok(result.clone());
        }
        Err(failure)
    }

    fn as_object(&self, value: AutoValue) -> Result<ObjectRef, BatchError> {
        match value {
            AutoValue::Object(object) => Ok(object),
            _ => Err(error("GeneralException", "the member did not return an object", None, 0)),
        }
    }

    fn args(&self, context: u64, args: &[WireValue]) -> Result<Vec<AutoValue>, BatchError> {
        let lookup = |h: u64| self.handles.borrow().get(&(context, h)).cloned();
        args.iter()
            .map(|a| from_wire(a, &lookup).map_err(|m| error("InvalidArgument", &m, None, 0)))
            .collect()
    }

    fn to_wire(&self, context: u64, value: &AutoValue) -> WireValue {
        let mut next = self.handles.borrow().keys().filter(|k| k.0 == context).map(|k| k.1).max().unwrap_or(0) + 1_000_000;
        let mut bound = Vec::new();
        let wire = to_wire(value, &mut |object| {
            next += 1;
            bound.push((next, object.clone()));
            next
        });
        for (h, object) in bound {
            self.bind(context, h, object);
        }
        wire
    }

    fn invoke(&self, target: &ObjectRef, office: &str, operation: Operation, args: Vec<AutoValue>) -> Result<AutoValue, BatchError> {
        let base = office.strip_suffix("OrNullObject").unwrap_or(office);
        let mapped = map::lookup(base);
        let com_name = mapped.as_ref().map(|m| m.com).unwrap_or(base);
        let descriptor = Descriptor::get();
        let found = map::kinds(operation)
            .iter()
            .find_map(|kind| descriptor.member(target.interface, com_name, *kind));
        let Some(member) = found else {
            return Err(error(
                "ApiNotFound",
                &format!("{office} is not modelled on {}", descriptor.ty(target.interface).name),
                Some(OWNER),
                0,
            ));
        };
        let mut args = args;
        if let (Some(m), true) = (&mapped, args.is_empty()) {
            args = (m.preset)();
        }
        let call = Call { interface: target.interface, member, args, lcid: 0 };
        self.model.invoke(target, &call).map_err(|failure| {
            self.model.record_refusal(&failure);
            map_error(&failure)
        })
    }
}

fn error(code: &str, message: &str, owner: Option<&str>, ordinal: usize) -> BatchError {
    BatchError { code: code.into(), message: message.into(), owner: owner.map(Into::into), ordinal, committed: false }
}

pub fn map_error(failure: &AutoError) -> BatchError {
    match failure {
        AutoError::NotModelled { owner, .. } => error("ApiNotFound", &failure.description(), Some(owner), 0),
        AutoError::Excel { .. } | AutoError::Status(_) => error("GeneralException", &failure.description(), None, 0),
        AutoError::Disconnected => error("InvalidObjectPath", &failure.description(), None, 0),
    }
}
