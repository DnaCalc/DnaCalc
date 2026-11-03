//! Workbook commands shared by every host client. Excel rules are prepared here before
//! the session commits one transaction, records history and recalculates.
use crate::value::{CellKey, GridArea, SheetId};
use crate::{Axis, CellEdit, HostState, SheetEdit, SheetKey, SheetName, SheetVisibility};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SheetChange {
    Add { name: String },
    Rename { name: String },
    Move { position: u32 },
    Remove,
    Hide,
    Show,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConditionalChange {
    Add {
        kind: String,
        applies: String,
        color: String,
    },
    Update {
        applies: String,
        formulas: Vec<String>,
        stop: bool,
    },
    Move {
        up: bool,
    },
    Remove,
}
/// Authored workbook commands. The session is the sole dispatch route for all clients.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkbookCommand {
    Name(crate::NameEdit),
    AutoFilter { sheet:SheetId, revision:u64, change:crate::FilterChange },
    FormatCells {
        areas: Vec<GridArea>,
        patch: crate::FormatPatch,
    },
    Sheet {
        sheet: Option<SheetId>,
        change: SheetChange,
    },
    AxisFormat {
        sheet: SheetId,
        rows: bool,
        first: u32,
        count: u32,
        size_bits: Option<u64>,
        hidden: Option<bool>,
    },
    FitContents {
        sheet: SheetId,
        rows: bool,
        revision: u64,
        sizes: Vec<(u32, u64)>,
    },
    FreezePanes {
        sheet: SheetId,
        rows: u32,
        columns: u32,
    },
    Conditional {
        sheet: SheetId,
        revision: u64,
        index: Option<u32>,
        change: ConditionalChange,
    },
    Replace {
        options: crate::search::SearchOptions,
        replacement: String,
        revision: u64,
        only: Option<CellKey>,
    },
    SortRows {
        area: GridArea,
        column: u32,
        header: bool,
        descending: bool,
        revision: u64,
    },
    Paste(crate::paste::PastePlan),
    ShiftCells {
        area: GridArea,
        down: bool,
        insert: bool,
    },
}

pub(crate) fn prepare(
    host: &HostState,
    keys: &mut BTreeSet<SheetKey>,
    command: WorkbookCommand,
) -> Result<Vec<CellEdit>, String> {
    match command {
        WorkbookCommand::AutoFilter { sheet,revision,change }=>crate::filter::prepare(host,sheet,revision,change),
        WorkbookCommand::SortRows {
            area,
            column,
            header,
            descending,
            revision,
        } => sort_rows(host, area, column, header, descending, revision),
        WorkbookCommand::Name(edit) => crate::names::prepare(host, edit),
        WorkbookCommand::FormatCells { areas, patch } => {
            crate::format::prepare(host, &areas, &patch)
        }
        WorkbookCommand::Sheet { sheet, change } => sheet_edits(host, keys, sheet, change),
        WorkbookCommand::AxisFormat {
            sheet,
            rows,
            first,
            count,
            size_bits,
            hidden,
        } => axis_format_edits(host, sheet, rows, first, count, size_bits, hidden),
        WorkbookCommand::FitContents {
            sheet,
            rows,
            revision,
            sizes,
        } => {
            if revision != host.revision().get() {
                return Err("The workbook changed while fitting; try again".into());
            }
            if sizes.len() > 16384 {
                return Err("Fit at most 16384 rows or columns at once".into());
            }
            let axis = if rows { Axis::Rows } else { Axis::Columns };
            let mut edits = Vec::new();
            let mut seen = BTreeSet::new();
            for (index, size) in sizes {
                if !seen.insert(index) {
                    return Err("A fitted band was repeated".into());
                }
                let hidden = host
                    .model()
                    .definitions()
                    .axis_layouts()
                    .get(sheet, axis, index)
                    .hidden();
                edits.extend(axis_format_edits(
                    host,
                    sheet,
                    rows,
                    index,
                    1,
                    Some(size),
                    Some(hidden),
                )?);
            }
            Ok(edits)
        }
        WorkbookCommand::FreezePanes {
            sheet,
            rows,
            columns,
        } => {
            if host.model().sheet_by_id(sheet).is_none() {
                return Err("The sheet is unavailable".into());
            }
            let panes = crate::FrozenPanes::new(rows, columns);
            if panes.is_none() && (rows != 0 || columns != 0) {
                return Err("The frozen boundary is outside the worksheet".into());
            }
            Ok(if panes == host.model().definitions().frozen_panes(sheet) {
                Vec::new()
            } else {
                vec![CellEdit::FrozenPanes { sheet, panes }]
            })
        }
        WorkbookCommand::Conditional {
            sheet,
            revision,
            index,
            change,
        } => conditional_edits(host, sheet, revision, index, change),
        WorkbookCommand::Replace {
            options,
            replacement,
            revision,
            only,
        } => {
            if revision != host.revision().get() {
                return Err("The workbook changed; run Find again before replacing".into());
            }
            let edits = crate::search::replace_edits(host, &options, &replacement, only)?;
            if edits.is_empty() {
                return Err("No entries changed".into());
            }
            Ok(edits)
        }
        WorkbookCommand::Paste(plan) => plan.into_edits(host),
        WorkbookCommand::ShiftCells { area, down, insert } => {
            crate::shift_cell_edits(host, area, down, insert)
        }
    }
}

