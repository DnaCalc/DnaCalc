//! Runtime composition over neutral code/session ownership; no spreadsheet adapter dependency.
use super::*;
pub struct OwnedSession {
    inner: calcvba_host::OwnedSession,
    functions: Vec<(Function, calcvba_host::Procedure)>,
    spreadsheet: Option<Arc<spreadsheet::SpreadsheetHost>>,
    bound: Vec<calcvba_host_api::BoundProcedure>,
    catalogue: Vec<calcvba_host_api::Procedure>,
    typed: Vec<Option<typed::TypedEntry>>,
    event_project: bool,
    /// Per function of a document project: why it runs on the dynamic runtime, as a worksheet
    /// function of an add-in or a module does, instead of its owning typed entry: a signature the
    /// owning value carriers do not hold (Date, Currency, Range, arrays), or a body the
    /// typed route cannot link (a VBA library function it has no owning builtin for, a class
    /// created with `New`). `None`: the owning typed entry runs it (dc-hsa5.12).
    dynamic: Vec<Option<String>>,
}
impl OwnedSession {
    fn wrap(
        inner: calcvba_host::OwnedSession,
        functions: Vec<(Function, calcvba_host::Procedure)>,
        spreadsheet: Option<Arc<spreadsheet::SpreadsheetHost>>,
        event_project: bool,
    ) -> Result<Self, Error> {
        let bound = bind_functions(&functions, |p| inner.bind(&p.module, &p.name, p.kind))?;
        let bound: Vec<_> = bound
            .into_iter()
            .map(|bound| {
                calcvba_host_api::BoundProcedure::from_provider(bound.clone(), bound.generation())
            })
            .collect();
        let catalogue = neutral::catalogue(&inner);
        let typed = typed::entries(&inner, &functions, &bound);
        let mut dynamic = vec![None; functions.len()];
        if event_project {
            for (index, ((_, procedure), bound)) in functions.iter().zip(&bound).enumerate() {
                let carried = |ty: &OxTy| {
                    matches!(
                        ty,
                        OxTy::Long | OxTy::Double | OxTy::Bool | OxTy::Str | OxTy::Variant
                    )
                };
                let fixed = procedure.parameters.iter().take_while(|p| !p.param_array);
                if !(fixed.clone().all(|p| carried(&p.ty))
                    && procedure.return_type.as_ref().is_some_and(carried))
                {
                    dynamic[index] =
                        Some("a signature outside the owning value carriers".to_owned());
                    continue;
                }
                let linked = inner.bind_values(
                    neutral::handle(bound).map_err(neutral::adapter_error)?,
                    &fixed.map(|p| owning_type(&p.ty)).collect::<Vec<_>>(),
                    owning_type(procedure.return_type.as_ref().expect("function result")),
                );
                match linked {
                    Ok(()) => {}
                    Err(calcvba_host::Error::UnsupportedSignature(why)) => {
                        dynamic[index] = Some(why)
                    }
                    Err(error) => return Err(engine_error(error)),
                }
            }
        }
        Ok(Self {
            dynamic,
            typed,
            catalogue,
            inner,
            functions,
            spreadsheet,
            bound,
            event_project,
        })
    }
    pub fn backend(&self) -> Backend {
        self.inner.backend()
    }
    /// Why function `index` of a document project runs on the dynamic runtime, if it does.
    pub fn dynamic(&self, index: usize) -> Option<&str> {
        self.dynamic.get(index).and_then(Option::as_deref)
    }
    pub fn call(&self, index: usize, args: &[Value], max_steps: u64) -> Result<Value, Error> {
        let call = || self.call_inner(index, args, max_steps);
        if self.spreadsheet.is_some() {
            spreadsheet::without_context(call)
        } else {
            call()
        }
    }
    pub fn call_with_context(
        &self,
        index: usize,
        args: &[Value],
        max_steps: u64,
        context: &mut dyn SpreadsheetContext,
    ) -> Result<Value, Error> {
        spreadsheet::with_context(context, || self.call_inner(index, args, max_steps))
    }
    fn call_inner(&self, index: usize, args: &[Value], max_steps: u64) -> Result<Value, Error> {
        call_function(
            &self.functions,
            self.spreadsheet.as_deref(),
            index,
            args,
            |mut values, tail| {
                if self.event_project && self.dynamic(index).is_none() {
                    // The owning typed route checks the whole reachable call graph before any
                    // body effect. A callee it cannot link (a class created with `New`, a
                    // typed Raise, a logical Or) refuses with UnsupportedSignature before
                    // anything ran, so the function runs on the dynamic runtime instead, as
                    // a function the signature check already sent there does (dc-tey4.3).
                    match self.call_owning(
                        index,
                        &values,
                        tail.as_deref().unwrap_or_default(),
                        max_steps,
                    ) {
                        Err(Error::UnsupportedSignature(_)) => {}
                        other => return other,
                    }
                }
                // The dynamic entry takes the packed carrier. The owning API packs its
                // own tail from the individual worksheet arguments.
                if let Some(tail) = tail {
                    values.push(Variant::from_safearray(SafeArray::from_variants(tail)));
                }
                calcvba_host_api::Session::call_bound(self, &self.bound[index], &values, max_steps)
                    .map_err(neutral::adapter_error)
            },
        )
    }
}
type SplitModule = (
    calcvba_host::Module,
    Vec<(Function, calcvba_host::Procedure)>,
    Option<Arc<spreadsheet::SpreadsheetHost>>,
    bool,
);
fn split_module(
    module: Module,
    model: Option<std::rc::Rc<dyn calc_object_model::ObjectModel>>,
    values_only: bool,
) -> SplitModule {
    let spreadsheet = module.spreadsheet_objects.then(|| {
        let external = model.is_some();
        let resolved: std::rc::Rc<dyn calc_object_model::ObjectModel> = model
            .unwrap_or_else(|| std::rc::Rc::new(crate::invocation_model::InvocationModel::new(values_only)));
        Arc::new(spreadsheet::SpreadsheetHost::new(
            module.com,
            module.late.clone(),
            resolved,
            external,
        ))
    });
    let engine = if let Some(host) = &spreadsheet {
        module.engine.with_host(crate::policy::wrap(host.clone()))
    } else {
        module.engine
    };
    (engine, module.functions, spreadsheet, module.event_project)
}
pub struct CodeCache(calcvba_host::CodeCache);
impl CodeCache {
    pub fn new(max_entries: usize, max_description_bytes: usize) -> Self {
        Self(calcvba_host::CodeCache::new(
            max_entries,
            max_description_bytes,
        ))
    }
    pub fn stats(&self) -> CacheStats {
        self.0.stats()
    }
    pub fn clear(&mut self) {
        self.0.clear();
    }
    pub fn prepare(
        &mut self,
        module: Module,
        backend: Backend,
        model: Option<std::rc::Rc<dyn calc_object_model::ObjectModel>>,
        values_only: bool,
    ) -> Result<OwnedSession, Error> {
        // The dynamic JIT currently drops a native WithEvents binding without
        // calling the host's subscription refusal. Decline that image before
        // initialization or user code; default admission can select the VM.
        if backend == Backend::Jit
            && !module.event_project
            && module
                .engine
                .event_routes()
                .iter()
                .any(|route| route.document_class.is_none() && route.native.is_some())
        {
            return Err(Error::UnsupportedBackend(
                "dynamic native WithEvents binding requires the interpreter (dc-snz)".into(),
            ));
        }
        let (module, functions, spreadsheet, event_project) = split_module(module, model, values_only);
        OwnedSession::wrap(
            self.0.prepare(module, backend).map_err(engine_error)?,
            functions,
            spreadsheet,
            event_project,
        )
    }
}

