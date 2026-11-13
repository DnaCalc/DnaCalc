//! Windows controls drive the same COM identities, vtables and IDispatch as the applications.
use crate::{
    projection::Projection,
    runtime::{self, guid, id},
    values::{self, Objects},
};
use calc_object_model::{
    AutoArray, AutoError, AutoValue, Call, Descriptor, EventSource, InvokeKind, ObjectKey,
    ObjectModel, ObjectRef,
    profile::{ApplicationHost, values_only::ValuesOnly},
};
use calc_value::{CellKey, ExcelText, GridArea, ReferenceLike, SheetId};
use std::{
    cell::{Cell, RefCell},
    ffi::c_void,
    mem::ManuallyDrop,
    ptr,
    rc::Rc,
};
use windows::{
    Win32::{
        Foundation::*,
        System::{Com::*, Ole::*, Variant::*},
    },
    core::{BSTR, GUID, HRESULT, IUnknown, Interface, PCWSTR, implement},
};

pub(crate) struct Apartment;
impl Apartment {
    pub fn new() -> Self {
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
            .ok()
            .unwrap();
        Self
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}
fn projection() -> Projection {
    Projection::new(1, Rc::new(calc_xll_host::XllHost::new(1))).unwrap()
}
fn dispatch(value: VARIANT) -> IDispatch {
    IDispatch::try_from(&value).unwrap()
}
fn call(
    object: &IDispatch,
    name: &str,
    flags: DISPATCH_FLAGS,
    args: Vec<VARIANT>,
) -> windows::core::Result<VARIANT> {
    let name: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
    let mut dispid = 0;
    unsafe { object.GetIDsOfNames(&GUID::zeroed(), &PCWSTR(name.as_ptr()), 1, 0, &mut dispid) }?;
    let mut args: Vec<_> = args.into_iter().rev().collect();
    let mut named = -3;
    let put = flags.0 & (DISPATCH_PROPERTYPUT.0 | DISPATCH_PROPERTYPUTREF.0) != 0;
    let params = DISPPARAMS {
        rgvarg: args.as_mut_ptr(),
        rgdispidNamedArgs: if put { &mut named } else { ptr::null_mut() },
        cArgs: args.len() as u32,
        cNamedArgs: u32::from(put),
    };
    let mut result = VARIANT::default();
    unsafe {
        object.Invoke(
            dispid,
            &GUID::zeroed(),
            0,
            flags,
            &params,
            Some(&mut result),
            None,
            None,
        )
    }?;
    Ok(result)
}
fn get(object: &IDispatch, name: &str) -> VARIANT {
    call(object, name, DISPATCH_PROPERTYGET, vec![]).unwrap()
}
fn text(value: &VARIANT) -> String {
    unsafe { value.Anonymous.Anonymous.Anonymous.bstrVal.to_string() }
}

struct Recorded {
    inner: Rc<dyn ObjectModel>,
    refusals: Cell<u32>,
    released: Cell<u32>,
    calls: RefCell<Vec<(String, Vec<AutoValue>, u32)>>,
}
impl ObjectModel for Recorded {
    fn application(&self) -> ObjectRef {
        self.inner.application()
    }
    fn invoke(&self, target: &ObjectRef, call: &Call<'_>) -> Result<AutoValue, AutoError> {
        self.calls
            .borrow_mut()
            .push((call.label(), call.args.clone(), call.lcid));
        self.inner.invoke(target, call)
    }
    fn released(&self, target: &ObjectRef) {
        self.released.set(self.released.get() + 1);
        self.inner.released(target);
    }
    fn record_refusal(&self, error: &AutoError) {
        self.refusals.set(self.refusals.get() + 1);
        self.inner.record_refusal(error);
    }
}
fn recorded(inner: Rc<dyn ObjectModel>) -> Rc<Recorded> {
    Rc::new(Recorded {
        inner,
        refusals: Cell::new(0),
        released: Cell::new(0),
        calls: RefCell::default(),
    })
}

/// Native-only: Caller slot 74 and both Invoke exception forms must report the same Excel 1004.
#[test]
fn caller_and_this_cell_need_a_workbook_on_every_route() {
    let _apartment = Apartment::new();
    let projection = projection();
    let model = recorded(Rc::new(ValuesOnly::new(projection.application_host())));
    projection.bind_model(model.clone()).unwrap();
    let app = projection.application().unwrap();
    for name in ["Caller", "ThisCell"] {
        let member = Descriptor::get()
            .member(id("Application"), name, InvokeKind::Get)
            .unwrap();
        let mut output = VARIANT::default();
        let mut exception = EXCEPINFO::default();
        let params = DISPPARAMS::default();
        let failure = unsafe {
            app.Invoke(
                member.dispid,
                &GUID::zeroed(),
                0,
                DISPATCH_PROPERTYGET,
                &params,
                Some(&mut output),
                Some(&mut exception),
                None,
            )
        }
        .unwrap_err();
        assert_eq!(failure.code(), DISP_E_EXCEPTION);
        assert_eq!(exception.wCode, 1004);
        assert_eq!(exception.scode as u32, 0x800A03EC);
        assert_eq!(
            exception.bstrDescription.to_string(),
            "needs a workbook: open in DNA Calc"
        );
        unsafe { crate::events::clear_exception(&mut exception) };
        assert_eq!(
            unsafe {
                app.Invoke(
                    member.dispid,
                    &GUID::zeroed(),
                    0,
                    DISPATCH_PROPERTYGET,
                    &params,
                    None,
                    None,
                    None,
                )
            }
            .unwrap_err()
            .code()
            .0 as u32,
            0x800A03EC
        );
        let table = unsafe { *(app.as_raw() as *const *const *const c_void) };
        let status = if name == "Caller" {
            assert_eq!(member.slot, Some(74));
            let function: unsafe extern "system" fn(
                *mut c_void,
                ManuallyDrop<VARIANT>,
                u32,
                *mut VARIANT,
            ) -> HRESULT = unsafe { std::mem::transmute(*table.add(74)) }; // ubs:ignore - Function-pointer ABI and slot are checked against the descriptor and native layout tests.
            let missing = values::to_variant(&projection.state, &AutoValue::Missing).unwrap();
            unsafe {
                function(
                    app.as_raw(),
                    ManuallyDrop::new(ptr::read(&missing)),
                    1033,
                    &mut output,
                )
            }
        } else {
            let function: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT =
                unsafe { std::mem::transmute(*table.add(member.slot.unwrap() as usize)) }; // ubs:ignore - Function-pointer ABI and slot are checked against the descriptor and native layout tests.
            let mut out = ptr::null_mut();
            unsafe { function(app.as_raw(), &mut out) }
        };
        assert_eq!(status.0 as u32, 0x800A03EC);
        let error = unsafe { GetErrorInfo(0) }.unwrap();
        assert_eq!(
            unsafe { error.GetDescription() }.unwrap().to_string(),
            "needs a workbook: open in DNA Calc"
        );
    }
    assert_eq!(model.refusals.get(), 6);
}

#[derive(Default)]
struct RunHost {
    seen: RefCell<Vec<(String, usize)>>,
    events: Cell<bool>,
}
impl ApplicationHost for RunHost {
    fn hwnd(&self) -> isize {
        1
    }
    fn run(&self, name: &str, args: &[AutoValue]) -> Result<AutoValue, AutoError> {
        self.seen.borrow_mut().push((name.into(), args.len()));
        Ok(AutoValue::I4(37))
    }
    fn events_enabled(&self) -> bool {
        self.events.get()
    }
    fn set_events_enabled(&self, enabled: bool) {
        self.events.set(enabled);
    }
}
/// Native-only: calls the generated Run vtable slot using the real 31-VARIANT x64 ABI.
#[test]
fn early_bound_run_accepts_thirty_omitted_optionals() {
    let _apartment = Apartment::new();
    let projection = projection();
    let host = Rc::new(RunHost::default());
    projection
        .bind_model(Rc::new(ValuesOnly::new(host.clone())))
        .unwrap();
    assert_eq!(
        projection.run_early("SyncMacro_fixture").unwrap(),
        calc_value::CalcValue::number(37.)
    );
    assert_eq!(*host.seen.borrow(), [("SyncMacro_fixture".into(), 0)]);
}

/// Native-only: descriptor/layout verification includes every advertised IID and every actual slot.
#[test]
fn every_advertised_interface_has_exact_descriptor_vtable_length() {
    let _apartment = Apartment::new();
    let projection = projection();
    projection.verify_layout().unwrap();
    for (interface, ty) in Descriptor::get()
        .types()
        .filter(|(id, _)| runtime::advertised(*id))
    {
        let object = projection
            .state
            .wrap(&ObjectRef::new(interface, ObjectKey::Application))
            .unwrap();
        let mut raw = ptr::null_mut();
        unsafe { object.query(&guid(interface), &mut raw) }
            .ok()
            .unwrap();
        let held = unsafe { IUnknown::from_raw(raw) };
        assert_eq!(
            runtime::layout(interface).unwrap().len(),
            ty.vtable_slots.unwrap() as usize
        );
        assert!(
            runtime::layout(interface)
                .unwrap()
                .iter()
                .all(|entry| !entry.is_null())
        );
        let identity = held.cast::<IUnknown>().unwrap();
        assert_eq!(identity.as_raw(), object.as_raw());
        assert!(object.cast::<ISupportErrorInfo>().is_ok());
    }
}

/// Native-only: named arguments, property puts, VT_BYREF, LCID and reversed positional arguments.
#[test]
fn dispatch_decodes_names_reverse_order_and_byref_through_the_shared_model() {
    let _apartment = Apartment::new();
    let projection = projection();
    let host = Rc::new(RunHost::default());
    let model = recorded(Rc::new(ValuesOnly::new(host)));
    projection.bind_model(model.clone()).unwrap();
    let app = projection.application().unwrap();
    let strings: Vec<Vec<u16>> = ["rUn", "aRg2", "mAcRo"]
        .iter()
        .map(|s| s.encode_utf16().chain(Some(0)).collect())
        .collect();
    let names: Vec<_> = strings.iter().map(|s| PCWSTR(s.as_ptr())).collect();
    let mut ids = [0; 3];
    unsafe { app.GetIDsOfNames(&GUID::zeroed(), names.as_ptr(), 3, 1033, ids.as_mut_ptr()) }
        .unwrap();
    assert_eq!(ids, [0x103, 2, 0]);
    let mut numeric = 11i32;
    let byref = values::tagged(
        VT_BYREF | VT_I4,
        VARIANT_0_0_0 {
            plVal: &mut numeric,
        },
    );
    let mut args = [VARIANT::from(22i32), VARIANT::from("named"), byref];
    let mut named = [2i32, 0];
    let params = DISPPARAMS {
        rgvarg: args.as_mut_ptr(),
        rgdispidNamedArgs: named.as_mut_ptr(),
        cArgs: 3,
        cNamedArgs: 2,
    };
    unsafe {
        app.Invoke(
            ids[0],
            &GUID::zeroed(),
            1033,
            DISPATCH_METHOD,
            &params,
            None,
            None,
            None,
        )
    }
    .unwrap();
    let calls = model.calls.borrow();
    let (_, decoded, lcid) = calls.last().unwrap();
    assert_eq!(*lcid, 1033);
    assert_eq!(decoded[0].as_text().as_deref(), Some("named"));
    assert!(matches!(decoded[1], AutoValue::I4(11)));
    assert!(matches!(decoded[2], AutoValue::I4(22)));
    assert!(decoded[3..].iter().all(AutoValue::is_missing));
    drop(calls);
    call(
        &app,
        "EnableEvents",
        DISPATCH_PROPERTYPUT,
        vec![VARIANT::from(false)],
    )
    .unwrap();
    assert!(!projection.state.events_enabled().unwrap());
}

struct WorkbookFixture {
    fallback: ValuesOnly<Rc<dyn ApplicationHost>>,
    full: RefCell<String>,
    enabled: Cell<bool>,
    released: Cell<u32>,
}
impl WorkbookFixture {
    fn reference() -> ReferenceLike {
        ReferenceLike::area(
            GridArea::new(
                CellKey::new(SheetId::from_parts(7, 1), 1, 1).unwrap(),
                CellKey::new(SheetId::from_parts(7, 1), 2, 2).unwrap(),
            )
            .unwrap(),
        )
    }
    fn range() -> ObjectRef {
        ObjectRef::new(
            id("Range"),
            ObjectKey::Range(calc_func::StoredReference::Inline(Self::reference())),
        )
    }
}
impl ObjectModel for WorkbookFixture {
    fn application(&self) -> ObjectRef {
        self.fallback.application()
    }
    fn invoke(&self, target: &ObjectRef, call: &Call<'_>) -> Result<AutoValue, AutoError> {
        match (&target.key, call.member.name.as_str(), call.member.invoke) {
            (ObjectKey::Application, "Range" | "Caller" | "ThisCell", _) => {
                Ok(AutoValue::Object(Self::range()))
            }
            (ObjectKey::Application, "EnableEvents", InvokeKind::Get) => {
                Ok(AutoValue::Bool(self.enabled.get()))
            }
            (ObjectKey::Application, "EnableEvents", InvokeKind::Put) => {
                self.enabled
                    .set(calc_object_model::geometry::boolean(call.arg(0))?);
                Ok(AutoValue::Empty)
            }
            (ObjectKey::Application, "ActiveWorkbook", _) => Ok(AutoValue::Object(ObjectRef::new(
                id("Workbook"),
                ObjectKey::Workbook(1),
            ))),
            (ObjectKey::Workbook(_), "FullName", _) => Ok(AutoValue::text(&self.full.borrow())),
            (ObjectKey::Workbook(_), "Name", _) => Ok(AutoValue::text(
                self.full.borrow().rsplit(['/', '\\']).next().unwrap(),
            )),
            (ObjectKey::Workbook(_), "Path", _) => Ok(AutoValue::text(
                self.full
                    .borrow()
                    .rsplit_once(['/', '\\'])
                    .map(|p| p.0)
                    .unwrap_or(""),
            )),
            (ObjectKey::Range(_), "Address", _) => {
                Ok(AutoValue::text(&calc_object_model::geometry::address(
                    &Self::reference(),
                    calc_object_model::geometry::AddressOptions::from_call(call)?,
                    None,
                )?))
            }
            (ObjectKey::Range(_), "Row", _) => Ok(AutoValue::I4(1)),
            (ObjectKey::Range(_), "Count", _) => Ok(AutoValue::I4(4)),
            (ObjectKey::Range(_), "Value" | "Value2", _) => {
                Ok(AutoValue::Array(Box::new(AutoArray {
                    rows: 2,
                    columns: 3,
                    lower: (1, 1),
                    values: (10..16).map(|n| AutoValue::R8(f64::from(n))).collect(),
                })))
            }
            _ => self.fallback.invoke(target, call),
        }
    }
    fn released(&self, target: &ObjectRef) {
        if matches!(target.key, ObjectKey::Range(_)) {
            self.released.set(self.released.get() + 1);
        }
    }
}
fn workbook(projection: &Projection) -> Rc<WorkbookFixture> {
    let model = Rc::new(WorkbookFixture {
        fallback: ValuesOnly::new(projection.application_host()),
        full: RefCell::new("C:\\Models\\Named.xlsx".into()),
        enabled: Cell::new(true),
        released: Cell::new(0),
    });
    projection.bind_model(model.clone()).unwrap();
    model
}

/// Native-only: Range TypeName/FUNCDESCs, non-square SAFEARRAYs, CVErr and tracked-object release.
#[test]
fn workbook_ranges_project_metadata_values_and_type_information() {
    let _apartment = Apartment::new();
    let projection = projection();
    let model = workbook(&projection);
    let app = projection.application().unwrap();
    let range = dispatch(
        call(
            &app,
            "Range",
            DISPATCH_PROPERTYGET,
            vec![VARIANT::from("A1:B2")],
        )
        .unwrap(),
    );
    assert_eq!(text(&get(&range, "Address")), "$A$1:$B$2");
    let value = get(&range, "Value2");
    let array = unsafe { value.Anonymous.Anonymous.Anonymous.parray };
    assert_eq!(unsafe { SafeArrayGetDim(array) }, 2);
    assert_eq!(
        (
            unsafe { SafeArrayGetLBound(array, 1) }.unwrap(),
            unsafe { SafeArrayGetUBound(array, 1) }.unwrap()
        ),
        (1, 2)
    );
    assert_eq!(
        (
            unsafe { SafeArrayGetLBound(array, 2) }.unwrap(),
            unsafe { SafeArrayGetUBound(array, 2) }.unwrap()
        ),
        (1, 3)
    );
    let mut last = VARIANT::default();
    unsafe { SafeArrayGetElement(array, [2, 3].as_ptr(), (&mut last as *mut VARIANT).cast()) }
        .unwrap();
    assert_eq!(unsafe { last.Anonymous.Anonymous.Anonymous.dblVal }, 15.0);
    assert_eq!(unsafe { range.GetTypeInfoCount() }.unwrap(), 1);
    let info = unsafe { range.GetTypeInfo(0, 0) }.unwrap();
    let mut name = BSTR::new();
    unsafe { info.GetDocumentation(-1, Some(&mut name), None, ptr::null_mut(), None) }.unwrap();
    assert_eq!(name.to_string(), "Range");
    let attribute = unsafe { info.GetTypeAttr() }.unwrap();
    assert_eq!(
        unsafe { (*attribute).cFuncs as usize },
        Descriptor::get().members(id("Range")).len()
    );
    unsafe {
        info.ReleaseTypeAttr(attribute);
    }
    let error = values::to_variant(&projection.state, &AutoValue::Error(2007)).unwrap();
    assert_eq!(error.vt(), VT_ERROR);
    assert_eq!(
        unsafe { error.Anonymous.Anonymous.Anonymous.scode as u32 },
        0x800A07D7
    );
    drop(range);
    assert_eq!(model.released.get(), 1);
}

/// Native-only: replacement for the C++ workbook bridge test through the supplied ObjectModel.
#[test]
fn workbook_metadata_and_enable_events_follow_the_supplied_model() {
    let _apartment = Apartment::new();
    let projection = projection();
    let model = workbook(&projection);
    let app = projection.application().unwrap();
    let book = dispatch(get(&app, "ActiveWorkbook"));
    assert_eq!(text(&get(&book, "Name")), "Named.xlsx");
    assert_eq!(text(&get(&book, "Path")), "C:\\Models");
    *model.full.borrow_mut() = "D:\\Other\\Saved.xlsx".into();
    assert_eq!(text(&get(&book, "FullName")), "D:\\Other\\Saved.xlsx");
    call(
        &app,
        "EnableEvents",
        DISPATCH_PROPERTYPUT,
        vec![VARIANT::from(false)],
    )
    .unwrap();
    assert!(!model.enabled.get());
    model.enabled.set(true);
    assert!(matches!(
        values::from_variant(&projection.state, &get(&app, "EnableEvents")).unwrap(),
        AutoValue::Bool(true)
    ));
}

/// Native-only: actual Office controls retain deletion/disconnection state and exact QI layouts.
#[test]
fn office_controls_track_edits_and_disconnect_retained_objects() {
    let _apartment = Apartment::new();
    let projection = projection();
    let app = projection.application().unwrap();
    let addins = dispatch(get(&app, "COMAddIns"));
    let bars = dispatch(get(&app, "CommandBars"));
    let bar = dispatch(
        call(
            &bars,
            "Item",
            DISPATCH_PROPERTYGET,
            vec![VARIANT::from(1i32)],
        )
        .unwrap(),
    );
    let controls = dispatch(get(&bar, "Controls"));
    let popup = dispatch(
        call(
            &controls,
            "Add",
            DISPATCH_METHOD,
            vec![VARIANT::from(10i32)],
        )
        .unwrap(),
    );
    call(
        &popup,
        "Caption",
        DISPATCH_PROPERTYPUT,
        vec![VARIANT::from("Private test menu")],
    )
    .unwrap();
    assert_eq!(text(&get(&popup, "Caption")), "Private test menu");
    assert!(matches!(
        values::from_variant(&projection.state, &get(&controls, "Count")).unwrap(),
        AutoValue::I4(1)
    ));
    // Rule 4 explicitly withholds an IID if its inherited descriptor layout is incomplete.
    for name in ["CommandBarPopup", "CommandBarControl"] {
        let mut raw = ptr::null_mut();
        let result = unsafe { popup.query(&guid(id(name)), &mut raw) };
        assert_eq!(result.is_ok(), runtime::advertised(id(name)));
        if !raw.is_null() {
            drop(unsafe { IUnknown::from_raw(raw) });
        }
    }
    let mut other = ptr::null_mut();
    assert_eq!(
        unsafe { popup.query(&guid(id("_CommandBarButton")), &mut other) },
        E_NOINTERFACE
    );
    call(&popup, "Delete", DISPATCH_METHOD, vec![]).unwrap();
    assert_eq!(
        call(&popup, "Caption", DISPATCH_PROPERTYGET, vec![])
            .unwrap_err()
            .code(),
        CO_E_OBJNOTCONNECTED
    );
    assert!(matches!(
        values::from_variant(&projection.state, &get(&controls, "Count")).unwrap(),
        AutoValue::I4(0)
    ));
    drop(projection);
    assert_eq!(
        call(&controls, "Count", DISPATCH_PROPERTYGET, vec![])
            .unwrap_err()
            .code(),
        CO_E_OBJNOTCONNECTED
    );
    let table = unsafe { *(addins.as_raw() as *const *const *const c_void) };
    let item: unsafe extern "system" fn(*mut c_void, *const VARIANT, *mut *mut c_void) -> HRESULT =
        unsafe { std::mem::transmute(*table.add(9)) }; // ubs:ignore - Function-pointer ABI and slot are checked against the descriptor and native layout tests.
    let mut out = ptr::null_mut();
    assert_eq!(
        unsafe { item(addins.as_raw(), &VARIANT::from(1i32), &mut out) },
        CO_E_OBJNOTCONNECTED
    );
}

#[windows::core::interface("00024413-0000-0000-C000-000000000046")]
unsafe trait AppEvents: IDispatch {}
#[derive(Default)]
struct EventState {
    changes: Cell<u32>,
    calculations: Cell<u32>,
    retained: RefCell<Option<IDispatch>>,
    remove: RefCell<Option<(IConnectionPoint, u32)>>,
}
#[implement(AppEvents, Agile = false)]
struct EventProbe {
    state: Rc<EventState>,
}
impl AppEvents_Impl for EventProbe_Impl {}
impl IDispatch_Impl for EventProbe_Impl {
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
        dispid: i32,
        iid: *const GUID,
        _: u32,
        flags: DISPATCH_FLAGS,
        params: *const DISPPARAMS,
        _: *mut VARIANT,
        _: *mut EXCEPINFO,
        _: *mut u32,
    ) -> windows::core::Result<()> {
        assert_eq!(unsafe { *iid }, GUID::zeroed());
        assert_eq!(flags.0, 3);
        let params = unsafe { &*params };
        if dispid
            == Descriptor::get()
                .dispid_of(id("AppEvents"), "AfterCalculate")
                .unwrap()
        {
            assert_eq!(params.cArgs, 0);
            self.state
                .calculations
                .set(self.state.calculations.get() + 1);
        } else {
            assert_eq!(
                dispid,
                Descriptor::get()
                    .dispid_of(id("AppEvents"), "SheetChange")
                    .unwrap()
            );
            assert_eq!((params.cArgs, params.cNamedArgs), (2, 2));
            assert_eq!(
                unsafe { std::slice::from_raw_parts(params.rgdispidNamedArgs, 2) },
                [0, 1]
            );
            let range = IDispatch::try_from(unsafe { &*params.rgvarg.add(1) }).unwrap();
            assert_eq!(text(&get(&range, "Address")), "$A$1:$B$2");
            *self.state.retained.borrow_mut() = Some(range);
            self.state.changes.set(self.state.changes.get() + 1);
            let remove = self.state.remove.take();
            if let Some((point, cookie)) = remove {
                unsafe { point.Unadvise(cookie) }?;
            }
        }
        Ok(())
    }
}

