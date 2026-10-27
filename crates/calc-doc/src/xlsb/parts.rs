//! What the parts of an Excel binary workbook share ([`Shared`]: sheet names, supporting
//! workbooks, defined names, tables and fonts, which formulas and rich text name), and the parts
//! other than the workbook, worksheets and styles: shared strings (MS-XLSB 2.1.7.45), tables
//! (2.1.7.56), external links (2.1.7.22), comments (2.1.7.8) and cell and value metadata
//! (2.1.7.34), each written as the SpreadsheetML part of the same content.

use super::formula::{self, Context, NameRef, Origin, SupBook, TableRef};
use super::records::{Cursor, Malformed, Read, records, text};
use super::styles::{Font, fonts, write_font};
use super::workbook::{Book, Link, name_refs};
use super::{MAIN_NAMESPACE, RELATIONSHIPS_NAMESPACE, Xml, refusal};
use crate::OpenError;
use crate::opc::{Package, TargetMode};
use crate::save::text::{a1, needs_preserve};
use std::collections::BTreeMap;

const BRT_SST_ITEM: u16 = 19;
const BRT_BEGIN_SST: u16 = 159;
const BRT_FRT_GUID: u16 = 3072;
const BRT_BEGIN_LIST: u16 = 343;
const BRT_BEGIN_LIST_COL: u16 = 347;
const BRT_LIST_CC_FMLA: u16 = 351;
const BRT_LIST_TR_FMLA: u16 = 352;
const BRT_TABLE_STYLE_CLIENT: u16 = 513;
const BRT_BEGIN_AFILTER: u16 = 161;
const BRT_BEGIN_SUP_BOOK: u16 = 360;
const BRT_SUP_TABS: u16 = 359;
const BRT_EXTERN_TABLE_START: u16 = 363;
const BRT_EXTERN_TABLE_END: u16 = 364;
const BRT_EXTERN_ROW_HDR: u16 = 366;
const BRT_EXTERN_CELL_BLANK: u16 = 367;
const BRT_EXTERN_CELL_REAL: u16 = 368;
const BRT_EXTERN_CELL_BOOL: u16 = 369;
const BRT_EXTERN_CELL_ERROR: u16 = 370;
const BRT_EXTERN_CELL_STRING: u16 = 371;
const BRT_SUP_NAME_START: u16 = 577;
const BRT_COMMENT_AUTHOR: u16 = 632;
const BRT_BEGIN_COMMENT: u16 = 635;
const BRT_COMMENT_TEXT: u16 = 637;
const BRT_MDTINFO: u16 = 335;
const BRT_BEGIN_ESMDB: u16 = 337;
const BRT_MDB: u16 = 51;
const BRT_BEGIN_ESFMD: u16 = 339;
const BRT_BEGIN_FMD: u16 = 52;
const BRT_END_FMD: u16 = 53;
const BRT_END_DYNAMIC_ARRAY_PR: u16 = 4097;
const BRT_END_RICH_VALUE_BLOCK: u16 = 5003;

const REVISION_NAMESPACE: &str = "http://schemas.microsoft.com/office/spreadsheetml/2014/revision";
const REVISION3_NAMESPACE: &str =
    "http://schemas.microsoft.com/office/spreadsheetml/2016/revision3";
const COMPATIBILITY_NAMESPACE: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";

/// What the parts of the workbook share.
pub(crate) struct Shared {
    /// The sheets' names, by tab index.
    pub(crate) sheet_names: Vec<String>,
    /// The supporting workbooks, in the workbook part's order.
    pub(crate) supbooks: Vec<SupBook>,
    /// The defined names, by 1-based index less one.
    pub(crate) names: Vec<NameRef>,
    /// The tables, by their id.
    pub(crate) tables: BTreeMap<u32, TableRef>,
    /// The fonts of the styles part, which rich text runs name.
    pub(crate) fonts: Vec<Font>,
    /// The XTIs of the workbook.
    pub(crate) xti: Vec<formula::Xti>,
}

