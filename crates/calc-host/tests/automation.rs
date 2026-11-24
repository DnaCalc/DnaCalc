//! Workbook members through the same ObjectModel used by the native projection.
use calc_func::{UserFunction, UserFunctionContext};
use calc_host::{
    automation::{AutomationHost, WorkbookAutomation, busy},
    *,
};
use calc_object_model::profile::ApplicationHost;
use calc_object_model::{
    AutoArray, AutoError, AutoValue, Call, Descriptor, EventSource, InvokeKind, ObjectModel, ObjectRef,
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
    runs: Cell<u32>,
}
impl ApplicationHost for Host {
    fn hwnd(&self) -> isize {
        123
    }
    fn run(&self, name: &str, _: &[AutoValue]) -> Reply {
        if name == "TestCommand" {
            self.runs.set(self.runs.get() + 1);
            return Ok(AutoValue::Empty);
        }
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
        runs: Cell::new(0),
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
fn dc_oxh8w_2_horizontal_alignment_formats_a_whole_column() {
    let (host, model, _) = blank_report();
    let column = range(&model, "D:D");
    assert!(matches!(
        invoke(&model, &column, "HorizontalAlignment", vec![]).unwrap(),
        AutoValue::I4(1)
    ));
    invoke_kind(
        &model,
        &column,
        "HorizontalAlignment",
        InvokeKind::Put,
        vec![AutoValue::I4(-4108)],
    )
    .unwrap();
    assert!(matches!(
        invoke(
            &model,
            &range(&model, "D1048576"),
            "HorizontalAlignment",
            vec![]
        )
        .unwrap(),
        AutoValue::I4(-4108)
    ));
    assert!(matches!(
        invoke(&model, &range(&model, "D:E"), "HorizontalAlignment", vec![]).unwrap(),
        AutoValue::Null
    ));
    assert!(
        host.session
            .borrow_mut()
            .dispatch(SkinIntent::Undo)
            .accepted()
    );
    assert!(matches!(
        invoke(&model, &column, "HorizontalAlignment", vec![]).unwrap(),
        AutoValue::I4(1)
    ));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_2_vertical_alignment_preserves_other_properties() {
    let (_, model, _) = blank_report();
    let cells = range(&model, "A1:B2");
    let cell = range(&model, "A1");
    assert!(matches!(
        invoke(&model, &cells, "VerticalAlignment", vec![]).unwrap(),
        AutoValue::I4(-4107)
    ));
    invoke_kind(
        &model,
        &cell,
        "HorizontalAlignment",
        InvokeKind::Put,
        vec![AutoValue::I4(-4131)],
    )
    .unwrap();
    invoke_kind(
        &model,
        &cells,
        "VerticalAlignment",
        InvokeKind::Put,
        vec![AutoValue::I2(-4160)],
    )
    .unwrap();
    assert!(matches!(
        invoke(&model, &cells, "VerticalAlignment", vec![]).unwrap(),
        AutoValue::I4(-4160)
    ));
    assert!(matches!(
        invoke(&model, &cell, "HorizontalAlignment", vec![]).unwrap(),
        AutoValue::I4(-4131)
    ));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_2_wrap_text_reports_mixed_null_and_round_trips() {
    let (host, model, _) = blank_report();
    let cells = range(&model, "B1:B2");
    let cell = range(&model, "B1");
    assert!(matches!(
        invoke(&model, &cells, "WrapText", vec![]).unwrap(),
        AutoValue::Bool(false)
    ));
    invoke_kind(
        &model,
        &cell,
        "WrapText",
        InvokeKind::Put,
        vec![AutoValue::Bool(true)],
    )
    .unwrap();
    assert!(matches!(
        invoke(&model, &cells, "WrapText", vec![]).unwrap(),
        AutoValue::Null
    ));
    let mut saved = Vec::new();
    host.session
        .borrow_mut()
        .save(&SavePolicy::default(), &mut saved)
        .unwrap();
    let reopened = HostSession::open(
        Arc::new(MemorySource::new(saved)),
        &OpenPolicy::default(),
        HostOptions::default(),
    )
    .unwrap();
    let (_, reopened) = setup_session(reopened);
    assert!(matches!(
        invoke(&reopened, &range(&reopened, "B1"), "WrapText", vec![]).unwrap(),
        AutoValue::Bool(true)
    ));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_2_number_format_get_reports_uniform_codes_and_mixed_null() {
    let (_, model, _) = blank_report();
    let cells = range(&model, "A1:B2");
    assert_eq!(text(&model, &cells, "NumberFormat"), "General");
    invoke_kind(
        &model,
        &range(&model, "A1"),
        "NumberFormat",
        InvokeKind::Put,
        vec![AutoValue::text("0.00")],
    )
    .unwrap();
    assert!(matches!(
        invoke(&model, &cells, "NumberFormat", vec![]).unwrap(),
        AutoValue::Null
    ));
    // This workbook was authored by Excel. A2 is formatted as 0%.
    let (_, imported) = setup();
    assert_eq!(
        text(&imported, &range(&imported, "A2"), "NumberFormat"),
        "0%"
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_2_number_format_put_formats_a_report_column_in_one_undo_step() {
    let (host, model, _) = blank_report();
    fill(&host, (2, 4), &[&["30"], &["70"]]);
    let column = range(&model, "D:D");
    invoke_kind(
        &model,
        &column,
        "NumberFormat",
        InvokeKind::Put,
        vec![AutoValue::text("0.00")],
    )
    .unwrap();
    assert_eq!(text(&model, &column, "NumberFormat"), "0.00");
    assert_eq!(text(&model, &range(&model, "D2"), "Text"), "30.00");
    assert_eq!(
        text(&model, &range(&model, "D1048576"), "NumberFormat"),
        "0.00"
    );
    assert_eq!(text(&model, &range(&model, "E2"), "NumberFormat"), "General");

    let mut saved = Vec::new();
    host.session
        .borrow_mut()
        .save(&SavePolicy::default(), &mut saved)
        .unwrap();
    #[cfg(windows)]
    if let Some(path) = std::env::var_os("DNACALC_FORMATTING_WORKBOOK") {
        std::fs::write(path, &saved).unwrap();
    }
    let reopened = HostSession::open(
        Arc::new(MemorySource::new(saved)),
        &OpenPolicy::default(),
        HostOptions::default(),
    )
    .unwrap();
    let (_, reopened) = setup_session(reopened);
    assert_eq!(
        text(&reopened, &range(&reopened, "D:D"), "NumberFormat"),
        "0.00"
    );

    assert!(
        host.session
            .borrow_mut()
            .dispatch(SkinIntent::Undo)
            .accepted()
    );
    assert_eq!(text(&model, &column, "NumberFormat"), "General");
    assert_eq!(text(&model, &range(&model, "D2"), "Text"), "30");
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
        let model = self.model.upgrade().unwrap();
        if interface != "AppEvents" {
            for value in args {
                if let AutoValue::Object(object) = value {
                    model.released(object);
                }
            }
            return Ok(());
        }
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
        interface: &str,
        event: &str,
        args: &[AutoValue],
    ) -> Result<(), AutoError> {
        let model = self.model.upgrade().unwrap();
        if interface != "AppEvents" {
            for value in args {
                if let AutoValue::Object(object) = value {
                    model.released(object);
                }
            }
            return Ok(());
        }
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
        interface: &str,
        event: &str,
        args: &[AutoValue],
    ) -> Result<(), AutoError> {
        // The calculation events are counted by other tests; this one watches the writes.
        if event == "AfterCalculate" || event == "SheetCalculate" {
            return Ok(());
        }
        let model = self.model.upgrade().unwrap();
        if interface != "AppEvents" {
            for value in args {
                if let AutoValue::Object(object) = value {
                    model.released(object);
                }
            }
            return Ok(());
        }
        assert_eq!(event, "SheetChange");
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
    assert_eq!(text(&model, &other, "Name"), "Table2");
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
    // Q4: without xlYes a first row of text is taken as the headers (any other first row makes
    // Excel insert a header row: Q12, dc_piw0_10__q12_*).
    fill(&host, (9, 1), &[&["a", "b"], &["1", "2"]]);
    let guessed = obj(tables_add(&model, &sheet, &range(&model, "A9:B10"), None).unwrap());
    assert_eq!(text(&model, &guessed, "Name"), "Table3");
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

/// dc-km7i: `Range("A1,C1:C2").Value2 = 5` writes every area (Excel's one Change over the union, as the
/// AppEvents cases write it). A clear reaches each area. Mixed area sizes still refuse an array put.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn range_value2_put_and_clear_contents_reach_every_area_of_a_multi_area_range() {
    let (host, model, _) = blank_report();
    fill(&host, (1, 2), &[&["keep"]]);
    let union = range(&model, "A1,C1:C2");
    invoke_kind(&model, &union, "Value2", InvokeKind::Put, vec![AutoValue::R8(5.0)]).unwrap();
    for (address, expected) in [("A1", 5.0), ("C1", 5.0), ("C2", 5.0)] {
        assert!(matches!(invoke(&model, &range(&model, address), "Value2", vec![]).unwrap(),
            AutoValue::R8(v) if v == expected), "{address}");
    }
    assert_eq!(text(&model, &range(&model, "B1"), "Text"), "keep");
    let refused = invoke_kind(&model, &union, "Value2", InvokeKind::Put, vec![AutoValue::Array(Box::new(AutoArray {
        rows: 1, columns: 1, lower: (1, 1), values: vec![AutoValue::R8(1.0)] }))]);
    assert!(matches!(refused, Err(AutoError::NotModelled { owner: "dc-1nbp.6", .. })));
    invoke_kind(&model, &union, "ClearContents", InvokeKind::Method, vec![]).unwrap();
    for address in ["A1", "C1", "C2"] {
        assert!(matches!(invoke(&model, &range(&model, address), "Value2", vec![]).unwrap(), AutoValue::Empty), "{address}");
    }
    assert_eq!(text(&model, &range(&model, "B1"), "Text"), "keep");
}

/// E11 in 2026-11-14-om-probes: equal-sized areas each receive the same array.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_1nbp_6_e11_value2_arrays_repeat_across_areas() {
    let (host, model, _) = blank_report();
    let union = range(&model, "B2:C3,F5:G6");
    let locations = ["B2", "C2", "B3", "C3", "F5", "G5", "F6", "G6"];
    for (side, values, expected) in [
        (2, vec![1.0, 2.0, 3.0, 4.0], [1.0, 2.0, 3.0, 4.0, 1.0, 2.0, 3.0, 4.0]),
        (1, vec![9.0], [9.0; 8]),
    ] {
        let array = AutoValue::Array(Box::new(AutoArray {
            rows: side,
            columns: side,
            lower: (1, 1),
            values: values.into_iter().map(AutoValue::R8).collect(),
        }));
        let previous_log = host.session.borrow().intent_log().len();
        invoke_kind(&model, &union, "Value2", InvokeKind::Put, vec![array]).unwrap();
        assert_eq!(host.session.borrow().intent_log().len(), previous_log + 1);
        for (address, expected) in locations.into_iter().zip(expected) {
            let actual = invoke(&model, &range(&model, address), "Value2", vec![]).unwrap();
            assert!(
                matches!(actual, AutoValue::R8(v) if v.to_bits() == f64::to_bits(expected)),
                "{address}: {actual:?}"
            );
        }
    }
    for address in ["D4", "H7"] {
        let actual = invoke(&model, &range(&model, address), "Value2", vec![]).unwrap();
        assert!(matches!(actual, AutoValue::Empty), "{address}: {actual:?}");
    }
}

// --- dc-piw0.10: the recorded Excel answers Q11-Q17 (one test each) ---------------------------------

/// Q11 (excel-answers.md, run1 q11/q11b): a table's Name is kept as written, its DisplayName is derived
/// from it and is the name formulas use; 256 characters is error 5, empty and a clash (any case) 1004.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_piw0_10__q11_table_names_keep_their_text_and_display_names_are_derived() {
    let (host, model, sheet) = blank_report();
    fill(&host, (1, 1), &[&["H1", "H2"], &["1", "2"]]);
    fill(&host, (1, 5), &[&["K1"], &["3"]]);
    let table = obj(tables_add(&model, &sheet, &range(&model, "A1:B2"), Some(1)).unwrap());
    let other = obj(tables_add(&model, &sheet, &range(&model, "E1:E2"), Some(1)).unwrap());
    assert_eq!(text(&model, &other, "Name"), "Table2");
    let set = |name: &str| invoke_kind(&model, &table, "Name", InvokeKind::Put, vec![AutoValue::text(name)]);
    let long = "x".repeat(255);
    for (name, display) in [
        ("My Table", "My_Table"),
        ("My-Table", "My_Table"),
        ("1Table", "_1Table"),
        ("A1", "A1_"),
        ("R1C1", "_R1C1"),
        ("R", "R_"),
        ("C", "C_"),
        ("r1", "_r1"),
        ("Table.1", "Table.1"),
        ("Sales2024", "Sales2024"),
        ("\\x", "\\x"),
        (" lead", "lead"),
        ("trail ", "trail"),
        ("Tab\tle", "Tab_le"),
        ("?q", "_?q"),
        ("a!b", "a_b"),
        ("True", "True_"),
        ("x[1]", "x_1"),
        (long.as_str(), long.as_str()),
    ] {
        set(name).unwrap_or_else(|e| panic!("{name:?}: {e:?}"));
        assert_eq!((text(&model, &table, "Name"), text(&model, &table, "DisplayName")), (name.to_owned(), display.to_owned()));
    }
    // Refused sets leave the old name.
    let kept = text(&model, &table, "Name");
    assert_eq!(excel_error(set("")).0, 1004);
    assert_eq!(excel_error(set(&"x".repeat(256))), (5, "Invalid procedure call or argument".to_owned()));
    assert_eq!(excel_error(set("Table2")).0, 1004);
    assert_eq!(excel_error(set("table2")).0, 1004);
    assert_eq!(text(&model, &table, "Name"), kept);
    // DisplayName: a put needs a defined-name shape (1004), and the Name follows it.
    let put_display = |name: &str| {
        invoke_kind(&model, &table, "DisplayName", InvokeKind::Put, vec![AutoValue::text(name)])
    };
    assert_eq!(excel_error(put_display("Disp Name")).0, 1004);
    put_display("Disp_Name").unwrap();
    assert_eq!(
        (text(&model, &table, "Name"), text(&model, &table, "DisplayName")),
        ("Disp_Name".to_owned(), "Disp_Name".to_owned())
    );
    // Formulas use the DisplayName; a table is found by its Name (its DisplayName is error 9).
    set("My Table2").unwrap();
    enter(&host, at(&host, 1, 8), "=COUNT(My_Table2[H1])");
    assert!(matches!(invoke(&model, &range(&model, "H1"), "Value2", vec![]).unwrap(), AutoValue::R8(v) if v == 1.0));
    let tables = obj(invoke(&model, &sheet, "ListObjects", vec![]).unwrap());
    let find = |name: &str| invoke_kind(&model, &tables, "Item", InvokeKind::Get, vec![AutoValue::text(name)]);
    assert!(find("My Table2").is_ok());
    assert_eq!(excel_error(find("My_Table2")).0, 9);
}

const XL_YES: i32 = 1;
const XL_NO: i32 = 2;
const XL_GUESS: i32 = 0;

fn table_columns(model: &WorkbookAutomation, table: &ObjectRef) -> Vec<String> {
    let columns = obj(invoke(model, table, "ListColumns", vec![]).unwrap());
    (1..=count(model, &columns)).map(|i| text(model, &item(model, &columns, i), "Name")).collect()
}

fn table_address(model: &WorkbookAutomation, table: &ObjectRef) -> String {
    let range_of = obj(invoke(model, table, "Range", vec![]).unwrap());
    text(model, &range_of, "Address")
}

fn cell_text_of(model: &WorkbookAutomation, address: &str) -> String {
    listing_text(&invoke(model, &range(model, address), "Value2", vec![]).unwrap())
}

/// Q12 (excel-answers.md, run1 q12): xlNo, and xlGuess over numbers, insert a header row Column1, Column2 at the
/// source's top row and move the data down (no room above is needed, the cell above is not used); xlYes turns every
/// first-row cell into its text in place (a formula is replaced by its text, an error becomes text, an empty cell
/// becomes ColumnN); a one-row xlYes source gets an added empty data row.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_piw0_10__q12_xl_no_and_guess_insert_a_header_row_and_xl_yes_keeps_the_first_row() {
    // (a) xlNo over A1:B2 (1, 2 / 3, 4): table A1:B3, header Column1/Column2, data A2:B3.
    let (host, model, sheet) = blank_report();
    fill(&host, (1, 1), &[&["1", "2"], &["3", "4"]]);
    let table = obj(tables_add(&model, &sheet, &range(&model, "A1:B2"), Some(XL_NO)).unwrap());
    assert_eq!(table_address(&model, &table), "$A$1:$B$3");
    assert_eq!(table_columns(&model, &table), ["Column1", "Column2"]);
    assert_eq!(
        ["A1", "B1", "A2", "B2", "A3", "B3"].map(|a| cell_text_of(&model, a)),
        ["Column1", "Column2", "1", "2", "3", "4"]
    );
    // (b) xlNo over E3:F4 with E2 = "above": E2 is untouched, the header goes to row 3.
    let (host, model, sheet) = blank_report();
    fill(&host, (2, 5), &[&["above"], &["x", "y"], &["5", "6"]]);
    let table = obj(tables_add(&model, &sheet, &range(&model, "E3:F4"), Some(XL_NO)).unwrap());
    assert_eq!(table_address(&model, &table), "$E$3:$F$5");
    assert_eq!(cell_text_of(&model, "E2"), "above");
    assert_eq!(["E3", "F3", "E4", "F4", "E5", "F5"].map(|a| cell_text_of(&model, a)), ["Column1", "Column2", "x", "y", "5", "6"]);
    // (c) xlYes over 1.5, TRUE, #DIV/0!, =1+1 and an empty cell: each becomes its text in place.
    let (host, model, sheet) = blank_report();
    fill(&host, (1, 1), &[&["1.5", "TRUE", "=1/0", "=1+1", ""], &["1", "2", "3", "4", "5"]]);
    let table = obj(tables_add(&model, &sheet, &range(&model, "A1:E2"), Some(XL_YES)).unwrap());
    assert_eq!(table_columns(&model, &table), ["1.5", "TRUE", "#DIV/0!", "2", "Column1"]);
    assert_eq!(cell_text_of(&model, "C1"), "#DIV/0!");
    assert_eq!(cell_text_of(&model, "D1"), "2");
    assert_eq!(cell_text_of(&model, "A2"), "1");
    // (d) xlGuess over numbers (1, 2 / 3, 4): no header, Column1/Column2 inserted, table P1:Q3.
    let (host, model, sheet) = blank_report();
    fill(&host, (1, 16), &[&["1", "2"], &["3", "4"]]);
    let table = obj(tables_add(&model, &sheet, &range(&model, "P1:Q2"), Some(XL_GUESS)).unwrap());
    assert_eq!(table_address(&model, &table), "$P$1:$Q$3");
    assert_eq!(table_columns(&model, &table), ["Column1", "Column2"]);
    assert_eq!(cell_text_of(&model, "P2"), "1");
    // (e) a one-row xlYes source a, b: S1:T2, row 1 the header, an empty data row added.
    let (host, model, sheet) = blank_report();
    fill(&host, (1, 19), &[&["a", "b"]]);
    let table = obj(tables_add(&model, &sheet, &range(&model, "S1:T1"), Some(XL_YES)).unwrap());
    assert_eq!(table_address(&model, &table), "$S$1:$T$2");
    assert_eq!(table_columns(&model, &table), ["a", "b"]);
}

fn put_value(model: &WorkbookAutomation, target: &ObjectRef, value: AutoValue) -> Reply {
    invoke_kind(model, target, "Value", InvokeKind::Put, vec![value])
}

fn grid_text(model: &WorkbookAutomation, from: &str) -> Vec<String> {
    // the four rows and five columns from A1 as `a|b|...` lines
    let _ = from;
    (1..=4u32)
        .map(|r| {
            (0..5u8)
                .map(|c| cell_text_of(model, &format!("{}{r}", char::from(b'A' + c))))
                .collect::<Vec<_>>()
                .join("|")
        })
        .collect()
}

/// Q13 (excel-answers.md, run4 q13b): Range, DataBodyRange and HeaderRowRange are read-only members; what VBA's Let
/// does through them is the default member of the returned range. `lo.Range.Value = 9` writes every cell and deletes
/// the table; the body and header puts write their cells and keep the table (the header names made unique).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_piw0_10__q13_a_value_put_over_the_whole_table_deletes_it_body_and_header_puts_keep_it() {
    let fresh = || {
        let (host, model, sheet) = blank_report();
        fill(&host, (1, 1), &[&["H1", "H2"], &["1", "2"], &["3", "4"]]);
        let table = obj(tables_add(&model, &sheet, &range(&model, "A1:B3"), Some(XL_YES)).unwrap());
        (model, sheet, table)
    };
    let tables_count = |model: &WorkbookAutomation, sheet: &ObjectRef| {
        count(model, &obj(invoke(model, sheet, "ListObjects", vec![]).unwrap()))
    };
    // lo.Range.Value = 9 (the control; `lo.Range = 7` is the same Let): cells written, table gone.
    let (model, sheet, table) = fresh();
    let whole = obj(invoke(&model, &table, "Range", vec![]).unwrap());
    put_value(&model, &whole, AutoValue::R8(9.0)).unwrap();
    assert_eq!(tables_count(&model, &sheet), 0);
    assert_eq!(grid_text(&model, "A1"), ["9|9|||", "9|9|||", "9|9|||", "||||"]);
    // lo.DataBodyRange = 5: the body cells become 5, header and table stay.
    let (model, sheet, table) = fresh();
    let body = obj(invoke(&model, &table, "DataBodyRange", vec![]).unwrap());
    put_value(&model, &body, AutoValue::R8(5.0)).unwrap();
    assert_eq!(tables_count(&model, &sheet), 1);
    assert_eq!(grid_text(&model, "A1"), ["H1|H2|||", "5|5|||", "5|5|||", "||||"]);
    // lo.HeaderRowRange = "z": the header cells become z and z2 (made unique), the body is unchanged.
    let (model, sheet, table) = fresh();
    let header = obj(invoke(&model, &table, "HeaderRowRange", vec![]).unwrap());
    put_value(&model, &header, AutoValue::text("z")).unwrap();
    assert_eq!(tables_count(&model, &sheet), 1);
    assert_eq!(table_columns(&model, &table), ["z", "z2"]);
    assert_eq!(grid_text(&model, "A1"), ["z|z2|||", "1|2|||", "3|4|||", "||||"]);
}

fn add_rectangle(model: &WorkbookAutomation, shapes: &ObjectRef, left: f64) -> (String, i64) {
    let shape = obj(
        invoke_kind(
            model,
            shapes,
            "AddShape",
            InvokeKind::Method,
            vec![AutoValue::I4(1), AutoValue::R8(left), AutoValue::R8(10.0), AutoValue::R8(50.0), AutoValue::R8(30.0)],
        )
        .unwrap(),
    );
    (text(model, &shape, "Name"), int(invoke(model, &shape, "ID", vec![]).unwrap()))
}

fn delete_shape(model: &WorkbookAutomation, shapes: &ObjectRef, name: &str) {
    let shape = obj(invoke_kind(model, shapes, "Item", InvokeKind::Method, vec![AutoValue::text(name)]).unwrap());
    invoke_kind(model, &shape, "Delete", InvokeKind::Method, vec![]).unwrap();
}

/// Q14 (excel-answers.md, run1 q14, run4 q14b): within a session a shape id, name and table number is never reused
/// (one counter, name number = id - 1, whatever is deleted); after save and reopen the lowest free id is taken.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_piw0_10__q14_ids_and_names_are_not_reused_in_a_session_but_the_lowest_free_after_reopen() {
    let (host, model, sheet) = blank_report();
    let shapes = obj(invoke(&model, &sheet, "Shapes", vec![]).unwrap());
    let rect = |left: f64| add_rectangle(&model, &shapes, left);
    assert_eq!(rect(10.0), ("Rectangle 1".to_owned(), 2));
    assert_eq!(rect(70.0), ("Rectangle 2".to_owned(), 3));
    assert_eq!(rect(130.0), ("Rectangle 3".to_owned(), 4));
    // Delete the middle one: the next is 5, not 3.
    delete_shape(&model, &shapes, "Rectangle 2");
    assert_eq!(rect(10.0), ("Rectangle 4".to_owned(), 5));
    // Delete the highest two (ids 5 and 4): the next is 6, not 5 or 3.
    delete_shape(&model, &shapes, "Rectangle 4");
    delete_shape(&model, &shapes, "Rectangle 3");
    assert_eq!(rect(10.0), ("Rectangle 5".to_owned(), 6));
    // Tables: the number only counts up (Delete and Unlist alike).
    fill(&host, (1, 1), &[&["a", "b"], &["1", "2"]]);
    fill(&host, (1, 5), &[&["a", "b"], &["1", "2"]]);
    fill(&host, (1, 9), &[&["a", "b"], &["1", "2"]]);
    let tables = obj(invoke(&model, &sheet, "ListObjects", vec![]).unwrap());
    let first = obj(tables_add(&model, &sheet, &range(&model, "A1:B2"), Some(XL_YES)).unwrap());
    let second = obj(tables_add(&model, &sheet, &range(&model, "E1:F2"), Some(XL_YES)).unwrap());
    assert_eq!((text(&model, &first, "Name"), text(&model, &second, "Name")), ("Table1".to_owned(), "Table2".to_owned()));
    invoke_kind(&model, &first, "Delete", InvokeKind::Method, vec![]).unwrap();
    let third = obj(tables_add(&model, &sheet, &range(&model, "I1:J2"), Some(XL_YES)).unwrap());
    assert_eq!(text(&model, &third, "Name"), "Table3");
    invoke_kind(&model, &second, "Unlist", InvokeKind::Method, vec![]).unwrap();
    assert_eq!(count(&model, &tables), 1);
    fill(&host, (1, 1), &[&["a", "b"], &["1", "2"]]);
    let fourth = obj(tables_add(&model, &sheet, &range(&model, "A1:B2"), Some(XL_YES)).unwrap());
    assert_eq!(text(&model, &fourth, "Name"), "Table4");
    // After save and reopen the counters are gone: the file holds Rectangle 1 (id 2) and Rectangle 5 (id 6),
    // so the new shape takes the lowest free id 3 and a new table the lowest free number.
    let mut bytes = Vec::new();
    host.session.borrow_mut().save(&SavePolicy::default(), &mut bytes).unwrap();
    let session = HostSession::open(Arc::new(MemorySource::new(bytes)), &OpenPolicy::default(), HostOptions::default()).unwrap();
    let (host, model) = setup_session(session);
    let app = model.application();
    let sheet = obj(invoke(&model, &app, "ActiveSheet", vec![]).unwrap());
    let shapes = obj(invoke(&model, &sheet, "Shapes", vec![]).unwrap());
    assert_eq!(add_rectangle(&model, &shapes, 200.0), ("Rectangle 2".to_owned(), 3));
    fill(&host, (6, 1), &[&["a", "b"], &["1", "2"]]);
    let reopened = obj(tables_add(&model, &sheet, &range(&model, "A6:B7"), Some(XL_YES)).unwrap());
    // the file holds Table3 (I1:J2) and Table4 (A1:B2): Table1 is the lowest free number
    assert_eq!(text(&model, &reopened, "Name"), "Table1");
}

/// Q15 (excel-answers.md, run1 q15): every run Excel writes carries the machine's install/regional language
/// (`a:rPr lang="en-ZA"` there), while `c:lang` stays en-US; defRPr carries no lang. The host's locale stands in
/// for the machine's, `en-US` without one.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_piw0_10__q15_text_runs_carry_the_host_language() {
    for (locale, tag) in [
        (Some(calc_func::locale::format_profile(calc_func::locale::LocaleId::EnZa)), "en-ZA"),
        (None, "en-US"),
    ] {
        let session = HostSession::open(
            Arc::new(MemorySource::new(blank_package())),
            &OpenPolicy::default(),
            HostOptions { locale, ..Default::default() },
        )
        .unwrap();
        let (host, model) = setup_session(session);
        let app = model.application();
        let sheet = obj(invoke(&model, &app, "ActiveSheet", vec![]).unwrap());
        fill(&host, (1, 1), &[&["Region", "Q1"], &["North", "10"], &["South", "30"]]);
        let shapes = obj(invoke(&model, &sheet, "Shapes", vec![]).unwrap());
        let chart_shape = obj(
            invoke_kind(&model, &shapes, "AddChart2", InvokeKind::Method, vec![AutoValue::I4(201), AutoValue::I4(51)]).unwrap(),
        );
        let chart = obj(invoke(&model, &chart_shape, "Chart", vec![]).unwrap());
        let title = obj(invoke(&model, &chart, "ChartTitle", vec![]).unwrap());
        invoke_kind(&model, &title, "Text", InvokeKind::Put, vec![AutoValue::text("T")]).unwrap();
        let (_, _) = (add_rectangle(&model, &shapes, 200.0), 0);
        let rectangle = obj(invoke_kind(&model, &shapes, "Item", InvokeKind::Method, vec![AutoValue::text("Rectangle 2")]).unwrap());
        let box_shape = obj(
            invoke_kind(
                &model,
                &shapes,
                "AddTextbox",
                InvokeKind::Method,
                vec![AutoValue::I4(1), AutoValue::R8(300.0), AutoValue::R8(10.0), AutoValue::R8(80.0), AutoValue::R8(20.0)],
            )
            .unwrap(),
        );
        for (shape, words) in [(&rectangle, "shape text"), (&box_shape, "box text")] {
            let frame = obj(invoke(&model, shape, "TextFrame2", vec![]).unwrap());
            let range_of = obj(invoke(&model, &frame, "TextRange", vec![]).unwrap());
            invoke_kind(&model, &range_of, "Text", InvokeKind::Put, vec![AutoValue::text(words)]).unwrap();
        }
        let mut session = host.session.borrow_mut();
        let chart_xml = saved_part(&mut session, "xl/charts/");
        let drawing_xml = saved_part(&mut session, "xl/drawings/");
        assert!(chart_xml.contains("<c:lang val=\"en-US\"/>"), "{chart_xml}");
        assert!(
            chart_xml.contains(&format!("<a:p><a:pPr><a:defRPr/></a:pPr><a:r><a:rPr lang=\"{tag}\"/><a:t>T</a:t></a:r></a:p>")),
            "{tag}: {chart_xml}"
        );
        for words in ["shape text", "box text"] {
            assert!(
                drawing_xml.contains(&format!("<a:r><a:rPr lang=\"{tag}\" sz=\"1100\"/><a:t>{words}</a:t></a:r>")),
                "{tag} {words}: {drawing_xml}"
            );
        }
        assert!(!drawing_xml.contains("lang=\"en-US\"") || tag == "en-US");
    }
}

/// The `c16:uniqueId` counters of the series of the saved chart, in file order.
fn unique_ids(session: &mut HostSession) -> Vec<u32> {
    let xml = saved_part(session, "xl/charts/");
    xml.split("<c16:uniqueId val=\"{")
        .skip(1)
        .map(|rest| u32::from_str_radix(&rest[..8], 16).unwrap())
        .collect()
}

fn set_source(model: &WorkbookAutomation, chart: &ObjectRef, address: &str) {
    set_source_by(model, chart, address, None);
}

/// `SetSourceData` with PlotBy (1 xlRows, 2 xlColumns) or omitted.
fn set_source_by(model: &WorkbookAutomation, chart: &ObjectRef, address: &str, plot_by: Option<i32>) {
    let mut args = vec![AutoValue::Object(range(model, address))];
    args.extend(plot_by.map(AutoValue::I4));
    invoke_kind(model, chart, "SetSourceData", InvokeKind::Method, args).unwrap();
}

/// Q17 (excel-answers.md, run1 q17, run2 q17b): a macro chart's series carry a `c16:uniqueId` counter that only goes up:
/// `Shapes.AddChart2` starts at 0 (its series 0, 1), a re-plot keeps the numbers of the series it reuses and a series
/// it adds takes the next counter (never a freed one). For `ChartObjects.Add` the start is the series count of the
/// automatic source (Q18, the tests below).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_piw0_10__q17_series_unique_ids_start_at_zero_for_add_chart2_and_only_go_up() {
    let data: &[&[&str]] = &[&["Region", "Q1", "Q2", "Q3"], &["North", "10", "20", "70"], &["South", "30", "40", "80"], &["East", "50", "60", "90"]];
    // AddChart2 over the active cell's region A1:D4 (three series), then re-plots.
    let (host, model, sheet) = blank_report();
    fill(&host, (1, 1), data);
    let shapes = obj(invoke(&model, &sheet, "Shapes", vec![]).unwrap());
    let made = obj(invoke_kind(&model, &shapes, "AddChart2", InvokeKind::Method, vec![AutoValue::I4(201), AutoValue::I4(51)]).unwrap());
    let chart = obj(invoke(&model, &made, "Chart", vec![]).unwrap());
    assert_eq!(unique_ids(&mut host.session.borrow_mut()), [0, 1, 2]);
    set_source(&model, &chart, "A1:B4");
    assert_eq!(unique_ids(&mut host.session.borrow_mut()), [0]);
    set_source(&model, &chart, "A1:D4");
    // the first series is reused (0); the two it adds take 3 and 4, not the numbers 1 and 2 it once had
    assert_eq!(unique_ids(&mut host.session.borrow_mut()), [0, 3, 4]);
    // ChartObjects.Add with A1 active over the region A1:D4 (three automatic series) starts at 3: SetSourceData A1:C4
    // gives 3 and 4, a re-plot to one series keeps 3.
    let (host, model, sheet) = blank_report();
    fill(&host, (1, 1), data);
    let charts = obj(invoke_kind(&model, &sheet, "ChartObjects", InvokeKind::Method, vec![]).unwrap());
    let frame = obj(
        invoke_kind(
            &model,
            &charts,
            "Add",
            InvokeKind::Method,
            vec![AutoValue::R8(200.0), AutoValue::R8(20.0), AutoValue::R8(300.0), AutoValue::R8(200.0)],
        )
        .unwrap(),
    );
    let chart = obj(invoke(&model, &frame, "Chart", vec![]).unwrap());
    set_source(&model, &chart, "A1:C4");
    assert_eq!(unique_ids(&mut host.session.borrow_mut()), [3, 4]);
    set_source(&model, &chart, "A1:B4");
    assert_eq!(unique_ids(&mut host.session.borrow_mut()), [3]);
}

