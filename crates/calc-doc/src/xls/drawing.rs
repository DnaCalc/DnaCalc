//! Drawings of a BIFF8 worksheet: the OfficeArt shapes of its MSODRAWING records (MS-ODRAWING),
//! the OBJ records that say what each shape is (MS-XLS 2.4.178), the pictures in the workbook's
//! MSODRAWINGGROUP blip store, text boxes (TXO) and the chart substreams, written as the DrawingML
//! drawing part, its images and its chart parts. A shape this reader does not read (a connector,
//! a form control, an ActiveX object) is counted, never fatal.

use super::features::Rel;
use super::records::Record;
use super::sheet::column_name;
use crate::xlsb::Xml;
use std::collections::BTreeMap;

pub(crate) const MSODRAWING: u16 = 0x00EC;
pub(crate) const MSODRAWING_GROUP: u16 = 0x00EB;
const OBJ: u16 = 0x005D;
const TXO: u16 = 0x01B6;

const DRAWING_NAMESPACE: &str = "http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing";
const MAIN_NAMESPACE: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const RELATIONSHIPS_NAMESPACE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const CHART_NAMESPACE: &str = "http://schemas.openxmlformats.org/drawingml/2006/chart";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

// ---- OfficeArt records ----

struct Atom<'a> {
    instance: u16,
    kind: u16,
    container: bool,
    body: &'a [u8],
}

/// The OfficeArt records in `data` (a record header: version and instance, type, length).
fn atoms(data: &[u8]) -> Vec<Atom<'_>> {
    let mut out = Vec::new();
    let mut at = 0usize;
    while at + 8 <= data.len() {
        let word = u16::from_le_bytes([data[at], data[at + 1]]);
        let kind = u16::from_le_bytes([data[at + 2], data[at + 3]]);
        let length = u32::from_le_bytes([data[at + 4], data[at + 5], data[at + 6], data[at + 7]]) as usize;
        let end = (at + 8).saturating_add(length).min(data.len());
        out.push(Atom {
            instance: word >> 4,
            kind,
            container: word & 0xF == 0xF,
            body: &data[at + 8..end],
        });
        at = end;
    }
    out
}

/// An image of the blip store.
#[derive(Clone, Debug)]
pub(crate) struct Image {
    pub(crate) extension: &'static str,
    pub(crate) content_type: &'static str,
    pub(crate) bytes: Vec<u8>,
}

/// The pictures of the MSODRAWINGGROUP record (the blip store of the drawing group container), in
/// store order (a picture's `pib` property is a one-based index into it).
pub(crate) fn blip_store(group: &[u8]) -> Vec<Option<Image>> {
    fn find_store<'a>(data: &'a [u8]) -> Option<&'a [u8]> {
        for atom in atoms(data) {
            if atom.kind == 0xF001 {
                return Some(atom.body);
            }
            if atom.container
                && let Some(found) = find_store(atom.body)
            {
                return Some(found);
            }
        }
        None
    }
    let Some(store) = find_store(group) else {
        return Vec::new();
    };
    atoms(store)
        .into_iter()
        .map(|bse| {
            if bse.kind != 0xF007 || bse.body.len() < 36 {
                return None;
            }
            let name = usize::from(bse.body[33]);
            let blip = bse.body.get(36 + name..)?;
            let atom = atoms(blip).into_iter().next()?;
            blip_image(&atom)
        })
        .collect()
}