fn sheet_edits(
    host: &HostState,
    sheet_keys: &mut BTreeSet<SheetKey>,
    sheet: Option<SheetId>,
    change: SheetChange,
) -> Result<Vec<CellEdit>, String> {
    use SheetChange as C;
    let model = host.model();
    let id = sheet;
    let require = || {
        id.filter(|id| model.sheet_by_id(*id).is_some())
            .ok_or_else(|| "The worksheet is unavailable".to_owned())
    };
    let name = |text: String| -> Result<SheetName, String> {
        if text.eq_ignore_ascii_case("History") {
            return Err("History is reserved by Excel; choose another worksheet name".to_owned());
        }
        SheetName::new(crate::value::ExcelText::from_utf16_code_units(
            text.encode_utf16().collect(),
        ))
        .map_err(|e| e.to_string())
    };
    let edit = match change {
        C::Add { name: text } => {
            let key = (1u32..)
                .map(SheetKey::new)
                .find(|key| {
                    !sheet_keys.contains(key) && model.sheets().iter().all(|s| s.key() != *key)
                })
                .ok_or("No sheet keys remain")?;
            sheet_keys.insert(key);
            SheetEdit::Insert {
                key,
                name: name(text)?,
                position: std::num::NonZeroU32::new(model.sheets().len() as u32 + 1).unwrap(),
            }
        }
        C::Rename { name: text } => SheetEdit::Rename {
            sheet: require()?,
            name: name(text)?,
        },
        C::Move { position } => {
            if position == 0 || position as usize > model.sheets().len() {
                return Err("Choose a worksheet position in the workbook".to_owned());
            }
            SheetEdit::Move {
                sheet: require()?,
                position: std::num::NonZeroU32::new(position).unwrap(),
            }
        }
        C::Remove => SheetEdit::Remove { sheet: require()? },
        C::Hide | C::Show => {
            let sheet = require()?;
            if model.sheet_by_id(sheet).unwrap().visibility() == SheetVisibility::VeryHidden {
                return Err("This sheet is VeryHidden; unhiding it is unavailable here".to_owned());
            }
            SheetEdit::SetVisibility {
                sheet,
                visibility: if change == C::Hide {
                    SheetVisibility::Hidden
                } else {
                    SheetVisibility::Visible
                },
            }
        }
    };
    Ok(vec![CellEdit::Sheet(edit)])
}

pub use calc_value::stored_axis_size;

