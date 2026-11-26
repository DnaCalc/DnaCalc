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

fn save_bytes(book: &mut HostSession) -> Vec<u8> {
    let mut bytes = Vec::new();
    book.save(&SavePolicy::default(), &mut bytes).unwrap();
    bytes
}

fn reopened(book: &mut HostSession) -> HostSession {
    let bytes = save_bytes(book);
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
    // A stored shape a stored connector is glued to is deleted and the connector stays, unglued (dc-zjsg.5).
    shape(&mut reloaded, sheet, ShapeChange::Delete { id: 2 });
    assert_eq!(reloaded.host().shapes(sheet).len(), 3);
    let again = reopened(&mut reloaded);
    let connector = again.host().shapes(sheet).iter().find(|s| s.kind == ShapeKind::Connector).unwrap().clone();
    assert_eq!((connector.begin, connector.end), (None, Some(ShapeConnection { shape: 3, site: 0 })));
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

/// dc-piw0 Q10 / dc-rlkcm (Excel build 20527): `ChartObjects.Add(200, 20, 300, 200)` and nothing
/// else, saved with `SaveCopyAs`, matches `tests/fixtures/excel-q10/chart1.xml` element by element
/// (axis identities normalised) and `tests/fixtures/excel-q10/drawing1.xml` outside its anchor
/// coordinates (the recorded Q10 fixture ran in Safe Mode, where Aptos Narrow fell back to Arial
/// 11's 54 pt columns / 14.25 pt rows; normal-mode Excel uses 48 pt columns / 15 pt rows).
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
    let written_drawing = find("xl/drawings/");
    // Normal-mode Aptos Narrow 11 (48 pt cols, 15 pt rows): (200, 20) -> (4+8pt, 1+5pt),
    // (500, 220) -> (10+20pt, 14+10pt).
    assert!(
        written_drawing.contains(
            "<xdr:from><xdr:col>4</xdr:col><xdr:colOff>101600</xdr:colOff><xdr:row>1</xdr:row><xdr:rowOff>63500</xdr:rowOff></xdr:from><xdr:to><xdr:col>10</xdr:col><xdr:colOff>254000</xdr:colOff><xdr:row>14</xdr:row><xdr:rowOff>127000</xdr:rowOff></xdr:to>"
        ),
        "{written_drawing}"
    );
    let strip_corners = |xml: &str| {
        let a = xml.find("<xdr:from>").unwrap();
        let b = xml.find("</xdr:to>").unwrap() + "</xdr:to>".len();
        format!("{}{}", &xml[..a], &xml[b..])
    };
    assert_eq!(elements(&strip_corners(&written_drawing)), elements(&strip_corners(recorded_drawing)));
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

/// dc-rlkcm (normal-mode Excel build 20527, 96 DPI; UI30, dw2 `om.json`, dw3
/// `dw3-spark-weights.xlsx`): both Aptos Narrow 11 and Calibri 11 default columns are 64 px =
/// 48 pt (7 px digit width, 8.43 characters) and default rows 15 pt (20 px).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_zaj7__q9_default_column_and_row_follow_the_default_font() {
    let aptos = open(aptos_narrow_package());
    let sheet = aptos.host().sheets()[0].id();
    assert_eq!(aptos.host().column_left(sheet, 1), 48.0);
    assert_eq!(aptos.host().column_left(sheet, 4), 192.0);
    assert_eq!(aptos.host().row_top(sheet, 1), 15.0);
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
    let renamed = id(&book, "Pattern lgGrid");
    shape(&mut book, sheet, ShapeChange::Name { id: renamed, name: "Grid & lines".into() });
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
    let grid = read.iter().find(|s| s.id == renamed).unwrap();
    assert_eq!((grid.name.as_str(), &grid.fill), ("Grid & lines", &before("Pattern lgGrid").fill));
    let group = read.iter().find(|s| s.kind == ShapeKind::Group).unwrap().clone();
    let names: Vec<_> = group.children.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["Pattern pct50", "Pattern ltHorz"]);
    assert_eq!(group.children[0].fill, before("Pattern pct50").fill);
    // Untouched objects are as stored.
    for name in ["Freeform curves", "Rect rot 60", "Pattern smCheck"] {
        assert_eq!(found(name), before(name), "{name}");
    }
    // An arrow-key step: the reopened (stored) "Style accent2" moves 3 pt right and 1.5 pt up.
    let styled = reloaded.host().shapes(sheet).iter().find(|s| s.name == "Style accent2").unwrap().clone();
    let (sl, st, sw, sh) = reloaded.host().anchor_rectangle(sheet, &styled.anchor);
    shape(&mut reloaded, sheet, ShapeChange::Nudge { id: styled.id, dx: pts(3.0), dy: pts(-1.5) });
    let nudged = reloaded.host().shapes(sheet).iter().find(|s| s.id == styled.id).unwrap().clone();
    assert_eq!(reloaded.host().anchor_rectangle(sheet, &nudged.anchor), (sl + 3.0, st - 1.5, sw, sh));
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

/// dc-zjsg.4: a duplicate of the file's own object is written from its stored element (its custom geometry and fill
/// kept), with its own identity and place; a duplicate of a new shape is renumbered and renamed from its identity.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_zjsg_4__duplicates_of_stored_and_new_shapes_save_and_reopen() {
    let mut book = open(include_bytes!("../../calc-doc/tests/fixtures/drawing/shapes-paint4.xlsx").to_vec());
    let sheet = book.host().sheets()[0].id();
    let triangle = book.host().shapes(sheet).iter().find(|s| s.name == "Freeform triangle").unwrap().clone();
    shape(&mut book, sheet, ShapeChange::Duplicate { id: triangle.id, dx: pts(12.0), dy: pts(12.0) });
    shape(&mut book, sheet, add(ShapeKind::Shape, "ellipse", 10.0, 10.0, 40.0, 30.0));
    let oval = book.host().shapes(sheet).last().unwrap().clone();
    shape(&mut book, sheet, ShapeChange::Duplicate { id: oval.id, dx: pts(12.0), dy: pts(12.0) });
    let reloaded = reopened(&mut book);
    let read = reloaded.host().shapes(sheet);
    assert_eq!(read.len(), 23);
    let copies: Vec<_> = read.iter().filter(|s| s.name == "Freeform triangle").collect();
    assert_eq!(copies.len(), 2);
    let copy = copies.iter().find(|s| s.id != triangle.id).unwrap();
    assert_eq!((&copy.custom_geometry, &copy.fill, &copy.text), (&triangle.custom_geometry, &triangle.fill, &triangle.text));
    let (l, t, w, h) = book.host().anchor_rectangle(sheet, &triangle.anchor);
    let (l2, t2, w2, h2) = reloaded.host().anchor_rectangle(sheet, &copy.anchor);
    assert!((l2 - l - 12.0).abs() < 0.01 && (t2 - t - 12.0).abs() < 0.01 && (w2 - w).abs() < 0.01 && (h2 - h).abs() < 0.01);
    let oval_copy = read.last().unwrap();
    assert_eq!((oval_copy.geometry.as_deref(), oval_copy.name.clone()), (Some("ellipse"), format!("Oval {}", oval_copy.id - 1)));
    // Identities stay unique in the saved drawing.
    let mut ids: Vec<u32> = read.iter().map(|s| s.id).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), 23);
}

