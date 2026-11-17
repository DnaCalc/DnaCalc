//! Sheet-level records of a BIFF8 worksheet that are not cells: window and pane (WINDOW2, PANE,
//! SELECTION, SCL), merged cells, hyperlinks, protection, page setup and print records, written
//! as the SpreadsheetML elements around the sheet's data (MS-XLS 2.4.346, 2.4.169, 2.4.220, 2.4.141,
//! 2.4.226 and the page setup records).

use super::records::Record;
use super::sheet::column_name;
use super::strings::Pieces;
use crate::xlsb::formula::number_text;
use crate::xlsb::{Cursor, Xml};

const WINDOW2: u16 = 0x023E;
const SCL: u16 = 0x00A0;
const PANE: u16 = 0x0041;
const SELECTION: u16 = 0x001D;
const MERGED_CELLS: u16 = 0x00E5;
const HLINK: u16 = 0x01B8;
const HLINK_TOOLTIP: u16 = 0x0800;
const PROTECT: u16 = 0x0012;
const PASSWORD: u16 = 0x0013;
const OBJECT_PROTECT: u16 = 0x0063;
const SCEN_PROTECT: u16 = 0x00DD;
const SETUP: u16 = 0x00A1;
const LEFT_MARGIN: u16 = 0x0026;
const RIGHT_MARGIN: u16 = 0x0027;
const TOP_MARGIN: u16 = 0x0028;
const BOTTOM_MARGIN: u16 = 0x0029;
const HEADER: u16 = 0x0014;
const FOOTER: u16 = 0x0015;
const H_CENTER: u16 = 0x0083;
const V_CENTER: u16 = 0x0084;
const PRINT_GRIDLINES: u16 = 0x002B;
const PRINT_HEADERS: u16 = 0x002A;
const WSBOOL: u16 = 0x0081;
const H_PAGE_BREAKS: u16 = 0x001B;
const V_PAGE_BREAKS: u16 = 0x001A;
const DEF_COL_WIDTH: u16 = 0x0055;
const STANDARD_WIDTH: u16 = 0x0099;
const OBJ: u16 = 0x005D;
const TXO: u16 = 0x01B6;
const NOTE: u16 = 0x001C;
const CODE_NAME: u16 = 0x01BA;
const FEAT: u16 = 0x0868;
const FEAT_HEADER: u16 = 0x0867;

/// A relationship the sheet part needs: a hyperlink target, a comments part, a VML drawing.
#[derive(Clone, Debug)]
pub(crate) struct Rel {
    pub(crate) id: String,
    pub(crate) kind: &'static str,
    pub(crate) target: String,
    pub(crate) external: bool,
}

const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

struct Pane {
    x: u16,
    y: u16,
    top: u16,
    left: u16,
    active: u8,
}

struct Selection {
    pane: u8,
    active: (u16, u16),
    ranges: Vec<String>,
}

struct Hyperlink {
    range: String,
    target: Option<String>,
    location: Option<String>,
    display: Option<String>,
    tooltip: Option<String>,
    first: (u16, u16),
}

#[derive(Default)]
pub(crate) struct Features {
    pub(crate) biff4: bool,
    window: Option<(u16, u16, u16, Option<u16>, Option<u16>, Option<u16>)>,
    grid_color_representable: bool,
    zoom: Option<(u16, u16)>,
    pane: Option<Pane>,
    selections: Vec<Selection>,
    merges: Vec<String>,
    phonetic: Option<(usize, u8, u8)>,
    phonetic_ranges: Vec<(u16, u16, u16, u16)>,
    ignored_errors: Vec<(Vec<String>, u32)>,
    /// PLS records, kept separate until a single length-consistent DEVMODE can be identified.
    printer_settings: Vec<Vec<u8>>,
    hyperlinks: Vec<Hyperlink>,
    protect: bool,
    objects: bool,
    scenarios: bool,
    password: Option<u16>,
    setup: Option<[u16; 6]>,
    setup_printer: Option<(u16, u16, Option<u16>)>,
    chart_setup: Option<[u8; 34]>,
    setup_numbers: Option<(f64, f64)>,
    margins: [Option<f64>; 4],
    header: Option<String>,
    footer: Option<String>,
    h_center: bool,
    v_center: bool,
    grid_lines: bool,
    headings: bool,
    ws_bool: Option<u16>,
    tab_color: Option<String>,
    tab_color_seen: bool,
    filter_mode: Option<bool>,
    row_breaks: Vec<(u16, u16, u16)>,
    column_breaks: Vec<(u16, u16, u16)>,
    pub(crate) outline_levels: Option<(u8, u8)>,
    pub(crate) default_column_width: Option<u16>,
    pub(crate) standard_column_width: Option<u16>,
    /// The id of the comment object (OBJ of type note) waiting for its text box.
    waiting_object: Option<u16>,
    /// The sheet's number, which names its comment parts.
    pub(crate) number: usize,
    pub(crate) macro_sheet: bool,
    /// The code page of a BIFF5 or BIFF7 workbook.
    pub(crate) v5: Option<u16>,
    /// The sheet's VBA code name (CODENAME).
    pub(crate) code_name: Option<String>,
    note_texts: std::collections::BTreeMap<u16, Vec<u16>>,
    note_runs: std::collections::BTreeMap<u16, Vec<(u16, u16)>>,
    notes: Vec<Note>,
}

struct Note {
    row: u16,
    column: u16,
    visible: bool,
    object: u16,
    author: String,
}

fn e(m: crate::xlsb::Malformed) -> String {
    m.0
}

fn cell(row: u16, column: u16) -> String {
    format!("{}{}", column_name(u32::from(column)), u32::from(row) + 1)
}

fn range(rows: (u16, u16), columns: (u16, u16)) -> String {
    if rows.0 == rows.1 && columns.0 == columns.1 {
        cell(rows.0, columns.0)
    } else {
        format!("{}:{}", cell(rows.0, columns.0), cell(rows.1, columns.1))
    }
}

