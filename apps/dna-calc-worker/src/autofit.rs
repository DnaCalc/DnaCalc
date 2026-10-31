//! Existing workbook text-style projection.
use dna_calc_model::projection::TextStyle;
/// Read existing style records in the worker; the UI receives no document handle.
pub fn text_style(
    host: &calc_host::HostState,
    at: calc_host::value::CellKey,
    width: f64,
) -> TextStyle {
    let styles = host.loaded().styles();
    let mut out = TextStyle::default();
    out.width = width;
    let index = host.cell_style(at);
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
        let base = styles.fonts.first();
        out.family = font
            .name
            .clone()
            .filter(|n| base.and_then(|f| f.name.as_ref()) != Some(n));
        let size = font
            .size
            .as_deref()
            .and_then(|n| n.parse::<f64>().ok())
            .unwrap_or(11.);
        let base_size = base
            .and_then(|f| f.size.as_deref())
            .and_then(|n| n.parse::<f64>().ok())
            .filter(|n| n.is_finite() && *n > 0.)
            .unwrap_or(11.);
        if size.is_finite() && size > 0. && size <= 409. {
            out.pixels = 13. * size / base_size;
        } else {
            out.unsupported = Some("The font size is unavailable".into());
        }
        out.bold = font.bold;
        out.italic = font.italic;
    } else if font_id != 0 {
        out.unsupported = Some("The font record is unavailable".into());
    }
    if let Some(a) = host.cell_alignment(at) {
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
        assert_eq!(a.pixels, 13.);
        assert_eq!(a.family, None);
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
}
