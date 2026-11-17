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
    complete: bool,
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
            complete: length <= data.len() - at - 8,
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

/// Document drawing defaults can supply properties omitted from an individual shape
/// (MS-ODRAW 2.2.12). Decline the literal-solid subset when those defaults change its meaning.
pub(crate) fn solid_fill_defaults_safe(data: &[u8]) -> bool {
    let records = atoms(data);
    if records.iter().map(|atom| atom.body.len() + 8).sum::<usize>() != data.len() { return false; }
    for atom in records {
        if !atom.complete { return false; }
        if matches!(atom.kind, 0xF00B | 0xF121 | 0xF122) {
            if usize::from(atom.instance) * 6 > atom.body.len() { return false; }
            for p in atom.body[..usize::from(atom.instance) * 6].chunks_exact(6) {
                let id = u16::from_le_bytes([p[0], p[1]]);
                let value = u32::from_le_bytes(p[2..6].try_into().expect("four bytes"));
                if id & 0x3fff == 0x0301 && value != 0 { return false; }
                if (0x0180..=0x01bf).contains(&(id & 0x3fff)) {
                    if id & 0xc000 != 0 { return false; }
                    match id {
                        0x0180 if value == 0 => {},
                        0x0182 if value == 0x10000 => {},
                        0x0181 | 0x0183 | 0x0184 => {}, // Explicit foreground; unused background.
                        0x01bf if value & !0x0011_0011 == 0 => {},
                        _ => return false,
                    }
                }
            }
        } else if atom.container && !solid_fill_defaults_safe(atom.body) { return false; }
    }
    true
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
    fill_seen: bool,
    fill_invalid: bool,
    no_fill: bool,
    fill_properties: BTreeMap<u16, u32>,
    solid_fill_invalid: bool,
    fixed_geometry_unsafe: bool,
    flip_invalid: bool,
    text_insets: [Option<u32>; 4],
    text_insets_invalid: bool,
    text_boolean: Option<u32>,
    text_anchor_ambiguous: bool,
    text_wrap: Option<u32>,
    text_wrap_invalid: bool,
    flags: Option<u32>,
}

impl Shape {
    /// MS-ODRAW 2.3.7: an explicitly enabled, opaque solid fill with a literal RGB.
    /// Keep palette/system colours, gradients and extended colour overrides unread.
    fn solid_fill(&self) -> Option<String> {
        if self.fill_invalid || self.solid_fill_invalid
            || self.fill_properties.get(&0x0180).copied().unwrap_or(0) != 0
            || self.fill_properties.get(&0x0182).copied().unwrap_or(0x10000) != 0x10000
            || self.fill_properties.get(&0x01bf).copied()? & 0x0010_0010 != 0x0010_0010
        { return None; }
        let rgb = *self.fill_properties.get(&0x0181)?;
        if rgb >> 24 != 0 { return None; }
        Some(format!("{:02X}{:02X}{:02X}", rgb & 255, (rgb >> 8) & 255, (rgb >> 16) & 255))
    }
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
                            if shape.flags.replace(flags).is_some() || child.body.len() != 8 {
                                shape.solid_fill_invalid = true;
                                shape.fixed_geometry_unsafe = true;
                                shape.flip_invalid = true;
                                shape.text_insets_invalid = true;
                                shape.text_anchor_ambiguous = true;
                            }
                        } else {
                            shape.solid_fill_invalid = true;
                            shape.fixed_geometry_unsafe = true;
                            shape.flip_invalid = true;
                            shape.text_insets_invalid = true;
                            shape.text_anchor_ambiguous = true;
                        }
                    }
                    0xF00B => {
                        // Properties: id (14 bits), blip flag, complex flag, then a 32-bit value.
                        if usize::from(child.instance) * 6 > child.body.len() {
                            shape.fill_invalid = true;
                            shape.fixed_geometry_unsafe = true;
                            shape.flip_invalid = true;
                            shape.text_insets_invalid = true;
                            shape.text_anchor_ambiguous = true;
                        }
                        for k in 0..usize::from(child.instance) {
                            let at = k * 6;
                            let Some(p) = child.body.get(at..at + 6) else { break };
                            let id = u16::from_le_bytes([p[0], p[1]]);
                            let value = u32::from_le_bytes([p[2], p[3], p[4], p[5]]);
                            let property = id & 0x3fff;
                            if property == 0x0301 && value != 0 { shape.solid_fill_invalid = true; }
                            if (0x0180..=0x01bf).contains(&property) {
                                if id & 0xc000 != 0
                                    || !matches!(property, 0x0180..=0x0184 | 0x01bf)
                                    || shape.fill_properties.insert(property, value).is_some() {
                                    shape.solid_fill_invalid = true;
                                }
                            }
                            if property == 0x0087 { shape.text_anchor_ambiguous = true; }
                            if property == 0x0085 && (id & 0xc000 != 0 || value > 2 || shape.text_wrap.replace(value).is_some()) {
                                shape.text_wrap_invalid = true;
                            }
                            if (0x0081..=0x0084).contains(&property) {
                                let slot = usize::from(property - 0x0081);
                                if id & 0xc000 != 0 || value > 0x0132_f540 || shape.text_insets[slot].replace(value).is_some() {
                                    shape.text_insets_invalid = true;
                                }
                            }
                            if property == 0x00bf && (id & 0xc000 != 0 || shape.text_boolean.replace(value).is_some()) {
                                shape.text_insets_invalid = true;
                            }
                            if (property == 4 && (value != 0 || id & 0xc000 != 0)) || property == 0x0088 || property == 0x033f {
                                shape.text_insets_invalid = true;
                            }
                            // Transform or override properties and rotation require separate handling.
                            if matches!(property, 0x0004 | 0x003f | 0x033f) { shape.flip_invalid = true; }
                            // Do not infer a preset when the shape has rotation, custom geometry,
                            // or group visibility properties other than the observed default.
                            if (property == 4 && (value != 0 || id & 0xc000 != 0))
                                || (0x0140..=0x017f).contains(&property)
                                || (property == 0x03bf && (id & 0xc000 != 0 || ![0, 0x0008_0000].contains(&value)))
                            {
                                shape.fixed_geometry_unsafe = true;
                            }
                            if id & 0x3FFF == 0x0104 {
                                shape.picture = Some(value);
                            }
                            if id & 0x3FFF == 0x01BF {
                                if shape.fill_seen || id & 0xC000 != 0 { shape.fill_invalid = true; }
                                shape.fill_seen = true;
                                // fUsefFilled and fFilled: an explicit false suppresses the fill.
                                shape.no_fill = value & 0x0010_0010 == 0x0010_0000;
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
                    // Secondary/tertiary properties can override the primary fill colour.
                    0xF121 | 0xF122 => {
                        shape.solid_fill_invalid = true;
                        shape.flip_invalid = true;
                        shape.text_insets_invalid = true;
                        shape.text_anchor_ambiguous = true;
                    }
                    _ => {}
                }
            }
            out.push(shape);
        } else if atom.container {
            shapes_of(atom.body, out);
        }
    }
}

