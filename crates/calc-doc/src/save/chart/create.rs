//! New chart parts and frames, composed with the existing retained-package writer.
use super::super::parts;
use super::*;
use crate::load::{LoadedSheet, MAIN_NAMESPACE};
use crate::opc::TargetMode;
use calc_doc_model::{ChartAnchor, ChartAxis, DrawingPoint};
use std::borrow::Cow;
use std::collections::BTreeSet;
const XDR: &str = "http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const DRAWING: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing";
const CHART: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart";

fn invalid(part: &str, detail: &str) -> SaveRefusal {
    SaveRefusal::ChartNotModelled {
        part: part.into(),
        detail: detail.into(),
        owner: "dc-971p.49",
    }
}

/// New parts have no opaque source records. Validate everything the fresh serializer owns.
pub(crate) fn render(chart: &Chart) -> Result<Vec<u8>, SaveRefusal> {
    let fail = |s| invalid(&chart.part, s);
    font::validate(&chart.text_style).map_err(fail)?;
    let valid_text = |s: &str| {
        s.encode_utf16().count() <= 32767
            && !s.chars().any(|c| {
                (c < ' ' && !matches!(c, '\t' | '\n' | '\r'))
                    || matches!(c, '\u{fffe}' | '\u{ffff}')
            })
    };
    if !valid_text(&chart.name)
        || chart.title.as_deref().is_some_and(|s| !valid_text(s))
        || chart.title_formula.is_some()
        || chart.auto_title_deleted != chart.title.is_none()
    {
        return Err(fail("Use a literal chart title and a valid frame name"));
    }
    if chart
        .legend
        .as_deref()
        .is_some_and(|p| !matches!(p, "r" | "l" | "t" | "b" | "tr"))
    {
        return Err(fail("Choose a supported legend position"));
    }
    let combo = category_combination(&chart.plots);
    if chart.plots.len() != 1 && !combo {
        return Err(fail(
            "Use one supported plot or up to three distinct vertical category plots (dc-971p.51)",
        ));
    }
    if chart.plots.iter().map(|p| p.series.len()).sum::<usize>() > 255 {
        return Err(fail("Use at most 255 series"));
    }
    let mut indices = BTreeSet::new();
    for p in chart.plots.iter() {
        let pie = pie_kind(&p.kind);
        let scatter = p.kind == "scatterChart";
        if !category_kind(&p.kind) && !pie && !scatter {
            return Err(fail("Choose a supported two-dimensional chart type"));
        }
        if (p.kind == "barChart" && !matches!(p.bar_direction.as_deref(), Some("col" | "bar")))
            || (p.kind != "barChart" && p.bar_direction.is_some())
        {
            return Err(fail("Invalid bar direction"));
        }
        if category_kind(&p.kind)
            && p.grouping.as_deref() != Some("stacked")
            && p.grouping.as_deref() != Some("percentStacked")
            && p.grouping.as_deref()
                != Some(if p.kind == "barChart" {
                    "clustered"
                } else {
                    "standard"
                })
        {
            return Err(fail("Choose Normal, Stacked or 100% stacked grouping"));
        }
        if (pie || scatter) && p.grouping.is_some() {
            return Err(fail("This plot has no grouping"));
        }
        if p.series.is_empty() || p.series.len() > 255 || (pie && p.series.len() != 1) {
            return Err(fail(
                "Use 1 to 255 series, or one series for a pie or doughnut",
            ));
        }
        for s in &p.series {
            if !indices.insert(s.index)
                || s.fill
                    .as_deref()
                    .is_some_and(|s| s.len() != 6 || !s.bytes().all(|b| b.is_ascii_hexdigit()))
            {
                return Err(fail("Use distinct series indices and RGB colours"));
            }
            if s.values.is_none() {
                return Err(fail("Each series needs a values range"));
            }
            for d in [&s.name, &s.categories, &s.values].into_iter().flatten() {
                if d.formula
                    .as_deref()
                    .is_some_and(|f| f.is_empty() || !valid_text(f))
                    || d.cache.len() > 32767
                    || d.cache.iter().flatten().any(|s| !valid_text(s))
                {
                    return Err(fail("The series data exceeds chart text or point limits"));
                }
            }
        }
        if pie {
            if !chart.axes.is_empty() || !p.axis_ids.is_empty() {
                return Err(fail("Pie and doughnut plots have no axes"));
            }
        } else {
            if chart.axes.len() != (if combo { 4 } else { 2 })
                || p.axis_ids.len() != 2
                || p.axis_ids[0] == p.axis_ids[1]
            {
                return Err(fail("Use one reciprocal axis pair"));
            }
            let a = chart
                .axes
                .iter()
                .find(|a| a.id == p.axis_ids[0])
                .ok_or_else(|| fail("Missing chart axis"))?;
            let b = chart
                .axes
                .iter()
                .find(|a| a.id == p.axis_ids[1])
                .ok_or_else(|| fail("Missing chart axis"))?;
            if a.cross_axis != Some(b.id)
                || b.cross_axis != Some(a.id)
                || a.kind != if scatter { "valAx" } else { "catAx" }
                || b.kind != "valAx"
            {
                return Err(fail(
                    "Use one reciprocal category/value or scatter axis pair",
                ));
            }
            let horizontal = p.kind == "barChart" && p.bar_direction.as_deref() == Some("bar");
            // The secondary pair of a combination is the one whose value axis sits on the right;
            // Excel keeps its category axis, deleted, at the bottom (UI28 combos-created.xlsx).
            let secondary = combo && b.position.as_deref() == Some("r");
            if a.position.as_deref() != Some(if horizontal { "l" } else { "b" })
                || b.position.as_deref()
                    != Some(if secondary {
                        "r"
                    } else if horizontal {
                        "b"
                    } else {
                        "l"
                    })
            {
                return Err(fail("Invalid chart axis positions"));
            }
            for axis in &chart.axes {
                valid_log_axis(axis).map_err(fail)?;
                for n in [&axis.minimum, &axis.maximum, &axis.major_unit]
                    .into_iter()
                    .flatten()
                {
                    if n.parse::<f64>().ok().is_none_or(|n| !n.is_finite()) {
                        return Err(fail("Use finite axis settings"));
                    }
                }
                if axis
                    .major_unit
                    .as_deref()
                    .and_then(|n| n.parse::<f64>().ok())
                    .is_some_and(|n| n <= 0.)
                {
                    return Err(fail("Use a positive major unit"));
                }
                if let (Some(min), Some(max)) = (&axis.minimum, &axis.maximum) {
                    if min.parse::<f64>().unwrap() >= max.parse::<f64>().unwrap() {
                        return Err(fail("The axis maximum must exceed its minimum"));
                    }
                }
            }
        }
    }
    if chart
        .axes
        .iter()
        .map(|a| a.id)
        .collect::<BTreeSet<_>>()
        .len()
        != chart.axes.len()
    {
        return Err(fail("Use distinct axis identities"));
    }
    let mut bytes = format!("<c:chartSpace xmlns:c=\"{C}\" xmlns:a=\"{A}\"><c:chart>").into_bytes();
    if let Some(title) = &chart.title {
        bytes.extend(b"<c:title>");
        bytes.extend(tx(title));
        bytes.extend(b"<c:overlay val=\"0\"/></c:title>");
    }
    bytes.extend(fresh_number(
        "autoTitleDeleted",
        if chart.auto_title_deleted { "1" } else { "0" },
    ));
    bytes.extend(b"<c:plotArea><c:layout/>");
    for p in &chart.plots {
        let scatter = p.kind == "scatterChart";
        bytes.extend(format!("<c:{}>", p.kind).as_bytes());
        if let Some(dir) = &p.bar_direction {
            bytes.extend(fresh_number("barDir", dir));
        }
        if let Some(grouping) = &p.grouping {
            bytes.extend(fresh_number("grouping", grouping));
        }
        if scatter {
            bytes.extend(fresh_number("scatterStyle", "marker"));
        }
        bytes.extend(fresh_number(
            "varyColors",
            if p.vary_colors { "1" } else { "0" },
        ));
        for s in &p.series {
            bytes.extend(b"<c:ser>");
            bytes.extend(fresh_number("idx", &s.index.to_string()));
            bytes.extend(fresh_number("order", &s.index.to_string()));
            bytes.extend(data_xml("tx", s.name.as_ref()));
            bytes.extend(colour::fresh(s.fill.as_deref()));
            for (slot, data) in [
                (if scatter { "xVal" } else { "cat" }, s.categories.as_ref()),
                (if scatter { "yVal" } else { "val" }, s.values.as_ref()),
            ] {
                bytes.extend(data_xml(slot, data));
            }
            bytes.extend(b"</c:ser>");
        }
        if p.kind == "barChart" {
            bytes.extend(fresh_number("gapWidth", "150"));
            // Excel writes overlap only for a stacked or 100% stacked bar plot (UI28, UI30 P10).
            if matches!(p.grouping.as_deref(), Some("stacked" | "percentStacked")) {
                bytes.extend(fresh_number("overlap", "100"));
            }
        }
        if p.kind == "doughnutChart" {
            bytes.extend(fresh_number("holeSize", "50"));
        }
        for id in &p.axis_ids {
            bytes.extend(fresh_number("axId", &id.to_string()));
        }
        bytes.extend(format!("</c:{}>", p.kind).as_bytes());
    }
    // Excel writes the primary pair, then the secondary value axis, then the secondary category
    // axis, which is deleted and sits at the bottom (UI28 combos-created.xlsx).
    let secondary_values: BTreeSet<u32> = chart
        .axes
        .iter()
        .filter(|a| a.kind == "valAx" && matches!(a.position.as_deref(), Some("t" | "r")))
        .map(|a| a.id)
        .collect();
    let is_secondary = |a: &ChartAxis| {
        combo
            && (secondary_values.contains(&a.id)
                || a.cross_axis.is_some_and(|c| secondary_values.contains(&c)))
    };
    let secondary_axis = |a: &&ChartAxis| is_secondary(a);
    let mut axes: Vec<&ChartAxis> = chart.axes.iter().collect();
    axes.sort_by_key(|a| {
        if !secondary_axis(a) {
            0
        } else if a.kind == "valAx" {
            1
        } else {
            2
        }
    });
    for a in axes {
        bytes.extend(format!("<c:{}>", a.kind).as_bytes());
        bytes.extend(fresh_number("axId", &a.id.to_string()));
        bytes.extend(fresh_scaling(a));
        let secondary = is_secondary(a);
        if secondary && a.kind == "catAx" {
            bytes.extend(fresh_number("delete", "1"));
        }
        bytes.extend(fresh_number(
            "axPos",
            if secondary && a.kind == "catAx" {
                "b"
            } else {
                a.position.as_deref().unwrap()
            },
        ));
        if a.kind == "valAx" && !secondary {
            bytes.extend(b"<c:majorGridlines/>");
        }
        // A value axis of a 100% stacked plot is linked to the percent format Excel gives it
        // (UI30 P10: `0%`, sourceLinked 1, on the secondary value axis too).
        let percent_axis = a.kind == "valAx"
            && chart.plots.iter().any(|p| {
                p.grouping.as_deref() == Some("percentStacked") && p.axis_ids.contains(&a.id)
            });
        bytes.extend(
            format!(
                "<c:numFmt formatCode=\"{}\" sourceLinked=\"1\"/>",
                if percent_axis { "0%" } else { "General" }
            )
            .as_bytes(),
        );
        bytes.extend(b"<c:majorTickMark val=\"none\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/>");
        bytes.extend(fresh_number("crossAx", &a.cross_axis.unwrap().to_string()));
        if !(secondary && a.kind == "catAx") {
            bytes.extend(fresh_number(
                "crosses",
                if secondary { "max" } else { "autoZero" },
            ));
        }
        if a.kind == "catAx" {
            bytes.extend(b"<c:auto val=\"1\"/><c:lblAlgn val=\"ctr\"/><c:lblOffset val=\"100\"/>");
        } else {
            bytes.extend(fresh_number(
                "crossBetween",
                a.cross_between.as_deref().unwrap_or("between"),
            ));
            if let Some(unit) = &a.major_unit {
                bytes.extend(fresh_number("majorUnit", unit));
            }
        }
        bytes.extend(format!("</c:{}>", a.kind).as_bytes());
    }
    bytes.extend(b"</c:plotArea>");
    if let Some(pos) = &chart.legend {
        bytes.extend(
            format!("<c:legend><c:legendPos val=\"{pos}\"/><c:overlay val=\"0\"/></c:legend>")
                .as_bytes(),
        );
    }
    bytes.extend(b"<c:plotVisOnly val=\"1\"/><c:dispBlanksAs val=\"gap\"/></c:chart>");
    if chart.text_style != Default::default() {
        bytes.extend(font::fresh(&chart.text_style));
    }
    bytes.extend(b"</c:chartSpace>");
    font::apply_elements(
        &crate::xml::XmlPart::new(chart.part.clone(), bytes, crate::xml::XmlLimits::default()),
        &Default::default(),
        &chart.element_text,
    )
}

