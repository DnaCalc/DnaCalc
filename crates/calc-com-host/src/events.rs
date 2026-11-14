//! Connection points share cookies per source/interface and snapshot subscribers before delivery.
use crate::{
    projection::State,
    runtime::{Wrapper, guid},
    values::{self, status},
};
use calc_object_model::{AutoError, AutoValue, Descriptor, InterfaceId, InvokeKind, ObjectRef};
use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap},
    mem::ManuallyDrop,
    ptr,
    rc::{Rc, Weak},
};
use windows::{
    Win32::{
        Foundation::*,
        System::{Com::*, Ole::*},
    },
    core::{GUID, IUnknown, Interface, Ref, implement},
};

#[derive(Default)]
pub(crate) struct Subscriptions {
    next: u32,
    // Snapshot clones must not call a foreign AddRef while `connections` is borrowed.
    sinks: BTreeMap<u32, Rc<IDispatch>>,
}
pub(crate) type Connections = RefCell<HashMap<(ObjectRef, InterfaceId), Subscriptions>>;

#[implement(IConnectionPoint, Agile = false)]
struct Point {
    state: Weak<State>,
    target: ObjectRef,
    interface: InterfaceId,
    // Keeps COM identity stable for GetConnectionPointContainer; no strong Rust state link.
    owner: IUnknown,
    apartment: std::sync::Arc<crate::runtime::Apartment>,
}
impl Point {
    fn state(&self) -> windows::core::Result<Rc<State>> {
        self.apartment.check().map_err(|e| {
            windows::core::Error::from_hresult(crate::runtime::error(&e, ptr::null_mut()))
        })?;
        let state = self
            .state
            .upgrade()
            .ok_or_else(|| windows::core::Error::from_hresult(CO_E_OBJNOTCONNECTED))?;
        state.apartment.check().map_err(|e| {
            windows::core::Error::from_hresult(crate::runtime::error(&e, ptr::null_mut()))
        })?;
        Ok(state)
    }
}
impl IConnectionPoint_Impl for Point_Impl {
    fn GetConnectionInterface(&self) -> windows::core::Result<GUID> {
        self.state()?;
        Ok(guid(self.interface))
    }
    fn GetConnectionPointContainer(&self) -> windows::core::Result<IConnectionPointContainer> {
        self.state()?;
        self.owner.cast()
    }
    fn Advise(&self, sink: Ref<'_, IUnknown>) -> windows::core::Result<u32> {
        let state = self.state()?;
        let sink = sink
            .as_ref()
            .ok_or_else(|| windows::core::Error::from_hresult(E_POINTER))?;
        let mut raw = ptr::null_mut();
        unsafe { sink.query(&guid(self.interface), &mut raw) }
            .ok()
            .map_err(|_| windows::core::Error::from_hresult(CONNECT_E_CANNOTCONNECT))?;
        let sink = unsafe { IDispatch::from_raw(raw) };
        let mut all = state.connections.borrow_mut();
        let connections = all
            .entry((self.target.clone(), self.interface))
            .or_default();
        let cookie = connections
            .next
            .checked_add(1)
            .ok_or_else(|| windows::core::Error::from_hresult(CONNECT_E_ADVISELIMIT))?;
        connections.next = cookie;
        connections.sinks.insert(cookie, Rc::new(sink));
        Ok(cookie)
    }
    fn Unadvise(&self, cookie: u32) -> windows::core::Result<()> {
        let state = self.state()?;
        // A provider's Release may reenter. Remove it before dropping the reference.
        let removed = state
            .connections
            .borrow_mut()
            .get_mut(&(self.target.clone(), self.interface))
            .and_then(|connections| connections.sinks.remove(&cookie));
        removed
            .map(drop)
            .ok_or_else(|| windows::core::Error::from_hresult(CONNECT_E_NOCONNECTION))
    }
    fn EnumConnections(&self) -> windows::core::Result<IEnumConnections> {
        self.state()?;
        Err(windows::core::Error::from_hresult(E_NOTIMPL))
    }
}

