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
        } else if !matches!(
            fill.pattern_type.as_deref(),
            None | Some("none" | "gray125")
        ) || !fill.other_children.is_empty()
        {
            out.unsupported =
                Some("This patterned or gradient fill is preserved but not painted".into());
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
        if b.diagonal_up || b.diagonal_down {
            out.unsupported = Some("Diagonal borders are preserved but not painted".into());
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
}
