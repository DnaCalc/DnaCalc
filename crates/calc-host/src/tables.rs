//! Table commands author cells and table definitions in the same workbook transaction.
use crate::{CellEdit, CellInput, CellText, FormulaText, HostState};
use calc_doc_model::{AutoFilter, Table, TableStyle};
use calc_value::{CellKey, ExcelText, GridArea, fold_workbook_name};
use std::collections::{BTreeMap, BTreeSet};
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
    RenameColumn { part: String, column: u32, name: String },
    CalculatedColumn { part: String, column: u32, formula: Option<String> },
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
    if matches!(value.core_ref(), CoreRef::Number(_) | CoreRef::Text(_)) {
        let code = host.number_format(at).ok_or_else(|| refusal("The header's number format is unavailable"))?;
        if let Some(profile) = host.options().locale.as_ref() {
            let system = if host.declared_date1904() { calc_func::locale::DateSystem::System1904 } else { calc_func::locale::DateSystem::System1900 };
            return calc_func::cell_text(profile, system, value, code).map_err(|_| refusal("The header's number format cannot be rendered"));
        } else if !code.eq_ignore_ascii_case("General") {
            return Err(refusal("Formatted headers need the workbook's locale"));
        }
    }
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
    // Names are made in two passes, as Excel makes them (m7-names): headers with text first, a repeated
    // name taking the smallest number from 2 that is free; then each blank header, which is `Column` and
    // the smallest number from 1 that no other header has, in column order.
    let mut used = BTreeSet::new();
    let mut names: Vec<Option<String>> = Vec::new();
    for i in 0..area.column_count() {
        let name = if let Some(name) = existing.get(i as usize) {
            if !used.insert(fold_workbook_name(name)) {
                return Err(refusal("Existing table column names must be unique"));
            }
            names.push(Some(name.clone()));
            continue;
        } else {
            cell_text(
                host,
                CellKey::new(area.sheet(), area.first().row(), area.first().column() + i).unwrap(),
            )?
        };
        let name = name
            .chars()
            .scan(0, |n, c| {
                *n += c.len_utf16();
                (*n <= 255).then_some(c)
            })
            .collect::<String>();
        if name.trim().is_empty() {
            names.push(None);
            continue;
        }
        let original = name.clone();
        let mut name = name;
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
        names.push(Some(name));
    }
    let mut taken: Vec<String> = names.iter().flatten().cloned().collect();
    let mut out = Vec::with_capacity(names.len());
    for name in names {
        out.push(match name {
            Some(name) => name,
            None => {
                let blank = calc_doc_model::free_column_name(&taken);
                taken.push(blank.clone());
                blank
            }
        });
    }
    Ok(out)
}
fn quoted_column(name: &str) -> String {
    calc_fml::reference::quote_table_column(name)
}

