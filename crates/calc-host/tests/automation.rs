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
                assert!(matches!(
                    error,
                    AutoError::NotModelled {
                        owner: "dc-1nbp.6",
                        ..
                    }
                ));
                return Ok(CalcValue::number(99.)); // Error swallowed: the formula must still refuse.
            }
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
fn value_refuses_unmeasured_date_and_custom_numeric_conversion() {
    let (_host, model) = setup();
    for address in ["A3", "A8", "A11"] {
        let range = range(&model, address);
        assert!(matches!(
            invoke(&model, &range, "Value2", vec![]).unwrap(),
            AutoValue::R8(_)
        ));
        assert!(matches!(
            invoke(&model, &range, "Value", vec![]),
            Err(AutoError::NotModelled {
                owner: "dc-1nbp.6",
                ..
            })
        ));
    }
    assert!(matches!(
        invoke(&model, &range(&model, "A12"), "Value", vec![]).unwrap(),
        AutoValue::R8(_)
    ));
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
        if event == "AfterCalculate" {
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
