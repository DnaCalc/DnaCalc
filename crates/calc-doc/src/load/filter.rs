//! Read the supported filter forms; unknown source is retained and makes editing unavailable.
use super::{Events, LoadError, MAIN_NAMESPACE, address};
use crate::xml::{XmlElement, XmlEvent};
use calc_doc_model::value::{CellKey, GridArea, SheetId};
use calc_doc_model::{AutoFilter, FilterCriteria, FilterOperator, FilterPredicate};
use std::collections::BTreeSet;
pub(crate) const VISIBILITY: &str = "urn:dnacalc:autofilter-visibility:v1";
fn known(e: &XmlElement, names: &[&str]) -> bool {
    e.attributes
        .iter()
        .all(|a| a.name.namespace.is_none() && names.contains(&a.name.local.as_ref()))
}
fn rows(s: Option<&str>) -> Option<BTreeSet<u32>> {
    s?.split_whitespace().map(|s| s.parse().ok()).collect()
}

fn value(text: &str, unsupported: &mut bool) -> String {
    let units = super::strings::decode(text);
    match String::from_utf16(&units) {
        Ok(s) => s,
        Err(_) => {
            *unsupported = true;
            String::from_utf16_lossy(&units)
        }
    }
}
fn leaf(events: &mut Events<'_>, unsupported: &mut bool) -> Result<(), LoadError> {
    loop {
        match events.next()? {
            Some(XmlEvent::End(_)) => return Ok(()),
            Some(XmlEvent::Text(t)) if t.trim().is_empty() => {}
            Some(XmlEvent::Start(_)) => {
                *unsupported = true;
                events.skip()?;
            }
            Some(XmlEvent::Text(_)) => *unsupported = true,
            _ => return Err(events.invalid("the filter leaf ends unexpectedly")),
        }
    }
}
fn content(events: &mut Events<'_>) -> Result<Option<XmlEvent>, LoadError> {
    loop {
        match events.next()? {
            Some(XmlEvent::Text(t)) if t.trim().is_empty() => {}
            event => return Ok(event),
        }
    }
}