fn formula_text(source: &str) -> Result<FormulaText, String> {
    FormulaText::with_form(ExcelText::from_utf16_code_units(source.encode_utf16().collect()),calc_doc_model::FormulaForm::Legacy).map_err(|_| refusal("Enter a column formula"))
}
fn column_index(table: &Table, column: u32) -> Result<usize, String> {
    column.checked_sub(table.area.first().column()).filter(|i| *i < table.area.column_count()).map(|i| i as usize).ok_or_else(|| refusal("Choose a table column"))
}
fn calculated_input(host: &HostState, table: &Table, index: usize, row: u32) -> Result<CellInput, String> {
    let source = table.column_formulas[index].as_deref().ok_or_else(|| refusal("This column has no formula rule"))?;
    let first = CellKey::new(table.sheet, table.area.first().row() + table.header_rows, table.area.first().column() + index as u32).unwrap();
    let target = CellKey::new(table.sheet, row, first.column()).unwrap();
    let formula = formula_text(source)?;
    let formula = if first == target { formula } else {
        calc_engine::relocate_formula(host.model(), &formula, first, target).ok_or_else(|| refusal("This calculated-column formula cannot be copied"))?
    };
    Ok(CellInput::Formula(formula))
}
fn column_writes(host: &HostState, table: &Table, index: usize, first: u32, edits: &mut Vec<CellEdit>) -> Result<(), String> {
    for row in first..=table.area.last().row() - table.totals_rows {
        let at = CellKey::new(table.sheet, row, table.area.first().column() + index as u32).unwrap();
        edits.push(CellEdit::Set { at, input: calculated_input(host, table, index, row)? });
    }
    Ok(())
}
fn renamed(host: &HostState, at: Option<CellKey>, source: &str, table: &str, old: &str, new: &str) -> Result<String, String> {
    use calc_fml::reference::TableColumnContext as C;
    let context = at.map_or(C::Name, |at| C::Cell(host.model().definitions().tables().iter().find(|t| t.area.contains(at)).map(|t| t.display_name.as_str())));
    let columns = &host.model().definitions().tables().iter().find(|t| calc_value::workbook_names_equal(&t.display_name,table)).ok_or_else(|| refusal("The table is unavailable"))?.columns;
    calc_fml::reference::rename_table_column(source, table, columns, old, new, context, &|name| host.sheets().iter().any(|s| calc_value::workbook_names_equal(&s.name().text().to_string_lossy(), name))).map_err(refusal)
}
fn rename_references(host: &HostState, tables: &mut [Table], table: &str, old: &str, new: &str, edits: &mut Vec<CellEdit>) -> Result<(), String> {
    for sheet in host.sheets() {
        if host.loaded().sheet(sheet.id()).is_some_and(|s| s.conditional_formats().iter().any(|b| b.unread_container)) {
            return Err(refusal("A conditional-format extension cannot be checked for this column rename"));
        }
        let charts = host.loaded().sheet(sheet.id()).map(|s| s.charts());
        if let Some(charts) = charts {
            if charts.unread > 0 { return Err(refusal("An unread chart cannot be checked for this column rename")); }
            for chart in &charts.charts {
                for source in chart.title_formula.iter().map(String::as_str).chain(chart.plots.iter().flat_map(|p| &p.series).flat_map(|s| [s.name.as_ref(), s.categories.as_ref(), s.values.as_ref()]).flatten().filter_map(|d| d.formula.as_deref())) {
                    if renamed(host, None, source, table, old, new)? != source { return Err(refusal("A chart source uses this column; chart formula authoring is unavailable")); }
                }
            }
        }
        for (at, input) in host.model().sheet_cells(sheet.id()).into_iter().flatten() {
            if let CellInput::Formula(formula) = input {
                let source = String::from_utf16(formula.text().utf16_code_units()).map_err(|_| refusal("A formula's Unicode text cannot be checked for this rename"))?;
                let next = renamed(host, Some(at), &source, table, old, new)?;
                if next != source {
                    let next = ExcelText::from_utf16_code_units(next.encode_utf16().collect());
                    let formula = if formula.is_form_explicit() { FormulaText::with_form(next, formula.form()) } else { FormulaText::new(next) }.map_err(|_| refusal("The renamed formula is empty"))?;
                    if let Some(array) = host.model().definitions().arrays().find(|a| a.anchor() == at) {
                        edits.push(CellEdit::SetArray { array: *array, formula });
                    } else { edits.push(CellEdit::Set { at, input: CellInput::Formula(formula) }); }
                }
            }
        }
        let mut rules = host.model().definitions().conditional_rules(sheet.id()).to_vec();
        let mut changed = false;
        for rule in &mut rules {
            let at = rule.origin();
            for source in rule.formulas.iter_mut().chain(&mut rule.threshold_formulas) {
                let next = renamed(host, at, source, table, old, new)?;
                if next != *source { *source = next; changed = true; }
            }
        }
        if changed { edits.push(CellEdit::ConditionalRules { sheet: sheet.id(), rules }); }
        let mut rules = host.model().definitions().validations(sheet.id()).to_vec();
        let mut changed = false;
        for rule in &mut rules {
            let at = rule.areas.first().and_then(|&(r,c,_,_)| CellKey::new(sheet.id(),r,c).ok());
            for source in [&mut rule.formula1, &mut rule.formula2].into_iter().flatten() {
                let next = renamed(host, at, source, table, old, new)?;
                if next != *source { *source = next; changed = true; }
            }
        }
        if changed { edits.push(CellEdit::Validations { sheet: sheet.id(), rules }); }
    }
    for name in host.model().definitions().names() {
        let next = renamed(host, None, &name.formula, table, old, new)?;
        if next != name.formula { edits.push(CellEdit::SetName { name: name.name.clone(), scope: name.scope, formula: Some(next) }); }
    }
    for t in tables {
        for index in 0..t.columns.len() {
            if let Some(source) = &t.column_formulas[index] {
                let at = CellKey::new(t.sheet, t.area.first().row()+t.header_rows, t.area.first().column()+index as u32).unwrap();
                let next = renamed(host, Some(at), source, table, old, new)?;
                if next != *source {
                    if t.axis_opaque { return Err(refusal("An imported calculated column has unsupported metadata")); }
                    t.column_formulas[index] = Some(next);
                }
            }
        }
    }
    Ok(())
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
                | TableChange::Total { part, .. }
                | TableChange::RenameColumn { part, .. }
                | TableChange::CalculatedColumn { part, .. } => part,
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
                    if area != old.area && old.column_formulas.iter().any(Option::is_some)
                        && old.filter.as_ref().is_some_and(|f| !f.columns.is_empty()) {
                        return Err(refusal("Clear the table's filters before resizing calculated columns"));
                    }
                    if old.axis_opaque {
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
                    table.opaque = table.column_formulas.iter().any(Option::is_some);
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
                    let first_new_row = old.area.last().row() - old.totals_rows + 1;
                    for i in 0..table.columns.len() {
                        if table.column_formulas[i].is_some() { column_writes(host, &table, i, first_new_row, &mut edits)?; }
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
                    if old.axis_opaque
                        && (headers != (old.header_rows > 0) || totals != (old.totals_rows > 0))
                    {
                        return Err(refusal(
                            "This imported table's header and total metadata is preserved",
                        ));
                    }
                    if !style.is_empty() && (TableStyle { name: Some(style.clone()), ..Default::default() }).medium_accent_theme().is_none()
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
                    if old.axis_opaque || old.totals_rows == 0 {
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
                TableChange::RenameColumn { column, name, .. } => {
                    if old.axis_opaque { return Err(refusal("This imported table's column metadata is preserved")); }
                    let i = column_index(&old, column)?;
                    if name.trim().is_empty() || name.encode_utf16().count() > 255 || name.chars().any(char::is_control)
                        || old.columns.iter().enumerate().any(|(j,c)| j != i && calc_value::workbook_names_equal(c, &name)) {
                        return Err(refusal("Use a nonempty, unique column name of at most 255 UTF-16 units"));
                    }
                    if name == old.columns[i] { return Ok(Vec::new()); }
                    rename_references(host, &mut tables, &old.display_name, &old.columns[i], &name, &mut edits)?;
                    table = tables[index].clone();
                    table.columns[i] = name.clone();
                    if table.header_rows > 0 {
                        edits.push(CellEdit::Set { at: CellKey::new(table.sheet,table.area.first().row(),column).unwrap(), input: text(&name) });
                    }
                }
                TableChange::CalculatedColumn { column, formula, .. } => {
                    if old.axis_opaque { return Err(refusal("This imported table's calculated metadata is preserved")); }
                    if formula.is_some() && old.filter.as_ref().is_some_and(|f| !f.columns.is_empty()) {
                        return Err(refusal("Clear the table's filters before filling a calculated column"));
                    }
                    let i = column_index(&old, column)?;
                    let formula = formula.map(|s| s.strip_prefix('=').unwrap_or(&s).to_owned());
                    if let Some(source) = &formula {
                        let parsed = calc_fml::parse_formula(source);
                        if source.encode_utf16().count() > 8192 || parsed.expression().is_none() || !parsed.diagnostics().is_empty() {
                            return Err(refusal("Enter a complete supported column formula of at most 8192 UTF-16 units"));
                        }
                    }
                    table.column_formulas[i] = formula;
                    table.opaque = table.column_formulas.iter().any(Option::is_some);
                    if table.column_formulas[i].is_some() {
                        column_writes(host, &table, i, table.area.first().row()+table.header_rows, &mut edits)?;
                    }
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
                || (old.axis_opaque && old.column_formulas != table.column_formulas)
                || (old.opaque != table.opaque && (old.axis_opaque || table.opaque != table.column_formulas.iter().any(Option::is_some)))
                || (old.axis_opaque
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
    let mut writes = BTreeMap::new();
    let mut clears = Vec::new();
    for (index, op) in batch.operations.iter().enumerate() {
        match op {
            O::SetCell { at, input } => { writes.insert(*at,(index,input)); }
            O::Clear { area } => clears.push((index,*area)), _ => {}
        }
    }
    let proposed_at = |at: CellKey| -> Option<Option<&CellInput>> {
        let write = writes.get(&at);
        let clear = clears.iter().rev().find(|(_,area)|area.contains(at));
        if let Some((index,_)) = clear {
            if write.is_none_or(|(written,_)|index>written) { return Some(None); }
        }
        write.map(|(_,input)|Some(*input))
    };
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
            if table.column_formulas[index].is_some() {
                let changed = old.is_none_or(|t| t.column_formulas.get(index) != table.column_formulas.get(index));
                let first = table.area.first().row() + table.header_rows;
                let last = table.area.last().row() - table.totals_rows;
                let bounds_changed = old.is_none_or(|t| first != t.area.first().row()+t.header_rows || last != t.area.last().row()-t.totals_rows);
                if changed || bounds_changed { for row in first..=last {
                    let added = old.is_none_or(|t| row < t.area.first().row()+t.header_rows || row > t.area.last().row()-t.totals_rows);
                    if !changed && !added { continue; }
                    let at = CellKey::new(table.sheet,row,table.area.first().column()+index as u32).unwrap();
                    let expected = calculated_input(host,table,index,row)?;
                    let proposed = proposed_at(at).unwrap_or(host.model().cell(at));
                    if proposed != Some(&expected) { return Err(refusal("Calculated-column metadata and its data formulas must change together")); }
                } }
            }
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
                let proposed = proposed_at(at);
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
    // Ordinary worksheet insertion does not author calculated cells for the new rows.
    // Table Resize does, so keep insertion within a calculated table atomic until the
    // worksheet command can provide that same fill transaction.
    if edit.sheet() == table.sheet
        && edit.axis() == calc_doc_model::Axis::Rows
        && edit.change() == calc_doc_model::AxisChange::Insert
        && edit.first() > table.area.first().row()
        && edit.first() <= table.area.last().row()
        && table.column_formulas.iter().any(Option::is_some)
    {
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
