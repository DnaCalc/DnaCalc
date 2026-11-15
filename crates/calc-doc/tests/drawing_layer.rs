//! dc-zjsg.1: the drawing layer in the document model. The fixture (tests/fixtures/drawing,
//! written by make-drawing-fixture.py) holds an auto shape, a text box, a picture, a connector
//! glued to two shapes, a group of two and a diagram frame the model does not hold. Reading
//! gives each its fields; an untouched save keeps every drawing part byte for byte; a new shape
//! is appended after the stored anchors, which are never rewritten.

#![allow(non_snake_case)]

mod cdoc_m1_1_support;

use calc_doc::model::value::SheetId;
use calc_doc::model::{
    ChartAnchor, DrawingPoint, Shape, ShapeColour, ShapeConnection, ShapeFill, ShapeKind,
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

fn paint2_shapes() -> Vec<Shape> {
    use calc_doc::model::{LineEnd, ShapeGradient, ShapeLine};
    const W: i64 = 3 * CELL_W;
    const H: i64 = 3 * CELL_H;
    let mut shapes: Vec<Shape> = Vec::new();
    let mut next = 2u32;
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
        shape.line = ShapeLine { weight: Some(19_050), colour: Some(ShapeColour::Rgb(0x1F3864)), ..Default::default() };
        shape.text = Some(vec![name.into()]);
        shape.body.anchor = Some("ctr".into());
        shape.body.alignment = vec![Some("ctr".into())];
        shape.body.size = Some(900);
        shape.body.colour = Some(ShapeColour::Rgb(0xFFFFFF));
        shapes.push(shape);
        shapes.len() - 1
    }
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
        ("hexagon", &[("adj", 40_000)]),
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
