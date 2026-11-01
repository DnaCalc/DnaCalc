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
        return out;
    };
    let parent = format
        .xf_id
        .and_then(|id| styles.cell_style_formats.get(id as usize));
    let font_id = format
        .font_id
        .or_else(|| parent.and_then(|p| p.font_id))
        .unwrap_or(0);
    if let Some(font) = styles.fonts.get(font_id as usize) {
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
        out.underline = font.underline.as_deref().is_some_and(|v| v != "none");
        out.strike = font.strike;
        if !font.other_children.is_empty()
            || font
                .underline
                .as_deref()
                .is_some_and(|v| !matches!(v, "none" | "single"))
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
                _ => {
                    out.unsupported =
                        Some("Vertical or invalid text rotation is unavailable".into())
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
    out
}

#[cfg(test)]
mod tests {
    use super::*;
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