/// dc-zjsg.4: a row inside an object's span made taller: an object that moves but doesn't size (editAs oneCell) keeps
/// its size, its anchor's end found anew; an object that moves and sizes with its cells (a new one) grows; the stored
/// anchor is rewritten on save, and hiding the row works the same way.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_zjsg_4__objects_that_dont_size_with_cells_keep_their_size_when_a_row_grows() {
    let mut book = open(include_bytes!("../../calc-doc/tests/fixtures/drawing/shapes-paint4.xlsx").to_vec());
    let sheet = book.host().sheets()[0].id();
    let triangle = book.host().shapes(sheet).iter().find(|s| s.name == "Freeform triangle").unwrap().clone();
    assert_eq!(triangle.edit_as.as_deref(), Some("oneCell"));
    let before = book.host().anchor_rectangle(sheet, &triangle.anchor);
    // A new shape over the same rows (rows 5 to 8 at 15 pt), moving and sizing with its cells.
    shape(&mut book, sheet, add(ShapeKind::Shape, "rect", 300.0, before.1, 40.0, 40.0));
    let added = book.host().shapes(sheet).last().unwrap().clone();
    let added_before = book.host().shape_rectangle(sheet, &added);
    let row = (before.1 / 15.0).floor() as u32 + 2;
    command(&mut book, |_| WorkbookCommand::AxisFormat { sheet, rows: true, first: row, count: 1, size_bits: Some(30.0f64.to_bits()), hidden: None });
    let now = book.host().shapes(sheet).iter().find(|s| s.id == triangle.id).unwrap().clone();
    let after = book.host().anchor_rectangle(sheet, &now.anchor);
    assert!((after.2 - before.2).abs() < 0.01 && (after.3 - before.3).abs() < 0.01 && (after.0 - before.0).abs() < 0.01, "{before:?} {after:?}");
    let grown = book.host().shapes(sheet).iter().find(|s| s.id == added.id).unwrap().clone();
    assert!((book.host().anchor_rectangle(sheet, &grown.anchor).3 - added_before.3 - 15.0).abs() < 0.01);
    let reloaded = reopened(&mut book);
    let read = reloaded.host().shapes(sheet).iter().find(|s| s.id == triangle.id).unwrap().clone();
    let reread = reloaded.host().anchor_rectangle(sheet, &read.anchor);
    assert!((reread.3 - before.3).abs() < 0.01, "{reread:?}");
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

/// dw3 review (a): a row resized on a sheet with kept objects (oneCell anchors) leaves them as stored; the save works.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dw3__a_row_resize_with_kept_objects_still_saves() {
    let mut book = open(include_bytes!("../../calc-doc/tests/fixtures/drawing/kept-objects.xlsx").to_vec());
    let sheet = book.host().sheets()[0].id();
    command(&mut book, |_| WorkbookCommand::AxisFormat { sheet, rows: true, first: 2, count: 3, size_bits: Some(40.0f64.to_bits()), hidden: None });
    let reloaded = reopened(&mut book);
    assert_eq!(reloaded.host().shapes(sheet).len(), 5);
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
    // Excel (dw2 Windows item 4): the lower shape moved right turns the connector from 90 to 270 degrees, flipped in x.
    assert_eq!((moved.0, moved.1, moved.2), ((70.0, 170.0), (250.0, 260.0), 270 * 60_000));
    let c = stored.host().shapes(sheet).iter().find(|s| s.id == 4).unwrap().clone();
    assert!(c.flip_horizontal && !c.flip_vertical);
    // Anchored on the box from site to site.
    assert_eq!(moved.3, (70.0, 170.0, 180.0, 90.0));
    let again = reopened(&mut stored);
    assert_eq!(ends(&again), moved);
}

/// dc-zjsg.5 (dw3): an elbow glued from a right site to a top site is a two-leg elbow (right, then down); when the
/// lower shape moves to a place the route needs another class (its top site is now left of the start), the connector
/// takes the class and turn that leave and arrive the way the sites face. Excel's own choice is not measured.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dw3_dc_zjsg_5__an_elbow_changes_class_when_the_route_needs_it() {
    let mut book = open(calc_host::blank_package());
    let sheet = book.host().sheets()[0].id();
    shape(&mut book, sheet, add(ShapeKind::Shape, "rect", 20.0, 20.0, 100.0, 50.0));
    shape(&mut book, sheet, add(ShapeKind::Shape, "rect", 220.0, 160.0, 100.0, 50.0));
    shape(&mut book, sheet, add(ShapeKind::Connector, "bentConnector3", 10.0, 10.0, 10.0, 10.0));
    // A rectangle's sites (1-based): top, left, bottom, right.
    shape(&mut book, sheet, ShapeChange::Connect { id: 4, begin: true, shape: 2, site: 4 });
    shape(&mut book, sheet, ShapeChange::Connect { id: 4, begin: false, shape: 3, site: 1 });
    let connector = |b: &HostSession| b.host().shapes(sheet).iter().find(|s| s.id == 4).unwrap().clone();
    let c = connector(&book);
    assert_eq!((c.geometry.as_deref(), c.rotation), (Some("bentConnector2"), 0));
    let (l, t, w, h) = book.host().anchor_rectangle(sheet, &c.anchor);
    assert_eq!((l.round(), t.round(), w.round(), h.round()), (120.0, 45.0, 150.0, 115.0));
    // The same edit on a stored connector: the save rewrites its geometry and adjust values.
    let mut book = reopened(&mut book);
    // The lower shape moves under the first one: right of the start is no longer the way to its top site.
    shape(&mut book, sheet, ShapeChange::Place { id: 3, left: Some(pts(20.0)), top: None, width: None, height: None });
    let c = connector(&book);
    // Right out of the box, down, back left past the lower shape's top site, down into it: four legs.
    assert_eq!((c.geometry.as_deref(), c.rotation), (Some("bentConnector4"), 0));
    assert_eq!(c.adjust.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), ["adj1", "adj2"]);
    // Mid-gap y = (70 + 160)/2 = 115 -> (115 - 45) / 115 = 60870 (Excel-measured: dw3 Windows item 4).
    assert!(c.adjust[0].1 < 0 && (c.adjust[1].1 - 60_870).abs() <= 2, "{:?}", c.adjust);
    let again = reopened(&mut book);
    let read = again.host().shapes(sheet).iter().find(|s| s.id == 4).unwrap().clone();
    assert_eq!((read.geometry, read.rotation, read.begin, read.end), (c.geometry, c.rotation, c.begin, c.end));
}