/// Native-only: source-owned cookies, snapshot delivery, reentrant Unadvise and disconnected ranges.
#[test]
fn event_source_owns_cookies_and_disconnects_retained_ranges() {
    let _apartment = Apartment::new();
    let projection = projection();
    let model = workbook(&projection);
    let app = projection.application().unwrap();
    let container = app.cast::<IConnectionPointContainer>().unwrap();
    let first = unsafe { container.FindConnectionPoint(&guid(id("AppEvents"))) }.unwrap();
    let second = unsafe { container.FindConnectionPoint(&guid(id("AppEvents"))) }.unwrap();
    let a = Rc::new(EventState::default());
    let b = Rc::new(EventState::default());
    let sink_a: AppEvents = EventProbe { state: a.clone() }.into();
    let sink_b: AppEvents = EventProbe { state: b.clone() }.into();
    let cookie_a = unsafe { first.Advise(&sink_a.cast::<IUnknown>().unwrap()) }.unwrap();
    let cookie_b = unsafe { second.Advise(&sink_b.cast::<IUnknown>().unwrap()) }.unwrap();
    *a.remove.borrow_mut() = Some((second.clone(), cookie_a));
    let args = [
        AutoValue::Object(ObjectRef::new(
            id("Worksheet"),
            ObjectKey::Worksheet {
                workbook: 1,
                sheet: SheetId::from_parts(7, 1),
            },
        )),
        AutoValue::Object(WorkbookFixture::range()),
    ];
    for _ in 0..2 {
        projection
            .raise(&model.application(), "AppEvents", "SheetChange", &args)
            .unwrap();
    }
    projection.enable_events(false).unwrap();
    projection
        .raise(&model.application(), "AppEvents", "SheetChange", &args)
        .unwrap();
    projection.after_calculate().unwrap();
    projection.enable_events(true).unwrap();
    projection.after_calculate().unwrap();
    assert_eq!(
        (
            a.changes.get(),
            b.changes.get(),
            a.calculations.get(),
            b.calculations.get()
        ),
        (1, 2, 0, 1)
    );
    unsafe { first.Unadvise(cookie_b) }.unwrap();
    assert_eq!(
        unsafe { second.Unadvise(cookie_b) }.unwrap_err().code(),
        CONNECT_E_NOCONNECTION
    );
    drop(projection);
    let retained = b.retained.borrow();
    assert_eq!(
        call(
            retained.as_ref().unwrap(),
            "Address",
            DISPATCH_PROPERTYGET,
            vec![]
        )
        .unwrap_err()
        .code(),
        CO_E_OBJNOTCONNECTED
    );
}