fn read_string(c: &mut Cursor<'_>, units: usize, what: &str) -> Result<String, String> {
    // A length in UTF-16 units including the terminating NUL.
    let all = c.units(units, what).map_err(e)?;
    let text = String::from_utf16_lossy(&all);
    Ok(text.trim_end_matches('\0').to_owned())
}

/// Shared chart-sheet and embedded-chart page setup projection.
pub(crate) fn write_chart_setup(x: &mut Xml, data: &[u8; 34], element: &str) {
    let word = |at: usize| u16::from_le_bytes([data[at], data[at + 1]]);
    let flags = word(10);
    let mut a = Vec::new();
    if flags & 4 == 0 {
        let paper = word(0);
        if (1..=117).contains(&paper) && paper != 48 && paper != 49 {
            a.push(("paperSize", paper.to_string()));
        }
        a.push(("orientation", if flags & 0x42 != 0 { "portrait" } else { "landscape" }.to_owned()));
        for (name, value) in [("horizontalDpi", word(12)), ("verticalDpi", word(14)), ("copies", word(32))] {
            if value != 0 { a.push((name, value.to_string())); }
        }
    } else {
        a.push(("usePrinterDefaults", "1".to_owned()));
    }
    if flags & 8 != 0 { a.push(("blackAndWhite", "1".to_owned())); }
    if flags & 16 != 0 { a.push(("draft", "1".to_owned())); }
    let first = word(4) as i16;
    if flags & 0x80 != 0 && first > 0 {
        a.push(("firstPageNumber", first.to_string()));
        a.push(("useFirstPageNumber", "1".to_owned()));
    }
    if !a.is_empty() { x.empty(element, &a); }
}

impl Features {
    /// Chart-sheet Setup has a different OOXML pageSetup schema from worksheets.
    pub(crate) fn read_chart_setup(&mut self, record: &Record<'_>) {
        if record.continues.is_empty() {
            self.chart_setup = record.body.try_into().ok();
        }
    }

    pub(crate) fn write_chart_setup(&self, x: &mut Xml) {
        if let Some(data) = self.chart_setup { write_chart_setup(x, &data, "pageSetup"); }
    }

    /// Chart-sheet margins are retained only when all six values are available.
    pub(crate) fn write_chart_margins(&self, x: &mut Xml) {
        if let ([Some(left), Some(right), Some(top), Some(bottom)], Some((header, footer))) =
            (self.margins, self.setup_numbers)
        {
            x.empty("pageMargins", &[("left", number_text(left)), ("right", number_text(right)),
                ("top", number_text(top)), ("bottom", number_text(bottom)),
                ("header", number_text(header)), ("footer", number_text(footer))]);
        }
    }