/// dc-zjsg.5 (dw4): curved connectors (curvedConnector2..5) follow the same route-class and turn rules as elbow
/// connectors when glued shapes move (curved preset names kept; Excel's curved-connector detour values ASSUMED
/// analogous to bentConnector until measured on Windows).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dw4_dc_zjsg_5__a_curved_connector_changes_class_and_turn_when_rerouted() {
    let mut book = open(calc_host::blank_package());
    let sheet = book.host().sheets()[0].id();
    shape(&mut book, sheet, add(ShapeKind::Shape, "rect", 20.0, 20.0, 100.0, 50.0));
    shape(&mut book, sheet, add(ShapeKind::Shape, "rect", 220.0, 160.0, 100.0, 50.0));
    shape(&mut book, sheet, add(ShapeKind::Connector, "curvedConnector3", 10.0, 10.0, 10.0, 10.0));
    // A rectangle's sites (1-based): top, left, bottom, right.
    shape(&mut book, sheet, ShapeChange::Connect { id: 4, begin: true, shape: 2, site: 4 });
    shape(&mut book, sheet, ShapeChange::Connect { id: 4, begin: false, shape: 3, site: 1 });
    let connector = |b: &HostSession, id: u32| b.host().shapes(sheet).iter().find(|s| s.id == id).unwrap().clone();
    let c = connector(&book, 4);
    assert_eq!((c.geometry.as_deref(), c.rotation), (Some("curvedConnector2"), 0));
    let (l, t, w, h) = book.host().anchor_rectangle(sheet, &c.anchor);
    assert_eq!((l.round(), t.round(), w.round(), h.round()), (120.0, 45.0, 150.0, 115.0));
    let mut book = reopened(&mut book);
    shape(&mut book, sheet, ShapeChange::Place { id: 3, left: Some(pts(20.0)), top: None, width: None, height: None });
    let c = connector(&book, 4);
    assert_eq!((c.geometry.as_deref(), c.rotation), (Some("curvedConnector4"), 0));
    assert_eq!(c.adjust.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), ["adj1", "adj2"]);
    // Mid-gap y = (70 + 160)/2 = 115 -> (115 - 45) / 115 = 60870 (Excel-measured: dw3 Windows item 4).
    assert!(c.adjust[0].1 < 0 && (c.adjust[1].1 - 60_870).abs() <= 2, "{:?}", c.adjust);
    let again = reopened(&mut book);
    let read = connector(&again, 4);
    assert_eq!((read.geometry, read.rotation, read.adjust, read.begin, read.end), (c.geometry, c.rotation, c.adjust, c.begin, c.end));

    // Turned curvedConnector3 (bottom -> top, rot 90): moving the lower shape right turns 90 -> 270 with flipH.
    let mut book = open(calc_host::blank_package());
    let sheet = book.host().sheets()[0].id();
    shape(&mut book, sheet, add(ShapeKind::Shape, "rect", 20.0, 120.0, 100.0, 50.0));
    shape(&mut book, sheet, add(ShapeKind::Shape, "rect", 20.0, 260.0, 100.0, 50.0));
    shape(&mut book, sheet, add(ShapeKind::Connector, "curvedConnector3", 10.0, 10.0, 10.0, 10.0));
    shape(&mut book, sheet, ShapeChange::Rotate { id: 4, rotation: 90 * 60_000 });
    shape(&mut book, sheet, ShapeChange::Connect { id: 4, begin: true, shape: 2, site: 3 });
    shape(&mut book, sheet, ShapeChange::Connect { id: 4, begin: false, shape: 3, site: 1 });
    let mut stored = reopened(&mut book);
    shape(&mut stored, sheet, ShapeChange::Place { id: 3, left: Some(pts(200.0)), top: None, width: None, height: None });
    let c = connector(&stored, 4);
    assert_eq!((c.geometry.as_deref(), c.rotation, c.flip_horizontal, c.flip_vertical), (Some("curvedConnector3"), 270 * 60_000, true, false));
}

/// dc-zjsg.5 (dw4): Excel-measured elbow connector routes from dw3 Windows item 4 (c2, c3, c4, c8):
/// 18 pt detour standoff, middle leg at the midpoint of the gap between the two shapes' boxes, and unturned
/// horizontal-start connectors keeping rot 0 (flipV in c4, never rotating to 180).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dw4_dc_zjsg_5__elbow_reroute_matches_excel_measured_cases_c2_c3_c4_c8() {
    let route = |a: (f64, f64, f64, f64), s1: u32, b: (f64, f64, f64, f64), s2: u32| {
        let mut book = open(calc_host::blank_package());
        let sheet = book.host().sheets()[0].id();
        shape(&mut book, sheet, add(ShapeKind::Shape, "rect", a.0, a.1, a.2, a.3));
        shape(&mut book, sheet, add(ShapeKind::Shape, "rect", b.0, b.1, b.2, b.3));
        shape(&mut book, sheet, add(ShapeKind::Connector, "bentConnector3", 10.0, 10.0, 10.0, 10.0));
        shape(&mut book, sheet, ShapeChange::Connect { id: 4, begin: true, shape: 2, site: s1 });
        shape(&mut book, sheet, ShapeChange::Connect { id: 4, begin: false, shape: 3, site: s2 });
        book.host().shapes(sheet).iter().find(|s| s.id == 4).unwrap().clone()
    };
    let approx = |actual: i64, expected: i64| (actual - expected).abs() <= 2;

    // c2: right -> top, B lower left (A 100,40; B 20,150, both 100x40).
    let c2 = route((100.0, 40.0, 100.0, 40.0), 4, (20.0, 150.0, 100.0, 40.0), 1);
    assert_eq!((c2.geometry.as_deref(), c2.rotation, c2.flip_horizontal, c2.flip_vertical), (Some("bentConnector4"), 0, true, false));
    assert!(approx(c2.adjust[0].1, -13_846) && approx(c2.adjust[1].1, 61_111), "c2: {:?}", c2.adjust);

    // c3: right -> top, B straight below (A 100,40; B 100,150).
    let c3 = route((100.0, 40.0, 100.0, 40.0), 4, (100.0, 150.0, 100.0, 40.0), 1);
    assert_eq!((c3.geometry.as_deref(), c3.rotation, c3.flip_horizontal, c3.flip_vertical), (Some("bentConnector4"), 0, true, false));
    assert!(approx(c3.adjust[0].1, -36_000) && approx(c3.adjust[1].1, 61_111), "c3: {:?}", c3.adjust);

    // c4: right -> top, B upper right (A 100,150; B 300,40).
    let c4 = route((100.0, 150.0, 100.0, 40.0), 4, (300.0, 40.0, 100.0, 40.0), 1);
    assert_eq!((c4.geometry.as_deref(), c4.rotation, c4.flip_horizontal, c4.flip_vertical), (Some("bentConnector4"), 0, false, true));
    assert!(approx(c4.adjust[0].1, 33_333) && approx(c4.adjust[1].1, 113_846), "c4: {:?}", c4.adjust);

    // c8: right -> left, B lower left (A 200,40; B 20,150).
    let c8 = route((200.0, 40.0, 100.0, 40.0), 4, (20.0, 150.0, 100.0, 40.0), 2);
    assert_eq!((c8.geometry.as_deref(), c8.rotation, c8.flip_horizontal, c8.flip_vertical), (Some("bentConnector5"), 0, true, false));
    assert!(approx(c8.adjust[0].1, -6_429) && approx(c8.adjust[1].1, 50_000) && approx(c8.adjust[2].1, 106_429), "c8: {:?}", c8.adjust);
}

