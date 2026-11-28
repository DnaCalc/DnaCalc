//! Native validation authoring over CalcHost's worksheet rules.

use crate::{invalid, query::checked_sheet, unavailable};
use calc_doc_model::Validation;
use calc_host::value::GridArea;
use calc_host::{CellEdit, HostState, editable_validation_list};
use calc_skin::grid::Sheet;
use calc_skin::{Refusal, ValidationSettings};

pub(crate) struct Target {
    index: Option<usize>,
    pub(crate) settings: ValidationSettings,
}

fn bounds(area: GridArea) -> (u32, u32, u32, u32) {
    let (first, last) = (area.first(), area.last());
    (first.row(), first.column(), last.row(), last.column())
}

pub(crate) fn target(
    host: &HostState,
    selected: GridArea,
    mut work: impl FnMut() -> Result<(), Refusal>,
) -> Result<Target, Refusal> {
    let sheet = checked_sheet(host, Sheet::of(selected.sheet()))?;
    let scope @ (top, left, bottom, right) = bounds(selected);
    let mut target = Target {
        index: None,
        settings: ValidationSettings::AnyValue,
    };
    for (index, rule) in host
        .model()
        .definitions()
        .validations(sheet)
        .iter()
        .enumerate()
    {
        for &(rule_top, rule_left, rule_bottom, rule_right) in &rule.areas {
            work()?;
            if rule_bottom < top || bottom < rule_top || rule_right < left || right < rule_left {
                continue;
            }
            if rule.areas.as_slice() != [scope] || target.index.is_some() {
                return Err(unavailable(
                    "dc-s2xup",
                    "Changing mixed or partially selected validation rules",
                ));
            }
            let source = editable_validation_list(rule)
                .ok_or_else(|| unavailable("dc-s2xup", "Editing this data validation rule"))?;
            target = Target {
                index: Some(index),
                settings: ValidationSettings::List { source },
            };
        }
    }
    Ok(target)
}

pub(crate) fn prepare(
    host: &HostState,
    selected: GridArea,
    settings: &ValidationSettings,
) -> Result<Option<CellEdit>, Refusal> {
    let sheet = selected.sheet();
    if host.sheet_contents_protected(sheet) {
        return Err(unavailable(
            "dc-s2xup",
            "Data Validation on a protected worksheet",
        ));
    }
    let target = target(host, selected, || Ok(()))?;
    if &target.settings == settings {
        return Ok(None);
    }
    let mut rules = host.model().definitions().validations(sheet).to_vec();
    match settings {
        ValidationSettings::AnyValue => {
            if let Some(index) = target.index {
                rules.remove(index);
            }
        }
        ValidationSettings::List { source } => {
            if source.trim().is_empty() || source.encode_utf16().count() > 255 {
                return Err(invalid(
                    "Enter a comma-separated list of up to 255 characters.",
                ));
            }
            if source.trim_start().starts_with('=') {
                return Err(unavailable(
                    "dc-s2xup",
                    "Data validation lists with a formula source",
                ));
            }
            let formula = Some(format!("\"{}\"", source.replace('"', "\"\"")));
            if let Some(index) = target.index {
                rules[index].formula1 = formula;
            } else {
                rules.push(Validation {
                    authored_copy: false,
                    source: None,
                    areas: vec![bounds(selected)],
                    kind: "list".into(),
                    operator: "between".into(),
                    allow_blank: true,
                    show_input: false,
                    show_error: true,
                    in_cell_dropdown: true,
                    error_style: "stop".into(),
                    error_title: None,
                    error: None,
                    prompt_title: None,
                    prompt: None,
                    formula1: formula,
                    formula2: None,
                });
            }
        }
    }
    Ok(Some(CellEdit::Validations { sheet, rules }))
}
