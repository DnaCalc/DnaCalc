//! dc-piw0 / dc-zjsg.1: what a macro builds, at the session's command level: a data table, a chart
//! over it, a title, two shapes joined by a connector, then save, reopen, list. The object-model
//! members that reach these commands are in `automation/report.rs`; their tests are in
//! `automation.rs` (`dc_piw0__*`).

#![allow(non_snake_case)]

use calc_doc_model::{ShapeColour, ShapeConnection, ShapeFill, ShapeKind, ShapeLine};
use calc_host::value::{CellKey, GridArea, SheetId};
use calc_host::{
    HostOptions, HostSession, MemorySource, OpenPolicy, Points, SavePolicy, ShapeChange, ShapeOrder,
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
        flips: (false, false),
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
    // A stored shape a stored connector is glued to is not deleted (dc-zjsg.5): refused, nothing changes.
    let revision = reloaded.host().revision().get();
    let receipt = reloaded.dispatch(SkinIntent::Command(WorkbookCommand::Shape {
        sheet,
        revision,
        change: ShapeChange::Delete { id: 2 },
    }));
    assert!(!receipt.accepted());
    assert!(format!("{:?}", receipt.error.unwrap()).contains("dc-zjsg.5"));
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
    // ChartObject.Name and .Left/.Top/.Width/.Height in points.
    command(&mut book, |b| WorkbookCommand::ChartFrame {
        sheet,
        revision: b.host().revision().get(),
        part: b.host().charts(sheet).0[0].part.clone(),
        name: Some("SalesChart".into()),
        place: Some([pts(200.0), pts(20.0), pts(300.0), pts(200.0)]),
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
    assert_eq!(charts[0].name, "SalesChart");
    let chart = &reloaded.host().model().definitions().charts(sheet)[0];
    assert_eq!(
        chart.anchor,
        // Excel writes a macro's chart as a two-cell anchor (excel-answers.md Q0); the position
        // 200, 20 and the size 300 x 200 points fall in these cells (48 pt columns, 15 pt rows).
        calc_doc_model::ChartAnchor::TwoCell {
            from: calc_doc_model::DrawingPoint {
                column: 4,
                column_offset: 101_600,
                row: 1,
                row_offset: 63_500,
            },
            to: calc_doc_model::DrawingPoint {
                column: 10,
                column_offset: 254_000,
                row: 14,
                row_offset: 127_000,
            },
        }
    );
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

/// ListObject.Name put, ListRows.Add and ListColumns(i).Name put at the command level: the same
/// table commands the user interface runs.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_piw0_1__rename_add_row_and_rename_column() {
    let mut book = open(calc_host::blank_package());
    let sheet = book.host().sheets()[0].id();
    for (cell, text) in [
        ("A1", "Region"),
        ("B1", "Q1"),
        ("A2", "North"),
        ("B2", "10"),
        ("A3", "South"),
        ("B3", "30"),
    ] {
        enter(&mut book, sheet, cell, text);
    }
    let area = GridArea::new(at(sheet, "A1"), at(sheet, "B3")).unwrap();
    command(&mut book, |b| WorkbookCommand::Table {
        revision: b.host().revision().get(),
        change: TableChange::Create {
            area,
            name: "Table1".into(),
        },
    });
    let part = book.host().model().definitions().tables()[0].part.clone();
    // ListObject.Name = "Sales"
    command(&mut book, |b| WorkbookCommand::Table {
        revision: b.host().revision().get(),
        change: TableChange::Rename {
            part: part.clone(),
            name: "Sales".into(),
        },
    });
    assert_eq!(
        book.host().model().definitions().tables()[0].display_name,
        "Sales"
    );
    // ListRows.Add: the table grows by one data row.
    let grown = GridArea::new(at(sheet, "A1"), at(sheet, "B4")).unwrap();
    command(&mut book, |b| WorkbookCommand::Table {
        revision: b.host().revision().get(),
        change: TableChange::Resize {
            part: part.clone(),
            area: grown,
        },
    });
    assert_eq!(book.host().model().definitions().tables()[0].area, grown);
    // ListColumns(2).Name = "Sales Q1"
    command(&mut book, |b| WorkbookCommand::Table {
        revision: b.host().revision().get(),
        change: TableChange::RenameColumn {
            part: part.clone(),
            column: 2,
            name: "Q1 total".into(),
        },
    });
    let reloaded = reopened(&mut book);
    let table = &reloaded.host().model().definitions().tables()[0];
    assert_eq!((table.display_name.as_str(), table.area), ("Sales", grown));
    assert_eq!(table.columns, ["Region", "Q1 total"]);

    // A formula that uses the table's name: Excel rewrites it on rename; here it is refused and
    // nothing changes.
    enter(&mut book, sheet, "D1", "=COUNT(Sales[Q1 total])");
    let before = book.host().revision().get();
    let receipt = book.dispatch(SkinIntent::Command(WorkbookCommand::Table {
        revision: before,
        change: TableChange::Rename {
            part,
            name: "Other".into(),
        },
    }));
    assert!(!receipt.accepted());
    assert_eq!(book.host().revision().get(), before);
    assert_eq!(
        book.host().model().definitions().tables()[0].display_name,
        "Sales"
    );
}

/// dc-piw0.2: `ChartObjects.Add` without `SetSourceData` saves an empty chart, as Excel does
/// (excel-answers.md Q6: type 51, legend, no series), and the saved file reads back as that chart.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_piw0_2__an_empty_chart_saves_and_reads_back() {
    let mut book = open(calc_host::blank_package());
    let sheet = book.host().sheets()[0].id();
    command(&mut book, |b| WorkbookCommand::ChartCreateEmpty {
        sheet,
        revision: b.host().revision().get(),
        kind: "column".into(),
        place: [pts(200.0), pts(20.0), pts(300.0), pts(200.0)],
        title: None,
        legend: true,
        unique_start: 2,
    });
    shape(
        &mut book,
        sheet,
        add(ShapeKind::Shape, "rect", 20.0, 200.0, 100.0, 50.0),
    );
    let chart = &book.host().model().definitions().charts(sheet)[0];
    assert_eq!((chart.name.as_str(), chart.drawing_id), ("Chart 1", Some(2)));
    assert_eq!(book.host().shapes(sheet)[0].id, 3);
    let reloaded = reopened(&mut book);
    let (charts, unread) = reloaded.host().charts(sheet);
    assert_eq!((charts.len(), unread), (1, 0));
    assert_eq!(charts[0].name, "Chart 1");
    assert!(charts[0].plots[0].series.is_empty());
    let chart = &reloaded.host().model().definitions().charts(sheet)[0];
    assert!(chart.legend.is_some() && chart.title.is_none());
    assert_eq!(reloaded.host().shapes(sheet).len(), 1);
}

/// The elements of `xml` as Excel's `SaveCopyAs` writes them, compared without the namespace
/// declarations' placement: the declaration, namespace attributes and the random numbers Excel
/// picks per chart (axis identities) are normalised; every element and attribute stays.
fn elements(xml: &str) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    let mut out = Vec::new();
    let mut rest = xml.trim_start_matches('\u{feff}');
    if let Some(end) = rest.strip_prefix("<?xml").and_then(|r| r.find("?>")) {
        rest = rest[5 + end + 2..].trim_start();
    }
    while let Some(start) = rest.find('<') {
        if start > 0 && !rest[..start].trim().is_empty() {
            out.push(rest[..start].trim().to_owned());
        }
        let end = rest[start..].find('>').unwrap() + start;
        let mut tag = rest[start..=end].to_owned();
        rest = &rest[end + 1..];
        // Drop namespace declarations.
        while let Some(i) = tag.find(" xmlns") {
            let value = tag[i..].find("=\"").unwrap() + i + 2;
            let close = tag[value..].find('"').unwrap() + value;
            tag.replace_range(i..=close, "");
        }
        // Per-chart random identities: axes, the frame's creation GUID.
        for name in ["axId", "crossAx", "creationId"] {
            let needle = if name == "creationId" { " id=\"" } else { " val=\"" };
            if tag.contains(&format!("<c:{name} ")) || tag.contains(&format!(":{name} ")) {
                let i = tag.find(needle).unwrap() + needle.len();
                let close = tag[i..].find('"').unwrap() + i;
                let value = tag[i..close].to_owned();
                let n = ids.iter().position(|v| *v == value).unwrap_or_else(|| {
                    ids.push(value);
                    ids.len() - 1
                });
                tag.replace_range(i..close, &format!("#{n}"));
            }
        }
        out.push(tag);
    }
    out
}

