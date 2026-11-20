//! A worksheet's drawing objects other than charts (dc-zjsg.1): shapes, text boxes, pictures,
//! connectors and groups, read from the drawing part its relationships name. Each anchor is read
//! into a small tree and the fields the model holds are taken from it; the part itself stays in
//! the package as stored, so what is not held here (effects, custom geometry, unsupported rich runs,
//! extension lists) is never lost. An anchor holding anything else (SmartArt, OLE, ink, an
//! alternate-content block) is counted in [`SheetShapes::unread`]; chart frames are the chart
//! reader's and are not counted.

use super::{Events, LoadError, read_xml};
use crate::opc::Package;
use crate::xml::XmlEvent;
use calc_doc_model::{
    ChartAnchor, CustomGeometry, DrawingNode, DrawingPoint, FormControl, GeometryCommand, GeometryPath, GeometryPoint, LineEnd, ShapePattern, ShapeStyle,
    ShapeStyleRef, Shape, ShapeBody, ShapeColour, ShapeConnection, ShapeFill, ShapeShadow,
    PictureMedia, ShapeGradient, ShapeKind, ShapeLine, ShapePicture, ShapeTextRun, SheetShapes,
};

const DRAWING: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing";
const RELATIONSHIPS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const MATH: &str = "http://schemas.openxmlformats.org/officeDocument/2006/math";
const DIAGRAM: &str = "http://schemas.openxmlformats.org/drawingml/2006/diagram";
const SPREADSHEET_DRAWING: &str =
    "http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing";

/// One element with its attributes (local names), text and children.
pub(super) struct Node {
    pub(super) local: String,
    attributes: Vec<(String, String)>,
    pub(super) text: String,
    pub(super) children: Vec<Node>,
}

impl Node {
    pub(super) fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
    pub(super) fn child(&self, local: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.local == local)
    }
    pub(super) fn number(&self, name: &str) -> Option<i64> {
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
    // Namespaced attributes read: a blip's relationship id (`r:embed`), the other relationship ids (`r:id`,
    // `r:dm`) and OMML's (`m:val`).
    for attribute in &start.attributes {
        let prefix = match attribute.name.namespace.as_deref() {
            _ if attribute.name.local == "embed" && attribute.name.namespace.is_some() => "r",
            Some(RELATIONSHIPS) => "r",
            Some(MATH) => "m",
            _ => continue,
        };
        node.attributes.push((format!("{prefix}:{}", attribute.name.local), attribute.value.clone()));
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
    let mut read = match read_drawing(package, &drawing) {
        Ok(mut shapes) => {
            let relationships = package.relationships_of(Some(&drawing));
            let mut kept = std::collections::BTreeMap::new();
            for shape in &mut shapes.shapes {
                resolve_media(package, relationships, shape, &mut kept);
                if shape.kept.as_deref() == Some("SmartArt") {
                    shape.children = diagram_shapes(package, relationships, shape).unwrap_or_default();
                }
            }
            // A SmartArt frame without the drawing Excel caches for it is kept in the package, not drawn.
            let before = shapes.shapes.len();
            shapes.shapes.retain(|s| s.kept.as_deref() != Some("SmartArt") || !s.children.is_empty());
            let dropped = before - shapes.shapes.len();
            if dropped > 0 {
                shapes.unread += dropped;
                *shapes.unread_by.entry("graphicFrame (diagram)".into()).or_default() += dropped;
            }
            shapes
        }
        Err(_) => {
            read.unread += 1;
            read
        }
    };
    let (ole, controls, active_x) = sheet_objects(package, sheet_part);
    read.shapes.extend(ole);
    read.shapes.extend(active_x);
    // A form control's caption is the text of the hidden shape Excel writes for it in the drawing.
    read.controls = controls
        .into_iter()
        .map(|mut control| {
            control.caption = read
                .shapes
                .iter()
                .find(|s| s.id == control.id && s.hidden)
                .and_then(|s| s.text.as_ref())
                .map(|p| p.join("\n"))
                .filter(|t| !t.is_empty());
            control
        })
        .collect();
    read
}

/// The first element of the part `part` as a tree.
pub(super) fn part_tree(package: &Package, part: &str) -> Option<Node> {
    let xml = read_xml(package, part).ok()?;
    let mut events = Events::new(&xml);
    loop {
        match events.next().ok()?? {
            XmlEvent::Start(element) => return tree(&mut events, element).ok(),
            _ => continue,
        }
    }
}

fn descendant<'a>(node: &'a Node, local: &str) -> Option<&'a Node> {
    node.children.iter().find_map(|c| if c.local == local { Some(c) } else { descendant(c, local) })
}

