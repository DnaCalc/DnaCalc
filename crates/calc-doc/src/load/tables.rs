//! A worksheet's tables (ListObjects): each table part its relationships name, read for its name,
//! display name, area, header and totals row counts and column names. The parts themselves stay
//! in the package as stored.

use super::{Events, LoadError, MAIN_NAMESPACE, address, read_xml, sheet::collapsed};
use crate::opc::{Package, TargetMode};
use crate::xml::XmlEvent;
use calc_doc_model::value::{CellKey, GridArea, SheetId};
use calc_doc_model::{Table, TableStyle};

pub(crate) const TABLE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/table";

/// A table's `headerRowCount` (1 when absent) or `totalsRowCount` (0 when absent).
fn count(events: &Events<'_>, value: Option<&str>, absent: u32) -> Result<u32, LoadError> {
    match value {
        None => Ok(absent),
        Some(text) => collapsed(text)
            .parse()
            .map_err(|_| events.invalid("a table whose row count is not a count")),
    }
}

fn read_table(
    package: &Package,
    part: &str,
    sheet: SheetId,
) -> Result<(Table, Option<std::collections::BTreeSet<u32>>), LoadError> {
    let xml = read_xml(package, part)?;
    let mut events = Events::new(&xml);
    let root = events.root(
        MAIN_NAMESPACE,
        "table",
        "the document element is not a SpreadsheetML table",
    )?;
    let (top, left, bottom, right) = root
        .attribute("ref")
        .and_then(address::area)
        .ok_or_else(|| events.invalid("a table without a ref that is an area"))?;
    let key = |row, column| CellKey::new(sheet, row, column).expect("an area inside the grid");
    let area = GridArea::new(key(top, left), key(bottom, right)).expect("one sheet");
    let header_rows = count(&events, root.attribute("headerRowCount"), 1)?;
    let totals_rows = count(&events, root.attribute("totalsRowCount"), 0)?;
    let decode = |s: &str| String::from_utf16_lossy(&super::strings::decode(s));
    let id = root
        .attribute("id")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let name = decode(root.attribute("name").unwrap_or_default());
    let display_name = root
        .attribute("displayName")
        .map_or_else(|| name.clone(), decode);
    let mut column_formulas = Vec::new();
    let mut columns = Vec::new();
    let mut totals_functions = Vec::new();
    let mut totals_labels = Vec::new();
    let mut style = TableStyle::default();
    let mut filter = None;
    let mut manual = None;
    let mut opaque = root
        .attribute("tableType")
        .is_some_and(|s| s != "worksheet")
        || root.attribute("connectionId").is_some();
    let mut axis_opaque = opaque;
    loop {
        match events.next()? {
            Some(XmlEvent::Start(e)) if e.name.is(MAIN_NAMESPACE, "tableColumns") => loop {
                match events.next()? {
                    Some(XmlEvent::Start(e)) if e.name.is(MAIN_NAMESPACE, "tableColumn") => {
                        let linked = e.attribute("queryTableFieldId").is_some()
                            || e.attribute("uniqueName").is_some();
                        opaque |= linked;
                        axis_opaque |= linked;
                        columns.push(decode(e.attribute("name").unwrap_or_default()));
                        column_formulas.push(None);
                        totals_functions.push(e.attribute("totalsRowFunction").map(str::to_owned));
                        totals_labels.push(e.attribute("totalsRowLabel").map(decode));
                        loop {
                            match events.next()? {
                                Some(XmlEvent::Start(e))
                                    if e.name.is(MAIN_NAMESPACE, "calculatedColumnFormula") =>
                                {
                                    opaque = true;
                                    let formula = events.text()?;
                                    *column_formulas.last_mut().unwrap() = Some(decode(&formula));
                                }
                                Some(XmlEvent::Start(_)) => {
                                    opaque = true;
                                    axis_opaque = true;
                                    events.skip()?;
                                }
                                Some(XmlEvent::End(_)) => break,
                                Some(XmlEvent::Text(_)) => {}
                                None => {
                                    return Err(events.invalid("table column ends unexpectedly"));
                                }
                            }
                        }
                    }
                    Some(XmlEvent::Start(_)) => {
                        opaque = true;
                        axis_opaque = true;
                        events.skip()?;
                    }
                    Some(XmlEvent::End(_)) => break,
                    Some(XmlEvent::Text(_)) => {}
                    None => return Err(events.invalid("table columns end unexpectedly")),
                }
            },
            Some(XmlEvent::Start(e)) if e.name.is(MAIN_NAMESPACE, "tableStyleInfo") => {
                let flag = |name| matches!(e.attribute(name), Some("1" | "true"));
                style = TableStyle {
                    name: e.attribute("name").map(decode),
                    first_column: flag("showFirstColumn"),
                    last_column: flag("showLastColumn"),
                    row_stripes: flag("showRowStripes"),
                    column_stripes: flag("showColumnStripes"),
                };
                events.skip()?;
            }
            Some(XmlEvent::Start(e)) if e.name.is(MAIN_NAMESPACE, "autoFilter") => {
                let parsed = super::filter::read(&mut events, &e, sheet)?;
                filter = parsed.0;
                manual = parsed.1;
            }
            Some(XmlEvent::Start(_)) => {
                opaque = true;
                axis_opaque = true;
                events.skip()?;
            }
            Some(XmlEvent::End(_)) => break,
            Some(XmlEvent::Text(_)) => {}
            None => return Err(events.invalid("table ends unexpectedly")),
        }
    }
    events.end()?;
    Ok((
        Table {
            id,
            style,
            filter,
            totals_functions,
            totals_labels,
            opaque,
            axis_opaque,
            column_formulas,
            name,
            display_name,
            part: part.to_owned(),
            sheet,
            area,
            header_rows,
            totals_rows,
            columns,
        },
        manual,
    ))
}

/// The tables the worksheet part `sheet_part` relates to, in relationship order.
pub(crate) fn read_tables(
    package: &Package,
    sheet_part: &str,
    sheet: SheetId,
) -> Result<Vec<(Table, Option<std::collections::BTreeSet<u32>>)>, LoadError> {
    let Some(relationships) = package.relationships_of(Some(sheet_part)) else {
        return Ok(Vec::new());
    };
    let mut tables = Vec::new();
    for relationship in relationships.items() {
        if relationship.kind != TABLE || relationship.mode != TargetMode::Internal {
            continue;
        }
        let part = relationship
            .resolved
            .clone()
            .ok_or_else(|| LoadError::MissingPart {
                part: relationship.target.clone(),
            })?;
        tables.push(read_table(package, &part, sheet)?);
    }
    Ok(tables)
}