impl Shared {
    pub(crate) fn new(
        package: &Package,
        workbook: &str,
        book: &Book,
        read: &dyn Fn(&str) -> Result<Vec<u8>, OpenError>,
    ) -> Result<Shared, OpenError> {
        let target = |source: &str, id: &str| -> Option<String> {
            package
                .relationships_of(Some(source))?
                .items()
                .iter()
                .find(|item| item.id == id && item.mode == TargetMode::Internal)
                .and_then(|item| item.resolved.clone())
        };
        let mut supbooks = Vec::new();
        let mut number = 0;
        for link in &book.links {
            supbooks.push(match link {
                Link::This | Link::Same => SupBook::This,
                Link::AddIn { names } => SupBook::AddIn {
                    names: names.clone(),
                },
                Link::External { relationship } => {
                    number += 1;
                    let part = target(workbook, relationship).ok_or_else(|| {
                        refusal(
                            workbook,
                            format!("the external link {relationship}, which names no part"),
                        )
                    })?;
                    let data = read(&part)?;
                    let (sheets, names) =
                        external_names(&data).map_err(|error| refusal(&part, error.0))?;
                    SupBook::External {
                        number,
                        sheets,
                        names,
                    }
                }
            });
        }
        let mut tables = BTreeMap::new();
        let mut fonts_read = Vec::new();
        for part in package.parts() {
            match part.content_type() {
                Some("application/vnd.ms-excel.table") => {
                    let data = read(part.name())?;
                    let (id, table) =
                        table_ref(&data).map_err(|error| refusal(part.name(), error.0))?;
                    tables.insert(id, table);
                }
                Some("application/vnd.ms-excel.styles") => {
                    let data = read(part.name())?;
                    fonts_read = fonts(&data).map_err(|error| refusal(part.name(), error.0))?;
                }
                _ => {}
            }
        }
        Ok(Shared {
            sheet_names: book.sheets.iter().map(|sheet| sheet.name.clone()).collect(),
            supbooks,
            names: name_refs(book),
            tables,
            fonts: fonts_read,
            xti: book.xti.clone(),
        })
    }

    /// The context a formula on the sheet `sheet` (a tab index) is printed against.
    pub(crate) fn context(&self, sheet: Option<u32>) -> Context<'_> {
        Context {
            sheets: &self.sheet_names,
            xti: &self.xti,
            supbooks: &self.supbooks,
            names: &self.names,
            tables: &self.tables,
            sheet,
        }
    }
}

/// A GUID as SpreadsheetML writes it, `{15F46FFD-14D3-4A30-BD4D-0185C00D503D}`.
pub(crate) fn guid(bytes: &[u8]) -> Option<String> {
    let b: &[u8; 16] = bytes.get(..16)?.try_into().ok()?;
    Some(format!(
        "{{{:02X}{:02X}{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
        b[3],
        b[2],
        b[1],
        b[0],
        b[5],
        b[4],
        b[7],
        b[6],
        b[8],
        b[9],
        b[10],
        b[11],
        b[12],
        b[13],
        b[14],
        b[15]
    ))
}

/// A0-style text of a 0-based area: `A1` for one cell, `A1:B2` otherwise.
pub(crate) fn area_text(area: &super::records::Area0) -> String {
    let first = a1(area.first_row + 1, area.first_column + 1);
    if area.first_row == area.last_row && area.first_column == area.last_column {
        first
    } else {
        format!("{first}:{}", a1(area.last_row + 1, area.last_column + 1))
    }
}

/// A rich string (RichStr, 2.5.122): its text and its runs, each the first character and the
/// font it starts.
pub(crate) struct RichText {
    pub(crate) units: Vec<u16>,
    pub(crate) runs: Vec<(u16, u16)>,
}

pub(crate) fn rich_text(c: &mut Cursor<'_>) -> Read<RichText> {
    let flags = c.u8("a rich string")?;
    let units = c.wide("a rich string")?;
    let mut runs = Vec::new();
    if flags & 0x01 != 0 {
        let count = c.u32("a rich string's runs")?;
        for _ in 0..count {
            runs.push((c.u16("a run")?, c.u16("a run")?));
        }
    }
    // The phonetic text (flags & 0x02) is not carried.
    Ok(RichText { units, runs })
}

/// Writes a string item's content: `<t>` alone, or one `<r>` per run.
pub(crate) fn write_rich(x: &mut Xml, rich: &RichText, fonts: &[Font]) -> Read<()> {
    let t = |x: &mut Xml, units: &[u16]| {
        let attributes = if needs_preserve(units) {
            vec![("xml:space", "preserve".to_owned())]
        } else {
            Vec::new()
        };
        x.open("t", &attributes);
        x.text(units);
        x.close("t");
    };
    if rich.runs.is_empty() {
        t(x, &rich.units);
        return Ok(());
    }
    let mut starts: Vec<(usize, Option<u16>)> = Vec::new();
    if rich.runs[0].0 != 0 {
        starts.push((0, None));
    }
    starts.extend(
        rich.runs
            .iter()
            .map(|(at, font)| (usize::from(*at), Some(*font))),
    );
    for (i, (start, font)) in starts.iter().enumerate() {
        let end = starts
            .get(i + 1)
            .map_or(rich.units.len(), |next| next.0)
            .min(rich.units.len());
        let start = (*start).min(end);
        x.open("r", &[]);
        if let Some(font) = font {
            let font = fonts
                .get(usize::from(*font))
                .ok_or_else(|| Malformed(format!("the font {font} of a rich text run")))?;
            x.open("rPr", &[]);
            write_font(x, font, true);
            x.close("rPr");
        }
        t(x, &rich.units[start..end]);
        x.close("r");
    }
    Ok(())
}