fn resolved(relationships: Option<&crate::opc::Relationships>, id: Option<&str>) -> Option<String> {
    relationships?
        .items()
        .iter()
        .find(|r| Some(r.id.as_str()) == id && r.mode == crate::opc::TargetMode::Internal)?
        .resolved
        .clone()
}

/// The shapes of the drawing Excel caches for a SmartArt frame (dc-zjsg.9): its data part names the diagram
/// drawing (`dsp:dataModelExt relId`, a relationship of the sheet's drawing), whose `dsp:spTree` holds ordinary
/// DrawingML shapes placed in the frame's space.
fn diagram_shapes(package: &Package, relationships: Option<&crate::opc::Relationships>, frame: &Shape) -> Option<Vec<Shape>> {
    let data = part_tree(package, &resolved(relationships, frame.source_part.as_deref())?)?;
    let id = descendant(&data, "dataModelExt")?.attribute("relId")?;
    let drawing = part_tree(package, &resolved(relationships, Some(id))?)?;
    let tree = descendant(&drawing, "spTree")?;
    Some(tree.children.iter().filter_map(|child| object_shape(child, frame.anchor)).collect())
}

/// The sheet's objects stored outside its drawing: OLE objects (dc-zjsg.9) as pictures, legacy form controls
/// (dc-zjsg.8), and ActiveX controls as pictures of the image Excel stores for them; all kept as stored.
fn sheet_objects(package: &Package, sheet_part: &str) -> (Vec<Shape>, Vec<FormControl>, Vec<Shape>) {
    let relationships = package.relationships_of(Some(sheet_part));
    let none = || (Vec::new(), Vec::new(), Vec::new());
    if !relationships.is_some_and(|r| {
        r.items().iter().any(|i| {
            i.kind.ends_with("/oleObject") || i.kind.ends_with("/package") || i.kind.ends_with("/ctrlProp") || i.kind.ends_with("/control")
        })
    }) {
        return none();
    }
    let Ok(xml) = read_xml(package, sheet_part) else { return none() };
    let mut events = Events::new(&xml);
    let (mut objects, mut controls) = (None, None);
    while let Ok(Some(event)) = events.next() {
        if let XmlEvent::Start(element) = event {
            match element.name.local.as_str() {
                "sheetData" => {
                    if events.skip().is_err() {
                        return none();
                    }
                }
                "oleObjects" => objects = tree(&mut events, element).ok(),
                "controls" => controls = tree(&mut events, element).ok(),
                _ => {}
            }
        }
    }
    let (form, active_x) = controls.map_or((Vec::new(), Vec::new()), |c| form_controls(package, relationships, &c));
    (objects.map_or_else(Vec::new, |o| ole_objects(package, relationships, &o)), form, active_x)
}