/// dc-piw0 Q10 (excel-answers.md, Excel build 20527): `ChartObjects.Add(200, 20, 300, 200)` and
/// nothing else, saved with `SaveCopyAs`, is `xl/charts/chart1.xml` and `xl/drawings/drawing1.xml`
/// of tests/fixtures/excel-q10, byte for byte as recorded. The engine's saved chart and drawing
/// parts equal them element by element; the axis identities and the frame's creation GUID are
/// the random values Excel picks for each chart. The anchor also needs Q9's column width: the
/// blank workbook's default font is Aptos Narrow 11, 54 pt columns (72 px) and 14.25 pt rows.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_piw0__q10_an_empty_macro_chart_saves_as_excel_wrote_it() {
    let mut book = open(aptos_narrow_package());
    let sheet = book.host().sheets()[0].id();
    command(&mut book, |b| WorkbookCommand::ChartCreateEmpty {
        sheet,
        revision: b.host().revision().get(),
        kind: "column".into(),
        place: [pts(200.0), pts(20.0), pts(300.0), pts(200.0)],
        title: None,
        legend: true,
        unique_start: 2,
    });
    let mut bytes = Vec::new();
    book.save(&SavePolicy::default(), &mut bytes).unwrap();
    let source: Arc<dyn calc_host::PackageRead> = Arc::new(MemorySource::new(bytes));
    let document = calc_doc::open(source, &calc_host::OpenPolicy::default()).unwrap();
    let find = |folder: &str| {
        let names: Vec<_> = document
            .package()
            .parts()
            .iter()
            .map(|p| p.name().to_owned())
            .filter(|n| n.starts_with(folder) && n.ends_with(".xml"))
            .collect();
        assert_eq!(names.len(), 1, "{names:?}");
        String::from_utf8(document.package().read_part(&names[0]).unwrap()).unwrap()
    };
    let recorded_chart = include_str!("fixtures/excel-q10/chart1.xml");
    let recorded_drawing = include_str!("fixtures/excel-q10/drawing1.xml");
    assert_eq!(elements(&find("xl/charts/")), elements(recorded_chart));
    assert_eq!(elements(&find("xl/drawings/")), elements(recorded_drawing));
}

/// The `c:ser` elements of a chart part with Excel's per-chart GUID and the data caches left out:
/// what the series are, not the numbers Excel cached with them (dc-piw0.8).
fn series_elements(xml: &str) -> Vec<String> {
    series_elements_with(xml, false)
}

/// [`series_elements`], keeping the data caches when `caches`.
fn series_elements_with(xml: &str, caches: bool) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<c:ser>") {
        let end = rest[start..].find("</c:ser>").unwrap() + start + "</c:ser>".len();
        let mut ser = rest[start..end].to_owned();
        rest = &rest[end..];
        for cache in ["strCache", "numCache"].into_iter().filter(|_| !caches) {
            while let Some(a) = ser.find(&format!("<c:{cache}>")) {
                let close = format!("</c:{cache}>");
                let b = ser[a..].find(&close).unwrap() + a + close.len();
                ser.replace_range(a..b, "");
            }
        }
        // The counter stays, the GUID (random in Excel) does not.
        while let Some(a) = ser.find("<c16:uniqueId val=\"{") {
            let from = a + "<c16:uniqueId val=\"{".len();
            let dash = ser[from..].find('-').unwrap() + from;
            let close = ser[dash..].find('}').unwrap() + dash;
            ser.replace_range(dash..close, "-GUID");
            ser.replace_range(a..a + 1, "\u{1}");
        }
        out.extend(elements(&ser.replace('\u{1}', "<")));
    }
    out
}

/// The chart part a macro-made column chart over A1:C3 (the Q0 report data) is saved as.
fn q0_chart_xml() -> String {
    let mut book = open(aptos_narrow_package());
    let sheet = book.host().sheets()[0].id();
    for (address, text) in [
        ("A1", "Region"), ("B1", "Q1"), ("C1", "Q2"),
        ("A2", "North"), ("B2", "10"), ("C2", "20"),
        ("A3", "South"), ("B3", "30"), ("C3", "40"),
    ] {
        enter(&mut book, sheet, address, text);
    }
    command(&mut book, |b| WorkbookCommand::ChartCreateEmpty {
        sheet,
        revision: b.host().revision().get(),
        kind: "column".into(),
        place: [pts(200.0), pts(20.0), pts(300.0), pts(200.0)],
        title: None,
        legend: true,
        unique_start: 2,
    });
    let part = book.host().model().definitions().charts(sheet)[0].part.clone();
    command(&mut book, |b| WorkbookCommand::ChartSource {
        sheet,
        revision: b.host().revision().get(),
        part: part.clone(),
        area: GridArea::new(at(sheet, "A1"), at(sheet, "C3")).unwrap(),
        rows: false,
        headers: 1,
        categories: 1,
    });
    let mut bytes = Vec::new();
    book.save(&SavePolicy::default(), &mut bytes).unwrap();
    let source: Arc<dyn calc_host::PackageRead> = Arc::new(MemorySource::new(bytes));
    let document = calc_doc::open(source, &calc_host::OpenPolicy::default()).unwrap();
    let name = document
        .package()
        .parts()
        .iter()
        .map(|p| p.name().to_owned())
        .find(|n| n.starts_with("xl/charts/") && n.ends_with(".xml"))
        .unwrap();
    String::from_utf8(document.package().read_part(&name).unwrap()).unwrap()
}