/// dc-zjsg.4 (dw3): a stored object edited after rows were inserted in the same session: the edit is made where the
/// object is now (rows down), a new shape is placed in the now-current sheet, and the saved file holds each where the
/// session shows it.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dw3_dc_zjsg_4__editing_after_a_row_insert_in_the_same_session() {
    use calc_doc_model::{Axis, AxisChange, AxisEdit};
    let bytes = include_bytes!("../../calc-doc/tests/fixtures/drawing/shapes-paint4.xlsx").to_vec();
    let mut book = open(bytes);
    let sheet = book.host().sheets()[0].id();
    let triangle = book.host().shapes(sheet).iter().find(|s| s.name == "Freeform triangle").unwrap().clone();
    let other = book.host().shapes(sheet).iter().find(|s| s.id != triangle.id).unwrap().clone();
    let before = book.host().anchor_rectangle(sheet, &triangle.anchor);
    let other_before = book.host().anchor_rectangle(sheet, &other.anchor);
    // Two rows inserted at row 2: every object starts below row 3, so each is 30 pt lower now.
    let edit = AxisEdit::new(sheet, Axis::Rows, AxisChange::Insert, 2, 2).unwrap();
    assert!(book.dispatch(SkinIntent::EditAxis(edit)).accepted());
    // Move the triangle 20 pt to the right (its top as it is shown now) and add a shape at a place on the new sheet.
    let top_now = before.1 + 30.0;
    shape(&mut book, sheet, ShapeChange::Place { id: triangle.id, left: Some(pts(before.0 + 20.0)), top: Some(pts(top_now)), width: None, height: None });
    shape(&mut book, sheet, add(ShapeKind::Shape, "rect", 400.0, 200.0, 40.0, 40.0));
    let added = book.host().shapes(sheet).last().unwrap().id;
    // A place inside the rows inserted this session has no place in the file's rows yet: refused, nothing changed.
    let revision = book.host().revision().get();
    let refused = book.dispatch(SkinIntent::Command(WorkbookCommand::Shape {
        sheet,
        revision,
        change: ShapeChange::Place { id: added, left: None, top: Some(pts(20.0)), width: None, height: None },
    }));
    assert!(!refused.accepted());
    let reloaded = reopened(&mut book);
    let rect = |b: &HostSession, id: u32| {
        let s = b.host().shapes(sheet).iter().find(|s| s.id == id).unwrap().clone();
        b.host().anchor_rectangle(sheet, &s.anchor)
    };
    let moved = rect(&reloaded, triangle.id);
    assert!((moved.0 - before.0 - 20.0).abs() < 0.5 && (moved.1 - top_now).abs() < 0.5 && (moved.2 - before.2).abs() < 0.5, "{before:?} {moved:?}");
    let untouched = rect(&reloaded, other.id);
    assert!((untouched.1 - other_before.1 - 30.0).abs() < 0.5 && (untouched.0 - other_before.0).abs() < 0.5, "{other_before:?} {untouched:?}");
    let new = rect(&reloaded, added);
    assert!((new.0 - 400.0).abs() < 0.5 && (new.1 - 200.0).abs() < 0.5, "{new:?}");
}

