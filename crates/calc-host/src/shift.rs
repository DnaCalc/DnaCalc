//! Rectangular cell insertion/deletion. Sparse inputs and styles use the existing command route.
use crate::{CellEdit, HostState};
use calc_doc_model::value::{CellKey, ExcelText, GridArea, MAX_COLUMNS, MAX_ROWS};
use calc_doc_model::{CellInput, FormulaText};
use std::collections::{BTreeMap, BTreeSet};

/// Prepares a single command. Rich layout metadata is refused before any edit is submitted.
pub fn shift_cell_edits(
    host: &HostState,
    area: GridArea,
    down: bool,
    insert: bool,
) -> Result<Vec<CellEdit>, String> {
    let model = host.model();
    if model.sheet_by_id(area.sheet()).is_none() {
        return Err("The worksheet is unavailable".into());
    }
    let loaded = host.loaded().sheet(area.sheet());
    if loaded.is_some_and(|s| s.protected()) {
        return Err("Cell shifts on protected worksheets are unavailable (dc-971p.7)".into());
    }
    if area.cell_count() > 10000 {
        return Err("Shift at most 10000 selected cells at once".into());
    }
    let end = CellKey::new(
        area.sheet(),
        if down { MAX_ROWS } else { area.last().row() },
        if down {
            area.last().column()
        } else {
            MAX_COLUMNS
        },
    )
    .unwrap();
    let tail = GridArea::new(area.first(), end).unwrap();
    if model
        .definitions()
        .all_conditional_rules()
        .any(|(_, r)| !r.is_empty())
        || host.loaded().sheets().iter().any(|s| {
            !s.conditional_formats().is_empty()
                || !s.validations().is_empty()
                || !s.charts().charts.is_empty()
                || s.charts().unread != 0
                || s.is_macro_sheet()
        })
        || !model.definitions().tables().is_empty()
        || model
            .layout()
            .blocks()
            .iter()
            .any(|b| b.anchor().is_some_and(|a| intersects(a, tail)))
        || !host.loaded().sheet_reference_metadata_supported()
        || host
            .loaded()
            .sheets()
            .iter()
            .any(|s| !host.loaded().cell_shift_metadata_supported(s.id()))
        || loaded.is_some_and(|s| {
            !s.conditional_formats().is_empty()
                || !s.validations().is_empty()
                || !s.charts().charts.is_empty()
                || s.charts().unread != 0
                || s.is_macro_sheet()
        })
        || host
            .merged_areas(area.sheet())
            .iter()
            .any(|a| intersects(*a, tail))
        || model
            .definitions()
            .tables()
            .iter()
            .any(|t| intersects(t.area, tail))
        || model.definitions().arrays_meeting(tail).next().is_some()
        || host.engine.spills().any(|s| intersects(s.area(), tail))
        || model.definitions().unloaded_in(tail).next().is_some()
        || model
            .definitions()
            .hidden_rows(area.sheet(), tail.first().row(), tail.last().row())
            .next()
            .is_some()
    {
        return Err("Shifting cells with merged, array, table, rule, chart, hidden or unloaded metadata is not modelled (dc-971p.7); nothing changed".into());
    }
    let edit = calc_fml::reference::StructuralEdit::ShiftCells { area, down, insert };
    let mut positions: BTreeSet<CellKey> = model
        .cells(tail)
        .into_iter()
        .flatten()
        .map(|(at, _)| at)
        .collect();
    positions.extend(
        model
            .definitions()
            .cell_formats()
            .filter_map(|(at, _)| tail.contains(at).then_some(at)),
    );
    if let Some(sheet) = loaded {
        positions.extend(sheet.styled_cells().filter_map(|(row, col, _)| {
            let mut at = CellKey::new(area.sheet(), row, col).ok()?;
            for axis in &host.axis_edits {
                at = calc_doc_model::moved_cell(*axis, at)?;
            }
            tail.contains(at).then_some(at)
        }));
    }
    if positions.len() > 100000 {
        return Err("Shift at most 100000 occupied or styled cells at once".into());
    }
    if insert && positions.iter().any(|at| edit.cell(*at).is_none()) {
        return Err(
            "The insertion would push occupied or styled cells past the worksheet edge".into(),
        );
    }
    let source_styles: Vec<_> = positions
        .iter()
        .copied()
        .map(|at| (at, host.cell_style(at)))
        .collect();
    let mut edits = vec![CellEdit::Clear { area: tail }];
    let mut styles = BTreeMap::new();
    for at in positions {
        styles.insert(at, 0);
        if let Some(to) = edit.cell(at) {
            styles.entry(to).or_insert(0);
        }
        // Inputs, including formulas outside the stripe, are rewritten together below.
        if model
            .cell(at)
            .is_some_and(|input| !matches!(input, CellInput::Formula(_)))
        {
            if let Some(to) = edit.cell(at) {
                edits.push(CellEdit::Set {
                    at: to,
                    input: model.cell(at).unwrap().clone(),
                });
            }
        }
    }
    // Clear old styles first, then put the immutable source styles at their new coordinates.
    for (at, style) in source_styles {
        if let Some(to) = edit.cell(at) {
            styles.insert(to, style);
        }
    }
    if insert {
        for row in area.first().row()..=area.last().row() {
            for col in area.first().column()..=area.last().column() {
                let at = CellKey::new(area.sheet(), row, col).unwrap();
                let source = if down {
                    area.first()
                        .row()
                        .checked_sub(1)
                        .filter(|r| *r > 0)
                        .and_then(|r| CellKey::new(area.sheet(), r, col).ok())
                } else {
                    area.first()
                        .column()
                        .checked_sub(1)
                        .filter(|c| *c > 0)
                        .and_then(|c| CellKey::new(area.sheet(), row, c).ok())
                };
                styles.insert(at, source.map_or(0, |at| host.cell_style(at)));
            }
        }
    }
    edits.extend(styles.into_iter().map(|(at, format)| CellEdit::SetFormat {
        at,
        format: Some(format),
    }));
    for sheet in model.sheets() {
        for (at, input) in model.sheet_cells(sheet.id()).into_iter().flatten() {
            let CellInput::Formula(formula) = input else {
                continue;
            };
            let Some(to) = edit.cell(at) else {
                continue;
            };
            let text = String::from_utf16(formula.text().utf16_code_units())
                .map_err(|_| "The formula cannot be rewritten")?;
            let rewritten = calc_engine::shift_formula(model, &text, at, area, down, insert, false)
                .map_err(str::to_owned)?;
            if tail.contains(at) || rewritten.is_some() {
                let input = rewritten
                    .map(|text| {
                        FormulaText::new(ExcelText::from_utf16_code_units(
                            text.encode_utf16().collect(),
                        ))
                        .map(CellInput::Formula)
                    })
                    .transpose()
                    .map_err(|e| e.to_string())?
                    .unwrap_or_else(|| input.clone());
                edits.push(CellEdit::Set { at: to, input });
            }
        }
    }
    let mut name_changed = false;
    for name in model.definitions().names() {
        let sheet = match name.scope {
            calc_doc_model::NameScope::Workbook => model.sheets()[0].id(),
            calc_doc_model::NameScope::Sheet(sheet) => sheet,
        };
        if let Some(formula) = calc_engine::shift_formula(
            model,
            &name.formula,
            CellKey::new(sheet, 1, 1).unwrap(),
            area,
            down,
            insert,
            true,
        )
        .map_err(str::to_owned)?
        {
            edits.push(CellEdit::SetName {
                name: name.name.clone(),
                scope: name.scope,
                formula: Some(formula),
            });
            name_changed = true;
        }
    }
    if name_changed {
        // Rebind name users through the same edit, as copy/move already does.
        for sheet in model.sheets() {
            for (at, input) in model.sheet_cells(sheet.id()).into_iter().flatten() {
                if matches!(input, CellInput::Formula(_))
                    && !tail.contains(at)
                    && !edits
                        .iter()
                        .any(|e| matches!(e,CellEdit::Set {at:held,..} if *held==at))
                {
                    edits.push(CellEdit::Set {
                        at,
                        input: input.clone(),
                    });
                }
            }
        }
    }
    Ok(edits)
}
fn intersects(a: GridArea, b: GridArea) -> bool {
    a.sheet() == b.sheet()
        && a.first().row() <= b.last().row()
        && b.first().row() <= a.last().row()
        && a.first().column() <= b.last().column()
        && b.first().column() <= a.last().column()
}