fn malformed(part: &str) -> impl Fn(Malformed) -> OpenError + '_ {
    move |error| refusal(part, error.0)
}

/// The shared strings part.
pub(crate) fn shared_strings(
    part: &str,
    data: &[u8],
    shared: &Shared,
) -> Result<Vec<u8>, OpenError> {
    (|| -> Read<Vec<u8>> {
        let records = records(data)?;
        let mut x = Xml::new();
        let mut attributes = vec![("xmlns", MAIN_NAMESPACE.to_owned())];
        if let Some(begin) = records.iter().find(|r| r.kind == BRT_BEGIN_SST) {
            let mut c = Cursor::new(begin.body);
            let total = c.u32("BrtBeginSst")?;
            let unique = c.u32("BrtBeginSst")?;
            attributes.push(("count", total.to_string()));
            attributes.push(("uniqueCount", unique.to_string()));
        }
        x.open("sst", &attributes);
        for record in records.iter().filter(|r| r.kind == BRT_SST_ITEM) {
            let rich = rich_text(&mut Cursor::new(record.body))?;
            x.open("si", &[]);
            write_rich(&mut x, &rich, &shared.fonts)?;
            x.close("si");
        }
        x.close("sst");
        Ok(x.out)
    })()
    .map_err(malformed(part))
}

/// A table part's id, name and column names.
fn table_ref(data: &[u8]) -> Read<(u32, TableRef)> {
    let records = records(data)?;
    let begin = records
        .iter()
        .find(|r| r.kind == BRT_BEGIN_LIST)
        .ok_or_else(|| Malformed("a table part without BrtBeginList".to_owned()))?;
    let list = begin_list(begin.body)?;
    let mut columns = Vec::new();
    for r in records.iter().filter(|r| r.kind == BRT_BEGIN_LIST_COL) {
        columns.push(begin_list_col(r.body)?.name);
    }
    Ok((
        list.id,
        TableRef {
            name: list.display_name.clone(),
            columns,
        },
    ))
}

struct List {
    area: super::records::Area0,
    kind: u32,
    id: u32,
    header_rows: u32,
    totals_rows: u32,
    flags: u32,
    dxfs: [u32; 6],
    connection: u32,
    name: String,
    display_name: String,
    comment: Option<String>,
    styles: [Option<String>; 3],
}

fn begin_list(body: &[u8]) -> Read<List> {
    let mut c = Cursor::new(body);
    let area = c.rfx("BrtBeginList")?;
    let kind = c.u32("BrtBeginList")?;
    let id = c.u32("BrtBeginList")?;
    let header_rows = c.u32("BrtBeginList")?;
    let totals_rows = c.u32("BrtBeginList")?;
    let flags = c.u32("BrtBeginList")?;
    let mut dxfs = [0u32; 6];
    for dxf in &mut dxfs {
        *dxf = c.u32("BrtBeginList")?;
    }
    let connection = c.u32("BrtBeginList")?;
    let name = c
        .nullable_wide("BrtBeginList")?
        .map(|u| text(&u))
        .unwrap_or_default();
    let display_name = c
        .nullable_wide("BrtBeginList")?
        .map(|u| text(&u))
        .unwrap_or_else(|| name.clone());
    let comment = c
        .nullable_wide("BrtBeginList")?
        .map(|u| text(&u))
        .filter(|t| !t.is_empty());
    let mut styles: [Option<String>; 3] = [None, None, None];
    for style in &mut styles {
        if c.remaining() > 0 {
            *style = c.nullable_wide("BrtBeginList")?.map(|u| text(&u));
        }
    }
    Ok(List {
        area,
        kind,
        id,
        header_rows,
        totals_rows,
        flags,
        dxfs,
        connection,
        name,
        display_name,
        comment,
        styles,
    })
}

struct ListCol {
    id: u32,
    function: u32,
    dxfs: [u32; 3],
    name: String,
    total_label: Option<String>,
    styles: [Option<String>; 3],
}