type SinkAddRef = unsafe extern "system" fn(*mut c_void) -> u32;
struct SinkAddHook {
    original: SinkAddRef,
    remove: Rc<RefCell<Option<(IConnectionPoint, u32)>>>,
    result: Rc<Cell<HRESULT>>,
}
thread_local! { static SINK_ADD_HOOK: RefCell<Option<SinkAddHook>> = const { RefCell::new(None) }; }
unsafe extern "system" fn sink_add_ref_unadvises(this: *mut c_void) -> u32 {
    let (original, remove, result) = SINK_ADD_HOOK.with(|hook| {
        let hook = hook.borrow();
        let hook = hook.as_ref().unwrap();
        (hook.original, hook.remove.clone(), hook.result.clone())
    });
    let count = unsafe { original(this) };
    let removed = remove.take();
    if let Some((point, cookie)) = removed {
        result.set(unsafe { point.Unadvise(cookie) }.map_or_else(|e| e.code(), |()| S_OK));
    }
    count
}
struct SinkVtableHook {
    pointer: *mut *const *const c_void,
    original: *const *const c_void,
    table: Vec<*const c_void>,
}
impl Drop for SinkVtableHook {
    fn drop(&mut self) {
        unsafe { *self.pointer = self.original };
        SINK_ADD_HOOK.with(|hook| {
            hook.borrow_mut().take();
        });
    }
}

