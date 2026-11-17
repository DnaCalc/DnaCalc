//! The formatting records of a BIFF8 workbook globals substream (MS-XLS 2.4.122 Font, 2.4.126
//! Format, 2.4.353 XF, 2.4.269 Style, 2.4.188 Palette) read into the styles part they stand for:
//! number formats, fonts, fills, borders, cell style formats and cell formats, with the palette's
//! colours written as explicit RGB values.
//!
//! The XF table keeps cell formats first (so the workbook's default cell format is format 0, as in
//! an Excel-written .xlsx) and the style formats after them; [`Styles::cell_format`] turns a BIFF
//! XF index into the index a cell's `s` attribute holds.

use super::records::Record;
use crate::xlsb::{Cursor, Xml};
use std::collections::BTreeMap;

pub(crate) const FONT: u16 = 0x0031;
pub(crate) const FORMAT: u16 = 0x041E;
pub(crate) const XF: u16 = 0x00E0;
pub(crate) const STYLE: u16 = 0x0293;
pub(crate) const PALETTE: u16 = 0x0092;

const MAIN_NAMESPACE: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";

/// The default 56-colour palette (indices 8 to 63), as RGB.
const DEFAULT_PALETTE: [u32; 56] = [
    0x000000, 0xFFFFFF, 0xFF0000, 0x00FF00, 0x0000FF, 0xFFFF00, 0xFF00FF, 0x00FFFF, 0x800000,
    0x008000, 0x000080, 0x808000, 0x800080, 0x008080, 0xC0C0C0, 0x808080, 0x9999FF, 0x993366,
    0xFFFFCC, 0xCCFFFF, 0x660066, 0xFF8080, 0x0066CC, 0xCCCCFF, 0x000080, 0xFF00FF, 0xFFFF00,
    0x00FFFF, 0x800080, 0x800000, 0x008080, 0x0000FF, 0x00CCFF, 0xCCFFFF, 0xCCFFCC, 0xFFFF99,
    0x99CCFF, 0xFF99CC, 0xCC99FF, 0xFFCC99, 0x3366FF, 0x33CCCC, 0x99CC00, 0xFFCC00, 0xFF9900,
    0xFF6600, 0x666699, 0x969696, 0x003366, 0x339966, 0x003300, 0x333300, 0x993300, 0x993366,
    0x333399, 0x333333,
];

#[derive(Clone, Debug)]
struct FontRecord {
    height: u16,
    flags: u16,
    color: u16,
    weight: u16,
    script: u16,
    underline: u8,
    family: u8,
    charset: u8,
    name: String,
}

#[derive(Clone, Debug)]
struct XfRecord {
    font: u16,
    format: u16,
    flags: u16,
    alignment: u8,
    rotation: u8,
    indent: u8,
    used: u8,
    border1: u32,
    border2: u32,
    fill: u16,
}

impl XfRecord {
    fn is_style(&self) -> bool {
        self.flags & 0x0004 != 0
    }

    fn parent(&self) -> u16 {
        self.flags >> 4
    }
}

#[derive(Clone, Debug)]
struct StyleRecord {
    xf: u16,
    built_in: Option<(u8, u8)>,
    name: String,
}

#[derive(Debug, Default)]
pub(crate) struct Styles {
    fonts: Vec<FontRecord>,
    formats: BTreeMap<u16, String>,
    xfs: Vec<XfRecord>,
    styles: Vec<StyleRecord>,
    palette: Option<Vec<u32>>,
    /// Differential formats conditional formats ask for, as `<dxf>` elements.
    dxfs: std::cell::RefCell<Vec<Vec<u8>>>,
}

fn e(m: crate::xlsb::Malformed) -> String {
    m.0
}