fn frame(chart: &Chart, id: u32, rel: &str) -> Result<Vec<u8>, SaveRefusal> {
    let ChartAnchor::OneCell {
        from,
        width,
        height,
    } = chart.anchor
    else {
        return Err(invalid(
            &chart.part,
            "New chart frames use a cell anchor and fixed size",
        ));
    };
    if from.column >= 16384
        || from.row >= 1048576
        || from.column_offset < 0
        || from.row_offset < 0
        || width <= 0
        || height <= 0
        || width > 100_000_000
        || height > 100_000_000
    {
        return Err(invalid(
            &chart.part,
            "The chart frame must fit the worksheet and a bounded positive size",
        ));
    }
    let DrawingPoint {
        column,
        column_offset,
        row,
        row_offset,
    } = from;
    let mut bytes=format!("<xdr:oneCellAnchor xmlns:xdr=\"{XDR}\" xmlns:a=\"{A}\" xmlns:r=\"{REL}\"><xdr:from><xdr:col>{column}</xdr:col><xdr:colOff>{column_offset}</xdr:colOff><xdr:row>{row}</xdr:row><xdr:rowOff>{row_offset}</xdr:rowOff></xdr:from><xdr:ext cx=\"{width}\" cy=\"{height}\"/><xdr:graphicFrame macro=\"\"><xdr:nvGraphicFramePr><xdr:cNvPr id=\"{id}\" name=").into_bytes();
    super::super::text::attribute_value(&chart.name, &mut bytes);
    bytes.extend(format!("/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr><xdr:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{width}\" cy=\"{height}\"/></xdr:xfrm><a:graphic><a:graphicData uri=\"{C}\"><c:chart xmlns:c=\"{C}\" r:id=\"{rel}\"/></a:graphicData></a:graphic></xdr:graphicFrame><xdr:clientData/></xdr:oneCellAnchor>").as_bytes());
    Ok(bytes)
}