fn blip_image(atom: &Atom<'_>) -> Option<Image> {
    let body = atom.body;
    // One 16-byte UID, two when the instance is the "with second UID" form.
    let (uids, extension, content_type): (&[u16], &'static str, &'static str) = match atom.kind {
        0xF01A => (&[0x3D5], "emf", "image/x-emf"),
        0xF01B => (&[0x217], "wmf", "image/x-wmf"),
        0xF01C => (&[0x543], "pict", "image/x-pict"),
        0xF01D => (&[0x46B, 0x6E3], "jpeg", "image/jpeg"),
        0xF01E => (&[0x6E1], "png", "image/png"),
        0xF01F => (&[0x7A9], "bmp", "image/bmp"),
        0xF029 => (&[0x6E5], "tiff", "image/tiff"),
        _ => return None,
    };
    let mut at = if uids.contains(&atom.instance) { 32 } else { 16 };
    match atom.kind {
        0xF01A..=0xF01C => {
            // Metafile header: size, bounds, size in EMU, saved size, compression, filter.
            let header = body.get(at..at + 34)?;
            let saved = u32::from_le_bytes([header[28], header[29], header[30], header[31]]) as usize;
            let compression = header[32];
            at += 34;
            let data = body.get(at..)?;
            let data = &data[..saved.min(data.len())];
            let bytes = if compression == 0 {
                miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(data, 64 << 20).ok()?
            } else {
                data.to_vec()
            };
            Some(Image {
                extension,
                content_type,
                bytes,
            })
        }
        0xF01F => {
            // A DIB without its file header: add one.
            let dib = body.get(at + 1..)?;
            let size = u32::from_le_bytes(dib.get(..4)?.try_into().ok()?) as usize;
            let bits = u16::from_le_bytes(dib.get(14..16)?.try_into().ok()?);
            let compression = u32::from_le_bytes(dib.get(16..20)?.try_into().ok()?);
            let used = u32::from_le_bytes(dib.get(32..36)?.try_into().ok()?) as usize;
            let palette = if used != 0 {
                used
            } else if bits <= 8 {
                1usize << bits
            } else {
                0
            };
            let offset = 14 + size + palette * 4 + if compression == 3 { 12 } else { 0 };
            let mut bytes = b"BM".to_vec();
            bytes.extend(((14 + dib.len()) as u32).to_le_bytes());
            bytes.extend([0u8; 4]);
            bytes.extend((offset as u32).to_le_bytes());
            bytes.extend_from_slice(dib);
            Some(Image {
                extension,
                content_type,
                bytes,
            })
        }
        _ => Some(Image {
            extension,
            content_type,
            bytes: body.get(at + 1..)?.to_vec(),
        }),
    }
}

// ---- Shapes ----

#[derive(Clone, Debug, Default)]
struct Shape {
    shape_type: u16,
    group: bool,
    /// The one-based blip store index of a picture.
    picture: Option<u32>,
    /// col, col offset (1/1024 of the column), row, row offset (1/256 of the row), to col, ...
    anchor: Option<[u16; 8]>,
    client_data: bool,
    text_box: bool,
}

fn shapes_of(data: &[u8], out: &mut Vec<Shape>) {
    for atom in atoms(data) {
        if atom.kind == 0xF004 {
            let mut shape = Shape::default();
            for child in atoms(atom.body) {
                match child.kind {
                    0xF00A => {
                        shape.shape_type = child.instance;
                        if child.body.len() >= 8 {
                            let flags = u32::from_le_bytes(child.body[4..8].try_into().expect("4 bytes"));
                            shape.group = flags & 0x1 != 0;
                        }
                    }
                    0xF00B => {
                        // Properties: id (14 bits), blip flag, complex flag, then a 32-bit value.
                        for k in 0..usize::from(child.instance) {
                            let at = k * 6;
                            let Some(p) = child.body.get(at..at + 6) else { break };
                            let id = u16::from_le_bytes([p[0], p[1]]);
                            let value = u32::from_le_bytes([p[2], p[3], p[4], p[5]]);
                            if id & 0x3FFF == 0x0104 {
                                shape.picture = Some(value);
                            }
                        }
                    }
                    0xF010 if child.body.len() >= 18 => {
                        let mut a = [0u16; 8];
                        for (k, item) in a.iter_mut().enumerate() {
                            *item = u16::from_le_bytes([child.body[2 + 2 * k], child.body[3 + 2 * k]]);
                        }
                        shape.anchor = Some(a);
                    }
                    0xF011 => shape.client_data = true,
                    0xF00D => shape.text_box = true,
                    _ => {}
                }
            }
            out.push(shape);
        } else if atom.container {
            shapes_of(atom.body, out);
        }
    }
}

// ---- Objects and what they own ----