/// The `c16:uniqueId` counters of every saved chart part, per part in name order.
fn charts_unique_ids(session: &mut HostSession) -> Vec<Vec<u32>> {
    let mut bytes = Vec::new();
    session.save(&SavePolicy::default(), &mut bytes).unwrap();
    let document = calc_doc::open(Arc::new(MemorySource::new(bytes)), &OpenPolicy::default()).expect("the saved package opens");
    let mut names: Vec<String> = document
        .package()
        .parts()
        .iter()
        .map(|p| p.name().to_owned())
        .filter(|n| n.starts_with("xl/charts/") && n.ends_with(".xml") && !n.contains("/style") && !n.contains("/colors"))
        .collect();
    names.sort();
    names
        .iter()
        .map(|name| {
            let xml = String::from_utf8(document.package().read_part(name).expect("the part")).unwrap();
            xml.split("<c16:uniqueId val=\"{").skip(1).map(|rest| u32::from_str_radix(&rest[..8], 16).unwrap()).collect()
        })
        .collect()
}

fn select_cells(host: &Host, first: (u32, u32), last: (u32, u32)) {
    let (active, end) = (at(host, first.0, first.1), at(host, last.0, last.1));
    let area = calc_value::GridArea::new(active, end).unwrap();
    assert!(
        host.session
            .borrow_mut()
            .dispatch(SkinIntent::Select(Selection { active: Some(active), areas: vec![area] }))
            .accepted()
    );
}

