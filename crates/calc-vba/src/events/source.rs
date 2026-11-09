//! Canonical portable document event sources. Subscriptions are supplied by the runtime.
use super::types;
use calc_func::value::{CalcValue, ReferenceLike};
use calcvba_host_api::{
    self as api,
    object::{
        EventCallback, EventDescriptor, EventParameter, EventSubscription, HostObjectServices,
    },
};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::{Rc, Weak},
};

/// Borrowed only while a synchronous handler runs. Reads never calculate or add graph edges.
pub trait EventReadContext {
    fn read_reference(&self, reference: &ReferenceLike) -> Result<CalcValue, String>;
    fn resolve_reference(&self, _text: &str) -> Result<ReferenceLike, String> {
        Err("Range lookup without a mutable application owner is not modelled".into())
    }
    fn write_reference(&self, _reference: &ReferenceLike, _value: CalcValue) -> Result<(), String> {
        Err("Range writes without a mutable application owner are not modelled".into())
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
scoped_tls_hkt::scoped_thread_local!(static READ: for<'a> (&'a (dyn EventReadContext + 'a), u64, &'a RefCell<Option<String>>));

pub(crate) fn callback_active() -> bool {
    READ.is_set()
}

pub(crate) fn object_type(object: &oxvba_runtime::ObjectRef) -> Option<&'static str> {
    let object = api::HostObject::from_object_ref(object)?;
    match object.header().iid {
        types::APP => Some("Application"),
        types::RANGE => Some("Range"),
        _ => None,
    }
}

fn event_operation<T>(
    call: impl FnOnce(&dyn EventReadContext) -> Result<T, String>,
) -> Result<T, i32> {
    if !READ.is_set() || !crate::runtime::spreadsheet::event_view_active() {
        crate::runtime::spreadsheet::record_capability_refusal(
            "an application command outside its admitted document event",
        );
        return Err(1004);
    }
    READ.with(|(context, _, refused)| {
        call(context).map_err(|reason| {
            refused.borrow_mut().get_or_insert(reason);
            1004
        })
    })
}

/// Persistent identity only. Every slot resolves its authority from the current
/// scoped event loan; no workbook borrow or budget is retained in an object.
pub(crate) fn application_object() -> Result<api::HostObject, String> {
    use api::{HostSlot, Value, ValueSlot, ValueType};
    let mut slots = vec![
        HostSlot::typed(
            vec![ValueType::String],
            ValueType::Interface(types::RANGE),
            |_, args| {
                let [ValueSlot::String(text)] = args else {
                    return Err(13);
                };
                let text = text.as_str();
                let reference = event_operation(|context| context.resolve_reference(&text))?;
                let lease = READ.with(|(_, lease, _)| lease);
                Ok(Value::Object(Some(
                    event_range(reference, lease).map_err(|_| 1004)?,
                )))
            },
        )
        .map_err(|e| e.to_string())?,
    ];
    for command in [
        super::CalculationCommand::Dirty,
        super::CalculationCommand::Full,
        super::CalculationCommand::Rebuild,
    ] {
        slots.push(
            HostSlot::typed(vec![], ValueType::Variant, move |budget, _| {
                api::with_native_call(budget, || {
                    event_operation(|context| context.calculate(command.clone()))
                })?;
                Ok(Value::Variant(api::Variant::empty()))
            })
            .map_err(|e| e.to_string())?,
        );
    }
    slots.push(
        HostSlot::typed(vec![], ValueType::Boolean, |_, _| {
            event_operation(|context| context.events_enabled()).map(Value::Boolean)
        })
        .map_err(|e| e.to_string())?,
    );
    slots.push(
        HostSlot::typed(vec![ValueType::Boolean], ValueType::Variant, |_, args| {
            let [ValueSlot::Boolean(enabled)] = args else {
                return Err(13);
            };
            event_operation(|context| context.set_events_enabled(*enabled))?;
            Ok(Value::Variant(api::Variant::empty()))
        })
        .map_err(|e| e.to_string())?,
    );
    Ok(api::HostObject::new(types::APP, slots))
}

struct EventSpreadsheet;
impl crate::runtime::SpreadsheetContext for EventSpreadsheet {
    fn event_view(&self) -> bool {
        true
    }
    fn refuse(&self, reason: &'static str) -> String {
        if READ.is_set() {
            READ.with(|(_, _, refused)| {
                refused.borrow_mut().get_or_insert_with(|| reason.into());
            });
        }
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
            .refuse("legacy Range cells in a document event are not modelled")
            .into())
    }
    fn area(
        &self,
        _: crate::runtime::Range,
        _: u32,
    ) -> Result<crate::runtime::Range, crate::runtime::ContextError> {
        Err(self
            .refuse("legacy Range areas in a document event are not modelled")
            .into())
    }
    fn read_range(
        &self,
        _: crate::runtime::Range,
    ) -> Result<crate::runtime::Value, crate::runtime::ContextError> {
        Err(self
            .refuse("legacy Range reads in a document event are not modelled")
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

struct Hub {
    iid: [u8; 16],
    next: Cell<u64>,
    callbacks: RefCell<BTreeMap<u64, (EventDescriptor, EventCallback)>>,
}
/// A document's events, and its `Name` (the one member a worksheet function reads of it).
struct Services(Rc<Hub>, Rc<RefCell<String>>);
struct Subscription {
    hub: Weak<Hub>,
    id: u64,
}
impl Drop for Subscription {
    fn drop(&mut self) {
        if let Some(hub) = self.hub.upgrade() {
            hub.callbacks.borrow_mut().remove(&self.id);
        }
    }
}
impl HostObjectServices for Services {
    fn invoke_dynamic(
        &self,
        call: &api::object::DynamicMemberCall,
        arguments: &mut [api::Variant],
        _: &mut api::TypedContext,
    ) -> Result<api::Variant, api::TypedFault> {
        if matches!(call.kind, api::object::DynamicMemberKind::Get)
            && arguments.is_empty()
            && call
                .name
                .as_deref()
                .is_some_and(|name| name.eq_ignore_ascii_case("Name"))
        {
            return Ok(api::Variant::from_string(self.1.borrow().clone()));
        }
        Err(api::TypedFault {
            number: 438,
            description: "a document member outside Name is not modelled".into(),
            source: None,
            help_file: None,
            help_context: None,
        })
    }
    fn subscribe_event(
        &self,
        descriptor: &EventDescriptor,
        callback: EventCallback,
    ) -> Result<EventSubscription, i32> {
        let valid = if self.0.iid == types::SHEET {
            descriptor.interface == types::SHEET_EVENTS
                && match descriptor.dispatch_id {
                    1 => {
                        descriptor.name.eq_ignore_ascii_case("Change")
                            && descriptor.parameters == [EventParameter::Object(Some(types::RANGE))]
                    }
                    2 => {
                        descriptor.name.eq_ignore_ascii_case("Calculate")
                            && descriptor.parameters.is_empty()
                    }
                    _ => false,
                }
        } else if self.0.iid == types::BOOK {
            descriptor.interface == types::BOOK_EVENTS
                && match descriptor.dispatch_id {
                    1 => {
                        descriptor.name.eq_ignore_ascii_case("BeforeSave")
                            && descriptor.parameters
                                == [EventParameter::Boolean, EventParameter::BooleanByRef]
                    }
                    2 => {
                        descriptor.name.eq_ignore_ascii_case("Open")
                            && descriptor.parameters.is_empty()
                    }
                    3 => {
                        descriptor.name.eq_ignore_ascii_case("AddinInstall")
                            && descriptor.parameters.is_empty()
                    }
                    _ => false,
                }
        } else {
            false
        };
        if !valid {
            return Err(13);
        }
        let id = self.0.next.get();
        self.0.next.set(id.checked_add(1).ok_or(7)?);
        self.0
            .callbacks
            .borrow_mut()
            .insert(id, (descriptor.clone(), callback));
        Ok(EventSubscription::new(Subscription {
            hub: Rc::downgrade(&self.0),
            id,
        }))
    }
}

pub struct EventSource {
    hub: Rc<Hub>,
    object: api::HostObject,
    name: Rc<RefCell<String>>,
}
impl EventSource {
    pub fn worksheet(name: &str) -> Self {
        Self::new(types::SHEET, name)
    }
    pub fn workbook(name: &str) -> Self {
        Self::new(types::BOOK, name)
    }
    pub fn open(&self, read: &dyn EventReadContext, max_steps: u64) -> Result<(), String> {
        self.raise("Open", &mut [], read, max_steps, next_lease())
    }
    /// `Workbook_AddinInstall`: the workbook was installed as an add-in.
    pub fn addin_install(&self, read: &dyn EventReadContext, max_steps: u64) -> Result<(), String> {
        self.raise("AddinInstall", &mut [], read, max_steps, next_lease())
    }
    fn new(iid: [u8; 16], name: &str) -> Self {
        let hub = Rc::new(Hub {
            iid,
            next: Cell::new(1),
            callbacks: RefCell::default(),
        });
        let name = Rc::new(RefCell::new(name.to_owned()));
        let current_name = name.clone();
        let slot = api::HostSlot::typed(vec![], api::ValueType::String, move |_, _| {
            Ok(api::Value::String(current_name.borrow().clone().into()))
        })
        .expect("declared name slot");
        // SAFETY: payload and service retain this canonical Rc allocation; every table
        // view is the same declared DNA interface. No native COM pointer is fabricated.
        let object = unsafe {
            api::HostObject::new_with_services(
                iid,
                vec![slot],
                hub.clone(),
                Rc::as_ptr(&hub).cast(),
                Rc::new(Services(hub.clone(), name.clone())),
            )
        };
        Self { hub, object, name }
    }
    /// Refresh the document identity without replacing its object or subscriptions.
    pub fn set_name(&self, name: &str) {
        *self.name.borrow_mut() = name.to_owned();
    }
    pub(super) fn object(&self) -> &api::HostObject {
        &self.object
    }
    pub fn subscription_count(&self) -> usize {
        self.hub.callbacks.borrow().len()
    }

    fn raise(
        &self,
        name: &str,
        arguments: &mut [api::Value],
        context: &dyn EventReadContext,
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
        context: &dyn EventReadContext,
        budget: &mut api::TypedContext,
        lease: u64,
        nested: bool,
    ) -> Result<(), String> {
        let ids = self
            .hub
            .callbacks
            .borrow()
            .iter()
            .filter_map(|(id, (descriptor, _))| {
                descriptor.name.eq_ignore_ascii_case(name).then_some(*id)
            })
            .collect::<Vec<_>>();
        let refused = RefCell::new(None);
        let result = crate::runtime::spreadsheet::with_context(&EventSpreadsheet, || {
            READ.set((context, lease, &refused), || {
                api::with_native_call(budget, || {
                    for id in ids {
                        // Release the source table before entering user code. A reset/drop can
                        // unsubscribe a later callback; a newly subscribed callback waits for a new event.
                        let callback = self
                            .hub
                            .callbacks
                            .borrow()
                            .get(&id)
                            .map(|(_, callback)| callback.clone());
                        if let Some(callback) = callback {
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

    pub fn change(
        &self,
        target: ReferenceLike,
        context: &dyn EventReadContext,
        max_steps: u64,
    ) -> Result<(), String> {
        let lease = next_lease();
        let target = event_range(target, lease)?;
        self.raise(
            "Change",
            &mut [api::Value::Object(Some(target))],
            context,
            max_steps,
            lease,
        )
    }
    pub fn calculate(&self, context: &dyn EventReadContext, max_steps: u64) -> Result<(), String> {
        self.raise("Calculate", &mut [], context, max_steps, next_lease())
    }
    pub fn before_save(
        &self,
        save_as_ui: bool,
        context: &dyn EventReadContext,
        max_steps: u64,
    ) -> Result<bool, String> {
        let mut args = [api::Value::Boolean(save_as_ui), api::Value::Boolean(false)];
        self.raise("BeforeSave", &mut args, context, max_steps, next_lease())?;
        match args[1] {
            api::Value::Boolean(cancel) => Ok(cancel),
            _ => Err("invalid BeforeSave Cancel writeback (dc-hsa5.12)".into()),
        }
    }
}
fn next_lease() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
        .expect("event read lease exhausted")
}
fn in_lease<T>(
    lease: u64,
    call: impl FnOnce(&dyn EventReadContext) -> Result<T, i32>,
) -> Result<T, i32> {
    if !READ.is_set() || !crate::runtime::spreadsheet::event_view_active() {
        crate::runtime::spreadsheet::record_capability_refusal(
            "an event Target outside its admitted event view",
        );
        return Err(91);
    }
    READ.with(|(context, current, refused)| {
        if current == lease {
            call(context)
        } else {
            refused
                .borrow_mut()
                .get_or_insert_with(|| "an event Target outside its admitted event view".into());
            Err(91)
        }
    })
}
fn event_range(reference: ReferenceLike, lease: u64) -> Result<api::HostObject, String> {
    let info = crate::range_metadata(&reference);
    let write_reference = reference.clone();
    let calculation_reference = reference.clone();
    let mut slots = Vec::new();
    slots.push(
        api::HostSlot::typed(vec![], api::ValueType::Variant, move |_, _| {
            in_lease(lease, |context| {
                let value = context.read_reference(&reference).map_err(|reason| {
                    READ.with(|(_, _, refused)| {
                        refused.borrow_mut().get_or_insert(reason);
                    });
                    1004
                })?;
                let value = crate::to_vba(&value).map_err(|reason| {
                    READ.with(|(_, _, refused)| {
                        refused.borrow_mut().get_or_insert(reason);
                    });
                    13
                })?;
                let value = crate::runtime::to_variant(&value).map_err(|reason| {
                    READ.with(|(_, _, refused)| {
                        refused
                            .borrow_mut()
                            .get_or_insert_with(|| reason.to_string());
                    });
                    13
                })?;
                Ok(api::Value::Variant(value))
            })
        })
        .map_err(|e| e.to_string())?,
    );
    for n in [info.row, info.column] {
        slots.push(
            api::HostSlot::typed(vec![], api::ValueType::Long, move |_, _| {
                in_lease(lease, |_| Ok(api::Value::Long(n as i32)))
            })
            .map_err(|e| e.to_string())?,
        );
    }
    let cells = info.cells;
    slots.push(
        api::HostSlot::typed(vec![], api::ValueType::Long, move |_, _| {
            in_lease(lease, |_| {
                Ok(api::Value::Long(i32::try_from(cells).map_err(|_| 6)?))
            })
        })
        .map_err(|e| e.to_string())?,
    );
    slots.push(
        api::HostSlot::typed(vec![], api::ValueType::String, move |_, _| {
            in_lease(lease, |_| {
                Ok(api::Value::String(info.address.clone().into()))
            })
        })
        .map_err(|e| e.to_string())?,
    );
    slots.push(
        api::HostSlot::typed(
            vec![api::ValueType::Variant],
            api::ValueType::Variant,
            move |budget, args| {
                let [api::ValueSlot::Variant(value)] = args else {
                    return Err(13);
                };
                let value = event_operation(|_| {
                    crate::runtime::from_variant(value, false)
                        .map_err(|e| e.to_string())
                        .and_then(crate::from_vba)
                })?;
                api::with_native_call(budget, || {
                    in_lease(lease, |context| {
                        context
                            .write_reference(&write_reference, value)
                            .map_err(|reason| {
                                READ.with(|(_, _, refused)| {
                                    refused.borrow_mut().get_or_insert(reason);
                                });
                                1004
                            })?;
                        Ok(api::Value::Variant(api::Variant::empty()))
                    })
                })
            },
        )
        .map_err(|e| e.to_string())?,
    );
    slots.push(
        api::HostSlot::typed(vec![], api::ValueType::Variant, move |budget, _| {
            api::with_native_call(budget, || {
                in_lease(lease, |context| {
                    context
                        .calculate(super::CalculationCommand::Range(
                            calculation_reference.clone(),
                        ))
                        .map_err(|reason| {
                            READ.with(|(_, _, refused)| {
                                refused.borrow_mut().get_or_insert(reason);
                            });
                            1004
                        })?;
                    Ok(api::Value::Variant(api::Variant::empty()))
                })
            })
        })
        .map_err(|e| e.to_string())?,
    );
    Ok(api::HostObject::new(types::RANGE, slots))
}