    /// Reads `record` when it is a sheet-feature record; `false` when it is not one.
    pub(crate) fn read(&mut self, record: &Record<'_>, styles: &super::styles::Styles) -> Result<bool, String> {
        let mut c = Cursor::new(record.body);
        match record.id {
            WINDOW2 => {
                let flags = c.u16("Window2").map_err(e)?;
                let top = c.u16("Window2").map_err(e)?;
                let left = c.u16("Window2").map_err(e)?;
                let word_at = |offset: usize| record.body.get(offset..offset + 2)
                    .map(|b| u16::from_le_bytes([b[0], b[1]]));
                let color = word_at(6);
                let zoom_page = word_at(10);
                let zoom_normal = word_at(12);
                self.window = Some((flags, top, left, color, zoom_page, zoom_normal));
                self.grid_color_representable = styles.indexed_colors_representable();
                // A nondefault grid color without a representable index is not fully read.
                if flags & 0x0020 == 0 && (color.is_none_or(|value| value >= 64) || !self.grid_color_representable) { return Ok(false); }
                if [zoom_page, zoom_normal].into_iter().flatten().any(|value| value != 0 && !(10..=400).contains(&value)) {
                    return Ok(false);
                }
            }
            SCL => {
                self.zoom = Some((c.u16("Scl").map_err(e)?, c.u16("Scl").map_err(e)?));
            }
            PANE => {
                self.pane = Some(Pane {
                    x: c.u16("Pane").map_err(e)?,
                    y: c.u16("Pane").map_err(e)?,
                    top: c.u16("Pane").map_err(e)?,
                    left: c.u16("Pane").map_err(e)?,
                    active: c.u8("Pane").map_err(e)?,
                });
            }
            SELECTION => {
                let pane = c.u8("Selection").map_err(e)?;
                let active = (c.u16("Selection").map_err(e)?, c.u16("Selection").map_err(e)?);
                c.skip(2, "Selection").map_err(e)?;
                let count = c.u16("Selection").map_err(e)?;
                let mut ranges = Vec::new();
                for _ in 0..count {
                    let rows = (c.u16("Selection").map_err(e)?, c.u16("Selection").map_err(e)?);
                    let columns = (u16::from(c.u8("Selection").map_err(e)?), u16::from(c.u8("Selection").map_err(e)?));
                    ranges.push(range(rows, columns));
                }
                self.selections.push(Selection {
                    pane,
                    active,
                    ranges,
                });
            }
            MERGED_CELLS => {
                let count = c.u16("MergeCells").map_err(e)?;
                for _ in 0..count {
                    let rows = (c.u16("MergeCells").map_err(e)?, c.u16("MergeCells").map_err(e)?);
                    let columns = (c.u16("MergeCells").map_err(e)?, c.u16("MergeCells").map_err(e)?);
                    self.merges.push(range(rows, columns));
                }
            }
            HLINK => self.hyperlink(&mut c)?,
            HLINK_TOOLTIP => {
                // Reserved type (2), reference (8), then the tooltip as UTF-16 with its NUL.
                c.skip(10, "HLinkTooltip").map_err(e)?;
                let units = c.remaining() / 2;
                let text = read_string(&mut c, units, "a tooltip")?;
                if let Some(last) = self.hyperlinks.last_mut() {
                    last.tooltip = Some(text);
                }
            }
            PROTECT => self.protect = c.u16("Protect").map_err(e)? != 0,
            OBJECT_PROTECT => self.objects = c.u16("ObjProtect").map_err(e)? != 0,
            SCEN_PROTECT => self.scenarios = c.u16("ScenProtect").map_err(e)? != 0,
            PASSWORD => {
                let hash = c.u16("Password").map_err(e)?;
                self.password = (hash != 0).then_some(hash);
            }
            SETUP => {
                let mut v = [0u16; 6];
                for item in &mut v {
                    *item = c.u16("Setup").map_err(e)?;
                }
                if self.biff4 && c.remaining() == 0 {
                    self.setup = Some(v);
                    // BIFF4 has no printer resolution, header/footer margin or copies fields.
                    return Ok(false);
                }
                let horizontal = c.u16("Setup").map_err(e)?;
                let vertical = c.u16("Setup").map_err(e)?;
                self.setup = Some(v);
                self.setup_numbers = Some((c.f64("Setup").map_err(e)?, c.f64("Setup").map_err(e)?));
                let copies = if c.remaining() >= 2 { Some(c.u16("Setup").map_err(e)?) } else { None };
                self.setup_printer = Some((horizontal, vertical, copies));
                // Other page setup and printer-specific details are not all retained.
                return Ok(false);
            }
            LEFT_MARGIN => self.margins[0] = Some(c.f64("LeftMargin").map_err(e)?),
            RIGHT_MARGIN => self.margins[1] = Some(c.f64("RightMargin").map_err(e)?),
            TOP_MARGIN => self.margins[2] = Some(c.f64("TopMargin").map_err(e)?),
            BOTTOM_MARGIN => self.margins[3] = Some(c.f64("BottomMargin").map_err(e)?),
            HEADER | FOOTER => {
                let text = if record.body.is_empty() {
                    String::new()
                } else if let Some(page) = self.v5 {
                    super::workbook::byte_string(&mut c, page, "a header or footer")?
                } else {
                    let mut p = Pieces::new(record);
                    String::from_utf16_lossy(&p.unicode_string("a header or footer").map_err(|m| m.0)?)
                };
                if record.id == HEADER {
                    self.header = Some(text);
                } else {
                    self.footer = Some(text);
                }
            }
            H_CENTER => self.h_center = c.u16("HCenter").map_err(e)? != 0,
            V_CENTER => self.v_center = c.u16("VCenter").map_err(e)? != 0,
            PRINT_GRIDLINES => self.grid_lines = c.u16("PrintGrid").map_err(e)? != 0,
            PRINT_HEADERS => self.headings = c.u16("PrintRowCol").map_err(e)? != 0,
            WSBOOL => self.ws_bool = Some(c.u16("WsBool").map_err(e)?),
            0x0862 => {
                let b = record.body;
                if self.tab_color_seen { self.tab_color = None; return Ok(false); }
                self.tab_color_seen = true;
                // MS-XLS 2.4.259: the twenty-byte form has only a palette tab colour.
                // Optional CFColor extensions need separate handling.
                if self.v5.is_none() && b.len() == 20 && record.continues.is_empty()
                    && b[..12] == [0x62, 0x08, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
                    && b[12..16] == [20, 0, 0, 0] && styles.indexed_colors_representable()
                    && (8..=63).contains(&(b[16] & 0x7f)) {
                    self.tab_color = styles.rgb(u16::from(b[16] & 0x7f));
                }
                // Retained in the converted/saved package; the tab UI and optional fields
                // are not modelled. Keep SheetExt counted as incomplete.
                return Ok(false);
            }
            0x009B => {
                let valid = record.body.is_empty() && record.continues.is_empty();
                self.filter_mode = Some(valid && self.filter_mode.is_none());
                // The application does not use this saved state directly.
                return Ok(false);
            }
            0x004D => {
                let joined = record.joined();
                self.printer_settings.push(joined.get(2..).unwrap_or_default().to_vec());
                // Count it unread until the sheet has been checked for a single valid record.
                return Ok(false);
            }
            0x00EF => {
                let font = c.u16("PhoneticInfo").map_err(e)?;
                let flags = c.u16("PhoneticInfo").map_err(e)?;
                let count = usize::from(c.u16("PhoneticInfo").map_err(e)?);
                for _ in 0..count {
                    let first_row = c.u16("PhoneticInfo range").map_err(e)?;
                    let last_row = c.u16("PhoneticInfo range").map_err(e)?;
                    let first_col = c.u16("PhoneticInfo range").map_err(e)?;
                    let last_col = c.u16("PhoneticInfo range").map_err(e)?;
                    if first_row > last_row || first_col > last_col || last_col > 255 {
                        return Err("an invalid PhoneticInfo range".to_owned());
                    }
                    self.phonetic_ranges.push((first_row, last_row, first_col, last_col));
                }
                let Some(font) = styles.font_index(font).or((font == 0).then_some(0)) else { return Ok(false) };
                self.phonetic = Some((font, (flags & 3) as u8, ((flags >> 2) & 3) as u8));
                // Existing cells receive visibility; unstored cells in the range do not yet.
                if count != 0 { return Ok(false); }
            }
            0x0080 => {
                // Old writers sometimes omit both outline-level words. Retain the
                // record as unread rather than inventing levels from the gutter widths.
                if self.v5.is_some() && record.body.len() == 4 { return Ok(false); }
                c.skip(4, "Guts").map_err(e)?;
                let level = |value: u16| -> Result<u8, String> {
                    match value { 0 => Ok(0), 2..=8 => Ok((value - 1) as u8),
                        _ => Err(format!("the outline gutter level {value}")) }
                };
                self.outline_levels = Some((level(c.u16("Guts").map_err(e)?)?, level(c.u16("Guts").map_err(e)?)?));
            }
            H_PAGE_BREAKS | V_PAGE_BREAKS => {
                let count = c.u16("PageBreaks").map_err(e)?;
                let mut breaks = Vec::new();
                for _ in 0..count {
                    let at = c.u16("PageBreaks").map_err(e)?;
                    let (start, end) = if self.v5.is_some() {
                        // BIFF5/7 stores only the index; each break crosses the whole sheet.
                        (0, if record.id == H_PAGE_BREAKS { 255 } else { 65535 })
                    } else {
                        (c.u16("PageBreaks").map_err(e)?, c.u16("PageBreaks").map_err(e)?)
                    };
                    breaks.push((at, start, end));
                }
                if record.id == H_PAGE_BREAKS {
                    self.row_breaks = breaks;
                } else {
                    self.column_breaks = breaks;
                }
            }
            CODE_NAME => {
                let mut p = Pieces::new(record);
                if let Ok(units) = p.unicode_string("a code name") {
                    self.code_name = Some(String::from_utf16_lossy(&units));
                }
            }
            OBJ => {
                // Sub-records: type, size, data; the common object data (0x15) holds the object
                // type and id.
                while c.remaining() >= 4 {
                    let kind = c.u16("Obj").map_err(e)?;
                    let size = usize::from(c.u16("Obj").map_err(e)?);
                    if kind == 0 {
                        break;
                    }
                    let data = c.bytes(size.min(c.remaining()), "Obj").map_err(e)?;
                    if kind == 0x0015 && data.len() >= 4 {
                        let object_type = u16::from_le_bytes([data[0], data[1]]);
                        let id = u16::from_le_bytes([data[2], data[3]]);
                        self.waiting_object = (object_type == 0x0019).then_some(id);
                    }
                }
            }
            TXO => {
                if let Some(id) = self.waiting_object.take() {
                    if let Some(units) = super::strings::txo_text(record).map_err(|m| m.0)? {
                        self.note_runs.remove(&id);
                        if let Some(runs) = super::strings::txo_runs(record, &units)
                            && runs.iter().all(|&(_, font)| font != 4 && styles.font_index(font).is_some()) {
                            self.note_runs.insert(id, runs);
                        }
                        self.note_texts.insert(id, units);
                    }
                }
            }
            NOTE if self.v5.is_some() => {
                // BIFF5: the text is in the record. A row of 0xFFFF continues the note before.
                let row = c.u16("Note").map_err(e)?;
                let column = c.u16("Note").map_err(e)?;
                let count = usize::from(c.u16("Note").map_err(e)?);
                let bytes = c.bytes(count.min(c.remaining()), "Note").map_err(e)?;
                let text: Vec<u16> = super::codepage::decode(self.v5.unwrap_or(1252), bytes)?
                    .encode_utf16()
                    .collect();
                if row == 0xFFFF {
                    if let Some(last) = self.notes.last() {
                        self.note_texts.entry(last.object).or_default().extend(text);
                    }
                } else {
                    let object = self.notes.len() as u16 + 1;
                    self.note_texts.insert(object, text);
                    self.notes.push(Note {
                        row,
                        column,
                        visible: false,
                        object,
                        author: String::new(),
                    });
                }
            }
            NOTE => {
                let row = c.u16("Note").map_err(e)?;
                let column = c.u16("Note").map_err(e)?;
                let flags = c.u16("Note").map_err(e)?;
                let object = c.u16("Note").map_err(e)?;
                let mut p = Pieces::new(record);
                p.skip(8, "Note").map_err(|m| m.0)?;
                let author = if p.done() {
                    String::new()
                } else {
                    String::from_utf16_lossy(&p.unicode_string("a note's author").map_err(|m| m.0)?)
                };
                self.notes.push(Note {
                    row,
                    column,
                    visible: flags & 0x0002 != 0,
                    object,
                    author,
                });
            }
            FEAT_HEADER if !self.macro_sheet && record.continues.is_empty() => {
                let b = record.body;
                if b.len() != 19 || b.get(..2) != Some(&[0x67, 0x08]) || b[2] & 3 != 0 || b.get(12..14) != Some(&[3, 0])
                    || b.get(14) != Some(&1) || b.get(15..19) != Some(&[0, 0, 0, 0]) {
                    return Ok(false);
                }
            }
            FEAT if !self.macro_sheet && record.continues.is_empty() => {
                // ISFFEC2: one FFErrorCheck bit per kind of ignored error, with Ref8U ranges.
                let b = record.body;
                if b.len() < 31 || b.get(..2) != Some(&[0x68, 0x08]) || b[2] & 3 != 0 || b.get(12..14) != Some(&[3, 0])
                    || b.get(14..19) != Some(&[0, 0, 0, 0, 0]) || b.get(25..27) != Some(&[0, 0]) {
                    return Ok(false);
                }
                let count = usize::from(u16::from_le_bytes([b[19], b[20]]));
                let size = u32::from_le_bytes([b[21], b[22], b[23], b[24]]);
                let Some(end) = count.checked_mul(8).and_then(|n| 27usize.checked_add(n)) else { return Ok(false) };
                if count == 0 || size != 4 || end.checked_add(4) != Some(b.len()) { return Ok(false); }
                let mut ranges = Vec::with_capacity(count);
                for r in b[27..end].chunks_exact(8) {
                    let first_row = u16::from_le_bytes([r[0], r[1]]);
                    let last_row = u16::from_le_bytes([r[2], r[3]]);
                    let first_col = u16::from_le_bytes([r[4], r[5]]);
                    let last_col = u16::from_le_bytes([r[6], r[7]]);
                    if first_row > last_row || first_col > last_col || last_col > 255 { return Ok(false); }
                    ranges.push(range((first_row, last_row), (first_col, last_col)));
                }
                let flags = u32::from_le_bytes(b[end..end + 4].try_into().expect("four bytes"));
                self.ignored_errors.push((ranges, flags & 0xFF));
            }
            DEF_COL_WIDTH => self.default_column_width = Some(c.u16("DefColWidth").map_err(e)?),
            STANDARD_WIDTH if record.body.len() == 2 => {
                self.standard_column_width = Some(c.u16("DxGCol").map_err(e)?);
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    pub(crate) fn show_phonetic(&self, row: u16, column: u16) -> bool {
        self.phonetic_ranges.iter().any(|&(r1, r2, c1, c2)| (r1..=r2).contains(&row) && (c1..=c2).contains(&column))
    }

    /// A single length-consistent DEVMODEA or DEVMODEW; ambiguous or short records stay unread.
    pub(crate) fn printer_part(&self) -> Option<(String, &'static str, Vec<u8>)> {
        let [data] = self.printer_settings.as_slice() else { return None };
        let length_consistent = [36usize, 68].into_iter().any(|offset| {
            let Some(bytes) = data.get(offset..offset + 4) else { return false };
            let size = usize::from(u16::from_le_bytes([bytes[0], bytes[1]]));
            let extra = usize::from(u16::from_le_bytes([bytes[2], bytes[3]]));
            size >= offset + 4 && size.checked_add(extra) == Some(data.len())
        });
        length_consistent.then(|| (format!("xl/printerSettings/printerSettings{}.bin", self.number),
            "application/vnd.openxmlformats-officedocument.spreadsheetml.printerSettings", data.clone()))
    }

    /// HLINK (MS-XLS 2.4.140, with the HLink object of MS-OSHARED 2.3.7.1).
    fn hyperlink(&mut self, c: &mut Cursor<'_>) -> Result<(), String> {
        let what = "an HLINK record";
        let rows = (c.u16(what).map_err(e)?, c.u16(what).map_err(e)?);
        let columns = (c.u16(what).map_err(e)?, c.u16(what).map_err(e)?);
        c.skip(16, what).map_err(e)?;
        let _version = c.u32(what).map_err(e)?;
        let flags = c.u32(what).map_err(e)?;
        let mut link = Hyperlink {
            range: range(rows, columns),
            target: None,
            location: None,
            display: None,
            tooltip: None,
            first: (rows.0, columns.0),
        };
        if flags & 0x10 != 0 {
            let units = c.u32(what).map_err(e)? as usize;
            link.display = Some(read_string(c, units, what)?);
        }
        if flags & 0x80 != 0 {
            let units = c.u32(what).map_err(e)? as usize;
            let _frame = read_string(c, units, what)?;
        }
        if flags & 0x01 != 0 {
            if flags & 0x100 != 0 {
                let units = c.u32(what).map_err(e)? as usize;
                link.target = Some(read_string(c, units, what)?);
            } else {
                let clsid = c.bytes(16, what).map_err(e)?.to_vec();
                const URL: [u8; 16] = [
                    0xE0, 0xC9, 0xEA, 0x79, 0xF9, 0xBA, 0xCE, 0x11, 0x8C, 0x82, 0x00, 0xAA, 0x00,
                    0x4B, 0xA9, 0x0B,
                ];
                const FILE: [u8; 16] = [
                    0x03, 0x03, 0, 0, 0, 0, 0, 0, 0xC0, 0, 0, 0, 0, 0, 0, 0x46,
                ];
                if clsid == URL {
                    // A length in bytes, then the URL as UTF-16 with its NUL, then optional extras.
                    let length = c.u32(what).map_err(e)? as usize;
                    let units = length.min(c.remaining()) / 2;
                    link.target = Some(read_string(c, units, what)?);
                } else if clsid == FILE {
                    let up = usize::from(c.u16(what).map_err(e)?);
                    let length = c.u32(what).map_err(e)? as usize;
                    let bytes = c.bytes(length, what).map_err(e)?;
                    let ansi_end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
                    let mut path = super::codepage::decode(1252, &bytes[..ansi_end])?;
                    // FileMoniker's fixed tail precedes both its optional Unicode path and
                    // the hyperlink location. The Unicode path, when present, is complete.
                    c.skip(24, what).map_err(e)?;
                    let unicode_size = c.u32(what).map_err(e)? as usize;
                    if unicode_size != 0 {
                        let bytes = c.u32(what).map_err(e)? as usize;
                        let key = c.u16(what).map_err(e)?;
                        if bytes % 2 != 0 || unicode_size != bytes + 6 || key != 3 {
                            return Err("an invalid Unicode file moniker path".into());
                        }
                        path = read_string(c, bytes / 2, what)?;
                    }
                    if up != 0 {
                        if path.starts_with('/') || path.starts_with('\\') || path.as_bytes().get(1) == Some(&b':') {
                            return Err("an absolute file moniker path with parent indicators".into());
                        }
                        let mut rest = path.as_str();
                        let mut present = 0usize;
                        while let Some(next) = rest.strip_prefix("../").or_else(|| rest.strip_prefix("..\\")) {
                            present += 1;
                            rest = next;
                        }
                        if present < up { path = format!("{}{}", "../".repeat(up - present), path); }
                    }
                    link.target = Some(path);
                } else {
                    return Err("a hyperlink with a moniker of a kind other than URL or file".into());
                }
            }
        }
        if flags & 0x08 != 0 {
            let units = c.u32(what).map_err(e)? as usize;
            link.location = Some(read_string(c, units, what)?);
        }
        self.hyperlinks.push(link);
        Ok(())
    }

    /// `<sheetPr>`, when the sheet has outline or page setup properties to state.
    pub(crate) fn write_properties(&self, x: &mut Xml, has_active_filter: bool) {
        let flags = self.ws_bool.unwrap_or(0x00C0);
        let summary_below = flags & 0x0040 != 0;
        let summary_right = flags & 0x0080 != 0;
        let fit = flags & 0x0100 != 0;
        let filter_mode = self.filter_mode == Some(true) && has_active_filter;
        if summary_below && summary_right && !fit && self.code_name.is_none() && !filter_mode && self.tab_color.is_none() {
            return;
        }
        let mut properties = Vec::new();
        if filter_mode { properties.push(("filterMode", "1".to_owned())); }
        if let Some(code_name) = &self.code_name {
            properties.push(("codeName", code_name.clone()));
        }
        x.open("sheetPr", &properties);
        if let Some(color) = &self.tab_color { x.empty("tabColor", &[("rgb", color.clone())]); }
        if !summary_below || !summary_right {
            let mut a = Vec::new();
            if !summary_below {
                a.push(("summaryBelow", "0".to_owned()));
            }
            if !summary_right {
                a.push(("summaryRight", "0".to_owned()));
            }
            x.empty("outlinePr", &a);
        }
        if fit {
            x.empty("pageSetUpPr", &[("fitToPage", "1".to_owned())]);
        }
        x.close("sheetPr");
    }

    /// `<sheetViews>`.
    pub(crate) fn write_views(&self, x: &mut Xml) {
        let Some((flags, top, left, color, zoom_page, zoom_normal)) = self.window else { return };
        let mut a: Vec<(&str, String)> = Vec::new();
        if flags & 0x0001 != 0 {
            a.push(("showFormulas", "1".to_owned()));
        }
        if flags & 0x0002 == 0 {
            a.push(("showGridLines", "0".to_owned()));
        }
        if flags & 0x0004 == 0 {
            a.push(("showRowColHeaders", "0".to_owned()));
        }
        if flags & 0x0010 == 0 {
            a.push(("showZeros", "0".to_owned()));
        }
        if flags & 0x0040 != 0 {
            a.push(("rightToLeft", "1".to_owned()));
        }
        if flags & 0x0080 == 0 {
            a.push(("showOutlineSymbols", "0".to_owned()));
        }
        if flags & 0x0020 == 0 && self.grid_color_representable {
            if let Some(color) = color.filter(|value| *value < 64) {
                a.push(("defaultGridColor", "0".to_owned()));
                if color != 64 { a.push(("colorId", color.to_string())); }
            }
        }
        if flags & 0x0800 != 0 {
            a.push(("view", "pageBreakPreview".to_owned()));
        }
        if flags & 0x0200 != 0 {
            a.push(("tabSelected", "1".to_owned()));
        }
        if let Some((numerator, denominator)) = self.zoom
            && denominator != 0
            && numerator != denominator
        {
            a.push(("zoomScale", (u32::from(numerator) * 100 / u32::from(denominator)).to_string()));
        }
        if let Some(scale) = zoom_page.filter(|value| (10..=400).contains(value))
            && (flags & 0x0800 == 0 || self.zoom.is_none()) {
            a.push(("zoomScaleSheetLayoutView", scale.to_string()));
        }
        if let Some(scale) = zoom_normal.filter(|value| (10..=400).contains(value))
            && (flags & 0x0800 != 0 || self.zoom.is_none()) {
            a.push(("zoomScaleNormal", scale.to_string()));
        }
        if (top, left) != (0, 0) {
            a.push(("topLeftCell", cell(top, left)));
        }
        a.push(("workbookViewId", "0".to_owned()));
        x.open("sheetViews", &[]);
        x.open("sheetView", &a);
        let panes = ["bottomRight", "topRight", "bottomLeft", "topLeft"];
        if let Some(pane) = &self.pane {
            let frozen = flags & 0x0008 != 0;
            let mut p = Vec::new();
            if pane.x != 0 {
                p.push(("xSplit", pane.x.to_string()));
            }
            if pane.y != 0 {
                p.push(("ySplit", pane.y.to_string()));
            }
            p.push(("topLeftCell", cell(pane.top, pane.left)));
            p.push(("activePane", panes[usize::from(pane.active & 3)].to_owned()));
            p.push(("state", if frozen { "frozen" } else { "split" }.to_owned()));
            x.empty("pane", &p);
        }
        for selection in &self.selections {
            let mut s = Vec::new();
            if self.pane.is_some() {
                s.push(("pane", panes[usize::from(selection.pane & 3)].to_owned()));
            }
            s.push(("activeCell", cell(selection.active.0, selection.active.1)));
            s.push(("sqref", selection.ranges.join(" ")));
            x.empty("selection", &s);
        }
        x.close("sheetView");
        x.close("sheetViews");
    }

    pub(crate) fn write_header_footer(&self, x: &mut Xml) {
        if self.header.as_deref().is_some_and(|h| !h.is_empty()) || self.footer.as_deref().is_some_and(|f| !f.is_empty()) {
            x.open("headerFooter", &[]);
            if let Some(h) = self.header.as_deref().filter(|h| !h.is_empty()) { x.element("oddHeader", &[], h); }
            if let Some(f) = self.footer.as_deref().filter(|f| !f.is_empty()) { x.element("oddFooter", &[], f); }
            x.close("headerFooter");
        }
    }

    /// The elements after `<sheetData>`; hyperlinks add relationships to `rels`.
    pub(crate) fn write_after_data(
        &self,
        x: &mut Xml,
        rels: &mut Vec<Rel>,
        drawing: Option<&str>,
        rules: &super::rules::Rules,
        filter_range: Option<&str>,
        scenarios: Option<&super::scenario::Scenarios>,
    ) {
        if self.protect || self.objects || self.scenarios {
            let mut a = Vec::new();
            if let Some(hash) = self.password {
                a.push(("password", format!("{hash:04X}")));
            }
            if self.protect {
                a.push(("sheet", "1".to_owned()));
            }
            if self.objects {
                a.push(("objects", "1".to_owned()));
            }
            if self.scenarios {
                a.push(("scenarios", "1".to_owned()));
            }
            x.empty("sheetProtection", &a);
        }
        if let Some(scenarios) = scenarios { scenarios.write(x); }
        rules.write_auto_filter(x, filter_range);
        if !self.merges.is_empty() {
            x.open("mergeCells", &[("count", self.merges.len().to_string())]);
            for m in &self.merges {
                x.empty("mergeCell", &[("ref", m.clone())]);
            }
            x.close("mergeCells");
        }
        if let Some((font, kind, alignment)) = self.phonetic {
            let kind = ["halfwidthKatakana", "fullwidthKatakana", "Hiragana", "noConversion"][kind as usize];
            let alignment = ["noControl", "left", "center", "distributed"][alignment as usize];
            x.empty("phoneticPr", &[("fontId", font.to_string()), ("type", kind.to_owned()), ("alignment", alignment.to_owned())]);
        }
        rules.write_rules(x);
        if !self.hyperlinks.is_empty() {
            x.open("hyperlinks", &[]);
            for link in &self.hyperlinks {
                let mut a = vec![("ref", link.range.clone())];
                if let Some(target) = &link.target {
                    let id = format!("rIdLink{}", rels.len() + 1);
                    rels.push(Rel {
                        id: id.clone(),
                        kind: "hyperlink",
                        target: target.clone(),
                        external: true,
                    });
                    a.push(("r:id", id));
                }
                if let Some(location) = &link.location {
                    a.push(("location", location.clone()));
                }
                if let Some(tooltip) = &link.tooltip {
                    a.push(("tooltip", tooltip.clone()));
                }
                if let Some(display) = &link.display {
                    a.push(("display", display.clone()));
                }
                let _ = link.first;
                x.empty("hyperlink", &a);
            }
            x.close("hyperlinks");
        }
        if self.h_center || self.v_center || self.grid_lines || self.headings {
            let mut a = Vec::new();
            if self.h_center {
                a.push(("horizontalCentered", "1".to_owned()));
            }
            if self.v_center {
                a.push(("verticalCentered", "1".to_owned()));
            }
            if self.headings {
                a.push(("headings", "1".to_owned()));
            }
            if self.grid_lines {
                a.push(("gridLines", "1".to_owned()));
            }
            x.empty("printOptions", &a);
        }
        if self.margins.iter().any(Option::is_some) || self.setup_numbers.is_some() {
            let header = self.setup_numbers.map_or(0.3, |n| n.0);
            let footer = self.setup_numbers.map_or(0.3, |n| n.1);
            x.empty(
                "pageMargins",
                &[
                    ("left", number_text(self.margins[0].unwrap_or(0.75))),
                    ("right", number_text(self.margins[1].unwrap_or(0.75))),
                    ("top", number_text(self.margins[2].unwrap_or(1.0))),
                    ("bottom", number_text(self.margins[3].unwrap_or(1.0))),
                    ("header", number_text(header)),
                    ("footer", number_text(footer)),
                ],
            );
        }
        let printer_id = self.printer_part().map(|_| {
            rels.push(Rel {
                id: "rIdPrinter".to_owned(),
                kind: "printerSettings",
                target: format!("../printerSettings/printerSettings{}.bin", self.number),
                external: false,
            });
            "rIdPrinter"
        });
        if let Some([paper, scale, first_page, fit_width, fit_height, flags]) = self.setup {
            let mut a = Vec::new();
            // fNoPls makes the printer-dependent paper, scale and orientation undefined.
            if flags & 0x0004 == 0 {
                a.push(("paperSize", paper.to_string()));
                if scale != 100 && scale != 0 {
                    a.push(("scale", scale.to_string()));
                }
            } else {
                a.push(("usePrinterDefaults", "1".to_owned()));
            }
            if flags & 0x0080 != 0 && (first_page as i16) > 0 {
                a.push(("firstPageNumber", first_page.to_string()));
                a.push(("useFirstPageNumber", "1".to_owned()));
            }
            if fit_width != 1 {
                a.push(("fitToWidth", fit_width.to_string()));
            }
            if fit_height != 1 {
                a.push(("fitToHeight", fit_height.to_string()));
            }
            if flags & 0x0001 != 0 {
                a.push(("pageOrder", "overThenDown".to_owned()));
            }
            if flags & 0x0004 == 0 {
                a.push((
                    "orientation",
                    if flags & 0x0040 != 0 || flags & 0x0002 != 0 { "portrait" } else { "landscape" }.to_owned(),
                ));
                if let Some((horizontal, vertical, copies)) = self.setup_printer {
                    if horizontal != 0 { a.push(("horizontalDpi", horizontal.to_string())); }
                    if vertical != 0 { a.push(("verticalDpi", vertical.to_string())); }
                    if let Some(copies) = copies.filter(|n| *n != 0) { a.push(("copies", copies.to_string())); }
                }
            }
            if flags & 0x0008 != 0 {
                a.push(("blackAndWhite", "1".to_owned()));
            }
            if flags & 0x0010 != 0 {
                a.push(("draft", "1".to_owned()));
            }
            if flags & 0x0020 != 0 {
                a.push(("cellComments", if flags & 0x0200 != 0 { "atEnd" } else { "asDisplayed" }.to_owned()));
            }
            if let Some(errors) = [None, Some("blank"), Some("dash"), Some("NA")][usize::from((flags >> 10) & 3)] {
                a.push(("errors", errors.to_owned()));
            }
            if let Some(id) = printer_id { a.push(("r:id", id.to_owned())); }
            x.empty("pageSetup", &a);
        } else if let Some(id) = printer_id {
            x.empty("pageSetup", &[("r:id", id.to_owned())]);
        }
        self.write_header_footer(x);
        for (name, breaks, full, full_text) in [
            ("rowBreaks", &self.row_breaks, 255u16, "16383"),
            ("colBreaks", &self.column_breaks, 65535u16, "1048575"),
        ] {
            if breaks.is_empty() {
                continue;
            }
            x.open(
                name,
                &[("count", breaks.len().to_string()), ("manualBreakCount", breaks.len().to_string())],
            );
            for (at, start, end) in breaks {
                x.empty(
                    "brk",
                    &[
                        ("id", at.to_string()),
                        ("min", start.to_string()),
                        ("max", if *end == full { full_text.to_owned() } else { end.to_string() }),
                        ("man", "1".to_owned()),
                    ],
                );
            }
            x.close(name);
        }
        if self.ignored_errors.iter().any(|(_, flags)| *flags != 0) {
            x.open("ignoredErrors", &[]);
            for (ranges, flags) in &self.ignored_errors {
                if *flags == 0 { continue; }
                let mut attributes = vec![("sqref", ranges.join(" "))];
                for (bit, name) in ["evalError", "emptyCellReference", "numberStoredAsText", "formulaRange",
                    "formula", "twoDigitTextYear", "unlockedFormula", "listDataValidation"].into_iter().enumerate() {
                    if flags & (1 << bit) != 0 { attributes.push((name, "1".to_owned())); }
                }
                x.empty("ignoredError", &attributes);
            }
            x.close("ignoredErrors");
        }
        if let Some(id) = drawing {
            x.empty("drawing", &[("r:id", id.to_owned())]);
        }
        let _ = REL;
        if !self.notes.is_empty() {
            rels.push(Rel {
                id: "rIdComments".to_owned(),
                kind: "comments",
                target: format!("../comments{}.xml", self.number),
                external: false,
            });
            rels.push(Rel {
                id: "rIdVml".to_owned(),
                kind: "vmlDrawing",
                target: format!("../drawings/vmlDrawing{}.vml", self.number),
                external: false,
            });
            x.empty("legacyDrawing", &[("r:id", "rIdVml".to_owned())]);
        }
    }

    /// The comments part and the VML drawing that shows each note.
    pub(crate) fn comment_parts(&self, styles: &super::styles::Styles) -> Vec<(String, &'static str, Vec<u8>)> {
        if self.notes.is_empty() {
            return Vec::new();
        }
        let mut authors: Vec<&str> = Vec::new();
        for note in &self.notes {
            if !authors.contains(&note.author.as_str()) {
                authors.push(&note.author);
            }
        }
        let mut x = Xml::new();
        x.open("comments", &[("xmlns", super::MAIN_NAMESPACE.to_owned())]);
        x.open("authors", &[]);
        for author in &authors {
            x.element("author", &[], author);
        }
        x.close("authors");
        x.open("commentList", &[]);
        for note in &self.notes {
            let id = authors.iter().position(|a| *a == note.author).unwrap_or(0);
            x.open(
                "comment",
                &[("ref", cell(note.row, note.column)), ("authorId", id.to_string())],
            );
            x.open("text", &[]);
            let text = self.note_texts.get(&note.object).map_or(&[][..], |t| t);
            if let Some(runs) = self.note_runs.get(&note.object) {
                for (index, &(start, font)) in runs.iter().enumerate() {
                    let end = runs.get(index + 1).map_or(text.len(), |next| usize::from(next.0));
                    x.open("r", &[]);
                    styles.write_run_properties(&mut x, font);
                    super::sheet::text_element(&mut x, &text[usize::from(start)..end]);
                    x.close("r");
                }
            } else {
                super::sheet::text_element(&mut x, text);
            }
            x.close("text");
            x.close("comment");
        }
        x.close("commentList");
        x.close("comments");
        let mut v = String::from(
            "<xml xmlns:v=\"urn:schemas-microsoft-com:vml\" xmlns:o=\"urn:schemas-microsoft-com:office:office\" xmlns:x=\"urn:schemas-microsoft-com:office:excel\">\r\n <o:shapelayout v:ext=\"edit\"><o:idmap v:ext=\"edit\" data=\"1\"/></o:shapelayout>\r\n <v:shapetype id=\"_x0000_t202\" coordsize=\"21600,21600\" o:spt=\"202\" path=\"m,l,21600r21600,l21600,xe\"><v:stroke joinstyle=\"miter\"/><v:path gradientshapeok=\"t\" o:connecttype=\"rect\"/></v:shapetype>\r\n",
        );
        for (k, note) in self.notes.iter().enumerate() {
            v.push_str(&format!(
                " <v:shape id=\"_x0000_s{}\" type=\"#_x0000_t202\" style=\"position:absolute;margin-left:80pt;margin-top:{}pt;width:108pt;height:60pt;z-index:{};visibility:{}\" fillcolor=\"#ffffe1\" o:insetmode=\"auto\"><v:fill color2=\"#ffffe1\"/><v:shadow on=\"t\" color=\"black\" obscured=\"t\"/><v:path o:connecttype=\"none\"/><v:textbox style=\"mso-direction-alternative:auto\"><div style=\"text-align:left\"></div></v:textbox><x:ClientData ObjectType=\"Note\"><x:MoveWithCells/><x:SizeWithCells/><x:AutoFill>False</x:AutoFill><x:Row>{}</x:Row><x:Column>{}</x:Column></x:ClientData></v:shape>\r\n",
                1025 + k,
                15 * u32::from(note.row),
                k + 1,
                if note.visible { "visible" } else { "hidden" },
                note.row,
                note.column
            ));
        }
        v.push_str("</xml>");
        vec![
            (
                format!("xl/comments{}.xml", self.number),
                "application/vnd.openxmlformats-officedocument.spreadsheetml.comments+xml",
                x.out,
            ),
            (
                format!("xl/drawings/vmlDrawing{}.vml", self.number),
                "application/vnd.openxmlformats-officedocument.vmlDrawing",
                v.into_bytes(),
            ),
        ]
    }
}
