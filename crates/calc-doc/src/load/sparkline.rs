//! A worksheet's sparklines (dc-zjsg.7): the `x14:sparklineGroups` element of its extension list, read into
//! [`SparklineGroup`]s. The element stays in the worksheet part as stored.

use super::{Events, LoadError};
use crate::xml::XmlEvent;
use calc_doc_model::{Sparkline, SparklineGroup, StyleColor};

fn flag(element: &crate::xml::XmlElement, name: &str) -> bool {
    matches!(element.attribute(name), Some("1" | "true"))
}

fn owned(element: &crate::xml::XmlElement, name: &str) -> Option<String> {
    element.attribute(name).map(|v| v.trim().to_owned())
}

/// Reads the groups of the `x14:sparklineGroups` element whose start tag was just read, through its end tag.
pub(crate) fn read_groups(events: &mut Events<'_>) -> Result<Vec<SparklineGroup>, LoadError> {
    let mut groups = Vec::new();
    let mut group: Option<SparklineGroup> = None;
    let mut sparkline: Option<Sparkline> = None;
    // The open elements below sparklineGroups, by local name.
    let mut open: Vec<String> = Vec::new();
    loop {
        match events.next()? {
            Some(XmlEvent::Start(element)) => {
                let local = element.name.local.as_str().to_owned();
                match local.as_str() {
                    "sparklineGroup" => {
                        group = Some(SparklineGroup {
                            kind: owned(&element, "type"),
                            line_weight: owned(&element, "lineWeight"),
                            empty_cells: owned(&element, "displayEmptyCellsAs"),
                            markers: flag(&element, "markers"),
                            high: flag(&element, "high"),
                            low: flag(&element, "low"),
                            first: flag(&element, "first"),
                            last: flag(&element, "last"),
                            negative: flag(&element, "negative"),
                            axis: flag(&element, "displayXAxis"),
                            hidden: flag(&element, "displayHidden"),
                            right_to_left: flag(&element, "rightToLeft"),
                            min_axis: owned(&element, "minAxisType"),
                            max_axis: owned(&element, "maxAxisType"),
                            manual_min: owned(&element, "manualMin"),
                            manual_max: owned(&element, "manualMax"),
                            date_axis: flag(&element, "dateAxis"),
                            ..Default::default()
                        });
                    }
                    "sparkline" => sparkline = Some(Sparkline::default()),
                    // `xm:f` and `xm:sqref` hold text; reading it reads their end tags.
                    "f" | "sqref" => {
                        let text = events.text()?.trim().to_owned();
                        match (local.as_str(), sparkline.as_mut(), group.as_mut()) {
                            ("f", Some(s), _) => s.data = text,
                            ("sqref", Some(s), _) => s.cell = text,
                            ("f", None, Some(g)) => g.dates = Some(text),
                            _ => {}
                        }
                        continue;
                    }
                    name if name.starts_with("color") => {
                        if let Some(g) = group.as_mut() {
                            g.colours.push((
                                local.clone(),
                                StyleColor {
                                    rgb: owned(&element, "rgb"),
                                    indexed: owned(&element, "indexed"),
                                    theme: owned(&element, "theme"),
                                    tint: owned(&element, "tint"),
                                    auto: flag(&element, "auto"),
                                },
                            ));
                        }
                    }
                    _ => {}
                }
                open.push(local);
            }
            Some(XmlEvent::End(_)) => match open.pop().as_deref() {
                None => return Ok(groups),
                Some("sparkline") => {
                    if let (Some(s), Some(g)) = (sparkline.take(), group.as_mut()) {
                        g.sparklines.push(s);
                    }
                }
                Some("sparklineGroup") => groups.extend(group.take()),
                _ => {}
            },
            Some(XmlEvent::Text(_)) => {}
            None => return Err(events.invalid("the part ends inside sparklineGroups")),
        }
    }
}
