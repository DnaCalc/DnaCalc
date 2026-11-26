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
    files: RefCell<Vec<(String, Vec<u8>)>>,
}
impl ApplicationHost for Host {
    fn write_file(&self, path: &str, bytes: &[u8]) -> Result<(), AutoError> {
        self.files.borrow_mut().push((path.to_owned(), bytes.to_vec()));
        Ok(())
    }
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
        files: RefCell::new(Vec::new()),
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
fn dc_oxh8w_2_column_width_uses_excel_character_units() {
    let (host, model) = setup();
    let captured: serde_json::Value = serde_json::from_str(
        include_str!("fixtures/dc_09v/excel-shown.json").trim_start_matches('\u{feff}'),
    )
    .unwrap();
    let a = range(&model, "A1");
    let b = range(&model, "B3");
    let both = range(&model, "A1:B2");
    for (cell, key) in [(&a, "A"), (&b, "B")] {
        assert!(
            matches!(invoke(&model, cell, "ColumnWidth", vec![]).unwrap(),
            AutoValue::R8(n) if n == captured["widths"][key].as_f64().unwrap())
        );
    }
    assert!(matches!(
        invoke(&model, &both, "ColumnWidth", vec![]).unwrap(),
        AutoValue::Null
    ));
    assert_eq!(
        excel_error(invoke_kind(
            &model,
            &both,
            "ColumnWidth",
            InvokeKind::Put,
            vec![]
        ))
        .0,
        1004
    );
    assert_eq!(
        excel_error(invoke_kind(
            &model,
            &both,
            "ColumnWidth",
            InvokeKind::Put,
            vec![AutoValue::I4(256)]
        ))
        .0,
        1004
    );
    invoke_kind(
        &model,
        &both,
        "ColumnWidth",
        InvokeKind::Put,
        vec![AutoValue::I4(16)],
    )
    .unwrap();
    assert!(matches!(
        invoke(&model, &b, "ColumnWidth", vec![]).unwrap(),
        AutoValue::R8(16.0)
    ));
    assert!(
        host.session
            .borrow_mut()
            .dispatch(SkinIntent::Undo)
            .accepted()
    );
    assert!(matches!(
        invoke(&model, &both, "ColumnWidth", vec![]).unwrap(),
        AutoValue::Null
    ));
    invoke_kind(
        &model,
        &b,
        "ColumnWidth",
        InvokeKind::Put,
        vec![AutoValue::I4(0)],
    )
    .unwrap();
    let reopened = reopen_formatting(&host);
    assert!(matches!(
        invoke(&reopened, &range(&reopened, "B:B"), "ColumnWidth", vec![]).unwrap(),
        AutoValue::R8(0.0)
    ));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_2_row_height_uses_excel_points() {
    let (host, model) = setup();
    let captured: serde_json::Value = serde_json::from_str(
        include_str!("fixtures/dc_09v/excel-shown.json").trim_start_matches('\u{feff}'),
    )
    .unwrap();
    for (address, key) in [("B1", "h1"), ("D3", "h3")] {
        assert!(
            matches!(invoke(&model, &range(&model, address), "RowHeight", vec![]).unwrap(),
            AutoValue::R8(n) if n == captured["rows"][key].as_f64().unwrap())
        );
    }
    let report = range(&model, "A3:C4");
    assert_eq!(
        excel_error(invoke(&model, &report, "RowHeight", vec![AutoValue::I4(1)])).0,
        1004
    );
    assert_eq!(
        excel_error(invoke_kind(
            &model,
            &report,
            "RowHeight",
            InvokeKind::Put,
            vec![AutoValue::I4(-1)]
        ))
        .0,
        1004
    );
    invoke_kind(
        &model,
        &report,
        "RowHeight",
        InvokeKind::Put,
        vec![AutoValue::R8(22.5)],
    )
    .unwrap();
    let reopened = reopen_formatting(&host);
    assert!(matches!(
        invoke(&reopened, &range(&reopened, "D3:D4"), "RowHeight", vec![]).unwrap(),
        AutoValue::R8(22.5)
    ));
    assert!(matches!(
        invoke(&reopened, &range(&reopened, "D1"), "RowHeight", vec![]).unwrap(),
        AutoValue::R8(15.0)
    ));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_2_indent_level_uses_format_cells() {
    let (host, model, _) = blank_report();
    let cells = range(&model, "B1:B2");
    let cell = range(&model, "B1");
    invoke_kind(
        &model,
        &cells,
        "HorizontalAlignment",
        InvokeKind::Put,
        vec![AutoValue::I4(-4131)],
    )
    .unwrap();
    assert!(matches!(
        invoke(&model, &cells, "IndentLevel", vec![]).unwrap(),
        AutoValue::I4(0)
    ));
    invoke_kind(
        &model,
        &cell,
        "IndentLevel",
        InvokeKind::Put,
        vec![AutoValue::I4(15)],
    )
    .unwrap();
    assert!(matches!(
        invoke(&model, &cells, "IndentLevel", vec![]).unwrap(),
        AutoValue::Null
    ));
    assert_eq!(
        excel_error(invoke_kind(
            &model,
            &cell,
            "IndentLevel",
            InvokeKind::Put,
            vec![AutoValue::I4(16)]
        ))
        .0,
        1004
    );
    let reopened = reopen_formatting(&host);
    let cell = range(&reopened, "B1");
    assert!(matches!(
        invoke(&reopened, &cell, "IndentLevel", vec![]).unwrap(),
        AutoValue::I4(15)
    ));
    assert!(matches!(
        invoke(&reopened, &cell, "HorizontalAlignment", vec![]).unwrap(),
        AutoValue::I4(-4131)
    ));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_2_orientation_preserves_signed_rotation() {
    let (host, model, _) = blank_report();
    let cells = range(&model, "B1:B2");
    for (address, degrees) in [("B1", -35), ("B2", 44)] {
        invoke_kind(
            &model,
            &range(&model, address),
            "Orientation",
            InvokeKind::Put,
            vec![AutoValue::I4(degrees)],
        )
        .unwrap();
    }
    assert!(matches!(
        invoke(&model, &cells, "Orientation", vec![]).unwrap(),
        AutoValue::Null
    ));
    assert_eq!(
        excel_error(invoke_kind(
            &model,
            &cells,
            "Orientation",
            InvokeKind::Put,
            vec![AutoValue::I4(91)]
        ))
        .0,
        1004
    );
    assert_eq!(
        excel_error(invoke_kind(
            &model,
            &cells,
            "Orientation",
            InvokeKind::Put,
            vec![AutoValue::R8(10.5)]
        ))
        .0,
        1004
    );
    let reopened = reopen_formatting(&host);
    for (address, degrees) in [("B1", -35), ("B2", 44)] {
        assert!(
            matches!(invoke(&reopened, &range(&reopened, address), "Orientation", vec![]).unwrap(),
            AutoValue::I4(n) if n == degrees)
        );
    }
}

fn reopen_formatting(host: &Host) -> Rc<WorkbookAutomation> {
    let mut saved = Vec::new();
    host.session
        .borrow_mut()
        .save(&SavePolicy::default(), &mut saved)
        .unwrap();
    let session = HostSession::open(
        Arc::new(MemorySource::new(saved)),
        &OpenPolicy::default(),
        HostOptions::default(),
    )
    .unwrap();
    setup_session(session).1
}

fn font(model: &WorkbookAutomation, address: &str) -> ObjectRef {
    let target = range(model, address);
    let font = obj(invoke(model, &target, "Font", vec![]).unwrap());
    model.released(&target);
    font
}

fn put_font(model: &WorkbookAutomation, font: &ObjectRef, member: &str, value: AutoValue) -> Reply {
    invoke_kind(model, font, member, InvokeKind::Put, vec![value])
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_2_font_bold_applies_to_the_held_range() {
    let (host, model, _) = blank_report();
    let first = font(&model, "D1");
    assert!(matches!(
        invoke(&model, &first, "Bold", vec![]).unwrap(),
        AutoValue::Bool(false)
    ));
    put_font(&model, &first, "Bold", AutoValue::Bool(true)).unwrap();
    let mixed = font(&model, "D1:D2");
    assert!(matches!(
        invoke(&model, &mixed, "Bold", vec![]).unwrap(),
        AutoValue::Null
    ));
    let reopened = reopen_formatting(&host);
    assert!(matches!(
        invoke(&reopened, &font(&reopened, "D1"), "Bold", vec![]).unwrap(),
        AutoValue::Bool(true)
    ));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_2_font_italic_preserves_bold() {
    let (host, model, _) = blank_report();
    let cells = font(&model, "A1:B3");
    put_font(&model, &cells, "Bold", AutoValue::Bool(true)).unwrap();
    put_font(&model, &cells, "Italic", AutoValue::Bool(true)).unwrap();
    let reopened = reopen_formatting(&host);
    let cells = font(&reopened, "A1:B3");
    for member in ["Bold", "Italic"] {
        assert!(matches!(
            invoke(&reopened, &cells, member, vec![]).unwrap(),
            AutoValue::Bool(true)
        ));
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_2_font_underline_uses_the_excel_enum() {
    let (host, model, _) = blank_report();
    let cell = font(&model, "C3");
    assert!(matches!(
        invoke(&model, &cell, "Underline", vec![]).unwrap(),
        AutoValue::I4(-4142)
    ));
    assert_eq!(
        excel_error(put_font(&model, &cell, "Underline", AutoValue::I4(17))).0,
        1004
    );
    put_font(&model, &cell, "Underline", AutoValue::I4(-4119)).unwrap();
    let reopened = reopen_formatting(&host);
    let cell = font(&reopened, "C3");
    assert!(matches!(
        invoke(&reopened, &cell, "Underline", vec![]).unwrap(),
        AutoValue::I4(-4119)
    ));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_2_font_size_uses_points() {
    let (host, model, _) = blank_report();
    let cells = font(&model, "B2:B3");
    assert_eq!(
        excel_error(put_font(&model, &cells, "Size", AutoValue::R8(0.0))).0,
        1004
    );
    put_font(&model, &cells, "Size", AutoValue::R8(14.5)).unwrap();
    let reopened = reopen_formatting(&host);
    assert!(matches!(
        invoke(&reopened, &font(&reopened, "B2:B3"), "Size", vec![]).unwrap(),
        AutoValue::R8(14.5)
    ));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_2_font_name_survives_reopen() {
    let (host, model, _) = blank_report();
    let cells = font(&model, "B2:B3");
    assert_eq!(
        excel_error(invoke(&model, &cells, "Name", vec![AutoValue::I4(0)])).0,
        1004
    );
    put_font(&model, &cells, "Name", AutoValue::text("Courier New")).unwrap();
    let reopened = reopen_formatting(&host);
    assert_eq!(
        text(&reopened, &font(&reopened, "B2:B3"), "Name"),
        "Courier New"
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_2_font_color_uses_excel_rgb_order() {
    let (host, model, _) = blank_report();
    let cell = font(&model, "B2");
    let rgb = 0x0033_2211;
    put_font(&model, &cell, "Color", AutoValue::I4(rgb)).unwrap();
    let reopened = reopen_formatting(&host);
    assert!(
        matches!(invoke(&reopened, &font(&reopened, "B2"), "Color", vec![]).unwrap(), AutoValue::I4(n) if n == rgb)
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_2_font_color_index_preserves_palette_identity() {
    let (host, model, _) = blank_report();
    let cell = font(&model, "B2");
    assert!(matches!(
        invoke(&model, &cell, "ColorIndex", vec![]).unwrap(),
        AutoValue::I4(-4105)
    ));
    assert_eq!(
        excel_error(put_font(&model, &cell, "ColorIndex", AutoValue::I4(57))).0,
        1004
    );
    put_font(&model, &cell, "ColorIndex", AutoValue::I4(3)).unwrap();
    let reopened = reopen_formatting(&host);
    let cell = font(&reopened, "B2");
    assert!(matches!(
        invoke(&reopened, &cell, "ColorIndex", vec![]).unwrap(),
        AutoValue::I4(3)
    ));
    assert!(matches!(
        invoke(&reopened, &cell, "Color", vec![]).unwrap(),
        AutoValue::I4(255)
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
            .member(app.interface, "Interactive", InvokeKind::Get)
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
            owner: "dc-oxh8w",
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
    // The listing now includes the sheet-local name (one more than the global ones defined above).
    let listed = invoke(&model, &names, "Count", vec![]).unwrap();
    assert!(matches!(listed, AutoValue::I4(n) if n >= 2), "{listed:?}");
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
            owner: "dc-zjsg",
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
    fn xlc_text_reference_uses_active_cell_and_quoted_sheet() {
        let (host, model) = setup();
        assert!(
            host.session
                .borrow_mut()
                .dispatch(SkinIntent::Command(WorkbookCommand::Sheet {
                    sheet: None,
                    change: SheetChange::Add { name: "O'Brien ! data".into() },
                }))
                .accepted()
        );
        select_cells(&host, (5, 4), (5, 4));
        model.context_changed(&host.session.borrow());
        let cases = [
            ("R[-2]C[1]", "E3", "relative"),
            ("rC2", "B5", "mixed"),
            ("'O''Brien ! data'!R[1]C[-1]", "'O''Brien ! data'!C6", "qualified"),
        ];
        for (r1c1, a1, value) in cases {
            done(run(&*model, "xlcFormula", vec![AutoValue::text(value), AutoValue::text(r1c1)]));
            assert_eq!(text(&model, &range(&model, a1), "Value2"), value);
        }
        for bad in ["R[-5]C", "R0C1", "R1C1[1]", "A1", "R1C1+R2C2"] {
            assert_eq!(
                run(&*model, "xlcFormula", vec![AutoValue::I4(111), AutoValue::text(bad)]).unwrap_err(),
                XlcError::Operand("text reference"),
                "{bad}",
            );
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn xlc_workbook_name_renames_the_requested_sheet() {
        let (host, model) = setup();
        let old = sheet_names(&host)[0].clone();
        done(xlc::execute(&*model, 386, &[AutoValue::text(&old), AutoValue::text("Report")]));
        assert_eq!(sheet_names(&host)[0], "Report");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn xlc_paste_special_transposes_values_without_copying_formulas() {
        let (host, model) = setup();
        enter(&host, at(&host, 30, 1), "=6*7");
        enter(&host, at(&host, 30, 2), "43");
        done(run(&*model, "xlcCalculateNow", vec![]));
        done(run(&*model, "xlcCopy", vec![AutoValue::text("R30C1:R30C2")]));
        select_cells(&host, (31, 3), (31, 3));
        model.context_changed(&host.session.borrow());
        done(xlc::execute(
            &*model,
            53,
            &[AutoValue::I4(3), AutoValue::I4(1), AutoValue::Bool(false), AutoValue::Bool(true)],
        ));
        assert_eq!(number(&model, "C31"), 42.0);
        assert_eq!(number(&model, "C32"), 43.0);
        assert_eq!(text(&model, &range(&model, "C31"), "Formula"), "42");
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
        let (host, model) = setup();
        value2(&model, "A1", AutoValue::R8(1.0));
        value2(&model, "A2", AutoValue::R8(2.0));
        let object = AutoValue::Object(range(&model, "B1:B2"));
        done(run(&*model, "xlcFormulaFill", vec![AutoValue::text("=RC[-1]+1"), object]));
        assert_eq!(text(&model, &range(&model, "B1"), "Formula"), "=A1+1");
        assert_eq!(text(&model, &range(&model, "B2"), "Formula"), "=A2+1");
        done(run(&*model, "xlcCalculateDocument", vec![]));
        assert_eq!((number(&model, "B1"), number(&model, "B2")), (2.0, 3.0));
        // C2: FORMULA writes only the upper-left cell, with an explicit or selected reference.
        value2(&model, "D2", AutoValue::R8(21.0));
        let several = AutoValue::text("R1C4:R2C4");
        done(run(&*model, "xlcFormula", vec![AutoValue::text("=RC[-1]+1"), several]));
        assert_eq!(text(&model, &range(&model, "D1"), "Formula"), "=C1+1");
        assert_eq!(number(&model, "D2"), 21.0);

        select_cells(&host, (1, 4), (2, 4));
        model.context_changed(&host.session.borrow());
        done(run(&*model, "xlcFormula", vec![AutoValue::text("=RC[-1]+2")]));
        assert_eq!(text(&model, &range(&model, "D1"), "Formula"), "=C1+2");
        assert_eq!(number(&model, "D2"), 21.0);

        select_cells(&host, (2, 4), (1, 4));
        model.context_changed(&host.session.borrow());
        done(run(&*model, "xlcFormula", vec![AutoValue::text("=RC[-1]+3")]));
        assert_eq!(text(&model, &range(&model, "D1"), "Formula"), "=C1+2");
        assert_eq!(text(&model, &range(&model, "D2"), "Formula"), "=C2+3");
    }

    /// Application.Build is the build the captures record; Application.Calculation reads and
    /// sets the workbook's calculation mode (dc-oxh8w.31).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn application_build_and_calculation_mode() {
        let (host, model) = setup();
        let app = model.application();
        assert!(matches!(invoke(&model, &app, "Build", vec![]), Ok(AutoValue::I4(20527))));
        let err = invoke(&model, &app, "Build", vec![AutoValue::I4(1)]).unwrap_err();
        assert_eq!(err.number(), 1004);
        // The fixture opens in manual calculation.
        assert!(matches!(invoke(&model, &app, "Calculation", vec![]), Ok(AutoValue::I4(-4135))));
        let put = |value: AutoValue| invoke_kind(&model, &app, "Calculation", InvokeKind::Put, vec![value]);
        assert!(put(AutoValue::I4(-4105)).is_ok());
        assert_eq!(host.session.borrow().calc_mode(), CalcMode::Automatic);
        assert!(matches!(invoke(&model, &app, "Calculation", vec![]), Ok(AutoValue::I4(-4105))));
        assert!(put(AutoValue::I4(2)).is_ok());
        assert_eq!(host.session.borrow().calc_mode(), CalcMode::AutomaticExceptTables);
        assert!(matches!(invoke(&model, &app, "Calculation", vec![]), Ok(AutoValue::I4(2))));
        assert!(put(AutoValue::I4(-4135)).is_ok());
        assert_eq!(host.session.borrow().calc_mode(), CalcMode::Manual);
        // An invalid enumerator and a wrong argument count are Excel errors, not refusals.
        assert_eq!(put(AutoValue::I4(7)).unwrap_err().number(), 1004);
        assert_eq!(
            invoke_kind(&model, &app, "Calculation", InvokeKind::Put, vec![]).unwrap_err().number(),
            1004
        );
        assert_eq!(invoke(&model, &app, "Calculation", vec![AutoValue::I4(1)]).unwrap_err().number(), 1004);
        assert_eq!(host.session.borrow().calc_mode(), CalcMode::Manual);
    }

    /// Application.CalculationState is xlPending (2) while a recalculation waits and xlDone (0)
    /// after it; Application.MultiThreadedCalculation answers Enabled, ThreadMode and
    /// ThreadCount, and puts outside Excel's ranges are 1004 (dc-oxh8w.40).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn application_calculation_state_and_multithreaded_calculation() {
        let (_host, model) = setup();
        let app = model.application();
        // The fixture's file caches wait for a first calculation in manual mode.
        assert!(invoke_kind(&model, &app, "Calculate", InvokeKind::Method, vec![]).is_ok());
        assert!(matches!(invoke(&model, &app, "CalculationState", vec![]), Ok(AutoValue::I4(0))));
        assert_eq!(invoke(&model, &app, "CalculationState", vec![AutoValue::I4(1)]).unwrap_err().number(), 1004);
        // An edit in manual mode leaves formulas waiting: xlPending until Calculate.
        let a1 = range(&model, "A1");
        assert!(invoke_kind(&model, &a1, "Value", InvokeKind::Put, vec![AutoValue::R8(41.0)]).is_ok());
        assert!(matches!(invoke(&model, &app, "CalculationState", vec![]), Ok(AutoValue::I4(2))));
        assert!(invoke_kind(&model, &app, "Calculate", InvokeKind::Method, vec![]).is_ok());
        assert!(matches!(invoke(&model, &app, "CalculationState", vec![]), Ok(AutoValue::I4(0))));
        let mtc = obj(invoke(&model, &app, "MultiThreadedCalculation", vec![]).unwrap());
        assert!(matches!(invoke(&model, &mtc, "Enabled", vec![]), Ok(AutoValue::Bool(true))));
        assert!(matches!(invoke(&model, &mtc, "ThreadMode", vec![]), Ok(AutoValue::I4(0))));
        assert!(matches!(invoke(&model, &mtc, "ThreadCount", vec![]), Ok(AutoValue::I4(n)) if n >= 1));
        let put = |name: &str, value: AutoValue| invoke_kind(&model, &mtc, name, InvokeKind::Put, vec![value]);
        assert!(put("Enabled", AutoValue::Bool(false)).is_ok());
        assert!(matches!(invoke(&model, &mtc, "Enabled", vec![]), Ok(AutoValue::Bool(false))));
        // The state belongs to the application: a second acquisition sees it.
        let again = obj(invoke(&model, &app, "MultiThreadedCalculation", vec![]).unwrap());
        assert!(matches!(invoke(&model, &again, "Enabled", vec![]), Ok(AutoValue::Bool(false))));
        assert!(put("ThreadMode", AutoValue::I4(1)).is_ok());
        assert!(put("ThreadCount", AutoValue::I4(3)).is_ok());
        assert!(matches!(invoke(&model, &mtc, "ThreadMode", vec![]), Ok(AutoValue::I4(1))));
        assert!(matches!(invoke(&model, &mtc, "ThreadCount", vec![]), Ok(AutoValue::I4(3))));
        for (name, bad) in [
            ("ThreadMode", AutoValue::I4(2)),
            ("ThreadCount", AutoValue::I4(0)),
            ("ThreadCount", AutoValue::I4(1025)),
        ] {
            assert_eq!(put(name, bad).unwrap_err().number(), 1004, "{name}");
        }
        assert_eq!(invoke(&model, &mtc, "Enabled", vec![AutoValue::I4(1)]).unwrap_err().number(), 1004);
    }

    /// The settings read-out the corpus probes print (dc-oxh8w.39): CalculationVersion and
    /// OperatingSystem as the captures record, CalculateBeforeSave from the file, the iteration
    /// settings get and put, Workbook.Date1904 from the loaded date system.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn application_calculation_settings() {
        let (host, model) = setup();
        let app = model.application();
        assert!(matches!(invoke(&model, &app, "CalculationVersion", vec![]), Ok(AutoValue::I4(191029))));
        assert_eq!(text(&model, &app, "OperatingSystem"), "Windows (64-bit) NT 10.00");
        for name in ["CalculationVersion", "OperatingSystem", "CalculateBeforeSave", "Iteration", "MaxIterations", "MaxChange"] {
            let err = invoke(&model, &app, name, vec![AutoValue::I4(1)]).unwrap_err();
            assert_eq!(err.number(), 1004, "{name} with an argument");
        }
        assert!(matches!(invoke(&model, &app, "CalculateBeforeSave", vec![]), Ok(AutoValue::Bool(true))));
        // The fixture states no iteration settings: Excel's defaults.
        assert!(matches!(invoke(&model, &app, "Iteration", vec![]), Ok(AutoValue::Bool(false))));
        assert!(matches!(invoke(&model, &app, "MaxIterations", vec![]), Ok(AutoValue::I4(100))));
        assert!(matches!(invoke(&model, &app, "MaxChange", vec![]), Ok(AutoValue::R8(v)) if v == 0.001));
        let put = |name: &str, value: AutoValue| invoke_kind(&model, &app, name, InvokeKind::Put, vec![value]);
        assert!(put("Iteration", AutoValue::Bool(true)).is_ok());
        assert!(put("MaxIterations", AutoValue::I4(7)).is_ok());
        assert!(put("MaxChange", AutoValue::R8(0.5)).is_ok());
        let settings = host.session.borrow().host().model().definitions().iteration();
        assert_eq!((settings.enabled(), settings.max_iterations(), settings.max_change()), (true, 7, 0.5));
        assert!(matches!(invoke(&model, &app, "Iteration", vec![]), Ok(AutoValue::Bool(true))));
        assert!(matches!(invoke(&model, &app, "MaxIterations", vec![]), Ok(AutoValue::I4(7))));
        assert!(matches!(invoke(&model, &app, "MaxChange", vec![]), Ok(AutoValue::R8(v)) if v == 0.5));
        // Each put keeps the other two settings.
        assert!(put("Iteration", AutoValue::Bool(false)).is_ok());
        assert!(matches!(invoke(&model, &app, "MaxIterations", vec![]), Ok(AutoValue::I4(7))));
        // Out-of-range values and a wrong argument count are Excel errors.
        for (name, bad) in [
            ("MaxIterations", AutoValue::I4(0)),
            ("MaxIterations", AutoValue::I4(32768)),
            ("MaxChange", AutoValue::R8(-1.0)),
        ] {
            assert_eq!(put(name, bad).unwrap_err().number(), 1004, "{name}");
        }
        // A worksheet error value is a type mismatch (13), not a refusal.
        assert!(matches!(put("Iteration", AutoValue::Error(2015)), Err(AutoError::Status(_))));
        assert_eq!(
            invoke_kind(&model, &app, "MaxChange", InvokeKind::Put, vec![]).unwrap_err().number(),
            1004
        );
        let settings = host.session.borrow().host().model().definitions().iteration();
        assert_eq!((settings.max_iterations(), settings.max_change()), (7, 0.5));
        // Workbook.Date1904 reads the loaded date system.
        let book = obj(invoke(&model, &app, "ActiveWorkbook", vec![]).unwrap());
        assert!(matches!(invoke(&model, &book, "Date1904", vec![]), Ok(AutoValue::Bool(false))));
        assert_eq!(invoke(&model, &book, "Date1904", vec![AutoValue::I4(1)]).unwrap_err().number(), 1004);
        assert!(matches!(invoke(&model, &book, "PrecisionAsDisplayed", vec![]), Ok(AutoValue::Bool(false))));
        assert_eq!(invoke(&model, &book, "PrecisionAsDisplayed", vec![AutoValue::I4(1)]).unwrap_err().number(), 1004);
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
        for expected in [false, true] {
            done(run(&*model, "xlcEcho", vec![]));
            assert!(matches!(
                invoke(&model, &app, "ScreenUpdating", vec![]),
                Ok(AutoValue::Bool(value)) if value == expected
            ));
        }
        assert!(matches!(run(&*model, "xlcEcho", vec![AutoValue::text("x")]), Err(XlcError::Operand(_))));

        assert!(matches!(invoke(&model, &app, "StatusBar", vec![]), Ok(AutoValue::Bool(false))));
        done(run(&*model, "xlcMessage", vec![AutoValue::Bool(true), AutoValue::text("Working")]));
        assert_eq!(text(&model, &app, "StatusBar"), "Working");
        done(run(&*model, "xlcMessage", vec![AutoValue::R8(0.0), AutoValue::text("ignored")]));
        assert!(matches!(invoke(&model, &app, "StatusBar", vec![]), Ok(AutoValue::Bool(false))));
        // The Windows C2 capture cleared an existing message when the text operand was omitted.
        done(run(&*model, "xlcMessage", vec![AutoValue::Bool(true), AutoValue::text("Working")]));
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
        let selected = range(&model, "A1");
        value2(&model, "A1", AutoValue::R8(6.0));
        done(run(&*model, "xlcFormatNumber", vec![AutoValue::text("0.00")]));
        done(run(&*model, "xlcClear", vec![]));
        assert!(matches!(invoke(&model, &selected, "Value2", vec![]), Ok(AutoValue::Empty)));
        assert_eq!(text(&model, &selected, "NumberFormat"), "0.00");
        assert!(matches!(run(&*model, "xlcClear", vec![AutoValue::R8(9.0)]), Err(XlcError::Operand(_))));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn omitted_edit_delete_shifts_the_horizontal_selection_up_as_in_c2() {
        let (host, model, _) = blank_report();
        for (address, value) in [("B2", 22.0), ("C2", 23.0), ("B3", 32.0), ("C3", 33.0)] {
            value2(&model, address, AutoValue::R8(value));
        }
        select_cells(&host, (2, 2), (2, 3));
        model.context_changed(&host.session.borrow());
        let app = model.application();
        let selected = obj(invoke(&model, &app, "Selection", vec![]).unwrap());
        let active = obj(invoke(&model, &app, "ActiveCell", vec![]).unwrap());
        assert_eq!(text(&model, &selected, "Address"), "$B$2:$C$2");
        assert_eq!(text(&model, &active, "Address"), "$B$2");
        done(run(&*model, "xlcEditDelete", vec![]));
        assert_eq!((number(&model, "B2"), number(&model, "C2")), (32.0, 33.0));
        for address in ["B3", "C3"] {
            assert!(matches!(
                invoke(&model, &range(&model, address), "Value2", vec![]),
                Ok(AutoValue::Empty)
            ));
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn xlc_workbook_insert_adds_and_activates_a_worksheet_before_the_active_sheet() {
        let (host, model) = setup();
        let before = sheet_names(&host);
        done(xlc::execute(&*model, 354, &[AutoValue::I4(1)]));
        let after = sheet_names(&host);
        assert_eq!(&after[1..], before.as_slice());
        let active = obj(invoke(&model, &model.application(), "ActiveSheet", vec![]).unwrap());
        assert_eq!(text(&model, &active, "Name"), after[0]);
        let invalid = xlc::execute(&*model, 354, &[AutoValue::R8(1.25)]);
        assert!(matches!(invalid, Err(XlcError::Model(AutoError::Excel { number: 1004, .. }))), "{invalid:?}");
        assert_eq!(sheet_names(&host), after);
        done(xlc::execute(&*model, 354, &[]));
        assert_eq!(&sheet_names(&host)[1..], after.as_slice());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn xlc_set_print_area_sets_a_range_and_clears_with_empty_text() {
        let (_, model) = setup();
        let sheet = obj(invoke(&model, &model.application(), "ActiveSheet", vec![]).unwrap());
        let page = obj(invoke(&model, &sheet, "PageSetup", vec![]).unwrap());
        let print_range = AutoValue::Object(range(&model, "B2:C3"));
        done(xlc::execute(&*model, 22, &[print_range]));
        assert_eq!(text(&model, &page, "PrintArea"), "$B$2:$C$3");
        done(xlc::execute(&*model, 22, &[AutoValue::text("R5C4:R6C5")]));
        assert_eq!(text(&model, &page, "PrintArea"), "$D$5:$E$6");
        done(xlc::execute(&*model, 22, &[AutoValue::text("")]));
        assert_eq!(text(&model, &page, "PrintArea"), "");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn documented_optional_operands_refuse_before_dispatch() {
        let (_, model) = setup();
        let spy = spy(&model);
        let text = AutoValue::text;
        let i4 = AutoValue::I4;
        let mut wrong = Vec::new();
        for (symbol, mut operands) in [
            ("xlcRun", vec![text("TestCommand"), i4(1)]),
            (
                "xlcColumnWidth",
                vec![i4(12), text("R1C2"), i4(0), i4(3), i4(10)],
            ),
            ("xlcInsert", vec![i4(3), i4(1)]),
            (
                "xlcDefineName",
                vec![
                    text("Total"),
                    text("=R1C1"),
                    i4(1),
                    i4(1),
                    i4(1),
                    i4(1),
                    i4(1),
                ],
            ),
            ("xlcSelect", vec![text("R1C1:R3C3"), text("R2C2")]),
            ("xlcRowHeight", vec![i4(20), text("R2C1"), i4(0), i4(3)]),
        ] {
            let result = run(&spy, symbol, operands.clone());
            if !matches!(
                result,
                Err(XlcError::NotModelled {
                    owner: "dc-oxh8w.9",
                    ..
                })
            ) {
                wrong.push(format!("{symbol}: {result:?}"));
            }
            operands.push(i4(1));
            assert_eq!(
                run(&spy, symbol, operands).unwrap_err(),
                XlcError::Operand("too many operands")
            );
        }
        assert!(wrong.is_empty(), "{wrong:?}");
        assert!(spy.calls.borrow().is_empty());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn formula_commands_accept_i2_constants() {
        let (_, model) = setup();
        done(run(
            &*model,
            "xlcFormula",
            vec![AutoValue::I2(-17), AutoValue::text("R1C3")],
        ));
        done(run(
            &*model,
            "xlcFormulaFill",
            vec![AutoValue::I2(21), AutoValue::text("R2C3:R3C3")],
        ));
        assert_eq!(number(&model, "C1").to_bits(), (-17.0_f64).to_bits());
        assert_eq!(number(&model, "C2").to_bits(), 21.0_f64.to_bits());
        assert_eq!(number(&model, "C3").to_bits(), 21.0_f64.to_bits());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn clear_and_shift_require_exact_enum_values() {
        let (_, model) = setup();
        let spy = spy(&model);
        let mut wrong = Vec::new();
        value2(&model, "A1", AutoValue::R8(5.0));
        for (symbol, operand) in [
            ("xlcClear", AutoValue::R8(3.9)),
            ("xlcInsert", AutoValue::R4(3.9)),
            ("xlcEditDelete", AutoValue::R8(2.1)),
            ("xlcInsert", AutoValue::R8(f64::NAN)),
            ("xlcClear", AutoValue::R8(f64::INFINITY)),
        ] {
            let result = run(&spy, symbol, vec![operand]);
            if !matches!(result, Err(XlcError::Operand("type_num" | "shift_num"))) {
                wrong.push(format!("{symbol}: {result:?}"));
            }
        }
        assert!(wrong.is_empty(), "{wrong:?}");
        assert!(spy.calls.borrow().is_empty());
        assert_eq!(number(&model, "A1"), 5.0);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn implicit_relative_coordinates_and_whole_axes_translate_to_a1() {
        let (host, model) = setup();
        select_cells(&host, (5, 4), (5, 4));
        model.context_changed(&host.session.borrow());
        let spy = spy(&model);
        for (operand, a1) in [
            ("RC", "D5"), ("R", "5:5"), ("c", "D:D"), ("R1C", "D1"), ("rC1", "A5"),
            ("R1C1:RC", "A1:D5"), ("RC1:R2C2", "A5:B2"), ("R1:R", "1:5"), ("C2:c", "B:D"),
        ] {
            spy.calls.borrow_mut().clear();
            // Select moves the active cell the next operand is relative to: start each from D5.
            select_cells(&host, (5, 4), (5, 4));
            model.context_changed(&host.session.borrow());
            let _ = run(&spy, "xlcSelect", vec![AutoValue::text(operand)]);
            let expected = format!("_Application.Range Get('{a1}')");
            assert!(spy.calls.borrow().contains(&expected), "{operand}");
        }
        spy.calls.borrow_mut().clear();
        assert_eq!(
            run(&spy, "xlcSelect", vec![AutoValue::text("R0C")]).unwrap_err(),
            XlcError::Operand("text reference"),
        );
        assert!(spy.calls.borrow().is_empty());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn status_bar_refuses_unmeasured_coercion_without_changing_text() {
        let (_, model) = setup();
        let app = model.application();
        let put = |value| invoke_kind(&model, &app, "StatusBar", InvokeKind::Put, vec![value]);
        put(AutoValue::text("Working")).unwrap();
        for value in [AutoValue::Bool(true), AutoValue::I4(1), AutoValue::Null] {
            assert!(matches!(
                put(value),
                Err(AutoError::NotModelled {
                    owner: "dc-oxh8w.9",
                    ..
                })
            ));
            assert_eq!(text(&model, &app, "StatusBar"), "Working");
        }
        put(AutoValue::Bool(false)).unwrap();
        assert!(matches!(
            invoke(&model, &app, "StatusBar", vec![]),
            Ok(AutoValue::Bool(false))
        ));
    }

    /// Assert the complete descriptor call sequence and the workbook profile's reply for each case.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn commands_call_the_members_and_pass_refusals_through() {
        let (host, model) = setup();
        let text = AutoValue::text;
        let unmodelled = |what| AutoError::not_modelled(what, "dc-oxh8w");
        let metadata_shift = |member| {
            AutoError::not_modelled(
                format!(
                    "Range.{member}: Shifting cells with merged, array, table, rule, chart, hidden or unloaded metadata is not modelled (dc-971p); nothing changed"
                ),
                "dc-oxh8w",
            )
        };
        let cases: Vec<(&str, Vec<AutoValue>, &[&str], Option<AutoError>)> = vec![
            (
                "xlcSelect",
                vec![text("R2C3:R4C5")],
                &["_Application.Range Get('C2:E4')", "Range.Select Method()"],
                None,
            ),
            (
                "xlcClear",
                vec![AutoValue::R8(1.0)],
                &["_Application.Selection Get()", "Range.Clear Method()"],
                Some(unmodelled("Range.Clear")),
            ),
            (
                "xlcClear",
                vec![AutoValue::R8(2.0)],
                &[
                    "_Application.Selection Get()",
                    "Range.ClearFormats Method()",
                ],
                Some(unmodelled("Range.ClearFormats")),
            ),
            (
                "xlcClear",
                vec![AutoValue::R8(4.0)],
                &[
                    "_Application.Selection Get()",
                    "Range.ClearComments Method()",
                ],
                Some(unmodelled("Range.ClearComments")),
            ),
            (
                "xlcFormatNumber",
                vec![text("0.00")],
                &[
                    "_Application.Selection Get()",
                    "Range.NumberFormat Put('0.00')",
                ],
                None,
            ),
            (
                "xlcColumnWidth",
                vec![AutoValue::R8(12.0), text("R1C2")],
                &[
                    "_Application.Range Get('B1')",
                    "Range.ColumnWidth Put(R8(12.0))",
                ],
                None,
            ),
            (
                "xlcRowHeight",
                vec![AutoValue::R8(30.0)],
                &[
                    "_Application.Selection Get()",
                    "Range.RowHeight Put(R8(30.0))",
                ],
                None,
            ),
            (
                "xlcInsert",
                vec![AutoValue::R8(1.0)],
                &[
                    "_Application.Selection Get()",
                    "Range.Insert Method(I4(-4161))",
                ],
                None,
            ),
            (
                "xlcInsert",
                vec![AutoValue::R8(2.0)],
                &[
                    "_Application.Selection Get()",
                    "Range.Insert Method(I4(-4121))",
                ],
                Some(metadata_shift("Insert")),
            ),
            (
                "xlcInsert",
                vec![AutoValue::R8(3.0)],
                &[
                    "_Application.Selection Get()",
                    "Range.EntireRow Get()",
                    "Range.Insert Method()",
                ],
                None,
            ),
            (
                "xlcInsert",
                vec![AutoValue::R8(4.0)],
                &[
                    "_Application.Selection Get()",
                    "Range.EntireColumn Get()",
                    "Range.Insert Method()",
                ],
                None,
            ),
            (
                "xlcEditDelete",
                vec![AutoValue::R8(1.0)],
                &[
                    "_Application.Selection Get()",
                    "Range.Delete Method(I4(-4159))",
                ],
                None,
            ),
            (
                "xlcEditDelete",
                vec![AutoValue::R8(2.0)],
                &[
                    "_Application.Selection Get()",
                    "Range.Delete Method(I4(-4162))",
                ],
                Some(metadata_shift("Delete")),
            ),
            (
                "xlcEditDelete",
                vec![AutoValue::R8(4.0)],
                &[
                    "_Application.Selection Get()",
                    "Range.EntireColumn Get()",
                    "Range.Delete Method()",
                ],
                None,
            ),
            (
                "xlcCopy",
                vec![text("R1C1")],
                &["_Application.Range Get('A1')", "Range.Copy Method()"],
                None,
            ),
            (
                "xlcCopy",
                vec![text("R1C1"), text("R3C3")],
                &[
                    "_Application.Range Get('A1')",
                    "_Application.Range Get('C3')",
                    "Range.Copy Method(obj)",
                ],
                None,
            ),
            (
                "xlcCut",
                vec![text("R1C1"), text("R3C3")],
                &[
                    "_Application.Range Get('A1')",
                    "_Application.Range Get('C3')",
                    "Range.Cut Method(obj)",
                ],
                None,
            ),
            (
                "xlcPaste",
                vec![text("R5C1")],
                &[
                    "_Application.ActiveSheet Get()",
                    "_Application.Range Get('A5')",
                    "_Worksheet.Paste Method(obj)",
                ],
                None,
            ),
            (
                "xlcPaste",
                vec![],
                &[
                    "_Application.ActiveSheet Get()",
                    "_Worksheet.Paste Method()",
                ],
                None,
            ),
            (
                "xlcDefineName",
                vec![text("Total"), text("=R1C1:R3C1")],
                &[
                    "_Application.Names Get()",
                    "Names.Add Method('Total',_,_,_,_,_,_,_,_,'=R1C1:R3C1')",
                ],
                Some(AutoError::not_modelled(
                    "Names.Add RefersToR1C1",
                    "dc-oxh8w.43",
                )),
            ),
            (
                "xlcDeleteName",
                vec![text("Total")],
                &["_Application.Names Get()", "Names.Item Method('Total')"],
                Some(AutoError::excel(1004, "")),
            ),
            (
                "xlcSave",
                vec![],
                &[
                    "_Application.ActiveWorkbook Get()",
                    "_Workbook.Save Method()",
                ],
                Some(unmodelled("_Workbook.Save")),
            ),
        ];
        for (symbol, operands, calls, error) in cases {
            if symbol == "xlcPaste" {
                value2(&model, "G31", AutoValue::R8(61.0));
                done(run(&*model, "xlcCopy", vec![text("R31C7")]));
                select_cells(&host, (30, 7), (30, 7));
                model.context_changed(&host.session.borrow());
            }
            let spy = spy(&model);
            let result = run(&spy, symbol, operands);
            assert_eq!(&*spy.calls.borrow(), calls, "{symbol}");
            match error {
                Some(error) => assert_eq!(result.unwrap_err(), XlcError::Model(error), "{symbol}"),
                None => assert!(
                    matches!(result, Ok(AutoValue::Bool(true))),
                    "{symbol}: {result:?}"
                ),
            }
            match symbol {
                "xlcColumnWidth" => assert!(matches!(
                    invoke(&model, &range(&model, "B1"), "ColumnWidth", vec![]).unwrap(),
                    AutoValue::R8(12.0)
                )),
                "xlcRowHeight" => {
                    let selection =
                        obj(invoke(&model, &model.application(), "Selection", vec![]).unwrap());
                    assert!(matches!(
                        invoke(&model, &selection, "RowHeight", vec![]).unwrap(),
                        AutoValue::R8(30.0)
                    ));
                }
                _ => {}
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
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
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
    // Other sheet types than worksheets and chart sheets (dc-oxh8w.23) are refused typed (dc-oxh8w.36 owns them),
    // nothing changes: xlDialogSheet is -4116.
    for args in [
        vec![AutoValue::Missing, AutoValue::Missing, AutoValue::Missing, AutoValue::I4(-4116)],
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
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
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

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_2g5fn_worksheet_delete_refuses_with_the_vba_project_owner() {
    let package = calc_doc::source_vba_package_for_sheets(
        &["Keep", "Remove"],
        &[calc_doc::SourceModule {
            name: "Sheet2",
            source: "Option Explicit\n",
            kind: calc_doc::SourceModuleKind::Worksheet,
        }],
    )
    .unwrap();
    let session = HostSession::open(
        Arc::new(MemorySource::new(package)),
        &OpenPolicy::default(),
        HostOptions::default(),
    )
    .unwrap();
    let (host, model) = setup_session(session);
    let sheets = worksheets(&model);
    let target = obj(invoke(&model, &sheets, "Item", vec![AutoValue::text("Remove")]).unwrap());
    let before = host.session.borrow().host().revision();

    let result = invoke_kind(&model, &target, "Delete", InvokeKind::Method, vec![]);

    assert_eq!(host.session.borrow().host().revision(), before);
    assert_eq!(sheet_names(&host), ["Keep", "Remove"]);
    assert!(
        matches!(&result, Err(AutoError::NotModelled { owner: "dc-2g5fn", .. })),
        "{result:?}"
    );
}

fn cell_text(model: &WorkbookAutomation, address: &str) -> AutoValue {
    invoke(model, &range(model, address), "Value2", vec![]).unwrap()
}

/// dc-oxh8w.4: Range.Insert and Range.Delete, with Shift given and omitted, and on whole rows and columns.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
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
    // The SUM moved down with its column and its range grew to take in the inserted cell.
    assert!(matches!(cell_text(&model, "Z6"), AutoValue::Empty));
    assert_eq!(text(&model, &range(&model, "Z7"), "Formula"), "=SUM(Z1:Z5)");
    assert!(matches!(cell_text(&model, "Z7"), AutoValue::R8(10.)));
    // Delete with Shift omitted on a single cell shifts up and brings it back.
    call("Z2", "Delete", vec![]).unwrap();
    assert!(matches!(cell_text(&model, "Z2"), AutoValue::R8(2.)));
    assert_eq!(text(&model, &range(&model, "Z6"), "Formula"), "=SUM(Z1:Z4)");
    assert!(matches!(cell_text(&model, "Z6"), AutoValue::R8(10.)));
    assert!(matches!(cell_text(&model, "Z7"), AutoValue::Empty));
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
    // CopyOrigin: inserted cells take the format from the left/above by default (xlFormatFromLeftOrAbove = 0),
    // from the right/below with xlFormatFromRightOrBelow = 1.
    let format = |address: &str| text(&model, &range(&model, address), "NumberFormat");
    let set_format = |address: &str, code: &str| {
        invoke_kind(&model, &range(&model, address), "NumberFormat", InvokeKind::Put, vec![AutoValue::text(code)])
            .unwrap();
    };
    set_format("E40", "0.00");
    set_format("E42", "0.0%");
    call("E41", "Insert", vec![AutoValue::I4(-4121), AutoValue::I4(1)]).unwrap();
    assert_eq!(format("E41"), "0.0%", "from below");
    assert_eq!(format("E42"), "General");
    assert_eq!(format("E43"), "0.0%");
    call("E41", "Delete", vec![AutoValue::I4(-4162)]).unwrap();
    call("E41", "Insert", vec![AutoValue::I4(-4121), AutoValue::I4(0)]).unwrap();
    assert_eq!(format("E41"), "0.00", "from above");
    call("E41", "Insert", vec![AutoValue::I4(-4121)]).unwrap();
    assert_eq!(format("E41"), "0.00", "default is from above");
    // Whole rows with CopyOrigin below are refused typed; the sheet is unchanged.
    assert!(matches!(
        call("50:51", "Insert", vec![AutoValue::Missing, AutoValue::I4(1)]),
        Err(AutoError::NotModelled { .. })
    ));
    assert!(matches!(cell_text(&model, "A20"), AutoValue::R8(7.)));
}

/// dc-oxh8w.4: Names.Add defines a workbook name, redefining an existing one, and returns the Name.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
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
    // An invalid name is Excel's 1004; non-formula RefersTo is a typed refusal.
    let error = add("1bad name", "=1").unwrap_err();
    assert!(matches!(error, AutoError::Excel { number: 1004, .. }), "{error:?}");
    assert!(matches!(add("Plain", "text"), Err(AutoError::NotModelled { .. })));
    assert!(matches!(invoke(&model, &names, "Count", vec![]).unwrap(), AutoValue::I4(1)));
}

/// dc-oxh8w.4: Range.Copy and Range.Cut with a Destination relocate formulas; a cut moves references.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
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
    // No destination means the clipboard (dc-oxh8w.15 pastes it: dc_oxh8w_15__clipboard_copy_cut_paste_special).
    assert!(matches!(call("A2", "Copy", vec![]), Ok(AutoValue::Bool(true))));
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

/// dc-oxh8w.26: Chart.Axes resolves axes by kind (and scatter orientation), counts arguments and refuses
/// invalid kinds and groups with 1004; a category axis cannot take IsAuto False.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_26__axes_resolve_by_kind_and_refuse_bad_arguments() {
    let (host, model, sheet) = blank_report();
    fill(&host, (1, 1), &[&["", "Y"], &["1", "10"], &["2", "30"], &["3", "50"]]);
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
    let axes = |args: Vec<AutoValue>| invoke_kind(&model, &chart, "Axes", InvokeKind::Method, args);
    let is_1004 = |r: &Result<AutoValue, AutoError>| matches!(r, Err(AutoError::Excel { number: 1004, .. }));
    // Argument counts and invalid enum values are Excel errors, not NotModelled.
    for args in [
        vec![AutoValue::I4(0)],
        vec![AutoValue::I4(4)],
        vec![AutoValue::I4(-1)],
        vec![AutoValue::I4(1), AutoValue::I4(3)],
        vec![AutoValue::I4(1), AutoValue::I4(0)],
    ] {
        let r = axes(args.clone());
        assert!(is_1004(&r), "{args:?}: {r:?}");
    }
    let r = axes(vec![AutoValue::I4(1), AutoValue::I4(1), AutoValue::I4(1)]);
    assert!(matches!(r, Err(AutoError::Status(-2147352562))), "{r:?}");
    let value = obj(axes(vec![AutoValue::I4(2)]).unwrap());
    let category = obj(axes(vec![AutoValue::I4(1)]).unwrap());
    // IsAuto False on the category axis is 1004, and a put with no or two values is an argument error.
    let r = invoke_kind(&model, &category, "MinimumScaleIsAuto", InvokeKind::Put, vec![AutoValue::Bool(false)]);
    assert!(is_1004(&r), "{r:?}");
    let r = invoke_kind(&model, &value, "MinimumScale", InvokeKind::Put, vec![]);
    assert!(matches!(r, Err(AutoError::Status(-2147352562))), "{r:?}");
    let r = invoke_kind(&model, &value, "MinimumScale", InvokeKind::Put, vec![AutoValue::R8(1.0), AutoValue::R8(2.0)]);
    assert!(matches!(r, Err(AutoError::Status(-2147352562))), "{r:?}");
    // A scatter chart has two value axes: xlCategory is the horizontal one.
    select_cells(&host, (30, 1), (30, 1));
    let shapes = obj(invoke(&model, &sheet, "Shapes", vec![]).unwrap());
    let shape = obj(
        invoke_kind(&model, &shapes, "AddChart2", InvokeKind::Method, vec![AutoValue::I4(-1), AutoValue::I4(-4169)])
            .unwrap(),
    );
    let chart = obj(invoke(&model, &shape, "Chart", vec![]).unwrap());
    let axes = |args: Vec<AutoValue>| invoke_kind(&model, &chart, "Axes", InvokeKind::Method, args);
    let x = obj(axes(vec![AutoValue::I4(1)]).unwrap());
    let y = obj(axes(vec![AutoValue::I4(2)]).unwrap());
    invoke_kind(&model, &x, "MaximumScale", InvokeKind::Put, vec![AutoValue::R8(7.0)]).unwrap();
    invoke_kind(&model, &y, "MaximumScale", InvokeKind::Put, vec![AutoValue::R8(90.0)]).unwrap();
    assert!(matches!(invoke(&model, &x, "MaximumScale", vec![]).unwrap(), AutoValue::R8(v) if v == 7.0));
    assert!(matches!(invoke(&model, &y, "MaximumScale", vec![]).unwrap(), AutoValue::R8(v) if v == 90.0));
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

/// A column chart over A1:B3 and its first series, for the data label tests.
fn labelled_series() -> (Rc<Host>, Rc<WorkbookAutomation>, ObjectRef) {
    let (host, model, _chart, series) = labelled_chart();
    (host, model, series)
}

fn labelled_chart() -> (Rc<Host>, Rc<WorkbookAutomation>, ObjectRef, ObjectRef) {
    let (host, model, sheet) = blank_report();
    fill(&host, (1, 1), &[&["Region", "Q1"], &["North", "10"], &["South", "30"]]);
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
    set_source(&model, &chart, "A1:B3");
    let series = obj(invoke_kind(&model, &chart, "SeriesCollection", InvokeKind::Method, vec![AutoValue::I4(1)]).unwrap());
    (host, model, chart, series)
}

fn saved_chart_xml(host: &Host) -> String {
    saved_part(&mut host.session.borrow_mut(), "xl/charts/")
        .replace(" xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\"", "")
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_58__pending_point_and_label_members_name_their_owning_bead() {
    let (_, model, series) = labelled_series();
    let points = invoke_kind(&model, &series, "Points", InvokeKind::Method, vec![AutoValue::I4(1)]);
    assert_eq!(points.unwrap_err(), AutoError::not_modelled("Series.Points", "dc-oxh8w.58"));

    invoke_kind(&model, &series, "HasDataLabels", InvokeKind::Put, vec![AutoValue::Bool(true)]).unwrap();
    let labels = obj(invoke_kind(&model, &series, "DataLabels", InvokeKind::Method, vec![]).unwrap());
    let label = obj(invoke_kind(&model, &series, "DataLabels", InvokeKind::Method, vec![AutoValue::I4(1)]).unwrap());
    for (target, interface, member) in [
        (&labels, "DataLabels", "Font"),
        (&label, "DataLabel", "NumberFormat"),
        (&label, "DataLabel", "NumberFormatLinked"),
        (&label, "DataLabel", "Text"),
        (&label, "DataLabel", "Caption"),
        (&label, "DataLabel", "Left"),
        (&label, "DataLabel", "Top"),
    ] {
        assert_eq!(
            invoke(&model, target, member, vec![]).unwrap_err(),
            AutoError::not_modelled(format!("{interface}.{member}"), "dc-oxh8w.58"),
        );
    }
}

/// dc-oxh8w.18: a macro turns a series' data labels on and off; the chart saves them and invalid arguments are 1004.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_18__a_macro_toggles_series_data_labels_and_the_chart_saves_them() {
    let (host, model, series) = labelled_series();
    let get = || invoke(&model, &series, "HasDataLabels", vec![]).unwrap();
    let put = |v: bool| invoke_kind(&model, &series, "HasDataLabels", InvokeKind::Put, vec![AutoValue::Bool(v)]);
    assert!(matches!(get(), AutoValue::Bool(false)));
    // No labels: the DataLabels object is Excel's 1004.
    let none = invoke_kind(&model, &series, "DataLabels", InvokeKind::Method, vec![]);
    assert!(matches!(none, Err(AutoError::Excel { number: 1004, .. })), "{none:?}");
    put(true).unwrap();
    assert!(matches!(get(), AutoValue::Bool(true)));
    let xml = saved_chart_xml(&host);
    assert!(xml.contains("<c:showVal val=\"1\"/>"), "{xml}");
    assert!(xml.contains("<c:dLbls><c:showLegendKey val=\"0\"/><c:showVal val=\"1\"/><c:showCatName val=\"0\"/>"), "{xml}");
    // ApplyDataLabels: category name and percent; ShowNone removes them.
    invoke_kind(&model, &series, "ApplyDataLabels", InvokeKind::Method, vec![AutoValue::I4(5)]).unwrap();
    let xml = saved_chart_xml(&host);
    assert!(xml.contains("<c:showVal val=\"0\"/><c:showCatName val=\"1\"/><c:showSerName val=\"0\"/><c:showPercent val=\"1\"/>"), "{xml}");
    invoke_kind(&model, &series, "ApplyDataLabels", InvokeKind::Method, vec![AutoValue::I4(-4142)]).unwrap();
    assert!(matches!(get(), AutoValue::Bool(false)));
    let xml = saved_chart_xml(&host);
    assert!(!xml.contains("<c:dLbls><c:showLegendKey val=\"0\"/><c:showVal val=\"0\"/><c:showCatName val=\"1\"/>"), "{xml}");
    put(true).unwrap();
    put(false).unwrap();
    assert!(matches!(get(), AutoValue::Bool(false)));
    // Invalid types and argument counts.
    for bad in [0, 1, 6, 7, 99] {
        let r = invoke_kind(&model, &series, "ApplyDataLabels", InvokeKind::Method, vec![AutoValue::I4(bad)]);
        assert!(matches!(r, Err(AutoError::Excel { number: 1004, .. })), "{bad}: {r:?}");
    }
    let r = invoke_kind(&model, &series, "ApplyDataLabels", InvokeKind::Method, vec![AutoValue::I4(2), AutoValue::I4(2)]);
    assert!(matches!(r, Err(AutoError::Status(-2147352562))), "{r:?}");
    let r = invoke_kind(&model, &series, "HasDataLabels", InvokeKind::Put, vec![]);
    assert!(matches!(r, Err(AutoError::Status(-2147352562))), "{r:?}");
    // The label of a point of a series that shows none is Excel's 1004.
    let r = invoke_kind(&model, &series, "DataLabels", InvokeKind::Method, vec![AutoValue::I4(1)]);
    assert!(matches!(r, Err(AutoError::Excel { number: 1004, .. })), "{r:?}");
}

/// dc-oxh8w.18: the `DataLabels` members (run once the descriptor holds DataLabels, RUN-ON-WINDOWS).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_18__data_labels_members_through_the_object_model() {
    if calc_object_model::Descriptor::get().id("DataLabels").is_none() {
        eprintln!("dc-oxh8w.18: descriptor lacks DataLabels; member round trip not run");
        return;
    }
    let (host, model, series) = labelled_series();
    invoke_kind(&model, &series, "HasDataLabels", InvokeKind::Put, vec![AutoValue::Bool(true)]).unwrap();
    let labels = obj(invoke_kind(&model, &series, "DataLabels", InvokeKind::Method, vec![]).unwrap());
    let flag = |name: &str| matches!(invoke(&model, &labels, name, vec![]).unwrap(), AutoValue::Bool(true));
    assert!(flag("ShowValue") && !flag("ShowCategoryName") && !flag("ShowSeriesName") && !flag("ShowPercentage"));
    invoke_kind(&model, &labels, "ShowSeriesName", InvokeKind::Put, vec![AutoValue::Bool(true)]).unwrap();
    invoke_kind(&model, &labels, "Position", InvokeKind::Put, vec![AutoValue::I4(3)]).unwrap();
    invoke_kind(&model, &labels, "Separator", InvokeKind::Put, vec![AutoValue::text("; ")]).unwrap();
    assert!(flag("ShowSeriesName"));
    assert_eq!(int(invoke(&model, &labels, "Position", vec![]).unwrap()), 3);
    assert_eq!(text(&model, &labels, "Separator"), "; ");
    let bad = invoke_kind(&model, &labels, "Position", InvokeKind::Put, vec![AutoValue::I4(12345)]);
    assert!(matches!(bad, Err(AutoError::Excel { number: 1004, .. })), "{bad:?}");
    let xml = saved_chart_xml(&host);
    assert!(xml.contains("<c:dLblPos val=\"inEnd\"/>"), "{xml}");
    assert!(xml.contains("<c:showSerName val=\"1\"/>"), "{xml}");
    assert!(xml.contains("<c:separator>; </c:separator>"), "{xml}");
}

/// dc-oxh8w.29: `Series.DataLabels(Index)` is the label of one point (`DataLabel`): its flags, position and separator
/// are its own, the others follow the series; `Delete` removes it. Out-of-range indexes and positions the chart type
/// does not allow are 1004. The chart saves each override as a `c:dLbl` and a reopened workbook reads it back.
/// `DataLabels.Position` unstated is the chart type's default: outside end for clustered columns, centre when
/// stacked, right for lines, best fit for pies, none (1004) for doughnuts.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_29__a_macro_labels_single_points_and_reads_default_positions() {
    let (host, model, series) = labelled_series();
    invoke_kind(&model, &series, "HasDataLabels", InvokeKind::Put, vec![AutoValue::Bool(true)]).unwrap();
    let labels = obj(invoke_kind(&model, &series, "DataLabels", InvokeKind::Method, vec![]).unwrap());
    assert_eq!(int(invoke(&model, &labels, "Count", vec![]).unwrap()), 2);
    let point = |at: i32| invoke_kind(&model, &series, "DataLabels", InvokeKind::Method, vec![AutoValue::I4(at)]);
    for bad in [0, 3, -1] {
        let r = point(bad);
        assert!(matches!(r, Err(AutoError::Excel { number: 1004, .. })), "{bad}: {r:?}");
    }
    let second = obj(point(2).unwrap());
    let flag = |label: &ObjectRef, name: &str| matches!(invoke(&model, label, name, vec![]).unwrap(), AutoValue::Bool(true));
    assert!(flag(&second, "ShowValue") && !flag(&second, "ShowCategoryName"));
    invoke_kind(&model, &second, "ShowCategoryName", InvokeKind::Put, vec![AutoValue::Bool(true)]).unwrap();
    invoke_kind(&model, &second, "Position", InvokeKind::Put, vec![AutoValue::I4(3)]).unwrap();
    assert!(flag(&second, "ShowCategoryName"));
    assert_eq!(int(invoke(&model, &second, "Position", vec![]).unwrap()), 3);
    // The first point and the series keep the series' own state.
    let first = obj(point(1).unwrap());
    assert!(!flag(&first, "ShowCategoryName") && !flag(&labels, "ShowCategoryName"));
    assert_eq!(int(invoke(&model, &first, "Position", vec![]).unwrap()), 2);
    let xml = saved_chart_xml(&host);
    assert!(
        xml.contains("<c:dLbl><c:idx val=\"1\"/><c:dLblPos val=\"inEnd\"/><c:showLegendKey val=\"0\"/><c:showVal val=\"1\"/><c:showCatName val=\"1\"/>"),
        "{xml}"
    );
    // A position the clustered column does not allow, and the label's Item through the collection.
    let bad = invoke_kind(&model, &first, "Position", InvokeKind::Put, vec![AutoValue::I4(-4152)]);
    assert!(matches!(bad, Err(AutoError::Excel { number: 1004, .. })), "{bad:?}");
    let again = obj(invoke_kind(&model, &labels, "Item", InvokeKind::Method, vec![AutoValue::I4(2)]).unwrap());
    assert!(flag(&again, "ShowCategoryName"));
    // The labels' number format: linked to the source ("General") until a code is put, which unlinks it.
    assert_eq!(text(&model, &labels, "NumberFormat"), "General");
    assert!(flag(&labels, "NumberFormatLinked"));
    invoke_kind(&model, &labels, "NumberFormat", InvokeKind::Put, vec![AutoValue::text("0.0%")]).unwrap();
    assert_eq!(text(&model, &labels, "NumberFormat"), "0.0%");
    assert!(!flag(&labels, "NumberFormatLinked"));
    let xml = saved_chart_xml(&host);
    assert!(xml.contains("<c:numFmt formatCode=\"0.0%\" sourceLinked=\"0\"/><c:showLegendKey"), "{xml}");
    let bad = invoke_kind(&model, &labels, "NumberFormat", InvokeKind::Put, vec![AutoValue::text("")]);
    assert!(matches!(bad, Err(AutoError::Excel { number: 1004, .. })), "{bad:?}");
    // Delete takes the first point's label; its members are then 1004 and the file states it.
    invoke_kind(&model, &first, "Delete", InvokeKind::Method, vec![]).unwrap();
    let gone = invoke(&model, &first, "ShowValue", vec![]);
    assert!(matches!(gone, Err(AutoError::Excel { number: 1004, .. })), "{gone:?}");
    let xml = saved_chart_xml(&host);
    assert!(xml.contains("<c:dLbl><c:idx val=\"0\"/><c:delete val=\"1\"/></c:dLbl><c:dLbl><c:idx val=\"1\"/>"), "{xml}");
    // A reopened workbook reads the overrides back.
    let mut bytes = Vec::new();
    host.session.borrow_mut().save(&SavePolicy::default(), &mut bytes).unwrap();
    let (_h2, model2) =
        setup_session(HostSession::open(Arc::new(MemorySource::new(bytes)), &OpenPolicy::default(), HostOptions::default()).unwrap());
    let book = obj(invoke(&model2, &model2.application(), "ActiveWorkbook", vec![]).unwrap());
    let sheets = obj(invoke(&model2, &book, "Worksheets", vec![]).unwrap());
    let sheet = obj(invoke_kind(&model2, &sheets, "Item", InvokeKind::Get, vec![AutoValue::I4(1)]).unwrap());
    let objects = obj(invoke_kind(&model2, &sheet, "ChartObjects", InvokeKind::Method, vec![]).unwrap());
    let frame = obj(invoke_kind(&model2, &objects, "Item", InvokeKind::Method, vec![AutoValue::I4(1)]).unwrap());
    let chart = obj(invoke(&model2, &frame, "Chart", vec![]).unwrap());
    let series2 = obj(invoke_kind(&model2, &chart, "SeriesCollection", InvokeKind::Method, vec![AutoValue::I4(1)]).unwrap());
    let second2 = obj(invoke_kind(&model2, &series2, "DataLabels", InvokeKind::Method, vec![AutoValue::I4(2)]).unwrap());
    assert!(matches!(invoke(&model2, &second2, "ShowCategoryName", vec![]), Ok(AutoValue::Bool(true))));
    assert_eq!(int(invoke(&model2, &second2, "Position", vec![]).unwrap()), 3);
    let labels2 = obj(invoke_kind(&model2, &series2, "DataLabels", InvokeKind::Method, vec![]).unwrap());
    assert_eq!(text(&model2, &labels2, "NumberFormat"), "0.0%");
    let first2 = obj(invoke_kind(&model2, &series2, "DataLabels", InvokeKind::Method, vec![AutoValue::I4(1)]).unwrap());
    assert!(invoke(&model2, &first2, "ShowValue", vec![]).is_err());
    // Defaults by chart type: a file that states no position reads as the type's own.
    for (chart_type, position) in [(51, Some(2)), (52, Some(-4108)), (4, Some(-4152)), (5, Some(5)), (-4120, None), (1, None)] {
        let (host, model, sheet) = blank_report();
        fill(&host, (1, 1), &[&["Region", "Q1"], &["North", "10"], &["South", "30"]]);
        let shapes = obj(invoke(&model, &sheet, "Shapes", vec![]).unwrap());
        let shape = obj(
            invoke_kind(&model, &shapes, "AddChart2", InvokeKind::Method, vec![AutoValue::I4(201), AutoValue::I4(chart_type)]).unwrap(),
        );
        let chart = obj(invoke(&model, &shape, "Chart", vec![]).unwrap());
        set_source(&model, &chart, "A1:B3");
        if int(invoke(&model, &chart, "ChartType", vec![]).unwrap()) != i64::from(chart_type) {
            invoke_kind(&model, &chart, "ChartType", InvokeKind::Put, vec![AutoValue::I4(chart_type)]).unwrap();
        }
        let series = obj(invoke_kind(&model, &chart, "SeriesCollection", InvokeKind::Method, vec![AutoValue::I4(1)]).unwrap());
        invoke_kind(&model, &series, "HasDataLabels", InvokeKind::Put, vec![AutoValue::Bool(true)]).unwrap();
        let labels = obj(invoke_kind(&model, &series, "DataLabels", InvokeKind::Method, vec![]).unwrap());
        let read = invoke(&model, &labels, "Position", vec![]);
        match position {
            Some(code) => assert_eq!(int(read.unwrap()), code, "type {chart_type}"),
            None => assert!(matches!(read, Err(AutoError::Excel { number: 1004, .. })), "type {chart_type}: {read:?}"),
        }
    }
}

/// dc-oxh8w.31: `Chart.ChartArea` and `Chart.PlotArea` fills (`Interior.Color`, `Format.Fill.ForeColor.RGB`, BGR numbers)
/// and `Series.Format.Fill.ForeColor.RGB`; the chart saves `c:spPr` fills for the chart space and the plot area, and
/// out-of-range colours are 1004. An unstated area reads white.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_31__chart_area_plot_area_and_series_fills() {
    let (host, model, chart, series) = labelled_chart();
    let area = obj(invoke(&model, &chart, "ChartArea", vec![]).unwrap());
    let plot = obj(invoke(&model, &chart, "PlotArea", vec![]).unwrap());
    let interior = |owner: &ObjectRef| obj(invoke(&model, owner, "Interior", vec![]).unwrap());
    let fore = |owner: &ObjectRef| {
        let format = obj(invoke(&model, owner, "Format", vec![]).unwrap());
        let fill = obj(invoke(&model, &format, "Fill", vec![]).unwrap());
        obj(invoke(&model, &fill, "ForeColor", vec![]).unwrap())
    };
    assert_eq!(int(invoke(&model, &interior(&area), "Color", vec![]).unwrap()), 0xFF_FF_FF);
    // RGB(255, 0, 0) is 255; the chart area takes it through Interior, the plot area through Format.Fill.ForeColor.
    invoke_kind(&model, &interior(&area), "Color", InvokeKind::Put, vec![AutoValue::I4(255)]).unwrap();
    invoke_kind(&model, &fore(&plot), "RGB", InvokeKind::Put, vec![AutoValue::I4(65_280)]).unwrap();
    assert_eq!(int(invoke(&model, &fore(&area), "RGB", vec![]).unwrap()), 255);
    assert_eq!(int(invoke(&model, &interior(&plot), "Color", vec![]).unwrap()), 65_280);
    invoke_kind(&model, &fore(&series), "RGB", InvokeKind::Put, vec![AutoValue::I4(16_711_680)]).unwrap();
    assert_eq!(int(invoke(&model, &fore(&series), "RGB", vec![]).unwrap()), 16_711_680);
    let xml = saved_chart_xml(&host);
    let at = |needle: &str| xml.find(needle).unwrap_or_else(|| panic!("{needle}: {xml}"));
    assert!(at("<a:srgbClr val=\"FF0000\"/>") > at("</c:chart>"), "{xml}");
    assert!(at("<a:srgbClr val=\"00FF00\"/>") < at("</c:plotArea>") && at("<a:srgbClr val=\"00FF00\"/>") > at("</c:valAx>"), "{xml}");
    assert!(xml.contains("<a:srgbClr val=\"0000FF\"/>"), "{xml}");
    for bad in [AutoValue::I4(-1), AutoValue::I4(16_777_216), AutoValue::R8(1.5)] {
        for target in [interior(&area), fore(&plot)] {
            let member = if target.interface == interior(&area).interface { "Color" } else { "RGB" };
            let r = invoke_kind(&model, &target, member, InvokeKind::Put, vec![bad.clone()]);
            assert!(matches!(r, Err(AutoError::Excel { number: 1004, .. })), "{bad:?}: {r:?}");
        }
    }
    let r = invoke_kind(&model, &interior(&area), "Color", InvokeKind::Put, vec![]);
    assert!(matches!(r, Err(AutoError::Status(-2147352562))), "{r:?}");
}

/// dc-oxh8w.31: the fills of a chart the file holds: a chart space and a plot area without `c:spPr` gain one, a stated
/// one is replaced (its outline stays), and a reopened workbook reads them back.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_31__fills_of_a_chart_the_file_holds_are_written_and_replaced() {
    let open = |bytes: Vec<u8>| {
        setup_session(HostSession::open(Arc::new(MemorySource::new(bytes)), &OpenPolicy::default(), HostOptions::default()).unwrap())
    };
    let chart_of = |model: &WorkbookAutomation| {
        let book = obj(invoke(model, &model.application(), "ActiveWorkbook", vec![]).unwrap());
        let charts = obj(invoke(model, &book, "Charts", vec![]).unwrap());
        obj(invoke_kind(model, &charts, "Item", InvokeKind::Get, vec![AutoValue::I4(1)]).unwrap())
    };
    let fill_of = |model: &WorkbookAutomation, chart: &ObjectRef, owner: &str| {
        let area = obj(invoke(model, chart, owner, vec![]).unwrap());
        let interior = obj(invoke(model, &area, "Interior", vec![]).unwrap());
        interior
    };
    let (host, model) = open(include_bytes!("../../calc-doc/tests/fixtures/preserved_sheets/chart-sheet-and-embedded.xlsx").to_vec());
    let chart = chart_of(&model);
    invoke_kind(&model, &fill_of(&model, &chart, "ChartArea"), "Color", InvokeKind::Put, vec![AutoValue::I4(255)]).unwrap();
    invoke_kind(&model, &fill_of(&model, &chart, "PlotArea"), "Color", InvokeKind::Put, vec![AutoValue::I4(65_280)]).unwrap();
    let mut bytes = Vec::new();
    host.session.borrow_mut().save(&SavePolicy::default(), &mut bytes).expect("the save is not refused");
    let (host2, model2) = open(bytes);
    let chart2 = chart_of(&model2);
    assert_eq!(int(invoke(&model2, &fill_of(&model2, &chart2, "ChartArea"), "Color", vec![]).unwrap()), 255);
    assert_eq!(int(invoke(&model2, &fill_of(&model2, &chart2, "PlotArea"), "Color", vec![]).unwrap()), 65_280);
    // A second colour replaces the first: one fill each.
    invoke_kind(&model2, &fill_of(&model2, &chart2, "ChartArea"), "Color", InvokeKind::Put, vec![AutoValue::I4(16_711_680)]).unwrap();
    let mut again = Vec::new();
    host2.session.borrow_mut().save(&SavePolicy::default(), &mut again).expect("the save is not refused");
    let document = calc_doc::open(Arc::new(MemorySource::new(again.clone())), &OpenPolicy::default()).unwrap();
    let part = document.package().parts().iter().map(|p| p.name().to_owned()).find(|n| n == "xl/charts/chart1.xml").unwrap();
    let xml = String::from_utf8(document.package().read_part(&part).unwrap()).unwrap();
    assert_eq!(xml.matches("<c:spPr").count(), 2, "{xml}");
    assert!(xml.contains("val=\"0000FF\"") && !xml.contains("val=\"FF0000\"") && xml.contains("val=\"00FF00\""), "{xml}");
    let (_h3, model3) = open(again);
    assert_eq!(int(invoke(&model3, &fill_of(&model3, &chart_of(&model3), "ChartArea"), "Color", vec![]).unwrap()), 16_711_680);
}

/// dc-oxh8w.31: `TickLabels.NumberFormat` takes any code the formatter applies on a value axis (an empty one is 1004), a
/// code put states it (unlinks, "General" too), `NumberFormatLinked = False` keeps the format shown ("General" when the
/// labels follow the source), and a category axis refuses typed.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_31__tick_label_codes_and_unlinking() {
    let (host, model, chart, _series) = labelled_chart();
    let axis = |kind: i32| obj(invoke_kind(&model, &chart, "Axes", InvokeKind::Method, vec![AutoValue::I4(kind)]).unwrap());
    let labels = obj(invoke(&model, &axis(2), "TickLabels", vec![]).unwrap());
    let linked = || matches!(invoke(&model, &labels, "NumberFormatLinked", vec![]).unwrap(), AutoValue::Bool(true));
    assert!(linked());
    invoke_kind(&model, &labels, "NumberFormatLinked", InvokeKind::Put, vec![AutoValue::Bool(false)]).unwrap();
    assert!(!linked());
    assert_eq!(text(&model, &labels, "NumberFormat"), "General");
    assert!(saved_chart_xml(&host).contains("<c:numFmt formatCode=\"General\" sourceLinked=\"0\"/>"));
    invoke_kind(&model, &labels, "NumberFormat", InvokeKind::Put, vec![AutoValue::text("#,##0.0,\"k\"")]).unwrap();
    assert_eq!(text(&model, &labels, "NumberFormat"), "#,##0.0,\"k\"");
    assert!(saved_chart_xml(&host).contains("formatCode=\"#,##0.0,&quot;k&quot;\" sourceLinked=\"0\""));
    invoke_kind(&model, &labels, "NumberFormatLinked", InvokeKind::Put, vec![AutoValue::Bool(true)]).unwrap();
    assert!(linked());
    assert_eq!(text(&model, &labels, "NumberFormat"), "General");
    let bad = invoke_kind(&model, &labels, "NumberFormat", InvokeKind::Put, vec![AutoValue::text("")]);
    assert!(matches!(bad, Err(AutoError::Excel { number: 1004, .. })), "{bad:?}");
}

/// dc-oxh8w.61: `Border` of the chart area and the plot area (Color BGR, LineStyle, Weight) saves `a:ln` beside the fill;
/// a category axis takes a tick-label format; bad values are 1004; `Series.Border` refuses typed under dc-oxh8w.71.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_61__borders_of_chart_and_plot_area_and_category_axis_formats() {
    let (host, model, chart, series) = labelled_chart();
    let border = |owner: &str| {
        let area = obj(invoke(&model, &chart, owner, vec![]).unwrap());
        obj(invoke(&model, &area, "Border", vec![]).unwrap())
    };
    let area = border("ChartArea");
    let plot = border("PlotArea");
    // An unstated outline reads as the default style's: continuous, thin, light grey.
    assert_eq!(int(invoke(&model, &area, "LineStyle", vec![]).unwrap()), 1);
    assert_eq!(int(invoke(&model, &area, "Weight", vec![]).unwrap()), 2);
    assert_eq!(int(invoke(&model, &area, "Color", vec![]).unwrap()), 0xD9_D9_D9);
    invoke_kind(&model, &area, "Color", InvokeKind::Put, vec![AutoValue::I4(255)]).unwrap();
    invoke_kind(&model, &area, "Weight", InvokeKind::Put, vec![AutoValue::I4(4)]).unwrap();
    invoke_kind(&model, &plot, "Color", InvokeKind::Put, vec![AutoValue::I4(65_280)]).unwrap();
    invoke_kind(&model, &plot, "LineStyle", InvokeKind::Put, vec![AutoValue::I4(-4115)]).unwrap();
    assert_eq!(int(invoke(&model, &area, "Color", vec![]).unwrap()), 255);
    assert_eq!(int(invoke(&model, &area, "Weight", vec![]).unwrap()), 4);
    assert_eq!(int(invoke(&model, &plot, "LineStyle", vec![]).unwrap()), -4115);
    let xml = saved_chart_xml(&host);
    let ln = |xml: &str, from: &str| {
        let at = xml.find(from).unwrap_or_else(|| panic!("{from}: {xml}"));
        let start = xml[at..].find("<a:ln").map(|i| at + i).unwrap_or_else(|| panic!("no a:ln after {from}: {xml}"));
        xml[start..start + xml[start..].find("</a:ln>").unwrap()].to_owned()
    };
    let area_ln = ln(&xml, "</c:chart>");
    assert!(area_ln.contains("w=\"28575\"") && area_ln.contains("<a:srgbClr val=\"FF0000\"/>"), "{xml}");
    let plot_ln = ln(&xml, "</c:valAx>");
    assert!(plot_ln.contains("<a:srgbClr val=\"00FF00\"/>") && plot_ln.contains("<a:prstDash val=\"dash\"/>"), "{xml}");
    invoke_kind(&model, &area, "LineStyle", InvokeKind::Put, vec![AutoValue::I4(-4142)]).unwrap();
    assert_eq!(int(invoke(&model, &area, "LineStyle", vec![]).unwrap()), -4142);
    let xml = saved_chart_xml(&host);
    assert!(ln(&xml, "</c:chart>").contains("<a:noFill/>"), "{xml}");
    for (target, member, bad) in [
        (&area, "Color", AutoValue::I4(-1)),
        (&area, "Color", AutoValue::I4(16_777_216)),
        (&area, "Color", AutoValue::R8(1.5)),
        (&area, "LineStyle", AutoValue::I4(7)),
        (&plot, "Weight", AutoValue::I4(3)),
    ] {
        let r = invoke_kind(&model, target, member, InvokeKind::Put, vec![bad.clone()]);
        assert!(matches!(r, Err(AutoError::Excel { number: 1004, .. })), "{member} {bad:?}: {r:?}");
    }
    let r = invoke_kind(&model, &area, "Weight", InvokeKind::Put, vec![]);
    assert!(matches!(r, Err(AutoError::Status(-2147352562))), "{r:?}");
    // A series that states no outline answers typed-refused until Excel's automatic styles are recorded (dc-oxh8w.60).
    let series_border = obj(invoke(&model, &series, "Border", vec![]).unwrap());
    let r = invoke(&model, &series_border, "LineStyle", vec![]);
    assert!(matches!(r, Err(AutoError::NotModelled { .. })), "{r:?}");

    // A category axis states a tick-label format and unlinks.
    let axis = obj(invoke_kind(&model, &chart, "Axes", InvokeKind::Method, vec![AutoValue::I4(1)]).unwrap());
    let labels = obj(invoke(&model, &axis, "TickLabels", vec![]).unwrap());
    invoke_kind(&model, &labels, "NumberFormat", InvokeKind::Put, vec![AutoValue::text("0.0")]).unwrap();
    assert_eq!(text(&model, &labels, "NumberFormat"), "0.0");
    assert!(matches!(invoke(&model, &labels, "NumberFormatLinked", vec![]).unwrap(), AutoValue::Bool(false)));
    let xml = saved_chart_xml(&host);
    let cat = &xml[xml.find("<c:catAx>").expect("catAx")..xml.find("</c:catAx>").unwrap()];
    assert!(cat.contains("<c:numFmt formatCode=\"0.0\" sourceLinked=\"0\"/>"), "{xml}");
    invoke_kind(&model, &labels, "NumberFormatLinked", InvokeKind::Put, vec![AutoValue::Bool(true)]).unwrap();
    assert!(!saved_chart_xml(&host).contains("formatCode=\"0.0\""));
    invoke_kind(&model, &labels, "NumberFormatLinked", InvokeKind::Put, vec![AutoValue::Bool(false)]).unwrap();
    assert_eq!(text(&model, &labels, "NumberFormat"), "General");
}

/// dc-oxh8w.61: the outlines of a chart the file holds are replaced in place (fill kept) or added, and read back after a save.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_61__borders_of_a_chart_the_file_holds_survive_a_save() {
    let open = |bytes: Vec<u8>| {
        setup_session(HostSession::open(Arc::new(MemorySource::new(bytes)), &OpenPolicy::default(), HostOptions::default()).unwrap())
    };
    let chart_of = |model: &WorkbookAutomation| {
        let book = obj(invoke(model, &model.application(), "ActiveWorkbook", vec![]).unwrap());
        let charts = obj(invoke(model, &book, "Charts", vec![]).unwrap());
        obj(invoke_kind(model, &charts, "Item", InvokeKind::Get, vec![AutoValue::I4(1)]).unwrap())
    };
    let border_of = |model: &WorkbookAutomation, chart: &ObjectRef, owner: &str| {
        let area = obj(invoke(model, chart, owner, vec![]).unwrap());
        obj(invoke(model, &area, "Border", vec![]).unwrap())
    };
    let (host, model) = open(include_bytes!("../../calc-doc/tests/fixtures/preserved_sheets/chart-sheet-and-embedded.xlsx").to_vec());
    let chart = chart_of(&model);
    let fill = obj(invoke(&model, &obj(invoke(&model, &chart, "ChartArea", vec![]).unwrap()), "Interior", vec![]).unwrap());
    invoke_kind(&model, &fill, "Color", InvokeKind::Put, vec![AutoValue::I4(65_280)]).unwrap();
    for owner in ["ChartArea", "PlotArea"] {
        let border = border_of(&model, &chart, owner);
        invoke_kind(&model, &border, "Color", InvokeKind::Put, vec![AutoValue::I4(255)]).unwrap();
        invoke_kind(&model, &border, "Weight", InvokeKind::Put, vec![AutoValue::I4(-4138)]).unwrap();
    }
    let mut bytes = Vec::new();
    host.session.borrow_mut().save(&SavePolicy::default(), &mut bytes).expect("the save is not refused");
    let (host2, model2) = open(bytes);
    let chart2 = chart_of(&model2);
    for owner in ["ChartArea", "PlotArea"] {
        let border = border_of(&model2, &chart2, owner);
        assert_eq!(int(invoke(&model2, &border, "Color", vec![]).unwrap()), 255, "{owner}");
        assert_eq!(int(invoke(&model2, &border, "Weight", vec![]).unwrap()), -4138, "{owner}");
    }
    let fill2 = obj(invoke(&model2, &obj(invoke(&model2, &chart2, "ChartArea", vec![]).unwrap()), "Interior", vec![]).unwrap());
    assert_eq!(int(invoke(&model2, &fill2, "Color", vec![]).unwrap()), 65_280);
    // A second colour replaces the first: one outline each.
    invoke_kind(&model2, &border_of(&model2, &chart2, "ChartArea"), "Color", InvokeKind::Put, vec![AutoValue::I4(16_711_680)]).unwrap();
    let mut again = Vec::new();
    host2.session.borrow_mut().save(&SavePolicy::default(), &mut again).expect("the save is not refused");
    let document = calc_doc::open(Arc::new(MemorySource::new(again)), &OpenPolicy::default()).unwrap();
    let xml = String::from_utf8(document.package().read_part("xl/charts/chart1.xml").unwrap()).unwrap();
    let after_chart = &xml[xml.find("</c:chart>").unwrap()..];
    assert_eq!(after_chart.matches("<a:ln").count(), 1, "{xml}");
    assert!(after_chart.contains("val=\"0000FF\"") && after_chart.contains("val=\"00FF00\""), "{xml}");
}

/// dc-oxh8w.19: a macro colours a series through Interior.Color (a BGR number); the chart saves the fill, and
/// out-of-range colours and wrong argument counts are Excel errors.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_19__a_macro_colours_a_series_through_interior_color() {
    let (host, model, series) = labelled_series();
    let interior = obj(invoke(&model, &series, "Interior", vec![]).unwrap());
    // The automatic colour reads as the colour drawn: accent 1 of the default theme, 4472C4, as BGR.
    assert_eq!(int(invoke(&model, &interior, "Color", vec![]).unwrap()), 0xC4_72_44);
    // RGB(255, 0, 0) is 255.
    invoke_kind(&model, &interior, "Color", InvokeKind::Put, vec![AutoValue::I4(255)]).unwrap();
    assert_eq!(int(invoke(&model, &interior, "Color", vec![]).unwrap()), 255);
    invoke_kind(&model, &interior, "Color", InvokeKind::Put, vec![AutoValue::R8(16_711_680.0)]).unwrap();
    assert_eq!(int(invoke(&model, &interior, "Color", vec![]).unwrap()), 16_711_680);
    let xml = saved_chart_xml(&host);
    assert!(xml.contains("<a:srgbClr val=\"0000FF\"/>"), "{xml}");
    for bad in [AutoValue::I4(-1), AutoValue::I4(16_777_216), AutoValue::R8(1.5)] {
        let r = invoke_kind(&model, &interior, "Color", InvokeKind::Put, vec![bad.clone()]);
        assert!(matches!(r, Err(AutoError::Excel { number: 1004, .. })), "{bad:?}: {r:?}");
    }
    let r = invoke_kind(&model, &interior, "Color", InvokeKind::Put, vec![]);
    assert!(matches!(r, Err(AutoError::Status(-2147352562))), "{r:?}");
    let r = invoke(&model, &interior, "Color", vec![AutoValue::I4(1)]);
    assert!(matches!(r, Err(AutoError::Status(-2147352562))), "{r:?}");
}

/// dc-oxh8w.4: Range.Sort with keys as ranges, Order and Header; unmodelled options refuse typed.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
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

/// dc-oxh8w.15: Range.Find / FindNext / FindPrevious are one member each in calc-host: the search starts
/// after `After` (default the range's first cell) and wraps; LookIn, LookAt, SearchOrder, SearchDirection and
/// MatchCase apply, are saved between calls, and a miss is Nothing.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_15__range_find_next_previous() {
    let (host, model) = setup_session(
        HostSession::open(
            Arc::new(MemorySource::new(blank_package())),
            &OpenPolicy::default(),
            HostOptions::default(),
        )
        .unwrap(),
    );
    fill(&host, (1, 1), &[&["apple", "pie", "x"], &["Apple pie", "fig", "12.5"], &["banana", "APPLE", "=1+1"]]);
    let area = range(&model, "A1:C3");
    let find = |name: &str, args: Vec<AutoValue>| invoke_kind(&model, &area, name, InvokeKind::Method, args);
    let found = |name: &str, args: Vec<AutoValue>| -> String {
        match find(name, args).unwrap() {
            AutoValue::Object(hit) => text(&model, &hit, "Address"),
            other => format!("{other:?}"),
        }
    };
    let cell = |address: &str| AutoValue::Object(range(&model, address));
    let t = |s: &str| AutoValue::text(s);
    let m = || AutoValue::Missing;
    // Defaults: formulas, part of the cell, by rows, next, any case. The first cell searched is after A1.
    assert_eq!(found("Find", vec![t("apple")]), "$A$2");
    assert_eq!(found("FindNext", vec![cell("A2")]), "$B$3");
    assert_eq!(found("FindNext", vec![cell("B3")]), "$A$1", "wraps to the first cell");
    assert_eq!(found("FindPrevious", vec![cell("A1")]), "$B$3", "backwards from A1 wraps to the end");
    // Whole cell, then match case: only A1 matches, and a lone match is found even from itself.
    assert_eq!(found("Find", vec![t("apple"), m(), m(), AutoValue::I4(1)]), "$B$3");
    let case = vec![t("apple"), m(), m(), AutoValue::I4(1), m(), m(), AutoValue::Bool(true)];
    assert_eq!(found("Find", case), "$A$1");
    // Wildcards, and the saved settings: LookAt stays part, MatchCase resets to the argument.
    assert_eq!(found("Find", vec![t("a*e"), m(), m(), AutoValue::I4(1), m(), m(), AutoValue::Bool(false)]), "$A$2");
    assert_eq!(found("Find", vec![t("*pie"), m(), m(), AutoValue::I4(2)]), "$B$1");
    // By columns: A1, A2, A3, B1, ... so the first apple after A1 is A2 and then B3.
    let by_columns = vec![t("apple"), m(), m(), AutoValue::I4(2), AutoValue::I4(2)];
    assert_eq!(found("Find", by_columns), "$A$2");
    assert_eq!(found("FindNext", vec![cell("A2")]), "$B$3");
    // LookIn values searches what the cells show; formulas the text of the entry.
    let values = |what: &str, at: i64| vec![t(what), m(), AutoValue::I4(at as i32), AutoValue::I4(1), AutoValue::I4(1)];
    assert_eq!(found("Find", values("2", -4163)), "$C$3");
    assert_eq!(found("Find", values("2", -4123)), format!("{:?}", AutoValue::Empty));
    assert_eq!(found("Find", values("=1+1", -4123)), "$C$3");
    // Nothing for a miss; a missing What is Excel's error; After must be one cell of the range.
    assert_eq!(found("Find", vec![t("zzz")]), format!("{:?}", AutoValue::Empty));
    assert!(matches!(find("Find", vec![]), Err(AutoError::Excel { number: 1004, .. })));
    assert!(matches!(find("Find", vec![t("apple"), cell("E5")]), Err(AutoError::Excel { number: 1004, .. })));
    assert!(matches!(find("Find", vec![t("apple"), cell("A1:B2")]), Err(AutoError::Excel { number: 1004, .. })));
    // Comments and formats are refused typed.
    assert!(matches!(
        find("Find", vec![t("a"), m(), AutoValue::I4(-4144)]),
        Err(AutoError::NotModelled { .. })
    ));
}

/// dc-oxh8w.15: Range.Copy / Cut without a Destination fill the application's clipboard; Range.PasteSpecial
/// and Worksheet.Paste write it through the host's paste planner (Values, Formulas, Transpose, All; a Cut is
/// spent by its paste; no clipboard is Excel's 1004).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_15__clipboard_copy_cut_paste_special() {
    let (host, model) = setup_session(
        HostSession::open(
            Arc::new(MemorySource::new(blank_package())),
            &OpenPolicy::default(),
            HostOptions::default(),
        )
        .unwrap(),
    );
    fill(&host, (1, 1), &[&["1", "2", "3"], &["=A1+10", "x", "=SUM(A1:C1)"]]);
    let call = |address: &str, name: &str, args: Vec<AutoValue>| {
        invoke_kind(&model, &range(&model, address), name, InvokeKind::Method, args)
    };
    let value = |address: &str| cell_text_of(&model, address);
    let formula = |address: &str| text(&model, &range(&model, address), "Formula");
    // Nothing on the clipboard yet: Excel's 1004.
    assert!(matches!(call("E1", "PasteSpecial", vec![]), Err(AutoError::Excel { number: 1004, .. })));
    // Copy with no Destination answers True and pastes values only.
    assert!(matches!(call("A1:C2", "Copy", vec![]).unwrap(), AutoValue::Bool(true)));
    call("E1", "PasteSpecial", vec![AutoValue::I4(-4163)]).unwrap();
    assert_eq!([value("E1"), value("F1"), value("G1")], ["1", "2", "3"]);
    assert_eq!(formula("E2"), "11", "a value, not the formula =A1+10");
    assert_eq!(formula("G2"), "6");
    // Formulas relocate; the Copy stays for another paste.
    call("E4", "PasteSpecial", vec![AutoValue::I4(-4123)]).unwrap();
    assert_eq!(formula("E5"), "=E4+10");
    assert_eq!(formula("G5"), "=SUM(E4:G4)");
    // Transpose swaps rows and columns.
    call("J1", "PasteSpecial", vec![AutoValue::I4(-4163), AutoValue::I4(-4142), AutoValue::Bool(false), AutoValue::Bool(true)])
        .unwrap();
    assert_eq!([value("J1"), value("K1")], ["1", "11"]);
    assert_eq!([value("J2"), value("K2")], ["2", "x"]);
    assert_eq!([value("J3"), value("K3")], ["3", "6"]);
    // Formula sources and destinations refuse an Operation; nothing is pasted.
    let before = value("M1");
    for args in [
        vec![AutoValue::I4(-4163), AutoValue::I4(2)],
        vec![AutoValue::I4(-4163), AutoValue::I4(2), AutoValue::Bool(true)],
    ] {
        assert!(matches!(call("M1", "PasteSpecial", args), Err(AutoError::NotModelled { .. })));
    }
    assert!(matches!(
        call("M1", "PasteSpecial", vec![AutoValue::I4(-4163), AutoValue::I4(99)]),
        Err(AutoError::Excel { number: 1004, .. })
    ));
    assert_eq!(value("M1"), before);
    // Worksheet.Paste at a Destination pastes everything.
    let sheet = obj(invoke(&model, &model.application(), "ActiveSheet", vec![]).unwrap());
    invoke_kind(&model, &sheet, "Paste", InvokeKind::Method, vec![AutoValue::Object(range(&model, "A10"))]).unwrap();
    assert_eq!(formula("A11"), "=A10+10");
    // A Cut moves on paste, only All is allowed, and the clipboard is then spent.
    assert!(matches!(call("A10:A11", "Cut", vec![]).unwrap(), AutoValue::Bool(true)));
    assert!(matches!(call("C10", "PasteSpecial", vec![AutoValue::I4(-4163)]), Err(AutoError::Excel { number: 1004, .. })));
    call("C10", "PasteSpecial", vec![]).unwrap();
    assert_eq!(value("C10"), "1");
    assert_eq!(value("A10"), "", "the cut source is emptied");
    assert!(matches!(call("C13", "PasteSpecial", vec![]), Err(AutoError::Excel { number: 1004, .. })));
}

/// dc-oxh8w.24: argument counts and enum values on the structure members give Excel's 1004 or the argument-count error.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_24__structure_arguments_and_enums_give_excel_errors() {
    let (host, model) = setup();
    let sheets = worksheets(&model);
    for _ in 0..2 {
        invoke_kind(&model, &sheets, "Add", InvokeKind::Method, vec![]).unwrap();
    }
    let item = |n: i32| obj(invoke(&model, &sheets, "Item", vec![AutoValue::I4(n)]).unwrap());
    let before = sheet_names(&host);
    let is_1004 = |r: Reply| matches!(r, Err(AutoError::Excel { number: 1004, .. }));
    let method = |target: &ObjectRef, member: &str, args: Vec<AutoValue>| {
        invoke_kind(&model, target, member, InvokeKind::Method, args)
    };
    let (a, b) = (AutoValue::Object(item(2)), AutoValue::Object(item(3)));
    // Move with both Before and After is 1004, like Add; excess arguments are an argument-count error.
    assert!(is_1004(method(&item(1), "Move", vec![a.clone(), b.clone()])));
    let excess = method(&item(1), "Move", vec![a.clone(), AutoValue::Missing, AutoValue::Missing]);
    assert!(matches!(excess, Err(ref e) if !matches!(e, AutoError::NotModelled { .. })), "{excess:?}");
    let excess = method(
        &sheets,
        "Add",
        vec![AutoValue::Missing, AutoValue::Missing, AutoValue::Missing, AutoValue::Missing, AutoValue::Missing],
    );
    assert!(matches!(excess, Err(ref e) if !matches!(e, AutoError::NotModelled { .. })), "{excess:?}");
    assert_eq!(sheet_names(&host), before);
    // Invalid enum values: Insert/Delete Shift and CopyOrigin, Sort Header/Orientation/SortMethod/DataOption.
    let a1 = range(&model, "A1:B2");
    assert!(is_1004(method(&a1, "Insert", vec![AutoValue::I4(99)])));
    assert!(is_1004(method(&a1, "Delete", vec![AutoValue::I4(99)])));
    assert!(is_1004(method(&a1, "Insert", vec![AutoValue::I4(-4121), AutoValue::I4(99)])));
    let key = AutoValue::Object(range(&model, "A1"));
    for (index, value) in [(7, 99), (10, 99), (11, 99), (14, 99)] {
        let mut args = vec![AutoValue::Missing; 15];
        args[0] = key.clone();
        args[index] = AutoValue::I4(value);
        assert!(is_1004(method(&a1, "Sort", args)), "Sort arg {index}");
    }
}

/// dc-oxh8w.24: deleting the whole sheet clears every cell (rows and columns), as Excel does.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_24__whole_sheet_delete_clears_the_sheet() {
    let (_host, model) = setup();
    let put = |address: &str, value: f64| {
        invoke_kind(&model, &range(&model, address), "Value2", InvokeKind::Put, vec![AutoValue::R8(value)]).unwrap();
    };
    put("A1", 1.0);
    put("D40", 2.0);
    invoke_kind(&model, &range(&model, "A1:XFD1048576"), "Delete", InvokeKind::Method, vec![]).unwrap();
    assert!(matches!(cell_text(&model, "A1"), AutoValue::Empty));
    assert!(matches!(cell_text(&model, "D40"), AutoValue::Empty));
}

/// dc-jtl4m: PasteSpecial Operation (add, subtract, multiply, divide) and SkipBlanks, and Worksheet.PasteSpecial.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_jtl4m__paste_special_operation_skip_blanks_and_worksheet_paste_special() {
    let (host, model) = setup_session(
        HostSession::open(
            Arc::new(MemorySource::new(blank_package())),
            &OpenPolicy::default(),
            HostOptions::default(),
        )
        .unwrap(),
    );
    fill(&host, (1, 1), &[&["1", "2", "3"], &["5", "", "7"], &["10", "20", "0"], &["=A1", "", ""]]);
    let call = |address: &str, name: &str, args: Vec<AutoValue>| {
        invoke_kind(&model, &range(&model, address), name, InvokeKind::Method, args)
    };
    let value = |address: &str| cell_text_of(&model, address);
    let values = AutoValue::I4(-4163);
    let operation = |kind: i32| vec![values.clone(), AutoValue::I4(kind)];
    // Add, subtract, multiply, divide the copied numbers into numbers; the Copy stays for each paste.
    // A fresh destination row of 10, 20, 30 makes each result unambiguous.
    fill(&host, (8, 1), &[&["10", "20", "30"]]);
    call("A1:C1", "Copy", vec![]).unwrap();
    call("A8", "PasteSpecial", operation(2)).unwrap();
    assert_eq!([value("A8"), value("B8"), value("C8")], ["11", "22", "33"]);
    call("A8", "PasteSpecial", operation(3)).unwrap();
    assert_eq!([value("A8"), value("B8"), value("C8")], ["10", "20", "30"]);
    call("A8", "PasteSpecial", operation(4)).unwrap();
    assert_eq!([value("A8"), value("B8"), value("C8")], ["10", "40", "90"]);
    call("A8", "PasteSpecial", operation(5)).unwrap();
    assert_eq!([value("A8"), value("B8"), value("C8")], ["10", "20", "30"]);
    // Dividing by a copied zero is #DIV/0!; an empty destination counts as zero.
    call("A3:C3", "Copy", vec![]).unwrap();
    call("A8", "PasteSpecial", operation(5)).unwrap();
    assert_eq!(text(&model, &range(&model, "C8"), "Text"), "#DIV/0!");
    assert_eq!(value("A40"), "", "an empty destination");
    call("A40", "PasteSpecial", operation(2)).unwrap();
    assert_eq!([value("A40"), value("B40"), value("C40")], ["10", "20", "0"]);
    // A blank copied cell counts as zero: add and subtract keep the destination, multiply gives 0, divide #DIV/0!.
    fill(&host, (30, 1), &[&["7", "8", "9"]]);
    call("A2:C2", "Copy", vec![]).unwrap();
    call("A30", "PasteSpecial", operation(2)).unwrap();
    assert_eq!([value("A30"), value("B30"), value("C30")], ["12", "8", "16"]);
    call("A30", "PasteSpecial", operation(4)).unwrap();
    assert_eq!([value("A30"), value("B30"), value("C30")], ["60", "0", "112"]);
    fill(&host, (31, 1), &[&["7", "8", "9"]]);
    call("A31", "PasteSpecial", operation(5)).unwrap();
    assert_eq!(text(&model, &range(&model, "B31"), "Text"), "#DIV/0!");
    // A formula destination is refused typed and nothing changes.
    call("A4", "Copy", vec![]).unwrap();
    call("A12", "PasteSpecial", operation(2)).unwrap();
    assert_eq!(value("A12"), "1", "a formula source pastes its value");
    call("A1:C1", "Copy", vec![]).unwrap();
    assert!(matches!(call("A4", "PasteSpecial", operation(2)), Err(AutoError::NotModelled { .. })));
    // SkipBlanks keeps the destination where the copied cell is blank; without it the blank overwrites.
    fill(&host, (14, 1), &[&["1", "2", "3"], &["1", "2", "3"]]);
    call("A2:C2", "Copy", vec![]).unwrap();
    call("A14", "PasteSpecial", vec![AutoValue::Missing, AutoValue::Missing, AutoValue::Bool(true)]).unwrap();
    assert_eq!([value("A14"), value("B14"), value("C14")], ["5", "2", "7"]);
    call("A15", "PasteSpecial", vec![AutoValue::Missing, AutoValue::Missing, AutoValue::Bool(false)]).unwrap();
    assert_eq!([value("A15"), value("B15"), value("C15")], ["5", "", "7"]);
    // Worksheet.PasteSpecial pastes the clipboard at the selection; Link and other formats are refused typed.
    let sheet = obj(invoke(&model, &model.application(), "ActiveSheet", vec![]).unwrap());
    select_cells(&host, (20, 1), (20, 1));
    model.context_changed(&host.session.borrow());
    invoke_kind(&model, &sheet, "PasteSpecial", InvokeKind::Method, vec![]).unwrap();
    assert_eq!([value("A20"), value("C20")], ["5", "7"]);
    for args in [vec![AutoValue::text("Text")], vec![AutoValue::Missing, AutoValue::Bool(true)]] {
        assert!(matches!(
            invoke_kind(&model, &sheet, "PasteSpecial", InvokeKind::Method, args),
            Err(AutoError::NotModelled { .. })
        ));
    }
}

/// dc-jtl4m: Worksheets.Add Count, worksheet-scoped Names.Add and a Sort of one cell (its current region).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_jtl4m__add_count_scoped_names_and_one_cell_sort() {
    let (host, model) = setup();
    let sheets = worksheets(&model);
    let first = sheet_names(&host)[0].clone();
    let before = sheet_names(&host).len();
    let added = obj(invoke_kind(&model, &sheets, "Add", InvokeKind::Method, vec![AutoValue::Missing, AutoValue::Missing, AutoValue::I4(3)]).unwrap());
    let names = sheet_names(&host);
    assert_eq!(names.len(), before + 3);
    assert_eq!(names[3], first, "three new sheets sit before the active sheet");
    assert_eq!(text(&model, &added, "Name"), names[0], "the first new sheet is returned");
    for bad in [0, -1, 256] {
        let reply = invoke_kind(&model, &sheets, "Add", InvokeKind::Method, vec![AutoValue::Missing, AutoValue::Missing, AutoValue::I4(bad)]);
        assert!(matches!(reply, Err(AutoError::Excel { number: 1004, .. })), "{reply:?}");
    }
    assert_eq!(sheet_names(&host).len(), before + 3);
    // Scoped names: "Sheet!Name" belongs to that sheet; redefining keeps one; an unknown sheet is 1004.
    let book = obj(invoke(&model, &model.application(), "ActiveWorkbook", vec![]).unwrap());
    let book_names = obj(invoke(&model, &book, "Names", vec![]).unwrap());
    let add = |name: &str, refers: &str| {
        invoke_kind(&model, &book_names, "Add", InvokeKind::Method, vec![AutoValue::text(name), AutoValue::text(refers)])
    };
    let local = obj(add(&format!("{first}!Local"), "=1").unwrap());
    let shown = text(&model, &local, "Name");
    assert!(shown.ends_with("!Local") || shown == "Local", "{shown}");
    assert!(matches!(add("NoSuchSheet!Local", "=1"), Err(AutoError::Excel { number: 1004, .. })));
    let again = obj(add(&format!("{first}!Local"), "=2").unwrap());
    assert_eq!(again.key, local.key, "redefining the scoped name keeps one name");
    // Workbook.Names lists the sheet-local name as "<sheet>!Local" and Item finds it by that text.
    let count = |names: &ObjectRef| match invoke(&model, names, "Count", vec![]).unwrap() {
        AutoValue::I4(n) => n,
        other => panic!("{other:?}"),
    };
    let before_global = count(&book_names);
    assert_eq!(shown, format!("{first}!Local"));
    add("Global1", "=2").unwrap();
    assert_eq!(count(&book_names), before_global + 1);
    let found = obj(invoke_kind(&model, &book_names, "Item", InvokeKind::Method, vec![AutoValue::text(&format!("{first}!Local"))]).unwrap());
    assert_eq!(found.key, local.key);
    assert_eq!(text(&model, &found, "Name"), format!("{first}!Local"));
    assert!(invoke_kind(&model, &book_names, "Item", InvokeKind::Method, vec![AutoValue::text("Local")]).is_err(), "a bare local name is not in the workbook list");
    // A Sort of one cell sorts its current region.
    let put = |address: &str, value: AutoValue| {
        invoke_kind(&model, &range(&model, address), "Value2", InvokeKind::Put, vec![value]).unwrap();
    };
    // The first new sheet is the active one and is empty.
    for (row, value) in [(3., "c"), (1., "a"), (2., "b")].into_iter().enumerate() {
        put(&format!("A{}", row + 2), AutoValue::R8(value.0));
        put(&format!("B{}", row + 2), AutoValue::text(value.1));
    }
    let key = AutoValue::Object(range(&model, "A2"));
    invoke_kind(&model, &range(&model, "B3"), "Sort", InvokeKind::Method, vec![key]).unwrap();
    assert_eq!(cell_text(&model, "B2").as_text().unwrap(), "a");
    assert!(matches!(cell_text(&model, "A4"), AutoValue::R8(3.)));
    // A lone cell with no neighbours has no region to sort: 1004.
    put("H20", AutoValue::R8(1.));
    let key = AutoValue::Object(range(&model, "H20"));
    let reply = invoke_kind(&model, &range(&model, "H20"), "Sort", InvokeKind::Method, vec![key]);
    assert!(matches!(reply, Err(AutoError::Excel { number: 1004, .. })), "{reply:?}");
}

/// dc-oxh8w.43: Worksheet.Names lists only the names scoped to that worksheet; Add on it scopes a bare name to it.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_43__worksheet_names_lists_the_sheet_scoped_names() {
    let (host, model) = setup();
    let first = sheet_names(&host)[0].clone();
    let book = obj(invoke(&model, &model.application(), "ActiveWorkbook", vec![]).unwrap());
    let book_names = obj(invoke(&model, &book, "Names", vec![]).unwrap());
    let sheet = obj(invoke_kind(&model, &obj(invoke(&model, &book, "Worksheets", vec![]).unwrap()), "Item", InvokeKind::Get, vec![AutoValue::I4(1)]).unwrap());
    let sheet_scoped = obj(invoke(&model, &sheet, "Names", vec![]).unwrap());
    let count = |names: &ObjectRef| match invoke(&model, names, "Count", vec![]).unwrap() {
        AutoValue::I4(n) => n,
        other => panic!("{other:?}"),
    };
    assert_eq!(count(&sheet_scoped), 0);
    invoke_kind(&model, &book_names, "Add", InvokeKind::Method, vec![AutoValue::text("Global1"), AutoValue::text("=1")]).unwrap();
    assert_eq!(count(&sheet_scoped), 0, "a workbook name is not on the sheet's list");
    let added = obj(invoke_kind(&model, &sheet_scoped, "Add", InvokeKind::Method, vec![AutoValue::text("Local"), AutoValue::text("=2")]).unwrap());
    assert_eq!(text(&model, &added, "Name"), format!("{first}!Local"));
    assert_eq!(count(&sheet_scoped), 1);
    assert_eq!(count(&book_names), 2);
    let found = obj(invoke_kind(&model, &sheet_scoped, "Item", InvokeKind::Method, vec![AutoValue::text(&format!("{first}!Local"))]).unwrap());
    assert_eq!(found.key, added.key);
    assert!(invoke_kind(&model, &sheet_scoped, "Item", InvokeKind::Method, vec![AutoValue::text("Global1")]).is_err());
    let parent = obj(invoke(&model, &sheet_scoped, "Parent", vec![]).unwrap());
    assert_eq!(text(&model, &parent, "Name"), first);
}

/// dc-oxh8w.22: Chart.Export counts its arguments (1004), answers False for a filter it has no
/// encoder for, refuses while no rasteriser is installed (typed, owner dc-oxh8w.22), and otherwise draws the
/// chart at its frame's size at 100% (300 x 200 points = 400 x 267 pixels) and writes the bytes it gets
/// through the application host, by the filter name or else the file extension.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_22__chart_export_writes_the_drawn_chart_through_the_host() {
    let (host, model, sheet) = blank_report();
    fill(&host, (1, 1), &[&["Region", "Q1"], &["North", "10"], &["South", "30"]]);
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
    set_source(&model, &chart, "A1:B3");
    let export = |args: Vec<AutoValue>| invoke_kind(&model, &chart, "Export", InvokeKind::Method, args);
    for args in [vec![], vec![AutoValue::text("a.png"), AutoValue::text("PNG"), AutoValue::Bool(false), AutoValue::I4(1)]] {
        let bad = export(args);
        assert!(matches!(bad, Err(AutoError::Status(-2147352562))), "{bad:?}");
    }
    let nothing_installed = export(vec![AutoValue::text("a.png"), AutoValue::text("PNG")]);
    assert!(matches!(nothing_installed, Err(AutoError::NotModelled { .. })), "{nothing_installed:?}");
    assert!(host.files.borrow().is_empty());

    fn stub(_: &ChartView, width: u32, height: u32, format: chart::ExportFormat) -> Result<Vec<u8>, String> {
        Ok(format!("{format:?} {width}x{height}").into_bytes())
    }
    chart::set_chart_exporter(stub);
    let unknown = export(vec![AutoValue::text("a.bmp"), AutoValue::text("BMP")]);
    assert!(matches!(unknown, Ok(AutoValue::Bool(false))), "{unknown:?}");
    assert!(matches!(export(vec![AutoValue::text("a.png"), AutoValue::text("png")]), Ok(AutoValue::Bool(true))));
    assert!(matches!(export(vec![AutoValue::text("b.gif")]), Ok(AutoValue::Bool(true))));
    assert!(matches!(export(vec![AutoValue::text("c.dat"), AutoValue::text("JPG")]), Ok(AutoValue::Bool(true))));
    let files = host.files.borrow();
    let written: Vec<(&str, &str)> =
        files.iter().map(|(p, b)| (p.as_str(), std::str::from_utf8(b).unwrap())).collect();
    assert_eq!(written, [("a.png", "Png 400x267"), ("b.gif", "Gif 400x267"), ("c.dat", "Jpg 400x267")]);
}

fn interior(model: &WorkbookAutomation, address: &str) -> ObjectRef {
    let target = range(model, address);
    let interior = obj(invoke(model, &target, "Interior", vec![]).unwrap());
    model.released(&target);
    interior
}

fn i4(model: &WorkbookAutomation, target: &ObjectRef, member: &str) -> i32 {
    match invoke(model, target, member, vec![]).unwrap() {
        AutoValue::I4(n) => n,
        other => panic!("{member} returned {other:?}"),
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_33_interior_defaults_report_no_fill() {
    let (_, model, _) = blank_report();
    let cell = interior(&model, "B2");
    assert_eq!(i4(&model, &cell, "Color"), 16_777_215);
    assert_eq!(i4(&model, &cell, "ColorIndex"), -4142);
    assert_eq!(i4(&model, &cell, "Pattern"), -4142);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_33_interior_color_is_solid_and_survives_reopen() {
    let (host, model, _) = blank_report();
    let cells = interior(&model, "B2:C3");
    // RGB(255, 128, 0) in VBA's byte order.
    put_font(&model, &cells, "Color", AutoValue::I4(255 | 128 << 8)).unwrap();
    assert_eq!(i4(&model, &cells, "Color"), 255 | 128 << 8);
    assert_eq!(i4(&model, &cells, "Pattern"), 1);
    let mixed = interior(&model, "B2:D3");
    assert!(matches!(
        invoke(&model, &mixed, "Color", vec![]).unwrap(),
        AutoValue::Null
    ));
    for bad in [AutoValue::I4(-1), AutoValue::I4(0x0100_0000)] {
        assert_eq!(excel_error(put_font(&model, &cells, "Color", bad)).0, 1004);
    }
    assert_eq!(
        excel_error(invoke(&model, &cells, "Color", vec![AutoValue::I4(1)])).0,
        1004
    );
    let reopened = reopen_formatting(&host);
    let cell = interior(&reopened, "C3");
    assert_eq!(i4(&reopened, &cell, "Color"), 255 | 128 << 8);
    assert_eq!(i4(&reopened, &cell, "Pattern"), 1);
    assert_eq!(i4(&reopened, &interior(&reopened, "D3"), "Pattern"), -4142);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_33_interior_color_index_uses_the_palette() {
    let (host, model, _) = blank_report();
    let cell = interior(&model, "E5");
    put_font(&model, &cell, "ColorIndex", AutoValue::I4(3)).unwrap();
    assert_eq!(i4(&model, &cell, "ColorIndex"), 3);
    assert_eq!(i4(&model, &cell, "Color"), 255);
    assert_eq!(i4(&model, &cell, "Pattern"), 1);
    for bad in [0, 57, -4143] {
        assert_eq!(
            excel_error(put_font(&model, &cell, "ColorIndex", AutoValue::I4(bad))).0,
            1004
        );
    }
    let reopened = reopen_formatting(&host);
    assert_eq!(i4(&reopened, &interior(&reopened, "E5"), "ColorIndex"), 3);
    put_font(&model, &cell, "ColorIndex", AutoValue::I4(-4142)).unwrap();
    assert_eq!(i4(&model, &cell, "Pattern"), -4142);
    assert_eq!(i4(&model, &cell, "Color"), 16_777_215);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_33_interior_pattern_keeps_color_and_pattern_color() {
    let (host, model, _) = blank_report();
    let cell = interior(&model, "A7");
    put_font(&model, &cell, "Color", AutoValue::I4(0x00FF00)).unwrap();
    put_font(&model, &cell, "Pattern", AutoValue::I4(-4126)).unwrap();
    assert_eq!(i4(&model, &cell, "Pattern"), -4126);
    assert_eq!(i4(&model, &cell, "Color"), 0x00FF00);
    put_font(&model, &cell, "PatternColor", AutoValue::I4(0xFF0000)).unwrap();
    assert_eq!(i4(&model, &cell, "PatternColor"), 0xFF0000);
    assert_eq!(i4(&model, &cell, "Color"), 0x00FF00);
    assert_eq!(
        excel_error(put_font(&model, &cell, "Pattern", AutoValue::I4(99))).0,
        1004
    );
    let reopened = reopen_formatting(&host);
    let cell = interior(&reopened, "A7");
    assert_eq!(i4(&reopened, &cell, "Pattern"), -4126);
    assert_eq!(i4(&reopened, &cell, "Color"), 0x00FF00);
    assert_eq!(i4(&reopened, &cell, "PatternColor"), 0xFF0000);
}

fn border(model: &WorkbookAutomation, address: &str, index: i32) -> ObjectRef {
    let target = range(model, address);
    let borders = obj(invoke(model, &target, "Borders", vec![]).unwrap());
    let one = obj(invoke(model, &borders, "Item", vec![AutoValue::I4(index)]).unwrap());
    model.released(&target);
    one
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_33_border_edges_read_defaults_and_refuse_bad_indexes() {
    let (_, model, _) = blank_report();
    let target = range(&model, "B2");
    let borders = obj(invoke(&model, &target, "Borders", vec![]).unwrap());
    for bad in [0, 4, 13] {
        assert_eq!(excel_error(invoke(&model, &borders, "Item", vec![AutoValue::I4(bad)])).0, 1004);
    }
    assert_eq!(excel_error(invoke(&model, &borders, "Item", vec![])).0, 1004);
    let edge = border(&model, "B2", 7);
    assert_eq!(i4(&model, &edge, "LineStyle"), -4142);
    assert_eq!(i4(&model, &edge, "Weight"), 2);
    assert_eq!(i4(&model, &edge, "Color"), 0);
    // ww23 (Excel 16.0.20625): an edge without a line reads ColorIndex xlNone.
    assert_eq!(i4(&model, &edge, "ColorIndex"), -4142);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_33_border_edge_of_a_range_marks_only_that_side_and_the_neighbour() {
    let (host, model, _) = blank_report();
    let bottom = border(&model, "B2:C3", 9);
    put_font(&model, &bottom, "LineStyle", AutoValue::I4(1)).unwrap();
    put_font(&model, &bottom, "Weight", AutoValue::I4(-4138)).unwrap();
    put_font(&model, &bottom, "Color", AutoValue::I4(255)).unwrap();
    assert_eq!(i4(&model, &bottom, "LineStyle"), 1);
    assert_eq!(i4(&model, &bottom, "Weight"), -4138);
    assert_eq!(i4(&model, &bottom, "Color"), 255);
    // The cells above the bottom row have no bottom border; the row below shares it as its top.
    assert_eq!(i4(&model, &border(&model, "B2:C2", 9), "LineStyle"), -4142);
    assert_eq!(i4(&model, &border(&model, "B3", 8), "LineStyle"), -4142);
    assert_eq!(i4(&model, &border(&model, "B4:C4", 8), "LineStyle"), 1);
    assert_eq!(i4(&model, &border(&model, "B3", 7), "LineStyle"), -4142);
    put_font(&model, &border(&model, "C3", 9), "LineStyle", AutoValue::I4(-4142)).unwrap();
    let mixed = border(&model, "B3:C3", 9);
    assert!(matches!(invoke(&model, &mixed, "LineStyle", vec![]).unwrap(), AutoValue::Null));
    put_font(&model, &border(&model, "C3", 9), "LineStyle", AutoValue::I4(1)).unwrap();
    put_font(&model, &border(&model, "C3", 9), "Weight", AutoValue::I4(-4138)).unwrap();
    put_font(&model, &border(&model, "C3", 9), "Color", AutoValue::I4(255)).unwrap();
    for bad in [3, 99] {
        assert_eq!(excel_error(put_font(&model, &bottom, "LineStyle", AutoValue::I4(bad))).0, 1004);
    }
    assert_eq!(excel_error(put_font(&model, &bottom, "Weight", AutoValue::I4(3))).0, 1004);
    assert_eq!(excel_error(put_font(&model, &bottom, "ColorIndex", AutoValue::I4(0))).0, 1004);
    let reopened = reopen_formatting(&host);
    let edge = border(&reopened, "C3", 9);
    assert_eq!(i4(&reopened, &edge, "LineStyle"), 1);
    assert_eq!(i4(&reopened, &edge, "Weight"), -4138);
    assert_eq!(i4(&reopened, &edge, "Color"), 255);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_33_border_line_style_and_weight_combine() {
    let (_, model, _) = blank_report();
    let edge = border(&model, "E5", 10);
    put_font(&model, &edge, "LineStyle", AutoValue::I4(-4119)).unwrap();
    assert_eq!(i4(&model, &edge, "LineStyle"), -4119);
    assert_eq!(i4(&model, &edge, "Weight"), 4);
    put_font(&model, &edge, "LineStyle", AutoValue::I4(-4115)).unwrap();
    put_font(&model, &edge, "Weight", AutoValue::I4(-4138)).unwrap();
    assert_eq!(i4(&model, &edge, "LineStyle"), -4115);
    assert_eq!(i4(&model, &edge, "Weight"), -4138);
    put_font(&model, &edge, "LineStyle", AutoValue::I4(-4142)).unwrap();
    assert_eq!(i4(&model, &edge, "LineStyle"), -4142);
    // A weight set on an edge without a line draws one.
    put_font(&model, &edge, "Weight", AutoValue::I4(4)).unwrap();
    assert_eq!(i4(&model, &edge, "LineStyle"), 1);
    assert_eq!(i4(&model, &edge, "Weight"), 4);
}

/// ww23 (Excel 16.0.20625.20108): a Weight set on an edge whose line style has no such weight makes the edge
/// continuous; dash-dot, dash-dot-dot and slant-dash-dot keep their style at their own weights; Borders.Count is 6.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_33__weight_on_a_dashed_edge_and_borders_count() {
    let (_, model, _) = blank_report();
    // (LineStyle, Weight) -> (LineStyle, Weight) read back.
    let cases = [
        (-4115, 1, 1, 1),
        (-4115, 4, 1, 4),
        (-4118, -4138, 1, -4138),
        (-4118, 4, 1, 4),
        (-4119, 2, 1, 2),
        (-4119, -4138, 1, -4138),
        (-4119, 1, 1, 1),
        (13, 2, 1, 2),
        (13, -4138, 13, -4138),
        (4, 2, 4, 2),
        (5, 2, 5, 2),
    ];
    for (n, (line, weight, want_line, want_weight)) in cases.into_iter().enumerate() {
        let edge = border(&model, &format!("H{}", n + 2), 7);
        put_font(&model, &edge, "LineStyle", AutoValue::I4(line)).unwrap();
        put_font(&model, &edge, "Weight", AutoValue::I4(weight)).unwrap();
        assert_eq!(i4(&model, &edge, "LineStyle"), want_line, "ls {line} w {weight}");
        assert_eq!(i4(&model, &edge, "Weight"), want_weight, "ls {line} w {weight}");
    }
    // Colour alone on an edge with no line draws a continuous line at weight 2 (thin).
    let edge = border(&model, "F4", 7);
    put_font(&model, &edge, "Color", AutoValue::I4(255)).unwrap();
    assert_eq!(i4(&model, &edge, "LineStyle"), 1);
    assert_eq!(i4(&model, &edge, "Weight"), 2);
    assert_eq!(i4(&model, &edge, "Color"), 255);
    for address in ["B2:C3", "B2"] {
        let all = obj(invoke(&model, &range(&model, address), "Borders", vec![]).unwrap());
        assert_eq!(i4(&model, &all, "Count"), 6, "{address}");
    }
}

/// ww23 (Excel 16.0.20625.20108): an RGB colour outside the palette reads the nearest ColorIndex (0x123456 -> 52);
/// Pattern xlAutomatic (-4105) reads xlSolid (1); solid red reads ColorIndex 3.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_33__interior_nearest_palette_and_automatic_pattern() {
    let (_, model, _) = blank_report();
    let red = interior(&model, "B3");
    put_font(&model, &red, "Color", AutoValue::I4(255)).unwrap();
    assert_eq!(i4(&model, &red, "ColorIndex"), 3);
    let odd = interior(&model, "B4");
    put_font(&model, &odd, "Color", AutoValue::I4(0x123456)).unwrap();
    assert_eq!(i4(&model, &odd, "Color"), 0x123456);
    assert_eq!(i4(&model, &odd, "ColorIndex"), 52);
    let auto = interior(&model, "B5");
    put_font(&model, &auto, "Pattern", AutoValue::I4(-4105)).unwrap();
    assert_eq!(i4(&model, &auto, "Pattern"), 1);
    assert_eq!(i4(&model, &auto, "Color"), 16_777_215);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_33_borders_collection_and_inside_edges() {
    let (host, model, _) = blank_report();
    let all = obj(invoke(&model, &range(&model, "B2:C3"), "Borders", vec![]).unwrap());
    put_font(&model, &all, "LineStyle", AutoValue::I4(1)).unwrap();
    assert_eq!(i4(&model, &all, "LineStyle"), 1);
    assert_eq!(i4(&model, &border(&model, "B2:C3", 11), "LineStyle"), 1);
    assert_eq!(i4(&model, &border(&model, "C2", 7), "LineStyle"), 1);
    put_font(&model, &border(&model, "B2:C3", 12), "LineStyle", AutoValue::I4(-4142)).unwrap();
    assert!(matches!(invoke(&model, &all, "LineStyle", vec![]).unwrap(), AutoValue::Null));
    assert_eq!(excel_error(put_font(&model, &border(&model, "B2", 11), "LineStyle", AutoValue::I4(1))).0, 1004);
    let reopened = reopen_formatting(&host);
    assert_eq!(i4(&reopened, &border(&reopened, "B2:C3", 11), "LineStyle"), 1);
    assert_eq!(i4(&reopened, &border(&reopened, "B2:C3", 12), "LineStyle"), -4142);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_33_border_around_draws_the_perimeter() {
    let (host, model, _) = blank_report();
    let cells = range(&model, "B2:D4");
    let around = |args: Vec<AutoValue>| invoke_kind(&model, &cells, "BorderAround", InvokeKind::Method, args);
    around(vec![AutoValue::I4(1), AutoValue::I4(4), AutoValue::I4(3)]).unwrap();
    for (address, index) in [("B2:B4", 7), ("D2:D4", 10), ("B2:D2", 8), ("B4:D4", 9)] {
        let edge = border(&model, address, index);
        assert_eq!(i4(&model, &edge, "LineStyle"), 1);
        assert_eq!(i4(&model, &edge, "Weight"), 4);
        assert_eq!(i4(&model, &edge, "ColorIndex"), 3);
    }
    assert_eq!(i4(&model, &border(&model, "C3", 7), "LineStyle"), -4142);
    assert_eq!(excel_error(around(vec![AutoValue::I4(7)])).0, 1004);
    assert_eq!(excel_error(around(vec![AutoValue::I4(1), AutoValue::I4(9)])).0, 1004);
    let reopened = reopen_formatting(&host);
    assert_eq!(i4(&reopened, &border(&reopened, "D3", 10), "Weight"), 4);
    assert_eq!(i4(&reopened, &border(&reopened, "C3", 10), "LineStyle"), -4142);
}

fn styled_report() -> (Rc<Host>, Rc<WorkbookAutomation>) {
    let session = HostSession::open(
        Arc::new(MemorySource::new(
            include_bytes!("../../calc-doc/tests/fixtures/dc_5sr/features.xlsx").to_vec(),
        )),
        &OpenPolicy::default(),
        HostOptions::default(),
    )
    .unwrap();
    setup_session(session)
}

fn style_name(model: &WorkbookAutomation, address: &str) -> String {
    let target = range(model, address);
    match invoke(model, &target, "Style", vec![]).unwrap() {
        AutoValue::Object(style) => text(model, &style, "Name"),
        AutoValue::Null => "<mixed>".into(),
        other => panic!("Style returned {other:?}"),
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_33_range_style_applies_a_named_style_and_survives_reopen() {
    let (host, model) = styled_report();
    assert_eq!(style_name(&model, "A1"), "Normal");
    let normal = obj(invoke(&model, &range(&model, "A1"), "Style", vec![]).unwrap());
    assert!(matches!(invoke(&model, &normal, "BuiltIn", vec![]).unwrap(), AutoValue::Bool(true)));
    assert_eq!(text(&model, &normal, "NumberFormat"), "General");
    let cells = range(&model, "B2:B3");
    invoke_kind(&model, &cells, "Style", InvokeKind::Put, vec![AutoValue::text("hyperlink")]).unwrap();
    assert_eq!(style_name(&model, "B2:B3"), "Hyperlink");
    assert_eq!(style_name(&model, "A2:B3"), "<mixed>");
    assert_eq!(i4(&model, &font(&model, "B2"), "Underline"), 2);
    // A Style object applies too.
    let link = obj(invoke(&model, &range(&model, "B2"), "Style", vec![]).unwrap());
    invoke_kind(&model, &range(&model, "D4"), "Style", InvokeKind::Put, vec![AutoValue::Object(link)]).unwrap();
    assert_eq!(style_name(&model, "D4"), "Hyperlink");
    for bad in [AutoValue::text("No such style"), AutoValue::I4(3)] {
        assert_eq!(excel_error(invoke_kind(&model, &cells, "Style", InvokeKind::Put, vec![bad])).0, 1004);
    }
    // Applying the style replaces direct formatting.
    put_font(&model, &font(&model, "B3"), "Bold", AutoValue::Bool(true)).unwrap();
    invoke_kind(&model, &range(&model, "B3"), "Style", InvokeKind::Put, vec![AutoValue::text("Normal")]).unwrap();
    assert!(matches!(invoke(&model, &font(&model, "B3"), "Bold", vec![]).unwrap(), AutoValue::Bool(false)));
    let reopened = reopen_formatting(&host);
    assert_eq!(style_name(&reopened, "B2"), "Hyperlink");
    assert_eq!(style_name(&reopened, "D4"), "Hyperlink");
    assert_eq!(style_name(&reopened, "B3"), "Normal");
}

fn merge_call(model: &WorkbookAutomation, address: &str, member: &str, kind: InvokeKind, args: Vec<AutoValue>) -> Reply {
    invoke_kind(model, &range(model, address), member, kind, args)
}

fn merged(model: &WorkbookAutomation, address: &str) -> AutoValue {
    invoke(model, &range(model, address), "MergeCells", vec![]).unwrap()
}

fn merge_area_address(model: &WorkbookAutomation, address: &str) -> String {
    let area = obj(invoke(model, &range(model, address), "MergeArea", vec![]).unwrap());
    text(model, &area, "Address")
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oc78o_merge_keeps_the_upper_left_value_and_survives_reopen() {
    let (host, model, _) = blank_report();
    for (address, value) in [("B2", 1.), ("C2", 2.), ("B3", 3.), ("C3", 4.)] {
        merge_call(&model, address, "Value2", InvokeKind::Put, vec![AutoValue::R8(value)]).unwrap();
    }
    assert!(matches!(merged(&model, "B2"), AutoValue::Bool(false)));
    merge_call(&model, "B2:C3", "Merge", InvokeKind::Method, vec![]).unwrap();
    assert!(matches!(merged(&model, "C3"), AutoValue::Bool(true)));
    assert!(matches!(merged(&model, "B2:C3"), AutoValue::Bool(true)));
    assert!(matches!(merged(&model, "B2:D3"), AutoValue::Null));
    assert!(matches!(merged(&model, "A1"), AutoValue::Bool(false)));
    assert_eq!(merge_area_address(&model, "C3"), "$B$2:$C$3");
    assert_eq!(merge_area_address(&model, "A1"), "$A$1");
    assert!(matches!(cell_text(&model, "B2"), AutoValue::R8(1.)));
    for covered in ["C2", "B3", "C3"] {
        assert!(matches!(cell_text(&model, covered), AutoValue::Empty), "{covered}");
    }
    let reopened = reopen_formatting(&host);
    assert!(matches!(merged(&reopened, "C2"), AutoValue::Bool(true)));
    assert_eq!(merge_area_address(&reopened, "B3"), "$B$2:$C$3");
    assert!(matches!(cell_text(&reopened, "B2"), AutoValue::R8(1.)));
    assert!(matches!(merged(&reopened, "D2"), AutoValue::Bool(false)));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oc78o_merge_across_merges_each_row_and_unmerge_removes_it() {
    let (host, model, _) = blank_report();
    merge_call(&model, "B5:D6", "Merge", InvokeKind::Method, vec![AutoValue::Bool(true)]).unwrap();
    assert_eq!(merge_area_address(&model, "D6"), "$B$6:$D$6");
    assert_eq!(merge_area_address(&model, "C5"), "$B$5:$D$5");
    assert!(matches!(merged(&model, "B5:D6"), AutoValue::Bool(true)));
    // MergeCells:=False unmerges whatever meets the range.
    merge_call(&model, "C5", "MergeCells", InvokeKind::Put, vec![AutoValue::Bool(false)]).unwrap();
    assert!(matches!(merged(&model, "B5:D5"), AutoValue::Bool(false)));
    assert!(matches!(merged(&model, "B6"), AutoValue::Bool(true)));
    let reopened = reopen_formatting(&host);
    assert!(matches!(merged(&reopened, "B5"), AutoValue::Bool(false)));
    assert_eq!(merge_area_address(&reopened, "C6"), "$B$6:$D$6");
    merge_call(&reopened, "A1:Z9", "UnMerge", InvokeKind::Method, vec![]).unwrap();
    assert!(matches!(merged(&reopened, "B6"), AutoValue::Bool(false)));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oc78o_merge_arguments_and_array_overlap_are_error_1004() {
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
    for args in [vec![AutoValue::text("x")], vec![AutoValue::Bool(true), AutoValue::Bool(true)]] {
        assert_eq!(excel_error(merge_call(&model, "B2:C3", "Merge", InvokeKind::Method, args)).0, 1004);
    }
    assert_eq!(excel_error(merge_call(&model, "B2:C3", "MergeCells", InvokeKind::Put, vec![AutoValue::text("x")])).0, 1004);
    assert_eq!(excel_error(merge_call(&model, "B2:C3", "UnMerge", InvokeKind::Method, vec![AutoValue::Bool(true)])).0, 1004);
    assert_eq!(excel_error(merge_call(&model, "B2:C3", "MergeCells", InvokeKind::Get, vec![AutoValue::Bool(true)])).0, 1004);
    // A merge across part of an array formula is refused.
    let sheet = host.session.borrow().host().sheets()[0].id();
    enter(&host, CellKey::new(sheet, 2, 5).unwrap(), "=SEQUENCE(2)");
    assert_eq!(excel_error(merge_call(&model, "E2:F3", "Merge", InvokeKind::Method, vec![])).0, 1004);
    assert!(matches!(merged(&model, "E2:F3"), AutoValue::Bool(false)));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oc78o_authored_merges_move_with_inserted_rows_and_undo_restores_them() {
    let (host, model, _) = blank_report();
    merge_call(&model, "B3:C4", "Merge", InvokeKind::Method, vec![]).unwrap();
    merge_call(&model, "1:1", "Insert", InvokeKind::Method, vec![]).unwrap();
    assert_eq!(merge_area_address(&model, "B5"), "$B$4:$C$5");
    assert!(matches!(merged(&model, "B3"), AutoValue::Bool(false)));
    let reopened = reopen_formatting(&host);
    assert_eq!(merge_area_address(&reopened, "C5"), "$B$4:$C$5");
    assert!(
        host.session
            .borrow_mut()
            .dispatch(SkinIntent::Undo)
            .accepted()
    );
    assert_eq!(merge_area_address(&model, "C4"), "$B$3:$C$4");
    assert!(
        host.session
            .borrow_mut()
            .dispatch(SkinIntent::Undo)
            .accepted()
    );
    assert!(matches!(merged(&model, "B3:C4"), AutoValue::Bool(false)));
}

/// dc-oxh8w.23: a loaded chart sheet is a Chart in Sheets and Charts, not a Worksheet: Worksheets counts only the
/// worksheets (the fixture's Data and After), Charts only the chart sheet, Sheets all four (the dialog sheet
/// is a DialogSheet in Excel and refuses typed here). Indexes count within the collection, a name
/// of another kind is Excel's subscript error, and the chart answers its members.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_23__chart_sheets_are_charts_in_sheets_and_charts_collections() {
    let session = HostSession::open(
        Arc::new(MemorySource::new(
            include_bytes!("../../calc-doc/tests/fixtures/preserved_sheets/chart-and-dialog-sheets.xlsx").to_vec(),
        )),
        &OpenPolicy::default(),
        HostOptions::default(),
    )
    .unwrap();
    let (_host, model) = setup_session(session);
    let book = obj(invoke(&model, &model.application(), "ActiveWorkbook", vec![]).unwrap());
    let collection = |name: &str| obj(invoke(&model, &book, name, vec![]).unwrap());
    let count = |c: &ObjectRef| int(invoke(&model, c, "Count", vec![]).unwrap());
    let (worksheets, charts, sheets) = (collection("Worksheets"), collection("Charts"), collection("Sheets"));
    assert_eq!((count(&worksheets), count(&charts), count(&sheets)), (2, 1, 4));
    let item = |c: &ObjectRef, key: AutoValue| invoke_kind(&model, c, "Item", InvokeKind::Get, vec![key]);
    // The chart, by place and name, through Charts and Sheets.
    for (c, key) in [
        (&charts, AutoValue::I4(1)),
        (&charts, AutoValue::text("Chart1")),
        (&sheets, AutoValue::text("Chart1")),
        (&sheets, AutoValue::I4(2)),
    ] {
        let chart = obj(item(c, key.clone()).unwrap_or_else(|e| panic!("{key:?}: {e:?}")));
        let kind = invoke(&model, &chart, "ChartType", vec![]);
        assert!(matches!(kind, Ok(AutoValue::I4(51))), "{kind:?}");
        let _ = chart;
    }
    // Worksheets counts worksheets only: Worksheets(2) is After, Worksheets("Chart1") is out of range.
    let after = obj(item(&worksheets, AutoValue::I4(2)).unwrap());
    let name = invoke(&model, &after, "Name", vec![]).unwrap();
    assert!(matches!(&name, AutoValue::Text(t) if t.to_string_lossy() == "After"), "{name:?}");
    for (c, key) in [
        (&worksheets, AutoValue::text("Chart1")),
        (&worksheets, AutoValue::I4(3)),
        (&charts, AutoValue::I4(2)),
        (&charts, AutoValue::text("Data")),
    ] {
        let bad = item(c, key.clone());
        assert!(matches!(bad, Err(AutoError::Status(s)) if s == 0x8002_000Bu32 as i32), "{key:?}: {bad:?}");
    }
    let dialog = item(&sheets, AutoValue::I4(3));
    assert!(matches!(dialog, Err(AutoError::NotModelled { .. })), "{dialog:?}");
    let bad = invoke_kind(&model, &charts, "Item", InvokeKind::Get, vec![]);
    assert!(bad.is_err());
}

/// The text of a Text value.
fn text_of(value: AutoValue) -> String {
    match value {
        AutoValue::Text(t) => t.to_string_lossy().to_string(),
        other => panic!("text: {other:?}"),
    }
}

/// dc-oxh8w.23: `Charts.Add` makes a chart sheet (Excel's `Charts.Add(Before, After, Count)`): the new sheet is a
/// Chart in Sheets and Charts, not in Worksheets, it becomes the active sheet (ActiveSheet is a Chart), sits
/// before the active sheet, is named `Chart1`, and `Sheets.Add(Type:=xlChart)` makes the same. Counts and types
/// that Excel refuses give 1004.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_23__charts_add_makes_chart_sheets_in_sheets_not_worksheets() {
    let (_host, model, _sheet) = blank_report();
    let book = obj(invoke(&model, &model.application(), "ActiveWorkbook", vec![]).unwrap());
    let collection = |name: &str| obj(invoke(&model, &book, name, vec![]).unwrap());
    let count = |c: &ObjectRef| int(invoke(&model, c, "Count", vec![]).unwrap());
    let (worksheets, charts, sheets) = (collection("Worksheets"), collection("Charts"), collection("Sheets"));
    assert_eq!((count(&worksheets), count(&charts), count(&sheets)), (1, 0, 1));
    let chart = obj(invoke_kind(&model, &charts, "Add", InvokeKind::Method, vec![]).unwrap());
    assert_eq!((count(&worksheets), count(&charts), count(&sheets)), (1, 1, 2));
    // Before the active sheet, so the chart sheet is first; it is named Chart1 and plots a clustered column chart.
    let first = obj(invoke_kind(&model, &sheets, "Item", InvokeKind::Get, vec![AutoValue::I4(1)]).unwrap());
    assert_eq!(text_of(invoke(&model, &first, "Name", vec![]).unwrap()), "Chart1");
    assert!(matches!(invoke(&model, &chart, "ChartType", vec![]), Ok(AutoValue::I4(51))));
    assert_eq!(text_of(invoke(&model, &chart, "Name", vec![]).unwrap()), "Chart1");
    // The new sheet is the active one, and it is a Chart.
    let active = obj(invoke(&model, &model.application(), "ActiveSheet", vec![]).unwrap());
    assert_eq!(text_of(invoke(&model, &active, "Name", vec![]).unwrap()), "Chart1");
    assert!(matches!(invoke(&model, &active, "ChartType", vec![]), Ok(AutoValue::I4(51))));
    // Sheets.Add with xlChart (-4109) makes another; Count 2 makes two; the names count up.
    let more = invoke_kind(&model, &sheets, "Add", InvokeKind::Method, vec![AutoValue::Missing, AutoValue::Missing, AutoValue::I4(2), AutoValue::I4(-4109)]);
    assert!(matches!(more, Ok(AutoValue::Object(_))), "{more:?}");
    assert_eq!((count(&worksheets), count(&charts), count(&sheets)), (1, 3, 4));
    let names: Vec<String> = (1..=4)
        .map(|i| {
            let s = obj(invoke_kind(&model, &sheets, "Item", InvokeKind::Get, vec![AutoValue::I4(i)]).unwrap());
            text_of(invoke(&model, &s, "Name", vec![]).unwrap())
        })
        .collect();
    assert_eq!(names, ["Chart2", "Chart3", "Chart1", "Sheet1"]);
    // Errors are 1004: a count outside 1..=255, a Before and an After together.
    for args in [
        vec![AutoValue::Missing, AutoValue::Missing, AutoValue::I4(0)],
        vec![AutoValue::Missing, AutoValue::Missing, AutoValue::I4(256)],
        vec![AutoValue::Object(first.clone()), AutoValue::Object(first.clone())],
    ] {
        let bad = invoke_kind(&model, &charts, "Add", InvokeKind::Method, args.clone());
        assert!(matches!(bad, Err(AutoError::Excel { number: 1004, .. })), "{args:?}: {bad:?}");
    }
    assert_eq!(count(&charts), 3);
}

/// dc-oxh8w.23: a chart sheet a macro makes saves as a chartsheet part (content type, workbook relationship,
/// drawing, chart) and reopens as a chart sheet whose chart answers; its source data and type survive.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_23__a_new_chart_sheet_round_trips_through_save_and_open() {
    let (host, model, _sheet) = blank_report();
    fill(&host, (1, 1), &[&["Region", "Q1"], &["North", "10"], &["South", "30"], &["East", "50"]]);
    let book = obj(invoke(&model, &model.application(), "ActiveWorkbook", vec![]).unwrap());
    let charts = obj(invoke(&model, &book, "Charts", vec![]).unwrap());
    let chart = obj(invoke_kind(&model, &charts, "Add", InvokeKind::Method, vec![AutoValue::Missing, AutoValue::Missing, AutoValue::I4(1)]).unwrap());
    set_source(&model, &chart, "Sheet1!A1:B4");
    invoke_kind(&model, &chart, "ChartType", InvokeKind::Put, vec![AutoValue::I4(57)]).unwrap();
    // The saved package.
    let mut bytes = Vec::new();
    host.session.borrow_mut().save(&SavePolicy::default(), &mut bytes).unwrap();
    let document = calc_doc::open(Arc::new(MemorySource::new(bytes.clone())), &OpenPolicy::default()).expect("the saved package opens");
    let names: Vec<String> = document.package().parts().iter().map(|p| p.name().to_owned()).collect();
    assert!(names.iter().any(|n| n.starts_with("xl/chartsheets/") && n.ends_with(".xml")), "{names:?}");
    let sheet_part = names.iter().find(|n| n.starts_with("xl/chartsheets/") && n.ends_with(".xml")).unwrap();
    let xml = String::from_utf8(document.package().read_part(sheet_part).unwrap()).unwrap();
    assert!(xml.contains("<chartsheet") && xml.contains("<drawing"), "{xml}");
    let drawing = names.iter().find(|n| n.starts_with("xl/drawings/") && n.ends_with(".xml")).expect("a drawing");
    let xml = String::from_utf8(document.package().read_part(drawing).unwrap()).unwrap();
    assert!(xml.contains("absoluteAnchor"), "{xml}");
    assert_eq!(
        document.package().part(sheet_part).and_then(|p| p.content_type()),
        Some("application/vnd.openxmlformats-officedocument.spreadsheetml.chartsheet+xml")
    );
    let workbook = String::from_utf8(document.package().read_part("xl/_rels/workbook.xml.rels").unwrap()).unwrap();
    assert!(workbook.contains("relationships/chartsheet"), "{workbook}");
    // Reopened: Sheets(1) is a Chart named Chart1 with the type and a series over the data.
    let reopened = HostSession::open(Arc::new(MemorySource::new(bytes)), &OpenPolicy::default(), HostOptions::default()).unwrap();
    let (_host2, model2) = setup_session(reopened);
    let book2 = obj(invoke(&model2, &model2.application(), "ActiveWorkbook", vec![]).unwrap());
    let collection = |name: &str| obj(invoke(&model2, &book2, name, vec![]).unwrap());
    let count = |c: &ObjectRef| int(invoke(&model2, c, "Count", vec![]).unwrap());
    assert_eq!((count(&collection("Worksheets")), count(&collection("Charts")), count(&collection("Sheets"))), (1, 1, 2));
    let again = obj(invoke_kind(&model2, &collection("Charts"), "Item", InvokeKind::Get, vec![AutoValue::text("Chart1")]).unwrap());
    assert!(matches!(invoke(&model2, &again, "ChartType", vec![]), Ok(AutoValue::I4(57))));
    let series = obj(invoke_kind(&model2, &again, "SeriesCollection", InvokeKind::Method, vec![]).unwrap());
    assert_eq!(count(&series), 1);
}

/// dc-oxh8w.23: `Chart.Location(xlLocationAsObject, name)` moves a chart sheet's chart onto a worksheet, and
/// `xlLocationAsNewSheet` moves an embedded chart onto a new chart sheet; both return the chart and keep its type.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_23__chart_location_moves_a_chart_between_sheet_and_object() {
    let (host, model, _sheet) = blank_report();
    fill(&host, (1, 1), &[&["Region", "Q1"], &["North", "10"], &["South", "30"]]);
    let book = obj(invoke(&model, &model.application(), "ActiveWorkbook", vec![]).unwrap());
    let collection = |name: &str| obj(invoke(&model, &book, name, vec![]).unwrap());
    let count = |c: &ObjectRef| int(invoke(&model, c, "Count", vec![]).unwrap());
    let charts = collection("Charts");
    let chart = obj(invoke_kind(&model, &charts, "Add", InvokeKind::Method, vec![]).unwrap());
    set_source(&model, &chart, "Sheet1!A1:B3");
    invoke_kind(&model, &chart, "ChartType", InvokeKind::Put, vec![AutoValue::I4(57)]).unwrap();
    // As object on Sheet1: the chart sheet is gone, the worksheet has one chart object.
    let moved = obj(invoke_kind(&model, &chart, "Location", InvokeKind::Method, vec![AutoValue::I4(2), AutoValue::text("Sheet1")]).unwrap());
    assert!(matches!(invoke(&model, &moved, "ChartType", vec![]), Ok(AutoValue::I4(57))));
    assert_eq!((count(&charts), count(&collection("Sheets"))), (0, 1));
    let sheet1 = obj(invoke_kind(&model, &collection("Worksheets"), "Item", InvokeKind::Get, vec![AutoValue::text("Sheet1")]).unwrap());
    let objects = obj(invoke_kind(&model, &sheet1, "ChartObjects", InvokeKind::Method, vec![]).unwrap());
    assert_eq!(count(&objects), 1);
    // As a new sheet named Sales: a chart sheet again, with that name.
    let back = obj(invoke_kind(&model, &moved, "Location", InvokeKind::Method, vec![AutoValue::I4(1), AutoValue::text("Sales")]).unwrap());
    assert!(matches!(invoke(&model, &back, "ChartType", vec![]), Ok(AutoValue::I4(57))));
    assert_eq!(count(&objects), 0);
    assert_eq!(text_of(invoke(&model, &back, "Name", vec![]).unwrap()), "Sales");
    assert_eq!((count(&charts), count(&collection("Sheets"))), (1, 2));
    // Errors: a Where that is neither is 1004; as object onto a sheet that does not exist is 1004.
    let bad = invoke_kind(&model, &back, "Location", InvokeKind::Method, vec![AutoValue::I4(3), AutoValue::text("Sheet1")]);
    assert!(matches!(bad, Err(AutoError::Excel { number: 1004, .. })), "{bad:?}");
    let bad = invoke_kind(&model, &back, "Location", InvokeKind::Method, vec![AutoValue::I4(2), AutoValue::text("Nowhere")]);
    assert!(matches!(bad, Err(AutoError::Excel { number: 1004, .. })), "{bad:?}");
}

/// dc-oxh8w.48: a chart a file holds moves between sheets and the save keeps it. The file (made by
/// `make-fixture.py ... embedded`) has Data with a bar chart object and the chart sheet Chart1 (a column chart).
/// `Chart.Export` of the chart sheet's chart draws its frame (731.8 x 472.4 points = 976 x 630 pixels);
/// `Chart.Location` moves it onto Data (the chart sheet goes), then the object onto a new chart sheet (its
/// frame leaves Data's drawing); the save writes both over the parts the file holds and reopens the same.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_48__file_held_charts_move_between_sheets_and_save() {
    let (host, model) = setup_session(
        HostSession::open(
            Arc::new(MemorySource::new(
                include_bytes!("../../calc-doc/tests/fixtures/preserved_sheets/chart-sheet-and-embedded.xlsx").to_vec(),
            )),
            &OpenPolicy::default(),
            HostOptions::default(),
        )
        .unwrap(),
    );
    let book = obj(invoke(&model, &model.application(), "ActiveWorkbook", vec![]).unwrap());
    let collection = |name: &str| obj(invoke(&model, &book, name, vec![]).unwrap());
    let count = |c: &ObjectRef| int(invoke(&model, c, "Count", vec![]).unwrap());
    let charts = collection("Charts");
    let chart = obj(invoke_kind(&model, &charts, "Item", InvokeKind::Get, vec![AutoValue::I4(1)]).unwrap());
    // Export draws the frame of the chart sheet's chart.
    fn stub(_: &ChartView, width: u32, height: u32, format: chart::ExportFormat) -> Result<Vec<u8>, String> {
        Ok(format!("{format:?} {width}x{height}").into_bytes())
    }
    chart::set_chart_exporter(stub);
    let exported = invoke_kind(&model, &chart, "Export", InvokeKind::Method, vec![AutoValue::text("sheet.png")]);
    assert!(matches!(exported, Ok(AutoValue::Bool(true))), "{exported:?}");
    let written: Vec<(String, String)> =
        host.files.borrow().iter().map(|(p, b)| (p.clone(), String::from_utf8(b.clone()).unwrap())).collect();
    assert_eq!(written, [("sheet.png".to_owned(), "Png 976x630".to_owned())]);
    // Location as object on Data: the chart sheet goes, Data holds two chart objects.
    let moved = obj(invoke_kind(&model, &chart, "Location", InvokeKind::Method, vec![AutoValue::I4(2), AutoValue::text("Data")]).unwrap());
    assert!(matches!(invoke(&model, &moved, "ChartType", vec![]), Ok(AutoValue::I4(51))));
    assert_eq!((count(&charts), count(&collection("Sheets"))), (0, 2));
    let data = obj(invoke_kind(&model, &collection("Worksheets"), "Item", InvokeKind::Get, vec![AutoValue::text("Data")]).unwrap());
    let objects = obj(invoke_kind(&model, &data, "ChartObjects", InvokeKind::Method, vec![]).unwrap());
    assert_eq!(count(&objects), 2);
    // The chart object the file held goes to a new chart sheet named Sales; Data keeps the other.
    let first = obj(invoke_kind(&model, &objects, "Item", InvokeKind::Method, vec![AutoValue::I4(1)]).unwrap());
    let bar = obj(invoke(&model, &first, "Chart", vec![]).unwrap());
    assert!(matches!(invoke(&model, &bar, "ChartType", vec![]), Ok(AutoValue::I4(57))));
    let sales = obj(invoke_kind(&model, &bar, "Location", InvokeKind::Method, vec![AutoValue::I4(1), AutoValue::text("Sales")]).unwrap());
    assert_eq!(text_of(invoke(&model, &sales, "Name", vec![]).unwrap()), "Sales");
    assert_eq!((count(&charts), count(&objects)), (1, 1));
    // The save keeps both, over the file's own chart parts.
    let mut bytes = Vec::new();
    host.session.borrow_mut().save(&SavePolicy::default(), &mut bytes).expect("the save is not refused");
    let document = calc_doc::open(Arc::new(MemorySource::new(bytes.clone())), &OpenPolicy::default()).expect("the saved package opens");
    let names: Vec<String> = document.package().parts().iter().map(|p| p.name().to_owned()).collect();
    let chart_parts: Vec<&String> = names.iter().filter(|n| n.starts_with("xl/charts/chart")).collect();
    assert_eq!(chart_parts.len(), 2, "{names:?}");
    // The removed chart sheet's part and the drawing only it used are gone; Data's drawing holds one frame.
    assert!(!names.iter().any(|n| n == "xl/chartsheets/sheet1.xml" || n == "xl/drawings/drawing1.xml"), "{names:?}");
    let drawing = String::from_utf8(document.package().read_part("xl/drawings/drawing2.xml").unwrap()).unwrap();
    assert_eq!(drawing.matches("<xdr:graphicFrame").count(), 1, "{drawing}");
    let reopened = HostSession::open(Arc::new(MemorySource::new(bytes)), &OpenPolicy::default(), HostOptions::default()).unwrap();
    let (_h2, model2) = setup_session(reopened);
    let book2 = obj(invoke(&model2, &model2.application(), "ActiveWorkbook", vec![]).unwrap());
    let collection2 = |name: &str| obj(invoke(&model2, &book2, name, vec![]).unwrap());
    let charts2 = collection2("Charts");
    assert_eq!(int(invoke(&model2, &charts2, "Count", vec![]).unwrap()), 1);
    let sales2 = obj(invoke_kind(&model2, &charts2, "Item", InvokeKind::Get, vec![AutoValue::text("Sales")]).unwrap());
    assert!(matches!(invoke(&model2, &sales2, "ChartType", vec![]), Ok(AutoValue::I4(57))));
    let data2 = obj(invoke_kind(&model2, &collection2("Worksheets"), "Item", InvokeKind::Get, vec![AutoValue::text("Data")]).unwrap());
    let objects2 = obj(invoke_kind(&model2, &data2, "ChartObjects", InvokeKind::Method, vec![]).unwrap());
    assert_eq!(int(invoke(&model2, &objects2, "Count", vec![]).unwrap()), 1);
    let only = obj(invoke_kind(&model2, &objects2, "Item", InvokeKind::Method, vec![AutoValue::I4(1)]).unwrap());
    let column = obj(invoke(&model2, &only, "Chart", vec![]).unwrap());
    assert!(matches!(invoke(&model2, &column, "ChartType", vec![]), Ok(AutoValue::I4(51))));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_59__file_held_chart_frame_edits_name_their_owning_bead() {
    let session = HostSession::open(
        Arc::new(MemorySource::new(
            include_bytes!("../../calc-doc/tests/fixtures/preserved_sheets/chart-sheet-and-embedded.xlsx")
                .to_vec(),
        )),
        &OpenPolicy::default(),
        HostOptions::default(),
    )
    .unwrap();
    let (host, model) = setup_session(session);
    let revision = host.session.borrow().host().revision();
    let sheet = obj(invoke(&model, &model.application(), "ActiveSheet", vec![]).unwrap());
    let frame = obj(
        invoke_kind(&model, &sheet, "ChartObjects", InvokeKind::Method, vec![AutoValue::I4(1)])
            .unwrap(),
    );
    let chart = obj(invoke(&model, &frame, "Chart", vec![]).unwrap());
    for (target, interface, member, value) in [
        (&frame, "ChartObject", "Name", AutoValue::text("Sales")),
        (&frame, "ChartObject", "Left", AutoValue::R8(50.7)),
        (&frame, "ChartObject", "Top", AutoValue::R8(30.0)),
    ] {
        let error = invoke_kind(&model, target, member, InvokeKind::Put, vec![value]).unwrap_err();
        assert_eq!(error, AutoError::not_modelled(format!("{interface}.{member}"), "dc-oxh8w.59"));
    }
    // Excel raises error 7 for an embedded chart's Chart.Name whether the file holds it or not (ww22 VBA).
    let r = invoke_kind(&model, &chart, "Name", InvokeKind::Put, vec![AutoValue::text("Sales")]);
    assert!(matches!(&r, Err(AutoError::Excel { number: 7, .. })), "{r:?}");
    assert_eq!(host.session.borrow().host().revision(), revision);
}

/// dc-3qsm0: a chart-sheet chart moved onto a sheet keeps the content edits made after the move (its source sheet is
/// deleted), and an embedded chart moved to a chart sheet leaves no relationship to its part in the old drawing.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_3qsm0__moved_charts_save_their_edits_and_leave_no_dangling_rels() {
    let (host, model) = setup_session(
        HostSession::open(
            Arc::new(MemorySource::new(
                include_bytes!("../../calc-doc/tests/fixtures/preserved_sheets/chart-sheet-and-embedded.xlsx").to_vec(),
            )),
            &OpenPolicy::default(),
            HostOptions::default(),
        )
        .unwrap(),
    );
    let book = obj(invoke(&model, &model.application(), "ActiveWorkbook", vec![]).unwrap());
    let collection = |name: &str| obj(invoke(&model, &book, name, vec![]).unwrap());
    let charts = collection("Charts");
    let chart = obj(invoke_kind(&model, &charts, "Item", InvokeKind::Get, vec![AutoValue::I4(1)]).unwrap());
    let moved = obj(invoke_kind(&model, &chart, "Location", InvokeKind::Method, vec![AutoValue::I4(2), AutoValue::text("Data")]).unwrap());
    invoke_kind(&model, &moved, "HasTitle", InvokeKind::Put, vec![AutoValue::Bool(true)]).unwrap();
    let title = obj(invoke(&model, &moved, "ChartTitle", vec![]).unwrap());
    invoke_kind(&model, &title, "Text", InvokeKind::Put, vec![AutoValue::text("Moved title")]).unwrap();
    let data = obj(invoke_kind(&model, &collection("Worksheets"), "Item", InvokeKind::Get, vec![AutoValue::text("Data")]).unwrap());
    let objects = obj(invoke_kind(&model, &data, "ChartObjects", InvokeKind::Method, vec![]).unwrap());
    let first = obj(invoke_kind(&model, &objects, "Item", InvokeKind::Method, vec![AutoValue::I4(1)]).unwrap());
    let bar = obj(invoke(&model, &first, "Chart", vec![]).unwrap());
    obj(invoke_kind(&model, &bar, "Location", InvokeKind::Method, vec![AutoValue::I4(1), AutoValue::text("Sales")]).unwrap());
    let mut bytes = Vec::new();
    host.session.borrow_mut().save(&SavePolicy::default(), &mut bytes).expect("the save is not refused");
    let document = calc_doc::open(Arc::new(MemorySource::new(bytes.clone())), &OpenPolicy::default()).expect("the saved package opens");
    // Every chart relationship of a drawing is used by a frame of that drawing.
    for part in document.package().parts().iter().map(|p| p.name().to_owned()).filter(|n| n.starts_with("xl/drawings/drawing")) {
        let rels_name = format!("xl/drawings/_rels/{}.rels", part.rsplit('/').next().unwrap());
        let Ok(rels) = document.package().read_part(&rels_name) else { continue };
        let (rels, drawing) = (String::from_utf8(rels).unwrap(), String::from_utf8(document.package().read_part(&part).unwrap()).unwrap());
        let links = rels.matches("relationships/chart\"").count();
        assert_eq!(links, drawing.matches("<xdr:graphicFrame").count(), "{part}: {rels} / {drawing}");
    }
    let (_h2, model2) = setup_session(HostSession::open(Arc::new(MemorySource::new(bytes)), &OpenPolicy::default(), HostOptions::default()).unwrap());
    let book2 = obj(invoke(&model2, &model2.application(), "ActiveWorkbook", vec![]).unwrap());
    let data2 = obj(invoke_kind(&model2, &obj(invoke(&model2, &book2, "Worksheets", vec![]).unwrap()), "Item", InvokeKind::Get, vec![AutoValue::text("Data")]).unwrap());
    let objects2 = obj(invoke_kind(&model2, &data2, "ChartObjects", InvokeKind::Method, vec![]).unwrap());
    let only = obj(invoke_kind(&model2, &objects2, "Item", InvokeKind::Method, vec![AutoValue::I4(1)]).unwrap());
    let column = obj(invoke(&model2, &only, "Chart", vec![]).unwrap());
    let title2 = obj(invoke(&model2, &column, "ChartTitle", vec![]).unwrap());
    assert_eq!(text_of(invoke(&model2, &title2, "Text", vec![]).unwrap()), "Moved title");
}

/// dc-oxh8w.59 (ww22 charts/observations.jsonl Frame.chartName, VBA: Excel 16.0.20625.20108): `Chart.Name = ...` of an
/// embedded chart raises error 7 (Out of memory) and changes nothing; `ChartObject.Name` takes the name, including one
/// another chart or a shape already has (ww22 Frame.duplicate-chart, Frame.duplicate-shape).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_59__embedded_chart_name_put_is_out_of_memory_and_chart_object_names_repeat() {
    let (host, model, sheet) = blank_report();
    fill(&host, (1, 1), &[&["Region", "Q1"], &["North", "10"]]);
    let objects = obj(invoke_kind(&model, &sheet, "ChartObjects", InvokeKind::Method, vec![]).unwrap());
    let add = || {
        obj(
            invoke_kind(&model, &objects, "Add", InvokeKind::Method, vec![AutoValue::R8(0.0), AutoValue::R8(0.0), AutoValue::R8(200.0), AutoValue::R8(100.0)])
                .unwrap(),
        )
    };
    let frame = add();
    let chart = obj(invoke(&model, &frame, "Chart", vec![]).unwrap());
    assert_eq!(text_of(invoke(&model, &chart, "Name", vec![]).unwrap()), "Sheet1 Chart 1");
    for name in ["Takings", "New & <First>"] {
        let r = invoke_kind(&model, &chart, "Name", InvokeKind::Put, vec![AutoValue::text(name)]);
        assert!(matches!(&r, Err(AutoError::Excel { number: 7, description }) if description == "Out of memory"), "{r:?}");
    }
    assert_eq!(text_of(invoke(&model, &chart, "Name", vec![]).unwrap()), "Sheet1 Chart 1");
    assert_eq!(text_of(invoke(&model, &frame, "Name", vec![]).unwrap()), "Chart 1");
    // ChartObject.Name: any text, repeats included.
    invoke_kind(&model, &frame, "Name", InvokeKind::Put, vec![AutoValue::text("New & <First>")]).unwrap();
    assert_eq!(text_of(invoke(&model, &chart, "Name", vec![]).unwrap()), "Sheet1 New & <First>");
    let second = add();
    invoke_kind(&model, &second, "Name", InvokeKind::Put, vec![AutoValue::text("New & <First>")]).unwrap();
    assert_eq!(text_of(invoke(&model, &second, "Name", vec![]).unwrap()), "New & <First>");
    let shapes = obj(invoke(&model, &sheet, "Shapes", vec![]).unwrap());
    let rectangle = obj(
        invoke_kind(
            &model,
            &shapes,
            "AddShape",
            InvokeKind::Method,
            vec![AutoValue::I4(1), AutoValue::R8(10.0), AutoValue::R8(240.0), AutoValue::R8(50.0), AutoValue::R8(40.0)],
        )
        .unwrap(),
    );
    invoke_kind(&model, &rectangle, "Name", InvokeKind::Put, vec![AutoValue::text("Other")]).unwrap();
    invoke_kind(&model, &frame, "Name", InvokeKind::Put, vec![AutoValue::text("Other")]).unwrap();
    assert_eq!(text_of(invoke(&model, &frame, "Name", vec![]).unwrap()), "Other");
}

/// dc-oxh8w.11: `HPageBreaks` and `VPageBreaks` list the automatic breaks of the one pagination route as well as the
/// manual ones, as Excel does. Letter portrait, default margins, 100%, A1:J120 of 15 pt rows and 8.43 character columns
/// (Excel PB1 case c1, 16.0.20527: pages 6, H A48 and A95, V J1, all xlPageBreakAutomatic).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_11_page_break_collections_list_automatic_breaks() {
    let (host, model, sheet) = blank_report();
    enter(&host, at(&host, 120, 10), "1");
    let list = |name: &str| -> Vec<(String, i32)> {
        let breaks = obj(invoke(&model, &sheet, name, vec![]).unwrap());
        let AutoValue::I4(count) = invoke(&model, &breaks, "Count", vec![]).unwrap() else { panic!("count") };
        (1..=count)
            .map(|i| {
                let one = obj(invoke(&model, &breaks, "Item", vec![AutoValue::I4(i)]).unwrap());
                let location = obj(invoke(&model, &one, "Location", vec![]).unwrap());
                let AutoValue::I4(kind) = invoke(&model, &one, "Type", vec![]).unwrap() else { panic!("type") };
                (text(&model, &location, "Address"), kind)
            })
            .collect()
    };
    assert_eq!(list("HPageBreaks"), [("$A$48".to_owned(), -4105), ("$A$95".to_owned(), -4105)]);
    assert_eq!(list("VPageBreaks"), [("$J$1".to_owned(), -4105)]);
    // A manual break joins the automatic ones in position order and is typed manual.
    let breaks = obj(invoke(&model, &sheet, "HPageBreaks", vec![]).unwrap());
    invoke_kind(&model, &breaks, "Add", InvokeKind::Method, vec![AutoValue::Object(range(&model, "A31"))]).unwrap();
    let rows = list("HPageBreaks");
    assert_eq!(rows[0], ("$A$31".to_owned(), -4135), "{rows:?}");
    assert!(rows.iter().skip(1).all(|(_, kind)| *kind == -4105), "{rows:?}");
}


/// dc-oxh8w.36: Worksheet.Copy makes a sheet clone: cells (values, formulas that stay on their own sheet),
/// styles, column widths and merges; it is placed by Before or After, named "<name> (2)", and becomes active.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_36__worksheet_copy_clones_the_sheet() {
    let (host, model) = setup_session(
        HostSession::open(
            Arc::new(MemorySource::new(blank_package())),
            &OpenPolicy::default(),
            HostOptions { calc_mode: Some(CalcMode::Automatic), ..Default::default() },
        )
        .unwrap(),
    );
    let sheets = worksheets(&model);
    let source_name = sheet_names(&host)[0].clone();
    let put = |address: &str, value: AutoValue| {
        invoke_kind(&model, &range(&model, address), "Value2", InvokeKind::Put, vec![value]).unwrap();
    };
    put("A1", AutoValue::R8(5.));
    invoke_kind(&model, &range(&model, "A2"), "Formula", InvokeKind::Put, vec![AutoValue::text("=A1*2")]).unwrap();
    enter(&host, at(&host, 3, 3), "label");
    invoke_kind(&model, &range(&model, "C3"), "ColumnWidth", InvokeKind::Put, vec![AutoValue::R8(30.)]).unwrap();
    invoke_kind(&model, &range(&model, "E5:F6"), "MergeCells", InvokeKind::Put, vec![AutoValue::Bool(true)]).unwrap();
    let source = obj(invoke(&model, &sheets, "Item", vec![AutoValue::I4(1)]).unwrap());
    let width = |model: &WorkbookAutomation| invoke(model, &range(model, "C3"), "ColumnWidth", vec![]).unwrap();
    let AutoValue::R8(source_width) = width(&model) else { panic!("width") };

    // Copy with After the source: the clone sits second, is named "<name> (2)" and is the active sheet.
    invoke_kind(&model, &source, "Copy", InvokeKind::Method, vec![AutoValue::Missing, AutoValue::Object(source.clone())]).unwrap();
    let names = sheet_names(&host);
    assert_eq!(names[0], source_name);
    assert_eq!(names[1], format!("{source_name} (2)"));
    assert!(matches!(cell_text(&model, "A1"), AutoValue::R8(v) if v == 5.), "the clone is active and holds the values");
    assert_eq!(invoke(&model, &range(&model, "A2"), "Formula", vec![]).unwrap().as_text().unwrap(), "=A1*2");
    assert!(matches!(cell_text(&model, "A2"), AutoValue::R8(v) if v == 10.));
    assert_eq!(cell_text(&model, "C3").as_text().unwrap(), "label");
    assert!(matches!(width(&model), AutoValue::R8(w) if w == source_width), "column widths are cloned");
    assert!(matches!(invoke(&model, &range(&model, "E5"), "MergeCells", vec![]).unwrap(), AutoValue::Bool(true)));
    // The clone's formula reads the clone's A1: changing it leaves the source alone.
    put("A1", AutoValue::R8(7.));
    assert!(matches!(cell_text(&model, "A2"), AutoValue::R8(v) if v == 14.));
    let first = obj(invoke(&model, &sheets, "Item", vec![AutoValue::I4(1)]).unwrap());
    let source_a2 = obj(invoke(&model, &first, "Range", vec![AutoValue::text("A2")]).unwrap());
    assert!(matches!(invoke(&model, &source_a2, "Value2", vec![]).unwrap(), AutoValue::R8(v) if v == 10.), "the source is unchanged");

    // Before places the clone ahead of the named sheet; Before and After together are 1004; a second copy is "(3)".
    invoke_kind(&model, &first, "Copy", InvokeKind::Method, vec![AutoValue::Object(first.clone())]).unwrap();
    let names = sheet_names(&host);
    assert_eq!(names[0], format!("{source_name} (3)"), "{names:?}");
    let both = invoke_kind(&model, &first, "Copy", InvokeKind::Method, vec![AutoValue::Object(first.clone()), AutoValue::Object(first.clone())]);
    assert!(matches!(both, Err(AutoError::Excel { number: 1004, .. })), "{both:?}");
    let count = sheet_names(&host).len();
    // No application factory here, so a copy to a new workbook refuses naming the Workbooks owner (calc-host-vba tests the copy).
    let to_new_book = invoke_kind(&model, &first, "Copy", InvokeKind::Method, vec![]);
    assert!(matches!(to_new_book, Err(AutoError::NotModelled { owner, .. }) if owner.to_string() == "dc-oxh8w.75"), "{to_new_book:?}");
    let bad = invoke_kind(&model, &first, "Copy", InvokeKind::Method, vec![AutoValue::I4(1)]);
    assert!(bad.is_err());
    assert_eq!(sheet_names(&host).len(), count);
}

/// dc-oxh8w.43: Sheets.Copy / Worksheets.Copy copy every listed worksheet, in order, after the After anchor or
/// before the Before anchor; cells come along; no anchor (a new workbook) is a typed refusal; both anchors are 1004.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_43__sheets_copy_copies_every_worksheet() {
    let (host, model) = setup_session(
        HostSession::open(
            Arc::new(MemorySource::new(blank_package())),
            &OpenPolicy::default(),
            HostOptions { calc_mode: Some(CalcMode::Automatic), ..Default::default() },
        )
        .unwrap(),
    );
    let sheets = worksheets(&model);
    let first = obj(invoke(&model, &sheets, "Item", vec![AutoValue::I4(1)]).unwrap());
    invoke_kind(&model, &range(&model, "A1"), "Value2", InvokeKind::Put, vec![AutoValue::R8(5.)]).unwrap();
    invoke_kind(&model, &sheets, "Add", InvokeKind::Method, vec![]).unwrap();
    let second = obj(invoke(&model, &sheets, "Item", vec![AutoValue::I4(2)]).unwrap());
    let base = sheet_names(&host);
    assert_eq!(base.len(), 2);

    // After the last sheet: the copies keep their order.
    invoke_kind(&model, &sheets, "Copy", InvokeKind::Method, vec![AutoValue::Missing, AutoValue::Object(second.clone())]).unwrap();
    let names = sheet_names(&host);
    assert_eq!(names, vec![base[0].clone(), base[1].clone(), format!("{} (2)", base[0]), format!("{} (2)", base[1])], "{names:?}");
    let third = obj(invoke(&model, &sheets, "Item", vec![AutoValue::I4(4)]).unwrap()); // the copy of the sheet that holds A1
    let a1 = obj(invoke(&model, &third, "Range", vec![AutoValue::text("A1")]).unwrap());
    let got = invoke(&model, &a1, "Value2", vec![]).unwrap();
    assert!(matches!(got, AutoValue::R8(v) if v == 5.), "the copy holds the cells: {got:?} {names:?}");

    // Before the second sheet: all four copies land ahead of it, in order; a copy of "X (2)" is "X (n)", n free.
    invoke_kind(&model, &sheets, "Copy", InvokeKind::Method, vec![AutoValue::Object(second.clone())]).unwrap();
    let names = sheet_names(&host);
    let (a, b) = (&base[0], &base[1]);
    let want = [a.clone(), format!("{a} (3)"), format!("{b} (3)"), format!("{a} (4)"), format!("{b} (4)"), b.clone(), format!("{a} (2)"), format!("{b} (2)")];
    assert_eq!(names, want, "{names:?}");

    let both = invoke_kind(&model, &sheets, "Copy", InvokeKind::Method, vec![AutoValue::Object(first.clone()), AutoValue::Object(second.clone())]);
    assert!(matches!(both, Err(AutoError::Excel { number: 1004, .. })), "{both:?}");
    let new_book = invoke_kind(&model, &sheets, "Copy", InvokeKind::Method, vec![]);
    assert!(matches!(new_book, Err(AutoError::NotModelled { owner, .. }) if owner.to_string() == "dc-oxh8w.75"), "{new_book:?}");
    assert_eq!(sheet_names(&host).len(), 8);
}

/// dc-oxh8w.43: Worksheets(Array(...)) is a collection of just those sheets (workbook order, no duplicates):
/// Count and Item answer for it, Copy and Move take it as a group, an unknown member is an error.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_43__worksheets_of_an_array_is_a_group() {
    let (host, model) = setup_session(
        HostSession::open(
            Arc::new(MemorySource::new(blank_package())),
            &OpenPolicy::default(),
            HostOptions { calc_mode: Some(CalcMode::Automatic), ..Default::default() },
        )
        .unwrap(),
    );
    let sheets = worksheets(&model);
    for _ in 0..2 {
        invoke_kind(&model, &sheets, "Add", InvokeKind::Method, vec![]).unwrap();
    }
    let base = sheet_names(&host);
    assert_eq!(base.len(), 3);
    let array = |values: Vec<AutoValue>| {
        AutoValue::Array(Box::new(calc_object_model::AutoArray { rows: 1, columns: values.len() as u32, lower: (1, 1), values }))
    };
    let group = obj(invoke(&model, &sheets, "Item", vec![array(vec![AutoValue::I4(3), AutoValue::I4(1), AutoValue::I4(3)])]).unwrap());
    let count = invoke(&model, &group, "Count", vec![]).unwrap();
    assert!(matches!(count, AutoValue::I4(2)), "{count:?}");
    let first = obj(invoke(&model, &group, "Item", vec![AutoValue::I4(1)]).unwrap());
    let name = invoke(&model, &first, "Name", vec![]).unwrap();
    assert_eq!(name.as_text().as_deref(), Some(base[0].as_str()), "workbook order: sheet 1 comes first");
    let missing = invoke(&model, &sheets, "Item", vec![array(vec![AutoValue::I4(1), AutoValue::I4(9)])]);
    assert!(missing.is_err());
    // Move the group after the middle sheet: 2, 1, 3.
    let middle = obj(invoke(&model, &sheets, "Item", vec![AutoValue::I4(2)]).unwrap());
    invoke_kind(&model, &group, "Move", InvokeKind::Method, vec![AutoValue::Missing, AutoValue::Object(middle.clone())]).unwrap();
    assert_eq!(sheet_names(&host), vec![base[1].clone(), base[0].clone(), base[2].clone()]);
    // Copy the group before the first sheet.
    let head = obj(invoke(&model, &sheets, "Item", vec![AutoValue::I4(1)]).unwrap());
    invoke_kind(&model, &group, "Copy", InvokeKind::Method, vec![AutoValue::Object(head)]).unwrap();
    assert_eq!(sheet_names(&host).len(), 5);
    let both = invoke_kind(&model, &group, "Move", InvokeKind::Method, vec![AutoValue::Object(middle.clone()), AutoValue::Object(middle)]);
    assert!(matches!(both, Err(AutoError::Excel { number: 1004, .. })), "{both:?}");
}

/// dc-oxh8w.43 (ww23 b/observations.jsonl VBA.case22-28, Excel 16.0.20625.20108): a copy of "A" is "A (2)", a copy of that
/// "A (3)"; a copy Before the sheet lands before it and activates; a long name is cut so " (2)" still fits in 31
/// characters; a group copy keeps tab order and lands as a block After the last or Before the first sheet.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_43__sheet_copy_names_and_group_placement() {
    let new_session = || {
        setup_session(
            HostSession::open(
                Arc::new(MemorySource::new(blank_package())),
                &OpenPolicy::default(),
                HostOptions { calc_mode: Some(CalcMode::Automatic), ..Default::default() },
            )
            .unwrap(),
        )
    };
    let add = |model: &WorkbookAutomation, name: &str| {
        let sheets = worksheets(model);
        let sheet = obj(invoke_kind(model, &sheets, "Add", InvokeKind::Method, vec![]).unwrap());
        invoke_kind(model, &sheet, "Name", InvokeKind::Put, vec![AutoValue::text(name)]).unwrap();
        sheet
    };
    let copy = |model: &WorkbookAutomation, sheet: &ObjectRef, args: Vec<AutoValue>| {
        invoke_kind(model, sheet, "Copy", InvokeKind::Method, args).unwrap();
    };
    let active = |model: &WorkbookAutomation| {
        let sheet = obj(invoke(model, &model.application(), "ActiveSheet", vec![]).unwrap());
        (
            invoke(model, &sheet, "Name", vec![]).unwrap().as_text().unwrap().to_string(),
            int(invoke(model, &sheet, "Index", vec![]).unwrap()),
        )
    };
    let after_self = |model: &WorkbookAutomation, sheet: &ObjectRef| vec![AutoValue::Missing, AutoValue::Object(sheet.clone())];
    // case 23: A -> A (2) -> A (3)
    let (host, model) = new_session();
    let a = add(&model, "A");
    copy(&model, &a, after_self(&model, &a));
    assert_eq!(active(&model).0, "A (2)");
    let second = obj(invoke(&model, &model.application(), "ActiveSheet", vec![]).unwrap());
    copy(&model, &second, after_self(&model, &second));
    assert_eq!(active(&model).0, "A (3)");
    assert_eq!(sheet_names(&host).join(","), "A,A (2),A (3),Sheet1");
    // case 24: Before
    let (host, model) = new_session();
    let a = add(&model, "A");
    copy(&model, &a, vec![AutoValue::Object(a.clone())]);
    assert_eq!((active(&model).0, active(&model).1), ("A (2)".to_string(), 1));
    assert_eq!(sheet_names(&host).join(","), "A (2),A,Sheet1");
    copy(&model, &a, vec![AutoValue::Object(a.clone())]);
    assert_eq!((active(&model).0, active(&model).1), ("A (3)".to_string(), 2));
    assert_eq!(sheet_names(&host).join(","), "A (2),A (3),A,Sheet1");
    // case 22: a 31-character name
    let (_host, model) = new_session();
    let long = add(&model, "ABCDEFGHIJKLMNOPQRSTUVWXYZ12345");
    copy(&model, &long, after_self(&model, &long));
    assert_eq!(active(&model).0, "ABCDEFGHIJKLMNOPQRSTUVWXYZ1 (2)");
    let second = obj(invoke(&model, &model.application(), "ActiveSheet", vec![]).unwrap());
    copy(&model, &second, after_self(&model, &second));
    assert_eq!(active(&model).0, "ABCDEFGHIJKLMNOPQRSTUVWXYZ1 (3)");
    // cases 27 and 28: group copies
    for (before_first, expected) in [(false, "Q,P,Sheet1,Q (2),P (2)"), (true, "Q (2),P (2),Q,P,Sheet1")] {
        let (host, model) = new_session();
        add(&model, "P");
        add(&model, "Q");
        let sheets = worksheets(&model);
        let group = obj(
            invoke(
                &model,
                &sheets,
                "Item",
                vec![AutoValue::Array(Box::new(calc_object_model::AutoArray {
                    rows: 1,
                    columns: 2,
                    lower: (1, 1),
                    values: vec![AutoValue::text("P"), AutoValue::text("Q")],
                }))],
            )
            .unwrap(),
        );
        let anchor = |i: i32| AutoValue::Object(obj(invoke(&model, &sheets, "Item", vec![AutoValue::I4(i)]).unwrap()));
        let args = if before_first { vec![anchor(1)] } else { vec![AutoValue::Missing, anchor(3)] };
        invoke_kind(&model, &group, "Copy", InvokeKind::Method, args).unwrap();
        assert_eq!(sheet_names(&host).join(","), expected, "before_first = {before_first}");
    }
}

/// dc-fa8cx (ww22 Excel 16.0.20625): a group moved Before or After one of its own sheets changes nothing and
/// activates its first sheet in tab order; Array(3,2) moved Before sheet 1 keeps tab order (2,3,1,4), active 2.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_fa8cx__group_move_anchored_inside_the_group() {
    let (host, model) = setup_session(
        HostSession::open(
            Arc::new(MemorySource::new(blank_package())),
            &OpenPolicy::default(),
            HostOptions { calc_mode: Some(CalcMode::Automatic), ..Default::default() },
        )
        .unwrap(),
    );
    let sheets = worksheets(&model);
    for _ in 0..3 {
        invoke_kind(&model, &sheets, "Add", InvokeKind::Method, vec![]).unwrap();
    }
    let base = sheet_names(&host);
    assert_eq!(base.len(), 4);
    let array = |values: Vec<AutoValue>| {
        AutoValue::Array(Box::new(calc_object_model::AutoArray { rows: 1, columns: values.len() as u32, lower: (1, 1), values }))
    };
    let item = |i: i32| obj(invoke(&model, &sheets, "Item", vec![AutoValue::I4(i)]).unwrap());
    let app = model.application();
    let active = || {
        let sheet = obj(invoke(&model, &app, "ActiveSheet", vec![]).unwrap());
        invoke(&model, &sheet, "Name", vec![]).unwrap().as_text().unwrap().to_string()
    };
    let group = |a: i32, b: i32| obj(invoke(&model, &sheets, "Item", vec![array(vec![AutoValue::I4(a), AutoValue::I4(b)])]).unwrap());
    invoke_kind(&model, &item(1), "Activate", InvokeKind::Method, vec![]).unwrap();
    invoke_kind(&model, &group(2, 3), "Move", InvokeKind::Method, vec![AutoValue::Object(item(2))]).unwrap();
    assert_eq!(sheet_names(&host), base);
    assert_eq!(active(), base[1]);
    invoke_kind(&model, &item(1), "Activate", InvokeKind::Method, vec![]).unwrap();
    invoke_kind(&model, &group(2, 3), "Move", InvokeKind::Method, vec![AutoValue::Missing, AutoValue::Object(item(3))]).unwrap();
    assert_eq!(sheet_names(&host), base);
    assert_eq!(active(), base[1]);
    invoke_kind(&model, &group(3, 2), "Move", InvokeKind::Method, vec![AutoValue::Object(item(1))]).unwrap();
    assert_eq!(sheet_names(&host), vec![base[1].clone(), base[2].clone(), base[0].clone(), base[3].clone()]);
    assert_eq!(active(), base[1]);
}

/// dc-oxh8w.47: Worksheet.Visible reads and writes xlSheetVisible (-1), xlSheetHidden (0) and xlSheetVeryHidden (2);
/// True and False map to visible and hidden; the last visible sheet cannot be hidden and other values are 1004.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_47__worksheet_visible_get_and_put() {
    let (_host, model) = setup();
    let sheets = worksheets(&model);
    let first = obj(invoke(&model, &sheets, "Item", vec![AutoValue::I4(1)]).unwrap());
    let added = obj(invoke_kind(&model, &sheets, "Add", InvokeKind::Method, vec![]).unwrap());
    let visible = |o: &ObjectRef| invoke_kind(&model, o, "Visible", InvokeKind::Get, vec![]).unwrap();
    let put = |o: &ObjectRef, v: AutoValue| invoke_kind(&model, o, "Visible", InvokeKind::Put, vec![v]);
    assert!(matches!(visible(&first), AutoValue::I4(-1)), "{:?}", visible(&first));
    put(&first, AutoValue::I4(0)).unwrap();
    assert!(matches!(visible(&first), AutoValue::I4(0)));
    put(&first, AutoValue::I4(2)).unwrap();
    assert!(matches!(visible(&first), AutoValue::I4(2)));
    put(&first, AutoValue::Bool(true)).unwrap();
    assert!(matches!(visible(&first), AutoValue::I4(-1)));
    put(&first, AutoValue::Bool(false)).unwrap();
    assert!(matches!(visible(&first), AutoValue::I4(0)));
    // The first sheet is hidden: the added sheet is the last visible one and cannot be hidden.
    for v in [AutoValue::I4(0), AutoValue::Bool(false), AutoValue::I4(2)] {
        let last = put(&added, v);
        assert!(matches!(last, Err(AutoError::Excel { number: 1004, .. })), "{last:?}");
    }
    assert!(matches!(visible(&added), AutoValue::I4(-1)));
    let bad = put(&added, AutoValue::I4(7));
    assert!(matches!(bad, Err(AutoError::Excel { number: 1004, .. })), "{bad:?}");
    put(&first, AutoValue::I4(-1)).unwrap();
    put(&added, AutoValue::I4(2)).unwrap();
    assert!(matches!(visible(&added), AutoValue::I4(2)));
}

/// dc-oxh8w.49: Application.Goto without a Reference goes to the last range Goto selected (Excel documentation);
/// before any Goto it is a typed refusal.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_49__goto_without_reference_returns_to_the_last_goto() {
    let (_host, model) = setup();
    let app = model.application();
    let sheets = worksheets(&model);
    let first = obj(invoke(&model, &sheets, "Item", vec![AutoValue::I4(1)]).unwrap());
    let sub = |a: &str| obj(invoke(&model, &first, "Range", vec![AutoValue::text(a)]).unwrap());
    let goto = |args: Vec<AutoValue>| invoke_kind(&model, &app, "Goto", InvokeKind::Method, args);
    let selection = || text(&model, &obj(invoke(&model, &app, "Selection", vec![]).unwrap()), "Address");
    let none = goto(vec![]);
    assert!(matches!(none, Err(AutoError::NotModelled { owner, .. }) if owner == "dc-oxh8w.49"), "{none:?}");
    goto(vec![AutoValue::Object(sub("C3:D4"))]).unwrap();
    invoke_kind(&model, &sub("F9"), "Select", InvokeKind::Method, vec![]).unwrap();
    assert_eq!(selection(), "$F$9");
    goto(vec![]).unwrap();
    assert_eq!(selection(), "$C$3:$D$4");
    invoke_kind(&model, &sub("A1"), "Select", InvokeKind::Method, vec![]).unwrap();
    goto(vec![AutoValue::Missing, AutoValue::Bool(true)]).unwrap();
    assert_eq!(selection(), "$C$3:$D$4");
}

/// dc-oxh8w.47: Range.End(direction) follows Ctrl+arrow: inside a block it goes to the block's last cell, at a block's
/// edge or from an empty cell it goes to the next occupied cell, else the sheet's edge. Bad direction is 1004.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_47__range_end_follows_ctrl_arrow() {
    let (_host, model) = setup();
    let sheets = worksheets(&model);
    let added = obj(invoke_kind(&model, &sheets, "Add", InvokeKind::Method, vec![]).unwrap());
    let sub = |a: &str| obj(invoke(&model, &added, "Range", vec![AutoValue::text(a)]).unwrap());
    for a in ["B2", "B3", "B4", "B6", "D2"] {
        put_value(&model, &sub(a), AutoValue::R8(1.0)).unwrap();
    }
    let end = |from: &str, direction: i32| {
        let r = invoke_kind(&model, &sub(from), "End", InvokeKind::Get, vec![AutoValue::I4(direction)]).unwrap();
        text(&model, &obj(r), "Address")
    };
    let (down, up, left, right) = (-4121, -4162, -4159, -4161);
    assert_eq!(end("B2", down), "$B$4");
    assert_eq!(end("B3", down), "$B$4");
    assert_eq!(end("B4", down), "$B$6");
    assert_eq!(end("B6", down), "$B$1048576");
    assert_eq!(end("B1", down), "$B$2");
    assert_eq!(end("B5", down), "$B$6");
    assert_eq!(end("B4", up), "$B$2");
    assert_eq!(end("B6", up), "$B$4");
    assert_eq!(end("B2", up), "$B$1");
    assert_eq!(end("B5", up), "$B$4");
    assert_eq!(end("B2", right), "$D$2");
    assert_eq!(end("D2", right), "$XFD$2");
    assert_eq!(end("D2", left), "$B$2");
    assert_eq!(end("B2", left), "$A$2");
    assert_eq!(end("A2", right), "$B$2");
    assert_eq!(end("B3", right), "$XFD$3");
    let bad = invoke_kind(&model, &sub("B2"), "End", InvokeKind::Get, vec![AutoValue::I4(99)]);
    assert!(matches!(bad, Err(AutoError::Excel { number: 1004, .. })), "{bad:?}");
    let many = invoke_kind(&model, &sub("B2:B3"), "End", InvokeKind::Get, vec![AutoValue::I4(down)]);
    assert!(matches!(many, Err(AutoError::NotModelled { .. })), "{many:?}");
}

/// dc-oxh8w.47: Application.InchesToPoints converts inches to points (72 per inch); a text number is coerced,
/// non-numeric text is DISP_E_TYPEMISMATCH.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_47__inches_to_points() {
    let (_host, model) = setup();
    let app = model.application();
    let points = |v: AutoValue| invoke_kind(&model, &app, "InchesToPoints", InvokeKind::Method, vec![v]);
    assert!(matches!(points(AutoValue::R8(1.0)).unwrap(), AutoValue::R8(v) if v == 72.0));
    assert!(matches!(points(AutoValue::I4(2)).unwrap(), AutoValue::R8(v) if v == 144.0));
    assert!(matches!(points(AutoValue::R8(0.25)).unwrap(), AutoValue::R8(v) if v == 18.0));
    assert!(matches!(points(AutoValue::text("1.5")).unwrap(), AutoValue::R8(v) if v == 108.0));
    let bad = points(AutoValue::text("abc"));
    assert!(matches!(bad, Err(AutoError::Status(s)) if s == 0x8002_0005u32 as i32), "{bad:?}");
    let none = invoke_kind(&model, &app, "InchesToPoints", InvokeKind::Method, vec![]);
    assert!(none.is_err(), "{none:?}");
}

/// dc-oxh8w.46: Range.Select, Worksheet.Select/Activate and Application.Goto move the session's selection;
/// Range.Select on a sheet that is not the active one is 1004 (Excel: "Select method of Range class failed").
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_46__select_activate_and_goto_move_the_selection() {
    let (host, model) = setup();
    let app = model.application();
    let sheets = worksheets(&model);
    let first = obj(invoke(&model, &sheets, "Item", vec![AutoValue::I4(1)]).unwrap());
    let added = obj(invoke_kind(&model, &sheets, "Add", InvokeKind::Method, vec![]).unwrap());
    let sub = |o: &ObjectRef, a: &str| obj(invoke(&model, o, "Range", vec![AutoValue::text(a)]).unwrap());
    let address = |o: &ObjectRef| text(&model, o, "Address");
    let active_name = || {
        let s = obj(invoke(&model, &app, "ActiveSheet", vec![]).unwrap());
        text(&model, &s, "Name")
    };
    let select = |o: &ObjectRef| invoke_kind(&model, o, "Select", InvokeKind::Method, vec![]);

    // The added sheet is active: selecting on the first sheet's range fails like Excel.
    let wrong_sheet = select(&sub(&first, "B2:C3"));
    assert!(matches!(wrong_sheet, Err(AutoError::Excel { number: 1004, .. })), "{wrong_sheet:?}");
    // Activate the first sheet, then select.
    invoke_kind(&model, &first, "Activate", InvokeKind::Method, vec![]).unwrap();
    assert_eq!(active_name(), text(&model, &first, "Name"));
    assert!(matches!(select(&sub(&first, "B2:C3")).unwrap(), AutoValue::Bool(true)));
    let selection = obj(invoke(&model, &app, "Selection", vec![]).unwrap());
    assert_eq!(address(&selection), "$B$2:$C$3");
    let cell = obj(invoke(&model, &app, "ActiveCell", vec![]).unwrap());
    assert_eq!(address(&cell), "$B$2");
    // A multi-area range selects all its areas; the active cell is the first area's first cell.
    let union = obj(invoke_kind(&model, &app, "Union", InvokeKind::Method, vec![
        AutoValue::Object(sub(&first, "E5:F6")), AutoValue::Object(sub(&first, "A1"))]).unwrap());
    select(&union).unwrap();
    assert_eq!(host.session.borrow().selection().areas.len(), 2);
    assert_eq!(address(&obj(invoke(&model, &app, "ActiveCell", vec![]).unwrap())), "$E$5");

    // Select the other sheet: its own selection comes back (A1 for a new sheet), and the first sheet's is remembered.
    select(&sub(&first, "B2:C3")).unwrap();
    invoke_kind(&model, &added, "Select", InvokeKind::Method, vec![]).unwrap();
    assert_eq!(active_name(), text(&model, &added, "Name"));
    assert_eq!(address(&obj(invoke(&model, &app, "ActiveCell", vec![]).unwrap())), "$A$1");
    select(&sub(&added, "C3")).unwrap();
    invoke_kind(&model, &first, "Select", InvokeKind::Method, vec![AutoValue::Bool(true)]).unwrap();
    assert_eq!(active_name(), text(&model, &first, "Name"));
    assert_eq!(address(&obj(invoke(&model, &app, "Selection", vec![]).unwrap())), "$B$2:$C$3");
    // Range.Activate moves the active cell inside the selection; outside it is a typed refusal.
    select(&sub(&first, "B2:C3")).unwrap();
    invoke_kind(&model, &sub(&first, "C3"), "Activate", InvokeKind::Method, vec![]).unwrap();
    assert_eq!(address(&obj(invoke(&model, &app, "ActiveCell", vec![]).unwrap())), "$C$3");
    assert_eq!(address(&obj(invoke(&model, &app, "Selection", vec![]).unwrap())), "$B$2:$C$3");
    // ww20 (Excel 16.0.20625): a cell outside the selection replaces it.
    invoke_kind(&model, &sub(&first, "F9"), "Activate", InvokeKind::Method, vec![]).unwrap();
    assert_eq!(address(&obj(invoke(&model, &app, "Selection", vec![]).unwrap())), "$F$9");
    assert_eq!(address(&obj(invoke(&model, &app, "ActiveCell", vec![]).unwrap())), "$F$9");
    select(&sub(&first, "B2:C3")).unwrap();
    // Select(Replace:=False) would group sheets: typed refusal, no change.
    let group = invoke_kind(&model, &added, "Select", InvokeKind::Method, vec![AutoValue::Bool(false)]);
    assert!(matches!(group, Err(AutoError::NotModelled { .. })), "{group:?}");
    assert_eq!(active_name(), text(&model, &first, "Name"));

    // Goto a range on another sheet activates that sheet and selects the range.
    invoke_kind(&model, &app, "Goto", InvokeKind::Method, vec![AutoValue::Object(sub(&added, "D4:E5"))]).unwrap();
    assert_eq!(active_name(), text(&model, &added, "Name"));
    assert_eq!(address(&obj(invoke(&model, &app, "Selection", vec![]).unwrap())), "$D$4:$E$5");
    // Scroll is accepted; a non-range reference is a type mismatch (13), a bad argument count 1004-class.
    invoke_kind(&model, &app, "Goto", InvokeKind::Method, vec![AutoValue::Object(sub(&first, "A1")), AutoValue::Bool(true)]).unwrap();
    assert_eq!(active_name(), text(&model, &first, "Name"));
    let bad = invoke_kind(&model, &app, "Goto", InvokeKind::Method, vec![AutoValue::R8(3.)]);
    assert!(bad.is_err(), "{bad:?}");
    // A text reference is R1C1 (relative to the active cell), or A1, or a defined name.
    let goto_text = |t: &str| invoke_kind(&model, &app, "Goto", InvokeKind::Method, vec![AutoValue::text(t)]);
    select(&sub(&first, "B2")).unwrap();
    goto_text("R3C4").unwrap();
    assert_eq!(address(&obj(invoke(&model, &app, "Selection", vec![]).unwrap())), "$D$3");
    goto_text("R[1]C[1]:R[2]C[3]").unwrap();
    assert_eq!(address(&obj(invoke(&model, &app, "Selection", vec![]).unwrap())), "$E$4:$G$5");
    goto_text("B7").unwrap();
    assert_eq!(address(&obj(invoke(&model, &app, "Selection", vec![]).unwrap())), "$B$7");
    let unknown = goto_text("NoSuchName");
    assert!(unknown.is_err(), "{unknown:?}");
}

/// dc-oxh8w.49 (ww22, Excel 16.0.20625): Range.Activate of a multi-cell range inside the selection keeps the selection
/// and makes the range's first cell active; Application.CutCopyMode = True, 1, 2, 1.0 or 2.0 clears a marked copy and
/// stays False from False.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_49__multi_cell_activate_and_cut_copy_mode_assignments() {
    let (_host, model) = setup();
    let app = model.application();
    let sheet = obj(invoke(&model, &worksheets(&model), "Item", vec![AutoValue::I4(1)]).unwrap());
    let sub = |a: &str| obj(invoke(&model, &sheet, "Range", vec![AutoValue::text(a)]).unwrap());
    let address = |o: &ObjectRef| text(&model, o, "Address");
    invoke_kind(&model, &sub("A1:B2"), "Select", InvokeKind::Method, vec![]).unwrap();
    for (target, active) in [("B2:D5", "$B$2"), ("A1:B1", "$A$1")] {
        let done = invoke_kind(&model, &sub(target), "Activate", InvokeKind::Method, vec![]);
        assert!(done.is_ok(), "{target}: {done:?}");
        assert_eq!(address(&obj(invoke(&model, &app, "Selection", vec![]).unwrap())), "$A$1:$B$2", "{target}");
        assert_eq!(address(&obj(invoke(&model, &app, "ActiveCell", vec![]).unwrap())), active, "{target}");
    }
    let mode = || invoke(&model, &app, "CutCopyMode", vec![]).unwrap();
    for value in [AutoValue::Bool(true), AutoValue::I4(1), AutoValue::I4(2), AutoValue::R8(1.0), AutoValue::R8(2.0)] {
        invoke_kind(&model, &sub("A1:B2"), "Copy", InvokeKind::Method, vec![]).unwrap();
        assert!(matches!(mode(), AutoValue::I4(1)), "{:?}", mode());
        let put = invoke_kind(&model, &app, "CutCopyMode", InvokeKind::Put, vec![value.clone()]);
        assert!(put.is_ok(), "{value:?}: {put:?}");
        assert!(matches!(mode(), AutoValue::Bool(false)), "{value:?}: {:?}", mode());
        invoke_kind(&model, &app, "CutCopyMode", InvokeKind::Put, vec![value.clone()]).unwrap();
        assert!(matches!(mode(), AutoValue::Bool(false)), "inactive start {value:?}");
    }
}

/// dc-oxh8w.49 (ww21, Excel 16.0.20625): hiding the active (ungrouped) middle sheet activates the following sheet.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_49__hiding_the_active_middle_sheet_activates_the_next() {
    let (_host, model) = setup();
    let app = model.application();
    let sheets = worksheets(&model);
    for _ in 0..2 {
        invoke_kind(&model, &sheets, "Add", InvokeKind::Method, vec![]).unwrap();
    }
    let item = |i: i32| obj(invoke(&model, &sheets, "Item", vec![AutoValue::I4(i)]).unwrap());
    let active = || {
        let s = obj(invoke(&model, &app, "ActiveSheet", vec![]).unwrap());
        text(&model, &s, "Name")
    };
    let (middle, last) = (text(&model, &item(2), "Name"), text(&model, &item(3), "Name"));
    invoke_kind(&model, &item(2), "Activate", InvokeKind::Method, vec![]).unwrap();
    assert_eq!(active(), middle);
    invoke_kind(&model, &item(2), "Visible", InvokeKind::Put, vec![AutoValue::I4(0)]).unwrap();
    assert_eq!(active(), last);
}

/// dc-oxh8w.46: Worksheet.Rows/Columns and Application.Rows/Columns are the whole sheet's rows and columns.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_46__worksheet_and_application_rows_and_columns_cover_the_sheet() {
    let (_host, model) = setup();
    let app = model.application();
    let sheet = obj(invoke(&model, &app, "ActiveSheet", vec![]).unwrap());
    for (owner, label) in [(&sheet, "Worksheet"), (&app, "Application")] {
        let rows = obj(invoke(&model, owner, "Rows", vec![]).unwrap_or_else(|e| panic!("{label}.Rows: {e:?}")));
        assert_eq!(text(&model, &rows, "Address"), "$1:$1048576", "{label}");
        assert!(matches!(invoke(&model, &rows, "Count", vec![]).unwrap(), AutoValue::I4(1_048_576)), "{label}");
        let columns = obj(invoke(&model, owner, "Columns", vec![]).unwrap());
        assert_eq!(text(&model, &columns, "Address"), "$A:$XFD", "{label}");
        assert!(matches!(invoke(&model, &columns, "Count", vec![]).unwrap(), AutoValue::I4(16_384)), "{label}");
    }
    // Neither takes arguments (the Item form is the returned range's).
    assert!(invoke(&model, &sheet, "Rows", vec![AutoValue::I4(2)]).is_err());
}

/// dc-oxh8w.46: Range.Range reads a text cell reference relative to the range's top-left cell; a Range object is
/// used as it is; two arguments span both; a name or union is a typed refusal.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_46__range_range_is_relative_to_the_range() {
    let (_host, model) = setup();
    let base = range(&model, "B2:C3");
    let address = |a: Vec<AutoValue>| {
        let r = obj(invoke(&model, &base, "Range", a).unwrap());
        text(&model, &r, "Address")
    };
    assert_eq!(address(vec![AutoValue::text("A1")]), "$B$2");
    assert_eq!(address(vec![AutoValue::text("B2")]), "$C$3");
    assert_eq!(address(vec![AutoValue::text("A1:B3")]), "$B$2:$C$4");
    assert_eq!(address(vec![AutoValue::text("A1"), AutoValue::text("C3")]), "$B$2:$D$4");
    assert_eq!(address(vec![AutoValue::Object(range(&model, "F6"))]), "$F$6");
    assert_eq!(address(vec![AutoValue::text("D4"), AutoValue::Object(range(&model, "A1"))]), "$A$1:$E$5");
    let off_grid = invoke(&model, &base, "Range", vec![AutoValue::text("XFD1")]);
    assert!(matches!(off_grid, Err(AutoError::Excel { number: 1004, .. })), "{off_grid:?}");
    let unioned = invoke(&model, &base, "Range", vec![AutoValue::text("A1,C3")]);
    assert!(matches!(unioned, Err(AutoError::NotModelled { .. })), "{unioned:?}");
    assert!(invoke(&model, &base, "Range", vec![]).is_err());
}

/// dc-oxh8w.46: Application.DisplayAlerts is a held flag (True at start); Application.CutCopyMode reads False,
/// xlCopy (1) after a Copy and xlCut (2) after a Cut, and False clears it.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_46__display_alerts_and_cut_copy_mode() {
    let (_host, model) = setup();
    let app = model.application();
    assert!(matches!(invoke(&model, &app, "DisplayAlerts", vec![]).unwrap(), AutoValue::Bool(true)));
    invoke_kind(&model, &app, "DisplayAlerts", InvokeKind::Put, vec![AutoValue::Bool(false)]).unwrap();
    assert!(matches!(invoke(&model, &app, "DisplayAlerts", vec![]).unwrap(), AutoValue::Bool(false)));
    let bad = invoke_kind(&model, &app, "DisplayAlerts", InvokeKind::Put, vec![AutoValue::text("x")]);
    assert!(bad.is_err(), "{bad:?}");

    let mode = || invoke(&model, &app, "CutCopyMode", vec![]).unwrap();
    assert!(matches!(mode(), AutoValue::Bool(false)), "{:?}", mode());
    let source = range(&model, "A1:B2");
    invoke_kind(&model, &source, "Copy", InvokeKind::Method, vec![]).unwrap();
    assert!(matches!(mode(), AutoValue::I4(1)), "{:?}", mode());
    invoke_kind(&model, &app, "CutCopyMode", InvokeKind::Put, vec![AutoValue::Bool(false)]).unwrap();
    assert!(matches!(mode(), AutoValue::Bool(false)));
    invoke_kind(&model, &source, "Cut", InvokeKind::Method, vec![]).unwrap();
    assert!(matches!(mode(), AutoValue::I4(2)), "{:?}", mode());
    // Only False (and the xlCopy/xlCut constants Excel reports) are assignable here.
    let bad = invoke_kind(&model, &app, "CutCopyMode", InvokeKind::Put, vec![AutoValue::I4(7)]);
    assert!(bad.is_err(), "{bad:?}");
}

/// Inside a worksheet function call the application and workbook settings answer from the host
/// state the evaluation carries, the same values the session reads give (dc-oxh8w.52).
struct SettingsProbe {
    model: Weak<WorkbookAutomation>,
    seen: Rc<RefCell<Vec<(String, String)>>>,
}
impl std::fmt::Debug for SettingsProbe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SettingsProbe")
    }
}
impl UserFunction for SettingsProbe {
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
            let app = model.application();
            let book = obj(invoke(&model, &app, "ActiveWorkbook", vec![]).unwrap());
            let mut seen = self.seen.borrow_mut();
            for (name, target) in [
                ("Calculation", &app),
                ("CalculateBeforeSave", &app),
                ("Iteration", &app),
                ("MaxIterations", &app),
                ("MaxChange", &app),
                ("CalculationState", &app),
                ("Date1904", &book),
                ("PrecisionAsDisplayed", &book),
            ] {
                seen.push((name.to_owned(), format!("{:?}", invoke(&model, target, name, vec![]))));
            }
            Ok(CalcValue::number(1.))
        })
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn settings_read_inside_a_worksheet_function_call() {
    let (host, model) = setup();
    let app = model.application();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let mut functions = CallableTable::new();
    functions
        .register("SettingsProbe", Rc::new(SettingsProbe { model: Rc::downgrade(&model), seen: seen.clone() }))
        .unwrap();
    host.session.borrow_mut().set_user_functions(functions);
    let caller = at(&host, 20, 5);
    // Expected from the session's own reads, outside any call.
    let outside: Vec<(String, String)> = [
        ("Calculation", &app),
        ("CalculateBeforeSave", &app),
        ("Iteration", &app),
        ("MaxIterations", &app),
        ("MaxChange", &app),
    ]
    .iter()
    .map(|(name, target)| (name.to_string(), format!("{:?}", invoke(&model, target, name, vec![]))))
    .collect();
    enter(&host, caller, "=SettingsProbe()");
    model.context_changed(&host.session.borrow());
    host.session.borrow_mut().dispatch(SkinIntent::Recalculate);
    while host.session.borrow().has_pending_calculation() {
        host.session.borrow_mut().advance_calculation();
    }
    let seen = seen.borrow();
    assert!(!seen.is_empty(), "the probe ran");
    for (name, expected) in &outside {
        let got = seen.iter().rev().find(|(n, _)| n == name).unwrap();
        assert_eq!(&got.1, expected, "{name} inside the call");
    }
    let state = seen.iter().rev().find(|(n, _)| n == "CalculationState").unwrap();
    assert_eq!(state.1, "Ok(I4(1))", "xlCalculating inside a call");
    for name in ["Date1904", "PrecisionAsDisplayed"] {
        let got = seen.iter().rev().find(|(n, _)| n == name).unwrap();
        assert!(got.1.starts_with("Ok(Bool("), "{name}: {}", got.1);
    }
}

/// dc-tqcvu: a macro adds rules with `FormatConditions.Add` (cell value and expression), sets their `Interior.Color`
/// and `Font` afterwards, reads Type/Operator/Formula1/Formula2/Count/Item back and deletes; the saved workbook
/// reopens with the same rules and formats. Bad types, missing operators or formulas are 1004.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_tqcvu__format_conditions_add_set_interior_and_font_and_delete() {
    let (host, model, _) = blank_report();
    let conditions_of = |model: &WorkbookAutomation, address: &str| {
        let target = range(model, address);
        let conditions = obj(invoke(model, &target, "FormatConditions", vec![]).unwrap());
        model.released(&target);
        conditions
    };
    let count = |model: &WorkbookAutomation, conditions: &ObjectRef| int(invoke(model, conditions, "Count", vec![]).unwrap());
    let add = |model: &WorkbookAutomation, conditions: &ObjectRef, args: Vec<AutoValue>| {
        invoke_kind(model, conditions, "Add", InvokeKind::Method, args)
    };
    let conditions = conditions_of(&model, "B2:B6");
    assert_eq!(count(&model, &conditions), 0);
    let greater = obj(add(&model, &conditions, vec![AutoValue::I4(1), AutoValue::I4(5), AutoValue::text("=3")]).unwrap());
    assert_eq!(int(invoke(&model, &greater, "Type", vec![]).unwrap()), 1);
    assert_eq!(int(invoke(&model, &greater, "Operator", vec![]).unwrap()), 5);
    assert_eq!(text(&model, &greater, "Formula1"), "=3");
    let interior = obj(invoke(&model, &greater, "Interior", vec![]).unwrap());
    invoke_kind(&model, &interior, "Color", InvokeKind::Put, vec![AutoValue::I4(255)]).unwrap();
    assert_eq!(int(invoke(&model, &interior, "Color", vec![]).unwrap()), 255);
    let rule_font = obj(invoke(&model, &greater, "Font", vec![]).unwrap());
    put_font(&model, &rule_font, "Bold", AutoValue::Bool(true)).unwrap();
    put_font(&model, &rule_font, "Color", AutoValue::I4(65_280)).unwrap();
    assert!(matches!(invoke(&model, &rule_font, "Bold", vec![]).unwrap(), AutoValue::Bool(true)));
    assert_eq!(int(invoke(&model, &rule_font, "Color", vec![]).unwrap()), 65_280);
    let between = obj(add(
        &model,
        &conditions,
        vec![AutoValue::I4(1), AutoValue::I4(1), AutoValue::text("1"), AutoValue::I4(9)],
    )
    .unwrap());
    assert_eq!(text(&model, &between, "Formula2"), "=9");
    let expression = obj(add(&model, &conditions, vec![AutoValue::I4(2), AutoValue::Missing, AutoValue::text("=B2>B3")]).unwrap());
    assert_eq!(int(invoke(&model, &expression, "Type", vec![]).unwrap()), 2);
    assert_eq!(count(&model, &conditions), 3);
    // A range elsewhere does not see these rules.
    assert_eq!(count(&model, &conditions_of(&model, "D1:D2")), 0);
    let second = obj(invoke_kind(&model, &conditions, "Item", InvokeKind::Method, vec![AutoValue::I4(2)]).unwrap());
    assert_eq!(text(&model, &second, "Formula2"), "=9");
    // Errors are 1004: unknown type, missing operator, missing formula, between without Formula2, subscript.
    for args in [
        vec![AutoValue::I4(99)],
        vec![AutoValue::I4(1)],
        vec![AutoValue::I4(1), AutoValue::I4(5)],
        vec![AutoValue::I4(1), AutoValue::I4(1), AutoValue::text("1")],
        vec![AutoValue::I4(1), AutoValue::I4(42), AutoValue::text("1")],
        vec![AutoValue::I4(2)],
    ] {
        let r = add(&model, &conditions, args.clone());
        assert!(matches!(r, Err(AutoError::Excel { number: 1004, .. })), "{args:?}: {r:?}");
    }
    assert_eq!(count(&model, &conditions), 3);
    let r = invoke_kind(&model, &conditions, "Item", InvokeKind::Method, vec![AutoValue::I4(4)]);
    assert!(matches!(r, Err(AutoError::Excel { number: 1004, .. })), "{r:?}");
    invoke_kind(&model, &greater, "StopIfTrue", InvokeKind::Put, vec![AutoValue::Bool(true)]).unwrap();
    assert!(matches!(invoke(&model, &greater, "StopIfTrue", vec![]).unwrap(), AutoValue::Bool(true)));
    assert_eq!(int(invoke(&model, &greater, "Priority", vec![]).unwrap()), 1);
    assert_eq!(int(invoke(&model, &expression, "Priority", vec![]).unwrap()), 3);
    // The saved workbook reopens with the rules and the rule's own format.
    let reopened = reopen_formatting(&host);
    let again = conditions_of(&reopened, "B2:B6");
    assert_eq!(count(&reopened, &again), 3);
    let first = obj(invoke_kind(&reopened, &again, "Item", InvokeKind::Method, vec![AutoValue::I4(1)]).unwrap());
    assert_eq!(text(&reopened, &first, "Formula1"), "=3");
    let interior = obj(invoke(&reopened, &first, "Interior", vec![]).unwrap());
    assert_eq!(int(invoke(&reopened, &interior, "Color", vec![]).unwrap()), 255);
    // Delete one rule, then the rest.
    invoke_kind(&model, &greater, "Delete", InvokeKind::Method, vec![]).unwrap();
    assert_eq!(count(&model, &conditions), 2);
    invoke_kind(&model, &conditions, "Delete", InvokeKind::Method, vec![]).unwrap();
    assert_eq!(count(&model, &conditions), 0);
}

/// dc-oxh8w.62: `FormatCondition.AppliesTo`, `ModifyAppliesToRange`, `Priority` put, `SetFirstPriority` and
/// `SetLastPriority`. Moving a rule's range changes which `FormatConditions` collections see it; priorities are
/// positions in the worksheet's list. A priority outside 1..Count, a non-range argument and a range on another
/// worksheet are 1004.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_62__format_condition_applies_to_and_priority() {
    let (_host, model, _) = blank_report();
    let conditions_of = |address: &str| {
        let target = range(&model, address);
        let conditions = obj(invoke(&model, &target, "FormatConditions", vec![]).unwrap());
        model.released(&target);
        conditions
    };
    let count = |c: &ObjectRef| int(invoke(&model, c, "Count", vec![]).unwrap());
    let conditions = conditions_of("B2:B6");
    let add = |formula: &str| {
        obj(invoke_kind(&model, &conditions, "Add", InvokeKind::Method, vec![AutoValue::I4(2), AutoValue::Missing, AutoValue::text(formula)]).unwrap())
    };
    let (a, b, c) = (add("=B2>1"), add("=B2>2"), add("=B2>3"));
    let priority = |r: &ObjectRef| int(invoke(&model, r, "Priority", vec![]).unwrap());
    assert_eq!((priority(&a), priority(&b), priority(&c)), (1, 2, 3));
    let applies = obj(invoke(&model, &a, "AppliesTo", vec![]).unwrap());
    assert_eq!(text(&model, &applies, "Address"), "$B$2:$B$6");
    // Priority put, SetFirstPriority and SetLastPriority.
    invoke_kind(&model, &c, "SetFirstPriority", InvokeKind::Method, vec![]).unwrap();
    assert_eq!((priority(&a), priority(&b), priority(&c)), (2, 3, 1));
    invoke_kind(&model, &c, "SetLastPriority", InvokeKind::Method, vec![]).unwrap();
    assert_eq!((priority(&a), priority(&b), priority(&c)), (1, 2, 3));
    invoke_kind(&model, &b, "Priority", InvokeKind::Put, vec![AutoValue::I4(1)]).unwrap();
    assert_eq!((priority(&a), priority(&b), priority(&c)), (2, 1, 3));
    for bad in [0, 4, -1] {
        let r = invoke_kind(&model, &a, "Priority", InvokeKind::Put, vec![AutoValue::I4(bad)]);
        assert!(matches!(r, Err(AutoError::Excel { number: 1004, .. })), "{bad}: {r:?}");
    }
    // ModifyAppliesToRange moves the rule out of B2:B6 and into D1:D2.
    let elsewhere = range(&model, "D1:D2");
    invoke_kind(&model, &a, "ModifyAppliesToRange", InvokeKind::Method, vec![AutoValue::Object(elsewhere)]).unwrap();
    assert_eq!(count(&conditions), 2);
    assert_eq!(count(&conditions_of("D1:D2")), 1);
    let applies = obj(invoke(&model, &a, "AppliesTo", vec![]).unwrap());
    assert_eq!(text(&model, &applies, "Address"), "$D$1:$D$2");
    for bad in [AutoValue::text("D1"), AutoValue::I4(3)] {
        let r = invoke_kind(&model, &a, "ModifyAppliesToRange", InvokeKind::Method, vec![bad.clone()]);
        assert!(matches!(r, Err(AutoError::Excel { number: 1004, .. })) || r.is_err(), "{bad:?}: {r:?}");
    }
    let r = invoke_kind(&model, &a, "ModifyAppliesToRange", InvokeKind::Method, vec![]);
    assert!(r.is_err(), "{r:?}");
}

/// dc-jupjv / dc-qrnem: Excel creates top-10, above-average and duplicate-values rules via AddTop10,
/// AddAboveAverage and AddUniqueValues (dc-oawf4), not FormatConditions.Add(5|8|12); those stay typed
/// refusals citing dc-jupjv until Excel's error behaviour on Add(5|8|12) is recorded.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_jupjv__format_conditions_add_top10_average_and_duplicates() {
    let (_host, model, _) = blank_report();
    let target = range(&model, "B2:B6");
    let conditions = obj(invoke(&model, &target, "FormatConditions", vec![]).unwrap());
    for kind in [5, 8, 12] {
        let r = invoke_kind(&model, &conditions, "Add", InvokeKind::Method, vec![AutoValue::I4(kind)]);
        assert!(matches!(&r, Err(AutoError::NotModelled { owner, .. }) if *owner == "dc-jupjv"), "{kind}: {r:?}");
    }
    assert_eq!(int(invoke(&model, &conditions, "Count", vec![]).unwrap()), 0);
}

/// dc-jupjv (ww21): FormatCondition.Operator on an expression rule and absent Formula2 return Empty;
/// an unset FormatCondition.Interior.Color returns 0.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_jupjv__expression_operator_formula2_and_unset_interior_match_excel() {
    let (_host, model, _) = blank_report();
    let target = range(&model, "B2:B6");
    let conditions = obj(invoke(&model, &target, "FormatConditions", vec![]).unwrap());
    let cell_rule = obj(
        invoke_kind(
            &model,
            &conditions,
            "Add",
            InvokeKind::Method,
            vec![AutoValue::I4(1), AutoValue::I4(5), AutoValue::text("=3")],
        )
        .unwrap(),
    );
    let expr_rule = obj(
        invoke_kind(
            &model,
            &conditions,
            "Add",
            InvokeKind::Method,
            vec![AutoValue::I4(2), AutoValue::Missing, AutoValue::text("=B2>B3")],
        )
        .unwrap(),
    );
    assert!(matches!(invoke(&model, &cell_rule, "Formula2", vec![]).unwrap(), AutoValue::Empty));
    assert!(matches!(invoke(&model, &expr_rule, "Operator", vec![]).unwrap(), AutoValue::Empty));
    assert!(matches!(invoke(&model, &expr_rule, "Formula2", vec![]).unwrap(), AutoValue::Empty));
    let interior = obj(invoke(&model, &cell_rule, "Interior", vec![]).unwrap());
    assert_eq!(int(invoke(&model, &interior, "Color", vec![]).unwrap()), 0);
}

/// dc-jupjv (ww21): FormatCondition.Font Bold=False / Italic=False (dxf b/i val="0"), Font.Underline,
/// NumberFormat and Borders get/put round-trip through save/reopen; invalid arguments give 1004.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_jupjv__font_false_underline_number_format_and_borders() {
    let (host, model, _) = blank_report();
    let target = range(&model, "B2:B6");
    let conditions = obj(invoke(&model, &target, "FormatConditions", vec![]).unwrap());
    let rule = obj(
        invoke_kind(
            &model,
            &conditions,
            "Add",
            InvokeKind::Method,
            vec![AutoValue::I4(1), AutoValue::I4(5), AutoValue::text("=3")],
        )
        .unwrap(),
    );
    // Font: Bold=False, Italic=False, Underline=xlUnderlineStyleSingle (2).
    let font = obj(invoke(&model, &rule, "Font", vec![]).unwrap());
    invoke_kind(&model, &font, "Bold", InvokeKind::Put, vec![AutoValue::Bool(false)]).unwrap();
    invoke_kind(&model, &font, "Italic", InvokeKind::Put, vec![AutoValue::Bool(false)]).unwrap();
    invoke_kind(&model, &font, "Underline", InvokeKind::Put, vec![AutoValue::I4(2)]).unwrap();
    assert!(matches!(invoke(&model, &font, "Bold", vec![]).unwrap(), AutoValue::Bool(false)));
    assert!(matches!(invoke(&model, &font, "Italic", vec![]).unwrap(), AutoValue::Bool(false)));
    assert_eq!(int(invoke(&model, &font, "Underline", vec![]).unwrap()), 2);
    let bad_u = invoke_kind(&model, &font, "Underline", InvokeKind::Put, vec![AutoValue::I4(999)]);
    assert!(matches!(bad_u, Err(AutoError::Excel { number: 1004, .. })), "{bad_u:?}");
    // NumberFormat: initially Empty, then "0.00".
    assert!(matches!(invoke(&model, &rule, "NumberFormat", vec![]).unwrap(), AutoValue::Empty));
    invoke_kind(&model, &rule, "NumberFormat", InvokeKind::Put, vec![AutoValue::text("0.00")]).unwrap();
    assert_eq!(text(&model, &rule, "NumberFormat"), "0.00");
    let bad_nf = invoke_kind(&model, &rule, "NumberFormat", InvokeKind::Put, vec![AutoValue::I4(7)]);
    assert!(matches!(bad_nf, Err(AutoError::Excel { number: 1004, .. })), "{bad_nf:?}");
    // Borders: xlEdgeLeft (7) Continuous, Thin, Red (255).
    let borders = obj(invoke(&model, &rule, "Borders", vec![]).unwrap());
    let left = obj(invoke(&model, &borders, "Item", vec![AutoValue::I4(7)]).unwrap());
    invoke_kind(&model, &left, "LineStyle", InvokeKind::Put, vec![AutoValue::I4(1)]).unwrap();
    invoke_kind(&model, &left, "Weight", InvokeKind::Put, vec![AutoValue::I4(2)]).unwrap();
    invoke_kind(&model, &left, "Color", InvokeKind::Put, vec![AutoValue::I4(255)]).unwrap();
    assert_eq!(int(invoke(&model, &left, "LineStyle", vec![]).unwrap()), 1);
    assert_eq!(int(invoke(&model, &left, "Weight", vec![]).unwrap()), 2);
    assert_eq!(int(invoke(&model, &left, "Color", vec![]).unwrap()), 255);
    let bad_edge = invoke(&model, &borders, "Item", vec![AutoValue::I4(99)]);
    assert!(matches!(bad_edge, Err(AutoError::Excel { number: 1004, .. })), "{bad_edge:?}");
    // Save and verify the dxf XML carries explicit b val="0" and i val="0", and reopens intact.
    let mut bytes = Vec::new();
    host.session.borrow_mut().save(&SavePolicy::default(), &mut bytes).unwrap();
    let reopened = reopen_formatting(&host);
    let r_target = range(&reopened, "B2:B6");
    let r_conds = obj(invoke(&reopened, &r_target, "FormatConditions", vec![]).unwrap());
    let r_rule = obj(invoke_kind(&reopened, &r_conds, "Item", InvokeKind::Method, vec![AutoValue::I4(1)]).unwrap());
    let r_font = obj(invoke(&reopened, &r_rule, "Font", vec![]).unwrap());
    assert!(matches!(invoke(&reopened, &r_font, "Bold", vec![]).unwrap(), AutoValue::Bool(false)));
    assert!(matches!(invoke(&reopened, &r_font, "Italic", vec![]).unwrap(), AutoValue::Bool(false)));
    assert_eq!(int(invoke(&reopened, &r_font, "Underline", vec![]).unwrap()), 2);
    assert_eq!(text(&reopened, &r_rule, "NumberFormat"), "0.00");
    let r_borders = obj(invoke(&reopened, &r_rule, "Borders", vec![]).unwrap());
    let r_left = obj(invoke(&reopened, &r_borders, "Item", vec![AutoValue::I4(7)]).unwrap());
    assert_eq!(int(invoke(&reopened, &r_left, "LineStyle", vec![]).unwrap()), 1);
    assert_eq!(int(invoke(&reopened, &r_left, "Color", vec![]).unwrap()), 255);
    // Confirm the reopened dxf carries bold_off and italic_off from b/i val="0".
    let dxfs = host.session.borrow().host().styles().differential_formats.clone();
    let dxf = dxfs.last().unwrap();
    let f = dxf.font.as_ref().unwrap();
    assert!(!f.bold && f.bold_off, "expected explicit bold_off: {f:?}");
    assert!(!f.italic && f.italic_off, "expected explicit italic_off: {f:?}");
}

/// dc-jupjv: editing a FormatCondition whose dxf came from the opened file copies-on-write to a new dxf
/// instead of refusing or mutating loaded dxf records.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_jupjv__file_dxf_copy_on_write() {
    let (host, model, _) = blank_report();
    let target = range(&model, "B2:B6");
    let conditions = obj(invoke(&model, &target, "FormatConditions", vec![]).unwrap());
    let rule = obj(
        invoke_kind(
            &model,
            &conditions,
            "Add",
            InvokeKind::Method,
            vec![AutoValue::I4(1), AutoValue::I4(5), AutoValue::text("=3")],
        )
        .unwrap(),
    );
    let interior = obj(invoke(&model, &rule, "Interior", vec![]).unwrap());
    invoke_kind(&model, &interior, "Color", InvokeKind::Put, vec![AutoValue::I4(255)]).unwrap();
    // Reopen so the rule's dxfId points into loaded.styles().differential_formats.
    let reopened = reopen_formatting(&host);
    let r_target = range(&reopened, "B2:B6");
    let r_conds = obj(invoke(&reopened, &r_target, "FormatConditions", vec![]).unwrap());
    let r_rule = obj(invoke_kind(&reopened, &r_conds, "Item", InvokeKind::Method, vec![AutoValue::I4(1)]).unwrap());
    let r_interior = obj(invoke(&reopened, &r_rule, "Interior", vec![]).unwrap());
    assert_eq!(int(invoke(&reopened, &r_interior, "Color", vec![]).unwrap()), 255);
    invoke_kind(&reopened, &r_interior, "Color", InvokeKind::Put, vec![AutoValue::I4(65_280)]).unwrap();
    assert_eq!(int(invoke(&reopened, &r_interior, "Color", vec![]).unwrap()), 65_280);
}

/// dc-jupjv (ww21 Condition.Delete.partial): deleting FormatConditions over part of a rule's range
/// removes the rule only from the requested cells and keeps the remaining strips intact.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_jupjv__partial_delete_subtracts_range() {
    let (_host, model, _) = blank_report();
    let conditions_of = |addr: &str| {
        let r = range(&model, addr);
        let c = obj(invoke(&model, &r, "FormatConditions", vec![]).unwrap());
        model.released(&r);
        c
    };
    let count = |addr: &str| int(invoke(&model, &conditions_of(addr), "Count", vec![]).unwrap());
    let conds = conditions_of("B2:B6");
    for formula in ["=B2>1", "=B2>2", "=B2>3"] {
        invoke_kind(
            &model,
            &conds,
            "Add",
            InvokeKind::Method,
            vec![AutoValue::I4(2), AutoValue::Missing, AutoValue::text(formula)],
        )
        .unwrap();
    }
    invoke_kind(&model, &conditions_of("B3:B4"), "Delete", InvokeKind::Method, vec![]).unwrap();
    assert_eq!(count("B2"), 3);
    assert_eq!(count("B3:B4"), 0);
    assert_eq!(count("B5:B6"), 3);
    // 2D hole cutout: remove C3 from B2:D4.
    let box_conds = conditions_of("B2:D4");
    invoke_kind(
        &model,
        &box_conds,
        "Add",
        InvokeKind::Method,
        vec![AutoValue::I4(2), AutoValue::Missing, AutoValue::text("=B2>9")],
    )
    .unwrap();
    invoke_kind(&model, &conditions_of("C3"), "Delete", InvokeKind::Method, vec![]).unwrap();
    assert_eq!(count("C3"), 0);
    assert_eq!(count("C2"), 1);
    assert_eq!(count("C4"), 1);
    assert_eq!(count("D3"), 1);
}

/// dc-lj4j5: `Style.NumberFormat` put changes the format of a style no cell uses yet; a range given the style shows it.
/// Normal is in use by every cell, so changing it is refused with a typed not-modelled error; an unusable format code
/// is 1004.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_lj4j5__style_number_format_put() {
    let (_host, model, _) = blank_report();
    let book = obj(invoke(&model, &model.application(), "ActiveWorkbook", vec![]).unwrap());
    let styles = obj(invoke(&model, &book, "Styles", vec![]).unwrap());
    let mine = obj(invoke_kind(&model, &styles, "Add", InvokeKind::Method, vec![AutoValue::text("Mine5")]).unwrap());
    invoke_kind(&model, &mine, "NumberFormat", InvokeKind::Put, vec![AutoValue::text("0.00")]).unwrap();
    assert_eq!(text(&model, &mine, "NumberFormat"), "0.00");
    let cell = range(&model, "D2");
    invoke_kind(&model, &cell, "Value", InvokeKind::Put, vec![AutoValue::I4(30)]).unwrap();
    invoke_kind(&model, &cell, "Style", InvokeKind::Put, vec![AutoValue::text("Mine5")]).unwrap();
    assert_eq!(text(&model, &cell, "NumberFormat"), "0.00");
    assert_eq!(text(&model, &cell, "Text"), "30.00");
    let normal = obj(invoke_kind(&model, &styles, "Item", InvokeKind::Get, vec![AutoValue::text("Normal")]).unwrap());
    let r = invoke_kind(&model, &normal, "NumberFormat", InvokeKind::Put, vec![AutoValue::text("0.0")]);
    assert!(matches!(r, Err(AutoError::NotModelled { .. })), "{r:?}");
    let r = invoke_kind(&model, &mine, "NumberFormat", InvokeKind::Put, vec![AutoValue::I4(3)]);
    assert!(matches!(r, Err(AutoError::Excel { number: 1004, .. })), "{r:?}");
}

/// dc-lj4j5: `Style.WrapText/ShrinkToFit/Locked/FormulaHidden` puts change a style no cell uses; a cell given the style
/// shows them; Normal (in use) is refused typed; a non-boolean is 1004.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_lj4j5__style_flag_puts() {
    let (_host, model, _) = blank_report();
    let book = obj(invoke(&model, &model.application(), "ActiveWorkbook", vec![]).unwrap());
    let styles = obj(invoke(&model, &book, "Styles", vec![]).unwrap());
    let mine = obj(invoke_kind(&model, &styles, "Add", InvokeKind::Method, vec![AutoValue::text("Mine6")]).unwrap());
    for m in ["WrapText", "ShrinkToFit", "FormulaHidden"] {
        invoke_kind(&model, &mine, m, InvokeKind::Put, vec![AutoValue::Bool(true)]).unwrap();
        assert!(matches!(invoke(&model, &mine, m, vec![]).unwrap(), AutoValue::Bool(true)), "{m}");
    }
    invoke_kind(&model, &mine, "Locked", InvokeKind::Put, vec![AutoValue::Bool(false)]).unwrap();
    assert!(matches!(invoke(&model, &mine, "Locked", vec![]).unwrap(), AutoValue::Bool(false)));
    let cell = range(&model, "E2");
    invoke_kind(&model, &cell, "Style", InvokeKind::Put, vec![AutoValue::text("Mine6")]).unwrap();
    assert!(matches!(invoke(&model, &cell, "WrapText", vec![]).unwrap(), AutoValue::Bool(true)));
    let normal = obj(invoke_kind(&model, &styles, "Item", InvokeKind::Get, vec![AutoValue::text("Normal")]).unwrap());
    let r = invoke_kind(&model, &normal, "WrapText", InvokeKind::Put, vec![AutoValue::Bool(true)]);
    assert!(matches!(r, Err(AutoError::NotModelled { .. })), "{r:?}");
    let r = invoke_kind(&model, &mine, "WrapText", InvokeKind::Put, vec![AutoValue::I4(3)]);
    assert!(matches!(r, Err(AutoError::Excel { number: 1004, .. })), "{r:?}");
}

/// dc-lj4j5: Style.HorizontalAlignment, VerticalAlignment, IndentLevel, Orientation, Font and Interior
/// writes update an unused custom style, propagate to ranges that apply the style, and round-trip through save.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_lj4j5__style_alignment_font_and_interior_puts() {
    let (host, model, _) = blank_report();
    let book = obj(invoke(&model, &model.application(), "ActiveWorkbook", vec![]).unwrap());
    let styles = obj(invoke(&model, &book, "Styles", vec![]).unwrap());
    let mine = obj(invoke_kind(&model, &styles, "Add", InvokeKind::Method, vec![AutoValue::text("Mine7")]).unwrap());
    // Alignment puts.
    invoke_kind(&model, &mine, "HorizontalAlignment", InvokeKind::Put, vec![AutoValue::I4(-4108)]).unwrap(); // xlCenter
    invoke_kind(&model, &mine, "VerticalAlignment", InvokeKind::Put, vec![AutoValue::I4(-4160)]).unwrap(); // xlTop
    invoke_kind(&model, &mine, "IndentLevel", InvokeKind::Put, vec![AutoValue::I4(3)]).unwrap();
    invoke_kind(&model, &mine, "Orientation", InvokeKind::Put, vec![AutoValue::I4(-45)]).unwrap();
    assert_eq!(int(invoke(&model, &mine, "HorizontalAlignment", vec![]).unwrap()), -4108);
    assert_eq!(int(invoke(&model, &mine, "VerticalAlignment", vec![]).unwrap()), -4160);
    assert_eq!(int(invoke(&model, &mine, "IndentLevel", vec![]).unwrap()), 3);
    assert_eq!(int(invoke(&model, &mine, "Orientation", vec![]).unwrap()), -45);
    // Out-of-range / invalid enum gives 1004.
    for (m, v) in [("HorizontalAlignment", 999), ("VerticalAlignment", 999), ("IndentLevel", 16), ("Orientation", 91)] {
        let r = invoke_kind(&model, &mine, m, InvokeKind::Put, vec![AutoValue::I4(v)]);
        assert!(matches!(r, Err(AutoError::Excel { number: 1004, .. })), "{m}={v}: {r:?}");
    }
    // Style.Font get/put.
    let font = obj(invoke(&model, &mine, "Font", vec![]).unwrap());
    invoke_kind(&model, &font, "Bold", InvokeKind::Put, vec![AutoValue::Bool(true)]).unwrap();
    invoke_kind(&model, &font, "Italic", InvokeKind::Put, vec![AutoValue::Bool(true)]).unwrap();
    invoke_kind(&model, &font, "Size", InvokeKind::Put, vec![AutoValue::I4(14)]).unwrap();
    invoke_kind(&model, &font, "Name", InvokeKind::Put, vec![AutoValue::text("Arial")]).unwrap();
    invoke_kind(&model, &font, "Color", InvokeKind::Put, vec![AutoValue::I4(255)]).unwrap();
    assert!(matches!(invoke(&model, &font, "Bold", vec![]).unwrap(), AutoValue::Bool(true)));
    assert!(matches!(invoke(&model, &font, "Italic", vec![]).unwrap(), AutoValue::Bool(true)));
    assert_eq!(text(&model, &font, "Name"), "Arial");
    assert_eq!(int(invoke(&model, &font, "Color", vec![]).unwrap()), 255);
    // Style.Interior get/put.
    let interior = obj(invoke(&model, &mine, "Interior", vec![]).unwrap());
    invoke_kind(&model, &interior, "Color", InvokeKind::Put, vec![AutoValue::I4(65_280)]).unwrap();
    assert_eq!(int(invoke(&model, &interior, "Color", vec![]).unwrap()), 65_280);
    assert_eq!(int(invoke(&model, &interior, "Pattern", vec![]).unwrap()), 1);
    // Apply to a cell and verify the cell inherits them and survives save/reopen.
    let cell = range(&model, "F2");
    invoke_kind(&model, &cell, "Style", InvokeKind::Put, vec![AutoValue::text("Mine7")]).unwrap();
    assert_eq!(int(invoke(&model, &cell, "HorizontalAlignment", vec![]).unwrap()), -4108);
    assert_eq!(int(invoke(&model, &cell, "IndentLevel", vec![]).unwrap()), 3);
    assert_eq!(int(invoke(&model, &cell, "Orientation", vec![]).unwrap()), -45);
    let cell_font = obj(invoke(&model, &cell, "Font", vec![]).unwrap());
    assert!(matches!(invoke(&model, &cell_font, "Bold", vec![]).unwrap(), AutoValue::Bool(true)));
    assert_eq!(text(&model, &cell_font, "Name"), "Arial");
    let cell_int = obj(invoke(&model, &cell, "Interior", vec![]).unwrap());
    assert_eq!(int(invoke(&model, &cell_int, "Color", vec![]).unwrap()), 65_280);
    let reopened = reopen_formatting(&host);
    let r_book = obj(invoke(&reopened, &reopened.application(), "ActiveWorkbook", vec![]).unwrap());
    let r_styles = obj(invoke(&reopened, &r_book, "Styles", vec![]).unwrap());
    let r_mine = obj(invoke_kind(&reopened, &r_styles, "Item", InvokeKind::Get, vec![AutoValue::text("Mine7")]).unwrap());
    assert_eq!(int(invoke(&reopened, &r_mine, "HorizontalAlignment", vec![]).unwrap()), -4108);
    let r_font = obj(invoke(&reopened, &r_mine, "Font", vec![]).unwrap());
    assert!(matches!(invoke(&reopened, &r_font, "Bold", vec![]).unwrap(), AutoValue::Bool(true)));
    let r_int = obj(invoke(&reopened, &r_mine, "Interior", vec![]).unwrap());
    assert_eq!(int(invoke(&reopened, &r_int, "Color", vec![]).unwrap()), 65_280);
}

/// dc-lj4j5 (ww21): Styles.Add accepts BasedOn:=Range and empty name "", and deleting a custom style
/// in use resets cells using it back to Normal.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_lj4j5__styles_add_based_on_empty_name_and_delete_in_use() {
    let (_host, model, _) = blank_report();
    let book = obj(invoke(&model, &model.application(), "ActiveWorkbook", vec![]).unwrap());
    let styles = obj(invoke(&model, &book, "Styles", vec![]).unwrap());
    // Format B2, then Styles.Add("FromB2", BasedOn:=Range("B2")).
    let b2 = range(&model, "B2");
    invoke_kind(&model, &b2, "NumberFormat", InvokeKind::Put, vec![AutoValue::text("0.000")]).unwrap();
    let b2_font = obj(invoke(&model, &b2, "Font", vec![]).unwrap());
    invoke_kind(&model, &b2_font, "Bold", InvokeKind::Put, vec![AutoValue::Bool(true)]).unwrap();
    let from_b2 = obj(
        invoke_kind(
            &model,
            &styles,
            "Add",
            InvokeKind::Method,
            vec![AutoValue::text("FromB2"), AutoValue::Object(b2.clone())],
        )
        .unwrap(),
    );
    assert_eq!(text(&model, &from_b2, "NumberFormat"), "0.000");
    let s_font = obj(invoke(&model, &from_b2, "Font", vec![]).unwrap());
    assert!(matches!(invoke(&model, &s_font, "Bold", vec![]).unwrap(), AutoValue::Bool(true)));
    // Non-range BasedOn gives 1004.
    let bad = invoke_kind(&model, &styles, "Add", InvokeKind::Method, vec![AutoValue::text("BadBased"), AutoValue::I4(1)]);
    assert!(matches!(bad, Err(AutoError::Excel { number: 1004, .. })), "{bad:?}");
    // ww21 Styles.Add.empty.confirm: empty name "" is accepted.
    let empty = obj(invoke_kind(&model, &styles, "Add", InvokeKind::Method, vec![AutoValue::text("")]).unwrap());
    assert_eq!(text(&model, &empty, "Name"), "");
    // ww21 Styles.Delete.inUse: deleting an in-use custom style resets the cell to Normal.
    let d10 = range(&model, "D10");
    invoke_kind(&model, &d10, "Style", InvokeKind::Put, vec![AutoValue::text("FromB2")]).unwrap();
    assert_eq!(text(&model, &obj(invoke(&model, &d10, "Style", vec![]).unwrap()), "Name"), "FromB2");
    invoke_kind(&model, &from_b2, "Delete", InvokeKind::Method, vec![]).unwrap();
    assert_eq!(text(&model, &obj(invoke(&model, &d10, "Style", vec![]).unwrap()), "Name"), "Normal");
}

/// dc-p3rg9: `Window.Caption` put sets the window's title for the macro session and the get returns it; a non-text
/// value is 1004.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_p3rg9__window_caption_put() {
    let (_host, model, _) = blank_report();
    let window = obj(invoke(&model, &model.application(), "ActiveWindow", vec![]).unwrap());
    let book = obj(invoke(&model, &model.application(), "ActiveWorkbook", vec![]).unwrap());
    let before = text(&model, &window, "Caption");
    assert_eq!(before, text(&model, &book, "Name"));
    invoke_kind(&model, &window, "Caption", InvokeKind::Put, vec![AutoValue::text("Quarterly")]).unwrap();
    assert_eq!(text(&model, &window, "Caption"), "Quarterly");
    // The workbook's own name is unchanged.
    assert_eq!(text(&model, &book, "Name"), before);
    let r = invoke_kind(&model, &window, "Caption", InvokeKind::Put, vec![AutoValue::Object(book.clone())]);
    assert!(matches!(r, Err(AutoError::Excel { number: 1004, .. })), "{r:?}");
}

/// dc-oxh8w.69: `Window.Caption` is the workbook's name, `Type` is xlWorkbook, `WindowNumber` is 1, `Visible` is True,
/// and `Activate` makes the workbook active. An argument is 1004-class error rather than ignored.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_69__window_caption_type_number_visible_and_activate() {
    let (_host, model, _) = blank_report();
    let app = model.application();
    let window = obj(invoke(&model, &app, "ActiveWindow", vec![]).unwrap());
    let book = obj(invoke(&model, &app, "ActiveWorkbook", vec![]).unwrap());
    assert_eq!(text(&model, &window, "Caption"), text(&model, &book, "Name"));
    assert_eq!(int(invoke(&model, &window, "Type", vec![]).unwrap()), 1);
    assert_eq!(int(invoke(&model, &window, "WindowNumber", vec![]).unwrap()), 1);
    assert!(matches!(invoke(&model, &window, "Visible", vec![]).unwrap(), AutoValue::Bool(true)));
    invoke_kind(&model, &window, "Activate", InvokeKind::Method, vec![]).unwrap();
    let active = obj(invoke(&model, &app, "ActiveWorkbook", vec![]).unwrap());
    assert_eq!(active.key, book.key);
    let r = invoke_kind(&model, &window, "Activate", InvokeKind::Method, vec![AutoValue::I4(1)]);
    assert!(r.is_err(), "{r:?}");
    let r = invoke(&model, &window, "Caption", vec![AutoValue::I4(1)]);
    assert!(r.is_err(), "{r:?}");
}

/// dc-oxh8w.63: the alignment and protection members of a `Style` read the style's own format: Normal and a style a
/// macro adds answer Excel's defaults, `Value` and the default member name the style, and an argument is 1004.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_63__style_alignment_and_protection_members_read_the_style_format() {
    let (_host, model, _) = blank_report();
    let book = obj(invoke(&model, &model.application(), "ActiveWorkbook", vec![]).unwrap());
    let styles = obj(invoke(&model, &book, "Styles", vec![]).unwrap());
    let normal = obj(invoke_kind(&model, &styles, "Item", InvokeKind::Get, vec![AutoValue::text("Normal")]).unwrap());
    let mine = obj(invoke_kind(&model, &styles, "Add", InvokeKind::Method, vec![AutoValue::text("Mine63")]).unwrap());
    for style in [&normal, &mine] {
        assert_eq!(int(invoke(&model, style, "HorizontalAlignment", vec![]).unwrap()), 1);
        assert_eq!(int(invoke(&model, style, "VerticalAlignment", vec![]).unwrap()), -4107);
        assert_eq!(int(invoke(&model, style, "IndentLevel", vec![]).unwrap()), 0);
        assert_eq!(int(invoke(&model, style, "Orientation", vec![]).unwrap()), 0);
        for (member, expected) in [("WrapText", false), ("ShrinkToFit", false), ("Locked", true), ("FormulaHidden", false)] {
            let got = invoke(&model, style, member, vec![]).unwrap();
            assert!(matches!(got, AutoValue::Bool(b) if b == expected), "{member}: {got:?}");
        }
    }
    assert_eq!(text(&model, &mine, "Value"), "Mine63");
    let default = invoke_kind(&model, &mine, "_Default", InvokeKind::Get, vec![]).unwrap();
    assert_eq!(default.as_text().as_deref(), Some("Mine63"));
    let r = invoke(&model, &mine, "WrapText", vec![AutoValue::I4(1)]);
    assert!(r.is_err(), "{r:?}");
}

/// dc-orxz0: `Workbook.Styles` with Count, Item (index and name), Add and Style.Delete. A style a macro adds applies to
/// ranges, saves and reopens; a duplicate or empty name, an unknown name, deleting Normal and deleting a style in use
/// are refused (1004 where Excel answers an error).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_orxz0__styles_collection_adds_applies_saves_and_deletes() {
    let (host, model, _) = blank_report();
    let book = obj(invoke(&model, &model.application(), "ActiveWorkbook", vec![]).unwrap());
    let styles = obj(invoke(&model, &book, "Styles", vec![]).unwrap());
    let before = int(invoke(&model, &styles, "Count", vec![]).unwrap());
    assert!(before >= 1);
    let normal = obj(invoke_kind(&model, &styles, "Item", InvokeKind::Get, vec![AutoValue::text("normal")]).unwrap());
    assert_eq!(text(&model, &normal, "Name"), "Normal");
    let mine = obj(invoke_kind(&model, &styles, "Add", InvokeKind::Method, vec![AutoValue::text("Mine")]).unwrap());
    assert_eq!(text(&model, &mine, "Name"), "Mine");
    assert!(matches!(invoke(&model, &mine, "BuiltIn", vec![]).unwrap(), AutoValue::Bool(false)));
    assert_eq!(int(invoke(&model, &styles, "Count", vec![]).unwrap()), before + 1);
    let last = obj(invoke_kind(&model, &styles, "Item", InvokeKind::Get, vec![AutoValue::I4(before as i32 + 1)]).unwrap());
    assert_eq!(text(&model, &last, "Name"), "Mine");
    for args in [vec![AutoValue::text("MINE")], vec![AutoValue::I4(3)]] {
        let r = invoke_kind(&model, &styles, "Add", InvokeKind::Method, args.clone());
        assert!(matches!(r, Err(AutoError::Excel { number: 1004, .. })), "{args:?}: {r:?}");
    }
    for item in [AutoValue::text("nope"), AutoValue::I4(0), AutoValue::I4(before as i32 + 2)] {
        let r = invoke_kind(&model, &styles, "Item", InvokeKind::Get, vec![item.clone()]);
        assert!(matches!(r, Err(AutoError::Excel { number: 1004, .. })), "{item:?}: {r:?}");
    }
    let r = invoke_kind(&model, &normal, "Delete", InvokeKind::Method, vec![]);
    assert!(matches!(r, Err(AutoError::Excel { number: 1004, .. })), "{r:?}");
    // Apply it, save, reopen: the style is still there and still applied.
    let target = range(&model, "B2");
    invoke_kind(&model, &target, "Style", InvokeKind::Put, vec![AutoValue::text("Mine")]).unwrap();
    let reopened = reopen_formatting(&host);
    let book2 = obj(invoke(&reopened, &reopened.application(), "ActiveWorkbook", vec![]).unwrap());
    let styles2 = obj(invoke(&reopened, &book2, "Styles", vec![]).unwrap());
    assert_eq!(int(invoke(&reopened, &styles2, "Count", vec![]).unwrap()), before + 1);
    let there = range(&reopened, "B2");
    let applied = obj(invoke(&reopened, &there, "Style", vec![]).unwrap());
    assert_eq!(text(&reopened, &applied, "Name"), "Mine");
    // An unused style added in this session deletes; deleting one a cell uses reverts the cell to Normal (ww21).
    let spare = obj(invoke_kind(&model, &styles, "Add", InvokeKind::Method, vec![AutoValue::text("Spare")]).unwrap());
    invoke_kind(&model, &spare, "Delete", InvokeKind::Method, vec![]).unwrap();
    assert_eq!(int(invoke(&model, &styles, "Count", vec![]).unwrap()), before + 1);
    invoke_kind(&model, &mine, "Delete", InvokeKind::Method, vec![]).unwrap();
    assert_eq!(int(invoke(&model, &styles, "Count", vec![]).unwrap()), before);
    let reverted = obj(invoke(&model, &target, "Style", vec![]).unwrap());
    assert_eq!(text(&model, &reverted, "Name"), "Normal");
}

/// dc-oxh8w.50: `Application.ActiveWindow` and `Workbook.Windows(1)` answer the workbook's window: ActiveSheet, ActiveCell,
/// Selection, Zoom (10 to 400), ScrollRow/ScrollColumn, the Display* flags and FreezePanes with SplitRow/SplitColumn
/// (through the stored frozen panes). Out-of-range values and a second window are 1004.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_50__window_view_members_and_freeze_panes() {
    let (host, model, _) = blank_report();
    let app = model.application();
    let window = obj(invoke(&model, &app, "ActiveWindow", vec![]).unwrap());
    let book = obj(invoke(&model, &app, "ActiveWorkbook", vec![]).unwrap());
    let windows = obj(invoke(&model, &book, "Windows", vec![]).unwrap());
    assert_eq!(int(invoke(&model, &windows, "Count", vec![]).unwrap()), 1);
    let first = obj(invoke_kind(&model, &windows, "Item", InvokeKind::Get, vec![AutoValue::I4(1)]).unwrap());
    assert_eq!(first.key, window.key);
    let r = invoke_kind(&model, &windows, "Item", InvokeKind::Get, vec![AutoValue::I4(2)]);
    assert!(matches!(r, Err(AutoError::Excel { number: 1004, .. })), "{r:?}");
    let put = |member: &str, value: AutoValue| invoke_kind(&model, &window, member, InvokeKind::Put, vec![value]);
    // Zoom
    assert_eq!(int(invoke(&model, &window, "Zoom", vec![]).unwrap()), 100);
    put("Zoom", AutoValue::I4(150)).unwrap();
    assert_eq!(int(invoke(&model, &window, "Zoom", vec![]).unwrap()), 150);
    for bad in [9, 401, 0, -5] {
        let r = put("Zoom", AutoValue::I4(bad));
        assert!(matches!(r, Err(AutoError::Excel { number: 1004, .. })), "{bad}: {r:?}");
    }
    assert_eq!(int(invoke(&model, &window, "Zoom", vec![]).unwrap()), 150);
    // Scroll position and flags
    assert_eq!(int(invoke(&model, &window, "ScrollRow", vec![]).unwrap()), 1);
    put("ScrollRow", AutoValue::I4(20)).unwrap();
    put("ScrollColumn", AutoValue::I4(3)).unwrap();
    assert_eq!(int(invoke(&model, &window, "ScrollRow", vec![]).unwrap()), 20);
    assert_eq!(int(invoke(&model, &window, "ScrollColumn", vec![]).unwrap()), 3);
    for (member, bad) in [("ScrollRow", 0), ("ScrollRow", 1_048_577), ("ScrollColumn", 16_385)] {
        let r = put(member, AutoValue::I4(bad));
        assert!(matches!(r, Err(AutoError::Excel { number: 1004, .. })), "{member} {bad}: {r:?}");
    }
    put("ScrollRow", AutoValue::I4(1)).unwrap();
    put("ScrollColumn", AutoValue::I4(1)).unwrap();
    assert!(matches!(invoke(&model, &window, "DisplayGridlines", vec![]).unwrap(), AutoValue::Bool(true)));
    put("DisplayGridlines", AutoValue::Bool(false)).unwrap();
    assert!(matches!(invoke(&model, &window, "DisplayGridlines", vec![]).unwrap(), AutoValue::Bool(false)));
    assert!(matches!(invoke(&model, &window, "DisplayFormulas", vec![]).unwrap(), AutoValue::Bool(false)));
    // FreezePanes at C3 freezes two rows and two columns.
    assert!(matches!(invoke(&model, &window, "FreezePanes", vec![]).unwrap(), AutoValue::Bool(false)));
    let r = put("FreezePanes", AutoValue::Bool(true));
    assert!(r.is_err(), "freezing at the first cell is refused: {r:?}");
    let c3 = range(&model, "C3");
    invoke_kind(&model, &c3, "Select", InvokeKind::Method, vec![]).unwrap();
    put("FreezePanes", AutoValue::Bool(true)).unwrap();
    assert!(matches!(invoke(&model, &window, "FreezePanes", vec![]).unwrap(), AutoValue::Bool(true)));
    assert_eq!(int(invoke(&model, &window, "SplitRow", vec![]).unwrap()), 2);
    assert_eq!(int(invoke(&model, &window, "SplitColumn", vec![]).unwrap()), 2);
    let active = obj(invoke(&model, &window, "ActiveCell", vec![]).unwrap());
    assert_eq!(text(&model, &active, "Address"), "$C$3");
    let sel = obj(invoke(&model, &window, "Selection", vec![]).unwrap());
    assert_eq!(text(&model, &sel, "Address"), "$C$3");
    let reopened = reopen_formatting(&host);
    let w2 = obj(invoke(&reopened, &reopened.application(), "ActiveWindow", vec![]).unwrap());
    assert!(matches!(invoke(&reopened, &w2, "FreezePanes", vec![]).unwrap(), AutoValue::Bool(true)));
    put("FreezePanes", AutoValue::Bool(false)).unwrap();
    assert!(matches!(invoke(&model, &window, "FreezePanes", vec![]).unwrap(), AutoValue::Bool(false)));
    assert_eq!(int(invoke(&model, &window, "SplitRow", vec![]).unwrap()), 0);
    let sheet = obj(invoke(&model, &window, "ActiveSheet", vec![]).unwrap());
    assert!(!text(&model, &sheet, "Name").is_empty());
}

/// Application.CalculateBeforeSave and Workbook.PrecisionAsDisplayed puts (dc-oxh8w.53): the value
/// reads back, a non-Boolean is a type mismatch (as the iteration puts), a put without a value is 1004, and PrecisionAsDisplayed
/// True is a typed refusal owned by dc-oxh8w.64 while False is accepted.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_53__calculate_before_save_and_precision_as_displayed_puts() {
    let (host, model) = setup();
    let app = model.application();
    let book = obj(invoke(&model, &app, "ActiveWorkbook", vec![]).unwrap());
    let put = |target: &_, name: &str, value: AutoValue| invoke_kind(&model, target, name, InvokeKind::Put, vec![value]);
    for on in [false, true, false] {
        assert!(put(&app, "CalculateBeforeSave", AutoValue::Bool(on)).is_ok());
        assert!(matches!(invoke(&model, &app, "CalculateBeforeSave", vec![]), Ok(AutoValue::Bool(v)) if v == on));
        assert_eq!(host.session.borrow().host().calculate_before_save(), on);
    }
    assert!(matches!(put(&app, "CalculateBeforeSave", AutoValue::text("yes")), Err(AutoError::Status(_))));
    assert_eq!(
        invoke_kind(&model, &app, "CalculateBeforeSave", InvokeKind::Put, vec![]).unwrap_err().number(),
        1004
    );
    assert!(put(&book, "PrecisionAsDisplayed", AutoValue::Bool(false)).is_ok());
    assert!(matches!(invoke(&model, &book, "PrecisionAsDisplayed", vec![]), Ok(AutoValue::Bool(false))));
    let refused = put(&book, "PrecisionAsDisplayed", AutoValue::Bool(true)).unwrap_err();
    assert!(matches!(refused, AutoError::NotModelled { owner: "dc-oxh8w.64", .. }), "{refused:?}");
    assert!(matches!(invoke(&model, &book, "PrecisionAsDisplayed", vec![]), Ok(AutoValue::Bool(false))));
    assert!(matches!(put(&book, "PrecisionAsDisplayed", AutoValue::text("x")), Err(AutoError::Status(_))));
}

/// dc-oxh8w.71: Range.AdvancedFilter xlFilterCopy copies the header and the records that meet the criteria range
/// (the database-function criteria: a row of conditions per alternative, text matches a prefix, operators), to the
/// CopyToRange; Unique drops repeated records; bad Action or a missing CopyToRange is 1004; in-place is a typed refusal.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_71__advanced_filter_copies_matching_records() {
    let (_host, model) = setup();
    let sheets = worksheets(&model);
    let first = obj(invoke_kind(&model, &sheets, "Add", InvokeKind::Method, vec![]).unwrap());
    let sub = |a: &str| obj(invoke(&model, &first, "Range", vec![AutoValue::text(a)]).unwrap());
    let put = |a: &str, v: AutoValue| {
        put_value(&model, &sub(a), v).unwrap();
    };
    let data = [("Name", "Qty"), ("apple", "5"), ("pear", "20"), ("apple", "5"), ("plum", "7"), ("apricot", "30")];
    for (i, (name, qty)) in data.iter().enumerate() {
        let row = i + 1;
        put(&format!("A{row}"), AutoValue::text(name));
        match qty.parse::<f64>() {
            Ok(n) => put(&format!("B{row}"), AutoValue::R8(n)),
            Err(_) => put(&format!("B{row}"), AutoValue::text(qty)),
        }
    }
    // Criteria: Name begins "ap" (any row below the labels is an alternative).
    put("D1", AutoValue::text("Name"));
    put("D2", AutoValue::text("ap"));
    // A second alternative: Qty above 15 on the next row.
    put("E1", AutoValue::text("Qty"));
    put("E3", AutoValue::text(">15"));
    let value = |a: &str| invoke_kind(&model, &sub(a), "Value", InvokeKind::Get, vec![]).unwrap();
    let run = |args: Vec<AutoValue>| invoke_kind(&model, &sub("A1:B6"), "AdvancedFilter", InvokeKind::Method, args);
    let copy_to = AutoValue::Object(sub("G1"));
    // Row 2 of the criteria is Name "ap" AND Qty blank; row 3 is Qty >15 (Name blank): records 1,3,5 (apples, apricot) and 2 (pear 20).
    run(vec![AutoValue::I4(2), AutoValue::Object(sub("D1:E3")), copy_to.clone(), AutoValue::Bool(false)]).unwrap();
    let names: Vec<String> = (1..=6).map(|r| match value(&format!("G{r}")) { AutoValue::Text(t) => t.to_string_lossy(), _ => String::new() }).collect();
    assert_eq!(names, ["Name", "apple", "pear", "apple", "apricot", ""]);
    assert!(matches!(value("H5"), AutoValue::R8(n) if n == 30.0));
    // Unique drops the second "apple 5".
    run(vec![AutoValue::I4(2), AutoValue::Object(sub("D1:E3")), AutoValue::Object(sub("J1")), AutoValue::Bool(true)]).unwrap();
    let names: Vec<String> = (1..=5).map(|r| match value(&format!("J{r}")) { AutoValue::Text(t) => t.to_string_lossy(), _ => String::new() }).collect();
    assert_eq!(names, ["Name", "apple", "pear", "apricot", ""]);
    // No criteria range: every record.
    run(vec![AutoValue::I4(2), AutoValue::Missing, AutoValue::Object(sub("M1"))]).unwrap();
    assert!(matches!(value("M6"), AutoValue::Text(_)));
    let bad = |args: Vec<AutoValue>| matches!(run(args), Err(AutoError::Excel { number: 1004, .. }));
    assert!(bad(vec![AutoValue::I4(3)]));
    assert!(bad(vec![AutoValue::I4(2), AutoValue::Missing]));
    // In place with no criteria keeps every record (ww24 e/: True; the rows are all shown).
    let inplace = run(vec![AutoValue::I4(1), AutoValue::Missing]);
    assert!(matches!(inplace, Ok(AutoValue::Bool(true))), "{inplace:?}");
}

/// dc-oxh8w.43: Range.AutoFilter switches the filter on and off and sets one column's criterion through the
/// host's own AutoFilter command; Worksheet.AutoFilterMode, FilterMode and ShowAllData read and clear it.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_43__range_auto_filter_sets_criteria() {
    let (host, model) = setup_session(
        HostSession::open(
            Arc::new(MemorySource::new(blank_package())),
            &OpenPolicy::default(),
            HostOptions::default(),
        )
        .unwrap(),
    );
    fill(&host, (1, 1), &[&["name", "qty"], &["a", "5"], &["b", "15"], &["c", "25"], &["d", "35"]]);
    let sheet = obj(invoke(&model, &model.application(), "ActiveSheet", vec![]).unwrap());
    let get = |name: &str| invoke(&model, &sheet, name, vec![]).unwrap();
    let hidden = || -> Vec<u32> {
        let session = host.session.borrow();
        let id = session.host().sheets()[0].id();
        session
            .host()
            .model()
            .definitions()
            .auto_filter(id)
            .map(|f| f.filtered_rows.iter().copied().collect())
            .unwrap_or_default()
    };
    let filter = |address: &str, args: Vec<AutoValue>| {
        invoke_kind(&model, &range(&model, address), "AutoFilter", InvokeKind::Method, args)
    };
    let t = |s: &str| AutoValue::text(s);
    assert!(matches!(get("AutoFilterMode"), AutoValue::Bool(false)));
    // No Field: on over the current region of a single cell.
    filter("A1", vec![]).unwrap();
    assert!(matches!(get("AutoFilterMode"), AutoValue::Bool(true)));
    assert!(matches!(get("FilterMode"), AutoValue::Bool(false)));
    // Field 2 > 10: rows 2 hidden.
    filter("A1:B5", vec![AutoValue::I4(2), t(">10")]).unwrap();
    assert_eq!(hidden(), vec![2]);
    assert!(matches!(get("FilterMode"), AutoValue::Bool(true)));
    // Second criterion with xlAnd narrows; xlOr widens.
    filter("A1:B5", vec![AutoValue::I4(2), t(">10"), AutoValue::I4(1), t("<30")]).unwrap();
    assert_eq!(hidden(), vec![2, 5]);
    filter("A1:B5", vec![AutoValue::I4(2), t("<10"), AutoValue::I4(2), t(">30")]).unwrap();
    assert_eq!(hidden(), vec![3, 4]);
    // Top 2 items; a plain number is equality; an array is xlFilterValues.
    filter("A1:B5", vec![AutoValue::I4(2), AutoValue::I4(2), AutoValue::I4(3)]).unwrap();
    assert_eq!(hidden(), vec![2, 3]);
    filter("A1:B5", vec![AutoValue::I4(2), AutoValue::I4(15)]).unwrap();
    assert_eq!(hidden(), vec![2, 4, 5]);
    let values = AutoValue::Array(Box::new(calc_object_model::AutoArray {
        rows: 1,
        columns: 2,
        lower: (1, 1),
        values: vec![t("5"), t("35")],
    }));
    filter("A1:B5", vec![AutoValue::I4(2), values, AutoValue::I4(7)]).unwrap();
    assert_eq!(hidden(), vec![3, 4]);
    // Text criteria on column 1 combine with column 2.
    filter("A1:B5", vec![AutoValue::I4(1), t("<>a")]).unwrap();
    assert_eq!(hidden(), vec![2, 3, 4]);
    // ShowAllData clears the criteria and keeps the filter; a second call is an error.
    invoke_kind(&model, &sheet, "ShowAllData", InvokeKind::Method, vec![]).unwrap();
    assert_eq!(hidden(), Vec::<u32>::new());
    assert!(matches!(get("FilterMode"), AutoValue::Bool(false)));
    assert!(matches!(get("AutoFilterMode"), AutoValue::Bool(true)));
    assert!(matches!(
        invoke_kind(&model, &sheet, "ShowAllData", InvokeKind::Method, vec![]),
        Err(AutoError::Excel { number: 1004, .. })
    ));
    // Argument errors are 1004; colour filters refuse typed.
    assert!(matches!(filter("A1:B5", vec![AutoValue::I4(3), t(">1")]), Err(AutoError::Excel { number: 1004, .. })));
    assert!(matches!(filter("A1:B5", vec![AutoValue::I4(0), t(">1")]), Err(AutoError::Excel { number: 1004, .. })));
    assert!(matches!(
        filter("A1:B5", vec![AutoValue::I4(1), t("x"), AutoValue::I4(20)]),
        Err(AutoError::Excel { number: 1004, .. })
    ));
    assert!(matches!(
        filter("A1:B5", vec![AutoValue::I4(1), t("x"), AutoValue::I4(8)]),
        Err(AutoError::NotModelled { .. })
    ));
    // No Field again switches it off; AutoFilterMode = False does the same, True is an error.
    filter("A1:B5", vec![]).unwrap();
    assert!(matches!(get("AutoFilterMode"), AutoValue::Bool(false)));
    filter("A1:B5", vec![]).unwrap();
    invoke_kind(&model, &sheet, "AutoFilterMode", InvokeKind::Put, vec![AutoValue::Bool(false)]).unwrap();
    assert!(matches!(get("AutoFilterMode"), AutoValue::Bool(false)));
    assert!(matches!(
        invoke_kind(&model, &sheet, "AutoFilterMode", InvokeKind::Put, vec![AutoValue::Bool(true)]),
        Err(AutoError::Excel { number: 1004, .. })
    ));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_43__paste_special_column_widths_and_source_theme() {
    let (host, model) = setup_session(
        HostSession::open(
            Arc::new(MemorySource::new(blank_package())),
            &OpenPolicy::default(),
            HostOptions::default(),
        )
        .unwrap(),
    );
    fill(&host, (1, 1), &[&["1", "2"], &["3", "4"]]);
    let call = |address: &str, name: &str, args: Vec<AutoValue>| {
        invoke_kind(&model, &range(&model, address), name, InvokeKind::Method, args)
    };
    let width = |address: &str| invoke(&model, &range(&model, address), "ColumnWidth", vec![]).unwrap();
    invoke_kind(&model, &range(&model, "A1"), "ColumnWidth", InvokeKind::Put, vec![AutoValue::I4(20)]).unwrap();
    invoke_kind(&model, &range(&model, "B1"), "ColumnWidth", InvokeKind::Put, vec![AutoValue::I4(30)]).unwrap();
    // xlPasteColumnWidths (8) sets the destination columns' widths and writes no cell.
    call("A1:B2", "Copy", vec![]).unwrap();
    call("E5", "PasteSpecial", vec![AutoValue::I4(8)]).unwrap();
    assert!(matches!(width("E1"), AutoValue::R8(n) if n == 20.0));
    assert!(matches!(width("F1"), AutoValue::R8(n) if n == 30.0));
    assert_eq!(cell_text_of(&model, "E5"), "", "no cell content is pasted");
    // Excel ww21: ColumnWidths ignores Add and Transpose, leaving the destination cells alone.
    fill(&host, (1, 8), &[&["7", "8"]]);
    call("H1", "PasteSpecial", vec![AutoValue::I4(8), AutoValue::I4(2), AutoValue::Bool(false), AutoValue::Bool(true)])
        .unwrap();
    assert!(matches!(width("H1"), AutoValue::R8(n) if n == 20.0));
    assert!(matches!(width("I1"), AutoValue::R8(n) if n == 30.0));
    assert_eq!(cell_text_of(&model, "H1"), "7");
    assert_eq!(cell_text_of(&model, "I1"), "8");
    assert!(matches!(call("XFD1", "PasteSpecial", vec![AutoValue::I4(8)]), Err(AutoError::Excel { number: 1004, .. })));
    // xlPasteAllUsingSourceTheme (13) pastes everything within one workbook;
    // xlPasteAllMergingConditionalFormats (14) pastes everything (merging rules).
    call("A10", "PasteSpecial", vec![AutoValue::I4(13)]).unwrap();
    assert_eq!(cell_text_of(&model, "B11"), "4");
    call("A13", "PasteSpecial", vec![AutoValue::I4(14)]).unwrap();
    assert_eq!(cell_text_of(&model, "B14"), "4");
}

/// dc-oxh8w.43: `PasteSpecial` xlPasteValuesAndNumberFormats (12), xlPasteFormulasAndNumberFormats (11) and
/// xlPasteAllExceptBorders (7) splice the source's number format (and, for 7, its font and fill) into the
/// destination's own format; 12 and 11 leave the font and fill alone.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_43__paste_special_number_format_mixes() {
    let (host, model) = setup_session(
        HostSession::open(
            Arc::new(MemorySource::new(blank_package())),
            &OpenPolicy::default(),
            HostOptions::default(),
        )
        .unwrap(),
    );
    fill(&host, (1, 1), &[&["1.5"]]);
    let put = |address: &str, member: &str, value: AutoValue| {
        invoke_kind(&model, &range(&model, address), member, InvokeKind::Put, vec![value]).unwrap()
    };
    put("A1", "NumberFormat", AutoValue::text("0.00"));
    put_font(&model, &font(&model, "A1"), "Bold", AutoValue::Bool(true)).unwrap();
    invoke_kind(&model, &interior(&model, "A1"), "Color", InvokeKind::Put, vec![AutoValue::I4(255)]).unwrap();
    put("C1", "NumberFormat", AutoValue::text("0%"));
    let call = |address: &str, name: &str, args: Vec<AutoValue>| {
        invoke_kind(&model, &range(&model, address), name, InvokeKind::Method, args)
    };
    let bold = |address: &str| invoke(&model, &font(&model, address), "Bold", vec![]).unwrap();
    let shown = |address: &str| text(&model, &range(&model, address), "Text");
    call("A1", "Copy", vec![]).unwrap();
    for paste in [12, 11] {
        let at = if paste == 12 { "C1" } else { "D1" };
        call(at, "PasteSpecial", vec![AutoValue::I4(paste)]).unwrap();
        assert_eq!(shown(at), "1.50", "paste {paste} carries the number format");
        assert!(matches!(bold(at), AutoValue::Bool(false)), "paste {paste} leaves the font");
        assert_eq!(i4(&model, &interior(&model, at), "Color"), 16_777_215, "paste {paste} leaves the fill");
    }
    call("E1", "PasteSpecial", vec![AutoValue::I4(7)]).unwrap();
    assert_eq!(shown("E1"), "1.50");
    assert!(matches!(bold("E1"), AutoValue::Bool(true)));
    assert_eq!(i4(&model, &interior(&model, "E1"), "Color"), 255);
}

/// dc-oxh8w.43 (ww23 e/observations.jsonl VBA.e33-e36, Excel 16.0.20625.20108): PasteSpecial All with Operation Add keeps
/// the source fill and adds the values; All with SkipBlanks leaves a blank source cell's format out; Transpose with
/// SkipBlanks transposes; Divide by a zero source is #DIV/0! while the other cell divides.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_43__paste_special_all_with_operation_and_skip_blanks() {
    let new_session = || {
        setup_session(HostSession::open(Arc::new(MemorySource::new(blank_package())), &OpenPolicy::default(), HostOptions::default()).unwrap())
    };
    let call = |model: &WorkbookAutomation, address: &str, name: &str, args: Vec<AutoValue>| {
        invoke_kind(model, &range(model, address), name, InvokeKind::Method, args)
    };
    let put = |model: &WorkbookAutomation, address: &str, member: &str, value: AutoValue| {
        invoke_kind(model, &range(model, address), member, InvokeKind::Put, vec![value]).unwrap()
    };
    let shown = |model: &WorkbookAutomation, address: &str| text(model, &range(model, address), "Text");
    let all = AutoValue::I4(-4104);
    // e34: all+add
    let (_h, model) = new_session();
    put(&model, "A1", "Value", AutoValue::R8(3.0));
    put(&model, "B1", "Value", AutoValue::R8(4.0));
    invoke_kind(&model, &interior(&model, "A1"), "Color", InvokeKind::Put, vec![AutoValue::I4(255)]).unwrap();
    put(&model, "D1:E1", "Value", AutoValue::R8(10.0));
    call(&model, "A1:B1", "Copy", vec![]).unwrap();
    call(&model, "D1:E1", "PasteSpecial", vec![all.clone(), AutoValue::I4(2)]).unwrap();
    assert_eq!((shown(&model, "D1"), shown(&model, "E1")), ("13".into(), "14".into()));
    assert_eq!(i4(&model, &interior(&model, "D1"), "Color"), 255, "all+add keeps the source fill");
    // e35: skipblanks all
    let (_h, model) = new_session();
    put(&model, "A1", "Value", AutoValue::R8(3.0));
    invoke_kind(&model, &interior(&model, "B1"), "Color", InvokeKind::Put, vec![AutoValue::I4(255)]).unwrap();
    call(&model, "A1:B1", "Copy", vec![]).unwrap();
    call(&model, "D1:E1", "PasteSpecial", vec![all.clone(), AutoValue::Missing, AutoValue::Bool(true)]).unwrap();
    assert_eq!(i4(&model, &interior(&model, "E1"), "Color"), 16_777_215, "skip blanks leaves the blank source's fill out");
    // e36: transpose with skipblanks
    let (_h, model) = new_session();
    for (address, value) in [("A1", 1.0), ("B1", 2.0), ("A2", 3.0), ("B2", 4.0)] {
        put(&model, address, "Value", AutoValue::R8(value));
    }
    call(&model, "A1:B2", "Copy", vec![]).unwrap();
    call(&model, "D1", "PasteSpecial", vec![all.clone(), AutoValue::Missing, AutoValue::Bool(true), AutoValue::Bool(true)]).unwrap();
    let got: String = ["D1", "E1", "D2", "E2"].iter().map(|a| shown(&model, a)).collect();
    assert_eq!(got, "1324");
    // e33: divide by zero
    let (_h, model) = new_session();
    put(&model, "A1:A2", "Value", AutoValue::R8(1.0));
    put(&model, "D1", "Value", AutoValue::R8(0.0));
    put(&model, "D2", "Value", AutoValue::R8(4.0));
    call(&model, "D1:D2", "Copy", vec![]).unwrap();
    call(&model, "A1:A2", "PasteSpecial", vec![all, AutoValue::I4(5)]).unwrap();
    assert_eq!((shown(&model, "A1"), shown(&model, "A2")), ("#DIV/0!".into(), "0.25".into()));
}

/// dc-oxh8w.43: `PasteSpecial` xlPasteValidation (6) pastes only the validation rules and xlPasteComments (-4144) only the
/// notes: the cell contents stay, Comments leaves the rules alone, and Validation replaces the destination's rule.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_43__paste_special_validation_and_comments() {
    let (host, model) = setup_session(
        HostSession::open(
            Arc::new(MemorySource::new(include_bytes!("fixtures/dc_6yw/data-validation.xlsx").to_vec())),
            &OpenPolicy::default(),
            HostOptions::default(),
        )
        .unwrap(),
    );
    let ruled = |row: u32, column: u32| host.session.borrow().host().validation(at(&host, row, column)).is_some();
    // The first cell of column A..H with a rule is the source.
    let (row, column) = (1..=40u32)
        .flat_map(|r| (1..=8u32).map(move |c| (r, c)))
        .find(|&(r, c)| ruled(r, c))
        .expect("the fixture has a rule");
    let sheet = host.session.borrow().host().sheets()[0].name().text().to_string_lossy();
    let source = format!("{}{row}", (b'A' + column as u8 - 1) as char);
    let call = |address: &str, name: &str, args: Vec<AutoValue>| {
        invoke_kind(&model, &range(&model, &format!("'{sheet}'!{address}")), name, InvokeKind::Method, args)
    };
    let cell_text = |address: &str| cell_text_of(&model, &format!("'{sheet}'!{address}"));
    let (dest_row, dest_column) = (60u32, 3u32);
    assert!(!ruled(dest_row, dest_column));
    call(&source, "Copy", vec![]).unwrap();
    // Comments pastes no rule and no content.
    call("C60", "PasteSpecial", vec![AutoValue::I4(-4144)]).unwrap();
    assert!(!ruled(dest_row, dest_column));
    assert_eq!(cell_text("C60"), "");
    // Validation pastes the rule and still no content.
    call("C60", "PasteSpecial", vec![AutoValue::I4(6)]).unwrap();
    assert!(ruled(dest_row, dest_column));
    assert_eq!(cell_text("C60"), "");
    assert_eq!(
        host.session.borrow().host().validation(at(&host, dest_row, dest_column)).map(|r| r.kind),
        host.session.borrow().host().validation(at(&host, row, column)).map(|r| r.kind)
    );
}

/// dc-2rjye (rw12 finding 17): the paste-comments test above runs on a workbook with no notes, so it proves nothing about
/// them. On the structural-base fixture, sheet S has a note on A2: xlPasteComments copies the note and leaves the
/// destination's contents, xlPasteValidation copies no note.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_2rjye__paste_special_comments_copies_a_real_note_and_validation_does_not() {
    let (host, model) = setup_session(
        HostSession::open(
            Arc::new(MemorySource::new(include_bytes!("fixtures/d2/structural-base.xlsx").to_vec())),
            &OpenPolicy::default(),
            HostOptions::default(),
        )
        .unwrap(),
    );
    let sheet_id = host
        .session
        .borrow()
        .host()
        .sheets()
        .iter()
        .find(|s| s.name().text().to_string_lossy() == "S")
        .expect("sheet S")
        .id();
    let notes_at = |address: &str| {
        let session = host.session.borrow();
        let (col, row) = address.split_at(1);
        let key = CellKey::new(sheet_id, row.parse().unwrap(), u32::from(col.as_bytes()[0] - b'A') + 1).unwrap();
        session.host().model().definitions().review_notes(sheet_id).iter().filter(|n| n.at == key).count()
    };
    assert_eq!(notes_at("A2"), 1, "the fixture has a note on S!A2");
    assert_eq!(notes_at("L40"), 0);
    let call = |address: &str, name: &str, args: Vec<AutoValue>| {
        invoke_kind(&model, &range(&model, &format!("S!{address}")), name, InvokeKind::Method, args)
    };
    call("A2", "Copy", vec![]).unwrap();
    call("L40", "PasteSpecial", vec![AutoValue::I4(6)]).unwrap();
    assert_eq!(notes_at("L40"), 0, "xlPasteValidation copies no note");
    call("L40", "PasteSpecial", vec![AutoValue::I4(-4144)]).unwrap();
    assert_eq!(notes_at("L40"), 1, "xlPasteComments copies the note");
    assert_eq!(cell_text_of(&model, "S!L40"), "", "and not the contents");
    assert_eq!(notes_at("A2"), 1, "the source keeps its note");
}

/// dc-oxh8w.67: `Series.Border` (Color, ColorIndex, LineStyle, Weight) and `ColorIndex` of the chart-area borders: stated,
/// saved to the series' own `a:ln` (the fill stays) and read back after a save; bad values are 1004.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_67__series_border_and_color_index() {
    let (host, model, chart, series) = labelled_chart();
    let border = obj(invoke(&model, &series, "Border", vec![]).unwrap());
    let put = |target: &ObjectRef, member: &str, value: i32| {
        invoke_kind(&model, target, member, InvokeKind::Put, vec![AutoValue::I4(value)])
    };
    put(&border, "Color", 255).unwrap();
    put(&border, "Weight", -4138).unwrap();
    put(&border, "LineStyle", -4115).unwrap();
    assert_eq!(int(invoke(&model, &border, "Color", vec![]).unwrap()), 255);
    assert_eq!(int(invoke(&model, &border, "Weight", vec![]).unwrap()), -4138);
    assert_eq!(int(invoke(&model, &border, "LineStyle", vec![]).unwrap()), -4115);
    // The palette: index 3 is red, and the colour reads back as that index.
    put(&border, "ColorIndex", 5).unwrap();
    assert_eq!(int(invoke(&model, &border, "Color", vec![]).unwrap()), 0xFF_00_00);
    assert_eq!(int(invoke(&model, &border, "ColorIndex", vec![]).unwrap()), 5);
    for bad in [0, 57, -1] {
        let r = put(&border, "ColorIndex", bad);
        assert!(matches!(r, Err(AutoError::Excel { number: 1004, .. })), "{bad}: {r:?}");
    }
    let xml = saved_chart_xml(&host);
    let ser = &xml[xml.find("<c:ser>").expect("ser")..xml.find("</c:ser>").unwrap()];
    let ln = &ser[ser.find("<a:ln").expect("series a:ln")..];
    assert!(ln.contains("w=\"19050\"") && ln.contains("<a:srgbClr val=\"0000FF\"/>") && ln.contains("<a:prstDash val=\"dash\"/>"), "{xml}");
    put(&border, "LineStyle", -4142).unwrap();
    assert_eq!(int(invoke(&model, &border, "LineStyle", vec![]).unwrap()), -4142);
    assert!(saved_chart_xml(&host).contains("<a:noFill/>"));
    // ColorIndex on a chart-area border.
    let area = obj(invoke(&model, &obj(invoke(&model, &chart, "ChartArea", vec![]).unwrap()), "Border", vec![]).unwrap());
    put(&area, "ColorIndex", 3).unwrap();
    assert_eq!(int(invoke(&model, &area, "Color", vec![]).unwrap()), 255);
    assert_eq!(int(invoke(&model, &area, "ColorIndex", vec![]).unwrap()), 3);
}

/// dc-oxh8w.67: the outline of a series of a chart the file holds is written into its own `c:spPr` and read back after a save.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_67__a_series_outline_of_a_file_held_chart_survives_a_save() {
    let open = |bytes: Vec<u8>| {
        setup_session(HostSession::open(Arc::new(MemorySource::new(bytes)), &OpenPolicy::default(), HostOptions::default()).unwrap())
    };
    let border_of = |model: &WorkbookAutomation| {
        let book = obj(invoke(model, &model.application(), "ActiveWorkbook", vec![]).unwrap());
        let charts = obj(invoke(model, &book, "Charts", vec![]).unwrap());
        let chart = obj(invoke_kind(model, &charts, "Item", InvokeKind::Get, vec![AutoValue::I4(1)]).unwrap());
        let all = obj(invoke_kind(model, &chart, "SeriesCollection", InvokeKind::Method, vec![]).unwrap());
        let series = obj(invoke_kind(model, &all, "Item", InvokeKind::Method, vec![AutoValue::I4(1)]).unwrap());
        obj(invoke(model, &series, "Border", vec![]).unwrap())
    };
    let (host, model) = open(include_bytes!("../../calc-doc/tests/fixtures/preserved_sheets/chart-sheet-and-embedded.xlsx").to_vec());
    let border = border_of(&model);
    invoke_kind(&model, &border, "Color", InvokeKind::Put, vec![AutoValue::I4(255)]).unwrap();
    invoke_kind(&model, &border, "Weight", InvokeKind::Put, vec![AutoValue::I4(4)]).unwrap();
    let mut bytes = Vec::new();
    host.session.borrow_mut().save(&SavePolicy::default(), &mut bytes).expect("the save is not refused");
    let (_host2, model2) = open(bytes);
    let border2 = border_of(&model2);
    assert_eq!(int(invoke(&model2, &border2, "Color", vec![]).unwrap()), 255);
    assert_eq!(int(invoke(&model2, &border2, "Weight", vec![]).unwrap()), 4);
}

/// dc-oxh8w.68: `Worksheet.Protect` / `Unprotect` / `ProtectContents` and the edit enforcement a macro meets. A locked cell
/// refuses value, clear and format writes with 1004 while the sheet is protected; an unlocked cell (`Range.Locked = False`)
/// stays writable; a wrong password is 1004 and leaves the sheet protected; `UserInterfaceOnly` lets macros edit.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_68__protect_unprotect_and_locked_cell_enforcement() {
    let (_host, model, _) = blank_report();
    let sheets = worksheets(&model);
    let sheet = obj(invoke(&model, &sheets, "Item", vec![AutoValue::I4(1)]).unwrap());
    let flag = |name: &str| matches!(invoke(&model, &sheet, name, vec![]).unwrap(), AutoValue::Bool(true));
    let method = |target: &ObjectRef, name: &str, args: Vec<AutoValue>| invoke_kind(&model, target, name, InvokeKind::Method, args);
    let put = |target: &ObjectRef, name: &str, args: Vec<AutoValue>| invoke_kind(&model, target, name, InvokeKind::Put, args);
    let protected_error = |r: Reply| matches!(&r, Err(AutoError::Excel { number: 1004, description }) if description.contains("protected sheet"));
    assert!(!flag("ProtectContents") && !flag("ProtectDrawingObjects") && !flag("ProtectScenarios"));
    // Locked is the default; a cell can be unlocked while the sheet is unprotected.
    assert!(matches!(invoke(&model, &range(&model, "A1"), "Locked", vec![]).unwrap(), AutoValue::Bool(true)));
    put(&range(&model, "B2"), "Locked", vec![AutoValue::Bool(false)]).unwrap();
    assert!(matches!(invoke(&model, &range(&model, "B2"), "Locked", vec![]).unwrap(), AutoValue::Bool(false)));
    put(&range(&model, "A1"), "Value", vec![AutoValue::R8(1.0)]).unwrap();
    method(&sheet, "Protect", vec![AutoValue::text("pw")]).unwrap();
    assert!(flag("ProtectContents") && flag("ProtectDrawingObjects") && flag("ProtectScenarios") && !flag("ProtectionMode"));
    assert!(protected_error(put(&range(&model, "A1"), "Value", vec![AutoValue::R8(2.0)])));
    assert!(protected_error(method(&range(&model, "A1"), "ClearContents", vec![])));
    assert!(protected_error(put(&range(&model, "A1"), "Formula", vec![AutoValue::text("=1+1")])));
    assert!(protected_error(put(&range(&model, "A1:B2"), "Value", vec![AutoValue::R8(3.0)])));
    assert!(protected_error(put(&range(&model, "A1"), "Locked", vec![AutoValue::Bool(false)])));
    assert!(matches!(invoke(&model, &range(&model, "A1"), "Value", vec![]).unwrap(), AutoValue::R8(n) if n == 1.0));
    put(&range(&model, "B2"), "Value", vec![AutoValue::R8(5.0)]).unwrap();
    // ww21 protection/observations.jsonl: wrong text raises 1004, omission opens a password dialog.
    let wrong = method(&sheet, "Unprotect", vec![AutoValue::text("wrong")]);
    assert!(matches!(&wrong, Err(AutoError::Excel { number: 1004, description }) if description.contains("password you supplied is not correct")), "{wrong:?}");
    let omitted = method(&sheet, "Unprotect", vec![]);
    assert!(matches!(&omitted, Err(AutoError::NotModelled { owner: "dc-9b7zd", .. })), "{omitted:?}");
    assert!(flag("ProtectContents"));
    method(&sheet, "Unprotect", vec![AutoValue::text("pw")]).unwrap();
    assert!(!flag("ProtectContents"));
    put(&range(&model, "A1"), "Value", vec![AutoValue::R8(2.0)]).unwrap();
    // No password: Unprotect needs none. UserInterfaceOnly protects the interface only: macros still edit.
    method(&sheet, "Protect", vec![]).unwrap();
    assert!(protected_error(put(&range(&model, "A1"), "Value", vec![AutoValue::R8(4.0)])));
    method(&sheet, "Unprotect", vec![]).unwrap();
    let args = vec![AutoValue::Missing, AutoValue::Missing, AutoValue::Missing, AutoValue::Missing, AutoValue::Bool(true)];
    method(&sheet, "Protect", args).unwrap();
    assert!(flag("ProtectContents") && flag("ProtectionMode"));
    put(&range(&model, "A1"), "Value", vec![AutoValue::R8(6.0)]).unwrap();
    // Contents:=False leaves the cells editable and ProtectContents False.
    method(&sheet, "Unprotect", vec![]).unwrap();
    method(&sheet, "Protect", vec![AutoValue::Missing, AutoValue::Bool(false), AutoValue::Bool(false)]).unwrap();
    assert!(!flag("ProtectContents") && !flag("ProtectDrawingObjects") && flag("ProtectScenarios"));
    put(&range(&model, "A1"), "Value", vec![AutoValue::R8(7.0)]).unwrap();
    // Too many arguments are an argument-count error.
    let r = method(&sheet, "Protect", vec![AutoValue::Missing; 17]);
    assert!(matches!(r, Err(ref e) if !matches!(e, AutoError::NotModelled { .. })), "{r:?}");
}

/// ww21 protection/observations.jsonl, Excel 16.0.20625.20108: Protect accepts the integer 123
/// as a password and Unprotect clears the protection when supplied the corresponding text.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_9b7zd__protect_accepts_an_integer_password() {
    let (_host, model, _) = blank_report();
    let sheet = obj(invoke(&model, &model.application(), "ActiveSheet", vec![]).unwrap());
    let protected = || invoke(&model, &sheet, "ProtectContents", vec![]).unwrap();
    for password in [AutoValue::I2(123), AutoValue::I4(123)] {
        invoke_kind(&model, &sheet, "Protect", InvokeKind::Method, vec![password]).unwrap();
        assert!(matches!(protected(), AutoValue::Bool(true)));
        let wrong = invoke_kind(&model, &sheet, "Unprotect", InvokeKind::Method, vec![AutoValue::text("124")]);
        assert!(matches!(wrong, Err(AutoError::Excel { number: 1004, .. })), "{wrong:?}");
        assert!(matches!(protected(), AutoValue::Bool(true)));
        invoke_kind(&model, &sheet, "Unprotect", InvokeKind::Method, vec![AutoValue::text("123")]).unwrap();
        assert!(matches!(protected(), AutoValue::Bool(false)));
    }
}

/// ww24 g/case4-5 (Excel 16.0.20625.20108): a numeric Password is the text Excel shows for it (1.5 needs "1.5", 123.0 needs
/// "123"); Unprotect given a number is 1004 whatever it equals, and the sheet stays protected.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_9b7zd__protect_floating_point_password_and_numeric_unprotect() {
    let (_host, model, _) = blank_report();
    let sheet = obj(invoke(&model, &model.application(), "ActiveSheet", vec![]).unwrap());
    let protected = || matches!(invoke(&model, &sheet, "ProtectContents", vec![]).unwrap(), AutoValue::Bool(true));
    let call = |name: &str, args: Vec<AutoValue>| invoke_kind(&model, &sheet, name, InvokeKind::Method, args);
    for (password, text, number) in [(1.5, "1.5", AutoValue::R8(1.5)), (123.0, "123", AutoValue::R8(123.0))] {
        call("Protect", vec![AutoValue::R8(password)]).unwrap();
        assert!(protected());
        let by_number = call("Unprotect", vec![number]);
        assert!(matches!(by_number, Err(AutoError::Excel { number: 1004, .. })), "{by_number:?}");
        let by_int = call("Unprotect", vec![AutoValue::I4(123)]);
        assert!(matches!(by_int, Err(AutoError::Excel { number: 1004, .. })), "{by_int:?}");
        assert!(protected());
        call("Unprotect", vec![AutoValue::text(text)]).unwrap();
        assert!(!protected());
    }
}

/// ww24 g/case10 (Excel 16.0.20625.20108): a macro's Protect saves as `sheetProtection`: passwordless `sheet objects scenarios`,
/// with a password the SHA-512 hash (spinCount 100000), Contents only `sheet` alone, Drawing and Scenarios only `objects
/// scenarios` without `sheet`. The saved password hash unprotects after reopen; UserInterfaceOnly is not saved (ww22: False
/// after reopen); an Unprotect removes the element.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_9b7zd__macro_protect_saves_as_sheet_protection() {
    let (host, model, _) = blank_report();
    let sheet = obj(invoke(&model, &model.application(), "ActiveSheet", vec![]).unwrap());
    let call = |name: &str, args: Vec<AutoValue>| invoke_kind(&model, &sheet, name, InvokeKind::Method, args).unwrap();
    let xml = || saved_part(&mut host.session.borrow_mut(), "xl/worksheets/sheet");
    let element = |xml: &str| xml.find("<sheetProtection").map(|at| xml[at..xml[at..].find("/>").unwrap() + at + 2].to_owned());
    assert_eq!(element(&xml()), None);
    call("Protect", vec![]);
    assert_eq!(element(&xml()).as_deref(), Some("<sheetProtection sheet=\"1\" objects=\"1\" scenarios=\"1\"/>"));
    call("Unprotect", vec![]);
    assert_eq!(element(&xml()), None);
    call("Protect", vec![AutoValue::Missing, AutoValue::Bool(false), AutoValue::Bool(true), AutoValue::Bool(false)]);
    assert_eq!(element(&xml()).as_deref(), Some("<sheetProtection sheet=\"1\"/>"));
    call("Unprotect", vec![]);
    call("Protect", vec![AutoValue::Missing, AutoValue::Bool(true), AutoValue::Bool(false), AutoValue::Bool(true)]);
    assert_eq!(element(&xml()).as_deref(), Some("<sheetProtection objects=\"1\" scenarios=\"1\"/>"));
    call("Unprotect", vec![]);

    call("Protect", vec![AutoValue::text("abc"), AutoValue::Missing, AutoValue::Missing, AutoValue::Missing, AutoValue::Bool(true)]);
    let saved = element(&xml()).expect("a sheetProtection element");
    assert!(saved.starts_with("<sheetProtection algorithmName=\"SHA-512\" hashValue=\""), "{saved}");
    assert!(saved.contains("spinCount=\"100000\" sheet=\"1\" objects=\"1\" scenarios=\"1\"/>"), "{saved}");
    let reopened = reopen_formatting(&host);
    let sheet2 = obj(invoke(&reopened, &reopened.application(), "ActiveSheet", vec![]).unwrap());
    let flag = |name: &str| matches!(invoke(&reopened, &sheet2, name, vec![]).unwrap(), AutoValue::Bool(true));
    assert!(flag("ProtectContents") && flag("ProtectDrawingObjects") && flag("ProtectScenarios"));
    assert!(!flag("ProtectionMode"), "UserInterfaceOnly survived the reopen");
    let unprotect = |text: &str| invoke_kind(&reopened, &sheet2, "Unprotect", InvokeKind::Method, vec![AutoValue::text(text)]);
    assert!(matches!(unprotect("nope"), Err(AutoError::Excel { number: 1004, .. })));
    unprotect("abc").unwrap();
    assert!(!flag("ProtectContents"));
}

/// dc-p3rg9: Window view state (Zoom, ScrollRow/ScrollColumn, DisplayGridlines/Headings/Formulas/Zeros/Outline)
/// persists through save and reopen (ww21 window/sheet-xml.json: zoomScale=125, topLeftCell=D15, showGridLines=0).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_p3rg9__window_view_state_saves_and_reopens() {
    let (host, model, _) = blank_report();
    let window = obj(invoke(&model, &model.application(), "ActiveWindow", vec![]).unwrap());
    let put = |m: &WorkbookAutomation, w: &ObjectRef, name: &str, value: AutoValue| {
        invoke_kind(m, w, name, InvokeKind::Put, vec![value]).unwrap();
    };
    put(&model, &window, "Zoom", AutoValue::I4(125));
    put(&model, &window, "ScrollRow", AutoValue::I4(15));
    put(&model, &window, "ScrollColumn", AutoValue::I4(4));
    put(&model, &window, "DisplayGridlines", AutoValue::Bool(false));
    put(&model, &window, "DisplayHeadings", AutoValue::Bool(false));
    put(&model, &window, "DisplayFormulas", AutoValue::Bool(true));
    put(&model, &window, "DisplayZeros", AutoValue::Bool(false));
    put(&model, &window, "DisplayOutline", AutoValue::Bool(false));

    let reopened = reopen_formatting(&host);
    let w2 = obj(invoke(&reopened, &reopened.application(), "ActiveWindow", vec![]).unwrap());
    assert_eq!(int(invoke(&reopened, &w2, "Zoom", vec![]).unwrap()), 125);
    assert_eq!(int(invoke(&reopened, &w2, "ScrollRow", vec![]).unwrap()), 15);
    assert_eq!(int(invoke(&reopened, &w2, "ScrollColumn", vec![]).unwrap()), 4);
    for (member, expected) in [
        ("DisplayGridlines", false),
        ("DisplayHeadings", false),
        ("DisplayFormulas", true),
        ("DisplayZeros", false),
        ("DisplayOutline", false),
    ] {
        let got = invoke(&reopened, &w2, member, vec![]).unwrap();
        assert!(matches!(got, AutoValue::Bool(b) if b == expected), "{member}: {got:?}");
    }
}

/// dc-p3rg9: Window.Caption numeric/empty coercion (ww21: 12 -> "12", "" and Empty -> ""), WindowState (-4143
/// xlNormal default, -4137/-4140 valid, invalid -> 1004), Application.Windows collection, and Window.Split.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_p3rg9__window_caption_coercion_state_split_and_app_windows() {
    let (_host, model, _) = blank_report();
    let app = model.application();
    let window = obj(invoke(&model, &app, "ActiveWindow", vec![]).unwrap());
    let put = |name: &str, value: AutoValue| invoke_kind(&model, &window, name, InvokeKind::Put, vec![value]);

    // Caption numeric and empty coercion (ww21 followup/ and vba-boundaries/)
    put("Caption", AutoValue::I4(12)).unwrap();
    assert_eq!(text(&model, &window, "Caption"), "12");
    put("Caption", AutoValue::R8(3.5)).unwrap();
    assert_eq!(text(&model, &window, "Caption"), "3.5");
    put("Caption", AutoValue::text("")).unwrap();
    assert_eq!(text(&model, &window, "Caption"), "");
    put("Caption", AutoValue::text("CustomTitle")).unwrap();
    assert_eq!(text(&model, &window, "Caption"), "CustomTitle");
    put("Caption", AutoValue::Empty).unwrap();
    assert_eq!(text(&model, &window, "Caption"), "");

    // WindowState defaults to xlNormal (-4143)
    assert_eq!(int(invoke(&model, &window, "WindowState", vec![]).unwrap()), -4143);
    put("WindowState", AutoValue::I4(-4137)).unwrap();
    assert_eq!(int(invoke(&model, &window, "WindowState", vec![]).unwrap()), -4137);
    put("WindowState", AutoValue::I4(-4140)).unwrap();
    assert_eq!(int(invoke(&model, &window, "WindowState", vec![]).unwrap()), -4140);
    let bad_state = put("WindowState", AutoValue::I4(99));
    assert!(matches!(bad_state, Err(AutoError::Excel { number: 1004, .. })), "{bad_state:?}");

    // Application.Windows collection
    put("Caption", AutoValue::text("ReportWin")).unwrap();
    let app_windows = obj(invoke(&model, &app, "Windows", vec![]).unwrap());
    assert_eq!(int(invoke(&model, &app_windows, "Count", vec![]).unwrap()), 1);
    let by_index = obj(invoke_kind(&model, &app_windows, "Item", InvokeKind::Get, vec![AutoValue::I4(1)]).unwrap());
    assert_eq!(by_index.key, window.key);
    let by_name = obj(invoke_kind(&model, &app_windows, "Item", InvokeKind::Get, vec![AutoValue::text("reportwin")]).unwrap());
    assert_eq!(by_name.key, window.key);
    for bad in [AutoValue::I4(2), AutoValue::text("nope")] {
        let r = invoke_kind(&model, &app_windows, "Item", InvokeKind::Get, vec![bad.clone()]);
        assert!(matches!(r, Err(AutoError::Excel { number: 1004, .. })), "{bad:?}: {r:?}");
    }

    // Window.Split get/put
    assert!(matches!(invoke(&model, &window, "Split", vec![]).unwrap(), AutoValue::Bool(false)));
    let c3 = range(&model, "C3");
    invoke_kind(&model, &c3, "Select", InvokeKind::Method, vec![]).unwrap();
    put("FreezePanes", AutoValue::Bool(true)).unwrap();
    assert!(matches!(invoke(&model, &window, "Split", vec![]).unwrap(), AutoValue::Bool(true)));
    put("Split", AutoValue::Bool(false)).unwrap();
    assert!(matches!(invoke(&model, &window, "Split", vec![]).unwrap(), AutoValue::Bool(false)));
    assert!(matches!(invoke(&model, &window, "FreezePanes", vec![]).unwrap(), AutoValue::Bool(false)));
    assert_eq!(int(invoke(&model, &window, "SplitRow", vec![]).unwrap()), 0);
    let split_true = put("Split", AutoValue::Bool(true));
    assert!(matches!(split_true, Err(AutoError::NotModelled { owner: "dc-p3rg9", .. })), "{split_true:?}");
}

/// dc-qrnem (rw12 Finding 13): FormatConditions.Add(5|8|12) stays a typed refusal (Excel creates those
/// via AddTop10/AddAboveAverage/AddUniqueValues, owned by dc-oawf4/dc-jupjv), and ConditionalChange::Format
/// checks dxf sharing across ALL sheets in the workbook before mutating in place.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_qrnem_format_conditions_add_5_8_12_refuse_and_cross_sheet_dxf_copy_on_write() {
    let (host, model, _) = blank_report();
    let target = range(&model, "B2:B6");
    let conditions = obj(invoke(&model, &target, "FormatConditions", vec![]).unwrap());
    for kind in [5, 8, 12] {
        let r = invoke_kind(&model, &conditions, "Add", InvokeKind::Method, vec![AutoValue::I4(kind)]);
        assert!(
            matches!(&r, Err(AutoError::NotModelled { owner, .. }) if *owner == "dc-jupjv"),
            "Add({kind}) must refuse citing dc-jupjv: {r:?}"
        );
    }

    // Add a second sheet and create one red conditional rule on each sheet so both share the same new dxf.
    {
        let mut session = host.session.borrow_mut();
        assert!(
            session
                .dispatch(SkinIntent::Command(WorkbookCommand::Sheet {
                    sheet: None,
                    change: calc_host::SheetChange::Add { name: "Sheet2".into() },
                }))
                .accepted()
        );
        let sheets: Vec<_> = session.host().sheets().iter().map(|s| s.id()).collect();
        for &sheet in &sheets[..2] {
            let revision = session.host().revision().get();
            assert!(
                session
                    .dispatch(SkinIntent::Command(WorkbookCommand::Conditional {
                        sheet,
                        revision,
                        index: None,
                        change: calc_host::ConditionalChange::Condition {
                            kind: "cellIs".into(),
                            applies: "A1:A5".into(),
                            color: "#FF0000".into(),
                            formulas: vec!["5".into()],
                            operator: "greaterThan".into(),
                            rank: 10,
                            percent: false,
                            stop: false,
                        },
                    }))
                    .accepted()
            );
        }
    }
    model.context_changed(&host.session.borrow());
    let book = obj(invoke(&model, &model.application(), "ActiveWorkbook", vec![]).unwrap());
    let sheets = obj(invoke(&model, &book, "Worksheets", vec![]).unwrap());
    let s1 = obj(invoke_kind(&model, &sheets, "Item", InvokeKind::Get, vec![AutoValue::I4(1)]).unwrap());
    let s2 = obj(invoke_kind(&model, &sheets, "Item", InvokeKind::Get, vec![AutoValue::I4(2)]).unwrap());
    let r1 = obj(invoke(&model, &s1, "Range", vec![AutoValue::text("A1:A5")]).unwrap());
    let r2 = obj(invoke(&model, &s2, "Range", vec![AutoValue::text("A1:A5")]).unwrap());
    let fc1 = obj(invoke_kind(&model, &obj(invoke(&model, &r1, "FormatConditions", vec![]).unwrap()), "Item", InvokeKind::Method, vec![AutoValue::I4(1)]).unwrap());
    let fc2 = obj(invoke_kind(&model, &obj(invoke(&model, &r2, "FormatConditions", vec![]).unwrap()), "Item", InvokeKind::Method, vec![AutoValue::I4(1)]).unwrap());
    let int1 = obj(invoke(&model, &fc1, "Interior", vec![]).unwrap());
    let int2 = obj(invoke(&model, &fc2, "Interior", vec![]).unwrap());
    assert_eq!(int(invoke(&model, &int1, "Color", vec![]).unwrap()), 0x0000FF);
    assert_eq!(int(invoke(&model, &int2, "Color", vec![]).unwrap()), 0x0000FF);
    // Change Sheet1's rule to green: Sheet2's rule MUST stay red!
    invoke_kind(&model, &int1, "Color", InvokeKind::Put, vec![AutoValue::I4(0x00FF00)]).unwrap();
    assert_eq!(int(invoke(&model, &int1, "Color", vec![]).unwrap()), 0x00FF00);
    assert_eq!(
        int(invoke(&model, &int2, "Color", vec![]).unwrap()),
        0x0000FF,
        "editing Sheet1 rule format must not mutate Sheet2 rule sharing the same dxf"
    );
}

/// dc-qrnem (rw12 Finding 21): Style.Delete refuses non-Normal built-ins as typed NotModelled (never fake
/// 1004) and deleting one custom style neither shifts live handles to later styles nor aliases the deleted handle.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_qrnem_styles_delete_builtin_refuses_and_live_handles_survive_delete() {
    let blank = blank_package();
    let archive = existing_wire_support::parse_zip(&blank);
    let entries = archive
        .entries
        .iter()
        .map(|entry| {
            let payload = entry.payload(&blank);
            if entry.name == "xl/styles.xml" {
                let bytes = if entry.method == 0 {
                    payload.to_vec()
                } else {
                    miniz_oxide::inflate::decompress_to_vec(payload).unwrap()
                };
                let xml = String::from_utf8(bytes)
                    .unwrap()
                    .replace("<cellStyleXfs count=\"1\">", "<cellStyleXfs count=\"2\">")
                    .replace(
                        "</cellStyleXfs>",
                        "<xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/></cellStyleXfs>",
                    )
                    .replace("<cellStyles count=\"1\">", "<cellStyles count=\"2\">")
                    .replace(
                        "</cellStyles>",
                        "<cellStyle name=\"Currency\" xfId=\"1\" builtinId=\"4\"/></cellStyles>",
                    );
                existing_wire_support::RawEntry::stored(&entry.name, xml.as_bytes())
            } else if entry.method == 0 {
                existing_wire_support::RawEntry::stored(&entry.name, payload)
            } else {
                existing_wire_support::RawEntry::deflated(
                    &entry.name,
                    payload,
                    entry.size,
                    entry.crc32,
                )
            }
        })
        .collect::<Vec<_>>();
    let package = existing_wire_support::build_zip(&entries);
    let (_host, model) = setup_session(
        HostSession::open(
            Arc::new(MemorySource::new(package)),
            &OpenPolicy::default(),
            HostOptions::default(),
        )
        .unwrap(),
    );
    let book = obj(invoke(&model, &model.application(), "ActiveWorkbook", vec![]).unwrap());
    let styles = obj(invoke(&model, &book, "Styles", vec![]).unwrap());
    let normal = obj(invoke_kind(&model, &styles, "Item", InvokeKind::Get, vec![AutoValue::text("Normal")]).unwrap());
    let currency = obj(invoke_kind(&model, &styles, "Item", InvokeKind::Get, vec![AutoValue::text("Currency")]).unwrap());
    // Normal -> 1004; non-Normal built-in -> typed NotModelled citing dc-lj4j5 (not fake 1004).
    let del_normal = invoke_kind(&model, &normal, "Delete", InvokeKind::Method, vec![]);
    assert!(matches!(del_normal, Err(AutoError::Excel { number: 1004, .. })), "{del_normal:?}");
    let del_builtin = invoke_kind(&model, &currency, "Delete", InvokeKind::Method, vec![]);
    assert!(
        matches!(&del_builtin, Err(AutoError::NotModelled { owner, .. }) if *owner == "dc-lj4j5"),
        "deleting non-Normal built-in must refuse typed citing dc-lj4j5: {del_builtin:?}"
    );
    // Live handles across deletion: deleting s1 must not shift s2 or alias s1 to s2 / recreated First.
    let s1 = obj(invoke_kind(&model, &styles, "Add", InvokeKind::Method, vec![AutoValue::text("First")]).unwrap());
    let s2 = obj(invoke_kind(&model, &styles, "Add", InvokeKind::Method, vec![AutoValue::text("Second")]).unwrap());
    let s2_font = obj(invoke(&model, &s2, "Font", vec![]).unwrap());
    invoke_kind(&model, &s1, "Delete", InvokeKind::Method, vec![]).unwrap();
    assert_eq!(text(&model, &s2, "Name"), "Second");
    invoke_kind(&model, &s2_font, "Bold", InvokeKind::Put, vec![AutoValue::Bool(true)]).unwrap();
    assert!(matches!(invoke(&model, &s2_font, "Bold", vec![]).unwrap(), AutoValue::Bool(true)));
    let a1 = range(&model, "A1");
    invoke_kind(&model, &a1, "Style", InvokeKind::Put, vec![AutoValue::Object(s2.clone())]).unwrap();
    assert_eq!(text(&model, &obj(invoke(&model, &a1, "Style", vec![]).unwrap()), "Name"), "Second");
    let stale_s1 = invoke(&model, &s1, "Name", vec![]);
    assert!(matches!(stale_s1, Err(AutoError::Excel { number: 424, .. })), "deleted handle must raise 424: {stale_s1:?}");
    // Re-adding "First" must NOT revive the deleted handle s1.
    let s1_new = obj(invoke_kind(&model, &styles, "Add", InvokeKind::Method, vec![AutoValue::text("First")]).unwrap());
    assert_eq!(text(&model, &s1_new, "Name"), "First");
    let still_stale = invoke(&model, &s1, "Name", vec![]);
    assert!(matches!(still_stale, Err(AutoError::Excel { number: 424, .. })), "{still_stale:?}");
}

/// dc-qrnem (rw12 Finding 21): ScrollRow preserves stored ScrollColumn across reopen, FreezePanes uses
/// macro-set scroll position (ww21: ScrollRow=11, ScrollColumn=2, D20 -> SplitRow=9, SplitColumn=2),
/// and Workbook.Windows(own_name) resolves its window.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_qrnem_window_scroll_freeze_and_workbook_windows_by_name() {
    let (host, model, _) = blank_report();
    let app = model.application();
    let book = obj(invoke(&model, &app, "ActiveWorkbook", vec![]).unwrap());
    let book_name = text(&model, &book, "Name");
    let book_windows = obj(invoke(&model, &book, "Windows", vec![]).unwrap());
    let by_name = obj(invoke_kind(&model, &book_windows, "Item", InvokeKind::Get, vec![AutoValue::text(&book_name)]).unwrap());
    let window = obj(invoke(&model, &app, "ActiveWindow", vec![]).unwrap());
    assert_eq!(by_name.key, window.key);

    // Save ScrollRow=11, ScrollColumn=4, reopen, then put ScrollRow=15 alone -> ScrollColumn stays 4.
    invoke_kind(&model, &window, "ScrollRow", InvokeKind::Put, vec![AutoValue::I4(11)]).unwrap();
    invoke_kind(&model, &window, "ScrollColumn", InvokeKind::Put, vec![AutoValue::I4(4)]).unwrap();
    let reopened = reopen_formatting(&host);
    let w2 = obj(invoke(&reopened, &reopened.application(), "ActiveWindow", vec![]).unwrap());
    invoke_kind(&reopened, &w2, "ScrollRow", InvokeKind::Put, vec![AutoValue::I4(15)]).unwrap();
    assert_eq!(int(invoke(&reopened, &w2, "ScrollRow", vec![]).unwrap()), 15);
    assert_eq!(int(invoke(&reopened, &w2, "ScrollColumn", vec![]).unwrap()), 4);

    // Macro-set scroll (11, 2) + active cell D20 + FreezePanes=True -> SplitRow=9, SplitColumn=2.
    invoke_kind(&model, &window, "ScrollRow", InvokeKind::Put, vec![AutoValue::I4(11)]).unwrap();
    invoke_kind(&model, &window, "ScrollColumn", InvokeKind::Put, vec![AutoValue::I4(2)]).unwrap();
    let d20 = range(&model, "D20");
    invoke_kind(&model, &d20, "Select", InvokeKind::Method, vec![]).unwrap();
    invoke_kind(&model, &window, "FreezePanes", InvokeKind::Put, vec![AutoValue::Bool(true)]).unwrap();
    assert_eq!(int(invoke(&model, &window, "SplitRow", vec![]).unwrap()), 9);
    assert_eq!(int(invoke(&model, &window, "SplitColumn", vec![]).unwrap()), 2);
}

/// dc-qrnem (rw12 Finding 21): Range.Text records MacroObservation when reading a stale or file-cached
/// cell under strict file provenance (matching Range.Value).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_qrnem_range_text_records_macro_observation_on_stale_cell() {
    let mut session = HostSession::open(
        Arc::new(MemorySource::new(blank_package()).named("obs.xlsx")),
        &OpenPolicy::default(),
        HostOptions::default(),
    )
    .unwrap();
    let sheet = session.host().sheets()[0].id();
    let a1 = CellKey::new(sheet, 1, 1).unwrap();
    let b1 = CellKey::new(sheet, 1, 2).unwrap();
    assert!(session.dispatch(SkinIntent::EnterCell { at: a1, text: "10".into() }).accepted());
    assert!(session.dispatch(SkinIntent::EnterCell { at: b1, text: "=A1*2".into() }).accepted());
    while session.has_pending_calculation() {
        session.advance_calculation();
    }
    // Switch to manual mode and edit A1 so B1 becomes stale (last value 20).
    assert!(session.dispatch(SkinIntent::SetCalcMode(calc_host::CalcMode::Manual)).accepted());
    assert!(session.dispatch(SkinIntent::EnterCell { at: a1, text: "99".into() }).accepted());

    let (_dummy, model) = setup_session(
        HostSession::open(
            Arc::new(MemorySource::new(blank_package())),
            &OpenPolicy::default(),
            HostOptions::default(),
        )
        .unwrap(),
    );
    let got = model.with_event_session(&mut session, true, || {
        let r = range(&model, "B1");
        text(&model, &r, "Text")
    });
    assert_eq!(got, "20");
    let observed = session.take_macro_observations();
    assert!(
        observed.iter().any(|o| o.at == b1 && o.provenance == calc_host::ObservedProvenance::Stale),
        "Range.Text must record MacroObservation on stale cell under strict provenance: {observed:?}"
    );
}

/// rw12 finding 14 (dc-iq8o8): `Style.Locked` and `FormulaHidden` puts on an unused custom style reach the saved
/// `<protection>` of its cellStyleXf, so a cell given the style reads them back after save and reopen.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_iq8o8__style_locked_and_formula_hidden_survive_save() {
    let (host, model, _) = blank_report();
    let book = obj(invoke(&model, &model.application(), "ActiveWorkbook", vec![]).unwrap());
    let styles = obj(invoke(&model, &book, "Styles", vec![]).unwrap());
    let mine = obj(invoke_kind(&model, &styles, "Add", InvokeKind::Method, vec![AutoValue::text("Mine8")]).unwrap());
    invoke_kind(&model, &mine, "Locked", InvokeKind::Put, vec![AutoValue::Bool(false)]).unwrap();
    invoke_kind(&model, &mine, "FormulaHidden", InvokeKind::Put, vec![AutoValue::Bool(true)]).unwrap();
    let cell = range(&model, "G2");
    invoke_kind(&model, &cell, "Style", InvokeKind::Put, vec![AutoValue::text("Mine8")]).unwrap();
    let reopened = reopen_formatting(&host);
    let again = range(&reopened, "G2");
    assert!(matches!(invoke(&reopened, &again, "Locked", vec![]).unwrap(), AutoValue::Bool(false)), "Locked lost on save");
    let book = obj(invoke(&reopened, &reopened.application(), "ActiveWorkbook", vec![]).unwrap());
    let styles = obj(invoke(&reopened, &book, "Styles", vec![]).unwrap());
    let style = obj(invoke_kind(&reopened, &styles, "Item", InvokeKind::Get, vec![AutoValue::text("Mine8")]).unwrap());
    assert!(matches!(invoke(&reopened, &style, "Locked", vec![]).unwrap(), AutoValue::Bool(false)), "Style.Locked lost on save");
    assert!(matches!(invoke(&reopened, &style, "FormulaHidden", vec![]).unwrap(), AutoValue::Bool(true)), "FormulaHidden lost on save");
}

/// rw12 finding 14 (dc-iq8o8): a write to an imported style no cell uses must not make Save fail with "Imported
/// cellStyleXfs records cannot be replaced": it is refused typed (dc-lj4j5 owns file-style writes) and the save works.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_iq8o8__writing_an_unused_imported_style_does_not_break_save() {
    let (host, model, _) = blank_report();
    let book = obj(invoke(&model, &model.application(), "ActiveWorkbook", vec![]).unwrap());
    let styles = obj(invoke(&model, &book, "Styles", vec![]).unwrap());
    invoke_kind(&model, &styles, "Add", InvokeKind::Method, vec![AutoValue::text("Mine9")]).unwrap();
    // After a save and reopen the style is one the file holds, which no cell uses.
    let mut first = Vec::new();
    host.session.borrow_mut().save(&SavePolicy::default(), &mut first).unwrap();
    let (reopened_host, reopened) = setup_session(
        HostSession::open(Arc::new(MemorySource::new(first)), &OpenPolicy::default(), HostOptions::default()).unwrap(),
    );
    let book = obj(invoke(&reopened, &reopened.application(), "ActiveWorkbook", vec![]).unwrap());
    let styles = obj(invoke(&reopened, &book, "Styles", vec![]).unwrap());
    let imported = obj(invoke_kind(&reopened, &styles, "Item", InvokeKind::Get, vec![AutoValue::text("Mine9")]).unwrap());
    let writes = [
        ("WrapText", AutoValue::Bool(true)),
        ("Locked", AutoValue::Bool(false)),
        ("NumberFormat", AutoValue::text("0.0")),
        ("HorizontalAlignment", AutoValue::I4(-4108)),
    ];
    for (member, value) in writes {
        let r = invoke_kind(&reopened, &imported, member, InvokeKind::Put, vec![value]);
        assert!(matches!(r, Err(AutoError::NotModelled { owner: "dc-lj4j5", .. })), "{member}: {r:?}");
    }
    let mut saved = Vec::new();
    reopened_host.session.borrow_mut().save(&SavePolicy::default(), &mut saved).expect("Save still works");
}

/// rw12 finding 7 (dc-6q9t4): every write path meets the protection of its own destination, not of what it reads.
/// On a sheet protected for contents, with only B2 unlocked: Copy/Cut with a Destination and Paste/PasteSpecial write
/// the destination (refused over a locked cell, allowed over B2) and only read a locked source; Borders, a
/// FormatConditions rule and a one-cell Sort that widens to a locked region are refused.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_6q9t4__write_paths_meet_the_protection_of_their_destination() {
    let (_host, model, _) = blank_report();
    let sheets = worksheets(&model);
    let sheet = obj(invoke(&model, &sheets, "Item", vec![AutoValue::I4(1)]).unwrap());
    let method = |target: &ObjectRef, name: &str, args: Vec<AutoValue>| invoke_kind(&model, target, name, InvokeKind::Method, args);
    let put = |target: &ObjectRef, name: &str, args: Vec<AutoValue>| invoke_kind(&model, target, name, InvokeKind::Put, args);
    let protected_error = |r: &Reply| matches!(r, Err(AutoError::Excel { number: 1004, description }) if description.contains("protected sheet"));
    for (address, value) in [("A1", 3.0), ("A2", 1.0), ("A3", 2.0)] {
        put(&range(&model, address), "Value", vec![AutoValue::R8(value)]).unwrap();
    }
    for (address, value) in [("H1", 3.0), ("H2", 1.0), ("H3", 2.0)] {
        put(&range(&model, address), "Value", vec![AutoValue::R8(value)]).unwrap();
    }
    // A copy carries the source's Locked with its format, so each write gets its own unlocked cell.
    for address in ["B2", "C3", "E5", "H2"] {
        put(&range(&model, address), "Locked", vec![AutoValue::Bool(false)]).unwrap();
    }
    method(&sheet, "Protect", vec![]).unwrap();
    let a = |name: &str| AutoValue::Object(range(&model, name));
    // Copy with a Destination: the locked source is only read.
    let r = method(&range(&model, "A1"), "Copy", vec![a("D4")]);
    assert!(protected_error(&r), "Copy onto a locked cell: {r:?}");
    assert!(matches!(invoke(&model, &range(&model, "D4"), "Value", vec![]).unwrap(), AutoValue::Empty), "D4 was written");
    let r = method(&range(&model, "A1"), "Copy", vec![a("B2")]);
    assert!(r.is_ok(), "Copy of a locked cell onto an unlocked one: {r:?}");
    assert!(matches!(invoke(&model, &range(&model, "B2"), "Value", vec![]).unwrap(), AutoValue::R8(n) if n == 3.0));
    // Cut moves its source too: a locked source is refused.
    let r = method(&range(&model, "A3"), "Cut", vec![a("B2")]);
    assert!(protected_error(&r), "Cut of a locked cell: {r:?}");
    // The clipboard route.
    method(&range(&model, "A1"), "Copy", vec![]).unwrap();
    let r = method(&range(&model, "D4"), "PasteSpecial", vec![]);
    assert!(protected_error(&r), "PasteSpecial onto a locked cell: {r:?}");
    assert!(matches!(invoke(&model, &range(&model, "D4"), "Value", vec![]).unwrap(), AutoValue::Empty), "D4 was pasted");
    let r = method(&range(&model, "C3"), "PasteSpecial", vec![]);
    assert!(r.is_ok(), "PasteSpecial onto an unlocked cell: {r:?}");
    // Borders.
    let edge = |address: &str| {
        let borders = obj(invoke(&model, &range(&model, address), "Borders", vec![]).unwrap());
        obj(invoke(&model, &borders, "Item", vec![AutoValue::I4(9)]).unwrap())
    };
    let r = put(&edge("D4"), "LineStyle", vec![AutoValue::I4(1)]);
    assert!(protected_error(&r), "a border on a locked cell: {r:?}");
    assert_eq!(i4(&model, &edge("D4"), "LineStyle"), -4142);
    assert!(put(&edge("E5"), "LineStyle", vec![AutoValue::I4(1)]).is_ok());
    // A conditional format.
    let conditions = obj(invoke(&model, &range(&model, "D4"), "FormatConditions", vec![]).unwrap());
    let r = method(&conditions, "Add", vec![AutoValue::I4(1), AutoValue::I4(3), AutoValue::text("=1")]);
    assert!(protected_error(&r), "a rule over a locked cell: {r:?}");
    // A one-cell Sort of an unlocked cell widens to its current region, which holds locked cells: H1:H3.
    let r = method(&range(&model, "H2"), "Sort", vec![a("H1")]);
    assert!(protected_error(&r), "a Sort of a locked region: {r:?}");
    assert!(matches!(invoke(&model, &range(&model, "H1"), "Value", vec![]).unwrap(), AutoValue::R8(n) if n == 3.0), "H1 was sorted");
}

/// rw12 finding 7 (dc-6q9t4): a value write over a whole table of a protected sheet is refused before the table is
/// deleted (the table pre-step must not run first).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_6q9t4__a_refused_write_over_a_table_keeps_the_table() {
    let (_host, model, _) = blank_report();
    let sheets = worksheets(&model);
    let sheet = obj(invoke(&model, &sheets, "Item", vec![AutoValue::I4(1)]).unwrap());
    let put = |target: &ObjectRef, name: &str, args: Vec<AutoValue>| invoke_kind(&model, target, name, InvokeKind::Put, args);
    for (address, value) in [("A1", "h1"), ("B1", "h2")] {
        put(&range(&model, address), "Value", vec![AutoValue::text(value)]).unwrap();
    }
    for (address, value) in [("A2", 1.0), ("B2", 2.0)] {
        put(&range(&model, address), "Value", vec![AutoValue::R8(value)]).unwrap();
    }
    let tables = obj(invoke(&model, &sheet, "ListObjects", vec![]).unwrap());
    let args = vec![AutoValue::I4(1), AutoValue::Object(range(&model, "A1:B2")), AutoValue::Missing, AutoValue::I4(1)];
    invoke_kind(&model, &tables, "Add", InvokeKind::Method, args).unwrap();
    let count = || int(invoke(&model, &tables, "Count", vec![]).unwrap());
    assert_eq!(count(), 1);
    invoke_kind(&model, &sheet, "Protect", InvokeKind::Method, vec![]).unwrap();
    let r = put(&range(&model, "A1:B2"), "Value", vec![AutoValue::text("x")]);
    assert!(matches!(&r, Err(AutoError::Excel { number: 1004, .. })), "{r:?}");
    assert_eq!(count(), 1, "the table was unlisted before the write was refused");
}

/// rw12 finding 7 (dc-6q9t4): AdvancedFilter reads its list and writes its CopyToRange: a locked list is no obstacle
/// and a locked CopyToRange is refused.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_6q9t4__advanced_filter_guards_its_destination_not_its_source() {
    let (_host, model, _) = blank_report();
    let sheets = worksheets(&model);
    let sheet = obj(invoke(&model, &sheets, "Item", vec![AutoValue::I4(1)]).unwrap());
    let put = |a: &str, v: AutoValue| {
        invoke_kind(&model, &range(&model, a), "Value", InvokeKind::Put, vec![v]).unwrap();
    };
    for (i, (name, qty)) in [("Name", "Qty"), ("apple", "5"), ("pear", "20")].iter().enumerate() {
        put(&format!("A{}", i + 1), AutoValue::text(name));
        match qty.parse::<f64>() {
            Ok(n) => put(&format!("B{}", i + 1), AutoValue::R8(n)),
            Err(_) => put(&format!("B{}", i + 1), AutoValue::text(qty)),
        }
    }
    invoke_kind(&model, &range(&model, "G1"), "Locked", InvokeKind::Put, vec![AutoValue::Bool(false)]).unwrap();
    invoke_kind(&model, &sheet, "Protect", InvokeKind::Method, vec![]).unwrap();
    let run = |to: &str| {
        let args = vec![AutoValue::I4(2), AutoValue::Missing, AutoValue::Object(range(&model, to)), AutoValue::Bool(false)];
        invoke_kind(&model, &range(&model, "A1:B3"), "AdvancedFilter", InvokeKind::Method, args)
    };
    let r = run("D1");
    assert!(matches!(&r, Err(AutoError::Excel { number: 1004, description }) if description.contains("protected sheet")), "{r:?}");
    // The output block G1:H3 is not all unlocked, so this is refused too; the point is that a locked source alone is not.
    let r = run("G1");
    assert!(matches!(&r, Err(AutoError::Excel { number: 1004, description }) if description.contains("protected sheet")), "{r:?}");
    for a in ["G1:H3"] {
        invoke_kind(&model, &sheet, "Unprotect", InvokeKind::Method, vec![]).unwrap();
        invoke_kind(&model, &range(&model, a), "Locked", InvokeKind::Put, vec![AutoValue::Bool(false)]).unwrap();
        invoke_kind(&model, &sheet, "Protect", InvokeKind::Method, vec![]).unwrap();
    }
    let r = run("G1");
    assert!(r.is_ok(), "a locked list over an unlocked CopyToRange: {r:?}");
}

/// rw12 finding 7 (dc-6q9t4, dc-5oumk): Protect's Allow* arguments (5..16) are stored and the sheet protects;
/// UserInterfaceOnly on an already-protected sheet is not modelled and is refused typed (dc-ml3k0).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_6q9t4__protect_allow_arguments_and_user_interface_only_are_refused_typed() {
    let (_host, model, _) = blank_report();
    let sheets = worksheets(&model);
    let sheet = obj(invoke(&model, &sheets, "Item", vec![AutoValue::I4(1)]).unwrap());
    let protect = |args: Vec<AutoValue>| invoke_kind(&model, &sheet, "Protect", InvokeKind::Method, args);
    let flag = |name: &str| matches!(invoke(&model, &sheet, name, vec![]).unwrap(), AutoValue::Bool(true));
    for index in 5..16 {
        let mut args = vec![AutoValue::Missing; index];
        args.push(AutoValue::Bool(true));
        protect(args).unwrap_or_else(|e| panic!("arg {index}: {e:?}"));
        assert!(flag("ProtectContents"), "arg {index} did not protect the sheet");
        invoke_kind(&model, &sheet, "Unprotect", InvokeKind::Method, vec![]).unwrap();
    }
    protect(vec![]).unwrap();
    let r = protect(vec![AutoValue::Missing, AutoValue::Missing, AutoValue::Missing, AutoValue::Missing, AutoValue::Bool(true)]);
    assert!(matches!(r, Err(AutoError::NotModelled { owner: "dc-ml3k0", .. })), "{r:?}");
    assert!(!flag("ProtectionMode"));
}

/// dc-ml3k0 (ww22 allow/observations.jsonl, Excel 16.0.20625.20108): Protect again with the same Password re-protects
/// (UserInterfaceOnly becomes True), a wrong Password is 1004 and changes nothing, an omitted one opens the Unprotect
/// Sheet prompt (typed refusal).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_ml3k0__protect_again_checks_the_password() {
    let (_host, model, _) = blank_report();
    let sheets = worksheets(&model);
    let sheet = obj(invoke(&model, &sheets, "Item", vec![AutoValue::I4(1)]).unwrap());
    let protect = |args: Vec<AutoValue>| invoke_kind(&model, &sheet, "Protect", InvokeKind::Method, args);
    let mode = || matches!(invoke(&model, &sheet, "ProtectionMode", vec![]).unwrap(), AutoValue::Bool(true));
    protect(vec![AutoValue::text("abc")]).unwrap();
    let ui = |password: AutoValue| vec![password, AutoValue::Bool(true), AutoValue::Bool(true), AutoValue::Bool(true), AutoValue::Bool(true)];
    let r = protect(ui(AutoValue::text("wrong")));
    assert!(matches!(&r, Err(AutoError::Excel { number: 1004, description }) if description.starts_with("The password you supplied is not correct")), "{r:?}");
    assert!(!mode(), "a wrong password changed the protection");
    let r = protect(ui(AutoValue::Missing));
    assert!(matches!(r, Err(AutoError::NotModelled { owner: "dc-9b7zd", .. })), "{r:?}");
    assert!(!mode());
    protect(ui(AutoValue::text("abc"))).unwrap();
    assert!(mode(), "the same password did not re-protect with UserInterfaceOnly");
    let put = invoke_kind(&model, &range(&model, "A1"), "Value", InvokeKind::Put, vec![AutoValue::R8(3.0)]);
    assert!(put.is_ok(), "a locked cell under UserInterfaceOnly: {put:?}");
}

/// Native-only: waits for the Windows descriptor regeneration (the `Protection` interface; RUN-ON-WINDOWS.txt, dc-ml3k0).
/// Each Allow* flag set alone reads back True through `Worksheet.Protection` and the others False (ww22 allow/).
#[test]
#[ignore = "Protection is not in the descriptor until it is regenerated on Windows"]
fn dc_ml3k0__allow_flags_read_back_through_protection() {
    let names = [
        "AllowFormattingCells", "AllowFormattingColumns", "AllowFormattingRows", "AllowInsertingColumns", "AllowInsertingRows",
        "AllowInsertingHyperlinks", "AllowDeletingColumns", "AllowDeletingRows", "AllowSorting", "AllowFiltering",
        "AllowUsingPivotTables",
    ];
    let (_host, model, _) = blank_report();
    let sheets = worksheets(&model);
    let sheet = obj(invoke(&model, &sheets, "Item", vec![AutoValue::I4(1)]).unwrap());
    for (index, set) in names.iter().enumerate() {
        let mut args = vec![AutoValue::text("abc"), AutoValue::Missing, AutoValue::Missing, AutoValue::Missing, AutoValue::Missing];
        args.extend((0..11).map(|i| if i == index { AutoValue::Bool(true) } else { AutoValue::Missing }));
        invoke_kind(&model, &sheet, "Protect", InvokeKind::Method, args).unwrap();
        let protection = obj(invoke(&model, &sheet, "Protection", vec![]).unwrap());
        for name in names {
            let on = matches!(invoke(&model, &protection, name, vec![]).unwrap(), AutoValue::Bool(true));
            assert_eq!(on, name == *set, "{set} set alone, reading {name}");
        }
        invoke_kind(&model, &sheet, "Unprotect", InvokeKind::Method, vec![AutoValue::text("abc")]).unwrap();
    }
}

/// dc-9b7zd (ww22 protect/packages.json text.xlsx, Excel 16.0.20625.20108): a sheet the file protects with the SHA-512
/// hash of the password "abc" (salt, spinCount 100000) unprotects with that text only: a wrong one is 1004 and leaves
/// it protected, an omitted one opens the Unprotect Sheet prompt (typed refusal), the right one unprotects.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_9b7zd__unprotect_checks_the_password_hash_a_file_holds() {
    let blank = blank_package();
    let archive = existing_wire_support::parse_zip(&blank);
    let entries = archive
        .entries
        .iter()
        .map(|entry| {
            let payload = entry.payload(&blank);
            if entry.name == "xl/worksheets/sheet1.xml" {
                let bytes = if entry.method == 0 { payload.to_vec() } else { miniz_oxide::inflate::decompress_to_vec(payload).unwrap() };
                let xml = String::from_utf8(bytes).unwrap().replace(
                    "<sheetData/>",
                    "<sheetData/><sheetProtection algorithmName=\"SHA-512\" hashValue=\"HlkqK902Kw0Yyj0cUNuJMJoH1ti513mUsZNFUXfNOfu0S6HHtjsGr9IF43bSiwYn+2RUYBiETeJGY7bShrpAiQ==\" saltValue=\"LofWZ3ZdLXNq/7wV9Ll0WA==\" spinCount=\"100000\" sheet=\"1\" formatCells=\"0\"/>",
                );
                existing_wire_support::RawEntry::stored(&entry.name, xml.as_bytes())
            } else if entry.method == 0 {
                existing_wire_support::RawEntry::stored(&entry.name, payload)
            } else {
                existing_wire_support::RawEntry::deflated(&entry.name, payload, entry.size, entry.crc32)
            }
        })
        .collect::<Vec<_>>();
    let package = existing_wire_support::build_zip(&entries);
    let (_host, model) = setup_session(HostSession::open(Arc::new(MemorySource::new(package)), &OpenPolicy::default(), HostOptions::default()).unwrap());
    let sheet = obj(invoke(&model, &model.application(), "ActiveSheet", vec![]).unwrap());
    let contents = || matches!(invoke(&model, &sheet, "ProtectContents", vec![]).unwrap(), AutoValue::Bool(true));
    let unprotect = |args: Vec<AutoValue>| invoke_kind(&model, &sheet, "Unprotect", InvokeKind::Method, args);
    assert!(contents());
    let r = unprotect(vec![AutoValue::text("wrong")]);
    assert!(matches!(&r, Err(AutoError::Excel { number: 1004, description }) if description.starts_with("The password you supplied is not correct")), "{r:?}");
    assert!(contents(), "a wrong password unprotected the sheet");
    let r = unprotect(vec![]);
    assert!(matches!(r, Err(AutoError::NotModelled { owner: "dc-9b7zd", .. })), "{r:?}");
    assert!(contents());
    unprotect(vec![AutoValue::text("abc")]).unwrap();
    assert!(!contents(), "the right password did not unprotect the sheet");
}

/// dc-p7gpy (rw12 Finding 19): typed refusals must cite OPEN owning beads, never closed ones.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_p7gpy_refusals_cite_open_beads() {
    let (_host, model, _) = blank_report();
    let app = model.application();

    // 1. protection.rs: Unprotect of a passworded sheet without a password (the modal prompt) -> dc-9b7zd
    let sheet = obj(invoke(&model, &app, "ActiveSheet", vec![]).unwrap());
    invoke_kind(&model, &sheet, "Protect", InvokeKind::Method, vec![AutoValue::text("pw")]).unwrap();
    let protect = invoke_kind(&model, &sheet, "Unprotect", InvokeKind::Method, vec![]);
    assert!(
        matches!(&protect, Err(AutoError::NotModelled { owner, .. }) if *owner == "dc-9b7zd"),
        "the Unprotect prompt must cite dc-9b7zd: {protect:?}"
    );
    invoke_kind(&model, &sheet, "Unprotect", InvokeKind::Method, vec![AutoValue::text("pw")]).unwrap();

    // 2. chart_format.rs & chart_axes.rs: unstated Series.Border -> dc-yship (was closed dc-oxh8w.60);
    //    Chart.Axes without Type / secondary group -> dc-oxh8w.3 (was closed dc-oxh8w.48).
    let charts = obj(invoke_kind(&model, &sheet, "ChartObjects", InvokeKind::Method, vec![]).unwrap());
    let frame = obj(invoke_kind(
        &model,
        &charts,
        "Add",
        InvokeKind::Method,
        vec![AutoValue::R8(0.0), AutoValue::R8(0.0), AutoValue::R8(240.0), AutoValue::R8(160.0)],
    )
    .unwrap());
    let chart = obj(invoke(&model, &frame, "Chart", vec![]).unwrap());
    invoke_kind(&model, &range(&model, "A1"), "Value2", InvokeKind::Put, vec![AutoValue::text("X")]).unwrap();
    invoke_kind(&model, &range(&model, "B1"), "Value2", InvokeKind::Put, vec![AutoValue::text("Y")]).unwrap();
    invoke_kind(&model, &range(&model, "A2"), "Value2", InvokeKind::Put, vec![AutoValue::R8(1.0)]).unwrap();
    invoke_kind(&model, &range(&model, "B2"), "Value2", InvokeKind::Put, vec![AutoValue::R8(2.0)]).unwrap();
    invoke_kind(
        &model,
        &chart,
        "SetSourceData",
        InvokeKind::Method,
        vec![AutoValue::Object(range(&model, "A1:B2"))],
    )
    .unwrap();
    let series = obj(invoke_kind(&model, &chart, "SeriesCollection", InvokeKind::Method, vec![AutoValue::I4(1)]).unwrap());
    let border = obj(invoke(&model, &series, "Border", vec![]).unwrap());
    let unstated = invoke(&model, &border, "LineStyle", vec![]);
    assert!(
        matches!(&unstated, Err(AutoError::NotModelled { owner, .. }) if *owner == "dc-yship"),
        "unstated Series.Border.LineStyle must cite dc-yship: {unstated:?}"
    );
    let axes_all = invoke_kind(&model, &chart, "Axes", InvokeKind::Method, vec![]);
    assert!(
        matches!(&axes_all, Err(AutoError::NotModelled { owner, .. }) if *owner == "dc-oxh8w.3"),
        "Chart.Axes() must cite dc-oxh8w.3: {axes_all:?}"
    );
    let axes_sec = invoke_kind(&model, &chart, "Axes", InvokeKind::Method, vec![AutoValue::I4(2), AutoValue::I4(2)]);
    assert!(
        matches!(&axes_sec, Err(AutoError::NotModelled { owner, .. }) if *owner == "dc-oxh8w.3"),
        "Chart.Axes secondary group must cite dc-oxh8w.3: {axes_sec:?}"
    );

    // 3. books.rs: Workbooks.Add without factory -> dc-oxh8w.75 (was closed dc-oxh8w.42)
    calc_host::automation::set_book_factory(None);
    let books = obj(invoke(&model, &app, "Workbooks", vec![]).unwrap());
    let add_book = invoke_kind(&model, &books, "Add", InvokeKind::Method, vec![]);
    assert!(
        matches!(&add_book, Err(AutoError::NotModelled { owner, .. }) if *owner == "dc-oxh8w.75"),
        "Workbooks.Add without factory must cite dc-oxh8w.75: {add_book:?}"
    );

    // 4. autofilter.rs: host refusal on merged range -> dc-oxh8w.43 (was closed dc-0f1k.24)
    let merged = range(&model, "D1:E2");
    invoke_kind(&model, &merged, "Merge", InvokeKind::Method, vec![]).unwrap();
    let af = invoke_kind(&model, &merged, "AutoFilter", InvokeKind::Method, vec![]);
    assert!(
        matches!(&af, Err(AutoError::NotModelled { owner, .. }) if *owner == "dc-oxh8w.43"),
        "Range.AutoFilter host refusal must cite dc-oxh8w.43: {af:?}"
    );

    // 5. application.rs: Application.Calculation inside invocation without UserView.settings -> dc-1nbp.6 (was closed dc-oxh8w.52)
    struct NoSettingsContext(CellKey);
    impl UserFunctionContext for NoSettingsContext {
        fn caller(&self) -> Option<CellKey> {
            Some(self.0)
        }
        fn user_view(&self) -> Option<calc_func::UserView> {
            Some(calc_func::UserView {
                active_sheet: self.0.sheet(),
                selection: self.0,
                settings: None,
            })
        }
        fn reference_qualifier(&mut self, _: calc_value::SheetId) -> Result<String, String> {
            Ok("'[book.xlsx]Sheet1'".into())
        }
        fn refuse(&mut self, reason: &'static str) -> String {
            reason.into()
        }
        fn values_of(&mut self, _: &calc_value::ReferenceLike) -> Result<CalcValue, String> {
            Ok(CalcValue::empty())
        }
        fn worksheet_function(&mut self, _: &str, _: &[CalcValue]) -> Result<CalcValue, String> {
            Ok(CalcValue::empty())
        }
    }
    let (inv_host, inv_model) = setup();
    let sheet_id = inv_host.session.borrow().host().sheets()[0].id();
    let cell = CellKey::new(sheet_id, 1, 1).unwrap();
    let mut ctx = NoSettingsContext(cell);
    let loan = Loan(RefCell::new(&mut ctx));
    let calc_in_udf = INVOCATION.set(&loan, || {
        invoke(&inv_model, &inv_model.application(), "Calculation", vec![])
    });
    assert!(
        matches!(&calc_in_udf, Err(AutoError::NotModelled { owner, .. }) if *owner == "dc-1nbp.6"),
        "Application.Calculation without UserView.settings must cite dc-1nbp.6: {calc_in_udf:?}"
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_p7gpy_remaining_host_and_automation_refusals_cite_open_beads() {
    // 1. automation.rs busy() -> dc-oxh8w (was closed dc-0f1k.24)
    let busy = calc_host::automation::busy();
    assert!(
        matches!(&busy, AutoError::NotModelled { owner, .. } if *owner == "dc-oxh8w"),
        "automation::busy() must cite open dc-oxh8w: {busy:?}"
    );

    // 2. automation.rs fallback unmodelled member -> dc-oxh8w (was closed dc-1nbp.5)
    let (_host, model, _) = blank_report();
    let app = model.application();
    let unknown = invoke(&model, &app, "Interactive", vec![]);
    assert!(
        matches!(&unknown, Err(AutoError::NotModelled { owner, .. }) if *owner == "dc-oxh8w"),
        "unmodelled fallback member must cite open dc-oxh8w: {unknown:?}"
    );

    // 3. report.rs ListObjects.Add unsupported SourceType -> dc-piw0 (was closed dc-piw0.1)
    let sheet = obj(invoke(&model, &app, "ActiveSheet", vec![]).unwrap());
    let lists = obj(invoke(&model, &sheet, "ListObjects", vec![]).unwrap());
    let bad_list = invoke_kind(&model, &lists, "Add", InvokeKind::Method, vec![AutoValue::I4(0)]);
    assert!(
        matches!(&bad_list, Err(AutoError::NotModelled { owner, .. }) if *owner == "dc-piw0"),
        "ListObjects.Add unsupported SourceType must cite open dc-piw0: {bad_list:?}"
    );

    // 4. report_draw.rs AddShape unsupported type -> dc-zjsg (was closed dc-piw0.3)
    let shapes = obj(invoke(&model, &sheet, "Shapes", vec![]).unwrap());
    let bad_shape = invoke_kind(
        &model,
        &shapes,
        "AddShape",
        InvokeKind::Method,
        vec![
            AutoValue::I4(999),
            AutoValue::R8(0.0),
            AutoValue::R8(0.0),
            AutoValue::R8(50.0),
            AutoValue::R8(50.0),
        ],
    );
    assert!(
        matches!(&bad_shape, Err(AutoError::NotModelled { owner, .. }) if *owner == "dc-zjsg"),
        "Shapes.AddShape unsupported kind must cite open dc-zjsg: {bad_shape:?}"
    );
}

/// dc-y7tqj (rw12 finding 17a): `Worksheet.Names("Local")` bare lookup resolves the sheet-scoped
/// name without requiring the `Sheet1!` prefix.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_y7tqj__worksheet_names_bare_lookup() {
    let (_host, model) = setup();
    let book = obj(invoke(&model, &model.application(), "ActiveWorkbook", vec![]).unwrap());
    let sheet = obj(
        invoke_kind(
            &model,
            &obj(invoke(&model, &book, "Worksheets", vec![]).unwrap()),
            "Item",
            InvokeKind::Get,
            vec![AutoValue::I4(1)],
        )
        .unwrap(),
    );
    let sheet_scoped = obj(invoke(&model, &sheet, "Names", vec![]).unwrap());
    let added = obj(
        invoke_kind(
            &model,
            &sheet_scoped,
            "Add",
            InvokeKind::Method,
            vec![AutoValue::text("Local"), AutoValue::text("=42")],
        )
        .unwrap(),
    );
    let bare = obj(
        invoke_kind(
            &model,
            &sheet_scoped,
            "Item",
            InvokeKind::Method,
            vec![AutoValue::text("Local")],
        )
        .expect("Worksheet.Names(\"Local\") bare lookup must succeed"),
    );
    assert_eq!(bare.key, added.key);
}

/// dc-y7tqj (rw12 finding 17b/17c): xlPasteColumnWidths preserves destination hidden/outline flags,
/// accepts mismatched row counts, and refuses unmeasured multi-column tiling (dc-2rjye);
/// fabricated Paste:=21 refuses typed (dc-2rjye); xlPasteValidation and xlPasteComments are not
/// blocked by unread conditional formatting containers.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_y7tqj__paste_special_column_widths_and_comments_edges() {
    let (host, model) = setup_session(
        HostSession::open(
            Arc::new(MemorySource::new(blank_package())),
            &OpenPolicy::default(),
            HostOptions::default(),
        )
        .unwrap(),
    );
    fill(&host, (1, 1), &[&["1", "2"], &["3", "4"]]);
    let call = |address: &str, name: &str, args: Vec<AutoValue>| {
        invoke_kind(&model, &range(&model, address), name, InvokeKind::Method, args)
    };
    invoke_kind(&model, &range(&model, "A1"), "ColumnWidth", InvokeKind::Put, vec![AutoValue::I4(20)]).unwrap();
    invoke_kind(&model, &range(&model, "B1"), "ColumnWidth", InvokeKind::Put, vec![AutoValue::I4(30)]).unwrap();
    // Set Hidden and outline_level on column E via the host's axis layout before pasting widths onto E1:F1
    // (mismatched row count: source is 2 rows, dest is 1 row).
    let sheet_id = host.session.borrow().host().sheets()[0].id();
    let hidden_outlined = calc_doc_model::AxisFormat::new(None, true, false)
        .unwrap()
        .with_outline(2, 0)
        .unwrap();
    let axis_edit = calc_doc_model::AxisFormatEdit::new(
        sheet_id,
        calc_doc_model::Axis::Columns,
        5,
        hidden_outlined,
    )
    .unwrap();
    assert!(host
        .session
        .borrow_mut()
        .dispatch(SkinIntent::Edit { edits: vec![calc_host::CellEdit::AxisFormat(axis_edit)] })
        .accepted());
    call("A1:B2", "Copy", vec![]).unwrap();
    call("E1:F1", "PasteSpecial", vec![AutoValue::I4(8)])
        .expect("xlPasteColumnWidths should accept mismatched row count");
    assert!(matches!(invoke(&model, &range(&model, "F1"), "ColumnWidth", vec![]).unwrap(), AutoValue::R8(n) if n == 30.0));
    let (after_a, after_e) = {
        let session = host.session.borrow();
        let layouts = session.host().model().definitions().axis_layouts();
        (
            layouts.get(sheet_id, calc_doc_model::Axis::Columns, 1),
            layouts.get(sheet_id, calc_doc_model::Axis::Columns, 5),
        )
    };
    assert_eq!(after_e.size(), after_a.size(), "xlPasteColumnWidths must copy column A's stored width to E");
    assert!(!after_e.hidden(), "xlPasteColumnWidths copies the source's hidden state (Excel ww22)");
    assert_eq!(after_e.outline_level(), 2, "xlPasteColumnWidths must preserve destination column outline_level");
    // Paste:=21 is a 1004 in Excel (ww22).
    let paste_21 = call("E5", "PasteSpecial", vec![AutoValue::I4(21)]);
    assert!(matches!(&paste_21, Err(AutoError::Excel { number: 1004, .. })), "got {paste_21:?}");
    // Validation/Comments paste must not be blocked by an unread conditional formatting container.
    let base = include_bytes!("fixtures/d2/structural-base.xlsx");
    let archive = existing_wire_support::parse_zip(base);
    let entries: Vec<_> = archive
        .entries
        .iter()
        .map(|entry| {
            let payload = entry.payload(base);
            if entry.name == "xl/worksheets/sheet1.xml" {
                let raw = if entry.method == 0 {
                    payload.to_vec()
                } else {
                    miniz_oxide::inflate::decompress_to_vec(payload).unwrap()
                };
                let xml = String::from_utf8(raw).unwrap();
                let patched = xml.replace(
                    "</worksheet>",
                    "<conditionalFormatting><cfRule type=\"cellIs\" priority=\"1\"/></conditionalFormatting></worksheet>",
                );
                existing_wire_support::RawEntry::stored(&entry.name, patched.as_bytes())
            } else if entry.method == 0 {
                existing_wire_support::RawEntry::stored(&entry.name, payload)
            } else {
                existing_wire_support::RawEntry::deflated(&entry.name, payload, entry.size, entry.crc32)
            }
        })
        .collect();
    let pkg = existing_wire_support::build_zip(&entries);
    let (_cf_host, cf_model) = setup_session(
        HostSession::open(Arc::new(MemorySource::new(pkg)), &OpenPolicy::default(), HostOptions::default()).unwrap(),
    );
    let sheet_name = _cf_host.session.borrow().host().sheets()[0].name().text().to_string_lossy();
    let cf_call = |addr: &str, name: &str, args: Vec<AutoValue>| {
        invoke_kind(&cf_model, &range(&cf_model, &format!("'{sheet_name}'!{addr}")), name, InvokeKind::Method, args)
    };
    cf_call("A2", "Copy", vec![]).unwrap();
    cf_call("L40", "PasteSpecial", vec![AutoValue::I4(-4144)])
        .expect("xlPasteComments must not be blocked by unread CF container");
    cf_call("L41", "PasteSpecial", vec![AutoValue::I4(6)])
        .expect("xlPasteValidation must not be blocked by unread CF container");
}

/// dc-y7tqj (rw12 finding 17d): valid XlAutoFilterOperator values 12..=14 refuse as NotModelled
/// (not 1004).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_y7tqj__autofilter_operators_12_to_14_refuse_typed() {
    let (host, model) = setup_session(
        HostSession::open(Arc::new(MemorySource::new(blank_package())), &OpenPolicy::default(), HostOptions::default()).unwrap(),
    );
    fill(&host, (1, 1), &[&["name", "qty"], &["a", "5"], &["b", "15"]]);
    for op in [12, 13, 14] {
        let res = invoke_kind(
            &model,
            &range(&model, "A1:B3"),
            "AutoFilter",
            InvokeKind::Method,
            vec![AutoValue::I4(1), AutoValue::Missing, AutoValue::I4(op)],
        );
        assert!(
            matches!(&res, Err(AutoError::NotModelled { owner, .. }) if *owner == "dc-oxh8w.43"),
            "XlAutoFilterOperator {op} must refuse as NotModelled(dc-oxh8w.43), got {res:?}"
        );
    }
    let bad = invoke_kind(
        &model,
        &range(&model, "A1:B3"),
        "AutoFilter",
        InvokeKind::Method,
        vec![AutoValue::I4(1), AutoValue::Missing, AutoValue::I4(15)],
    );
    assert!(matches!(bad, Err(AutoError::Excel { number: 1004, .. })), "operator 15 must be 1004: {bad:?}");
}

/// dc-y7tqj (rw12 finding 17e): hiding the active worksheet moves ActiveSheet and Selection off the
/// hidden sheet to another visible worksheet.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_y7tqj__worksheet_visible_false_moves_active_sheet() {
    let (_host, model) = setup();
    let app = model.application();
    let sheets = worksheets(&model);
    let first = obj(invoke(&model, &sheets, "Item", vec![AutoValue::I4(1)]).unwrap());
    let added = obj(invoke_kind(&model, &sheets, "Add", InvokeKind::Method, vec![]).unwrap());
    let first_name = text(&model, &first, "Name");
    let added_name = text(&model, &added, "Name");
    invoke_kind(&model, &first, "Activate", InvokeKind::Method, vec![]).unwrap();
    let active_name = || text(&model, &obj(invoke(&model, &app, "ActiveSheet", vec![]).unwrap()), "Name");
    assert_eq!(active_name(), first_name);
    invoke_kind(&model, &first, "Visible", InvokeKind::Put, vec![AutoValue::Bool(false)]).unwrap();
    assert_eq!(active_name(), added_name, "hiding ActiveSheet must activate a visible sheet");
    let sel_sheet = text(
        &model,
        &obj(invoke(&model, &obj(invoke(&model, &app, "Selection", vec![]).unwrap()), "Worksheet", vec![]).unwrap()),
        "Name",
    );
    assert_eq!(sel_sheet, added_name, "Selection must move off the hidden sheet");
}

/// dc-971p.74 (Excel ww22): Worksheet.Move across a hidden sheet leaves the hidden sheet hidden and makes the moved
/// sheet the active one, whichever sheet was active before.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_971p_74__worksheet_move_across_a_hidden_sheet_activates_the_moved_sheet() {
    let (host, model) = setup();
    let app = model.application();
    let sheets = worksheets(&model);
    let item = |n: i32| obj(invoke(&model, &sheets, "Item", vec![AutoValue::I4(n)]).unwrap());
    let a = item(1);
    let hidden = obj(invoke_kind(&model, &sheets, "Add", InvokeKind::Method, vec![AutoValue::Missing, AutoValue::Object(a.clone())]).unwrap());
    let c = obj(invoke_kind(&model, &sheets, "Add", InvokeKind::Method, vec![AutoValue::Missing, AutoValue::Object(hidden.clone())]).unwrap());
    let (a_name, h_name, c_name) = (text(&model, &a, "Name"), text(&model, &hidden, "Name"), text(&model, &c, "Name"));
    assert_eq!(sheet_names(&host), [a_name.clone(), h_name.clone(), c_name.clone()]);
    invoke_kind(&model, &hidden, "Visible", InvokeKind::Put, vec![AutoValue::Bool(false)]).unwrap();
    let active_name = || text(&model, &obj(invoke(&model, &app, "ActiveSheet", vec![]).unwrap()), "Name");
    invoke_kind(&model, &a, "Activate", InvokeKind::Method, vec![]).unwrap();
    invoke_kind(&model, &c, "Move", InvokeKind::Method, vec![AutoValue::Object(a.clone())]).unwrap();
    assert_eq!(sheet_names(&host), [c_name.clone(), a_name.clone(), h_name.clone()]);
    assert_eq!(active_name(), c_name, "the moved sheet is active (A was active before)");
    assert!(matches!(invoke_kind(&model, &hidden, "Visible", InvokeKind::Get, vec![]).unwrap(), AutoValue::I4(0)));
    // Move A after C with C active: A becomes active.
    invoke_kind(&model, &a, "Move", InvokeKind::Method, vec![AutoValue::Missing, AutoValue::Object(c.clone())]).unwrap();
    assert_eq!(sheet_names(&host), [c_name.clone(), a_name.clone(), h_name.clone()]);
    invoke_kind(&model, &c, "Activate", InvokeKind::Method, vec![]).unwrap();
    invoke_kind(&model, &a, "Move", InvokeKind::Method, vec![AutoValue::Missing, AutoValue::Object(h_to_last(&model, &sheets))]).unwrap();
    assert_eq!(active_name(), a_name, "the moved sheet is active (C was active before)");
}

fn h_to_last(model: &WorkbookAutomation, sheets: &ObjectRef) -> ObjectRef {
    let count = match invoke(model, sheets, "Count", vec![]).unwrap() {
        AutoValue::I4(n) => n,
        other => panic!("{other:?}"),
    };
    obj(invoke(model, sheets, "Item", vec![AutoValue::I4(count)]).unwrap())
}

/// dc-y7tqj (rw12 finding 17g): AdvancedFilter xlFilterCopy copies numeric/date/bool/formula-like
/// text cells without E11 refusal, clears stale cells below CopyToRange, refuses cross-workbook
/// ranges, and refuses unknown criteria headers instead of dropping all rows.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_y7tqj__advanced_filter_copy_edge_cases() {
    let (host, model) = setup_session(
        HostSession::open(Arc::new(MemorySource::new(blank_package())), &OpenPolicy::default(), HostOptions::default()).unwrap(),
    );
    let sheet_id = host.session.borrow().host().sheets()[0].id();
    let set_text = |r: u32, c: u32, s: &str| {
        let at = CellKey::new(sheet_id, r, c).unwrap();
        let text = calc_doc_model::CellText::new(calc_value::ExcelText::from_utf16_code_units(
            s.encode_utf16().collect(),
        ))
        .unwrap();
        host.session
            .borrow_mut()
            .dispatch(SkinIntent::Edit {
                edits: vec![calc_host::CellEdit::Set { at, input: calc_doc_model::CellInput::Text(text) }],
            });
    };
    set_text(1, 1, "Tag");
    set_text(1, 2, "Note");
    set_text(2, 1, "123");
    set_text(2, 2, "=1+1");
    set_text(3, 1, "TRUE");
    set_text(3, 2, "2026-01-01");
    let run = |args: Vec<AutoValue>| {
        invoke_kind(&model, &range(&model, "A1:B3"), "AdvancedFilter", InvokeKind::Method, args)
    };
    // 1) Copying numeric/formula/bool/date-like text cells must succeed without E11 refusal.
    run(vec![AutoValue::I4(2), AutoValue::Missing, AutoValue::Object(range(&model, "E1"))])
        .expect("AdvancedFilter must copy text cells without E11 parsing");
    assert_eq!(cell_text_of(&model, "E2"), "123");
    assert_eq!(cell_text_of(&model, "F2"), "=1+1");
    assert_eq!(cell_text_of(&model, "E3"), "TRUE");
    assert_eq!(cell_text_of(&model, "F3"), "2026-01-01");

    // 2) Unknown criteria header must refuse typed citing dc-hduqd (not silently drop all rows).
    set_text(1, 9, "NoSuchColumn");
    set_text(2, 9, ">0");
    let unknown = run(vec![
        AutoValue::I4(2),
        AutoValue::Object(range(&model, "I1:I2")),
        AutoValue::Object(range(&model, "K1")),
    ]);
    assert!(
        matches!(&unknown, Err(AutoError::NotModelled { owner, .. }) if *owner == "dc-hduqd"),
        "unknown criteria header must refuse typed citing dc-hduqd, got {unknown:?}"
    );

    // 3) Excel ww22: a single-cell CopyTo clears stale cells below the extract, even isolated ones.
    set_text(10, 5, "stale");
    run(vec![AutoValue::I4(2), AutoValue::Missing, AutoValue::Object(range(&model, "E1"))]).unwrap();
    assert_eq!(cell_text_of(&model, "E10"), "", "the stale cell below the extract is cleared");
    assert_eq!(cell_text_of(&model, "E3"), "TRUE");

    // 4) Foreign-workbook CopyToRange or CriteriaRange must refuse typed citing dc-hduqd.
    let (_other_host, other_model) = setup();
    let foreign_range = range(&other_model, "A1");
    let foreign = run(vec![AutoValue::I4(2), AutoValue::Missing, AutoValue::Object(foreign_range)]);
    assert!(
        matches!(&foreign, Err(AutoError::NotModelled { owner, .. }) if *owner == "dc-hduqd"),
        "foreign workbook CopyToRange must refuse typed citing dc-hduqd, got {foreign:?}"
    );
}

/// dc-hduqd (Excel ww21, ww22): the header cells of CopyToRange choose and order the copied columns, an output header
/// the list lacks is 1004, a formula list copies its values, and a one-cell CopyTo naming a list header copies that column.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_hduqd__advanced_filter_copy_chooses_columns_and_copies_formula_values() {
    let (host, model) = setup_session(
        HostSession::open(Arc::new(MemorySource::new(blank_package())), &OpenPolicy::default(), HostOptions::default()).unwrap(),
    );
    fill(&host, (1, 1), &[&["Item", "Qty", "Total"], &["a", "5", "=B2*2"], &["b", "30", "=B3*2"], &["c", "25", "=B4*2"]]);
    fill(&host, (1, 6), &[&["Qty"], &[">20"]]);
    let run = |dest: &str| {
        invoke_kind(
            &model,
            &range(&model, "A1:C4"),
            "AdvancedFilter",
            InvokeKind::Method,
            vec![AutoValue::I4(2), AutoValue::Object(range(&model, "F1:F2")), AutoValue::Object(range(&model, dest))],
        )
    };
    let put = |address: &str, text: &str| {
        invoke_kind(&model, &range(&model, address), "Value", InvokeKind::Put, vec![AutoValue::text(text)]).unwrap();
    };
    // Formula values are copied.
    assert!(matches!(run("A8"), Ok(AutoValue::Bool(true))));
    assert_eq!(cell_text_of(&model, "C9"), "60");
    // Requested order, two columns.
    put("A12", "Total");
    put("B12", "Item");
    assert!(matches!(run("A12:B12"), Ok(AutoValue::Bool(true))));
    assert_eq!(cell_text_of(&model, "A12"), "Total");
    assert_eq!(cell_text_of(&model, "A13"), "60");
    assert_eq!(cell_text_of(&model, "B13"), "b");
    assert_eq!(cell_text_of(&model, "A14"), "50");
    assert_eq!(cell_text_of(&model, "B14"), "c");
    assert_eq!(cell_text_of(&model, "C12"), "", "only the requested columns are written");
    // A one-cell CopyTo holding a list header copies that column alone.
    put("H1", "Item");
    assert!(matches!(run("H1"), Ok(AutoValue::Bool(true))));
    assert_eq!(cell_text_of(&model, "H2"), "b");
    assert_eq!(cell_text_of(&model, "I1"), "");
    // An output header the list lacks, or a one-cell label that is not a header, is 1004.
    put("A20", "Nope");
    assert!(matches!(run("A20:B20"), Err(AutoError::Excel { number: 1004, .. })));
    assert!(matches!(run("A20"), Err(AutoError::Excel { number: 1004, .. })));
}

/// ww24 e/ (Excel 16.0.20625.20108): Unique compares only the columns CopyToRange's header cells name (Item,Qty -> 3
/// records; Item,Region -> 4; Item -> 3); a blank cell among several header cells is 1004 "The extract range has a missing
/// or invalid field name"; a header repeated copies that column twice.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_hduqd__advanced_filter_unique_over_the_copied_columns_and_blank_headers() {
    let (host, model) = setup_session(
        HostSession::open(Arc::new(MemorySource::new(blank_package())), &OpenPolicy::default(), HostOptions::default()).unwrap(),
    );
    fill(
        &host,
        (1, 1),
        &[&["Item", "Qty", "Region"], &["a", "10", "N"], &["b", "20", "S"], &["a", "10", "S"], &["c", "30", "N"], &["a", "10", "N"]],
    );
    let put = |address: &str, text: &str| {
        invoke_kind(&model, &range(&model, address), "Value", InvokeKind::Put, vec![AutoValue::text(text)]).unwrap();
    };
    let run = |dest: &str, unique: bool| {
        invoke_kind(
            &model,
            &range(&model, "A1:C6"),
            "AdvancedFilter",
            InvokeKind::Method,
            vec![AutoValue::I4(2), AutoValue::Missing, AutoValue::Object(range(&model, dest)), AutoValue::Bool(unique)],
        )
    };
    let rows = |first: &str, last: &str| {
        let (c, a, b) = (first.as_bytes()[0] as char, first[1..].parse::<u32>().unwrap(), last[1..].parse::<u32>().unwrap());
        (a..=b).filter(|r| !cell_text_of(&model, &format!("{c}{r}")).is_empty()).count()
    };
    put("G1", "Item");
    put("H1", "Qty");
    assert!(matches!(run("G1:H1", true), Ok(AutoValue::Bool(true))));
    assert_eq!(rows("G1", "G8"), 4, "Item,Qty: three records and the header");
    put("J1", "Item");
    put("K1", "Region");
    assert!(matches!(run("J1:K1", true), Ok(AutoValue::Bool(true))));
    assert_eq!(rows("J1", "J8"), 5, "Item,Region: four records and the header");
    put("M1", "Item");
    assert!(matches!(run("M1", true), Ok(AutoValue::Bool(true))));
    assert_eq!(rows("M1", "M8"), 4, "Item: three records and the header");
    // A blank cell among several header cells.
    put("P1", "Item");
    put("R1", "Region");
    match run("P1:R1", false) {
        Err(AutoError::Excel { number: 1004, description }) => {
            assert_eq!(description, "The extract range has a missing or invalid field name")
        }
        other => panic!("a blank CopyTo header cell: {other:?}"),
    }
    // A repeated header copies the column twice.
    put("T1", "Item");
    put("U1", "Item");
    assert!(matches!(run("T1:U1", false), Ok(AutoValue::Bool(true))));
    assert_eq!(cell_text_of(&model, "T2"), "a");
    assert_eq!(cell_text_of(&model, "U2"), "a");
}

/// ww24 e/ (Excel 16.0.20625.20108): `xlFilterInPlace` returns True and hides the rows that miss the criteria, the header
/// row never; FilterMode is True and AutoFilterMode False; a second call filters from the full list again (">=10" hides
/// nothing, then ">=20" hides the three "a" rows); ShowAllData unhides and clears FilterMode, and is then 1004. The hidden
/// rows are saved as hidden rows.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_hduqd__advanced_filter_in_place_hides_rows_and_show_all_data_undoes_it() {
    let (host, model) = setup_session(
        HostSession::open(Arc::new(MemorySource::new(blank_package())), &OpenPolicy::default(), HostOptions::default()).unwrap(),
    );
    fill(
        &host,
        (1, 1),
        &[&["Item", "Qty", "Region"], &["a", "10", "N"], &["b", "20", "S"], &["a", "10", "S"], &["c", "30", "N"], &["a", "10", "N"]],
    );
    fill(&host, (1, 5), &[&["Qty"], &[">=10"]]);
    let sheet = obj(invoke(&model, &model.application(), "ActiveSheet", vec![]).unwrap());
    let flag = |name: &str| matches!(invoke(&model, &sheet, name, vec![]).unwrap(), AutoValue::Bool(true));
    // Range.Hidden is not modelled (dc-oxh8w), so the rows are read from the model.
    let hidden = || -> Vec<bool> {
        let session = host.session.borrow();
        let sheet_id = session.host().sheets()[0].id();
        (1..=6).map(|row| session.host().model().definitions().axis_layouts().get(sheet_id, calc_doc_model::Axis::Rows, row).hidden()).collect()
    };
    let filter = || {
        invoke_kind(
            &model,
            &range(&model, "A1:C6"),
            "AdvancedFilter",
            InvokeKind::Method,
            vec![AutoValue::I4(1), AutoValue::Object(range(&model, "E1:E2"))],
        )
    };
    let first = filter();
    assert!(matches!(first, Ok(AutoValue::Bool(true))), "{first:?}");
    assert_eq!(hidden(), [false; 6]);
    assert!(flag("FilterMode") && !flag("AutoFilterMode"));
    invoke_kind(&model, &range(&model, "E2"), "Value", InvokeKind::Put, vec![AutoValue::text(">=20")]).unwrap();
    assert!(matches!(filter(), Ok(AutoValue::Bool(true))));
    assert_eq!(hidden(), [false, true, false, true, false, true]);
    assert!(flag("FilterMode") && !flag("AutoFilterMode"));
    // The hidden rows are saved.
    let saved = saved_part(&mut host.session.borrow_mut(), "xl/worksheets/sheet");
    assert_eq!(saved.matches("hidden=\"1\"").count(), 3, "{saved}");
    invoke_kind(&model, &sheet, "ShowAllData", InvokeKind::Method, vec![]).unwrap();
    assert_eq!(hidden(), [false; 6]);
    assert!(!flag("FilterMode"));
    let again = invoke_kind(&model, &sheet, "ShowAllData", InvokeKind::Method, vec![]);
    assert!(matches!(again, Err(AutoError::Excel { number: 1004, .. })), "{again:?}");
}

/// dc-y7tqj (rw12 finding 22a): Application.MultiThreadedCalculation puts must enforce UDF
/// write-protection (1004 inside a worksheet function invocation, leaving state unchanged).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_y7tqj_multithreaded_calculation_puts_respect_udf_write_protection() {
    struct StubContext(CellKey);
    impl UserFunctionContext for StubContext {
        fn caller(&self) -> Option<CellKey> { Some(self.0) }
        fn refuse(&mut self, reason: &'static str) -> String { reason.into() }
        fn values_of(&mut self, _: &calc_value::ReferenceLike) -> Result<CalcValue, String> { Ok(CalcValue::empty()) }
        fn worksheet_function(&mut self, _: &str, _: &[CalcValue]) -> Result<CalcValue, String> { Ok(CalcValue::empty()) }
    }
    let (host, model) = setup();
    let sheet_id = host.session.borrow().host().sheets()[0].id();
    let cell = CellKey::new(sheet_id, 1, 1).unwrap();
    let mtc = obj(invoke(&model, &model.application(), "MultiThreadedCalculation", vec![]).unwrap());
    let mut ctx = StubContext(cell);
    let loan = Loan(RefCell::new(&mut ctx));
    INVOCATION.set(&loan, || {
        for (member, val) in [
            ("Enabled", AutoValue::Bool(false)),
            ("ThreadMode", AutoValue::I4(1)),
            ("ThreadCount", AutoValue::I4(2)),
        ] {
            let res = invoke_kind(&model, &mtc, member, InvokeKind::Put, vec![val]);
            assert!(
                matches!(res, Err(AutoError::Excel { number: 1004, .. })),
                "MultiThreadedCalculation.{member} put inside UDF must fail with 1004, got {res:?}"
            );
        }
    });
    assert!(matches!(invoke(&model, &mtc, "Enabled", vec![]).unwrap(), AutoValue::Bool(true)));
    assert!(matches!(invoke(&model, &mtc, "ThreadMode", vec![]).unwrap(), AutoValue::I4(0)));
}

/// dc-y7tqj (rw12 finding 22b): Application.CalculationState must return xlPending (2), not
/// xlDone (0), when a calculation is cancelled while dirty formulas remain.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_y7tqj_calculation_state_stays_pending_after_cancel() {
    let (host, model) = setup_session(
        HostSession::open(Arc::new(MemorySource::new(blank_package())), &OpenPolicy::default(), HostOptions::default()).unwrap(),
    );
    let sheet_id = host.session.borrow().host().sheets()[0].id();
    let at = |r, c| CellKey::new(sheet_id, r, c).unwrap();
    {
        let mut s = host.session.borrow_mut();
        s.dispatch(SkinIntent::SetCalcMode(CalcMode::Automatic));
        for (r, text) in [(1, "1"), (2, "=A1+1"), (3, "=A2+1"), (4, "=A3+1")] {
            assert!(s.dispatch(SkinIntent::EnterCell { at: at(r, 1), text: text.into() }).accepted());
        }
        while s.has_pending_calculation() {
            s.advance_calculation();
        }
        assert_eq!(s.host().calculation_state(), 0);
        // With budget=1, editing A1 leaves dirty formulas after one step; cancelling must keep xlPending (2).
        s.set_calculation_budget(calc_host::WorkBudget::evaluations(1));
        assert!(s.dispatch(SkinIntent::EnterCell { at: at(1, 1), text: "2".into() }).accepted());
        assert!(s.has_pending_calculation());
        s.cancel_calculation();
        assert!(!s.has_pending_calculation());
        assert_eq!(s.host().calculation_state(), 2, "cancelled calculation with dirty formulas must report xlPending (2)");
    }
    let state = invoke(&model, &model.application(), "CalculationState", vec![]).unwrap();
    assert!(matches!(state, AutoValue::I4(2)), "Application.CalculationState after cancel must be 2, got {state:?}");
}

/// dc-2rjye (Excel ww22): xlPasteAllMergingConditionalFormats (14) keeps the destination rule that overlaps the paste
/// and puts the translated source rule first; xlPasteAll trims the destination rule. Paste 21 is 1004. Blank on blank
/// stays empty for every Operation, with SkipBlanks either way.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_2rjye__paste_special_merges_conditional_formats_and_blank_on_blank_stays_empty() {
    let (host, model) = setup_session(
        HostSession::open(Arc::new(MemorySource::new(blank_package())), &OpenPolicy::default(), HostOptions::default()).unwrap(),
    );
    fill(&host, (1, 1), &[&["10"]]);
    let call = |address: &str, name: &str, args: Vec<AutoValue>| {
        invoke_kind(&model, &range(&model, address), name, InvokeKind::Method, args)
    };
    let add_rule = |address: &str, operator: i32, formula: &str| {
        let fcs = obj(invoke(&model, &range(&model, address), "FormatConditions", vec![]).unwrap());
        invoke_kind(
            &model,
            &fcs,
            "Add",
            InvokeKind::Method,
            vec![AutoValue::I4(1), AutoValue::I4(operator), AutoValue::text(formula)],
        )
        .unwrap();
    };
    add_rule("A1", 5, "1");
    add_rule("C1", 5, "2");
    let sheet_id = host.session.borrow().host().sheets()[0].id();
    let rules = || host.session.borrow().host().model().definitions().conditional_rules(sheet_id).to_vec();
    let at_c1 = CellKey::new(sheet_id, 1, 3).unwrap();
    call("A1", "Copy", vec![]).unwrap();
    call("C1", "PasteSpecial", vec![AutoValue::I4(14)]).unwrap();
    let merged = rules();
    assert_eq!(merged.len(), 3, "the source's own rule, the destination rule and the pasted rule: {merged:?}");
    let on_c1: Vec<_> = merged.iter().filter(|r| r.areas.iter().any(|a| a.contains(at_c1))).collect();
    assert_eq!(on_c1.len(), 2, "{merged:?}");
    assert!(on_c1[0].formulas == ["1"], "source rule first: {merged:?}");
    assert!(on_c1[1].formulas == ["2"], "destination rule kept: {merged:?}");
    // xlPasteAll replaces the destination's rule.
    call("C1", "PasteSpecial", vec![AutoValue::I4(-4104)]).unwrap();
    assert!(rules().iter().all(|r| r.formulas != ["2"]), "plain paste trims the destination rule: {:?}", rules());
    // xlPasteCommentsAndNotes (21) is a 1004.
    assert!(matches!(call("C1", "PasteSpecial", vec![AutoValue::I4(21)]), Err(AutoError::Excel { number: 1004, .. })));
    // Blank source onto blank destination stays empty for every Operation, SkipBlanks both ways.
    call("D1", "Copy", vec![]).unwrap();
    for op in [2, 3, 4, 5] {
        for skip in [false, true] {
            call("E1", "PasteSpecial", vec![AutoValue::I4(-4163), AutoValue::I4(op), AutoValue::Bool(skip)]).unwrap();
            assert_eq!(cell_text_of(&model, "E1"), "", "blank on blank stays empty on op {op}, skip {skip}");
        }
    }
}

/// dc-2rjye (Excel ww22): column widths tile whole copies of the source across the destination columns (a two-column
/// source over three columns fills one pair; a single column fills every destination column), and the hidden state is
/// copied.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_2rjye__paste_special_column_widths_tile_and_carry_hidden() {
    let (host, model) = setup_session(
        HostSession::open(Arc::new(MemorySource::new(blank_package())), &OpenPolicy::default(), HostOptions::default()).unwrap(),
    );
    let put = |address: &str, name: &str, value: AutoValue| {
        invoke_kind(&model, &range(&model, address), name, InvokeKind::Put, vec![value]).unwrap();
    };
    let call = |address: &str, name: &str, args: Vec<AutoValue>| {
        invoke_kind(&model, &range(&model, address), name, InvokeKind::Method, args)
    };
    let width = |address: &str| match invoke(&model, &range(&model, address), "ColumnWidth", vec![]).unwrap() {
        AutoValue::R8(n) => n,
        other => panic!("width {other:?}"),
    };
    put("A1", "ColumnWidth", AutoValue::I4(20));
    put("B1", "ColumnWidth", AutoValue::I4(30));
    call("A1:B1", "Copy", vec![]).unwrap();
    call("E1:H1", "PasteSpecial", vec![AutoValue::I4(8)]).unwrap();
    assert_eq!((width("E1"), width("F1"), width("G1"), width("H1")), (20.0, 30.0, 20.0, 30.0), "two pairs");
    call("J1:L1", "PasteSpecial", vec![AutoValue::I4(8)]).unwrap();
    assert_eq!((width("J1"), width("K1")), (20.0, 30.0), "one pair across three columns");
    assert_ne!(width("L1"), 20.0, "the third column is left alone");
    call("A1", "Copy", vec![]).unwrap();
    call("N1:P1", "PasteSpecial", vec![AutoValue::I4(8)]).unwrap();
    assert_eq!((width("N1"), width("O1"), width("P1")), (20.0, 20.0, 20.0), "a single column tiles");
    call("A1", "Copy", vec![]).unwrap();
    call("R1", "PasteSpecial", vec![AutoValue::I4(8), AutoValue::I4(-4142), AutoValue::Bool(true)]).unwrap();
    assert_eq!(width("R1"), 20.0, "SkipBlanks does not stop the widths");
    // Hidden state is copied.
    let sheet_id = host.session.borrow().host().sheets()[0].id();
    let axis = |column: u32| {
        host.session.borrow().host().model().definitions().axis_layouts().get(sheet_id, calc_doc_model::Axis::Columns, column)
    };
    let hidden = calc_doc_model::AxisFormat::new(Some(12.0), true, true).unwrap();
    let edit = calc_doc_model::AxisFormatEdit::new(sheet_id, calc_doc_model::Axis::Columns, 20, hidden).unwrap();
    assert!(host
        .session
        .borrow_mut()
        .dispatch(SkinIntent::Edit { edits: vec![calc_host::CellEdit::AxisFormat(edit)] })
        .accepted());
    call("T1", "Copy", vec![]).unwrap();
    call("V1", "PasteSpecial", vec![AutoValue::I4(8)]).unwrap();
    assert!(axis(22).hidden(), "the hidden state is copied");
    assert_eq!(axis(22).outline_level(), 0);
}

/// dc-2rjye (ww24 a/case16): column widths ignore Operation (all five) and Transpose, pasting no content.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_2rjye__paste_column_widths_ignores_operation_and_transpose() {
    let (_host, model) = setup_session(
        HostSession::open(Arc::new(MemorySource::new(blank_package())), &OpenPolicy::default(), HostOptions::default()).unwrap(),
    );
    let put = |address: &str, name: &str, value: AutoValue| {
        invoke_kind(&model, &range(&model, address), name, InvokeKind::Put, vec![value]).unwrap();
    };
    let call = |address: &str, name: &str, args: Vec<AutoValue>| {
        invoke_kind(&model, &range(&model, address), name, InvokeKind::Method, args)
    };
    let width = |address: &str| match invoke(&model, &range(&model, address), "ColumnWidth", vec![]).unwrap() {
        AutoValue::R8(n) => n,
        other => panic!("width {other:?}"),
    };
    put("A1", "Value", AutoValue::I4(7));
    put("A1", "ColumnWidth", AutoValue::I4(20));
    put("B1", "ColumnWidth", AutoValue::I4(30));
    call("A1:B2", "Copy", vec![]).unwrap();
    // xlPasteColumnWidths with Transpose:=True onto E5.
    call("E5", "PasteSpecial", vec![AutoValue::I4(8), AutoValue::I4(-4142), AutoValue::Bool(false), AutoValue::Bool(true)])
        .unwrap();
    assert_eq!((width("E5"), width("F5")), (20.0, 30.0), "widths untransposed");
    // Every Operation is ignored the same way.
    for (n, op) in [2, 3, 4, 5].iter().enumerate() {
        let col = ["H", "I", "J", "K"][n];
        call(&format!("{col}5"), "PasteSpecial", vec![AutoValue::I4(8), AutoValue::I4(*op)]).unwrap();
        assert_eq!(width(&format!("{col}5")), 20.0, "operation {op}");
    }
    let value = invoke(&model, &range(&model, "E5"), "Value2", vec![]).unwrap();
    assert!(matches!(value, AutoValue::Empty), "no cell content: {value:?}");
}

/// dc-y7tqj (rw12 finding 24c): Worksheet.Names validates deleted sheet, Name.Parent returns the
/// Worksheet for a sheet-scoped name (Workbook for global), and Sheet1.Names.Add("Sheet2!X", ...)
/// refuses typed instead of escaping scope.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_y7tqj_worksheet_names_parent_scope_and_deleted_sheet() {
    let (_host, model) = setup();
    let book = obj(invoke(&model, &model.application(), "ActiveWorkbook", vec![]).unwrap());
    let sheets = obj(invoke(&model, &book, "Worksheets", vec![]).unwrap());
    let s1 = obj(invoke_kind(&model, &sheets, "Item", InvokeKind::Get, vec![AutoValue::I4(1)]).unwrap());
    let s2 = obj(invoke_kind(&model, &sheets, "Add", InvokeKind::Method, vec![]).unwrap());
    let s1_name = text(&model, &s1, "Name");
    let s2_name = text(&model, &s2, "Name");
    let s1_names = obj(invoke(&model, &s1, "Names", vec![]).unwrap());
    let s2_names = obj(invoke(&model, &s2, "Names", vec![]).unwrap());
    let book_names = obj(invoke(&model, &book, "Names", vec![]).unwrap());

    let local = obj(
        invoke_kind(&model, &s1_names, "Add", InvokeKind::Method, vec![AutoValue::text("Loc"), AutoValue::text("=1")])
            .unwrap(),
    );
    let global = obj(
        invoke_kind(&model, &book_names, "Add", InvokeKind::Method, vec![AutoValue::text("Glob"), AutoValue::text("=2")])
            .unwrap(),
    );
    let local_parent = obj(invoke(&model, &local, "Parent", vec![]).unwrap());
    assert_eq!(local_parent.key, s1.key, "sheet-scoped Name.Parent must be Worksheet");
    assert_eq!(text(&model, &local_parent, "Name"), s1_name);
    let global_parent = obj(invoke(&model, &global, "Parent", vec![]).unwrap());
    assert_eq!(global_parent.key, book.key, "workbook-scoped Name.Parent must be Workbook");

    let escape = invoke_kind(
        &model,
        &s1_names,
        "Add",
        InvokeKind::Method,
        vec![AutoValue::text(&format!("{s2_name}!Escaped")), AutoValue::text("=3")],
    );
    assert!(
        matches!(&escape, Err(AutoError::NotModelled { owner, .. }) if *owner == "dc-oxh8w.43"),
        "Sheet1.Names.Add with Sheet2! prefix must refuse typed, got {escape:?}"
    );
    // Deleting Sheet2 must invalidate the held Sheet2.Names collection.
    invoke_kind(&model, &s2, "Delete", InvokeKind::Method, vec![]).unwrap();
    let after_del = invoke(&model, &s2_names, "Count", vec![]);
    assert!(after_del.is_err(), "deleted sheet's Names collection must error, got {after_del:?}");
}

/// dc-y7tqj (rw12 finding 24d/24e/24f/24g): CutCopyMode accepts 0.0/Empty and refuses True/1/2
/// typed; Range.Activate refuses multi-cell ranges; InchesToPoints rejects NaN/inf; Application.Goto
/// tracks row/column shifts via StoredReference and refuses foreign-workbook ranges.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_y7tqj_cut_copy_activate_inches_and_goto_edges() {
    let (_host, model) = setup();
    let app = model.application();
    // 24d: CutCopyMode = 0.0 / Empty clears; True / 1 / 2 refuse typed citing dc-oxh8w.49.
    for clear_val in [AutoValue::R8(0.0), AutoValue::Empty] {
        invoke_kind(&model, &range(&model, "A1"), "Copy", InvokeKind::Method, vec![]).unwrap();
        assert!(matches!(invoke(&model, &app, "CutCopyMode", vec![]).unwrap(), AutoValue::I4(1)));
        invoke_kind(&model, &app, "CutCopyMode", InvokeKind::Put, vec![clear_val.clone()])
            .unwrap_or_else(|e| panic!("CutCopyMode = {clear_val:?} must clear: {e:?}"));
        assert!(matches!(invoke(&model, &app, "CutCopyMode", vec![]).unwrap(), AutoValue::Bool(false)));
    }
    // ww22 (Excel 16.0.20625): True / 1 / 2 clear the marked copy as well (the refusal here was a guess).
    for val in [AutoValue::Bool(true), AutoValue::I4(1), AutoValue::I4(2)] {
        invoke_kind(&model, &range(&model, "A1"), "Copy", InvokeKind::Method, vec![]).unwrap();
        invoke_kind(&model, &app, "CutCopyMode", InvokeKind::Put, vec![val.clone()]).unwrap();
        assert!(matches!(invoke(&model, &app, "CutCopyMode", vec![]).unwrap(), AutoValue::Bool(false)), "{val:?}");
    }

    // 24e: Range.Activate of B2:D5 with A1:B2 selected keeps the selection and makes B2 active (ww22).
    invoke_kind(&model, &range(&model, "A1:B2"), "Select", InvokeKind::Method, vec![]).unwrap();
    invoke_kind(&model, &range(&model, "B2:D5"), "Activate", InvokeKind::Method, vec![]).unwrap();
    assert_eq!(text(&model, &obj(invoke(&model, &app, "Selection", vec![]).unwrap()), "Address"), "$A$1:$B$2");
    assert_eq!(text(&model, &obj(invoke(&model, &app, "ActiveCell", vec![]).unwrap()), "Address"), "$B$2");

    // 24f: InchesToPoints rejects "NaN", "inf", "-inf" with DISP_E_TYPEMISMATCH.
    for bad in ["NaN", "inf", "-inf", "Infinity"] {
        let res = invoke_kind(&model, &app, "InchesToPoints", InvokeKind::Method, vec![AutoValue::text(bad)]);
        assert!(
            matches!(res, Err(AutoError::Status(s)) if s == 0x8002_0005u32 as i32),
            "InchesToPoints({bad:?}) must return DISP_E_TYPEMISMATCH, got {res:?}"
        );
    }

    // 24g: Goto tracks row insertion shifts via StoredReference and refuses foreign-workbook ranges.
    invoke_kind(&model, &app, "Goto", InvokeKind::Method, vec![AutoValue::Object(range(&model, "C5"))]).unwrap();
    let row1 = obj(invoke(&model, &range(&model, "A1"), "EntireRow", vec![]).unwrap());
    invoke_kind(&model, &row1, "Insert", InvokeKind::Method, vec![]).unwrap();
    invoke_kind(&model, &range(&model, "A1"), "Select", InvokeKind::Method, vec![]).unwrap();
    invoke_kind(&model, &app, "Goto", InvokeKind::Method, vec![]).unwrap();
    let sel = text(&model, &obj(invoke(&model, &app, "Selection", vec![]).unwrap()), "Address");
    assert_eq!(sel, "$C$6", "Goto without Reference must track inserted row shift from C5 to C6");

    let (_other_host, other_model) = setup();
    let foreign_range = range(&other_model, "B2");
    let foreign_goto = invoke_kind(&model, &app, "Goto", InvokeKind::Method, vec![AutoValue::Object(foreign_range)]);
    assert!(
        matches!(&foreign_goto, Err(AutoError::NotModelled { owner, .. }) if *owner == "dc-oxh8w.49"),
        "Goto with foreign-workbook range must refuse typed citing dc-oxh8w.49, got {foreign_goto:?}"
    );
}

/// dc-y7tqj (rw12 finding 24h): Series.Border.ColorIndex = -4105 (and ChartArea.Border.ColorIndex =
/// -4105) must read back as -4105 (xlColorIndexAutomatic).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_y7tqj_series_border_color_index_automatic_reads_back() {
    let (_host, model, chart, series) = labelled_chart();
    let border = obj(invoke(&model, &series, "Border", vec![]).unwrap());
    invoke_kind(&model, &border, "ColorIndex", InvokeKind::Put, vec![AutoValue::I4(5)]).unwrap();
    invoke_kind(&model, &border, "ColorIndex", InvokeKind::Put, vec![AutoValue::I4(-4105)]).unwrap();
    let got = invoke(&model, &border, "ColorIndex", vec![])
        .expect("Series.Border.ColorIndex = -4105 must read back");
    assert_eq!(int(got), -4105);
    let area = obj(invoke(&model, &obj(invoke(&model, &chart, "ChartArea", vec![]).unwrap()), "Border", vec![]).unwrap());
    invoke_kind(&model, &area, "ColorIndex", InvokeKind::Put, vec![AutoValue::I4(3)]).unwrap();
    invoke_kind(&model, &area, "ColorIndex", InvokeKind::Put, vec![AutoValue::I4(-4105)]).unwrap();
    assert_eq!(int(invoke(&model, &area, "ColorIndex", vec![]).unwrap()), -4105);
}

/// dc-y7tqj (rw12 finding 24i): Chart.Location moving from a chart sheet onto a worksheet must roll
/// back ChartMove if removing the vacated chart sheet fails (e.g. on a macro-enabled .xlsm).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_y7tqj_chart_location_rolls_back_when_chart_sheet_removal_fails() {
    let (host, model) = setup_session(
        HostSession::open(
            Arc::new(MemorySource::new(
                include_bytes!("../../calc-doc/tests/fixtures/vba_udf_names/local.xlsm").to_vec(),
            )),
            &OpenPolicy::default(),
            HostOptions::default(),
        )
        .unwrap(),
    );
    let book = obj(invoke(&model, &model.application(), "ActiveWorkbook", vec![]).unwrap());
    let charts = obj(invoke(&model, &book, "Charts", vec![]).unwrap());
    let chart = obj(invoke_kind(&model, &charts, "Add", InvokeKind::Method, vec![]).unwrap());
    let sheet1 = obj(
        invoke_kind(
            &model,
            &obj(invoke(&model, &book, "Worksheets", vec![]).unwrap()),
            "Item",
            InvokeKind::Get,
            vec![AutoValue::I4(1)],
        )
        .unwrap(),
    );
    let sheet1_name = text(&model, &sheet1, "Name");
    let moved = invoke_kind(
        &model,
        &chart,
        "Location",
        InvokeKind::Method,
        vec![AutoValue::I4(2), AutoValue::text(&sheet1_name)],
    );
    assert!(moved.is_err(), "removing a chart sheet in an .xlsm workbook must fail");
    let sheet1_charts = obj(invoke_kind(&model, &sheet1, "ChartObjects", InvokeKind::Method, vec![]).unwrap());
    assert_eq!(
        int(invoke(&model, &sheet1_charts, "Count", vec![]).unwrap()),
        0,
        "failed Chart.Location must roll back ChartMove so Sheet1 gets no leaked chart object"
    );
    // And the chart sheet itself must still hold its chart (not left empty/vacated).
    let session = host.session.borrow();
    let chart_sheet_id = session
        .host()
        .model()
        .sheets()
        .iter()
        .find(|s| s.kind() == calc_doc_model::SheetKind::Chart)
        .expect("chart sheet remains")
        .id();
    assert_eq!(
        session.host().model().definitions().charts(chart_sheet_id).len(),
        1,
        "chart sheet must retain its chart after Chart.Location rollback"
    );
}

/// rw13 finding 5 (dc-v55ku): a multi-cell Copy/Cut/Paste onto a one-cell unlocked destination writes the whole
/// rectangle the source's size gives, so its locked neighbours meet the protection too.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_v55ku__a_one_cell_destination_guards_the_whole_rectangle_written() {
    let (_host, model, _) = blank_report();
    let sheets = worksheets(&model);
    let sheet = obj(invoke(&model, &sheets, "Item", vec![AutoValue::I4(1)]).unwrap());
    let method = |target: &ObjectRef, name: &str, args: Vec<AutoValue>| invoke_kind(&model, target, name, InvokeKind::Method, args);
    let put = |target: &ObjectRef, name: &str, args: Vec<AutoValue>| invoke_kind(&model, target, name, InvokeKind::Put, args);
    let protected_error = |r: &Reply| matches!(r, Err(AutoError::Excel { number: 1004, description }) if description.contains("protected sheet"));
    for (address, value) in [("A1", 1.0), ("A2", 2.0), ("B3", 9.0)] {
        put(&range(&model, address), "Value", vec![AutoValue::R8(value)]).unwrap();
    }
    put(&range(&model, "B2"), "Locked", vec![AutoValue::Bool(false)]).unwrap();
    method(&sheet, "Protect", vec![]).unwrap();
    let a = |name: &str| AutoValue::Object(range(&model, name));
    let r = method(&range(&model, "A1:A2"), "Copy", vec![a("B2")]);
    assert!(protected_error(&r), "Copy of two cells onto B2 reaches locked B3: {r:?}");
    assert!(matches!(invoke(&model, &range(&model, "B3"), "Value", vec![]).unwrap(), AutoValue::R8(n) if n == 9.0), "B3 was written");
    assert!(matches!(invoke(&model, &range(&model, "B2"), "Value", vec![]).unwrap(), AutoValue::Empty), "B2 was written");
    method(&range(&model, "A1:A2"), "Copy", vec![]).unwrap();
    let r = method(&range(&model, "B2"), "PasteSpecial", vec![]);
    assert!(protected_error(&r), "Paste of two cells onto B2 reaches locked B3: {r:?}");
    assert!(matches!(invoke(&model, &range(&model, "B3"), "Value", vec![]).unwrap(), AutoValue::R8(n) if n == 9.0), "B3 was pasted");
    // Transposed, the two cells make a row: B2:C2 reaches locked C2.
    let r = method(&range(&model, "B2"), "PasteSpecial", vec![AutoValue::I4(-4104), AutoValue::I4(-4142), AutoValue::Bool(false), AutoValue::Bool(true)]);
    assert!(protected_error(&r), "Transposed paste reaches locked C2: {r:?}");
}

/// dc-5oumk (ww22 protect/observations.jsonl, Excel 16.0.20625.20108): on a sheet protected for contents ListObjects.Add
/// is error 3000 and ListObject.Delete 1004 ("Table features aren't available because the sheet is protected."), Unlist
/// works, AutoFilterMode = False is 1004 unless AllowFiltering is set, ShowAllData is 1004 unless UserInterfaceOnly,
/// and Range.Insert / Range.Delete of cells whose shift reaches locked cells is 1004 unless UserInterfaceOnly. A
/// refused call changes nothing.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_5oumk__table_filter_and_insert_on_a_protected_sheet() {
    let table_text = "Table features aren't available because the sheet is protected.";
    let excel = |r: &Reply, n: i32, text: &str| matches!(r, Err(AutoError::Excel { number, description }) if *number == n && description == text);
    // mode: 0 = Protect(), 1 = every Allow* flag, 2 = UserInterfaceOnly.
    for mode in 0..3 {
        let (_host, model, _) = blank_report();
        let sheets = worksheets(&model);
        let sheet = obj(invoke(&model, &sheets, "Item", vec![AutoValue::I4(1)]).unwrap());
        let put = |target: &ObjectRef, name: &str, args: Vec<AutoValue>| invoke_kind(&model, target, name, InvokeKind::Put, args);
        let method = |target: &ObjectRef, name: &str, args: Vec<AutoValue>| invoke_kind(&model, target, name, InvokeKind::Method, args);
        for (address, value) in [("A20", "Name"), ("B20", "Qty")] {
            put(&range(&model, address), "Value", vec![AutoValue::text(value)]).unwrap();
        }
        for (address, value) in [("A21", 1.0), ("B21", 5.0), ("A22", 2.0), ("B22", 15.0), ("D12", 99.0)] {
            put(&range(&model, address), "Value", vec![AutoValue::R8(value)]).unwrap();
        }
        put(&range(&model, "D11"), "Locked", vec![AutoValue::Bool(false)]).unwrap();
        let tables = obj(invoke(&model, &sheet, "ListObjects", vec![]).unwrap());
        let add = vec![AutoValue::I4(1), AutoValue::Object(range(&model, "A20:B22")), AutoValue::Missing, AutoValue::I4(1)];
        invoke_kind(&model, &tables, "Add", InvokeKind::Method, add.clone()).unwrap();
        let table = obj(invoke(&model, &tables, "Item", vec![AutoValue::I4(1)]).unwrap());
        for (address, value) in [("F1", AutoValue::text("h")), ("F2", AutoValue::R8(5.0)), ("F3", AutoValue::R8(15.0))] {
            put(&range(&model, address), "Value", vec![value]).unwrap();
        }
        method(&range(&model, "F1:F3"), "AutoFilter", vec![AutoValue::I4(1), AutoValue::text(">10")]).unwrap();
        let mut args = vec![AutoValue::Missing, AutoValue::Bool(true), AutoValue::Bool(true), AutoValue::Bool(true), AutoValue::Bool(mode == 2)];
        args.extend(std::iter::repeat_n(AutoValue::Bool(mode == 1), 11));
        method(&sheet, "Protect", args).unwrap();
        let tag = format!("mode {mode}");
        let r = invoke_kind(&model, &tables, "Add", InvokeKind::Method, add);
        assert!(excel(&r, 3000, table_text), "{tag} ListObjects.Add: {r:?}");
        let r = method(&table, "Delete", vec![]);
        assert!(excel(&r, 1004, table_text), "{tag} ListObject.Delete: {r:?}");
        assert_eq!(int(invoke(&model, &tables, "Count", vec![]).unwrap()), 1, "{tag} the table was deleted");
        let r = method(&sheet, "ShowAllData", vec![]);
        if mode == 0 || mode == 1 {
            assert!(excel(&r, 1004, "Unable to get the ShowAllData property of the Worksheet class"), "{tag} {r:?}");
        } else {
            assert!(r.is_ok(), "{tag} ShowAllData under UserInterfaceOnly: {r:?}");
        }
        let r = put(&sheet, "AutoFilterMode", vec![AutoValue::Bool(false)]);
        let removed = matches!(invoke(&model, &sheet, "AutoFilterMode", vec![]).unwrap(), AutoValue::Bool(false));
        if mode == 1 {
            assert!(r.is_ok() && removed, "{tag} AutoFilterMode = False with AllowFiltering: {r:?}");
        } else {
            assert!(excel(&r, 1004, "Unable to set the AutoFilterMode property of the Worksheet class"), "{tag} {r:?}");
            assert!(!removed, "{tag} the filter was removed");
        }
        let r = method(&range(&model, "D11"), "Insert", vec![AutoValue::I4(-4121)]);
        if mode == 2 {
            assert!(r.is_ok(), "{tag} Insert under UserInterfaceOnly: {r:?}");
            assert!(matches!(invoke(&model, &range(&model, "D13"), "Value", vec![]).unwrap(), AutoValue::R8(n) if n == 99.0), "{tag} D12 did not shift");
        } else {
            assert!(excel(&r, 1004, "Unable to get the Insert property of the Range class"), "{tag} Range.Insert: {r:?}");
            assert!(matches!(invoke(&model, &range(&model, "D12"), "Value", vec![]).unwrap(), AutoValue::R8(n) if n == 99.0), "{tag} D12 shifted");
        }
        let r = method(&range(&model, "D11"), "Delete", vec![AutoValue::I4(-4162)]);
        if mode != 2 {
            assert!(excel(&r, 1004, "Unable to get the Delete property of the Range class"), "{tag} Range.Delete: {r:?}");
        }
        // Unlist works in every mode and leaves the cells.
        let r = method(&table, "Unlist", vec![]);
        assert!(r.is_ok(), "{tag} Unlist: {r:?}");
        assert_eq!(int(invoke(&model, &tables, "Count", vec![]).unwrap()), 0, "{tag} the table stayed");
    }
}