fn owning_type(ty: &OxTy) -> calcvba_host_api::ValueType {
    use calcvba_host_api::ValueType as T;
    match ty {
        OxTy::Long => T::Long,
        OxTy::Double => T::Double,
        OxTy::Bool => T::Boolean,
        OxTy::Str => T::String,
        OxTy::Variant => T::Variant,
        _ => unreachable!("document project signature was checked before preparation"),
    }
}
impl OwnedSession {
    fn call_owning(
        &self,
        index: usize,
        values: &[Variant],
        tail: &[Variant],
        max_steps: u64,
    ) -> Result<Variant, Error> {
        use calcvba_host_api as api;
        let procedure = &self.functions[index].1;
        let values = values
            .iter()
            .zip(&procedure.parameters)
            .map(|(value, parameter)| {
                Ok(match owning_type(&parameter.ty) {
                    api::ValueType::Long => api::Value::Long(value.as_i32().ok_or_else(|| {
                        Error::ValueConversion("Long event-project argument".into())
                    })?),
                    api::ValueType::Double => {
                        api::Value::Double(value.as_f64().ok_or_else(|| {
                            Error::ValueConversion("Double event-project argument".into())
                        })?)
                    }
                    api::ValueType::Boolean => {
                        api::Value::Boolean(value.as_bool().ok_or_else(|| {
                            Error::ValueConversion("Boolean event-project argument".into())
                        })?)
                    }
                    api::ValueType::String => api::Value::String(
                        value
                            .as_bstr()
                            .ok_or_else(|| {
                                Error::ValueConversion("String event-project argument".into())
                            })?
                            .into(),
                    ),
                    api::ValueType::Variant => api::Value::Variant(value.clone()),
                    _ => unreachable!("admitted owning value type"),
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let mut slots = values
            .iter()
            .map(|v| {
                v.borrow_slot()
                    .ok_or_else(|| Error::ValueConversion("event-project argument carrier".into()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        slots.extend(tail.iter().map(api::ValueSlot::Variant));
        let handle = neutral::handle(&self.bound[index]).map_err(neutral::adapter_error)?;
        let call =
            |context: &mut api::TypedContext| self.inner.call_values(handle, &slots, context);
        let returned = if crate::events::callback_active() {
            api::with_native_callback_context(call).map_err(neutral::adapter_error)?
        } else {
            call(&mut api::TypedContext {
                remaining_blocks: max_steps,
            })
        }
        .map_err(engine_error)?;
        Ok(match returned {
            api::Value::Long(v) => Variant::from_i32(v),
            api::Value::Double(v) => Variant::from_f64(v),
            api::Value::Boolean(v) => Variant::from_bool(v),
            api::Value::String(v) => Variant::from_string(v),
            api::Value::Variant(v) => v,
            _ => {
                return Err(Error::ValueConversion(
                    "event-project result carrier".into(),
                ));
            }
        })
    }
}

pub(super) mod neutral;

mod typed;