impl Styles {
    /// Reads one formatting record of the globals; `false` when `record` is not one.
    /// `v5` is the code page of a BIFF5 or BIFF7 workbook, whose records lay these out differently.
    pub(crate) fn read(&mut self, record: &Record<'_>, v5: Option<u16>) -> Result<bool, String> {
        match record.id {
            FONT => {
                let body = record.joined();
                let mut c = Cursor::new(&body);
                let height = c.u16("Font").map_err(e)?;
                let flags = c.u16("Font").map_err(e)?;
                let color = c.u16("Font").map_err(e)?;
                let weight = c.u16("Font").map_err(e)?;
                let script = c.u16("Font").map_err(e)?;
                let underline = c.u8("Font").map_err(e)?;
                let family = c.u8("Font").map_err(e)?;
                let charset = c.u8("Font").map_err(e)?;
                c.skip(1, "Font").map_err(e)?;
                let name = match v5 {
                    Some(page) => super::workbook::byte_string(&mut c, page, "a font name")?,
                    None => super::workbook::short_string(&mut c, "a font name")?,
                };
                self.fonts.push(FontRecord {
                    height,
                    flags,
                    color,
                    weight,
                    script,
                    underline,
                    family,
                    charset,
                    name,
                });
            }
            FORMAT if v5.is_some() => {
                let mut c = Cursor::new(record.body);
                let id = c.u16("Format").map_err(e)?;
                let code = super::workbook::byte_string(&mut c, v5.unwrap_or(1252), "a number format")?;
                self.formats.insert(id, code);
            }
            XF if v5.is_some() => {
                // 16 bytes: font, format, type/protection/parent, alignment, orientation and
                // used attributes, fill colours and pattern with the bottom border, the other
                // three borders with their colours. Laid out as the BIFF8 XF the writer reads.
                let mut c = Cursor::new(record.body);
                let font = c.u16("XF").map_err(e)?;
                let format = c.u16("XF").map_err(e)?;
                let flags = c.u16("XF").map_err(e)?;
                let alignment = c.u8("XF").map_err(e)?;
                let orientation = c.u8("XF").map_err(e)?;
                let fill = c.u32("XF").map_err(e)?;
                let lines = c.u32("XF").map_err(e)?;
                let (top, left, right) = (lines & 7, (lines >> 3) & 7, (lines >> 6) & 7);
                let (top_color, left_color, right_color) =
                    ((lines >> 9) & 0x7F, (lines >> 16) & 0x7F, (lines >> 23) & 0x7F);
                let (bottom, bottom_color) = ((fill >> 22) & 7, (fill >> 25) & 0x7F);
                let pattern = (fill >> 16) & 0x3F;
                self.xfs.push(XfRecord {
                    font,
                    format,
                    // Type, protection and parent: the same bits as BIFF8.
                    flags,
                    alignment: alignment & 0x7F,
                    // 0 none, 1 stacked, 2 90 degrees up, 3 90 degrees down.
                    rotation: match orientation & 3 {
                        1 => 255,
                        2 => 90,
                        3 => 180,
                        _ => 0,
                    },
                    indent: 0,
                    used: orientation & 0xFC,
                    border1: left | right << 4 | top << 8 | bottom << 12 | left_color << 16 | right_color << 23,
                    border2: top_color | bottom_color << 7 | pattern << 25,
                    fill: ((fill & 0x7F) | (((fill >> 7) & 0x7F) << 7)) as u16,
                });
            }
            FORMAT => {
                let mut p = super::strings::Pieces::new(record);
                let id = p.u16("Format").map_err(|m| m.0)?;
                let code = p.unicode_string("a number format").map_err(|m| m.0)?;
                self.formats.insert(id, String::from_utf16_lossy(&code));
            }
            XF => {
                let mut c = Cursor::new(record.body);
                let font = c.u16("XF").map_err(e)?;
                let format = c.u16("XF").map_err(e)?;
                let flags = c.u16("XF").map_err(e)?;
                let alignment = c.u8("XF").map_err(e)?;
                let rotation = c.u8("XF").map_err(e)?;
                let indent = c.u8("XF").map_err(e)?;
                let used = c.u8("XF").map_err(e)?;
                let border1 = c.u32("XF").map_err(e)?;
                let border2 = c.u32("XF").map_err(e)?;
                let fill = c.u16("XF").map_err(e)?;
                self.xfs.push(XfRecord {
                    font,
                    format,
                    flags,
                    alignment,
                    rotation,
                    indent,
                    used,
                    border1,
                    border2,
                    fill,
                });
            }
            STYLE => {
                let mut p = super::strings::Pieces::new(record);
                let head = p.u16("Style").map_err(|m| m.0)?;
                let xf = head & 0x0FFF;
                if v5.is_some() && head & 0x8000 == 0 {
                    // BIFF5 names a user style as a byte string with an 8-bit length.
                    let mut c = Cursor::new(&record.body[2.min(record.body.len())..]);
                    let name = super::workbook::byte_string(&mut c, v5.unwrap_or(1252), "a style name")?;
                    self.styles.push(StyleRecord {
                        xf,
                        built_in: None,
                        name,
                    });
                } else if head & 0x8000 != 0 {
                    let id = p.u8("Style").map_err(|m| m.0)?;
                    let level = p.u8("Style").map_err(|m| m.0)?;
                    self.styles.push(StyleRecord {
                        xf,
                        built_in: Some((id, level)),
                        name: String::new(),
                    });
                } else {
                    let name = p.unicode_string("a style name").map_err(|m| m.0)?;
                    self.styles.push(StyleRecord {
                        xf,
                        built_in: None,
                        name: String::from_utf16_lossy(&name),
                    });
                }
            }
            PALETTE => {
                let mut c = Cursor::new(record.body);
                let count = usize::from(c.u16("Palette").map_err(e)?);
                let mut colors = Vec::with_capacity(count);
                for _ in 0..count {
                    let r = c.u8("Palette").map_err(e)?;
                    let g = c.u8("Palette").map_err(e)?;
                    let b = c.u8("Palette").map_err(e)?;
                    c.skip(1, "Palette").map_err(e)?;
                    colors.push(u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b));
                }
                self.palette = Some(colors);
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.xfs.is_empty()
    }

    /// Whether a styles part is written: there are formats, or differential formats.
    pub(crate) fn needs_part(&self) -> bool {
        !self.xfs.is_empty() || !self.dxfs.borrow().is_empty() || self.palette.as_ref().is_some_and(|p| p.len() == 56)
    }

    pub(crate) fn indexed_colors_representable(&self) -> bool {
        self.palette.as_ref().is_none_or(|p| p.len() == 56)
    }

    /// The index of BIFF XF `ixfe` in the written `cellXfs`: cell formats keep their order, style
    /// formats follow them.
    pub(crate) fn cell_format(&self, ixfe: u16) -> u32 {
        let index = usize::from(ixfe);
        let Some(xf) = self.xfs.get(index) else {
            return 0;
        };
        let cell_formats = self.xfs.iter().filter(|x| !x.is_style()).count();
        let before = self.xfs[..index].iter().filter(|x| x.is_style() == xf.is_style()).count();
        (if xf.is_style() { cell_formats + before } else { before }) as u32
    }

    fn rgb(&self, icv: u16) -> Option<String> {
        let value = match icv {
            0..=7 => DEFAULT_PALETTE[usize::from(icv)],
            8..=63 => {
                let at = usize::from(icv) - 8;
                match &self.palette {
                    Some(p) if at < p.len() => p[at],
                    _ => DEFAULT_PALETTE[at],
                }
            }
            _ => return None,
        };
        Some(format!("FF{value:06X}"))
    }

    /// `<color .../>` attributes for palette index `icv`.
    pub(crate) fn color_attributes(&self, icv: u16) -> Vec<(&'static str, String)> {
        self.color(icv)
    }

    /// Adds a differential format (a `<dxf>` element) and returns its index; equal ones are shared.
    pub(crate) fn add_dxf(&self, xml: Vec<u8>) -> usize {
        let mut dxfs = self.dxfs.borrow_mut();
        match dxfs.iter().position(|d| *d == xml) {
            Some(at) => at,
            None => {
                dxfs.push(xml);
                dxfs.len() - 1
            }
        }
    }

    pub(crate) fn pattern_name(&self, style: u16) -> &'static str {
        PATTERNS.get(usize::from(style)).copied().unwrap_or("none")
    }

