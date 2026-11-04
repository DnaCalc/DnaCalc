//! A worksheet's conditional formats (`<conditionalFormatting>`, dc-6yw), read as the part states
//! them: each block's areas (`sqref`) and its rules (`<cfRule>`): the rule's type, priority,
//! stop-if-true flag, differential format (`dxfId`), operator and formulas, and a colour scale's,
//! data bar's or icon set's thresholds (`<cfvo>`) and colours. A data bar's lengths and fill come
//! from the Excel 2010 extension Excel writes beside it (`x14:dataBar`, found by the rule's
//! `x14:id`); a rule that only the extension holds (an Excel 2010 rule with no SpreadsheetML 2006
//! form) is not read. What a rule shows is calc-host's to work out.

use super::{Events, LoadError, MAIN_NAMESPACE, address};
use crate::xml::{XmlElement, XmlEvent};
use calc_doc_model::StyleColor;
use std::collections::BTreeMap;

/// The namespace of Excel 2010's worksheet extensions.
pub(crate) const X14_NAMESPACE: &str =
    "http://schemas.microsoft.com/office/spreadsheetml/2009/9/main";

/// One `<conditionalFormatting>` block: its areas, as (top, left, bottom, right), in the order
/// `sqref` lists them, and its rules in document order.
#[derive(Clone, Debug, PartialEq)]
pub struct ConditionalFormat {
    pub unread_container: bool,
    pub areas: Vec<(u32, u32, u32, u32)>,
    pub rules: Vec<ConditionalRule>,
}

/// A threshold of a colour scale, data bar or icon set (`<cfvo>`): its type (`min`, `max`,
/// `num`, `percent`, `percentile`, `formula`), its value or formula, and whether a value equal
/// to it reaches it (`gte`, true unless the file says 0).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Threshold {
    pub kind: String,
    pub value: Option<String>,
    pub gte: bool,
}

/// A colour scale's thresholds and their colours.
#[derive(Clone, Debug, PartialEq)]
pub struct ColorScale {
    pub thresholds: Vec<Threshold>,
    pub colors: Vec<StyleColor>,
}

/// A data bar's two thresholds, colour and how it is drawn: the bar of the lowest value is
/// `min_length` percent of the cell's width and the highest `max_length` percent (SpreadsheetML
/// 2006's defaults are 10 and 90; Excel 2010's extension, which Excel writes, 0 and 100), filled
/// with a gradient or solid, the value shown or not.
#[derive(Clone, Debug, PartialEq)]
pub struct DataBar {
    pub thresholds: Vec<Threshold>,
    pub color: StyleColor,
    pub min_length: u32,
    pub max_length: u32,
    pub gradient: bool,
    pub show_value: bool,
}

/// An icon set: its name (`3Arrows`, `3TrafficLights1`, ...), thresholds, whether its icons run
/// in reverse and whether the value is shown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IconSet {
    pub name: String,
    pub thresholds: Vec<Threshold>,
    pub reverse: bool,
    pub show_value: bool,
}

/// One rule (`<cfRule>`).
#[derive(Clone, Debug, PartialEq)]
pub struct ConditionalRule {
    /// Source fields outside the interpreted standard rule; consumers needing a complete look
    /// must refuse rather than treating an unrecognised colour or condition as absent.
    pub unread: bool,
    /// The rule's `type`: `cellIs`, `expression`, `colorScale`, `dataBar`, `iconSet`, `top10`,
    /// `aboveAverage`, `duplicateValues`, `uniqueValues`, `containsText`, `notContainsText`,
    /// `beginsWith`, `endsWith`, `containsBlanks`, `notContainsBlanks`, `containsErrors`,
    /// `notContainsErrors`, `timePeriod`.
    pub kind: String,
    pub priority: i64,
    pub stop_if_true: bool,
    /// The differential format the rule applies (the styles part's `dxfs`).
    pub dxf: Option<u32>,
    pub operator: Option<String>,
    /// The rule's formulas (`<formula>`), as written: relative to the top-left cell of the
    /// block's first area.
    pub formulas: Vec<String>,
    pub text: Option<String>,
    pub rank: Option<u32>,
    pub percent: bool,
    pub bottom: bool,
    /// `aboveAverage`: above the average (true, the default) or below it.
    pub above_average: bool,
    pub equal_average: bool,
    pub std_dev: Option<u32>,
    pub time_period: Option<String>,
    pub color_scale: Option<ColorScale>,
    pub data_bar: Option<DataBar>,
    pub icon_set: Option<IconSet>,
    /// The rule's Excel 2010 extension id (`x14:id`), which a data bar's lengths come from.
    pub extension_id: Option<String>,
}

