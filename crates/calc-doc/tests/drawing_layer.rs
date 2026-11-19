//! dc-zjsg.1: the drawing layer in the document model. The fixture (tests/fixtures/drawing,
//! written by make-drawing-fixture.py) holds an auto shape, a text box, a picture, a connector
//! glued to two shapes, a group of two and a diagram frame the model does not hold. Reading
//! gives each its fields; an untouched save keeps every drawing part byte for byte; a new shape
//! is appended after the stored anchors, which are never rewritten.

#![allow(non_snake_case)]

mod cdoc_m1_1_support;

use calc_doc::model::value::SheetId;
use calc_doc::model::{
    ChartAnchor, DrawingPoint, Shape, ShapeColour, ShapeConnection, ShapeFill, ShapeKind, ShapeTextRun,
    WorkbookOp,
};
use cdoc_m1_1_support::d1::*;

const WORKBOOK: &[u8] = include_bytes!("fixtures/drawing/drawing-layer.xlsx");
const CHARTS: &[u8] = include_bytes!("../../calc-host/tests/fixtures/dc_6yw/charts.xlsx");

fn first_sheet(loaded: &calc_doc::LoadedWorkbook) -> SheetId {
    loaded.sheets()[0].id()
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_zjsg_1__every_object_kind_reads_with_its_fields() {
    let loaded = load(WORKBOOK);
    let read = loaded.sheets()[0].shapes();
    // The diagram frame is kept in the package, counted, not modelled.
    assert_eq!(read.unread, 1);
    let kinds: Vec<_> = read
        .shapes
        .iter()
        .map(|s| (s.id, s.name.as_str(), s.kind))
        .collect();
    assert_eq!(
        kinds,
        [
            (2, "Rectangle 1", ShapeKind::Shape),
            (3, "TextBox 2", ShapeKind::TextBox),
            (4, "Picture 3", ShapeKind::Picture),
            (5, "Straight Arrow Connector 4", ShapeKind::Connector),
            (6, "Group 5", ShapeKind::Group),
        ]
    );
    let [rect, text_box, picture, connector, group] = &read.shapes[..] else {
        panic!("five objects")
    };
    assert_eq!(rect.geometry.as_deref(), Some("roundRect"));
    assert_eq!(rect.fill, ShapeFill::Solid(ShapeColour::Rgb(0xFF8000)));
    assert_eq!(rect.line.weight, Some(19050));
    assert_eq!(
        rect.line.colour,
        Some(ShapeColour::Scheme("accent2".into()))
    );
    assert_eq!(rect.line.dash.as_deref(), Some("dash"));
    assert_eq!(rect.offset, (609600, 190500));
    assert_eq!(rect.extent, (1219200, 571500));
    assert_eq!(
        rect.text.as_deref(),
        Some(&["Hello shape".to_owned(), String::new()][..])
    );
    assert_eq!(
        rect.anchor,
        ChartAnchor::TwoCell {
            from: DrawingPoint {
                column: 1,
                column_offset: 0,
                row: 1,
                row_offset: 0
            },
            to: DrawingPoint {
                column: 3,
                column_offset: 0,
                row: 4,
                row_offset: 0
            },
        }
    );
    // The text body's properties (dc-zjsg.2): bodyPr anchor, the paragraphs' algn, the first run's font.
    assert_eq!(rect.body.anchor.as_deref(), Some("ctr"));
    assert_eq!(rect.body.alignment, [Some("ctr".to_owned()), None]);
    assert_eq!((rect.body.size, rect.body.bold, rect.body.italic), (Some(1100), true, false));
    assert_eq!(text_box.body.wrap.as_deref(), Some("square"));
    assert_eq!(text_box.body.left_inset, None);
    assert_eq!(text_box.edit_as.as_deref(), Some("oneCell"));
    assert_eq!(text_box.fill, ShapeFill::None);
    assert_eq!(
        text_box.text.as_deref(),
        Some(&["a & b < c".to_owned()][..])
    );
    assert_eq!(picture.picture.as_deref(), Some("rId1"));
    assert!(matches!(
        picture.anchor,
        ChartAnchor::OneCell {
            width: 952500,
            height: 952500,
            ..
        }
    ));
    assert_eq!(connector.begin, Some(ShapeConnection { shape: 2, site: 3 }));
    assert_eq!(connector.end, Some(ShapeConnection { shape: 3, site: 1 }));
    assert!(connector.flip_vertical);
    assert_eq!(connector.geometry.as_deref(), Some("straightConnector1"));
    assert!(matches!(
        group.anchor,
        ChartAnchor::Absolute {
            x: 5715000,
            y: 190500,
            ..
        }
    ));
    let [oval, hidden] = &group.children[..] else {
        panic!("two children")
    };
    assert_eq!((oval.name.as_str(), oval.rotation), ("Oval 6", 5400000));
    assert!(hidden.hidden);
    // The model root holds the same objects for the document's sheet.
    let sheet = first_sheet(&loaded);
    assert_eq!(loaded.model().definitions().shapes(sheet), &read.shapes[..]);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_zjsg_1__an_untouched_save_keeps_the_drawing_parts_byte_for_byte() {
    for (name, bytes) in [("shapes", WORKBOOK), ("charts", CHARTS)] {
        let saved = save_untouched(bytes);
        for part_name in [
            "xl/drawings/drawing1.xml",
            "xl/drawings/_rels/drawing1.xml.rels",
            "xl/media/image1.png",
        ] {
            assert_eq!(
                part(&saved, part_name),
                part(bytes, part_name),
                "{name} {part_name}"
            );
        }
    }
    let loaded = load(CHARTS);
    assert!(loaded.sheets().iter().all(|s| s.shapes().shapes.is_empty()));
}

fn new_shape(id: u32) -> Shape {
    let mut shape = Shape::new(
        id,
        "Rectangle 9".into(),
        ShapeKind::Shape,
        ChartAnchor::TwoCell {
            from: DrawingPoint {
                column: 8,
                column_offset: 0,
                row: 8,
                row_offset: 0,
            },
            to: DrawingPoint {
                column: 10,
                column_offset: 0,
                row: 11,
                row_offset: 0,
            },
        },
    );
    shape.geometry = Some("rect".into());
    shape.offset = (4876800, 1524000);
    shape.extent = (1219200, 571500);
    shape.fill = ShapeFill::Solid(ShapeColour::Rgb(0x336699));
    shape.text = Some(vec!["Q & A".into()]);
    shape
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn explicit_shape_text_runs_survive_save_and_reopen() {
    let loaded = load(WORKBOOK);
    let sheet = first_sheet(&loaded);
    let mut shapes = loaded.model().definitions().shapes(sheet).to_vec();
    let mut shape = new_shape(20);
    shape.text = Some(vec!["A&B".into(), "C".into()]);
    shape.body.size = Some(1200); shape.body.bold = true;
    shape.body.colour = Some(ShapeColour::Rgb(0xff0000));
    shape.body.underline = true;
    shape.body.runs = vec![vec![
        ShapeTextRun { text: "A&".into(), face: None, size: 1200, bold: true, italic: false, underline: true, colour: Some(ShapeColour::Rgb(0xff0000)) },
        ShapeTextRun { text: "B".into(), face: Some("A&B\"C".into()), size: 1000, bold: false, italic: true, underline: false, colour: None },
    ], vec![
        ShapeTextRun { text: "C".into(), face: None, size: 1000, bold: false, italic: true, underline: false, colour: None },
    ]];
    shapes.push(shape.clone());
    let model = edited(&loaded, vec![WorkbookOp::SetShapes { sheet, shapes }]);
    let (saved, _) = save_model(&loaded, &model);
    let loaded = load(&saved);
    let read = loaded.sheets()[0].shapes();
    let saved_shape = read.shapes.iter().find(|s| s.name == shape.name).unwrap();
    assert_eq!(saved_shape.text, shape.text);
    assert_eq!(saved_shape.body.runs, shape.body.runs);
    assert!(saved_shape.body.rich_runs_for(saved_shape.text.as_ref().unwrap()).is_some());
    let mut changed = saved_shape.clone();
    changed.body.bold = false;
    assert!(changed.body.rich_runs_for(changed.text.as_ref().unwrap()).is_none());
    changed = saved_shape.clone();
    changed.body.face = Some("Changed".into());
    assert!(changed.body.rich_runs_for(changed.text.as_ref().unwrap()).is_none());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn explicit_single_shape_typeface_survives_save_and_reopen() {
    let loaded = load(WORKBOOK);
    let sheet = first_sheet(&loaded);
    let mut shapes = loaded.model().definitions().shapes(sheet).to_vec();
    let mut shape = new_shape(21);
    shape.name = "Named font".into();
    shape.body.face = Some("A&B\"C".into());
    shape.body.underline = true;
    shapes.push(shape);
    let model = edited(&loaded, vec![WorkbookOp::SetShapes { sheet, shapes }]);
    let (saved, _) = save_model(&loaded, &model);
    let loaded = load(&saved);
    let read = loaded.sheets()[0].shapes();
    let saved_shape = read.shapes.iter().find(|s| s.name == "Named font").unwrap();
    assert_eq!(saved_shape.body.face.as_deref(), Some("A&B\"C"));
    assert!(saved_shape.body.underline);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_zjsg_1__new_shapes_are_appended_and_stored_anchors_stay() {
    let loaded = load(WORKBOOK);
    let sheet = first_sheet(&loaded);
    let mut shapes = loaded.model().definitions().shapes(sheet).to_vec();
    // id 2 is taken by the stored rectangle: the saver gives the next free id and the connector
    // glued to the new shape follows.
    let mut line = Shape::new(
        10,
        "Connector 10".into(),
        ShapeKind::Connector,
        ChartAnchor::TwoCell {
            from: DrawingPoint {
                column: 4,
                column_offset: 0,
                row: 9,
                row_offset: 0,
            },
            to: DrawingPoint {
                column: 8,
                column_offset: 0,
                row: 9,
                row_offset: 0,
            },
        },
    );
    line.geometry = Some("line".into());
    line.begin = Some(ShapeConnection { shape: 3, site: 3 });
    line.end = Some(ShapeConnection { shape: 2, site: 1 });
    shapes.push(new_shape(2));
    shapes.push(line);
    let model = edited(&loaded, vec![WorkbookOp::SetShapes { sheet, shapes }]);
    let (saved, _) = save_model(&loaded, &model);
    let before = part_text(WORKBOOK, "xl/drawings/drawing1.xml");
    let after = part_text(&saved, "xl/drawings/drawing1.xml");
    // Everything stored before the closing tag is still there, unchanged and first.
    let stored = before.trim_end().strip_suffix("</xdr:wsDr>").unwrap();
    assert!(
        after.starts_with(stored),
        "stored anchors are not rewritten"
    );
    assert!(after.contains("name=\"Rectangle 9\""));
    assert!(after.contains("Q &amp; A"));
    assert_eq!(
        part(&saved, "xl/media/image1.png"),
        part(WORKBOOK, "xl/media/image1.png")
    );

    let reloaded = load(&saved);
    let read = reloaded.sheets()[0].shapes();
    assert_eq!(read.unread, 1);
    assert_eq!(read.shapes.len(), 7);
    let new = &read.shapes[5];
    assert_eq!(
        (new.name.as_str(), new.kind, new.geometry.as_deref()),
        ("Rectangle 9", ShapeKind::Shape, Some("rect"))
    );
    assert_eq!(
        new.id, 11,
        "the colliding id 2 moved to the next free id (10 is the connector's)"
    );
    assert_eq!(new.fill, ShapeFill::Solid(ShapeColour::Rgb(0x336699)));
    assert_eq!(new.text.as_deref(), Some(&["Q & A".to_owned()][..]));
    assert_eq!(new.extent, (1219200, 571500));
    let glued = &read.shapes[6];
    assert_eq!(glued.begin, Some(ShapeConnection { shape: 3, site: 3 }));
    assert_eq!(glued.end, Some(ShapeConnection { shape: 11, site: 1 }));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_zjsg_1__editing_a_stored_object_refuses_at_save() {
    let loaded = load(WORKBOOK);
    let sheet = first_sheet(&loaded);
    let mut shapes = loaded.model().definitions().shapes(sheet).to_vec();
    shapes[0].name = "Renamed".into();
    let model = edited(&loaded, vec![WorkbookOp::SetShapes { sheet, shapes }]);
    let plan = calc_doc::prepare_save(&loaded.snapshot(&model), &calc_doc::SavePolicy::default());
    let error = plan.err().expect("refused").to_string();
    assert!(error.contains("dc-zjsg.4"), "{error}");
    // The loaded document itself still checks its own shape set.
    let own = loaded.model().definitions().shapes(sheet).to_vec();
    assert!(loaded.check_shape_set(sheet, &own).is_ok());
}

/// `bytes` with part `name` replaced by `content`, the other parts as they were.
fn with_part(bytes: &[u8], name: &str, content: &[u8]) -> Vec<u8> {
    use cdoc_m1_1_support::{build_zip, parse_zip, RawEntry};
    let archive = parse_zip(bytes);
    let entries: Vec<_> = archive
        .entries
        .iter()
        .map(|entry| {
            let payload = entry.payload(bytes);
            if entry.name == name {
                RawEntry::stored(&entry.name, content)
            } else if entry.method == 0 {
                RawEntry::stored(&entry.name, payload)
            } else {
                RawEntry::deflated(&entry.name, payload, entry.size, entry.crc32)
            }
        })
        .collect();
    build_zip(&entries)
}

/// dc-piw0 / dc-zjsg.1 corpus measure: Excel writes a shape that has an effect it added later as
/// `mc:AlternateContent` inside the anchor (a `Choice Requires="a14"` shape and a `Fallback`
/// without the effect). Excel reads the Choice, so the object is the Choice's shape; a Choice
/// requiring a namespace Excel does not know (here `zz9`) reads the Fallback; a Choice holding a
/// frame the model does not hold (a slicer) stays unread. The block is never rewritten: an
/// untouched save keeps the drawing part byte for byte.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_zjsg_1__alternate_content_shapes_read_the_branch_excel_reads() {
    let original = String::from_utf8(part(WORKBOOK, "xl/drawings/drawing1.xml").unwrap()).unwrap();
    let start = original.find("<xdr:sp macro=\"\" textlink=\"\">").unwrap();
    let end = original[start..].find("</xdr:sp>").unwrap() + start + "</xdr:sp>".len();
    let sp = &original[start..end];
    let fallback = sp.replace("Rectangle 1", "Fallback 1");
    let wrap = |choice: &str, requires: &str| {
        format!(
            "<mc:AlternateContent xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\"><mc:Choice Requires=\"{requires}\" xmlns:{requires}=\"urn:x\">{choice}</mc:Choice><mc:Fallback>{fallback}</mc:Fallback></mc:AlternateContent>"
        )
    };
    let name_of = |xml: String| {
        let bytes = with_part(WORKBOOK, "xl/drawings/drawing1.xml", xml.as_bytes());
        let loaded = load(&bytes);
        let read = loaded.sheets()[0].shapes();
        (
            read.shapes.first().map(|s| (s.id, s.name.clone())),
            read.unread,
            save_untouched(&bytes),
            bytes,
        )
    };
    let (first, unread, saved, bytes) = name_of(format!(
        "{}{}{}",
        &original[..start],
        wrap(sp, "a14"),
        &original[end..]
    ));
    assert_eq!(first, Some((2, "Rectangle 1".to_owned())));
    assert_eq!(unread, 1, "only the diagram frame stays unread");
    assert_eq!(
        part(&saved, "xl/drawings/drawing1.xml"),
        part(&bytes, "xl/drawings/drawing1.xml")
    );
    let (first, ..) = name_of(format!(
        "{}{}{}",
        &original[..start],
        wrap(sp, "zz9"),
        &original[end..]
    ));
    assert_eq!(first, Some((2, "Fallback 1".to_owned())));
    let frame = "<xdr:graphicFrame macro=\"\"><xdr:nvGraphicFramePr><xdr:cNvPr id=\"9\" name=\"Slicer 1\"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr><xdr:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/></xdr:xfrm><a:graphic><a:graphicData uri=\"http://schemas.microsoft.com/office/drawing/2010/slicer\"/></a:graphic></xdr:graphicFrame>";
    let (first, unread, ..) = name_of(format!(
        "{}{}{}",
        &original[..start],
        wrap(frame, "sle15"),
        &original[end..]
    ));
    assert_eq!(first.map(|f| f.0), Some(3), "the next object, not the fallback");
    assert_eq!(unread, 2);
    // The block around the whole anchor (a form control or an a14 shape): the branch's anchor is read.
    let a_start = original.find("<xdr:twoCellAnchor>").unwrap();
    let a_end = original[a_start..].find("</xdr:twoCellAnchor>").unwrap() + a_start + "</xdr:twoCellAnchor>".len();
    let anchor = &original[a_start..a_end];
    let around = format!(
        "{}<mc:AlternateContent xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\"><mc:Choice Requires=\"a14\" xmlns:a14=\"urn:x\">{anchor}</mc:Choice></mc:AlternateContent>{}",
        &original[..a_start],
        &original[a_end..]
    );
    let (first, unread, saved, bytes) = name_of(around);
    assert_eq!(first, Some((2, "Rectangle 1".to_owned())));
    assert_eq!(unread, 1);
    assert_eq!(
        part(&saved, "xl/drawings/drawing1.xml"),
        part(&bytes, "xl/drawings/drawing1.xml")
    );
}

// dc-zjsg.2: the workbook the paint comparison draws (cargo xtask app paint-compare calc). Its shapes are
// added to shapes-base.xlsx by calc-doc's own save route; the committed shapes-paint.xlsx is that
// save. Regenerate with DNACALC_WRITE_FIXTURE=1 cargo test -p calc-doc --test drawing_layer
// dc_zjsg_2__shapes_paint_workbook.
const SHAPES_BASE: &[u8] = include_bytes!("fixtures/drawing/shapes-base.xlsx");
const SHAPES_PAINT: &[u8] = include_bytes!("fixtures/drawing/shapes-paint.xlsx");

fn at(column: u32, row: u32) -> DrawingPoint {
    DrawingPoint { column, column_offset: 0, row, row_offset: 0 }
}

fn cells(from: DrawingPoint, to: DrawingPoint) -> ChartAnchor {
    ChartAnchor::TwoCell { from, to }
}

fn paint_shapes() -> Vec<Shape> {
    use calc_doc::model::ShapeLine;
    #[allow(non_snake_case)]
    use ShapeKind::{Connector, TextBox};
    const SHAPE: ShapeKind = ShapeKind::Shape;
    let rgb = |v| Some(ShapeColour::Rgb(v));
    let mut shapes = Vec::new();
    let mut add = |id: u32, name: &str, kind, geometry: &str, anchor, fill: ShapeFill, line: ShapeLine| -> usize {
        let mut shape = Shape::new(id, name.into(), kind, anchor);
        shape.geometry = Some(geometry.into());
        shape.extent = (952_500, 476_250);
        shape.fill = fill;
        shape.line = line;
        shapes.push(shape);
        shapes.len() - 1
    };
    let solid = |v| ShapeFill::Solid(ShapeColour::Rgb(v));
    let line = |weight: i64, colour: u32, dash: Option<&str>| ShapeLine {
        weight: Some(weight),
        hidden: false,
        colour: Some(ShapeColour::Rgb(colour)),
        dash: dash.map(str::to_owned),
        ..Default::default()
    };
    let hidden_line = ShapeLine { hidden: true, ..Default::default() };
    let a = add(2, "Rectangle 1", SHAPE, "rect", cells(at(1, 1), at(3, 4)), solid(0xFFC000), line(25_400, 0x1F4E79, None));
    let b = add(3, "Rounded 2", SHAPE, "roundRect", cells(at(4, 1), at(6, 4)), ShapeFill::None, line(38_100, 0xC00000, Some("dash")));
    let c = add(4, "Oval 3", SHAPE, "ellipse", cells(at(7, 1), at(9, 4)), solid(0x70AD47), hidden_line.clone());
    add(5, "Line 4", Connector, "line", cells(at(1, 6), at(3, 8)), ShapeFill::Style, line(19_050, 0xFF0000, Some("sysDash")));
    add(6, "Straight Connector 5", Connector, "straightConnector1", cells(at(4, 6), at(6, 6)), ShapeFill::Style, ShapeLine::default());
    let t = add(7, "TextBox 6", TextBox, "rect", cells(at(7, 6), at(9, 9)), solid(0xFFFF99), line(12_700, 0x7F7F7F, None));
    let u = add(8, "TextBox 7", TextBox, "rect", cells(at(1, 9), at(3, 11)), ShapeFill::None, hidden_line.clone());
    let v = add(9, "Rectangle 8", SHAPE, "rect", cells(at(2, 4), at(4, 7)), solid(0x5B9BD5), hidden_line.clone());
    let w = add(10, "Oval 9", SHAPE, "ellipse", ChartAnchor::OneCell { from: at(4, 9), width: 952_500, height: 476_250 }, solid(0xED7D31), hidden_line);
    let x = add(11, "Rectangle 10", SHAPE, "rect", ChartAnchor::Absolute { x: 5_715_000, y: 3_810_000, width: 1_143_000, height: 381_000 }, solid(0xA5A5A5), ShapeLine::default());
    shapes[3].flip_vertical = true;
    shapes[a].text = Some(vec!["Rectangle".into()]);
    shapes[a].body.anchor = Some("ctr".into());
    shapes[a].body.alignment = vec![Some("ctr".into())];
    shapes[a].body.colour = rgb(0x000000);
    shapes[b].text = Some(vec!["Dashed".into()]);
    shapes[b].body.colour = rgb(0xC00000);
    shapes[c].text = None;
    shapes[t].text = Some(vec!["Left aligned".into(), "Centre".into(), "Right aligned".into()]);
    shapes[t].body.left_inset = Some(182_880);
    shapes[t].body.top_inset = Some(182_880);
    shapes[t].body.right_inset = Some(182_880);
    shapes[t].body.bottom_inset = Some(182_880);
    shapes[t].body.anchor = Some("ctr".into());
    shapes[t].body.wrap = Some("square".into());
    shapes[t].body.alignment = vec![Some("l".into()), Some("ctr".into()), Some("r".into())];
    shapes[t].body.size = Some(1400);
    shapes[t].body.bold = true;
    shapes[t].body.colour = rgb(0x1F4E79);
    shapes[u].text = Some(vec!["Bottom, no wrap, a long line past the right edge".into()]);
    shapes[u].body.anchor = Some("b".into());
    shapes[u].body.wrap = Some("none".into());
    shapes[v].text = Some(vec!["Over hidden D and row 6".into()]);
    shapes[v].body.colour = rgb(0xFFFFFF);
    shapes[w].text = None;
    shapes[x].text = Some(vec!["Absolute".into()]);
    shapes[x].body.anchor = Some("ctr".into());
    shapes[x].body.alignment = vec![Some("ctr".into())];
    shapes[x].body.colour = rgb(0x000000);
    shapes
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_zjsg_2__shapes_paint_workbook() {
    let base = load(SHAPES_BASE);
    let sheet = first_sheet(&base);
    let model = edited(&base, vec![WorkbookOp::SetShapes { sheet, shapes: paint_shapes() }]);
    let (saved, _) = save_model(&base, &model);
    #[cfg(not(target_arch = "wasm32"))]
    if std::env::var_os("DNACALC_WRITE_FIXTURE").is_some() {
        std::fs::write(
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/drawing/shapes-paint.xlsx"),
            &saved,
        )
        .unwrap();
    }
    // The committed workbook is that save: it reads back as the shapes made, with their text bodies.
    let committed = load(SHAPES_PAINT);
    let read = committed.sheets()[0].shapes();
    assert_eq!(read.unread, 0);
    let made = paint_shapes();
    assert_eq!(read.shapes.len(), made.len());
    for (read, made) in read.shapes.iter().zip(&made) {
        assert_eq!(read.name, made.name);
        assert_eq!(read.kind, made.kind);
        assert_eq!(read.geometry, made.geometry, "{}", made.name);
        assert_eq!(read.anchor, made.anchor, "{}", made.name);
        assert_eq!(read.fill, made.fill, "{}", made.name);
        assert_eq!(read.line.weight, made.line.weight, "{}", made.name);
        assert_eq!(read.line.dash, made.line.dash, "{}", made.name);
        assert_eq!(read.flip_vertical, made.flip_vertical, "{}", made.name);
        // A shape without text is saved with one empty paragraph, as Excel writes it.
        let blank = || (made.kind != ShapeKind::Connector).then(|| vec![String::new()]);
        assert_eq!(read.text, made.text.clone().or_else(blank), "{}", made.name);
        assert_eq!(read.body.anchor.as_deref().unwrap_or("t"), made.body.anchor.as_deref().unwrap_or("t"), "{}", made.name);
        assert_eq!(read.body.wrap, made.body.wrap, "{}", made.name);
        assert_eq!(read.body.left_inset, made.body.left_inset, "{}", made.name);
        assert_eq!(read.body.size.unwrap_or(1100), made.body.size.unwrap_or(1100), "{}", made.name);
        assert_eq!((read.body.bold, &read.body.colour), (made.body.bold, &made.body.colour), "{}", made.name);
        let aligned: Vec<_> = read.body.alignment.iter().map(|a| a.as_deref()).collect();
        let stated: Vec<_> = made.body.alignment.iter().map(|a| a.as_deref()).collect();
        if !stated.is_empty() {
            assert_eq!(aligned, stated, "{}", made.name);
        }
    }
}

// dc-zjsg.2 second slice: rotation and flips, arrowheads, gradients, groups and the presets beyond the
// first five, on the same base workbook. The shapes sit below row 12 on the default 64 by 20 pixel cells,
// in slots 256 px wide (`slot`); a turned shape is anchored by the box around it, as Excel
// anchors it. Regenerate with DNACALC_WRITE_FIXTURE=1 cargo test -p calc-doc --test drawing_layer
// dc_zjsg_2__shapes_paint2_workbook.
const SHAPES_PAINT2: &[u8] = include_bytes!("fixtures/drawing/shapes-paint2.xlsx");
const CELL_W: i64 = 609_600;
const CELL_H: i64 = 190_500;

/// The anchor of a box of `w` by `h` EMU centred in slot `column` (4 cells wide) at `row` cells down, in the grid below row 12.
fn slot(column: i64, row: i64, w: i64, h: i64) -> ChartAnchor {
    let (cx, cy) = (column * 4 * CELL_W + 2 * CELL_W, row * CELL_H);
    let corner = |x: i64, y: i64| DrawingPoint {
        column: 5 + (x / CELL_W) as u32,
        column_offset: x % CELL_W,
        row: 12 + (y / CELL_H) as u32,
        row_offset: y % CELL_H,
    };
    ChartAnchor::TwoCell { from: corner(cx - w / 2, cy - h / 2), to: corner(cx + w / 2, cy + h / 2) }
}

/// The box around a `w` by `h` shape turned by `degrees`.
fn around(w: i64, h: i64, degrees: f64) -> (i64, i64) {
    let (sin, cos) = degrees.to_radians().sin_cos();
    (
        (w as f64 * cos.abs() + h as f64 * sin.abs()).round() as i64,
        (w as f64 * sin.abs() + h as f64 * cos.abs()).round() as i64,
    )
}

/// A shape of `size` in slot `(column, row)`, turned by `degrees`, filled and outlined blue, its name as its text.
fn put(
    shapes: &mut Vec<Shape>,
    next: &mut u32,
    name: &str,
    kind: ShapeKind,
    prst: &str,
    (column, row): (i64, i64),
    size: (i64, i64),
    degrees: f64,
) -> usize {
    let (bw, bh) = around(size.0, size.1, degrees);
    let mut shape = Shape::new(*next, name.into(), kind, slot(column, row, bw, bh));
    *next += 1;
    shape.geometry = Some(prst.into());
    shape.extent = size;
    shape.rotation = (degrees * 60_000.0) as i64;
    shape.fill = ShapeFill::Solid(ShapeColour::Rgb(0x2E75B6));
    shape.line = calc_doc::model::ShapeLine { weight: Some(19_050), colour: Some(ShapeColour::Rgb(0x1F3864)), ..Default::default() };
    shape.text = Some(vec![name.into()]);
    shape.body.anchor = Some("ctr".into());
    shape.body.alignment = vec![Some("ctr".into())];
    shape.body.size = Some(900);
    shape.body.colour = Some(ShapeColour::Rgb(0xFFFFFF));
    shapes.push(shape);
    shapes.len() - 1
}

fn paint2_shapes() -> Vec<Shape> {
    use calc_doc::model::{LineEnd, ShapeGradient, ShapeLine};
    const W: i64 = 3 * CELL_W;
    const H: i64 = 3 * CELL_H;
    let mut shapes: Vec<Shape> = Vec::new();
    let mut next = 2u32;
    // Turns and flips.
    let mut rotated = Vec::new();
    rotated.push(put(&mut shapes, &mut next, "Rot 30 rect", ShapeKind::Shape, "rect", (0, 6), (W, H), 30.0));
    rotated.push(put(&mut shapes, &mut next, "Rot 90 roundRect", ShapeKind::Shape, "roundRect", (1, 6), (W, H), 90.0));
    rotated.push(put(&mut shapes, &mut next, "Rot 45 flipH parallelogram", ShapeKind::Shape, "parallelogram", (2, 6), (W, H), 45.0));
    rotated.push(put(&mut shapes, &mut next, "Rot 180 rightArrow", ShapeKind::Shape, "rightArrow", (3, 6), (W, H), 180.0));
    let flip_h = put(&mut shapes, &mut next, "FlipH rightArrow", ShapeKind::Shape, "rightArrow", (0, 16), (W, H), 0.0);
    shapes[flip_h].flip_horizontal = true;
    let flip_v = put(&mut shapes, &mut next, "FlipV triangle text", ShapeKind::Shape, "triangle", (1, 16), (W, H), 0.0);
    shapes[flip_v].flip_vertical = true;
    let both = put(&mut shapes, &mut next, "FlipH+V rtTriangle", ShapeKind::Shape, "rtTriangle", (2, 16), (W, H), 0.0);
    shapes[both].flip_horizontal = true;
    shapes[both].flip_vertical = true;
    let tb = put(&mut shapes, &mut next, "TextBox rot 20", ShapeKind::TextBox, "rect", (3, 16), (W, H), 20.0);
    shapes[tb].fill = ShapeFill::Solid(ShapeColour::Rgb(0xFFFF99));
    shapes[tb].body.colour = Some(ShapeColour::Rgb(0x000000));
    // Arrowheads, a thin and a thick line of each type, head and tail, sizes.
    let end = |kind: &str, w: &str, len: &str| Some(LineEnd { kind: kind.into(), width: Some(w.into()), length: Some(len.into()) });
    let arrows: [(&str, Option<LineEnd>, Option<LineEnd>, i64); 12] = [
        ("Arrow triangle med 1pt", None, end("triangle", "med", "med"), 12_700),
        ("Arrow stealth med 1pt", None, end("stealth", "med", "med"), 12_700),
        ("Arrow diamond med 1pt", None, end("diamond", "med", "med"), 12_700),
        ("Arrow oval med 1pt", None, end("oval", "med", "med"), 12_700),
        ("Arrow open med 1pt", None, end("arrow", "med", "med"), 12_700),
        ("Arrow both triangle lg 1pt", end("triangle", "lg", "lg"), end("triangle", "lg", "lg"), 12_700),
        ("Arrow triangle sm 1pt", None, end("triangle", "sm", "sm"), 12_700),
        ("Arrow triangle med 4.5pt", None, end("triangle", "med", "med"), 57_150),
        ("Arrow stealth lg 4.5pt", None, end("stealth", "lg", "lg"), 57_150),
        ("Arrow oval wide short 3pt", end("oval", "lg", "sm"), None, 38_100),
        ("Arrow head diamond 3pt", end("diamond", "med", "med"), None, 38_100),
        ("Arrow open lg 3pt", None, end("arrow", "lg", "lg"), 38_100),
    ];
    for (i, (name, head, tail, weight)) in arrows.into_iter().enumerate() {
        let (column, row) = (i as i64 % 4, 24 + 4 * (i as i64 / 4));
        let kind = if i % 2 == 0 { ShapeKind::Connector } else { ShapeKind::Connector };
        let index = put(&mut shapes, &mut next, name, kind, if i % 2 == 0 { "straightConnector1" } else { "line" }, (column, row), (W, H), 0.0);
        let line = &mut shapes[index];
        line.text = None;
        line.fill = ShapeFill::Style;
        line.line = ShapeLine { weight: Some(weight), colour: Some(ShapeColour::Rgb(0xC00000)), head, tail, ..Default::default() };
        // Some lines run up, some down.
        line.flip_vertical = i % 3 == 1;
    }
    // Gradients.
    let stops = vec![(0, ShapeColour::Rgb(0xFF0000)), (50_000, ShapeColour::Rgb(0xFFFF00)), (100_000, ShapeColour::Rgb(0x0070C0))];
    let linear = |angle: i64, scaled: bool| ShapeFill::Gradient(ShapeGradient { stops: stops.clone(), angle: Some(angle), scaled, path: None, focus: None });
    let path = |kind: &str, focus: [i64; 4]| ShapeFill::Gradient(ShapeGradient { stops: stops.clone(), angle: None, scaled: false, path: Some(kind.into()), focus: Some(focus) });
    let gradients = [
        ("Grad linear 0", linear(0, false)),
        ("Grad linear 90", linear(5_400_000, false)),
        ("Grad linear 45 scaled", linear(2_700_000, true)),
        ("Grad linear 45 unscaled", linear(2_700_000, false)),
        ("Grad path circle centre", path("circle", [50_000, 50_000, 50_000, 50_000])),
        ("Grad path rect top-left", path("rect", [0, 0, 100_000, 100_000])),
        ("Grad path shape", path("shape", [50_000, 50_000, 50_000, 50_000])),
        ("Grad linear 315 ellipse", linear(18_900_000, false)),
    ];
    for (i, (name, fill)) in gradients.into_iter().enumerate() {
        let index = put(&mut shapes, &mut next, name, ShapeKind::Shape, if i == 7 { "ellipse" } else { "rect" }, (i as i64 % 4, 38 + 4 * (i as i64 / 4)), (W, H), 0.0);
        shapes[index].fill = fill;
    }
    // Groups of a rectangle and an ellipse in a child space of their own.
    for (i, (name, flip_h, degrees)) in [("Group plain", false, 0.0), ("Group flipH", true, 0.0), ("Group rot 20", false, 20.0)].into_iter().enumerate() {
        let index = put(&mut shapes, &mut next, name, ShapeKind::Group, "rect", (i as i64, 48), (W, H), degrees);
        let group = &mut shapes[index];
        group.geometry = None;
        group.text = None;
        group.fill = ShapeFill::Style;
        group.line = ShapeLine::default();
        group.flip_horizontal = flip_h;
        group.child_offset = (100_000, 100_000);
        group.child_extent = (2_000_000, 1_000_000);
        let mut square = Shape::new(next + 100, format!("{name} rect"), ShapeKind::Shape, group.anchor);
        square.geometry = Some("rect".into());
        square.offset = (100_000, 100_000);
        square.extent = (1_000_000, 1_000_000);
        square.fill = ShapeFill::Solid(ShapeColour::Rgb(0x70AD47));
        square.rotation = 0;
        let mut oval = Shape::new(next + 101, format!("{name} oval"), ShapeKind::Shape, group.anchor);
        oval.geometry = Some("ellipse".into());
        oval.offset = (1_100_000, 300_000);
        oval.extent = (900_000, 600_000);
        oval.fill = ShapeFill::Solid(ShapeColour::Rgb(0xED7D31));
        oval.text = Some(vec!["oval".into()]);
        oval.body.anchor = Some("ctr".into());
        oval.body.colour = Some(ShapeColour::Rgb(0x000000));
        let mut arrow = Shape::new(next + 102, format!("{name} arrow"), ShapeKind::Shape, group.anchor);
        arrow.geometry = Some("rightArrow".into());
        arrow.offset = (100_000, 1_000_000);
        arrow.extent = (800_000, 100_000);
        arrow.fill = ShapeFill::Solid(ShapeColour::Rgb(0x7030A0));
        arrow.rotation = 1_800_000;
        group.children = vec![square, oval, arrow];
        next += 3;
    }
    // The presets, default adjusts and some others.
    let presets: [(&str, &[(&str, i64)]); 28] = [
        ("triangle", &[]),
        ("triangle", &[("adj", 20_000)]),
        ("rtTriangle", &[]),
        ("diamond", &[]),
        ("parallelogram", &[]),
        ("parallelogram", &[("adj", 60_000)]),
        ("trapezoid", &[]),
        ("trapezoid", &[("adj", 40_000)]),
        ("pentagon", &[]),
        ("hexagon", &[]),
        ("hexagon", &[("adj", 40_000), ("vf", 115_470)]),
        ("octagon", &[]),
        ("rightArrow", &[]),
        ("rightArrow", &[("adj1", 25_000), ("adj2", 80_000)]),
        ("leftArrow", &[]),
        ("upArrow", &[]),
        ("downArrow", &[]),
        ("chevron", &[]),
        ("chevron", &[("adj", 80_000)]),
        ("wedgeRectCallout", &[]),
        ("wedgeRectCallout", &[("adj1", 70_000), ("adj2", -30_000)]),
        ("wedgeRoundRectCallout", &[]),
        ("wedgeRoundRectCallout", &[("adj1", -45_000), ("adj2", -70_000), ("adj3", 30_000)]),
        ("roundRect", &[("adj", 50_000)]),
        ("flowChartProcess", &[]),
        ("flowChartDecision", &[]),
        ("flowChartTerminator", &[]),
        ("flowChartDocument", &[]),
    ];
    for (i, (prst, adjust)) in presets.into_iter().enumerate() {
        let label = if adjust.is_empty() { prst.to_owned() } else { format!("{prst} {}", adjust.iter().map(|(n, v)| format!("{n}={v}")).collect::<Vec<_>>().join(",")) };
        let index = put(&mut shapes, &mut next, &label, ShapeKind::Shape, prst, (i as i64 % 4, 56 + 4 * (i as i64 / 4)), (W, H), 0.0);
        shapes[index].adjust = adjust.iter().map(|(n, v)| ((*n).to_owned(), *v)).collect();
    }
    shapes
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_zjsg_2__shapes_paint2_workbook() {
    let base = load(SHAPES_BASE);
    let sheet = first_sheet(&base);
    let model = edited(&base, vec![WorkbookOp::SetShapes { sheet, shapes: paint2_shapes() }]);
    let (saved, _) = save_model(&base, &model);
    #[cfg(not(target_arch = "wasm32"))]
    if std::env::var_os("DNACALC_WRITE_FIXTURE").is_some() {
        std::fs::write(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/drawing/shapes-paint2.xlsx"), &saved).unwrap();
    }
    // The committed workbook is that save and reads back as the objects made.
    let committed = load(SHAPES_PAINT2);
    let read = committed.sheets()[0].shapes();
    assert_eq!(read.unread, 0);
    let made = paint2_shapes();
    assert_eq!(read.shapes.len(), made.len());
    fn same(read: &Shape, made: &Shape) {
        assert_eq!(read.name, made.name);
        assert_eq!(read.kind, made.kind, "{}", made.name);
        assert_eq!(read.geometry, made.geometry, "{}", made.name);
        assert_eq!((read.offset, read.extent), (made.offset, made.extent), "{}", made.name);
        assert_eq!(read.rotation, made.rotation, "{}", made.name);
        assert_eq!((read.flip_horizontal, read.flip_vertical), (made.flip_horizontal, made.flip_vertical), "{}", made.name);
        assert_eq!(read.adjust, made.adjust, "{}", made.name);
        assert_eq!((read.child_offset, read.child_extent), (made.child_offset, made.child_extent), "{}", made.name);
        assert_eq!(read.fill, made.fill, "{}", made.name);
        assert_eq!((read.line.head.clone(), read.line.tail.clone()), (made.line.head.clone(), made.line.tail.clone()), "{}", made.name);
        assert_eq!(read.line.weight, made.line.weight, "{}", made.name);
        assert_eq!(read.children.len(), made.children.len(), "{}", made.name);
        for (r, m) in read.children.iter().zip(&made.children) {
            same(r, m);
        }
    }
    for (read, made) in read.shapes.iter().zip(&made) {
        same(read, made);
        assert_eq!(read.anchor, made.anchor, "{}", made.name);
    }
}

// dc-zjsg.3: the workbook the picture paint comparison draws, on the same base. Its pictures are the test card of
// fixtures/drawing/pictures (make-pictures.py) in each format, added by calc-doc's save route. Regenerate with
// DNACALC_WRITE_FIXTURE=1 cargo test -p calc-doc --test drawing_layer dc_zjsg_3__pictures_paint_workbook.
const PICTURES_PAINT: &[u8] = include_bytes!("fixtures/drawing/pictures-paint.xlsx");
const CARD_PNG: &[u8] = include_bytes!("fixtures/drawing/pictures/card.png");
const CARD_JPG: &[u8] = include_bytes!("fixtures/drawing/pictures/card.jpg");
const CARD_GIF: &[u8] = include_bytes!("fixtures/drawing/pictures/card.gif");
const CARD_BMP: &[u8] = include_bytes!("fixtures/drawing/pictures/card.bmp");
const CARD_SVG: &[u8] = include_bytes!("fixtures/drawing/pictures/card.svg");
const BARE_EMF: &[u8] = include_bytes!("fixtures/drawing/pictures/bare.emf");
const PNG_EMF: &[u8] = include_bytes!("fixtures/drawing/pictures/withpng.emf");
const BARE_WMF: &[u8] = include_bytes!("fixtures/drawing/pictures/bare.wmf");

fn zip_names(package: &[u8]) -> Vec<String> {
    cdoc_m1_1_support::parse_zip(package).entries.iter().map(|e| e.name.clone()).collect()
}

fn media(content_type: &str, bytes: &[u8]) -> calc_doc::model::PictureMedia {
    calc_doc::model::PictureMedia {
        part: String::new(),
        content_type: content_type.into(),
        size: bytes.len() as u64,
        bytes: bytes.into(),
    }
}

fn pictures() -> Vec<Shape> {
    use calc_doc::model::{ShapeLine, ShapePicture};
    // The card is 160 by 100: 1828800 by 1143000 EMU (192 by 120 px).
    const W: i64 = 1_828_800;
    const H: i64 = 1_143_000;
    let list: [(&str, calc_doc::model::PictureMedia, Option<calc_doc::model::PictureMedia>, [i64; 4], f64, bool); 11] = [
        ("Pic PNG", media("image/png", CARD_PNG), None, [0; 4], 0.0, false),
        ("Pic JPEG", media("image/jpeg", CARD_JPG), None, [0; 4], 0.0, false),
        ("Pic GIF", media("image/gif", CARD_GIF), None, [0; 4], 0.0, false),
        ("Pic BMP", media("image/bmp", CARD_BMP), None, [0; 4], 0.0, false),
        // Cropped by 25% left and right, 10% top and 40% bottom: the red/green boundary and the arrow's middle remain.
        ("Pic PNG crop", media("image/png", CARD_PNG), None, [25_000, 10_000, 25_000, 40_000], 0.0, false),
        ("Pic PNG rot 30 outlined", media("image/png", CARD_PNG), None, [0; 4], 30.0, true),
        ("Pic SVG with PNG fallback", media("image/png", CARD_PNG), Some(media("image/svg+xml", CARD_SVG)), [0; 4], 0.0, false),
        ("Pic EMF bare", media("image/x-emf", BARE_EMF), None, [0; 4], 0.0, false),
        ("Pic EMF with PNG", media("image/x-emf", PNG_EMF), None, [0; 4], 0.0, false),
        ("Pic WMF bare", media("image/x-wmf", BARE_WMF), None, [0; 4], 0.0, false),
        ("Pic PNG rot 90 crop", media("image/png", CARD_PNG), None, [0, 0, 50_000, 0], 90.0, false),
    ];
    let mut shapes = Vec::new();
    for (i, (name, image, svg, crop, degrees, outlined)) in list.into_iter().enumerate() {
        let (bw, bh) = around(W, H, degrees);
        let mut shape = Shape::new(2 + i as u32, name.into(), ShapeKind::Picture, slot(i as i64 % 4, 6 + 10 * (i as i64 / 4), bw, bh));
        shape.geometry = Some("rect".into());
        shape.extent = (W, H);
        shape.rotation = (degrees * 60_000.0) as i64;
        if outlined {
            shape.line = ShapeLine { weight: Some(38_100), colour: Some(ShapeColour::Rgb(0xC00000)), dash: Some("dash".into()), ..Default::default() };
        }
        shape.media = Some(ShapePicture { image: Some(image), svg, crop, description: Some(format!("{name} (test card)")) });
        shapes.push(shape);
    }
    shapes
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_zjsg_3__pictures_paint_workbook() {
    let base = load(SHAPES_BASE);
    let sheet = first_sheet(&base);
    let model = edited(&base, vec![WorkbookOp::SetShapes { sheet, shapes: pictures() }]);
    let (saved, _) = save_model(&base, &model);
    #[cfg(not(target_arch = "wasm32"))]
    if std::env::var_os("DNACALC_WRITE_FIXTURE").is_some() {
        std::fs::write(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/drawing/pictures-paint.xlsx"), &saved).unwrap();
    }
    // The save wrote an image part per picture (the SVG's too) and the drawing relates them.
    let names = zip_names(&saved);
    let media_parts: Vec<_> = names.iter().filter(|n| n.starts_with("xl/media/")).collect();
    assert_eq!(media_parts.len(), 12, "{names:?}");
    assert_eq!(media_parts.iter().filter(|n| n.ends_with(".svg")).count(), 1, "{media_parts:?}");
    // The committed workbook is that save: it reads back with each picture's bytes, crop and description.
    let committed = load(PICTURES_PAINT);
    let read = committed.sheets()[0].shapes();
    assert_eq!(read.unread, 0);
    let made = pictures();
    assert_eq!(read.shapes.len(), made.len());
    for (read, made) in read.shapes.iter().zip(&made) {
        assert_eq!((read.name.as_str(), read.kind), (made.name.as_str(), ShapeKind::Picture));
        assert_eq!(read.anchor, made.anchor, "{}", made.name);
        assert_eq!((read.rotation, read.extent), (made.rotation, made.extent), "{}", made.name);
        assert_eq!(read.line.weight, made.line.weight, "{}", made.name);
        let (r, m) = (read.media.as_ref().unwrap(), made.media.as_ref().unwrap());
        assert_eq!(r.crop, m.crop, "{}", made.name);
        assert_eq!(r.description, m.description, "{}", made.name);
        assert_eq!(&r.image.as_ref().unwrap().bytes, &m.image.as_ref().unwrap().bytes, "{}", made.name);
        assert_eq!(r.image.as_ref().unwrap().content_type, m.image.as_ref().unwrap().content_type, "{}", made.name);
        assert_eq!(r.svg.as_ref().map(|s| (&s.bytes, s.content_type.as_str())), m.svg.as_ref().map(|s| (&s.bytes, s.content_type.as_str())), "{}", made.name);
    }
}

// dc-zjsg.2 / dc-zjsg.3, third slice: the workbook the third paint comparison draws, on the same base: colour
// modifiers, outer shadows, the text rectangles of presets, bent and curved connectors with adjusts, flipped
// pictures and pictures inside groups, in slots 256 px wide below row 12 (`slot`). Regenerate with
// DNACALC_WRITE_FIXTURE=1 cargo test -p calc-doc --test drawing_layer dc_zjsg_2__shapes_paint3_workbook.
const SHAPES_PAINT3: &[u8] = include_bytes!("fixtures/drawing/shapes-paint3.xlsx");

fn modified(base: ShapeColour, modifiers: &[(&str, i64)]) -> ShapeColour {
    ShapeColour::Modified(Box::new(base), modifiers.iter().map(|(n, v)| ((*n).to_owned(), *v)).collect())
}

fn paint3_shapes() -> Vec<Shape> {
    use calc_doc::model::{LineEnd, ShapeLine, ShapePicture, ShapeShadow};
    const W: i64 = 3 * CELL_W;
    const H: i64 = 3 * CELL_H;
    let mut shapes: Vec<Shape> = Vec::new();
    let mut next = 2u32;
    let scheme = |name: &str| ShapeColour::Scheme(name.into());
    // Colour modifiers: Excel's palette variants of accent 1 and 2, then the others.
    let swatches: [(&str, ShapeColour); 12] = [
        ("Mod accent1 lumMod60 lumOff40", modified(scheme("accent1"), &[("lumMod", 60_000), ("lumOff", 40_000)])),
        ("Mod accent1 lumMod75", modified(scheme("accent1"), &[("lumMod", 75_000)])),
        ("Mod accent2 lumMod20 lumOff80", modified(scheme("accent2"), &[("lumMod", 20_000), ("lumOff", 80_000)])),
        ("Mod accent1 shade50", modified(scheme("accent1"), &[("shade", 50_000)])),
        ("Mod accent1 tint40", modified(scheme("accent1"), &[("tint", 40_000)])),
        ("Mod accent5 satMod50", modified(scheme("accent5"), &[("satMod", 50_000)])),
        ("Mod srgb red alpha50", modified(ShapeColour::Rgb(0xFF0000), &[("alpha", 50_000)])),
        ("Mod accent4 lumMod50", modified(scheme("accent4"), &[("lumMod", 50_000)])),
        ("Mod srgb 70AD47 lumOff-20", modified(ShapeColour::Rgb(0x70AD47), &[("lumOff", -20_000)])),
        ("Mod tx2 lumMod 90 lumOff 10", modified(scheme("tx2"), &[("lumMod", 90_000), ("lumOff", 10_000)])),
        ("Mod bg1 lumMod 85", modified(scheme("bg1"), &[("lumMod", 85_000)])),
        ("Mod accent6 hueOff 60", modified(scheme("accent6"), &[("hueOff", 3_600_000)])),
    ];
    for (i, (name, colour)) in swatches.into_iter().enumerate() {
        let index = put(&mut shapes, &mut next, name, ShapeKind::Shape, "rect", (i as i64 % 4, 6 + 4 * (i as i64 / 4)), (W, H), 0.0);
        shapes[index].fill = ShapeFill::Solid(colour);
        shapes[index].line = ShapeLine { weight: Some(28_575), colour: Some(modified(scheme("accent3"), &[("alpha", 60_000)])), ..Default::default() };
        shapes[index].body.colour = Some(ShapeColour::Rgb(0x000000));
    }
    // Outer shadows (rows 20 to 31): Excel's offset-bottom-right look, a wide blur, a scaled one aligned to the
    // top, a shadow of an outline only, of a turned shape both ways, of a line, and of a gradient-free ellipse.
    let shadow = |blur: i64, distance: i64, degrees: i64, alpha: i64, algn: Option<&str>, scale: (i64, i64), rotate: bool| ShapeShadow {
        blur,
        distance,
        direction: degrees * 60_000,
        scale,
        align: algn.map(str::to_owned),
        rotate_with_shape: rotate,
        colour: modified(ShapeColour::Rgb(0), &[("alpha", alpha)]),
    };
    let cases: [(&str, &str, f64, ShapeShadow); 8] = [
        ("Shadow offset BR blur 4pt", "rect", 0.0, shadow(50_800, 38_100, 45, 40_000, Some("tl"), (100_000, 100_000), false)),
        ("Shadow wide blur 12pt dist 8pt", "roundRect", 0.0, shadow(152_400, 101_600, 90, 50_000, None, (100_000, 100_000), false)),
        ("Shadow no blur dir 0", "rect", 0.0, shadow(0, 76_200, 0, 60_000, Some("l"), (100_000, 100_000), false)),
        ("Shadow scaled 90% algn t", "ellipse", 0.0, shadow(63_500, 0, 0, 35_000, Some("t"), (90_000, 90_000), false)),
        ("Shadow rot 30 not with shape", "rect", 30.0, shadow(50_800, 76_200, 45, 50_000, Some("tl"), (100_000, 100_000), false)),
        ("Shadow rot 30 with shape", "rect", 30.0, shadow(50_800, 76_200, 45, 50_000, Some("tl"), (100_000, 100_000), true)),
        ("Shadow diamond", "diamond", 0.0, shadow(38_100, 50_800, 135, 45_000, Some("b"), (100_000, 100_000), false)),
        ("Shadow flowChartDocument", "flowChartDocument", 0.0, shadow(50_800, 63_500, 60, 45_000, Some("br"), (100_000, 100_000), false)),
    ];
    for (i, (name, prst, degrees, s)) in cases.into_iter().enumerate() {
        let index = put(&mut shapes, &mut next, name, ShapeKind::Shape, prst, (i as i64 % 4, 22 + 5 * (i as i64 / 4)), (W, H), degrees);
        shapes[index].shadow = Some(s);
    }
    let outline = put(&mut shapes, &mut next, "Shadow of an outline only", ShapeKind::Shape, "rect", (0, 32), (W, H), 0.0);
    shapes[outline].fill = ShapeFill::None;
    shapes[outline].body.colour = Some(ShapeColour::Rgb(0x000000));
    shapes[outline].shadow = Some(shadow(50_800, 63_500, 45, 50_000, Some("tl"), (100_000, 100_000), false));
    let line = put(&mut shapes, &mut next, "Shadow of a line", ShapeKind::Connector, "line", (1, 32), (W, H), 0.0);
    shapes[line].shadow = Some(shadow(25_400, 63_500, 90, 50_000, Some("tl"), (100_000, 100_000), false));
    shapes[line].line.weight = Some(38_100);
    // The text rectangles of presets: a long text in each, wrapping inside the preset's rectangle (rows 38 on).
    let presets: [(&str, &[(&str, i64)]); 20] = [
        ("roundRect", &[("adj", 50_000)]),
        ("ellipse", &[]),
        ("triangle", &[]),
        ("rtTriangle", &[]),
        ("diamond", &[]),
        ("flowChartDecision", &[]),
        ("octagon", &[]),
        ("trapezoid", &[]),
        ("parallelogram", &[]),
        ("hexagon", &[]),
        ("pentagon", &[]),
        ("rightArrow", &[]),
        ("leftArrow", &[]),
        ("upArrow", &[]),
        ("downArrow", &[]),
        ("chevron", &[]),
        ("flowChartTerminator", &[]),
        ("flowChartDocument", &[]),
        ("wedgeRectCallout", &[]),
        ("wedgeRoundRectCallout", &[("adj1", -20_833), ("adj2", 62_500), ("adj3", 40_000)]),
    ];
    for (i, (prst, adjust)) in presets.into_iter().enumerate() {
        let name = format!("TextRect {prst}");
        let index = put(&mut shapes, &mut next, &name, ShapeKind::Shape, prst, (i as i64 % 4, 38 + 5 * (i as i64 / 4)), (W, H), 0.0);
        shapes[index].adjust = adjust.iter().map(|(n, v)| ((*n).to_owned(), *v)).collect();
        shapes[index].fill = ShapeFill::Solid(modified(scheme("accent1"), &[("lumMod", 40_000), ("lumOff", 60_000)]));
        shapes[index].body.colour = Some(ShapeColour::Rgb(0x000000));
        shapes[index].text = Some(vec![format!("{name} text wraps inside the text rectangle of its preset geometry")]);
    }
    // Bent and curved connectors with adjusts, arrowheads at the ends, some flipped and turned (rows 66 on).
    let connectors: [(&str, &[(&str, i64)], bool, bool, f64); 16] = [
        ("bentConnector2", &[], false, false, 0.0),
        ("bentConnector3", &[], false, false, 0.0),
        ("bentConnector3", &[("adj1", 25_000)], false, false, 0.0),
        ("bentConnector4", &[("adj1", 30_000), ("adj2", 120_000)], false, false, 0.0),
        ("bentConnector5", &[("adj1", 20_000), ("adj2", 40_000), ("adj3", 80_000)], false, false, 0.0),
        ("bentConnector3", &[], true, false, 0.0),
        ("bentConnector3", &[], false, true, 0.0),
        ("bentConnector2", &[], true, true, 0.0),
        ("curvedConnector2", &[], false, false, 0.0),
        ("curvedConnector3", &[], false, false, 0.0),
        ("curvedConnector3", &[("adj1", 25_000)], false, false, 0.0),
        ("curvedConnector4", &[("adj1", 30_000), ("adj2", 120_000)], false, false, 0.0),
        ("curvedConnector5", &[("adj1", 20_000), ("adj2", 40_000), ("adj3", 80_000)], false, false, 0.0),
        ("curvedConnector3", &[], true, false, 0.0),
        ("bentConnector3", &[], false, false, 90.0),
        ("curvedConnector3", &[], false, true, 90.0),
    ];
    for (i, (prst, adjust, flip_h, flip_v, degrees)) in connectors.into_iter().enumerate() {
        let tag = format!("{}{}{}{}", if flip_h { " flipH" } else { "" }, if flip_v { " flipV" } else { "" }, if degrees != 0.0 { " rot 90" } else { "" }, if adjust.is_empty() { String::new() } else { format!(" {}", adjust.iter().map(|(n, v)| format!("{n}={v}")).collect::<Vec<_>>().join(",")) });
        let name = format!("Conn {prst}{tag}");
        let index = put(&mut shapes, &mut next, &name, ShapeKind::Connector, prst, (i as i64 % 4, 66 + 5 * (i as i64 / 4)), (W, H), degrees);
        let s = &mut shapes[index];
        s.adjust = adjust.iter().map(|(n, v)| ((*n).to_owned(), *v)).collect();
        s.flip_horizontal = flip_h;
        s.flip_vertical = flip_v;
        s.fill = ShapeFill::None;
        s.text = None;
        s.line = ShapeLine {
            weight: Some(25_400),
            colour: Some(ShapeColour::Rgb(0xC00000)),
            head: Some(LineEnd { kind: "oval".into(), width: None, length: None }),
            tail: Some(LineEnd { kind: "triangle".into(), width: Some("med".into()), length: Some("med".into()) }),
            ..Default::default()
        };
    }
    // Flipped pictures and pictures inside groups (rows 88 on).
    let card = |id: u32, name: &str, crop: [i64; 4]| {
        let mut p = Shape::new(id, name.into(), ShapeKind::Picture, slot(0, 0, 0, 0));
        p.geometry = Some("rect".into());
        p.media = Some(ShapePicture { image: Some(media("image/png", CARD_PNG)), svg: None, crop, description: Some(format!("{name} (test card)")) });
        p
    };
    const PW: i64 = 1_828_800;
    const PH: i64 = 1_143_000;
    let flips: [(&str, bool, bool, f64, [i64; 4]); 5] = [
        ("Pic flipH", true, false, 0.0, [0; 4]),
        ("Pic flipV", false, true, 0.0, [0; 4]),
        ("Pic flipH+V", true, true, 0.0, [0; 4]),
        ("Pic flipH rot 30 crop", true, false, 30.0, [10_000, 0, 30_000, 20_000]),
        ("Pic flipV rot 90", false, true, 90.0, [0; 4]),
    ];
    for (i, (name, flip_h, flip_v, degrees, crop)) in flips.into_iter().enumerate() {
        let (bw, bh) = around(PW, PH, degrees);
        let mut p = card(next, name, crop);
        next += 1;
        p.anchor = slot(i as i64 % 4, 90 + 8 * (i as i64 / 4), bw, bh);
        p.extent = (PW, PH);
        p.rotation = (degrees * 60_000.0) as i64;
        p.flip_horizontal = flip_h;
        p.flip_vertical = flip_v;
        shapes.push(p);
    }
    for (i, (name, flip_h, degrees)) in [("Group with picture", false, 0.0), ("Group flipH with picture", true, 0.0), ("Group rot 20 with picture", false, 20.0)].into_iter().enumerate() {
        let (gw, gh) = (2 * PW, PH + 600_000);
        let (bw, bh) = around(gw, gh, degrees);
        let mut group = Shape::new(next, name.into(), ShapeKind::Group, slot(1 + i as i64, 98, bw, bh));
        next += 1;
        group.extent = (gw, gh);
        group.child_extent = (gw, gh);
        group.flip_horizontal = flip_h;
        group.rotation = (degrees * 60_000.0) as i64;
        let mut picture = card(next, &format!("{name} card"), [0; 4]);
        next += 1;
        picture.anchor = group.anchor;
        picture.offset = (0, 0);
        picture.extent = (PW, PH);
        let mut flipped = card(next, &format!("{name} card flipV"), [0; 4]);
        next += 1;
        flipped.anchor = group.anchor;
        flipped.offset = (PW, 600_000);
        flipped.extent = (PW, PH / 2);
        flipped.flip_vertical = true;
        let mut label = Shape::new(next, format!("{name} label"), ShapeKind::TextBox, group.anchor);
        next += 1;
        label.geometry = Some("rect".into());
        label.offset = (PW, 0);
        label.extent = (PW, 600_000);
        label.text = Some(vec![name.into()]);
        label.fill = ShapeFill::None;
        label.line = ShapeLine { hidden: true, ..Default::default() };
        group.children = vec![picture, flipped, label];
        shapes.push(group);
    }
    shapes
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_zjsg_2__shapes_paint3_workbook() {
    let base = load(SHAPES_BASE);
    let sheet = first_sheet(&base);
    let model = edited(&base, vec![WorkbookOp::SetShapes { sheet, shapes: paint3_shapes() }]);
    let (saved, _) = save_model(&base, &model);
    #[cfg(not(target_arch = "wasm32"))]
    if std::env::var_os("DNACALC_WRITE_FIXTURE").is_some() {
        std::fs::write(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/drawing/shapes-paint3.xlsx"), &saved).unwrap();
    }
    // The committed workbook is that save and reads back with each colour modifier, shadow and flip as made.
    let committed = load(SHAPES_PAINT3);
    let read = committed.sheets()[0].shapes();
    assert_eq!(read.unread, 0);
    let made = paint3_shapes();
    assert_eq!(read.shapes.len(), made.len());
    fn same(read: &Shape, made: &Shape) {
        assert_eq!((read.name.as_str(), read.kind, &read.geometry), (made.name.as_str(), made.kind, &made.geometry));
        assert_eq!((read.offset, read.extent, read.rotation), (made.offset, made.extent, made.rotation), "{}", made.name);
        assert_eq!((read.flip_horizontal, read.flip_vertical), (made.flip_horizontal, made.flip_vertical), "{}", made.name);
        assert_eq!(read.adjust, made.adjust, "{}", made.name);
        assert_eq!(read.fill, made.fill, "{}", made.name);
        assert_eq!(read.line.colour, made.line.colour, "{}", made.name);
        assert_eq!(read.shadow, made.shadow, "{}", made.name);
        assert_eq!(read.media.as_ref().map(|m| (m.crop, m.image.as_ref().map(|i| i.bytes.clone()))), made.media.as_ref().map(|m| (m.crop, m.image.as_ref().map(|i| i.bytes.clone()))), "{}", made.name);
        assert_eq!(read.children.len(), made.children.len(), "{}", made.name);
        for (r, m) in read.children.iter().zip(&made.children) {
            same(r, m);
        }
    }
    for (read, made) in read.shapes.iter().zip(&made) {
        same(read, made);
        assert_eq!(read.anchor, made.anchor, "{}", made.name);
    }
    // Spot checks that the made shapes hold what the names say.
    let by_name = |name: &str| made.iter().find(|s| s.name == name).unwrap_or_else(|| panic!("{name}"));
    assert!(matches!(&by_name("Mod accent1 shade50").fill, ShapeFill::Solid(ShapeColour::Modified(_, m)) if m == &[("shade".to_owned(), 50_000)]));
    assert_eq!(by_name("Shadow rot 30 with shape").shadow.as_ref().map(|s| s.rotate_with_shape), Some(true));
    assert_eq!(by_name("Group flipH with picture").children.len(), 3);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_8ari6__a_partial_guide_list_is_saved_complete() {
    // Excel 20527 repairs `<a:avLst>` of a hexagon that names adj without vf (bisect of shapes-paint2: anchor 42).
    let base = load(SHAPES_BASE);
    let sheet = first_sheet(&base);
    let mut shapes = Vec::new();
    let mut next = 2;
    let index = put(&mut shapes, &mut next, "hex", ShapeKind::Shape, "hexagon", (0, 56), (3 * CELL_W, 3 * CELL_H), 0.0);
    shapes[index].adjust = vec![("adj".to_owned(), 40_000)];
    let index = put(&mut shapes, &mut next, "callout", ShapeKind::Shape, "wedgeRoundRectCallout", (4, 56), (3 * CELL_W, 3 * CELL_H), 0.0);
    shapes[index].adjust = vec![("adj3".to_owned(), 40_000)];
    let model = edited(&base, vec![WorkbookOp::SetShapes { sheet, shapes }]);
    let (saved, _) = save_model(&base, &model);
    let part = part_text(&saved, "xl/drawings/dnaDrawing.xml");
    assert!(
        part.contains(r#"<a:avLst><a:gd name="adj" fmla="val 40000"/><a:gd name="vf" fmla="val 115470"/></a:avLst>"#),
        "{part}"
    );
    // The callout with adj3 alone (the failing anchor of shapes-paint3) lists adj1, adj2 and adj3.
    assert!(
        part.contains(r#"<a:avLst><a:gd name="adj1" fmla="val -20833"/><a:gd name="adj2" fmla="val 62500"/><a:gd name="adj3" fmla="val 40000"/></a:avLst>"#),
        "{part}"
    );
}

// dc-zjsg.2, fourth slice: shapes-paint4.xlsx (make-shapes-paint4.py, a drawing part written as Excel writes one)
// holds custom geometries, pattern fills, style references in other colours and turned rectangles. Each reads
// into the model, and an untouched save keeps the drawing and theme parts byte for byte and reads back the same.
const SHAPES_PAINT4: &[u8] = include_bytes!("fixtures/drawing/shapes-paint4.xlsx");

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_zjsg_2__custom_geometry_patterns_and_style_references_read_and_survive_save() {
    use calc_doc::model::{GeometryCommand, ShapeFill};
    let loaded = load(SHAPES_PAINT4);
    let shapes = &loaded.sheets()[0].shapes().shapes;
    assert_eq!(shapes.len(), 5 + 8 + 3 + 4);
    let by = |name: &str| shapes.iter().find(|s| s.name == name).unwrap_or_else(|| panic!("{name}"));
    // Custom geometry: no preset, its paths with their own spaces and commands.
    let triangle = by("Freeform triangle");
    assert_eq!(triangle.geometry, None);
    let custom = triangle.custom_geometry.as_ref().unwrap();
    assert_eq!((custom.paths.len(), custom.paths[0].width, custom.paths[0].height), (1, Some(1000), Some(1000)));
    assert_eq!(custom.paths[0].commands.len(), 4);
    assert_eq!(custom.paths[0].commands[0], GeometryCommand::Move(("500".into(), "0".into())));
    assert_eq!(custom.text_rect, Some(["l".into(), "t".into(), "r".into(), "b".into()]));
    let open = by("Freeform open arrow").custom_geometry.as_ref().unwrap();
    assert_eq!(open.paths[0].fill.as_deref(), Some("none"));
    let disc = by("Freeform half disc from guides").custom_geometry.as_ref().unwrap();
    assert_eq!(disc.guides, [("unused".to_owned(), "*/ w 1 2".to_owned())]);
    assert_eq!(disc.paths[0].commands[1], GeometryCommand::Arc(["hc".into(), "vc".into(), "0".into(), "cd2".into()]));
    let curves = by("Freeform curves").custom_geometry.as_ref().unwrap();
    assert!(matches!(curves.paths[0].commands[1], GeometryCommand::Quad(_)));
    assert!(matches!(curves.paths[0].commands[2], GeometryCommand::Cubic(_)));
    assert_eq!(by("Freeform two paths, one unfilled").custom_geometry.as_ref().unwrap().paths.len(), 2);
    // Pattern fills with their colours.
    let ShapeFill::Pattern(pattern) = &by("Pattern dkUpDiag").fill else { panic!("a pattern") };
    assert_eq!(pattern.preset, "dkUpDiag");
    assert_eq!(pattern.foreground, Some(ShapeColour::Rgb(0xC00000)));
    assert_eq!(pattern.background, Some(ShapeColour::Rgb(0xFFFF00)));
    // Style references: index and colour of each, the font reference's colour.
    let style = by("Style accent6 line only").style.as_ref().unwrap();
    assert_eq!((style.line.index, style.fill.index), (3, 0));
    assert_eq!(style.line.colour, Some(ShapeColour::Scheme("accent6".into())));
    assert_eq!(style.font, Some(ShapeColour::Scheme("dk1".into())));
    assert_eq!(by("Rect rot 120").rotation, 120 * 60_000);
    // The theme's line styles, for lnRef.
    assert_eq!(loaded.theme_line_widths(), [6_350, 12_700, 19_050]);
    // An untouched save keeps the parts as stored and reads back the same objects.
    let saved = save_untouched(SHAPES_PAINT4);
    for part_name in ["xl/drawings/drawing1.xml", "xl/theme/theme1.xml"] {
        assert_eq!(part(&saved, part_name), part(SHAPES_PAINT4, part_name), "{part_name}");
    }
    assert_eq!(&load(&saved).sheets()[0].shapes().shapes, shapes);
}

// dc-zjsg.9: kept-objects.xlsx (make-kept-objects.py) holds an equation, ink, a SmartArt diagram and an OLE object as
// Excel stores them. Each is read as what DNA Calc draws for it (the equation's and the ink's fallback pictures, the
// diagram's cached shapes, the OLE object's picture), marked kept; an untouched save keeps every part as stored.
const KEPT_OBJECTS: &[u8] = include_bytes!("fixtures/drawing/kept-objects.xlsx");

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_zjsg_9__equations_ink_smartart_and_ole_objects_are_kept_and_drawn_from_their_fallbacks() {
    let loaded = load(KEPT_OBJECTS);
    let read = loaded.sheets()[0].shapes();
    assert_eq!(read.unread, 0, "{:?}", read.unread_by);
    let kinds: Vec<_> = read.shapes.iter().map(|s| (s.id, s.name.as_str(), s.kind, s.kept.as_deref(), s.hidden)).collect();
    assert_eq!(
        kinds,
        [
            (2, "TextBox 1", ShapeKind::TextBox, Some("equation"), false),
            (3, "Ink 2", ShapeKind::Picture, Some("ink"), false),
            (4, "Diagram 3", ShapeKind::Group, Some("SmartArt"), false),
            (1025, "Object 1", ShapeKind::Shape, None, true),
            (1025, "Object 1", ShapeKind::Picture, Some("OLE object"), false),
        ]
    );
    // The equation: the Fallback's picture fill and unfilled text, the Choice's OMML.
    let equation = &read.shapes[0];
    assert_eq!(equation.fill, ShapeFill::Picture("rId1".into()));
    assert!(equation.body.text_hidden);
    let image = equation.media.as_ref().unwrap().image.as_ref().unwrap();
    assert_eq!((image.part.as_str(), image.content_type.as_str()), ("xl/media/image1.png", "image/png"));
    let math = equation.math.as_ref().unwrap();
    assert_eq!(math.local, "oMathPara");
    let omath = math.child("oMath").unwrap();
    assert_eq!(omath.children.iter().map(|c| c.local.as_str()).collect::<Vec<_>>(), ["f", "r", "sSup", "r", "rad", "r", "nary", "d"]);
    let nary = omath.child("nary").unwrap();
    assert_eq!(nary.child("naryPr").unwrap().child("chr").unwrap().attribute("m:val"), Some("\u{2211}"));
    // Ink: the Fallback picture.
    assert_eq!(read.shapes[1].media.as_ref().unwrap().image.as_ref().unwrap().part, "xl/media/image2.png");
    // SmartArt: the five shapes of the cached diagram drawing, in the frame's space.
    let diagram = &read.shapes[2];
    assert_eq!(diagram.child_extent, (512 * 9525, 160 * 9525));
    let geometries: Vec<_> = diagram.children.iter().map(|c| c.geometry.as_deref().unwrap()).collect();
    assert_eq!(geometries, ["roundRect", "rightArrow", "roundRect", "rightArrow", "roundRect"]);
    assert_eq!(diagram.children[2].text.as_deref(), Some(&["Build".to_owned()][..]));
    assert_eq!(diagram.children[2].offset, (192 * 9525, 30 * 9525));
    // The OLE object: its picture from the sheet's relationship, its program as the description.
    let ole = &read.shapes[4];
    let media = ole.media.as_ref().unwrap();
    assert_eq!((media.image.as_ref().unwrap().part.as_str(), media.description.as_deref()), ("xl/media/image3.png", Some("Package")));
    assert_eq!(ole.anchor, cells(at(11, 8), at(13, 13)));
    // An untouched save keeps every part as stored, and reads back the same objects.
    let saved = save_untouched(KEPT_OBJECTS);
    for part_name in [
        "xl/worksheets/sheet1.xml",
        "xl/drawings/drawing1.xml",
        "xl/drawings/_rels/drawing1.xml.rels",
        "xl/drawings/vmlDrawing1.vml",
        "xl/ink/ink1.xml",
        "xl/diagrams/data1.xml",
        "xl/diagrams/drawing1.xml",
        "xl/embeddings/Package1.bin",
        "xl/media/image1.png",
        "xl/media/image3.png",
    ] {
        assert_eq!(part(&saved, part_name), part(KEPT_OBJECTS, part_name), "{part_name}");
    }
    assert_eq!(&load(&saved).sheets()[0].shapes().shapes, &read.shapes);
}

// dc-zjsg.7: sparklines.xlsx (make-sparklines.py) holds four sparkline groups in the worksheet's x14 extension. Each
// reads with its look and sparklines; an untouched save keeps the worksheet part as stored.
const SPARKLINES: &[u8] = include_bytes!("fixtures/drawing/sparklines.xlsx");

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_zjsg_7__sparkline_groups_read_and_survive_save() {
    let loaded = load(SPARKLINES);
    let groups = loaded.sheets()[0].sparklines();
    assert_eq!(groups.len(), 4);
    let kinds: Vec<_> = groups.iter().map(|g| (g.kind.as_deref(), g.sparklines.len())).collect();
    assert_eq!(kinds, [(None, 2), (Some("column"), 2), (Some("stacked"), 1), (None, 1)]);
    let line = &groups[0];
    assert!(line.markers && line.high && line.low && !line.negative);
    assert_eq!(line.empty_cells.as_deref(), Some("gap"));
    assert_eq!((line.sparklines[1].data.as_str(), line.sparklines[1].cell.as_str()), ("Spark!A3:E3", "F3"));
    let series = line.colour("colorSeries").unwrap();
    assert_eq!((series.theme.as_deref(), series.tint.as_deref()), (Some("4"), Some("-0.499984740745262")));
    assert_eq!(groups[1].min_axis.as_deref(), Some("group"));
    assert!(groups[1].axis && groups[1].negative);
    let custom = &groups[3];
    assert_eq!((custom.manual_min.as_deref(), custom.manual_max.as_deref(), custom.line_weight.as_deref()), (Some("0"), Some("10"), Some("2.25")));
    let sheet = first_sheet(&loaded);
    assert_eq!(loaded.model().definitions().sparklines(sheet), groups);
    let saved = save_untouched(SPARKLINES);
    assert_eq!(part(&saved, "xl/worksheets/sheet1.xml"), part(SPARKLINES, "xl/worksheets/sheet1.xml"));
    assert_eq!(load(&saved).sheets()[0].sparklines(), groups);
}

// dc-zjsg.8: form-controls.xlsx (make-form-controls.py) holds the nine legacy form controls as Excel 2010+ stores
// them. Each reads with its anchor, ctrlProp properties and the caption of its hidden drawing shape; an untouched save
// keeps the sheet, the ctrlProp parts and the VML as stored.
const FORM_CONTROLS: &[u8] = include_bytes!("fixtures/drawing/form-controls.xlsx");

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_zjsg_8__form_controls_read_with_their_properties_and_captions() {
    let loaded = load(FORM_CONTROLS);
    let read = loaded.sheets()[0].shapes();
    let kinds: Vec<_> = read.controls.iter().map(|c| (c.id, c.kind.as_str(), c.caption.as_deref())).collect();
    assert_eq!(
        kinds,
        [
            (1025, "Button", Some("Run")),
            (1026, "CheckBox", Some("Include tax")),
            (1027, "Radio", Some("Monthly")),
            (1028, "Radio", Some("Yearly")),
            (1029, "List", None),
            (1030, "Scroll", None),
            (1031, "Spin", None),
            (1032, "Drop", None),
            (1033, "GBox", Some("Options")),
            (1034, "Label", Some("A label")),
        ]
    );
    let check = &read.controls[1];
    assert_eq!((check.checked.as_deref(), check.link.as_deref()), (Some("Checked"), Some("$B$2")));
    assert_eq!(check.anchor, cells(at(6, 4), at(8, 5)));
    assert_eq!(read.controls[0].macro_name.as_deref(), Some("[0]!Button1_Click"));
    let list = &read.controls[4];
    assert_eq!((list.range.as_deref(), list.selected), (Some("$D$2:$D$5"), Some(3)));
    let scroll = &read.controls[5];
    assert_eq!((scroll.value, scroll.max, scroll.page, scroll.horizontal), (Some(30), Some(100), Some(10), true));
    // The hidden drawing shapes stay hidden objects of the drawing.
    assert!(read.shapes.iter().all(|s| s.hidden));
    let saved = save_untouched(FORM_CONTROLS);
    for part_name in ["xl/worksheets/sheet1.xml", "xl/ctrlProps/ctrlProp2.xml", "xl/drawings/vmlDrawing1.vml", "xl/drawings/drawing1.xml"] {
        assert_eq!(part(&saved, part_name), part(FORM_CONTROLS, part_name), "{part_name}");
    }
    assert_eq!(load(&saved).sheets()[0].shapes().controls, read.controls);
}