fn begin_list_col(body: &[u8]) -> Read<ListCol> {
    let mut c = Cursor::new(body);
    let id = c.u32("BrtBeginListCol")?;
    let function = c.u32("BrtBeginListCol")?;
    let mut dxfs = [0u32; 3];
    for dxf in &mut dxfs {
        *dxf = c.u32("BrtBeginListCol")?;
    }
    let _query_field = c.u32("BrtBeginListCol")?;
    let unique = c.nullable_wide("BrtBeginListCol")?.map(|u| text(&u));
    let caption = c.nullable_wide("BrtBeginListCol")?.map(|u| text(&u));
    let total_label = c.nullable_wide("BrtBeginListCol")?.map(|u| text(&u));
    let mut styles: [Option<String>; 3] = [None, None, None];
    for style in &mut styles {
        if c.remaining() > 0 {
            *style = c.nullable_wide("BrtBeginListCol")?.map(|u| text(&u));
        }
    }
    Ok(ListCol {
        id,
        function,
        dxfs,
        name: caption.or(unique).unwrap_or_default(),
        total_label,
        styles,
    })
}

/// ListTotalRowFunction (2.5.90) as SpreadsheetML names it; a table's totals row SUM (6) measured
/// in the dc-5sr fixture.
const TOTAL_FUNCTIONS: [&str; 10] = [
    "none",
    "average",
    "count",
    "countNums",
    "max",
    "min",
    "sum",
    "stdDev",
    "var",
    "custom",
];

