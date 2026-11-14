//! A worksheet's drawing objects other than charts (dc-zjsg.1): shapes, text boxes, pictures,
//! connectors and groups, read from the drawing part its relationships name. Each anchor is read
//! into a small tree and the fields the model holds are taken from it; the part itself stays in
//! the package as stored, so what is not held here (effects, custom geometry, rich text runs,
//! extension lists) is never lost. An anchor holding anything else (SmartArt, OLE, ink, an
//! alternate-content block) is counted in [`SheetShapes::unread`]; chart frames are the chart
//! reader's and are not counted.

use super::{Events, LoadError, read_xml};
use crate::opc::Package;
use crate::xml::XmlEvent;
use calc_doc_model::{
    ChartAnchor, DrawingPoint, Shape, ShapeColour, ShapeConnection, ShapeFill, ShapeKind,
    ShapeLine, SheetShapes,
};

const DRAWING: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing";
const SPREADSHEET_DRAWING: &str =
    "http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing";

/// One element with its attributes (local names), text and children.
struct Node {
    local: String,
    attributes: Vec<(String, String)>,
    text: String,
    children: Vec<Node>,
}

impl Node {
    fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
    fn child(&self, local: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.local == local)
    }
    fn number(&self, name: &str) -> Option<i64> {
        self.attribute(name).and_then(|v| v.trim().parse().ok())
    }
    fn flag(&self, name: &str) -> bool {
        matches!(self.attribute(name), Some("1" | "true"))
    }
}

fn tree(events: &mut Events<'_>, start: crate::xml::XmlElement) -> Result<Node, LoadError> {
    let mut node = Node {
        local: start.name.local.as_str().to_owned(),
        attributes: start
            .attributes
            .iter()
            .filter(|a| a.name.namespace.is_none())
            .map(|a| (a.name.local.as_str().to_owned(), a.value.clone()))
            .collect(),
        text: String::new(),
        children: Vec::new(),
    };
    // The relationship id of a blip is the one namespaced attribute read.
    for attribute in &start.attributes {
        if attribute.name.namespace.is_some() && attribute.name.local == "embed" {
            node.attributes
                .push(("r:embed".to_owned(), attribute.value.clone()));
        }
    }
    loop {
        match events.next()? {
            Some(XmlEvent::Start(child)) => node.children.push(tree(events, child)?),
            Some(XmlEvent::Text(text)) => node.text.push_str(&text),
            Some(XmlEvent::End(_)) => return Ok(node),
            None => return Err(events.invalid("the part ends inside an element")),
        }
    }
}

/// The drawing objects of the worksheet part `sheet_part`, in the drawing's order.
pub(crate) fn read_shapes(package: &Package, sheet_part: &str) -> SheetShapes {
    let mut read = SheetShapes::default();
    let Some(drawing) = package.relationships_of(Some(sheet_part)).and_then(|r| {
        r.items()
            .iter()
            .find(|item| item.kind == DRAWING && item.mode == crate::opc::TargetMode::Internal)
            .and_then(|item| item.resolved.clone())
    }) else {
        return read;
    };
    match read_drawing(package, &drawing) {
        Ok(shapes) => shapes,
        Err(_) => {
            read.unread += 1;
            read
        }
    }
}

