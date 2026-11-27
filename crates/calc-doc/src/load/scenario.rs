//! SpreadsheetML Scenario Manager records. Source XML is retained by the package save path.

use super::{Events, LoadError, MAIN_NAMESPACE, address, strings};
use crate::xml::{XmlElement, XmlEvent};
use calc_doc_model::value::{CellKey, ExcelText, SheetId, fold_workbook_name};
use calc_doc_model::{ScenarioInput, WorksheetScenario, WorksheetScenarios};
use std::collections::BTreeSet;

fn flag(element: &XmlElement, name: &str) -> bool {
    matches!(element.attribute(name), Some("true" | "1"))
}

fn input(sheet: SheetId, source: &XmlElement) -> Option<ScenarioInput> {
    let (row, column) = address::cell(source.attribute("r")?)?;
    let at = CellKey::new(sheet, row, column).ok()?;
    let value = strings::decode(source.attribute("val")?);
    if value.len() > 255 {
        return None;
    }
    Some(ScenarioInput {
        at,
        value: ExcelText::from_utf16_code_units(value),
        number_format: source.attribute("numFmtId").unwrap_or("0").parse().ok()?,
        deleted: flag(source, "deleted"),
        undone: flag(source, "undone"),
    })
}

fn scenario(
    events: &mut Events<'_>,
    sheet: SheetId,
    element: XmlElement,
) -> Result<Option<WorksheetScenario>, LoadError> {
    let name = element
        .attribute("name")
        .map(strings::decode)
        .filter(|text| !text.is_empty() && text.len() <= 255)
        .and_then(|utf16| String::from_utf16(&utf16).ok());
    let expected = element
        .attribute("count")
        .map(|count| count.parse::<usize>().ok());
    let mut inputs = Vec::new();
    let mut seen = BTreeSet::new();
    let mut valid = true;
    loop {
        match events.next()? {
            Some(XmlEvent::Start(child)) => {
                if child.name.is(MAIN_NAMESPACE, "inputCells") {
                    if let Some(entry) = input(sheet, &child)
                        && inputs.len() < 32
                        && seen.insert(entry.at)
                    {
                        inputs.push(entry);
                    } else {
                        valid = false;
                    }
                }
                events.skip()?;
            }
            Some(XmlEvent::Text(_)) => {}
            Some(XmlEvent::End(_)) => break,
            None => return Err(events.invalid("the part ends inside a scenario")),
        }
    }
    if !valid || inputs.is_empty() || expected.is_some_and(|count| count != Some(inputs.len())) {
        return Ok(None);
    }
    Ok(name.map(|name| WorksheetScenario {
        name,
        hidden: flag(&element, "hidden"),
        locked: flag(&element, "locked"),
        inputs,
    }))
}

pub(super) fn read(
    events: &mut Events<'_>,
    sheet: SheetId,
) -> Result<WorksheetScenarios, LoadError> {
    let mut scenarios = WorksheetScenarios::default();
    let mut names = BTreeSet::new();
    loop {
        match events.next()? {
            Some(XmlEvent::Start(element)) if element.name.is(MAIN_NAMESPACE, "scenario") => {
                if let Some(entry) = scenario(events, sheet, element)?
                    && names.insert(fold_workbook_name(&entry.name))
                {
                    scenarios.entries.push(entry);
                } else {
                    scenarios.unread += 1;
                }
            }
            Some(XmlEvent::Start(_)) => events.skip()?,
            Some(XmlEvent::Text(_)) => {}
            Some(XmlEvent::End(_)) => return Ok(scenarios),
            None => return Err(events.invalid("the part ends inside scenarios")),
        }
    }
}