/// Native-only: a provider's AddRef can synchronously Unadvise; snapshotting must allow this reentry.
#[test]
fn event_snapshots_do_not_borrow_subscriptions_across_provider_add_ref() {
    let _apartment = Apartment::new();
    let projection = projection();
    let app = projection.application().unwrap();
    let container = app.cast::<IConnectionPointContainer>().unwrap();
    let point = unsafe { container.FindConnectionPoint(&guid(id("AppEvents"))) }.unwrap();
    let state = Rc::new(EventState::default());
    let sink: AppEvents = EventProbe {
        state: state.clone(),
    }
    .into();
    let pointer = sink.as_raw() as *mut *const *const c_void;
    let original = unsafe { *pointer };
    let mut patch = SinkVtableHook {
        pointer,
        original,
        table: unsafe { std::slice::from_raw_parts(original, 7) }.to_vec(),
    };
    let remove = Rc::new(RefCell::new(None));
    let result = Rc::new(Cell::new(S_OK));
    SINK_ADD_HOOK.with(|hook| {
        *hook.borrow_mut() = Some(SinkAddHook {
            original: unsafe { std::mem::transmute(patch.table[1]) }, // ubs:ignore - IUnknown slot 1 is AddRef with this exact ABI; the copied vtable stays owned by the guard.
            remove: remove.clone(),
            result: result.clone(),
        });
    });
    patch.table[1] = sink_add_ref_unadvises as *const c_void;
    unsafe { *pointer = patch.table.as_ptr() };
    let cookie = unsafe { point.Advise(&sink.cast::<IUnknown>().unwrap()) }.unwrap();
    *remove.borrow_mut() = Some((point.clone(), cookie));
    projection.after_calculate().unwrap();
    assert_eq!(state.calculations.get(), 1);
    assert_eq!(result.get(), S_OK);
    // Exercise the armed hook explicitly if snapshotting retained a Rust lease instead.
    let held = sink.clone();
    assert!(remove.borrow().is_none());
    assert_eq!(result.get(), S_OK);
    drop(held);
    projection.after_calculate().unwrap();
    assert_eq!(state.calculations.get(), 1);
    drop(patch);
}