/// A table part. Its autoFilter is carried as its area; filter columns and a sort state are not.
pub(crate) fn table(part: &str, data: &[u8], shared: &Shared) -> Result<Vec<u8>, OpenError> {
    (|| -> Read<Vec<u8>> {
        let records = records(data)?;
        let mut x = Xml::new();
        let mut list: Option<List> = None;
        let mut list_uid = None;
        let mut filter = None;
        // Each column: its record, its uid, its calculated formula and its totals formula.
        let mut columns: Vec<(ListCol, Option<String>, Option<String>, Option<String>)> =
            Vec::new();
        let mut style = None;
        let mut guid_pending: Option<String> = None;
        let context = shared.context(None);
        for record in &records {
            match record.kind {
                BRT_FRT_GUID => guid_pending = guid(record.body),
                BRT_BEGIN_LIST => {
                    list = Some(begin_list(record.body)?);
                    list_uid = guid_pending.take();
                }
                BRT_BEGIN_AFILTER => {
                    filter = Some((
                        Cursor::new(record.body).rfx("BrtBeginAFilter")?,
                        guid_pending.take(),
                    ));
                }
                BRT_BEGIN_LIST_COL => {
                    columns.push((
                        begin_list_col(record.body)?,
                        guid_pending.take(),
                        None,
                        None,
                    ));
                }
                BRT_LIST_CC_FMLA | BRT_LIST_TR_FMLA => {
                    let mut c = Cursor::new(record.body);
                    let _flags = c.u8("a table column formula")?;
                    let cce = c.u32("a table column formula")? as usize;
                    let rgce = c.bytes(cce, "a table column formula")?;
                    let cb = c.u32("a table column formula")? as usize;
                    let rgcb = c.bytes(cb, "a table column formula")?;
                    let text =
                        formula::print(rgce, rgcb, &context, Origin::default())?.map_err(|_| {
                            Malformed("a table column formula holding a PtgExp".to_owned())
                        })?;
                    let column = columns
                        .last_mut()
                        .ok_or_else(|| Malformed("a column formula outside a column".to_owned()))?;
                    if record.kind == BRT_LIST_CC_FMLA {
                        column.2 = Some(text);
                    } else {
                        column.3 = Some(text);
                    }
                }
                BRT_TABLE_STYLE_CLIENT => {
                    let mut c = Cursor::new(record.body);
                    let flags = c.u16("BrtTableStyleClient")?;
                    let name = c.nullable_wide("BrtTableStyleClient")?.map(|u| text(&u));
                    style = Some((flags, name));
                }
                _ => {}
            }
        }
        let list = list.ok_or_else(|| Malformed("a table part without BrtBeginList".to_owned()))?;
        let mut attributes = vec![
            ("xmlns", MAIN_NAMESPACE.to_owned()),
            ("xmlns:mc", COMPATIBILITY_NAMESPACE.to_owned()),
            ("mc:Ignorable", "xr xr3".to_owned()),
            ("xmlns:xr", REVISION_NAMESPACE.to_owned()),
            ("xmlns:xr3", REVISION3_NAMESPACE.to_owned()),
            ("id", list.id.to_string()),
        ];
        if let Some(uid) = &list_uid {
            attributes.push(("xr:uid", uid.clone()));
        }
        attributes.push(("name", list.name.clone()));
        attributes.push(("displayName", list.display_name.clone()));
        if let Some(comment) = &list.comment {
            attributes.push(("comment", comment.clone()));
        }
        attributes.push(("ref", area_text(&list.area)));
        match list.kind {
            0 => {}
            1 => attributes.push(("tableType", "xml".to_owned())),
            2 => attributes.push(("tableType", "queryTable".to_owned())),
            other => return Err(Malformed(format!("the table type {other}"))),
        }
        if list.header_rows != 1 {
            attributes.push(("headerRowCount", list.header_rows.to_string()));
        }
        if list.flags & 0x08 != 0 {
            attributes.push(("insertRow", "1".to_owned()));
        }
        if list.flags & 0x04 != 0 {
            attributes.push(("insertRowShift", "1".to_owned()));
        }
        if list.totals_rows != 0 {
            attributes.push(("totalsRowCount", list.totals_rows.to_string()));
        }
        if list.flags & 0x01 == 0 {
            attributes.push(("totalsRowShown", "0".to_owned()));
        }
        if list.flags & 0x10 != 0 {
            attributes.push(("published", "1".to_owned()));
        }
        for (dxf, name) in list.dxfs.iter().zip([
            "headerRowDxfId",
            "dataDxfId",
            "totalsRowDxfId",
            "tableBorderDxfId",
            "headerRowBorderDxfId",
            "totalsRowBorderDxfId",
        ]) {
            if *dxf != u32::MAX {
                attributes.push((name, dxf.to_string()));
            }
        }
        for (style, name) in
            list.styles
                .iter()
                .zip(["headerRowCellStyle", "dataCellStyle", "totalsRowCellStyle"])
        {
            if let Some(style) = style {
                attributes.push((name, style.clone()));
            }
        }
        if list.connection != 0 {
            attributes.push(("connectionId", list.connection.to_string()));
        }
        x.open("table", &attributes);
        if let Some((area, uid)) = &filter {
            let mut attributes = vec![("ref", area_text(area))];
            if let Some(uid) = uid {
                attributes.push(("xr:uid", uid.clone()));
            }
            x.empty("autoFilter", &attributes);
        }
        x.open("tableColumns", &[("count", columns.len().to_string())]);
        for (column, uid, calculated, totals) in &columns {
            let mut attributes = vec![("id", column.id.to_string())];
            if let Some(uid) = uid {
                attributes.push(("xr3:uid", uid.clone()));
            }
            attributes.push(("name", column.name.clone()));
            if column.function != 0 {
                let name = TOTAL_FUNCTIONS
                    .get(column.function as usize)
                    .ok_or_else(|| Malformed(format!("the totals function {}", column.function)))?;
                attributes.push(("totalsRowFunction", (*name).to_owned()));
            }
            if let Some(label) = &column.total_label {
                attributes.push(("totalsRowLabel", label.clone()));
            }
            for (dxf, name) in
                column
                    .dxfs
                    .iter()
                    .zip(["headerRowDxfId", "dataDxfId", "totalsRowDxfId"])
            {
                if *dxf != u32::MAX {
                    attributes.push((name, dxf.to_string()));
                }
            }
            for (style, name) in column.styles.iter().zip([
                "headerRowCellStyle",
                "dataCellStyle",
                "totalsRowCellStyle",
            ]) {
                if let Some(style) = style {
                    attributes.push((name, style.clone()));
                }
            }
            if calculated.is_none() && totals.is_none() {
                x.empty("tableColumn", &attributes);
                continue;
            }
            x.open("tableColumn", &attributes);
            if let Some(text) = calculated {
                x.element("calculatedColumnFormula", &[], text);
            }
            if let Some(text) = totals {
                x.element("totalsRowFormula", &[], text);
            }
            x.close("tableColumn");
        }
        x.close("tableColumns");
        if let Some((flags, name)) = &style {
            let mut attributes = Vec::new();
            if let Some(name) = name {
                attributes.push(("name", name.clone()));
            }
            for (bit, name) in [
                (0, "showFirstColumn"),
                (1, "showLastColumn"),
                (2, "showRowStripes"),
                (3, "showColumnStripes"),
            ] {
                attributes.push((
                    name,
                    if flags & (1 << bit) != 0 { "1" } else { "0" }.to_owned(),
                ));
            }
            x.empty("tableStyleInfo", &attributes);
        }
        x.close("table");
        Ok(x.out)
    })()
    .map_err(malformed(part))
}

/// An external link part's sheet names and defined names.
fn external_names(data: &[u8]) -> Read<(Vec<String>, Vec<String>)> {
    let records = records(data)?;
    let mut sheets = Vec::new();
    let mut names = Vec::new();
    for record in &records {
        let mut c = Cursor::new(record.body);
        match record.kind {
            BRT_SUP_TABS => {
                let count = c.u32("BrtSupTabs")?;
                for _ in 0..count {
                    sheets.push(text(&c.wide("BrtSupTabs")?));
                }
            }
            BRT_SUP_NAME_START => names.push(text(&c.wide("BrtSupNameStart")?)),
            _ => {}
        }
    }
    Ok((sheets, names))
}