/// dc-piw0.11 (3), Q18 (excel-answers q18-uniqueid-start.txt): a `ChartObjects.Add` chart takes its first uniqueId
/// from the series count of Excel's automatic source at Add time, then `SetSourceData A1:C3`; a second chart is made
/// with A1 selected. `second` is the ids of that second chart.
///
/// Excel's macro (eng5's Q18.bas) passed xlColumns to `SetSourceData`, so every case is compared exactly. (c07 and c11 were
/// once compared by their first id only, from a test that omitted PlotBy; what an omitted PlotBy does on a chart that already
/// has series is Q22, the tests below.)
fn q18_case(data: &[&[&str]], select: ((u32, u32), (u32, u32)), first: &[u32], second: &[u32]) {
    let (host, model, sheet) = blank_report();
    fill(&host, (1, 1), data);
    select_cells(&host, select.0, select.1);
    let charts = obj(invoke_kind(&model, &sheet, "ChartObjects", InvokeKind::Method, vec![]).unwrap());
    let add = |top: f64| {
        let frame = obj(
            invoke_kind(
                &model,
                &charts,
                "Add",
                InvokeKind::Method,
                vec![AutoValue::R8(200.0), AutoValue::R8(top), AutoValue::R8(300.0), AutoValue::R8(200.0)],
            )
            .unwrap(),
        );
        obj(invoke(&model, &frame, "Chart", vec![]).unwrap())
    };
    set_source_by(&model, &add(20.0), "A1:C3", Some(2));
    let shown = |ids: Vec<Vec<u32>>, want: &[&[u32]]| {
        assert_eq!(ids, want.iter().map(|w| w.to_vec()).collect::<Vec<_>>());
    };
    shown(charts_unique_ids(&mut host.session.borrow_mut()), &[first]);
    select_cells(&host, (1, 1), (1, 1));
    set_source_by(&model, &add(250.0), "A1:C3", Some(2));
    shown(charts_unique_ids(&mut host.session.borrow_mut()), &[first, second]);
}

