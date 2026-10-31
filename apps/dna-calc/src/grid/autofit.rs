//! One-shot fitting of the same text styles and geometry the Grid renders.
use crate::wire;
use std::collections::BTreeMap;

pub const FIT_CELLS: usize = 10_000;

/// Aggregate measured display pixels into physical point/character inputs. Blank bands retain
/// their existing size. A number also needs the retained numeric-fit renderer's width estimate.
pub fn fit_sizes(rows: bool, samples: impl IntoIterator<Item = (u32, f64)>) -> Vec<(u32, u64)> {
    let mut sizes = BTreeMap::<u32, f64>::new();
    for (index, pixels) in samples {
        if index == 0 || !pixels.is_finite() || pixels <= 0.0 {
            continue;
        }
        let value = if rows {
            (pixels.ceil().max(20.0) * 0.75).min(409.0)
        } else {
            ((pixels.ceil().max(12.0) - 5.0) / 7.0).clamp(1.0, 255.0)
        };
        sizes
            .entry(index)
            .and_modify(|n| *n = n.max(value))
            .or_insert(value);
    }
    sizes.into_iter().map(|(i, n)| (i, n.to_bits())).collect()
}

pub use dna_calc_model::projection::TextStyle;

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
/// Exact axis-aligned bounds of the same CSS rotation applied to the text span.
pub fn rotated_size(width: f64, height: f64, degrees: i16) -> (f64, f64) {
    let angle = f64::from(degrees).to_radians();
    let (c, s) = (angle.cos().abs(), angle.sin().abs());
    (width * c + height * s, width * s + height * c)
}
fn family_css(family: Option<&str>) -> String {
    if let Some(f) = family.filter(|f| {
        matches!(
            f.to_ascii_lowercase().as_str(),
            "serif" | "sans-serif" | "monospace" | "system-ui"
        )
    }) {
        return f.into();
    }
    family
        .map(|f| {
            format!(
                "'{}',system-ui",
                f.replace('\\', "\\\\")
                    .replace('\'', "\\'")
                    .replace(['\n', '\r', '\u{000c}'], " ")
            )
        })
        .unwrap_or("system-ui".into())
}
/// One font declaration for rendering, measurement, and the font-presence probes.
pub fn font_css(
    style: &TextStyle,
    conditional: Option<&crate::cells::ConditionalShown>,
    italic: bool,
    zoom: f64,
) -> String {
    format!(
        "font-family:{};font-size:{}px;font-weight:{};font-style:{};line-height:1",
        family_css(style.family.as_deref()),
        style.pixels * zoom,
        if style.bold || conditional.is_some_and(|c| c.bold) {
            700
        } else {
            400
        },
        if style.italic || italic || conditional.is_some_and(|c| c.italic) {
            "italic"
        } else {
            "normal"
        }
    )
}
/// A shared DOM font probe. Its inline declarations are assigned through CSSOM under the CSP.
pub(super) fn probe(font: &str, text: &str) -> Result<(f64, f64), String> {
    use leptos::prelude::*;
    use wasm_bindgen::JsCast;
    let node = document()
        .create_element("span")
        .map_err(|_| "Font measurement is unavailable")?
        .dyn_into::<web_sys::HtmlElement>()
        .map_err(|_| "Font measurement is unavailable")?;
    node.set_text_content(Some(text));
    node.set_class_name("dc-fit-measure");
    crate::style::css(
        node.clone().unchecked_into(),
        format!("{font};white-space:pre"),
    );
    document()
        .body()
        .ok_or("The document is unavailable")?
        .append_child(&node)
        .map_err(|_| "Font measurement is unavailable")?;
    let rect = node.get_bounding_client_rect();
    node.remove();
    Ok((rect.width(), rect.height()))
}
/// Indentation uses measured spaces of the normal font, rather than a digit-width estimate.
pub fn indent_pixels(
    style: &TextStyle,
    _conditional: Option<&crate::cells::ConditionalShown>,
    _italic: bool,
    zoom: f64,
) -> f64 {
    if style.indent == 0 {
        return 0.;
    }
    // OOXML indent uses three spaces of the normal font, which the Grid maps to its base font.
    let font = font_css(&TextStyle::default(), None, false, zoom);
    probe(&font, "   ")
        .map(|(w, _)| w * f64::from(style.indent))
        .unwrap_or(0.)
}
fn font_available(style: &TextStyle) -> bool {
    let Some(family) = style.family.as_deref() else {
        return true;
    };
    if matches!(
        family.to_ascii_lowercase().as_str(),
        "serif" | "sans-serif" | "monospace" | "system-ui"
    ) {
        return true;
    }
    let sample = "WWWWiiiiMMMM1234567890";
    ["monospace", "serif"].into_iter().any(|fallback| {
        let base = probe(&format!("font:24px {fallback}"), sample).map(|p| p.0);
        let named = probe(
            &format!(
                "font:24px {},{fallback}",
                family_css(Some(family)).trim_end_matches(",system-ui")
            ),
            sample,
        )
        .map(|p| p.0);
        matches!((base,named),(Ok(a),Ok(b)) if (a-b).abs()>0.01)
    })
}
/// Measure the Grid's text font, wrapping, indentation and supported rotation at 100% zoom.
pub fn measure(rows: bool, cells: &[wire::Cell]) -> Result<Vec<(u32, u64)>, String> {
    use leptos::prelude::*;
    use wasm_bindgen::JsCast;
    let layer = document()
        .query_selector("[data-dnacalc=grid][data-grid-active=true] .dc-layer")
        .map_err(|_| "The grid is unavailable")?
        .ok_or("Return to Grid before fitting contents")?;
    for cell in cells {
        if let Some(reason) = &cell.style.unsupported {
            return Err(reason.clone());
        }
        if !font_available(&cell.style) {
            return Err(format!(
                "AutoFit needs the unavailable font {}",
                cell.style.family.as_deref().unwrap_or("unknown")
            ));
        }
        if !rows && cell.style.wrap {
            return Err("AutoFit columns with wrapped text is unavailable; set column width, then fit row height".into());
        }
    }
    let probe = document()
        .create_element("span")
        .map_err(|_| "Text measurement is unavailable")?
        .dyn_into::<web_sys::HtmlElement>()
        .map_err(|_| "Text measurement is unavailable")?;
    probe.set_class_name("dc-fit-measure");
    layer
        .append_child(&probe)
        .map_err(|_| "Text measurement is unavailable")?;
    let mut samples = Vec::new();
    for cell in cells {
        let conditional = cell.conditional.as_ref();
        let text = if conditional.is_some_and(|c| c.hide_value) {
            ""
        } else {
            &cell.text
        };
        if text.is_empty() && !conditional.is_some_and(|c| c.icon.is_some()) {
            continue;
        }
        let italic = matches!(
            cell.provenance,
            crate::cells::Provenance::FileCache | crate::cells::Provenance::Refused
        );
        let indent = indent_pixels(&cell.style, conditional, italic, 1.);
        let icon = if conditional.is_some_and(|c| c.icon.is_some()) {
            20.
        } else {
            0.
        };
        let width = if rows && cell.style.wrap {
            format!(
                ";width:{}px;white-space:pre-wrap;overflow-wrap:anywhere",
                (cell.style.width - 8. - indent - icon).max(1.)
            )
        } else {
            ";width:auto;white-space:pre".into()
        };
        crate::style::css(
            probe.clone().unchecked_into(),
            format!(
                "{}{}",
                font_css(&cell.style, conditional, italic, 1.),
                width
            ),
        );
        probe.set_text_content(Some(text));
        let r = probe.get_bounding_client_rect();
        let (w, h) = rotated_size(r.width(), r.height(), cell.style.rotation);
        let pixels = if rows {
            h + 4.
        } else {
            let visible = w + 8. + indent + icon;
            if cell.kind == "number" {
                visible.max(text.chars().count() as f64 * 7. + 5.)
            } else {
                visible
            }
        };
        samples.push((if rows { cell.at.2 } else { cell.at.3 }, pixels));
    }
    probe.remove();
    Ok(fit_sizes(rows, samples))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn fitting_keeps_each_bands_largest_sample_and_documented_limits() {
        let columns: Vec<_> = fit_sizes(
            false,
            [(2, 75.0), (1, 145.0), (2, 40.0), (4, f64::NAN), (3, 5000.0)],
        )
        .into_iter()
        .map(|(i, n)| (i, f64::from_bits(n)))
        .collect();
        assert_eq!(columns, [(1, 20.0), (2, 10.0), (3, 255.0)]);
        let rows: Vec<_> = fit_sizes(true, [(1, 13.0), (2, 40.0), (3, 1000.0)])
            .into_iter()
            .map(|(i, n)| (i, f64::from_bits(n)))
            .collect();
        assert_eq!(rows, [(1, 15.0), (2, 30.0), (3, 409.0)]);
        assert!(fit_sizes(false, []).is_empty());
    }
}

#[cfg(test)]
mod style_tests {
    use super::*;
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn imported_style_projection_is_the_same_source_for_rendering_and_fitting() {
        let source: std::sync::Arc<dyn calc_host::PackageRead> =
            std::sync::Arc::new(calc_host::MemorySource::new(
                include_bytes!("../../../../crates/calc-doc/tests/fixtures/dc_5sr/features.xlsx")
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
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn rotated_bounds_follow_the_rendered_text_span() {
        assert_eq!(rotated_size(80., 20., 0), (80., 20.));
        let (w, h) = rotated_size(80., 20., 90);
        assert!((w - 20.).abs() < 1e-10 && (h - 80.).abs() < 1e-10);
        let a = rotated_size(80., 20., 45);
        let b = rotated_size(80., 20., -45);
        assert_eq!(a, b);
        assert!((a.0 - 100. / 2f64.sqrt()).abs() < 1e-10);
        assert!((a.0 - a.1).abs() < 1e-10);
    }
}