/// Native-only: event acquisitions release once with no subscribers, disabled events or a failed raise.
#[test]
fn event_arguments_release_with_no_delivery_and_on_errors() {
    let _apartment = Apartment::new();
    let projection = projection();
    let model = workbook(&projection);
    let args = [
        AutoValue::Object(ObjectRef::new(
            id("Worksheet"),
            ObjectKey::Worksheet {
                workbook: 1,
                sheet: SheetId::from_parts(7, 1),
            },
        )),
        AutoValue::Object(WorkbookFixture::range()),
    ];
    projection
        .raise(&model.application(), "AppEvents", "SheetChange", &args)
        .unwrap();
    assert_eq!(model.released.get(), 1);
    projection.enable_events(false).unwrap();
    projection
        .raise(&model.application(), "AppEvents", "SheetChange", &args)
        .unwrap();
    assert_eq!(model.released.get(), 2);
    projection.enable_events(true).unwrap();
    assert!(
        projection
            .raise(&model.application(), "AppEvents", "unknown event", &args)
            .is_err()
    );
    assert_eq!(model.released.get(), 3);
    let malformed = [
        AutoValue::Foreign(u64::MAX),
        AutoValue::Object(WorkbookFixture::range()),
    ];
    assert!(
        projection
            .raise(&model.application(), "AppEvents", "SheetChange", &malformed)
            .is_err()
    );
    assert_eq!(model.released.get(), 4);
}