/// dc-piw0.9 (Q0, Q17 answer): every series reference carries the cache Excel writes: `strCache` (no `formatCode`)
/// for the name and the text categories, `numCache` with `General` for the values, filled from the cells.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_piw0_9__series_caches_are_what_excel_wrote() {
    let written = q0_chart_xml();
    let recorded = include_str!("fixtures/excel-q0/chart1.xml");
    let (written, recorded) = (series_elements_with(&written, true), series_elements_with(recorded, true));
    assert_eq!(written, recorded);
}

/// dc-piw0.9: the caches are read from the cells when the part is written, not when the chart was made: a cell
/// edited after the chart, and a formula cell (its calculated value), show in the numCache.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_piw0_9__series_caches_follow_the_cells_at_save() {
    let mut book = open(aptos_narrow_package());
    let sheet = book.host().sheets()[0].id();
    for (address, text) in [
        ("A1", "Region"), ("B1", "Q1"), ("A2", "North"), ("B2", "=5+5"), ("A3", "South"), ("B3", "30"),
    ] {
        enter(&mut book, sheet, address, text);
    }
    command(&mut book, |b| WorkbookCommand::ChartCreateEmpty {
        sheet,
        revision: b.host().revision().get(),
        kind: "column".into(),
        place: [pts(200.0), pts(20.0), pts(300.0), pts(200.0)],
        title: None,
        legend: true,
        unique_start: 2,
    });
    let part = book.host().model().definitions().charts(sheet)[0].part.clone();
    command(&mut book, |b| WorkbookCommand::ChartSource {
        sheet,
        revision: b.host().revision().get(),
        part: part.clone(),
        area: GridArea::new(at(sheet, "A1"), at(sheet, "B3")).unwrap(),
        rows: false,
        headers: 1,
        categories: 1,
    });
    enter(&mut book, sheet, "B3", "31");
    assert!(book.dispatch(SkinIntent::Recalculate).accepted());
    let mut bytes = Vec::new();
    book.save(&SavePolicy::default(), &mut bytes).unwrap();
    let document = calc_doc::open(
        Arc::new(MemorySource::new(bytes)) as Arc<dyn calc_host::PackageRead>,
        &calc_host::OpenPolicy::default(),
    )
    .unwrap();
    let name = document
        .package()
        .parts()
        .iter()
        .map(|p| p.name().to_owned())
        .find(|n| n.starts_with("xl/charts/") && n.ends_with(".xml"))
        .unwrap();
    let xml = String::from_utf8(document.package().read_part(&name).unwrap()).unwrap();
    assert!(
        xml.contains("<c:numCache><c:formatCode>General</c:formatCode><c:ptCount val=\"2\"/><c:pt idx=\"0\"><c:v>10</c:v></c:pt><c:pt idx=\"1\"><c:v>31</c:v></c:pt></c:numCache>"),
        "{xml}"
    );
}

/// dc-piw0.8 (Q0, excel-answers.md, ~/codex-runs/eng1 excel-report chart1.xml): the series of a
/// macro-made column chart over A1:C3 in columns are written as Excel writes them: `invertIfNegative`
/// after the name, the `c16:uniqueId` extension last. Excel's data caches (`strCache`, `numCache`)
/// are not written yet and are left out of the comparison.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_piw0_8__series_xml_is_what_excel_wrote() {
    let written = q0_chart_xml();
    let recorded = include_str!("fixtures/excel-q0/chart1.xml");
    let (written, recorded) = (series_elements(&written), series_elements(recorded));
    assert_eq!(written.len(), recorded.len(), "{written:#?}");
    assert_eq!(written, recorded);
}

/// The plot's opening tags, its first series and the tags between the last series and the axis ids, with the
/// data caches and Excel's per-chart GUID left out.
fn plot_children(xml: &str) -> (String, Vec<String>, String) {
    let plot = xml.find("<c:plotArea><c:layout/>").unwrap() + "<c:plotArea><c:layout/>".len();
    let first = xml.find("<c:ser>").unwrap();
    let first_end = xml.find("</c:ser>").unwrap() + "</c:ser>".len();
    let last_end = xml.rfind("</c:ser>").unwrap() + "</c:ser>".len();
    // A pie or doughnut plot has no axes: its tail runs to the end of the plot area.
    let axes = xml[last_end..].find("<c:axId").or_else(|| xml[last_end..].find("</c:plotArea>")).unwrap() + last_end;
    // Each element the writer makes on its own repeats the chart namespace declaration.
    let bare = |text: &str| text.replace(" xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\"", "");
    (
        bare(&xml[plot..first]),
        series_elements(&xml[first..first_end]),
        bare(&xml[last_end..axes]),
    )
}