/// What Excel 2010's extension says of a data bar, by its rule's id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DataBarExtension {
    pub(crate) min_length: u32,
    pub(crate) max_length: u32,
    pub(crate) gradient: bool,
}

fn flag(element: &XmlElement, name: &str, default: bool) -> bool {
    match element.attribute(name).map(str::trim) {
        Some("1" | "true") => true,
        Some("0" | "false") => false,
        _ => default,
    }
}

fn number<T: std::str::FromStr>(element: &XmlElement, name: &str) -> Option<T> {
    element
        .attribute(name)
        .and_then(|value| value.trim().parse().ok())
}

fn color(element: &XmlElement) -> StyleColor {
    StyleColor {
        rgb: element.attribute("rgb").map(str::to_owned),
        indexed: element.attribute("indexed").map(str::to_owned),
        theme: element.attribute("theme").map(str::to_owned),
        tint: element.attribute("tint").map(str::to_owned),
        auto: flag(element, "auto", false),
    }
}

fn threshold(element: &XmlElement) -> Threshold {
    Threshold {
        kind: element.attribute("type").unwrap_or("num").to_owned(),
        value: element
            .attribute("val")
            .map(|s| String::from_utf16_lossy(&super::strings::decode(s))),
        gte: flag(element, "gte", true),
    }
}

/// The areas of a `sqref`: its space-separated cells and ranges.
pub(crate) fn areas(sqref: &str) -> Vec<(u32, u32, u32, u32)> {
    sqref
        .split_whitespace()
        .filter_map(|part| {
            address::area(part)
                .or_else(|| address::cell(part).map(|(row, column)| (row, column, row, column)))
        })
        .collect()
}

fn complete_areas(sqref: &str) -> Option<Vec<(u32, u32, u32, u32)>> {
    sqref.split_whitespace().map(|part| address::area(part)
        .or_else(|| address::cell(part).map(|(r,c)| (r,c,r,c))))
        .collect::<Option<Vec<_>>>().filter(|areas| !areas.is_empty())
}

/// Reads the `<conditionalFormatting>` element whose start tag `element` was just read.
pub(crate) fn read_block(
    events: &mut Events<'_>,
    element: &XmlElement,
) -> Result<ConditionalFormat, LoadError> {
    let areas = element.attribute("sqref").and_then(complete_areas);
    let mut block = ConditionalFormat {
        unread_container: areas.is_none() || element.attributes.iter().any(|a| {
            a.name.namespace.is_some() || !(a.name.local.as_str() == "sqref"
                || (a.name.local.as_str() == "pivot" && matches!(a.value.as_str(), "0" | "false")))
        }),
        areas: areas.unwrap_or_default(),
        rules: Vec::new(),
    };
    loop {
        match events.next()? {
            Some(XmlEvent::Start(rule)) if rule.name.is(MAIN_NAMESPACE, "cfRule") => {
                block.rules.push(read_rule(events, &rule)?);
            }
            Some(XmlEvent::Start(_)) => {
                block.unread_container = true;
                events.skip()?;
            }
            Some(XmlEvent::Text(_)) => {}
            Some(XmlEvent::End(_)) => return Ok(block),
            None => return Err(events.invalid("the part ends inside conditionalFormatting")),
        }
    }
}

