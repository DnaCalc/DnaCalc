//! Raw COM identity and generated x64 ABI entry points. No member semantics live here.
use crate::{dispatch::decode_slot, projection::State, typeinfo};
use calc_object_model::{AutoError, Descriptor, InterfaceId, ObjectRef};
use std::{
    ffi::c_void,
    panic::{AssertUnwindSafe, catch_unwind},
    ptr,
    rc::{Rc, Weak},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
};
use windows::{
    Win32::{
        Foundation::*,
        System::Threading::GetCurrentThreadId,
        System::{Com::*, Ole::*},
    },
    core::{GUID, HRESULT, IUnknown, Interface},
};

pub(crate) fn guid(id: InterfaceId) -> GUID {
    let g =
        calc_object_model::Guid::parse(&Descriptor::get().ty(id).guid).expect("descriptor GUID");
    GUID {
        data1: g.data1,
        data2: g.data2,
        data3: g.data3,
        data4: g.data4,
    }
}
pub(crate) fn id(name: &str) -> InterfaceId {
    let descriptor = Descriptor::get();
    descriptor.default_interface(descriptor.id(name).expect("descriptor interface"))
}
pub(crate) fn abi(f: impl FnOnce() -> HRESULT) -> HRESULT {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(E_UNEXPECTED)
}

pub(crate) struct Apartment {
    pub thread: u32,
    pub connected: AtomicBool,
    // Raw clients that incorrectly release on another thread cannot drop an Rc or a provider
    // there. Reclaim them on the next owning-STA entry or projection shutdown.
    retired: Mutex<Vec<usize>>,
}
impl Apartment {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            thread: unsafe { GetCurrentThreadId() },
            connected: AtomicBool::new(true),
            retired: Mutex::new(Vec::new()),
        })
    }
    pub fn check(&self) -> Result<(), AutoError> {
        if !self.connected.load(Ordering::Acquire) {
            return Err(AutoError::Disconnected);
        }
        if unsafe { GetCurrentThreadId() } != self.thread {
            return Err(AutoError::Status(RPC_E_WRONG_THREAD.0));
        }
        self.drain();
        Ok(())
    }
    pub fn drain(&self) {
        if unsafe { GetCurrentThreadId() } != self.thread {
            return;
        }
        let retired = std::mem::take(&mut *self.retired.lock().unwrap_or_else(|p| p.into_inner()));
        for pointer in retired {
            unsafe { destroy(pointer as *mut Wrapper) };
        }
    }
}

#[repr(C)]
pub(crate) struct Wrapper {
    pub vtable: *const *const c_void,
    references: AtomicU32,
    pub target: ObjectRef,
    state: Weak<State>,
    model: Weak<dyn calc_object_model::ObjectModel>,
    apartment: Arc<Apartment>,
    support: TearOff,
    container: TearOff,
}
#[repr(C)]
struct TearOff {
    vtable: *const *const c_void,
    owner: *mut Wrapper,
}

pub(crate) struct RawVtable<const N: usize>(pub [*const c_void; N]);
// Every entry is an immutable function address, used only through its declared ABI.
unsafe impl<const N: usize> Sync for RawVtable<N> {}
include!(concat!(env!("OUT_DIR"), "/vtables.rs"));

