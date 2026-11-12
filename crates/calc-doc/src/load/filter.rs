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
    filter.unsupported = !root.attributes.iter().all(|a| {
        (a.name.namespace.is_none() && a.name.local.as_str() == "ref")
            || a.name.is(
                "http://schemas.microsoft.com/office/spreadsheetml/2014/revision",
                "uid",
            )
    });
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
                            let mut groups = Vec::new();
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
                                    Some(XmlEvent::Start(e))
                                        if e.name.is(MAIN_NAMESPACE, "dateGroupItem") =>
                                    {
                                        let fields =
                                            ["year", "month", "day", "hour", "minute", "second"];
                                        let grouping =
                                            ["year", "month", "day", "hour", "minute", "second"]
                                                .iter()
                                                .position(|n| {
                                                    Some(*n) == e.attribute("dateTimeGrouping")
                                                });
                                        filter.unsupported |= !known(
                                            &e,
                                            &[
                                                "dateTimeGrouping",
                                                "year",
                                                "month",
                                                "day",
                                                "hour",
                                                "minute",
                                                "second",
                                            ],
                                        );
                                        if let Some(last) = grouping {
                                            let group = fields[..=last]
                                                .iter()
                                                .map(|f| {
                                                    e.attribute(f)
                                                        .and_then(|s| s.parse::<u16>().ok())
                                                })
                                                .collect::<Option<Vec<_>>>();
                                            if let Some(group) = group {
                                                groups.push(group);
                                            } else {
                                                filter.unsupported = true;
                                            }
                                            filter.unsupported |= fields[last + 1..]
                                                .iter()
                                                .any(|f| e.attribute(f).is_some());
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
                                .replace(if groups.is_empty() {
                                    FilterCriteria::Values { values, blank }
                                } else {
                                    FilterCriteria::Dates {
                                        groups,
                                        values,
                                        blank,
                                    }
                                })
                                .is_some()
                            {
                                filter.unsupported = true;
                            }
                        }
                        Some(XmlEvent::Start(e)) if e.name.is(MAIN_NAMESPACE, "colorFilter") => {
                            filter.unsupported |= !known(&e, &["dxfId", "cellColor"]);
                            let cell_color = match e.attribute("cellColor") {
                                None | Some("1" | "true") => Some(true),
                                Some("0" | "false") => Some(false),
                                _ => None,
                            };
                            if let (Some(dxf_id), Some(cell_color)) = (
                                e.attribute("dxfId").and_then(|v| v.parse::<u32>().ok()),
                                cell_color,
                            ) {
                                filter.unsupported |= criteria
                                    .replace(FilterCriteria::Color { dxf_id, cell_color })
                                    .is_some();
                            } else {
                                filter.unsupported = true;
                            }
                            leaf(events, &mut filter.unsupported)?;
                        }
                        Some(XmlEvent::Start(e)) if e.name.is(MAIN_NAMESPACE, "top10") => {
                            filter.unsupported |=
                                !known(&e, &["top", "percent", "val", "filterVal"]);
                            let boolean = |name, default| match e.attribute(name) {
                                None => Some(default),
                                Some("1" | "true") => Some(true),
                                Some("0" | "false") => Some(false),
                                _ => None,
                            };
                            if let (Some(top), Some(percent), Some(value)) = (
                                boolean("top", true),
                                boolean("percent", false),
                                e.attribute("val"),
                            ) {
                                filter.unsupported |= criteria
                                    .replace(FilterCriteria::Top {
                                        top,
                                        percent,
                                        value: value.to_owned(),
                                    })
                                    .is_some();
                            } else {
                                filter.unsupported = true;
                            }
                            filter.unsupported |= e
                                .attribute("filterVal")
                                .is_some_and(|v| !v.parse::<f64>().is_ok_and(f64::is_finite));
                            leaf(events, &mut filter.unsupported)?;
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

/// Restore authored manual hiding separately from the rows a filter excludes.
pub(super) fn visibility(
    filter: &mut AutoFilter,
    manual: Option<BTreeSet<u32>>,
    hidden: &[u32],
    geometry: &super::SheetGeometry,
    peer_rows: &BTreeSet<u32>,
) -> Option<BTreeSet<u32>> {
    let physically_hidden = hidden
        .iter()
        .copied()
        .filter(|r| *r > filter.area.first().row() && *r <= filter.area.last().row())
        .collect::<BTreeSet<_>>();
    let mut layout = calc_doc_model::AxisLayouts::default();
    geometry.add_to_model(filter.area.sheet(), &mut layout);
    let outlined = layout
        .outline_groups(filter.area.sheet(), calc_doc_model::Axis::Rows)
        .into_iter()
        .filter(|g| g.collapsed)
        .flat_map(|g| g.first..=g.last)
        .filter(|r| *r > filter.area.first().row() && *r <= filter.area.last().row())
        .collect::<BTreeSet<_>>();
    let manual = manual.filter(|m| {
        m.union(&filter.filtered_rows)
            .copied()
            .chain(
                peer_rows
                    .iter()
                    .copied()
                    .filter(|r| *r > filter.area.first().row() && *r <= filter.area.last().row()),
            )
            .chain(outlined.iter().copied())
            .collect::<BTreeSet<_>>()
            == physically_hidden
            && m.iter()
                .all(|r| *r > filter.area.first().row() && *r <= filter.area.last().row())
    });
    if manual.is_none() {
        filter.filtered_rows = if filter.columns.is_empty() && !filter.unsupported {
            Default::default()
        } else {
            physically_hidden
        };
    }
    manual
}
