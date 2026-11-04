//! Conditional-rule source preservation, editing and serialization through one XML route.
use super::{
    SaveRefusal,
    reader::Reader,
    text::{RawTag, Splice, xstring},
};
use crate::{
    load::{self, Events, MAIN_NAMESPACE},
    xml::{XmlEvent, XmlLimits, XmlPart},
};
use calc_doc_model::{
    ConditionalRuleSource,
    value::{CellKey, GridArea, SheetId},
};

fn sqref(areas: &[GridArea]) -> String {
    areas
        .iter()
        .map(|a| {
            let first = super::text::a1(a.first().row(), a.first().column());
            if a.cell_count() == 1 {
                first
            } else {
                format!(
                    "{first}:{}",
                    super::text::a1(a.last().row(), a.last().column())
                )
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Capture one source fragment per standard rule, including its original namespace context.
pub(crate) fn sources(
    xml: &XmlPart,
    sheet: SheetId,
) -> Result<Vec<ConditionalRuleSource>, SaveRefusal> {
    let mut reader = Reader::new(xml);
    let (_, root) = reader.root("worksheet")?;
    let root_tag = RawTag::parse(&xml.bytes()[root.0..root.1]);
    let opening = root_tag.rebuilt(&[], false);
    let closing = format!("</{}worksheet>", String::from_utf8_lossy(root_tag.prefix()));
    let mut held = Vec::new();
    let mut extensions = Vec::new();
    while let Some((event, span)) = reader.next()? {
        let XmlEvent::Start(e) = event else {
            continue;
        };
        if e.name.is(MAIN_NAMESPACE, "conditionalFormatting") {
            let tag = RawTag::parse(&xml.bytes()[span.0..span.1]);
            let start = tag.rebuilt(&[], false);
            let end = format!(
                "</{}conditionalFormatting>",
                String::from_utf8_lossy(tag.prefix())
            );
            let mut rule_parts = Vec::new();
            let mut container = Vec::new();
            loop {
                match reader.expect()? {
                    (XmlEvent::Start(child), at) => {
                        let tail = reader.element_end()?;
                        if child.name.is(MAIN_NAMESPACE, "cfRule") {
                            rule_parts.push(xml.bytes()[at.0..tail].to_vec());
                        } else {
                            container.extend_from_slice(&xml.bytes()[at.0..tail]);
                        }
                    }
                    (XmlEvent::End(_), _) => break,
                    _ => {}
                }
            }
            for rule in rule_parts {
                let mut fragment = opening.clone();
                fragment.extend_from_slice(&start);
                fragment.extend_from_slice(&rule);
                fragment.extend_from_slice(&container);
                fragment.extend_from_slice(end.as_bytes());
                held.push(fragment);
            }
        } else if e.name.is(MAIN_NAMESPACE, "extLst") {
            let end = reader.element_end()?;
            extensions.extend_from_slice(&xml.bytes()[span.0..end]);
        }
    }
    held.into_iter()
        .map(|mut fragment| {
            fragment.extend_from_slice(&extensions);
            fragment.extend_from_slice(closing.as_bytes());
            let part = XmlPart::new(
                "conditional-source.xml".into(),
                fragment.clone(),
                XmlLimits::default(),
            );
            let blocks = parse(&part).map_err(|detail| SaveRefusal::InvalidPart {
                part: xml.name().into(),
                detail,
            })?;
            let block = blocks.first().ok_or_else(|| SaveRefusal::InvalidPart {
                part: xml.name().into(),
                detail: "empty conditional source".into(),
            })?;
            let rule = &block.rules[0];
            let areas = block
                .areas
                .iter()
                .filter_map(|&(t, l, b, r)| {
                    GridArea::new(
                        CellKey::new(sheet, t, l).ok()?,
                        CellKey::new(sheet, b, r).ok()?,
                    )
                    .ok()
                })
                .collect();
            Ok(ConditionalRuleSource {
                source: fragment.clone().into(),
                areas,
                priority: rule.priority,
                stop: rule.stop_if_true,
                formulas: rule.formulas.clone(),
                threshold_formulas:threshold_formulas(&XmlPart::new("conditional-threshold-source.xml".into(),fragment.clone(),XmlLimits::default()))?,
                authored: false,
            })
        })
        .collect()
}

/// Materialize the editable fields while retaining the source rule's other fields and extensions.
pub(crate) fn materialize(source: &ConditionalRuleSource) -> Result<XmlPart, SaveRefusal> {
    let xml = XmlPart::new(
        "conditional-source.xml".into(),
        source.source.to_vec(),
        XmlLimits::default(),
    );
    let mut reader = Reader::new(&xml);
    let mut splice = Splice::new(xml.bytes());
    let mut formula_index = 0usize;
    let mut threshold_index=0usize;
    let mut in_rule = false;
    let mut formula_prefix = Vec::new();
    let mut empty_rule = false;
    while let Some((event, span)) = reader.next()? {
        match event {
            XmlEvent::Start(e) if e.name.is(MAIN_NAMESPACE, "conditionalFormatting") => {
                let value = sqref(&source.areas);
                let tag = RawTag::parse(&xml.bytes()[span.0..span.1]);
                splice.replace(
                    span,
                    &tag.rebuilt(&[("sqref", Some(&value))], tag.is_empty()),
                );
            }
            XmlEvent::Start(e) if e.name.is(MAIN_NAMESPACE, "cfRule") => {
                in_rule = true;
                let priority = source.priority.to_string();
                let tag = RawTag::parse(&xml.bytes()[span.0..span.1]);
                formula_prefix = tag.prefix().to_vec();
                empty_rule = tag.is_empty();
                splice.replace(
                    span,
                    &tag.rebuilt(
                        &[
                            ("priority", Some(&priority)),
                            ("stopIfTrue", Some(if source.stop { "1" } else { "0" })),
                        ],
                        false,
                    ),
                );
            }
            XmlEvent::Start(e) if in_rule && e.name.is(MAIN_NAMESPACE,"cfvo") && e.attribute("type")==Some("formula") && e.attribute("val").is_some() => {
                let value=source.threshold_formulas.get(threshold_index).ok_or_else(||reader.invalid("A conditional threshold formula is missing"))?;
                threshold_index+=1;
                let tag=RawTag::parse(&xml.bytes()[span.0..span.1]);splice.replace(span,&tag.rebuilt_xstring("val",&value.encode_utf16().collect::<Vec<_>>(),tag.is_empty()));
            }
            XmlEvent::Start(e) if in_rule && e.name.is(MAIN_NAMESPACE, "formula") => {
                let end = reader.element_end()?;
                let mut replacement = Vec::new();
                if let Some(formula) = source.formulas.get(formula_index) {
                    let tag = RawTag::parse(&xml.bytes()[span.0..span.1]);
                    replacement.extend(tag.rebuilt(&[], false));
                    xstring(&formula.encode_utf16().collect::<Vec<_>>(), &mut replacement);
                    replacement.extend_from_slice(b"</");
                    replacement.extend_from_slice(tag.prefix());
                    replacement.extend_from_slice(b"formula>");
                }
                formula_index += 1;
                splice.replace((span.0, end), &replacement);
            }
            XmlEvent::End(e) if in_rule && e.is(MAIN_NAMESPACE, "cfRule") => {
                let mut replacement = Vec::new();
                for formula in source.formulas.iter().skip(formula_index) {
                    write_formula(&formula_prefix, formula, &mut replacement);
                }
                if empty_rule {
                    replacement.extend_from_slice(b"</");
                    replacement.extend_from_slice(&formula_prefix);
                    replacement.extend_from_slice(b"cfRule>");
                }
                splice.replace((span.0, span.0), &replacement);
                in_rule = false;
            }
            _ => {}
        }
    }
    if threshold_index!=source.threshold_formulas.len(){return Err(reader.invalid("A conditional threshold formula has no source field"));}
    Ok(xml.with_bytes(splice.finish()))
}
fn write_formula(prefix: &[u8], text: &str, out: &mut Vec<u8>) {
    out.push(b'<');
    out.extend_from_slice(prefix);
    out.extend_from_slice(b"formula>");
    xstring(&text.encode_utf16().collect::<Vec<_>>(), out);
    out.extend_from_slice(b"</");
    out.extend_from_slice(prefix);
    out.extend_from_slice(b"formula>");
}

fn parse(xml: &XmlPart) -> Result<Vec<load::ConditionalFormat>, String> {
    let mut events = Events::new(xml);
    let mut blocks = Vec::new();
    let mut extensions = std::collections::BTreeMap::new();
    while let Some(event) = events.next().map_err(|e| e.to_string())? {
        if let XmlEvent::Start(e) = event {
            if e.name.is(MAIN_NAMESPACE, "conditionalFormatting") {
                blocks.push(
                    load::conditional::read_block(&mut events, &e).map_err(|e| e.to_string())?,
                );
            } else if e.name.is(MAIN_NAMESPACE, "extLst") {
                load::conditional::read_extensions(&mut events, &mut extensions, &mut Vec::new())
                    .map_err(|e| e.to_string())?;
            }
        }
    }
    for block in &mut blocks {
        for rule in &mut block.rules {
            if let Some(ext) = rule.extension_id.as_ref().and_then(|id| extensions.get(id)) {
                if let Some(bar) = rule.data_bar.as_mut() {
                    bar.min_length = ext.min_length;
                    bar.max_length = ext.max_length;
                    bar.gradient = ext.gradient;
                }
            }
        }
    }
    Ok(blocks)
}

pub fn conditional_rule(source: &ConditionalRuleSource) -> Result<load::ConditionalRule, String> {
    let xml = materialize(source).map_err(|e| e.to_string())?;
    parse(&xml)?
        .into_iter()
        .flat_map(|b| b.rules)
        .next()
        .ok_or_else(|| "No conditional rule was read".into())
}

/// Creates one of the supported visual rules, with original application artwork supplied by UI.
pub fn new_conditional_rule(
    areas: Vec<GridArea>,
    kind: &str,
    color: &str,
) -> Result<ConditionalRuleSource, String> {
    if areas.is_empty() || color.len() != 6 || !color.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Choose a range and a six-digit RGB colour".into());
    }
    let body=match kind {
        "dataBar"=>format!("<dataBar minLength=\"0\" maxLength=\"100\"><cfvo type=\"min\"/><cfvo type=\"max\"/><color rgb=\"FF{color}\"/></dataBar>"),
        "colorScale"=>format!("<colorScale><cfvo type=\"min\"/><cfvo type=\"percentile\" val=\"50\"/><cfvo type=\"max\"/><color rgb=\"FFF2F4F8\"/><color rgb=\"FFCFDEEB\"/><color rgb=\"FF{color}\"/></colorScale>"),
        "iconSet"=>"<iconSet iconSet=\"3Arrows\"><cfvo type=\"percent\" val=\"0\"/><cfvo type=\"percent\" val=\"33\"/><cfvo type=\"percent\" val=\"67\"/></iconSet>".into(),
        _=>return Err("This rule type cannot be authored yet (dc-971p.8)".into())
    };
    let source=format!("<worksheet xmlns=\"{MAIN_NAMESPACE}\"><conditionalFormatting xmlns=\"{MAIN_NAMESPACE}\" sqref=\"{}\"><cfRule type=\"{kind}\" priority=\"1\">{body}</cfRule></conditionalFormatting></worksheet>",sqref(&areas)).into_bytes();
    Ok(ConditionalRuleSource {
        source: source.into(),
        areas,
        priority: 1,
        stop: false,
        formulas: Vec::new(),
        threshold_formulas:Vec::new(),
        authored: true,
    })
}

/// A standard non-visual rule, whose formulas have already passed the host formula route.
pub fn new_condition_rule(
    areas: Vec<GridArea>, kind: &str, operator: &str, formulas: Vec<String>,
    rank: u32, percent: bool, dxf: u32,
) -> Result<ConditionalRuleSource, String> {
    let (kind, extra) = match kind {
        "cellIs" => {
            if !matches!(operator,"between"|"notBetween"|"equal"|"notEqual"|"greaterThan"|"lessThan"|"greaterThanOrEqual"|"lessThanOrEqual") {return Err("Choose a supported comparison".into());}
            let count=if matches!(operator,"between"|"notBetween") {2}else{1};
            if formulas.len()!=count {return Err(format!("This comparison needs {count} formula(s)"));}
            (kind,format!(" operator=\"{operator}\""))
        }
        "expression" if formulas.len()==1 => (kind,String::new()),
        "top10"|"bottom10" if formulas.is_empty() && (1..=if percent {100}else{1000}).contains(&rank) =>
            ("top10",format!(" rank=\"{rank}\" percent=\"{}\" bottom=\"{}\"",u8::from(percent),u8::from(kind=="bottom10"))),
        "aboveAverage"|"belowAverage" if formulas.is_empty() =>
            ("aboveAverage",format!(" aboveAverage=\"{}\"",u8::from(kind=="aboveAverage"))),
        "duplicateValues"|"uniqueValues" if formulas.is_empty() => (kind,String::new()),
        _=>return Err("Choose a supported rule and its required formulas or rank (dc-971p.40)".into()),
    };
    if areas.is_empty() {return Err("Choose a rule range".into());}
    let mut body=Vec::new();
    for formula in &formulas {write_formula(b"",formula,&mut body);}
    let body=String::from_utf8(body).expect("XML writer emits UTF-8");
    let source=format!("<worksheet xmlns=\"{MAIN_NAMESPACE}\"><conditionalFormatting sqref=\"{}\"><cfRule type=\"{kind}\" dxfId=\"{dxf}\" priority=\"1\"{extra}>{body}</cfRule></conditionalFormatting></worksheet>",sqref(&areas)).into_bytes();
    Ok(ConditionalRuleSource {source:source.into(),areas,priority:1,stop:false,formulas,threshold_formulas:Vec::new(),authored:true})
}

/// Replaces standard blocks in the same worksheet, retaining all unrelated elements.
pub(super) fn rewrite(
    xml: &XmlPart,
    rules: &[ConditionalRuleSource],
) -> Result<Vec<u8>, SaveRefusal> {
    if parse(xml)
        .map_err(|detail| SaveRefusal::InvalidPart {
            part: xml.name().into(),
            detail,
        })?
        .iter()
        .any(|b| b.unread_container)
    {
        return Err(SaveRefusal::InvalidPart {
            part: xml.name().into(),
            detail: "Conditional container extensions cannot be rewritten (dc-971p.8)".into(),
        });
    }
    let mut replacement = Vec::new();
    for source in rules {
        let part = materialize(source)?;
        if parse(&part)
            .map_err(|detail| SaveRefusal::InvalidPart {
                part: xml.name().into(),
                detail,
            })?
            .iter()
            .any(|b| b.unread_container)
        {
            return Err(SaveRefusal::InvalidPart {
                part: xml.name().into(),
                detail: "Conditional container extensions cannot be rewritten (dc-971p.8)".into(),
            });
        }
        let mut reader = Reader::new(&part);
        let (_, root_span) = reader.root("worksheet")?;
        let root = RawTag::parse(&part.bytes()[root_span.0..root_span.1]);
        while let Some((event, span)) = reader.next()? {
            if let XmlEvent::Start(e) = event
                && e.name.is(MAIN_NAMESPACE, "conditionalFormatting")
            {
                let end = reader.element_end()?;
                let tag = RawTag::parse(&part.bytes()[span.0..span.1]);
                replacement.extend_from_slice(&tag.with_inherited_namespaces(&root));
                replacement.extend_from_slice(&part.bytes()[span.1..end]);
            }
        }
    }
    let mut reader = Reader::new(xml);
    let (_, span) = reader.root("worksheet")?;
    let root = RawTag::parse(&xml.bytes()[span.0..span.1]);
    let mut splice = Splice::new(xml.bytes());
    if root.is_empty() {
        let mut body = root.rebuilt(&[], false);
        body.extend_from_slice(&replacement);
        body.extend_from_slice(
            format!("</{}worksheet>", String::from_utf8_lossy(root.prefix())).as_bytes(),
        );
        splice.replace(span, &body);
        return Ok(splice.finish());
    }
    let mut inserted = false;
    while let Some((event, span)) = reader.next()? {
        match event {
            XmlEvent::Start(e) if e.name.is(MAIN_NAMESPACE, "conditionalFormatting") => {
                let end = reader.element_end()?;
                splice.replace(
                    (span.0, end),
                    if inserted {
                        &[]
                    } else {
                        inserted = true;
                        &replacement
                    },
                );
            }
            // ECMA-376 worksheet order: after merges/phonetics and before validations/hyperlinks.
            XmlEvent::Start(e)
                if !inserted
                    && e.name.namespace.as_deref() == Some(MAIN_NAMESPACE)
                    && matches!(
                        e.name.local.as_ref(),
                        "dataValidations"
                            | "hyperlinks"
                            | "printOptions"
                            | "pageMargins"
                            | "pageSetup"
                            | "headerFooter"
                            | "rowBreaks"
                            | "colBreaks"
                            | "customProperties"
                            | "cellWatches"
                            | "ignoredErrors"
                            | "smartTags"
                            | "drawing"
                            | "legacyDrawing"
                            | "legacyDrawingHF"
                            | "picture"
                            | "oleObjects"
                            | "controls"
                            | "webPublishItems"
                            | "tableParts"
                            | "extLst"
                    ) =>
            {
                splice.replace((span.0, span.0), &replacement);
                inserted = true;
            }
            XmlEvent::End(e) if !inserted && e.is(MAIN_NAMESPACE, "worksheet") => {
                splice.replace((span.0, span.0), &replacement);
                inserted = true;
            }
            _ => {}
        }
    }
    Ok(splice.finish())
}

fn threshold_formulas(xml:&XmlPart)->Result<Vec<String>,SaveRefusal>{
    let mut reader=Reader::new(xml);let mut out=Vec::new();
    while let Some((event,_))=reader.next()? {if let XmlEvent::Start(e)=event&&e.name.is(MAIN_NAMESPACE,"cfvo")&&e.attribute("type")==Some("formula")&&let Some(value)=e.attribute("val"){out.push(String::from_utf16_lossy(&crate::load::strings::decode(value)));}}
    Ok(out)
}
/// Copies retain unknown rule fields; revision identifiers belong to the original instance.
pub fn copied_conditional_source(rule:&ConditionalRuleSource)->Result<ConditionalRuleSource,String>{
    let mut rule=rule.clone();let xml=XmlPart::new("conditional-copy.xml".into(),rule.source.to_vec(),XmlLimits::default());
    rule.source=super::reader::without_revision_ids(&xml).map_err(|e|e.to_string())?.into();Ok(rule)
}
pub fn conditional_with_dxf(rule:&ConditionalRuleSource,id:u32)->Result<ConditionalRuleSource,String>{
    let mut rule=rule.clone();let xml=XmlPart::new("conditional-copy.xml".into(),rule.source.to_vec(),XmlLimits::default());
    let mut reader=Reader::new(&xml);let mut splice=Splice::new(xml.bytes());let value=id.to_string();
    while let Some((event,span))=reader.next().map_err(|e|e.to_string())? {if let XmlEvent::Start(e)=event&&e.name.is(MAIN_NAMESPACE,"cfRule"){let tag=RawTag::parse(&xml.bytes()[span.0..span.1]);splice.replace(span,&tag.rebuilt(&[("dxfId",Some(&value))],tag.is_empty()));}}
    rule.source=splice.finish().into();Ok(rule)
}