/// An external link part: the external workbook, its sheet names and the cached values of the
/// cells formulas read. Its defined names are carried by name alone, without what they refer to.
pub(crate) fn external_link(part: &str, data: &[u8]) -> Result<Vec<u8>, OpenError> {
    (|| -> Read<Vec<u8>> {
        let records = records(data)?;
        let begin = records
            .iter()
            .find(|r| r.kind == BRT_BEGIN_SUP_BOOK)
            .ok_or_else(|| Malformed("an external link part without BrtBeginSupBook".to_owned()))?;
        let mut c = Cursor::new(begin.body);
        let kind = c.u16("BrtBeginSupBook")?;
        if kind != 0 {
            return Err(Malformed(format!(
                "an external link of type {kind} (a DDE or OLE link), which is not read"
            )));
        }
        let relationship = text(&c.wide("BrtBeginSupBook")?);
        let (sheets, names) = external_names(data)?;
        let mut x = Xml::new();
        x.open("externalLink", &[("xmlns", MAIN_NAMESPACE.to_owned())]);
        x.open(
            "externalBook",
            &[
                ("xmlns:r", RELATIONSHIPS_NAMESPACE.to_owned()),
                ("r:id", relationship),
            ],
        );
        if !sheets.is_empty() {
            x.open("sheetNames", &[]);
            for sheet in &sheets {
                x.empty("sheetName", &[("val", sheet.clone())]);
            }
            x.close("sheetNames");
        }
        if !names.is_empty() {
            x.open("definedNames", &[]);
            for name in &names {
                x.empty("definedName", &[("name", name.clone())]);
            }
            x.close("definedNames");
        }
        let mut tables = false;
        let mut row: Option<u32> = None;
        for record in &records {
            let mut c = Cursor::new(record.body);
            let close_row = |x: &mut Xml, row: &mut Option<u32>| {
                if row.take().is_some() {
                    x.close("row");
                }
            };
            match record.kind {
                BRT_EXTERN_TABLE_START => {
                    if !tables {
                        x.open("sheetDataSet", &[]);
                        tables = true;
                    }
                    let tab = c.u32("BrtExternTableStart")?;
                    let flags = c.u8("BrtExternTableStart")?;
                    let mut attributes = vec![("sheetId", tab.to_string())];
                    if flags & 1 != 0 {
                        attributes.push(("refreshError", "1".to_owned()));
                    }
                    x.open("sheetData", &attributes);
                }
                BRT_EXTERN_TABLE_END => {
                    close_row(&mut x, &mut row);
                    x.close("sheetData");
                }
                BRT_EXTERN_ROW_HDR => {
                    close_row(&mut x, &mut row);
                    let r = c.u32("BrtExternRowHdr")?;
                    x.open("row", &[("r", (r + 1).to_string())]);
                    row = Some(r);
                }
                BRT_EXTERN_CELL_BLANK..=BRT_EXTERN_CELL_STRING => {
                    let r =
                        row.ok_or_else(|| Malformed("an external cell outside a row".to_owned()))?;
                    let column = c.u32("an external cell")?;
                    let reference = a1(r + 1, column + 1);
                    let (kind, value) = match record.kind {
                        BRT_EXTERN_CELL_BLANK => (None, None),
                        BRT_EXTERN_CELL_REAL => (
                            None,
                            Some(formula::number_text(c.f64("BrtExternCellReal")?)),
                        ),
                        BRT_EXTERN_CELL_BOOL => (
                            Some("b"),
                            Some(c.u8("BrtExternCellBool")?.min(1).to_string()),
                        ),
                        BRT_EXTERN_CELL_ERROR => {
                            let code = c.u8("BrtExternCellError")?;
                            let error = formula::error_text(code)
                                .ok_or_else(|| Malformed(format!("the error code 0x{code:02X}")))?;
                            (Some("e"), Some(error.to_owned()))
                        }
                        _ => (Some("str"), Some(text(&c.wide("BrtExternCellString")?))),
                    };
                    let mut attributes = vec![("r", reference)];
                    if let Some(kind) = kind {
                        attributes.push(("t", kind.to_owned()));
                    }
                    match value {
                        None => x.empty("cell", &attributes),
                        Some(value) => {
                            x.open("cell", &attributes);
                            x.element("v", &[], &value);
                            x.close("cell");
                        }
                    }
                }
                _ => {}
            }
        }
        if tables {
            x.close("sheetDataSet");
        }
        x.close("externalBook");
        x.close("externalLink");
        Ok(x.out)
    })()
    .map_err(malformed(part))
}

