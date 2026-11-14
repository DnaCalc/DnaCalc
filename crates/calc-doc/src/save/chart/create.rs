//! New chart parts and frames, composed with the existing retained-package writer.
use super::super::parts;
use super::*;
use crate::load::{LoadedSheet, MAIN_NAMESPACE};
use crate::opc::TargetMode;
use calc_doc_model::{ChartAnchor, ChartAxis, DrawingPoint};
use std::borrow::Cow;
use std::collections::BTreeSet;
const XDR: &str = "http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing";
const MC: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";
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
/// `<c:extLst>` with the series' `c16:uniqueId` (`{counter-GUID}`, the GUID fixed by the part).
fn series_unique_id(part: &str, drawing_id: Option<u32>, counter: u32) -> Vec<u8> {
    let mut h: u128 = 0x6c62272e07bb014262b821756295c58d;
    for b in part.bytes().chain(drawing_id.unwrap_or(0).to_le_bytes()).chain(*b"uniqueId") {
        h = (h ^ u128::from(b)).wrapping_mul(0x0000000001000000000000000000013b);
    }
    format!(
        "<c:extLst><c:ext uri=\"{{C3380CC4-5D6E-409C-BE32-E72D297353CC}}\" xmlns:c16=\"http://schemas.microsoft.com/office/drawing/2014/chart\"><c16:uniqueId val=\"{{{:08X}-{:04X}-{:04X}-{:04X}-{:012X}}}\"/></c:ext></c:extLst>",
        counter,
        (h >> 80) as u16,
        // Version 4, variant 10 bits, as Excel's GUIDs carry.
        ((h >> 64) as u16 & 0x0fff) | 0x4000,
        ((h >> 48) as u16 & 0x3fff) | 0x8000,
        (h & 0xffff_ffff_ffff) as u64
    )
    .into_bytes()
}

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
        // A chart with a title has not had its automatic title deleted; one without may leave
        // Excel to title a one-series chart with the series' name.
        || chart.auto_title_deleted && chart.title.is_some()
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
        // A chart a macro adds has no series until `SetSourceData` (dc-piw0.2).
        if p.series.len() > 255 || (pie && p.series.len() > 1) {
            return Err(fail(
                "Use up to 255 series, or one series for a pie or doughnut",
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
    // A chart a macro makes (a two-cell anchor, dc-piw0 Q0/Q10) is the chart Excel's object model
    // builds, with the elements Excel writes for it: the language, corner and style header, the
    // data label defaults, outward tick marks, the data display extension and the print settings.
    let excel = matches!(chart.anchor, ChartAnchor::TwoCell { .. });
    let mut bytes = if excel {
        format!(
            "<c:chartSpace xmlns:c=\"{C}\" xmlns:a=\"{A}\" xmlns:r=\"{REL}\" xmlns:c16r2=\"http://schemas.microsoft.com/office/drawing/2015/06/chart\"><c:date1904 val=\"0\"/><c:lang val=\"en-US\"/><c:roundedCorners val=\"0\"/><mc:AlternateContent xmlns:mc=\"{MC}\"><mc:Choice Requires=\"c14\" xmlns:c14=\"http://schemas.microsoft.com/office/drawing/2007/8/2/chart\"><c14:style val=\"102\"/></mc:Choice><mc:Fallback><c:style val=\"2\"/></mc:Fallback></mc:AlternateContent><c:chart>"
        )
        .into_bytes()
    } else {
        format!("<c:chartSpace xmlns:c=\"{C}\" xmlns:a=\"{A}\"><c:chart>").into_bytes()
    };
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
            // Excel writes `invertIfNegative` (off) on every bar series, after the fill and before
            // the data (Q0, ~/codex-runs/eng1 excel-report chart1.xml).
            if excel && p.kind == "barChart" {
                bytes.extend(fresh_number("invertIfNegative", "0"));
            }
            for (slot, data) in [
                (if scatter { "xVal" } else { "cat" }, s.categories.as_ref()),
                (if scatter { "yVal" } else { "val" }, s.values.as_ref()),
            ] {
                bytes.extend(data_xml(slot, data));
            }
            // Excel gives every series a `c16:uniqueId`: a counter and the chart's GUID. The GUID
            // is fixed by the part name (a save is reproducible); Excel's is random. The counter
            // is the series' idx + 2 in the one recording (Q0: 2, 3), the rule is asked as Q17.
            if excel {
                bytes.extend(series_unique_id(&chart.part, chart.drawing_id, s.index + 2));
            }
            bytes.extend(b"</c:ser>");
        }
        if excel && p.kind == "barChart" {
            bytes.extend(b"<c:dLbls><c:showLegendKey val=\"0\"/><c:showVal val=\"0\"/><c:showCatName val=\"0\"/><c:showSerName val=\"0\"/><c:showPercent val=\"0\"/><c:showBubbleSize val=\"0\"/></c:dLbls>");
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
        } else if excel {
            bytes.extend(fresh_number("delete", "0"));
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
        // Excel links the axis number format to the source only when there is a source (Q10: an
        // empty chart's axes have no `numFmt`; Q0: with series they have `General`).
        if !excel || chart.plots.iter().any(|p| !p.series.is_empty()) {
            bytes.extend(
                format!(
                    "<c:numFmt formatCode=\"{}\" sourceLinked=\"1\"/>",
                    if percent_axis { "0%" } else { "General" }
                )
                .as_bytes(),
            );
        }
        bytes.extend(
            format!(
                "<c:majorTickMark val=\"{}\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/>",
                if excel { "out" } else { "none" }
            )
            .as_bytes(),
        );
        bytes.extend(fresh_number("crossAx", &a.cross_axis.unwrap().to_string()));
        if !(secondary && a.kind == "catAx") {
            bytes.extend(fresh_number(
                "crosses",
                if secondary { "max" } else { "autoZero" },
            ));
        }
        if a.kind == "catAx" {
            bytes.extend(b"<c:auto val=\"1\"/><c:lblAlgn val=\"ctr\"/><c:lblOffset val=\"100\"/>");
            if excel {
                bytes.extend(b"<c:noMultiLvlLbl val=\"0\"/>");
            }
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
    bytes.extend(b"<c:plotVisOnly val=\"1\"/><c:dispBlanksAs val=\"gap\"/>");
    if excel {
        bytes.extend(b"<c:showDLblsOverMax val=\"0\"/><c:extLst><c:ext uri=\"{56B9EC1D-385E-4148-901F-78D8002777C0}\" xmlns:c16r3=\"http://schemas.microsoft.com/office/drawing/2017/03/chart\"><c16r3:dataDisplayOptions16><c16r3:dispNaAsBlank val=\"1\"/></c16r3:dataDisplayOptions16></c:ext></c:extLst>");
    }
    bytes.extend(b"</c:chart>");
    if chart.text_style != Default::default() {
        bytes.extend(font::fresh(&chart.text_style));
    }
    if excel {
        bytes.extend(b"<c:printSettings><c:headerFooter/><c:pageMargins b=\"0.75\" l=\"0.7\" r=\"0.7\" t=\"0.75\" header=\"0.3\" footer=\"0.3\"/><c:pageSetup/></c:printSettings>");
    }
    bytes.extend(b"</c:chartSpace>");
    font::apply_elements(
        &crate::xml::XmlPart::new(chart.part.clone(), bytes, crate::xml::XmlLimits::default()),
        &Default::default(),
        &chart.element_text,
    )
}

fn frame(chart: &Chart, id: u32, rel: &str) -> Result<Vec<u8>, SaveRefusal> {
    let point = |p: &DrawingPoint| {
        format!(
            "<xdr:col>{}</xdr:col><xdr:colOff>{}</xdr:colOff><xdr:row>{}</xdr:row><xdr:rowOff>{}</xdr:rowOff>",
            p.column, p.column_offset, p.row, p.row_offset
        )
    };
    let valid = |p: &DrawingPoint| {
        p.column < 16384 && p.row < 1048576 && p.column_offset >= 0 && p.row_offset >= 0
    };
    // A chart a macro makes is a two-cell anchor with no `editAs` and a zero `xfrm`, as Excel
    // writes it (dc-piw0 Q0); a chart the interface makes is a one-cell anchor of fixed size.
    let (tag, anchor, ext) = match chart.anchor {
        ChartAnchor::OneCell {
            from,
            width,
            height,
        } if valid(&from)
            && (1..=100_000_000).contains(&width)
            && (1..=100_000_000).contains(&height) =>
        {
            (
                "oneCellAnchor",
                format!(
                    "<xdr:from>{}</xdr:from><xdr:ext cx=\"{width}\" cy=\"{height}\"/>",
                    point(&from)
                ),
                (width, height),
            )
        }
        ChartAnchor::TwoCell { from, to } if valid(&from) && valid(&to) => (
            "twoCellAnchor",
            format!(
                "<xdr:from>{}</xdr:from><xdr:to>{}</xdr:to>",
                point(&from),
                point(&to)
            ),
            (0, 0),
        ),
        _ => {
            return Err(invalid(
                &chart.part,
                "The chart frame must be a cell anchor that fits the worksheet and has a bounded size",
            ));
        }
    };
    let (width, height) = ext;
    let mut bytes=format!("<xdr:{tag} xmlns:xdr=\"{XDR}\" xmlns:a=\"{A}\" xmlns:r=\"{REL}\">{anchor}<xdr:graphicFrame macro=\"\"><xdr:nvGraphicFramePr><xdr:cNvPr id=\"{id}\" name=").into_bytes();
    super::super::text::attribute_value(&chart.name, &mut bytes);
    // Excel gives a frame it creates a `creationId` (a GUID of its own choosing); the engine's is
    // fixed by the part name and identity, so a save is reproducible.
    let creation = if tag == "twoCellAnchor" {
        let mut h: u128 = 0x6c62272e07bb014262b821756295c58d;
        for b in chart.part.bytes().chain(id.to_le_bytes()) {
            h = (h ^ u128::from(b)).wrapping_mul(0x0000000001000000000000000000013b);
        }
        format!(
            "<a:extLst><a:ext uri=\"{{FF2B5EF4-FFF2-40B4-BE49-F238E27FC236}}\"><a16:creationId xmlns:a16=\"http://schemas.microsoft.com/office/drawing/2014/main\" id=\"{{{:08X}-{:04X}-{:04X}-{:04X}-{:012X}}}\"/></a:ext></a:extLst>",
            (h >> 96) as u32,
            (h >> 80) as u16,
            (h >> 64) as u16,
            (h >> 48) as u16,
            (h & 0xffff_ffff_ffff) as u64
        )
    } else {
        String::new()
    };
    let close = if creation.is_empty() {
        "/>".to_owned()
    } else {
        format!(">{creation}</xdr:cNvPr>")
    };
    let xfrm = if tag == "twoCellAnchor" {
        "<a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/>".to_owned()
    } else {
        format!("<a:off x=\"0\" y=\"0\"/><a:ext cx=\"{width}\" cy=\"{height}\"/>")
    };
    bytes.extend(format!("{close}<xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr><xdr:xfrm>{xfrm}</xdr:xfrm><a:graphic><a:graphicData uri=\"{C}\"><c:chart xmlns:c=\"{C}\" r:id=\"{rel}\"/></a:graphicData></a:graphic></xdr:graphicFrame><xdr:clientData/></xdr:{tag}>").as_bytes());
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
    // A chart a macro made keeps its identity when the drawing leaves it free (its name `Chart N`
    // is that identity less one); the shapes take theirs around it.
    let mut chart_ids: std::collections::BTreeMap<&str, u32> = Default::default();
    for chart in new {
        if let Some(id) = chart.drawing_id.filter(|id| !ids.contains(id)) {
            ids.insert(id);
            chart_ids.insert(chart.part.as_str(), id);
        }
    }
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
        let id = match chart_ids.get(chart.part.as_str()) {
            Some(id) => *id,
            None => {
                let id = (2..=u32::MAX)
                    .find(|i| !ids.contains(i))
                    .ok_or_else(|| invalid(&drawing, "No frame identity is available"))?;
                ids.insert(id);
                id
            }
        };
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
