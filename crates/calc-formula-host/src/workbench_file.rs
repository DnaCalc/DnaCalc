//! Workbook interchange for named literal trials, over the CalcDoc authoring route.

use super::trial::{evaluate_value, invalid, local_names};
use crate::Session;
use calc_doc::model::{
    ArrayFormula, ArrayKind, CellInput, CellText, FiniteNumber, FormulaForm, FormulaText,
    ModelRoot, NameScope, OpBatch, WorkbookOp,
    value::{CellKey, ExcelText, GridArea},
};
use calc_doc::{LoadedWorkbook, MemorySource, OpenPolicy, SavePolicy};
use calc_func::value::{CalcValue, CoreValue};
use calc_skin::address::column_letters;
use calc_skin::{Refusal, TestInput, Trial};
use std::{collections::BTreeSet, sync::Arc};

const SHEET: &str = "Trial";
const INPUTS: &str = "Inputs";

fn codec(error: impl std::fmt::Debug) -> Refusal {
    invalid(format!("Workbook: {error:?}"))
}

fn load(bytes: Vec<u8>) -> Result<LoadedWorkbook, Refusal> {
    let source = Arc::new(MemorySource::new(bytes));
    let opened = calc_doc::open(source, &OpenPolicy::default()).map_err(codec)?;
    calc_doc::load(&opened, &mut Default::default()).map_err(codec)
}

pub(crate) fn export(session: &mut Session, trial: Trial) -> Result<Vec<u8>, Refusal> {
    if trial.caller.is_some() {
        return Err(invalid(
            "Export the workbook from DNA Calc when the formula uses a caller snapshot",
        ));
    }
    let names = local_names(&trial.inputs)?;
    let loaded = load(calc_doc::blank_package_for_sheets(&[SHEET, INPUTS]).map_err(invalid)?)?;
    let model = loaded.model();
    let output = model.sheets()[0].id();
    let inputs = model.sheets()[1].id();
    let mut operations = Vec::new();
    let mut column = 1u32;
    for entry in trial.inputs {
        let name = entry.name.trim();
        let Some(literal) = names.get(&calc_func::value::fold_defined_name(name)) else {
            continue;
        };
        let value = evaluate_value(
            session,
            &Trial {
                formula: literal.clone(),
                ..Trial::default()
            },
            true,
        )?
        .value;
        let array = value.as_array();
        let (rows, columns) =
            array.map_or((1, 1), |array| (array.shape().rows, array.shape().cols));
        let last = CellKey::new(inputs, rows as u32, column + columns as u32 - 1).map_err(codec)?;
        for r in 0..rows {
            for c in 0..columns {
                let item = array.and_then(|array| array.get(r, c)).unwrap_or(&value);
                operations.push(WorkbookOp::SetCell {
                    at: CellKey::new(inputs, r as u32 + 1, column + c as u32).map_err(codec)?,
                    input: excel_cell(item)?,
                });
            }
        }
        let first = CellKey::new(inputs, 1, column).map_err(codec)?;
        let reference = format!(
            "{INPUTS}!{}{}",
            absolute(first),
            if first == last {
                String::new()
            } else {
                format!(":{}", absolute(last))
            }
        );
        operations.push(WorkbookOp::SetNameFormula {
            name: name.to_owned(),
            scope: NameScope::Workbook,
            formula: Some(reference),
        });
        column = last.column() + 1;
    }
    let model = calc_doc::prepare_edit(
        model,
        &OpBatch {
            base: model.revision(),
            operations,
        },
    )
    .map_err(codec)?
    .into_model();
    let at = CellKey::new(output, 1, 1).expect("worksheet origin");
    let formula = trial.formula.trim();
    let body = formula.strip_prefix('=').unwrap_or(formula);
    let entered = FormulaText::with_form(excel_text(body), FormulaForm::Dynamic).map_err(codec)?;
    let form = calc_doc::file_form(&entered, model.definitions().names()).map_err(codec)?;
    let spelling = ExcelText::from_utf16_code_units(form.text);
    let operation = if form.dynamic {
        let input = CellInput::Formula(
            FormulaText::with_form(spelling, FormulaForm::Dynamic).map_err(codec)?,
        );
        WorkbookOp::SetArray {
            array: ArrayFormula::new(GridArea::cell(at), ArrayKind::Dynamic { collapsed: false }),
            input,
        }
    } else {
        let input = CellInput::Formula(
            FormulaText::with_form(spelling, FormulaForm::Legacy).map_err(codec)?,
        );
        WorkbookOp::SetCell { at, input }
    };
    let model = calc_doc::prepare_edit(
        &model,
        &OpBatch {
            base: model.revision(),
            operations: vec![operation],
        },
    )
    .map_err(codec)?
    .into_model();
    let snapshot = loaded.snapshot(&model);
    let plan = calc_doc::prepare_save(&snapshot, &SavePolicy::default()).map_err(codec)?;
    let mut bytes = Vec::new();
    calc_doc::write(plan, &mut bytes).map_err(codec)?;
    Ok(bytes)
}

