//! Native RTD transport, ported from rtd.cpp at ef34ca50. It owns no evaluator state.
use crate::{
    interfaces::{IRTDUpdateEvent, IRTDUpdateEvent_Impl, IRtdServer},
    office::registry_string,
    values::{self, status},
};
use calc_object_model::{AutoError, AutoValue};
use calc_value::CalcValue;
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::c_void,
    ptr,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering},
    },
};
use windows::{
    Win32::{
        Foundation::*,
        System::{
            Com::*, Ole::*, Registry::HKEY_CLASSES_ROOT, SystemInformation::GetTickCount64,
            Threading::GetCurrentThreadId, Variant::*,
        },
    },
    core::{GUID, HRESULT, Interface, PCWSTR, implement},
};

type Result<T> = std::result::Result<T, AutoError>;
fn hr(result: Result<()>) -> i32 {
    result
        .err()
        .map_or(0, |e| crate::runtime::error(&e, ptr::null_mut()).0)
}
fn now() -> u64 {
    unsafe { GetTickCount64() }
}
pub(crate) type Value = CalcValue;
struct Notices {
    alive: AtomicBool,
    dirty: AtomicBool,
    heartbeat: AtomicI32,
    notified: AtomicU64,
}
impl Notices {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            alive: AtomicBool::new(true),
            dirty: AtomicBool::new(false),
            heartbeat: AtomicI32::new(15000),
            notified: AtomicU64::new(now()),
        })
    }
}
// windows::implement's default agile implementation supplies IMarshal through the system
// free-threaded marshaler. This sink contains only atomics and an Arc, never an STA pointer.
#[implement(IRTDUpdateEvent)]
struct UpdateSink {
    notices: Arc<Notices>,
}
impl IRTDUpdateEvent_Impl for UpdateSink_Impl {
    unsafe fn UpdateNotify(&self) -> HRESULT {
        if self.notices.alive.load(Ordering::Acquire) {
            self.notices.notified.fetch_max(now(), Ordering::AcqRel);
            self.notices.dirty.store(true, Ordering::Release);
        }
        S_OK
    }
    unsafe fn HeartbeatInterval(&self, result: *mut i32) -> HRESULT {
        if result.is_null() {
            return E_POINTER;
        }
        unsafe {
            *result = self.notices.heartbeat.load(Ordering::Acquire);
        }
        S_OK
    }
    unsafe fn SetHeartbeatInterval(&self, value: i32) -> HRESULT {
        if value < 0 {
            return E_INVALIDARG;
        }
        self.notices.heartbeat.store(value, Ordering::Release);
        S_OK
    }
    unsafe fn Disconnect(&self) -> HRESULT {
        self.notices.alive.store(false, Ordering::Release);
        S_OK
    }
}
impl IDispatch_Impl for UpdateSink_Impl {
    fn GetTypeInfoCount(&self) -> windows::core::Result<u32> {
        Ok(0)
    }
    fn GetTypeInfo(&self, _: u32, _: u32) -> windows::core::Result<ITypeInfo> {
        Err(windows::core::Error::from_hresult(E_NOTIMPL))
    }
    fn GetIDsOfNames(
        &self,
        _: *const GUID,
        names: *const PCWSTR,
        count: u32,
        _: u32,
        ids: *mut i32,
    ) -> windows::core::Result<()> {
        if names.is_null() || ids.is_null() {
            return Err(windows::core::Error::from_hresult(E_POINTER));
        }
        let descriptor = calc_object_model::Descriptor::get();
        let id = crate::runtime::id("IRTDUpdateEvent");
        for i in 0..count as usize {
            let name = unsafe { (*names.add(i)).to_string() }?;
            let dispid = descriptor
                .dispid_of(id, &name)
                .ok_or_else(|| windows::core::Error::from_hresult(DISP_E_UNKNOWNNAME))?;
            unsafe {
                *ids.add(i) = dispid;
            }
        }
        Ok(())
    }
    fn Invoke(
        &self,
        dispid: i32,
        _: *const GUID,
        _: u32,
        flags: DISPATCH_FLAGS,
        params: *const DISPPARAMS,
        result: *mut VARIANT,
        _: *mut EXCEPINFO,
        _: *mut u32,
    ) -> windows::core::Result<()> {
        let descriptor = calc_object_model::Descriptor::get();
        let interface = crate::runtime::id("IRTDUpdateEvent");
        let member = descriptor
            .members(interface)
            .into_iter()
            .find(|m| {
                m.dispid == dispid
                    && match m.invoke {
                        calc_object_model::InvokeKind::Get => flags.0 & DISPATCH_PROPERTYGET.0 != 0,
                        calc_object_model::InvokeKind::Put => flags.0 & DISPATCH_PROPERTYPUT.0 != 0,
                        _ => flags.0 & DISPATCH_METHOD.0 != 0,
                    }
            })
            .ok_or_else(|| windows::core::Error::from_hresult(DISP_E_MEMBERNOTFOUND))?;
        match member.name.as_str() {
            "UpdateNotify" => unsafe { self.UpdateNotify() }.ok(),
            "Disconnect" => unsafe { self.Disconnect() }.ok(),
            "HeartbeatInterval" if member.invoke == calc_object_model::InvokeKind::Get => {
                if result.is_null() {
                    return Err(windows::core::Error::from_hresult(E_POINTER));
                }
                unsafe {
                    result.write(VARIANT::from(
                        self.notices.heartbeat.load(Ordering::Acquire),
                    ));
                }
                Ok(())
            }
            "HeartbeatInterval" => {
                if params.is_null()
                    || unsafe { (*params).cArgs } != 1
                    || unsafe { (*params).rgvarg }.is_null()
                {
                    return Err(windows::core::Error::from_hresult(DISP_E_BADPARAMCOUNT));
                }
                let value = values::from_variant(&values::NoObjects, unsafe { &*(*params).rgvarg })
                    .map_err(com_error)?;
                unsafe {
                    self.SetHeartbeatInterval(values::number(&value).map_err(com_error)? as i32)
                }
                .ok()
            }
            _ => Err(windows::core::Error::from_hresult(DISP_E_MEMBERNOTFOUND)),
        }
    }
}
fn com_error(error: AutoError) -> windows::core::Error {
    windows::core::Error::from_hresult(crate::runtime::error(&error, ptr::null_mut()))
}