pub(super) fn read(
    events: &mut Events<'_>,
    root: &XmlElement,
    sheet: SheetId,
) -> Result<(Option<AutoFilter>, Option<BTreeSet<u32>>), LoadError> {
    let Some((r, c, b, d)) = root.attribute("ref").and_then(address::area) else {
        return Err(events.invalid("the AutoFilter range is invalid"));
    };
    let area = GridArea::new(
        CellKey::new(sheet, r, c).unwrap(),
        CellKey::new(sheet, b, d).unwrap(),
    )
    .unwrap();
    let mut filter = AutoFilter::new(area);
    filter.unsupported = !known(root, &["ref"]);
    let mut manual = None;
    loop {
        match events.next()? {
            Some(XmlEvent::Start(e)) if e.name.is(MAIN_NAMESPACE, "filterColumn") => {
                let col = e
                    .attribute("colId")
                    .and_then(|s| s.parse::<u32>().ok())
                    .and_then(|n| c.checked_add(n))
                    .filter(|n| *n <= d);
                filter.unsupported |= !known(&e, &["colId"]);
                let mut criteria = None;
                loop {
                    match events.next()? {
                        Some(XmlEvent::Start(e)) if e.name.is(MAIN_NAMESPACE, "filters") => {
                            filter.unsupported |= !known(&e, &["blank"]);
                            let blank = matches!(e.attribute("blank"), Some("1" | "true"));
                            let mut values = Vec::new();
                            loop {
                                match events.next()? {
                                    Some(XmlEvent::Start(e))
                                        if e.name.is(MAIN_NAMESPACE, "filter") =>
                                    {
                                        filter.unsupported |= !known(&e, &["val"]);
                                        if let Some(v) = e.attribute("val") {
                                            values.push(value(v, &mut filter.unsupported));
                                        } else {
                                            filter.unsupported = true;
                                        }
                                        leaf(events, &mut filter.unsupported)?;
                                    }
                                    Some(XmlEvent::Start(_)) => {
                                        filter.unsupported = true;
                                        events.skip()?;
                                    }
                                    Some(XmlEvent::End(_)) => break,
                                    Some(XmlEvent::Text(t)) if t.trim().is_empty() => {}
                                    _ => {
                                        return Err(
                                            events.invalid("the filter values end unexpectedly")
                                        );
                                    }
                                }
                            }
                            if criteria
                                .replace(FilterCriteria::Values { values, blank })
                                .is_some()
                            {
                                filter.unsupported = true;
                            }
                        }
                        Some(XmlEvent::Start(e)) if e.name.is(MAIN_NAMESPACE, "customFilters") => {
                            filter.unsupported |= !known(&e, &["and"]);
                            let and = matches!(e.attribute("and"), Some("1" | "true"));
                            let mut predicates = Vec::new();
                            loop {
                                match events.next()? {
                                    Some(XmlEvent::Start(e))
                                        if e.name.is(MAIN_NAMESPACE, "customFilter") =>
                                    {
                                        filter.unsupported |= !known(&e, &["operator", "val"]);
                                        match (
                                            FilterOperator::from_xml(
                                                e.attribute("operator").unwrap_or("equal"),
                                            ),
                                            e.attribute("val"),
                                        ) {
                                            (Some(operator), Some(value)) => {
                                                predicates.push(FilterPredicate {
                                                    operator,
                                                    value: self::value(
                                                        value,
                                                        &mut filter.unsupported,
                                                    ),
                                                })
                                            }
                                            _ => filter.unsupported = true,
                                        }
                                        leaf(events, &mut filter.unsupported)?;
                                    }
                                    Some(XmlEvent::Start(_)) => {
                                        filter.unsupported = true;
                                        events.skip()?;
                                    }
                                    Some(XmlEvent::End(_)) => break,
                                    Some(XmlEvent::Text(t)) if t.trim().is_empty() => {}
                                    _ => {
                                        return Err(
                                            events.invalid("the custom filters end unexpectedly")
                                        );
                                    }
                                }
                            }
                            if criteria
                                .replace(FilterCriteria::Custom { and, predicates })
                                .is_some()
                            {
                                filter.unsupported = true;
                            }
                        }
                        Some(XmlEvent::Start(_)) => {
                            filter.unsupported = true;
                            events.skip()?;
                        }
                        Some(XmlEvent::End(_)) => break,
                        Some(XmlEvent::Text(t)) if t.trim().is_empty() => {}
                        _ => return Err(events.invalid("the filter column ends unexpectedly")),
                    }
                }
                if let Some(col) = col {
                    if let Some(criteria) = criteria {
                        if filter.columns.insert(col, criteria).is_some() {
                            filter.unsupported = true;
                        }
                    } else {
                        filter.unsupported = true;
                    }
                } else {
                    filter.unsupported = true;
                }
            }
            Some(XmlEvent::Start(e)) if e.name.is(MAIN_NAMESPACE, "extLst") => {
                filter.unsupported |= !known(&e, &[]);
                loop {
                    match events.next()? {
                        Some(XmlEvent::Start(e))
                            if e.name.is(MAIN_NAMESPACE, "ext")
                                && e.attribute("uri") == Some(VISIBILITY) =>
                        {
                            filter.unsupported |= !known(&e, &["uri"]);
                            match content(events)? {
                                Some(XmlEvent::Start(e)) if e.name.is(VISIBILITY, "visibility") => {
                                    filter.unsupported |=
                                        !known(&e, &["manual", "filtered"]) || manual.is_some();
                                    manual = rows(e.attribute("manual"));
                                    filter.unsupported |= manual.is_none();
                                    if let Some(rows) = rows(e.attribute("filtered")) {
                                        filter.filtered_rows = rows;
                                    } else {
                                        filter.unsupported = true;
                                    }
                                    leaf(events, &mut filter.unsupported)?;
                                }
                                _ => {
                                    return Err(
                                        events.invalid("the filter visibility record is invalid")
                                    );
                                }
                            }
                            filter.unsupported |= !known(&e, &["uri"]);
                            match content(events)? {
                                Some(XmlEvent::End(_)) => {}
                                _ => return Err(events.invalid("the filter extension is invalid")),
                            }
                        }
                        Some(XmlEvent::Start(_)) => {
                            filter.unsupported = true;
                            events.skip()?;
                        }
                        Some(XmlEvent::End(_)) => break,
                        Some(XmlEvent::Text(t)) if t.trim().is_empty() => {}
                        _ => return Err(events.invalid("the filter extensions end unexpectedly")),
                    }
                }
            }
            Some(XmlEvent::Start(_)) => {
                filter.unsupported = true;
                events.skip()?;
            }
            Some(XmlEvent::End(_)) => break,
            Some(XmlEvent::Text(t)) if t.trim().is_empty() => {}
            _ => return Err(events.invalid("the AutoFilter ends unexpectedly")),
        }
    }
    filter.unsupported |= !filter.valid();
    Ok((Some(filter), manual))
}
