//! The Rust COM projection: identity/ABI translation plus the COM-owned Office composition.
use crate::{
    application::ApplicationServices,
    events::Connections,
    office::Office,
    runtime,
    values::{self, Objects},
};
use calc_object_model::{
    AutoError, AutoValue, Call, Descriptor, EventSource, InvokeKind, ObjectKey, ObjectModel,
    ObjectRef,
    profile::{ApplicationHost, values_only::ValuesOnly},
};
use calc_value::CalcValue;
use calc_xll_host::XllHost;
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    ptr,
    rc::Rc,
    sync::{Arc, atomic::Ordering},
};
use windows::{
    Win32::{Foundation::*, System::Com::*, UI::Accessibility::LresultFromObject},
    core::{IUnknown, Interface},
};

pub(crate) struct State {
    pub apartment: Arc<runtime::Apartment>,
    pub hwnd: usize,
    pub host: Rc<XllHost>,
    pub model: RefCell<Rc<dyn ObjectModel>>,
    pub office: Office,
    pub connections: Connections,
    pub wrappers: RefCell<HashMap<usize, ObjectRef>>,
    foreign: RefCell<HashMap<u64, Foreign>>,
    next_foreign: Cell<u64>,
}
struct Foreign {
    dispatch: IDispatch,
    transfer: bool,
}

/// One lease per object occurrence, including duplicate identities in arrays. A successfully
/// created wrapper owns its occurrence even if a later conversion or QueryInterface fails.
pub(crate) struct ResultValues<'a> {
    state: &'a Rc<State>,
    model: Rc<dyn ObjectModel>,
    values: &'a [AutoValue],
    wrapped: RefCell<Vec<ObjectRef>>,
}
impl Objects for ResultValues<'_> {
    fn wrap(&self, target: &ObjectRef) -> Result<IDispatch, AutoError> {
        let wrapper = self.state.wrap(target)?;
        self.wrapped.borrow_mut().push(target.clone());
        Ok(wrapper)
    }
    fn identify(&self, dispatch: &IDispatch) -> Result<AutoValue, AutoError> {
        self.state.identify(dispatch)
    }
    fn foreign(&self, key: u64) -> Result<IDispatch, AutoError> {
        self.state.foreign(key)
    }
}
impl Drop for ResultValues<'_> {
    fn drop(&mut self) {
        for value in self.values {
            self.state
                .discard_result(value, self.wrapped.get_mut(), &*self.model);
        }
        debug_assert!(self.wrapped.get_mut().is_empty());
    }
}

