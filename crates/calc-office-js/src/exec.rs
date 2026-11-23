//! Batch execution: ops run in order on the owner lane; the first failure stops the batch.
//! Earlier ops stay committed, later ones do not run (API_FACES.md 5.2).

use crate::map::{self, Operation};
use crate::wire::*;
use calc_object_model::{
    AutoError, AutoValue, Call, Descriptor, InvokeKind, ObjectModel, ObjectRef,
};
use dna_calc_protocol::{Reply, Request};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

const OWNER: &str = "dc-17spk.5";

pub struct Executor<M: ObjectModel> {
    model: M,
    handles: RefCell<HashMap<(u64, u64), Handle>>,
    /// Batch ids already answered: a repeated id is refused, never re-run.
    seen: RefCell<HashSet<u64>>,
}

struct Handle {
    object: Option<ObjectRef>,
    tracked: bool,
}

impl<M: ObjectModel> Executor<M> {
    pub fn new(model: M) -> Self {
        Executor {
            model,
            handles: Default::default(),
            seen: Default::default(),
        }
    }
    pub fn model(&self) -> &M {
        &self.model
    }
    pub fn live_handles(&self) -> usize {
        self.handles.borrow().len()
    }

    /// Handle the Office.js requests carried by the shared application protocol.
    pub fn handle(&self, request: &Request) -> Option<Reply> {
        match request {
            Request::AutomationBatch(batch) => Some(Reply::AutomationBatch(self.run(batch))),
            Request::AutomationRelease(request) => {
                self.release(request.context);
                Some(Reply::AutomationReleased)
            }
            _ => None,
        }
    }