fn read_rule(events: &mut Events<'_>, element: &XmlElement) -> Result<ConditionalRule, LoadError> {
    let bad_priority=number::<u32>(element,"priority").is_none_or(|n|n==0);
    let bad_unsigned=["dxfId","rank","stdDev"].iter().any(|key|element.attribute(key).is_some() && number::<u32>(element,key).is_none());
    let bad_flags=["stopIfTrue","percent","bottom","aboveAverage","equalAverage"].iter().any(|key|element.attribute(key).is_some_and(|v|!matches!(v.trim(),"0"|"1"|"true"|"false")));
    let mut rule = ConditionalRule {
        unread: bad_priority || bad_unsigned || bad_flags || element.attributes.iter().any(|a| {
            a.name.namespace.is_some()
                || !matches!(a.name.local.as_str(),
                    "type" | "priority" | "stopIfTrue" | "dxfId" | "operator" | "text"
                    | "rank" | "percent" | "bottom" | "aboveAverage" | "equalAverage"
                    | "stdDev" | "timePeriod")
        }),
        kind: element.attribute("type").unwrap_or_default().to_owned(),
        priority: number(element, "priority").unwrap_or(i64::MAX),
        stop_if_true: flag(element, "stopIfTrue", false),
        dxf: number(element, "dxfId"),
        operator: element.attribute("operator").map(str::to_owned),
        formulas: Vec::new(),
        text: element.attribute("text").map(str::to_owned),
        rank: number(element, "rank"),
        percent: flag(element, "percent", false),
        bottom: flag(element, "bottom", false),
        above_average: flag(element, "aboveAverage", true),
        equal_average: flag(element, "equalAverage", false),
        std_dev: number(element, "stdDev"),
        time_period: element.attribute("timePeriod").map(str::to_owned),
        color_scale: None,
        data_bar: None,
        icon_set: None,
        extension_id: None,
    };
    loop {
        match events.next()? {
            Some(XmlEvent::Start(child)) if child.name.is(MAIN_NAMESPACE, "formula") => {
                rule.unread |= !child.attributes.is_empty();
                rule.formulas
                    .push(String::from_utf16_lossy(&super::strings::decode(
                        &events.text()?,
                    )));
            }
            Some(XmlEvent::Start(child)) if child.name.is(MAIN_NAMESPACE, "colorScale") => {
                let (thresholds, colors) = read_scale(events)?;
                rule.color_scale = Some(ColorScale { thresholds, colors });
            }
            Some(XmlEvent::Start(child)) if child.name.is(MAIN_NAMESPACE, "dataBar") => {
                let (thresholds, colors) = read_scale(events)?;
                rule.data_bar = Some(DataBar {
                    thresholds,
                    color: colors.into_iter().next().unwrap_or_default(),
                    min_length: number(&child, "minLength").unwrap_or(10),
                    max_length: number(&child, "maxLength").unwrap_or(90),
                    gradient: true,
                    show_value: flag(&child, "showValue", true),
                });
            }
            Some(XmlEvent::Start(child)) if child.name.is(MAIN_NAMESPACE, "iconSet") => {
                let (thresholds, _) = read_scale(events)?;
                rule.icon_set = Some(IconSet {
                    name: child
                        .attribute("iconSet")
                        .unwrap_or("3TrafficLights1")
                        .to_owned(),
                    thresholds,
                    reverse: flag(&child, "reverse", false),
                    show_value: flag(&child, "showValue", true),
                });
            }
            Some(XmlEvent::Start(child)) if child.name.is(MAIN_NAMESPACE, "extLst") => {
                rule.unread = true;
                rule.extension_id = read_rule_extension_id(events)?;
            }
            Some(XmlEvent::Start(_)) => {
                rule.unread = true;
                events.skip()?;
            }
            Some(XmlEvent::Text(_)) => {}
            Some(XmlEvent::End(_)) => {
                if matches!(rule.kind.as_str(),"expression"|"cellIs") {
                    let count=if rule.kind=="cellIs" && matches!(rule.operator.as_deref(),Some("between"|"notBetween")){2}else{1};
                    rule.unread |= rule.formulas.len()!=count || rule.color_scale.is_some() || rule.data_bar.is_some() || rule.icon_set.is_some();
                }
                return Ok(rule);
            },
            None => return Err(events.invalid("the part ends inside cfRule")),
        }
    }
}

/// The `<cfvo>` and `<color>` children of a colour scale, data bar or icon set.
fn read_scale(events: &mut Events<'_>) -> Result<(Vec<Threshold>, Vec<StyleColor>), LoadError> {
    let (mut thresholds, mut colors) = (Vec::new(), Vec::new());
    loop {
        match events.next()? {
            Some(XmlEvent::Start(child)) if child.name.local == "cfvo" => {
                thresholds.push(threshold(&child));
                events.skip()?;
            }
            Some(XmlEvent::Start(child)) if child.name.local == "color" => {
                colors.push(color(&child));
                events.skip()?;
            }
            Some(XmlEvent::Start(_)) => events.skip()?,
            Some(XmlEvent::Text(_)) => {}
            Some(XmlEvent::End(_)) => return Ok((thresholds, colors)),
            None => return Err(events.invalid("the part ends inside a conditional format's scale")),
        }
    }
}

/// The `x14:id` inside a rule's `<extLst>`.
fn read_rule_extension_id(events: &mut Events<'_>) -> Result<Option<String>, LoadError> {
    let mut id = None;
    let mut depth = 1u32;
    while depth > 0 {
        match events.next()? {
            Some(XmlEvent::Start(child)) if child.name.is(X14_NAMESPACE, "id") => {
                id = Some(events.text()?.trim().to_owned());
            }
            Some(XmlEvent::Start(_)) => depth += 1,
            Some(XmlEvent::End(_)) => depth -= 1,
            Some(XmlEvent::Text(_)) => {}
            None => return Err(events.invalid("the part ends inside extLst")),
        }
    }
    Ok(id)
}

