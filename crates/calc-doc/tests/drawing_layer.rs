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