pub(crate) fn axis_format_edits(
    host: &HostState,
    sheet: SheetId,
    rows: bool,
    first: u32,
    count: u32,
    size_bits: Option<u64>,
    hidden: Option<bool>,
) -> Result<Vec<CellEdit>, String> {
    let loaded = host
        .loaded()
        .sheet(sheet)
        .ok_or("The sheet is unavailable")?;
    if loaded.protected() {
        return Err("Dimensions on a protected sheet are not available here".to_owned());
    }
    let axis = if rows {
        crate::Axis::Rows
    } else {
        crate::Axis::Columns
    };
    let last = first
        .checked_add(count)
        .and_then(|n| n.checked_sub(1))
        .filter(|n| first >= 1 && count > 0 && *n <= axis.size())
        .ok_or("The dimensions are outside the worksheet")?;
    if count > 16_384 {
        return Err("Change at most 16384 rows or columns at once".to_owned());
    }
    if rows && (hidden.is_some() || size_bits.is_some()) && host.model().definitions().auto_filter(sheet).is_some_and(|f|f.unsupported && first<=f.area.last().row() && last>f.area.first().row()) {
        return Err("Row visibility for imported filter metadata is not modelled (dc-971p.37); nothing changed".into());
    }
    if rows && host.model().definitions().hidden_rows(sheet,first,last).any(|(row,why)|
        why==crate::RowHidden::Filtered && (!host.model().definitions().auto_filter(sheet).is_some_and(|f|f.filtered_rows.contains(&row)) || host.model().definitions().tables().iter().any(|t|t.sheet==sheet && row>t.area.first().row() && row<=t.area.last().row()))) {
        return Err("Rows hidden by an imported table filter cannot be resized or shown here (dc-971p.37)".into());
    }
    let size = size_bits
        .map(|b| stored_axis_size(rows, f64::from_bits(b)))
        .transpose()
        .map_err(str::to_owned)?;
    let mut edits = Vec::new();
    for index in first..=last {
        let old = host
            .model()
            .definitions()
            .axis_layouts()
            .get(sheet, axis, index);
        let mut new = old;
        if let Some(size) = size {
            new = match size {
                None => old.with_hidden(true),
                Some(n) => crate::AxisFormat::new(Some(n), false, !rows).expect("a checked size"),
            };
        }
        if let Some(hidden) = hidden {
            new = if !hidden && new.size() == Some(0.0) {
                crate::AxisFormat::default()
            } else {
                new.with_hidden(hidden)
            };
        }
        if old != new {
            edits.push(CellEdit::AxisFormat(
                crate::AxisFormatEdit::new(sheet, axis, index, new).expect("checked band indices"),
            ));
        }
    }
    Ok(edits)
}

pub(crate) fn conditional_edits(
    host: &HostState,
    sheet: crate::value::SheetId,
    revision: u64,
    index: Option<u32>,
    change: ConditionalChange,
) -> Result<Vec<CellEdit>, String> {
    if revision != host.revision().get() {
        return Err("Rules changed; reopen the current rule and try again".into());
    }
    if host.loaded().sheet(sheet).is_some_and(|s| s.protected()) {
        return Err("Worksheet protection prevents conditional rule edits".into());
    }
    if host
        .loaded()
        .sheet(sheet)
        .is_some_and(|s| s.conditional_formats().iter().any(|b| b.unread_container))
    {
        return Err("This worksheet has additional conditional-rule metadata; editing its rules is unavailable and the file is preserved (dc-971p.8)".into());
    }
    let mut rules = host.model().definitions().conditional_rules(sheet).to_vec();
    let ranges = |text: &str| -> Result<Vec<GridArea>, String> {
        let at = CellKey::new(sheet, 1, 1).unwrap();
        let mut areas = Vec::new();
        for token in text.split_whitespace() {
            match host.resolve_reference_text(at, token) {
                crate::ReferenceTarget::Area(a) if a.sheet() == sheet => areas.push(a),
                _ => {
                    return Err("Applies to needs space-separated ranges on this worksheet".into());
                }
            }
        }
        if areas.is_empty() || areas.len() > 128 {
            return Err("Choose between 1 and 128 ranges".into());
        }
        Ok(areas)
    };
    if let ConditionalChange::Add {
        kind,
        applies,
        color,
    } = change
    {
        let mut source =
            crate::new_conditional_rule(ranges(&applies)?, &kind, color.trim_start_matches('#'))?;
        source.priority = rules
            .iter()
            .map(|r| r.priority)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        rules.push(source);
    } else {
        let i = index
            .map(|i| i as usize)
            .filter(|i| *i < rules.len())
            .ok_or("Select a rule first")?;
        let parsed = crate::conditional_rule(&rules[i])?;
        match change {
            ConditionalChange::Update {
                applies,
                formulas,
                stop,
            } => {
                let areas = ranges(&applies)?;
                if parsed.extension_id.is_some() {
                    return Err("Editing extension-backed rules is unavailable; the imported rule is preserved (dc-971p.8)".into());
                }
                let mut formulas = formulas;
                if formulas != rules[i].formulas {
                    if !matches!(parsed.kind.as_str(), "cellIs" | "expression")
                        || formulas.len() != rules[i].formulas.len()
                    {
                        return Err("This rule condition cannot be edited yet (dc-971p.8)".into());
                    }
                    for formula in &mut formulas {
                        let text = crate::FormulaText::new(
                            crate::value::ExcelText::from_utf16_code_units(
                                formula
                                    .trim()
                                    .trim_start_matches('=')
                                    .encode_utf16()
                                    .collect(),
                            ),
                        )
                        .map_err(|e| e.to_string())?;
                        let form = crate::file_form_with_links(
                            &text,
                            host.model().definitions().names(),
                            &host.model().definitions().external_file_names(),
                        )
                        .map_err(|e| format!("Conditional formula is unavailable: {e:?}"))?;
                        *formula = String::from_utf16_lossy(&form.text)
                            .trim_start_matches('=')
                            .to_owned();
                    }
                }
                rules[i].areas = areas;
                rules[i].formulas = formulas;
                rules[i].stop =
                    stop && !matches!(parsed.kind.as_str(), "colorScale" | "dataBar" | "iconSet");
                rules[i].authored = true;
            }
            ConditionalChange::Remove => {
                if parsed.extension_id.is_some() {
                    return Err("Removing extension-backed rules is unavailable; the imported rule is preserved (dc-971p.8)".into());
                }
                rules.remove(i);
            }
            ConditionalChange::Move { up } => {
                let mut order = (0..rules.len()).collect::<Vec<_>>();
                order.sort_by_key(|&n| rules[n].priority);
                let old = order.iter().position(|&n| n == i).unwrap();
                let destination = if up {
                    old.saturating_sub(1)
                } else {
                    (old + 1).min(order.len() - 1)
                };
                order.swap(old, destination);
                for (rank, index) in order.into_iter().enumerate() {
                    rules[index].priority = rank as i64 + 1;
                    rules[index].authored = true;
                }
            }
            _ => unreachable!(),
        }
    }
    let mut order = (0..rules.len()).collect::<Vec<_>>();
    order.sort_by_key(|&n| rules[n].priority);
    for (rank, index) in order.into_iter().enumerate() {
        rules[index].priority = rank as i64 + 1;
    }
    Ok(vec![CellEdit::ConditionalRules { sheet, rules }])
}

