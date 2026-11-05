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
use calc_doc_model::{Chart, ChartPlot, ChartSeries, ChartData, ModelRoot};
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
    retained.plots = before.plots.clone();
    retained.axes = before.axes.clone();
    if retained != *before {
        return Err(invalid(
            "Changes to the chart frame are not modelled",
        ));
    }
    if before.plots.len()!=after.plots.len() {return Err(invalid("Adding or removing chart plots is not modelled (dc-971p.46)"));}
    if before.plots.len()!=1 && before.plots.iter().zip(&after.plots).any(|(a,b)|plot_type_changed(a,b)){return Err(invalid("Changing combination chart types is not modelled (dc-971p.46)"));}
    let old_total=before.plots.iter().map(|p|p.series.len()).sum::<usize>();let new_total=after.plots.iter().map(|p|p.series.len()).sum::<usize>();
    if new_total>255 && new_total>old_total{return Err(invalid("A chart can have at most 255 series (dc-971p.46)"));}
    for (old,new) in before.plots.iter().zip(&after.plots) {
        let mut retained=new.clone();retained.series=old.series.clone();retained.kind=old.kind.clone();retained.bar_direction=old.bar_direction.clone();retained.grouping=old.grouping.clone();
        if retained!=*old || !valid_plot_type(old,new) {return Err(invalid("Changing these plot properties is not modelled (dc-971p.46)"));}
        if old.series==new.series {continue;}
        if pie_kind(&old.kind) && new.series.len()>old.series.len(){return Err(invalid("Adding pie or doughnut series is not modelled (dc-971p.46)"));}
        if !matches!(old.kind.as_str(),"barChart"|"lineChart"|"areaChart"|"pieChart"|"doughnutChart"|"scatterChart") {return Err(invalid("Editing this plot's series is not modelled (dc-971p.46)"));}
        let mut indices=std::collections::BTreeSet::new();
        if new.series.is_empty() || !new.series.iter().all(|s|indices.insert(s.index)) {return Err(invalid("Use 1 to 255 series with distinct indices (dc-971p.46)"));}
        let existing=new.series.iter().filter(|s|old.series.iter().any(|o|o.index==s.index)).map(|s|s.index).collect::<Vec<_>>();
        let retained=old.series.iter().filter(|s|new.series.iter().any(|n|n.index==s.index)).map(|s|s.index).collect::<Vec<_>>();
        if existing!=retained {return Err(invalid("Reordering chart series is not modelled (dc-971p.46)"));}
        for series in &new.series {
            if let Some(original)=old.series.iter().find(|s|s.index==series.index) {
                let mut keep=series.clone();keep.name=original.name.clone();keep.categories=original.categories.clone();keep.values=original.values.clone();
                if keep!=*original {return Err(invalid("Changing series formatting is not modelled (dc-971p.46)"));}
            } else if series.fill.is_some() {return Err(invalid("New series formatting is not modelled (dc-971p.46)"));}
        }
    }
    if before.axes.len()!=after.axes.len(){return Err(invalid("Creating or removing chart axes is not modelled (dc-971p.46)"));}
    let swap=before.plots.first().zip(after.plots.first()).is_some_and(|(a,b)|category_kind(&a.kind)&&category_kind(&b.kind) && (a.kind=="barChart" && a.bar_direction.as_deref()==Some("bar"))!=(b.kind=="barChart" && b.bar_direction.as_deref()==Some("bar")));
    for (old,new) in before.axes.iter().zip(&after.axes) {
        let expected=old.position.as_deref().map(|p|(if swap{match p{"b"=>"l","t"=>"r","l"=>"b","r"=>"t",other=>other}}else{p}).to_owned());
        if new.position!=expected{return Err(invalid("Only a type conversion can transpose chart axis positions (dc-971p.46)"));}
        let settings=old.minimum!=new.minimum || old.maximum!=new.maximum || old.major_unit!=new.major_unit || old.reversed!=new.reversed;
        let mut retained=new.clone();retained.position=old.position.clone();retained.minimum=old.minimum.clone();retained.maximum=old.maximum.clone();retained.major_unit=old.major_unit.clone();retained.reversed=old.reversed;
        if retained!=*old || (settings && (old.kind!="valAx" || old.log_base.is_some())) {return Err(invalid("Only retained linear value-axis bounds, units and direction can be edited (dc-971p.46)"));}
        if old==new {continue;}
        for text in [new.minimum.as_deref(),new.maximum.as_deref(),new.major_unit.as_deref()].into_iter().flatten() {
            if text.parse::<f64>().ok().is_none_or(|n|!n.is_finite()){return Err(invalid("Use finite numeric axis settings (dc-971p.46)"));}
        }
        if new.major_unit.as_deref().and_then(|s|s.parse::<f64>().ok()).is_some_and(|n|n<=0.) {return Err(invalid("The major unit must be positive (dc-971p.46)"));}
        if let (Some(min),Some(max))=(&new.minimum,&new.maximum) {if min.parse::<f64>().unwrap()>=max.parse::<f64>().unwrap(){return Err(invalid("The axis maximum must exceed its minimum (dc-971p.46)"));}}
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
    validate_series_orders(&xml,before,after)?;
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
                let end=plot_area(xml,reader,before,after,patches)?;
                plot = Some((span.0, end));
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
    structural: Option<&super::structural::StructuralEdits>,
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
                // Author in the source coordinate basis, then use the existing structural
                // pass for every formula, including newly added series.
                let mut bytes = source(package,before,old,new)?;
                if let Some(structural)=structural {
                    let original=read_part(package,&old.part)?;
                    for &edit in structural.edits() {
                        let xml=original.with_bytes(bytes);
                        let rewrite=super::structural::Rewrite{root:before,rewriter:structural.rewriter.as_ref(),change:calc_doc_model::StructuralChange::Axis(edit)};
                        bytes=super::structural::chart_pass(&xml,edit,&rewrite)?.unwrap_or_else(||xml.bytes().to_vec());
                    }
                }
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


fn plot_area(xml:&XmlPart, reader:&mut Reader<'_>, before:&Chart, after:&Chart, patches:&mut Vec<Patch>)->Result<usize,SaveRefusal>{
    let mut i=0;let mut a=0;
    loop {match reader.expect()? {
        (XmlEvent::Start(e),span) if e.name.namespace.as_deref()==Some(C) && e.name.local.ends_with("Chart")=>{
            let old=before.plots.get(i).ok_or_else(||reader.invalid("Unexpected chart plot (dc-971p.46)"))?;
            let new=&after.plots[i]; i+=1;
            if old==new {reader.element_end()?;} else {plot_series(xml,reader,span,old,new,patches)?;}
        },
        (XmlEvent::Start(e),span) if ["valAx","catAx","dateAx","serAx"].iter().any(|n|e.name.is(C,n))=>{
            let old=before.axes.get(a).ok_or_else(||reader.invalid("Unexpected chart axis (dc-971p.46)"))?;let new=&after.axes[a];a+=1;
            if old==new{reader.element_end()?;}else{axis_settings(xml,reader,span,old,new,patches)?;}
        },
        (XmlEvent::Start(_),_)=>{reader.element_end()?;},
        (XmlEvent::End(_),(_,end))=>{
            if i!=before.plots.len() || a!=before.axes.len(){return Err(reader.invalid("Chart plot or axis count changed (dc-971p.46)"));}return Ok(end);
        },_=>{}
    }}
}
fn plot_series(xml:&XmlPart,reader:&mut Reader<'_>,root:(usize,usize),before:&ChartPlot,after:&ChartPlot,patches:&mut Vec<Patch>)->Result<(),SaveRefusal>{
    let changed=plot_type_changed(before,after);let converted=before.kind!=after.kind;
    if converted{patches.push((root,renamed_tag(xml,root,&after.kind)));}
    if changed {
        let mut header=Vec::new();
        if let Some(direction)=after.bar_direction.as_deref(){header.extend(fresh_number("barDir",direction));}
        if let Some(group)=after.grouping.as_deref(){header.extend(fresh_number("grouping",group));}
        if !header.is_empty(){patches.push(((root.1,root.1),header));}
    }
    let set_overlap=after.kind=="barChart" && (before.kind!=after.kind || before.grouping!=after.grouping);
    let mut i=0;let mut insert=None;let mut overlap=false;let mut late=None;let mut extension=None;
    let close=loop {match reader.expect()? {
        (XmlEvent::Start(e),span) if e.name.is(C,"ser")=>{
            let old=before.series.get(i).ok_or_else(||reader.invalid("Unexpected chart series (dc-971p.46)"))?;i+=1;
            let end=if let Some((order,new))=after.series.iter().enumerate().find(|(_,s)|s.index==old.index) {
                if old==new && order==i-1 && !converted {reader.element_end()?}else{series_source(xml,reader,span,old,new,&before.kind,&after.kind,(before.series.iter().map(|s|s.index).ne(after.series.iter().map(|s|s.index))).then_some(order),patches)?}
            }else{let end=reader.element_end()?;patches.push(((span.0,end),Vec::new()));end};
            insert=Some(end);
        },
        (XmlEvent::Start(e),span)=>{
            let end=reader.element_end()?;
            if ["serLines","axId","extLst"].iter().any(|n|e.name.is(C,n)){late.get_or_insert(span.0);}
            if e.name.is(C,"extLst"){extension.get_or_insert(span.0);}
            if changed && ["barDir","grouping"].iter().any(|n|e.name.is(C,n)){patches.push(((span.0,end),Vec::new()));}
            else if e.name.is(C,"overlap") && set_overlap {overlap=true;numeric_field(xml,span,end,"overlap",Some(if after.grouping.as_deref()==Some("stacked"){"100"}else{"0"}),patches);}
            else if converted && e.name.namespace.as_deref()==Some(C) {
                if !plot_child(&before.kind,&e.name.local){return Err(reader.invalid("This plot has unsupported type-specific records (dc-971p.46)"));}
                if !plot_child(&after.kind,&e.name.local){patches.push(((span.0,end),Vec::new()));}
            }
        },
        (XmlEvent::End(_),span)=>{if converted{patches.push((span,renamed_tag(xml,span,&after.kind)));}break span.0;},_=>{}
    }};
    if i!=before.series.len(){return Err(reader.invalid("Chart series count changed (dc-971p.46)"));}
    let mut added=Vec::new();
    for (order,s) in after.series.iter().enumerate().filter(|(_,s)|!before.series.iter().any(|o|o.index==s.index)) {
        added.extend_from_slice(format!("<c:ser xmlns:c=\"{C}\"><c:idx val=\"{}\"/><c:order val=\"{order}\"/>",s.index).as_bytes());
        for (slot,data) in [("tx",s.name.as_ref()),(if after.kind=="scatterChart"{"xVal"}else{"cat"},s.categories.as_ref()),(if after.kind=="scatterChart"{"yVal"}else{"val"},s.values.as_ref())] {added.extend_from_slice(&data_xml(slot,data));}
        added.extend_from_slice(b"</c:ser>");
    }
    if !added.is_empty(){let at=insert.ok_or_else(||reader.invalid("This plot has no series insertion point (dc-971p.46)"))?;patches.push(((at,at),added));}
    if set_overlap && !overlap{let at=late.unwrap_or(close);patches.push(((at,at),fresh_number("overlap",if after.grouping.as_deref()==Some("stacked"){"100"}else{"0"})));}
    if before.kind=="pieChart" && after.kind=="doughnutChart"{let at=extension.unwrap_or(close);patches.push(((at,at),fresh_number("holeSize","50")));}
    Ok(())
}
fn data_xml(slot:&str,data:Option<&ChartData>)->Vec<u8>{
    let Some(data)=data else{return Vec::new();};
    let body=if let Some(formula)=&data.formula {
        let kind=if slot=="tx" || !data.numeric {"strRef"}else{"numRef"};
        format!("<c:{kind}><c:f>{}</c:f></c:{kind}>",plain_text(formula))
    }else if slot=="tx" {
        format!("<c:v xml:space=\"preserve\">{}</c:v>",plain_text(data.cache.first().and_then(|s|s.as_deref()).unwrap_or_default()))
    }else{
        let kind=if data.numeric{"numLit"}else{"strLit"};
        let points=data.cache.iter().enumerate().filter_map(|(i,p)|p.as_ref().map(|p|format!("<c:pt idx=\"{i}\"><c:v>{}</c:v></c:pt>",plain_text(p)))).collect::<String>();
        format!("<c:{kind}><c:ptCount val=\"{}\"/>{points}</c:{kind}>",data.cache.len())
    };
    format!("<c:{slot} xmlns:c=\"{C}\">{body}</c:{slot}>").into_bytes()
}
fn series_source(xml:&XmlPart,reader:&mut Reader<'_>,span:(usize,usize),before:&ChartSeries,after:&ChartSeries,kind:&str,next_kind:&str,order:Option<usize>,patches:&mut Vec<Patch>)->Result<usize,SaveRefusal>{
    let mut children=Vec::new();
    let (close,end)=loop {match reader.expect()? {
        (XmlEvent::Start(e),span)=>{let end=reader.element_end()?;children.push((e.name,(span.0,end)));},
        (XmlEvent::End(_),(close,end))=>break (close,end),_=>{}
    }};
    if let Some(order)=order {
        let order_span=children.iter().find(|(n,_)|n.is(C,"order")).ok_or_else(||reader.invalid("The chart series has no order (dc-971p.46)"))?.1;
        patches.push((order_span,format!("<c:order xmlns:c=\"{C}\" val=\"{order}\"/>").into_bytes()));
    }
    let rank=|name:&str|match name {"idx"=>0,"order"=>1,"tx"=>2,"spPr"=>3,"cat"|"xVal"=>20,"val"|"yVal"=>21,"smooth"|"shape"|"extLst"=>30,_=>10};
    for (slot,old,new) in [("tx",&before.name,&after.name),(if kind=="scatterChart"{"xVal"}else{"cat"},&before.categories,&after.categories),(if kind=="scatterChart"{"yVal"}else{"val"},&before.values,&after.values)] {
        if old==new {continue;}
        let found=children.iter().filter(|(n,_)|n.is(C,slot)).collect::<Vec<_>>();
        if found.len()>1 {return Err(reader.invalid("Duplicate chart series source fields (dc-971p.46)"));}
        let bytes=data_xml(slot,new.as_ref());
        if let Some((_,span))=found.first(){patches.push((*span,bytes));}
        else if !bytes.is_empty(){
            if RawTag::parse(&xml.bytes()[span.0..span.1]).is_empty(){return Err(reader.invalid("Empty chart series cannot be edited (dc-971p.46)"));}
            let at=children.iter().find(|(n,_)|n.namespace.as_deref()==Some(C) && rank(&n.local)>rank(slot)).map_or(close,|(_,s)|s.0);
            patches.push(((at,at),bytes));
        }
    }
    if kind!=next_kind {for (name,span) in &children {if name.namespace.as_deref()==Some(C){
        if !series_child(kind,&name.local){return Err(reader.invalid("This series has unsupported type-specific records (dc-971p.46)"));}
        if !series_child(next_kind,&name.local){patches.push((*span,Vec::new()));}
    }}}
    Ok(end)
}


/// Source edits preserve retained collection order. Membership edits currently require
/// positional ordering, so an imported reordered collection cannot acquire duplicate orders.
fn validate_series_orders(xml:&XmlPart,before:&Chart,after:&Chart)->Result<(),SaveRefusal>{
    let mut reader=Reader::new(xml);let mut path=Vec::new();let mut orders=Vec::<Vec<Vec<u32>>>::new();
    while let Some((event,_))=reader.next()? {match event {
        XmlEvent::Start(e)=>{
            if e.name.namespace.as_deref()==Some(C) {
                if path.last().is_some_and(|n:&crate::xml::XmlName|n.is(C,"plotArea")) && e.name.local.ends_with("Chart") {orders.push(Vec::new());}
                if e.name.is(C,"ser") && path.last().is_some_and(|n|n.namespace.as_deref()==Some(C) && n.local.ends_with("Chart")) {
                    if let Some(plot)=orders.last_mut(){plot.push(Vec::new());}
                }
                if e.name.is(C,"order") && path.last().is_some_and(|n|n.is(C,"ser")) {
                    if let Some(series)=orders.last_mut().and_then(|p|p.last_mut()) {
                        series.push(e.attribute("val").and_then(|v|v.parse::<u32>().ok()).unwrap_or(u32::MAX));
                    }
                }
            }
            path.push(e.name);
        },XmlEvent::End(_)=>{path.pop();},_=>{}
    }}
    for (p,(old,new)) in before.plots.iter().zip(&after.plots).enumerate() {
        if old.series.iter().map(|s|s.index).eq(new.series.iter().map(|s|s.index)){continue;}
        if !orders.get(p).is_some_and(|o|o.len()==old.series.len() && o.iter().enumerate().all(|(i,v)|v.as_slice()==[i as u32])) {
            return Err(reader.invalid("Adding or removing series with a retained custom order is not modelled (dc-971p.46)"));
        }
    }
    Ok(())
}


fn axis_settings(xml:&XmlPart,reader:&mut Reader<'_>,_span:(usize,usize),before:&calc_doc_model::ChartAxis,after:&calc_doc_model::ChartAxis,patches:&mut Vec<Patch>)->Result<(),SaveRefusal>{
    let mut scale=false;let mut unit=false;let mut scale_at=None;let mut unit_at=None;
    let close=loop{match reader.expect()?{
        (XmlEvent::Start(e),_) if e.name.is(C,"axId")=>{scale_at=Some(reader.element_end()?);},
        (XmlEvent::Start(e),child) if e.name.is(C,"scaling")=>{
            if scale{return Err(reader.invalid("Duplicate axis scaling records (dc-971p.46)"));}scale=true;
            if before.minimum!=after.minimum || before.maximum!=after.maximum || before.reversed!=after.reversed {axis_scaling(xml,reader,child,before,after,patches)?;}else{reader.element_end()?;}
        },
        (XmlEvent::Start(e),child) if e.name.is(C,"axPos")=>{let end=reader.element_end()?;if before.position!=after.position{numeric_field(xml,child,end,"axPos",after.position.as_deref(),patches);}},
        (XmlEvent::Start(e),child) if e.name.is(C,"majorUnit")=>{
            if unit{return Err(reader.invalid("Duplicate major units (dc-971p.46)"));}unit=true;
            let end=reader.element_end()?;
            if before.major_unit!=after.major_unit {numeric_field(xml,child,end,"majorUnit",after.major_unit.as_deref(),patches);}
        },
        (XmlEvent::Start(e),child)=>{
            if ["minorUnit","dispUnits","extLst"].iter().any(|n|e.name.is(C,n)){unit_at.get_or_insert(child.0);}
            reader.element_end()?;
        },(XmlEvent::End(_),(close,_))=>break close,_=>{}
    }};
    if !scale && (before.minimum!=after.minimum || before.maximum!=after.maximum || before.reversed!=after.reversed) {
        let at=scale_at.ok_or_else(||reader.invalid("The axis has no identity before scaling (dc-971p.46)"))?;
        patches.push(((at,at),fresh_scaling(after)));
    }
    if !unit && before.major_unit!=after.major_unit {
        if let Some(value)=after.major_unit.as_deref(){let at=unit_at.unwrap_or(close);patches.push(((at,at),fresh_number("majorUnit",value)));}
    }
    Ok(())
}
fn fresh_number(name:&str,value:&str)->Vec<u8>{format!("<c:{name} xmlns:c=\"{C}\" val=\"{value}\"/>").into_bytes()}
fn numeric_field(xml:&XmlPart,span:(usize,usize),end:usize,_name:&str,value:Option<&str>,patches:&mut Vec<Patch>){
    if let Some(value)=value{let tag=RawTag::parse(&xml.bytes()[span.0..span.1]);patches.push((span,tag.rebuilt(&[("val",Some(value))],tag.is_empty())));}else{patches.push(((span.0,end),Vec::new()));}
}
fn fresh_scaling(axis:&calc_doc_model::ChartAxis)->Vec<u8>{
    let mut bytes=format!("<c:scaling xmlns:c=\"{C}\">").into_bytes();
    bytes.extend(fresh_number("orientation",if axis.reversed{"maxMin"}else{"minMax"}));
    if let Some(value)=axis.maximum.as_deref(){bytes.extend(fresh_number("max",value));}
    if let Some(value)=axis.minimum.as_deref(){bytes.extend(fresh_number("min",value));}
    bytes.extend_from_slice(b"</c:scaling>");bytes
}
fn axis_scaling(xml:&XmlPart,reader:&mut Reader<'_>,span:(usize,usize),before:&calc_doc_model::ChartAxis,after:&calc_doc_model::ChartAxis,patches:&mut Vec<Patch>)->Result<(),SaveRefusal>{
    let mut children=Vec::new();let mut opening=Vec::new();
    let (close,end)=loop{match reader.expect()?{
        (XmlEvent::Start(e),child)=>{let end=reader.element_end()?;children.push((e.name,(child.0,end)));opening.push(child);},
        (XmlEvent::End(_),(close,end))=>break(close,end),_=>{}
    }};
    if RawTag::parse(&xml.bytes()[span.0..span.1]).is_empty(){
        let tag=RawTag::parse(&xml.bytes()[span.0..span.1]);let mut bytes=tag.rebuilt(&[],false);
        bytes.extend(fresh_number("orientation",if after.reversed{"maxMin"}else{"minMax"}));
        if let Some(value)=after.maximum.as_deref(){bytes.extend(fresh_number("max",value));}
        if let Some(value)=after.minimum.as_deref(){bytes.extend(fresh_number("min",value));}
        bytes.extend_from_slice(b"</");bytes.extend_from_slice(tag.qualified_name());bytes.push(b'>');patches.push(((span.0,end),bytes));return Ok(());
    }
    let rank=|name:&str|match name{"logBase"=>0,"orientation"=>1,"max"=>2,"min"=>3,_=>4};
    for (name,changed,value) in [
        ("orientation",before.reversed!=after.reversed,Some(if after.reversed{"maxMin"}else{"minMax"})),
        ("max",before.maximum!=after.maximum,after.maximum.as_deref()),
        ("min",before.minimum!=after.minimum,after.minimum.as_deref()),
    ]{
        if !changed{continue;}
        let matches=children.iter().enumerate().filter(|(_, (n,_))|n.is(C,name)).collect::<Vec<_>>();
        if matches.len()>1{return Err(reader.invalid("Duplicate axis scaling fields (dc-971p.46)"));}
        if let Some((i,(_,s)))=matches.first(){numeric_field(xml,opening[*i],s.1,name,value,patches);}
        else if let Some(value)=value{let at=children.iter().find(|(n,_)|n.namespace.as_deref()==Some(C) && rank(&n.local)>rank(name)).map_or(close,|(_,s)|s.0);patches.push(((at,at),fresh_number(name,value)));}
    }
    Ok(())
}

fn category_kind(kind:&str)->bool{matches!(kind,"barChart"|"lineChart"|"areaChart")}
fn pie_kind(kind:&str)->bool{matches!(kind,"pieChart"|"doughnutChart")}
fn plot_type_changed(old:&ChartPlot,new:&ChartPlot)->bool{old.kind!=new.kind || old.bar_direction!=new.bar_direction || old.grouping!=new.grouping}
fn valid_plot_type(old:&ChartPlot,new:&ChartPlot)->bool {
    if !plot_type_changed(old,new){return true;}
    if category_kind(&old.kind)&&category_kind(&new.kind) {
        let direction=if new.kind=="barChart"{matches!(new.bar_direction.as_deref(),Some("col"|"bar"))}else{new.bar_direction.is_none()};
        let grouping=new.grouping.as_deref()==Some("stacked") || new.grouping.as_deref()==Some(if new.kind=="barChart"{"clustered"}else{"standard"});
        direction&&grouping
    }else{pie_kind(&old.kind)&&pie_kind(&new.kind)&&new.bar_direction.is_none()&&new.grouping.is_none()}
}
/// Rename only the XML QName; retain source attributes and namespace declarations byte for byte.
fn renamed_tag(xml:&XmlPart,span:(usize,usize),local:&str)->Vec<u8>{
    let bytes=&xml.bytes()[span.0..span.1];let start=if bytes.get(1)==Some(&b'/'){2}else{1};
    let end=(start..bytes.len()).find(|i|matches!(bytes[*i],b' '|b'\t'|b'\r'|b'\n'|b'/'|b'>')).unwrap_or(bytes.len());
    let prefix=bytes[start..end].iter().rposition(|b|*b==b':').map_or(start,|i|start+i+1);
    let mut out=bytes[..prefix].to_vec();out.extend_from_slice(local.as_bytes());out.extend_from_slice(&bytes[end..]);out
}
fn plot_child(kind:&str,local:&str)->bool{
    matches!(local,"varyColors"|"ser"|"dLbls"|"axId"|"extLst") || match kind {
        "barChart"=>matches!(local,"barDir"|"grouping"|"gapWidth"|"overlap"|"serLines"),
        "lineChart"=>matches!(local,"grouping"|"dropLines"|"hiLowLines"|"upDownBars"|"marker"|"smooth"),
        "areaChart"=>matches!(local,"grouping"|"dropLines"),
        "pieChart"=>local=="firstSliceAng",
        "doughnutChart"=>matches!(local,"firstSliceAng"|"holeSize"),_=>false,
    }
}
fn series_child(kind:&str,local:&str)->bool{
    matches!(local,"idx"|"order"|"tx"|"spPr"|"dPt"|"dLbls"|"cat"|"val"|"extLst") || match kind {
        "barChart"=>matches!(local,"invertIfNegative"|"pictureOptions"|"trendline"|"errBars"|"shape"),
        "lineChart"=>matches!(local,"marker"|"trendline"|"errBars"|"smooth"),
        "areaChart"=>matches!(local,"pictureOptions"|"trendline"|"errBars"),
        "pieChart"|"doughnutChart"=>local=="explosion",_=>false,
    }
}
