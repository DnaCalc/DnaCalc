//! Document event sources backed by `calc_object_model::ObjectModel` (dc-piw0.5).
//!
//! Each `EventSource` wraps an `ObjectRef` (`_Worksheet` or `_Workbook`) as a service-backed
//! `calcvba_host_api::HostObject` using the real descriptor IID and an `EventHub` validating
//! real Excel `DocEvents` / `WorkbookEvents` DISPIDs.
use crate::{invocation_model, projection};
use calc_func::value::{CalcValue, ReferenceLike};
use calc_object_model::{AutoError, ObjectModel, ObjectRef};
use calcvba_host_api as api;
use std::{
    cell::RefCell,
    rc::Rc,
};

/// Borrowed only while a synchronous standalone handler runs (tests and `load_office_project`).
/// Workbook hosts lend their `HostSession` directly through `WorkbookAutomation::with_event_session`.
pub trait EventReadContext {
    fn read_reference(&self, reference: &ReferenceLike) -> Result<CalcValue, String>;
    fn resolve_reference(&self, _text: &str) -> Result<ReferenceLike, String> {
        Err("Range lookup without a mutable application owner is not modelled".into())
    }
    fn write_reference(&self, _reference: &ReferenceLike, _value: CalcValue) -> Result<(), String> {
        Err("Range writes without a mutable application owner is not modelled".into())
    }
    fn calculate(&self, _command: super::CalculationCommand) -> Result<(), String> {
        Err("calculation without a mutable application owner is not modelled".into())
    }
    fn events_enabled(&self) -> Result<bool, String> {
        Err("EnableEvents without a mutable application owner is not modelled".into())
    }
    fn set_events_enabled(&self, _enabled: bool) -> Result<(), String> {
        Err("EnableEvents without a mutable application owner is not modelled".into())
    }
}

scoped_tls_hkt::scoped_thread_local!(static EVENT_SCOPE: for<'a> (
    Option<&'a (dyn EventReadContext + 'a)>, u64, &'a RefCell<Option<String>>
));

pub(crate) fn callback_active() -> bool {
    EVENT_SCOPE.is_set()
}

pub(crate) fn lease_is_active(expected: u64) -> bool {
    EVENT_SCOPE.is_set()
        && crate::runtime::spreadsheet::event_view_active()
        && EVENT_SCOPE.with(|(_, current, _)| current == expected)
}

pub(crate) fn record_event_refusal(reason: String) {
    if EVENT_SCOPE.is_set() {
        EVENT_SCOPE.with(|(_, _, refused)| {
            refused.borrow_mut().get_or_insert(reason);
        });
    }
}

pub(crate) fn with_event_read<T>(
    call: impl FnOnce(&dyn EventReadContext) -> Result<T, String>,
) -> Result<T, AutoError> {
    if !EVENT_SCOPE.is_set() || !crate::runtime::spreadsheet::event_view_active() {
        return Err(AutoError::not_modelled(
            "an application command outside its admitted document event",
            "dc-hsa5.12",
        ));
    }
    EVENT_SCOPE.with(|(context, _, _)| {
        let Some(context) = context else {
            return Err(AutoError::not_modelled(
                "standalone event read context is unavailable",
                "dc-hsa5.12",
            ));
        };
        call(context).map_err(|reason| AutoError::not_modelled(reason, "dc-hsa5.12"))
    })
}

struct EventSpreadsheet;
impl crate::runtime::SpreadsheetContext for EventSpreadsheet {
    fn event_view(&self) -> bool {
        true
    }
    fn refuse(&self, reason: &'static str) -> String {
        record_event_refusal(reason.into());
        reason.into()
    }
    fn caller(&self) -> Option<crate::runtime::Range> {
        self.refuse("Application.Caller in a document event is not modelled");
        None
    }
    fn metadata(
        &self,
        _: crate::runtime::Range,
    ) -> Result<crate::runtime::RangeInfo, crate::runtime::ContextError> {
        Err(self
            .refuse("legacy Range metadata in a document event is not modelled")
            .into())
    }
    fn cell(
        &self,
        _: crate::runtime::Range,
        _: u32,
        _: u32,
    ) -> Result<crate::runtime::Range, crate::runtime::ContextError> {
        Err(self
            .refuse("legacy Range cells in a document event is not modelled")
            .into())
    }
    fn area(
        &self,
        _: crate::runtime::Range,
        _: u32,
    ) -> Result<crate::runtime::Range, crate::runtime::ContextError> {
        Err(self
            .refuse("legacy Range areas in a document event is not modelled")
            .into())
    }
    fn read_range(
        &self,
        _: crate::runtime::Range,
    ) -> Result<crate::runtime::Value, crate::runtime::ContextError> {
        Err(self
            .refuse("legacy Range reads in a document event is not modelled")
            .into())
    }
    fn worksheet_function(
        &self,
        _: &str,
        _: &[crate::runtime::Value],
    ) -> Result<crate::runtime::Value, crate::runtime::ContextError> {
        Err(self
            .refuse("WorksheetFunction callbacks in document events are not modelled")
            .into())
    }
    fn set_volatile(&self, _: bool) -> Result<(), crate::runtime::ContextError> {
        Ok(())
    }
}

