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
