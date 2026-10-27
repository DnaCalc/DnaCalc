//! A worksheet part of an Excel binary workbook (MS-XLSB 2.1.7.62), written as the SpreadsheetML
//! worksheet part of the same content: sheet properties, dimension, views with their panes and
//! selections, format defaults, columns, the cells (values, formulas with their shared, array and
//! data table forms, cell and value metadata), protection, the autoFilter's area, merged cells,
//! conditional formats, data validations, hyperlinks, print settings, drawings and table parts.
//!
//! Not carried: an autoFilter's filter criteria and sort state, a sheet's custom views, row and
//! column page breaks, ignored-error ranges, cell watches, phonetic properties, and the Excel 2010
//! extensions (data bars' and icon sets' newer properties, data validations whose formulas name
//! other sheets), which the binary form keeps in future record types.

use super::formula::{self, Expanded, Origin};
use super::parts::{Shared, area_text, guid, rich_text, write_rich};
use super::records::{Area0, Cursor, Malformed, Read, Record, records, text};
use super::styles::color;
use super::{MAIN_NAMESPACE, RELATIONSHIPS_NAMESPACE, Xml, refusal};
use crate::OpenError;
use crate::save::text::a1;
use std::collections::BTreeMap;

const BRT_ROW_HDR: u16 = 0;
const BRT_CELL_BLANK: u16 = 1;
const BRT_CELL_RK: u16 = 2;
const BRT_CELL_ERROR: u16 = 3;
const BRT_CELL_BOOL: u16 = 4;
const BRT_CELL_REAL: u16 = 5;
const BRT_CELL_ST: u16 = 6;
const BRT_CELL_ISST: u16 = 7;
const BRT_FMLA_STRING: u16 = 8;
const BRT_FMLA_NUM: u16 = 9;
const BRT_FMLA_BOOL: u16 = 10;
const BRT_FMLA_ERROR: u16 = 11;
const BRT_CELL_META: u16 = 49;
const BRT_VALUE_META: u16 = 50;
const BRT_CELL_RSTRING: u16 = 62;
const BRT_COL_INFO: u16 = 60;
const BRT_DVAL: u16 = 64;
const BRT_BEGIN_WS_VIEWS: u16 = 133;
const BRT_END_WS_VIEWS: u16 = 134;
const BRT_BEGIN_WS_VIEW: u16 = 137;
const BRT_END_WS_VIEW: u16 = 138;
const BRT_WS_PROP: u16 = 147;
const BRT_WS_DIM: u16 = 148;
const BRT_PANE: u16 = 151;
const BRT_SEL: u16 = 152;
const BRT_BEGIN_AFILTER: u16 = 161;
const BRT_MERGE_CELL: u16 = 176;
const BRT_BEGIN_RW_BRK: u16 = 392;
const BRT_END_RW_BRK: u16 = 393;
const BRT_BEGIN_COL_BRK: u16 = 394;
const BRT_END_COL_BRK: u16 = 395;
const BRT_BRK: u16 = 396;
const BRT_ARR_FMLA: u16 = 426;
const BRT_SHR_FMLA: u16 = 427;
const BRT_TABLE: u16 = 428;
const BRT_BEGIN_CONDITIONAL_FORMATTING: u16 = 461;
const BRT_END_CONDITIONAL_FORMATTING: u16 = 462;
const BRT_BEGIN_CF_RULE: u16 = 463;
const BRT_END_CF_RULE: u16 = 464;
const BRT_BEGIN_ICON_SET: u16 = 465;
const BRT_END_ICON_SET: u16 = 466;
const BRT_BEGIN_DATABAR: u16 = 467;
const BRT_END_DATABAR: u16 = 468;
const BRT_BEGIN_COLOR_SCALE: u16 = 469;
const BRT_END_COLOR_SCALE: u16 = 470;
const BRT_CFVO: u16 = 471;
const BRT_MARGINS: u16 = 476;
const BRT_PRINT_OPTIONS: u16 = 477;
const BRT_PAGE_SETUP: u16 = 478;
const BRT_BEGIN_HEADER_FOOTER: u16 = 479;
const BRT_WS_FMT_INFO: u16 = 485;
const BRT_HLINK: u16 = 494;
const BRT_SHEET_PROTECTION: u16 = 535;
const BRT_PHONETIC_INFO: u16 = 537;
const BRT_DRAWING: u16 = 550;
const BRT_LEGACY_DRAWING: u16 = 551;
const BRT_COLOR: u16 = 564;
const BRT_BEGIN_DVALS: u16 = 573;
const BRT_LIST_PART: u16 = 661;
const BRT_FRT_GUID: u16 = 3072;

const REVISION_NAMESPACE: &str = "http://schemas.microsoft.com/office/spreadsheetml/2014/revision";
const COMPATIBILITY_NAMESPACE: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";

pub(crate) fn convert(
    part: &str,
    data: &[u8],
    tab: Option<usize>,
    shared: &Shared,
) -> Result<Vec<u8>, OpenError> {
    write(data, tab.map(|t| t as u32), shared).map_err(|error| refusal(part, error.0))
}

/// The sections of the worksheet, each written as its records come and put together in the
/// schema's order at the end.
#[derive(Default)]
struct Sections {
    properties: Vec<u8>,
    dimension: Vec<u8>,
    views: Vec<u8>,
    format: Vec<u8>,
    columns: Vec<u8>,
    data: Vec<u8>,
    protection: Vec<u8>,
    filter: Vec<u8>,
    merges: Vec<String>,
    phonetic: Vec<u8>,
    conditional: Vec<u8>,
    validations: Vec<u8>,
    validation_count: u32,
    hyperlinks: Vec<u8>,
    print_options: Vec<u8>,
    margins: Vec<u8>,
    page_setup: Vec<u8>,
    header_footer: Vec<u8>,
    breaks: Vec<u8>,
    drawing: Vec<u8>,
    legacy_drawing: Vec<u8>,
    tables: Vec<String>,
}

/// A fragment writer: an [`Xml`] without the declaration.
fn fragment() -> Xml {
    Xml { out: Vec::new() }
}

fn number(value: f64) -> String {
    formula::number_text(value)
}

fn flag(set: bool) -> String {
    if set { "1" } else { "0" }.to_owned()
}

fn sqref(c: &mut Cursor<'_>, what: &str) -> Read<(Vec<Area0>, String)> {
    let count = c.u32(what)?;
    let mut areas = Vec::new();
    for _ in 0..count {
        areas.push(c.rfx(what)?);
    }
    let text = areas.iter().map(area_text).collect::<Vec<_>>().join(" ");
    Ok((areas, text))
}

/// A cell's column, style and phonetic flag (Cell, 2.5.10).
struct Cell {
    column: u32,
    style: u32,
    phonetic: bool,
}

fn cell(c: &mut Cursor<'_>) -> Read<Cell> {
    let column = c.u32("a cell")?;
    let packed = c.u32("a cell")?;
    Ok(Cell {
        column,
        style: packed & 0x00FF_FFFF,
        phonetic: packed & 0x0100_0000 != 0,
    })
}

/// A formula as a cell record holds it (CellParsedFormula).
fn parsed<'a>(c: &mut Cursor<'a>, what: &str) -> Read<(&'a [u8], &'a [u8])> {
    let cce = c.u32(what)? as usize;
    let rgce = c.bytes(cce, what)?;
    let cb = c.u32(what)? as usize;
    let rgcb = c.bytes(cb, what)?;
    Ok((rgce, rgcb))
}

/// What the cells of a shared formula, an array formula or a data table name as their formula.
#[derive(Clone, Copy)]
enum Group {
    /// A shared formula: its index.
    Shared(u32),
    /// An array formula: its members hold values only.
    Array,
    /// A data table: its members hold values only.
    DataTable,
}

struct Writer<'s> {
    shared: &'s Shared,
    sheet: Option<u32>,
    /// The groups by their top-left cell (0-based row and column).
    groups: BTreeMap<(u32, u32), Group>,
    next_shared: u32,
    row: Option<u32>,
    cell_meta: Option<u32>,
    value_meta: Option<u32>,
    /// The sheet's default row height in twips (BrtWsFmtInfo): a row of another height is written
    /// with it.
    default_height: u16,
}

