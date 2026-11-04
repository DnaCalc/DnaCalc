//! Table commands author cells and table definitions in the same workbook transaction.
use crate::{CellEdit, CellInput, CellText, FormulaText, HostState};
use calc_doc_model::{AutoFilter, Table, TableStyle};
use calc_value::{CellKey, ExcelText, GridArea, fold_workbook_name};
use std::collections::BTreeSet;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TableChange {
    Create {
        area: GridArea,
        name: String,
    },
    Resize {
        part: String,
        area: GridArea,
    },
    Options {
        part: String,
        headers: bool,
        totals: bool,
        style: String,
        row_stripes: bool,
        column_stripes: bool,
        first_column: bool,
        last_column: bool,
    },
    Total {
        part: String,
        column: u32,
        function: String,
    },
}
fn refusal(s: &str) -> String {
    format!("{s} (dc-971p.41); nothing changed")
}
fn cell_text(host: &HostState, at: CellKey) -> Result<String, String> {
    use crate::{ProjectedValue, ProjectionRequest};
    use calc_value::CoreRef;
    let page =
        crate::project(host, &ProjectionRequest { cells: vec![at] }).map_err(|e| e.to_string())?;
    let value = match page.cells().first().map(|c| c.value()) {
        Some(ProjectedValue::Empty) => return Ok(String::new()),
        Some(ProjectedValue::Literal(v) | ProjectedValue::Calculated { value: v, .. }) => v,
        _ => {
            return Err(refusal(
                "Calculate header formulas before creating the table",
            ));
        }
    };
    Ok(match value.core_ref() {
        CoreRef::Text(t) => t.to_string_lossy(),
        CoreRef::Number(n) => n.to_string(),
        CoreRef::Logical(b) => if b { "TRUE" } else { "FALSE" }.into(),
        CoreRef::Error(e) => e.excel_spelling().into(),
        CoreRef::Empty => String::new(),
        _ => return Err(refusal("Table headers need scalar values")),
    })
}
fn text(text: &str) -> CellInput {
    CellInput::Text(
        CellText::new(ExcelText::from_utf16_code_units(
            text.encode_utf16().collect(),
        ))
        .expect("bounded table text"),
    )
}
fn header_names(
    host: &HostState,
    area: GridArea,
    existing: &[String],
) -> Result<Vec<String>, String> {
    let mut used = BTreeSet::new();
    (0..area.column_count())
        .map(|i| {
            let mut name = if let Some(name) = existing.get(i as usize) {
                name.clone()
            } else {
                cell_text(
                    host,
                    CellKey::new(area.sheet(), area.first().row(), area.first().column() + i)
                        .unwrap(),
                )?
            };
            if existing.get(i as usize).is_some() {
                if !used.insert(fold_workbook_name(&name)) {
                    return Err(refusal("Existing table column names must be unique"));
                }
                return Ok(name);
            }
            name = name
                .chars()
                .scan(0, |n, c| {
                    *n += c.len_utf16();
                    (*n <= 255).then_some(c)
                })
                .collect::<String>();
            if name.trim().is_empty() {
                name = format!("Column{}", i + 1);
            }
            let original = name.clone();
            let mut n = 2;
            while !used.insert(fold_workbook_name(&name)) {
                let suffix = n.to_string();
                let limit = 255 - suffix.len();
                let prefix = original
                    .chars()
                    .scan(0, |count, c| {
                        *count += c.len_utf16();
                        (*count <= limit).then_some(c)
                    })
                    .collect::<String>();
                name = format!("{prefix}{suffix}");
                n += 1;
            }
            Ok(name)
        })
        .collect()
}
fn quoted_column(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        if "#[]@'".contains(ch) {
            out.push('\'');
        }
        out.push(ch);
    }
    out
}
fn total_input(table: &Table, index: usize) -> Option<CellInput> {
    if let Some(function) = table.totals_functions.get(index).and_then(|f| f.as_deref()) {
        let code = match function {
            "average" => 101,
            "countNums" => 102,
            "count" => 103,
            "max" => 104,
            "min" => 105,
            "stdDev" => 107,
            "sum" => 109,
            "var" => 110,
            _ => return None,
        };
        let text = format!(
            "SUBTOTAL({code},{}[{}])",
            table.display_name,
            quoted_column(&table.columns[index])
        );
        Some(CellInput::Formula(
            FormulaText::new(ExcelText::from_utf16_code_units(
                text.encode_utf16().collect(),
            ))
            .expect("table total formula"),
        ))
    } else {
        table
            .totals_labels
            .get(index)
            .and_then(|s| s.as_deref())
            .filter(|s| s.encode_utf16().count() <= 32767)
            .map(text)
    }
}
fn total_writes(table: &Table, edits: &mut Vec<CellEdit>, only: Option<usize>) {
    if table.totals_rows > 0 {
        for index in 0..table.columns.len() {
            if only.is_some_and(|i| i != index) {
                continue;
            }
            let at = CellKey::new(
                table.sheet,
                table.area.last().row(),
                table.area.first().column() + index as u32,
            )
            .unwrap();
            edits.push(match total_input(table, index) {
                Some(input) => CellEdit::Set { at, input },
                None => CellEdit::Clear {
                    area: GridArea::cell(at),
                },
            });
        }
    }
}
fn bounds(host: &HostState, area: GridArea, part: Option<&str>) -> Result<(), String> {
    if host.model().sheet_by_id(area.sheet()).is_none()
        || host
            .loaded()
            .sheet(area.sheet())
            .is_some_and(|s| s.protected() || s.is_macro_sheet())
    {
        return Err(refusal("Tables need an editable worksheet"));
    }
    if area.row_count() < 2
        || area.row_count() > 32769
        || area.column_count() > 256
        || area.cell_count() > 262144
    {
        return Err(refusal(
            "Choose a table with 2–32,769 rows, at most 256 columns and 262,144 cells",
        ));
    }
    if host
        .model()
        .definitions()
        .tables()
        .iter()
        .any(|t| Some(t.part.as_str()) != part && t.area.intersection(area).is_some())
        || host
            .model()
            .definitions()
            .auto_filter(area.sheet())
            .is_some_and(|f| f.area.intersection(area).is_some())
    {
        return Err(refusal("Table and AutoFilter ranges cannot overlap"));
    }
    if host
        .merged_areas(area.sheet())
        .iter()
        .any(|a| a.intersection(area).is_some())
        || host
            .model()
            .definitions()
            .arrays()
            .any(|a| a.area().intersection(area).is_some())
        || host
            .model()
            .definitions()
            .unloaded_cells()
            .any(|(at, _)| area.contains(at))
    {
        return Err(refusal(
            "Tables cannot contain merges, arrays or unread cell content",
        ));
    }
    Ok(())
}
pub(crate) fn prepare(
    host: &HostState,
    revision: u64,
    change: TableChange,
) -> Result<Vec<CellEdit>, String> {
    if revision != host.revision().get() {
        return Err(refusal("The workbook changed; reopen the table controls"));
    }
    let mut tables = host.model().definitions().tables().to_vec();
    validate(host, &tables)?;
    let mut edits = Vec::new();
    match change {
        TableChange::Create { area, name } => {
            bounds(host, area, None)?;
            let name = name.trim().to_owned();
            if !calc_engine::valid_defined_name(&name)
                || name.chars().count() > 255
                || tables
                    .iter()
                    .any(|t| fold_workbook_name(&t.display_name) == fold_workbook_name(&name))
                || host
                    .model()
                    .definitions()
                    .names()
                    .iter()
                    .any(|n| fold_workbook_name(&n.name) == fold_workbook_name(&name))
            {
                return Err(refusal("Choose a unique Excel table name"));
            }
            let reserved = tables.iter().map(|t| t.part.clone()).collect::<Vec<_>>();
            let columns = header_names(host, area, &[])?;
            let id = tables
                .iter()
                .chain(host.loaded().model().definitions().tables().iter())
                .map(|t| t.id)
                .max()
                .unwrap_or(0)
                .checked_add(1)
                .ok_or_else(|| refusal("The table ID space is exhausted"))?;
            let table = Table {
                id,
                name: name.clone(),
                display_name: name,
                part: host
                    .loaded()
                    .available_part_name("tables/dnaTable", "xml", &reserved),
                sheet: area.sheet(),
                area,
                header_rows: 1,
                totals_rows: 0,
                totals_functions: vec![None; columns.len()],
                totals_labels: vec![None; columns.len()],
                column_formulas: vec![None; columns.len()],
                columns,
                style: TableStyle {
                    name: Some("TableStyleMedium2".into()),
                    row_stripes: true,
                    ..Default::default()
                },
                filter: Some(AutoFilter::new(area)),
                opaque: false,
                axis_opaque: false,
            };
            for (i, name) in table.columns.iter().enumerate() {
                edits.push(CellEdit::Set {
                    at: CellKey::new(
                        table.sheet,
                        area.first().row(),
                        area.first().column() + i as u32,
                    )
                    .unwrap(),
                    input: text(name),
                });
            }
            tables.push(table);
        }
        change => {
            let part = match &change {
                TableChange::Resize { part, .. }
                | TableChange::Options { part, .. }
                | TableChange::Total { part, .. } => part,
                _ => unreachable!(),
            };
            let index = tables
                .iter()
                .position(|t| &t.part == part)
                .ok_or_else(|| refusal("The table no longer exists"))?;
            let old = tables[index].clone();
            let mut table = old.clone();
            let mut rewrite_totals = false;
            let mut total_column = None;
            match change {
                TableChange::Resize { area, .. } => {
                    if old.opaque {
                        return Err(refusal(
                            "This imported table has calculated columns or other metadata whose resizing is unavailable",
                        ));
                    }
                    bounds(host, area, Some(&old.part))?;
                    if area.first() != old.area.first() {
                        return Err(refusal("Keep the table's first cell when resizing"));
                    }
                    if area.row_count() <= old.header_rows + old.totals_rows {
                        return Err(refusal("Keep at least one data row"));
                    }
                    rewrite_totals = old.totals_rows > 0 && (old.area.last() != area.last());
                    table.area = area;
                    table.columns = if table.header_rows > 0 {
                        header_names(host, area, &old.columns)?
                    } else {
                        (0..area.column_count())
                            .map(|i| {
                                old.columns
                                    .get(i as usize)
                                    .cloned()
                                    .unwrap_or_else(|| format!("Column{}", i + 1))
                            })
                            .collect()
                    };
                    table.column_formulas.resize(table.columns.len(), None);
                    table.totals_functions.resize(table.columns.len(), None);
                    table.totals_labels.resize(table.columns.len(), None);
                    if table.header_rows > 0 {
                        for (i, name) in table.columns.iter().enumerate() {
                            if i >= old.columns.len() {
                                edits.push(CellEdit::Set {
                                    at: CellKey::new(
                                        table.sheet,
                                        area.first().row(),
                                        area.first().column() + i as u32,
                                    )
                                    .unwrap(),
                                    input: text(name),
                                });
                            }
                        }
                    }
                    if old.totals_rows > 0 && old.area.last().row() != area.last().row() {
                        for col in old.area.first().column()..=old.area.last().column() {
                            edits.push(CellEdit::Clear {
                                area: GridArea::cell(
                                    CellKey::new(old.sheet, old.area.last().row(), col).unwrap(),
                                ),
                            });
                        }
                    }
                }
                TableChange::Options {
                    headers,
                    totals,
                    style,
                    row_stripes,
                    column_stripes,
                    first_column,
                    last_column,
                    ..
                } => {
                    if old.opaque
                        && (headers != (old.header_rows > 0) || totals != (old.totals_rows > 0))
                    {
                        return Err(refusal(
                            "This imported table's header and total metadata is preserved",
                        ));
                    }
                    if !matches!(style.as_str(), "" | "TableStyleMedium2")
                        && old.style.name.as_deref() != Some(style.as_str())
                    {
                        return Err(refusal("Choose a supported table style"));
                    }
                    let (mut top, mut bottom) = (old.area.first().row(), old.area.last().row());
                    if headers != (old.header_rows > 0) {
                        if headers {
                            top = top.checked_sub(1).filter(|r| *r > 0).ok_or_else(|| {
                                refusal("There is no row available above the data for headers")
                            })?;
                            for (i, name) in table.columns.iter().enumerate() {
                                let at = CellKey::new(
                                    table.sheet,
                                    top,
                                    table.area.first().column() + i as u32,
                                )
                                .unwrap();
                                if host.model().cell(at).is_some()
                                    && !matches!(host.model().cell(at),Some(CellInput::Text(t))if t.text().to_string_lossy()==*name)
                                {
                                    return Err(refusal(
                                        "Clear the row above the table before showing its headers",
                                    ));
                                }
                                edits.push(CellEdit::Set {
                                    at,
                                    input: text(name),
                                });
                            }
                        } else {
                            top += 1;
                            table.filter = None;
                        }
                        table.header_rows = u32::from(headers);
                    }
                    if totals != (old.totals_rows > 0) {
                        rewrite_totals = totals;
                        if totals {
                            bottom = bottom
                                .checked_add(1)
                                .filter(|r| *r <= calc_value::MAX_ROWS)
                                .ok_or_else(|| refusal("There is no row available for totals"))?;
                            if table.totals_functions.iter().all(Option::is_none)
                                && table.totals_labels.iter().all(Option::is_none)
                            {
                                if let Some(last) = table.totals_functions.last_mut() {
                                    *last = Some("sum".into());
                                }
                                if table.columns.len() > 1 {
                                    table.totals_labels[0] = Some("Total".into());
                                }
                            }
                        } else {
                            for col in old.area.first().column()..=old.area.last().column() {
                                edits.push(CellEdit::Clear {
                                    area: GridArea::cell(
                                        CellKey::new(old.sheet, bottom, col).unwrap(),
                                    ),
                                });
                            }
                            bottom -= 1;
                        }
                        table.totals_rows = u32::from(totals);
                    }
                    table.area = GridArea::new(
                        CellKey::new(table.sheet, top, table.area.first().column()).unwrap(),
                        CellKey::new(table.sheet, bottom, table.area.last().column()).unwrap(),
                    )
                    .unwrap();
                    table.style = TableStyle {
                        name: (!style.is_empty()).then_some(style),
                        row_stripes,
                        column_stripes,
                        first_column,
                        last_column,
                    };
                }
                TableChange::Total {
                    column, function, ..
                } => {
                    if old.opaque || old.totals_rows == 0 {
                        return Err(refusal("Show an editable total row first"));
                    }
                    let i = column
                        .checked_sub(old.area.first().column())
                        .filter(|i| *i < old.area.column_count())
                        .ok_or_else(|| refusal("Choose a table column"))?
                        as usize;
                    if !matches!(
                        function.as_str(),
                        "" | "sum"
                            | "average"
                            | "count"
                            | "countNums"
                            | "min"
                            | "max"
                            | "stdDev"
                            | "var"
                    ) {
                        return Err(refusal("Choose a supported total function"));
                    }
                    rewrite_totals = true;
                    total_column = Some(i);
                    table.totals_functions[i] = (!function.is_empty()).then_some(function);
                    table.totals_labels[i] = None;
                }
                _ => unreachable!(),
            }
            bounds(host, table.area, Some(&old.part))?;
            if table.totals_rows > 0 {
                let moved =
                    old.totals_rows == 0 || old.area.last().row() != table.area.last().row();
                if moved
                    && (table.area.first().column()..=table.area.last().column()).any(|c| {
                        host.model()
                            .cell(CellKey::new(table.sheet, table.area.last().row(), c).unwrap())
                            .is_some()
                    })
                {
                    return Err(refusal(
                        "Clear the destination row before moving or adding totals",
                    ));
                }
                if rewrite_totals {
                    if total_column.is_none() && old.totals_rows > 0 {
                        for i in 0..old.columns.len() {
                            let at = CellKey::new(
                                old.sheet,
                                old.area.last().row(),
                                old.area.first().column() + i as u32,
                            )
                            .unwrap();
                            if host.model().cell(at) != total_input(&old, i).as_ref() {
                                return Err(refusal(
                                    "The existing total row contains custom cells that cannot be moved",
                                ));
                            }
                        }
                    }
                    if table
                        .totals_functions
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| total_column.is_none_or(|c| c == *i))
                        .filter_map(|(_, f)| f.as_ref())
                        .any(|f| {
                            !matches!(
                                f.as_str(),
                                "average"
                                    | "countNums"
                                    | "count"
                                    | "max"
                                    | "min"
                                    | "stdDev"
                                    | "sum"
                                    | "var"
                            )
                        })
                    {
                        return Err(refusal("This imported total function cannot be rewritten"));
                    }
                    total_writes(&table, &mut edits, total_column);
                }
            }
            if table.header_rows > 0 {
                let area = GridArea::new(
                    table.area.first(),
                    CellKey::new(
                        table.sheet,
                        table.area.last().row() - table.totals_rows,
                        table.area.last().column(),
                    )
                    .unwrap(),
                )
                .unwrap();
                let mut filter = table.filter.take().unwrap_or_else(|| AutoFilter::new(area));
                filter.area = area;
                filter
                    .columns
                    .retain(|c, _| *c >= area.first().column() && *c <= area.last().column());
                filter
                    .filtered_rows
                    .retain(|r| *r > area.first().row() && *r <= area.last().row());
                if !filter.unsupported && old.filter.as_ref().is_none_or(|f| f.area != filter.area)
                {
                    filter.filtered_rows = crate::filter::apply(host, &filter, host.styles())?;
                }
                table.filter = Some(filter);
            }
            tables[index] = table;
        }
    }
    if tables != host.model().definitions().tables() {
        edits.push(CellEdit::Tables(tables));
    }
    Ok(edits)
}
pub(crate) fn validate(host: &HostState, tables: &[Table]) -> Result<(), String> {
    if tables.len() > 256 {
        return Err(refusal("A workbook can author at most 256 tables"));
    }
    let mut ids = BTreeSet::new();
    let mut names = BTreeSet::new();
    let mut parts = BTreeSet::new();
    for old in host.model().definitions().tables() {
        if !tables.iter().any(|t| t.part == old.part) {
            bounds(host, old.area, Some(&old.part))?;
        }
    }
    for table in tables {
        if table.area.sheet() != table.sheet
            || table.id == 0
            || !ids.insert(table.id)
            || !names.insert(fold_workbook_name(&table.display_name))
            || !parts.insert(table.part.clone())
            || table.columns.len() != table.area.column_count() as usize
            || table.column_formulas.len() != table.columns.len()
            || table.totals_functions.len() != table.columns.len()
            || table.totals_labels.len() != table.columns.len()
            || table.header_rows > 1
            || table.totals_rows > 1
            || table.area.row_count() <= table.header_rows + table.totals_rows
        {
            return Err(refusal("The table definitions are inconsistent"));
        }
        if table
            .totals_labels
            .iter()
            .flatten()
            .any(|s| s.encode_utf16().count() > 32767)
        {
            return Err(refusal("Table total labels exceed the cell text limit"));
        }
        let mut columns = BTreeSet::new();
        if table.columns.iter().any(|c| {
            c.is_empty() || c.encode_utf16().count() > 255 || !columns.insert(fold_workbook_name(c))
        }) {
            return Err(refusal("Table column names must be nonempty and unique"));
        }
        if let Some(old) = host
            .model()
            .definitions()
            .tables()
            .iter()
            .chain(host.loaded().model().definitions().tables().iter())
            .find(|t| t.part == table.part)
        {
            if old != table {
                bounds(host, table.area, Some(&old.part))?;
            }
            if old.filter.as_ref().is_some_and(|f| f.unsupported) && old.filter != table.filter {
                return Err(refusal(
                    "This table has filter metadata whose changes are unavailable",
                ));
            }
            if old.totals_functions != table.totals_functions
                && table.totals_functions.iter().flatten().any(|f| {
                    !matches!(
                        f.as_str(),
                        "average"
                            | "countNums"
                            | "count"
                            | "max"
                            | "min"
                            | "stdDev"
                            | "sum"
                            | "var"
                    )
                })
            {
                return Err(refusal(
                    "Unsupported total functions retain their original metadata",
                ));
            }
            if old.id != table.id
                || old.name != table.name
                || old.display_name != table.display_name
                || old.sheet != table.sheet
                || old.axis_opaque != table.axis_opaque
                || ((old.column_formulas.iter().any(Option::is_some)
                    || table.column_formulas.iter().any(Option::is_some))
                    && old.column_formulas != table.column_formulas)
                || old.opaque != table.opaque
                || (old.opaque
                    && (old.area != table.area
                        || old.columns != table.columns
                        || old.header_rows != table.header_rows
                        || old.totals_rows != table.totals_rows
                        || old.totals_functions != table.totals_functions
                        || old.totals_labels != table.totals_labels))
            {
                return Err(refusal(
                    "Retain imported table identities and unsupported metadata",
                ));
            }
        } else {
            bounds(host, table.area, None)?;
            if table.totals_functions.iter().flatten().any(|f| {
                !matches!(
                    f.as_str(),
                    "average" | "countNums" | "count" | "max" | "min" | "stdDev" | "sum" | "var"
                )
            }) {
                return Err(refusal("New tables need supported total functions"));
            }
            if table.name != table.display_name
                || table.opaque
                || table.axis_opaque
                || table.column_formulas.iter().any(Option::is_some)
                || table.id == 0
                || host
                    .model()
                    .definitions()
                    .names()
                    .iter()
                    .any(|n| fold_workbook_name(&n.name) == fold_workbook_name(&table.name))
            {
                return Err(refusal("New table names must be unique"));
            }
            let parent = host
                .loaded()
                .workbook_part()
                .rsplit_once('/')
                .map_or("", |(p, _)| p);
            let prefix = format!("{parent}/tables/dnaTable")
                .trim_start_matches('/')
                .to_owned();
            if host.loaded().has_part(&table.part)
                || !table
                    .part
                    .strip_prefix(&prefix)
                    .and_then(|s| s.strip_suffix(".xml"))
                    .is_some_and(|s| s.bytes().all(|b| b.is_ascii_digit()))
                || !calc_engine::valid_defined_name(&table.display_name)
            {
                return Err(refusal(
                    "New tables need a unique authored package identity",
                ));
            }
        }
        if table.filter.as_ref().is_some_and(|f| {
            !f.valid()
                || f.area.sheet() != table.sheet
                || f.area.first() != table.area.first()
                || f.area.last().row() != table.area.last().row() - table.totals_rows
                || f.area.last().column() != table.area.last().column()
                || table.header_rows == 0
        }) {
            return Err(refusal("The table's filter must cover its header and data"));
        }
        if tables
            .iter()
            .any(|other| other.part != table.part && other.area.intersection(table.area).is_some())
        {
            return Err(refusal("Tables cannot overlap"));
        }
    }
    Ok(())
}