/// dc-zjsg.4 (dw4): a stored object edited after rows were deleted in the same session (both rows above the shape
/// and rows overlapping its top), plus a new shape placed in the current sheet; the saved file holds each where
/// the session shows it.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dw4_dc_zjsg_4__editing_after_a_row_delete_in_the_same_session() {
    use calc_doc_model::{Axis, AxisChange, AxisEdit};
    let bytes = include_bytes!("../../calc-doc/tests/fixtures/drawing/shapes-paint4.xlsx").to_vec();
    let mut book = open(bytes);
    let sheet = book.host().sheets()[0].id();
    let triangle = book.host().shapes(sheet).iter().find(|s| s.name == "Freeform triangle").unwrap().clone();
    let other = book.host().shapes(sheet).iter().find(|s| s.id != triangle.id).unwrap().clone();
    let before = book.host().anchor_rectangle(sheet, &triangle.anchor);
    let other_before = book.host().anchor_rectangle(sheet, &other.anchor);
    // Delete two rows at row 1: every object starts below row 2, so each moves 30 pt up.
    let edit = AxisEdit::new(sheet, Axis::Rows, AxisChange::Delete, 1, 2).unwrap();
    assert!(book.dispatch(SkinIntent::EditAxis(edit)).accepted());
    let top_now = before.1 - 30.0;
    shape(&mut book, sheet, ShapeChange::Place { id: triangle.id, left: Some(pts(before.0 + 20.0)), top: Some(pts(top_now)), width: None, height: None });
    shape(&mut book, sheet, add(ShapeKind::Shape, "rect", 400.0, 15.0, 40.0, 40.0));
    let added = book.host().shapes(sheet).last().unwrap().id;
    let reloaded = reopened(&mut book);
    let rect = |b: &HostSession, id: u32| {
        let s = b.host().shapes(sheet).iter().find(|s| s.id == id).unwrap().clone();
        b.host().anchor_rectangle(sheet, &s.anchor)
    };
    let moved = rect(&reloaded, triangle.id);
    assert!((moved.0 - before.0 - 20.0).abs() < 0.5 && (moved.1 - top_now).abs() < 0.5 && (moved.2 - before.2).abs() < 0.5, "{before:?} {moved:?}");
    let untouched = rect(&reloaded, other.id);
    assert!((untouched.1 - (other_before.1 - 30.0)).abs() < 0.5 && (untouched.0 - other_before.0).abs() < 0.5, "{other_before:?} {untouched:?}");
    let new = rect(&reloaded, added);
    assert!((new.0 - 400.0).abs() < 0.5 && (new.1 - 15.0).abs() < 0.5, "{new:?}");

    // Deleting rows that overlap a stored shape's top edge, then moving it horizontally.
    let mut book = open(calc_host::blank_package());
    let sheet = book.host().sheets()[0].id();
    shape(&mut book, sheet, add(ShapeKind::Shape, "rect", 40.0, 60.0, 80.0, 60.0));
    let mut book = reopened(&mut book);
    let id = book.host().shapes(sheet)[0].id;
    // Rows 5..6 (0-based 4..5, 60..90 pt) overlap the shape's top (60..120 pt).
    let del = AxisEdit::new(sheet, Axis::Rows, AxisChange::Delete, 5, 2).unwrap();
    assert!(book.dispatch(SkinIntent::EditAxis(del)).accepted());
    shape(&mut book, sheet, ShapeChange::Place { id, left: Some(pts(90.0)), top: None, width: None, height: None });
    let reloaded = reopened(&mut book);
    let r = reloaded.host().anchor_rectangle(sheet, &reloaded.host().shapes(sheet)[0].anchor);
    assert!((r.0 - 90.0).abs() < 0.5 && (r.1 - 60.0).abs() < 0.5 && (r.2 - 80.0).abs() < 0.5 && (r.3 - 30.0).abs() < 0.5, "{r:?}");

    // Deleting two columns opens two blank columns at the sheet's right edge (16383..16384): placing a shape
    // there has no pre-image in the file's columns and must be refused rather than silently shifting left on save.
    let mut book = open(calc_host::blank_package());
    let sheet = book.host().sheets()[0].id();
    let del_cols = AxisEdit::new(sheet, Axis::Columns, AxisChange::Delete, 1, 2).unwrap();
    assert!(book.dispatch(SkinIntent::EditAxis(del_cols)).accepted());
    let tail_left = book.host().column_left(sheet, 16382) + 1.0;
    let revision = book.host().revision().get();
    let refused = book.dispatch(SkinIntent::Command(WorkbookCommand::Shape {
        sheet,
        revision,
        change: add(ShapeKind::Shape, "rect", tail_left, 15.0, 20.0, 20.0),
    }));
    assert!(!refused.accepted(), "placing in tail columns opened by a delete must be refused");
}

/// dc-zjsg.4 (dw4): editing stored and new shapes after a chain of axis edits (row insert, row delete, column
/// insert) in the same session; placements into still-inserted rows or columns are refused, and the saved file
/// holds every shape where the session shows it.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dw4_dc_zjsg_4__editing_after_an_axis_edit_chain_in_the_same_session() {
    use calc_doc_model::{Axis, AxisChange, AxisEdit};
    let bytes = include_bytes!("../../calc-doc/tests/fixtures/drawing/shapes-paint4.xlsx").to_vec();
    let mut book = open(bytes);
    let sheet = book.host().sheets()[0].id();
    let col_w = book.host().column_left(sheet, 1);
    let triangle = book.host().shapes(sheet).iter().find(|s| s.name == "Freeform triangle").unwrap().clone();
    let nudged = book.host().shapes(sheet).iter().find(|s| s.id != triangle.id && s.kept.is_none()).unwrap().clone();
    let untouched = book.host().shapes(sheet).iter().find(|s| s.id != triangle.id && s.id != nudged.id).unwrap().clone();
    let tri_before = book.host().anchor_rectangle(sheet, &triangle.anchor);
    let nudged_before = book.host().anchor_rectangle(sheet, &nudged.anchor);
    let untouched_before = book.host().anchor_rectangle(sheet, &untouched.anchor);
    // Add one shape before the axis chain so both pre-chain and post-chain session shapes are exercised.
    shape(&mut book, sheet, add(ShapeKind::Shape, "rect", 200.0, 90.0, 50.0, 30.0));
    let pre_added = book.host().shapes(sheet).last().unwrap().id;

    // Chain: insert 3 rows at row 2, delete 1 row at row 3 (net +2 rows = +30 pt; rows 2..3 still inserted),
    // insert 2 columns at column 1 (shift right by 2 * col_w; cols 1..2 inserted).
    for edit in [
        AxisEdit::new(sheet, Axis::Rows, AxisChange::Insert, 2, 3).unwrap(),
        AxisEdit::new(sheet, Axis::Rows, AxisChange::Delete, 3, 1).unwrap(),
        AxisEdit::new(sheet, Axis::Columns, AxisChange::Insert, 1, 2).unwrap(),
    ] {
        assert!(book.dispatch(SkinIntent::EditAxis(edit)).accepted());
    }

    // Move triangle (+20 pt right, +15 pt down from its shifted position), nudge `nudged` (+10, -15), add a shape.
    let tri_target = (tri_before.0 + 2.0 * col_w + 20.0, tri_before.1 + 30.0 + 15.0);
    shape(&mut book, sheet, ShapeChange::Place { id: triangle.id, left: Some(pts(tri_target.0)), top: Some(pts(tri_target.1)), width: None, height: None });
    shape(&mut book, sheet, ShapeChange::Nudge { id: nudged.id, dx: pts(10.0), dy: pts(-15.0) });
    shape(&mut book, sheet, add(ShapeKind::Shape, "rect", 400.0, 200.0, 40.0, 40.0));
    let post_added = book.host().shapes(sheet).last().unwrap().id;

    // Placing into a still-inserted row (row 2 -> 20 pt) or inserted column (col 1 -> 10 pt) is refused.
    for (left, top) in [(None, Some(pts(20.0))), (Some(pts(10.0)), None)] {
        let revision = book.host().revision().get();
        let refused = book.dispatch(SkinIntent::Command(WorkbookCommand::Shape {
            sheet,
            revision,
            change: ShapeChange::Place { id: post_added, left, top, width: None, height: None },
        }));
        assert!(!refused.accepted(), "expected refusal for ({left:?}, {top:?})");
    }

    let reloaded = reopened(&mut book);
    let rect = |b: &HostSession, id: u32| {
        let s = b.host().shapes(sheet).iter().find(|s| s.id == id).unwrap().clone();
        b.host().anchor_rectangle(sheet, &s.anchor)
    };
    let tri_after = rect(&reloaded, triangle.id);
    assert!((tri_after.0 - tri_target.0).abs() < 0.5 && (tri_after.1 - tri_target.1).abs() < 0.5 && (tri_after.2 - tri_before.2).abs() < 0.5, "{tri_before:?} {tri_after:?}");
    let nudged_after = rect(&reloaded, nudged.id);
    assert!((nudged_after.0 - (nudged_before.0 + 2.0 * col_w + 10.0)).abs() < 0.5 && (nudged_after.1 - (nudged_before.1 + 30.0 - 15.0)).abs() < 0.5, "{nudged_before:?} {nudged_after:?}");
    let untouched_after = rect(&reloaded, untouched.id);
    assert!((untouched_after.0 - (untouched_before.0 + 2.0 * col_w)).abs() < 0.5 && (untouched_after.1 - (untouched_before.1 + 30.0)).abs() < 0.5, "{untouched_before:?} {untouched_after:?}");
    let pre_after = rect(&reloaded, pre_added);
    assert!((pre_after.0 - (200.0 + 2.0 * col_w)).abs() < 0.5 && (pre_after.1 - 120.0).abs() < 0.5, "{pre_after:?}");
    let post_after = rect(&reloaded, post_added);
    assert!((post_after.0 - 400.0).abs() < 0.5 && (post_after.1 - 200.0).abs() < 0.5, "{post_after:?}");
}