/// Native-only: calls the actual generated COM ABI from another thread, then after disconnection.
#[test]
fn sta_and_disconnection_checks_apply_to_dispatch_and_vtables() {
    let _apartment = Apartment::new();
    let projection = projection();
    let app = projection.application().unwrap();
    let raw = app.as_raw() as usize;
    let result = std::thread::spawn(move || {
        let raw = raw as *mut c_void;
        let table = unsafe { *(raw as *const *const *const c_void) };
        let hwnd: unsafe extern "system" fn(*mut c_void, *mut i32) -> HRESULT =
            unsafe { std::mem::transmute(*table.add(351)) }; // ubs:ignore - Function-pointer ABI and slot are checked against the descriptor and native layout tests.
        let mut value = 99;
        let status = unsafe { hwnd(raw, &mut value) };
        let query: unsafe extern "system" fn(
            *mut c_void,
            *const GUID,
            *mut *mut c_void,
        ) -> HRESULT = unsafe { std::mem::transmute(*table) }; // ubs:ignore - Function-pointer ABI and slot are checked against the descriptor and native layout tests.
        let mut out = ptr::null_mut();
        let query_status = unsafe { query(raw, &IDispatch::IID, &mut out) };
        (status.0, query_status.0)
    })
    .join()
    .unwrap();
    assert_eq!(result, (RPC_E_WRONG_THREAD.0, RPC_E_WRONG_THREAD.0));
    drop(projection);
    assert_eq!(
        call(&app, "Version", DISPATCH_PROPERTYGET, vec![])
            .unwrap_err()
            .code(),
        CO_E_OBJNOTCONNECTED
    );
}

