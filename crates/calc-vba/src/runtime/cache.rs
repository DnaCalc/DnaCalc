//! Runtime composition over neutral code/session ownership; no spreadsheet adapter dependency.
use super::*;
pub struct OwnedSession {
    inner: oxvba_embed::OwnedSession,
    functions: Vec<(Function, oxvba_embed::Procedure)>,
    spreadsheet: Option<Arc<spreadsheet::SpreadsheetHost>>,
    bound: Vec<calcvba_host_api::BoundProcedure>,
    catalogue: Vec<calcvba_host_api::Procedure>,
    typed: Vec<Option<typed::TypedEntry>>,
}
impl OwnedSession {
    fn wrap(
        inner: oxvba_embed::OwnedSession,
        functions: Vec<(Function, oxvba_embed::Procedure)>,
        spreadsheet: Option<Arc<spreadsheet::SpreadsheetHost>>,
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
        Ok(Self {
            typed,
            catalogue,
            inner,
            functions,
            spreadsheet,
            bound,
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
    (engine, module.functions, spreadsheet)
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
        let (module, functions, spreadsheet) = split_module(module);
        OwnedSession::wrap(
            self.0.prepare(module, backend).map_err(engine_error)?,
            functions,
            spreadsheet,
        )
    }
}

mod neutral;

mod typed;