struct Server {
    object: IRtdServer,
    _sink: IRTDUpdateEvent,
    notices: Arc<Notices>,
    topics: BTreeSet<i32>,
    retired: BTreeSet<i32>,
    refresh: u64,
    heartbeat: u64,
    stop_attempted: bool,
    stop_error: Option<AutoError>,
}
impl Server {
    fn new(object: IRtdServer) -> Self {
        let notices = Notices::new();
        let sink: IRTDUpdateEvent = UpdateSink {
            notices: notices.clone(),
        }
        .into();
        Self {
            object,
            _sink: sink,
            notices,
            topics: BTreeSet::new(),
            retired: BTreeSet::new(),
            refresh: 0,
            heartbeat: now(),
            stop_attempted: false,
            stop_error: None,
        }
    }
    fn stop(&mut self) -> Result<()> {
        if self.stop_attempted {
            return self.stop_error.clone().map_or(Ok(()), Err);
        }
        self.stop_attempted = true;
        self.notices.alive.store(false, Ordering::Release);
        let mut first = None;
        for topic in &self.topics {
            if let Err(e) = unsafe { self.object.DisconnectData(*topic) }.ok() {
                first.get_or_insert_with(|| status(e));
            }
        }
        if let Err(e) = unsafe { self.object.ServerTerminate() }.ok() {
            first.get_or_insert_with(|| status(e));
        }
        if first.is_none() {
            self.topics.clear();
        }
        self.stop_error = first.clone();
        first.map_or(Ok(()), Err)
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
pub(crate) struct NativeClient {
    owner: u32,
    closed: bool,
    close_error: Option<AutoError>,
    throttle: u64,
    servers: BTreeMap<String, Server>,
    topics: BTreeMap<i32, String>,
    #[cfg(test)]
    fixture: Option<std::rc::Rc<FixtureState>>,
}
impl NativeClient {
    fn new() -> Self {
        Self {
            owner: unsafe { GetCurrentThreadId() },
            closed: false,
            close_error: None,
            throttle: 2000,
            servers: BTreeMap::new(),
            topics: BTreeMap::new(),
            #[cfg(test)]
            fixture: None,
        }
    }
    fn check(&self) -> Result<()> {
        if unsafe { GetCurrentThreadId() } != self.owner {
            return Err(AutoError::Status(RPC_E_WRONG_THREAD.0));
        }
        if self.closed {
            return Err(AutoError::Disconnected);
        }
        Ok(())
    }
    fn server(&mut self, progid: &str) -> Result<&mut Server> {
        if !self.servers.contains_key(progid) {
            if progid.is_empty() || progid.contains(['\\', '/']) {
                return Err(AutoError::Status(E_INVALIDARG.0));
            }
            // Read exact mappings. CLSIDFromProgID may create OLE1 mappings for unknown names.
            let clsid = registry_string(HKEY_CLASSES_ROOT, &format!("{progid}\\CLSID"), None)
                .ok_or(AutoError::Status(REGDB_E_CLASSNOTREG.0))?;
            let wide: Vec<_> = clsid.encode_utf16().chain(Some(0)).collect();
            let clsid = unsafe { CLSIDFromString(PCWSTR(wide.as_ptr())) }.map_err(status)?;
            let object = unsafe {
                CoCreateInstance::<_, IRtdServer>(
                    &clsid,
                    None,
                    CLSCTX_INPROC_SERVER | CLSCTX_LOCAL_SERVER,
                )
            }
            .map_err(status)?;
            let mut server = Server::new(object);
            let mut started = 0;
            let result = unsafe {
                server
                    .object
                    .ServerStart(server._sink.as_raw(), &mut started)
            }
            .ok()
            .map_err(status);
            if result.is_err() || started <= 0 {
                let failure = result.err().unwrap_or(AutoError::Status(E_FAIL.0));
                if let Err(error) = server.stop() {
                    self.close_error = Some(error);
                    self.closed = true;
                    self.servers.insert(progid.into(), server);
                }
                return Err(failure);
            }
            self.servers.insert(progid.into(), server);
        }
        Ok(self.servers.get_mut(progid).unwrap())
    }
    fn connect(&mut self, progid: &str, topic: i32, strings: &[String]) -> Result<CalcValue> {
        self.check()?;
        if topic <= 0
            || self.topics.contains_key(&topic)
            || strings.is_empty()
            || strings.len() > 253
        {
            return Err(AutoError::Status(E_INVALIDARG.0));
        }
        let server = self.server(progid)?;
        let mut array = unsafe { SafeArrayCreateVector(VT_VARIANT, 0, strings.len() as u32) };
        let owned = values::ArrayOwner::new(array)?;
        for (i, text) in strings.iter().enumerate() {
            let value = VARIANT::from(text.as_str());
            unsafe { SafeArrayPutElement(array, &(i as i32), (&value as *const VARIANT).cast()) }
                .map_err(status)?;
        }
        let _ = owned.take();
        let mut initial = VARIANT::default();
        let mut fresh = -1i16;
        let result = unsafe {
            server
                .object
                .ConnectData(topic, &mut array, &mut fresh, &mut initial)
        }
        .ok()
        .map_err(status);
        drop(values::ArrayOwner(array));
        result?;
        server.topics.insert(topic);
        match flatten(&initial) {
            Ok(value) => {
                self.topics.insert(topic, progid.into());
                Ok(value)
            }
            Err(error) => {
                let _ = unsafe { server.object.DisconnectData(topic) };
                server.topics.remove(&topic);
                server.retired.insert(topic);
                Err(error)
            }
        }
    }
    fn poll(&mut self, mut publish: impl FnMut(i32, &CalcValue) -> Result<()>) -> Result<()> {
        self.check()?;
        let now = now();
        for server in self.servers.values_mut() {
            if !server.notices.alive.load(Ordering::Acquire) {
                return Err(AutoError::Disconnected);
            }
            let heartbeat = server.notices.heartbeat.load(Ordering::Acquire);
            let last = server
                .notices
                .notified
                .load(Ordering::Acquire)
                .max(server.heartbeat);
            if heartbeat > 0 && now.saturating_sub(last) >= heartbeat as u64 {
                let mut alive = 0;
                unsafe { server.object.Heartbeat(&mut alive) }
                    .ok()
                    .map_err(status)?;
                server.heartbeat = now;
                if alive <= 0 {
                    return Err(AutoError::Disconnected);
                }
            }
            if now.saturating_sub(server.refresh) < self.throttle
                || !server.notices.dirty.swap(false, Ordering::AcqRel)
            {
                continue;
            }
            let mut count = 0;
            let mut array = ptr::null_mut();
            let result = unsafe { server.object.RefreshData(&mut count, &mut array) }
                .ok()
                .map_err(status);
            server.refresh = now;
            let owned = values::ArrayOwner(array);
            result?;
            if array.is_null() {
                if count == 0 {
                    continue;
                }
                return Err(AutoError::Status(E_UNEXPECTED.0));
            }
            let (first, low, last, high) = unsafe { values::array_bounds(array) }?;
            if i64::from(last) - i64::from(first) + 1 != 2
                || count < 0
                || i64::from(high) - i64::from(low) + 1 != i64::from(count)
            {
                return values::mismatch();
            }
            for column in low..=high {
                let topic_value = unsafe { values::array_element(array, first, column)? };
                // An RTD topic ID must be exactly VT_I4, as the old transport requires.
                if topic_value.vt() != VT_I4 {
                    return values::mismatch();
                }
                let AutoValue::I4(topic) = values::from_variant(&values::NoObjects, &topic_value)?
                else {
                    return values::mismatch();
                };
                if !server.topics.contains(&topic) {
                    if server.retired.contains(&topic) {
                        continue;
                    }
                    return Err(AutoError::Status(E_INVALIDARG.0));
                }
                let value = flatten(&unsafe { values::array_element(array, first + 1, column)? })?;
                publish(topic, &value)?;
            }
            drop(owned);
        }
        Ok(())
    }
    fn disconnect(&mut self, topic: i32) -> Result<()> {
        self.check()?;
        let key = self
            .topics
            .get(&topic)
            .cloned()
            .ok_or(AutoError::Status(E_INVALIDARG.0))?;
        let server = self.servers.get_mut(&key).unwrap();
        unsafe { server.object.DisconnectData(topic) }
            .ok()
            .map_err(status)?;
        server.topics.remove(&topic);
        server.retired.insert(topic);
        self.topics.remove(&topic);
        if server.topics.is_empty() {
            if let Err(error) = server.stop() {
                self.closed = true;
                self.close_error = Some(error.clone());
                return Err(error);
            }
            self.servers.remove(&key);
        }
        Ok(())
    }
    fn close(&mut self) -> Result<()> {
        if unsafe { GetCurrentThreadId() } != self.owner {
            return Err(AutoError::Status(RPC_E_WRONG_THREAD.0));
        }
        if self.closed {
            return self.close_error.clone().map_or(Ok(()), Err);
        }
        self.closed = true;
        let mut first = None;
        for server in self.servers.values_mut() {
            if let Err(error) = server.stop() {
                first.get_or_insert(error);
            }
        }
        if first.is_none() {
            self.topics.clear();
            self.servers.clear();
        }
        self.close_error = first.clone();
        first.map_or(Ok(()), Err)
    }
}
impl Drop for NativeClient {
    fn drop(&mut self) {
        if self.close().is_err() {
            let servers = std::mem::take(&mut self.servers);
            std::mem::forget(servers);
        }
    }
}
fn flatten(value: &VARIANT) -> Result<CalcValue> {
    if value.vt().0 & VT_BYREF.0 != 0 {
        return values::mismatch();
    }
    if value.vt() == VT_ARRAY | VT_VARIANT {
        return Ok(CalcValue::error(calc_value::WorksheetErrorCode::NA));
    }
    flatten_auto(&values::from_variant(&values::NoObjects, value)?)
}
fn flatten_auto(value: &AutoValue) -> Result<CalcValue> {
    if matches!(value, AutoValue::Array(_)) {
        return Ok(CalcValue::error(calc_value::WorksheetErrorCode::NA));
    }
    match value {
        AutoValue::Empty
        | AutoValue::Null
        | AutoValue::I2(_)
        | AutoValue::I4(_)
        | AutoValue::R4(_)
        | AutoValue::R8(_)
        | AutoValue::Text(_)
        | AutoValue::Bool(_)
        | AutoValue::Error(_) => values::to_calc(value),
        _ => values::mismatch(),
    }
}

// Internal Rust transport adapter: rtd.rs keeps the existing lease/declaration machinery.
// None of these are exported C functions or a second Automation converter.
pub(crate) unsafe fn dna_rtd_create() -> *mut c_void {
    Box::into_raw(Box::new(NativeClient::new())).cast()
}
pub(crate) unsafe fn dna_rtd_connect(
    client: *mut c_void,
    progid: *const u16,
    topic: i32,
    strings: *const *const u16,
    count: usize,
    out: *mut Value,
) -> i32 {
    hr((|| {
        if client.is_null() || progid.is_null() || out.is_null() || strings.is_null() {
            return Err(AutoError::Status(E_POINTER.0));
        }
        let progid =
            unsafe { PCWSTR(progid).to_string() }.map_err(|_| AutoError::Status(E_INVALIDARG.0))?;
        let strings = unsafe { std::slice::from_raw_parts(strings, count) }
            .iter()
            .map(|s| {
                unsafe { PCWSTR(*s).to_string() }.map_err(|_| AutoError::Status(E_INVALIDARG.0))
            })
            .collect::<Result<Vec<_>>>()?;
        let value =
            unsafe { &mut *client.cast::<NativeClient>() }.connect(&progid, topic, &strings)?;
        unsafe {
            *out = value;
        }
        Ok(())
    })())
}
pub(crate) unsafe fn dna_rtd_poll(
    client: *mut c_void,
    publish: unsafe extern "system" fn(*mut c_void, i32, *const Value) -> i32,
    context: *mut c_void,
) -> i32 {
    hr(
        unsafe { &mut *client.cast::<NativeClient>() }.poll(|topic, value| {
            HRESULT(unsafe { publish(context, topic, value) })
                .ok()
                .map_err(status)
        }),
    )
}
pub(crate) unsafe fn dna_rtd_disconnect(client: *mut c_void, topic: i32) -> i32 {
    hr(unsafe { &mut *client.cast::<NativeClient>() }.disconnect(topic))
}
pub(crate) unsafe fn dna_rtd_close(client: *mut c_void) -> i32 {
    hr(unsafe { &mut *client.cast::<NativeClient>() }.close())
}
pub(crate) unsafe fn dna_rtd_destroy(client: *mut c_void) {
    drop(unsafe { Box::from_raw(client.cast::<NativeClient>()) });
}
pub(crate) unsafe fn dna_rtd_value_clear(value: *mut Value) {
    unsafe {
        *value = CalcValue::empty();
    }
}

#[cfg(test)]
#[derive(Default)]
struct FixtureState {
    unknown: std::cell::Cell<bool>,
    calls: std::cell::RefCell<Vec<(i32, i32)>>,
}
#[cfg(test)]
#[implement(IRtdServer, Agile = false)]
struct QueuedServer {
    state: std::rc::Rc<FixtureState>,
}
#[cfg(test)]
impl crate::interfaces::IRtdServer_Impl for QueuedServer_Impl {
    unsafe fn ServerStart(&self, _: *mut c_void, result: *mut i32) -> HRESULT {
        if result.is_null() {
            return E_POINTER;
        }
        unsafe {
            *result = 1;
        }
        S_OK
    }
    unsafe fn ConnectData(
        &self,
        topic: i32,
        _: *mut *mut SAFEARRAY,
        fresh: *mut i16,
        result: *mut VARIANT,
    ) -> HRESULT {
        if fresh.is_null() || result.is_null() {
            return E_POINTER;
        }
        self.state.calls.borrow_mut().push((1, topic));
        unsafe {
            *fresh = -1;
            result.write(VARIANT::from(f64::from(topic) * 11.0));
        }
        S_OK
    }
    unsafe fn RefreshData(&self, count: *mut i32, result: *mut *mut SAFEARRAY) -> HRESULT {
        if count.is_null() || result.is_null() {
            return E_POINTER;
        }
        let bounds = [
            SAFEARRAYBOUND {
                cElements: 2,
                lLbound: 0,
            },
            SAFEARRAYBOUND {
                cElements: 3,
                lLbound: 0,
            },
        ];
        let array = unsafe { SafeArrayCreate(VT_VARIANT, 2, bounds.as_ptr()) };
        let owned = match values::ArrayOwner::new(array) {
            Ok(a) => a,
            Err(e) => return HRESULT(e.number()),
        };
        for column in 0..3i32 {
            let topic = if self.state.unknown.get() && column == 0 {
                4
            } else {
                column + 1
            };
            let topic_value = VARIANT::from(topic);
            let value = VARIANT::from(11.0 * f64::from(column + 1));
            if let Err(e) = unsafe {
                SafeArrayPutElement(
                    array,
                    [0, column].as_ptr(),
                    (&topic_value as *const VARIANT).cast(),
                )
            } {
                return e.code();
            }
            if let Err(e) = unsafe {
                SafeArrayPutElement(
                    array,
                    [1, column].as_ptr(),
                    (&value as *const VARIANT).cast(),
                )
            } {
                return e.code();
            }
        }
        unsafe {
            *count = 3;
            *result = owned.take();
        }
        S_OK
    }
    unsafe fn DisconnectData(&self, topic: i32) -> HRESULT {
        self.state.calls.borrow_mut().push((2, topic));
        S_OK
    }
    unsafe fn Heartbeat(&self, result: *mut i32) -> HRESULT {
        if result.is_null() {
            return E_POINTER;
        }
        unsafe {
            *result = 1;
        }
        S_OK
    }
    unsafe fn ServerTerminate(&self) -> HRESULT {
        self.state.calls.borrow_mut().push((3, 0));
        S_OK
    }
}
#[cfg(test)]
impl IDispatch_Impl for QueuedServer_Impl {
    fn GetTypeInfoCount(&self) -> windows::core::Result<u32> {
        Ok(0)
    }
    fn GetTypeInfo(&self, _: u32, _: u32) -> windows::core::Result<ITypeInfo> {
        Err(windows::core::Error::from_hresult(E_NOTIMPL))
    }
    fn GetIDsOfNames(
        &self,
        _: *const GUID,
        _: *const PCWSTR,
        _: u32,
        _: u32,
        _: *mut i32,
    ) -> windows::core::Result<()> {
        Err(windows::core::Error::from_hresult(E_NOTIMPL))
    }
    fn Invoke(
        &self,
        _: i32,
        _: *const GUID,
        _: u32,
        _: DISPATCH_FLAGS,
        _: *const DISPPARAMS,
        _: *mut VARIANT,
        _: *mut EXCEPINFO,
        _: *mut u32,
    ) -> windows::core::Result<()> {
        Err(windows::core::Error::from_hresult(E_NOTIMPL))
    }
}
#[cfg(test)]
pub(crate) unsafe fn dna_rtd_test_attach_queued(pointer: *mut c_void) -> i32 {
    let client = unsafe { &mut *pointer.cast::<NativeClient>() };
    if let Err(e) = client.check() {
        return hr(Err(e));
    }
    if !client.servers.is_empty() || !client.topics.is_empty() {
        return E_UNEXPECTED.0;
    }
    let state = std::rc::Rc::new(FixtureState::default());
    client.fixture = Some(state.clone());
    client
        .servers
        .insert("fixture".into(), Server::new(QueuedServer { state }.into()));
    0
}
#[cfg(test)]
pub(crate) unsafe fn dna_rtd_test_event(
    pointer: *mut c_void,
    index: i32,
    method: *mut i32,
    topic: *mut i32,
) -> i32 {
    if pointer.is_null() || method.is_null() || topic.is_null() || index < 0 {
        return E_INVALIDARG.0;
    }
    let client = unsafe { &*pointer.cast::<NativeClient>() };
    if let Err(e) = client.check() {
        return hr(Err(e));
    }
    if !client.servers.contains_key("fixture") {
        return S_FALSE.0;
    }
    let calls = client.fixture.as_ref().unwrap().calls.borrow();
    if let Some(&(m, t)) = calls.get(index as usize) {
        unsafe {
            *method = m;
            *topic = t;
        }
        0
    } else {
        S_FALSE.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Native-only: retains the RTD transport's scalar-kind refusals and array-valued #N/A.
    #[test]
    fn rtd_topic_values_keep_the_existing_automation_kind_rules() {
        for value in [
            AutoValue::Date(1.25),
            AutoValue::Currency(12500),
            AutoValue::Missing,
        ] {
            let value = values::to_variant(&values::NoObjects, &value).unwrap();
            assert_eq!(
                flatten(&value).unwrap_err(),
                AutoError::Status(DISP_E_TYPEMISMATCH.0)
            );
        }
        let array =
            values::ArrayOwner::new(unsafe { SafeArrayCreateVector(VT_VARIANT, 0, 1) }).unwrap();
        let value = values::tagged(
            VT_ARRAY | VT_VARIANT,
            VARIANT_0_0_0 {
                parray: array.take(),
            },
        );
        assert_eq!(
            flatten(&value).unwrap(),
            CalcValue::error(calc_value::WorksheetErrorCode::NA)
        );
    }

    /// Native-only: drives IRtdServer's real COM vtable and two-dimensional SAFEARRAY refresh.
    #[test]
    fn obsolete_transport_updates_cannot_reach_a_reconnected_topic() {
        let mut client = NativeClient::new();
        client.throttle = 0;
        assert_eq!(
            unsafe { dna_rtd_test_attach_queued((&mut client as *mut NativeClient).cast()) },
            0
        );
        assert_eq!(
            client.connect("fixture", 1, &["old".into()]).unwrap(),
            CalcValue::number(11.)
        );
        client.connect("fixture", 2, &["other".into()]).unwrap();
        client.disconnect(1).unwrap();
        assert_eq!(
            client.connect("fixture", 3, &["new".into()]).unwrap(),
            CalcValue::number(33.)
        );
        client.servers["fixture"]
            .notices
            .dirty
            .store(true, Ordering::Release);
        let mut values = Vec::new();
        client
            .poll(|id, value| {
                values.push((id, value.clone()));
                Ok(())
            })
            .unwrap();
        assert_eq!(
            values,
            [(2, CalcValue::number(22.)), (3, CalcValue::number(33.))]
        );
        client.fixture.as_ref().unwrap().unknown.set(true);
        client.servers["fixture"]
            .notices
            .dirty
            .store(true, Ordering::Release);
        assert_eq!(
            client.poll(|_, _| Ok(())).unwrap_err(),
            AutoError::Status(E_INVALIDARG.0)
        );
        client.close().unwrap();
    }

    /// Native-only: proves the update sink exposes IMarshal and admits cross-thread notifications.
    #[test]
    fn update_sink_is_free_threaded_and_disconnect_stops_notices() {
        let _apartment = crate::tests::Apartment::new();
        let notices = Notices::new();
        let sink: IRTDUpdateEvent = UpdateSink {
            notices: notices.clone(),
        }
        .into();
        assert!(
            sink.cast::<windows::Win32::System::Com::Marshal::IMarshal>()
                .is_ok()
        );
        let raw = sink.as_raw() as usize;
        std::thread::spawn(move || unsafe {
            IRTDUpdateEvent::from_raw_borrowed(&(raw as *mut c_void))
                .unwrap()
                .UpdateNotify()
                .ok()
                .unwrap();
        })
        .join()
        .unwrap();
        assert!(notices.dirty.swap(false, Ordering::AcqRel));
        unsafe {
            sink.Disconnect().ok().unwrap();
            sink.UpdateNotify().ok().unwrap();
        }
        assert!(!notices.dirty.load(Ordering::Acquire));
    }
}