/// One OBJ record's drawing object.
#[derive(Debug, Default)]
struct Object {
    kind: u16,
    shape: Option<Shape>,
    text: Option<Vec<u16>>,
    chart: Option<super::chart::Chart>,
}

#[derive(Default)]
pub(crate) struct Drawing {
    shapes: Vec<Shape>,
    next_shape: usize,
    objects: Vec<Object>,
    /// An object waiting for its TXO text record.
    waiting_text: Option<usize>,
    /// The chart object whose substream is being read.
    chart_object: Option<usize>,
}

impl Drawing {
    pub(crate) fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }

    /// Takes MSODRAWING, OBJ and TXO records; `false` for any other record.
    pub(crate) fn read(&mut self, record: &Record<'_>) -> bool {
        match record.id {
            MSODRAWING => {
                shapes_of(&record.joined(), &mut self.shapes);
                true
            }
            OBJ => {
                let body = record.body;
                let mut at = 0usize;
                let mut kind = None;
                while at + 4 <= body.len() {
                    let ft = u16::from_le_bytes([body[at], body[at + 1]]);
                    let cb = usize::from(u16::from_le_bytes([body[at + 2], body[at + 3]]));
                    if ft == 0 {
                        break;
                    }
                    if ft == 0x15 && body.len() >= at + 8 {
                        kind = Some(u16::from_le_bytes([body[at + 4], body[at + 5]]));
                    }
                    at += 4 + cb;
                }
                if let Some(kind) = kind {
                    // The shape the MSODRAWING records gave for this object: the next one with
                    // client data, in order.
                    let mut shape = None;
                    while self.next_shape < self.shapes.len() {
                        let candidate = self.shapes[self.next_shape].clone();
                        self.next_shape += 1;
                        if candidate.client_data && !candidate.group {
                            shape = Some(candidate);
                            break;
                        }
                    }
                    let text_box = shape.as_ref().is_some_and(|s| s.text_box);
                    self.objects.push(Object {
                        kind,
                        shape,
                        ..Default::default()
                    });
                    self.waiting_text = (text_box && kind != 0x19).then_some(self.objects.len() - 1);
                    if kind == 0x05 {
                        self.chart_object = Some(self.objects.len() - 1);
                    }
                }
                false
            }
            TXO => match self.waiting_text.take() {
                Some(index) => {
                    if let Ok(Some(units)) = super::strings::txo_text(record) {
                        self.objects[index].text = Some(units);
                    }
                    true
                }
                None => false,
            },
            _ => false,
        }
    }

    /// A record of a chart substream, for the chart object it belongs to.
    pub(crate) fn chart_record(&mut self, record: &Record<'_>, print: &dyn Fn(&[u8]) -> Option<String>,
        styles: &super::styles::Styles) {
        if let Some(index) = self.chart_object {
            let chart = self.objects[index].chart.get_or_insert_with(Default::default);
            chart.read(record, print, None, styles, true);
        }
    }

    /// The drawing part, with the relationships, charts and images it needs, as parts of the
    /// package; `None` when the sheet has nothing to draw. `anchor_emu` turns an anchor into
    /// EMU offsets.
    pub(crate) fn write(
        &self,
        number: usize,
        images: &[Option<Image>],
        geometry: &dyn Fn(u16, u16, u16, u16) -> (u16, i64, u16, i64),
        links_sheet: &str,
    ) -> Option<DrawingParts> {
        let mut x = Xml::new();
        x.open(
            "xdr:wsDr",
            &[
                ("xmlns:xdr", DRAWING_NAMESPACE.to_owned()),
                ("xmlns:a", MAIN_NAMESPACE.to_owned()),
                ("xmlns:r", RELATIONSHIPS_NAMESPACE.to_owned()),
                ("xmlns:c", CHART_NAMESPACE.to_owned()),
            ],
        );
        let mut parts = DrawingParts::default();
        let mut rels: Vec<Rel> = Vec::new();
        let mut written = 0u32;
        let mut unread: BTreeMap<u16, u32> = BTreeMap::new();
        for object in &self.objects {
            let Some(shape) = &object.shape else {
                if object.kind != 0x19 {
                    *unread.entry(object.kind).or_insert(0) += 1;
                }
                continue;
            };
            let Some(a) = shape.anchor else {
                if object.kind != 0x19 {
                    *unread.entry(object.kind).or_insert(0) += 1;
                }
                continue;
            };
            let name_id = written + 2;
            let anchor = |x: &mut Xml| {
                let (c1, d1, r1, e1) = geometry(a[0], a[1], a[2], a[3]);
                let (c2, d2, r2, e2) = geometry(a[4], a[5], a[6], a[7]);
                x.open("xdr:from", &[]);
                x.element("xdr:col", &[], &c1.to_string());
                x.element("xdr:colOff", &[], &d1.to_string());
                x.element("xdr:row", &[], &r1.to_string());
                x.element("xdr:rowOff", &[], &e1.to_string());
                x.close("xdr:from");
                x.open("xdr:to", &[]);
                x.element("xdr:col", &[], &c2.to_string());
                x.element("xdr:colOff", &[], &d2.to_string());
                x.element("xdr:row", &[], &r2.to_string());
                x.element("xdr:rowOff", &[], &e2.to_string());
                x.close("xdr:to");
            };
            let xfrm = |x: &mut Xml| {
                x.open("a:xfrm", &[]);
                x.empty("a:off", &[("x", "0".to_owned()), ("y", "0".to_owned())]);
                x.empty("a:ext", &[("cx", "0".to_owned()), ("cy", "0".to_owned())]);
                x.close("a:xfrm");
            };
            match object.kind {
                // A picture, or an OfficeArt object that is one.
                0x08 | 0x1E if shape.picture.is_some() => {
                    let index = shape.picture.unwrap_or(0) as usize;
                    let Some(Some(image)) = index.checked_sub(1).and_then(|k| images.get(k)) else {
                        *unread.entry(object.kind).or_insert(0) += 1;
                        continue;
                    };
                    parts.images.push(image.clone());
                    let id = format!("rId{}", rels.len() + 1);
                    rels.push(Rel {
                        id: id.clone(),
                        kind: "image",
                        target: format!("../media/image{}_{}.{}", number, parts.images.len(), image.extension),
                        external: false,
                    });
                    x.open("xdr:twoCellAnchor", &[("editAs", "oneCell".to_owned())]);
                    anchor(&mut x);
                    x.open("xdr:pic", &[]);
                    x.open("xdr:nvPicPr", &[]);
                    x.empty(
                        "xdr:cNvPr",
                        &[("id", name_id.to_string()), ("name", format!("Picture {}", written + 1))],
                    );
                    x.empty("xdr:cNvPicPr", &[]);
                    x.close("xdr:nvPicPr");
                    x.open("xdr:blipFill", &[]);
                    x.empty("a:blip", &[("r:embed", id)]);
                    x.open("a:stretch", &[]);
                    x.empty("a:fillRect", &[]);
                    x.close("a:stretch");
                    x.close("xdr:blipFill");
                    x.open("xdr:spPr", &[]);
                    xfrm(&mut x);
                    x.open("a:prstGeom", &[("prst", "rect".to_owned())]);
                    x.empty("a:avLst", &[]);
                    x.close("a:prstGeom");
                    x.close("xdr:spPr");
                    x.close("xdr:pic");
                    x.empty("xdr:clientData", &[]);
                    x.close("xdr:twoCellAnchor");
                }
                0x05 => {
                    let Some(chart) = &object.chart else {
                        *unread.entry(object.kind).or_insert(0) += 1;
                        continue;
                    };
                    let Some(chart_xml) = chart.write(links_sheet) else {
                        *unread.entry(object.kind).or_insert(0) += 1;
                        continue;
                    };
                    parts.charts.push(chart_xml);
                    let id = format!("rId{}", rels.len() + 1);
                    rels.push(Rel {
                        id: id.clone(),
                        kind: "chart",
                        target: format!("../charts/chart{}_{}.xml", number, parts.charts.len()),
                        external: false,
                    });
                    x.open("xdr:twoCellAnchor", &[]);
                    anchor(&mut x);
                    x.open("xdr:graphicFrame", &[("macro", String::new())]);
                    x.open("xdr:nvGraphicFramePr", &[]);
                    x.empty(
                        "xdr:cNvPr",
                        &[("id", name_id.to_string()), ("name", format!("Chart {}", written + 1))],
                    );
                    x.empty("xdr:cNvGraphicFramePr", &[]);
                    x.close("xdr:nvGraphicFramePr");
                    x.open("xdr:xfrm", &[]);
                    x.empty("a:off", &[("x", "0".to_owned()), ("y", "0".to_owned())]);
                    x.empty("a:ext", &[("cx", "0".to_owned()), ("cy", "0".to_owned())]);
                    x.close("xdr:xfrm");
                    x.open("a:graphic", &[]);
                    x.open("a:graphicData", &[("uri", CHART_NAMESPACE.to_owned())]);
                    x.empty("c:chart", &[("r:id", id)]);
                    x.close("a:graphicData");
                    x.close("a:graphic");
                    x.close("xdr:graphicFrame");
                    x.empty("xdr:clientData", &[]);
                    x.close("xdr:twoCellAnchor");
                }
                // A text box, a rectangle or an oval (with or without text).
                0x02 | 0x03 | 0x06 | 0x1E => {
                    let geometry_name = match shape.shape_type {
                        3 => "ellipse",
                        1 | 202 => "rect",
                        _ => {
                            *unread.entry(object.kind).or_insert(0) += 1;
                            continue;
                        }
                    };
                    x.open("xdr:twoCellAnchor", &[]);
                    anchor(&mut x);
                    x.open("xdr:sp", &[("macro", String::new()), ("textlink", String::new())]);
                    x.open("xdr:nvSpPr", &[]);
                    x.empty(
                        "xdr:cNvPr",
                        &[("id", name_id.to_string()), ("name", format!("Shape {}", written + 1))],
                    );
                    if object.text.is_some() {
                        x.empty("xdr:cNvSpPr", &[("txBox", "1".to_owned())]);
                    } else {
                        x.empty("xdr:cNvSpPr", &[]);
                    }
                    x.close("xdr:nvSpPr");
                    x.open("xdr:spPr", &[]);
                    xfrm(&mut x);
                    x.open("a:prstGeom", &[("prst", geometry_name.to_owned())]);
                    x.empty("a:avLst", &[]);
                    x.close("a:prstGeom");
                    x.close("xdr:spPr");
                    if let Some(text) = &object.text {
                        x.open("xdr:txBody", &[]);
                        x.empty("a:bodyPr", &[]);
                        x.empty("a:lstStyle", &[]);
                        for line in text.split(|u| *u == 0x0D || *u == 0x0A) {
                            x.open("a:p", &[]);
                            if !line.is_empty() {
                                x.open("a:r", &[]);
                                x.open("a:t", &[]);
                                x.text(line);
                                x.close("a:t");
                                x.close("a:r");
                            }
                            x.close("a:p");
                        }
                        x.close("xdr:txBody");
                    }
                    x.close("xdr:sp");
                    x.empty("xdr:clientData", &[]);
                    x.close("xdr:twoCellAnchor");
                }
                other => {
                    *unread.entry(other).or_insert(0) += 1;
                    continue;
                }
            }
            written += 1;
        }
        x.close("xdr:wsDr");
        parts.unread = unread;
        if written == 0 {
            return if parts.unread.is_empty() { None } else { Some(parts) };
        }
        parts.xml = x.out;
        parts.rels = rels;
        Some(parts)
    }
}

/// What a drawing writes.
#[derive(Default)]
pub(crate) struct DrawingParts {
    pub(crate) xml: Vec<u8>,
    pub(crate) rels: Vec<Rel>,
    pub(crate) images: Vec<Image>,
    pub(crate) charts: Vec<Vec<u8>>,
    pub(crate) unread: BTreeMap<u16, u32>,
}

/// The column letters used by anchors in messages.
#[allow(dead_code)]
fn cell_name(row: u16, column: u16) -> String {
    format!("{}{}", column_name(u32::from(column)), u32::from(row) + 1)
}

#[allow(dead_code)]
const _RELATIONSHIPS: &str = REL;