/// The anchored `control`s of a worksheet's `controls`: a form control from its ctrlProp part, an ActiveX control
/// (its relationship names an ActiveX part) as a kept picture of its `controlPr` image.
fn form_controls(package: &Package, relationships: Option<&crate::opc::Relationships>, controls: &Node) -> (Vec<FormControl>, Vec<Shape>) {
    let mut kept = std::collections::BTreeMap::new();
    let (mut form, mut active_x) = (Vec::new(), Vec::new());
    for item in &controls.children {
        let control = match item.local.as_str() {
            "AlternateContent" => item
                .children
                .iter()
                .filter(|b| b.local == "Choice" || b.local == "Fallback")
                .flat_map(|b| b.children.iter())
                .find(|o| o.local == "control" && o.child("controlPr").is_some()),
            "control" => Some(item),
            _ => None,
        };
        let Some(control) = control else { continue };
        let Some(properties) = control.child("controlPr") else { continue };
        let Some(anchor) = properties.child("anchor") else { continue };
        let Some(id) = control.number("shapeId").and_then(|v| u32::try_from(v).ok()) else { continue };
        let position = ChartAnchor::TwoCell {
            from: anchor.child("from").map(point).unwrap_or_default(),
            to: anchor.child("to").map(point).unwrap_or_default(),
        };
        let name = control.attribute("name").unwrap_or_default().to_owned();
        let relationship = relationships.and_then(|r| r.items().iter().find(|i| Some(i.id.as_str()) == control.attribute("r:id")));
        if relationship.is_some_and(|r| r.kind.ends_with("/control")) {
            let mut shape = Shape::new(id, name, ShapeKind::Picture, position);
            shape.kept = Some("ActiveX control".into());
            shape.picture = properties.attribute("r:id").map(str::to_owned);
            shape.media = Some(ShapePicture::default());
            resolve_media(package, relationships, &mut shape, &mut kept);
            active_x.push(shape);
            continue;
        }
        let Some(part) = relationship.filter(|r| r.kind.ends_with("/ctrlProp")).and_then(|r| r.resolved.clone()) else { continue };
        let Some(pr) = part_tree(package, &part) else { continue };
        let number = |name: &str| pr.number(name);
        form.push(FormControl {
            id,
            name,
            anchor: position,
            kind: pr.attribute("objectType").unwrap_or("Button").to_owned(),
            caption: None,
            checked: pr.attribute("checked").map(str::to_owned),
            link: pr.attribute("fmlaLink").map(str::to_owned),
            range: pr.attribute("fmlaRange").map(str::to_owned),
            value: number("val"),
            min: number("min"),
            max: number("max"),
            increment: number("inc"),
            page: number("page"),
            selected: number("sel"),
            drop_lines: number("dropLines"),
            horizontal: pr.flag("horiz"),
            flat: pr.flag("noThreeD"),
            macro_name: properties.attribute("macro").map(str::to_owned),
            prints: !matches!(properties.attribute("print"), Some("0" | "false")),
        });
    }
    (form, active_x)
}