/// Reads the worksheet's `<extLst>`, whose start tag was just read, for the data bars of Excel
/// 2010's conditional formats (`x14:cfRule type="dataBar" id`), by id, and its data validations
/// (`x14:dataValidation`).
pub(crate) fn read_extensions(
    events: &mut Events<'_>,
    bars: &mut BTreeMap<String, DataBarExtension>,
    validations: &mut Vec<super::validation::Validation>,
) -> Result<(usize, bool, Option<Vec<(u32, u32, u32, u32)>>), LoadError> {
    let mut unread = 0usize;
    let mut conditional = false;
    let mut depth = 1u32;
    let mut rule_id: Option<String> = None;
    let mut ranges = Vec::new();
    let mut scope: Option<(u32, Option<Vec<(u32, u32, u32, u32)>>)> = None;
    let mut collection_depth = None;
    let mut unscoped = false;
    while depth > 0 {
        match events.next()? {
            Some(XmlEvent::Start(element))
                if element.name.is(X14_NAMESPACE, "conditionalFormatting")
                    || element.name.is(X14_NAMESPACE, "conditionalFormattings") =>
            {
                conditional = true;
                if element.name.local.as_str() == "conditionalFormatting" {
                    unscoped |= scope.is_some();
                    unscoped |= !element.attributes.is_empty();
                    scope = Some((depth + 1, None));
                } else {
                    unscoped |= collection_depth.is_some();
                    unscoped |= !element.attributes.is_empty();
                    collection_depth = Some(depth + 1);
                }
                depth += 1;
            }
            Some(XmlEvent::Start(element)) if element.name.is("http://schemas.microsoft.com/office/excel/2006/main", "sqref") && scope.is_some() => {
                unscoped |= !element.attributes.is_empty();
                let text = events.text()?;
                let parsed = complete_areas(&text);
                let (_, held) = scope.as_mut().unwrap();
                unscoped |= held.is_some() || parsed.is_none();
                *held = parsed;
                unscoped |= scope.as_ref().is_some_and(|(at,_)| *at != depth);
            }
            Some(XmlEvent::Start(element)) if element.name.is(X14_NAMESPACE, "dataValidation") => {
                unscoped |= collection_depth.is_some();
                validations.push(super::validation::read_x14(events, &element)?);
            }
            Some(XmlEvent::Start(element)) if element.name.is(X14_NAMESPACE, "cfRule") => {
                conditional = true;
                unscoped |= scope.as_ref().is_none_or(|(at,_)| *at != depth);
                if element.attribute("type") != Some("dataBar") {
                    unread += 1;
                }
                rule_id = element.attribute("id").map(|id| id.trim().to_owned());
                depth += 1;
            }
            Some(XmlEvent::Start(element)) if element.name.is(X14_NAMESPACE, "dataBar") => {
                conditional = true;
                unscoped |= scope.is_none();
                if let Some(id) = rule_id.clone() {
                    bars.insert(
                        id,
                        DataBarExtension {
                            min_length: number(&element, "minLength").unwrap_or(0),
                            max_length: number(&element, "maxLength").unwrap_or(100),
                            gradient: flag(&element, "gradient", true),
                        },
                    );
                }
                depth += 1;
            }
            Some(XmlEvent::Start(_)) => {
                unscoped |= collection_depth.is_some() && scope.is_none();
                depth += 1;
            }
            Some(XmlEvent::End(_)) => {
                if scope.as_ref().is_some_and(|(at,_)| *at == depth) {
                    if let Some((_, Some(areas))) = scope.take() { ranges.extend(areas); }
                    else { unscoped = true; }
                }
                if collection_depth == Some(depth) { collection_depth = None; }
                depth -= 1;
            }
            Some(XmlEvent::Text(_)) => {}
            None => return Err(events.invalid("the part ends inside extLst")),
        }
    }
    let ranges = (!unscoped && !ranges.is_empty()).then_some(ranges);
    Ok((unread, conditional, ranges))
}

/// The blocks with each data bar's lengths and fill taken from Excel 2010's extension when it
/// holds the bar's rule.
pub(crate) fn with_extensions(
    mut blocks: Vec<ConditionalFormat>,
    bars: &BTreeMap<String, DataBarExtension>,
) -> Vec<ConditionalFormat> {
    for rule in blocks.iter_mut().flat_map(|block| block.rules.iter_mut()) {
        if let (Some(bar), Some(id)) = (rule.data_bar.as_mut(), rule.extension_id.as_ref())
            && let Some(extension) = bars.get(id)
        {
            bar.min_length = extension.min_length;
            bar.max_length = extension.max_length;
            bar.gradient = extension.gradient;
        }
    }
    blocks
}