    pub fn run(&self, request: &BatchRequest) -> BatchReply {
        let mut reply = BatchReply {
            batch_id: request.batch_id,
            results: Vec::new(),
            error: None,
        };
        if !self.seen.borrow_mut().insert(request.batch_id) {
            reply.error = Some(error(
                "GeneralException",
                "batch id already used; batches are never replayed",
                None,
                0,
            ));
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
        self.handles.borrow_mut().retain(|&(owner, _), handle| {
            let keep = owner != context || handle.tracked;
            if !keep && let Some(object) = &handle.object {
                self.model.released(object);
            }
            keep
        });
    }

    fn object(&self, context: u64, handle: u64) -> Result<ObjectRef, BatchError> {
        self.handles
            .borrow()
            .get(&(context, handle))
            .ok_or_else(|| {
                error(
                    "InvalidObjectPath",
                    &format!("handle {handle} is not live"),
                    None,
                    0,
                )
            })?
            .object
            .clone()
            .ok_or_else(|| error("ItemNotFound", "the target is a null object", None, 0))
    }

    fn bind(&self, context: u64, handle: u64, object: Option<ObjectRef>) -> Result<(), BatchError> {
        use std::collections::hash_map::Entry;
        match self.handles.borrow_mut().entry((context, handle)) {
            Entry::Vacant(entry) => {
                entry.insert(Handle {
                    object,
                    tracked: false,
                });
            }
            Entry::Occupied(_) => {
                if let Some(object) = object {
                    self.model.released(&object);
                }
                return Err(error(
                    "InvalidObjectPath",
                    "the output handle is already bound",
                    None,
                    0,
                ));
            }
        }
        Ok(())
    }

    fn step(&self, context: u64, op: &Op) -> Result<OpResult, BatchError> {
        let mut result = OpResult {
            ordinal: 0,
            loaded: Vec::new(),
            null_object: false,
        };
        match op {
            Op::Root { name, out } => {
                let object = match name.as_str() {
                    "application" => self.model.application(),
                    "workbook" => {
                        let application = self.model.application();
                        let value = self.invoke_member(
                            &application,
                            "ActiveWorkbook",
                            &[InvokeKind::Get],
                            vec![],
                        );
                        self.model.released(&application);
                        self.as_object(value?)?
                    }
                    other => {
                        return Err(error(
                            "ApiNotFound",
                            &format!("context root {other} is not modelled"),
                            Some(OWNER),
                            0,
                        ));
                    }
                };
                self.bind(context, *out, Some(object))?;
            }
            Op::Get { target, name, out } => {
                let target = self.object(context, *target)?;
                let value = self.invoke(&target, name, Operation::Get, vec![])?;
                self.bind(context, *out, Some(self.as_object(value)?))?;
            }
            Op::Call {
                target,
                name,
                args,
                out,
            } => {
                let target = self.object(context, *target)?;
                let args = self.args(context, args)?;
                match self.invoke(&target, name, Operation::Call, args) {
                    Ok(value) => {
                        if let Some(out) = out {
                            let object = self.as_object(value)?;
                            self.bind(context, *out, Some(object))?;
                        } else if let AutoValue::Object(object) = value {
                            self.model.released(&object);
                        }
                    }
                    Err(failure)
                        if name == "getItemOrNullObject" && failure.code == "ItemNotFound" =>
                    {
                        if let Some(out) = out {
                            self.bind(context, *out, None)?;
                            result.null_object = true;
                        }
                    }
                    Err(failure) => return Err(failure),
                }
            }
            Op::Set {
                target,
                name,
                value,
            } => {
                let target = self.object(context, *target)?;
                let value = self.args(context, std::slice::from_ref(value))?;
                self.invoke(&target, name, Operation::Set, value)?;
            }
            Op::Load { target, props } => {
                if props.is_empty() {
                    return Ok(result);
                }
                let target = match self.object(context, *target) {
                    Ok(object) => Some(object),
                    Err(failure)
                        if failure.code == "ItemNotFound"
                            && props.iter().any(|prop| prop == "isNullObject") =>
                    {
                        None
                    }
                    Err(failure) => return Err(failure),
                };
                for prop in props {
                    if prop == "isNullObject" {
                        result.loaded.push((
                            prop.clone(),
                            WireValue::Bool {
                                v: target.is_none(),
                            },
                        ));
                        continue;
                    }
                    let Some(target) = &target else { continue };
                    let mapped = map::lookup(prop);
                    let value = self.invoke(target, prop, Operation::Get, vec![])?;
                    let value = if prop == "address" {
                        self.sheet_address(target, value)?
                    } else {
                        value
                    };
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
            Op::Track { target } | Op::Untrack { target } => {
                let mut handles = self.handles.borrow_mut();
                let handle = handles.get_mut(&(context, *target)).ok_or_else(|| {
                    error(
                        "InvalidObjectPath",
                        "the target handle is not live",
                        None,
                        0,
                    )
                })?;
                handle.tracked = matches!(op, Op::Track { .. });
            }
        }
        Ok(result)
    }

    fn sheet_address(&self, range: &ObjectRef, value: AutoValue) -> Result<AutoValue, BatchError> {
        let sheet =
            self.as_object(self.invoke_member(range, "Worksheet", &[InvokeKind::Get], vec![])?)?;
        let name = self.invoke_member(&sheet, "Name", &[InvokeKind::Get], vec![]);
        self.model.released(&sheet);
        let name = name?
            .as_text()
            .ok_or_else(|| error("GeneralException", "invalid sheet name", None, 0))?;
        let address = value
            .as_text()
            .ok_or_else(|| error("GeneralException", "invalid range address", None, 0))?;
        let qualifier = if name.starts_with(|c: char| c.is_alphabetic() || c == '_')
            && name
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
        {
            name
        } else {
            format!("'{}'", name.replace('\'', "''"))
        };
        Ok(AutoValue::text(&format!("{qualifier}!{address}")))
    }

    fn as_object(&self, value: AutoValue) -> Result<ObjectRef, BatchError> {
        match value {
            AutoValue::Object(object) => Ok(object),
            _ => Err(error(
                "GeneralException",
                "the member did not return an object",
                None,
                0,
            )),
        }
    }

    fn args(&self, context: u64, args: &[WireValue]) -> Result<Vec<AutoValue>, BatchError> {
        let lookup = |h: u64| self.object(context, h).ok();
        args.iter()
            .map(|a| from_wire(a, &lookup).map_err(|m| error("InvalidArgument", &m, None, 0)))
            .collect()
    }

    fn to_wire(&self, context: u64, value: &AutoValue) -> WireValue {
        // Allocate returned object handles from the upper end of JS's safe integer range.
        let mut handles = self.handles.borrow_mut();
        let mut next = (1 << 53) - 1;
        to_wire(value, &mut |object| {
            while handles.contains_key(&(context, next)) {
                next -= 1;
            }
            handles.insert(
                (context, next),
                Handle {
                    object: Some(object.clone()),
                    tracked: false,
                },
            );
            next
        })
    }

    fn invoke(
        &self,
        target: &ObjectRef,
        office: &str,
        operation: Operation,
        args: Vec<AutoValue>,
    ) -> Result<AutoValue, BatchError> {
        let base = office.strip_suffix("OrNullObject").unwrap_or(office);
        let Some(mapped) = map::lookup(base) else {
            return Err(error(
                "ApiNotFound",
                &format!("{office} is not modelled"),
                Some(OWNER),
                0,
            ));
        };
        let args = if args.is_empty() {
            (mapped.preset)()
        } else {
            args
        };
        self.invoke_member(target, mapped.com, map::kinds(operation, mapped.com), args)
    }

    fn invoke_member(
        &self,
        target: &ObjectRef,
        name: &str,
        kinds: &[InvokeKind],
        args: Vec<AutoValue>,
    ) -> Result<AutoValue, BatchError> {
        let descriptor = Descriptor::get();
        let found = kinds
            .iter()
            .find_map(|kind| descriptor.member(target.interface, name, *kind));
        let Some(member) = found else {
            return Err(error(
                "ApiNotFound",
                &format!(
                    "{name} is not modelled on {}",
                    descriptor.ty(target.interface).name
                ),
                Some(OWNER),
                0,
            ));
        };
        let call = Call {
            interface: target.interface,
            member,
            args,
            lcid: 0,
        };
        self.model.invoke(target, &call).map_err(|failure| {
            self.model.record_refusal(&failure);
            map_error(&failure)
        })
    }
}

impl<M: ObjectModel> Drop for Executor<M> {
    fn drop(&mut self) {
        for handle in self.handles.get_mut().values() {
            if let Some(object) = &handle.object {
                self.model.released(object);
            }
        }
    }
}

fn error(code: &str, message: &str, owner: Option<&str>, ordinal: usize) -> BatchError {
    BatchError {
        code: code.into(),
        message: message.into(),
        owner: owner.map(Into::into),
        ordinal,
        committed: false,
    }
}

pub fn map_error(failure: &AutoError) -> BatchError {
    match failure {
        AutoError::NotModelled { owner, .. } => {
            error("ApiNotFound", &failure.description(), Some(owner), 0)
        }
        AutoError::Status(status) if *status == 0x8002_000Bu32 as i32 => {
            error("ItemNotFound", "the item was not found", None, 0)
        }
        AutoError::PolicyDenied { .. } => error("AccessDenied", &failure.description(), None, 0),
        AutoError::Excel { .. } | AutoError::Status(_) => {
            error("GeneralException", &failure.description(), None, 0)
        }
        AutoError::Disconnected => error("InvalidObjectPath", &failure.description(), None, 0),
    }
}