/// The anchored `oleObject`s of a worksheet's `oleObjects` (dc-zjsg.9), each as a picture of the image Excel stores
/// for it (its `objectPr r:id`, a relationship of the sheet), kept as stored.
fn ole_objects(package: &Package, relationships: Option<&crate::opc::Relationships>, objects: &Node) -> Vec<Shape> {
    let mut kept = std::collections::BTreeMap::new();
    let mut out = Vec::new();
    for item in &objects.children {
        let object = match item.local.as_str() {
            "AlternateContent" => item
                .children
                .iter()
                .filter(|b| b.local == "Choice" || b.local == "Fallback")
                .flat_map(|b| b.children.iter())
                .find(|o| o.local == "oleObject" && o.child("objectPr").is_some()),
            "oleObject" => Some(item),
            _ => None,
        };
        let Some(object) = object else { continue };
        let Some(properties) = object.child("objectPr") else { continue };
        let Some(anchor) = properties.child("anchor") else { continue };
        let Some(id) = object.number("shapeId").and_then(|v| u32::try_from(v).ok()) else { continue };
        let position = ChartAnchor::TwoCell {
            from: anchor.child("from").map(point).unwrap_or_default(),
            to: anchor.child("to").map(point).unwrap_or_default(),
        };
        let mut shape = Shape::new(id, format!("Object {}", id.saturating_sub(1024)), ShapeKind::Picture, position);
        shape.kept = Some("OLE object".into());
        shape.picture = properties.attribute("r:id").map(str::to_owned);
        shape.media = Some(ShapePicture { description: object.attribute("progId").map(str::to_owned), ..Default::default() });
        resolve_media(package, relationships, &mut shape, &mut kept);
        out.push(shape);
    }
    out
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
                        // An equation or ink is drawn from the Fallback Excel writes beside it, its picture; an
                        // equation keeps its OMML too (dc-zjsg.9).
                        let fallback = block.child("Fallback").filter(|f| f.children.iter().any(is_anchor));
                        let (taken, kept) = match (kept_kind(branch), fallback) {
                            (Some(kind), Some(fallback)) => (fallback, Some(kind)),
                            _ => (branch, None),
                        };
                        let maths: Vec<Option<DrawingNode>> =
                            branch.children.iter().filter(|c| is_anchor(c)).map(|a| find_math(a).map(drawing_node)).collect();
                        for (index, anchor) in taken.children.iter().filter(|c| is_anchor(c)).enumerate() {
                            match anchor_shape(anchor) {
                                Anchored::Shape(mut shape) => {
                                    if let Some(kind) = kept {
                                        shape.kept = Some(kind.to_owned());
                                        if kind == "equation" {
                                            shape.math = maths.get(index).cloned().flatten();
                                        }
                                    }
                                    read.shapes.push(shape)
                                }
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

/// The largest media part whose bytes the model keeps.
const MEDIA_LIMIT: u64 = 32 * 1024 * 1024;

/// Fills in the media of a picture (and of a group's pictures) from the parts the drawing's relationships name.
fn resolve_media(
    package: &Package,
    relationships: Option<&crate::opc::Relationships>,
    shape: &mut Shape,
    kept: &mut std::collections::BTreeMap<String, PictureMedia>,
) {
    for child in &mut shape.children {
        resolve_media(package, relationships, child, kept);
    }
    let Some(media) = shape.media.as_mut() else {
        return;
    };
    let mut find = |id: &Option<String>| -> Option<PictureMedia> {
        let target = relationships?
            .items()
            .iter()
            .find(|r| Some(r.id.as_str()) == id.as_deref() && r.mode == crate::opc::TargetMode::Internal)?
            .resolved
            .clone()?;
        if let Some(found) = kept.get(&target) {
            return Some(found.clone());
        }
        let part = package.part(&target)?;
        let size = part.size();
        let bytes = if size <= MEDIA_LIMIT { package.read_part(&target).ok()? } else { Vec::new() };
        let content_type = package.content_types().of(&target).unwrap_or("application/octet-stream").to_owned();
        let found = PictureMedia { part: target.clone(), content_type, size, bytes: bytes.into() };
        kept.insert(target, found.clone());
        Some(found)
    };
    media.image = find(&shape.picture);
    media.svg = find(&shape.picture_svg);
}

/// What a block of alternatives holds that DNA Calc draws from its Fallback: `ink` for a content part, `equation`
/// for a shape whose text holds OMML.
fn kept_kind(branch: &Node) -> Option<&'static str> {
    if descendant(branch, "contentPart").is_some() {
        Some("ink")
    } else if find_math(branch).is_some() {
        Some("equation")
    } else {
        None
    }
}

/// The OMML of an equation (`a14:m`'s first child) under `node`.
fn find_math(node: &Node) -> Option<&Node> {
    node.children.iter().find_map(|c| {
        if c.local == "m" && c.children.iter().any(|m| matches!(m.local.as_str(), "oMathPara" | "oMath")) {
            c.children.iter().find(|m| matches!(m.local.as_str(), "oMathPara" | "oMath"))
        } else {
            find_math(c)
        }
    })
}

fn drawing_node(node: &Node) -> DrawingNode {
    DrawingNode {
        local: node.local.clone(),
        attributes: node.attributes.clone(),
        text: node.text.clone(),
        children: node.children.iter().map(drawing_node).collect(),
    }
}

enum Anchored {
    Shape(Shape),
    Chart,
    Unread(String),
}

pub(super) fn point(node: &Node) -> DrawingPoint {
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
    let mut kept = None;
    let object = match alternate_branch(object) {
        // Ink inside an anchor: its Fallback picture (dc-zjsg.9).
        Some(chosen) if chosen.local == "contentPart" => match object.child("Fallback").and_then(|f| f.children.first()) {
            Some(fallback) => {
                kept = Some("ink");
                fallback
            }
            None => chosen,
        },
        Some(chosen) => chosen,
        None => object,
    };
    if object.local == "graphicFrame" && is_chart_frame(object) {
        return Anchored::Chart;
    }
    // A SmartArt frame: a group whose children are read from the drawing Excel caches for it (`read_shapes`).
    if object.local == "graphicFrame"
        && let Some(ids) = object.child("graphic").and_then(|g| g.child("graphicData")).filter(|d| d.attribute("uri") == Some(DIAGRAM)).and_then(|d| d.child("relIds"))
        && let Some(identity) = object.child("nvGraphicFramePr").and_then(|n| n.child("cNvPr"))
        && let Some(id) = identity.number("id").and_then(|v| u32::try_from(v).ok())
    {
        let mut shape = Shape::new(id, identity.attribute("name").unwrap_or_default().to_owned(), ShapeKind::Group, position);
        shape.hidden = identity.flag("hidden");
        if let Some(ext) = object.child("xfrm").and_then(|x| x.child("ext")) {
            shape.extent = (ext.number("cx").unwrap_or(0), ext.number("cy").unwrap_or(0));
            shape.child_extent = shape.extent;
        }
        shape.kept = Some("SmartArt".into());
        shape.source_part = ids.attribute("r:dm").map(str::to_owned);
        shape.edit_as = anchor.attribute("editAs").map(str::to_owned);
        return Anchored::Shape(shape);
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
    shape.kept = kept.map(str::to_owned);
    shape.prints = !anchor.child("clientData").is_some_and(|c| matches!(c.attribute("fPrintsWithSheet"), Some("0" | "false")));
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
pub(super) fn alternate_content_branch(block: &Node) -> Option<&Node> {
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
        || super::chartex::is_chartex_frame(frame)
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
    shape.style = node.child("style").map(style);
    // A shape filled with a picture holds its image as a picture does (dc-zjsg.9).
    if let ShapeFill::Picture(embed) = &shape.fill
        && kind != ShapeKind::Picture
    {
        let crop = node.child(properties).and_then(|p| p.child("blipFill")).and_then(|f| f.child("srcRect"));
        shape.picture = Some(embed.clone());
        shape.media = Some(ShapePicture {
            crop: crop.map_or([0; 4], |r| ["l", "t", "r", "b"].map(|side| r.number(side).unwrap_or(0))),
            description: identity.attribute("descr").map(str::to_owned),
            ..Default::default()
        });
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
            let blip = node.child("blipFill").and_then(|f| f.child("blip"));
            shape.picture = blip.and_then(|b| b.attribute("r:embed")).map(str::to_owned);
            shape.picture_svg = blip
                .and_then(|b| b.child("extLst"))
                .and_then(|l| l.children.iter().find_map(|e| e.child("svgBlip")))
                .and_then(|b| b.attribute("r:embed"))
                .map(str::to_owned);
            let crop = node.child("blipFill").and_then(|f| f.child("srcRect"));
            shape.media = Some(ShapePicture {
                crop: crop.map_or([0; 4], |r| ["l", "t", "r", "b"].map(|side| r.number(side).unwrap_or(0))),
                description: identity.attribute("descr").map(str::to_owned),
                ..Default::default()
            });
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
        shape.body = text_body(body);
        shape.body.runs = rich_text_runs(body).unwrap_or_default();
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

/// Narrow, explicitly specified run properties; other run forms stay in the package.
fn rich_text_runs(body: &Node) -> Option<Vec<Vec<ShapeTextRun>>> {
    let mut out = Vec::new();
    for paragraph in body.children.iter().filter(|p| p.local == "p") {
      let mut runs = Vec::new();
      for child in &paragraph.children {
        match child.local.as_str() {
            "pPr" if child.children.is_empty() && child.attributes.iter().all(|(k, _)| k == "algn") => continue,
            "endParaRPr" => continue,
            "r" => {}
            _ => return None,
        }
        if child.children.iter().any(|n| !matches!(n.local.as_str(), "rPr" | "t"))
            || child.children.iter().filter(|n| n.local == "rPr").count() != 1
            || child.children.iter().filter(|n| n.local == "t").count() != 1 { return None; }
        let properties = child.child("rPr")?;
        if properties.attributes.iter().any(|(k, _)| !matches!(k.as_str(), "sz" | "b" | "i" | "u" | "lang"))
            || properties.children.iter().any(|n| !matches!(n.local.as_str(), "solidFill" | "latin"))
            || properties.children.iter().filter(|n| n.local == "solidFill").count() > 1
            || properties.children.iter().filter(|n| n.local == "latin").count() > 1 { return None; }
        let size = properties.number("sz")?;
        if !(100..=400_000).contains(&size) { return None; }
        let boolean = |name| match properties.attribute(name)? {
            "1" | "true" => Some(true), "0" | "false" => Some(false), _ => None,
        };
        let colour = match properties.child("solidFill") {
            Some(fill) => Some(colour(fill)?),
            None => None,
        };
        let face = match properties.child("latin") {
            Some(latin) if latin.children.is_empty() && latin.attributes.iter().all(|(k, _)| k == "typeface") => {
                let face = latin.attribute("typeface")?;
                if face.is_empty() || face.starts_with('+') || face.chars().any(|c| c.is_control() || matches!(c, '\u{fffe}' | '\u{ffff}')) { return None; }
                Some(face.to_owned())
            }
            Some(_) => return None,
            None => None,
        };
        let underline = match properties.attribute("u") { None | Some("none") => false, Some("sng") => true, _ => return None };
        runs.push(ShapeTextRun { text: child.child("t")?.text.clone(), face, size,
            bold: boolean("b")?, italic: boolean("i")?, underline, colour });
      }
      out.push(runs);
    }
    let mut runs = out.iter().flatten();
    let first = runs.next()?;
    runs.any(|run| !run.same_style_as(first)).then_some(out)
}

/// The properties of a text body: `a:bodyPr`, each paragraph's alignment and the first run's font.
fn text_body(body: &Node) -> ShapeBody {
    let mut read = ShapeBody::default();
    if let Some(properties) = body.child("bodyPr") {
        read.left_inset = properties.number("lIns");
        read.top_inset = properties.number("tIns");
        read.right_inset = properties.number("rIns");
        read.bottom_inset = properties.number("bIns");
        read.anchor = properties.attribute("anchor").map(str::to_owned);
        read.wrap = properties.attribute("wrap").map(str::to_owned);
    }
    let paragraphs = || body.children.iter().filter(|p| p.local == "p");
    read.alignment = paragraphs()
        .map(|p| {
            p.child("pPr")
                .and_then(|p| p.attribute("algn"))
                .map(str::to_owned)
        })
        .collect();
    let run = paragraphs()
        .flat_map(|p| p.children.iter())
        .find(|r| matches!(r.local.as_str(), "r" | "fld"))
        .and_then(|r| r.child("rPr"));
    if let Some(run) = run {
        read.size = run.number("sz");
        read.face = run.child("latin").filter(|n| n.children.is_empty() && n.attributes.iter().all(|(k, _)| k == "typeface"))
            .and_then(|n| n.attribute("typeface"))
            .filter(|face| !face.is_empty() && !face.starts_with('+') && !face.chars().any(|c| c.is_control() || matches!(c, '\u{fffe}' | '\u{ffff}')))
            .map(str::to_owned);
        read.bold = run.flag("b");
        read.italic = run.flag("i");
        read.underline = run.attribute("u") == Some("sng");
        read.colour = run.child("solidFill").and_then(colour);
        read.text_hidden = run.child("noFill").is_some();
    }
    read
}

fn colour(fill: &Node) -> Option<ShapeColour> {
    let c = fill.children.first()?;
    let base = match c.local.as_str() {
        "srgbClr" => ShapeColour::Rgb(u32::from_str_radix(c.attribute("val")?, 16).ok()?),
        "schemeClr" => ShapeColour::Scheme(c.attribute("val")?.to_owned()),
        _ => return None,
    };
    // The modifiers (`a:lumMod`, `a:tint`, `a:alpha`, ...) in file order; one without a value is kept out.
    let modifiers: Vec<(String, i64)> = c
        .children
        .iter()
        .filter_map(|m| Some((m.local.clone(), m.number("val")?)))
        .collect();
    Some(if modifiers.is_empty() { base } else { ShapeColour::Modified(Box::new(base), modifiers) })
}

/// `a:outerShdw` of an `a:effectLst`.
fn shadow(effects: &Node) -> Option<ShapeShadow> {
    let s = effects.child("outerShdw")?;
    Some(ShapeShadow {
        blur: s.number("blurRad").unwrap_or(0),
        distance: s.number("dist").unwrap_or(0),
        direction: s.number("dir").unwrap_or(0),
        scale: (s.number("sx").unwrap_or(100_000), s.number("sy").unwrap_or(100_000)),
        align: s.attribute("algn").map(str::to_owned),
        rotate_with_shape: s.attribute("rotWithShape").is_none_or(|v| matches!(v, "1" | "true")),
        colour: colour(s)?,
    })
}

/// `a:gradFill`: its stops and its linear direction or path; `None` when no stop has a colour held here.
fn gradient(fill: &Node) -> Option<ShapeGradient> {
    let mut read = ShapeGradient::default();
    for stop in fill.child("gsLst")?.children.iter().filter(|s| s.local == "gs") {
        read.stops.push((stop.number("pos")?, colour(stop)?));
    }
    if read.stops.is_empty() {
        return None;
    }
    if let Some(lin) = fill.child("lin") {
        read.angle = Some(lin.number("ang").unwrap_or(0));
        read.scaled = lin.flag("scaled");
    }
    if let Some(path) = fill.child("path") {
        read.path = Some(path.attribute("path").unwrap_or("rect").to_owned());
        read.focus = path.child("fillToRect").map(|r| {
            ["l", "t", "r", "b"].map(|side| r.number(side).unwrap_or(0))
        });
    }
    Some(read)
}

/// `xdr:style`: its line, fill and effect references and the font reference's colour.
fn style(node: &Node) -> ShapeStyle {
    let reference = |name: &str| {
        node.child(name).map_or_else(ShapeStyleRef::default, |r| ShapeStyleRef {
            index: r.number("idx").unwrap_or(0),
            colour: colour(r),
        })
    };
    ShapeStyle {
        line: reference("lnRef"),
        fill: reference("fillRef"),
        effect: reference("effectRef"),
        font: node.child("fontRef").and_then(colour),
    }
}

/// `a:custGeom`: its guides, text rectangle and paths as the file states them.
fn custom_geometry(node: &Node) -> CustomGeometry {
    let guides = |list: &str| {
        node.child(list).map_or_else(Vec::new, |l| {
            l.children
                .iter()
                .filter(|gd| gd.local == "gd")
                .filter_map(|gd| Some((gd.attribute("name")?.to_owned(), gd.attribute("fmla")?.to_owned())))
                .collect()
        })
    };
    let point = |p: &Node| -> Option<GeometryPoint> { Some((p.attribute("x")?.to_owned(), p.attribute("y")?.to_owned())) };
    let points = |c: &Node| c.children.iter().filter(|p| p.local == "pt").filter_map(point).collect::<Vec<_>>();
    let paths = node.child("pathLst").map_or_else(Vec::new, |list| {
        list.children
            .iter()
            .filter(|p| p.local == "path")
            .map(|p| GeometryPath {
                width: p.number("w"),
                height: p.number("h"),
                fill: p.attribute("fill").map(str::to_owned),
                stroke: p.attribute("stroke").is_none_or(|v| matches!(v, "1" | "true")),
                commands: p
                    .children
                    .iter()
                    .filter_map(|c| {
                        let pts = points(c);
                        Some(match c.local.as_str() {
                            "moveTo" => GeometryCommand::Move(pts.into_iter().next()?),
                            "lnTo" => GeometryCommand::Line(pts.into_iter().next()?),
                            "arcTo" => GeometryCommand::Arc(
                                ["wR", "hR", "stAng", "swAng"].map(|a| c.attribute(a).unwrap_or("0").to_owned()),
                            ),
                            "quadBezTo" => GeometryCommand::Quad(<[GeometryPoint; 2]>::try_from(pts).ok()?),
                            "cubicBezTo" => GeometryCommand::Cubic(<[GeometryPoint; 3]>::try_from(pts).ok()?),
                            "close" => GeometryCommand::Close,
                            _ => return None,
                        })
                    })
                    .collect(),
            })
            .collect()
    });
    CustomGeometry {
        adjust: guides("avLst"),
        guides: guides("gdLst"),
        text_rect: node.child("rect").map(|r| ["l", "t", "r", "b"].map(|a| r.attribute(a).unwrap_or("0").to_owned())),
        paths,
        sites: node.child("cxnLst").map_or_else(Vec::new, |list| {
            list.children
                .iter()
                .filter_map(|c| c.child("pos"))
                .filter_map(|p| Some((p.attribute("x")?.to_owned(), p.attribute("y")?.to_owned())))
                .collect()
        }),
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
        if let Some(off) = xfrm.child("chOff") {
            shape.child_offset = (off.number("x").unwrap_or(0), off.number("y").unwrap_or(0));
        }
        if let Some(ext) = xfrm.child("chExt") {
            shape.child_extent = (ext.number("cx").unwrap_or(0), ext.number("cy").unwrap_or(0));
        }
    }
    let preset = properties.child("prstGeom");
    shape.geometry = preset.and_then(|g| g.attribute("prst")).map(str::to_owned);
    if let Some(list) = preset.and_then(|g| g.child("avLst")) {
        shape.adjust = list
            .children
            .iter()
            .filter(|gd| gd.local == "gd")
            .filter_map(|gd| {
                let value = gd.attribute("fmla")?.trim().strip_prefix("val ")?;
                Some((gd.attribute("name")?.to_owned(), value.trim().parse().ok()?))
            })
            .collect();
    }
    for child in &properties.children {
        match child.local.as_str() {
            "noFill" => shape.fill = ShapeFill::None,
            "solidFill" => {
                shape.fill = colour(child).map_or(ShapeFill::Other, ShapeFill::Solid);
            }
            "gradFill" => shape.fill = gradient(child).map_or(ShapeFill::Other, ShapeFill::Gradient),
            "effectLst" => shape.shadow = shadow(child),
            "pattFill" => {
                shape.fill = ShapeFill::Pattern(ShapePattern {
                    preset: child.attribute("prst").unwrap_or("pct5").to_owned(),
                    foreground: child.child("fgClr").and_then(colour),
                    background: child.child("bgClr").and_then(colour),
                })
            }
            "blipFill" => {
                shape.fill = child
                    .child("blip")
                    .and_then(|b| b.attribute("r:embed"))
                    .map_or(ShapeFill::Other, |embed| ShapeFill::Picture(embed.to_owned()))
            }
            "grpFill" => shape.fill = ShapeFill::Other,
            "custGeom" => shape.custom_geometry = Some(custom_geometry(child)),
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
                        "headEnd" | "tailEnd" => {
                            let end = LineEnd {
                                kind: part.attribute("type").unwrap_or("none").to_owned(),
                                width: part.attribute("w").map(str::to_owned),
                                length: part.attribute("len").map(str::to_owned),
                            };
                            if part.local == "headEnd" {
                                line.head = Some(end);
                            } else {
                                line.tail = Some(end);
                            }
                        }
                        _ => {}
                    }
                }
                shape.line = line;
            }
            _ => {}
        }
    }
}