/// dc-zjsg.6 (dw3): a chart whose anchor says `editAs="oneCell"` keeps its size when a row inside its span is made taller
/// (as a shape does); one without keeps sizing with its cells.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dw3_dc_zjsg_6__a_chart_with_edit_as_one_cell_keeps_its_size() {
    let mut book = open(include_bytes!("../../calc-doc/tests/fixtures/drawing/chart-kinds.xlsx").to_vec());
    let sheet = book.host().sheets()[0].id();
    let rect = |b: &HostSession, name: &str| {
        let (charts, _) = b.host().charts(sheet);
        let part = charts.iter().find(|c| c.name == name).unwrap().part.clone();
        let chart = b.host().model().definitions().charts(sheet).iter().find(|c| c.part == part).unwrap().clone();
        (chart.edit_as.clone(), chart.anchor.clone(), b.host().anchor_rectangle(sheet, &chart.anchor))
    };
    let (edit_as, anchor, before) = rect(&book, "Column sd");
    assert_eq!(edit_as.as_deref(), Some("oneCell"));
    let (plain_as, plain_anchor, plain_before) = rect(&book, "Column labels");
    assert_eq!(plain_as, None);
    let row_of = |a: &calc_doc_model::ChartAnchor| match a {
        calc_doc_model::ChartAnchor::TwoCell { from, .. } => from.row + 2,
        other => panic!("{other:?}"),
    };
    for row in [row_of(&anchor), row_of(&plain_anchor)] {
        command(&mut book, |_| WorkbookCommand::AxisFormat { sheet, rows: true, first: row, count: 1, size_bits: Some(45.0f64.to_bits()), hidden: None });
    }
    // The display: the one-cell chart is drawn at its file size from its corner; the plain one spans its cells.
    let places = |b: &HostSession, name: &str| b.host().charts(sheet).0.iter().find(|c| c.name == name).unwrap().place;
    match places(&book, "Column sd") {
        calc_host::ChartPlace::Sized { width, height, .. } => {
            let px = 96.0 / 72.0;
            assert!((height - before.3 * px).abs() < 0.5 && (width - before.2 * px).abs() < 0.5, "{height} {width} {before:?}");
        }
        other => panic!("{other:?}"),
    }
    assert!(matches!(places(&book, "Column labels"), calc_host::ChartPlace::Cells { .. }));
    let _ = plain_before;
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
    // The stored oval deleted: the stored connector stays where it is, its end unglued, and saves so.
    let mut deleted = third;
    let place = rect_of(&deleted, 4);
    shape(&mut deleted, sheet, ShapeChange::Delete { id: 3 });
    let fourth = reopened(&mut deleted);
    let connector = fourth.host().shapes(sheet).iter().find(|s| s.id == 4).unwrap().clone();
    assert_eq!((connector.begin.map(|e| e.shape), connector.end), (Some(2), None));
    assert_eq!(rect_of(&fourth, 4), place);
}

/// dc-rlkcm (dw2 `om-elbow-before.xlsx` / `om-elbow-after.xlsx`, dw3 Windows item 10): an
/// Excel-saved Aptos Narrow 11 workbook with `<sheetFormatPr defaultRowHeight="15"/>` and no
/// `defaultColWidth` uses 48 pt (64 px) default columns; Excel's 100 x 40 pt Top/Bottom
/// rectangles read back at `(100, 40, 100, 40)` / `(100, 200, 100, 40)` (not 112 pt), and
/// moving Bottom right by 96 pt reroutes the elbow connector to `off = (138, 92)` pt,
/// `ext = (120, 96)` pt, `rot = 16200000`, `flipH = true`, `adj1 = 50000` — matching Excel's
/// `om-elbow-after.xlsx` to the EMU.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dw5_dc_rlkcm__aptos_narrow_default_column_width_matches_excel_48pt_and_elbow_reroute() {
    let mut book = open(aptos_narrow_package());
    let sheet = book.host().sheets()[0].id();
    shape(&mut book, sheet, add(ShapeKind::Shape, "rect", 100.0, 40.0, 100.0, 40.0));
    shape(&mut book, sheet, ShapeChange::Name { id: 2, name: "Top".into() });
    shape(&mut book, sheet, add(ShapeKind::Shape, "rect", 100.0, 200.0, 100.0, 40.0));
    shape(&mut book, sheet, ShapeChange::Name { id: 3, name: "Bottom".into() });
    shape(&mut book, sheet, add(ShapeKind::Connector, "bentConnector3", 0.0, 0.0, 10.0, 10.0));
    shape(&mut book, sheet, ShapeChange::Name { id: 4, name: "Elbow".into() });
    shape(&mut book, sheet, ShapeChange::Connect { id: 4, begin: true, shape: 2, site: 3 });
    shape(&mut book, sheet, ShapeChange::Connect { id: 4, begin: false, shape: 3, site: 1 });
    let mut stored = reopened(&mut book);
    let top = stored.host().shapes(sheet).iter().find(|s| s.name == "Top").unwrap().clone();
    assert_eq!(stored.host().anchor_rectangle(sheet, &top.anchor), (100.0, 40.0, 100.0, 40.0));
    assert_eq!(
        top.anchor,
        calc_doc_model::ChartAnchor::TwoCell {
            from: calc_doc_model::DrawingPoint { column: 2, column_offset: 50_800, row: 2, row_offset: 127_000 },
            to: calc_doc_model::DrawingPoint { column: 4, column_offset: 101_600, row: 5, row_offset: 63_500 },
        }
    );
    shape(&mut stored, sheet, ShapeChange::Place { id: 3, left: Some(pts(196.0)), top: Some(pts(200.0)), width: None, height: None });
    let reloaded = reopened(&mut stored);
    let elbow = reloaded.host().shapes(sheet).iter().find(|s| s.name == "Elbow").unwrap().clone();
    assert_eq!(elbow.geometry.as_deref(), Some("bentConnector3"));
    assert_eq!(elbow.rotation, 16_200_000);
    assert!(elbow.flip_horizontal);
    assert!(!elbow.flip_vertical);
    assert_eq!(elbow.offset, (1_752_600, 1_168_400));
    assert_eq!(elbow.extent, (1_524_000, 1_219_200));
    assert_eq!(elbow.adjust, vec![("adj1".into(), 50_000)]);
    assert_eq!(reloaded.host().anchor_rectangle(sheet, &elbow.anchor), (150.0, 80.0, 96.0, 120.0));
}

