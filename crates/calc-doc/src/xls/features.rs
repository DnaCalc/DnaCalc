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
const OBJ: u16 = 0x005D;
const TXO: u16 = 0x01B6;
const NOTE: u16 = 0x001C;

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
    window: Option<(u16, u16, u16)>,
    zoom: Option<(u16, u16)>,
    pane: Option<Pane>,
    selections: Vec<Selection>,
    merges: Vec<String>,
    hyperlinks: Vec<Hyperlink>,
    protect: bool,
    objects: bool,
    scenarios: bool,
    password: Option<u16>,
    setup: Option<[u16; 6]>,
    setup_numbers: Option<(f64, f64)>,
    margins: [Option<f64>; 4],
    header: Option<String>,
    footer: Option<String>,
    h_center: bool,
    v_center: bool,
    grid_lines: bool,
    headings: bool,
    ws_bool: Option<u16>,
    row_breaks: Vec<(u16, u16, u16)>,
    column_breaks: Vec<(u16, u16, u16)>,
    pub(crate) default_column_width: Option<u16>,
    /// The id of the comment object (OBJ of type note) waiting for its text box.
    waiting_object: Option<u16>,
    /// The sheet's number, which names its comment parts.
    pub(crate) number: usize,
    note_texts: std::collections::BTreeMap<u16, Vec<u16>>,
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

impl Features {
    /// Reads `record` when it is a sheet-feature record; `false` when it is not one.
    pub(crate) fn read(&mut self, record: &Record<'_>) -> Result<bool, String> {
        let mut c = Cursor::new(record.body);
        match record.id {
            WINDOW2 => {
                let flags = c.u16("Window2").map_err(e)?;
                let top = c.u16("Window2").map_err(e)?;
                let left = c.u16("Window2").map_err(e)?;
                self.window = Some((flags, top, left));
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
                c.skip(4, "Setup").map_err(e)?;
                self.setup = Some(v);
                self.setup_numbers = Some((c.f64("Setup").map_err(e)?, c.f64("Setup").map_err(e)?));
            }
            LEFT_MARGIN => self.margins[0] = Some(c.f64("LeftMargin").map_err(e)?),
            RIGHT_MARGIN => self.margins[1] = Some(c.f64("RightMargin").map_err(e)?),
            TOP_MARGIN => self.margins[2] = Some(c.f64("TopMargin").map_err(e)?),
            BOTTOM_MARGIN => self.margins[3] = Some(c.f64("BottomMargin").map_err(e)?),
            HEADER | FOOTER => {
                let text = if record.body.is_empty() {
                    String::new()
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
            H_PAGE_BREAKS | V_PAGE_BREAKS => {
                let count = c.u16("PageBreaks").map_err(e)?;
                let mut breaks = Vec::new();
                for _ in 0..count {
                    breaks.push((
                        c.u16("PageBreaks").map_err(e)?,
                        c.u16("PageBreaks").map_err(e)?,
                        c.u16("PageBreaks").map_err(e)?,
                    ));
                }
                if record.id == H_PAGE_BREAKS {
                    self.row_breaks = breaks;
                } else {
                    self.column_breaks = breaks;
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
                    c.skip(10, "Txo").map_err(e)?;
                    let length = usize::from(c.u16("Txo").map_err(e)?);
                    if let Some(text) = record.continues.first() {
                        let mut p = Pieces::new(&Record {
                            id: 0,
                            body: text,
                            continues: Vec::new(),
                            offset: 0,
                        });
                        let flag = p.u8("Txo").map_err(|m| m.0)?;
                        let units = p.characters(length, flag & 1 != 0, "a note's text").map_err(|m| m.0)?;
                        self.note_texts.insert(id, units);
                    }
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
            DEF_COL_WIDTH => self.default_column_width = Some(c.u16("DefColWidth").map_err(e)?),
            _ => return Ok(false),
        }
        Ok(true)
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
                    let _up = c.u16(what).map_err(e)?;
                    let length = c.u32(what).map_err(e)? as usize;
                    let bytes = c.bytes(length, what).map_err(e)?;
                    let path: String = bytes
                        .iter()
                        .take_while(|b| **b != 0)
                        .map(|b| char::from(*b))
                        .collect();
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
    pub(crate) fn write_properties(&self, x: &mut Xml) {
        let Some(flags) = self.ws_bool else { return };
        let summary_below = flags & 0x0040 != 0;
        let summary_right = flags & 0x0080 != 0;
        let fit = flags & 0x0100 != 0;
        if summary_below && summary_right && !fit {
            return;
        }
        x.open("sheetPr", &[]);
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
        let Some((flags, top, left)) = self.window else { return };
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
        if flags & 0x0200 != 0 {
            a.push(("tabSelected", "1".to_owned()));
        }
        if let Some((numerator, denominator)) = self.zoom
            && denominator != 0
            && numerator != denominator
        {
            a.push(("zoomScale", (u32::from(numerator) * 100 / u32::from(denominator)).to_string()));
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

    /// The elements after `<sheetData>`; hyperlinks add relationships to `rels`.
    pub(crate) fn write_after_data(&self, x: &mut Xml, rels: &mut Vec<Rel>) {
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
        if !self.merges.is_empty() {
            x.open("mergeCells", &[("count", self.merges.len().to_string())]);
            for m in &self.merges {
                x.empty("mergeCell", &[("ref", m.clone())]);
            }
            x.close("mergeCells");
        }
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
        if let Some([paper, scale, first_page, fit_width, fit_height, flags]) = self.setup {
            let mut a = vec![("paperSize", paper.to_string())];
            if scale != 100 && scale != 0 {
                a.push(("scale", scale.to_string()));
            }
            if flags & 0x0080 != 0 {
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
                    if flags & 0x0002 != 0 { "portrait" } else { "landscape" }.to_owned(),
                ));
            }
            if flags & 0x0008 != 0 {
                a.push(("blackAndWhite", "1".to_owned()));
            }
            if flags & 0x0010 != 0 {
                a.push(("draft", "1".to_owned()));
            }
            if flags & 0x0020 != 0 {
                a.push(("cellComments", "atEnd".to_owned()));
            }
            x.empty("pageSetup", &a);
        }
        if self.header.as_deref().is_some_and(|h| !h.is_empty()) || self.footer.as_deref().is_some_and(|f| !f.is_empty()) {
            x.open("headerFooter", &[]);
            if let Some(h) = self.header.as_deref().filter(|h| !h.is_empty()) {
                x.element("oddHeader", &[], h);
            }
            if let Some(f) = self.footer.as_deref().filter(|f| !f.is_empty()) {
                x.element("oddFooter", &[], f);
            }
            x.close("headerFooter");
        }
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
    pub(crate) fn comment_parts(&self) -> Vec<(String, &'static str, Vec<u8>)> {
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
            super::sheet::text_element(&mut x, self.note_texts.get(&note.object).map_or(&[][..], |t| t));
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
