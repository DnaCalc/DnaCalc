//! Workbook members through the same ObjectModel used by the native projection.
use calc_func::{UserFunction, UserFunctionContext};
use calc_host::{
    automation::{AutomationHost, WorkbookAutomation, busy},
    *,
};
use calc_object_model::profile::ApplicationHost;
use calc_object_model::{
    AutoError, AutoValue, Call, Descriptor, EventSource, InvokeKind, ObjectModel, ObjectRef,
};
use calc_value::{CalcValue, CellKey};
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
    sync::Arc,
};

type Reply = Result<AutoValue, AutoError>;
trait Invocation {
    fn call(&self, f: &mut dyn FnMut(&mut dyn UserFunctionContext) -> Reply) -> Reply;
}
struct Loan<'a>(RefCell<&'a mut dyn UserFunctionContext>);
impl Invocation for Loan<'_> {
    fn call(&self, f: &mut dyn FnMut(&mut dyn UserFunctionContext) -> Reply) -> Reply {
        let mut context = self.0.try_borrow_mut().map_err(|_| busy())?;
        f(&mut **context)
    }
}
scoped_tls_hkt::scoped_thread_local!(static INVOCATION: for<'a> &'a (dyn Invocation + 'a));

struct Host {
    session: RefCell<HostSession>,
    enabled: Cell<bool>,
    commands: Cell<u32>,
}
impl ApplicationHost for Host {
    fn hwnd(&self) -> isize {
        123
    }
    fn run(&self, _: &str, _: &[AutoValue]) -> Reply {
        Err(AutoError::not_modelled("Run in test host", "dc-1nbp.5"))
    }
    fn events_enabled(&self) -> bool {
        self.enabled.get()
    }
    fn set_events_enabled(&self, enabled: bool) {
        self.enabled.set(enabled);
    }
}
impl AutomationHost for Host {
    fn with_invocation(
        &self,
        f: &mut dyn FnMut(&mut dyn UserFunctionContext) -> Reply,
    ) -> Option<Reply> {
        if INVOCATION.is_set() {
            Some(INVOCATION.with(|context| context.call(f)))
        } else {
            None
        }
    }
    fn with_session(&self, f: &mut dyn FnMut(&HostSession) -> Reply) -> Reply {
        self.commands.set(self.commands.get() + 1);
        let session = self.session.try_borrow().map_err(|_| busy())?;
        f(&session)
    }
    fn with_session_mut(&self, f: &mut dyn FnMut(&mut HostSession) -> Reply) -> Reply {
        self.commands.set(self.commands.get() + 1);
        let mut session = self.session.try_borrow_mut().map_err(|_| busy())?;
        f(&mut session)
    }
}
fn setup() -> (Rc<Host>, Rc<WorkbookAutomation>) {
    let session = HostSession::open(
        Arc::new(MemorySource::new(
            include_bytes!("fixtures/dc_09v/formats.xlsx").to_vec(),
        )),
        &OpenPolicy::default(),
        HostOptions {
            calc_mode: Some(CalcMode::Manual),
            ..Default::default()
        },
    )
    .unwrap();
    setup_session(session)
}
fn setup_session(session: HostSession) -> (Rc<Host>, Rc<WorkbookAutomation>) {
    let host = Rc::new(Host {
        session: RefCell::new(session),
        enabled: Cell::new(true),
        commands: Cell::new(0),
    });
    let model = Rc::new(WorkbookAutomation::new(host.clone()));
    model.context_changed(&host.session.borrow());
    (host, model)
}
fn invoke(
    model: &WorkbookAutomation,
    object: &ObjectRef,
    name: &str,
    args: Vec<AutoValue>,
) -> Reply {
    invoke_kind(model, object, name, InvokeKind::Get, args)
}
fn invoke_kind(
    model: &WorkbookAutomation,
    object: &ObjectRef,
    name: &str,
    kind: InvokeKind,
    args: Vec<AutoValue>,
) -> Reply {
    model.invoke(
        object,
        &Call {
            interface: object.interface,
            member: Descriptor::get()
                .member(object.interface, name, kind)
                .unwrap(),
            args,
            lcid: 0,
        },
    )
}
fn obj(value: AutoValue) -> ObjectRef {
    if let AutoValue::Object(o) = value {
        o
    } else {
        panic!("object: {value:?}")
    }
}
fn range(model: &WorkbookAutomation, text: &str) -> ObjectRef {
    obj(invoke(
        model,
        &model.application(),
        "Range",
        vec![AutoValue::text(text)],
    )
    .unwrap())
}
fn at(host: &Host, row: u32, column: u32) -> CellKey {
    CellKey::new(host.session.borrow().host().sheets()[0].id(), row, column).unwrap()
}
fn enter(host: &Host, at: CellKey, text: &str) {
    assert!(
        host.session
            .borrow_mut()
            .dispatch(SkinIntent::EnterCell {
                at,
                text: text.into()
            })
            .accepted()
    );
}
fn text(model: &WorkbookAutomation, object: &ObjectRef, name: &str) -> String {
    invoke(model, object, name, vec![])
        .unwrap()
        .as_text()
        .unwrap()
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn workbook_range_members_follow_identity_selection_and_address_flags() {
    let (host, model) = setup();
    host.session
        .borrow_mut()
        .set_file_identity("Named.xlsx", false);
    host.session
        .borrow_mut()
        .set_file_location(r"C:\Models\Named.xlsx".into());
    let app = model.application();
    let books = obj(invoke(&model, &app, "Workbooks", vec![]).unwrap());
    assert!(matches!(
        invoke(&model, &books, "Count", vec![]).unwrap(),
        AutoValue::I4(1)
    ));
    let book = obj(invoke(&model, &books, "Item", vec![AutoValue::text("named.xlsx")]).unwrap());
    for (name, expected) in [
        ("Name", "Named.xlsx"),
        ("FullName", r"C:\Models\Named.xlsx"),
        ("Path", r"C:\Models"),
    ] {
        assert_eq!(text(&model, &book, name), expected);
    }
    let a = range(&model, "A1:B2");
    let b = range(&model, "D5");
    let overlap = range(&model, "B2:C3");
    let union = obj(invoke_kind(
        &model,
        &app,
        "Union",
        InvokeKind::Method,
        vec![
            AutoValue::Object(a.clone()),
            AutoValue::Object(b),
            AutoValue::Object(overlap),
        ],
    )
    .unwrap());
    assert_eq!(text(&model, &union, "Address"), "$A$1:$B$2,$D$5,$B$2:$C$3");
    assert!(matches!(
        invoke(&model, &union, "Count", vec![]).unwrap(),
        AutoValue::I4(9)
    ));
    for (row, col, expected) in [
        (false, false, "A1:B2,D5,B2:C3"),
        (false, true, "$A1:$B2,$D5,$B2:$C3"),
        (true, false, "A$1:B$2,D$5,B$2:C$3"),
    ] {
        assert_eq!(
            invoke(
                &model,
                &union,
                "Address",
                vec![AutoValue::Bool(row), AutoValue::Bool(col)]
            )
            .unwrap()
            .as_text()
            .unwrap(),
            expected
        );
    }
    assert_eq!(
        invoke(
            &model,
            &union,
            "Address",
            vec![AutoValue::Missing, AutoValue::Missing, AutoValue::I4(-4150)]
        )
        .unwrap()
        .as_text()
        .unwrap(),
        "R1C1:R2C2,R5C4,R2C2:R3C3"
    );
    let sheet = obj(invoke(&model, &union, "Worksheet", vec![]).unwrap());
    let name = text(&model, &sheet, "Name");
    let qualifier = calc_fml::reference::format_workbook_sheet_qualifier("Named.xlsx", &name);
    assert_eq!(
        invoke(
            &model,
            &union,
            "Address",
            vec![
                AutoValue::Missing,
                AutoValue::Missing,
                AutoValue::Missing,
                AutoValue::Bool(true)
            ]
        )
        .unwrap()
        .as_text()
        .unwrap(),
        format!("{qualifier}!$A$1:$B$2,$D$5,$B$2:$C$3")
    );
    for axis in ["Rows", "Columns"] {
        let view = obj(invoke(&model, &union, axis, vec![]).unwrap());
        assert!(matches!(
            invoke(&model, &view, "Count", vec![]).unwrap(),
            AutoValue::I4(2)
        ));
        assert_eq!(
            text(&model, &view, "Address"),
            text(&model, &union, "Address")
        );
        model.released(&view);
    }
    host.session
        .borrow_mut()
        .set_file_location(r"D:\Other\Saved.xlsx".into());
    assert_eq!(text(&model, &book, "Name"), "Saved.xlsx");
    assert_eq!(text(&model, &book, "Path"), r"D:\Other");
    invoke_kind(
        &model,
        &app,
        "EnableEvents",
        InvokeKind::Put,
        vec![AutoValue::Bool(false)],
    )
    .unwrap();
    assert!(!host.session.borrow().workbook_events_enabled());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn value2_returns_first_area_one_based_arrays_and_scalar_types() {
    let (host, model) = setup();
    for (row, col, value) in [
        (10, 1, "7"),
        (10, 2, "text"),
        (11, 1, "TRUE"),
        (11, 2, "#N/A"),
    ] {
        enter(&host, at(&host, row, col), value);
    }
    let a = range(&model, "A10:B11");
    let AutoValue::Array(array) = invoke(&model, &a, "Value2", vec![]).unwrap() else {
        panic!("array");
    };
    assert_eq!((array.rows, array.columns, array.lower), (2, 2, (1, 1)));
    assert!(matches!(array.values[0], AutoValue::R8(7.)));
    assert_eq!(array.values[1].as_text().as_deref(), Some("text"));
    assert!(matches!(array.values[2], AutoValue::Bool(true)));
    assert!(matches!(array.values[3], AutoValue::Error(2042)));
    let single = range(&model, "A10");
    let union = obj(invoke_kind(
        &model,
        &model.application(),
        "Union",
        InvokeKind::Method,
        vec![
            AutoValue::Object(single),
            AutoValue::Object(range(&model, "D10:E11")),
        ],
    )
    .unwrap());
    assert!(matches!(
        invoke(&model, &union, "Value2", vec![]).unwrap(),
        AutoValue::R8(7.)
    ));
    assert!(matches!(
        invoke(&model, &range(&model, "Z100"), "Value2", vec![]).unwrap(),
        AutoValue::Empty
    ));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn held_range_tracks_insert_then_fails_typed_after_delete_and_replacement() {
    let (host, model) = setup();
    let held = range(&model, "A10");
    let sheet = at(&host, 10, 1).sheet();
    assert!(
        host.session
            .borrow_mut()
            .dispatch(SkinIntent::EditAxis(
                AxisEdit::new(sheet, Axis::Rows, AxisChange::Insert, 1, 1).unwrap()
            ))
            .accepted()
    );
    assert_eq!(text(&model, &held, "Address"), "$A$11");
    assert!(
        host.session
            .borrow_mut()
            .dispatch(SkinIntent::EditAxis(
                AxisEdit::new(sheet, Axis::Rows, AxisChange::Delete, 11, 1).unwrap()
            ))
            .accepted()
    );
    let error = invoke(&model, &held, "Value2", vec![]).unwrap_err();
    assert!(matches!(error, AutoError::Excel { number: 424, .. }));
    let book = obj(invoke(&model, &model.application(), "ActiveWorkbook", vec![]).unwrap());
    *host.session.borrow_mut() = HostSession::open(
        Arc::new(MemorySource::new(blank_package())),
        &OpenPolicy::default(),
        HostOptions::default(),
    )
    .unwrap();
    assert_eq!(
        invoke(&model, &book, "Name", vec![]).unwrap_err(),
        AutoError::Disconnected
    );
    assert!(invoke(&model, &held, "Address", vec![]).is_err());
}

struct Events {
    model: Weak<WorkbookAutomation>,
    changes: RefCell<Vec<(String, u64)>>,
    targets: RefCell<Vec<ObjectRef>>,
    calculated: Cell<usize>,
}
impl EventSource for Events {
    fn raise(
        &self,
        _: &ObjectRef,
        interface: &str,
        event: &str,
        args: &[AutoValue],
    ) -> Result<(), AutoError> {
        assert_eq!(interface, "AppEvents");
        let model = self.model.upgrade().unwrap();
        if event == "AfterCalculate" {
            self.calculated.set(self.calculated.get() + 1);
            return Ok(());
        }
        if event == "SheetCalculate" {
            return Ok(());
        }
        assert_eq!(event, "SheetChange");
        assert_eq!(args.len(), 2);
        let target = obj(args[1].clone());
        let _sheet = obj(args[0].clone());
        let address = text(&model, &target, "Address");
        let value = invoke(&model, &target, "Value", vec![])?;
        assert!(matches!(value, AutoValue::R8(_) | AutoValue::Array(_)));
        let count = match invoke(&model, &target, "Count", vec![])? {
            AutoValue::I4(n) => n as u64,
            _ => panic!("Count"),
        };
        self.changes.borrow_mut().push((address, count));
        self.targets.borrow_mut().push(target);
        Ok(())
    }
}
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn events_read_target_in_borrowed_session_preserve_all_areas_and_share_switch() {
    let (host, model) = setup();
    let events = Rc::new(Events {
        model: Rc::downgrade(&model),
        changes: RefCell::default(),
        targets: RefCell::default(),
        calculated: Cell::new(0),
    });
    host.session
        .borrow_mut()
        .set_workbook_event_observer(Some(model.observer(events.clone())));
    let sheet = at(&host, 1, 1).sheet();
    let edits = [(20, 2), (20, 3), (21, 2), (21, 3), (25, 6)]
        .into_iter()
        .map(|(r, c)| CellEdit::Set {
            at: CellKey::new(sheet, r, c).unwrap(),
            input: CellInput::Number(FiniteNumber::new(7.).unwrap()),
        })
        .collect();
    assert!(
        host.session
            .borrow_mut()
            .dispatch(SkinIntent::Edit { edits })
            .accepted()
    );
    assert_eq!(
        &*events.changes.borrow(),
        &[("$B$20:$C$21,$F$25".into(), 5)]
    );
    assert!(
        host.session
            .borrow()
            .workbook_event_failures()
            .failures
            .is_empty()
    );
    let held = events.targets.borrow()[0].clone();
    assert_eq!(text(&model, &held, "Address"), "$B$20:$C$21,$F$25");
    host.session.borrow_mut().dispatch(SkinIntent::Recalculate);
    while host.session.borrow().has_pending_calculation() {
        host.session.borrow_mut().advance_calculation();
    }
    assert_eq!(events.calculated.get(), 1);
    // Calculate Now with nothing left to calculate is still a completed calculation: Excel raises
    // AfterCalculate for it (an empty workbook's Calculate acknowledges xlOil's start, X4).
    host.session.borrow_mut().dispatch(SkinIntent::Recalculate);
    while host.session.borrow().has_pending_calculation() {
        host.session.borrow_mut().advance_calculation();
    }
    assert_eq!(events.calculated.get(), 2);
    invoke_kind(
        &model,
        &model.application(),
        "EnableEvents",
        InvokeKind::Put,
        vec![AutoValue::Bool(false)],
    )
    .unwrap();
    enter(&host, at(&host, 10, 1), "8");
    assert_eq!(events.changes.borrow().len(), 1);
    host.session.borrow_mut().set_workbook_event_observer(None);
}

/// Records the AppEvents raised, with the worksheet name a SheetCalculate carries.
struct Names {
    model: Weak<WorkbookAutomation>,
    raised: RefCell<Vec<String>>,
}
impl EventSource for Names {
    fn raise(
        &self,
        _: &ObjectRef,
        _: &str,
        event: &str,
        args: &[AutoValue],
    ) -> Result<(), AutoError> {
        let model = self.model.upgrade().unwrap();
        let name = match event {
            "SheetCalculate" => {
                assert_eq!(args.len(), 1);
                format!(
                    "SheetCalculate {}",
                    text(&model, &obj(args[0].clone()), "Name")
                )
            }
            _ => event.to_owned(),
        };
        self.raised.borrow_mut().push(name);
        Ok(())
    }
}
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn sheet_calculate_reaches_the_projection_before_after_calculate_and_obeys_the_switch() {
    // Excel-DNA's AppEvents sink records SheetCalculate before AfterCalculate for a calculation
    // that calculated a formula of the sheet (X3 events capture, 2026-11-14).
    let (host, model) = setup();
    let events = Rc::new(Names {
        model: Rc::downgrade(&model),
        raised: RefCell::default(),
    });
    host.session
        .borrow_mut()
        .set_workbook_event_observer(Some(model.observer(events.clone())));
    let calculate = || {
        host.session.borrow_mut().dispatch(SkinIntent::Recalculate);
        while host.session.borrow().has_pending_calculation() {
            host.session.borrow_mut().advance_calculation();
        }
    };
    calculate();
    enter(&host, at(&host, 30, 1), "2");
    enter(&host, at(&host, 30, 2), "=A30*2");
    events.raised.borrow_mut().clear();
    calculate();
    let sheet = host.session.borrow().host().sheets()[0]
        .name()
        .text()
        .to_string_lossy()
        .to_string();
    assert_eq!(
        &*events.raised.borrow(),
        &[
            format!("SheetCalculate {sheet}"),
            "AfterCalculate".to_owned()
        ]
    );
    invoke_kind(
        &model,
        &model.application(),
        "EnableEvents",
        InvokeKind::Put,
        vec![AutoValue::Bool(false)],
    )
    .unwrap();
    events.raised.borrow_mut().clear();
    enter(&host, at(&host, 30, 1), "3");
    calculate();
    assert!(events.raised.borrow().is_empty());
    host.session.borrow_mut().set_workbook_event_observer(None);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn window_parent_is_the_session_workbook() {
    // xlOil connects through EXCEL7's Window: Window.Parent as a _Workbook, then its Application
    // (xlOil 0.22.1 Connect.cpp). Excel's Window.Parent is the workbook.
    let (host, model) = setup();
    host.session.borrow_mut().set_file_identity("Book2", false);
    let window = calc_object_model::profile::values_only::object(
        "Window",
        calc_object_model::ObjectKey::Window,
    );
    let book = obj(invoke(&model, &window, "Parent", vec![]).unwrap());
    let active = obj(invoke(&model, &model.application(), "ActiveWorkbook", vec![]).unwrap());
    assert_eq!(book, active);
    assert_eq!(text(&model, &book, "Name"), "Book2");
    let app = obj(invoke(&model, &book, "Application", vec![]).unwrap());
    assert_eq!(app, model.application());
}

struct Probe {
    model: Weak<WorkbookAutomation>,
    refuse: bool,
}
impl std::fmt::Debug for Probe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AutomationProbe")
    }
}
impl UserFunction for Probe {
    fn call(&self, _: &[CalcValue]) -> Result<CalcValue, String> {
        panic!("invocation required")
    }
    fn call_with_context(
        &self,
        _: &[CalcValue],
        context: &mut dyn UserFunctionContext,
    ) -> Result<CalcValue, String> {
        let loan = Loan(RefCell::new(context));
        INVOCATION.set(&loan, || {
            let model = self.model.upgrade().unwrap();
            if self.refuse {
                let r = range(&model, "A3");
                let error = invoke(&model, &r, "Value", vec![]).unwrap_err();
                // The projection records the refused call even if its client swallows it.
                model.record_refusal(&error);
                assert!(matches!(
                    error,
                    AutoError::NotModelled {
                        owner: "dc-1nbp.6",
                        ..
                    }
                ));
                return Ok(CalcValue::number(99.)); // Error swallowed: the formula must still refuse.
            }
            // E6 conversion also uses the evaluator's admitted read route.
            let date = range(&model, "A3");
            assert!(matches!(invoke(&model, &date, "Value", vec![]).unwrap(), AutoValue::Date(n) if n.to_bits() == 45322f64.to_bits()));
            model.released(&date);
            let app = model.application();
            let caller = obj(invoke(&model, &app, "Caller", vec![]).unwrap());
            // Excel: ThisCell inside an add-in function's invocation is error 1004 (X19).
            assert!(matches!(
                invoke(&model, &app, "ThisCell", vec![]),
                Err(AutoError::Excel { number: 1004, .. })
            ));
            assert_eq!(text(&model, &caller, "Address"), "$E$20");
            let read = range(&model, "A10");
            assert!(matches!(
                invoke_kind(
                    &model,
                    &read,
                    "Value2",
                    InvokeKind::Put,
                    vec![AutoValue::R8(99.)]
                ),
                Err(AutoError::Excel { number: 1004, .. })
            ));
            let AutoValue::R8(n) = invoke(&model, &read, "Value2", vec![]).unwrap() else {
                panic!("number");
            };
            let functions = obj(invoke(&model, &app, "WorksheetFunction", vec![]).unwrap());
            let AutoValue::R8(sum) = invoke_kind(
                &model,
                &functions,
                "Sum",
                InvokeKind::Method,
                vec![AutoValue::Object(read.clone()), AutoValue::I4(2)],
            )
            .unwrap() else {
                panic!("sum");
            };
            assert_eq!(sum, n + 2.);
            for (target, method) in [
                (&app, "Calculate"),
                (&app, "CalculateFull"),
                (&app, "CalculateFullRebuild"),
                (&read, "Calculate"),
                (&read, "ClearContents"),
            ] {
                assert!(matches!(
                    invoke_kind(&model, target, method, InvokeKind::Method, vec![]),
                    Err(AutoError::Excel { number: 1004, .. })
                ));
            }
            model.released(&caller);
            Ok(CalcValue::number(n))
        })
    }
}
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn invocation_uses_real_evaluator_context_and_swallowed_refusal_keeps_owning_bead() {
    for refuse in [false, true] {
        let (host, model) = setup();
        if refuse {
            let date_cell = at(&host, 3, 1);
            // Date conversion is now measured; keep the swallowed-refusal control on a
            // genuinely undecidable conditional format rather than the old date refusal.
            assert!(
                host.session
                    .borrow_mut()
                    .dispatch(SkinIntent::Command(WorkbookCommand::FormatCells {
                        areas: vec![calc_value::GridArea::cell(date_cell)],
                        patch: FormatPatch {
                            number_format: Some("[>=1]yyyy-mm-dd;0".into()),
                            ..Default::default()
                        },
                    }))
                    .accepted()
            );
        }
        let data = at(&host, 10, 1);
        enter(&host, data, "7");
        let caller = at(&host, 20, 5);
        let mut functions = CallableTable::new();
        functions
            .register(
                "Probe",
                Rc::new(Probe {
                    model: Rc::downgrade(&model),
                    refuse,
                }),
            )
            .unwrap();
        host.session.borrow_mut().set_user_functions(functions);
        enter(&host, caller, "=IFERROR(Probe(),123)");
        model.context_changed(&host.session.borrow());
        host.commands.set(0);
        host.session.borrow_mut().dispatch(SkinIntent::Recalculate);
        while host.session.borrow().has_pending_calculation() {
            host.session.borrow_mut().advance_calculation();
        }
        assert_eq!(
            host.commands.get(),
            0,
            "an invocation must not borrow the command session"
        );
        let session = host.session.borrow();
        let page = project(
            session.host(),
            &ProjectionRequest {
                cells: vec![caller],
            },
        )
        .unwrap();
        if refuse {
            assert!(
                matches!(page.cells()[0].value(), ProjectedValue::NotModeled { .. }),
                "{:?}",
                page.cells()[0].value()
            );
            assert!(format!("{:?}", page.cells()[0].value()).contains("dc-1nbp.6"));
        } else {
            assert!(
                matches!(page.cells()[0].value(), ProjectedValue::Calculated { value, .. } if value.as_number() == Some(7.))
            );
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn profiles_leave_refusal_recording_to_the_projection_once_per_call() {
    struct Refusals(Cell<u32>);
    impl Invocation for Refusals {
        fn call(&self, _: &mut dyn FnMut(&mut dyn UserFunctionContext) -> Reply) -> Reply {
            self.0.set(self.0.get() + 1);
            Ok(AutoValue::Empty)
        }
    }
    let (_host, model) = setup();
    let refusals = Refusals(Cell::new(0));
    INVOCATION.set(&refusals, || {
        let app = model.application();
        let member = Descriptor::get()
            .member(app.interface, "Iteration", InvokeKind::Get)
            .unwrap();
        let error = model
            .invoke(
                &app,
                &Call {
                    interface: app.interface,
                    member,
                    args: vec![],
                    lcid: 0,
                },
            )
            .unwrap_err();
        assert_eq!(
            refusals.0.get(),
            0,
            "profiles return the error without recording it"
        );
        model.record_refusal(&error);
        assert_eq!(
            refusals.0.get(),
            1,
            "the projection records the refused call exactly once"
        );
    });
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn busy_command_is_typed_and_outside_invocation_caller_is_ref_error() {
    let (host, model) = setup();
    assert!(matches!(
        invoke(&model, &model.application(), "Caller", vec![]).unwrap(),
        AutoValue::Error(2023)
    ));
    let _busy = host.session.borrow_mut();
    assert!(matches!(
        invoke(
            &model,
            &model.application(),
            "Range",
            vec![AutoValue::text("A1")]
        ),
        Err(AutoError::NotModelled {
            owner: "dc-0f1k.24",
            ..
        })
    ));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn value_formats_measured_dates_time_and_numeric_literals() {
    let (_host, model) = setup();
    for (address, date) in [("A3", true), ("A8", false), ("A11", false), ("A12", false)] {
        let range = range(&model, address);
        let AutoValue::R8(raw) = invoke(&model, &range, "Value2", vec![]).unwrap() else {
            panic!("Value2 numeric cell");
        };
        let value = invoke(&model, &range, "Value", vec![]).unwrap();
        match value {
            AutoValue::Date(n) if date => assert_eq!(n.to_bits(), raw.to_bits()),
            AutoValue::R8(n) if !date => assert_eq!(n.to_bits(), raw.to_bits()),
            other => panic!("unexpected Value type for {address}: {other:?}"),
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn released_ranges_balance_tracker_retains_without_invalidating_other_wrappers() {
    let (_host, model) = setup();
    let one = range(&model, "Z100");
    let two = range(&model, "Z100");
    assert_eq!(one, two, "the tracker interns the coordinate identity");
    model.released(&one);
    assert_eq!(text(&model, &two, "Address"), "$Z$100");
    model.released(&two);
    assert!(matches!(
        invoke(&model, &two, "Address", vec![]),
        Err(AutoError::Excel { number: 424, .. })
    ));
    assert_ne!(range(&model, "Z100"), two, "released slots never revive");
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn active_sheet_follows_selection_and_union_rejects_another_sheet() {
    let (host, model) = setup();
    let first = range(&model, "A1");
    assert!(
        host.session
            .borrow_mut()
            .dispatch(SkinIntent::Command(WorkbookCommand::Sheet {
                sheet: None,
                change: SheetChange::Add {
                    name: "Other".into()
                }
            }))
            .accepted()
    );
    let sheet = host
        .session
        .borrow()
        .host()
        .sheets()
        .iter()
        .find(|s| s.name().text().to_string_lossy() == "Other")
        .unwrap()
        .id();
    let active = CellKey::new(sheet, 3, 4).unwrap();
    assert!(
        host.session
            .borrow_mut()
            .dispatch(SkinIntent::Select(Selection {
                active: Some(active),
                areas: vec![calc_value::GridArea::cell(active)]
            }))
            .accepted()
    );
    let app = model.application();
    let selected = obj(invoke(&model, &app, "ActiveSheet", vec![]).unwrap());
    assert_eq!(text(&model, &selected, "Name"), "Other");
    let second = range(&model, "D3");
    assert!(matches!(
        invoke(&model, &second, "Row", vec![]).unwrap(),
        AutoValue::I4(3)
    ));
    assert!(matches!(
        invoke(&model, &second, "Column", vec![]).unwrap(),
        AutoValue::I4(4)
    ));
    assert!(matches!(
        invoke_kind(
            &model,
            &app,
            "Union",
            InvokeKind::Method,
            vec![AutoValue::Object(first), AutoValue::Object(second)]
        ),
        Err(AutoError::Excel { number: 1004, .. })
    ));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn measured_relative_to_and_optional_defaults_go_through_the_same_member_handler() {
    let (_host, model) = setup();
    let range = range(&model, "B2:C3");
    let base = obj(invoke(
        &model,
        &model.application(),
        "Range",
        vec![AutoValue::text("C3:D4")],
    )
    .unwrap());
    assert_eq!(
        invoke(
            &model,
            &range,
            "Address",
            vec![
                AutoValue::Bool(false),
                AutoValue::Bool(false),
                AutoValue::I4(-4150),
                AutoValue::Missing,
                AutoValue::Object(base.clone())
            ]
        )
        .unwrap()
        .as_text()
        .as_deref(),
        Some("R[-1]C[-1]:RC")
    );
    assert_eq!(
        invoke(
            &model,
            &range,
            "Address",
            vec![
                AutoValue::Bool(false),
                AutoValue::Bool(false),
                AutoValue::I4(1),
                AutoValue::Missing,
                AutoValue::Object(base)
            ]
        )
        .unwrap()
        .as_text()
        .as_deref(),
        Some("B2:C3")
    );
    for default in [AutoValue::Missing, AutoValue::Null, AutoValue::Error(2042)] {
        assert_eq!(
            invoke(&model, &range, "Address", vec![default])
                .unwrap()
                .as_text()
                .as_deref(),
            Some("$B$2:$C$3")
        );
    }
    assert_eq!(
        invoke(&model, &range, "Address", vec![AutoValue::Empty])
            .unwrap()
            .as_text()
            .as_deref(),
        Some("$B2:$C3")
    );
    for second in [
        AutoValue::Empty,
        AutoValue::Null,
        AutoValue::Error(2042),
        AutoValue::Bool(false),
    ] {
        assert!(matches!(
            invoke(
                &model,
                &model.application(),
                "Range",
                vec![AutoValue::text("A1"), second]
            ),
            Err(AutoError::Excel { number: 1004, .. })
        ));
    }
    assert!(matches!(
        invoke(&model, &range, "Value", vec![AutoValue::Null]),
        Err(AutoError::Status(status)) if status == 0x80020005u32 as i32
    ));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn worksheet_and_range_navigation_use_the_selected_object_and_tracked_geometry() {
    let (host, model) = setup();
    let app = model.application();
    let book = obj(invoke(&model, &app, "ActiveWorkbook", vec![]).unwrap());
    let original = obj(invoke(&model, &app, "ActiveSheet", vec![]).unwrap());
    assert!(
        host.session
            .borrow_mut()
            .dispatch(SkinIntent::Command(WorkbookCommand::Sheet {
                sheet: None,
                change: SheetChange::Add {
                    name: "Other".into()
                },
            }))
            .accepted()
    );
    let mut held = None;
    for name in ["Sheets", "Worksheets"] {
        let sheets = obj(invoke(&model, &book, name, vec![]).unwrap());
        assert!(matches!(
            invoke(&model, &sheets, "Count", vec![]).unwrap(),
            AutoValue::I4(2)
        ));
        let by_index = obj(invoke(&model, &sheets, "Item", vec![AutoValue::I4(2)]).unwrap());
        let by_name =
            obj(invoke(&model, &sheets, "_Default", vec![AutoValue::text("oThEr")]).unwrap());
        assert_eq!(by_index, by_name);
        assert!(matches!(
            invoke(&model, &by_name, "Index", vec![]).unwrap(),
            AutoValue::I4(2)
        ));
        held = Some(by_name);
    }
    let other = held.unwrap();
    let other_range = obj(invoke(&model, &other, "Range", vec![AutoValue::text("B2:C3")]).unwrap());
    assert_eq!(
        obj(invoke(&model, &other_range, "Worksheet", vec![]).unwrap()),
        other
    );
    let original_range =
        obj(invoke(&model, &original, "Range", vec![AutoValue::text("B2:C3")]).unwrap());
    assert_eq!(
        obj(invoke(&model, &original_range, "Worksheet", vec![]).unwrap()),
        original
    );
    let cells = obj(invoke(&model, &other, "Cells", vec![]).unwrap());
    let last = obj(invoke(
        &model,
        &cells,
        "Item",
        vec![AutoValue::I4(1_048_576), AutoValue::I4(16_384)],
    )
    .unwrap());
    assert_eq!(text(&model, &last, "Address"), "$XFD$1048576");
    for (member, args, expected) in [
        ("Item", vec![AutoValue::I4(3)], "$B$3"),
        (
            "_Default",
            vec![AutoValue::I4(2), AutoValue::text("B")],
            "$C$3",
        ),
        (
            "Offset",
            vec![AutoValue::I4(-1), AutoValue::I4(2)],
            "$D$1:$E$2",
        ),
        (
            "Resize",
            vec![AutoValue::I4(3), AutoValue::I4(1)],
            "$B$2:$B$4",
        ),
        ("EntireRow", vec![], "$2:$3"),
        ("EntireColumn", vec![], "$B:$C"),
    ] {
        let derived = obj(invoke(&model, &other_range, member, args).unwrap());
        assert_eq!(text(&model, &derived, "Address"), expected, "{member}");
        model.released(&derived);
    }
    let rows = obj(invoke(&model, &other_range, "Rows", vec![]).unwrap());
    let row = obj(invoke(&model, &rows, "Item", vec![AutoValue::I4(2)]).unwrap());
    assert_eq!(text(&model, &row, "Address"), "$B$3:$C$3");
    let cells = obj(invoke(&model, &rows, "Cells", vec![]).unwrap());
    let cell = obj(invoke(&model, &cells, "Item", vec![AutoValue::I4(2)]).unwrap());
    assert_eq!(text(&model, &cell, "Address"), "$C$2");
    let second = obj(invoke(&model, &other, "Range", vec![AutoValue::text("F5")]).unwrap());
    let union = obj(invoke_kind(
        &model,
        &app,
        "Union",
        InvokeKind::Method,
        vec![
            AutoValue::Object(other_range.clone()),
            AutoValue::Object(second),
        ],
    )
    .unwrap());
    let areas = obj(invoke(&model, &union, "Areas", vec![]).unwrap());
    model.released(&union);
    assert!(matches!(
        invoke(&model, &areas, "Count", vec![]).unwrap(),
        AutoValue::I4(2)
    ));
    let second = obj(invoke(&model, &areas, "Item", vec![AutoValue::I4(2)]).unwrap());
    assert_eq!(text(&model, &second, "Address"), "$F$5");
    assert!(invoke(&model, &areas, "Item", vec![AutoValue::I4(0)]).is_err());
    assert!(invoke(&model, &last, "Offset", vec![AutoValue::I4(1)]).is_err());
    model.released(&areas);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn workbook_names_keep_their_identity_and_resolve_through_the_workbook_binder() {
    let (host, model) = setup();
    let sheet = host.session.borrow().host().sheets()[0]
        .name()
        .text()
        .to_string_lossy();
    assert!(
        host.session
            .borrow_mut()
            .dispatch(SkinIntent::Command(WorkbookCommand::Name(NameEdit {
                id: None,
                version: None,
                scope: calc_doc_model::NameScope::Workbook,
                name: "InputBlock".into(),
                formula: Some(format!("='{sheet}'!$A$10:$B$11")),
            })))
            .accepted()
    );
    let book = obj(invoke(&model, &model.application(), "ActiveWorkbook", vec![]).unwrap());
    let names = obj(invoke(&model, &book, "Names", vec![]).unwrap());
    assert!(matches!(
        invoke(&model, &names, "Count", vec![]).unwrap(),
        AutoValue::I4(1)
    ));
    let name = obj(invoke_kind(
        &model,
        &names,
        "Item",
        InvokeKind::Method,
        vec![AutoValue::text("inputblock")],
    )
    .unwrap());
    assert_eq!(
        name,
        obj(invoke_kind(
            &model,
            &names,
            "_Default",
            InvokeKind::Method,
            vec![AutoValue::I4(1)]
        )
        .unwrap())
    );
    assert_eq!(text(&model, &name, "Name"), "InputBlock");
    assert_eq!(
        text(&model, &name, "RefersTo"),
        format!("='{sheet}'!$A$10:$B$11")
    );
    let referred = obj(invoke(&model, &name, "RefersToRange", vec![]).unwrap());
    assert_eq!(text(&model, &referred, "Address"), "$A$10:$B$11");
    let node = host
        .session
        .borrow()
        .host()
        .model()
        .definitions()
        .name_table()
        .nodes()
        .next()
        .unwrap()
        .clone();
    let receipt = host
        .session
        .borrow_mut()
        .dispatch(SkinIntent::Command(WorkbookCommand::Name(NameEdit {
            id: Some(node.id()),
            version: Some(node.binding_version()),
            scope: calc_doc_model::NameScope::Workbook,
            name: "RenamedBlock".into(),
            formula: Some(format!("={}", node.definition().formula)),
        })));
    assert!(receipt.accepted(), "{receipt:?}");
    assert_eq!(text(&model, &name, "Name"), "RenamedBlock");
    let node = host
        .session
        .borrow()
        .host()
        .model()
        .definitions()
        .name_table()
        .get(node.id())
        .unwrap()
        .clone();
    assert!(
        host.session
            .borrow_mut()
            .dispatch(SkinIntent::Command(WorkbookCommand::Name(NameEdit {
                id: Some(node.id()),
                version: Some(node.binding_version()),
                scope: calc_doc_model::NameScope::Workbook,
                name: "RenamedBlock".into(),
                formula: Some(format!("='{sheet}'!$D$20")),
            })))
            .accepted()
    );
    assert_eq!(
        text(
            &model,
            &obj(invoke(&model, &name, "RefersToRange", vec![]).unwrap()),
            "Address"
        ),
        "$D$20"
    );
    let version = host
        .session
        .borrow()
        .host()
        .model()
        .definitions()
        .name_table()
        .get(node.id())
        .unwrap()
        .binding_version();
    assert!(
        host.session
            .borrow_mut()
            .dispatch(SkinIntent::Command(WorkbookCommand::Name(NameEdit {
                id: Some(node.id()),
                version: Some(version),
                scope: calc_doc_model::NameScope::Workbook,
                name: "RenamedBlock".into(),
                formula: None,
            })))
            .accepted()
    );
    assert!(invoke(&model, &name, "Name", vec![]).is_err());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn command_writes_use_one_automation_transaction_and_calculate_real_formulas() {
    let (host, model) = setup();
    let app = model.application();
    let block = range(&model, "A20:B21");
    let array = || {
        AutoValue::Array(Box::new(calc_object_model::AutoArray {
            rows: 2,
            columns: 2,
            lower: (1, 1),
            values: vec![
                AutoValue::R8(7.),
                AutoValue::text("ordinary text"),
                AutoValue::Bool(true),
                AutoValue::Error(2042),
            ],
        }))
    };
    let log = host.session.borrow().intent_log().len();
    invoke_kind(&model, &block, "Value2", InvokeKind::Put, vec![array()]).unwrap();
    {
        let session = host.session.borrow();
        assert_eq!(session.intent_log().len(), log + 1);
        assert_eq!(
            session.intent_log().last().unwrap().ingress,
            CommandIngress::Automation
        );
    }
    let AutoValue::Array(read) = invoke(&model, &block, "Value2", vec![]).unwrap() else {
        panic!("array");
    };
    assert!(matches!(read.values[0], AutoValue::R8(7.)));
    assert_eq!(read.values[1].as_text().as_deref(), Some("ordinary text"));
    assert!(matches!(read.values[2], AutoValue::Bool(true)));
    assert!(matches!(read.values[3], AutoValue::Error(2042)));
    let log = host.session.borrow().intent_log().len();
    let AutoValue::Array(mut wrong) = array() else {
        unreachable!()
    };
    wrong.columns = 1;
    assert!(matches!(
        invoke_kind(
            &model,
            &block,
            "Value2",
            InvokeKind::Put,
            vec![AutoValue::Array(wrong)]
        ),
        Err(AutoError::NotModelled {
            owner: "dc-1nbp.6",
            ..
        })
    ));
    assert_eq!(
        host.session.borrow().intent_log().len(),
        log,
        "shape refusals are atomic"
    );
    for text in ["=1+1", "123", "TRUE", "'literal"] {
        let AutoValue::Array(mut ambiguous) = array() else {
            unreachable!()
        };
        ambiguous.values[3] = AutoValue::text(text);
        assert!(matches!(
            invoke_kind(
                &model,
                &block,
                "Value2",
                InvokeKind::Put,
                vec![AutoValue::Array(ambiguous)]
            ),
            Err(AutoError::NotModelled {
                owner: "dc-1nbp.6",
                ..
            })
        ));
        assert_eq!(
            host.session.borrow().intent_log().len(),
            log,
            "ambiguous text refuses before any cell is written"
        );
    }
    invoke_kind(
        &model,
        &block,
        "Value",
        InvokeKind::Put,
        vec![AutoValue::Missing, AutoValue::I4(3)],
    )
    .unwrap();
    let formula = at(&host, 20, 5);
    enter(&host, formula, "=SUM(A20:B21)");
    let sheet = obj(invoke(&model, &block, "Worksheet", vec![]).unwrap());
    let output = range(&model, "E20");
    for target in [&app, &sheet, &output] {
        invoke_kind(&model, target, "Calculate", InvokeKind::Method, vec![]).unwrap();
        assert!(matches!(
            invoke(&model, &output, "Value2", vec![]).unwrap(),
            AutoValue::R8(12.)
        ));
    }
    for method in ["CalculateFull", "CalculateFullRebuild"] {
        invoke_kind(&model, &app, method, InvokeKind::Method, vec![]).unwrap();
    }
    let functions = obj(invoke(&model, &app, "WorksheetFunction", vec![]).unwrap());
    assert!(matches!(
        invoke_kind(
            &model,
            &functions,
            "Sum",
            InvokeKind::Method,
            vec![AutoValue::Object(block.clone())]
        )
        .unwrap(),
        AutoValue::R8(12.)
    ));
    invoke_kind(&model, &block, "ClearContents", InvokeKind::Method, vec![]).unwrap();
    let AutoValue::Array(read) = invoke(&model, &block, "Value2", vec![]).unwrap() else {
        panic!("array");
    };
    assert!(read.values.iter().all(|v| matches!(v, AutoValue::Empty)));
}

struct WritingEvents {
    model: Weak<WorkbookAutomation>,
    changes: RefCell<Vec<String>>,
}
impl EventSource for WritingEvents {
    fn raise(
        &self,
        _: &ObjectRef,
        _: &str,
        event: &str,
        args: &[AutoValue],
    ) -> Result<(), AutoError> {
        // The calculation events are counted by other tests; this one watches the writes.
        if event == "AfterCalculate" || event == "SheetCalculate" {
            return Ok(());
        }
        assert_eq!(event, "SheetChange");
        let model = self.model.upgrade().unwrap();
        let target = obj(args[1].clone());
        let address = text(&model, &target, "Address");
        self.changes.borrow_mut().push(address.clone());
        let functions = obj(invoke(
            &model,
            &model.application(),
            "WorksheetFunction",
            vec![],
        )?);
        let AutoValue::R8(value) = invoke_kind(
            &model,
            &functions,
            "Sum",
            InvokeKind::Method,
            vec![AutoValue::Object(target.clone())],
        )?
        else {
            panic!("sum");
        };
        if address == "$A$30" {
            let destination = range(&model, "B30");
            invoke_kind(
                &model,
                &destination,
                "Value2",
                InvokeKind::Put,
                vec![AutoValue::R8(value + 1.)],
            )?;
            model.released(&destination);
        }
        model.released(&target);
        Ok(())
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn event_writes_reborrow_the_same_session_and_deliver_each_change_once() {
    let (host, model) = setup();
    host.session
        .borrow_mut()
        .dispatch(SkinIntent::SetCalcMode(CalcMode::Automatic));
    enter(&host, at(&host, 30, 5), "=B30*2");
    while host.session.borrow().has_pending_calculation() {
        host.session.borrow_mut().advance_calculation();
    }
    let events = Rc::new(WritingEvents {
        model: Rc::downgrade(&model),
        changes: RefCell::default(),
    });
    host.session
        .borrow_mut()
        .set_workbook_event_observer(Some(model.observer(events.clone())));
    let source = range(&model, "A30");
    let log = host.session.borrow().intent_log().len();
    invoke_kind(
        &model,
        &source,
        "Value2",
        InvokeKind::Put,
        vec![AutoValue::R8(7.)],
    )
    .unwrap();
    assert_eq!(&*events.changes.borrow(), &["$A$30", "$B$30"]);
    assert!(matches!(
        invoke(&model, &range(&model, "E30"), "Value2", vec![]).unwrap(),
        AutoValue::R8(16.)
    ));
    let session = host.session.borrow();
    assert!(
        session.workbook_event_failures().failures.is_empty(),
        "{:?}",
        session.workbook_event_failures()
    );
    let records = &session.intent_log()[log..];
    assert_eq!(records.len(), 2);
    assert!(
        records
            .iter()
            .all(|record| record.ingress == CommandIngress::Automation)
    );
    let root = records
        .iter()
        .find(|record| record.parent.is_none())
        .unwrap();
    assert_eq!(
        records
            .iter()
            .filter(|record| record.parent == Some(root.seq))
            .count(),
        1
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn workbook_descriptor_coverage_lists_own_and_inherited_members() {
    let (_, model) = setup();
    let report = model.coverage();
    for name in model.handlers().implemented() {
        assert!(report.contains(&format!("workbook: MEMBER {name}")));
    }
    println!("{report}");
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn names_index_order_is_alphabetical_and_local_names_cannot_silently_shadow_a_held_global() {
    let (host, model) = setup();
    let sheet = at(&host, 1, 1).sheet();
    for name in ["Zed", "Alpha"] {
        assert!(
            host.session
                .borrow_mut()
                .dispatch(SkinIntent::Command(WorkbookCommand::Name(NameEdit {
                    id: None,
                    version: None,
                    scope: calc_doc_model::NameScope::Workbook,
                    name: name.into(),
                    formula: Some("=$A$1".into()),
                })))
                .accepted()
        );
    }
    let book = obj(invoke(&model, &model.application(), "ActiveWorkbook", vec![]).unwrap());
    let names = obj(invoke(&model, &book, "Names", vec![]).unwrap());
    let global = obj(invoke_kind(
        &model,
        &names,
        "Item",
        InvokeKind::Method,
        vec![AutoValue::I4(1)],
    )
    .unwrap());
    assert_eq!(text(&model, &global, "Name"), "Alpha");
    assert!(
        host.session
            .borrow_mut()
            .dispatch(SkinIntent::Command(WorkbookCommand::Name(NameEdit {
                id: None,
                version: None,
                scope: calc_doc_model::NameScope::Sheet(sheet),
                name: "Alpha".into(),
                formula: Some("=$B$2".into()),
            })))
            .accepted()
    );
    assert!(matches!(
        invoke(&model, &global, "RefersToRange", vec![]),
        Err(AutoError::NotModelled {
            owner: "dc-hsa5.6",
            ..
        })
    ));
    assert!(matches!(
        invoke(&model, &names, "Count", vec![]),
        Err(AutoError::NotModelled {
            owner: "dc-hsa5.6",
            ..
        })
    ));
}

// The E6 inputs/answers are retained in calc-xll-reference's three independent Excel runs.
// Author their formats through the session's existing style transaction, then save/reopen:
// these are workbook-format tests, with no alternate value reader or fixture generator.
#[path = "../../calc-doc/tests/cdoc_m1_1_support/mod.rs"]
mod existing_wire_support;
fn e6_workbook() -> (
    Rc<Host>,
    Rc<WorkbookAutomation>,
    Vec<(f64, Result<AutoValue, AutoError>)>,
) {
    use calc_doc_model::{CellFormat, NumberFormat};
    let options = HostOptions {
        calc_mode: Some(CalcMode::Manual),
        locale: Some(calc_func::locale::format_profile(
            calc_func::locale::LocaleId::EnZa,
        )),
        ..Default::default()
    };
    let currency_code = "_($* #,##0.00_);_($* (#,##0.00);_($* \"-\"??_);_(@_)";
    let accounting = r#"_(\$* #\,##0.00_);_(\$* (#\,##0.00);_(\$* "-"??_);_(@_)"#;
    // Named style creation is not authored by Format Cells. Seed the imported Currency
    // lineage using the same independent package builder as the host's round-trip tests.
    let blank = blank_package();
    let archive = existing_wire_support::parse_zip(&blank);
    let entries = archive.entries.iter().map(|entry| {
        let payload = entry.payload(&blank);
        if entry.name == "xl/styles.xml" {
            let bytes = if entry.method == 0 { payload.to_vec() } else { miniz_oxide::inflate::decompress_to_vec(payload).unwrap() };
            let xml = String::from_utf8(bytes).unwrap()
                .replace("<fonts", &format!("<numFmts count=\"1\"><numFmt numFmtId=\"220\" formatCode=\"{}\"/></numFmts><fonts", currency_code.replace('"', "&quot;")))
                .replace("<cellStyleXfs count=\"1\">", "<cellStyleXfs count=\"2\">")
                .replace("</cellStyleXfs>", "<xf numFmtId=\"220\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/></cellStyleXfs>")
                .replace("<cellStyles count=\"1\">", "<cellStyles count=\"2\">")
                .replace("</cellStyles>", "<cellStyle name=\"Currency\" xfId=\"1\" builtinId=\"4\"/></cellStyles>");
            existing_wire_support::RawEntry::stored(&entry.name, xml.as_bytes())
        } else if entry.method == 0 { existing_wire_support::RawEntry::stored(&entry.name, payload) }
        else { existing_wire_support::RawEntry::deflated(&entry.name, payload, entry.size, entry.crc32) }
    }).collect::<Vec<_>>();
    let package = existing_wire_support::build_zip(&entries);
    let (host, model) = setup_session(
        HostSession::open(
            Arc::new(MemorySource::new(package)),
            &OpenPolicy::default(),
            options.clone(),
        )
        .unwrap(),
    );
    let overflow = || Err(AutoError::excel(0x8002_000Au32 as i32, ""));
    let cases = vec![
        (
            45292.25,
            "yyyy-mm-dd hh:mm",
            false,
            Ok(AutoValue::Date(45292.25)),
        ),
        (
            123.45678901234567,
            r"\$#\,##0.00",
            false,
            Ok(AutoValue::R8(123.45678901234567)),
        ),
        (
            123.45678901234567,
            accounting,
            false,
            Ok(AutoValue::R8(123.45678901234567)),
        ),
        (45292.25, "hh:mm:ss", false, Ok(AutoValue::R8(45292.25))),
        (
            123.45678901234567,
            currency_code,
            true,
            Ok(AutoValue::Currency(1234568)),
        ),
        (
            123.45678901234567,
            r"[$$-409]#\,##0.00",
            false,
            Ok(AutoValue::R8(123.45678901234567)),
        ),
        (
            123.45678901234567,
            r"[$R-1C09] #\,##0.00",
            false,
            Ok(AutoValue::R8(123.45678901234567)),
        ),
        (-0.00005, currency_code, true, Ok(AutoValue::Currency(-1))),
        (922337203685477.5807, currency_code, true, overflow()),
        (1e300, currency_code, true, overflow()),
        (-1., "yyyy-mm-dd", false, Ok(AutoValue::Date(-1.))),
        (3_000_000., "yyyy-mm-dd", false, overflow()),
    ];
    let sheet = at(&host, 1, 1).sheet();
    let mut styles = (**host.session.borrow().host().styles()).clone();
    let currency_xf = styles
        .cell_styles
        .iter()
        .find(|s| s.builtin_id == Some(4))
        .unwrap()
        .xf_id;
    let currency_format = 220;
    let mut edits = Vec::new();
    for (index, (number, code, use_currency, _)) in cases.iter().enumerate() {
        let id = if *use_currency {
            currency_format
        } else {
            let id = 221 + index as u32;
            styles.number_formats.push(NumberFormat {
                id,
                code: (*code).into(),
            });
            id
        };
        let style = styles.cell_formats.len() as u32;
        styles.cell_formats.push(CellFormat {
            number_format_id: Some(id),
            xf_id: Some(if *use_currency { currency_xf } else { 0 }),
            ..Default::default()
        });
        let at = CellKey::new(sheet, index as u32 + 1, 1).unwrap();
        edits.push(CellEdit::Set {
            at,
            input: CellInput::Number(calc_doc_model::FiniteNumber::new(*number).unwrap()),
        });
        edits.push(CellEdit::SetFormat {
            at,
            format: Some(style),
        });
    }
    edits.insert(0, CellEdit::Styles(Arc::new(styles)));
    assert!(
        host.session
            .borrow_mut()
            .dispatch(SkinIntent::Edit { edits })
            .accepted()
    );
    let mut saved = Vec::new();
    host.session
        .borrow_mut()
        .save(&SavePolicy::default(), &mut saved)
        .unwrap();
    host.session.replace(
        HostSession::open(
            Arc::new(MemorySource::new(saved)),
            &OpenPolicy::default(),
            options,
        )
        .unwrap(),
    );
    model.context_changed(&host.session.borrow());
    (
        host,
        model,
        cases
            .into_iter()
            .map(|(number, _, _, expected)| (number, expected))
            .collect(),
    )
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn range_value_e6_workbook_formats_preserve_excel_types_rounding_and_overflow() {
    let (_, model, cases) = e6_workbook();
    let trace = std::str::from_utf8(include_bytes!(
        "../../calc-xll-reference/traces/2026-11-14-om-probes/trial-1/om-e6.tsv"
    ))
    .unwrap();
    assert!(trace.contains("VT_CY:int64=-1 (-0.0001)"));
    assert!(trace.contains("EXCEPINFO wCode=0 scode=8002000A"));
    for (index, (number, expected)) in cases.iter().enumerate() {
        let cell = range(&model, &format!("A{}", index + 1));
        assert_e6(invoke(&model, &cell, "Value", vec![]), expected);
        let AutoValue::R8(raw) = invoke(&model, &cell, "Value2", vec![]).unwrap() else {
            panic!("Value2 must remain R8");
        };
        assert_eq!(raw.to_bits(), number.to_bits());
    }
    let AutoValue::Array(array) = invoke(&model, &range(&model, "A1:A8"), "Value", vec![]).unwrap()
    else {
        panic!("block");
    };
    assert_eq!((array.rows, array.columns, array.lower), (8, 1, (1, 1)));
    assert_eq!(array.values.len(), 8);
    for (actual, (_, expected)) in array.values.into_iter().zip(&cases[..8]) {
        assert_e6(Ok(actual), expected);
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn range_value_uses_current_session_formats_for_held_ranges_and_keeps_unknown_classes_typed() {
    let (host, model, _) = e6_workbook();
    let held = range(&model, "A1");
    let sheet = at(&host, 1, 1).sheet();
    assert!(
        host.session
            .borrow_mut()
            .dispatch(SkinIntent::EditAxis(
                AxisEdit::new(sheet, Axis::Rows, AxisChange::Insert, 1, 1).unwrap(),
            ))
            .accepted()
    );
    assert_eq!(text(&model, &held, "Address"), "$A$2");
    assert_e6(
        invoke(&model, &held, "Value", vec![]),
        &Ok(AutoValue::Date(45292.25)),
    );
    let target = CellKey::new(sheet, 2, 1).unwrap();
    for (code, expected) in [
        ("0.00", Some(AutoValue::R8(45292.25))),
        ("hh:mm:ss", Some(AutoValue::R8(45292.25))),
        ("yyyy\\-mm\\-dd", Some(AutoValue::Date(45292.25))),
        ("[>=1]yyyy-mm-dd;0", None),
    ] {
        assert!(
            host.session
                .borrow_mut()
                .dispatch(SkinIntent::Command(WorkbookCommand::FormatCells {
                    areas: vec![calc_value::GridArea::cell(target)],
                    patch: FormatPatch {
                        number_format: Some(code.into()),
                        ..Default::default()
                    },
                }))
                .accepted()
        );
        let actual = invoke(&model, &held, "Value", vec![]);
        match expected {
            Some(value) => assert_e6(actual, &Ok(value)),
            None => assert!(matches!(
                actual,
                Err(AutoError::NotModelled {
                    owner: "dc-1nbp.6",
                    ..
                })
            )),
        }
        assert_e6(
            invoke(&model, &held, "Value2", vec![]),
            &Ok(AutoValue::R8(45292.25)),
        );
    }
    // A number format override on a Currency-derived XF must use the current format.
    let currency = range(&model, "A6");
    assert!(
        host.session
            .borrow_mut()
            .dispatch(SkinIntent::Command(WorkbookCommand::FormatCells {
                areas: vec![calc_value::GridArea::cell(
                    CellKey::new(sheet, 6, 1).unwrap()
                )],
                patch: FormatPatch {
                    number_format: Some("yyyy-mm-dd".into()),
                    ..Default::default()
                },
            }))
            .accepted()
    );
    assert_e6(
        invoke(&model, &currency, "Value", vec![]),
        &Ok(AutoValue::Date(123.45678901234567)),
    );
    let mut saved = Vec::new();
    host.session
        .borrow_mut()
        .save(&SavePolicy::default(), &mut saved)
        .unwrap();
    host.session.replace(
        HostSession::open(
            Arc::new(MemorySource::new(saved)),
            &OpenPolicy::default(),
            HostOptions {
                calc_mode: Some(CalcMode::Manual),
                ..Default::default()
            },
        )
        .unwrap(),
    );
    model.context_changed(&host.session.borrow());
    let unknown_currency = range(&model, "A9");
    assert!(matches!(
        invoke(&model, &unknown_currency, "Value", vec![]),
        Err(AutoError::NotModelled {
            owner: "dc-1nbp.6",
            ..
        })
    ));
    assert_e6(
        invoke(&model, &unknown_currency, "Value2", vec![]),
        &Ok(AutoValue::R8(-0.00005)),
    );
    // Incomplete imported style lineage is not evidence for an accounting/R8 answer.
    let mut styles = (**host.session.borrow().host().styles()).clone();
    styles
        .cell_styles
        .retain(|style| style.builtin_id != Some(4));
    assert!(
        host.session
            .borrow_mut()
            .dispatch(SkinIntent::Edit {
                edits: vec![CellEdit::Styles(Arc::new(styles))]
            })
            .accepted()
    );
    assert!(matches!(
        invoke(&model, &unknown_currency, "Value", vec![]),
        Err(AutoError::NotModelled {
            owner: "dc-1nbp.6",
            ..
        })
    ));
}

fn assert_e6(actual: Reply, expected: &Reply) {
    match (&actual, expected) {
        (Ok(AutoValue::R8(a)), Ok(AutoValue::R8(b)))
        | (Ok(AutoValue::Date(a)), Ok(AutoValue::Date(b))) => assert_eq!(a.to_bits(), b.to_bits()),
        (Ok(AutoValue::Currency(a)), Ok(AutoValue::Currency(b))) => assert_eq!(a, b),
        (Err(a), Err(b)) => assert_eq!(a, b),
        _ => panic!("Automation type/value mismatch: {actual:?}, expected {expected:?}"),
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn range_value_currency_preserves_integer_units_for_large_in_range_numbers() {
    let (_, model, _) = e6_workbook();
    let cell = range(&model, "A5");
    // CY's exact integer-unit contract: binary64 multiplication would lose 112/216
    // units on these representable inputs. These are precision controls, not E6 captures.
    for (number, units) in [
        (123456789012345., 1_234_567_890_123_450_000i64),
        (-123456789012345., -1_234_567_890_123_450_000),
        (922337203685477.5, 9_223_372_036_854_775_000),
        (-922337203685477.5, -9_223_372_036_854_775_000),
    ] {
        invoke_kind(
            &model,
            &cell,
            "Value2",
            InvokeKind::Put,
            vec![AutoValue::R8(number)],
        )
        .unwrap();
        assert_e6(
            invoke(&model, &cell, "Value", vec![]),
            &Ok(AutoValue::Currency(units)),
        );
        assert_e6(
            invoke(&model, &cell, "Value2", vec![]),
            &Ok(AutoValue::R8(number)),
        );
    }
}

/// dc-piw0: the report members registered where the committed descriptor describes them. The count
/// holds in both states: registered plus pending is the full list, every pending member is
/// really absent from the descriptor, and every registered one is a handler of the table.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn report_members_register_where_the_descriptor_describes_them() {
    let (_, model) = setup();
    let pending = model.report_members_pending();
    let implemented = model.handlers().implemented();
    // 102 report members are written; the pending ones are those the descriptor lacks.
    assert!(pending.len() <= 102);
    for name in &pending {
        assert!(!implemented.contains(name), "{name} is pending and also registered");
    }
    let registered = 102 - pending.len();
    eprintln!(
        "report members: {registered} registered, {} pending the descriptor regeneration",
        pending.len()
    );
    if pending.is_empty() {
        assert!(implemented.contains(&"ListObjects.Add (Method)".to_owned()));
        assert!(implemented.contains(&"Shape.Delete (Method)".to_owned()));
    }
}

/// dc-piw0: the macro's steps through the object-model members: fill a range, add the table,
/// rename it, add a chart over it with titles, two shapes and a connector, save, reload, list.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_piw0__a_macro_builds_a_report_through_object_model_members() {
    let session = HostSession::open(
        Arc::new(MemorySource::new(blank_package())),
        &OpenPolicy::default(),
        HostOptions::default(),
    )
    .unwrap();
    let (host, model) = setup_session(session);
    for (cell, value) in [
        ((1, 1), "Region"), ((1, 2), "Q1"), ((1, 3), "Q2"),
        ((2, 1), "North"), ((2, 2), "10"), ((2, 3), "20"),
        ((3, 1), "South"), ((3, 2), "30"), ((3, 3), "40"),
    ] {
        enter(&host, at(&host, cell.0, cell.1), value);
    }
    let app = model.application();
    let sheet = obj(invoke(&model, &app, "ActiveSheet", vec![]).unwrap());
    let tables = obj(invoke(&model, &sheet, "ListObjects", vec![]).unwrap());
    let source = range(&model, "A1:C3");
    let table = obj(invoke_kind(
        &model, &tables, "Add", InvokeKind::Method,
        vec![AutoValue::I4(1), AutoValue::Object(source.clone()), AutoValue::Missing, AutoValue::I4(1)],
    ).unwrap());
    invoke_kind(&model, &table, "Name", InvokeKind::Put, vec![AutoValue::text("Sales")]).unwrap();
    assert_eq!(text(&model, &table, "Name"), "Sales");
    let table_range = obj(invoke(&model, &table, "Range", vec![]).unwrap());
    assert_eq!(text(&model, &table_range, "Address"), "$A$1:$C$3");

    let charts = obj(invoke_kind(&model, &sheet, "ChartObjects", InvokeKind::Method, vec![]).unwrap());
    let frame = obj(invoke_kind(
        &model, &charts, "Add", InvokeKind::Method,
        vec![AutoValue::R8(200.0), AutoValue::R8(20.0), AutoValue::R8(300.0), AutoValue::R8(200.0)],
    ).unwrap());
    let chart = obj(invoke(&model, &frame, "Chart", vec![]).unwrap());
    invoke_kind(&model, &chart, "SetSourceData", InvokeKind::Method, vec![AutoValue::Object(table_range)]).unwrap();
    invoke_kind(&model, &chart, "HasTitle", InvokeKind::Put, vec![AutoValue::Bool(true)]).unwrap();
    let title = obj(invoke(&model, &chart, "ChartTitle", vec![]).unwrap());
    invoke_kind(&model, &title, "Text", InvokeKind::Put, vec![AutoValue::text("Sales")]).unwrap();

    let shapes = obj(invoke(&model, &sheet, "Shapes", vec![]).unwrap());
    let add = |name: &str, args: Vec<AutoValue>| {
        obj(invoke_kind(&model, &shapes, name, InvokeKind::Method, args).unwrap())
    };
    let first = add("AddShape", vec![AutoValue::I4(1), AutoValue::R8(20.0), AutoValue::R8(200.0), AutoValue::R8(100.0), AutoValue::R8(50.0)]);
    let second = add("AddShape", vec![AutoValue::I4(9), AutoValue::R8(20.0), AutoValue::R8(300.0), AutoValue::R8(100.0), AutoValue::R8(50.0)]);
    let connector = add("AddConnector", vec![AutoValue::I4(1), AutoValue::R8(0.0), AutoValue::R8(0.0), AutoValue::R8(10.0), AutoValue::R8(10.0)]);
    let format = obj(invoke(&model, &connector, "ConnectorFormat", vec![]).unwrap());
    invoke_kind(&model, &format, "BeginConnect", InvokeKind::Method, vec![AutoValue::Object(first.clone()), AutoValue::I4(3)]).unwrap();
    invoke_kind(&model, &format, "EndConnect", InvokeKind::Method, vec![AutoValue::Object(second), AutoValue::I4(1)]).unwrap();
    let frame_text = obj(invoke(&model, &first, "TextFrame2", vec![]).unwrap());
    let text_range = obj(invoke(&model, &frame_text, "TextRange", vec![]).unwrap());
    invoke_kind(&model, &text_range, "Text", InvokeKind::Put, vec![AutoValue::text("Start")]).unwrap();
    assert_eq!(text(&model, &text_range, "Text"), "Start");
    // Geometry round trips in points; columns and rows of the table; a row added to it.
    assert!(
        matches!(invoke(&model, &first, "Left", vec![]).unwrap(), AutoValue::R4(v) if v == 20.0)
    );
    invoke_kind(
        &model,
        &first,
        "Width",
        InvokeKind::Put,
        vec![AutoValue::R8(150.0)],
    )
    .unwrap();
    assert!(
        matches!(invoke(&model, &first, "Width", vec![]).unwrap(), AutoValue::R4(v) if v == 150.0)
    );
    // ChartObject.Left is a Double (Excel Q2).
    assert!(
        matches!(invoke(&model, &frame, "Left", vec![]).unwrap(), AutoValue::R8(v) if v == 200.0)
    );
    assert_eq!(text(&model, &frame, "Name"), "Chart 1");
    let columns = obj(invoke(&model, &table, "ListColumns", vec![]).unwrap());
    let second_column = obj(invoke(&model, &columns, "Item", vec![AutoValue::I4(2)]).unwrap());
    assert_eq!(text(&model, &second_column, "Name"), "Q1");
    let rows = obj(invoke(&model, &table, "ListRows", vec![]).unwrap());
    invoke_kind(&model, &rows, "Add", InvokeKind::Method, vec![]).unwrap();
    assert!(matches!(
        invoke(&model, &rows, "Count", vec![]).unwrap(),
        AutoValue::I4(3)
    ));
    let body = obj(invoke(&model, &table, "DataBodyRange", vec![]).unwrap());
    assert_eq!(text(&model, &body, "Address"), "$A$2:$C$4");
    // A shape's colours: RGB as VBA's RGB().
    let fill = obj(invoke(&model, &first, "Fill", vec![]).unwrap());
    let colour = obj(invoke(&model, &fill, "ForeColor", vec![]).unwrap());
    invoke_kind(
        &model,
        &colour,
        "RGB",
        InvokeKind::Put,
        vec![AutoValue::I4(255 + 128 * 256)],
    )
    .unwrap();
    assert!(
        matches!(invoke(&model, &colour, "RGB", vec![]).unwrap(), AutoValue::I4(v) if v == 255 + 128 * 256)
    );
    // An unsupported shape type refuses by the one rule.
    let refused = invoke_kind(
        &model,
        &shapes,
        "AddShape",
        InvokeKind::Method,
        vec![
            AutoValue::I4(52),
            AutoValue::R8(0.0),
            AutoValue::R8(0.0),
            AutoValue::R8(10.0),
            AutoValue::R8(10.0),
        ],
    );
    assert!(matches!(
        refused,
        Err(AutoError::NotModelled {
            owner: "dc-piw0.3",
            ..
        })
    ));
    // The chart counts as a shape of the sheet, as in Excel.
    assert!(matches!(invoke(&model, &shapes, "Count", vec![]).unwrap(), AutoValue::I4(4)));

    let mut bytes = Vec::new();
    host.session.borrow_mut().save(&SavePolicy::default(), &mut bytes).unwrap();
    let reloaded = HostSession::open(
        Arc::new(MemorySource::new(bytes)),
        &OpenPolicy::default(),
        HostOptions::default(),
    )
    .unwrap();
    let sheet_id = reloaded.host().sheets()[0].id();
    assert_eq!(reloaded.host().model().definitions().tables().len(), 1);
    assert_eq!(reloaded.host().charts(sheet_id).0.len(), 1);
    assert_eq!(reloaded.host().shapes(sheet_id).len(), 3);
}

// --- dc-piw0: Excel's recorded answers (~/codex-runs/eng1/excel-answers.md, 2026-11-14) -------

fn excel_error(reply: Reply) -> (i32, String) {
    match reply {
        Err(AutoError::Excel {
            number,
            description,
        }) => (number, description),
        other => panic!("an Excel error: {other:?}"),
    }
}

/// The first saved part whose name holds `folder` (`xl/drawings/`, `xl/tables/`, `xl/charts/`).
fn saved_part(session: &mut HostSession, folder: &str) -> String {
    let mut bytes = Vec::new();
    session.save(&SavePolicy::default(), &mut bytes).unwrap();
    let document = calc_doc::open(
        Arc::new(MemorySource::new(bytes)),
        &OpenPolicy::default(),
    )
    .expect("the saved package opens");
    let name = document
        .package()
        .parts()
        .iter()
        .map(|p| p.name().to_owned())
        .find(|n| n.starts_with(folder) && n.ends_with(".xml"))
        .unwrap_or_else(|| panic!("no part in {folder}"));
    String::from_utf8(document.package().read_part(&name).expect("the part")).unwrap()
}

fn blank_report() -> (Rc<Host>, Rc<WorkbookAutomation>, ObjectRef) {
    let session = HostSession::open(
        Arc::new(MemorySource::new(blank_package())),
        &OpenPolicy::default(),
        HostOptions::default(),
    )
    .unwrap();
    let (host, model) = setup_session(session);
    let app = model.application();
    let sheet = obj(invoke(&model, &app, "ActiveSheet", vec![]).unwrap());
    (host, model, sheet)
}

fn fill(host: &Host, top_left: (u32, u32), rows: &[&[&str]]) {
    for (r, row) in rows.iter().enumerate() {
        for (c, value) in row.iter().enumerate() {
            enter(host, at(host, top_left.0 + r as u32, top_left.1 + c as u32), value);
        }
    }
}

fn int(value: AutoValue) -> i64 {
    match value {
        AutoValue::I2(v) => i64::from(v),
        AutoValue::I4(v) => i64::from(v),
        other => panic!("an integer: {other:?}"),
    }
}

fn tables_add(
    model: &WorkbookAutomation,
    sheet: &ObjectRef,
    source: &ObjectRef,
    headers: Option<i32>,
) -> Reply {
    let tables = obj(invoke(model, sheet, "ListObjects", vec![]).unwrap());
    invoke_kind(
        model,
        &tables,
        "Add",
        InvokeKind::Method,
        vec![
            AutoValue::I4(1),
            AutoValue::Object(source.clone()),
            AutoValue::Missing,
            headers.map_or(AutoValue::Missing, AutoValue::I4),
        ],
    )
}

/// Q0, Q1, Q2, Q7, Q8: the report macro of excel-questions.md through the members, read back as
/// Excel printed it, then the saved drawing against the XML Excel saved.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_piw0__excel_answers_q0_the_report_macro_reads_and_saves_as_excel_did() {
    let (host, model, sheet) = blank_report();
    fill(&host, (1, 1), &[&["Region", "Q1", "Q2"], &["North", "10", "20"], &["South", "30", "40"]]);
    let source = range(&model, "A1:C3");
    let table = obj(tables_add(&model, &sheet, &source, Some(1)).unwrap());
    // Q0: `table Table1 $A$1:$C$3 TableStyleMedium2 False`.
    assert_eq!(text(&model, &table, "Name"), "Table1");
    assert_eq!(text(&model, &table, "TableStyle"), "TableStyleMedium2");
    assert!(matches!(invoke(&model, &table, "ShowTotals", vec![]).unwrap(), AutoValue::Bool(false)));
    let table_range = obj(invoke(&model, &table, "Range", vec![]).unwrap());
    assert_eq!(text(&model, &table_range, "Address"), "$A$1:$C$3");

    // Q0: `chartobject Chart 1 200 20 300 200 51 False True`, `after SetSourceData 51 2 False True`.
    let charts = obj(invoke_kind(&model, &sheet, "ChartObjects", InvokeKind::Method, vec![]).unwrap());
    let frame = obj(invoke_kind(&model, &charts, "Add", InvokeKind::Method,
        vec![AutoValue::R8(200.0), AutoValue::R8(20.0), AutoValue::R8(300.0), AutoValue::R8(200.0)]).unwrap());
    assert_eq!(text(&model, &frame, "Name"), "Chart 1");
    for (side, value) in [("Left", 200.0), ("Top", 20.0), ("Width", 300.0), ("Height", 200.0)] {
        assert!(matches!(invoke(&model, &frame, side, vec![]).unwrap(), AutoValue::R8(v) if v == value), "{side}");
    }
    let chart = obj(invoke(&model, &frame, "Chart", vec![]).unwrap());
    assert_eq!(int(invoke(&model, &chart, "ChartType", vec![]).unwrap()), 51);
    assert!(matches!(invoke(&model, &chart, "HasTitle", vec![]).unwrap(), AutoValue::Bool(false)));
    assert!(matches!(invoke(&model, &chart, "HasLegend", vec![]).unwrap(), AutoValue::Bool(true)));
    let series = obj(invoke_kind(&model, &chart, "SeriesCollection", InvokeKind::Method, vec![]).unwrap());
    assert_eq!(int(invoke(&model, &series, "Count", vec![]).unwrap()), 0);
    invoke_kind(&model, &chart, "SetSourceData", InvokeKind::Method, vec![AutoValue::Object(table_range)]).unwrap();
    assert_eq!(int(invoke(&model, &series, "Count", vec![]).unwrap()), 2);
    assert!(matches!(invoke(&model, &chart, "HasTitle", vec![]).unwrap(), AutoValue::Bool(false)));
    // Q0/Q6: a table source is plotted by columns: series Q1, Q2 over the categories North, South.
    let first = obj(invoke_kind(&model, &series, "Item", InvokeKind::Method, vec![AutoValue::I4(1)]).unwrap());
    assert_eq!(text(&model, &first, "Name"), "Q1");
    assert_eq!(text(&model, &first, "Formula"), "=SERIES(Sheet1!$B$1,Sheet1!$A$2:$A$3,Sheet1!$B$2:$B$3,1)");
    invoke_kind(&model, &chart, "HasTitle", InvokeKind::Put, vec![AutoValue::Bool(true)]).unwrap();
    let title = obj(invoke(&model, &chart, "ChartTitle", vec![]).unwrap());
    assert_eq!(text(&model, &title, "Text"), "Chart Title");
    invoke_kind(&model, &title, "Text", InvokeKind::Put, vec![AutoValue::text("Sales")]).unwrap();

    // Q0/Q1: names, ids, types, positions of the drawing objects.
    let shapes = obj(invoke(&model, &sheet, "Shapes", vec![]).unwrap());
    let rect = |l: f64, t: f64, w: f64, h: f64| vec![AutoValue::R8(l), AutoValue::R8(t), AutoValue::R8(w), AutoValue::R8(h)];
    let with = |first: AutoValue, rest: Vec<AutoValue>| std::iter::once(first).chain(rest).collect::<Vec<_>>();
    let add = |name: &str, args: Vec<AutoValue>| obj(invoke_kind(&model, &shapes, name, InvokeKind::Method, args).unwrap());
    let sh1 = add("AddShape", with(AutoValue::I4(1), rect(20.0, 120.0, 100.0, 50.0)));
    let sh2 = add("AddShape", with(AutoValue::I4(9), rect(20.0, 220.0, 100.0, 50.0)));
    let tb = add("AddTextbox", with(AutoValue::I4(1), rect(160.0, 240.0, 120.0, 30.0)));
    let line = add("AddLine", vec![AutoValue::R8(150.0), AutoValue::R8(300.0), AutoValue::R8(250.0), AutoValue::R8(300.0)]);
    let cn = add("AddConnector", with(AutoValue::I4(1), rect(0.0, 0.0, 10.0, 10.0)));
    let cf = obj(invoke(&model, &cn, "ConnectorFormat", vec![]).unwrap());
    invoke_kind(&model, &cf, "BeginConnect", InvokeKind::Method, vec![AutoValue::Object(sh1.clone()), AutoValue::I4(3)]).unwrap();
    invoke_kind(&model, &cf, "EndConnect", InvokeKind::Method, vec![AutoValue::Object(sh2.clone()), AutoValue::I4(1)]).unwrap();
    let one = |o: &ObjectRef| (text(&model, o, "Name"), int(invoke(&model, o, "ID", vec![]).unwrap()),
        int(invoke(&model, o, "Type", vec![]).unwrap()), int(invoke(&model, o, "AutoShapeType", vec![]).unwrap()),
        int(invoke(&model, o, "Connector", vec![]).unwrap()));
    let chart_shape = obj(invoke_kind(&model, &shapes, "Item", InvokeKind::Method, vec![AutoValue::I4(1)]).unwrap());
    // `shape <name> <id> <Type> <AutoShapeType> ... <Connector>` lines of Q0.
    assert_eq!(one(&chart_shape), ("Chart 1".into(), 2, 3, -2, 0));
    assert_eq!(one(&sh1), ("Rectangle 2".into(), 3, 1, 1, 0));
    assert_eq!(one(&sh2), ("Oval 3".into(), 4, 1, 9, 0));
    assert_eq!(one(&tb), ("TextBox 4".into(), 5, 17, 1, 0));
    assert_eq!(one(&line), ("Straight Connector 5".into(), 6, 9, -2, -1));
    assert_eq!(one(&cn), ("Straight Arrow Connector 6".into(), 7, 1, -2, -1));
    assert_eq!(int(invoke(&model, &shapes, "Count", vec![]).unwrap()), 6);
    // Q0 `conn 70 170 0 50 -1 Rectangle 2 3`: the connector between the rectangle's bottom and the oval's top.
    let side = |o: &ObjectRef, n: &str| match invoke(&model, o, n, vec![]).unwrap() {
        AutoValue::R4(v) => f64::from(v),
        other => panic!("{other:?}"),
    };
    assert_eq!([side(&cn, "Left"), side(&cn, "Top"), side(&cn, "Width"), side(&cn, "Height")], [70.0, 170.0, 0.0, 50.0]);
    assert_eq!(int(invoke(&model, &cf, "BeginConnected", vec![]).unwrap()), -1);
    assert_eq!(int(invoke(&model, &cf, "BeginConnectionSite", vec![]).unwrap()), 3);
    let glued = obj(invoke(&model, &cf, "BeginConnectedShape", vec![]).unwrap());
    assert_eq!(text(&model, &glued, "Name"), "Rectangle 2");
    // Q7: site counts and flips.
    assert_eq!(int(invoke(&model, &sh1, "ConnectionSiteCount", vec![]).unwrap()), 4);
    assert_eq!(int(invoke(&model, &sh2, "ConnectionSiteCount", vec![]).unwrap()), 8);
    assert_eq!(int(invoke(&model, &line, "ConnectionSiteCount", vec![]).unwrap()), 2);
    assert_eq!(int(invoke(&model, &cn, "HorizontalFlip", vec![]).unwrap()), 0);
    // Q8: a vbCr starts a second paragraph, read back with a line feed; a vbLf stays in one.
    let range_of = |o: &ObjectRef| {
        let frame = obj(invoke(&model, o, "TextFrame2", vec![]).unwrap());
        obj(invoke(&model, &frame, "TextRange", vec![]).unwrap())
    };
    invoke_kind(&model, &range_of(&sh1), "Text", InvokeKind::Put, vec![AutoValue::text("a\rb")]).unwrap();
    assert_eq!(text(&model, &range_of(&sh1), "Text"), "a\nb");
    invoke_kind(&model, &range_of(&sh2), "Text", InvokeKind::Put, vec![AutoValue::text("a\nb")]).unwrap();
    assert_eq!(text(&model, &range_of(&sh2), "Text"), "a\nb");
    {
        let session = host.session.borrow();
        let sheet_id = session.host().sheets()[0].id();
        let held = session.host().shapes(sheet_id);
        assert_eq!(held[0].text.as_ref().map(Vec::len), Some(2));
        assert_eq!(held[1].text.as_ref().map(Vec::len), Some(1));
    }
    invoke_kind(&model, &range_of(&sh1), "Text", InvokeKind::Put, vec![AutoValue::text("Start")]).unwrap();
    invoke_kind(&model, &range_of(&sh2), "Text", InvokeKind::Put, vec![AutoValue::text("")]).unwrap();
    invoke_kind(&model, &range_of(&tb), "Text", InvokeKind::Put, vec![AutoValue::text("Note")]).unwrap();
    let fill_colour = obj(invoke(&model, &obj(invoke(&model, &sh1, "Fill", vec![]).unwrap()), "ForeColor", vec![]).unwrap());
    invoke_kind(&model, &fill_colour, "RGB", InvokeKind::Put, vec![AutoValue::I4(255 + 128 * 256)]).unwrap();
    let line_format = obj(invoke(&model, &sh1, "Line", vec![]).unwrap());
    let line_colour = obj(invoke(&model, &line_format, "ForeColor", vec![]).unwrap());
    invoke_kind(&model, &line_colour, "RGB", InvokeKind::Put, vec![AutoValue::I4(0)]).unwrap();
    invoke_kind(&model, &line_format, "Weight", InvokeKind::Put, vec![AutoValue::R8(2.0)]).unwrap();

    // The saved drawing against Excel's saved drawing1.xml (excel-report/xl/drawings/drawing1.xml).
    let drawing = saved_part(&mut host.session.borrow_mut(), "xl/drawings/");
    for expected in [
        // Every anchor is a twoCellAnchor with no editAs; the chart frame's xfrm is zero.
        r#"<xdr:cNvPr id="2" name="Chart 1"><a:extLst>"#,
        r#"<xdr:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/></xdr:xfrm>"#,
        r#"<xdr:cNvPr id="3" name="Rectangle 2"/>"#,
        r#"<a:xfrm><a:off x="254000" y="1524000"/><a:ext cx="1270000" cy="635000"/></a:xfrm><a:prstGeom prst="rect">"#,
        r#"<a:solidFill><a:srgbClr val="FF8000"/></a:solidFill><a:ln w="25400"><a:solidFill><a:srgbClr val="000000"/></a:solidFill></a:ln>"#,
        r#"<xdr:style><a:lnRef idx="2"><a:schemeClr val="accent1"><a:shade val="15000"/></a:schemeClr></a:lnRef><a:fillRef idx="1">"#,
        r#"<a:bodyPr vertOverflow="clip" horzOverflow="clip" rtlCol="0" anchor="t"/>"#,
        r#"<a:prstGeom prst="ellipse">"#,
        r#"<xdr:cNvSpPr txBox="1"/>"#,
        r#"<a:solidFill><a:schemeClr val="lt1"/></a:solidFill><a:ln w="9525" cmpd="sng"><a:solidFill><a:schemeClr val="lt1"><a:shade val="50000"/></a:schemeClr></a:solidFill></a:ln>"#,
        r#"<a:bodyPr vertOverflow="clip" horzOverflow="clip" vert="horz" rtlCol="0" anchor="t"/>"#,
        r#"<a:fontRef idx="minor"><a:schemeClr val="dk1"/></a:fontRef>"#,
        r#"<xdr:cNvPr id="6" name="Straight Connector 5"/>"#,
        r#"<a:prstGeom prst="line">"#,
        r#"<xdr:cNvPr id="7" name="Straight Arrow Connector 6"/>"#,
        r#"<a:stCxn id="3" idx="2"/><a:endCxn id="4" idx="0"/>"#,
        r#"<a:prstGeom prst="straightConnector1">"#,
        r#"<xdr:style><a:lnRef idx="2"><a:schemeClr val="accent1"/></a:lnRef><a:fillRef idx="0"><a:schemeClr val="accent1"/></a:fillRef><a:effectRef idx="1">"#,
    ] {
        assert!(drawing.contains(expected), "{expected}\n{drawing}");
    }
    assert!(!drawing.contains("editAs"), "{drawing}");
    assert!(!drawing.contains("oneCellAnchor"));
    assert_eq!(drawing.matches("<xdr:twoCellAnchor").count(), 6);
    // The table part: Excel's table1.xml has name and displayName Table1, TableStyleMedium2.
    let table_xml = saved_part(&mut host.session.borrow_mut(), "xl/tables/");
    for expected in [r#"name="Table1""#, r#"displayName="Table1""#, r#"ref="A1:C3""#, r#"name="TableStyleMedium2""#, r#"showRowStripes="1""#] {
        assert!(table_xml.contains(expected), "{expected}\n{table_xml}");
    }
}

/// Q6: how `SetSourceData` reads a range, series by series, as Excel printed them.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_piw0__excel_answers_q6_set_source_data_reads_ranges_as_excel_did() {
    let (host, model, sheet) = blank_report();
    fill(&host, (1, 1), &[&["Region", "Q1", "Q2"], &["North", "10", "20"], &["South", "30", "40"]]);
    fill(&host, (1, 6), &[&["1", "2", "3"], &["4", "5", "6"], &["7", "8", "9"]]);
    fill(&host, (2, 10), &[&["a", "1"], &["b", "2"]]);
    let charts = obj(invoke_kind(&model, &sheet, "ChartObjects", InvokeKind::Method, vec![]).unwrap());
    // (source, PlotBy, expected series as (name, formula)).
    let cases: Vec<(&str, Option<i32>, Vec<(&str, &str)>)> = vec![
        ("A1:C3", None, vec![
            ("North", "=SERIES(Sheet1!$A$2,Sheet1!$B$1:$C$1,Sheet1!$B$2:$C$2,1)"),
            ("South", "=SERIES(Sheet1!$A$3,Sheet1!$B$1:$C$1,Sheet1!$B$3:$C$3,2)")]),
        ("A1:C3", Some(1), vec![
            ("North", "=SERIES(Sheet1!$A$2,Sheet1!$B$1:$C$1,Sheet1!$B$2:$C$2,1)"),
            ("South", "=SERIES(Sheet1!$A$3,Sheet1!$B$1:$C$1,Sheet1!$B$3:$C$3,2)")]),
        ("A1:C3", Some(2), vec![
            ("Q1", "=SERIES(Sheet1!$B$1,Sheet1!$A$2:$A$3,Sheet1!$B$2:$B$3,1)"),
            ("Q2", "=SERIES(Sheet1!$C$1,Sheet1!$A$2:$A$3,Sheet1!$C$2:$C$3,2)")]),
        ("F1:H3", None, vec![
            ("Series1", "=SERIES(,,Sheet1!$F$1:$H$1,1)"),
            ("Series2", "=SERIES(,,Sheet1!$F$2:$H$2,2)"),
            ("Series3", "=SERIES(,,Sheet1!$F$3:$H$3,3)")]),
        ("J2:K3", None, vec![("Series1", "=SERIES(,Sheet1!$J$2:$J$3,Sheet1!$K$2:$K$3,1)")]),
        ("B1:C3", None, vec![
            ("Series1", "=SERIES(,Sheet1!$B$1:$C$1,Sheet1!$B$2:$C$2,1)"),
            ("Series2", "=SERIES(,Sheet1!$B$1:$C$1,Sheet1!$B$3:$C$3,2)")]),
        ("A2:C3", None, vec![
            ("North", "=SERIES(Sheet1!$A$2,,Sheet1!$B$2:$C$2,1)"),
            ("South", "=SERIES(Sheet1!$A$3,,Sheet1!$B$3:$C$3,2)")]),
    ];
    for (source, plot_by, expected) in cases {
        let frame = obj(invoke_kind(&model, &charts, "Add", InvokeKind::Method,
            vec![AutoValue::R8(0.0), AutoValue::R8(0.0), AutoValue::R8(100.0), AutoValue::R8(100.0)]).unwrap());
        let chart = obj(invoke(&model, &frame, "Chart", vec![]).unwrap());
        invoke_kind(&model, &chart, "SetSourceData", InvokeKind::Method,
            vec![AutoValue::Object(range(&model, source)), plot_by.map_or(AutoValue::Missing, AutoValue::I4)]).unwrap();
        let series = obj(invoke_kind(&model, &chart, "SeriesCollection", InvokeKind::Method, vec![]).unwrap());
        assert_eq!(int(invoke(&model, &series, "Count", vec![]).unwrap()) as usize, expected.len(), "{source} {plot_by:?}");
        for (i, (name, formula)) in expected.iter().enumerate() {
            let item = obj(invoke_kind(&model, &series, "Item", InvokeKind::Method, vec![AutoValue::I4(i as i32 + 1)]).unwrap());
            assert_eq!(text(&model, &item, "Name"), *name, "{source} {plot_by:?}");
            assert_eq!(text(&model, &item, "Formula"), *formula, "{source} {plot_by:?}");
            assert_eq!(int(invoke(&model, &item, "PlotOrder", vec![]).unwrap()), i as i64 + 1);
        }
        // Q6: a chart made by ChartObjects.Add has no title; a one-series chart shows its series' name.
        let titled = matches!(invoke(&model, &chart, "HasTitle", vec![]).unwrap(), AutoValue::Bool(true));
        assert_eq!(titled, expected.len() == 1, "{source}");
    }
    // Q6: a source with no data leaves the chart without series.
    let frame = obj(invoke_kind(&model, &charts, "Add", InvokeKind::Method,
        vec![AutoValue::R8(0.0), AutoValue::R8(0.0), AutoValue::R8(100.0), AutoValue::R8(100.0)]).unwrap());
    let chart = obj(invoke(&model, &frame, "Chart", vec![]).unwrap());
    invoke_kind(&model, &chart, "SetSourceData", InvokeKind::Method, vec![AutoValue::Object(range(&model, "Z1"))]).unwrap();
    let series = obj(invoke_kind(&model, &chart, "SeriesCollection", InvokeKind::Method, vec![]).unwrap());
    assert_eq!(int(invoke(&model, &series, "Count", vec![]).unwrap()), 0);
    // Q6: AddChart2 with the active cell in a data region builds from the region, with a title
    // and a legend, at the default place.
    let shapes = obj(invoke(&model, &sheet, "Shapes", vec![]).unwrap());
    let made = obj(invoke_kind(&model, &shapes, "AddChart2", InvokeKind::Method,
        vec![AutoValue::I4(201), AutoValue::I4(51)]).unwrap());
    let chart = obj(invoke(&model, &made, "Chart", vec![]).unwrap());
    assert!(matches!(invoke(&model, &chart, "HasTitle", vec![]).unwrap(), AutoValue::Bool(true)));
    assert!(matches!(invoke(&model, &chart, "HasLegend", vec![]).unwrap(), AutoValue::Bool(true)));
    let series = obj(invoke_kind(&model, &chart, "SeriesCollection", InvokeKind::Method, vec![]).unwrap());
    assert_eq!(int(invoke(&model, &series, "Count", vec![]).unwrap()), 2);
    let side = |n: &str| match invoke(&model, &made, n, vec![]).unwrap() {
        AutoValue::R4(v) => f64::from(v),
        other => panic!("{other:?}"),
    };
    // Q2: 520.8749, 169.125, 360 x 216 (single precision).
    // (Stored in whole EMU, a point is read back within a millionth of a point of the printed value.)
    for (got, printed) in [side("Left"), side("Top"), side("Width"), side("Height")]
        .into_iter()
        .zip([520.8749, 169.125, 360.0, 216.0])
    {
        assert!((got - printed).abs() < 1e-4, "{got} vs {printed}");
    }
    assert_eq!(int(invoke(&model, &made, "Type", vec![]).unwrap()), 3);
    // Saved, the empty chart and the others are charts of the drawing (one empty, one titled).
    let drawing = saved_part(&mut host.session.borrow_mut(), "xl/drawings/");
    assert!(drawing.contains(r#"name="Chart 1""#), "{drawing}");
    assert!(!drawing.contains("editAs"), "{drawing}");
}

/// Q3: the errors of the report members, as Excel numbered and worded them.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_piw0__excel_answers_q3_errors_follow_excel_numbers_and_descriptions() {
    let (host, model, sheet) = blank_report();
    fill(&host, (1, 1), &[&["Region", "Q1", "Q2"], &["North", "10", "20"], &["South", "30", "40"]]);
    let table = obj(tables_add(&model, &sheet, &range(&model, "A1:C3"), Some(1)).unwrap());
    let tables = obj(invoke(&model, &sheet, "ListObjects", vec![]).unwrap());
    let item = |collection: &ObjectRef, argument: AutoValue| {
        invoke_kind(&model, collection, "Item", InvokeKind::Method, vec![argument])
    };
    let get_item = |collection: &ObjectRef, argument: AutoValue| {
        invoke_kind(&model, collection, "Item", InvokeKind::Get, vec![argument])
    };
    for argument in [AutoValue::I4(5), AutoValue::I4(0), AutoValue::text("nope")] {
        assert_eq!(excel_error(get_item(&tables, argument)), (9, "Subscript out of range".to_owned()));
    }
    let columns = obj(invoke(&model, &table, "ListColumns", vec![]).unwrap());
    assert_eq!(excel_error(get_item(&columns, AutoValue::I4(9))), (9, "Subscript out of range".to_owned()));
    // An overlapping table.
    assert_eq!(
        excel_error(tables_add(&model, &sheet, &range(&model, "B2:C3"), Some(1))),
        (1004, "A table can't overlap another table. ".to_owned())
    );
    // Table names: same twice and a case-only change are accepted; empty and over 255 characters are not.
    let set_name = |name: &str| invoke_kind(&model, &table, "Name", InvokeKind::Put, vec![AutoValue::text(name)]);
    set_name("Sales").unwrap();
    set_name("Sales").unwrap();
    set_name("sales").unwrap();
    assert_eq!(text(&model, &table, "Name"), "sales");
    assert_eq!(excel_error(set_name("")).0, 1004);
    assert_eq!(excel_error(set_name(&"x".repeat(300))), (5, "Invalid procedure call or argument".to_owned()));
    // A second table's name against the first, in another case: 1004.
    fill(&host, (6, 1), &[&["a", "b"], &["1", "2"]]);
    let other = obj(tables_add(&model, &sheet, &range(&model, "A6:B7"), Some(1)).unwrap());
    assert_eq!(text(&model, &other, "Name"), "Table1");
    assert_eq!(
        excel_error(invoke_kind(&model, &other, "Name", InvokeKind::Put, vec![AutoValue::text("SALES")])).0,
        1004
    );
    // Column names: the same name is fine, a name another column has is ignored, empty becomes
    // ColumnN, a number is stored as text.
    let column = obj(get_item(&columns, AutoValue::I4(1)).unwrap());
    let rename = |name: AutoValue| invoke_kind(&model, &column, "Name", InvokeKind::Put, vec![name]);
    rename(AutoValue::text("Region")).unwrap();
    rename(AutoValue::text("Q1")).unwrap();
    assert_eq!(text(&model, &column, "Name"), "Region");
    rename(AutoValue::text("")).unwrap();
    assert_eq!(text(&model, &column, "Name"), "Column1");
    rename(AutoValue::I4(5)).unwrap();
    assert_eq!(text(&model, &column, "Name"), "5");

    // Shapes and charts collections.
    let shapes = obj(invoke(&model, &sheet, "Shapes", vec![]).unwrap());
    let by_name = "The item with the specified name wasn't found.".to_owned();
    let by_index = "The index into the specified collection is out of bounds.".to_owned();
    assert_eq!(excel_error(item(&shapes, AutoValue::text("nope"))), (-2147024809, by_name.clone()));
    for argument in [AutoValue::I4(99), AutoValue::I4(0)] {
        assert_eq!(excel_error(item(&shapes, argument)), (-2147024809, by_index.clone()));
    }
    let charts = obj(invoke_kind(&model, &sheet, "ChartObjects", InvokeKind::Method, vec![]).unwrap());
    assert_eq!(excel_error(item(&charts, AutoValue::I4(9))).0, 1004);
    assert_eq!(excel_error(item(&charts, AutoValue::text("nope"))), (-2147024809, by_name));
    let rect = |l: f64, t: f64, w: f64, h: f64| vec![AutoValue::R8(l), AutoValue::R8(t), AutoValue::R8(w), AutoValue::R8(h)];
    let with = |first: AutoValue, rest: Vec<AutoValue>| std::iter::once(first).chain(rest).collect::<Vec<_>>();
    let add = |name: &str, args: Vec<AutoValue>| invoke_kind(&model, &shapes, name, InvokeKind::Method, args);
    // A negative size is out of range and makes no shape; zero is accepted.
    assert_eq!(
        excel_error(add("AddShape", with(AutoValue::I4(1), rect(10.0, 10.0, -5.0, 5.0)))),
        (1004, "The specified value is out of range.".to_owned())
    );
    assert_eq!(int(invoke(&model, &shapes, "Count", vec![]).unwrap()), 0);
    let zero = obj(add("AddShape", with(AutoValue::I4(1), rect(10.0, 10.0, 0.0, 0.0))).unwrap());
    assert_eq!(text(&model, &zero, "Name"), "Rectangle 1");
    let sh1 = obj(add("AddShape", with(AutoValue::I4(1), rect(20.0, 120.0, 100.0, 50.0))).unwrap());
    let sh2 = obj(add("AddShape", with(AutoValue::I4(9), rect(20.0, 220.0, 100.0, 50.0))).unwrap());
    assert_eq!((text(&model, &sh1, "Name"), text(&model, &sh2, "Name")), ("Rectangle 2".to_owned(), "Oval 3".to_owned()));
    // Names: another shape's name is accepted; empty is out of range.
    invoke_kind(&model, &sh1, "Name", InvokeKind::Put, vec![AutoValue::text("Oval 3")]).unwrap();
    assert_eq!(
        excel_error(invoke_kind(&model, &sh1, "Name", InvokeKind::Put, vec![AutoValue::text("")])),
        (-2147024809, "The specified value is out of range.".to_owned())
    );
    // Connector sites: 99, 0, -1 and 9 (an oval has 8) are invalid, 8 is fine, itself is invalid,
    // a chart is a type mismatch.
    let cn = obj(add("AddConnector", with(AutoValue::I4(1), rect(0.0, 0.0, 10.0, 10.0))).unwrap());
    let cf = obj(invoke(&model, &cn, "ConnectorFormat", vec![]).unwrap());
    let invalid = (-2147024809, "The specified parameter has an invalid value.".to_owned());
    for site in [99, 0, -1, 9] {
        let connect = invoke_kind(&model, &cf, "BeginConnect", InvokeKind::Method,
            vec![AutoValue::Object(sh2.clone()), AutoValue::I4(site)]);
        assert_eq!(excel_error(connect), invalid, "site {site}");
    }
    invoke_kind(&model, &cf, "BeginConnect", InvokeKind::Method, vec![AutoValue::Object(sh2.clone()), AutoValue::I4(8)]).unwrap();
    assert_eq!(int(invoke(&model, &cf, "BeginConnectionSite", vec![]).unwrap()), 8);
    let itself = invoke_kind(&model, &cf, "BeginConnect", InvokeKind::Method, vec![AutoValue::Object(cn.clone()), AutoValue::I4(1)]);
    assert_eq!(excel_error(itself), invalid);
    let frame = obj(invoke_kind(&model, &charts, "Add", InvokeKind::Method, rect(300.0, 20.0, 100.0, 100.0)).unwrap());
    let chart_shape = obj(item(&shapes, AutoValue::text("Chart 5")).unwrap_or_else(|e| panic!("{e:?}")));
    assert_eq!(text(&model, &frame, "Name"), "Chart 5");
    let to_chart = invoke_kind(&model, &cf, "EndConnect", InvokeKind::Method, vec![AutoValue::Object(chart_shape), AutoValue::I4(1)]);
    // A type mismatch is VBA's error 13 (DISP_E_TYPEMISMATCH).
    assert!(matches!(to_chart, Err(AutoError::Status(-2_147_352_571))), "{to_chart:?}");
    // Deleting the oval leaves the connector glued at the other end only; a second delete is 424.
    invoke_kind(&model, &sh2, "Delete", InvokeKind::Method, vec![]).unwrap();
    assert_eq!(excel_error(invoke_kind(&model, &sh2, "Delete", InvokeKind::Method, vec![])), (424, "Object required".to_owned()));
    assert_eq!(excel_error(invoke(&model, &sh2, "Name", vec![])).0, 424);
    assert_eq!(int(invoke(&model, &cf, "BeginConnected", vec![]).unwrap()), 0);
    // Chart errors.
    let chart = obj(invoke(&model, &frame, "Chart", vec![]).unwrap());
    assert_eq!(
        excel_error(invoke_kind(&model, &chart, "ChartType", InvokeKind::Put, vec![AutoValue::I4(9999)])),
        (-2147467259, "Method 'ChartType' of object '_Chart' failed".to_owned())
    );
    assert_eq!(int(invoke(&model, &chart, "ChartType", vec![]).unwrap()), 51);
    invoke_kind(&model, &chart, "ChartType", InvokeKind::Put, vec![AutoValue::I4(4)]).unwrap();
    assert_eq!(int(invoke(&model, &chart, "ChartType", vec![]).unwrap()), 4);
    let series = obj(invoke_kind(&model, &chart, "SeriesCollection", InvokeKind::Method, vec![]).unwrap());
    assert_eq!(
        excel_error(invoke_kind(&model, &series, "Item", InvokeKind::Method, vec![AutoValue::I4(9)])),
        (1004, "Parameter not valid".to_owned())
    );
    assert_eq!(
        excel_error(invoke_kind(&model, &chart, "SetSourceData", InvokeKind::Method, vec![AutoValue::Empty])).0,
        -2147417848
    );
    let title = obj(invoke(&model, &chart, "ChartTitle", vec![]).unwrap());
    let no_title = (-2147024809, "This object has no title.".to_owned());
    assert_eq!(excel_error(invoke(&model, &title, "Text", vec![])), no_title);
    assert_eq!(excel_error(invoke_kind(&model, &title, "Text", InvokeKind::Put, vec![AutoValue::text("x")])), no_title);
    invoke_kind(&model, &chart, "HasTitle", InvokeKind::Put, vec![AutoValue::Bool(true)]).unwrap();
    assert_eq!(text(&model, &title, "Text"), "Chart Title");
    assert_eq!(
        excel_error(invoke_kind(&model, &title, "Text", InvokeKind::Put, vec![AutoValue::text(&"t".repeat(300))])),
        (-2147467259, "Method 'Text' of object 'ChartTitle' failed".to_owned())
    );
}

/// Q4, Q5: a table's defaults and the header text of the first row, as Excel printed them.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_piw0__excel_answers_q4_q5_table_headers_and_defaults() {
    let (host, model, sheet) = blank_report();
    // Q5: numeric header cells become text "1", "2", "3" and the columns are named so.
    fill(&host, (1, 1), &[&["1", "2", "3"], &["4", "5", "6"]]);
    let numeric = obj(tables_add(&model, &sheet, &range(&model, "A1:C2"), Some(1)).unwrap());
    let columns = obj(invoke(&model, &numeric, "ListColumns", vec![]).unwrap());
    for (i, name) in ["1", "2", "3"].iter().enumerate() {
        let column = obj(invoke_kind(&model, &columns, "Item", InvokeKind::Get, vec![AutoValue::I4(i as i32 + 1)]).unwrap());
        assert_eq!(text(&model, &column, "Name"), *name);
    }
    assert_eq!(
        invoke(&model, &range(&model, "A1"), "Value", vec![]).unwrap().as_text().as_deref(),
        Some("1")
    );
    // Q5: repeated headers x, x, x become x, x2, x3; blank headers Column1..Column3.
    fill(&host, (5, 1), &[&["x", "x", "x"], &["1", "2", "3"]]);
    let repeated = obj(tables_add(&model, &sheet, &range(&model, "A5:C6"), Some(1)).unwrap());
    let columns = obj(invoke(&model, &repeated, "ListColumns", vec![]).unwrap());
    let names: Vec<String> = (1..=3)
        .map(|i| {
            let column = obj(invoke_kind(&model, &columns, "Item", InvokeKind::Get, vec![AutoValue::I4(i)]).unwrap());
            text(&model, &column, "Name")
        })
        .collect();
    assert_eq!(names, ["x", "x2", "x3"]);
    // Q4: without xlYes a first row of text is taken as the headers; any other first row makes
    // Excel insert a header row, which stays a typed refusal.
    fill(&host, (9, 1), &[&["a", "b"], &["1", "2"]]);
    let guessed = obj(tables_add(&model, &sheet, &range(&model, "A9:B10"), None).unwrap());
    assert_eq!(text(&model, &guessed, "Name"), "Table3");
    let refused = tables_add(&model, &sheet, &range(&model, "A13:B15"), None);
    assert!(matches!(refused, Err(AutoError::NotModelled { owner: "dc-piw0.1", .. })), "{refused:?}");
}

// --- dc-piw0.4: the macro-built report fixture (tests/fixtures/macro-report) -------------------

fn listing_text(value: &AutoValue) -> String {
    match value {
        AutoValue::Empty | AutoValue::Missing | AutoValue::Null => String::new(),
        AutoValue::Bool(true) => "True".into(),
        AutoValue::Bool(false) => "False".into(),
        AutoValue::I2(v) => v.to_string(),
        AutoValue::I4(v) => v.to_string(),
        AutoValue::R4(v) => v.to_string(),
        AutoValue::R8(v) | AutoValue::Date(v) => v.to_string(),
        other => other
            .as_text()
            .unwrap_or_else(|| format!("<{other:?}>")),
    }
}

fn item(model: &WorkbookAutomation, collection: &ObjectRef, index: i32) -> ObjectRef {
    // `Item` is a method on some collections and a property on others (the descriptor says which).
    let kind = if Descriptor::get().member(collection.interface, "Item", InvokeKind::Method).is_some() {
        InvokeKind::Method
    } else {
        InvokeKind::Get
    };
    obj(invoke_kind(model, collection, "Item", kind, vec![AutoValue::I4(index)]).unwrap())
}

fn count(model: &WorkbookAutomation, collection: &ObjectRef) -> i32 {
    int(invoke(model, collection, "Count", vec![]).unwrap()) as i32
}

/// The same records as `DumpReport.bas` writes (see its header), read through the object-model
/// handlers of the sheet the report was built on.
fn report_listing(model: &WorkbookAutomation) -> String {
    let mut out = String::new();
    let mut line = |fields: Vec<String>| {
        out.push_str(&fields.join("\t"));
        out.push('\n');
    };
    let app = model.application();
    let sheet = obj(invoke(model, &app, "ActiveSheet", vec![]).unwrap());
    for row in 1..=6u32 {
        for column in 1..=4u32 {
            let address = format!("{}{row}", char::from(b'A' + column as u8 - 1));
            let cell = range(model, &address);
            line(vec!["CELL".into(), address.clone(), listing_text(&invoke(model, &cell, "Value2", vec![]).unwrap())]);
            let formula = listing_text(&invoke(model, &cell, "Formula", vec![]).unwrap());
            if formula.starts_with('=') {
                line(vec!["FORMULA".into(), address, formula]);
            }
        }
    }
    let tables = obj(invoke(model, &sheet, "ListObjects", vec![]).unwrap());
    for i in 1..=count(model, &tables) {
        let table = item(model, &tables, i);
        let columns = obj(invoke(model, &table, "ListColumns", vec![]).unwrap());
        let names: Vec<String> = (1..=count(model, &columns))
            .map(|c| text(model, &item(model, &columns, c), "Name"))
            .collect();
        let rows = obj(invoke(model, &table, "ListRows", vec![]).unwrap());
        let table_range = obj(invoke(model, &table, "Range", vec![]).unwrap());
        line(vec![
            "TABLE".into(),
            text(model, &table, "Name"),
            text(model, &table_range, "Address"),
            text(model, &table, "TableStyle"),
            names.join("|"),
            count(model, &rows).to_string(),
        ]);
    }
    let charts = obj(invoke_kind(model, &sheet, "ChartObjects", InvokeKind::Method, vec![]).unwrap());
    for i in 1..=count(model, &charts) {
        let frame = item(model, &charts, i);
        let chart = obj(invoke(model, &frame, "Chart", vec![]).unwrap());
        let has_title = matches!(invoke(model, &chart, "HasTitle", vec![]).unwrap(), AutoValue::Bool(true));
        let title = if has_title {
            let title = obj(invoke(model, &chart, "ChartTitle", vec![]).unwrap());
            text(model, &title, "Text")
        } else {
            String::new()
        };
        let series = obj(invoke_kind(model, &chart, "SeriesCollection", InvokeKind::Method, vec![]).unwrap());
        let series_count = count(model, &series);
        let get = |name: &str| listing_text(&invoke(model, &frame, name, vec![]).unwrap());
        line(vec![
            "CHART".into(),
            text(model, &frame, "Name"),
            listing_text(&invoke(model, &chart, "ChartType", vec![]).unwrap()),
            get("Left"), get("Top"), get("Width"), get("Height"),
            listing_text(&AutoValue::Bool(has_title)),
            title,
            listing_text(&invoke(model, &chart, "HasLegend", vec![]).unwrap()),
            series_count.to_string(),
        ]);
        for s in 1..=series_count {
            let one = item(model, &series, s);
            line(vec![
                "SERIES".into(),
                text(model, &frame, "Name"),
                s.to_string(),
                text(model, &one, "Name"),
                text(model, &one, "Formula"),
                listing_text(&invoke(model, &one, "PlotOrder", vec![]).unwrap()),
            ]);
        }
    }
    let shapes = obj(invoke(model, &sheet, "Shapes", vec![]).unwrap());
    for i in 1..=count(model, &shapes) {
        let shape = item(model, &shapes, i);
        let kind = int(invoke(model, &shape, "Type", vec![]).unwrap());
        // MsoTriState members answer -1/0 (the handlers' I4), Booleans answer True/False.
        let truthy = |v: AutoValue| matches!(v, AutoValue::Bool(true)) || matches!(v, AutoValue::I4(n) if n != 0);
        let connector = truthy(invoke(model, &shape, "Connector", vec![]).unwrap());
        let mut text_of = String::new();
        // msoAutoShape = 1, msoTextBox = 17.
        if (kind == 1 || kind == 17) && !connector {
            let frame = obj(invoke(model, &shape, "TextFrame2", vec![]).unwrap());
            let range = obj(invoke(model, &frame, "TextRange", vec![]).unwrap());
            text_of = text(model, &range, "Text");
        }
        let (mut begin, mut begin_site, mut end, mut end_site) = (String::new(), String::new(), String::new(), String::new());
        if connector {
            let format = obj(invoke(model, &shape, "ConnectorFormat", vec![]).unwrap());
            let is = |name: &str| truthy(invoke(model, &format, name, vec![]).unwrap());
            if is("BeginConnected") {
                begin = text(model, &obj(invoke(model, &format, "BeginConnectedShape", vec![]).unwrap()), "Name");
                begin_site = listing_text(&invoke(model, &format, "BeginConnectionSite", vec![]).unwrap());
            }
            if is("EndConnected") {
                end = text(model, &obj(invoke(model, &format, "EndConnectedShape", vec![]).unwrap()), "Name");
                end_site = listing_text(&invoke(model, &format, "EndConnectionSite", vec![]).unwrap());
            }
        }
        let get = |name: &str| listing_text(&invoke(model, &shape, name, vec![]).unwrap());
        line(vec![
            "SHAPE".into(),
            text(model, &shape, "Name"),
            get("ID"),
            kind.to_string(),
            get("Left"), get("Top"), get("Width"), get("Height"),
            text_of, begin, begin_site, end, end_site,
        ]);
    }
    out
}

/// dc-piw0.4: the report of `BuildReport.bas`, step for step through the object-model members,
/// listed in the format of `DumpReport.bas`. The coordinator diffs this listing (printed, and
/// written to `$DNACALC_REPORT_LISTING` when set) with the Excel run's `report-listing.txt`.
/// The member sequence is the macro's; `report-data.tsv` is the macro's data grid.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_piw0_4__report_listing() {
    let (host, model, _) = blank_report();
    let app = model.application();
    let sheet = obj(invoke(&model, &app, "ActiveSheet", vec![]).unwrap());
    for (r, row) in include_str!("fixtures/macro-report/report-data.tsv").lines().enumerate() {
        for (c, cell) in row.split('\t').enumerate() {
            let address = format!("{}{}", char::from(b'A' + c as u8), r + 1);
            if cell.starts_with('=') {
                // The macro's own member: Range.Formula put, through the typed-entry path.
                invoke_kind(&model, &range(&model, &address), "Formula", InvokeKind::Put, vec![AutoValue::text(cell)]).unwrap();
                continue;
            }
            let value = match cell.parse::<f64>() {
                Ok(number) => AutoValue::R8(number),
                Err(_) => AutoValue::text(cell),
            };
            invoke_kind(&model, &range(&model, &address), "Value2", InvokeKind::Put, vec![value]).unwrap();
        }
    }
    let tables = obj(invoke(&model, &sheet, "ListObjects", vec![]).unwrap());
    let table = obj(invoke_kind(
        &model, &tables, "Add", InvokeKind::Method,
        vec![AutoValue::I4(1), AutoValue::Object(range(&model, "A1:D6")), AutoValue::Missing, AutoValue::I4(1)],
    ).unwrap());
    invoke_kind(&model, &table, "Name", InvokeKind::Put, vec![AutoValue::text("Sales")]).unwrap();
    invoke_kind(&model, &table, "TableStyle", InvokeKind::Put, vec![AutoValue::text("TableStyleMedium4")]).unwrap();

    let charts = obj(invoke_kind(&model, &sheet, "ChartObjects", InvokeKind::Method, vec![]).unwrap());
    let frame = obj(invoke_kind(
        &model, &charts, "Add", InvokeKind::Method,
        vec![AutoValue::R8(300.0), AutoValue::R8(20.0), AutoValue::R8(360.0), AutoValue::R8(220.0)],
    ).unwrap());
    invoke_kind(&model, &frame, "Name", InvokeKind::Put, vec![AutoValue::text("SalesChart")]).unwrap();
    let chart = obj(invoke(&model, &frame, "Chart", vec![]).unwrap());
    invoke_kind(&model, &chart, "SetSourceData", InvokeKind::Method, vec![AutoValue::Object(range(&model, "A1:C6"))]).unwrap();
    invoke_kind(&model, &chart, "HasTitle", InvokeKind::Put, vec![AutoValue::Bool(true)]).unwrap();
    let title = obj(invoke(&model, &chart, "ChartTitle", vec![]).unwrap());
    invoke_kind(&model, &title, "Text", InvokeKind::Put, vec![AutoValue::text("Sales by region")]).unwrap();
    invoke_kind(&model, &chart, "HasLegend", InvokeKind::Put, vec![AutoValue::Bool(true)]).unwrap();

    let shapes = obj(invoke(&model, &sheet, "Shapes", vec![]).unwrap());
    let add = |name: &str, args: [f64; 4], kind: Option<i32>| {
        let mut values = Vec::new();
        if let Some(kind) = kind {
            values.push(AutoValue::I4(kind));
        }
        values.extend(args.map(AutoValue::R8));
        obj(invoke_kind(&model, &shapes, name, InvokeKind::Method, values).unwrap())
    };
    let name_it = |shape: &ObjectRef, name: &str| {
        invoke_kind(&model, shape, "Name", InvokeKind::Put, vec![AutoValue::text(name)]).unwrap();
    };
    let set_text = |shape: &ObjectRef, value: &str| {
        let frame = obj(invoke(&model, shape, "TextFrame2", vec![]).unwrap());
        let range = obj(invoke(&model, &frame, "TextRange", vec![]).unwrap());
        invoke_kind(&model, &range, "Text", InvokeKind::Put, vec![AutoValue::text(value)]).unwrap();
    };
    let start = add("AddShape", [20.0, 200.0, 100.0, 50.0], Some(1));
    name_it(&start, "Start");
    set_text(&start, "Start");
    let fill = obj(invoke(&model, &start, "Fill", vec![]).unwrap());
    let colour = obj(invoke(&model, &fill, "ForeColor", vec![]).unwrap());
    invoke_kind(&model, &colour, "RGB", InvokeKind::Put, vec![AutoValue::I4(255 + 128 * 256)]).unwrap();
    let line = obj(invoke(&model, &start, "Line", vec![]).unwrap());
    invoke_kind(&model, &line, "Weight", InvokeKind::Put, vec![AutoValue::R8(2.0)]).unwrap();
    let finish = add("AddShape", [200.0, 300.0, 100.0, 50.0], Some(9));
    name_it(&finish, "Finish");
    set_text(&finish, "Finish");
    let link = add("AddConnector", [0.0, 0.0, 10.0, 10.0], Some(1));
    name_it(&link, "Link");
    let format = obj(invoke(&model, &link, "ConnectorFormat", vec![]).unwrap());
    invoke_kind(&model, &format, "BeginConnect", InvokeKind::Method, vec![AutoValue::Object(start), AutoValue::I4(3)]).unwrap();
    invoke_kind(&model, &format, "EndConnect", InvokeKind::Method, vec![AutoValue::Object(finish), AutoValue::I4(1)]).unwrap();
    let note = add("AddTextbox", [160.0, 240.0, 120.0, 30.0], Some(1));
    name_it(&note, "Note");
    set_text(&note, "Quarterly sales");
    let rule = add("AddLine", [20.0, 380.0, 300.0, 380.0], None);
    name_it(&rule, "Rule");

    let listing = report_listing(&model);
    eprintln!("{listing}");
    if let Ok(path) = std::env::var("DNACALC_REPORT_LISTING") {
        std::fs::write(path, &listing).unwrap();
    }
    // The records the Excel run lists, by kind.
    let count_of = |kind: &str| listing.lines().filter(|l| l.starts_with(&format!("{kind}\t"))).count();
    assert_eq!((count_of("CELL"), count_of("TABLE"), count_of("CHART"), count_of("SERIES"), count_of("SHAPE")), (24, 1, 1, 2, 6));
    assert!(listing.contains("CELL\tD2\t30\n"), "the formula column calculates");
    assert_eq!(count_of("FORMULA"), 5);
    assert!(listing.contains("FORMULA\tD2\t=B2+C2\n"));
    // Text, and the series' points, through the same handlers.
    assert_eq!(text(&model, &range(&model, "D2"), "Text"), "30");
    assert_eq!(text(&model, &range(&model, "A1"), "Formula"), "Region");
    let sheet = obj(invoke(&model, &model.application(), "ActiveSheet", vec![]).unwrap());
    let charts = obj(invoke_kind(&model, &sheet, "ChartObjects", InvokeKind::Method, vec![]).unwrap());
    let chart = obj(invoke(&model, &item(&model, &charts, 1), "Chart", vec![]).unwrap());
    let series = obj(invoke_kind(&model, &chart, "SeriesCollection", InvokeKind::Method, vec![]).unwrap());
    let first = item(&model, &series, 1);
    let points = |name: &str| match invoke(&model, &first, name, vec![]).unwrap() {
        AutoValue::Array(a) => a.values.iter().map(listing_text).collect::<Vec<_>>().join("|"),
        other => panic!("{other:?}"),
    };
    assert_eq!(points("XValues"), "North|South|East|West|Central");
    assert_eq!(points("Values"), "10|30|50|70|90");
    invoke_kind(&model, &first, "Values", InvokeKind::Put, vec![AutoValue::Object(range(&model, "C2:C6"))]).unwrap();
    assert_eq!(points("Values"), "20|40|60|80|100");
    assert!(text(&model, &first, "Formula").contains("$C$2:$C$6"));
    assert!(listing.contains("TABLE\tSales\t$A$1:$D$6\tTableStyleMedium4\tRegion|Q1|Q2|Total\t5\n"));
    let _ = host;
}