/// Native-only: unimplemented getters have the same descriptor-named refusal on both COM routes.
#[test]
fn unsupported_members_record_uniform_refusals_and_error_info() {
    let _apartment = Apartment::new();
    let projection = projection();
    let model = recorded(Rc::new(ValuesOnly::new(projection.application_host())));
    projection.bind_model(model.clone()).unwrap();
    let app = projection.application().unwrap();
    let table = unsafe { *(app.as_raw() as *const *const *const c_void) };
    let iteration: unsafe extern "system" fn(*mut c_void, u32, *mut i16) -> HRESULT =
        unsafe { std::mem::transmute(*table.add(171)) }; // ubs:ignore - Function-pointer ABI and slot are checked against the descriptor and native layout tests.
    let mut value = 99;
    assert_eq!(
        unsafe { iteration(app.as_raw(), 1033, &mut value) }.0 as u32,
        0x800A03EC
    );
    assert_eq!(value, 0);
    let error = unsafe { GetErrorInfo(0) }.unwrap();
    let early = unsafe { error.GetDescription() }.unwrap().to_string();
    let mut exception = EXCEPINFO::default();
    let params = DISPPARAMS::default();
    let dispid = Descriptor::get()
        .dispid_of(id("Application"), "Iteration")
        .unwrap();
    assert_eq!(
        unsafe {
            app.Invoke(
                dispid,
                &GUID::zeroed(),
                1033,
                DISPATCH_PROPERTYGET,
                &params,
                None,
                Some(&mut exception),
                None,
            )
        }
        .unwrap_err()
        .code(),
        DISP_E_EXCEPTION
    );
    assert_eq!(
        early,
        "DNA Calc does not model _Application.Iteration yet (dc-1nbp.5)"
    );
    assert_eq!(exception.bstrDescription.to_string(), early);
    unsafe { crate::events::clear_exception(&mut exception) };
    assert_eq!(model.refusals.get(), 2);
}