/// Header and total cells and their metadata change in one authored transaction.
pub(crate) fn validate_cells(
    host: &HostState,
    batch: &calc_doc_model::OpBatch,
) -> Result<(), String> {
    use calc_doc_model::WorkbookOp as O;
    let authored = batch.operations.iter().rev().find_map(|op| match op {
        O::SetTables(t) => Some(t.as_slice()),
        _ => None,
    });
    if let Some(tables) = authored {
        validate(host, tables)?;
    }
    let tables = authored.unwrap_or(host.model().definitions().tables());
    for table in tables {
        if batch.operations.iter().any(|op|matches!(op,O::SetArray{array,..}if array.area().intersection(table.area).is_some())){return Err(refusal("Table data cannot contain array formulas"));}
        let old = host
            .model()
            .definitions()
            .tables()
            .iter()
            .find(|t| t.part == table.part);
        let headers_changed = old.is_none_or(|t| {
            t.columns != table.columns
                || t.header_rows != table.header_rows
                || t.area.first() != table.area.first()
        });
        let totals_changed = old.is_none_or(|t| {
            t.totals_functions != table.totals_functions
                || t.totals_labels != table.totals_labels
                || t.totals_rows != table.totals_rows
                || t.area.last() != table.area.last()
        });
        for index in 0..table.columns.len().min(table.area.column_count() as usize) {
            for header in [true, false] {
                if (header && table.header_rows == 0) || (!header && table.totals_rows == 0) {
                    continue;
                }
                let row = if header {
                    table.area.first().row()
                } else {
                    table.area.last().row()
                };
                let at = CellKey::new(table.sheet, row, table.area.first().column() + index as u32)
                    .unwrap();
                if batch
                    .operations
                    .iter()
                    .any(|op| matches!(op,O::SetArray{array,..}if array.area().contains(at)))
                {
                    return Err(refusal(
                        "Table header and total cells cannot hold array formulas",
                    ));
                }
                let touches = batch.operations.iter().any(|op| match op {
                    O::SetCell { at: key, .. } => *key == at,
                    O::Clear { area } => area.contains(at),
                    _ => false,
                });
                if !touches && authored.is_none() {
                    continue;
                }
                if header && table.columns[index].encode_utf16().count() > 32767 {
                    return Err(refusal(
                        "An imported table header exceeds the cell text limit",
                    ));
                }
                let expected = if header {
                    Some(text(&table.columns[index]))
                } else {
                    total_input(table, index)
                };
                let proposed = batch.operations.iter().rev().find_map(|op| match op {
                    O::SetCell { at: key, input } if *key == at => Some(Some(input)),
                    O::Clear { area } if area.contains(at) => Some(None),
                    _ => None,
                });
                let metadata_changed = if header {
                    headers_changed
                        && old.is_none_or(|t| {
                            t.header_rows != table.header_rows
                                || t.area.first() != table.area.first()
                                || t.columns.get(index) != table.columns.get(index)
                        })
                } else {
                    totals_changed
                        && old.is_none_or(|t| {
                            t.totals_rows != table.totals_rows
                                || t.area.last() != table.area.last()
                                || t.totals_functions.get(index)
                                    != table.totals_functions.get(index)
                                || t.totals_labels.get(index) != table.totals_labels.get(index)
                        })
                };
                if metadata_changed {
                    if proposed.unwrap_or(host.model().cell(at)) != expected.as_ref() {
                        return Err(refusal(
                            "Table definitions and header or total cells must change together",
                        ));
                    }
                } else if let Some(proposed) = proposed {
                    if proposed != host.model().cell(at) && proposed != expected.as_ref() {
                        return Err(refusal(
                            "Use table controls to change header and total cells",
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

/// Preserve imported calculated-column text only when the ordinary reference rewriter proves
/// this axis edit leaves it unchanged; edits requiring metadata formula authoring still refuse.
pub(crate) fn axis_compatible(
    host: &HostState,
    table: &Table,
    edit: calc_doc_model::AxisEdit,
) -> bool {
    use calc_doc_model::{FormulaRewriter, StructuralChange};
    if table.axis_opaque || table.filter.as_ref().is_some_and(|f| f.unsupported) {
        return false;
    }
    table
        .column_formulas
        .iter()
        .enumerate()
        .all(|(index, formula)| {
            let Some(formula) = formula else {
                return true;
            };
            if !calc_fml::parse_formula(formula).diagnostics().is_empty() {
                return false;
            }
            let Some(row) = table.area.first().row().checked_add(table.header_rows) else {
                return false;
            };
            let Ok(at) = CellKey::new(
                table.sheet,
                row,
                table.area.first().column().saturating_add(index as u32),
            ) else {
                return false;
            };
            let Ok(text) = FormulaText::new(ExcelText::from_utf16_code_units(
                formula.encode_utf16().collect(),
            )) else {
                return false;
            };
            calc_engine::ReferenceRewriter
                .formula(host.model(), &text, at, &StructuralChange::Axis(edit))
                .is_none_or(|new| new.text() == text.text())
        })
}
