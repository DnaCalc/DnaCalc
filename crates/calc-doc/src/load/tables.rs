//! A worksheet's tables (ListObjects): each table part its relationships name, read for its name,
//! display name, area, header and totals row counts and column names. The parts themselves stay
//! in the package as stored.

use super::{Events, LoadError, MAIN_NAMESPACE, address, read_xml, sheet::collapsed};
use crate::opc::{Package, TargetMode};
use crate::xml::XmlEvent;
use calc_doc_model::Table;
use calc_doc_model::value::{CellKey, GridArea, SheetId};

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

fn read_table(package: &Package, part: &str, sheet: SheetId) -> Result<Table, LoadError> {
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
    let name = root.attribute("name").unwrap_or_default().to_owned();
    let display_name = root
        .attribute("displayName")
        .map_or_else(|| name.clone(), str::to_owned);
    let mut columns = Vec::new();
    let mut depth = 1u32;
    while depth > 0 {
        match events.next()? {
            Some(XmlEvent::Start(element)) => {
                depth += 1;
                if depth == 3 && element.name.is(MAIN_NAMESPACE, "tableColumn") {
                    columns.push(element.attribute("name").unwrap_or_default().to_owned());
                }
            }
            Some(XmlEvent::End(_)) => depth -= 1,
            Some(XmlEvent::Text(_)) => {}
            None => return Err(events.invalid("the part ends inside table")),
        }
    }
    events.end()?;
    Ok(Table {
        name,
        display_name,
        part: part.to_owned(),
        sheet,
        area,
        header_rows,
        totals_rows,
        columns,
    })
}

/// The tables the worksheet part `sheet_part` relates to, in relationship order.
pub(crate) fn read_tables(
    package: &Package,
    sheet_part: &str,
    sheet: SheetId,
) -> Result<Vec<Table>, LoadError> {
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
