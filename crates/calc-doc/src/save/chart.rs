//! Edit retained chart properties while preserving plots, axes, styles and extension records.
use super::{
    SaveRefusal,
    author::Changes,
    reader::Reader,
    strings::read_part,
    text::{RawTag, Splice},
};
use crate::{
    opc::Package,
    xml::{XmlEvent, XmlPart},
};
use calc_doc_model::{Chart, ModelRoot};
const C: &str = "http://schemas.openxmlformats.org/drawingml/2006/chart";
const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
type Patch = ((usize, usize), Vec<u8>);

pub(crate) fn source(
    package: &Package,
    base: &ModelRoot,
    before: &Chart,
    after: &Chart,
) -> Result<Vec<u8>, SaveRefusal> {
    source_with_xml(package, base, before, after, None)
}

fn source_with_xml(
    package: &Package,
    base: &ModelRoot,
    before: &Chart,
    after: &Chart,
    rewritten: Option<&[u8]>,
) -> Result<Vec<u8>, SaveRefusal> {
    let invalid = |detail: &str| SaveRefusal::ChartNotModelled {
        part: before.part.clone(),
        detail: detail.into(),
        owner: "dc-e5z5",
    };
    if base
        .definitions()
        .all_charts()
        .flat_map(|(_, charts)| charts)
        .filter(|chart| chart.part == before.part)
        .count()
        != 1
    {
        return Err(invalid(
            "Editing chart parts shared by drawing frames is not modelled",
        ));
    }
    let mut retained = after.clone();
    retained.title = before.title.clone();
    retained.title_formula = before.title_formula.clone();
    retained.auto_title_deleted = before.auto_title_deleted;
    retained.legend = before.legend.clone();
    if retained != *before {
        return Err(invalid(
            "Chart creation, removal and changes outside title and legend are not modelled",
        ));
    }
    if after.title_formula.is_some() && after.title_formula != before.title_formula {
        return Err(invalid("Authoring a linked chart title is not modelled"));
    }
    if after
        .legend
        .as_deref()
        .is_some_and(|p| !matches!(p, "r" | "l" | "t" | "b" | "tr"))
    {
        return Err(invalid("Choose a supported legend position"));
    }
    if after.title.as_ref().is_some_and(|t| {
        t.encode_utf16().count() > 32767
            || t.chars().any(|c| {
                (c < ' ' && !matches!(c, '\t' | '\n' | '\r'))
                    || matches!(c, '\u{fffe}' | '\u{ffff}')
            })
    }) {
        return Err(invalid(
            "The chart title contains unsupported text or exceeds 32,767 UTF-16 units",
        ));
    }
    let drawings: Vec<&str> = package
        .parts()
        .iter()
        .flat_map(|part| {
            package
                .relationships_of(Some(part.name()))
                .into_iter()
                .flat_map(|rels| rels.items())
                .filter(move |r| {
                    r.kind.ends_with("/chart")
                        && r.resolved.as_deref() == Some(before.part.as_str())
                })
                .map(move |_| part.name())
        })
        .collect();
    if drawings.len() != 1 {
        return Err(invalid(
            "Editing a chart part shared by drawing frames is not modelled",
        ));
    }
    let owners = package
        .parts()
        .iter()
        .flat_map(|part| {
            package
                .relationships_of(Some(part.name()))
                .into_iter()
                .flat_map(|rels| rels.items())
        })
        .filter(|r| r.resolved.as_deref() == Some(drawings[0]))
        .count();
    if owners != 1 {
        return Err(invalid(
            "Editing a drawing part shared by worksheets is not modelled",
        ));
    }
    let original = read_part(package, &before.part)?;
    let xml = rewritten
        .map(|bytes| original.with_bytes(bytes.to_vec()))
        .unwrap_or(original);
    let mut reader = Reader::new(&xml);
    let mut patches = Vec::new();
    let mut found = false;
    match reader.expect()? {
        (XmlEvent::Start(e), _) if e.name.is(C, "chartSpace") => {}
        _ => return Err(invalid("The part has no chartSpace root")),
    }
    loop {
        match reader.expect()? {
            (XmlEvent::Start(e), span) if e.name.is(C, "chart") => {
                if found {
                    return Err(invalid("The part holds more than one chart"));
                }
                found = true;
                chart(&xml, &mut reader, span, before, after, &mut patches)?;
            }
            (XmlEvent::Start(_), _) => {
                reader.element_end()?;
            }
            (XmlEvent::End(_), _) => break,
            _ => {}
        }
    }
    reader.end()?;
    if !found {
        return Err(invalid("The part holds no chart"));
    }
    patches.sort_by_key(|(span, _)| (span.0, span.1));
    let mut splice = Splice::new(xml.bytes());
    for (span, bytes) in patches {
        splice.replace(span, &bytes);
    }
    Ok(splice.finish())
}
fn plain_text(title: &str) -> String {
    title
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\r', "&#13;")
}
fn tx(title: &str) -> Vec<u8> {
    let text = plain_text(title);
    format!("<c:tx xmlns:c=\"{C}\" xmlns:a=\"{A}\"><c:rich><a:bodyPr/><a:lstStyle/><a:p><a:r><a:t xml:space=\"preserve\">{text}</a:t></a:r></a:p></c:rich></c:tx>").into_bytes()
}

