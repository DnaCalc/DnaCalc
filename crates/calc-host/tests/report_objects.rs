//! dc-piw0 / dc-zjsg.1: what a macro builds, at the session's command level: a data table, a chart
//! over it, a title, two shapes joined by a connector, then save, reopen, list. The object-model
//! members that reach these commands are in `automation/report.rs`; their tests are `#[ignore]`d
//! until the Windows descriptor regeneration (RUN-ON-WINDOWS item 1) puts ListObject, Chart and
//! Shape into the descriptor.

#![allow(non_snake_case)]

use calc_doc_model::{ShapeColour, ShapeConnection, ShapeFill, ShapeKind, ShapeLine};
use calc_host::value::{CellKey, GridArea, SheetId};
use calc_host::{
    HostOptions, HostSession, MemorySource, OpenPolicy, Points, SavePolicy, ShapeChange,
    SkinIntent, TableChange, WorkbookCommand,
};
use std::sync::Arc;

fn open(bytes: Vec<u8>) -> HostSession {
    let source: Arc<dyn calc_host::PackageRead> = Arc::new(MemorySource::new(bytes));
    let mut session = HostSession::open(source, &OpenPolicy::default(), HostOptions::default())
        .expect("it opens");
    assert!(session.dispatch(SkinIntent::Recalculate).accepted());
    session
}

fn at(sheet: SheetId, address: &str) -> CellKey {
    let split = address.find(|c: char| c.is_ascii_digit()).unwrap();
    let (letters, digits) = address.split_at(split);
    let column = letters
        .bytes()
        .fold(0u32, |n, b| n * 26 + u32::from(b - b'A' + 1));
    CellKey::new(sheet, digits.parse().unwrap(), column).unwrap()
}

fn enter(book: &mut HostSession, sheet: SheetId, address: &str, text: &str) {
    assert!(
        book.dispatch(SkinIntent::EnterCell {
            at: at(sheet, address),
            text: text.into(),
        })
        .accepted(),
        "{address}"
    );
}

fn command(book: &mut HostSession, make: impl FnOnce(&HostSession) -> WorkbookCommand) {
    let command = make(book);
    let receipt = book.dispatch(SkinIntent::Command(command));
    assert!(receipt.accepted(), "{:?}", receipt.error);
}

fn shape(book: &mut HostSession, sheet: SheetId, change: ShapeChange) {
    command(book, |b| WorkbookCommand::Shape {
        sheet,
        revision: b.host().revision().get(),
        change,
    });
}

fn pts(v: f64) -> Points {
    Points::new(v)
}

fn reopened(book: &mut HostSession) -> HostSession {
    let mut bytes = Vec::new();
    book.save(&SavePolicy::default(), &mut bytes).unwrap();
    if let Ok(path) = std::env::var("ENG1_DUMP") {
        std::fs::write(path, &bytes).unwrap();
    }
    open(bytes)
}

fn add(kind: ShapeKind, geometry: &str, l: f64, t: f64, w: f64, h: f64) -> ShapeChange {
    ShapeChange::Add {
        kind,
        geometry: geometry.into(),
        left: pts(l),
        top: pts(t),
        width: pts(w),
        height: pts(h),
        name: None,
    }
}

