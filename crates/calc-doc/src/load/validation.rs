//! A worksheet's data validations (`<dataValidations>`, and Excel 2010's `x14:dataValidations`
//! in the worksheet's extension list, where Excel writes a rule whose formulas name another sheet;
//! dc-6yw), read as the part states them. What a rule allows is calc-host's to work out.

use super::{Events, LoadError, MAIN_NAMESPACE, conditional};
use crate::xml::{XmlElement, XmlEvent};

/// The namespace of the `xm:f` and `xm:sqref` elements of Excel's extensions.
const XM_NAMESPACE: &str = "http://schemas.microsoft.com/office/excel/2006/main";

pub use calc_doc_model::Validation;

fn flag(element: &XmlElement, name: &str) -> bool {
    matches!(element.attribute(name).map(str::trim), Some("1" | "true"))
}

fn from_attributes(element: &XmlElement) -> Validation {
    let text = |name: &str| {
        element
            .attribute(name)
            .map(|s| String::from_utf16_lossy(&super::strings::decode(s)))
    };
    Validation {
        source: None,
        areas: conditional::areas(element.attribute("sqref").unwrap_or_default()),
        kind: text("type").unwrap_or_else(|| "none".to_owned()),
        operator: text("operator").unwrap_or_else(|| "between".to_owned()),
        allow_blank: flag(element, "allowBlank"),
        show_input: flag(element, "showInputMessage"),
        show_error: flag(element, "showErrorMessage"),
        in_cell_dropdown: !flag(element, "showDropDown"),
        error_style: text("errorStyle").unwrap_or_else(|| "stop".to_owned()),
        error_title: text("errorTitle"),
        error: text("error"),
        prompt_title: text("promptTitle"),
        prompt: text("prompt"),
        formula1: None,
        formula2: None,
    }
}

/// Reads the `<dataValidations>` element whose start tag was just read.
pub(crate) fn read_validations(events: &mut Events<'_>) -> Result<Vec<Validation>, LoadError> {
    let mut validations = Vec::new();
    loop {
        match events.next()? {
            Some(XmlEvent::Start(element)) if element.name.is(MAIN_NAMESPACE, "dataValidation") => {
                let mut validation = from_attributes(&element);
                loop {
                    match events.next()? {
                        Some(XmlEvent::Start(child))
                            if child.name.is(MAIN_NAMESPACE, "formula1") =>
                        {
                            validation.formula1 = Some(events.text()?);
                        }
                        Some(XmlEvent::Start(child))
                            if child.name.is(MAIN_NAMESPACE, "formula2") =>
                        {
                            validation.formula2 = Some(events.text()?);
                        }
                        Some(XmlEvent::Start(_)) => events.skip()?,
                        Some(XmlEvent::Text(_)) => {}
                        Some(XmlEvent::End(_)) => break,
                        None => return Err(events.invalid("the part ends inside dataValidation")),
                    }
                }
                validations.push(validation);
            }
            Some(XmlEvent::Start(_)) => events.skip()?,
            Some(XmlEvent::Text(_)) => {}
            Some(XmlEvent::End(_)) => return Ok(validations),
            None => return Err(events.invalid("the part ends inside dataValidations")),
        }
    }
}

/// Reads an `x14:dataValidation`, whose start tag `element` was just read: its formulas are in
/// `x14:formula1`/`x14:formula2` as `xm:f`, its areas in `xm:sqref`.
pub(crate) fn read_x14(
    events: &mut Events<'_>,
    element: &XmlElement,
) -> Result<Validation, LoadError> {
    let mut validation = from_attributes(element);
    let mut slot = 0u8;
    let mut depth = 1u32;
    while depth > 0 {
        match events.next()? {
            Some(XmlEvent::Start(child)) if child.name.local == "formula1" => {
                slot = 1;
                depth += 1;
            }
            Some(XmlEvent::Start(child)) if child.name.local == "formula2" => {
                slot = 2;
                depth += 1;
            }
            Some(XmlEvent::Start(child)) if child.name.is(XM_NAMESPACE, "f") => {
                let text = events.text()?;
                match slot {
                    1 => validation.formula1 = Some(text),
                    2 => validation.formula2 = Some(text),
                    _ => {}
                }
            }
            Some(XmlEvent::Start(child)) if child.name.is(XM_NAMESPACE, "sqref") => {
                validation.areas = conditional::areas(&events.text()?);
            }
            Some(XmlEvent::Start(_)) => depth += 1,
            Some(XmlEvent::End(_)) => depth -= 1,
            Some(XmlEvent::Text(_)) => {}
            None => return Err(events.invalid("the part ends inside x14:dataValidation")),
        }
    }
    Ok(validation)
}
