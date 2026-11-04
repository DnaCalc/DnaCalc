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
                                    axis_opaque |= e.attribute("array").is_some_and(|v| !matches!(v, "0" | "false"));
                                    let formula = events.text()?;
                                    axis_opaque |= !calc_fml::parse_formula(&decode(&formula)).diagnostics().is_empty();
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

/// Row sorting leaves table definitions in place. Admit only complete, ordinary source
/// records, without per-row extensions, calculated columns or recorded sort criteria.
pub(super) fn sorting_supported(
    package: &Package,
    sheet_part: &str,
    sheet: SheetId,
    tables: &[Table],
) -> bool {
    use crate::xml::XmlElement;
    use std::collections::BTreeSet;
    fn attrs(e: &XmlElement, allowed: &[&str]) -> bool {
        e.attributes.iter().all(|a| {
            (a.name.namespace.is_none() && allowed.contains(&a.name.local.as_str()))
            || a.name.is("http://schemas.microsoft.com/office/spreadsheetml/2014/revision", "uid")
            || a.name.is("http://schemas.microsoft.com/office/spreadsheetml/2016/revision3", "uid")
            // Every actual attribute/child still passes this complete whitelist. An unchanged
            // Ignorable declaration cannot admit an unrecognised extension record.
            || a.name.is("http://schemas.openxmlformats.org/markup-compatibility/2006", "Ignorable")
        })
    }
    fn empty(events: &mut Events<'_>) -> Result<bool, LoadError> {
        loop {
            match events.next()? {
                Some(XmlEvent::Text(s)) if s.trim().is_empty() => {}
                Some(XmlEvent::End(_)) => return Ok(true),
                _ => return Ok(false),
            }
        }
    }
    fn table_source(package: &Package, table: &Table) -> Result<bool, LoadError> {
        let mut names = std::collections::BTreeSet::new();
        if table.opaque
            || table.axis_opaque
            || table.columns.len() != table.area.column_count() as usize
            || table.columns.iter().any(|name| {
                name.trim().is_empty()
                    || !names.insert(calc_doc_model::value::fold_workbook_name(name))
            })
            || package
                .relationships_of(Some(&table.part))
                .is_some_and(|r| !r.items().is_empty())
        {
            return Ok(false);
        }
        let xml = read_xml(package, &table.part)?;
        let mut events = Events::new(&xml);
        let root = events.root(MAIN_NAMESPACE, "table", "table source is unavailable")?;
        if !attrs(
            &root,
            &[
                "id",
                "name",
                "displayName",
                "ref",
                "headerRowCount",
                "totalsRowCount",
                "totalsRowShown",
            ],
        ) || root
            .attribute("id")
            .and_then(|s| s.parse::<u32>().ok())
            .is_none_or(|n| n == 0)
        {
            return Ok(false);
        }
        let (mut columns, mut filter, mut style) = (false, false, false);
        loop {
            match events.next()? {
                Some(XmlEvent::Start(e)) if e.name.is(MAIN_NAMESPACE, "tableColumns") => {
                    if columns
                        || !attrs(&e, &["count"])
                        || e.attribute("count").and_then(|s| s.parse::<usize>().ok())
                            != Some(table.columns.len())
                    {
                        return Ok(false);
                    }
                    columns = true;
                    let mut ids = BTreeSet::new();
                    let mut count = 0;
                    loop {
                        match events.next()? {
                            Some(XmlEvent::Start(e))
                                if e.name.is(MAIN_NAMESPACE, "tableColumn") =>
                            {
                                if !attrs(
                                    &e,
                                    &[
                                        "id",
                                        "name",
                                        "totalsRowFunction",
                                        "totalsRowLabel",
                                        "dataDxfId",
                                        "headerRowDxfId",
                                        "totalsRowDxfId",
                                    ],
                                ) || e
                                    .attribute("id")
                                    .and_then(|s| s.parse::<u32>().ok())
                                    .is_none_or(|id| id == 0 || !ids.insert(id))
                                    || e.attribute("name").is_none()
                                    || !empty(&mut events)?
                                {
                                    return Ok(false);
                                }
                                count += 1;
                            }
                            Some(XmlEvent::Text(s)) if s.trim().is_empty() => {}
                            Some(XmlEvent::End(_)) => break,
                            _ => return Ok(false),
                        }
                    }
                    if count != table.columns.len() {
                        return Ok(false);
                    }
                }
                Some(XmlEvent::Start(e)) if e.name.is(MAIN_NAMESPACE, "autoFilter") => {
                    if filter {
                        return Ok(false);
                    }
                    filter = true;
                    let (filter, _) = super::filter::read(&mut events, &e, table.sheet)?;
                    if filter.is_none_or(|f| f.unsupported) {
                        return Ok(false);
                    }
                }
                Some(XmlEvent::Start(e)) if e.name.is(MAIN_NAMESPACE, "tableStyleInfo") => {
                    if style
                        || !attrs(
                            &e,
                            &[
                                "name",
                                "showFirstColumn",
                                "showLastColumn",
                                "showRowStripes",
                                "showColumnStripes",
                            ],
                        )
                        || !empty(&mut events)?
                    {
                        return Ok(false);
                    }
                    style = true;
                }
                Some(XmlEvent::Text(s)) if s.trim().is_empty() => {}
                Some(XmlEvent::End(_)) => break,
                _ => return Ok(false),
            }
        }
        events.end()?;
        Ok(columns)
    }
    let check = || -> Result<bool, LoadError> {
        let related = package
            .relationships_of(Some(sheet_part))
            .map(|r| {
                r.items()
                    .iter()
                    .filter(|r| r.kind == TABLE)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let originals = tables
            .iter()
            .filter(|t| t.sheet == sheet)
            .collect::<Vec<_>>();
        let mut table_ids = BTreeSet::new();
        if tables.iter().any(|t| t.id == 0 || !table_ids.insert(t.id)) {
            return Ok(false);
        }
        if related.len() != originals.len() {
            return Ok(false);
        }
        let mut ids = BTreeSet::new();
        let mut targets = BTreeSet::new();
        for rel in related {
            let Some(target) = rel
                .resolved
                .as_deref()
                .filter(|_| rel.mode == TargetMode::Internal)
            else {
                return Ok(false);
            };
            if !ids.insert(rel.id.clone())
                || !targets.insert(target.to_ascii_lowercase())
                || package
                    .relationships()
                    .iter()
                    .flat_map(|r| r.items())
                    .filter(|r| {
                        r.resolved
                            .as_deref()
                            .is_some_and(|p| p.eq_ignore_ascii_case(target))
                    })
                    .count()
                    != 1
            {
                return Ok(false);
            }
            let Some(table) = originals.iter().find(|t| t.part == target) else {
                return Ok(false);
            };
            if !table_source(package, table)? {
                return Ok(false);
            }
        }
        let xml = read_xml(package, sheet_part)?;
        let mut events = Events::new(&xml);
        events.root(
            MAIN_NAMESPACE,
            "worksheet",
            "table worksheet is unavailable",
        )?;
        let mut seen = false;
        loop {
            match events.next()? {
                Some(XmlEvent::Start(e)) if e.name.local == "tableParts" => {
                    if seen
                        || !e.name.is(MAIN_NAMESPACE, "tableParts")
                        || !attrs(&e, &["count"])
                        || e.attribute("count").and_then(|s| s.parse::<usize>().ok())
                            != Some(ids.len())
                    {
                        return Ok(false);
                    }
                    seen = true;
                    loop {
                        match events.next()? {
                            Some(XmlEvent::Start(e)) if e.name.is(MAIN_NAMESPACE, "tablePart") => {
                                if e.attributes.len() != 1 {
                                    return Ok(false);
                                }
                                let a = &e.attributes[0];
                                if !a.name.is("http://schemas.openxmlformats.org/officeDocument/2006/relationships","id") || !ids.remove(&a.value) || !empty(&mut events)? {return Ok(false);}
                            }
                            Some(XmlEvent::Text(s)) if s.trim().is_empty() => {}
                            Some(XmlEvent::End(_)) => break,
                            _ => return Ok(false),
                        }
                    }
                }
                Some(XmlEvent::Start(_)) => events.skip()?,
                Some(XmlEvent::Text(s)) if s.trim().is_empty() => {}
                Some(XmlEvent::End(_)) => break,
                _ => return Ok(false),
            }
        }
        events.end()?;
        Ok(ids.is_empty())
    };
    check().unwrap_or(false)
}