/// The macro's drawing steps: AddShape twice, AddTextbox, a connector glued to both shapes,
/// text and colours; then save and reopen and list the objects.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_piw0_3__shapes_and_a_connector_save_and_reload() {
    let mut book = open(calc_host::blank_package());
    let sheet = book.host().sheets()[0].id();
    shape(
        &mut book,
        sheet,
        add(ShapeKind::Shape, "rect", 20.0, 120.0, 100.0, 50.0),
    );
    shape(
        &mut book,
        sheet,
        add(ShapeKind::Shape, "ellipse", 20.0, 220.0, 100.0, 50.0),
    );
    shape(
        &mut book,
        sheet,
        add(ShapeKind::TextBox, "rect", 160.0, 240.0, 120.0, 30.0),
    );
    shape(
        &mut book,
        sheet,
        add(
            ShapeKind::Connector,
            "straightConnector1",
            0.0,
            0.0,
            10.0,
            10.0,
        ),
    );
    let ids: Vec<(u32, String)> = book
        .host()
        .shapes(sheet)
        .iter()
        .map(|s| (s.id, s.name.clone()))
        .collect();
    // Interim default names (excel-questions.md Q1): kind plus id - 1, ids from 2.
    assert_eq!(
        ids,
        [
            (2, "Rectangle 1".to_owned()),
            (3, "Oval 2".to_owned()),
            (4, "TextBox 3".to_owned()),
            (5, "Straight Arrow Connector 4".to_owned()),
        ]
    );
    shape(
        &mut book,
        sheet,
        ShapeChange::Connect {
            id: 5,
            begin: true,
            shape: 2,
            site: 3,
        },
    );
    shape(
        &mut book,
        sheet,
        ShapeChange::Connect {
            id: 5,
            begin: false,
            shape: 3,
            site: 1,
        },
    );
    shape(
        &mut book,
        sheet,
        ShapeChange::Text {
            id: 2,
            text: "Start".into(),
        },
    );
    shape(
        &mut book,
        sheet,
        ShapeChange::Text {
            id: 4,
            text: "line one\r\nline two & <more>".into(),
        },
    );
    shape(
        &mut book,
        sheet,
        ShapeChange::Fill {
            id: 2,
            fill: ShapeFill::Solid(ShapeColour::Rgb(0xFF8000)),
        },
    );
    shape(
        &mut book,
        sheet,
        ShapeChange::Line {
            id: 2,
            line: ShapeLine {
                weight: Some(25400),
                colour: Some(ShapeColour::Rgb(0)),
                ..Default::default()
            },
        },
    );
    shape(
        &mut book,
        sheet,
        ShapeChange::Name {
            id: 3,
            name: "End".into(),
        },
    );
    // The connector joins the bottom of the rectangle (5 points below its box) to the top of the oval.
    let connector = book.host().shapes(sheet)[3].clone();
    assert_eq!(connector.begin, Some(ShapeConnection { shape: 2, site: 2 }));
    assert_eq!(connector.end, Some(ShapeConnection { shape: 3, site: 0 }));
    let (l, t, w, h) = book.host().shape_rectangle(sheet, &connector);
    assert_eq!((l, t, w, h), (70.0, 170.0, 0.0, 50.0));

    let reloaded = reopened(&mut book);
    let read = reloaded.host().shapes(sheet);
    assert_eq!(read.len(), 4);
    assert_eq!(read[0].fill, ShapeFill::Solid(ShapeColour::Rgb(0xFF8000)));
    assert_eq!(read[0].line.weight, Some(25400));
    assert_eq!(read[0].text.as_deref(), Some(&["Start".to_owned()][..]));
    assert_eq!(read[1].name, "End");
    assert_eq!(read[1].geometry.as_deref(), Some("ellipse"));
    assert_eq!(read[2].kind, ShapeKind::TextBox);
    assert_eq!(
        read[2].text.as_deref(),
        Some(&["line one".to_owned(), "line two & <more>".to_owned()][..])
    );
    assert_eq!(read[3].begin, Some(ShapeConnection { shape: 2, site: 2 }));
    assert_eq!(read[3].end, Some(ShapeConnection { shape: 3, site: 0 }));
    // Positions survive in points.
    assert_eq!(
        reloaded.host().shape_rectangle(sheet, &read[0]),
        (20.0, 120.0, 100.0, 50.0)
    );
    // Delete: the glued connector stays, unglued.
    let mut reloaded = reloaded;
    // Objects the file holds are not changed here (dc-zjsg.4): refused, nothing changes.
    let revision = reloaded.host().revision().get();
    let receipt = reloaded.dispatch(SkinIntent::Command(WorkbookCommand::Shape {
        sheet,
        revision,
        change: ShapeChange::Delete { id: 2 },
    }));
    assert!(!receipt.accepted());
    assert!(format!("{:?}", receipt.error.unwrap()).contains("dc-zjsg.4"));
    assert_eq!(reloaded.host().shapes(sheet).len(), 4);
    // A shape added in the session can be deleted again, and undo restores it.
    shape(&mut book, sheet, ShapeChange::Delete { id: 2 });
    assert_eq!(book.host().shapes(sheet).len(), 3);
    assert_eq!(book.host().shapes(sheet)[2].begin, None);
    assert!(book.dispatch(SkinIntent::Undo).accepted());
    assert_eq!(book.host().shapes(sheet).len(), 4);
}