/// Only keys created by this call (and not explicitly taken by a member) are borrowed.
/// Nested calls have later keys, so their cleanup cannot release an outer call's arguments.
pub(crate) struct ArgumentScope<'a> {
    state: &'a State,
    first: u64,
}
impl Drop for ArgumentScope<'_> {
    fn drop(&mut self) {
        let keys: Vec<_> = self
            .state
            .foreign
            .borrow()
            .iter()
            .filter_map(|(&key, held)| (key >= self.first && !held.transfer).then_some(key))
            .collect();
        for key in keys {
            let held = self.state.foreign.borrow_mut().remove(&key);
            // A client's Release may reenter the projection. Never hold the table borrow.
            drop(held);
        }
    }
}
impl State {
    pub fn argument_scope(&self) -> ArgumentScope<'_> {
        ArgumentScope {
            state: self,
            first: self.next_foreign.get(),
        }
    }
    fn hold_foreign(&self, dispatch: IDispatch, transfer: bool) -> Result<AutoValue, AutoError> {
        let key = self.next_foreign.get();
        self.next_foreign.set(
            key.checked_add(1)
                .ok_or(AutoError::Status(E_UNEXPECTED.0))?,
        );
        self.foreign
            .borrow_mut()
            .insert(key, Foreign { dispatch, transfer });
        Ok(AutoValue::Foreign(key))
    }
    /// Transfer the client's incoming lease into the COM-owned public-object field.
    pub fn take_foreign(&self, key: u64) -> Result<IDispatch, AutoError> {
        self.apartment.check()?;
        self.foreign
            .borrow_mut()
            .remove(&key)
            .map(|held| held.dispatch)
            .ok_or(AutoError::Disconnected)
    }
    /// A COM-owned result transfers its lease to the outgoing VARIANT, rather than pinning
    /// the provider again in the general foreign-value table after disconnection.
    pub fn foreign_result(&self, dispatch: IDispatch) -> Result<AutoValue, AutoError> {
        self.hold_foreign(dispatch, true)
    }
    pub fn result_values<'a>(self: &'a Rc<Self>, values: &'a [AutoValue]) -> ResultValues<'a> {
        ResultValues {
            state: self,
            model: self.model.borrow().clone(),
            values,
            wrapped: RefCell::default(),
        }
    }
    fn discard_result(
        &self,
        value: &AutoValue,
        wrapped: &mut Vec<ObjectRef>,
        model: &dyn ObjectModel,
    ) {
        match value {
            AutoValue::Object(object) => {
                if let Some(at) = wrapped.iter().position(|target| target == object) {
                    wrapped.swap_remove(at);
                } else {
                    model.released(object);
                }
            }
            AutoValue::Foreign(key) => {
                let transfer = self
                    .foreign
                    .borrow()
                    .get(key)
                    .is_some_and(|held| held.transfer);
                if transfer {
                    let removed = self.foreign.borrow_mut().remove(key);
                    drop(removed);
                }
            }
            AutoValue::Array(array) => {
                for value in &array.values {
                    self.discard_result(value, wrapped, model);
                }
            }
            _ => {}
        }
    }
    pub fn application(&self) -> ObjectRef {
        let model = self.model.borrow().clone();
        model.application()
    }
    pub fn call(
        self: &Rc<Self>,
        target: &ObjectRef,
        call: &Call<'_>,
    ) -> Result<AutoValue, AutoError> {
        self.apartment.check()?;
        if self.office.handles(target, call) {
            self.office.invoke(target, call)
        } else {
            let model = self.model.borrow().clone();
            model.invoke(target, call)
        }
    }
    pub fn record(&self, error: &AutoError) {
        let model = self.model.borrow().clone();
        model.record_refusal(error);
    }
    pub fn events_enabled(self: &Rc<Self>) -> Result<bool, AutoError> {
        let interface = runtime::id("Application");
        let member = Descriptor::get()
            .member(interface, "EnableEvents", InvokeKind::Get)
            .unwrap();
        let value = self.call(
            &self.application(),
            &Call {
                interface,
                member,
                args: vec![],
                lcid: 0,
            },
        )?;
        let _result = self.result_values(std::slice::from_ref(&value));
        match &value {
            AutoValue::Bool(enabled) => Ok(*enabled),
            _ => values::mismatch(),
        }
    }
}
impl Objects for Rc<State> {
    fn wrap(&self, target: &ObjectRef) -> Result<IDispatch, AutoError> {
        runtime::wrap(self, target)
    }
    fn identify(&self, dispatch: &IDispatch) -> Result<AutoValue, AutoError> {
        self.apartment.check()?;
        if let Some(target) = self
            .wrappers
            .borrow()
            .get(&(dispatch.as_raw() as usize))
            .cloned()
        {
            return Ok(AutoValue::Object(target));
        }
        let held = dispatch.clone();
        self.hold_foreign(held, false)
    }
    fn foreign(&self, key: u64) -> Result<IDispatch, AutoError> {
        self.apartment.check()?;
        let transfer = self
            .foreign
            .borrow()
            .get(&key)
            .is_some_and(|held| held.transfer);
        if transfer {
            return self.take_foreign(key);
        }
        let raw = self
            .foreign
            .borrow()
            .get(&key)
            .map(|held| held.dispatch.as_raw())
            .ok_or(AutoError::Disconnected)?;
        Ok(unsafe { IDispatch::from_raw_borrowed(&raw) }
            .unwrap()
            .clone())
    }
}