fn read_drawing(package: &Package, part: &str) -> Result<SheetShapes, LoadError> {
    let xml = read_xml(package, part)?;
    let mut events = Events::new(&xml);
    events.root(
        SPREADSHEET_DRAWING,
        "wsDr",
        "the document element is not a spreadsheet drawing",
    )?;
    let mut read = SheetShapes::default();
    loop {
        let start = match events.next()? {
            Some(XmlEvent::Start(element)) => element,
            Some(XmlEvent::Text(_)) => continue,
            Some(XmlEvent::End(_)) => break,
            None => return Err(events.invalid("the part ends inside wsDr")),
        };
        if !matches!(
            start.name.local.as_str(),
            "twoCellAnchor" | "oneCellAnchor" | "absoluteAnchor"
        ) {
            // A block with a fallback around whole anchors: say what its first branch holds.
            let label = if start.name.local == "AlternateContent" {
                let block = tree(&mut events, start)?;
                // Whole anchors in a block with a fallback (Excel writes a shape with an effect
                // it added later, a form control, a slicer or a chartex frame this way): the
                // branch Excel reads gives the anchors, read as any other.
                if let Some(branch) = alternate_content_branch(&block) {
                    if branch.children.iter().any(is_anchor) {
                        for anchor in branch.children.iter().filter(|c| is_anchor(c)) {
                            match anchor_shape(anchor) {
                                Anchored::Shape(shape) => read.shapes.push(shape),
                                Anchored::Chart => {}
                                Anchored::Unread(label) => {
                                    read.unread += 1;
                                    *read.unread_by.entry(label).or_default() += 1;
                                }
                            }
                        }
                        continue;
                    }
                }
                let held = block
                    .child("Choice")
                    .and_then(|c| c.children.first())
                    .and_then(|anchor| {
                        anchor.children.iter().find(|c| {
                            !matches!(
                                c.local.as_str(),
                                "from" | "to" | "pos" | "ext" | "clientData"
                            )
                        })
                    })
                    .map_or("nothing".to_owned(), |held| match held.local.as_str() {
                        "graphicFrame" => held
                            .child("graphic")
                            .and_then(|g| g.child("graphicData"))
                            .and_then(|d| d.attribute("uri"))
                            .map_or("graphicFrame", |uri| uri.rsplit('/').next().unwrap_or(uri))
                            .to_owned(),
                        other => other.to_owned(),
                    });
                format!("AlternateContent ({held})")
            } else {
                let label = start.name.local.as_str().to_owned();
                events.skip()?;
                label
            };
            read.unread += 1;
            *read.unread_by.entry(label).or_default() += 1;
            continue;
        }
        let anchor = tree(&mut events, start)?;
        match anchor_shape(&anchor) {
            Anchored::Shape(shape) => read.shapes.push(shape),
            Anchored::Chart => {}
            Anchored::Unread(label) => {
                read.unread += 1;
                *read.unread_by.entry(label).or_default() += 1;
            }
        }
    }
    Ok(read)
}

enum Anchored {
    Shape(Shape),
    Chart,
    Unread(String),
}

fn point(node: &Node) -> DrawingPoint {
    let value = |name: &str| {
        node.child(name)
            .and_then(|c| c.text.trim().parse::<i64>().ok())
            .unwrap_or(0)
    };
    DrawingPoint {
        column: u32::try_from(value("col")).unwrap_or(0),
        column_offset: value("colOff"),
        row: u32::try_from(value("row")).unwrap_or(0),
        row_offset: value("rowOff"),
    }
}