fn excel_text(text: &str) -> ExcelText {
    ExcelText::from_utf16_code_units(text.encode_utf16().collect())
}

fn excel_cell(value: &CalcValue) -> Result<CellInput, Refusal> {
    match value.core() {
        CoreValue::Number(value) => FiniteNumber::new(*value)
            .map(CellInput::Number)
            .ok_or_else(|| invalid("The input is not a finite number")),
        CoreValue::Logical(value) => Ok(CellInput::Logical(*value)),
        CoreValue::Text(value) => CellText::new(value.clone())
            .map(CellInput::Text)
            .ok_or_else(|| invalid("The input is longer than an Excel cell")),
        CoreValue::Error(value) => Ok(CellInput::Error(*value)),
        _ => Err(invalid(
            "Excel worksheet export requires a scalar input cell",
        )),
    }
}

fn absolute(at: CellKey) -> String {
    format!("\u{24}{}\u{24}{}", column_letters(at.column()), at.row())
}

pub(crate) fn import(bytes: Vec<u8>) -> Result<Trial, Refusal> {
    let loaded = load(bytes)?;
    let model = loaded.model();
    let off_scope = || {
        invalid(
            "Open this workbook in DNA Calc. Formula Explorer imports a single formula and literal named ranges exported from the workbench.",
        )
    };
    let [result, inputs] = model.sheets() else {
        return Err(off_scope());
    };
    if result.name().text().to_string_lossy() != SHEET
        || inputs.name().text().to_string_lossy() != INPUTS
        || model.definitions().external_books().next().is_some()
        || !model.definitions().tables().is_empty()
        || loaded.has_vba_project()
        || loaded.date_system() != calc_doc::DateSystem::Date1900
    {
        return Err(off_scope());
    }
    let mut formulas = model.sheet_cells(result.id()).into_iter().flatten();
    let (Some((_, CellInput::Formula(formula))), None) = (formulas.next(), formulas.next()) else {
        return Err(off_scope());
    };
    let formula = format!("={}", formula.rendered().to_string_lossy());
    let mut seen = BTreeSet::new();
    let mut entries = Vec::new();
    for name in model.definitions().names() {
        if name.scope != NameScope::Workbook
            || name.hidden
            || name.function
            || name.vb_procedure
            || name.xlm
        {
            return Err(off_scope());
        }
        let area = named_range(inputs.id(), &name.formula).ok_or_else(off_scope)?;
        let mut rows = Vec::new();
        for row in area.first().row()..=area.last().row() {
            let mut cols = Vec::new();
            for col in area.first().column()..=area.last().column() {
                let key = CellKey::new(inputs.id(), row, col).expect("cell in checked input range");
                if !seen.insert(key) {
                    return Err(off_scope());
                }
                let value = excel_literal(model, key).ok_or_else(off_scope)?;
                cols.push(value);
            }
            rows.push(cols.join(","));
        }
        let constant = if area.cell_count() == 1 {
            rows.remove(0)
        } else {
            format!("{{{}}}", rows.join(";"))
        };
        entries.push(TestInput {
            name: name.name.clone(),
            constant,
        });
    }
    if seen.len() != model.sheet_cells(inputs.id()).into_iter().flatten().count() {
        return Err(off_scope());
    }
    local_names(&entries)?;
    Ok(Trial {
        formula,
        culture: "en-US".into(),
        number_format: "General".into(),
        now: None,
        inputs: entries,
        caller: None,
    })
}

fn named_range(sheet: calc_func::value::SheetId, formula: &str) -> Option<GridArea> {
    let (name, addresses) = formula.trim_start_matches('=').split_once('!')?;
    if name.trim_matches('\'') != INPUTS {
        return None;
    }
    let to_key = |address: &str| {
        let (row, col) = calc_skin::address::parse_address(address)?;
        CellKey::new(sheet, row, col).ok()
    };
    let (first, last) = addresses.split_once(':').unwrap_or((addresses, addresses));
    GridArea::new(to_key(first)?, to_key(last)?).ok()
}

fn excel_literal(model: &ModelRoot, key: CellKey) -> Option<String> {
    let value = match model.cell(key)? {
        CellInput::Number(value) => CalcValue::number(value.get()),
        CellInput::Logical(value) => CalcValue::logical(*value),
        CellInput::Text(value) => CalcValue::text(value.text().clone()),
        CellInput::Error(value) => CalcValue::error(*value),
        CellInput::Formula(_) => return None,
    };
    calc_formula_kernel::display::formula_literal(&value).ok()
}
