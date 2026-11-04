//! One-shot fitting of the same text styles and geometry the Grid renders.
use crate::wire;
use std::collections::BTreeMap;

/// Aggregate measured display pixels into physical point/character inputs. Blank bands retain
/// their existing size.
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

/// Exact axis-aligned bounds of the same CSS rotation applied to the text span.
pub fn rotated_size(width: f64, height: f64, degrees: i16) -> (f64, f64) {
    let angle = f64::from(degrees).to_radians();
    let (c, s) = (angle.cos().abs(), angle.sin().abs());
    (width * c + height * s, width * s + height * c)
}
pub(super) fn family_css(family: Option<&str>) -> String {
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
/// Shared by the grid text and the Format Cells sample.
pub(crate) fn decoration_css(underline: bool, strike: bool) -> &'static str {
    match (underline, strike) {
        (true, true) => ";text-decoration:underline line-through",
        (true, false) => ";text-decoration:underline",
        (false, true) => ";text-decoration:line-through",
        (false, false) => "",
    }
}
pub(crate) fn double_underline(style: &TextStyle, conditional: Option<&crate::cells::ConditionalShown>) -> bool {
    style.underline && style.underline_style.as_deref()==Some("double")
        && !conditional.is_some_and(|c|c.underline)
}
pub(super) fn underline_width(pixels:f64,zoom:f64)->f64 { zoom.min(pixels*zoom/5.) }
pub(crate) fn double_underline_css(pixels:f64,zoom:f64)->String {
    format!(";text-decoration-line:underline;text-decoration-style:double;text-decoration-skip-ink:none;text-decoration-thickness:{}px",underline_width(pixels,zoom))
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
/// Canvas text and the plan's width provider use the same font as the DOM painter.
pub(super) fn canvas_font(
    style: &TextStyle,
    conditional: Option<&crate::cells::ConditionalShown>,
    italic: bool,
    zoom: f64,
) -> String {
    format!(
        "{} {} {}px {}",
        if style.italic || italic || conditional.is_some_and(|c| c.italic) {
            "italic"
        } else {
            "normal"
        },
        if style.bold || conditional.is_some_and(|c| c.bold) {
            700
        } else {
            400
        },
        style.pixels * zoom,
        family_css(style.family.as_deref())
    )
}
#[derive(Default)]
struct FontMeasures {
    revision: Option<u64>,
    context: Option<web_sys::CanvasRenderingContext2d>,
    widths: BTreeMap<(String, String), f64>,
    bytes: usize,
}
thread_local! {
    static FONT_MEASURES: std::cell::RefCell<FontMeasures> = Default::default();
}
/// Bounded browser advance cache. A real FontFaceSet completion invalidates fallback widths.
/// Failure leaves the plan's explicit estimate, rather than claiming a measurement.
pub(super) fn plan_advance(
    shown: &crate::cells::CellShown,
    text: &str,
    zoom: f64,
    revision: u64,
) -> Option<f64> {
    use leptos::prelude::*;
    use wasm_bindgen::JsCast;
    let font = canvas_font(
        &shown.style,
        shown.conditional.as_ref(),
        matches!(
            shown.provenance,
            crate::cells::Provenance::FileCache | crate::cells::Provenance::Refused
        ),
        zoom,
    );
    FONT_MEASURES.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.revision != Some(revision) {
            cache.revision = Some(revision);
            cache.widths.clear();
            cache.bytes = 0;
        }
        let key = (font.clone(), text.to_owned());
        if let Some(width) = cache.widths.get(&key) {
            return Some(*width);
        }
        if cache.context.is_none() {
            let canvas = document()
                .create_element("canvas")
                .ok()?
                .dyn_into::<web_sys::HtmlCanvasElement>()
                .ok()?;
            canvas.set_width(0);
            canvas.set_height(0);
            cache.context = canvas.get_context("2d").ok()??.dyn_into().ok();
        }
        let context = cache.context.as_ref()?;
        context.set_font(&font);
        let mut width: f64 = 0.0;
        for line in text.split('\n') {
            width = width.max(context.measure_text(line).ok()?.width());
        }
        if !width.is_finite() {
            return None;
        }
        let bytes = key.0.len().saturating_add(key.1.len()).saturating_add(128);
        const MAX_BYTES: usize = 2 * 1024 * 1024;
        if cache.widths.len() >= 8192 || cache.bytes.saturating_add(bytes) > MAX_BYTES {
            cache.widths.clear();
            cache.bytes = 0;
        }
        if bytes <= MAX_BYTES {
            cache.bytes += bytes;
            cache.widths.insert(key, width);
        }
        Some(width)
    })
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
pub fn font_available(style: &TextStyle) -> bool {
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
        probe.set_text_content(None);
        if cell.style.rotation == 255 {
            for cluster in super::alignment::graphemes(text) {
                let line = document().create_element("span")
                    .map_err(|_| "Stacked text measurement is unavailable")?;
                crate::style::css(line.clone(), super::alignment::STACKED_LINE_CSS.into());
                line.set_text_content(Some(&cluster));
                probe.append_child(&line)
                    .map_err(|_| "Stacked text measurement is unavailable")?;
            }
        } else {
            probe.set_text_content(Some(text));
        }
        let r = probe.get_bounding_client_rect();
        let (w, h) = if cell.style.rotation==255 { (r.width(),r.height()) }
            else { rotated_size(r.width(), r.height(), cell.style.rotation) };
        let pixels = if rows { h + 4. } else { w + 8. + indent + icon };
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
