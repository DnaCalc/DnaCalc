//! The information and reference functions that read host facts or build references, ported
//! from OxFunc (functions/ at 6fca3518): cell.rs, info_fn.rs, reference_metadata_family.rs
//! (ADDRESS, FORMULATEXT, SHEET, SHEETS) and address_name_classes.rs; and GETPIVOTDATA and
//! TRANSPOSE.
//!
//! A fact the host does not supply ([`crate::HostFacts`]) refuses the call as not modelled:
//! SHEET of the calling sheet or of a sheet name, SHEETS of the workbook, FORMULATEXT, and CELL's
//! address of a cell on another sheet. CELL reads `address`, `row`, `col`, `contents` and `type`
//! of the reference's first cell; its other info types (formats, widths, the file name,
//! protection) and a CELL without a reference (the last cell changed) are not modelled. INFO's
//! answers are all facts about Excel and the operating system that no host supplies, and every
//! INFO call with a known type text is refused. GETPIVOTDATA needs a pivot table, which no
//! workbook model holds, and is refused.

use super::kit::{fail, grid, num, text_arg};
use crate::call::EvalServices;
use crate::coerce;
use crate::kernel::{Exit, Site, Step};
use crate::ported::address;
use crate::value::{CalcArray, CalcValue, CellKey, CoreValue, ReferenceLike, WorksheetErrorCode};

const CODE: u16 = 204;

/// The first cell of a reference argument.
fn first_cell(site: &Site<'_>, a: &[CalcValue], index: usize) -> Step<CellKey> {
    match a[index].core() {
        CoreValue::Reference(reference) => Ok(reference.areas()[0].first()),
        _ => Err(site.not_modelled(Some(index))),
    }
}

/// The value of one cell, read through the services' cell reader.
fn cell_value(site: &Site<'_>, cell: CellKey, index: usize, sv: &mut EvalServices<'_>) -> Step<CalcValue> {
    let one = [CalcValue::reference(ReferenceLike::cell(cell))];
    let g = grid(site, &one, 0, sv).map_err(|exit| match exit {
        Exit::Fault(crate::call::FunctionFault::ReferenceUnreadable { id, .. }) => {
            Exit::Fault(crate::call::FunctionFault::ReferenceUnreadable { id, index })
        }
        other => other,
    })?;
    Ok(g.cells.into_iter().next().unwrap_or_else(CalcValue::empty))
}

fn column_letters(mut column: u32) -> String {
    let mut letters = Vec::new();
    while column > 0 {
        let rem = (column - 1) % 26;
        letters.push((b'A' + rem as u8) as char);
        column = (column - 1) / 26;
    }
    letters.iter().rev().collect()
}

fn cell(site: &Site<'_>, a: &[CalcValue], sv: &mut EvalServices<'_>) -> Step<CalcValue> {
    let info = text_arg(site, a, 0)?.to_string_lossy().trim().to_ascii_lowercase();
    let known = [
        "address", "row", "col", "contents", "type", "filename", "format", "color",
        "parentheses", "prefix", "protect", "width",
    ];
    if !known.contains(&info.as_str()) {
        return fail(WorksheetErrorCode::Value);
    }
    if a.len() < 2 || a[1].is_missing() {
        // Without a reference CELL reads the cell last changed, which is not modelled.
        return Err(site.not_modelled(None));
    }
    let at = first_cell(site, a, 1)?;
    match info.as_str() {
        "row" => s_number(site, at.row() as f64),
        "col" => s_number(site, at.column() as f64),
        "address" => {
            let caller = sv.host().and_then(|host| host.caller());
            match caller {
                Some(caller) if caller.sheet() == at.sheet() => Ok(crate::catalogue::kit::text_value(
                    &format!("${}${}", column_letters(at.column()), at.row()),
                )),
                // Another sheet's address carries the workbook's file name, which no host gives.
                _ => Err(site.not_modelled(Some(1))),
            }
        }
        "contents" => cell_value(site, at, 1, sv),
        "type" => {
            let value = cell_value(site, at, 1, sv)?;
            let kind = match value.core() {
                CoreValue::Empty | CoreValue::Missing => "b",
                CoreValue::Text(_) => "l",
                _ => "v",
            };
            Ok(crate::catalogue::kit::text_value(kind))
        }
        _ => Err(site.not_modelled(Some(0))),
    }
}

fn s_number(site: &Site<'_>, n: f64) -> Step<CalcValue> {
    site.number(n)
}

fn info(site: &Site<'_>, a: &[CalcValue]) -> Step<CalcValue> {
    let kind = text_arg(site, a, 0)?.to_string_lossy().trim().to_ascii_lowercase();
    match kind.as_str() {
        "directory" | "numfile" | "origin" | "osversion" | "recalc" | "release" | "system"
        | "memavail" | "memused" | "totmem" => Err(site.not_modelled(Some(0))),
        _ => fail(WorksheetErrorCode::Value),
    }
}