pub(super) fn finish(
    package: &Package,
    sheets: &[Cow<'_, LoadedSheet>],
    before: &ModelRoot,
    after: &ModelRoot,
    out: &mut Changes,
    structural: Option<&super::super::structural::StructuralEdits>,
) -> Result<(), SaveRefusal> {
    let touched: BTreeSet<_> = after
        .definitions()
        .all_charts()
        .map(|(sheet, _)| sheet)
        .chain(after.definitions().all_shapes().map(|(sheet, _)| sheet))
        .collect();
    for sheet in touched {
        let charts = after.definitions().charts(sheet);
        let original = before.definitions().charts(sheet);
        // Source frames keep their identities and order. Newly authored frames can disappear on Undo.
        if charts.len() < original.len()
            || !charts.iter().zip(original).all(|(a, b)| a.part == b.part)
        {
            return Err(invalid(
                "charts",
                "Removing or reordering imported chart frames is not modelled",
            ));
        }
        let new = &charts[original.len()..];
        let new_shapes = new_shapes(before, after, sheet)?;
        if new.is_empty() && new_shapes.is_empty() {
            continue;
        }
        let source = sheets.iter().find(|s| s.id() == sheet).ok_or_else(|| {
            invalid(
                "charts",
                "Creating a chart or shape on a new unsaved sheet needs Save and reopen",
            )
        })?;
        add(
            package,
            source,
            new,
            new_shapes,
            out,
            structural.map(|edits| (edits, before)),
        )?;
    }
    Ok(())
}

/// The shapes `after` adds to `sheet`. A source object stays as the file stores it: editing,
/// removing or reordering one is dc-zjsg.4's.
pub(crate) fn new_shapes<'a>(
    before: &ModelRoot,
    after: &'a ModelRoot,
    sheet: calc_doc_model::value::SheetId,
) -> Result<&'a [calc_doc_model::Shape], SaveRefusal> {
    let shapes = after.definitions().shapes(sheet);
    let original = before.definitions().shapes(sheet);
    if shapes.len() < original.len() || shapes[..original.len()] != *original {
        return Err(SaveRefusal::ChartNotModelled {
            part: "drawing".into(),
            detail: "Editing, removing or reordering imported drawing objects is not modelled"
                .into(),
            owner: "dc-zjsg.4",
        });
    }
    Ok(&shapes[original.len()..])
}

