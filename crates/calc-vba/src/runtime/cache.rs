//! Runtime composition over neutral code/session ownership; no spreadsheet adapter dependency.
use super::*;
pub struct OwnedSession {
    inner: oxvba_embed::OwnedSession,
    functions: Vec<(Function, oxvba_embed::Procedure)>,
    spreadsheet: Option<Arc<spreadsheet::SpreadsheetHost>>,
    bound: Vec<calcvba_host_api::BoundProcedure>,
    catalogue: Vec<calcvba_host_api::Procedure>,
    typed: Vec<Option<typed::TypedEntry>>,
    event_project: bool,
}
impl OwnedSession {
    fn wrap(
        inner: oxvba_embed::OwnedSession,
        functions: Vec<(Function, oxvba_embed::Procedure)>,
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
        if event_project {
            for ((_, procedure), bound) in functions.iter().zip(&bound) {
                inner
                    .bind_values(
                        neutral::handle(bound).map_err(neutral::adapter_error)?,
                        &procedure
                            .parameters
                            .iter()
                            .map(|p| owning_type(&p.ty))
                            .collect::<Vec<_>>(),
                        owning_type(procedure.return_type.as_ref().expect("function result")),
                    )
                    .map_err(engine_error)?;
            }
        }
        Ok(Self {
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
            |index, values| {
                if self.event_project {
                    return self.call_owning(index, values, max_steps);
                }
                calcvba_host_api::Session::call_bound(self, &self.bound[index], values, max_steps)
                    .map_err(neutral::adapter_error)
            },
        )
    }
}
type SplitModule = (
    oxvba_embed::Module,
    Vec<(Function, oxvba_embed::Procedure)>,
    Option<Arc<spreadsheet::SpreadsheetHost>>,
    bool,
);
fn split_module(module: Module) -> SplitModule {
    let spreadsheet = module
        .spreadsheet
        .as_ref()
        .map(|_| Arc::new(spreadsheet::SpreadsheetHost::new()));
    let engine = if let Some(host) = &spreadsheet {
        module.engine.with_host(host.clone())
    } else {
        module.engine
    };
    (engine, module.functions, spreadsheet, module.event_project)
}
pub struct CodeCache(oxvba_embed::CodeCache);
impl CodeCache {
    pub fn new(max_entries: usize, max_description_bytes: usize) -> Self {
        Self(oxvba_embed::CodeCache::new(
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
    pub fn prepare(&mut self, module: Module, backend: Backend) -> Result<OwnedSession, Error> {
        let (module, functions, spreadsheet, event_project) = split_module(module);
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
        let slots = values
            .iter()
            .map(|v| {
                v.borrow_slot()
                    .ok_or_else(|| Error::ValueConversion("event-project argument carrier".into()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let returned = self
            .inner
            .call_values(
                neutral::handle(&self.bound[index]).map_err(neutral::adapter_error)?,
                &slots,
                &mut api::TypedContext {
                    remaining_blocks: max_steps,
                },
            )
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

mod neutral;

mod typed;