pub struct EventSource {
    hub: Rc<projection::EventHub>,
    object: api::HostObject,
    model: Rc<dyn ObjectModel>,
    standalone: Option<Rc<invocation_model::InvocationModel>>,
    name: Option<Rc<RefCell<String>>>,
}

impl EventSource {
    pub fn worksheet(name: &str) -> Self {
        Self::new_standalone("Worksheet", name)
    }

    pub fn workbook(name: &str) -> Self {
        Self::new_standalone("Workbook", name)
    }

    /// Construct a document event source backed directly by a host `ObjectModel`
    /// (`WorkbookAutomation`) for a workbook or worksheet `ObjectRef`.
    pub fn for_object(model: Rc<dyn ObjectModel>, target: ObjectRef) -> Result<Self, String> {
        let hub = projection::EventHub::new(target.interface);
        let object = projection::wrap_object(model.clone(), target, None, Some(hub.clone()));
        Ok(Self {
            hub,
            object,
            model,
            standalone: None,
            name: None,
        })
    }

    fn new_standalone(interface: &str, name: &str) -> Self {
        let name_cell = Rc::new(RefCell::new(name.to_owned()));
        let standalone = Rc::new(invocation_model::InvocationModel::with_document_name(
            false,
            Some(name_cell.clone()),
        ));
        let target = invocation_model::standalone_document_object(interface);
        let hub = projection::EventHub::new(target.interface);
        let model: Rc<dyn ObjectModel> = standalone.clone();
        let object = projection::wrap_object(model.clone(), target, None, Some(hub.clone()));
        Self {
            hub,
            object,
            model,
            standalone: Some(standalone),
            name: Some(name_cell),
        }
    }

    /// Refresh the standalone document identity without replacing its object or subscriptions.
    pub fn set_name(&self, name: &str) {
        if let Some(cell) = &self.name {
            *cell.borrow_mut() = name.to_owned();
        }
    }

    pub(super) fn object(&self) -> &api::HostObject {
        &self.object
    }

    pub fn subscription_count(&self) -> usize {
        self.hub.subscription_count()
    }

    pub fn open(&self, read: &dyn EventReadContext, max_steps: u64) -> Result<(), String> {
        self.raise("Open", &mut [], Some(read), max_steps, next_lease())
    }

    /// `Workbook_Open` against the attached host `ObjectModel` (no `EventReadContext` needed).
    pub fn open_model(&self, max_steps: u64) -> Result<(), String> {
        self.raise("Open", &mut [], None, max_steps, next_lease())
    }

    /// `Workbook_AddinInstall`: the workbook was installed as an add-in.
    pub fn addin_install(&self, read: &dyn EventReadContext, max_steps: u64) -> Result<(), String> {
        self.raise("AddinInstall", &mut [], Some(read), max_steps, next_lease())
    }

    pub fn addin_install_model(&self, max_steps: u64) -> Result<(), String> {
        self.raise("AddinInstall", &mut [], None, max_steps, next_lease())
    }

    pub fn change(
        &self,
        target: ReferenceLike,
        context: &dyn EventReadContext,
        max_steps: u64,
    ) -> Result<(), String> {
        let Some(standalone) = &self.standalone else {
            return Err("change(ReferenceLike) requires a standalone EventSource".into());
        };
        let lease = next_lease();
        let target_ref = standalone.register_event_range(target);
        let target_obj = projection::wrap_object(self.model.clone(), target_ref, Some(lease), None);
        self.raise(
            "Change",
            &mut [api::Value::Object(Some(target_obj))],
            Some(context),
            max_steps,
            lease,
        )
    }