/// Fill a range, add the table, a chart over it with a title, two shapes and a connector, save,
/// reload, list: the macro-built report at the command level.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_piw0__table_chart_and_shapes_in_one_report() {
    let mut book = open(calc_host::blank_package());
    let sheet = book.host().sheets()[0].id();
    for (cell, text) in [
        ("A1", "Region"),
        ("B1", "Q1"),
        ("C1", "Q2"),
        ("A2", "North"),
        ("B2", "10"),
        ("C2", "20"),
        ("A3", "South"),
        ("B3", "30"),
        ("C3", "40"),
    ] {
        enter(&mut book, sheet, cell, text);
    }
    let area = GridArea::new(at(sheet, "A1"), at(sheet, "C3")).unwrap();
    command(&mut book, |b| WorkbookCommand::Table {
        revision: b.host().revision().get(),
        change: TableChange::Create {
            area,
            name: "Sales".into(),
        },
    });
    command(&mut book, |b| WorkbookCommand::ChartCreate {
        revision: b.host().revision().get(),
        area,
        kind: "column".into(),
        rows: false,
        headers: true,
        categories: true,
    });
    command(&mut book, |b| WorkbookCommand::ChartProperties {
        sheet,
        revision: b.host().revision().get(),
        part: b.host().charts(sheet).0[0].part.clone(),
        title: Some("Sales".into()),
        set_title: true,
        legend: Some(true),
    });
    shape(
        &mut book,
        sheet,
        add(ShapeKind::Shape, "rect", 20.0, 200.0, 100.0, 50.0),
    );
    shape(
        &mut book,
        sheet,
        add(ShapeKind::Shape, "ellipse", 20.0, 300.0, 100.0, 50.0),
    );
    shape(
        &mut book,
        sheet,
        add(ShapeKind::Connector, "line", 0.0, 0.0, 10.0, 10.0),
    );
    let [first, second, connector] = [0, 1, 2].map(|i| book.host().shapes(sheet)[i].id);
    shape(
        &mut book,
        sheet,
        ShapeChange::Connect {
            id: connector,
            begin: true,
            shape: first,
            site: 3,
        },
    );
    shape(
        &mut book,
        sheet,
        ShapeChange::Connect {
            id: connector,
            begin: false,
            shape: second,
            site: 1,
        },
    );

    let reloaded = reopened(&mut book);
    let tables = reloaded.host().model().definitions().tables();
    assert_eq!(tables.len(), 1);
    assert_eq!(
        (tables[0].name.as_str(), tables[0].columns.clone()),
        ("Sales", vec!["Region".into(), "Q1".into(), "Q2".into()])
    );
    let (charts, unread) = reloaded.host().charts(sheet);
    assert_eq!((charts.len(), unread), (1, 0));
    assert_eq!(charts[0].title.as_deref(), Some("Sales"));
    assert_eq!(charts[0].plots[0].series.len(), 2);
    let shapes = reloaded.host().shapes(sheet);
    assert_eq!(shapes.len(), 3);
    // Chart frame and shapes share one drawing; ids stay distinct.
    let mut ids: Vec<u32> = shapes.iter().map(|s| s.id).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), 3);
    assert_eq!(shapes[2].begin.map(|c| c.shape), Some(shapes[0].id));
    assert_eq!(shapes[2].end.map(|c| c.shape), Some(shapes[1].id));
}
