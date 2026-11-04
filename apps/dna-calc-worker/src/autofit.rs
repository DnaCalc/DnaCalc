//! Existing workbook text-style projection.
use dna_calc_model::projection::TextStyle;
/// Read existing style records in the worker; the UI receives no document handle.
pub fn text_style(
    host: &calc_host::HostState,
    at: calc_host::value::CellKey,
    width: f64,
) -> TextStyle {
    text_style_for(host, at, width, host.styles(), host.cell_style(at))
}

pub fn text_style_for(
    host: &calc_host::HostState,
    at: calc_host::value::CellKey,
    width: f64,
    styles: &calc_host::StyleTable,
    index: u32,
) -> TextStyle {
    let mut out = TextStyle::default();
    out.width = width;
    let Some(format) = styles.cell_formats.get(index as usize) else {
        if index != 0 {
            out.unsupported = Some("The cell style is unavailable".into());
        }
        return table_style(host,at,out,if index==0{0}else{u32::MAX},if index==0{0}else{u32::MAX},index==0);
    };
    let parent = format
        .xf_id
        .and_then(|id| styles.cell_style_formats.get(id as usize));
    let font_id = format
        .font_id
        .or_else(|| parent.and_then(|p| p.font_id))
        .unwrap_or(0);
    let mut automatic_font = false;
    if let Some(font) = styles.fonts.get(font_id as usize) {
        automatic_font = font.color.as_ref().is_none_or(|c|
            c.auto && c.rgb.is_none() && c.indexed.is_none() && c.theme.is_none() && c.tint.is_none());
        out.family = font.name.clone();
        let size = font
            .size
            .as_deref()
            .and_then(|n| n.parse::<f64>().ok())
            .unwrap_or(11.);
        if size.is_finite() && size > 0. && size <= 409. {
            out.pixels = size * 96. / 72.;
        } else {
            out.unsupported = Some("The font size is unavailable".into());
        }
        out.bold = font.bold;
        out.italic = font.italic;
        out.underline = matches!(font.underline.as_deref(),Some("single"|"double"));
        out.underline_style = font.underline.clone();
        out.strike = font.strike;
        if !font.other_children.is_empty()
            || font
                .underline
                .as_deref()
                .is_some_and(|v| !matches!(v, "none" | "single" | "double"))
        {
            out.unsupported = Some(
                "Additional font effects or underline variants are preserved but not painted"
                    .into(),
            );
        }
        out.font_color = font
            .color
            .as_ref()
            .and_then(|c| crate::cells::style_color(host, c));
    } else if font_id != 0 {
        out.unsupported = Some("The font record is unavailable".into());
    }
    let fill_id = format
        .fill_id
        .or_else(|| parent.and_then(|p| p.fill_id))
        .unwrap_or(0);
    if let Some(fill) = styles.fills.get(fill_id as usize) {
        if fill.pattern_type.as_deref() == Some("solid") {
            out.fill = fill
                .fg_color
                .as_ref()
                .and_then(|c| crate::cells::style_color(host, c));
        } else if let Some(g) = &fill.gradient {
            let parse = |n: Option<&str>, default: f64| {
                n.map_or(Some(default), |s| s.parse::<f64>().ok())
                    .filter(|n| n.is_finite())
            };
            let kind = g.kind.as_deref().unwrap_or("linear");
            let degree = parse(g.degree.as_deref(), 0.);
            let rectangle: Option<Vec<_>> = g
                .rectangle
                .iter()
                .map(|s| parse(s.as_deref(), 0.).filter(|n| (0. ..=1.).contains(n)))
                .collect();
            let stops: Option<Vec<_>> = g
                .stops
                .iter()
                .map(|s| {
                    Some((
                        parse(s.position.as_deref(), f64::NAN)
                            .filter(|n| (0. ..=1.).contains(n))?,
                        crate::cells::style_color(host, s.color.as_ref()?)?,
                    ))
                })
                .collect();
            match (degree,rectangle,stops) {
                (Some(degree),Some(rectangle),Some(mut stops)) if matches!(kind,"linear"|"path")&&rectangle[0]<=rectangle[1]&&rectangle[2]<=rectangle[3]&&(2..=256).contains(&stops.len())=> {
                    stops.sort_by(|a,b|a.0.total_cmp(&b.0));
                    out.brush=Some(dna_calc_model::projection::FillShown::Gradient {kind:kind.into(),degree,rectangle:rectangle.try_into().unwrap(),stops});
                },
                _=>out.unsupported=Some("This gradient's geometry or colours are unavailable; its stored fill is preserved".into()),
            }
        } else if let Some(name) = fill.pattern_type.as_deref().filter(|p| *p != "none") {
            if matches!(
                name,
                "darkGray"
                    | "mediumGray"
                    | "lightGray"
                    | "gray125"
                    | "gray0625"
                    | "darkHorizontal"
                    | "lightHorizontal"
                    | "darkVertical"
                    | "lightVertical"
                    | "darkDown"
                    | "lightDown"
                    | "darkUp"
                    | "lightUp"
                    | "darkGrid"
                    | "lightGrid"
                    | "darkTrellis"
                    | "lightTrellis"
            ) {
                let foreground = fill
                    .fg_color
                    .as_ref()
                    .and_then(|c| crate::cells::style_color(host, c));
                let background = fill
                    .bg_color
                    .as_ref()
                    .and_then(|c| crate::cells::style_color(host, c));
                if (foreground.is_none() && fill.fg_color.as_ref().is_some_and(|c| !c.auto))
                    || (background.is_none() && fill.bg_color.as_ref().is_some_and(|c| !c.auto))
                {
                    out.unsupported=Some("This pattern's supplied colour is unavailable; its stored fill is preserved".into());
                } else {
                    out.brush = Some(dna_calc_model::projection::FillShown::Pattern {
                        name: name.into(),
                        foreground: foreground.unwrap_or_else(|| "#000000".into()),
                        background: background.unwrap_or_else(|| "#ffffff".into()),
                    });
                }
            } else {
                out.unsupported =
                    Some("This fill pattern is unavailable; its stored fill is preserved".into());
            }
        }
    }
    let border_id = format
        .border_id
        .or_else(|| parent.and_then(|p| p.border_id))
        .unwrap_or(0);
    if let Some(b) = styles.borders.get(border_id as usize) {
        out.borders = [&b.left, &b.right, &b.top, &b.bottom].map(|e| {
            e.as_ref().and_then(|e| {
                e.style
                    .as_ref()
                    .filter(|style| style.as_str() != "none")
                    .map(|style| dna_calc_model::projection::BorderShown {
                        style: style.clone(),
                        color: e
                            .color
                            .as_ref()
                            .and_then(|c| crate::cells::style_color(host, c)),
                    })
            })
        });
        if out.borders.iter().flatten().any(|e| {
            !matches!(
                e.style.as_str(),
                "thin" | "medium" | "thick" | "double" | "dotted" | "dashed" | "hair"
            )
        }) {
            out.unsupported=Some("This border dash pattern is approximated by the DOM painter; its stored style is preserved".into());
        }
        if let Some(edge) = b.diagonal.as_ref().and_then(|e| {
            e.style
                .as_ref()
                .filter(|s| s.as_str() != "none")
                .map(|style| dna_calc_model::projection::BorderShown {
                    style: style.clone(),
                    color: e
                        .color
                        .as_ref()
                        .and_then(|c| crate::cells::style_color(host, c)),
                })
        }) {
            if b.diagonal_up {
                out.diagonal[0] = Some(edge.clone());
            }
            if b.diagonal_down {
                out.diagonal[1] = Some(edge);
            }
        }
    }
    if let Some(a) = calc_host::style_alignment(styles, index) {
        out.wrap = a.wrap_text;
        out.shrink = a.shrink_to_fit;
        out.horizontal = a.horizontal.clone();
        out.vertical = a.vertical.clone();
        if matches!(a.horizontal.as_deref(), Some("centerContinuous" | "fill")) {
            for column in at.column() + 1..=16_384 {
                let next = calc_host::value::CellKey::new(at.sheet(), at.row(), column).unwrap();
                if host.model().cell(next).is_some()
                    || host
                        .cell_alignment(next)
                        .and_then(|next| next.horizontal.as_deref())
                        != a.horizontal.as_deref()
                {
                    break;
                }
                out.alignment_columns += 1;
            }
        }

        if let Some(n) = a.indent.as_deref() {
            match n.parse::<u16>() {
                Ok(n) if n <= 250 => out.indent = n,
                _ => out.unsupported = Some("This indentation is unavailable".into()),
            }
        }
        if let Some(n) = a.text_rotation.as_deref() {
            match n.parse::<i16>() {
                Ok(n @ 0..=90) => out.rotation = -n,
                Ok(n @ 91..=180) => out.rotation = n - 90,
                Ok(255) => out.rotation = 255,
                _ => {
                    out.unsupported =
                        Some("This text rotation is unavailable".into())
                }
            }
        }
        if a.shrink_to_fit
            || matches!(
                a.horizontal.as_deref(),
                Some("fill" | "justify" | "distributed" | "centerContinuous")
            )
            || matches!(a.vertical.as_deref(), Some("justify" | "distributed"))
        {
            out.unsupported=Some("AutoFit with shrink-to-fit, fill, justification or distributed alignment is unavailable".into());
        }
        if out.wrap && out.rotation != 0 {
            out.unsupported = Some("AutoFit combining wrapping and rotation is unavailable".into());
        }
    }
    if out.shrink && (out.wrap || out.rotation!=0 || host.merged_areas(at.sheet()).iter().any(|a|a.contains(at))
        || matches!(out.horizontal.as_deref(),Some("fill"|"justify"|"distributed"|"centerContinuous"))) {
        out.unsupported=Some("Shrink-to-fit with wrapping, rotation, merged cells or special horizontal alignment is preserved but not painted (dc-971p.6)".into());
    }
    table_style(host,at,out,font_id,fill_id,automatic_font)
}
fn table_style(
    host: &calc_host::HostState,
    at: calc_host::value::CellKey,
    mut out: TextStyle,
    font_id: u32,
    fill_id: u32,
    automatic_font: bool,
) -> TextStyle {
    if let Some(table) = host.model().definitions().tables().iter().find(|t| t.area.contains(at)) {
        let style = &table.style;
        if let Some(theme) = style.medium_accent_theme() {
            let header = table.header_rows > 0 && at.row() == table.area.first().row();
            let total = table.totals_rows > 0 && at.row() == table.area.last().row();
            let emphasis = header || total
                || (style.first_column && at.column() == table.area.first().column())
                || (style.last_column && at.column() == table.area.last().column());
            let striped = (style.row_stripes
                && (at.row() - table.area.first().row()).saturating_sub(u32::from(table.header_rows > 0)) % 2 == 0)
                || (style.column_stripes && (at.column() - table.area.first().column()) % 2 == 0);
            let mut table_fill = false;
            if fill_id == 0 && out.fill.is_none() && out.brush.is_none() && (header || total || striped) {
                let color = calc_host::StyleColor {
                    rgb: host.theme_accent((theme - 4) as usize).map(|s| format!("FF{s}")),
                    tint: if header { None } else { Some(if total { "0.6" } else { "0.8" }.into()) },
                    ..Default::default()
                };
                out.fill = crate::cells::style_color(host, &color);
                table_fill = out.fill.is_some();
            }
            if font_id == 0 {
                if emphasis { out.bold = true; }
                if header { out.font_color = Some("#ffffff".into()); }
            }
            // A preset's pale bands and totals need a foreground independent of the UI theme.
            // Cell font overrides and manually supplied body fills retain their own styling.
            if automatic_font && out.font_color.is_none() && table_fill {
                out.font_color = Some(if header { "#ffffff" } else { "#000000" }.into());
            }
        } else if style.name.is_some() {
            out.unsupported.get_or_insert("This table style is preserved; its table effects are not painted".into());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn table_preset_fill_projects_readable_automatic_text_and_preserves_explicit_colours() {
        use calc_host::{HostSession, SkinIntent, TableChange, WorkbookCommand};
        let mut book = HostSession::open(
            std::sync::Arc::new(calc_host::MemorySource::new(calc_host::blank_package())),
            &Default::default(), Default::default(),
        ).unwrap();
        let sheet = book.host().sheets()[0].id();
        let at = |row| calc_host::value::CellKey::new(sheet, row, 1).unwrap();
        let revision = book.host().revision().get();
        let receipt = book.dispatch(SkinIntent::Command(WorkbookCommand::Table {
            revision, change: TableChange::Create {
                area: calc_host::value::GridArea::new(at(1), at(4)).unwrap(),
                name: "Contrast".into(),
            },
        }));
        assert!(receipt.accepted(), "{receipt:?}");
        let host = book.host();
        let header = text_style(host, at(1), 64.);
        assert!(header.fill.is_some());
        assert_eq!(header.font_color.as_deref(), Some("#ffffff"));
        let band = text_style(host, at(2), 64.);
        assert!(band.fill.is_some());
        assert_eq!(band.font_color.as_deref(), Some("#000000"));
        // A changed font family still has automatic colour.
        let mut styles = (**host.styles()).clone();
        let index = calc_host::patched_style(&mut styles, 0, &calc_host::FormatPatch {
            font_name: Some("monospace".into()), ..Default::default()
        }).unwrap();
        assert_eq!(text_style_for(host, at(2), 64., &styles, index).font_color.as_deref(), Some("#000000"));
        let normal = TextStyle { font_color: Some("#000000".into()), ..Default::default() };
        assert_eq!(table_style(host, at(1), normal, 0, 0, false).font_color.as_deref(), Some("#ffffff"));
        let explicit = TextStyle { font_color: Some("#123456".into()), ..Default::default() };
        assert_eq!(table_style(host, at(1), explicit, 1, 0, false).font_color.as_deref(), Some("#123456"));
        let font = styles.cell_formats[index as usize].font_id.unwrap() as usize;
        styles.fonts[font].color = Some(calc_host::StyleColor { theme: Some("99".into()), ..Default::default() });
        let unresolved = text_style_for(host, at(2), 64., &styles, index);
        assert!(unresolved.fill.is_some());
        assert_eq!(unresolved.font_color, None, "An unresolved explicit font is not automatic");
        styles.fonts[font].color = Some(calc_host::StyleColor { auto: true, ..Default::default() });
        assert_eq!(text_style_for(host, at(2), 64., &styles, index).font_color.as_deref(), Some("#000000"));
        let manual = TextStyle { fill: Some("#101010".into()), ..Default::default() };
        assert_eq!(table_style(host, at(2), manual, 0, 1, true).font_color, None);
        assert_eq!(text_style(host, at(3), 64.).font_color, None);
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn imported_style_projection_is_the_same_source_for_rendering_and_fitting() {
        let source: std::sync::Arc<dyn calc_host::PackageRead> =
            std::sync::Arc::new(calc_host::MemorySource::new(
                include_bytes!("../../../crates/calc-doc/tests/fixtures/dc_5sr/features.xlsx")
                    .to_vec(),
            ));
        let host =
            calc_host::HostState::open(source, &Default::default(), Default::default()).unwrap();
        let sheet = host.sheets()[0].id();
        let a = text_style(
            &host,
            calc_host::value::CellKey::new(sheet, 1, 1).unwrap(),
            64.,
        );
        assert!(a.bold);
        assert_eq!(a.pixels, 11. * 96. / 72.);
        assert_eq!(a.family.as_deref(), Some("Aptos Narrow"));
        assert_eq!(a.unsupported, None);
        let b = text_style(
            &host,
            calc_host::value::CellKey::new(sheet, 5, 2).unwrap(),
            64.,
        );
        assert!(b.wrap);
        assert!(!b.bold);
        assert_eq!(b.unsupported, None);
        assert!(b.width > 0.);
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn authored_font_points_do_not_depend_on_the_workbook_default_font() {
        let host = calc_host::HostState::open(
            std::sync::Arc::new(calc_host::MemorySource::new(calc_host::blank_package())),
            &Default::default(),
            Default::default(),
        )
        .unwrap();
        let at = calc_host::value::CellKey::new(host.sheets()[0].id(), 1, 1).unwrap();
        let mut table = (**host.styles()).clone();
        let index = calc_host::patched_style(
            &mut table,
            0,
            &calc_host::FormatPatch {
                font_name: Some("monospace".into()),
                font_size: Some("14".into()),
                ..Default::default()
            },
        )
        .unwrap();
        for size in ["11", "22"] {
            table.fonts[0].size = Some(size.into());
            let style = text_style_for(&host, at, 200., &table, index);
            assert_eq!(style.pixels, 14. * 96. / 72.);
            assert_eq!(style.family.as_deref(), Some("monospace"));
        }
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn physical_pattern_gradient_and_diagonal_project_resolved_geometry_and_colours() {
        let host = calc_host::HostState::open(
            std::sync::Arc::new(calc_host::MemorySource::new(calc_host::blank_package())),
            &Default::default(),
            Default::default(),
        )
        .unwrap();
        let at = calc_host::value::CellKey::new(host.sheets()[0].id(), 1, 1).unwrap();
        let mut table = (**host.styles()).clone();
        let rgb = |s: &str| calc_host::StyleColor {
            rgb: Some(s.into()),
            ..Default::default()
        };
        let mut fill = table.fills[0].clone();
        fill.pattern_type = Some("darkDown".into());
        fill.fg_color = Some(rgb("FFFF0000"));
        fill.bg_color = Some(rgb("FFFFFFFF"));
        table.fills.push(fill);
        let pattern = table.fills.len() - 1;
        let mut fill = table.fills[0].clone();
        fill.gradient = Some(Default::default());
        let g = fill.gradient.as_mut().unwrap();
        g.degree = Some("90".into());
        g.stops.resize_with(2, Default::default);
        for (i, color) in ["FFFF0000", "FF0000FF"].into_iter().enumerate() {
            g.stops[i].position = Some(i.to_string());
            g.stops[i].color = Some(rgb(color));
        }
        table.fills.push(fill);
        let gradient = table.fills.len() - 1;
        let mut border = table.borders[0].clone();
        border.diagonal_up = true;
        border.diagonal_down = true;
        let mut edge = border.diagonal.take().unwrap_or_default();
        edge.style = Some("double".into());
        edge.color = Some(rgb("FF008000"));
        border.diagonal = Some(edge);
        table.borders.push(border);
        let mut xf = table.cell_formats[0].clone();
        xf.fill_id = Some(pattern as u32);
        xf.border_id = Some((table.borders.len() - 1) as u32);
        table.cell_formats.push(xf);
        let index = (table.cell_formats.len() - 1) as u32;
        let projected = text_style_for(&host, at, 100., &table, index);
        assert_eq!(projected.unsupported, None);
        assert_eq!(
            projected.brush,
            Some(dna_calc_model::projection::FillShown::Pattern {
                name: "darkDown".into(),
                foreground: "#ff0000".into(),
                background: "#ffffff".into()
            })
        );
        for edge in projected.diagonal {
            let edge = edge.unwrap();
            assert_eq!(edge.style, "double");
            assert_eq!(edge.color.as_deref(), Some("#008000"));
        }
        table.cell_formats[index as usize].fill_id = Some(gradient as u32);
        let projected = text_style_for(&host, at, 100., &table, index);
        assert_eq!(projected.unsupported, None);
        assert!(
            matches!(projected.brush,Some(dna_calc_model::projection::FillShown::Gradient {degree,stops,..}) if degree==90. && stops==vec![(0.,"#ff0000".into()),(1.,"#0000ff".into())])
        );
        table.fills[gradient].gradient.as_mut().unwrap().stops[0].position = Some("NaN".into());
        let refused = text_style_for(&host, at, 100., &table, index);
        assert!(refused.brush.is_none());
        assert!(refused.unsupported.is_some());
    }
}