pub(crate) fn add(
    package: &Package,
    source: &LoadedSheet,
    new: &[Chart],
    new_shapes: &[calc_doc_model::Shape],
    out: &mut Changes,
    structural: Option<(&super::super::structural::StructuralEdits, &ModelRoot)>,
) -> Result<(), SaveRefusal> {
    let sheet_part = source.part().name();
    let drawings = package
        .relationships_of(Some(sheet_part))
        .into_iter()
        .flat_map(|r| r.items())
        .filter(|r| r.kind == DRAWING)
        .collect::<Vec<_>>();
    if drawings.len() > 1
        || drawings
            .first()
            .is_some_and(|r| r.mode != TargetMode::Internal || r.resolved.is_none())
    {
        return Err(invalid(
            sheet_part,
            "The worksheet has an unsupported drawing relationship",
        ));
    }
    let drawing = if let Some(r) = drawings.first() {
        r.resolved.clone().unwrap()
    } else {
        let reserved = out.added.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>();
        let folder = new
            .first()
            .and_then(|chart| chart.part.rsplit_once("/charts/").map(|(f, _)| f))
            .unwrap_or("xl");
        super::super::available_part_name(
            package,
            "",
            &format!("{folder}/drawings/dnaDrawing"),
            "xml",
            &reserved,
        )
    };
    if package
        .parts()
        .iter()
        .filter(|p| {
            package.relationships_of(Some(p.name())).is_some_and(|r| {
                r.items()
                    .iter()
                    .any(|r| r.resolved.as_deref() == Some(drawing.as_str()))
            })
        })
        .count()
        > 1
    {
        return Err(invalid(
            &drawing,
            "A drawing shared by worksheets cannot acquire a chart",
        ));
    }
    let empty = format!("<xdr:wsDr xmlns:xdr=\"{XDR}\"/>");
    let mut xml = parts::current_part(package, out, &drawing, empty.as_bytes())?;
    let mut ids = BTreeSet::new();
    let mut reader = Reader::new(&xml);
    match reader.expect()? {
        (XmlEvent::Start(e), _) if e.name.is(XDR, "wsDr") => {}
        _ => return Err(reader.invalid("Expected a spreadsheet drawing")),
    }
    while let Some((event, _)) = reader.next()? {
        if let XmlEvent::Start(e) = event {
            if e.name.is(XDR, "cNvPr") {
                if let Some(id) = e.attribute("id").and_then(|s| s.parse::<u32>().ok()) {
                    ids.insert(id);
                }
            }
        }
    }
    let mut frames = Vec::new();
    // Authored shapes keep their ids when the drawing leaves them free; one that collides with
    // an id of the drawing is given the next free one, and the connectors glued to it follow.
    let mut remap: std::collections::BTreeMap<u32, u32> = Default::default();
    for shape in new_shapes {
        let id = if ids.contains(&shape.id) {
            (2..=u32::MAX)
                .find(|i| !ids.contains(i) && !new_shapes.iter().any(|s| s.id == *i))
                .ok_or_else(|| invalid(&drawing, "No shape identity is available"))?
        } else {
            shape.id
        };
        ids.insert(id);
        remap.insert(shape.id, id);
    }
    for chart in new {
        let suffix = chart
            .part
            .rsplit_once("/charts/dnaChart")
            .and_then(|(_, s)| s.strip_suffix(".xml"));
        if chart.part.contains("..")
            || suffix.is_none_or(|s| !s.bytes().all(|b| b.is_ascii_digit()))
            || package.part(&chart.part).is_some()
            || out.added.iter().any(|(n, _)| n == &chart.part)
        {
            return Err(invalid(
                &chart.part,
                "A new chart needs a unique DNA chart part",
            ));
        }
        let mut bytes = render(chart)?;
        if let Some((edits, before)) = structural {
            for &edit in edits.edits() {
                let xml = crate::xml::XmlPart::new(
                    chart.part.clone(),
                    bytes,
                    crate::xml::XmlLimits::default(),
                );
                let rewrite = super::super::structural::Rewrite {
                    root: before,
                    rewriter: edits.rewriter.as_ref(),
                    change: calc_doc_model::StructuralChange::Axis(edit),
                };
                bytes = super::super::structural::chart_pass(&xml, edit, &rewrite)?
                    .unwrap_or_else(|| xml.bytes().to_vec());
            }
        }
        parts::put_part(package, out, &chart.part, bytes);
        parts::ensure_content_type(
            package,
            out,
            &chart.part,
            "application/vnd.openxmlformats-officedocument.drawingml.chart+xml",
        )?;
        let rel = parts::ensure_relationship(package, out, &drawing, CHART, &chart.part)?;
        let id = (2..=u32::MAX)
            .find(|i| !ids.contains(i))
            .ok_or_else(|| invalid(&drawing, "No frame identity is available"))?;
        ids.insert(id);
        let body = frame(chart, id, &rel)?;
        let body = finish_body(body, &drawing, source, structural)?;
        frames.extend(body);
    }
    // Authored charts come first, then shapes: the drawing's z-order is its document order.
    for shape in new_shapes {
        let body = super::super::shapes::frame(shape, &remap)?;
        frames.extend(finish_body(body, &drawing, source, structural)?);
    }
    xml = xml.with_bytes(parts::append_root(&xml, &frames)?);
    parts::put_part(package, out, &drawing, xml.bytes().to_vec());
    parts::ensure_content_type(
        package,
        out,
        &drawing,
        "application/vnd.openxmlformats-officedocument.drawing+xml",
    )?;
    let rel = parts::ensure_relationship(package, out, sheet_part, DRAWING, &drawing)?;
    let source_xml = super::super::sheets::read_sheet(package, sheet_part)?;
    let xml = parts::current_part(package, out, sheet_part, source_xml.bytes())?;
    let body = format!("<drawing xmlns=\"{MAIN_NAMESPACE}\" xmlns:r=\"{REL}\" r:id=\"{rel}\"/>");
    let bytes = parts::worksheet_child(
        &xml,
        "drawing",
        body.as_bytes(),
        &[
            "legacyDrawing",
            "legacyDrawingHF",
            "picture",
            "oleObjects",
            "controls",
            "webPublishItems",
            "tableParts",
            "extLst",
        ],
    )?;
    parts::put_part(package, out, sheet_part, bytes);
    Ok(())
}