/// A complete worksheet shape placed in a CONTINUE after the preceding OBJ.
fn continued_shape(data: &[u8]) -> Option<Shape> {
    if data.len() < 8 || u16::from_le_bytes([data[0], data[1]]) & 0xf != 0xf
        || u16::from_le_bytes([data[2], data[3]]) != 0xf004
        || u32::from_le_bytes(data[4..8].try_into().ok()?) as usize != data.len() - 8 { return None; }
    let mut at = 8usize;
    let mut has_shape = false;
    let mut has_anchor = false;
    let mut has_client = false;
    while at < data.len() {
        let header = data.get(at..at.checked_add(8)?)?;
        let info = u16::from_le_bytes([header[0], header[1]]);
        let kind = u16::from_le_bytes([header[2], header[3]]);
        let length = u32::from_le_bytes(header[4..8].try_into().ok()?) as usize;
        let end = at.checked_add(8)?.checked_add(length)?;
        if end > data.len() || has_client { return None; }
        match kind {
            0xf00a if info & 0xf == 2 && length == 8 && !has_shape => has_shape = true,
            0xf00b if info & 0xf == 3 && length >= usize::from(info >> 4) * 6 => {},
            0xf010 if info == 0 && length == 18 && !has_anchor => has_anchor = true,
            0xf011 if info == 0 && length == 0 => has_client = true,
            0xf00a | 0xf00b | 0xf010 | 0xf011 => return None,
            _ => {}
        }
        at = end;
    }
    if !(has_shape && has_anchor && has_client) { return None; }
    let mut shapes = Vec::new();
    shapes_of(data, &mut shapes);
    (shapes.len() == 1).then(|| shapes.remove(0))
}

pub(super) fn is_continued_shape(data: &[u8]) -> bool {
    continued_shape(data).is_some()
}

// ---- Objects and what they own ----

/// One OBJ record's drawing object.
#[derive(Debug, Default)]
struct Object {
    kind: u16,
    shape: Option<Shape>,
    text: Option<Vec<u16>>,
    text_font: Option<u16>,
    text_runs: Option<Vec<(u16, u16)>>,
    text_alignment: Option<(&'static str, &'static str)>,
    chart: Option<super::chart::Chart>,
}