/// Native-only: a measured Excel error without description must not reuse an earlier refusal's IErrorInfo.
#[test]
fn empty_excel_descriptions_clear_previous_error_information() {
    let _apartment = Apartment::new();
    let refusal = AutoError::not_modelled("_Application.Iteration", "dc-1nbp.5");
    assert_eq!(
        runtime::error(&refusal, ptr::null_mut()).0 as u32,
        0x800A03EC
    );
    assert_eq!(
        runtime::error(&AutoError::excel(1004, ""), ptr::null_mut()).0 as u32,
        0x800A03EC
    );
    assert!(unsafe { GetErrorInfo(0) }.is_err());
}

/// Native-only: lossless UTF-16, Currency/Date/Null and CVErr use the one Automation converter.
#[test]
fn automation_values_preserve_types_bits_and_unpaired_utf16() {
    let _apartment = Apartment::new();
    let projection = projection();
    for value in [
        AutoValue::Empty,
        AutoValue::Null,
        AutoValue::Missing,
        AutoValue::Bool(true),
        AutoValue::I2(-7),
        AutoValue::I4(-123),
        AutoValue::R4(-0.0),
        AutoValue::R8(-0.0),
        AutoValue::Date(1.25),
        AutoValue::Currency(i64::MIN + 1),
        AutoValue::Error(2042),
        AutoValue::Text(ExcelText::from_utf16_code_units(vec![65, 0, 0xD800, 66])),
    ] {
        let encoded = values::to_variant(&projection.state, &value).unwrap();
        let decoded = values::from_variant(&projection.state, &encoded).unwrap();
        assert_eq!(format!("{decoded:?}"), format!("{value:?}"));
    }
}