fn anchor_shape(anchor: &Node) -> Anchored {
    let position = match anchor.local.as_str() {
        "twoCellAnchor" => ChartAnchor::TwoCell {
            from: anchor.child("from").map(point).unwrap_or_default(),
            to: anchor.child("to").map(point).unwrap_or_default(),
        },
        "oneCellAnchor" => ChartAnchor::OneCell {
            from: anchor.child("from").map(point).unwrap_or_default(),
            width: anchor
                .child("ext")
                .and_then(|e| e.number("cx"))
                .unwrap_or(0),
            height: anchor
                .child("ext")
                .and_then(|e| e.number("cy"))
                .unwrap_or(0),
        },
        _ => ChartAnchor::Absolute {
            x: anchor.child("pos").and_then(|e| e.number("x")).unwrap_or(0),
            y: anchor.child("pos").and_then(|e| e.number("y")).unwrap_or(0),
            width: anchor
                .child("ext")
                .and_then(|e| e.number("cx"))
                .unwrap_or(0),
            height: anchor
                .child("ext")
                .and_then(|e| e.number("cy"))
                .unwrap_or(0),
        },
    };
    let Some(object) = anchor.children.iter().find(|c| {
        !matches!(
            c.local.as_str(),
            "from" | "to" | "pos" | "ext" | "clientData"
        )
    }) else {
        return Anchored::Unread("empty anchor".into());
    };
    // A block with a fallback around the object (Excel writes a shape with an effect it adds
    // later, a slicer, a timeline, a chartex frame this way): read the branch Excel reads. The
    // saved part is never rewritten from what is read here, so an untouched save keeps the block.
    let object = alternate_branch(object).unwrap_or(object);
    if object.local == "graphicFrame" && is_chart_frame(object) {
        return Anchored::Chart;
    }
    let Some(mut shape) = object_shape(object, position) else {
        let label = match object.local.as_str() {
            "graphicFrame" => format!(
                "graphicFrame ({})",
                object
                    .child("graphic")
                    .and_then(|g| g.child("graphicData"))
                    .and_then(|d| d.attribute("uri"))
                    .map_or("no data", |uri| uri.rsplit('/').next().unwrap_or(uri))
            ),
            "sp" | "pic" | "cxnSp" | "grpSp" => format!("{} (not read)", object.local),
            // A block with a fallback: what its first branch holds (a shape with an effect the
            // fallback leaves out, a slicer or timeline frame, a content part).
            "AlternateContent" => {
                let held = object
                    .child("Choice")
                    .and_then(|c| c.children.first())
                    .map_or("nothing", |held| match held.local.as_str() {
                        "graphicFrame" => held
                            .child("graphic")
                            .and_then(|g| g.child("graphicData"))
                            .and_then(|d| d.attribute("uri"))
                            .map_or("graphicFrame", |uri| uri.rsplit('/').next().unwrap_or(uri)),
                        other => other,
                    });
                format!("AlternateContent ({held})")
            }
            other => other.to_owned(),
        };
        return Anchored::Unread(label);
    };
    shape.edit_as = anchor.attribute("editAs").map(str::to_owned);
    Anchored::Shape(shape)
}

/// The namespace prefixes a `mc:Choice` may require and Excel understands (the producers' usual
/// prefixes: shape effects, drawing extensions, slicers, timelines, chartex, word-processing shapes).
fn understood_prefix(prefix: &str) -> bool {
    matches!(
        prefix,
        "a14" | "a15" | "a16" | "a1" | "wps" | "wpg" | "wp14" | "sle15" | "sle" | "tsle" | "xdr14"
            | "x14" | "x15" | "x12ac" | "v" | "w14" | "w15" | "cx" | "cx1" | "cx2" | "cx3" | "cx4"
            | "cx5" | "cx6" | "cx7" | "cx8" | "c14" | "c16" | "c16r2" | "c16r3" | "dgm14"
    )
}

/// The first object of the branch of an `mc:AlternateContent` block Excel takes: the `Choice`
/// whose required namespaces it understands, else the `Fallback`. `None` when `object` is not
/// such a block or the branch holds nothing.
fn alternate_branch(object: &Node) -> Option<&Node> {
    if object.local != "AlternateContent" {
        return None;
    }
    alternate_content_branch(object)?.children.first()
}

/// The branch of the `mc:AlternateContent` block `block` that Excel takes.
fn alternate_content_branch(block: &Node) -> Option<&Node> {
    let choice = block.children.iter().find(|choice| {
        choice.local == "Choice"
            && choice
                .attribute("Requires")
                .is_none_or(|requires| requires.split_whitespace().all(understood_prefix))
    });
    choice.or_else(|| block.child("Fallback"))
}

fn is_anchor(node: &Node) -> bool {
    matches!(
        node.local.as_str(),
        "twoCellAnchor" | "oneCellAnchor" | "absoluteAnchor"
    )
}

fn is_chart_frame(frame: &Node) -> bool {
    frame
        .child("graphic")
        .and_then(|g| g.child("graphicData"))
        .and_then(|d| d.attribute("uri"))
        .is_some_and(|uri| uri == "http://schemas.openxmlformats.org/drawingml/2006/chart")
}