/// dc-piw0.10 Q17 (excel-answers.md, run1 q17): the children of a macro chart's series and plot per chart type as
/// Excel wrote them: bar `invertIfNegative`; line `marker` (none) and `smooth` (0) with a plot-level `smooth`;
/// scatter `scatterStyle lineMarker`, an `spPr` with the line off and `smooth`; area nothing more but an `spPr` on the second series. Pie and doughnut
/// charts keep both series in the part, as Excel does (dc-piw0.11 (6)).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_piw0_10__q17_series_children_follow_the_chart_type() {
    for (kind, recorded) in [
        ("column", include_str!("fixtures/excel-q17/chart1.xml")),
        ("line", include_str!("fixtures/excel-q17/chart4.xml")),
        ("area", include_str!("fixtures/excel-q17/chart6.xml")),
        ("scatter", include_str!("fixtures/excel-q17/chart8.xml")),
        ("pie", include_str!("fixtures/excel-q17/chart7.xml")),
        ("doughnut", include_str!("fixtures/excel-q17/chart9.xml")),
    ] {
        let mut book = open(aptos_narrow_package());
        let sheet = book.host().sheets()[0].id();
        for (address, text) in [
            ("A1", "Region"), ("B1", "Q1"), ("C1", "Q2"),
            ("A2", "North"), ("B2", "10"), ("C2", "20"),
            ("A3", "South"), ("B3", "30"), ("C3", "40"),
            ("A4", "East"), ("B4", "50"), ("C4", "60"),
        ] {
            enter(&mut book, sheet, address, text);
        }
        command(&mut book, |b| WorkbookCommand::ChartCreateEmpty {
            sheet,
            revision: b.host().revision().get(),
            kind: kind.into(),
            place: [pts(200.0), pts(20.0), pts(300.0), pts(200.0)],
            title: None,
            legend: true,
            unique_start: 2,
        });
        let part = book.host().model().definitions().charts(sheet)[0].part.clone();
        command(&mut book, |b| WorkbookCommand::ChartSource {
            sheet,
            revision: b.host().revision().get(),
            part: part.clone(),
            area: GridArea::new(at(sheet, "A1"), at(sheet, "C4")).unwrap(),
            rows: false,
            headers: 1,
            categories: 1,
        });
        let mut bytes = Vec::new();
        book.save(&SavePolicy::default(), &mut bytes).unwrap();
        let document = calc_doc::open(
            Arc::new(MemorySource::new(bytes)) as Arc<dyn calc_host::PackageRead>,
            &calc_host::OpenPolicy::default(),
        )
        .unwrap();
        let name = document
            .package()
            .parts()
            .iter()
            .map(|p| p.name().to_owned())
            .find(|n| n.starts_with("xl/charts/") && n.ends_with(".xml"))
            .unwrap();
        let written = String::from_utf8(document.package().read_part(&name).unwrap()).unwrap();
        let (written_all, recorded_all) = (series_elements(&written), series_elements(recorded));
        let pie = matches!(kind, "pie" | "doughnut");
        if pie {
            assert_eq!(written.matches("<c:ser>").count(), 2, "{kind}: both series stay in the part");
        }
        let (written, recorded) = (plot_children(&written), plot_children(recorded));
        assert_eq!(written.0, recorded.0, "{kind}: plot head");
        assert_eq!(written.1, recorded.1, "{kind}: first series");
        // An area chart's second series carries `spPr` (the line off at 25400): compared with the first.
        if pie || kind == "area" {
            assert_eq!(written_all, recorded_all, "{kind}: all series");
        }
        assert_eq!(written.2, recorded.2, "{kind}: plot tail");
    }
}

#[allow(dead_code)]
#[path = "../../calc-doc/tests/cdoc_m1_1_support/mod.rs"]
mod existing_wire_support;

/// The blank workbook with Excel's current default font (Aptos Narrow 11, as Excel 16 writes a
/// new workbook's styles part, Q9), which sets the default column and row sizes.
fn aptos_narrow_package() -> Vec<u8> {
    let blank = calc_host::blank_package();
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
                    .replace("<name val=\"Calibri\"/>", "<name val=\"Aptos Narrow\"/>");
                assert!(xml.contains("Aptos Narrow"));
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
    existing_wire_support::build_zip(&entries)
}

/// dc-zaj7 / Q9 (excel-answers.md, Excel build 20527, 96 DPI): the default column of an Aptos
/// Narrow 11 workbook is 72 px = 54 pt and its rows 14.25 pt; a Calibri 11 workbook keeps
/// 64 px = 48 pt and 15 pt. Column positions are the sum of whole-pixel widths of the font's
/// maximum digit width (8 px, 7 px).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_zaj7__q9_default_column_and_row_follow_the_default_font() {
    let aptos = open(aptos_narrow_package());
    let sheet = aptos.host().sheets()[0].id();
    assert_eq!(aptos.host().column_left(sheet, 1), 54.0);
    assert_eq!(aptos.host().column_left(sheet, 4), 216.0);
    assert_eq!(aptos.host().row_top(sheet, 1), 14.25);
    let calibri = open(calc_host::blank_package());
    let sheet = calibri.host().sheets()[0].id();
    assert_eq!(calibri.host().column_left(sheet, 1), 48.0);
    assert_eq!(calibri.host().row_top(sheet, 1), 15.0);
}

/// dc-zjsg.4: objects the file holds are moved, resized and deleted; the save rewrites their stored anchors in place
/// (a two-cell anchor's corners, a transform's offset and extent) or drops them, and keeps everything else as stored.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_zjsg_4__stored_objects_move_resize_and_delete_and_save_as_stored() {
    let bytes = include_bytes!("../../calc-doc/tests/fixtures/drawing/shapes-paint4.xlsx").to_vec();
    let mut book = open(bytes.clone());
    let sheet = book.host().sheets()[0].id();
    let id = |book: &HostSession, name: &str| book.host().shapes(sheet).iter().find(|s| s.name == name).unwrap().id;
    let star = id(&book, "Freeform triangle");
    let gone = id(&book, "Freeform curves");
    // Move the triangle to 96 pt, 30 pt (column C at 48 pt a column, row 3 at 15 pt a row) and make it 60 pt by 45.
    shape(&mut book, sheet, ShapeChange::Place { id: star, left: Some(pts(96.0)), top: Some(pts(30.0)), width: Some(pts(60.0)), height: Some(pts(45.0)) });
    shape(&mut book, sheet, ShapeChange::Delete { id: gone });
    assert_eq!(book.host().shapes(sheet).len(), 19);
    let mut reloaded = reopened(&mut book);
    let read = reloaded.host().shapes(sheet).to_vec();
    assert_eq!(read.len(), 19);
    assert!(read.iter().all(|s| s.name != "Freeform curves"));
    let moved = read.iter().find(|s| s.id == star).unwrap();
    assert_eq!(reloaded.host().anchor_rectangle(sheet, &moved.anchor), (96.0, 30.0, 60.0, 45.0));
    assert_eq!((moved.offset, moved.extent), ((96 * 12_700, 30 * 12_700), (60 * 12_700, 45 * 12_700)));
    // Its geometry and every other object are as stored.
    let original = open(bytes);
    let before: Vec<_> = original.host().shapes(sheet).iter().filter(|s| s.id != gone).cloned().collect();
    for (a, b) in read.iter().zip(&before) {
        if a.id == star {
            assert_eq!((&a.custom_geometry, &a.fill, &a.text), (&b.custom_geometry, &b.fill, &b.text));
        } else {
            assert_eq!(a, b, "{}", a.name);
        }
    }
    // Undo after reopening is a fresh session; a second move of the reopened shape saves again.
    shape(&mut reloaded, sheet, ShapeChange::Place { id: star, left: Some(pts(0.0)), top: None, width: None, height: None });
    let again = reopened(&mut reloaded);
    let moved = again.host().shapes(sheet).iter().find(|s| s.id == star).unwrap().clone();
    assert_eq!(again.host().anchor_rectangle(sheet, &moved.anchor), (0.0, 30.0, 60.0, 45.0));
}