/// A bounded row permutation. Ordering comes from the existing evaluator; edits use the same
/// immutable copy snapshots and one transaction as Paste. Formula relocation and document
/// metadata movement need separate Excel measurements before this command admits them.
fn sort_rows(
    host: &HostState,
    area: GridArea,
    column: u32,
    header: bool,
    descending: bool,
    revision: u64,
) -> Result<Vec<CellEdit>, String> {
    let unavailable = |detail: &str| {
        format!("Sorting {detail} is not modelled (dc-971p.36); nothing was changed")
    };
    if revision != host.revision().get() {
        return Err("The workbook changed before sorting; select the range again".into());
    }
    crate::check_destination(host, area, crate::PasteMode::All).map_err(|e| e.to_string())?;
    if !(area.first().column()..=area.last().column()).contains(&column) {
        return Err("The sort column must lie in the selected range".into());
    }
    let first = area.first().row() + u32::from(header);
    if first > area.last().row() {
        return Err("Select at least one data row below the header".into());
    }
    let data = GridArea::new(
        CellKey::new(area.sheet(), first, area.first().column()).unwrap(),
        area.last(),
    )
    .unwrap();
    if host
        .merged_areas(area.sheet())
        .iter()
        .any(|a| a.intersection(data).is_some())
    {
        return Err(unavailable("merged cells"));
    }
    if host
        .model()
        .definitions()
        .tables()
        .iter()
        .any(|t| t.area.intersection(data).is_some())
    {
        return Err(unavailable("table rows"));
    }
    if host
        .model()
        .definitions()
        .arrays()
        .any(|a| a.area().intersection(data).is_some())
        || host
            .engine
            .spills()
            .any(|a| a.area().intersection(data).is_some())
    {
        return Err(unavailable(
            "array formulas; paste their Values into an ordinary range first",
        ));
    }
    if host.sheet_geometry(area.sheet()).is_some_and(|g| {
        g.rows
            .range(first..=area.last().row())
            .any(|(_, r)| r.hidden)
    }) {
        return Err(unavailable("hidden or filtered rows"));
    }
    if host
        .model()
        .definitions()
        .unloaded_in(data)
        .next()
        .is_some()
    {
        return Err(unavailable("unloaded cell content"));
    }
    if !host.loaded.cell_sort_metadata_supported(area.sheet()) {
        return Err(unavailable("source cell decorations or worksheet objects"));
    }
    let review = host.sheet_review(area.sheet());
    if !review.unsupported.is_empty()
        || review.notes.iter().any(|n| data.contains(n.at))
        || review
            .links
            .iter()
            .any(|l| data.intersection(l.area).is_some())
    {
        return Err(unavailable("cells with notes or hyperlinks"));
    }
    // Rule ranges and formulas are separate authored objects. Do not strand them by moving
    // only cell inputs, and never infer a formula value from a saved cache.
    if !host
        .model()
        .definitions()
        .conditional_rules(area.sheet())
        .is_empty()
    {
        return Err(unavailable("a sheet with conditional formatting"));
    }
    if host.loaded.sheet(area.sheet()).is_some_and(|sheet| {
        sheet.validations().iter().any(|validation| {
            host.current_areas(area.sheet(), &validation.areas)
                .iter()
                .any(|v| data.intersection(*v).is_some())
        })
    }) {
        return Err(unavailable("cells with data validation"));
    }
    for row in first..=area.last().row() {
        for col in area.first().column()..=area.last().column() {
            let at = CellKey::new(area.sheet(), row, col).unwrap();
            if matches!(host.model().cell(at), Some(crate::CellInput::Formula(_))) {
                return Err(unavailable("formula cells; use Paste Special Values first"));
            }
        }
    }
    for row in first..=data.last().row() {
        let key = CellKey::new(area.sheet(), row, column).unwrap();
        if host
            .model()
            .cell(key)
            .is_some_and(|input| !matches!(input, crate::CellInput::Number(_)))
        {
            return Err(unavailable(
                "non-numeric keys; choose a numeric or date column",
            ));
        }
    }
    let count = data.row_count();
    if count < 2 {
        return Ok(Vec::new());
    }
    let label = |row| {
        let mut col = column;
        let mut chars = Vec::new();
        while col > 0 {
            chars.push(char::from(b'A' + ((col - 1) % 26) as u8));
            col = (col - 1) / 26;
        }
        format!("{}{}", chars.into_iter().rev().collect::<String>(), row)
    };
    let key = format!("{}:{}", label(first), label(data.last().row()));
    // Empty keys stay last in both directions. Original row index is an explicit tie key.
    // DetachedFormula reads canonical XLSX function spellings.
    let formula = format!(
        "_xlfn.SORTBY(_xlfn.SEQUENCE({count}),ISBLANK({key}),1,{key},{},_xlfn.SEQUENCE({count}),1)",
        if descending { -1 } else { 1 }
    );
    let value = calc_engine::DetachedFormula::compile_dynamic(host.model(), &formula, data.first())
        .and_then(|f| f.evaluate(&host.engine, host.model(), data.first()))
        .map_err(|e| format!("Sort could not calculate the row order (dc-971p.36): {e}"))?;
    let calc_value::CoreRef::Array(order) = value.core_ref() else {
        return Err(match value.core_ref() {
            calc_value::CoreRef::Error(code) => format!(
                "Sort could not calculate the row order: {} (dc-971p.36)",
                code.excel_spelling()
            ),
            _ => "Sort did not return a row order (dc-971p.36)".into(),
        });
    };
    if order.shape().rows != count as usize || order.shape().cols != 1 {
        return Err("Sort returned an unexpected row count (dc-971p.36)".into());
    }
    let mut seen = BTreeSet::new();
    let mut edits = Vec::new();
    for index in 0..count {
        let Some(calc_value::CoreRef::Number(number)) =
            order.get(index as usize, 0).map(|v| v.core_ref())
        else {
            return Err("Sort did not return numeric row positions (dc-971p.36)".into());
        };
        if number < 1.
            || number > f64::from(count)
            || number.fract() != 0.
            || !seen.insert(number as u32)
        {
            return Err("Sort returned invalid row positions (dc-971p.36)".into());
        }
        let source_row = first + number as u32 - 1;
        let target_row = first + index;
        if source_row == target_row {
            continue;
        }
        let band = |row| {
            GridArea::new(
                CellKey::new(area.sheet(), row, data.first().column()).unwrap(),
                CellKey::new(area.sheet(), row, data.last().column()).unwrap(),
            )
            .unwrap()
        };
        let copy =
            crate::CopySnapshot::capture(host, band(source_row)).map_err(|e| e.to_string())?;
        edits.extend(
            copy.edits(host, band(target_row), crate::PasteMode::All, false, None)
                .map_err(|e| e.to_string())?,
        );
    }
    Ok(edits)
}