fn field(
    xml: &XmlPart,
    reader: &mut Reader<'_>,
    span: (usize, usize),
    local: &str,
    fresh: &[u8],
    position: Option<&str>,
    title: Option<&str>,
    patches: &mut Vec<Patch>,
) -> Result<(), SaveRefusal> {
    let tag = RawTag::parse(&xml.bytes()[span.0..span.1]);
    let mut found = false;
    let end = loop {
        match reader.expect()? {
            (XmlEvent::Start(e), child) if e.name.is(C, local) => {
                if found {
                    return Err(reader.invalid("A chart property has duplicate children (dc-e5z5)"));
                }
                found = true;
                if let Some(title) = title {
                    let mut retained = Vec::new();
                    let (end, literal) = literal_title(xml, reader, title, &mut retained)?;
                    if literal {
                        patches.extend(retained);
                    } else {
                        patches.push(((child.0, end), fresh.to_vec()));
                    }
                    continue;
                }
                let end = reader.element_end()?;
                if let Some(position) = position {
                    let child_tag = RawTag::parse(&xml.bytes()[child.0..child.1]);
                    patches.push((
                        child,
                        child_tag.rebuilt(&[("val", Some(position))], child_tag.is_empty()),
                    ));
                } else {
                    patches.push(((child.0, end), fresh.to_vec()));
                }
            }
            (XmlEvent::Start(_), _) => {
                reader.element_end()?;
            }
            (XmlEvent::End(_), (_, end)) => break end,
            _ => {}
        }
    };
    if !found {
        if tag.is_empty() {
            let mut expanded = tag.rebuilt(&[], false);
            expanded.extend_from_slice(fresh);
            expanded.extend_from_slice(b"</");
            expanded.extend_from_slice(tag.qualified_name());
            expanded.push(b'>');
            patches.push(((span.0, end), expanded));
        } else {
            patches.push(((span.1, span.1), fresh.to_vec()));
        }
    }
    Ok(())
}
fn chart(
    xml: &XmlPart,
    reader: &mut Reader<'_>,
    _: (usize, usize),
    before: &Chart,
    after: &Chart,
    patches: &mut Vec<Patch>,
) -> Result<(), SaveRefusal> {
    let title_changed = before.title != after.title
        || before.title_formula != after.title_formula
        || before.auto_title_deleted != after.auto_title_deleted;
    let legend_changed = before.legend != after.legend;
    let (mut title, mut auto, mut legend, mut plot) = (false, false, false, None);
    let mut auto_start = None;
    let mut following_title = None;
    loop {
        match reader.expect()? {
            (XmlEvent::Start(e), span) if e.name.is(C, "title") && title_changed => {
                if title {
                    return Err(reader.invalid("Duplicate chart titles (dc-e5z5)"));
                }
                title = true;
                if let Some(text) = &after.title {
                    field(
                        xml,
                        reader,
                        span,
                        "tx",
                        &tx(text),
                        None,
                        Some(text),
                        patches,
                    )?;
                } else {
                    let end = reader.element_end()?;
                    patches.push(((span.0, end), Vec::new()));
                }
            }
            (XmlEvent::Start(e), span) if e.name.is(C, "autoTitleDeleted") && title_changed => {
                if auto {
                    return Err(reader.invalid("Duplicate automatic title flags (dc-e5z5)"));
                }
                auto = true;
                auto_start = Some(span.0);
                let tag = RawTag::parse(&xml.bytes()[span.0..span.1]);
                patches.push((
                    span,
                    tag.rebuilt(
                        &[(
                            "val",
                            Some(if after.auto_title_deleted { "1" } else { "0" }),
                        )],
                        tag.is_empty(),
                    ),
                ));
                reader.element_end()?;
            }
            (XmlEvent::Start(e), span) if e.name.is(C, "legend") && legend_changed => {
                if legend {
                    return Err(reader.invalid("Duplicate chart legends (dc-e5z5)"));
                }
                legend = true;
                if let Some(position) = &after.legend {
                    let fresh = format!("<c:legendPos xmlns:c=\"{C}\" val=\"{position}\"/>");
                    field(
                        xml,
                        reader,
                        span,
                        "legendPos",
                        fresh.as_bytes(),
                        Some(position),
                        None,
                        patches,
                    )?;
                } else {
                    let end = reader.element_end()?;
                    patches.push(((span.0, end), Vec::new()));
                }
            }
            (XmlEvent::Start(e), span)
                if ["pivotFmts", "view3D", "floor", "sideWall", "backWall"]
                    .iter()
                    .any(|name| e.name.is(C, name)) =>
            {
                following_title.get_or_insert(span.0);
                reader.element_end()?;
            }
            (XmlEvent::Start(e), span) if e.name.is(C, "plotArea") => {
                if plot.is_some() {
                    return Err(reader.invalid("Duplicate chart plot areas (dc-e5z5)"));
                }
                plot = Some((span.0, reader.element_end()?));
            }
            (XmlEvent::Start(_), _) => {
                reader.element_end()?;
            }
            (XmlEvent::End(_), _) => break,
            _ => {}
        }
    }
    let plot = plot.ok_or_else(|| reader.invalid("A chart has no plot area (dc-e5z5)"))?;
    if title_changed {
        let mut fresh = Vec::new();
        if !title {
            if let Some(text) = &after.title {
                fresh.extend_from_slice(format!("<c:title xmlns:c=\"{C}\">").as_bytes());
                fresh.extend_from_slice(&tx(text));
                fresh.extend_from_slice(b"</c:title>");
            }
        }
        if !fresh.is_empty() {
            let at = auto_start.or(following_title).unwrap_or(plot.0);
            patches.push(((at, at), fresh));
            fresh = Vec::new();
        }
        if !auto {
            fresh.extend_from_slice(
                format!(
                    "<c:autoTitleDeleted xmlns:c=\"{C}\" val=\"{}\"/>",
                    u8::from(after.auto_title_deleted)
                )
                .as_bytes(),
            );
        }
        if !fresh.is_empty() {
            let at = following_title.unwrap_or(plot.0);
            patches.push(((at, at), fresh));
        }
    }
    if legend_changed && !legend {
        if let Some(position) = &after.legend {
            patches.push(((plot.1,plot.1),format!("<c:legend xmlns:c=\"{C}\"><c:legendPos val=\"{position}\"/><c:overlay val=\"0\"/></c:legend>").into_bytes()));
        }
    }
    Ok(())
}
pub(super) fn finish(
    package: &Package,
    before: &ModelRoot,
    after: &ModelRoot,
    out: &mut Changes,
) -> Result<(), SaveRefusal> {
    for (sheet, charts) in before.definitions().all_charts() {
        if after.sheet_by_id(sheet).is_none() {
            continue;
        }
        let next = after.definitions().charts(sheet);
        if charts == next {
            continue;
        }
        if charts.len() != next.len() {
            return Err(SaveRefusal::ChartNotModelled {
                part: "charts".into(),
                detail: "Adding and removing charts is not modelled".into(),
                owner: "dc-e5z5",
            });
        }
        for (old, new) in charts.iter().zip(next) {
            if old != new {
                let bytes = source_with_xml(
                    package,
                    before,
                    old,
                    new,
                    out.rewritten.get(&old.part).map(Vec::as_slice),
                )?;
                out.rewritten.insert(old.part.clone(), bytes);
            }
        }
    }
    for (sheet, charts) in after.definitions().all_charts() {
        if !charts.is_empty() && before.definitions().charts(sheet).is_empty() {
            return Err(SaveRefusal::ChartNotModelled {
                part: "charts".into(),
                detail: "Creating charts is not modelled".into(),
                owner: "dc-e5z5",
            });
        }
    }
    Ok(())
}