impl Wrapper {
    pub fn state(&self) -> Result<Rc<State>, AutoError> {
        self.apartment.check()?;
        self.state.upgrade().ok_or(AutoError::Disconnected)
    }
}
pub(crate) fn wrap(state: &Rc<State>, target: &ObjectRef) -> Result<IDispatch, AutoError> {
    state.apartment.check()?;
    let interface = Descriptor::get().default_interface(target.interface);
    // Incomplete inherited layouts still work through IDispatch. Their IID is never advertised.
    let table = generated_vtable(interface)
        .unwrap_or_else(|| generated_vtable(id("Window")).expect("complete IDispatch layout"));
    let mut wrapper = Box::new(Wrapper {
        vtable: table.as_ptr(),
        references: AtomicU32::new(1),
        target: ObjectRef {
            interface,
            key: target.key.clone(),
        },
        state: Rc::downgrade(state),
        model: Rc::downgrade(&state.model.borrow()),
        apartment: state.apartment.clone(),
        support: TearOff {
            vtable: SUPPORT.0.as_ptr(),
            owner: ptr::null_mut(),
        },
        container: TearOff {
            vtable: CONTAINER.0.as_ptr(),
            owner: ptr::null_mut(),
        },
    });
    let pointer = &mut *wrapper as *mut Wrapper;
    wrapper.support.owner = pointer;
    wrapper.container.owner = pointer;
    state
        .wrappers
        .borrow_mut()
        .insert(pointer as usize, wrapper.target.clone());
    Ok(unsafe { IDispatch::from_raw(Box::into_raw(wrapper).cast()) })
}
pub(crate) unsafe fn query_raw(this: *mut Wrapper, iid: &GUID, out: *mut *mut c_void) -> HRESULT {
    if out.is_null() {
        return E_POINTER;
    }
    unsafe {
        *out = ptr::null_mut();
    }
    let wrapper = unsafe { &*this };
    let state = match wrapper.state() {
        Ok(s) => s,
        Err(e) => return error(&e, ptr::null_mut()),
    };
    let descriptor = Descriptor::get();
    let mut interface = Some(wrapper.target.interface);
    let mut matches = *iid == IUnknown::IID || *iid == IDispatch::IID;
    while let Some(at) = interface {
        matches |= generated_vtable(at).is_some() && *iid == guid(at);
        interface = descriptor
            .ty(at)
            .base
            .as_deref()
            .and_then(|base| descriptor.id(base));
    }
    let pointer = if matches {
        this.cast()
    } else if *iid == ISupportErrorInfo::IID {
        (&wrapper.support as *const TearOff).cast_mut().cast()
    } else if *iid == IConnectionPointContainer::IID
        && !state.source_interfaces(&wrapper.target).is_empty()
    {
        (&wrapper.container as *const TearOff).cast_mut().cast()
    } else {
        return E_NOINTERFACE;
    };
    unsafe {
        *out = pointer;
        add_ref(this);
    }
    S_OK
}
unsafe extern "system" fn query(
    this: *mut Wrapper,
    iid: *const GUID,
    out: *mut *mut c_void,
) -> HRESULT {
    abi(|| {
        if iid.is_null() {
            E_POINTER
        } else {
            unsafe { query_raw(this, &*iid, out) }
        }
    })
}
unsafe extern "system" fn add_ref(this: *mut Wrapper) -> u32 {
    unsafe { &*this }.references.fetch_add(1, Ordering::Relaxed) + 1
}
unsafe extern "system" fn release(this: *mut Wrapper) -> u32 {
    let wrapper = unsafe { &*this };
    let count = wrapper.references.fetch_sub(1, Ordering::Release) - 1;
    if count == 0 {
        std::sync::atomic::fence(Ordering::Acquire);
        if unsafe { GetCurrentThreadId() } == wrapper.apartment.thread {
            let _ = catch_unwind(AssertUnwindSafe(|| unsafe { destroy(this) }));
        } else {
            wrapper
                .apartment
                .retired
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .push(this as usize);
        }
    }
    count
}
unsafe fn destroy(this: *mut Wrapper) {
    let wrapper = unsafe { Box::from_raw(this) };
    if let Some(state) = wrapper.state.upgrade() {
        state.wrappers.borrow_mut().remove(&(this as usize));
    }
    if let Some(model) = wrapper.model.upgrade() {
        model.released(&wrapper.target);
    }
}
unsafe extern "system" fn type_count(this: *mut Wrapper, count: *mut u32) -> HRESULT {
    abi(|| {
        if count.is_null() {
            return E_POINTER;
        }
        unsafe {
            *count = 0;
        }
        let wrapper = unsafe { &*this };
        if let Err(e) = wrapper.state() {
            return error(&e, ptr::null_mut());
        }
        match typeinfo::get(wrapper.target.interface) {
            Ok(_) => {
                unsafe { *count = 1 };
                S_OK
            }
            Err(e) => error(&e, ptr::null_mut()),
        }
    })
}
unsafe extern "system" fn type_info(
    this: *mut Wrapper,
    index: u32,
    _: u32,
    out: *mut *mut c_void,
) -> HRESULT {
    abi(|| {
        if out.is_null() {
            return E_POINTER;
        }
        unsafe {
            *out = ptr::null_mut();
        }
        let wrapper = unsafe { &*this };
        if let Err(e) = wrapper.state() {
            return error(&e, ptr::null_mut());
        }
        if index != 0 {
            return DISP_E_BADINDEX;
        }
        match typeinfo::get(wrapper.target.interface) {
            Ok(info) => {
                unsafe { *out = info.into_raw() };
                S_OK
            }
            Err(e) => error(&e, ptr::null_mut()),
        }
    })
}
use crate::dispatch::{invoke, names};