    pub(crate) fn border_style_name(&self, style: u32) -> &'static str {
        BORDER_STYLES.get(style as usize).copied().unwrap_or("thin")
    }

    fn color(&self, icv: u16) -> Vec<(&'static str, String)> {
        match self.rgb(icv) {
            Some(rgb) => vec![("rgb", rgb)],
            None if icv == 0x7FFF => vec![("auto", "1".to_owned())],
            None => vec![("indexed", (icv & 0x7F).to_string())],
        }
    }

    pub(crate) fn font_index(&self, ifnt: u16) -> Option<usize> {
        // BIFF numbers fonts from 0 but never uses 4.
        let at = if ifnt >= 5 { ifnt - 1 } else { ifnt };
        (usize::from(at) < self.fonts.len()).then_some(usize::from(at))
    }

    /// DrawingML text properties supported by the shape model, including explicit palette colours.
    pub(crate) fn drawing_text_font(&self, ifnt: u16) -> Option<(u32, bool, bool, Option<String>)> {
        if ifnt == 4 { return None; }
        let font = &self.fonts[self.font_index(ifnt)?];
        // DrawingML text sizes are at least one point; malformed BIFF heights stay incomplete.
        if font.height < 20 || (font.color > 63 && font.color != 0x7fff) { return None; }
        Some((u32::from(font.height) * 5, font.weight >= 700, font.flags & 0x0002 != 0,
            self.rgb(font.color).map(|argb| argb[2..].to_owned())))
    }

    /// A literal Latin font name that can be carried without guessing charset or theme behavior.
    pub(crate) fn drawing_latin_face(&self, ifnt: u16) -> Option<&str> {
        if ifnt == 4 { return None; }
        let font = &self.fonts[self.font_index(ifnt)?];
        (font.charset == 0 && font.family <= 5 && !font.name.is_empty() && !font.name.starts_with('+')
            && !font.name.chars().any(|c| c.is_control() || matches!(c, '\u{fffd}' | '\u{fffe}' | '\u{ffff}'))).then_some(font.name.as_str())
    }

    /// FontX uses a one-based index into workbook Font records, unlike FontIndex.
    pub(crate) fn chart_title_font(&self, index: u16) -> Option<(String, bool, bool)> {
        let zero_based = index.checked_sub(1)?;
        let ifnt = if zero_based >= 4 { zero_based.checked_add(1)? } else { zero_based };
        let ((_, bold, italic, _), _, underline) = self.drawing_rich_font(ifnt)?;
        if underline { return None; }
        Some((self.drawing_latin_face(ifnt)?.to_owned(), bold, italic))
    }

    /// Single underline is the only BIFF underline represented by the shape model.
    pub(crate) fn drawing_single_underline(&self, ifnt: u16) -> Option<bool> {
        if ifnt == 4 { return None; }
        (self.fonts[self.font_index(ifnt)?].underline == 1).then_some(true)
    }

    /// A run whose effects the drawing text model can represent, plus its font-face identity.
    pub(crate) fn drawing_rich_font(&self, ifnt: u16) -> Option<((u32, bool, bool, Option<String>), (&str, u8, u8), bool)> {
        if ifnt == 4 { return None; }
        let font = &self.fonts[self.font_index(ifnt)?];
        if font.name.is_empty() || font.underline > 1 || font.script != 0 || font.flags & 0x00f8 != 0
            || !matches!(font.weight, 400 | 700) { return None; }
        Some((self.drawing_text_font(ifnt)?, (&font.name, font.charset, font.family), font.underline == 1))
    }

    /// `<rPr>` for the font `ifnt` of a rich text run; nothing when there is no such font.
    pub(crate) fn write_run_properties(&self, x: &mut Xml, ifnt: u16) {
        let Some(font) = self.font_index(ifnt).map(|k| &self.fonts[k]) else {
            return;
        };
        x.open("rPr", &[]);
        x.empty("rFont", &[("val", font.name.clone())]);
        if font.charset != 0 {
            x.empty("charset", &[("val", font.charset.to_string())]);
        }
        if font.family != 0 {
            x.empty("family", &[("val", font.family.to_string())]);
        }
        if font.weight >= 700 {
            x.empty("b", &[]);
        }
        if font.flags & 0x0002 != 0 {
            x.empty("i", &[]);
        }
        if font.flags & 0x0008 != 0 {
            x.empty("strike", &[]);
        }
        x.empty("color", &self.color(font.color));
        x.empty("sz", &[("val", crate::xlsb::formula::number_text(f64::from(font.height) / 20.0))]);
        match font.underline {
            0 => {}
            0x02 => x.empty("u", &[("val", "double".to_owned())]),
            0x21 => x.empty("u", &[("val", "singleAccounting".to_owned())]),
            0x22 => x.empty("u", &[("val", "doubleAccounting".to_owned())]),
            _ => x.empty("u", &[]),
        }
        match font.script {
            1 => x.empty("vertAlign", &[("val", "superscript".to_owned())]),
            2 => x.empty("vertAlign", &[("val", "subscript".to_owned())]),
            _ => {}
        }
        x.close("rPr");
    }

    /// The styles part.
    pub(crate) fn write(&self) -> Vec<u8> {
        let mut x = Xml::new();
        x.open("styleSheet", &[("xmlns", MAIN_NAMESPACE.to_owned())]);
        if !self.formats.is_empty() {
            x.open("numFmts", &[("count", self.formats.len().to_string())]);
            for (id, code) in &self.formats {
                x.empty(
                    "numFmt",
                    &[("numFmtId", id.to_string()), ("formatCode", code.clone())],
                );
            }
            x.close("numFmts");
        }
        // Fonts.
        if self.fonts.is_empty() {
            x.open("fonts", &[("count", "1".to_owned())]);
            x.open("font", &[]);
            x.empty("sz", &[("val", "10".to_owned())]);
            x.empty("name", &[("val", "Arial".to_owned())]);
            x.close("font");
            x.close("fonts");
        }
        if !self.fonts.is_empty() {
        x.open("fonts", &[("count", self.fonts.len().to_string())]);
        for font in &self.fonts {
            x.open("font", &[]);
            if font.weight >= 700 {
                x.empty("b", &[]);
            }
            if font.flags & 0x0002 != 0 {
                x.empty("i", &[]);
            }
            if font.flags & 0x0008 != 0 {
                x.empty("strike", &[]);
            }
            if font.flags & 0x0010 != 0 {
                x.empty("outline", &[]);
            }
            if font.flags & 0x0020 != 0 {
                x.empty("shadow", &[]);
            }
            match font.underline {
                0 => {}
                0x02 => x.empty("u", &[("val", "double".to_owned())]),
                0x21 => x.empty("u", &[("val", "singleAccounting".to_owned())]),
                0x22 => x.empty("u", &[("val", "doubleAccounting".to_owned())]),
                _ => x.empty("u", &[]),
            }
            match font.script {
                1 => x.empty("vertAlign", &[("val", "superscript".to_owned())]),
                2 => x.empty("vertAlign", &[("val", "subscript".to_owned())]),
                _ => {}
            }
            x.empty("sz", &[("val", crate::xlsb::formula::number_text(f64::from(font.height) / 20.0))]);
            x.empty("color", &self.color(font.color));
            x.empty("name", &[("val", font.name.clone())]);
            if font.family != 0 {
                x.empty("family", &[("val", font.family.to_string())]);
            }
            if font.charset != 0 {
                x.empty("charset", &[("val", font.charset.to_string())]);
            }
            x.close("font");
        }
        x.close("fonts");
        }

        // Fills and borders, deduplicated; xlsx wants the "none" and "gray125" fills first.
        let mut fills: Vec<(u32, u16, u16)> = vec![(0, 0, 0), (17, 0, 0)];
        let mut borders: Vec<[u32; 3]> = vec![[0, 0, 0]];
        let mut ids: Vec<(usize, usize)> = Vec::with_capacity(self.xfs.len());
        for xf in &self.xfs {
            let pattern = xf.border2 >> 25;
            let fore = xf.fill & 0x7F;
            let back = (xf.fill >> 7) & 0x7F;
            let fill = if pattern == 0 {
                (0, 0, 0)
            } else {
                (pattern, fore, back)
            };
            let fill_id = match fills.iter().position(|f| *f == fill) {
                Some(p) => p,
                None => {
                    fills.push(fill);
                    fills.len() - 1
                }
            };
            let border = [xf.border1, xf.border2 & 0x01FF_FFFF, 0];
            let border_id = match borders.iter().position(|b| *b == border) {
                Some(p) => p,
                None => {
                    borders.push(border);
                    borders.len() - 1
                }
            };
            ids.push((fill_id, border_id));
        }
        x.open("fills", &[("count", fills.len().to_string())]);
        for (pattern, fore, back) in &fills {
            x.open("fill", &[]);
            let name = PATTERNS.get(*pattern as usize).copied().unwrap_or("none");
            x.open("patternFill", &[("patternType", name.to_owned())]);
            if *pattern != 0 && (*pattern, *fore, *back) != (17, 0, 0) {
                x.empty("fgColor", &self.color(*fore));
                x.empty("bgColor", &self.color(*back));
            }
            x.close("patternFill");
            x.close("fill");
        }
        x.close("fills");
        x.open("borders", &[("count", borders.len().to_string())]);
        for b in &borders {
            let (b1, b2) = (b[0], b[1]);
            let diagonal_up = b1 >> 30 & 1 != 0;
            let diagonal_down = b1 >> 31 & 1 != 0;
            let mut attributes = Vec::new();
            if diagonal_up {
                attributes.push(("diagonalUp", "1".to_owned()));
            }
            if diagonal_down {
                attributes.push(("diagonalDown", "1".to_owned()));
            }
            x.open("border", &attributes);
            let edges = [
                ("left", b1 & 0xF, (b1 >> 16) & 0x7F),
                ("right", (b1 >> 4) & 0xF, (b1 >> 23) & 0x7F),
                ("top", (b1 >> 8) & 0xF, b2 & 0x7F),
                ("bottom", (b1 >> 12) & 0xF, (b2 >> 7) & 0x7F),
                ("diagonal", (b2 >> 21) & 0xF, (b2 >> 14) & 0x7F),
            ];
            for (name, style, color) in edges {
                if style == 0 {
                    x.empty(name, &[]);
                } else {
                    x.open(
                        name,
                        &[(
                            "style",
                            BORDER_STYLES.get(style as usize).copied().unwrap_or("thin").to_owned(),
                        )],
                    );
                    x.empty("color", &self.color(color as u16));
                    x.close(name);
                }
            }
            x.close("border");
        }
        x.close("borders");

        // Style formats, then cell formats (cell formats first in cellXfs, style formats after).
        let style_positions: Vec<usize> = (0..self.xfs.len()).filter(|k| self.xfs[*k].is_style()).collect();
        let style_xf = |k: usize, parent: Option<usize>, x: &mut Xml, this: &Styles| {
            let xf = &this.xfs[k];
            this.write_xf(x, xf, ids[k], parent);
        };
        x.open("cellStyleXfs", &[("count", style_positions.len().max(1).to_string())]);
        if style_positions.is_empty() {
            x.empty(
                "xf",
                &[
                    ("numFmtId", "0".to_owned()),
                    ("fontId", "0".to_owned()),
                    ("fillId", "0".to_owned()),
                    ("borderId", "0".to_owned()),
                ],
            );
        }
        for k in &style_positions {
            style_xf(*k, None, &mut x, self);
        }
        x.close("cellStyleXfs");
        let order: Vec<usize> = (0..self.xfs.len())
            .filter(|k| !self.xfs[*k].is_style())
            .chain(style_positions.iter().copied())
            .collect();
        x.open("cellXfs", &[("count", order.len().max(1).to_string())]);
        if order.is_empty() {
            x.empty(
                "xf",
                &[
                    ("numFmtId", "0".to_owned()),
                    ("fontId", "0".to_owned()),
                    ("fillId", "0".to_owned()),
                    ("borderId", "0".to_owned()),
                    ("xfId", "0".to_owned()),
                ],
            );
        }
        for k in order {
            let parent = if self.xfs[k].is_style() {
                style_positions.iter().position(|p| *p == k)
            } else {
                style_positions
                    .iter()
                    .position(|p| *p == usize::from(self.xfs[k].parent()))
                    .or(Some(0))
            };
            style_xf(k, parent, &mut x, self);
        }
        x.close("cellXfs");
        x.open("cellStyles", &[("count", self.styles.len().max(1).to_string())]);
        if self.styles.is_empty() {
            x.empty(
                "cellStyle",
                &[
                    ("name", "Normal".to_owned()),
                    ("xfId", "0".to_owned()),
                    ("builtinId", "0".to_owned()),
                ],
            );
        }
        for style in &self.styles {
            let xf_id = style_positions
                .iter()
                .position(|p| *p == usize::from(style.xf))
                .unwrap_or(0);
            let mut attributes = Vec::new();
            match style.built_in {
                Some((id, level)) => {
                    let name = match id {
                        0 => "Normal".to_owned(),
                        1 => format!("RowLevel_{}", u32::from(level) + 1),
                        2 => format!("ColLevel_{}", u32::from(level) + 1),
                        3 => "Comma".to_owned(),
                        4 => "Currency".to_owned(),
                        5 => "Percent".to_owned(),
                        6 => "Comma [0]".to_owned(),
                        7 => "Currency [0]".to_owned(),
                        8 => "Hyperlink".to_owned(),
                        9 => "Followed Hyperlink".to_owned(),
                        other => format!("Style {other}"),
                    };
                    attributes.push(("name", name));
                    attributes.push(("xfId", xf_id.to_string()));
                    attributes.push(("builtinId", id.to_string()));
                    if matches!(id, 1 | 2) {
                        attributes.push(("iLevel", level.to_string()));
                    }
                }
                None => {
                    attributes.push(("name", style.name.clone()));
                    attributes.push(("xfId", xf_id.to_string()));
                }
            }
            x.empty("cellStyle", &attributes);
        }
        x.close("cellStyles");
        {
            let dxfs = self.dxfs.borrow();
            if dxfs.is_empty() {
                x.empty("dxfs", &[("count", "0".to_owned())]);
            } else {
                x.open("dxfs", &[("count", dxfs.len().to_string())]);
                for dxf in dxfs.iter() {
                    x.out.extend_from_slice(dxf);
                }
                x.close("dxfs");
            }
        }
        x.empty(
            "tableStyles",
            &[
                ("count", "0".to_owned()),
                ("defaultTableStyle", "TableStyleMedium9".to_owned()),
                ("defaultPivotStyle", "PivotStyleLight16".to_owned()),
            ],
        );
        if self.palette.as_ref().is_some_and(|p| p.len() == 56) {
            x.open("colors", &[]);
            x.open("indexedColors", &[]);
            for index in 0..64u16 {
                if let Some(rgb) = self.rgb(index) { x.empty("rgbColor", &[("rgb", rgb)]); }
            }
            x.close("indexedColors");
            x.close("colors");
        }
        x.close("styleSheet");
        x.out
    }

    fn write_xf(&self, x: &mut Xml, xf: &XfRecord, ids: (usize, usize), parent: Option<usize>) {
        let font = self.font_index(xf.font).unwrap_or(0);
        let mut attributes = vec![
            ("numFmtId", xf.format.to_string()),
            ("fontId", font.to_string()),
            ("fillId", ids.0.to_string()),
            ("borderId", ids.1.to_string()),
        ];
        if let Some(parent) = parent {
            attributes.push(("xfId", parent.to_string()));
        }
        let used = (xf.used >> 2) & 0x3F;
        // For a cell format the attribute bits say what differs from the style; for a style format
        // they say what it does not apply.
        let applied = |bit: u8| {
            if xf.is_style() {
                used & (1 << bit) == 0
            } else {
                used & (1 << bit) != 0
            }
        };
        if applied(0) {
            attributes.push(("applyNumberFormat", "1".to_owned()));
        }
        if applied(1) {
            attributes.push(("applyFont", "1".to_owned()));
        }
        if applied(4) {
            attributes.push(("applyFill", "1".to_owned()));
        }
        if applied(3) {
            attributes.push(("applyBorder", "1".to_owned()));
        }
        if applied(2) {
            attributes.push(("applyAlignment", "1".to_owned()));
        }
        if applied(5) {
            attributes.push(("applyProtection", "1".to_owned()));
        }
        if xf.flags & 0x0008 != 0 {
            attributes.push(("quotePrefix", "1".to_owned()));
        }
        let horizontal = match xf.alignment & 7 {
            1 => Some("left"),
            2 => Some("center"),
            3 => Some("right"),
            4 => Some("fill"),
            5 => Some("justify"),
            6 => Some("centerContinuous"),
            7 => Some("distributed"),
            _ => None,
        };
        let vertical = match (xf.alignment >> 4) & 7 {
            0 => Some("top"),
            1 => Some("center"),
            3 => Some("justify"),
            4 => Some("distributed"),
            _ => None,
        };
        let wrap = xf.alignment & 0x08 != 0;
        let shrink = xf.indent & 0x10 != 0;
        let indent = xf.indent & 0x0F;
        let reading = (xf.indent >> 6) & 3;
        let has_alignment = horizontal.is_some()
            || vertical.is_some()
            || wrap
            || shrink
            || indent != 0
            || xf.rotation != 0
            || reading != 0;
        let locked = xf.flags & 0x0001 != 0;
        let hidden = xf.flags & 0x0002 != 0;
        if !has_alignment && locked && !hidden {
            x.empty("xf", &attributes);
            return;
        }
        x.open("xf", &attributes);
        if has_alignment {
            let mut a = Vec::new();
            if let Some(h) = horizontal {
                a.push(("horizontal", h.to_owned()));
            }
            if let Some(v) = vertical {
                a.push(("vertical", v.to_owned()));
            }
            if xf.rotation != 0 {
                a.push(("textRotation", xf.rotation.to_string()));
            }
            if wrap {
                a.push(("wrapText", "1".to_owned()));
            }
            if indent != 0 {
                a.push(("indent", indent.to_string()));
            }
            if shrink {
                a.push(("shrinkToFit", "1".to_owned()));
            }
            if reading != 0 {
                a.push(("readingOrder", reading.to_string()));
            }
            x.empty("alignment", &a);
        }
        if !locked || hidden {
            let mut p = Vec::new();
            if !locked {
                p.push(("locked", "0".to_owned()));
            }
            if hidden {
                p.push(("hidden", "1".to_owned()));
            }
            x.empty("protection", &p);
        }
        x.close("xf");
    }
}

const PATTERNS: [&str; 19] = [
    "none",
    "solid",
    "mediumGray",
    "darkGray",
    "lightGray",
    "darkHorizontal",
    "darkVertical",
    "darkDown",
    "darkUp",
    "darkGrid",
    "darkTrellis",
    "lightHorizontal",
    "lightVertical",
    "lightDown",
    "lightUp",
    "lightGrid",
    "lightTrellis",
    "gray125",
    "gray0625",
];

const BORDER_STYLES: [&str; 14] = [
    "none",
    "thin",
    "medium",
    "dashed",
    "dotted",
    "thick",
    "double",
    "hair",
    "mediumDashed",
    "dashDot",
    "mediumDashDot",
    "dashDotDot",
    "mediumDashDotDot",
    "slantDashDot",
];