/// dc-zjsg.4: two stored objects deleted one after the other, then a new one added and edited: the second delete
/// reads the remaining stored objects (it panicked indexing past them), the new one is not taken for a stored one,
/// it does not take a deleted one's identity, and the save keeps it all.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_zjsg_4__second_stored_delete_and_a_new_shape_after_it() {
    let mut book = open(include_bytes!("../../calc-doc/tests/fixtures/drawing/shapes-paint4.xlsx").to_vec());
    let sheet = book.host().sheets()[0].id();
    let id = |book: &HostSession, name: &str| book.host().shapes(sheet).iter().find(|s| s.name == name).unwrap().id;
    let (first, second) = (id(&book, "Freeform curves"), id(&book, "Freeform triangle"));
    shape(&mut book, sheet, ShapeChange::Delete { id: first });
    shape(&mut book, sheet, ShapeChange::Delete { id: second });
    shape(&mut book, sheet, add(ShapeKind::Shape, "rect", 10.0, 10.0, 40.0, 20.0));
    let added = book.host().shapes(sheet).last().unwrap().id;
    assert!(added != first && added != second);
    shape(&mut book, sheet, ShapeChange::Text { id: added, text: "New".into() });
    assert_eq!(book.host().shapes(sheet).len(), 19);
    let reloaded = reopened(&mut book);
    let read = reloaded.host().shapes(sheet);
    assert_eq!(read.len(), 19);
    assert!(read.iter().all(|s| s.name != "Freeform curves" && s.name != "Freeform triangle"));
    assert_eq!(read.last().unwrap().text, Some(vec!["New".to_owned()]));
}

/// dc-zjsg.4: the file's own objects take new text, fill and outline, a turn, another place in the drawing order,
/// and go into a group and come out of it; each saved in place (the rest of the drawing as stored) and reopened
/// as edited.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_zjsg_4__stored_objects_take_text_fill_outline_turns_order_and_groups() {
    let bytes = include_bytes!("../../calc-doc/tests/fixtures/drawing/shapes-paint4.xlsx").to_vec();
    let mut book = open(bytes.clone());
    let sheet = book.host().sheets()[0].id();
    let id = |book: &HostSession, name: &str| book.host().shapes(sheet).iter().find(|s| s.name == name).unwrap().id;
    let get = |book: &HostSession, name: &str| book.host().shapes(sheet).iter().find(|s| s.name == name).unwrap().clone();
    let original = open(bytes);
    let before = |name: &str| get(&original, name);
    let text = id(&book, "Style accent2");
    shape(&mut book, sheet, ShapeChange::Text { id: text, text: "New text\rSecond & last".into() });
    let filled = id(&book, "Style accent6 line only");
    shape(&mut book, sheet, ShapeChange::Fill { id: filled, fill: ShapeFill::Solid(ShapeColour::Rgb(0xFF0000)) });
    let lined = id(&book, "Style accent4 dark text");
    let line = ShapeLine { weight: Some(25_400), colour: Some(ShapeColour::Rgb(0x0000FF)), ..ShapeLine::default() };
    shape(&mut book, sheet, ShapeChange::Line { id: lined, line: line.clone() });
    let turned = id(&book, "Rect rot 30");
    let (l, t, w, h) = book.host().anchor_rectangle(sheet, &before("Rect rot 30").anchor);
    shape(&mut book, sheet, ShapeChange::Rotate { id: turned, rotation: 90 * 60_000 });
    let front = id(&book, "Freeform triangle");
    shape(&mut book, sheet, ShapeChange::Order { id: front, order: ShapeOrder::Front });
    let (a, b) = (id(&book, "Pattern pct50"), id(&book, "Pattern ltHorz"));
    shape(&mut book, sheet, ShapeChange::Group { ids: vec![a, b] });
    let count = book.host().shapes(sheet).len();
    assert_eq!(count, 19);
    let mut reloaded = reopened(&mut book);
    if let Ok(path) = std::env::var("DW2_DUMP") {
        let mut bytes = Vec::new();
        book.save(&SavePolicy::default(), &mut bytes).unwrap();
        std::fs::write(path, &bytes).unwrap();
    }
    let read = reloaded.host().shapes(sheet).to_vec();
    assert_eq!(read.len(), 19);
    let found = |name: &str| read.iter().find(|s| s.name == name).unwrap().clone();
    assert_eq!(found("Style accent2").text, Some(vec!["New text".to_owned(), "Second & last".to_owned()]));
    // The stored paragraph and run properties are kept: each paragraph centred, 11 pt.
    let mut body = before("Style accent2").body;
    body.alignment = vec![Some("ctr".into()); 2];
    assert_eq!(found("Style accent2").body, body);
    assert_eq!(found("Style accent6 line only").fill, ShapeFill::Solid(ShapeColour::Rgb(0xFF0000)));
    assert_eq!(found("Style accent4 dark text").line, line);
    let r = found("Rect rot 30");
    assert_eq!(r.rotation, 90 * 60_000);
    // A quarter turn stores the box turned: its anchor is the box's width and height swapped about its centre.
    let (l2, t2, w2, h2) = reloaded.host().anchor_rectangle(sheet, &r.anchor);
    assert!((w2 - h).abs() < 0.5 && (h2 - w).abs() < 0.5, "{w2} {h2} from {w} {h}");
    assert!((l2 + w2 / 2.0 - (l + w / 2.0)).abs() < 0.5 && (t2 + h2 / 2.0 - (t + h / 2.0)).abs() < 0.5);
    assert_eq!(read.last().unwrap().name, "Freeform triangle");
    let group = read.iter().find(|s| s.kind == ShapeKind::Group).unwrap().clone();
    let names: Vec<_> = group.children.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["Pattern pct50", "Pattern ltHorz"]);
    assert_eq!(group.children[0].fill, before("Pattern pct50").fill);
    // Untouched objects are as stored.
    for name in ["Freeform curves", "Rect rot 60", "Pattern smCheck"] {
        assert_eq!(found(name), before(name), "{name}");
    }
    // Ungrouped again: both patterns at the top level where the group was, at their own places.
    shape(&mut reloaded, sheet, ShapeChange::Ungroup { id: group.id });
    let again = reopened(&mut reloaded);
    let read = again.host().shapes(sheet);
    assert_eq!(read.len(), 20);
    for name in ["Pattern pct50", "Pattern ltHorz"] {
        let now = read.iter().find(|s| s.name == name).unwrap();
        let (a, b) = (again.host().anchor_rectangle(sheet, &now.anchor), original.host().anchor_rectangle(sheet, &before(name).anchor));
        assert!([a.0 - b.0, a.1 - b.1, a.2 - b.2, a.3 - b.3].iter().all(|d| d.abs() < 0.5), "{name}: {a:?} {b:?}");
        assert_eq!(now.fill, before(name).fill);
    }
}