/// dc-zjsg.14 (dw3 `editas/editas-after-row.xlsx`): resizing or hiding a row or column inside a
/// chart with `editAs="oneCell"` (or above/inside `editAs="absolute"`) rewrites `<xdr:to>` (and
/// `<xdr:from>` for `absolute`) in `xl/drawings/drawing1.xml` on Save while keeping `editAs`,
/// leaving plain `twoCell` charts and chart XML parts untouched, and working for `chartEx` charts
/// and Undo/Redo.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dw5_dc_zjsg_14__one_cell_and_absolute_chart_anchors_rewrite_on_save() {
    let source_bytes = include_bytes!("../../calc-doc/tests/fixtures/drawing/chart-kinds.xlsx");
    let mut book = open(source_bytes.to_vec());
    let sheet = book.host().sheets()[0].id();
    let chart_by_name = |b: &HostSession, name: &str| {
        b.host()
            .model()
            .definitions()
            .charts(sheet)
            .iter()
            .find(|c| c.name == name)
            .unwrap()
            .clone()
    };
    let sd_before = chart_by_name(&book, "Column sd");
    let plain_before = chart_by_name(&book, "Column labels");
    assert_eq!(sd_before.edit_as.as_deref(), Some("oneCell"));
    assert_eq!(plain_before.edit_as, None);
    let sd_rect_before = book.host().anchor_rectangle(sheet, &sd_before.anchor);
    let plain_rect_before = book.host().anchor_rectangle(sheet, &plain_before.anchor);
    let (sd_from, sd_to) = match sd_before.anchor {
        calc_doc_model::ChartAnchor::TwoCell { from, to } => (from, to),
        other => panic!("{other:?}"),
    };
    let (plain_from, plain_to) = match plain_before.anchor {
        calc_doc_model::ChartAnchor::TwoCell { from, to } => (from, to),
        other => panic!("{other:?}"),
    };
    // 1. Resize a row inside both charts from 15 pt to 45 pt (+30 pt = 2 default rows), as in
    //    Excel's `dw3/windows-results/editas/editas-after-row.xlsx` (row 12: 15 -> 45 pt).
    let sd_row = sd_from.row + 4;
    command(&mut book, |_| WorkbookCommand::AxisFormat {
        sheet,
        rows: true,
        first: sd_row,
        count: 1,
        size_bits: Some(45.0f64.to_bits()),
        hidden: None,
    });
    if plain_from.row != sd_from.row {
        command(&mut book, |_| WorkbookCommand::AxisFormat {
            sheet,
            rows: true,
            first: plain_from.row + 4,
            count: 1,
            size_bits: Some(45.0f64.to_bits()),
            hidden: None,
        });
    }
    let saved = save_bytes(&mut book);
    let reloaded = open(saved.clone());
    let sd_after = chart_by_name(&reloaded, "Column sd");
    let plain_after = chart_by_name(&reloaded, "Column labels");
    assert_eq!(sd_after.edit_as.as_deref(), Some("oneCell"));
    assert_eq!(
        sd_after.anchor,
        calc_doc_model::ChartAnchor::TwoCell {
            from: sd_from,
            to: calc_doc_model::DrawingPoint { row: sd_to.row - 2, row_offset: 0, ..sd_to },
        }
    );
    let sd_rect_after = reloaded.host().anchor_rectangle(sheet, &sd_after.anchor);
    assert!((sd_rect_after.3 - sd_rect_before.3).abs() < 1e-6, "{sd_rect_before:?} {sd_rect_after:?}");
    // Plain chart keeps its stored `<xdr:to>` and grows by 30 pt.
    assert_eq!(plain_after.anchor, calc_doc_model::ChartAnchor::TwoCell { from: plain_from, to: plain_to });
    let plain_rect_after = reloaded.host().anchor_rectangle(sheet, &plain_after.anchor);
    assert!((plain_rect_after.3 - (plain_rect_before.3 + 30.0)).abs() < 1e-6, "{plain_rect_before:?} {plain_rect_after:?}");
    // The saved drawing keeps `editAs="oneCell"` and chart XML part is byte-identical.
    let read_entry = |zip: &[u8], name: &str| -> Vec<u8> {
        let archive = existing_wire_support::parse_zip(zip);
        let entry = archive.entries.iter().find(|e| e.name == name).unwrap();
        let payload = entry.payload(zip);
        if entry.method == 0 {
            payload.to_vec()
        } else {
            miniz_oxide::inflate::decompress_to_vec(payload).unwrap()
        }
    };
    let replace_entry = |zip: &[u8], name: &str, replacement: &[u8]| -> Vec<u8> {
        let archive = existing_wire_support::parse_zip(zip);
        let entries = archive
            .entries
            .iter()
            .map(|entry| {
                if entry.name == name {
                    existing_wire_support::RawEntry::stored(&entry.name, replacement)
                } else {
                    let payload = entry.payload(zip);
                    if entry.method == 0 {
                        existing_wire_support::RawEntry::stored(&entry.name, payload)
                    } else {
                        existing_wire_support::RawEntry::deflated(&entry.name, payload, entry.size, entry.crc32)
                    }
                }
            })
            .collect::<Vec<_>>();
        existing_wire_support::build_zip(&entries)
    };
    assert_eq!(
        read_entry(source_bytes, &sd_before.part),
        read_entry(&saved, &sd_before.part),
        "anchor-only edit must not rewrite the chart XML part"
    );
    let drawing_xml = String::from_utf8(read_entry(&saved, "xl/drawings/drawing1.xml")).unwrap();
    assert!(drawing_xml.contains("<xdr:twoCellAnchor editAs=\"oneCell\">"), "{drawing_xml}");
    // 2. Undo restores the original anchor, and Redo re-applies it.
    assert!(book.dispatch(SkinIntent::Undo).accepted());
    if plain_from.row != sd_from.row {
        assert!(book.dispatch(SkinIntent::Undo).accepted());
    }
    assert_eq!(chart_by_name(&book, "Column sd").anchor, sd_before.anchor);
    assert!(book.dispatch(SkinIntent::Redo).accepted());
    assert_eq!(chart_by_name(&book, "Column sd").anchor, sd_after.anchor);
    // 3. Fractional resize (15 -> 20 pt, +5 pt): `<xdr:to>` shrinks by 1 row with 10 pt (127000 EMU) offset.
    let mut frac_book = open(source_bytes.to_vec());
    command(&mut frac_book, |_| WorkbookCommand::AxisFormat {
        sheet,
        rows: true,
        first: sd_row,
        count: 1,
        size_bits: Some(20.0f64.to_bits()),
        hidden: None,
    });
    let frac_reloaded = reopened(&mut frac_book);
    let sd_frac = chart_by_name(&frac_reloaded, "Column sd");
    assert_eq!(
        sd_frac.anchor,
        calc_doc_model::ChartAnchor::TwoCell {
            from: sd_from,
            to: calc_doc_model::DrawingPoint { row: sd_to.row - 1, row_offset: 127_000, ..sd_to },
        }
    );
    assert!((frac_reloaded.host().anchor_rectangle(sheet, &sd_frac.anchor).3 - sd_rect_before.3).abs() < 1e-6);
    // 4. Hiding a row inside the span expands `<xdr:to>` by 1 row to preserve height; widening a column shrinks `to.col`.
    let mut hide_book = open(source_bytes.to_vec());
    command(&mut hide_book, |_| WorkbookCommand::AxisFormat {
        sheet,
        rows: true,
        first: sd_row,
        count: 1,
        size_bits: None,
        hidden: Some(true),
    });
    command(&mut hide_book, |_| WorkbookCommand::AxisFormat {
        sheet,
        rows: false,
        first: sd_from.column + 2,
        count: 1,
        size_bits: Some(17.5703125_f64.to_bits()), // 2x 48 pt = 96 pt (+1 default col)
        hidden: None,
    });
    let hide_reloaded = reopened(&mut hide_book);
    let sd_hide = chart_by_name(&hide_reloaded, "Column sd");
    let hide_rect = hide_reloaded.host().anchor_rectangle(sheet, &sd_hide.anchor);
    assert!((hide_rect.3 - sd_rect_before.3).abs() < 1e-6 && (hide_rect.2 - sd_rect_before.2).abs() < 0.5, "{hide_rect:?} {sd_rect_before:?}");
    assert_eq!(
        match sd_hide.anchor {
            calc_doc_model::ChartAnchor::TwoCell { to, .. } => to.row,
            other => panic!("{other:?}"),
        },
        sd_to.row + 1
    );
    // 5. `editAs="oneCell"` on a `chartEx` (Waterfall) and `editAs="absolute"` on a regular chart both rewrite on Save.
    let patched_zip = {
        let mut drawing = String::from_utf8(read_entry(source_bytes, "xl/drawings/drawing1.xml")).unwrap();
        let wf_at = drawing.find("name=\"Waterfall\"").unwrap();
        let wf_anchor = drawing[..wf_at].rfind("<xdr:twoCellAnchor>").unwrap();
        drawing.replace_range(wf_anchor..wf_anchor + "<xdr:twoCellAnchor>".len(), "<xdr:twoCellAnchor editAs=\"oneCell\">");
        let lbl_at = drawing.find("name=\"Column labels\"").unwrap();
        let lbl_anchor = drawing[..lbl_at].rfind("<xdr:twoCellAnchor>").unwrap();
        drawing.replace_range(lbl_anchor..lbl_anchor + "<xdr:twoCellAnchor>".len(), "<xdr:twoCellAnchor editAs=\"absolute\">");
        replace_entry(source_bytes, "xl/drawings/drawing1.xml", drawing.as_bytes())
    };
    let mut cx_book = open(patched_zip);
    let wf_before = chart_by_name(&cx_book, "Waterfall");
    let abs_before = chart_by_name(&cx_book, "Column labels");
    assert_eq!(wf_before.edit_as.as_deref(), Some("oneCell"));
    assert_eq!(abs_before.edit_as.as_deref(), Some("absolute"));
    let wf_rect = cx_book.host().anchor_rectangle(sheet, &wf_before.anchor);
    let abs_rect = cx_book.host().anchor_rectangle(sheet, &abs_before.anchor);
    let wf_from_row = match wf_before.anchor {
        calc_doc_model::ChartAnchor::TwoCell { from, .. } => from.row,
        other => panic!("{other:?}"),
    };
    // Resize a row inside Waterfall and row 1 above `absolute` Column labels (+30 pt each).
    command(&mut cx_book, |_| WorkbookCommand::AxisFormat {
        sheet,
        rows: true,
        first: wf_from_row + 3,
        count: 1,
        size_bits: Some(45.0f64.to_bits()),
        hidden: None,
    });
    command(&mut cx_book, |_| WorkbookCommand::AxisFormat {
        sheet,
        rows: true,
        first: 1,
        count: 1,
        size_bits: Some(45.0f64.to_bits()),
        hidden: None,
    });
    let cx_reloaded = reopened(&mut cx_book);
    let wf_after = chart_by_name(&cx_reloaded, "Waterfall");
    let abs_after = chart_by_name(&cx_reloaded, "Column labels");
    assert_ne!(wf_after.anchor, wf_before.anchor);
    assert_ne!(abs_after.anchor, abs_before.anchor);
    let wf_rect_after = cx_reloaded.host().anchor_rectangle(sheet, &wf_after.anchor);
    let abs_rect_after = cx_reloaded.host().anchor_rectangle(sheet, &abs_after.anchor);
    assert!((wf_rect_after.3 - wf_rect.3).abs() < 1e-6, "{wf_rect:?} {wf_rect_after:?}");
    assert!((abs_rect_after.1 - abs_rect.1).abs() < 1e-6 && (abs_rect_after.3 - abs_rect.3).abs() < 1e-6, "{abs_rect:?} {abs_rect_after:?}");
    // 6. Renaming the frame is saved (dc-oxh8w.59, Excel ww22/ww24); other non-anchor frame changes are still refused.
    let mut renamed = sd_before.clone();
    renamed.name = "Renamed frame".into();
    assert!(
        cx_reloaded.host().loaded().check_chart_properties(sheet, &renamed).is_ok(),
        "a frame name edit is patched into the drawing"
    );
    let mut bad = sd_before.clone();
    bad.edit_as = Some("twoCell".into());
    assert!(
        cx_reloaded.host().loaded().check_chart_properties(sheet, &bad).is_err(),
        "non-anchor, non-name frame edits must remain refused"
    );
}