const Q18_REGION: &[&[&str]] = &[&["Region", "Q1", "Q2"], &["North", "10", "20"], &["South", "30", "40"], &["East", "50", "60"]];
const Q18_REGION3: &[&[&str]] =
    &[&["Region", "Q1", "Q2", "Q3"], &["North", "10", "20", "70"], &["South", "30", "40", "80"], &["East", "50", "60", "90"]];

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_piw0_11__q18_c02_an_empty_active_cell_starts_at_zero_and_the_next_chart_at_the_region() {
    q18_case(Q18_REGION, ((10, 5), (10, 5)), &[0, 1], &[2, 3]);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_piw0_11__q18_c05_a_three_value_column_region_starts_at_three() {
    q18_case(Q18_REGION3, ((1, 1), (1, 1)), &[3, 4], &[3, 4]);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_piw0_11__q18_c06_a_selected_range_counts_instead_of_its_region() {
    q18_case(Q18_REGION3, ((2, 2), (3, 3)), &[2, 3], &[3, 4]);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_piw0_11__q18_c07_a_header_row_alone_is_one_series() {
    q18_case(&[&["Region", "Q1", "Q2"]], ((1, 1), (1, 1)), &[1, 2, 3], &[1, 2, 3]);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_piw0_11__q18_c09_a_region_of_text_is_one_series() {
    q18_case(&[&["a", "b", "c"], &["d", "e", "f"], &["g", "h", "i"]], ((1, 1), (1, 1)), &[1], &[1]);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_piw0_11__q18_c10_numbers_without_a_header_are_one_series_per_column() {
    q18_case(&[&["1", "2", "3"], &["4", "5", "6"], &["7", "8", "9"]], ((1, 1), (1, 1)), &[3, 4, 5], &[3, 4, 5]);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_piw0_11__q18_c11_a_date_column_under_a_text_header_is_a_series() {
    q18_case(
        &[&["Day", "Q1", "Q2"], &["46023", "10", "20"], &["46024", "30", "40"], &["46025", "50", "60"]],
        ((1, 1), (1, 1)),
        &[3, 4, 5],
        &[3, 4, 5],
    );
}

fn put_formula(model: &WorkbookAutomation, address: &str, value: AutoValue) {
    invoke_kind(model, &range(model, address), "Formula", InvokeKind::Put, vec![value]).unwrap();
}

fn formulas(model: &WorkbookAutomation, address: &str) -> Vec<String> {
    match invoke(model, &range(model, address), "Formula", vec![]).unwrap() {
        AutoValue::Array(a) => a
            .values
            .iter()
            .map(|v| v.as_text().unwrap_or_else(|| panic!("Formula is a String: {v:?}")))
            .collect(),
        one => vec![one.as_text().unwrap_or_else(|| panic!("Formula is a String: {one:?}"))],
    }
}

/// dc-piw0.11 (5): a second ListObjects.Add xlNo table, beside an existing table, inserts its header row as the first
/// does (Excel shifts only the source's columns down); the first table keeps its place and name, the second is Table2.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_piw0_11__a_second_xl_no_table_beside_another_table_inserts_its_header_row() {
    let (host, model, sheet) = blank_report();
    fill(&host, (1, 1), &[&["h1", "h2"], &["1", "2"], &["3", "4"]]);
    fill(&host, (1, 5), &[&["5", "6"], &["7", "8"]]);
    let first = obj(tables_add(&model, &sheet, &range(&model, "A1:B3"), Some(XL_YES)).unwrap());
    let second = obj(tables_add(&model, &sheet, &range(&model, "E1:F2"), Some(XL_NO)).unwrap());
    assert_eq!(table_address(&model, &first), "$A$1:$B$3");
    assert_eq!(table_address(&model, &second), "$E$1:$F$3");
    assert_eq!(table_columns(&model, &second), ["Column1", "Column2"]);
    assert_eq!(
        ["E1", "F1", "E2", "F2", "E3", "F3"].map(|a| cell_text_of(&model, a)),
        ["Column1", "Column2", "5", "6", "7", "8"]
    );
}

/// dc-piw0.11 (4), Q16 run1 A8: a typed `=sum( a1 : a2 )` is accepted, calculates, and shows as `=SUM( A1:A2 )`
/// (names and references upper-cased, the spaces around ":" dropped, the spaces inside the parentheses kept).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_piw0_11__q16_spaces_around_a_range_colon_are_accepted() {
    let (_host, model, _sheet) = blank_report();
    put_formula(&model, "A1", AutoValue::text("10"));
    put_formula(&model, "A2", AutoValue::text("0.1"));
    put_formula(&model, "A3", AutoValue::text("=sum( a1 : a2 )"));
    assert_eq!(formulas(&model, "A3"), ["=SUM( A1:A2 )"]);
    assert_eq!(invoke(&model, &range(&model, "A3"), "Text", vec![]).unwrap().as_text().unwrap(), "10.1");
}

fn row_array(values: Vec<AutoValue>) -> AutoValue {
    AutoValue::Array(Box::new(calc_object_model::AutoArray { rows: 1, columns: values.len() as u32, lower: (1, 1), values }))
}

/// Q16 (excel-answers.md, run1 q16 and run2 q16b): Range.Formula answers a String for every kind of cell (a formula in
/// canonical case, numbers to 15 digits, the quote prefix dropped); arrays put into a range fill what they do not reach
/// with #N/A, an array of arrays writes nothing, a 1-D array goes along rows and repeats its first element down a
/// column; Range.Text fits General to the column width and answers the shared text of several cells, else Null.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_piw0_10__q16_formula_is_a_string_arrays_fill_na_and_text_fits_the_column() {
    let (host, model, sheet) = blank_report();
    let _ = &sheet;
    // 1. Range.Formula get on constants and formulas.
    put_formula(&model, "A1", AutoValue::text("10"));
    put_formula(&model, "A2", AutoValue::text("0.1"));
    invoke_kind(&model, &range(&model, "A3"), "Value2", InvokeKind::Put, vec![AutoValue::R8(1.0 / 3.0)]).unwrap();
    put_formula(&model, "A4", AutoValue::text("abc"));
    put_formula(&model, "A5", AutoValue::text("TRUE"));
    put_formula(&model, "A6", AutoValue::text("=sum(a1:a2)"));
    put_formula(&model, "A7", AutoValue::text("=sum( a1:a2 )"));
    put_formula(&model, "A8", AutoValue::text("#N/A"));
    put_formula(&model, "A9", AutoValue::text("'=text"));
    put_formula(&model, "A10", AutoValue::text("'5"));
    put_formula(&model, "A12", AutoValue::text("1234567.891"));
    assert_eq!(
        formulas(&model, "A1:A12"),
        ["10", "0.1", "0.333333333333333", "abc", "TRUE", "=SUM(A1:A2)", "=SUM( A1:A2 )", "#N/A", "=text", "5", "", "1234567.891"]
    );
    // 2. A scalar put relocates; arrays: matching, smaller (the rest #N/A), 1-D, arrays of arrays.
    put_formula(&model, "B1:B3", AutoValue::text("=A1*2"));
    assert_eq!(formulas(&model, "B1:B3"), ["=A1*2", "=A2*2", "=A3*2"]);
    let two_by_two = || {
        AutoValue::Array(Box::new(calc_object_model::AutoArray {
            rows: 2,
            columns: 2,
            lower: (1, 1),
            values: ["=1+1", "2", "x", "=C1"].map(AutoValue::text).to_vec(),
        }))
    };
    put_formula(&model, "E1:F2", two_by_two());
    assert_eq!(formulas(&model, "E1:F2"), ["=1+1", "2", "x", "=C1"]);
    put_formula(&model, "G1:H3", two_by_two());
    assert_eq!(formulas(&model, "G1:H3"), ["=1+1", "2", "x", "=C1", "#N/A", "#N/A"]);
    put_formula(&model, "J1:L1", row_array(["=1+1", "2", "x"].map(AutoValue::text).to_vec()));
    assert_eq!(formulas(&model, "J1:L1"), ["=1+1", "2", "x"]);
    put_formula(&model, "N1:N3", row_array(["=1+1", "2", "x"].map(AutoValue::text).to_vec()));
    assert_eq!(formulas(&model, "N1:N3"), ["=1+1", "=1+1", "=1+1"]);
    let nested = row_array(vec![row_array(vec![AutoValue::text("a")]), row_array(vec![AutoValue::text("b")])]);
    put_formula(&model, "P1:Q2", nested);
    assert_eq!(formulas(&model, "P1:Q2"), ["", "", "", ""]);
    // 3. Range.Text: General fitted to the column (file width 10.75 shows 10 characters, 3.625 shows 3), and the
    // text of several cells (shared when equal, Null when not).
    let widen = |width: f64| {
        let sheet_id = host.session.borrow().host().sheets()[0].id();
        assert!(host
            .session
            .borrow_mut()
            .dispatch(SkinIntent::Command(WorkbookCommand::AxisFormat {
                sheet: sheet_id,
                rows: false,
                first: 1,
                count: 1,
                size_bits: Some(width.to_bits()),
                hidden: None,
            }))
            .accepted());
    };
    let shown = |address: &str| invoke(&model, &range(&model, address), "Text", vec![]).unwrap();
    widen(10.75);
    assert_eq!(shown("A3").as_text().unwrap(), "0.33333333");
    assert_eq!(shown("A1").as_text().unwrap(), "10");
    widen(3.625);
    assert_eq!(shown("A3").as_text().unwrap(), "0.3");
    assert_eq!(shown("A4").as_text().unwrap(), "abc");
    assert_eq!(shown("A12").as_text().unwrap(), "###");
    put_formula(&model, "R1:R3", AutoValue::text("7"));
    assert_eq!(shown("R1:R3").as_text().unwrap(), "7");
    assert!(matches!(shown("A1:A2"), AutoValue::Null));
}

/// Q21 (excel-answers/q21-setsource.txt, Excel 16.0, 2026-11-15, lane eng8's macro): `SetSourceData` on a fresh
/// `ChartObjects.Add` chart over 28 cases of source, each with PlotBy omitted, xlColumns (2) and xlRows (1). The recorded
/// lines give, per series, its SERIES formula (sheet k<N> here Sheet1); a case Excel could not read XValues for
/// (the ERR lines) is compared by its series count only.
fn q21_case(case: &str) {
    let log = std::str::from_utf8(include_bytes!("fixtures/excel-answers/q21-setsource.txt")).unwrap();
    struct Way {
        source: String,
        spec: String,
        plot_by: Option<i32>,
        count: usize,
        formulas: Vec<String>,
    }
    let mut ways: Vec<Way> = Vec::new();
    for line in log.lines().filter(|l| l.starts_with(case) && l[case.len()..].starts_with('\t')) {
        let f: Vec<&str> = line.split('\t').collect();
        if f[1] == "sheet" || f[1].starts_with("sheet ") {
            let way: i32 = f[2].trim_start_matches("way ").parse().unwrap();
            ways.push(Way {
                source: f[3].trim_start_matches("source ").to_string(),
                spec: f[4].trim_start_matches("spec ").to_string(),
                plot_by: (way != 0).then_some(way),
                count: 0,
                formulas: Vec::new(),
            });
        } else if f[2] == "series after" {
            ways.last_mut().unwrap().count = f[3].parse().unwrap();
        } else if f[2].starts_with("series ") {
            let formula = f[f.iter().position(|x| *x == "Formula").unwrap() + 1];
            ways.last_mut().unwrap().formulas.push(formula.to_string());
        }
    }
    assert!(!ways.is_empty(), "{case}");
    for (i, way) in ways.iter().enumerate() {
        let (host, model, _) = blank_report();
        for cell in way.spec.split(';').filter(|c| !c.is_empty()) {
            let (address, value) = cell.split_once('=').unwrap();
            let (kind, value) = value.split_once(':').unwrap();
            let column = u32::from(address.as_bytes()[0] - b'A') + 1;
            let row: u32 = address[1..].parse().unwrap();
            if kind == "d" {
                date_cell(&host, at(&host, row, column), value.parse().unwrap());
            } else {
                enter(&host, at(&host, row, column), value);
            }
        }
        let sheet = obj(invoke(&model, &model.application(), "ActiveSheet", vec![]).unwrap());
        let charts = obj(invoke_kind(&model, &sheet, "ChartObjects", InvokeKind::Method, vec![]).unwrap());
        let frame = obj(invoke_kind(&model, &charts, "Add", InvokeKind::Method,
            vec![AutoValue::R8(300.0), AutoValue::R8(20.0), AutoValue::R8(300.0), AutoValue::R8(200.0)]).unwrap());
        let chart = obj(invoke(&model, &frame, "Chart", vec![]).unwrap());
        invoke_kind(&model, &chart, "SetSourceData", InvokeKind::Method,
            vec![AutoValue::Object(range(&model, &way.source)), way.plot_by.map_or(AutoValue::Missing, AutoValue::I4)]).unwrap();
        let series = obj(invoke_kind(&model, &chart, "SeriesCollection", InvokeKind::Method, vec![]).unwrap());
        let label = format!("{case} way {i} {} {:?} spec {}", way.source, way.plot_by, way.spec);
        assert_eq!(int(invoke(&model, &series, "Count", vec![]).unwrap()) as usize, way.count, "{label}");
        for (n, recorded) in way.formulas.iter().enumerate() {
            let item = obj(invoke_kind(&model, &series, "Item", InvokeKind::Method, vec![AutoValue::I4(n as i32 + 1)]).unwrap());
            let sheet_prefix = recorded.find("'k").map(|p| recorded[p..].split_once("'!").unwrap().0.to_string() + "'!");
            let expected = match sheet_prefix {
                Some(prefix) => recorded.replace(&prefix, "Sheet1!"),
                None => recorded.clone(),
            };
            assert_eq!(text(&model, &item, "Formula"), expected, "{label} series {}", n + 1);
        }
    }
}

/// A cell holding the number `serial` under a date format (typing a date is refused by entry).
fn date_cell(host: &Host, at: CellKey, serial: f64) {
    use calc_doc_model::{CellFormat, NumberFormat};
    let mut styles = (**host.session.borrow().host().styles()).clone();
    let id = 300 + styles.number_formats.len() as u32;
    styles.number_formats.push(NumberFormat { id, code: "yyyy-mm-dd".into() });
    let style = styles.cell_formats.len() as u32;
    styles.cell_formats.push(CellFormat { number_format_id: Some(id), ..Default::default() });
    let edits = vec![
        CellEdit::Styles(Arc::new(styles)),
        CellEdit::Set { at, input: CellInput::Number(calc_doc_model::FiniteNumber::new(serial).unwrap()) },
        CellEdit::SetFormat { at, format: Some(style) },
    ];
    assert!(host.session.borrow_mut().dispatch(SkinIntent::Edit { edits }).accepted());
}

macro_rules! q21 {
    ($($name:ident => $case:literal),* $(,)?) => {$(
        #[cfg_attr(not(target_arch = "wasm32"), test)]
        #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
        fn $name() { q21_case($case); }
    )*};
}
q21! {
    dc_piw0_11__q21_c07_a_header_row_over_empty_rows => "c07",
    dc_piw0_11__q21_c09_a_region_of_text => "c09",
    dc_piw0_11__q21_c11_a_date_column_under_a_text_header => "c11",
    dc_piw0_11__q21_c12_an_empty_range_reads_one_label_row_and_column => "c12",
    dc_piw0_11__q21_e1_an_empty_two_by_two => "e1",
    dc_piw0_11__q21_e2_an_empty_row => "e2",
    dc_piw0_11__q21_e3_an_empty_column => "e3",
    dc_piw0_11__q21_e4_an_empty_four_by_four => "e4",
    dc_piw0_11__q21_e5_a_single_empty_cell_has_no_series => "e5",
    dc_piw0_11__q21_h1_a_text_row => "h1",
    dc_piw0_11__q21_h2_a_header_row_over_one_empty_row => "h2",
    dc_piw0_11__q21_h3_a_header_row_over_two_empty_rows_of_two => "h3",
    dc_piw0_11__q21_t1_a_text_column => "t1",
    dc_piw0_11__q21_t2_a_text_column_beside_an_empty_one => "t2",
    dc_piw0_11__q21_t3_a_text_column_beside_numbers => "t3",
    dc_piw0_11__q21_n1_a_header_over_one_numeric_column => "n1",
    dc_piw0_11__q21_n2_a_header_row_over_numbers => "n2",
    dc_piw0_11__q21_n3_numbers_three_by_two => "n3",
    dc_piw0_11__q21_n4_numbers_three_by_three => "n4",
    dc_piw0_11__q21_n5_header_row_and_label_column => "n5",
    dc_piw0_11__q21_d1_a_header_over_dates => "d1",
    dc_piw0_11__q21_d2_dates_beside_a_header_row => "d2",
    dc_piw0_11__q21_d3_dates_beside_numbers => "d3",
    dc_piw0_11__q21_d4_a_row_of_dates_over_numbers => "d4",
    dc_piw0_11__q21_b1_an_empty_corner => "b1",
    dc_piw0_11__q21_b2_a_blank_inside_the_header_row => "b2",
    dc_piw0_11__q21_g1_a_gap_in_a_column => "g1",
    dc_piw0_11__q21_g2_blank_values_under_a_header_row => "g2",
}

/// Q22 (excel-answers/q22-plotby.txt, Excel 16.0, 2026-11-15, lane eng12's macro): what an omitted PlotBy in
/// `SetSourceData` does on a chart that already has series, and the `PlotBy` property alone. Each case is one chart
/// (added with A1 selected, so over an automatic source, or with Z50 selected, so none) taken through steps S (omitted),
/// R (xlRows), C (xlColumns), P1 and P2 (`Chart.PlotBy = 1 / 2`), reading the series count, `PlotBy`, and every
/// series' name and SERIES formula after each step; a step Excel failed (the ERR line) must fail here too.
fn q22_case(case: &str) {
    let log = std::str::from_utf8(include_bytes!("fixtures/excel-answers/q22-plotby.txt")).unwrap();
    let lines: Vec<Vec<&str>> = log.lines().filter(|l| l.starts_with(case) && l[case.len()..].starts_with('\t')).map(|l| l.split('\t').collect()).collect();
    let header = lines.iter().find(|f| f[1].starts_with("sheet ")).expect(case);
    let field = |f: &[&str], key: &str| f.iter().find_map(|x| x.strip_prefix(key)).unwrap().to_string();
    let (source, selected, steps, spec) = (field(header, "source "), field(header, "selected "), field(header, "steps "), field(header, "spec "));
    let sheet_name = header[1].trim_start_matches("sheet ").to_string();
    let (host, model, _) = blank_report();
    for cell in spec.split(';').filter(|c| !c.is_empty()) {
        let (address, value) = cell.split_once('=').unwrap();
        let (kind, value) = value.split_once(':').unwrap();
        let column = u32::from(address.as_bytes()[0] - b'A') + 1;
        let row: u32 = address[1..].parse().unwrap();
        if kind == "d" {
            date_cell(&host, at(&host, row, column), value.parse().unwrap());
        } else {
            enter(&host, at(&host, row, column), value);
        }
    }
    let cell = if selected == "A1" { (1, 1) } else { (50, 26) };
    select_cells(&host, cell, cell);
    let sheet = obj(invoke(&model, &model.application(), "ActiveSheet", vec![]).unwrap());
    let charts = obj(invoke_kind(&model, &sheet, "ChartObjects", InvokeKind::Method, vec![]).unwrap());
    let frame = obj(
        invoke_kind(&model, &charts, "Add", InvokeKind::Method, vec![AutoValue::R8(300.0), AutoValue::R8(20.0), AutoValue::R8(300.0), AutoValue::R8(200.0)])
            .unwrap(),
    );
    let chart = obj(invoke(&model, &frame, "Chart", vec![]).unwrap());
    let check = |step: usize, code: &str| {
        let label = format!("{case} step {step} {code}");
        let head = lines
            .iter()
            .find(|f| f[1].starts_with(&format!("step {step} ")) && f.get(2) == Some(&"series"))
            .unwrap_or_else(|| panic!("{label}: no record"));
        let series = obj(invoke_kind(&model, &chart, "SeriesCollection", InvokeKind::Method, vec![]).unwrap());
        assert_eq!(int(invoke(&model, &series, "Count", vec![]).unwrap()).to_string(), head[3], "{label}: series count");
        assert_eq!(int(invoke(&model, &chart, "PlotBy", vec![]).unwrap()).to_string(), head[5], "{label}: PlotBy");
        let recorded: Vec<&Vec<&str>> = lines.iter().filter(|f| f[1] == format!("step {step}") && f[2].starts_with("series ")).collect();
        assert_eq!(recorded.len().to_string(), head[3], "{label}");
        for (n, f) in recorded.iter().enumerate() {
            let item = obj(invoke_kind(&model, &series, "Item", InvokeKind::Method, vec![AutoValue::I4(n as i32 + 1)]).unwrap());
            let expected = f[f.iter().position(|x| *x == "Formula").unwrap() + 1].replace(&format!("'{sheet_name}'!"), "Sheet1!");
            assert_eq!(text(&model, &item, "Formula"), expected, "{label}: series {} formula", n + 1);
            assert_eq!(text(&model, &item, "Name"), f[4], "{label}: series {} name", n + 1);
        }
    };
    check(0, "added");
    for (i, code) in steps.split(' ').enumerate() {
        let step = i + 1;
        let done = match code {
            "S" => invoke_kind(&model, &chart, "SetSourceData", InvokeKind::Method, vec![AutoValue::Object(range(&model, &source))]),
            "R" | "C" => invoke_kind(
                &model,
                &chart,
                "SetSourceData",
                InvokeKind::Method,
                vec![AutoValue::Object(range(&model, &source)), AutoValue::I4(if code == "R" { 1 } else { 2 })],
            ),
            "P1" | "P2" => invoke_kind(&model, &chart, "PlotBy", InvokeKind::Put, vec![AutoValue::I4(if code == "P1" { 1 } else { 2 })]),
            _ => Ok(AutoValue::Empty),
        };
        let failed = log.lines().any(|l| l.starts_with("ERR\t") && l.starts_with(&format!("ERR\t{case} step {step} {code}\t")));
        assert_eq!(done.is_err(), failed, "{case} step {step} {code}: {done:?}");
        check(step, code);
    }
}

macro_rules! q22 {
    ($($name:ident => $case:literal),* $(,)?) => {$(
        #[cfg_attr(not(target_arch = "wasm32"), test)]
        #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
        fn $name() { q22_case($case); }
    )*};
}
q22! {
    dc_piw0_11__q22_c07a_omitted_twice_on_a_header_row_over_a_chart_with_an_automatic_source => "c07a",
    dc_piw0_11__q22_c07b_omitted_after_rows_and_after_columns_keeps_the_orientation => "c07b",
    dc_piw0_11__q22_c07c_omitted_after_columns_then_rows => "c07c",
    dc_piw0_11__q22_c07d_plot_by_alone_fails_without_series_and_regroups_with_them => "c07d",
    dc_piw0_11__q22_c07e_omitted_twice_on_a_chart_without_an_automatic_source => "c07e",
    dc_piw0_11__q22_c07f_rows_columns_rows_on_a_chart_without_an_automatic_source => "c07f",
    dc_piw0_11__q22_c07g_columns_then_rows_on_a_chart_without_an_automatic_source => "c07g",
    dc_piw0_11__q22_c07h_plot_by_alone_on_a_chart_without_an_automatic_source => "c07h",
    dc_piw0_11__q22_c11a_omitted_twice_over_dates => "c11a",
    dc_piw0_11__q22_c11b_omitted_after_rows_and_after_columns_over_dates => "c11b",
    dc_piw0_11__q22_c11c_omitted_after_columns_then_rows_over_dates => "c11c",
    dc_piw0_11__q22_c11d_plot_by_alone_over_dates => "c11d",
    dc_piw0_11__q22_c11e_omitted_twice_over_dates_without_an_automatic_source => "c11e",
    dc_piw0_11__q22_c11f_rows_columns_rows_over_dates_without_an_automatic_source => "c11f",
    dc_piw0_11__q22_c11g_columns_then_rows_over_dates_without_an_automatic_source => "c11g",
    dc_piw0_11__q22_c11h_plot_by_alone_over_dates_without_an_automatic_source => "c11h",
    dc_piw0_11__q22_m1_a_non_square_source_is_plotted_by_the_rule_after_rows => "m1",
    dc_piw0_11__q22_m2_a_non_square_source_without_an_automatic_source => "m2",
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn guarded_policy_refuses_and_audits_reaching_members_once() {
    use calc_object_model::policy::{MacroSecurity, PolicyAudit, PolicyChecker};

    let (host, _) = setup();
    let level = Rc::new(Cell::new(MacroSecurity::Guarded));
    let audit = Rc::new(RefCell::new(Vec::<PolicyAudit>::new()));
    let checker = PolicyChecker::new(
        {
            let level = level.clone();
            move || level.get()
        },
        {
            let audit = audit.clone();
            move |record| audit.borrow_mut().push(record)
        },
    );
    let guarded = WorkbookAutomation::with_policy(host.clone(), checker.clone());
    guarded.context_changed(&host.session.borrow());
    let app = guarded.application();
    let run = |model: &dyn ObjectModel| {
        let member = Descriptor::get()
            .member(app.interface, "Run", InvokeKind::Method)
            .unwrap();
        model.invoke(&app, &Call {
            interface: app.interface,
            member,
            args: vec![AutoValue::text("TestCommand")],
            lcid: 0,
        })
    };

    assert!(matches!(run(&guarded), Err(AutoError::PolicyDenied { capability: "fs.read", number: 70, .. })));
    assert_eq!(host.runs.get(), 0);
    assert_eq!(audit.borrow().len(), 1);
    assert!(!audit.borrow()[0].allowed);

    let range = invoke(&guarded, &app, "Range", vec![AutoValue::text("A1")]).unwrap();
    let AutoValue::Object(range) = range else { panic!("expected a range") };
    invoke(&guarded, &range, "Value2", vec![]).unwrap();
    invoke_kind(&guarded, &app, "Calculate", InvokeKind::Method, vec![]).unwrap();
    assert_eq!(audit.borrow().len(), 1);

    level.set(MacroSecurity::Unguarded);
    assert!(matches!(run(&guarded), Ok(AutoValue::Empty)));
    assert_eq!(host.runs.get(), 1);
    assert_eq!(audit.borrow().len(), 2);
    assert!(audit.borrow()[1].allowed);
    assert_eq!(audit.borrow()[1].subject, "_Application.Run");

    let standalone = calc_object_model::profile::values_only::ValuesOnly::with_policy(
        host.clone(), checker,
    );
    level.set(MacroSecurity::Off);
    assert!(matches!(run(&standalone), Err(AutoError::PolicyDenied { .. })));
    assert_eq!(host.runs.get(), 1);
    assert_eq!(audit.borrow().len(), 3);
    assert!(!audit.borrow()[2].allowed);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn multiple_observers_receive_ordered_events_under_one_enable_events_switch() {
    struct Listener(&'static str, Rc<RefCell<Vec<(&'static str, &'static str)>>>);
    impl Listener {
        fn record(&self, event: &'static str) -> Result<(), String> {
            self.1.borrow_mut().push((self.0, event));
            Ok(())
        }
    }
    impl WorkbookEventHandler for Listener {
        fn worksheet_change(
            &self,
            _: &mut HostSession,
            _: calc_value::SheetId,
            _: &[calc_value::GridArea],
        ) -> Result<(), String> {
            self.record("Change")
        }
        fn worksheet_calculate(
            &self,
            _: &mut HostSession,
            _: calc_value::SheetId,
        ) -> Result<(), String> {
            self.record("Calculate")
        }
        fn selection_changed(
            &self,
            _: &mut HostSession,
            _: calc_value::SheetId,
            _: &[calc_value::GridArea],
        ) -> Result<(), String> {
            self.record("SelectionChanged")
        }
        fn before_save(
            &self,
            _: &mut HostSession,
            _: SaveOperation,
        ) -> Result<BeforeSaveOutcome, String> {
            self.record("BeforeSave")?;
            Ok(BeforeSaveOutcome::default())
        }
    }
    impl WorkbookEventObserver for Listener {
        fn worksheet_change(
            &self,
            _: &mut HostSession,
            _: calc_value::SheetId,
            _: &[calc_value::GridArea],
        ) -> Result<(), String> {
            self.record("Change")
        }
        fn worksheet_calculate(
            &self,
            _: &mut HostSession,
            _: calc_value::SheetId,
        ) -> Result<(), String> {
            self.record("Calculate")
        }
        fn after_calculate(&self, _: &mut HostSession) -> Result<(), String> {
            self.record("AfterCalculate")
        }
        fn selection_changed(
            &self,
            _: &mut HostSession,
            _: calc_value::SheetId,
            _: &[calc_value::GridArea],
        ) -> Result<(), String> {
            self.record("SelectionChanged")
        }
    }

    let (host, model) = setup();
    let calculate = || {
        host.session.borrow_mut().dispatch(SkinIntent::Recalculate);
        while host.session.borrow().has_pending_calculation() {
            host.session.borrow_mut().advance_calculation();
        }
    };
    calculate();
    let trace = Rc::new(RefCell::new(Vec::new()));
    let first = {
        let mut session = host.session.borrow_mut();
        session.set_workbook_event_handler(Some(Rc::new(Listener("VBA", trace.clone()))));
        let first = session.add_workbook_event_observer(Rc::new(Listener("COM", trace.clone())));
        session.add_workbook_event_observer(Rc::new(Listener("Office.js", trace.clone())));
        first
    };
    let select = |row| {
        let cell = at(&host, row, 1);
        let area = calc_value::GridArea::new(cell, cell).unwrap();
        assert!(
            host.session
                .borrow_mut()
                .dispatch(SkinIntent::Select(Selection {
                    active: Some(cell),
                    areas: vec![area],
                }))
                .accepted()
        );
    };

    enter(&host, at(&host, 30, 1), "=1+2");
    calculate();
    select(30);
    select(30);
    assert_eq!(
        &*trace.borrow(),
        &[
            ("VBA", "Change"),
            ("COM", "Change"),
            ("Office.js", "Change"),
            ("VBA", "Calculate"),
            ("COM", "Calculate"),
            ("Office.js", "Calculate"),
            ("COM", "AfterCalculate"),
            ("Office.js", "AfterCalculate"),
            ("VBA", "SelectionChanged"),
            ("COM", "SelectionChanged"),
            ("Office.js", "SelectionChanged"),
        ]
    );

    let set_events = |enabled| {
        invoke_kind(
            &model,
            &model.application(),
            "EnableEvents",
            InvokeKind::Put,
            vec![AutoValue::Bool(enabled)],
        )
        .unwrap();
    };
    trace.borrow_mut().clear();
    set_events(false);
    enter(&host, at(&host, 31, 1), "=2+3");
    calculate();
    select(31);
    assert!(trace.borrow().is_empty());

    assert!(
        host.session
            .borrow_mut()
            .remove_workbook_event_observer(first)
    );
    assert!(
        !host
            .session
            .borrow_mut()
            .remove_workbook_event_observer(first)
    );
    set_events(true);
    select(32);
    assert_eq!(
        &*trace.borrow(),
        &[
            ("VBA", "SelectionChanged"),
            ("Office.js", "SelectionChanged")
        ]
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn automation_projects_document_events_and_cancels_before_save_is_prepared() {
    struct Receiver {
        model: Rc<WorkbookAutomation>,
        trace: RefCell<Vec<(String, String)>>,
    }
    impl EventSource for Receiver {
        fn raise(
            &self,
            _: &ObjectRef,
            interface: &str,
            event: &str,
            args: &[AutoValue],
        ) -> Result<(), AutoError> {
            self.trace
                .borrow_mut()
                .push((interface.into(), event.into()));
            for value in args {
                if let AutoValue::Object(object) = value {
                    self.model.released(object);
                }
            }
            Ok(())
        }

        fn raise_with_cancel(
            &self,
            source: &ObjectRef,
            interface: &str,
            event: &str,
            args: &[AutoValue],
            cancel: &mut bool,
        ) -> Result<(), AutoError> {
            self.raise(source, interface, event, args)?;
            if interface == "WorkbookEvents" {
                assert!(!*cancel);
                assert!(matches!(args, [AutoValue::Bool(false)]));
                *cancel = true;
            } else {
                assert!(*cancel);
            }
            Ok(())
        }
    }
    let (host, model) = setup();
    let receiver = Rc::new(Receiver {
        model: model.clone(),
        trace: RefCell::default(),
    });
    {
        let mut session = host.session.borrow_mut();
        session.set_workbook_event_observer(Some(model.observer(receiver.clone())));
        session.notify_workbook_open();
        session.notify_workbook_open();
    }
    enter(&host, at(&host, 30, 1), "3");
    let mut output = Vec::new();
    assert!(matches!(
        host.session
            .borrow_mut()
            .save(&SavePolicy::default(), &mut output),
        Err(SaveFailed::Cancelled)
    ));
    assert!(output.is_empty());
    let trace = receiver.trace.borrow();
    let expected = [
        ("WorkbookEvents", "Open"),
        ("AppEvents", "WorkbookOpen"),
        ("DocEvents", "Change"),
        ("WorkbookEvents", "SheetChange"),
        ("AppEvents", "SheetChange"),
        ("WorkbookEvents", "BeforeSave"),
        ("AppEvents", "WorkbookBeforeSave"),
    ];
    assert_eq!(
        trace
            .iter()
            .map(|(i, e)| (i.as_str(), e.as_str()))
            .collect::<Vec<_>>(),
        expected
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn enable_events_is_rechecked_between_automation_event_levels() {
    struct Receiver {
        model: Rc<WorkbookAutomation>,
        disable_at: (&'static str, &'static str),
        trace: RefCell<Vec<String>>,
    }

    impl EventSource for Receiver {
        fn raise(
            &self,
            _: &ObjectRef,
            interface: &str,
            event: &str,
            args: &[AutoValue],
        ) -> Result<(), AutoError> {
            if matches!(
                event,
                "Open" | "WorkbookOpen" | "Calculate" | "SheetCalculate" | "AfterCalculate"
                    | "BeforeSave" | "WorkbookBeforeSave"
            ) {
                self.trace.borrow_mut().push(format!("{interface}.{event}"));
            }
            for value in args {
                if let AutoValue::Object(object) = value {
                    self.model.released(object);
                }
            }
            if (interface, event) == self.disable_at {
                let application = self.model.application();
                let result = invoke_kind(
                    &self.model,
                    &application,
                    "EnableEvents",
                    InvokeKind::Put,
                    vec![AutoValue::Bool(false)],
                );
                self.model.released(&application);
                result?;
            }
            Ok(())
        }

        fn raise_with_cancel(
            &self,
            source: &ObjectRef,
            interface: &str,
            event: &str,
            args: &[AutoValue],
            cancel: &mut bool,
        ) -> Result<(), AutoError> {
            if interface == "WorkbookEvents" {
                *cancel = true;
            }
            self.raise(source, interface, event, args)
        }
    }

    let cases: [(_, &[&str]); 4] = [
        (("WorkbookEvents", "Open"), &["WorkbookEvents.Open"]),
        (("DocEvents", "Calculate"), &["DocEvents.Calculate"]),
        (
            ("WorkbookEvents", "SheetCalculate"),
            &["DocEvents.Calculate", "WorkbookEvents.SheetCalculate"],
        ),
        (
            ("WorkbookEvents", "BeforeSave"),
            &["WorkbookEvents.BeforeSave"],
        ),
    ];
    let mut mismatches = Vec::new();
    for (disable_at, expected) in cases {
        let session = HostSession::open(
            Arc::new(MemorySource::new(blank_package())),
            &OpenPolicy::default(),
            HostOptions {
                calc_mode: Some(CalcMode::Manual),
                ..Default::default()
            },
        )
        .unwrap();
        let (host, model) = setup_session(session);
        let receiver = Rc::new(Receiver {
            model: model.clone(),
            disable_at,
            trace: RefCell::default(),
        });
        host.session
            .borrow_mut()
            .set_workbook_event_observer(Some(model.observer(receiver.clone())));
        match disable_at.1 {
            "Open" => host.session.borrow_mut().notify_workbook_open(),
            "BeforeSave" => {
                enter(&host, at(&host, 4, 2), "17");
                let mut output = Vec::new();
                assert!(matches!(
                    host.session.borrow_mut().save(&SavePolicy::default(), &mut output),
                    Err(SaveFailed::Cancelled)
                ));
                assert!(output.is_empty());
            }
            _ => {
                enter(&host, at(&host, 3, 2), "=1+2");
                let mut session = host.session.borrow_mut();
                session.dispatch(SkinIntent::Recalculate);
                while session.has_pending_calculation() {
                    session.advance_calculation();
                }
            }
        }
        assert!(!host.session.borrow().workbook_events_enabled(), "{disable_at:?}");
        let trace = receiver.trace.borrow();
        if trace.as_slice() != expected {
            mismatches.push(format!("{disable_at:?}: got {trace:?}, expected {expected:?}"));
        }
    }

    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

/// The Windows-generated descriptor supplies every member in the printing batch.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_8__print_members_register_where_the_descriptor_describes_them() {
    let (_, model) = setup();
    let pending = model.print_members_pending();
    let implemented = model.handlers().implemented();
    assert!(
        pending.is_empty(),
        "printing members missing from the descriptor: {pending:?}"
    );
    assert!(implemented.contains(&"PageSetup.Orientation (Put)".to_owned()));
    assert!(implemented.contains(&"HPageBreaks.Add (Method)".to_owned()));
    assert!(implemented.contains(&"VPageBreaks.Add (Method)".to_owned()));
}

/// dc-oxh8w.8: a macro sets up a sheet for printing through the members and reads it back, and the
/// same state is what the file saves.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_8__a_macro_sets_up_printing_through_members() {
    let (host, model) = setup();
    let app = model.application();
    let sheet = obj(invoke(&model, &app, "ActiveSheet", vec![]).unwrap());
    let setup = obj(invoke(&model, &sheet, "PageSetup", vec![]).unwrap());
    let put = |name: &str, value: AutoValue| {
        invoke_kind(&model, &setup, name, InvokeKind::Put, vec![value]).unwrap();
    };
    put("Orientation", AutoValue::I4(2));
    put("Zoom", AutoValue::Bool(false));
    put("FitToPagesWide", AutoValue::I4(1));
    put("FitToPagesTall", AutoValue::Bool(false));
    put("LeftMargin", AutoValue::R8(36.0));
    put("CenterHorizontally", AutoValue::Bool(true));
    put("PrintArea", AutoValue::text("$A$1:$C$20"));
    put("PrintTitleRows", AutoValue::text("$1:$1"));
    put("CenterFooter", AutoValue::text("Page &P"));
    assert!(matches!(
        invoke(&model, &setup, "Orientation", vec![]).unwrap(),
        AutoValue::I4(2)
    ));
    assert!(matches!(
        invoke(&model, &setup, "Zoom", vec![]).unwrap(),
        AutoValue::Bool(false)
    ));
    assert!(matches!(
        invoke(&model, &setup, "FitToPagesTall", vec![]).unwrap(),
        AutoValue::Bool(false)
    ));
    assert!(
        matches!(invoke(&model, &setup, "LeftMargin", vec![]).unwrap(), AutoValue::R8(v) if v == 36.0)
    );
    assert_eq!(text(&model, &setup, "PrintArea"), "$A$1:$C$20");
    assert_eq!(text(&model, &setup, "PrintTitleRows"), "$1:$1");
    assert_eq!(text(&model, &setup, "CenterFooter"), "Page &P");
    // The same state is what the Page Setup UI reads.
    let sheet_id = host.session.borrow().host().sheets()[0].id();
    let stored = host
        .session
        .borrow()
        .host()
        .model()
        .definitions()
        .page_setup(sheet_id);
    assert_eq!(stored.get("pageSetup", "orientation"), Some("landscape"));
    // Breaks.
    let h = obj(invoke(&model, &sheet, "HPageBreaks", vec![]).unwrap());
    assert!(matches!(
        invoke(&model, &h, "Count", vec![]).unwrap(),
        AutoValue::I4(0)
    ));
    let before = range(&model, "A11");
    let one = obj(invoke_kind(
        &model,
        &h,
        "Add",
        InvokeKind::Method,
        vec![AutoValue::Object(before)],
    )
    .unwrap());
    assert!(matches!(
        invoke(&model, &h, "Count", vec![]).unwrap(),
        AutoValue::I4(1)
    ));
    assert!(matches!(
        invoke(&model, &one, "Type", vec![]).unwrap(),
        AutoValue::I4(-4135)
    ));
    let location = obj(invoke(&model, &one, "Location", vec![]).unwrap());
    assert_eq!(text(&model, &location, "Address"), "$A$11");
    let v = obj(invoke(&model, &sheet, "VPageBreaks", vec![]).unwrap());
    let before = range(&model, "C1");
    invoke_kind(
        &model,
        &v,
        "Add",
        InvokeKind::Method,
        vec![AutoValue::Object(before)],
    )
    .unwrap();
    let mut saved = Vec::new();
    host.session
        .borrow_mut()
        .save(&SavePolicy::default(), &mut saved)
        .unwrap();
    let reopened = HostSession::open(
        Arc::new(MemorySource::new(saved)),
        &OpenPolicy::default(),
        HostOptions::default(),
    )
    .unwrap();
    let (_, reopened) = setup_session(reopened);
    let reopened_sheet =
        obj(invoke(&reopened, &reopened.application(), "ActiveSheet", vec![]).unwrap());
    let reopened_setup = obj(invoke(&reopened, &reopened_sheet, "PageSetup", vec![]).unwrap());
    assert!(matches!(
        invoke(&reopened, &reopened_setup, "Orientation", vec![]),
        Ok(AutoValue::I4(2))
    ));
    assert_eq!(text(&reopened, &reopened_setup, "PrintArea"), "$A$1:$C$20");
    for collection in ["HPageBreaks", "VPageBreaks"] {
        let breaks = obj(invoke(&reopened, &reopened_sheet, collection, vec![]).unwrap());
        assert!(
            matches!(
                invoke(&reopened, &breaks, "Count", vec![]),
                Ok(AutoValue::I4(1))
            ),
            "{collection}"
        );
    }
    invoke_kind(&model, &one, "Delete", InvokeKind::Method, vec![]).unwrap();
    assert!(matches!(
        invoke(&model, &h, "Count", vec![]).unwrap(),
        AutoValue::I4(0)
    ));
    invoke_kind(
        &model,
        &sheet,
        "ResetAllPageBreaks",
        InvokeKind::Method,
        vec![],
    )
    .unwrap();
    assert!(matches!(
        invoke(&model, &v, "Count", vec![]).unwrap(),
        AutoValue::I4(0)
    ));
    // Bad values are Excel's 1004 / 13.
    let bad = invoke_kind(
        &model,
        &setup,
        "Orientation",
        InvokeKind::Put,
        vec![AutoValue::I4(7)],
    );
    assert!(
        matches!(bad, Err(AutoError::Excel { number: 1004, .. })),
        "{bad:?}"
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_8_print_setters_and_add_require_one_argument() {
    let (_, model, sheet) = blank_report();
    let setup = obj(invoke(&model, &sheet, "PageSetup", vec![]).unwrap());
    let breaks = obj(invoke(&model, &sheet, "HPageBreaks", vec![]).unwrap());
    let before = AutoValue::Object(range(&model, "A10"));
    let line = obj(invoke_kind(
        &model,
        &breaks,
        "Add",
        InvokeKind::Method,
        vec![before.clone()],
    )
    .unwrap());
    let other = AutoValue::Object(range(&model, "A12"));
    let cases = [
        (
            &setup,
            "Orientation",
            InvokeKind::Put,
            vec![AutoValue::I4(1), AutoValue::I4(2)],
        ),
        (&breaks, "Add", InvokeKind::Method, vec![other, before]),
        (
            &line,
            "Type",
            InvokeKind::Put,
            vec![AutoValue::I4(-4135), AutoValue::I4(-4105)],
        ),
    ];
    let mut invalid = Vec::new();
    for (target, member, kind, extra) in cases {
        for args in [vec![], extra] {
            let reply = invoke_kind(&model, target, member, kind, args);
            if !matches!(reply, Err(AutoError::Status(status)) if status == 0x8002_000Eu32 as i32) {
                invalid.push(format!("{member}: {reply:?}"));
            }
        }
    }
    assert!(invalid.is_empty(), "{invalid:?}");
    assert!(matches!(
        invoke(&model, &setup, "Orientation", vec![]),
        Ok(AutoValue::I4(1))
    ));
    assert!(matches!(
        invoke(&model, &breaks, "Count", vec![]),
        Ok(AutoValue::I4(1))
    ));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_8_page_setup_accepts_the_user_defined_paper_code() {
    let (_, model, sheet) = blank_report();
    let setup = obj(invoke(&model, &sheet, "PageSetup", vec![]).unwrap());
    invoke_kind(
        &model,
        &setup,
        "PaperSize",
        InvokeKind::Put,
        vec![AutoValue::I4(256)],
    )
    .unwrap();
    assert!(matches!(
        invoke(&model, &setup, "PaperSize", vec![]),
        Ok(AutoValue::I4(256))
    ));
}

// ---- xlc command batch 1 (dc-oxh8w.5): the commands translate to the members tested above ----

mod xlc {
    use super::*;
    use calc_host::c_api_commands::{self as xlc, XlcError};

    /// Wraps a model and records every member call the commands make.
    struct Spy<'a> {
        inner: &'a WorkbookAutomation,
        calls: RefCell<Vec<String>>,
    }
    impl ObjectModel for Spy<'_> {
        fn application(&self) -> ObjectRef {
            self.inner.application()
        }
        fn invoke(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
            let args: Vec<String> = call
                .args
                .iter()
                .map(|a| match a {
                    AutoValue::Missing => "_".to_owned(),
                    AutoValue::Text(_) => format!("'{}'", a.as_text().unwrap()),
                    AutoValue::Object(_) => "obj".to_owned(),
                    other => format!("{other:?}"),
                })
                .collect();
            self.calls
                .borrow_mut()
                .push(format!("{} {:?}({})", call.label(), call.member.invoke, args.join(",")));
            self.inner.invoke(target, call)
        }
        fn record_refusal(&self, error: &AutoError) {
            self.inner.record_refusal(error)
        }
    }
    fn spy(model: &WorkbookAutomation) -> Spy<'_> {
        Spy {
            inner: model,
            calls: RefCell::default(),
        }
    }
    fn ordinal(symbol: &str) -> u16 {
        xlc::COMMANDS
            .iter()
            .find(|c| c.symbol == symbol)
            .unwrap_or_else(|| panic!("no command {symbol}"))
            .ordinal
    }
    fn run(model: &dyn ObjectModel, symbol: &str, operands: Vec<AutoValue>) -> Result<AutoValue, XlcError> {
        xlc::execute(model, ordinal(symbol), &operands)
    }
    fn done(result: Result<AutoValue, XlcError>) {
        assert!(matches!(result, Ok(AutoValue::Bool(true))), "{result:?}");
    }
    fn number(model: &WorkbookAutomation, address: &str) -> f64 {
        match invoke(model, &range(model, address), "Value2", vec![]).unwrap() {
            AutoValue::R8(n) => n,
            other => panic!("{address}: {other:?}"),
        }
    }
    fn value2(model: &WorkbookAutomation, address: &str, value: AutoValue) {
        invoke_kind(model, &range(model, address), "Value2", InvokeKind::Put, vec![value]).unwrap();
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn the_table_matches_the_inventory_and_the_descriptor() {
        let doc = String::from_utf8_lossy(include_bytes!("../../../docs/architecture/C_API_COMMANDS.md")).into_owned();
        let mut seen = std::collections::HashSet::new();
        for command in xlc::COMMANDS {
            assert!(seen.insert(command.ordinal), "{} twice", command.symbol);
            let row = format!(
                "| {} | `0x{:04X}` | `{}` | `{}` |",
                command.ordinal,
                0x8000 | command.ordinal,
                command.symbol,
                command.xlm
            );
            assert!(doc.contains(&row), "inventory row for {}: {row}", command.symbol);
            for member in command.member.split(',') {
                let (interface, name) = member.split_once('.').unwrap();
                let d = Descriptor::get();
                let id = d.id(interface).unwrap_or_else(|| panic!("{member}: no interface"));
                assert!(
                    [InvokeKind::Get, InvokeKind::Put, InvokeKind::Method]
                        .iter()
                        .any(|kind| d.member(d.default_interface(id), name, *kind).is_some()),
                    "{member}"
                );
            }
            assert_eq!(xlc::command_for_callback(0x8000 | u32::from(command.ordinal)), Some(command));
        }
        assert_eq!(xlc::command_for_callback(u32::from(ordinal("xlcFormula"))), None, "no command bit");
        assert_eq!(xlc::command_for_callback(0x8000 | 31 | 0x4000), None);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn formula_reads_its_text_in_r1c1_and_calculate_now_calculates() {
        let (_host, model) = setup();
        value2(&model, "A1", AutoValue::R8(5.0));
        done(run(&*model, "xlcFormula", vec![AutoValue::text("=RC[-1]*2"), AutoValue::text("R1C2")]));
        assert_eq!(text(&model, &range(&model, "B1"), "Formula"), "=A1*2");
        assert_eq!(text(&model, &range(&model, "B1"), "FormulaR1C1"), "=RC[-1]*2");
        done(run(&*model, "xlcCalculateNow", vec![]));
        assert_eq!(number(&model, "B1"), 10.0);
        // A constant is entered as typed, whatever its type.
        done(run(&*model, "xlcFormula", vec![AutoValue::R8(7.0), AutoValue::text("R1C3")]));
        assert_eq!(number(&model, "C1"), 7.0);
        // An omitted reference is the selection (the active cell, here A1).
        done(run(&*model, "xlcFormula", vec![AutoValue::text("=R[1]C+1")]));
        assert_eq!(text(&model, &range(&model, "A1"), "Formula"), "=A2+1");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn formula_fill_enters_the_relative_formula_in_every_cell() {
        let (_host, model) = setup();
        value2(&model, "A1", AutoValue::R8(1.0));
        value2(&model, "A2", AutoValue::R8(2.0));
        let object = AutoValue::Object(range(&model, "B1:B2"));
        done(run(&*model, "xlcFormulaFill", vec![AutoValue::text("=RC[-1]+1"), object]));
        assert_eq!(text(&model, &range(&model, "B1"), "Formula"), "=A1+1");
        assert_eq!(text(&model, &range(&model, "B2"), "Formula"), "=A2+1");
        done(run(&*model, "xlcCalculateDocument", vec![]));
        assert_eq!((number(&model, "B1"), number(&model, "B2")), (2.0, 3.0));
        // FORMULA over several cells is not documented; it is refused, not guessed.
        let several = AutoValue::text("R1C4:R2C4");
        assert_eq!(
            run(&*model, "xlcFormula", vec![AutoValue::text("=1"), several]).unwrap_err(),
            XlcError::NotModelled { what: "FORMULA over more than one cell", owner: "dc-oxh8w.9" }
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn echo_message_and_run_reach_the_application() {
        let (host, model) = setup();
        let app = model.application();
        assert!(matches!(invoke(&model, &app, "ScreenUpdating", vec![]), Ok(AutoValue::Bool(true))));
        done(run(&*model, "xlcEcho", vec![AutoValue::Bool(false)]));
        assert!(matches!(invoke(&model, &app, "ScreenUpdating", vec![]), Ok(AutoValue::Bool(false))));
        done(run(&*model, "xlcEcho", vec![AutoValue::R8(1.0)]));
        assert!(matches!(invoke(&model, &app, "ScreenUpdating", vec![]), Ok(AutoValue::Bool(true))));
        assert!(matches!(run(&*model, "xlcEcho", vec![]), Err(XlcError::NotModelled { owner: "dc-oxh8w.9", .. })));
        assert!(matches!(run(&*model, "xlcEcho", vec![AutoValue::text("x")]), Err(XlcError::Operand(_))));

        assert!(matches!(invoke(&model, &app, "StatusBar", vec![]), Ok(AutoValue::Bool(false))));
        done(run(&*model, "xlcMessage", vec![AutoValue::Bool(true), AutoValue::text("Working")]));
        assert_eq!(text(&model, &app, "StatusBar"), "Working");
        done(run(&*model, "xlcMessage", vec![AutoValue::R8(0.0), AutoValue::text("ignored")]));
        assert!(matches!(invoke(&model, &app, "StatusBar", vec![]), Ok(AutoValue::Bool(false))));
        done(run(&*model, "xlcMessage", vec![AutoValue::Bool(true)]));
        assert!(matches!(invoke(&model, &app, "StatusBar", vec![]), Ok(AutoValue::Bool(false))));

        done(run(&*model, "xlcRun", vec![AutoValue::text("TestCommand")]));
        assert_eq!(host.runs.get(), 1);
        assert!(matches!(
            run(&*model, "xlcRun", vec![AutoValue::Object(app.clone())]),
            Err(XlcError::NotModelled { owner: "dc-oxh8w.9", .. })
        ));
        assert!(matches!(run(&*model, "xlcRun", vec![AutoValue::text("Nope")]), Err(XlcError::Model(_))));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn clear_contents_acts_on_the_selection() {
        let (_host, model) = setup();
        value2(&model, "A1", AutoValue::R8(5.0));
        done(run(&*model, "xlcClear", vec![AutoValue::R8(3.0)]));
        assert!(matches!(invoke(&model, &range(&model, "A1"), "Value2", vec![]), Ok(AutoValue::Empty)));
        assert!(matches!(run(&*model, "xlcClear", vec![]), Err(XlcError::NotModelled { owner: "dc-oxh8w.9", .. })));
        assert!(matches!(run(&*model, "xlcClear", vec![AutoValue::R8(9.0)]), Err(XlcError::Operand(_))));
    }

    /// Each command calls the member Excel's object model has; a member this profile has not
    /// implemented refuses typed, and the command passes the refusal through.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn commands_call_the_members_and_pass_refusals_through() {
        let (_host, model) = setup();
        let text = AutoValue::text;
        let cases: Vec<(&str, Vec<AutoValue>, &[&str])> = vec![
            ("xlcSelect", vec![text("R2C3:R4C5")], &["_Application.Range Get('C2:E4')", "Range.Select Method()"]),
            ("xlcClear", vec![AutoValue::R8(1.0)], &["_Application.Selection Get()", "Range.Clear Method()"]),
            ("xlcClear", vec![AutoValue::R8(2.0)], &["_Application.Selection Get()", "Range.ClearFormats Method()"]),
            ("xlcClear", vec![AutoValue::R8(4.0)], &["_Application.Selection Get()", "Range.ClearComments Method()"]),
            ("xlcFormatNumber", vec![text("0.00")], &["_Application.Selection Get()", "Range.NumberFormat Put('0.00')"]),
            ("xlcColumnWidth", vec![AutoValue::R8(12.0), text("R1C2")], &["_Application.Range Get('B1')", "Range.ColumnWidth Put(R8(12.0))"]),
            ("xlcRowHeight", vec![AutoValue::R8(30.0)], &["_Application.Selection Get()", "Range.RowHeight Put(R8(30.0))"]),
            ("xlcInsert", vec![AutoValue::R8(1.0)], &["_Application.Selection Get()", "Range.Insert Method(I4(-4161))"]),
            ("xlcInsert", vec![AutoValue::R8(2.0)], &["_Application.Selection Get()", "Range.Insert Method(I4(-4121))"]),
            ("xlcInsert", vec![AutoValue::R8(3.0)], &["_Application.Selection Get()", "Range.EntireRow Get()", "Range.Insert Method()"]),
            ("xlcInsert", vec![AutoValue::R8(4.0)], &["_Application.Selection Get()", "Range.EntireColumn Get()", "Range.Insert Method()"]),
            ("xlcEditDelete", vec![AutoValue::R8(1.0)], &["_Application.Selection Get()", "Range.Delete Method(I4(-4159))"]),
            ("xlcEditDelete", vec![AutoValue::R8(2.0)], &["_Application.Selection Get()", "Range.Delete Method(I4(-4162))"]),
            ("xlcEditDelete", vec![AutoValue::R8(4.0)], &["_Application.Selection Get()", "Range.EntireColumn Get()", "Range.Delete Method()"]),
            ("xlcCopy", vec![text("R1C1")], &["_Application.Range Get('A1')", "Range.Copy Method()"]),
            ("xlcCopy", vec![text("R1C1"), text("R3C3")], &["_Application.Range Get('A1')", "_Application.Range Get('C3')", "Range.Copy Method(obj)"]),
            ("xlcCut", vec![text("R1C1"), text("R3C3")], &["_Application.Range Get('A1')", "_Application.Range Get('C3')", "Range.Cut Method(obj)"]),
            ("xlcPaste", vec![text("R5C1")], &["_Application.ActiveSheet Get()", "_Application.Range Get('A5')", "_Worksheet.Paste Method(obj)"]),
            ("xlcPaste", vec![], &["_Application.ActiveSheet Get()", "_Worksheet.Paste Method()"]),
            ("xlcDefineName", vec![text("Total"), text("=R1C1:R3C1")], &["_Application.Names Get()", "Names.Add Method('Total',_,_,_,_,_,_,_,_,'=R1C1:R3C1')"]),
            ("xlcDeleteName", vec![text("Total")], &["_Application.Names Get()", "Names.Item Method('Total')"]),
            ("xlcSave", vec![], &["_Application.ActiveWorkbook Get()", "_Workbook.Save Method()"]),
        ];
        for (symbol, operands, expected) in cases {
            let spy = spy(&model);
            let result = run(&spy, symbol, operands);
            let calls = spy.calls.borrow();
            // A call after a refused one is never made: the sequence ends at the first failure.
            assert!(calls.len() <= expected.len() && calls.iter().zip(expected).all(|(a, b)| a == b), "{symbol}: {calls:?}");
            match result {
                Ok(_) => assert_eq!(calls.len(), expected.len(), "{symbol}: {calls:?}"),
                Err(XlcError::Model(error)) => {
                    assert!(calls.len() <= expected.len(), "{symbol}");
                    assert!(matches!(error, AutoError::NotModelled { .. } | AutoError::Excel { .. } | AutoError::Status(_)), "{symbol}: {error:?}");
                }
                Err(other) => panic!("{symbol}: {other:?} after {calls:?}"),
            }
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn operands_that_cannot_be_read_are_refused_before_any_member_call() {
        let (_host, model) = setup();
        let spy = spy(&model);
        assert_eq!(xlc::execute(&spy, 9999, &[]).unwrap_err(), XlcError::NotServed(9999));
        assert_eq!(
            run(&spy, "xlcCalculateNow", vec![AutoValue::Bool(true)]).unwrap_err(),
            XlcError::Operand("too many operands")
        );
        // A trailing omitted operand is as if absent.
        assert!(run(&spy, "xlcCalculateNow", vec![AutoValue::Missing]).is_ok());
        for (symbol, operands) in [
            ("xlcSelect", vec![AutoValue::text("R1C1[1]")]),
            ("xlcSelect", vec![AutoValue::text("Sheet1!R1C1")]),
            ("xlcSelect", vec![AutoValue::text("A1")]),
            ("xlcSelect", vec![AutoValue::text("R0C1")]),
            ("xlcInsert", vec![AutoValue::R8(7.0)]),
            ("xlcInsert", vec![AutoValue::text("x")]),
            ("xlcFormatNumber", vec![AutoValue::R8(1.0)]),
            ("xlcDefineName", vec![AutoValue::R8(1.0), AutoValue::text("=R1C1")]),
        ] {
            let before = spy.calls.borrow().len();
            let result = run(&spy, symbol, operands.clone());
            assert!(matches!(result, Err(XlcError::Operand(_) | XlcError::NotModelled { .. })), "{symbol} {operands:?}: {result:?}");
            assert_eq!(spy.calls.borrow().len(), before, "{symbol} made a member call");
        }
        assert!(matches!(run(&spy, "xlcSelect", vec![]), Err(XlcError::NotModelled { owner: "dc-oxh8w.9", .. })));
        assert!(matches!(run(&spy, "xlcDefineName", vec![AutoValue::text("N")]), Err(XlcError::NotModelled { .. })));
    }
}

fn sheet_names(host: &Host) -> Vec<String> {
    host.session
        .borrow()
        .host()
        .model()
        .sheets()
        .iter()
        .map(|s| s.name().text().to_string_lossy())
        .collect()
}
fn worksheets(model: &WorkbookAutomation) -> ObjectRef {
    let book = obj(invoke(model, &model.application(), "ActiveWorkbook", vec![]).unwrap());
    obj(invoke(model, &book, "Worksheets", vec![]).unwrap())
}

/// dc-oxh8w.4: Worksheets.Add, Worksheet.Name put and Worksheet.Delete are one member each in calc-host.
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn dc_oxh8w_4__worksheets_add_rename_delete() {
    let (host, model) = setup();
    let sheets = worksheets(&model);
    let before = sheet_names(&host);
    let first = before[0].clone();
    // Add with no arguments: before the active sheet, named SheetN, and it becomes active.
    let added = obj(invoke_kind(&model, &sheets, "Add", InvokeKind::Method, vec![]).unwrap());
    let after = sheet_names(&host);
    assert_eq!(after.len(), before.len() + 1);
    assert_eq!(after[1..], before[..], "inserted before the active (first) sheet");
    assert!(after[0].starts_with("Sheet"), "{after:?}");
    assert_eq!(text(&model, &added, "Name"), after[0]);
    let active = obj(invoke(&model, &model.application(), "ActiveSheet", vec![]).unwrap());
    assert_eq!(text(&model, &active, "Name"), after[0]);
    // Add(After:=name) places after that sheet.
    let tail = invoke_kind(
        &model,
        &sheets,
        "Add",
        InvokeKind::Method,
        vec![
            AutoValue::Missing,
            invoke(&model, &sheets, "Item", vec![AutoValue::text(&first)]).unwrap(),
        ],
    )
    .unwrap();
    let names = sheet_names(&host);
    assert_eq!(names[2], text(&model, &obj(tail), "Name"));
    assert_eq!(names[1], first);
    // Count above one and chart sheets are refused typed, nothing changes.
    for args in [
        vec![AutoValue::Missing, AutoValue::Missing, AutoValue::I4(2)],
        vec![AutoValue::Missing, AutoValue::Missing, AutoValue::Missing, AutoValue::I4(-4109)],
    ] {
        let n = sheet_names(&host).len();
        assert!(matches!(
            invoke_kind(&model, &sheets, "Add", InvokeKind::Method, args),
            Err(AutoError::NotModelled { .. })
        ));
        assert_eq!(sheet_names(&host).len(), n);
    }
    // Name put renames; a duplicate (any case) is Excel's 1004.
    invoke_kind(&model, &added, "Name", InvokeKind::Put, vec![AutoValue::text("Data")]).unwrap();
    assert_eq!(sheet_names(&host)[0], "Data");
    let error = invoke_kind(&model, &added, "Name", InvokeKind::Put, vec![AutoValue::text(&first.to_uppercase())])
        .unwrap_err();
    assert!(matches!(error, AutoError::Excel { number: 1004, .. }), "{error:?}");
    // Delete removes the sheet.
    let count = sheet_names(&host).len();
    invoke_kind(&model, &added, "Delete", InvokeKind::Method, vec![]).unwrap();
    assert_eq!(sheet_names(&host).len(), count - 1);
    assert!(!sheet_names(&host).contains(&"Data".to_string()));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
fn dc_oxh8w_4__worksheet_move_and_the_last_sheet_stays() {
    let (host, model) = setup();
    let sheets = worksheets(&model);
    let item = |n: i32| obj(invoke(&model, &sheets, "Item", vec![AutoValue::I4(n)]).unwrap());
    for _ in 0..2 {
        invoke_kind(&model, &sheets, "Add", InvokeKind::Method, vec![]).unwrap();
    }
    let names = sheet_names(&host);
    assert!(names.len() >= 3);
    // Move the first sheet after the last: it is last, the others close up.
    let last = item(names.len() as i32);
    invoke_kind(&model, &item(1), "Move", InvokeKind::Method, vec![AutoValue::Missing, AutoValue::Object(last)]).unwrap();
    let moved = sheet_names(&host);
    assert_eq!(moved.last(), names.first());
    assert_eq!(moved[..moved.len() - 1], names[1..]);
    // Move the (new) last sheet before the first.
    invoke_kind(&model, &item(moved.len() as i32), "Move", InvokeKind::Method, vec![AutoValue::Object(item(1))]).unwrap();
    assert_eq!(sheet_names(&host), names);
    // Deleting down to one sheet works; deleting the last one is Excel's 1004.
    while sheet_names(&host).len() > 1 {
        invoke_kind(&model, &item(1), "Delete", InvokeKind::Method, vec![]).unwrap();
    }
    let error = invoke_kind(&model, &item(1), "Delete", InvokeKind::Method, vec![]).unwrap_err();
    assert!(matches!(error, AutoError::Excel { number: 1004, .. }), "{error:?}");
    assert_eq!(sheet_names(&host).len(), 1);
}

fn cell_text(model: &WorkbookAutomation, address: &str) -> AutoValue {
    invoke(model, &range(model, address), "Value2", vec![]).unwrap()
}

/// dc-oxh8w.4: Range.Insert and Range.Delete, with Shift given and omitted, and on whole rows and columns.
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn dc_oxh8w_4__range_insert_and_delete_shift_cells_and_references() {
    let (host, model) = setup_session(
        HostSession::open(
            Arc::new(MemorySource::new(blank_package())),
            &OpenPolicy::default(),
            HostOptions {
                calc_mode: Some(CalcMode::Automatic),
                ..Default::default()
            },
        )
        .unwrap(),
    );
    let sheet = host.session.borrow().host().sheets()[0].id();
    let name = sheet_names(&host)[0].clone();
    let mut put = |address: &str, value: AutoValue| {
        invoke_kind(&model, &range(&model, address), "Value2", InvokeKind::Put, vec![value]).unwrap();
    };
    for (row, value) in [(1, 1.), (2, 2.), (3, 3.), (4, 4.)] {
        put(&format!("Z{row}"), AutoValue::R8(value));
    }
    let sum = format!("=SUM(Z1:Z4)");
    enter(&host, CellKey::new(sheet, 6, 26).unwrap(), &sum);
    let _ = &name;
    let call = |address: &str, member: &str, args: Vec<AutoValue>| {
        invoke_kind(&model, &range(&model, address), member, InvokeKind::Method, args)
    };
    // A one-column range with Shift:=xlShiftDown pushes Z2.. down by its height; the SUM follows.
    call("Z2", "Insert", vec![AutoValue::I4(-4121)]).unwrap();
    assert!(matches!(cell_text(&model, "Z2"), AutoValue::Empty));
    assert!(matches!(cell_text(&model, "Z3"), AutoValue::R8(2.)));
    assert!(matches!(cell_text(&model, "Z5"), AutoValue::R8(4.)));
    // Delete with Shift omitted on a single cell shifts up and brings it back.
    call("Z2", "Delete", vec![]).unwrap();
    assert!(matches!(cell_text(&model, "Z2"), AutoValue::R8(2.)));
    // A taller-than-wide range with Shift omitted shifts cells to the right / left.
    put("A20", AutoValue::R8(7.));
    put("B20", AutoValue::R8(8.));
    call("A20:A21", "Insert", vec![]).unwrap();
    assert!(matches!(cell_text(&model, "A20"), AutoValue::Empty));
    assert!(matches!(cell_text(&model, "B20"), AutoValue::R8(7.)));
    assert!(matches!(cell_text(&model, "C20"), AutoValue::R8(8.)));
    call("A20:A21", "Delete", vec![]).unwrap();
    assert!(matches!(cell_text(&model, "A20"), AutoValue::R8(7.)));
    // A Shift constant for the other operation is Excel's 1004.
    let error = call("A20", "Insert", vec![AutoValue::I4(-4162)]).unwrap_err();
    assert!(matches!(error, AutoError::Excel { number: 1004, .. }), "{error:?}");
    // Whole rows ignore Shift and shift the sheet's rows.
    put("A30", AutoValue::R8(9.));
    call("30:31", "Insert", vec![]).unwrap();
    assert!(matches!(cell_text(&model, "A32"), AutoValue::R8(9.)));
    call("30:31", "Delete", vec![]).unwrap();
    assert!(matches!(cell_text(&model, "A30"), AutoValue::R8(9.)));
    // Whole columns likewise.
    put("AB1", AutoValue::R8(5.));
    call("AB:AC", "Insert", vec![]).unwrap();
    assert!(matches!(cell_text(&model, "AD1"), AutoValue::R8(5.)));
    call("AB:AC", "Delete", vec![]).unwrap();
    assert!(matches!(cell_text(&model, "AB1"), AutoValue::R8(5.)));
    // CopyOrigin from the right or below is not modelled yet and changes nothing.
    assert!(matches!(
        call("A20", "Insert", vec![AutoValue::I4(-4121), AutoValue::I4(1)]),
        Err(AutoError::NotModelled { .. })
    ));
    assert!(matches!(cell_text(&model, "A20"), AutoValue::R8(7.)));
}

/// dc-oxh8w.4: Names.Add defines a workbook name, redefining an existing one, and returns the Name.
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn dc_oxh8w_4__names_add_defines_and_redefines_a_workbook_name() {
    let (host, model) = setup_session(
        HostSession::open(
            Arc::new(MemorySource::new(blank_package())),
            &OpenPolicy::default(),
            HostOptions::default(),
        )
        .unwrap(),
    );
    let book = obj(invoke(&model, &model.application(), "ActiveWorkbook", vec![]).unwrap());
    let names = obj(invoke(&model, &book, "Names", vec![]).unwrap());
    let sheet = sheet_names(&host)[0].clone();
    let add = |name: &str, refers: &str| {
        invoke_kind(
            &model,
            &names,
            "Add",
            InvokeKind::Method,
            vec![AutoValue::text(name), AutoValue::text(refers)],
        )
    };
    let name = obj(add("Totals", &format!("={sheet}!$B$2:$B$4")).unwrap());
    assert_eq!(text(&model, &name, "Name"), "Totals");
    assert_eq!(text(&model, &name, "RefersTo"), format!("={sheet}!$B$2:$B$4"));
    assert!(matches!(invoke(&model, &names, "Count", vec![]).unwrap(), AutoValue::I4(1)));
    // Redefining keeps one name.
    assert!(matches!(add("totals", "=1"), Err(AutoError::NotModelled { .. })));
    let again = obj(add("Totals", &format!("={sheet}!$C$1")).unwrap());
    assert_eq!(text(&model, &again, "RefersTo"), format!("={sheet}!$C$1"));
    assert!(matches!(invoke(&model, &names, "Count", vec![]).unwrap(), AutoValue::I4(1)));
    // An invalid name is Excel's 1004; sheet-scoped names and non-formula RefersTo are typed refusals.
    let error = add("1bad name", "=1").unwrap_err();
    assert!(matches!(error, AutoError::Excel { number: 1004, .. }), "{error:?}");
    assert!(matches!(add(&format!("{sheet}!Local"), "=1"), Err(AutoError::NotModelled { .. })));
    assert!(matches!(add("Plain", "text"), Err(AutoError::NotModelled { .. })));
    assert!(matches!(invoke(&model, &names, "Count", vec![]).unwrap(), AutoValue::I4(1)));
}

/// dc-oxh8w.4: Range.Copy and Range.Cut with a Destination relocate formulas; a cut moves references.
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn dc_oxh8w_4__range_copy_and_cut_to_a_destination() {
    let (host, model) = setup_session(
        HostSession::open(
            Arc::new(MemorySource::new(blank_package())),
            &OpenPolicy::default(),
            HostOptions {
                calc_mode: Some(CalcMode::Automatic),
                ..Default::default()
            },
        )
        .unwrap(),
    );
    let sheet = host.session.borrow().host().sheets()[0].id();
    let put = |address: &str, value: AutoValue| {
        invoke_kind(&model, &range(&model, address), "Value2", InvokeKind::Put, vec![value]).unwrap();
    };
    put("A1", AutoValue::R8(2.));
    put("A2", AutoValue::R8(3.));
    enter(&host, CellKey::new(sheet, 1, 2).unwrap(), "=A1*10");
    enter(&host, CellKey::new(sheet, 5, 5).unwrap(), "=A1+1");
    let call = |from: &str, member: &str, args: Vec<AutoValue>| {
        invoke_kind(&model, &range(&model, from), member, InvokeKind::Method, args)
    };
    // Copy to a single destination cell takes the source's shape; the formula relocates.
    call("A1:B2", "Copy", vec![AutoValue::Object(range(&model, "D10"))]).unwrap();
    assert!(matches!(cell_text(&model, "D10"), AutoValue::R8(2.)));
    assert!(matches!(cell_text(&model, "D11"), AutoValue::R8(3.)));
    assert!(matches!(cell_text(&model, "E10"), AutoValue::R8(20.)));
    // Cut moves the cells; the formula that read A1 follows it.
    call("A1", "Cut", vec![AutoValue::Object(range(&model, "H1"))]).unwrap();
    assert!(matches!(cell_text(&model, "A1"), AutoValue::Empty));
    assert!(matches!(cell_text(&model, "H1"), AutoValue::R8(2.)));
    assert!(matches!(cell_text(&model, "E5"), AutoValue::R8(3.)));
    // No destination means the clipboard, which is refused typed.
    assert!(matches!(call("A2", "Copy", vec![]), Err(AutoError::NotModelled { .. })));
}

// ---- chart members batch 2 (dc-oxh8w.11): axes through the object model ----

/// A macro scales the value axis of a chart it built; the chart saves with the scale, reads it back, and the
/// category axis refuses the scale members as Excel does (1004).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_11__a_macro_scales_the_value_axis_and_the_chart_saves_it() {
    let (host, model, sheet) = blank_report();
    fill(&host, (1, 1), &[&["Region", "Q1"], &["North", "10"], &["South", "30"], &["East", "50"]]);
    let charts = obj(invoke_kind(&model, &sheet, "ChartObjects", InvokeKind::Method, vec![]).unwrap());
    let frame = obj(
        invoke_kind(
            &model,
            &charts,
            "Add",
            InvokeKind::Method,
            vec![AutoValue::R8(200.0), AutoValue::R8(20.0), AutoValue::R8(300.0), AutoValue::R8(200.0)],
        )
        .unwrap(),
    );
    let chart = obj(invoke(&model, &frame, "Chart", vec![]).unwrap());
    set_source(&model, &chart, "A1:B4");
    let axis = |kind: i32| invoke_kind(&model, &chart, "Axes", InvokeKind::Method, vec![AutoValue::I4(kind)]);
    let value = obj(axis(2).unwrap());
    let category = obj(axis(1).unwrap());
    assert!(matches!(invoke(&model, &value, "Type", vec![]).unwrap(), AutoValue::I4(2)));
    assert!(matches!(invoke(&model, &value, "MaximumScaleIsAuto", vec![]).unwrap(), AutoValue::Bool(true)));
    let put = |name: &str, v: AutoValue| invoke_kind(&model, &value, name, InvokeKind::Put, vec![v]);
    // Automatic scale for 10, 30, 50 reads as the axis the chart draws (dc-oxh8w.13).
    for (name, expected) in [("MinimumScale", 0.0), ("MaximumScale", 60.0), ("MajorUnit", 10.0)] {
        let read = invoke(&model, &value, name, vec![]);
        assert!(matches!(read, Ok(AutoValue::R8(v)) if v == expected), "{name}: {read:?}");
    }
    // False from automatic freezes the shown value.
    put("MajorUnitIsAuto", AutoValue::Bool(false)).unwrap();
    assert!(matches!(invoke(&model, &value, "MajorUnitIsAuto", vec![]).unwrap(), AutoValue::Bool(false)));
    assert!(matches!(invoke(&model, &value, "MajorUnit", vec![]).unwrap(), AutoValue::R8(v) if v == 10.0));
    put("MajorUnitIsAuto", AutoValue::Bool(true)).unwrap();
    put("MinimumScale", AutoValue::R8(0.0)).unwrap();
    put("MaximumScale", AutoValue::I4(100)).unwrap();
    put("MajorUnit", AutoValue::R8(25.0)).unwrap();
    put("ReversePlotOrder", AutoValue::Bool(true)).unwrap();
    assert!(matches!(invoke(&model, &value, "MaximumScale", vec![]).unwrap(), AutoValue::R8(v) if v == 100.0));
    assert!(matches!(invoke(&model, &value, "MajorUnit", vec![]).unwrap(), AutoValue::R8(v) if v == 25.0));
    assert!(matches!(invoke(&model, &value, "MaximumScaleIsAuto", vec![]).unwrap(), AutoValue::Bool(false)));
    assert!(matches!(invoke(&model, &value, "ReversePlotOrder", vec![]).unwrap(), AutoValue::Bool(true)));
    // A scale back to automatic.
    put("MaximumScaleIsAuto", AutoValue::Bool(true)).unwrap();
    assert!(matches!(invoke(&model, &value, "MaximumScaleIsAuto", vec![]).unwrap(), AutoValue::Bool(true)));
    put("MaximumScale", AutoValue::R8(80.0)).unwrap();
    // The category axis has no scale.
    let bad = invoke_kind(&model, &category, "MinimumScale", InvokeKind::Put, vec![AutoValue::R8(1.0)]);
    assert!(matches!(bad, Err(AutoError::Excel { number: 1004, .. })), "{bad:?}");
    let bad = invoke(&model, &category, "MaximumScale", vec![]);
    assert!(matches!(bad, Err(AutoError::Excel { number: 1004, .. })), "{bad:?}");
    // Saved.
    let xml = saved_part(&mut host.session.borrow_mut(), "xl/charts/")
        .replace(" xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\"", "");
    assert!(xml.contains("<c:max val=\"80\"/>"), "{xml}");
    assert!(xml.contains("<c:min val=\"0\"/>"), "{xml}");
    assert!(xml.contains("<c:majorUnit val=\"25\"/>"), "{xml}");
    assert!(xml.contains("<c:orientation val=\"maxMin\"/>"), "{xml}");
}

/// dc-oxh8w.12: axis titles and tick-label number formats set by a macro are saved in the chart part.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_12__a_macro_titles_the_axes_and_formats_tick_labels() {
    let (host, model, sheet) = blank_report();
    fill(&host, (1, 1), &[&["Region", "Q1"], &["North", "10"], &["South", "30"], &["East", "50"]]);
    let charts = obj(invoke_kind(&model, &sheet, "ChartObjects", InvokeKind::Method, vec![]).unwrap());
    let frame = obj(
        invoke_kind(
            &model,
            &charts,
            "Add",
            InvokeKind::Method,
            vec![AutoValue::R8(200.0), AutoValue::R8(20.0), AutoValue::R8(300.0), AutoValue::R8(200.0)],
        )
        .unwrap(),
    );
    let chart = obj(invoke(&model, &frame, "Chart", vec![]).unwrap());
    set_source(&model, &chart, "A1:B4");
    let axis = |kind: i32| obj(invoke_kind(&model, &chart, "Axes", InvokeKind::Method, vec![AutoValue::I4(kind)]).unwrap());
    let (category, value) = (axis(1), axis(2));
    assert!(matches!(invoke(&model, &value, "HasTitle", vec![]).unwrap(), AutoValue::Bool(false)));
    let none = invoke(&model, &value, "AxisTitle", vec![]);
    assert!(matches!(none, Err(AutoError::Excel { .. })), "{none:?}");
    invoke_kind(&model, &value, "HasTitle", InvokeKind::Put, vec![AutoValue::Bool(true)]).unwrap();
    let title = obj(invoke(&model, &value, "AxisTitle", vec![]).unwrap());
    assert_eq!(text(&model, &title, "Text"), "Axis Title");
    invoke_kind(&model, &title, "Text", InvokeKind::Put, vec![AutoValue::text("Sales & cost")]).unwrap();
    let cat_title = {
        invoke_kind(&model, &category, "HasTitle", InvokeKind::Put, vec![AutoValue::Bool(true)]).unwrap();
        obj(invoke(&model, &category, "AxisTitle", vec![]).unwrap())
    };
    invoke_kind(&model, &cat_title, "Caption", InvokeKind::Put, vec![AutoValue::text("Region")]).unwrap();
    let xml = saved_part(&mut host.session.borrow_mut(), "xl/charts/")
        .replace(" xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\"", "");
    assert!(xml.contains("Sales &amp; cost"), "{xml}");
    assert!(xml.contains("<a:t>Region</a:t>") || xml.contains("Region</a:t>"), "{xml}");
    // Titles off again.
    invoke_kind(&model, &value, "HasTitle", InvokeKind::Put, vec![AutoValue::Bool(false)]).unwrap();
    assert!(matches!(invoke(&model, &value, "HasTitle", vec![]).unwrap(), AutoValue::Bool(false)));
}

/// dc-oxh8w.12: `TickLabels.NumberFormat` (runs once the descriptor holds TickLabels, RUN-ON-WINDOWS).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_12__tick_label_number_format_through_the_object_model() {
    if calc_object_model::Descriptor::get().id("TickLabels").is_none() {
        eprintln!("dc-oxh8w.12: descriptor lacks TickLabels; member round trip not run");
        return;
    }
    let (host, model, sheet) = blank_report();
    fill(&host, (1, 1), &[&["Region", "Q1"], &["North", "10"], &["South", "30"]]);
    let charts = obj(invoke_kind(&model, &sheet, "ChartObjects", InvokeKind::Method, vec![]).unwrap());
    let frame = obj(invoke_kind(&model, &charts, "Add", InvokeKind::Method, vec![AutoValue::R8(200.0), AutoValue::R8(20.0), AutoValue::R8(300.0), AutoValue::R8(200.0)]).unwrap());
    let chart = obj(invoke(&model, &frame, "Chart", vec![]).unwrap());
    set_source(&model, &chart, "A1:B3");
    let value = obj(invoke_kind(&model, &chart, "Axes", InvokeKind::Method, vec![AutoValue::I4(2)]).unwrap());
    let labels = obj(invoke(&model, &value, "TickLabels", vec![]).unwrap());
    assert_eq!(text(&model, &labels, "NumberFormat"), "General");
    invoke_kind(&model, &labels, "NumberFormat", InvokeKind::Put, vec![AutoValue::text("0.00")]).unwrap();
    assert_eq!(text(&model, &labels, "NumberFormat"), "0.00");
    assert!(matches!(invoke(&model, &labels, "NumberFormatLinked", vec![]).unwrap(), AutoValue::Bool(false)));
    let xml = saved_part(&mut host.session.borrow_mut(), "xl/charts/");
    assert!(xml.contains("formatCode=\"0.00\""), "{xml}");
}

/// dc-oxh8w.4: Range.Sort with keys as ranges, Order and Header; unmodelled options refuse typed.
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn dc_oxh8w_4__range_sort_orders_rows_by_key_columns() {
    let (host, model) = setup_session(
        HostSession::open(
            Arc::new(MemorySource::new(blank_package())),
            &OpenPolicy::default(),
            HostOptions::default(),
        )
        .unwrap(),
    );
    let _ = &host;
    let put = |address: &str, value: AutoValue| {
        invoke_kind(&model, &range(&model, address), "Value2", InvokeKind::Put, vec![value]).unwrap();
    };
    for (row, (key, tag)) in [(3., "c"), (1., "a"), (2., "b")].into_iter().enumerate() {
        put(&format!("A{}", row + 2), AutoValue::R8(key));
        put(&format!("B{}", row + 2), AutoValue::text(tag));
    }
    put("A1", AutoValue::text("key"));
    put("B1", AutoValue::text("tag"));
    let sort = |args: Vec<AutoValue>| {
        invoke_kind(&model, &range(&model, "A1:B4"), "Sort", InvokeKind::Method, args)
    };
    let key = |address: &str| AutoValue::Object(range(&model, address));
    // Ascending by A with a header row (xlYes = 1) in the eighth argument.
    let mut args = vec![key("A1"), AutoValue::I4(1)];
    args.extend(std::iter::repeat_with(|| AutoValue::Missing).take(5));
    args.push(AutoValue::I4(1));
    sort(args.clone()).unwrap();
    assert!(matches!(cell_text(&model, "A2"), AutoValue::R8(1.)));
    assert!(matches!(cell_text(&model, "B2"), AutoValue::Text(_)));
    assert_eq!(cell_text(&model, "B2").as_text().unwrap(), "a");
    assert!(matches!(cell_text(&model, "A4"), AutoValue::R8(3.)));
    assert_eq!(cell_text(&model, "A1").as_text().unwrap(), "key");
    // Descending (xlDescending = 2) by the second column.
    args[0] = key("B1");
    args[1] = AutoValue::I4(2);
    sort(args.clone()).unwrap();
    assert_eq!(cell_text(&model, "B2").as_text().unwrap(), "c");
    assert!(matches!(cell_text(&model, "A2"), AutoValue::R8(3.)));
    // Sorting columns and xlGuess refuse typed and change nothing.
    let mut sideways = args.clone();
    sideways.push(AutoValue::Missing);
    sideways.push(AutoValue::Missing);
    sideways.push(AutoValue::I4(1));
    assert!(matches!(sort(sideways), Err(AutoError::NotModelled { .. })));
    args[7] = AutoValue::I4(0);
    assert!(matches!(sort(args), Err(AutoError::NotModelled { .. })));
    assert_eq!(cell_text(&model, "B2").as_text().unwrap(), "c");
}