fn write(data: &[u8], sheet: Option<u32>, shared: &Shared) -> Read<Vec<u8>> {
    let records = records(data)?;
    let mut s = Sections::default();
    let mut w = Writer {
        shared,
        sheet,
        groups: BTreeMap::new(),
        next_shared: 0,
        row: None,
        cell_meta: None,
        value_meta: None,
        default_height: 300,
    };
    let mut d = fragment();
    let mut in_data = false;
    let mut guid_pending: Option<String> = None;
    // An open sheet view: its attributes and its children.
    let mut view: Option<(Vec<(&'static str, String)>, Xml)> = None;
    // An open conditional format and rule.
    let mut format_open = false;
    let mut rule: Option<Rule> = None;
    let mut breaks_open = false;
    let mut i = 0;
    while i < records.len() {
        let record = records[i];
        let mut c = Cursor::new(record.body);
        match record.kind {
            BRT_FRT_GUID => guid_pending = guid(record.body),
            BRT_WS_PROP => s.properties = sheet_properties(&mut c)?,
            BRT_WS_DIM => {
                let area = c.rfx("BrtWsDim")?;
                let mut x = fragment();
                x.empty("dimension", &[("ref", area_text(&area))]);
                s.dimension = x.out;
            }
            BRT_BEGIN_WS_VIEWS => {}
            BRT_BEGIN_WS_VIEW => view = Some((sheet_view(&mut c)?, fragment())),
            BRT_PANE => {
                if let Some((_, children)) = &mut view {
                    pane(&mut c, children)?;
                }
            }
            BRT_SEL => {
                if let Some((_, children)) = &mut view {
                    selection(&mut c, children)?;
                }
            }
            BRT_END_WS_VIEW => {
                if let Some((attributes, children)) = view.take() {
                    let mut x = fragment();
                    if children.out.is_empty() {
                        x.empty("sheetView", &attributes);
                    } else {
                        x.open("sheetView", &attributes);
                        x.out.extend_from_slice(&children.out);
                        x.close("sheetView");
                    }
                    s.views.extend_from_slice(&x.out);
                }
            }
            BRT_END_WS_VIEWS => {}
            BRT_WS_FMT_INFO => {
                if let Some(height) = record.body.get(6..8) {
                    w.default_height = u16::from_le_bytes([height[0], height[1]]);
                }
                s.format = format_info(&mut c)?;
            }
            BRT_COL_INFO => column_info(&mut c, &mut s.columns)?,
            145 => in_data = true,
            146 => {
                if w.row.take().is_some() {
                    d.close("row");
                }
                in_data = false;
            }
            BRT_ROW_HDR if in_data => w.row_header(&mut c, &mut d)?,
            BRT_CELL_META => w.cell_meta = Some(c.u32("BrtCellMeta")?),
            BRT_VALUE_META => w.value_meta = Some(c.u32("BrtValueMeta")?),
            BRT_CELL_BLANK..=BRT_FMLA_ERROR | BRT_CELL_RSTRING if in_data => {
                w.cell(record, &records[i + 1..], &mut d)?;
            }
            BRT_SHR_FMLA | BRT_ARR_FMLA | BRT_TABLE => {
                // Read with the cell they follow.
            }
            BRT_SHEET_PROTECTION => s.protection = protection(&mut c)?,
            BRT_BEGIN_AFILTER => {
                let area = c.rfx("BrtBeginAFilter")?;
                let mut x = fragment();
                x.empty("autoFilter", &[("ref", area_text(&area))]);
                s.filter = x.out;
            }
            BRT_MERGE_CELL => s.merges.push(area_text(&c.rfx("BrtMergeCell")?)),
            BRT_PHONETIC_INFO => {
                let font = c.u16("BrtPhoneticInfo")?;
                let kind = c.u32("BrtPhoneticInfo")? & 0x3;
                let alignment = c.u32("BrtPhoneticInfo")? & 0x3;
                let mut attributes = vec![("fontId", font.to_string())];
                if kind != 1 {
                    let name = [
                        "halfwidthKatakana",
                        "fullwidthKatakana",
                        "Hiragana",
                        "noConversion",
                    ];
                    attributes.push(("type", name[kind as usize].to_owned()));
                }
                if alignment != 1 {
                    let name = ["noControl", "left", "center", "distributed"];
                    attributes.push(("alignment", name[alignment as usize].to_owned()));
                }
                let mut x = fragment();
                x.empty("phoneticPr", &attributes);
                s.phonetic = x.out;
            }
            BRT_BEGIN_CONDITIONAL_FORMATTING => {
                let _count = c.u32("BrtBeginConditionalFormatting")?;
                let pivot = c.u32("BrtBeginConditionalFormatting")? != 0;
                let (_, text) = sqref(&mut c, "BrtBeginConditionalFormatting")?;
                let mut x = fragment();
                let mut attributes = Vec::new();
                if pivot {
                    attributes.push(("pivot", "1".to_owned()));
                }
                attributes.push(("sqref", text));
                x.open("conditionalFormatting", &attributes);
                s.conditional.extend_from_slice(&x.out);
                format_open = true;
            }
            BRT_BEGIN_CF_RULE => {
                let origin = conditional_origin(&records[..i]);
                rule = Some(Rule::read(&mut c, &w.shared.context(w.sheet), origin)?);
            }
            BRT_BEGIN_COLOR_SCALE | BRT_BEGIN_DATABAR | BRT_BEGIN_ICON_SET => {
                if let Some(rule) = &mut rule {
                    rule.visual = Some((record.kind, record.body.to_vec()));
                }
            }
            BRT_CFVO => {
                if let Some(rule) = &mut rule {
                    let origin = conditional_origin(&records[..i]);
                    rule.values
                        .push(cfvo(&mut c, &w.shared.context(w.sheet), origin)?);
                }
            }
            BRT_COLOR => {
                if let Some(rule) = &mut rule {
                    rule.colors.push(color(&mut c)?);
                }
            }
            BRT_END_COLOR_SCALE | BRT_END_DATABAR | BRT_END_ICON_SET => {}
            BRT_END_CF_RULE => {
                if let Some(rule) = rule.take() {
                    let mut x = fragment();
                    rule.write(&mut x)?;
                    s.conditional.extend_from_slice(&x.out);
                }
            }
            BRT_END_CONDITIONAL_FORMATTING => {
                if format_open {
                    let mut x = fragment();
                    x.close("conditionalFormatting");
                    s.conditional.extend_from_slice(&x.out);
                    format_open = false;
                }
            }
            BRT_BEGIN_DVALS => {}
            BRT_DVAL => {
                let mut x = fragment();
                validation(&mut c, &mut x, &w.shared.context(w.sheet))?;
                s.validations.extend_from_slice(&x.out);
                s.validation_count += 1;
            }
            BRT_HLINK => {
                let mut x = fragment();
                hyperlink(&mut c, &mut x, guid_pending.take())?;
                s.hyperlinks.extend_from_slice(&x.out);
            }
            BRT_PRINT_OPTIONS => s.print_options = print_options(&mut c)?,
            BRT_MARGINS => {
                let mut x = fragment();
                let mut attributes = Vec::new();
                for name in ["left", "right", "top", "bottom", "header", "footer"] {
                    attributes.push((name, number(c.f64("BrtMargins")?)));
                }
                x.empty("pageMargins", &attributes);
                s.margins = x.out;
            }
            BRT_PAGE_SETUP => s.page_setup = page_setup(&mut c)?,
            BRT_BEGIN_HEADER_FOOTER => s.header_footer = header_footer(&mut c)?,
            BRT_BEGIN_RW_BRK | BRT_BEGIN_COL_BRK => {
                let count = c.u32("page breaks")?;
                let manual = c.u32("page breaks")?;
                let name = if record.kind == BRT_BEGIN_RW_BRK {
                    "rowBreaks"
                } else {
                    "colBreaks"
                };
                let mut x = fragment();
                let attributes = [
                    ("count", count.to_string()),
                    ("manualBreakCount", manual.to_string()),
                ];
                if count == 0 {
                    x.empty(name, &attributes);
                } else {
                    x.open(name, &attributes);
                    breaks_open = true;
                }
                s.breaks.extend_from_slice(&x.out);
            }
            BRT_BRK => {
                let id = c.u32("BrtBrk")?;
                let min = c.u32("BrtBrk")?;
                let max = c.u32("BrtBrk")?;
                let manual = c.u32("BrtBrk")? != 0;
                let pivot = c.u32("BrtBrk")? != 0;
                let mut attributes = vec![("id", id.to_string())];
                if min != 0 {
                    attributes.push(("min", min.to_string()));
                }
                if max != 0 {
                    attributes.push(("max", max.to_string()));
                }
                if manual {
                    attributes.push(("man", "1".to_owned()));
                }
                if pivot {
                    attributes.push(("pt", "1".to_owned()));
                }
                let mut x = fragment();
                x.empty("brk", &attributes);
                s.breaks.extend_from_slice(&x.out);
            }
            BRT_END_RW_BRK | BRT_END_COL_BRK => {
                let name = if record.kind == BRT_END_RW_BRK {
                    "rowBreaks"
                } else {
                    "colBreaks"
                };
                // The begin record wrote an empty element when it counted no break.
                if std::mem::take(&mut breaks_open) {
                    let mut x = fragment();
                    x.close(name);
                    s.breaks.extend_from_slice(&x.out);
                }
            }
            BRT_DRAWING => {
                let mut x = fragment();
                x.empty("drawing", &[("r:id", text(&c.wide("BrtDrawing")?))]);
                s.drawing = x.out;
            }
            BRT_LEGACY_DRAWING => {
                let mut x = fragment();
                x.empty(
                    "legacyDrawing",
                    &[("r:id", text(&c.wide("BrtLegacyDrawing")?))],
                );
                s.legacy_drawing = x.out;
            }
            BRT_LIST_PART => s.tables.push(text(&c.wide("BrtListPart")?)),
            _ => {}
        }
        i += 1;
    }
    if w.row.take().is_some() {
        d.close("row");
    }
    s.data = d.out;

    let mut x = Xml::new();
    x.open(
        "worksheet",
        &[
            ("xmlns", MAIN_NAMESPACE.to_owned()),
            ("xmlns:r", RELATIONSHIPS_NAMESPACE.to_owned()),
            ("xmlns:mc", COMPATIBILITY_NAMESPACE.to_owned()),
            ("mc:Ignorable", "xr".to_owned()),
            ("xmlns:xr", REVISION_NAMESPACE.to_owned()),
        ],
    );
    x.out.extend_from_slice(&s.properties);
    x.out.extend_from_slice(&s.dimension);
    if !s.views.is_empty() {
        x.open("sheetViews", &[]);
        x.out.extend_from_slice(&s.views);
        x.close("sheetViews");
    }
    x.out.extend_from_slice(&s.format);
    if !s.columns.is_empty() {
        x.open("cols", &[]);
        x.out.extend_from_slice(&s.columns);
        x.close("cols");
    }
    if s.data.is_empty() {
        x.empty("sheetData", &[]);
    } else {
        x.open("sheetData", &[]);
        x.out.extend_from_slice(&s.data);
        x.close("sheetData");
    }
    x.out.extend_from_slice(&s.protection);
    x.out.extend_from_slice(&s.filter);
    if !s.merges.is_empty() {
        x.open("mergeCells", &[("count", s.merges.len().to_string())]);
        for merge in &s.merges {
            x.empty("mergeCell", &[("ref", merge.clone())]);
        }
        x.close("mergeCells");
    }
    x.out.extend_from_slice(&s.phonetic);
    x.out.extend_from_slice(&s.conditional);
    if s.validation_count > 0 {
        x.open(
            "dataValidations",
            &[("count", s.validation_count.to_string())],
        );
        x.out.extend_from_slice(&s.validations);
        x.close("dataValidations");
    }
    if !s.hyperlinks.is_empty() {
        x.open("hyperlinks", &[]);
        x.out.extend_from_slice(&s.hyperlinks);
        x.close("hyperlinks");
    }
    x.out.extend_from_slice(&s.print_options);
    x.out.extend_from_slice(&s.margins);
    x.out.extend_from_slice(&s.page_setup);
    x.out.extend_from_slice(&s.header_footer);
    x.out.extend_from_slice(&s.breaks);
    x.out.extend_from_slice(&s.drawing);
    x.out.extend_from_slice(&s.legacy_drawing);
    if !s.tables.is_empty() {
        x.open("tableParts", &[("count", s.tables.len().to_string())]);
        for id in &s.tables {
            x.empty("tablePart", &[("r:id", id.clone())]);
        }
        x.close("tableParts");
    }
    x.close("worksheet");
    Ok(x.out)
}

impl Writer<'_> {
    fn row_header(&mut self, c: &mut Cursor<'_>, d: &mut Xml) -> Read<()> {
        if self.row.take().is_some() {
            d.close("row");
        }
        let row = c.u32("BrtRowHdr")?;
        let style = c.u32("BrtRowHdr")?;
        let height = c.u16("BrtRowHdr")?;
        let thick = c.u8("BrtRowHdr")?;
        let flags = c.u16("BrtRowHdr")?;
        let span_count = c.u32("BrtRowHdr")?;
        let mut spans = Vec::new();
        for _ in 0..span_count {
            let first = c.u32("BrtColSpan")?;
            let last = c.u32("BrtColSpan")?;
            spans.push(format!("{}:{}", first + 1, last + 1));
        }
        let mut attributes = vec![("r", (row + 1).to_string())];
        if !spans.is_empty() {
            attributes.push(("spans", spans.join(" ")));
        }
        let custom_format = flags & (1 << 6) != 0;
        if custom_format {
            if style != 0 {
                attributes.push(("s", style.to_string()));
            }
            attributes.push(("customFormat", "1".to_owned()));
        }
        let custom_height = flags & (1 << 5) != 0;
        let hidden = flags & (1 << 4) != 0;
        if custom_height || height != self.default_height {
            attributes.push(("ht", number(f64::from(height) / 20.0)));
        }
        if hidden {
            attributes.push(("hidden", "1".to_owned()));
        }
        if custom_height {
            attributes.push(("customHeight", "1".to_owned()));
        }
        let level = flags & 0x7;
        if level != 0 {
            attributes.push(("outlineLevel", level.to_string()));
        }
        if flags & (1 << 3) != 0 {
            attributes.push(("collapsed", "1".to_owned()));
        }
        if thick & 0x01 != 0 {
            attributes.push(("thickTop", "1".to_owned()));
        }
        if thick & 0x02 != 0 {
            attributes.push(("thickBot", "1".to_owned()));
        }
        d.open("row", &attributes);
        self.row = Some(row);
        Ok(())
    }

    /// One cell record; `after` are the records that follow it, where a shared formula, an array
    /// formula or a data table the cell starts is.
    fn cell(&mut self, record: Record<'_>, after: &[Record<'_>], d: &mut Xml) -> Read<()> {
        let row = self
            .row
            .ok_or_else(|| Malformed("a cell outside a row".to_owned()))?;
        let mut c = Cursor::new(record.body);
        let head = cell(&mut c)?;
        let mut attributes = vec![("r", a1(row + 1, head.column + 1))];
        if head.style != 0 {
            attributes.push(("s", head.style.to_string()));
        }
        let mut kind: Option<&str> = None;
        let mut value: Option<String> = None;
        let mut inline = None;
        let mut formula: Option<(&[u8], &[u8], u16)> = None;
        match record.kind {
            BRT_CELL_BLANK => {}
            BRT_CELL_RK => value = Some(number(c.rk("BrtCellRk")?)),
            BRT_CELL_REAL => value = Some(number(c.f64("BrtCellReal")?)),
            BRT_CELL_ERROR => {
                kind = Some("e");
                value = Some(error(c.u8("BrtCellError")?)?);
            }
            BRT_CELL_BOOL => {
                kind = Some("b");
                value = Some(flag(c.u8("BrtCellBool")? != 0));
            }
            BRT_CELL_ISST => {
                kind = Some("s");
                value = Some(c.u32("BrtCellIsst")?.to_string());
            }
            BRT_CELL_ST => {
                kind = Some("inlineStr");
                inline = Some(super::parts::RichText {
                    units: c.wide("BrtCellSt")?,
                    runs: Vec::new(),
                });
            }
            BRT_CELL_RSTRING => {
                kind = Some("inlineStr");
                inline = Some(rich_text(&mut c)?);
            }
            BRT_FMLA_STRING => {
                kind = Some("str");
                value = Some(text(&c.wide("BrtFmlaString")?));
                let flags = c.u16("BrtFmlaString")?;
                let (rgce, rgcb) = parsed(&mut c, "BrtFmlaString")?;
                formula = Some((rgce, rgcb, flags));
            }
            BRT_FMLA_NUM => {
                value = Some(number(c.f64("BrtFmlaNum")?));
                let flags = c.u16("BrtFmlaNum")?;
                let (rgce, rgcb) = parsed(&mut c, "BrtFmlaNum")?;
                formula = Some((rgce, rgcb, flags));
            }
            BRT_FMLA_BOOL => {
                kind = Some("b");
                value = Some(flag(c.u8("BrtFmlaBool")? != 0));
                let flags = c.u16("BrtFmlaBool")?;
                let (rgce, rgcb) = parsed(&mut c, "BrtFmlaBool")?;
                formula = Some((rgce, rgcb, flags));
            }
            BRT_FMLA_ERROR => {
                kind = Some("e");
                value = Some(error(c.u8("BrtFmlaError")?)?);
                let flags = c.u16("BrtFmlaError")?;
                let (rgce, rgcb) = parsed(&mut c, "BrtFmlaError")?;
                formula = Some((rgce, rgcb, flags));
            }
            other => return Err(Malformed(format!("the cell record {other}"))),
        }
        if let Some(kind) = kind {
            attributes.push(("t", kind.to_owned()));
        }
        if let Some(index) = self.cell_meta.take() {
            attributes.push(("cm", index.to_string()));
        }
        if let Some(index) = self.value_meta.take() {
            attributes.push(("vm", index.to_string()));
        }
        if head.phonetic {
            attributes.push(("ph", "1".to_owned()));
        }
        // The formula element, when the cell holds one of its own.
        let mut f: Option<(Vec<(&'static str, String)>, String)> = None;
        if let Some((rgce, rgcb, flags)) = formula {
            let mut f_attributes = Vec::new();
            let calc = flags & 0x02 != 0;
            let origin = Origin {
                row,
                column: head.column,
            };
            let context = self.shared.context(self.sheet);
            if rgce.first().map(|b| b & 0x1F) == Some(0x02) && rgce.len() == 5 {
                // PtgTbl: a cell of a data table.
                let anchor_row = u32::from_le_bytes(rgce[1..5].try_into().expect("4 bytes"));
                let anchor_column = Cursor::new(rgcb).u32("PtgExtraCol")?;
                let anchor = (anchor_row, anchor_column);
                if anchor == (row, head.column) {
                    let table = after
                        .iter()
                        .take_while(|r| matches!(r.kind, BRT_TABLE | 35..=38 | BRT_FRT_GUID))
                        .find(|r| r.kind == BRT_TABLE)
                        .ok_or_else(|| {
                            Malformed("a data table cell without BrtTable".to_owned())
                        })?;
                    f = Some(data_table(table.body)?);
                    self.groups.insert(anchor, Group::DataTable);
                } else if !matches!(self.groups.get(&anchor), Some(Group::DataTable)) {
                    return Err(Malformed("a data table cell before its table".to_owned()));
                }
            } else {
                match formula::print(rgce, rgcb, &context, origin)? {
                    Ok(text) => {
                        if calc {
                            f_attributes.push(("ca", "1".to_owned()));
                        }
                        f = Some((f_attributes, text));
                    }
                    Err(Expanded {
                        row: anchor_row,
                        column: anchor_column,
                    }) => {
                        let anchor = (anchor_row, anchor_column);
                        if anchor == (row, head.column) {
                            f = Some(self.group_start(after, anchor, calc)?);
                        } else {
                            match self.groups.get(&anchor) {
                                Some(Group::Shared(index)) => {
                                    let mut attributes = vec![("t", "shared".to_owned())];
                                    if calc {
                                        attributes.insert(0, ("ca", "1".to_owned()));
                                    }
                                    attributes.push(("si", index.to_string()));
                                    f = Some((attributes, String::new()));
                                }
                                Some(Group::Array) | Some(Group::DataTable) => {}
                                None => {
                                    return Err(Malformed(format!(
                                        "a cell of the shared or array formula at {} before it",
                                        a1(anchor_row + 1, anchor_column + 1)
                                    )));
                                }
                            }
                        }
                    }
                }
            }
        }
        if f.is_none() && value.is_none() && inline.is_none() {
            d.empty("c", &attributes);
            return Ok(());
        }
        d.open("c", &attributes);
        if let Some((f_attributes, text)) = &f {
            if text.is_empty() {
                d.empty("f", f_attributes);
            } else {
                d.element("f", f_attributes, text);
            }
        }
        if let Some(value) = &value {
            d.element("v", &[], value);
        }
        if let Some(rich) = &inline {
            d.open("is", &[]);
            write_rich(d, rich, &self.shared.fonts)?;
            d.close("is");
        }
        d.close("c");
        Ok(())
    }

    /// The formula element of the top-left cell of a shared or array formula, from the
    /// BrtShrFmla or BrtArrFmla that follows it.
    fn group_start(
        &mut self,
        after: &[Record<'_>],
        anchor: (u32, u32),
        calc: bool,
    ) -> Read<(Vec<(&'static str, String)>, String)> {
        let record = after
            .iter()
            .take_while(|r| matches!(r.kind, BRT_SHR_FMLA | BRT_ARR_FMLA | 35..=38 | BRT_FRT_GUID))
            .find(|r| matches!(r.kind, BRT_SHR_FMLA | BRT_ARR_FMLA))
            .ok_or_else(|| {
                Malformed(format!(
                    "the formula of {} names itself without a shared or array formula",
                    a1(anchor.0 + 1, anchor.1 + 1)
                ))
            })?;
        let mut c = Cursor::new(record.body);
        let area = c.rfx("a shared or array formula")?;
        let origin = Origin {
            row: anchor.0,
            column: anchor.1,
        };
        let context = self.shared.context(self.sheet);
        let mut attributes = Vec::new();
        if calc {
            attributes.push(("ca", "1".to_owned()));
        }
        if record.kind == BRT_ARR_FMLA {
            let flags = c.u8("BrtArrFmla")?;
            let (rgce, rgcb) = parsed(&mut c, "BrtArrFmla")?;
            let text = formula::print(rgce, rgcb, &context, origin)?
                .map_err(|_| Malformed("an array formula holding a PtgExp".to_owned()))?;
            if flags & 0x01 != 0 {
                attributes.push(("aca", "1".to_owned()));
            }
            attributes.insert(0, ("t", "array".to_owned()));
            attributes.insert(1, ("ref", area_text(&area)));
            self.groups.insert(anchor, Group::Array);
            Ok((attributes, text))
        } else {
            let (rgce, rgcb) = parsed(&mut c, "BrtShrFmla")?;
            let text = formula::print(rgce, rgcb, &context, origin)?
                .map_err(|_| Malformed("a shared formula holding a PtgExp".to_owned()))?;
            let index = self.next_shared;
            self.next_shared += 1;
            attributes.push(("t", "shared".to_owned()));
            attributes.push(("ref", area_text(&area)));
            attributes.push(("si", index.to_string()));
            self.groups.insert(anchor, Group::Shared(index));
            Ok((attributes, text))
        }
    }
}

fn error(code: u8) -> Read<String> {
    formula::error_text(code)
        .map(str::to_owned)
        .ok_or_else(|| Malformed(format!("the error code 0x{code:02X}")))
}

/// A data table's formula element (BrtTable, 2.4.846).
fn data_table(body: &[u8]) -> Read<(Vec<(&'static str, String)>, String)> {
    let mut c = Cursor::new(body);
    let area = c.rfx("BrtTable")?;
    let row1 = c.u32("BrtTable")?;
    let column1 = c.u32("BrtTable")?;
    let row2 = c.u32("BrtTable")?;
    let column2 = c.u32("BrtTable")?;
    let flags = c.u8("BrtTable")?;
    let mut attributes = vec![("t", "dataTable".to_owned()), ("ref", area_text(&area))];
    let two = flags & 0x02 != 0;
    if two {
        attributes.push(("dt2D", "1".to_owned()));
    }
    if flags & 0x01 != 0 {
        attributes.push(("dtr", "1".to_owned()));
    }
    if flags & 0x04 != 0 {
        attributes.push(("del1", "1".to_owned()));
    }
    if flags & 0x08 != 0 {
        attributes.push(("del2", "1".to_owned()));
    }
    attributes.push(("r1", a1(row1 + 1, column1 + 1)));
    if two {
        attributes.push(("r2", a1(row2 + 1, column2 + 1)));
    }
    if flags & 0x10 != 0 {
        attributes.insert(0, ("ca", "1".to_owned()));
    }
    Ok((attributes, String::new()))
}

/// `<sheetPr>` from BrtWsProp (2.4.877), empty when every property is the default.
fn sheet_properties(c: &mut Cursor<'_>) -> Read<Vec<u8>> {
    let bytes = c.bytes(3, "BrtWsProp")?;
    let flags = u32::from(bytes[0]) | u32::from(bytes[1]) << 8 | u32::from(bytes[2]) << 16;
    let bit = |n: u32| flags & (1 << n) != 0;
    let tab = color(c)?;
    let _row_sync = c.u32("BrtWsProp")?;
    let _column_sync = c.u32("BrtWsProp")?;
    let code = if c.remaining() > 0 {
        text(&c.wide("BrtWsProp")?)
    } else {
        String::new()
    };
    let mut attributes = Vec::new();
    if bit(12) {
        attributes.push(("syncHorizontal", "1".to_owned()));
    }
    if bit(13) {
        attributes.push(("syncVertical", "1".to_owned()));
    }
    if bit(14) {
        attributes.push(("transitionEvaluation", "1".to_owned()));
    }
    if bit(15) {
        attributes.push(("transitionEntry", "1".to_owned()));
    }
    if !bit(3) {
        attributes.push(("published", "0".to_owned()));
    }
    if !code.is_empty() {
        attributes.push(("codeName", code));
    }
    if bit(16) {
        attributes.push(("filterMode", "1".to_owned()));
    }
    if !bit(17) {
        attributes.push(("enableFormatConditionsCalculation", "0".to_owned()));
    }
    let tab_color = tab.first().is_some_and(|(name, _)| *name != "auto");
    let mut outline = Vec::new();
    if bit(5) {
        outline.push(("applyStyles", "1".to_owned()));
    }
    if !bit(6) {
        outline.push(("summaryBelow", "0".to_owned()));
    }
    if !bit(7) {
        outline.push(("summaryRight", "0".to_owned()));
    }
    if !bit(10) {
        outline.push(("showOutlineSymbols", "0".to_owned()));
    }
    let mut page = Vec::new();
    if !bit(0) {
        page.push(("autoPageBreaks", "0".to_owned()));
    }
    if bit(8) {
        page.push(("fitToPage", "1".to_owned()));
    }
    let mut x = fragment();
    if attributes.is_empty() && !tab_color && outline.is_empty() && page.is_empty() {
        return Ok(x.out);
    }
    if !tab_color && outline.is_empty() && page.is_empty() {
        x.empty("sheetPr", &attributes);
        return Ok(x.out);
    }
    x.open("sheetPr", &attributes);
    if tab_color {
        x.empty("tabColor", &tab);
    }
    if !outline.is_empty() {
        x.empty("outlinePr", &outline);
    }
    if !page.is_empty() {
        x.empty("pageSetUpPr", &page);
    }
    x.close("sheetPr");
    Ok(x.out)
}

/// A `<sheetView>`'s attributes from BrtBeginWsView (2.4.307).
fn sheet_view(c: &mut Cursor<'_>) -> Read<Vec<(&'static str, String)>> {
    let flags = c.u16("BrtBeginWsView")?;
    let bit = |n: u16| flags & (1 << n) != 0;
    let view = c.u32("BrtBeginWsView")?;
    let top = c.u32("BrtBeginWsView")?;
    let left = c.u32("BrtBeginWsView")?;
    let header_color = c.u8("BrtBeginWsView")?;
    c.skip(3, "BrtBeginWsView")?;
    let scale = c.u16("BrtBeginWsView")?;
    let scale_normal = c.u16("BrtBeginWsView")?;
    let scale_break = c.u16("BrtBeginWsView")?;
    let scale_layout = c.u16("BrtBeginWsView")?;
    let workbook_view = c.u32("BrtBeginWsView")?;
    let mut attributes = Vec::new();
    if bit(0) {
        attributes.push(("windowProtection", "1".to_owned()));
    }
    if bit(1) {
        attributes.push(("showFormulas", "1".to_owned()));
    }
    if !bit(2) {
        attributes.push(("showGridLines", "0".to_owned()));
    }
    if !bit(3) {
        attributes.push(("showRowColHeaders", "0".to_owned()));
    }
    if !bit(4) {
        attributes.push(("showZeros", "0".to_owned()));
    }
    if bit(5) {
        attributes.push(("rightToLeft", "1".to_owned()));
    }
    if bit(6) {
        attributes.push(("tabSelected", "1".to_owned()));
    }
    if !bit(7) {
        attributes.push(("showRuler", "0".to_owned()));
    }
    if !bit(8) {
        attributes.push(("showOutlineSymbols", "0".to_owned()));
    }
    if !bit(9) {
        attributes.push(("defaultGridColor", "0".to_owned()));
    }
    if bit(10) {
        attributes.push(("showWhiteSpace", "0".to_owned()));
    }
    match view {
        0 => {}
        1 => attributes.push(("view", "pageBreakPreview".to_owned())),
        2 => attributes.push(("view", "pageLayout".to_owned())),
        other => return Err(Malformed(format!("the sheet view {other}"))),
    }
    if top != 0 || left != 0 {
        attributes.push(("topLeftCell", a1(top + 1, left + 1)));
    }
    if header_color != 64 {
        attributes.push(("colorId", header_color.to_string()));
    }
    if scale != 100 && scale != 0 {
        attributes.push(("zoomScale", scale.to_string()));
    }
    if scale_normal != 0 {
        attributes.push(("zoomScaleNormal", scale_normal.to_string()));
    }
    if scale_break != 0 {
        attributes.push(("zoomScaleSheetLayoutView", scale_break.to_string()));
    }
    if scale_layout != 0 {
        attributes.push(("zoomScalePageLayoutView", scale_layout.to_string()));
    }
    attributes.push(("workbookViewId", workbook_view.to_string()));
    Ok(attributes)
}

const PANES: [&str; 4] = ["bottomRight", "topRight", "bottomLeft", "topLeft"];

fn pane_name(pnn: u32) -> Read<&'static str> {
    PANES
        .get(pnn as usize)
        .copied()
        .ok_or_else(|| Malformed(format!("the pane {pnn}")))
}

/// `<pane>` from BrtPane (2.4.723).
fn pane(c: &mut Cursor<'_>, x: &mut Xml) -> Read<()> {
    let x_split = c.f64("BrtPane")?;
    let y_split = c.f64("BrtPane")?;
    let top = c.u32("BrtPane")?;
    let left = c.u32("BrtPane")?;
    let active = c.u32("BrtPane")?;
    let flags = c.u8("BrtPane")?;
    let mut attributes = Vec::new();
    if x_split != 0.0 {
        attributes.push(("xSplit", number(x_split)));
    }
    if y_split != 0.0 {
        attributes.push(("ySplit", number(y_split)));
    }
    attributes.push(("topLeftCell", a1(top + 1, left + 1)));
    if active != 3 {
        attributes.push(("activePane", pane_name(active)?.to_owned()));
    }
    if flags & 0x02 != 0 {
        attributes.push(("state", "frozen".to_owned()));
    } else if flags & 0x01 != 0 {
        attributes.push(("state", "frozenSplit".to_owned()));
    }
    x.empty("pane", &attributes);
    Ok(())
}

/// `<selection>` from BrtSel (2.4.792), nothing when it is the default.
fn selection(c: &mut Cursor<'_>, x: &mut Xml) -> Read<()> {
    let pnn = c.u32("BrtSel")?;
    let row = c.u32("BrtSel")?;
    let column = c.u32("BrtSel")?;
    let active = c.u32("BrtSel")?;
    let (_, areas) = sqref(c, "BrtSel")?;
    let mut attributes = Vec::new();
    if pnn != 3 {
        attributes.push(("pane", pane_name(pnn)?.to_owned()));
    }
    let cell = a1(row + 1, column + 1);
    if cell != "A1" {
        attributes.push(("activeCell", cell));
    }
    if active != 0 {
        attributes.push(("activeCellId", active.to_string()));
    }
    if areas != "A1" {
        attributes.push(("sqref", areas));
    }
    if !attributes.is_empty() {
        x.empty("selection", &attributes);
    }
    Ok(())
}

/// `<sheetFormatPr>` from BrtWsFmtInfo (2.4.875).
fn format_info(c: &mut Cursor<'_>) -> Read<Vec<u8>> {
    let column_width = c.u32("BrtWsFmtInfo")?;
    let base_width = c.u16("BrtWsFmtInfo")?;
    let row_height = c.u16("BrtWsFmtInfo")?;
    let flags = c.u16("BrtWsFmtInfo")?;
    let row_level = c.u8("BrtWsFmtInfo")?;
    let column_level = c.u8("BrtWsFmtInfo")?;
    let mut attributes = Vec::new();
    if base_width != 8 {
        attributes.push(("baseColWidth", base_width.to_string()));
    }
    if column_width != u32::MAX {
        attributes.push(("defaultColWidth", number(f64::from(column_width) / 256.0)));
    }
    attributes.push(("defaultRowHeight", number(f64::from(row_height) / 20.0)));
    for (bit, name) in [
        (0, "customHeight"),
        (1, "zeroHeight"),
        (2, "thickTop"),
        (3, "thickBottom"),
    ] {
        if flags & (1 << bit) != 0 {
            attributes.push((name, "1".to_owned()));
        }
    }
    if row_level != 0 {
        attributes.push(("outlineLevelRow", row_level.to_string()));
    }
    if column_level != 0 {
        attributes.push(("outlineLevelCol", column_level.to_string()));
    }
    let mut x = fragment();
    x.empty("sheetFormatPr", &attributes);
    Ok(x.out)
}

/// A `<col>` from BrtColInfo (2.4.336).
fn column_info(c: &mut Cursor<'_>, out: &mut Vec<u8>) -> Read<()> {
    let first = c.u32("BrtColInfo")?;
    let last = c.u32("BrtColInfo")?;
    let width = c.u32("BrtColInfo")?;
    let style = c.u32("BrtColInfo")?;
    let flags = c.u16("BrtColInfo")?;
    let mut attributes = vec![
        ("min", (first + 1).to_string()),
        ("max", (last + 1).to_string()),
        ("width", number(f64::from(width) / 256.0)),
    ];
    if style != 0 {
        attributes.push(("style", style.to_string()));
    }
    if flags & 0x01 != 0 {
        attributes.push(("hidden", "1".to_owned()));
    }
    if flags & 0x04 != 0 {
        attributes.push(("bestFit", "1".to_owned()));
    }
    if flags & 0x02 != 0 {
        attributes.push(("customWidth", "1".to_owned()));
    }
    if flags & 0x08 != 0 {
        attributes.push(("phonetic", "1".to_owned()));
    }
    let level = (flags >> 8) & 0x7;
    if level != 0 {
        attributes.push(("outlineLevel", level.to_string()));
    }
    if flags & (1 << 12) != 0 {
        attributes.push(("collapsed", "1".to_owned()));
    }
    let mut x = fragment();
    x.empty("col", &attributes);
    out.extend_from_slice(&x.out);
    Ok(())
}

/// `<sheetProtection>` from BrtSheetProtection (2.4.800), nothing when the sheet is not
/// protected.
fn protection(c: &mut Cursor<'_>) -> Read<Vec<u8>> {
    let password = c.u16("BrtSheetProtection")?;
    let locked = c.u32("BrtSheetProtection")? != 0;
    let mut x = fragment();
    if !locked {
        return Ok(x.out);
    }
    let mut attributes = Vec::new();
    if password != 0 {
        attributes.push(("password", format!("{password:04X}")));
    }
    attributes.push(("sheet", "1".to_owned()));
    for (name, default) in [
        ("objects", false),
        ("scenarios", false),
        ("formatCells", true),
        ("formatColumns", true),
        ("formatRows", true),
        ("insertColumns", true),
        ("insertRows", true),
        ("insertHyperlinks", true),
        ("deleteColumns", true),
        ("deleteRows", true),
        ("selectLockedCells", false),
        ("sort", true),
        ("autoFilter", true),
        ("pivotTables", true),
        ("selectUnlockedCells", false),
    ] {
        let value = c.u32("BrtSheetProtection")? != 0;
        if value != default {
            attributes.push((name, flag(value)));
        }
    }
    x.empty("sheetProtection", &attributes);
    Ok(x.out)
}

/// A `<hyperlink>` from BrtHLink (2.4.693).
fn hyperlink(c: &mut Cursor<'_>, x: &mut Xml, uid: Option<String>) -> Read<()> {
    let area = c.rfx("BrtHLink")?;
    let relationship = c.nullable_wide("BrtHLink")?.map(|u| text(&u));
    let location = text(&c.wide("BrtHLink")?);
    let tooltip = text(&c.wide("BrtHLink")?);
    let display = text(&c.wide("BrtHLink")?);
    let mut attributes = vec![("ref", area_text(&area))];
    if let Some(id) = relationship.filter(|id| !id.is_empty()) {
        attributes.push(("r:id", id));
    }
    if !location.is_empty() {
        attributes.push(("location", location));
    }
    if !tooltip.is_empty() {
        attributes.push(("tooltip", tooltip));
    }
    if !display.is_empty() {
        attributes.push(("display", display));
    }
    if let Some(uid) = uid {
        attributes.push(("xr:uid", uid));
    }
    x.empty("hyperlink", &attributes);
    Ok(())
}

/// `<printOptions>` from BrtPrintOptions (2.4.762), nothing when every option is the default.
fn print_options(c: &mut Cursor<'_>) -> Read<Vec<u8>> {
    let flags = c.u16("BrtPrintOptions")?;
    let mut attributes = Vec::new();
    for (bit, name) in [
        (0, "horizontalCentered"),
        (1, "verticalCentered"),
        (2, "headings"),
        (3, "gridLines"),
    ] {
        if flags & (1 << bit) != 0 {
            attributes.push((name, "1".to_owned()));
        }
    }
    let mut x = fragment();
    if !attributes.is_empty() {
        x.empty("printOptions", &attributes);
    }
    Ok(x.out)
}

/// `<pageSetup>` from BrtPageSetup (2.4.722).
fn page_setup(c: &mut Cursor<'_>) -> Read<Vec<u8>> {
    let paper = c.u32("BrtPageSetup")?;
    let scale = c.u32("BrtPageSetup")?;
    let horizontal = c.u32("BrtPageSetup")?;
    let vertical = c.u32("BrtPageSetup")?;
    let copies = c.u32("BrtPageSetup")?;
    let first_page = c.i32("BrtPageSetup")?;
    let fit_width = c.u32("BrtPageSetup")?;
    let fit_height = c.u32("BrtPageSetup")?;
    let flags = c.u16("BrtPageSetup")?;
    let relationship = c.nullable_wide("BrtPageSetup")?.map(|u| text(&u));
    let bit = |n: u16| flags & (1 << n) != 0;
    let mut attributes = Vec::new();
    if paper != 1 {
        attributes.push(("paperSize", paper.to_string()));
    }
    if scale != 100 {
        attributes.push(("scale", scale.to_string()));
    }
    if first_page != 1 {
        attributes.push(("firstPageNumber", first_page.to_string()));
    }
    if fit_width != 1 {
        attributes.push(("fitToWidth", fit_width.to_string()));
    }
    if fit_height != 1 {
        attributes.push(("fitToHeight", fit_height.to_string()));
    }
    if bit(0) {
        attributes.push(("pageOrder", "overThenDown".to_owned()));
    }
    if !bit(6) {
        attributes.push((
            "orientation",
            if bit(1) { "landscape" } else { "portrait" }.to_owned(),
        ));
    }
    if bit(3) {
        attributes.push(("blackAndWhite", "1".to_owned()));
    }
    if bit(4) {
        attributes.push(("draft", "1".to_owned()));
    }
    if bit(5) {
        attributes.push((
            "cellComments",
            if bit(8) { "atEnd" } else { "asDisplayed" }.to_owned(),
        ));
    }
    if bit(7) {
        attributes.push(("useFirstPageNumber", "1".to_owned()));
    }
    match (flags >> 9) & 0x3 {
        1 => attributes.push(("errors", "blank".to_owned())),
        2 => attributes.push(("errors", "dash".to_owned())),
        3 => attributes.push(("errors", "NA".to_owned())),
        _ => {}
    }
    if horizontal != 600 {
        attributes.push(("horizontalDpi", horizontal.to_string()));
    }
    if vertical != 600 {
        attributes.push(("verticalDpi", vertical.to_string()));
    }
    if copies != 1 {
        attributes.push(("copies", copies.to_string()));
    }
    if let Some(id) = relationship.filter(|id| !id.is_empty()) {
        attributes.push(("r:id", id));
    }
    let mut x = fragment();
    x.empty("pageSetup", &attributes);
    Ok(x.out)
}

/// `<headerFooter>` from BrtBeginHeaderFooter (2.4.90).
fn header_footer(c: &mut Cursor<'_>) -> Read<Vec<u8>> {
    let flags = c.u16("BrtBeginHeaderFooter")?;
    let mut attributes = Vec::new();
    if flags & 0x01 != 0 {
        attributes.push(("differentOddEven", "1".to_owned()));
    }
    if flags & 0x02 != 0 {
        attributes.push(("differentFirst", "1".to_owned()));
    }
    if flags & 0x04 == 0 {
        attributes.push(("scaleWithDoc", "0".to_owned()));
    }
    if flags & 0x08 == 0 {
        attributes.push(("alignWithMargins", "0".to_owned()));
    }
    let mut children = Vec::new();
    for name in [
        "oddHeader",
        "oddFooter",
        "evenHeader",
        "evenFooter",
        "firstHeader",
        "firstFooter",
    ] {
        if let Some(units) = c.nullable_wide("BrtBeginHeaderFooter")? {
            children.push((name, text(&units)));
        }
    }
    let mut x = fragment();
    if children.is_empty() {
        x.empty("headerFooter", &attributes);
    } else {
        x.open("headerFooter", &attributes);
        for (name, value) in &children {
            x.element(name, &[], value);
        }
        x.close("headerFooter");
    }
    Ok(x.out)
}

/// The top-left cell of the conditional format whose records come last in `before`: the cell
/// relative references of its formulas are offsets from.
fn conditional_origin(before: &[Record<'_>]) -> Origin {
    before
        .iter()
        .rev()
        .find(|r| r.kind == BRT_BEGIN_CONDITIONAL_FORMATTING)
        .and_then(|r| {
            let mut c = Cursor::new(r.body);
            c.skip(8, "").ok()?;
            let (areas, _) = sqref(&mut c, "").ok()?;
            let first = areas.first()?;
            Some(Origin {
                row: first.first_row,
                column: first.first_column,
            })
        })
        .unwrap_or_default()
}

/// One conditional formatting rule as read.
struct Rule {
    attributes: Vec<(&'static str, String)>,
    formulas: Vec<String>,
    /// A colour scale, data bar or icon set: its begin record's kind and body.
    visual: Option<(u16, Vec<u8>)>,
    values: Vec<Vec<(&'static str, String)>>,
    colors: Vec<Vec<(&'static str, String)>>,
}

const OPERATORS: [&str; 8] = [
    "between",
    "notBetween",
    "equal",
    "notEqual",
    "greaterThan",
    "lessThan",
    "greaterThanOrEqual",
    "lessThanOrEqual",
];

const TIME_PERIODS: [&str; 10] = [
    "today",
    "tomorrow",
    "yesterday",
    "last7Days",
    "lastMonth",
    "nextMonth",
    "thisWeek",
    "nextWeek",
    "lastWeek",
    "thisMonth",
];

const ICON_SETS: [&str; 17] = [
    "3Arrows",
    "3ArrowsGray",
    "3Flags",
    "3TrafficLights1",
    "3TrafficLights2",
    "3Signs",
    "3Symbols",
    "3Symbols2",
    "4Arrows",
    "4ArrowsGray",
    "4RedToBlack",
    "4Rating",
    "4TrafficLights",
    "5Arrows",
    "5ArrowsGray",
    "5Rating",
    "5Quarters",
];

impl Rule {
    /// A rule from BrtBeginCFRule (2.4.23).
    fn read(c: &mut Cursor<'_>, context: &formula::Context<'_>, origin: Origin) -> Read<Rule> {
        let kind = c.u32("BrtBeginCFRule")?;
        let template = c.u32("BrtBeginCFRule")?;
        let dxf = c.u32("BrtBeginCFRule")?;
        let priority = c.i32("BrtBeginCFRule")?;
        let parameter = c.u32("BrtBeginCFRule")?;
        c.skip(8, "BrtBeginCFRule")?;
        let flags = c.u16("BrtBeginCFRule")?;
        let sizes = [
            c.u32("BrtBeginCFRule")?,
            c.u32("BrtBeginCFRule")?,
            c.u32("BrtBeginCFRule")?,
        ];
        let text_parameter = c.nullable_wide("BrtBeginCFRule")?.map(|u| text(&u));
        let mut formulas = Vec::new();
        for size in sizes {
            if size == 0 {
                continue;
            }
            let (rgce, rgcb) = parsed(c, "a conditional format's formula")?;
            let text = formula::print(rgce, rgcb, context, origin)?.map_err(|_| {
                Malformed("a conditional format's formula holding a PtgExp".to_owned())
            })?;
            formulas.push(text);
        }
        let type_name = match (kind, template) {
            (1, _) => "cellIs",
            (2, 1) | (2, 0) => "expression",
            (3, _) => "colorScale",
            (4, _) => "dataBar",
            (5, _) => "top10",
            (6, _) => "iconSet",
            (2, 7) => "uniqueValues",
            (2, 8) => match parameter {
                0 => "containsText",
                1 => "notContainsText",
                2 => "beginsWith",
                _ => "endsWith",
            },
            (2, 9) => "containsBlanks",
            (2, 10) => "notContainsBlanks",
            (2, 11) => "containsErrors",
            (2, 12) => "notContainsErrors",
            (2, 15..=24) => "timePeriod",
            (2, 25 | 26 | 29 | 30) => "aboveAverage",
            (2, 27) => "duplicateValues",
            other => return Err(Malformed(format!("the conditional format type {other:?}"))),
        };
        let mut attributes = vec![("type", type_name.to_owned())];
        if dxf != u32::MAX && !matches!(kind, 3 | 4 | 6) {
            attributes.push(("dxfId", dxf.to_string()));
        }
        attributes.push(("priority", priority.to_string()));
        if flags & 0x02 != 0 {
            attributes.push(("stopIfTrue", "1".to_owned()));
        }
        match (kind, template) {
            (1, _) => {
                let operator = OPERATORS
                    .get((parameter as usize).wrapping_sub(1))
                    .ok_or_else(|| Malformed(format!("the operator {parameter}")))?;
                attributes.push(("operator", (*operator).to_owned()));
            }
            (5, _) => {
                if flags & 0x10 != 0 {
                    attributes.push(("percent", "1".to_owned()));
                }
                if flags & 0x08 != 0 {
                    attributes.push(("bottom", "1".to_owned()));
                }
                attributes.push(("rank", parameter.to_string()));
            }
            (2, 8) => {
                let operator = match parameter {
                    0 => "containsText",
                    1 => "notContains",
                    2 => "beginsWith",
                    _ => "endsWith",
                };
                attributes.push(("operator", operator.to_owned()));
                if let Some(text) = &text_parameter {
                    attributes.push(("text", text.clone()));
                }
            }
            (2, period @ 15..=24) => {
                attributes.push((
                    "timePeriod",
                    TIME_PERIODS[(period - 15) as usize].to_owned(),
                ));
            }
            (2, average @ (25 | 26 | 29 | 30)) => {
                if matches!(average, 26 | 30) {
                    attributes.push(("aboveAverage", "0".to_owned()));
                }
                if matches!(average, 29 | 30) {
                    attributes.push(("equalAverage", "1".to_owned()));
                }
                if parameter != 0 {
                    attributes.push(("stdDev", parameter.to_string()));
                }
            }
            _ => {}
        }
        Ok(Rule {
            attributes,
            formulas,
            visual: None,
            values: Vec::new(),
            colors: Vec::new(),
        })
    }

    fn write(&self, x: &mut Xml) -> Read<()> {
        x.open("cfRule", &self.attributes);
        for text in &self.formulas {
            x.element("formula", &[], text);
        }
        if let Some((kind, body)) = &self.visual {
            let mut c = Cursor::new(body);
            match *kind {
                BRT_BEGIN_COLOR_SCALE => {
                    x.open("colorScale", &[]);
                    for value in &self.values {
                        x.empty("cfvo", &without_gte(value));
                    }
                    for color in &self.colors {
                        x.empty("color", color);
                    }
                    x.close("colorScale");
                }
                BRT_BEGIN_DATABAR => {
                    let minimum = c.u8("BrtBeginDatabar")?;
                    let maximum = c.u8("BrtBeginDatabar")?;
                    let show = c.u8("BrtBeginDatabar")? != 0;
                    let mut attributes = Vec::new();
                    if minimum != 10 {
                        attributes.push(("minLength", minimum.to_string()));
                    }
                    if maximum != 90 {
                        attributes.push(("maxLength", maximum.to_string()));
                    }
                    if !show {
                        attributes.push(("showValue", "0".to_owned()));
                    }
                    x.open("dataBar", &attributes);
                    for value in &self.values {
                        x.empty("cfvo", &without_gte(value));
                    }
                    for color in &self.colors {
                        x.empty("color", color);
                    }
                    x.close("dataBar");
                }
                _ => {
                    let set = c.u32("BrtBeginIconSet")?;
                    let flags = c.u16("BrtBeginIconSet")?;
                    let mut attributes = Vec::new();
                    if set != 3 {
                        let name = ICON_SETS
                            .get(set as usize)
                            .ok_or_else(|| Malformed(format!("the icon set {set}")))?;
                        attributes.push(("iconSet", (*name).to_owned()));
                    }
                    if flags & 0x02 != 0 {
                        attributes.push(("showValue", "0".to_owned()));
                    }
                    if flags & 0x04 != 0 {
                        attributes.push(("reverse", "1".to_owned()));
                    }
                    x.open("iconSet", &attributes);
                    for value in &self.values {
                        x.empty("cfvo", value);
                    }
                    x.close("iconSet");
                }
            }
        }
        x.close("cfRule");
        Ok(())
    }
}

/// A threshold's attributes without `gte`, which only an icon set's thresholds hold (BrtCFVO's
/// fGTE is ignored for a colour scale or a data bar).
fn without_gte(value: &[(&'static str, String)]) -> Vec<(&'static str, String)> {
    value
        .iter()
        .filter(|(name, _)| *name != "gte")
        .cloned()
        .collect()
}

/// A `<cfvo>`'s attributes from BrtCFVO (2.4.334).
fn cfvo(
    c: &mut Cursor<'_>,
    context: &formula::Context<'_>,
    origin: Origin,
) -> Read<Vec<(&'static str, String)>> {
    let kind = c.u32("BrtCFVO")?;
    let value = c.f64("BrtCFVO")?;
    let _save_gte = c.u32("BrtCFVO")?;
    let gte = c.u32("BrtCFVO")? != 0;
    let size = c.u32("BrtCFVO")?;
    let formula_text = if size > 0 {
        let (rgce, rgcb) = parsed(c, "BrtCFVO")?;
        Some(
            formula::print(rgce, rgcb, context, origin)?
                .map_err(|_| Malformed("a CFVO formula holding a PtgExp".to_owned()))?,
        )
    } else {
        None
    };
    let name = match kind {
        1 => "num",
        2 => "min",
        3 => "max",
        4 => "percent",
        5 => "percentile",
        7 => "formula",
        other => return Err(Malformed(format!("the CFVO type {other}"))),
    };
    let mut attributes = vec![("type", name.to_owned())];
    if !matches!(kind, 2 | 3) {
        attributes.push(("val", formula_text.unwrap_or_else(|| number(value))));
    }
    if !gte {
        attributes.push(("gte", "0".to_owned()));
    }
    Ok(attributes)
}

const VALIDATION_TYPES: [&str; 8] = [
    "none",
    "whole",
    "decimal",
    "list",
    "date",
    "time",
    "textLength",
    "custom",
];

/// A `<dataValidation>` from BrtDVal (2.4.356).
fn validation(c: &mut Cursor<'_>, x: &mut Xml, context: &formula::Context<'_>) -> Read<()> {
    let flags = c.u32("BrtDVal")?;
    let (areas, sqref_text) = sqref(c, "BrtDVal")?;
    let error_title = c.nullable_wide("DValStrings")?.map(|u| text(&u));
    let error = c.nullable_wide("DValStrings")?.map(|u| text(&u));
    let prompt_title = c.nullable_wide("DValStrings")?.map(|u| text(&u));
    let prompt = c.nullable_wide("DValStrings")?.map(|u| text(&u));
    let origin = areas
        .first()
        .map(|a| Origin {
            row: a.first_row,
            column: a.first_column,
        })
        .unwrap_or_default();
    let mut formulas = Vec::new();
    for _ in 0..2 {
        if c.remaining() == 0 {
            break;
        }
        let (rgce, rgcb) = parsed(c, "a data validation's formula")?;
        if rgce.is_empty() {
            formulas.push(None);
            continue;
        }
        formulas.push(Some(formula::print(rgce, rgcb, context, origin)?.map_err(
            |_| Malformed("a data validation formula holding a PtgExp".to_owned()),
        )?));
    }
    let kind = flags & 0xF;
    let style = (flags >> 4) & 0x7;
    let mut attributes = Vec::new();
    if kind != 0 {
        attributes.push((
            "type",
            VALIDATION_TYPES
                .get(kind as usize)
                .ok_or_else(|| Malformed(format!("the validation type {kind}")))?
                .to_string(),
        ));
    }
    match style {
        1 => attributes.push(("errorStyle", "warning".to_owned())),
        2 => attributes.push(("errorStyle", "information".to_owned())),
        _ => {}
    }
    let ime = (flags >> 10) & 0xFF;
    if ime != 0 {
        const MODES: [&str; 11] = [
            "noControl",
            "off",
            "on",
            "disabled",
            "hiragana",
            "fullKatakana",
            "halfKatakana",
            "fullAlpha",
            "halfAlpha",
            "fullHangul",
            "halfHangul",
        ];
        if let Some(mode) = MODES.get(ime as usize) {
            attributes.push(("imeMode", (*mode).to_owned()));
        }
    }
    let operator = (flags >> 20) & 0xF;
    if operator != 0 && kind != 0 && kind != 3 && kind != 7 {
        attributes.push((
            "operator",
            OPERATORS
                .get(operator as usize)
                .ok_or_else(|| Malformed(format!("the validation operator {operator}")))?
                .to_string(),
        ));
    }
    if flags & (1 << 8) != 0 {
        attributes.push(("allowBlank", "1".to_owned()));
    }
    if flags & (1 << 9) != 0 {
        attributes.push(("showDropDown", "1".to_owned()));
    }
    if flags & (1 << 18) != 0 {
        attributes.push(("showInputMessage", "1".to_owned()));
    }
    if flags & (1 << 19) != 0 {
        attributes.push(("showErrorMessage", "1".to_owned()));
    }
    for (name, value) in [
        ("errorTitle", error_title),
        ("error", error),
        ("promptTitle", prompt_title),
        ("prompt", prompt),
    ] {
        if let Some(value) = value {
            attributes.push((name, value));
        }
    }
    attributes.push(("sqref", sqref_text));
    let present: Vec<(usize, &String)> = formulas
        .iter()
        .enumerate()
        .filter_map(|(i, f)| f.as_ref().map(|f| (i, f)))
        .collect();
    if present.is_empty() {
        x.empty("dataValidation", &attributes);
        return Ok(());
    }
    x.open("dataValidation", &attributes);
    for (i, text) in present {
        x.element(if i == 0 { "formula1" } else { "formula2" }, &[], text);
    }
    x.close("dataValidation");
    Ok(())
}