unsafe extern "system" fn tear_query(
    this: *mut TearOff,
    iid: *const GUID,
    out: *mut *mut c_void,
) -> HRESULT {
    unsafe { query((*this).owner, iid, out) }
}
unsafe extern "system" fn tear_add(this: *mut TearOff) -> u32 {
    unsafe { add_ref((*this).owner) }
}
unsafe extern "system" fn tear_release(this: *mut TearOff) -> u32 {
    unsafe { release((*this).owner) }
}
unsafe extern "system" fn supports(this: *mut TearOff, iid: *const GUID) -> HRESULT {
    abi(|| {
        if iid.is_null() {
            return E_POINTER;
        }
        let owner = unsafe { &*(*this).owner };
        if let Err(e) = owner.state() {
            return error(&e, ptr::null_mut());
        }
        let descriptor = Descriptor::get();
        let mut interface = Some(owner.target.interface);
        while let Some(at) = interface {
            if generated_vtable(at).is_some() && unsafe { *iid } == guid(at) {
                return S_OK;
            }
            interface = descriptor
                .ty(at)
                .base
                .as_deref()
                .and_then(|base| descriptor.id(base));
        }
        S_FALSE
    })
}
unsafe extern "system" fn enum_points(_: *mut TearOff, out: *mut *mut c_void) -> HRESULT {
    if out.is_null() {
        return E_POINTER;
    }
    unsafe { *out = ptr::null_mut() };
    E_NOTIMPL
}
unsafe extern "system" fn find_point(
    this: *mut TearOff,
    iid: *const GUID,
    out: *mut *mut c_void,
) -> HRESULT {
    abi(|| {
        if iid.is_null() || out.is_null() {
            return E_POINTER;
        }
        unsafe { *out = ptr::null_mut() };
        let wrapper = unsafe { &*(*this).owner };
        let state = match wrapper.state() {
            Ok(s) => s,
            Err(e) => return error(&e, ptr::null_mut()),
        };
        match state.connection_point(&wrapper.target, unsafe { &*iid }, unsafe { (*this).owner }) {
            Ok(point) => {
                unsafe { *out = point.into_raw() };
                S_OK
            }
            Err(e) => error(&e, ptr::null_mut()),
        }
    })
}
static SUPPORT: RawVtable<4> = RawVtable([
    tear_query as *const () as _,
    tear_add as *const () as _,
    tear_release as *const () as _,
    supports as *const () as _,
]);
static CONTAINER: RawVtable<5> = RawVtable([
    tear_query as *const () as _,
    tear_add as *const () as _,
    tear_release as *const () as _,
    enum_points as *const () as _,
    find_point as *const () as _,
]);

/// Rule 5: vtable errors carry IErrorInfo; Invoke additionally carries EXCEPINFO when supplied.
pub(crate) fn error(error: &AutoError, exception: *mut EXCEPINFO) -> HRESULT {
    let number = error.number();
    let code = match error {
        AutoError::Status(code) => return HRESULT(*code),
        AutoError::Disconnected => return CO_E_OBJNOTCONNECTED,
        _ => HRESULT((0x800A0000u32 | (number as u32 & 0xffff)) as i32),
    };
    let description = error.description();
    let description_wide: Vec<u16> = description.encode_utf16().chain(Some(0)).collect();
    unsafe {
        if let Ok(create) = CreateErrorInfo() {
            let _ = create.SetSource(windows::core::w!("DNA Calc"));
            let _ = create.SetDescription(windows::core::PCWSTR(description_wide.as_ptr()));
            if let Ok(info) = create.cast::<IErrorInfo>() {
                let _ = SetErrorInfo(0, &info);
            }
        }
        if !exception.is_null() {
            exception.write(EXCEPINFO {
                wCode: number as u16,
                scode: code.0,
                bstrSource: ManuallyDrop::new(windows::core::BSTR::from("DNA Calc")),
                bstrDescription: ManuallyDrop::new(windows::core::BSTR::from(description)),
                ..Default::default()
            });
            return DISP_E_EXCEPTION;
        }
    }
    code
}
use std::mem::ManuallyDrop;
use windows::Win32::System::Variant::VARIANT;

#[cfg(test)]
pub(crate) fn advertised(id: InterfaceId) -> bool {
    generated_vtable(id).is_some()
}
#[cfg(test)]
pub(crate) fn layout(id: InterfaceId) -> Option<&'static [*const c_void]> {
    generated_vtable(id)
}