impl State {
    pub(crate) fn source_interfaces(&self, target: &ObjectRef) -> Vec<InterfaceId> {
        let descriptor = Descriptor::get();
        descriptor
            .types()
            .filter(|(_, ty)| {
                ty.coclass_interfaces.iter().any(|i| {
                    i.default && !i.source && descriptor.id(&i.name) == Some(target.interface)
                })
            })
            .flat_map(|(_, ty)| {
                ty.coclass_interfaces
                    .iter()
                    .filter(|i| i.source)
                    .filter_map(|i| descriptor.id(&i.name))
            })
            .collect()
    }
    pub(crate) fn connection_point(
        self: &Rc<Self>,
        target: &ObjectRef,
        iid: &GUID,
        owner: *mut Wrapper,
    ) -> Result<IConnectionPoint, AutoError> {
        self.apartment.check()?;
        let interface = self
            .source_interfaces(target)
            .into_iter()
            .find(|id| guid(*id) == *iid)
            .ok_or(AutoError::Status(CONNECT_E_NOCONNECTION.0))?;
        let raw = owner.cast();
        let owner = unsafe { IUnknown::from_raw_borrowed(&raw) }
            .ok_or(AutoError::Status(E_POINTER.0))?
            .clone();
        Ok(Point {
            state: Rc::downgrade(self),
            target: target.clone(),
            interface,
            owner,
            apartment: self.apartment.clone(),
        }
        .into())
    }
    pub(crate) fn raise(
        self: &Rc<Self>,
        source: &ObjectRef,
        interface: &str,
        event: &str,
        args: &[AutoValue],
    ) -> Result<(), AutoError> {
        self.apartment.check()?;
        let objects = self.result_values(args);
        let mut arguments = args
            .iter()
            .map(|arg| values::to_variant(&objects, arg))
            .collect::<Result<Vec<_>, _>>()?;
        if !self.events_enabled()? {
            return Ok(());
        }
        let descriptor = Descriptor::get();
        let interface = descriptor
            .id(interface)
            .ok_or(AutoError::Status(CONNECT_E_NOCONNECTION.0))?;
        if !self.source_interfaces(source).contains(&interface) {
            return Err(AutoError::Status(CONNECT_E_NOCONNECTION.0));
        }
        let member = descriptor
            .member(interface, event, InvokeKind::Method)
            .ok_or(AutoError::Status(DISP_E_MEMBERNOTFOUND.0))?;
        if args.len() != member.arguments().count() {
            return Err(AutoError::Status(DISP_E_BADPARAMCOUNT.0));
        }
        let snapshot: Vec<Rc<IDispatch>> = self
            .connections
            .borrow()
            .get(&(source.clone(), interface))
            .map(|connections| connections.sinks.values().cloned().collect())
            .unwrap_or_default();
        // X0 measured Excel AppEvents: declaration order with named argument DISPIDs 0,1,...
        let mut named: Vec<i32> = (0..args.len() as i32).collect();
        let params = DISPPARAMS {
            rgvarg: arguments.as_mut_ptr(),
            rgdispidNamedArgs: named.as_mut_ptr(),
            cArgs: args.len() as u32,
            cNamedArgs: named.len() as u32,
        };
        let mut first = None;
        for sink in snapshot {
            let mut exception = EXCEPINFO::default();
            let result = unsafe {
                sink.Invoke(
                    member.dispid,
                    &GUID::zeroed(),
                    0,
                    DISPATCH_METHOD | DISPATCH_PROPERTYGET,
                    &params,
                    None,
                    Some(&mut exception),
                    None,
                )
            };
            unsafe { clear_exception(&mut exception) };
            if let Err(e) = result {
                first.get_or_insert_with(|| status(e));
            }
        }
        first.map_or(Ok(()), Err)
    }
}
pub(crate) unsafe fn clear_exception(exception: &mut EXCEPINFO) {
    unsafe {
        ManuallyDrop::drop(&mut exception.bstrSource);
        ManuallyDrop::drop(&mut exception.bstrDescription);
        ManuallyDrop::drop(&mut exception.bstrHelpFile);
    }
    *exception = EXCEPINFO::default();
}
