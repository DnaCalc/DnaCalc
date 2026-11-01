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
        let mut slots = Vec::with_capacity(args.len());
        for (value, parameter) in args.iter().zip(&procedure.parameters) {
            let value = if let Value::Range(range) = value {
                self.spreadsheet
                    .as_ref()
                    .ok_or_else(|| Error::ValueConversion("Range requires a host".into()))?
                    .read_range(*range)?
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
