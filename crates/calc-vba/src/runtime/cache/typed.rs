//! Admission-time typed shims. Unsupported bodies retain the explicit Variant adapter.
use super::*;
use api::Session;
use calcvba_host_api as api;
pub(super) struct TypedEntry {
    bound: api::BoundProcedure,
}
pub(super) fn entries(
    inner: &oxvba_embed::OwnedSession,
    functions: &[(Function, oxvba_embed::Procedure)],
    bound: &[api::BoundProcedure],
) -> Vec<Option<TypedEntry>> {
    functions
        .iter()
        .zip(bound)
        .map(|((_, procedure), bound)| {
            let ty = |ty: &OxTy| match ty {
                OxTy::Long => Some(api::ScalarType::Long),
                OxTy::Double => Some(api::ScalarType::Double),
                OxTy::Object(oxvba_oxir::ObjClass::ComIface(id)|oxvba_oxir::ObjClass::Iface(id))
                    if inner.com_interfaces().get(id.0).is_some_and(|interface|matches!(interface,oxvba_oxir::ComInterface::Com(meta) if meta.iid==Some(spreadsheet::typed_iid()))) => Some(api::ScalarType::HostObject),
                _ => None,
            };
            if procedure
                .parameters
                .iter()
                .any(|p| p.optional || p.param_array)
            {
                return None;
            }
            let arguments = procedure
                .parameters
                .iter()
                .map(|p| ty(&p.ty))
                .collect::<Option<Vec<_>>>()?;
            let result = ty(procedure.return_type.as_ref()?)?;
            if result==api::ScalarType::HostObject {return None;}
            inner
                .bind_slots(neutral::handle(bound).ok()?, &arguments, result)
                .ok()?;
            // Pure scalar admission does not prove owning activation support (the owning
            // frame also covers globals). Allocate and drop an unstarted frame here.
            let placeholders: Vec<_> = arguments
                .iter()
                .map(|ty| match ty {
                    api::ScalarType::Long => api::Slot::Long(0),
                    api::ScalarType::Double => api::Slot::Double(0.0),
                api::ScalarType::HostObject=>api::Slot::Nothing,
                    _ => unreachable!(),
                })
                .collect();
            drop(
                inner
                    .begin_invocation(
                        neutral::handle(bound).ok()?,
                        &placeholders,
                        api::InvocationFrame::new(0, None, std::rc::Rc::new(())),
                        0,
                    )
                    .ok()?,
            );
            Some(TypedEntry {
                bound: bound.clone(),
            })
        })
        .collect()
}
impl OwnedSession {
    pub(crate) fn begin_typed(
        &self,
        index: usize,
        args: &[Value],
        frame: api::InvocationFrame,
        max_blocks: u64,
    ) -> Result<Option<Box<dyn api::Invocation>>, Error> {
        let Some(entry) = self.typed.get(index).and_then(Option::as_ref) else {
            return Ok(None);
        };
        let procedure = &self.functions[index].1;
        if args.len() != procedure.parameters.len() {
            return Err(Error::ArgumentCount {
                expected: procedure.parameters.len(),
                received: args.len(),
            });
        }
        let objects: Vec<_> = args
            .iter()
            .zip(&procedure.parameters)
            .map(|(value, parameter)| {
                if matches!(parameter.ty, OxTy::Object(_)) {
                    match value {
                        Value::Range(range) => self
                            .spreadsheet
                            .as_ref()
                            .map(|host| Some(host.typed_range(*range)))
                            .ok_or_else(|| Error::ValueConversion("Range requires a host".into())),
                        _ => Err(Error::ValueConversion(
                            "typed Range argument required".into(),
                        )),
                    }
                } else {
                    Ok(None)
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut slots = Vec::with_capacity(args.len());
        for ((value, parameter), object) in args.iter().zip(&procedure.parameters).zip(&objects) {
            if let Some(object) = object {
                slots.push(object.borrow_slot());
                continue;
            }
            let value = if let Value::Range(range) = value {
                self.spreadsheet
                    .as_ref()
                    .ok_or_else(|| Error::ValueConversion("Range requires a host".into()))?
                    .read_argument_range(*range)?
            } else {
                value.clone()
            };
            let converted = coerce_argument(to_variant(&value)?, &parameter.ty)?;
            slots.push(match parameter.ty {
                OxTy::Long => api::Slot::Long(converted.as_i32().expect("Long coercion")),
                OxTy::Double => api::Slot::Double(converted.as_f64().expect("Double coercion")),
                _ => unreachable!(),
            });
        }
        self.begin_invocation(&entry.bound, &slots, frame, max_blocks)
            .map(Some)
            .map_err(neutral::adapter_error)
    }
    pub(crate) fn in_context<T>(
        &self,
        context: &dyn SpreadsheetContext,
        call: impl FnOnce() -> Result<T, Error>,
    ) -> Result<T, Error> {
        spreadsheet::with_context(context, call)
    }
}