fn write_text_run(x: &mut Xml, text: &[u16], font: Option<(u32, bool, bool, Option<String>)>, face: Option<&str>, underline: Option<bool>) {
    x.open("a:r", &[]);
    if let Some((size, bold, italic, colour)) = font {
        let mut attributes = vec![("sz", size.to_string()), ("b", u8::from(bold).to_string()), ("i", u8::from(italic).to_string())];
        if let Some(underline) = underline { attributes.push(("u", if underline { "sng" } else { "none" }.to_owned())); }
        x.open("a:rPr", &attributes);
        if let Some(colour) = colour {
            x.open("a:solidFill", &[]);
            x.empty("a:srgbClr", &[("val", colour)]);
            x.close("a:solidFill");
        }
        if let Some(face) = face { x.empty("a:latin", &[("typeface", face.to_owned())]); }
        x.close("a:rPr");
    }
    x.open("a:t", &[("xml:space", "preserve".to_owned())]);
    x.text(text);
    x.close("a:t");
    x.close("a:r");
}

#[derive(Default)]
pub(crate) struct Drawing {
    shapes: Vec<Shape>,
    next_shape: usize,
    objects: Vec<Object>,
    /// An object waiting for its TXO text record.
    waiting_text: Option<usize>,
    /// End of the most recent successfully parsed OBJ, including its continuations.
    last_object_end: Option<usize>,
    /// A validated shape in the preceding OBJ's final CONTINUE, for an adjacent OBJ only.
    continued_shape: Option<(usize, Shape)>,
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
                let bytes = record.joined();
                // Excel may put the client-textbox marker after the OBJ in its own
                // MsoDrawing record; the immediately following TxO belongs to that object.
                if bytes.as_ref() == [0, 0, 0x0d, 0xf0, 0, 0, 0, 0]
                    && self.last_object_end == Some(record.offset) {
                    self.waiting_text = self.objects.len().checked_sub(1).filter(|&index| {
                        let object = &self.objects[index];
                        object.kind != 0x19 && object.shape.is_some()
                    });
                } else {
                    shapes_of(&bytes, &mut self.shapes);
                }
                true
            }
            OBJ => {
                if let Some((next, shape)) = self.continued_shape.take() && next == record.offset {
                    self.shapes.push(shape);
                }
                self.last_object_end = None;
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
                    self.last_object_end = record.offset.checked_add(4 + record.body.len())
                        .and_then(|end| record.continues.iter().try_fold(end, |at, piece| at.checked_add(4 + piece.len())));
                    if let (Some(end), Some(piece)) = (self.last_object_end, record.continues.last())
                        && let Some(shape) = continued_shape(piece) {
                        self.continued_shape = Some((end, shape));
                    }
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
            TXO => {
                self.continued_shape = None;
                if let Some(piece) = record.continues.last()
                    && let Some(shape) = continued_shape(piece)
                    && let Some(end) = record.offset.checked_add(4 + record.body.len())
                        .and_then(|end| record.continues.iter().try_fold(end, |at, piece| at.checked_add(4 + piece.len()))) {
                    self.continued_shape = Some((end, shape));
                }
                match self.waiting_text.take() {
                Some(index) => {
                    if let Ok(Some(units)) = super::strings::txo_text(record) {
                        if let Some(bytes) = record.body.get(..4) {
                            let flags = u16::from_le_bytes([bytes[0], bytes[1]]);
                            let rotation = u16::from_le_bytes([bytes[2], bytes[3]]);
                            let horizontal = match (flags >> 1) & 7 { 1 => Some("l"), 2 => Some("ctr"), 3 => Some("r"),
                                4 => Some("just"), _ => None };
                            let vertical = match (flags >> 4) & 7 { 1 => Some("t"), 2 => Some("ctr"), 3 => Some("b"), _ => None };
                            if rotation == 0 && flags & 0x4000 == 0 {
                                self.objects[index].text_alignment = horizontal.zip(vertical);
                            }
                        }
                        if let Some(runs) = super::strings::txo_runs(record, &units)
                            && let Some(&(_, font)) = runs.first() {
                            if runs.iter().all(|&(_, current)| current == font) {
                                self.objects[index].text_font = Some(font);
                            } else {
                                self.objects[index].text_runs = Some(runs);
                            }
                        }
                        self.objects[index].text = Some(units);
                    }
                    true
                }
                None => false,
                }
            }
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
        styles: &super::styles::Styles,
        solid_fill_defaults_safe: bool,
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
                let mut attrs = Vec::new();
                if let Some(flags) = shape.flags.filter(|flags| flags & 0x53f == 0 && !shape.flip_invalid) {
                    if flags & 0x40 != 0 { attrs.push(("flipH", "1".to_owned())); }
                    if flags & 0x80 != 0 { attrs.push(("flipV", "1".to_owned())); }
                }
                x.open("a:xfrm", &attrs);
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
                        // An unadjusted OfficeArt isosceles triangle has a centered apex.
                        5 if shape.flags.is_some_and(|flags| flags & 0xff == 0)
                            && !shape.fixed_geometry_unsafe => "triangle",
                        6 if shape.flags.is_some_and(|flags| flags & 0xff == 0)
                            && !shape.fixed_geometry_unsafe => "rtTriangle",
                        110 if shape.flags.is_some_and(|flags| flags & 0xff == 0)
                            && !shape.fixed_geometry_unsafe => "flowChartDecision",
                        120 if shape.flags.is_some_and(|flags| flags & 0xff == 0)
                            && !shape.fixed_geometry_unsafe => "flowChartConnector",
                        127 if shape.flags.is_some_and(|flags| flags & 0xff == 0)
                            && !shape.fixed_geometry_unsafe => "flowChartExtract",
                        // OfficeArt's alternate-process radius is 1/8 of the short side;
                        // the DrawingML preset uses 1/6, so it cannot be substituted here.
                        // MS-ODRAW msosptFlowChartProcess; preserve only the untransformed preset.
                        109 if shape.flags.is_some_and(|flags| flags & 0xff == 0)
                            && !shape.fixed_geometry_unsafe => "flowChartProcess",
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
                    if shape.no_fill && !shape.fill_invalid { x.empty("a:noFill", &[]); }
                    else if solid_fill_defaults_safe && let Some(rgb) = shape.solid_fill() {
                        x.open("a:solidFill", &[]);
                        x.empty("a:srgbClr", &[("val", rgb)]);
                        x.close("a:solidFill");
                    }
                    x.close("xdr:spPr");
                    if let Some(text) = &object.text {
                        x.open("xdr:txBody", &[]);
                        let mut body_attributes = Vec::new();
                        if let Some((_, vertical)) = object.text_alignment.filter(|_| !shape.text_anchor_ambiguous) {
                            body_attributes.push(("anchor", vertical.to_owned()));
                            let manual_margin = shape.text_boolean.is_some_and(|v| v & 0x0008_0008 == 0x0008_0000);
                            if shape.flags.is_some_and(|v| v & 0x05ff == 0) && !shape.text_insets_invalid {
                                if !shape.text_wrap_invalid && let Some(wrap) = shape.text_wrap {
                                    body_attributes.push(("wrap", if wrap == 2 { "none" } else { "square" }.to_owned()));
                                }
                                if manual_margin {
                                    for (name, value) in ["lIns", "tIns", "rIns", "bIns"].into_iter().zip(shape.text_insets) {
                                        if let Some(value) = value { body_attributes.push((name, value.to_string())); }
                                    }
                                }
                            }
                        }
                        x.empty("a:bodyPr", &body_attributes);
                        x.empty("a:lstStyle", &[]);
                        let rich = if String::from_utf16(text).is_ok() {
                            object.text_runs.as_ref().and_then(|runs| {
                                let fonts: Vec<_> = runs.iter().map(|&(_, font)| styles.drawing_rich_font(font)).collect::<Option<_>>()?;
                                let explicit = fonts.iter().all(|(_, (name, charset, family), _)|
                                    *charset == 0 && *family <= 5 && !name.starts_with('+')
                                        && !name.chars().any(|c| c.is_control() || matches!(c, '\u{fffd}' | '\u{fffe}' | '\u{ffff}')));
                                let first_face = &fonts.first()?.1;
                                (explicit || fonts.iter().all(|(_, face, _)| face == first_face)).then_some((runs, fonts, explicit))
                            })
                        } else { None };
                        let mut line_start = 0usize;
                        for line in text.split(|u| *u == 0x0D || *u == 0x0A) {
                            x.open("a:p", &[]);
                            if let Some((horizontal, _)) = object.text_alignment.filter(|_| !shape.text_anchor_ambiguous) {
                                x.empty("a:pPr", &[("algn", horizontal.to_owned())]);
                            }
                            if !line.is_empty() {
                                if let Some((runs, fonts, explicit)) = &rich {
                                    for (index, &(start, _)) in runs.iter().enumerate() {
                                        let next = runs.get(index + 1).map_or(text.len(), |(next, _)| usize::from(*next));
                                        let start = usize::from(start).max(line_start);
                                        let end = next.min(line_start + line.len());
                                        if start < end { write_text_run(&mut x, &text[start..end], Some(fonts[index].0.clone()), explicit.then_some(fonts[index].1.0), Some(fonts[index].2)); }
                                    }
                                } else {
                                    write_text_run(&mut x, line, object.text_font.and_then(|font| styles.drawing_text_font(font)),
                                        object.text_font.and_then(|font| styles.drawing_latin_face(font)),
                                        object.text_font.and_then(|font| styles.drawing_single_underline(font)));
                                }
                            }
                            x.close("a:p");
                            line_start += line.len() + 1;
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
