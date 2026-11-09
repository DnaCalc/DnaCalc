//! The seven CUBE functions as the catalogue holds them. The model and the measured rules are in
//! [`crate::cube`]; this file reads the arguments and shapes the results.

use super::kit::{fail, grid};
use crate::call::EvalServices;
use crate::cube::{CubeFailure, CubeLookup, CubeMember, CubeModel};
use crate::kernel::{Exit, Site, Step};
use crate::value::{CalcValue, CoreRef, CubeValue, ExcelText, WorksheetErrorCode};
use std::sync::Arc;

use calc_catalogue::ported::cube_functions::{CODE, ENTRIES as DECLARATIONS};

fn failed<T>(site: &Site<'_>, failure: CubeFailure) -> Step<T> {
    match failure {
        CubeFailure::Error(code) => Err(Exit::Error(code)),
        CubeFailure::NotModelled => Err(site.not_modelled(None)),
        CubeFailure::CrossTableNotModelled => Err(Exit::Fault(crate::FunctionFault::NotHeld {
            id: site.id(),
            owner: "dc-8wp.19",
        })),
    }
}

/// Argument `index` as its cells: one value, or every cell of an array or range.
fn cells(
    site: &Site<'_>,
    arguments: &[CalcValue],
    index: usize,
    services: &mut EvalServices<'_>,
) -> Step<Vec<CalcValue>> {
    Ok(grid(site, arguments, index, services)?.cells)
}

fn single(
    site: &Site<'_>,
    arguments: &[CalcValue],
    index: usize,
    services: &mut EvalServices<'_>,
) -> Step<CalcValue> {
    let held = cells(site, arguments, index, services)?;
    Ok(held.into_iter().next().unwrap_or_else(CalcValue::empty))
}