fn sheet(site: &Site<'_>, a: &[CalcValue], sv: &mut EvalServices<'_>) -> Step<CalcValue> {
    let sheet = if a.is_empty() || a[0].is_missing() {
        sv.host().and_then(|host| host.caller()).map(|cell| cell.sheet())
    } else {
        match a[0].core() {
            CoreValue::Reference(reference) => Some(reference.sheet()),
            CoreValue::Error(code) => return fail(*code),
            CoreValue::Text(text) => {
                let name = text.to_string_lossy();
                match sv.host() {
                    Some(host) => match host.sheet_named(&name) {
                        Some(sheet) => Some(sheet),
                        None if host.sheet_count().is_some() => return fail(WorksheetErrorCode::NA),
                        None => None,
                    },
                    None => None,
                }
            }
            _ => return fail(WorksheetErrorCode::NA),
        }
    };
    let index = sheet.and_then(|sheet| sv.host().and_then(|host| host.sheet_index(sheet)));
    match index {
        Some(index) => site.number(f64::from(index)),
        None => Err(site.not_modelled(Some(0))),
    }
}

fn sheets(site: &Site<'_>, a: &[CalcValue], sv: &mut EvalServices<'_>) -> Step<CalcValue> {
    if !a.is_empty() && !a[0].is_missing() {
        // A reference spans one sheet; 3-D references are not modelled by the language.
        return site.number(1.0);
    }
    match sv.host().and_then(|host| host.sheet_count()) {
        Some(count) => site.number(f64::from(count)),
        None => Err(site.not_modelled(None)),
    }
}

fn formulatext(site: &Site<'_>, a: &[CalcValue], sv: &mut EvalServices<'_>) -> Step<CalcValue> {
    let at = first_cell(site, a, 0)?;
    match sv.host().and_then(|host| host.formula_text(at)) {
        Some(Some(text)) => Ok(crate::catalogue::kit::text_value(&text)),
        Some(None) => fail(WorksheetErrorCode::NA),
        None => Err(site.not_modelled(Some(0))),
    }
}

fn address_fn(site: &Site<'_>, a: &[CalcValue]) -> Step<CalcValue> {
    let given = |i: usize| a.get(i).is_some_and(|v| !v.is_missing());
    let row = if a[0].is_missing() {
        0
    } else {
        let n = num(site, a, 0)?;
        if !n.is_finite() {
            return fail(WorksheetErrorCode::Value);
        }
        address::address_integer(n)
    };
    let col = {
        let n = num(site, a, 1)?;
        if !n.is_finite() {
            return fail(WorksheetErrorCode::Value);
        }
        address::address_integer(n)
    };
    let abs = if given(2) {
        let n = num(site, a, 2)?;
        if !n.is_finite() {
            return fail(WorksheetErrorCode::Value);
        }
        address::address_integer(n)
    } else {
        1
    };
    let a1 = if given(3) {
        match a[3].core() {
            CoreValue::Logical(b) => *b,
            CoreValue::Text(_) => site.argument(3, coerce::logical(&a[3]))?,
            _ => num(site, a, 3)? != 0.0,
        }
    } else {
        true
    };
    let sheet_text = if given(4) {
        Some(text_arg(site, a, 4)?)
    } else {
        None
    };
    if !(1..=4).contains(&abs) {
        return fail(WorksheetErrorCode::Value);
    }
    address::format_address_body(row, col, abs as usize, a1, sheet_text.as_ref())
        .map(CalcValue::text)
        .map_err(Exit::Error)
}

fn transpose(site: &Site<'_>, a: &[CalcValue], sv: &mut EvalServices<'_>) -> Step<CalcValue> {
    let g = grid(site, a, 0, sv)?;
    if !g.held {
        return Ok(a[0].clone());
    }
    let mut rows = Vec::with_capacity(g.cols);
    for c in 0..g.cols {
        let mut row = Vec::with_capacity(g.rows);
        for r in 0..g.rows {
            let value = g.cells[r * g.cols + c].clone();
            // A blank cell transposes as 0, as an array formula over a blank cell gives.
            row.push(if matches!(value.core(), CoreValue::Empty) {
                CalcValue::number(0.0)
            } else {
                value
            });
        }
        rows.push(row);
    }
    if rows.len() == 1 && rows[0].len() == 1 {
        return Ok(rows.remove(0).remove(0));
    }
    CalcArray::from_rows(rows)
        .map(CalcValue::array)
        .ok_or_else(|| site.not_modelled(None))
}

ported_family! {
    code: CODE;
    1 cell_fn "CELL" (1, 2) [V "info_type", F "reference"] |s, a, sv| { cell(s, a, sv) }
    2 info_fn "INFO" (1, 1) [V "type_text"] |s, a, sv| { info(s, a) }
    3 sheet_fn "SHEET" (0, 1) [R "value"] |s, a, sv| { sheet(s, a, sv) }
    4 sheets_fn "SHEETS" (0, 1) [F "reference"] |s, a, sv| { sheets(s, a, sv) }
    5 formulatext_fn "FORMULATEXT" (1, 1) [F "reference"] |s, a, sv| { formulatext(s, a, sv) }
    6 address_entry "ADDRESS" (2, 5) [V "row_num", V "column_num", V "abs_num", V "a1", V "sheet_text"] |s, a, sv| { address_fn(s, a) }
    7 getpivotdata_fn "GETPIVOTDATA" (2, 254) [V "data_field", F "pivot_table", V "field1", V "item1"] |s, a, sv| {
        Err(s.not_modelled(None))
    }
    8 transpose_fn "TRANSPOSE" (1, 1) [R "array"] |s, a, sv| { transpose(s, a, sv) }
}