/// A comments part.
pub(crate) fn comments(part: &str, data: &[u8], shared: &Shared) -> Result<Vec<u8>, OpenError> {
    (|| -> Read<Vec<u8>> {
        let records = records(data)?;
        let mut x = Xml::new();
        x.open(
            "comments",
            &[
                ("xmlns", MAIN_NAMESPACE.to_owned()),
                ("xmlns:mc", COMPATIBILITY_NAMESPACE.to_owned()),
                ("mc:Ignorable", "xr".to_owned()),
                ("xmlns:xr", REVISION_NAMESPACE.to_owned()),
            ],
        );
        x.open("authors", &[]);
        for record in records.iter().filter(|r| r.kind == BRT_COMMENT_AUTHOR) {
            let author = Cursor::new(record.body).wide("BrtCommentAuthor")?;
            x.open("author", &[]);
            x.text(&author);
            x.close("author");
        }
        x.close("authors");
        x.open("commentList", &[]);
        let mut guid_pending = None;
        let mut open: Option<Vec<(&'static str, String)>> = None;
        for record in &records {
            match record.kind {
                BRT_FRT_GUID => guid_pending = guid(record.body),
                BRT_BEGIN_COMMENT => {
                    let mut c = Cursor::new(record.body);
                    let author = c.u32("BrtBeginComment")?;
                    let area = c.rfx("BrtBeginComment")?;
                    let mut attributes = vec![
                        ("ref", area_text(&area)),
                        ("authorId", author.to_string()),
                        ("shapeId", "0".to_owned()),
                    ];
                    let own = c.bytes(16, "BrtBeginComment").ok().and_then(guid);
                    let uid = guid_pending
                        .take()
                        .or(own.filter(|g| g != "{00000000-0000-0000-0000-000000000000}"));
                    if let Some(uid) = uid {
                        attributes.push(("xr:uid", uid));
                    }
                    open = Some(attributes);
                }
                BRT_COMMENT_TEXT => {
                    let attributes = open.take().ok_or_else(|| {
                        Malformed("a comment's text outside a comment".to_owned())
                    })?;
                    let rich = rich_text(&mut Cursor::new(record.body))?;
                    x.open("comment", &attributes);
                    x.open("text", &[]);
                    write_rich(&mut x, &rich, &shared.fonts)?;
                    x.close("text");
                    x.close("comment");
                }
                _ => {}
            }
        }
        x.close("commentList");
        x.close("comments");
        Ok(x.out)
    })()
    .map_err(malformed(part))
}

/// The attributes of a metadata type's flags (MdtFlags, 2.5.95), as SpreadsheetML names them.
const METADATA_FLAGS: [(u32, &str); 26] = [
    (0, "ghostRow"),
    (1, "ghostCol"),
    (2, "edit"),
    (3, "delete"),
    (4, "copy"),
    (5, "pasteAll"),
    (6, "pasteFormulas"),
    (7, "pasteValues"),
    (8, "pasteFormats"),
    (9, "pasteComments"),
    (10, "pasteDataValidation"),
    (11, "pasteBorders"),
    (12, "pasteColWidths"),
    (13, "pasteNumberFormats"),
    (14, "merge"),
    (15, "splitFirst"),
    (16, "splitAll"),
    (17, "rowColShift"),
    (18, "clearAll"),
    (19, "clearFormats"),
    (20, "clearContents"),
    (21, "clearComments"),
    (22, "assign"),
    (28, "coerce"),
    (29, "adjust"),
    (30, "cellMeta"),
];

/// A metadata part: its types, its future metadata (dynamic array properties and rich value
/// blocks) and its cell and value metadata blocks. MDX metadata is refused.
pub(crate) fn metadata(part: &str, data: &[u8]) -> Result<Vec<u8>, OpenError> {
    (|| -> Read<Vec<u8>> {
        let records = records(data)?;
        let mut types: Vec<(u32, u32, String)> = Vec::new();
        // Each future metadata store: its name and its blocks' contents.
        let mut future: Vec<(String, Vec<String>)> = Vec::new();
        // Each metadata block collection: whether cell metadata, and its blocks of (type, value).
        let mut blocks: Vec<(bool, Vec<Vec<(u32, u32)>>)> = Vec::new();
        let mut uses_dynamic = false;
        let mut uses_rich = false;
        for record in &records {
            let mut c = Cursor::new(record.body);
            match record.kind {
                BRT_MDTINFO => {
                    let flags = c.u32("BrtMdtinfo")?;
                    let version = c.u32("BrtMdtinfo")?;
                    types.push((flags, version, text(&c.wide("BrtMdtinfo")?)));
                }
                BRT_BEGIN_ESFMD => {
                    let _count = c.u32("BrtBeginEsfmd")?;
                    future.push((text(&c.wide("BrtBeginEsfmd")?), Vec::new()));
                }
                BRT_BEGIN_FMD => {
                    future
                        .last_mut()
                        .ok_or_else(|| Malformed("a future metadata block outside its store".to_owned()))?
                        .1
                        .push(String::new());
                }
                BRT_END_DYNAMIC_ARRAY_PR => {
                    let dynamic = c.u8("BrtEndDynamicArrayPr")?;
                    let collapsed = c.u8("BrtEndDynamicArrayPr")?;
                    uses_dynamic = true;
                    let block = future
                        .last_mut()
                        .and_then(|f| f.1.last_mut())
                        .ok_or_else(|| Malformed("dynamic array properties outside a block".to_owned()))?;
                    *block = format!(
                        "<extLst><ext uri=\"{{bdbb8cdc-fa1e-496e-a857-3c3f30c029c3}}\"><xda:dynamicArrayProperties fDynamic=\"{}\" fCollapsed=\"{}\"/></ext></extLst>",
                        dynamic.min(1),
                        collapsed.min(1)
                    );
                }
                BRT_END_RICH_VALUE_BLOCK => {
                    let index = c.u32("BrtEndRichValueBlock")?;
                    uses_rich = true;
                    let block = future
                        .last_mut()
                        .and_then(|f| f.1.last_mut())
                        .ok_or_else(|| Malformed("a rich value block outside a block".to_owned()))?;
                    *block = format!(
                        "<extLst><ext uri=\"{{3e2802c4-a4d2-4d8b-9148-e3be6c30e623}}\"><xlrd:rvb i=\"{index}\"/></ext></extLst>"
                    );
                }
                BRT_END_FMD => {}
                BRT_BEGIN_ESMDB => {
                    let _count = c.u32("BrtBeginEsmdb")?;
                    let cell = c.u32("BrtBeginEsmdb")? != 0;
                    blocks.push((cell, Vec::new()));
                }
                BRT_MDB => {
                    let count = c.u32("BrtMdb")?;
                    let mut items = Vec::new();
                    for _ in 0..count {
                        items.push((c.u32("Mdir")?, c.u32("Mdir")?));
                    }
                    blocks
                        .last_mut()
                        .ok_or_else(|| Malformed("a metadata block outside its collection".to_owned()))?
                        .1
                        .push(items);
                }
                334 | 336 | 338 | 340 | 332 | 333 | 35 | 36 | 37 | 38 | 4096 | 5002 => {}
                other => return Err(Malformed(format!("the metadata record {other}"))),
            }
        }
        let mut x = Xml::new();
        let mut attributes = vec![("xmlns", MAIN_NAMESPACE.to_owned())];
        if uses_dynamic {
            attributes.push((
                "xmlns:xda",
                "http://schemas.microsoft.com/office/spreadsheetml/2017/dynamicarray".to_owned(),
            ));
        }
        if uses_rich {
            attributes.push((
                "xmlns:xlrd",
                "http://schemas.microsoft.com/office/spreadsheetml/2017/richdata".to_owned(),
            ));
        }
        x.open("metadata", &attributes);
        x.open("metadataTypes", &[("count", types.len().to_string())]);
        for (flags, version, name) in &types {
            let mut attributes = vec![
                ("name", name.clone()),
                ("minSupportedVersion", version.to_string()),
            ];
            for (bit, attribute) in METADATA_FLAGS {
                if flags & (1 << bit) != 0 {
                    attributes.push((attribute, "1".to_owned()));
                }
            }
            x.empty("metadataType", &attributes);
        }
        x.close("metadataTypes");
        for (name, items) in &future {
            x.open(
                "futureMetadata",
                &[("name", name.clone()), ("count", items.len().to_string())],
            );
            for item in items {
                x.open("bk", &[]);
                x.out.extend_from_slice(item.as_bytes());
                x.close("bk");
            }
            x.close("futureMetadata");
        }
        // Cell metadata before value metadata, as the schema orders them.
        for want_cell in [true, false] {
            for (cell, items) in blocks.iter().filter(|b| b.0 == want_cell) {
                let name = if *cell { "cellMetadata" } else { "valueMetadata" };
                x.open(name, &[("count", items.len().to_string())]);
                for block in items {
                    x.open("bk", &[]);
                    for (kind, value) in block {
                        x.empty("rc", &[("t", kind.to_string()), ("v", value.to_string())]);
                    }
                    x.close("bk");
                }
                x.close(name);
            }
        }
        x.close("metadata");
        Ok(x.out)
    })()
    .map_err(malformed(part))
}