/// dc-zjsg.4: a new shape turned, sent to the back behind the file's objects and grouped with one of them saves in
/// that order and reopens so.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_zjsg_4__a_new_shape_behind_stored_ones_and_grouped_with_one() {
    let mut book = open(include_bytes!("../../calc-doc/tests/fixtures/drawing/shapes-paint4.xlsx").to_vec());
    let sheet = book.host().sheets()[0].id();
    shape(&mut book, sheet, add(ShapeKind::Shape, "ellipse", 20.0, 20.0, 40.0, 30.0));
    let added = book.host().shapes(sheet).last().unwrap().id;
    shape(&mut book, sheet, ShapeChange::Rotate { id: added, rotation: 30 * 60_000 });
    shape(&mut book, sheet, ShapeChange::Order { id: added, order: ShapeOrder::Back });
    let reloaded = reopened(&mut book);
    let read = reloaded.host().shapes(sheet);
    assert_eq!((read[0].geometry.as_deref(), read[0].rotation), (Some("ellipse"), 30 * 60_000));
    assert_eq!(read[1].name, "Freeform triangle");
    let mut book = reloaded;
    let ids: Vec<u32> = book.host().shapes(sheet)[..2].iter().map(|s| s.id).collect();
    shape(&mut book, sheet, ShapeChange::Group { ids });
    let read = reopened(&mut book).host().shapes(sheet).to_vec();
    assert_eq!(read[0].kind, ShapeKind::Group);
    assert_eq!(read[0].children.len(), 2);
    assert_eq!(read[0].children[1].name, "Freeform triangle");
    assert!(read[0].children[1].custom_geometry.is_some());
}

/// dc-zjsg.4: a picture inserted from a file saves its image as a media part and reopens as the same picture.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_zjsg_4__an_inserted_picture_saves_and_reopens() {
    let png = include_bytes!("../../calc-doc/tests/fixtures/drawing/pictures/card.png");
    let mut book = open(include_bytes!("../../calc-doc/tests/fixtures/drawing/shapes-paint4.xlsx").to_vec());
    let sheet = book.host().sheets()[0].id();
    shape(&mut book, sheet, ShapeChange::AddPicture {
        left: pts(48.0),
        top: pts(15.0),
        width: pts(120.0),
        height: pts(75.0),
        content_type: "image/png".into(),
        bytes: png.to_vec().into(),
        description: Some("card.png".into()),
    });
    let reloaded = reopened(&mut book);
    let read = reloaded.host().shapes(sheet);
    assert_eq!(read.len(), 21);
    let picture = read.last().unwrap();
    assert_eq!((picture.kind, picture.name.as_str(), picture.edit_as.as_deref()), (ShapeKind::Picture, "Picture 21", Some("oneCell")));
    let media = picture.media.as_ref().unwrap();
    assert_eq!((&*media.image.as_ref().unwrap().bytes, media.description.as_deref()), (&png[..], Some("card.png")));
    assert_eq!(reloaded.host().anchor_rectangle(sheet, &picture.anchor), (48.0, 15.0, 120.0, 75.0));
}

/// dc-zjsg.7: sparklines added, changed and deleted; each save writes the worksheet's sparkline extension from the
/// groups, and the file reopens with them (the other groups as they were).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_zjsg_7__sparklines_are_added_changed_and_deleted_and_saved() {
    use calc_host::SparklineChange;
    let bytes = include_bytes!("../../calc-doc/tests/fixtures/drawing/sparklines.xlsx").to_vec();
    let mut book = open(bytes.clone());
    let sheet = book.host().sheets()[0].id();
    let before = book.host().model().definitions().sparklines(sheet).to_vec();
    let run = |book: &mut HostSession, change: SparklineChange| {
        command(book, |b| WorkbookCommand::Sparkline { sheet, revision: b.host().revision().get(), change: change.clone() });
    };
    let area = GridArea::new(at(sheet, "A2"), at(sheet, "E2")).unwrap();
    run(&mut book, SparklineChange::Add { data: area, cell: at(sheet, "G2"), kind: Some("column".into()) });
    run(&mut book, SparklineChange::Show { cell: at(sheet, "F4"), point: "markers".into(), on: true });
    run(&mut book, SparklineChange::Kind { cell: at(sheet, "F6"), kind: Some("line".into()) });
    run(&mut book, SparklineChange::Delete { cell: at(sheet, "F7") });
    let reloaded = reopened(&mut book);
    let groups = reloaded.host().model().definitions().sparklines(sheet).to_vec();
    assert_eq!(groups.len(), before.len());
    // F7's group is gone; the new one is last, over Spark!A2:E2 in G2, a column group with Excel's look.
    let new = groups.last().unwrap();
    assert_eq!((new.kind.as_deref(), new.sparklines[0].data.as_str(), new.sparklines[0].cell.as_str()), (Some("column"), "Spark!A2:E2", "G2"));
    assert_eq!(new.colour("colorSeries").and_then(|c| c.theme.as_deref()), Some("4"));
    assert!(groups[1].markers && groups[2].kind.is_none());
    assert_eq!((&groups[0], &groups[3]), (&before[0], &before[4]));
    let views = reloaded.host().sparklines(sheet);
    let g2 = views.iter().flat_map(|(_, v)| v).find(|v| v.cell == at(sheet, "G2")).unwrap();
    assert_eq!(g2.values, vec![Some(3.0), Some(5.0), Some(2.0), Some(8.0), Some(6.0)]);
    // A cell that holds one cannot take another; one without cannot be changed.
    let revision = book.host().revision().get();
    let taken = book.dispatch(SkinIntent::Command(WorkbookCommand::Sparkline { sheet, revision, change: SparklineChange::Add { data: area, cell: at(sheet, "F2"), kind: None } }));
    assert!(!taken.accepted());
    // Every group deleted: the extension goes, the file reopens without sparklines.
    let mut empty = reloaded;
    for cell in ["F2", "F3", "F4", "F5", "F6", "F8", "G2"] {
        run(&mut empty, SparklineChange::Delete { cell: at(sheet, cell) });
    }
    let none = reopened(&mut empty);
    assert!(none.host().model().definitions().sparklines(sheet).is_empty());
    // A worksheet without an extension list gets one.
    let mut blank = open(calc_host::blank_package());
    let sheet = blank.host().sheets()[0].id();
    let area = GridArea::new(at(sheet, "A1"), at(sheet, "A3")).unwrap();
    command(&mut blank, |b| WorkbookCommand::Sparkline { sheet, revision: b.host().revision().get(), change: SparklineChange::Add { data: area, cell: at(sheet, "B1"), kind: None } });
    let read = reopened(&mut blank);
    let groups = read.host().model().definitions().sparklines(sheet);
    assert_eq!((groups.len(), groups[0].sparklines[0].data.as_str()), (1, "Sheet1!A1:A3"));
}