fn object_shape(node: &Node, anchor: ChartAnchor) -> Option<Shape> {
    let (kind, non_visual, properties) = match node.local.as_str() {
        "sp" => (ShapeKind::Shape, "nvSpPr", "spPr"),
        "cxnSp" => (ShapeKind::Connector, "nvCxnSpPr", "spPr"),
        "pic" => (ShapeKind::Picture, "nvPicPr", "spPr"),
        "grpSp" => (ShapeKind::Group, "nvGrpSpPr", "grpSpPr"),
        _ => return None,
    };
    let non_visual = node.child(non_visual)?;
    let identity = non_visual.child("cNvPr")?;
    let mut shape = Shape::new(
        u32::try_from(identity.number("id")?).ok()?,
        identity.attribute("name").unwrap_or_default().to_owned(),
        kind,
        anchor,
    );
    shape.hidden = identity.flag("hidden");
    if kind == ShapeKind::Shape && non_visual.child("cNvSpPr").is_some_and(|p| p.flag("txBox")) {
        shape.kind = ShapeKind::TextBox;
    }
    if let Some(properties) = node.child(properties) {
        shape_properties(&mut shape, properties);
    }
    match kind {
        ShapeKind::Connector => {
            let connection = |name: &str| {
                non_visual
                    .child("cNvCxnSpPr")
                    .and_then(|p| p.child(name))
                    .and_then(|c| {
                        Some(ShapeConnection {
                            shape: u32::try_from(c.number("id")?).ok()?,
                            site: u32::try_from(c.number("idx")?).ok()?,
                        })
                    })
            };
            shape.begin = connection("stCxn");
            shape.end = connection("endCxn");
        }
        ShapeKind::Picture => {
            shape.picture = node
                .child("blipFill")
                .and_then(|f| f.child("blip"))
                .and_then(|b| b.attribute("r:embed"))
                .map(str::to_owned);
        }
        ShapeKind::Group => {
            shape.children = node
                .children
                .iter()
                .filter_map(|child| object_shape(child, anchor))
                .collect();
        }
        _ => {}
    }
    if let Some(body) = node.child("txBody") {
        shape.text = Some(
            body.children
                .iter()
                .filter(|p| p.local == "p")
                .map(|p| {
                    p.children
                        .iter()
                        .map(|run| match run.local.as_str() {
                            "r" | "fld" => {
                                run.child("t").map(|t| t.text.clone()).unwrap_or_default()
                            }
                            "br" => "\u{b}".to_owned(),
                            _ => String::new(),
                        })
                        .collect::<String>()
                })
                .collect(),
        );
    }
    Some(shape)
}

fn colour(fill: &Node) -> Option<ShapeColour> {
    let c = fill.children.first()?;
    match c.local.as_str() {
        "srgbClr" => u32::from_str_radix(c.attribute("val")?, 16)
            .ok()
            .map(ShapeColour::Rgb),
        "schemeClr" => Some(ShapeColour::Scheme(c.attribute("val")?.to_owned())),
        _ => None,
    }
}

fn shape_properties(shape: &mut Shape, properties: &Node) {
    if let Some(xfrm) = properties.child("xfrm") {
        shape.rotation = xfrm.number("rot").unwrap_or(0);
        shape.flip_horizontal = xfrm.flag("flipH");
        shape.flip_vertical = xfrm.flag("flipV");
        if let Some(off) = xfrm.child("off") {
            shape.offset = (off.number("x").unwrap_or(0), off.number("y").unwrap_or(0));
        }
        if let Some(ext) = xfrm.child("ext") {
            shape.extent = (ext.number("cx").unwrap_or(0), ext.number("cy").unwrap_or(0));
        }
    }
    shape.geometry = properties
        .child("prstGeom")
        .and_then(|g| g.attribute("prst"))
        .map(str::to_owned);
    for child in &properties.children {
        match child.local.as_str() {
            "noFill" => shape.fill = ShapeFill::None,
            "solidFill" => {
                shape.fill = colour(child).map_or(ShapeFill::Other, ShapeFill::Solid);
            }
            "gradFill" | "pattFill" | "blipFill" | "grpFill" => shape.fill = ShapeFill::Other,
            "ln" => {
                let mut line = ShapeLine {
                    weight: child.number("w"),
                    ..Default::default()
                };
                for part in &child.children {
                    match part.local.as_str() {
                        "noFill" => line.hidden = true,
                        "solidFill" => line.colour = colour(part),
                        "prstDash" => line.dash = part.attribute("val").map(str::to_owned),
                        _ => {}
                    }
                }
                shape.line = line;
            }
            _ => {}
        }
    }
}
