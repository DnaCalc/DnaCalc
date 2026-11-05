//! Explicit workbook projects and runtime-owned automatic document events.
//! Hosts attach document identities; the compiler selects private handlers.
use crate::{Backend, Module, OwnedSession, UserFunctions};
use std::rc::Rc;
mod source;
#[cfg(test)]
mod tests;
pub(crate) mod types;
pub use source::{EventReadContext, EventSource};
pub(crate) use source::{application_object, callback_active, object_type};

/// Commands admitted by a mutable document-event owner. Ordinary worksheet UDFs
/// have no authority to execute these operations.
#[derive(Clone, Debug)]
pub enum CalculationCommand {
    Dirty,
    Full,
    Rebuild,
    Range(calc_func::value::ReferenceLike),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModuleKind {
    Standard,
    Class,
    Worksheet,
    Workbook,
}
#[derive(Clone, Debug)]
pub struct ProjectModule {
    pub name: String,
    pub source: String,
    pub kind: ModuleKind,
}

pub struct LoadedProject {
    pub(crate) session: Rc<OwnedSession>,
    pub(crate) functions: UserFunctions,
}
impl LoadedProject {
    /// UDF entries and automatic handlers share this exact owning project session.
    pub fn functions(&self) -> UserFunctions {
        self.functions.clone()
    }
    pub fn attach_document(&self, module: &str, source: &EventSource) -> Result<(), String> {
        calcvba_host_api::Session::attach_document(&*self.session, module, source.object())
            .map_err(|e| format!("automatic document attachment ({module}, dc-hsa5.12): {e}"))
    }
    pub fn reset(&self) -> Result<(), String> {
        calcvba_host_api::Session::reset(&*self.session).map_err(|e| e.to_string())
    }
}

pub fn load_project(
    modules: &[ProjectModule],
    max_steps: u64,
    backend: Option<Backend>,
) -> Result<LoadedProject, String> {
    load_project_impl(modules, max_steps, backend, false, true)
}

/// An opened workbook's worksheet lookup also admits its private standard-module
/// functions, as recorded by dc-snz.7. VBA-to-VBA visibility stays unchanged.
pub fn load_file_project(
    modules: &[ProjectModule],
    max_steps: u64,
    backend: Option<Backend>,
) -> Result<LoadedProject, String> {
    load_file_project_with_events(modules, max_steps, backend, true)
}

pub fn load_file_project_with_events(
    modules: &[ProjectModule],
    max_steps: u64,
    backend: Option<Backend>,
    automatic_events: bool,
) -> Result<LoadedProject, String> {
    load_project_impl(modules, max_steps, backend, true, automatic_events)
}

fn load_project_impl(
    modules: &[ProjectModule],
    max_steps: u64,
    backend: Option<Backend>,
    private_worksheet_lookup: bool,
    automatic_events: bool,
) -> Result<LoadedProject, String> {
    if modules.is_empty()
        || modules.len() > 1024
        || modules.iter().any(|m| m.source.len() > 1_048_576)
        || modules.iter().map(|m| m.source.len()).sum::<usize>() > 4 * 1_048_576
    {
        return Err("VBA project exceeds its source/module limits (dc-hsa5.12)".into());
    }
    let module = Module::load_event_project(modules, private_worksheet_lookup, automatic_events)
        .map_err(|e| format!("document project (dc-hsa5.12): {e}"))?;
    crate::prepare_project(module, max_steps, backend)
        .map_err(|e| format!("document project (dc-hsa5.12): {e}"))
}