/// dc-zjsg.4: a turned or flipped group taken apart leaves each child where the group showed it: a group of two
/// side-by-side boxes turned a quarter becomes two boxes one above the other, each turned a quarter; flipped, the
/// left box goes right and is flipped itself.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_zjsg_4__a_turned_or_flipped_group_comes_apart_where_it_showed_its_children() {
    let mut book = open(calc_host::blank_package());
    let sheet = book.host().sheets()[0].id();
    // A at 100..140 x 100..120, B at 160..200 x 100..120: the group is 100..200 x 100..120, centred at 150, 110.
    shape(&mut book, sheet, add(ShapeKind::Shape, "rect", 100.0, 100.0, 40.0, 20.0));
    shape(&mut book, sheet, add(ShapeKind::Shape, "ellipse", 160.0, 100.0, 40.0, 20.0));
    shape(&mut book, sheet, ShapeChange::Group { ids: vec![2, 3] });
    let group = book.host().shapes(sheet)[0].id;
    shape(&mut book, sheet, ShapeChange::Rotate { id: group, rotation: 90 * 60_000 });
    let mut flipped = reopened(&mut book);
    shape(&mut book, sheet, ShapeChange::Ungroup { id: group });
    let centre = |b: &HostSession, id: u32| {
        let s = b.host().shapes(sheet).iter().find(|s| s.id == id).unwrap().clone();
        let p = |v: i64| v as f64 / 12_700.0;
        ((p(s.offset.0) + p(s.extent.0) / 2.0).round(), (p(s.offset.1) + p(s.extent.1) / 2.0).round(), s.rotation, s.flip_horizontal)
    };
    // Turned a quarter clockwise about (150, 110): A's centre (120, 110) goes to (150, 80), B's (180, 110) to (150, 140).
    assert_eq!(centre(&book, 2), (150.0, 80.0, 90 * 60_000, false));
    assert_eq!(centre(&book, 3), (150.0, 140.0, 90 * 60_000, false));
    let saved = reopened(&mut book);
    assert_eq!((centre(&saved, 2), centre(&saved, 3)), (centre(&book, 2), centre(&book, 3)));
    // The reopened group turned back and flipped left to right: A goes right, flipped; B left.
    let stored_group = flipped.host().shapes(sheet)[0].id;
    shape(&mut flipped, sheet, ShapeChange::Rotate { id: stored_group, rotation: 0 });
    shape(&mut flipped, sheet, ShapeChange::Flip { id: stored_group, horizontal: true });
    shape(&mut flipped, sheet, ShapeChange::Ungroup { id: stored_group });
    assert_eq!(centre(&flipped, 2), (180.0, 110.0, 0, true));
    assert_eq!(centre(&flipped, 3), (120.0, 110.0, 0, true));
    let saved = reopened(&mut flipped);
    assert_eq!(centre(&saved, 2), (180.0, 110.0, 0, true));
}

/// dc-zjsg.4: a kept object (an equation, ink, SmartArt, OLE) is not moved or deleted; it stays as the file stores it.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_zjsg_4__kept_objects_are_not_edited() {
    let mut book = open(include_bytes!("../../calc-doc/tests/fixtures/drawing/kept-objects.xlsx").to_vec());
    let sheet = book.host().sheets()[0].id();
    for id in [2, 3, 4] {
        let revision = book.host().revision().get();
        let receipt = book.dispatch(SkinIntent::Command(WorkbookCommand::Shape { sheet, revision, change: ShapeChange::Delete { id } }));
        assert!(!receipt.accepted(), "{id}");
    }
    assert_eq!(book.host().shapes(sheet).len(), 5);
}

/// dc-zjsg.4: inserting rows keeps a sheet's drawing objects (the model carries them through the edit) and the save
/// moves their stored anchors as Excel does; a session that also moved a stored object refuses to save rather than
/// write stale anchors.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_zjsg_4__drawings_survive_row_inserts_and_save_moved() {
    use calc_doc_model::{Axis, AxisChange, AxisEdit, ChartAnchor};
    let bytes = include_bytes!("../../calc-doc/tests/fixtures/drawing/shapes-paint4.xlsx").to_vec();
    let mut book = open(bytes);
    let sheet = book.host().sheets()[0].id();
    let before = book.host().shapes(sheet).to_vec();
    let edit = AxisEdit::new(sheet, Axis::Rows, AxisChange::Insert, 2, 2).unwrap();
    assert!(book.dispatch(SkinIntent::EditAxis(edit)).accepted());
    assert_eq!(book.host().shapes(sheet).len(), before.len());
    let reloaded = reopened(&mut book);
    let after = reloaded.host().shapes(sheet).to_vec();
    assert_eq!(after.len(), before.len());
    let rows = |a: &ChartAnchor| match a {
        ChartAnchor::TwoCell { from, to } => (from.row, to.row),
        other => panic!("{other:?}"),
    };
    // Every object starts at row 4 (0-based 3) or below, so each moves down two rows.
    for (a, b) in after.iter().zip(&before) {
        let ((af, at), (bf, bt)) = (rows(&a.anchor), rows(&b.anchor));
        assert_eq!((af, at), (bf + 2, bt + 2), "{}", a.name);
    }
}