    /// `Worksheet_Change` against the attached host `ObjectModel`, wrapping an already-acquired
    /// `Range` `ObjectRef` under a per-event lease.
    pub fn change_object(&self, target: ObjectRef, max_steps: u64) -> Result<(), String> {
        let lease = next_lease();
        let target_obj = projection::wrap_object(self.model.clone(), target, Some(lease), None);
        self.raise(
            "Change",
            &mut [api::Value::Object(Some(target_obj))],
            None,
            max_steps,
            lease,
        )
    }

    pub fn calculate(&self, context: &dyn EventReadContext, max_steps: u64) -> Result<(), String> {
        self.raise("Calculate", &mut [], Some(context), max_steps, next_lease())
    }

    pub fn calculate_model(&self, max_steps: u64) -> Result<(), String> {
        self.raise("Calculate", &mut [], None, max_steps, next_lease())
    }

    pub fn before_save(
        &self,
        save_as_ui: bool,
        context: &dyn EventReadContext,
        max_steps: u64,
    ) -> Result<bool, String> {
        let mut args = [api::Value::Boolean(save_as_ui), api::Value::Boolean(false)];
        self.raise(
            "BeforeSave",
            &mut args,
            Some(context),
            max_steps,
            next_lease(),
        )?;
        match args[1] {
            api::Value::Boolean(cancel) => Ok(cancel),
            _ => Err("invalid BeforeSave Cancel writeback (dc-hsa5.12)".into()),
        }
    }

    pub fn before_save_model(&self, save_as_ui: bool, max_steps: u64) -> Result<bool, String> {
        let mut args = [api::Value::Boolean(save_as_ui), api::Value::Boolean(false)];
        self.raise("BeforeSave", &mut args, None, max_steps, next_lease())?;
        match args[1] {
            api::Value::Boolean(cancel) => Ok(cancel),
            _ => Err("invalid BeforeSave Cancel writeback (dc-hsa5.12)".into()),
        }
    }

    fn raise(
        &self,
        name: &str,
        arguments: &mut [api::Value],
        context: Option<&dyn EventReadContext>,
        max_steps: u64,
        lease: u64,
    ) -> Result<(), String> {
        if callback_active() {
            return api::with_native_callback_context(|budget| {
                self.raise_in_context(name, arguments, context, budget, lease, true)
            })
            .map_err(|e| format!("nested event context (dc-hsa5.12): {e}"))?;
        }
        let mut budget = api::TypedContext {
            remaining_blocks: max_steps,
        };
        self.raise_in_context(name, arguments, context, &mut budget, lease, false)
    }

    fn raise_in_context(
        &self,
        name: &str,
        arguments: &mut [api::Value],
        context: Option<&dyn EventReadContext>,
        budget: &mut api::TypedContext,
        lease: u64,
        nested: bool,
    ) -> Result<(), String> {
        let ids = self.hub.matching_ids(name);
        let refused = RefCell::new(None);
        let result = crate::runtime::spreadsheet::with_context(&EventSpreadsheet, || {
            EVENT_SCOPE.set((context, lease, &refused), || {
                api::with_native_call(budget, || {
                    for id in ids {
                        // Release the source table before entering user code. A reset/drop can
                        // unsubscribe a later callback; a newly subscribed callback waits for a new event.
                        if let Some(callback) = self.hub.callback(id) {
                            callback(arguments).map_err(|number| {
                                crate::runtime::Error::Backend(format!(
                                    "VBA {name} event failed ({number}, dc-hsa5.12)"
                                ))
                            })?;
                        }
                    }
                    Ok(())
                })
            })
        })
        .map_err(|e| format!("{e} (dc-hsa5.12)"));
        // Nested handlers leave the fatal fault on the shared context so the
        // triggering VBA call cannot catch it and continue. Only the outer event
        // boundary drains it before a later top-level event receives a new budget.
        let fault = api::take_unhandled_event_fault(budget);
        let fault_reason = fault.as_ref().map(|fault| {
            format!(
                "VBA {name} event failed ({}: {}{}, dc-hsa5.12)",
                fault.number,
                fault.description,
                fault
                    .source
                    .as_ref()
                    .map(|source| format!(" [{source}]"))
                    .unwrap_or_default(),
            )
        });
        if nested && let Some(fault) = fault {
            api::publish_unhandled_event_fault(budget, fault);
        }
        match (refused.into_inner(), fault_reason) {
            (Some(reason), _) => Err(format!("{reason} (dc-hsa5.12)")),
            (None, Some(reason)) => Err(reason),
            (None, None) => result,
        }
    }
}

fn next_lease() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
        .expect("event read lease exhausted")
}