/// The model a connection argument names; any other connection is #N/A.
fn model(
    site: &Site<'_>,
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Step<Arc<CubeModel>> {
    let connection = single(site, arguments, 0, services)?;
    let CoreRef::Text(name) = connection.core_ref() else {
        return fail(WorksheetErrorCode::NA);
    };
    let facts = services.host().and_then(|host| host.workbook_facts());
    match facts.map(|facts| facts.cube_model(&name.to_string_lossy())) {
        Some(CubeLookup::Model(model)) => Ok(model),
        Some(CubeLookup::NotReadable) => Err(site.not_modelled(Some(0))),
        Some(CubeLookup::NoConnection) | None => fail(WorksheetErrorCode::NA),
    }
}

fn text_value(text: &str) -> ExcelText {
    ExcelText::from_utf16_code_units(text.encode_utf16().collect())
}

/// A caption argument as text: a number or logical reads as its text.
fn caption_text(value: &CalcValue) -> Option<String> {
    match value.core_ref() {
        CoreRef::Text(text) => Some(text.to_string_lossy()),
        CoreRef::Number(number) => Some(format!("{number}")),
        CoreRef::Logical(logical) => Some(if logical { "TRUE" } else { "FALSE" }.to_owned()),
        _ => None,
    }
}

fn held_value(model: &CubeModel, tuples: &[Vec<CubeMember>], set: bool) -> CubeValue {
    CubeValue {
        connection: model.connection.clone(),
        tuples: tuples
            .iter()
            .map(|tuple| {
                tuple
                    .iter()
                    .map(|member| model.unique_name(member))
                    .collect()
            })
            .collect(),
        set,
    }
}

fn given(arguments: &[CalcValue], index: usize) -> bool {
    arguments
        .get(index)
        .is_some_and(|value| !value.is_missing())
}

fn cubemember(site: &Site<'_>, a: &[CalcValue], sv: &mut EvalServices<'_>) -> Step<CalcValue> {
    let model = model(site, a, sv)?;
    let mut tuple: Vec<CubeMember> = Vec::new();
    for cell in cells(site, a, 1, sv)? {
        let members = match (cell.cube_value(), cell.core_ref()) {
            (Some(held), _) => model
                .held(held)
                .or_else(|failure| failed(site, failure))?
                .into_iter()
                .flatten()
                .collect(),
            (None, CoreRef::Text(text)) => vec![
                model
                    .member(&text.to_string_lossy())
                    .or_else(|failure| failed(site, failure))?,
            ],
            _ => return fail(WorksheetErrorCode::NA),
        };
        for member in members {
            if tuple
                .iter()
                .any(|held| CubeModel::same_hierarchy(held, &member))
            {
                return fail(WorksheetErrorCode::NA);
            }
            tuple.push(member);
        }
    }
    let shown = if given(a, 2) {
        caption_text(&single(site, a, 2, sv)?).unwrap_or_default()
    } else {
        tuple
            .last()
            .map(|member| model.caption(member))
            .unwrap_or_default()
    };
    let held = held_value(&model, &[tuple], false);
    Ok(CalcValue::cube(text_value(&shown), held))
}

/// One CUBEVALUE argument: `None` for what Excel passes over, else its alternative tuples.
fn value_argument(
    site: &Site<'_>,
    model: &CubeModel,
    cell: &CalcValue,
) -> Step<Option<Vec<Vec<CubeMember>>>> {
    if let Some(held) = cell.cube_value() {
        return model
            .held(held)
            .map(Some)
            .or_else(|failure| failed(site, failure));
    }
    match cell.core_ref() {
        CoreRef::Text(text) => match model.member(&text.to_string_lossy()) {
            Ok(member) => Ok(Some(vec![vec![member]])),
            Err(failure) => failed(site, failure),
        },
        CoreRef::Number(_) => fail(WorksheetErrorCode::NA),
        _ => Ok(None),
    }
}

fn cubevalue(site: &Site<'_>, a: &[CalcValue], sv: &mut EvalServices<'_>) -> Step<CalcValue> {
    let model = model(site, a, sv)?;
    let mut filters = Vec::new();
    for index in 1..a.len() {
        let held = grid(site, a, index, sv)?;
        if held.rows * held.cols > 1 {
            // A range of more than one cell is a tuple only for CUBEMEMBER.
            return fail(WorksheetErrorCode::Value);
        }
        for cell in held.cells {
            if let Some(tuples) = value_argument(site, &model, &cell)? {
                filters.push(tuples);
            }
        }
    }
    model
        .value_over(&filters)
        .or_else(|failure| failed(site, failure))
}

fn cubeset(site: &Site<'_>, a: &[CalcValue], sv: &mut EvalServices<'_>) -> Step<CalcValue> {
    let model = model(site, a, sv)?;
    let mut tuples = Vec::new();
    for cell in cells(site, a, 1, sv)? {
        match (cell.cube_value(), cell.core_ref()) {
            (Some(held), _) => {
                tuples.extend(model.held(held).or_else(|failure| failed(site, failure))?);
            }
            (None, CoreRef::Text(text)) => tuples.extend(
                model
                    .set(&text.to_string_lossy())
                    .or_else(|failure| failed(site, failure))?,
            ),
            _ => return fail(WorksheetErrorCode::NA),
        }
    }
    let order = if given(a, 3) {
        match single(site, a, 3, sv)?.core_ref() {
            CoreRef::Number(number) => number.trunc() as i64,
            CoreRef::Logical(logical) => i64::from(logical),
            _ => return fail(WorksheetErrorCode::NA),
        }
    } else {
        0
    };
    let sort_by = if given(a, 4) {
        let value = single(site, a, 4, sv)?;
        match (value.cube_value(), value.core_ref()) {
            (Some(held), _) => held.tuples.first().and_then(|tuple| tuple.first()).cloned(),
            (None, CoreRef::Text(text)) => Some(text.to_string_lossy()),
            _ => None,
        }
    } else {
        None
    };
    let ordered = model
        .ordered(tuples, order, sort_by.as_deref())
        .or_else(|failure| failed(site, failure))?;
    let shown = if given(a, 2) {
        caption_text(&single(site, a, 2, sv)?).unwrap_or_default()
    } else {
        String::new()
    };
    let held = held_value(&model, &ordered, true);
    Ok(CalcValue::cube(text_value(&shown), held))
}

fn cubesetcount(site: &Site<'_>, a: &[CalcValue], sv: &mut EvalServices<'_>) -> Step<CalcValue> {
    match single(site, a, 0, sv)?.cube_value() {
        Some(held) if held.set => Ok(CalcValue::number(held.tuples.len() as f64)),
        Some(_) => fail(WorksheetErrorCode::Value),
        None => fail(WorksheetErrorCode::NA),
    }
}

fn cuberankedmember(
    site: &Site<'_>,
    a: &[CalcValue],
    sv: &mut EvalServices<'_>,
) -> Step<CalcValue> {
    let model = model(site, a, sv)?;
    let set = single(site, a, 1, sv)?;
    let tuples = match (set.cube_value(), set.core_ref()) {
        (Some(held), _) if held.set => model.held(held).or_else(|failure| failed(site, failure))?,
        (None, CoreRef::Text(text)) => model
            .set(&text.to_string_lossy())
            .or_else(|failure| failed(site, failure))?,
        _ => return fail(WorksheetErrorCode::NA),
    };
    let rank = match single(site, a, 2, sv)?.core_ref() {
        CoreRef::Number(number) => number.trunc(),
        CoreRef::Logical(logical) => f64::from(u8::from(logical)),
        CoreRef::Text(text) => match text.to_string_lossy().trim().parse::<f64>() {
            Ok(number) => number.trunc(),
            Err(_) => return fail(WorksheetErrorCode::Value),
        },
        _ => return fail(WorksheetErrorCode::Value),
    };
    if rank < 1.0 || rank > tuples.len() as f64 {
        return fail(WorksheetErrorCode::NA);
    }
    let tuple = tuples[rank as usize - 1].clone();
    let shown = if given(a, 3) {
        caption_text(&single(site, a, 3, sv)?).unwrap_or_default()
    } else {
        tuple
            .last()
            .map(|member| model.caption(member))
            .unwrap_or_default()
    };
    let held = held_value(&model, &[tuple], false);
    Ok(CalcValue::cube(text_value(&shown), held))
}

fn cubememberproperty(
    site: &Site<'_>,
    a: &[CalcValue],
    sv: &mut EvalServices<'_>,
) -> Step<CalcValue> {
    model(site, a, sv)?;
    fail(WorksheetErrorCode::NA)
}

fn cubekpimember(site: &Site<'_>, a: &[CalcValue], sv: &mut EvalServices<'_>) -> Step<CalcValue> {
    model(site, a, sv)?;
    match single(site, a, 2, sv)?.core_ref() {
        CoreRef::Text(_) => fail(WorksheetErrorCode::Value),
        _ => fail(WorksheetErrorCode::NA),
    }
}

ported_shared_family! {
    code: CODE;
    declarations: DECLARATIONS;
    1 cubemember_fn |s, a, sv| { cubemember(s, a, sv) }
    2 cubevalue_fn |s, a, sv| { cubevalue(s, a, sv) }
    3 cubeset_fn |s, a, sv| { cubeset(s, a, sv) }
    4 cubesetcount_fn |s, a, sv| { cubesetcount(s, a, sv) }
    5 cuberankedmember_fn |s, a, sv| { cuberankedmember(s, a, sv) }
    6 cubememberproperty_fn |s, a, sv| { cubememberproperty(s, a, sv) }
    7 cubekpimember_fn |s, a, sv| { cubekpimember(s, a, sv) }
}