/// dc-zjsg.5: a connector turned a quarter (an elbow from a bottom site to a top site, as Excel draws one between a
/// shape and one below it) is rerouted when a glued shape moves: its turn and adjust values stay, its box and flips
/// are recomputed so that, turned, it runs from site to site; its anchor is that box turned, and both survive a save.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_zjsg_5__a_turned_elbow_connector_is_rerouted_between_its_sites() {
    let mut book = open(calc_host::blank_package());
    let sheet = book.host().sheets()[0].id();
    shape(&mut book, sheet, add(ShapeKind::Shape, "rect", 20.0, 120.0, 100.0, 50.0));
    shape(&mut book, sheet, add(ShapeKind::Shape, "rect", 20.0, 260.0, 100.0, 50.0));
    shape(&mut book, sheet, add(ShapeKind::Connector, "bentConnector3", 10.0, 10.0, 10.0, 10.0));
    shape(&mut book, sheet, ShapeChange::Rotate { id: 4, rotation: 90 * 60_000 });
    // A rectangle's sites (1-based): top, left, bottom, right.
    shape(&mut book, sheet, ShapeChange::Connect { id: 4, begin: true, shape: 2, site: 3 });
    shape(&mut book, sheet, ShapeChange::Connect { id: 4, begin: false, shape: 3, site: 1 });
    // Where the connector's path starts and ends on the sheet: its box's corners, flipped and turned about its centre.
    let ends = |b: &HostSession| {
        let c = b.host().shapes(sheet).iter().find(|s| s.id == 4).unwrap().clone();
        let p = |v: i64| v as f64 / 12_700.0;
        let (w, h) = (p(c.extent.0), p(c.extent.1));
        let centre = (p(c.offset.0) + w / 2.0, p(c.offset.1) + h / 2.0);
        let corner = (if c.flip_horizontal { w / 2.0 } else { -w / 2.0 }, if c.flip_vertical { h / 2.0 } else { -h / 2.0 });
        let (sin, cos) = (c.rotation as f64 / 60_000.0).to_radians().sin_cos();
        let at = |(x, y): (f64, f64)| ((centre.0 + x * cos - y * sin).round(), (centre.1 + x * sin + y * cos).round());
        (at(corner), at((-corner.0, -corner.1)), c.rotation, b.host().anchor_rectangle(sheet, &c.anchor))
    };
    assert_eq!(ends(&book), ((70.0, 170.0), (70.0, 260.0), 90 * 60_000, (70.0, 170.0, 0.0, 90.0)));
    let mut stored = reopened(&mut book);
    // The lower rectangle moves right: its top site is at 250 pt, 260 pt.
    shape(&mut stored, sheet, ShapeChange::Place { id: 3, left: Some(pts(200.0)), top: None, width: None, height: None });
    let moved = ends(&stored);
    assert_eq!((moved.0, moved.1, moved.2), ((70.0, 170.0), (250.0, 260.0), 90 * 60_000));
    // Anchored on the box from site to site.
    assert_eq!(moved.3, (70.0, 170.0, 180.0, 90.0));
    let again = reopened(&mut stored);
    assert_eq!(ends(&again), moved);
}

/// dc-zjsg.4: a stored object moved and then rows inserted above it in the same session: the save writes it where
/// the session left it (moved, then down with the rows), and every other object down with the rows.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_zjsg_4__a_stored_object_moved_then_rows_inserted_saves_where_it_is() {
    use calc_doc_model::{Axis, AxisChange, AxisEdit};
    let bytes = include_bytes!("../../calc-doc/tests/fixtures/drawing/shapes-paint4.xlsx").to_vec();
    let mut book = open(bytes);
    let sheet = book.host().sheets()[0].id();
    let triangle = book.host().shapes(sheet).iter().find(|s| s.name == "Freeform triangle").unwrap().id;
    shape(&mut book, sheet, ShapeChange::Place { id: triangle, left: Some(pts(96.0)), top: Some(pts(60.0)), width: None, height: None });
    let before: Vec<_> = book.host().shapes(sheet).iter().map(|s| (s.id, book.host().anchor_rectangle(sheet, &s.anchor))).collect();
    let edit = AxisEdit::new(sheet, Axis::Rows, AxisChange::Insert, 2, 2).unwrap();
    assert!(book.dispatch(SkinIntent::EditAxis(edit)).accepted());
    let reloaded = reopened(&mut book);
    for (id, (l, t, w, h)) in before {
        let s = reloaded.host().shapes(sheet).iter().find(|s| s.id == id).unwrap();
        let (l2, t2, w2, h2) = reloaded.host().anchor_rectangle(sheet, &s.anchor);
        // Two rows of 15 pt above every object.
        assert!((l2 - l).abs() < 0.01 && (t2 - t - 30.0).abs() < 0.01 && (w2 - w).abs() < 0.01 && (h2 - h).abs() < 0.01, "{}: {:?}", s.name, ((l, t, w, h), (l2, t2, w2, h2)));
    }
}

/// dc-zjsg.5: moving a stored shape reroutes the stored connector glued to it, as Excel does: the glued end at its
/// site, the box and flips from the ends; the save rewrites the connector's anchor and its transform's flips.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_zjsg_5__a_connector_follows_the_stored_shape_it_is_glued_to() {
    let mut book = open(calc_host::blank_package());
    let sheet = book.host().sheets()[0].id();
    shape(&mut book, sheet, add(ShapeKind::Shape, "rect", 20.0, 120.0, 100.0, 50.0));
    shape(&mut book, sheet, add(ShapeKind::Shape, "ellipse", 20.0, 220.0, 100.0, 50.0));
    shape(&mut book, sheet, add(ShapeKind::Connector, "straightConnector1", 10.0, 10.0, 10.0, 10.0));
    shape(&mut book, sheet, ShapeChange::Connect { id: 4, begin: true, shape: 2, site: 3 });
    shape(&mut book, sheet, ShapeChange::Connect { id: 4, begin: false, shape: 3, site: 1 });
    let mut stored = reopened(&mut book);
    let rect_of = |b: &HostSession, id: u32| {
        let s = b.host().shapes(sheet).iter().find(|s| s.id == id).unwrap().clone();
        (b.host().anchor_rectangle(sheet, &s.anchor), s.flip_horizontal, s.flip_vertical)
    };
    assert_eq!(rect_of(&stored, 4), ((70.0, 170.0, 0.0, 50.0), false, false));
    // The oval moves to 200 pt, 300 pt: its top site is at 250 pt, 300 pt.
    shape(&mut stored, sheet, ShapeChange::Place { id: 3, left: Some(pts(200.0)), top: Some(pts(300.0)), width: None, height: None });
    assert_eq!(rect_of(&stored, 4), ((70.0, 170.0, 180.0, 130.0), false, false));
    let mut again = reopened(&mut stored);
    assert_eq!(rect_of(&again, 4), ((70.0, 170.0, 180.0, 130.0), false, false));
    // Moved above and left of the rectangle's bottom: the connector runs up and left, flipped both ways.
    shape(&mut again, sheet, ShapeChange::Place { id: 3, left: Some(pts(0.0)), top: Some(pts(0.0)), width: None, height: None });
    let third = reopened(&mut again);
    assert_eq!(rect_of(&third, 4), ((50.0, 0.0, 20.0, 170.0), true, true));
    let connector = third.host().shapes(sheet).iter().find(|s| s.id == 4).unwrap().clone();
    assert_eq!((connector.begin.unwrap().shape, connector.end.unwrap().shape), (2, 3));
}