/// One authored anchor's bytes, moved by the structural edits of its sheet that the save also applies.
fn finish_body(
    mut body: Vec<u8>,
    drawing: &str,
    source: &LoadedSheet,
    structural: Option<(&super::super::structural::StructuralEdits, &ModelRoot)>,
) -> Result<Vec<u8>, SaveRefusal> {
    if let Some((edits, before)) = structural {
        for &edit in edits.edits().iter().filter(|e| e.sheet() == source.id()) {
            let xml = crate::xml::XmlPart::new(
                drawing.to_owned(),
                [
                    format!("<xdr:wsDr xmlns:xdr=\"{XDR}\">").as_bytes(),
                    &body,
                    b"</xdr:wsDr>",
                ]
                .concat(),
                crate::xml::XmlLimits::default(),
            );
            let rewrite = super::super::structural::Rewrite {
                root: before,
                rewriter: edits.rewriter.as_ref(),
                change: calc_doc_model::StructuralChange::Axis(edit),
            };
            if let Some(bytes) = super::super::structural::drawing_pass(&xml, edit, &rewrite)? {
                let xml = xml.with_bytes(bytes);
                let mut r = Reader::new(&xml);
                match r.expect()? {
                    (XmlEvent::Start(e), _) if e.name.is(XDR, "wsDr") => {}
                    _ => return Err(r.invalid("Expected a spreadsheet drawing")),
                }
                let (_, span) = r.expect()?;
                let end = r.element_end()?;
                body = xml.bytes()[span.0..end].to_vec();
            }
        }
    }
    Ok(body)
}
