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
    if rows
        && host
            .model()
            .definitions()
            .hidden_rows(sheet, first, last)
            .any(|(_, why)| why == crate::RowHidden::Filtered)
    {
        return Err("Rows hidden by a filter cannot be resized or shown here".to_owned());
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
