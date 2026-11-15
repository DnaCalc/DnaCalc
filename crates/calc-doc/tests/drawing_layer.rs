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