/// Lives on the workbook window's STA. Retained COM wrappers disconnect when this is dropped.
pub struct Projection {
    pub(crate) state: Rc<State>,
    services: Rc<ApplicationServices>,
}
impl Projection {
    pub fn new(hwnd: usize, host: Rc<XllHost>) -> Result<Self, String> {
        let services = Rc::new(ApplicationServices {
            hwnd,
            host: host.clone(),
            events: Cell::new(true),
            sheet: RefCell::new(None),
        });
        let model: Rc<dyn ObjectModel> = Rc::new(ValuesOnly::new(services.clone()));
        let state = Rc::new_cyclic(|weak| State {
            apartment: runtime::Apartment::new(),
            hwnd,
            host,
            model: RefCell::new(model),
            office: Office::new(weak.clone()),
            connections: RefCell::default(),
            wrappers: RefCell::default(),
            foreign: RefCell::default(),
            next_foreign: Cell::new(1),
        });
        Ok(Self { state, services })
    }
    pub fn application_host(&self) -> Rc<dyn ApplicationHost> {
        self.services.clone()
    }
    pub fn bind_model(&self, model: Rc<dyn ObjectModel>) -> Result<(), String> {
        self.state.apartment.check().map_err(message)?;
        // Binding is done on the STA before add-in activation. Drop a replaced model outside
        // the RefCell borrow so its cleanup can call the projection.
        let old = self.state.model.replace(model);
        drop(old);
        Ok(())
    }
    pub fn event_source(&self) -> &dyn EventSource {
        self
    }
    pub fn application(&self) -> Result<IDispatch, String> {
        self.state.wrap(&self.state.application()).map_err(message)
    }
    pub fn get_object(&self, wparam: usize, object_id: isize) -> isize {
        if object_id as i32 != -16 || self.state.apartment.check().is_err() {
            return 0;
        }
        let target = ObjectRef::new(runtime::id("Window"), ObjectKey::Window);
        let Ok(window) = self.state.wrap(&target) else {
            return 0;
        };
        unsafe { LresultFromObject(&IDispatch::IID, WPARAM(wparam), &window).0 }
    }
    pub fn verify_layout(&self) -> Result<(), String> {
        self.state.apartment.check().map_err(message)?;
        let application = self.application()?;
        unsafe {
            let mut text = ptr::null();
            let status = runtime::call_version(application.as_raw(), 0, &mut text);
            let text = windows::core::BSTR::from_raw(text);
            status.ok().map_err(|e| e.to_string())?;
            if text.to_string() != "16.0" {
                return Err("COM Version differs from the supplied application model".into());
            }
            let mut value = 0;
            runtime::call_hwnd(application.as_raw(), &mut value)
                .ok()
                .map_err(|e| e.to_string())?;
            if value != self.state.hwnd as i32 {
                return Err("COM Hwnd differs from the application window".into());
            }
        }
        let window = self
            .state
            .wrap(&ObjectRef::new(runtime::id("Window"), ObjectKey::Window))
            .map_err(message)?;
        let mut raw = ptr::null_mut();
        unsafe { window.query(&runtime::guid(runtime::id("Window")), &mut raw) }
            .ok()
            .map_err(|e| e.to_string())?;
        drop(unsafe { IUnknown::from_raw(raw) });
        Ok(())
    }
    pub fn enable_events(&self, enabled: bool) -> Result<(), String> {
        let interface = runtime::id("Application");
        let member = Descriptor::get()
            .member(interface, "EnableEvents", InvokeKind::Put)
            .unwrap();
        let value = self
            .state
            .call(
                &self.state.application(),
                &Call {
                    interface,
                    member,
                    args: vec![AutoValue::Bool(enabled)],
                    lcid: 0,
                },
            )
            .map_err(message)?;
        let _result = self.state.result_values(std::slice::from_ref(&value));
        Ok(())
    }
    pub fn after_calculate(&self) -> Result<(), String> {
        self.raise(
            &self.state.application(),
            "AppEvents",
            "AfterCalculate",
            &[],
        )
        .map_err(message)
    }
    /// Compatibility for the command-line workbook harness. The supplied model owns selection.
    pub fn set_sheet(&self, name: &str) -> Result<(), String> {
        // A values-only model answers ActiveSheet from this shell name; a bound workbook model
        // owns its selection and must already answer the same name.
        *self.services.sheet.borrow_mut() = Some(name.to_string());
        let value = self
            .member(&self.state.application(), "ActiveSheet", vec![])
            .map_err(message)?;
        let _result = self.state.result_values(std::slice::from_ref(&value));
        let AutoValue::Object(sheet) = &value else {
            return Err("ActiveSheet returned no worksheet".into());
        };
        let current = self.member(sheet, "Name", vec![]).map_err(message)?;
        let _current = self.state.result_values(std::slice::from_ref(&current));
        if current.as_text().as_deref() == Some(name) {
            Ok(())
        } else {
            Err("the supplied object model owns the active worksheet".into())
        }
    }
    /// Legacy single-cell event entry. Resolve the event objects through the supplied model.
    pub fn sheet_change(&self, row: u32, column: u32) -> Result<(), String> {
        let cell = calc_value::CellKey::new(calc_value::SheetId::from_parts(0, 0), row, column)
            .map_err(|e| format!("{e:?}"))?;
        // No workbook model answers here (the command-line harness): the target is an inline
        // reference, whose geometry the values-only profile answers.
        let descriptor = Descriptor::get();
        let range = ObjectRef::new(
            descriptor.default_interface(descriptor.id("Range").ok_or("no Range interface")?),
            calc_object_model::ObjectKey::Range(calc_func::StoredReference::Inline(
                calc_value::ReferenceLike::cell(cell),
            )),
        );
        let sheet = self.member(&range, "Worksheet", vec![]).map_err(message)?;
        self.raise(
            &self.state.application(),
            "AppEvents",
            "SheetChange",
            &[sheet, AutoValue::Object(range)],
        )
        .map_err(message)
    }
    fn member(
        &self,
        target: &ObjectRef,
        name: &str,
        args: Vec<AutoValue>,
    ) -> Result<AutoValue, AutoError> {
        let interface = target.interface;
        let member = Descriptor::get()
            .member(interface, name, InvokeKind::Get)
            .ok_or(AutoError::Status(DISP_E_MEMBERNOTFOUND.0))?;
        self.state.call(
            target,
            &Call {
                interface,
                member,
                args,
                lcid: 0,
            },
        )
    }
    pub fn run_early(&self, name: &str) -> Result<CalcValue, String> {
        let _arguments = self.state.argument_scope();
        let application = self.application()?;
        let name = VARIANT::from(name);
        let missing = values::to_variant(&self.state, &AutoValue::Missing).map_err(message)?;
        let mut output = VARIANT::default();
        unsafe { runtime::call_run(application.as_raw(), &name, &missing, &mut output) }
            .ok()
            .map_err(|e| e.to_string())?;
        values::to_calc(&values::from_variant(&self.state, &output).map_err(message)?)
            .map_err(message)
    }
    pub fn close_addins(&self) -> Result<(), String> {
        self.state.apartment.check().map_err(message)?;
        self.state.host.close()?;
        self.state.office.close().map_err(message)
    }
    pub fn ribbons(&self) -> Result<Vec<(String, String)>, String> {
        self.state.apartment.check().map_err(message)?;
        self.state.office.ribbons().map_err(message)
    }
}
impl EventSource for Projection {
    fn raise(
        &self,
        source: &ObjectRef,
        interface: &str,
        event: &str,
        args: &[AutoValue],
    ) -> Result<(), AutoError> {
        self.state.raise(source, interface, event, args)
    }
}
impl Drop for Projection {
    fn drop(&mut self) {
        if let Err(why) = self.close_addins() {
            eprintln!("COM projection shutdown: {why}");
        }
        self.state.apartment.drain();
        self.state
            .apartment
            .connected
            .store(false, Ordering::Release);
        let connections = std::mem::take(&mut *self.state.connections.borrow_mut());
        drop(connections);
        let foreign = std::mem::take(&mut *self.state.foreign.borrow_mut());
        drop(foreign);
    }
}
fn message(error: AutoError) -> String {
    error.description()
}
use windows::Win32::System::Variant::VARIANT;