/// Keep literal run and paragraph formatting; a changed title occupies the first run.
/// Later runs retain their properties with empty text, so their old characters cannot survive.
fn literal_title(
    xml: &XmlPart,
    reader: &mut Reader<'_>,
    title: &str,
    patches: &mut Vec<Patch>,
) -> Result<(usize, bool), SaveRefusal> {
    let mut parents = Vec::<crate::xml::XmlName>::new();
    let (mut rich, mut linked, mut texts, mut paragraphs) = (false, false, 0usize, 0usize);
    let end = loop {
        match reader.expect()? {
            (XmlEvent::Start(e), _) if parents.is_empty() && e.name.is(C, "strRef") => {
                if linked {
                    return Err(reader.invalid("Duplicate linked title records (dc-e5z5)"));
                }
                linked = true;
                reader.element_end()?;
            }
            (XmlEvent::Start(e), _) if parents.is_empty() && !e.name.is(C, "rich") => {
                return Err(
                    reader.invalid("This chart title has unsupported text records (dc-e5z5)")
                );
            }
            (XmlEvent::Start(e), _) if e.name.is(A, "fld") || e.name.is(A, "br") => {
                return Err(reader.invalid(
                    "Editing a chart title with DrawingML fields or manual line breaks is not modelled (dc-e5z5)",
                ));
            }
            (XmlEvent::Start(e), span) if e.name.is(A, "t") => {
                if parents.len() != 3
                    || !parents[0].is(C, "rich")
                    || !parents[1].is(A, "p")
                    || !parents[2].is(A, "r")
                {
                    return Err(
                        reader.invalid("This chart title has unsupported text placement (dc-e5z5)")
                    );
                }
                let end = reader.element_end()?;
                let tag = RawTag::parse(&xml.bytes()[span.0..span.1]);
                let mut bytes = tag.rebuilt(&[("xml:space", Some("preserve"))], false);
                if texts == 0 {
                    bytes.extend_from_slice(plain_text(title).as_bytes());
                }
                texts += 1;
                bytes.extend_from_slice(b"</");
                bytes.extend_from_slice(tag.qualified_name());
                bytes.push(b'>');
                patches.push(((span.0, end), bytes));
            }
            (XmlEvent::Start(e), _) => {
                if parents.is_empty() {
                    if rich {
                        return Err(reader.invalid("Duplicate rich title records (dc-e5z5)"));
                    }
                    rich = true;
                }
                if parents.len() == 1 && e.name.is(A, "p") {
                    paragraphs += 1;
                    if paragraphs > 1 {
                        return Err(reader.invalid(
                            "Editing a title with multiple paragraphs is not modelled (dc-e5z5)",
                        ));
                    }
                }
                parents.push(e.name);
            }
            (XmlEvent::End(_), (_, end)) => {
                if parents.pop().is_none() {
                    break end;
                }
            }
            _ => {}
        }
    };
    if rich && linked {
        return Err(reader.invalid("A chart title contains both literal and linked text (dc-e5z5)"));
    }
    Ok((end, rich && texts > 0))
}
