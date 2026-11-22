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
pub(crate) fn double_underline(
    style: &TextStyle,
    conditional: Option<&crate::cells::ConditionalShown>,
) -> bool {
    style.underline
        && style.underline_style.as_deref() == Some("double")
        && !conditional.is_some_and(|c| c.underline)
}
/// The accounting underline is drawn across the cell, independently of the glyphs.
pub(super) fn accounting_lines(
    style: &TextStyle,
    conditional: &crate::cells::ConditionalShown,
) -> u8 {
    if conditional.underline {
        return 0;
    }
    match style.underline_style.as_deref() {
        Some("singleAccounting") => 1,
        Some("doubleAccounting") => 2,
        _ => 0,
    }
}

pub(super) fn font_effect_css(style: &TextStyle, zoom: f64) -> String {
    let mut css = String::new();
    if style.outline {
        css.push_str(&format!(";-webkit-text-stroke:{zoom}px currentColor;-webkit-text-fill-color:transparent"));
    }
    if style.shadow {
        css.push_str(&format!(";text-shadow:{zoom}px {zoom}px rgba(0,0,0,0.4)"));
    }
    css
}

pub(super) fn underline_width(pixels: f64, zoom: f64) -> f64 {
    zoom.min(pixels * zoom / 5.)
}
pub(crate) fn double_underline_css(pixels: f64, zoom: f64) -> String {
    format!(
        ";text-decoration-line:underline;text-decoration-style:double;text-decoration-skip-ink:none;text-decoration-thickness:{}px",
        underline_width(pixels, zoom)
    )
}
/// Rows ask for more height under a superscript or subscript (Excel Q1, row AutoFit of a whole-cell font, 96 DPI):
/// Calibri 11, 20 and 36 pt (14.67, 26.67 and 48 px) take 3, 4 and 8 px more than plain text for a superscript and
/// 4, 6 and 10 px for a subscript; Arial 11 takes 3 and 6. The rule is those rows' fit, 0.15 and 0.205 of the stored
/// size rounded up; Arial's subscript (6, the rule gives 4) is the one measured row it misses.
pub(crate) fn script_row_extra(style: &TextStyle) -> f64 {
    match style.script.as_deref() {
        Some("superscript") => (style.pixels * 0.15).ceil(),
        Some("subscript") => (style.pixels * 0.205).ceil(),
        _ => 0.,
    }
}

/// A superscript or subscript font draws at this share of its size (assumed: Excel's picture of the cell came out
/// empty in Q1, and its column AutoFit is not separable from the plain rows above it, so scale and baseline stay
/// unmeasured: dc-971p.6).
pub(crate) const SCRIPT_SCALE: f64 = 2. / 3.;

/// The size in pixels the font draws at: its stored size, less for a superscript or subscript.
pub(crate) fn drawn_pixels(style: &TextStyle) -> f64 {
    if style.script.is_some() {
        style.pixels * SCRIPT_SCALE
    } else {
        style.pixels
    }
}

/// How far, in pixels at `zoom`, a superscript or subscript baseline sits from the line's own (negative is up): a
/// superscript rises by a third of the stored size and a subscript falls by an eighth (assumed until Excel decides).
pub(crate) fn script_shift(style: &TextStyle, zoom: f64) -> f64 {
    match style.script.as_deref() {
        Some("superscript") => -style.pixels * zoom / 3.,
        Some("subscript") => style.pixels * zoom / 8.,
        _ => 0.,
    }
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
        drawn_pixels(style) * zoom,
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
        drawn_pixels(style) * zoom,
        family_css(style.family.as_deref())
    )
}
#[derive(Default)]
struct FontMeasures {
    revision: Option<u64>,
    context: Option<web_sys::CanvasRenderingContext2d>,
    widths: BTreeMap<(String, String, bool), f64>,
    boxes: BTreeMap<String, (f64, f64)>,
    bytes: usize,
}
impl FontMeasures {
    fn context(&mut self) -> Option<&web_sys::CanvasRenderingContext2d> {
        use leptos::prelude::*;
        use wasm_bindgen::JsCast;
        if self.context.is_none() {
            let canvas = document()
                .create_element("canvas")
                .ok()?
                .dyn_into::<web_sys::HtmlCanvasElement>()
                .ok()?;
            canvas.set_width(0);
            canvas.set_height(0);
            self.context = canvas.get_context("2d").ok()??.dyn_into().ok();
        }
        self.context.as_ref()
    }
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
            cache.boxes.clear();
            cache.bytes = 0;
        }
        let integer = integer_text(shown, text);
        let key = (font.clone(), text.to_owned(), integer);
        if let Some(width) = cache.widths.get(&key) {
            return Some(*width);
        }
        let context = cache.context()?;
        context.set_font(&font);
        let mut width: f64 = 0.0;
        for line in text.split('\n') {
            let advance = if integer {
                super::alignment::graphemes(line)
                    .into_iter()
                    .try_fold(0., |sum, part| {
                        context.measure_text(&part).ok().map(|m| {
                            sum + integer_advance(
                                &shown.style,
                                shown.conditional.as_ref(),
                                matches!(
                                    shown.provenance,
                                    crate::cells::Provenance::FileCache
                                        | crate::cells::Provenance::Refused
                                ),
                                zoom,
                                &part,
                                m.width(),
                            )
                        })
                    })?
            } else {
                context.measure_text(line).ok()?.width()
            };
            width = width.max(advance);
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
/// One grapheme's whole-pixel advance: GDI's where its table has an answer, else the
/// browser's measurement rounded the same way.
pub(super) fn integer_advance(
    style: &TextStyle,
    conditional: Option<&crate::cells::ConditionalShown>,
    italic: bool,
    zoom: f64,
    glyph: &str,
    measured: f64,
) -> f64 {
    super::gdi_advances::advance(
        style.family.as_deref(),
        style.bold || conditional.is_some_and(|c| c.bold),
        style.italic || italic || conditional.is_some_and(|c| c.italic),
        style.pixels * zoom,
        glyph,
    )
    .unwrap_or_else(|| measured.round())
}
/// Captured unwrapped Western text advances in integral display pixels. Keep complex
/// scripts, shrinking, rotation and wrapping on their shaped text route.
pub(super) fn integer_text(shown: &crate::cells::CellShown, text: &str) -> bool {
    matches!(shown.kind, "number" | "text" | "logical" | "error")
        && !shown.style.shrink
        && !shown.style.wrap
        && shown.style.rotation == 0
        && !matches!(
            shown.style.horizontal.as_deref(),
            Some("fill" | "justify" | "distributed" | "centerContinuous")
        )
        && text.is_ascii()
        && !text.bytes().any(|c| c.is_ascii_control())
}
pub(super) fn text_advances(
    shown: &crate::cells::CellShown,
    text: &str,
    zoom: f64,
) -> Option<Vec<f64>> {
    if !integer_text(shown, text) {
        return None;
    }
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
        let context = cache.context()?;
        context.set_font(&font);
        super::alignment::graphemes(text)
            .into_iter()
            .map(|part| {
                context.measure_text(&part).ok().map(|m| {
                    integer_advance(
                        &shown.style,
                        shown.conditional.as_ref(),
                        matches!(
                            shown.provenance,
                            crate::cells::Provenance::FileCache | crate::cells::Provenance::Refused
                        ),
                        zoom,
                        &part,
                        m.width(),
                    )
                })
            })
            .collect()
    })
}
/// A segmented line is one atomic inline box, so the cell's text-decoration never reaches its
/// glyphs. These backgrounds draw the lines the Canvas painter strokes at the same offsets.
pub(super) fn segmented_decoration(
    style: &TextStyle,
    conditional: &crate::cells::ConditionalShown,
    italic: bool,
    zoom: f64,
) -> String {
    let (underline, strike) = (
        (style.underline && accounting_lines(style, conditional) == 0) || conditional.underline,
        style.strike || conditional.strike,
    );
    if !underline && !strike {
        return String::new();
    }
    let line_height = style.pixels * zoom;
    let (ascent, descent) = font_box(style, Some(conditional), italic, zoom)
        .unwrap_or((line_height * 0.8, line_height * 0.2));
    let baseline = (line_height - ascent - descent) / 2. + ascent;
    let double = double_underline(style, Some(conditional));
    let stroke = if double {
        underline_width(style.pixels, zoom)
    } else {
        zoom
    };
    let under = baseline + stroke;
    let lines: Vec<String> = [
        underline.then_some(under),
        double.then_some(under + 2. * stroke),
        strike.then_some(line_height / 2.),
    ]
    .into_iter()
    .flatten()
    .map(|offset| {
        format!(
            "linear-gradient(currentColor,currentColor) 0 {}px/100% {stroke}px no-repeat",
            offset - stroke / 2.
        )
    })
    .collect();
    let spare = 4. * stroke;
    format!(
        ";background:{};padding-bottom:{spare}px;margin-bottom:-{spare}px",
        lines.join(",")
    )
}
/// Reuse the advance probe and its font-load invalidation for baseline placement.
pub(super) fn font_box(
    style: &TextStyle,
    conditional: Option<&crate::cells::ConditionalShown>,
    italic: bool,
    zoom: f64,
) -> Option<(f64, f64)> {
    let font = canvas_font(style, conditional, italic, zoom);
    FONT_MEASURES.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(bounds) = cache.boxes.get(&font) {
            return Some(*bounds);
        }
        let context = cache.context()?;
        context.set_font(&font);
        let metrics = context.measure_text("Mg").ok()?;
        let bounds = (
            metrics.font_bounding_box_ascent(),
            metrics.font_bounding_box_descent(),
        );
        if !bounds.0.is_finite() || !bounds.1.is_finite() || bounds.0 + bounds.1 <= 0. {
            return None;
        }
        if cache.boxes.len() >= 256 {
            cache.boxes.clear();
        }
        cache.boxes.insert(font, bounds);
        Some(bounds)
    })
}
/// Bottom-aligned single lines in a row share the largest font descent. Top, middle,
/// wrapping and multi-row merges retain their own placement.
pub(super) fn align_row_baselines(
    plan: &mut super::render_plan::RenderPlan,
    shown: &[crate::cells::CellShown],
    geometry: Option<&dna_calc_model::projection::Geometry>,
    zoom: f64,
) {
    let eligible = |cell: &crate::cells::CellShown| {
        !cell.text.is_empty()
            && !cell.text.contains('\n')
            && !cell.style.wrap
            && !cell.style.shrink
            && cell.style.rotation == 0
            && matches!(cell.style.vertical.as_deref(), None | Some("bottom"))
            && !geometry.is_some_and(|g| {
                g.merges.iter().any(|a| {
                    a.0.2 != a.1.2
                        && cell.at.row() >= a.0.2
                        && cell.at.row() <= a.1.2
                        && cell.at.column() >= a.0.3
                        && cell.at.column() <= a.1.3
                })
            })
    };
    let descent = |cell: &crate::cells::CellShown| {
        font_box(
            &cell.style,
            cell.conditional.as_ref(),
            matches!(
                cell.provenance,
                crate::cells::Provenance::FileCache | crate::cells::Provenance::Refused
            ),
            zoom,
        )
        .map(|(_, d)| d)
    };
    let mut rows = BTreeMap::<u32, f64>::new();
    for cell in shown.iter().filter(|c| eligible(c)) {
        if let Some(d) = descent(cell) {
            rows.entry(cell.at.row())
                .and_modify(|n| *n = n.max(d))
                .or_insert(d);
        }
    }
    for cell in &mut plan.cells {
        if eligible(&cell.shown) && cell.row_span == 1 {
            if let (Some(row), Some(own)) = (rows.get(&cell.row), descent(&cell.shown)) {
                cell.baseline_inset = (row - own).max(0.);
            }
        }
    }
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
        if !rows && cell.style.wrap && cell.style.rotation != 255 {
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
        let (left, right) = super::render_plan::text_insets(1.);
        let padding = left + right;
        let width = if rows && cell.style.wrap && cell.style.rotation != 255 {
            format!(
                ";width:{}px;white-space:pre-wrap;overflow-wrap:anywhere",
                (cell.style.width - padding - indent - icon).max(1.)
            )
        } else {
            ";width:auto;white-space:pre".into()
        };
        // Excel fits the unwrapped content before shrink, fill or centering across a selection.
        let mut fitted = cell.style.clone();
        fitted.shrink = false;
        if matches!(fitted.horizontal.as_deref(), Some("fill" | "centerContinuous")) {
            fitted.horizontal = None;
        }
        if rows {
            // Fit to the stored size plus Excel's extra for the script (Q1).
            fitted.script = None;
        }
        crate::style::css(
            probe.clone().unchecked_into(),
            format!(
                "{}{}",
                font_css(&fitted, conditional, italic, 1.),
                width
            ),
        );
        probe.set_text_content(None);
        if cell.style.rotation == 255 {
            for cluster in super::alignment::graphemes(text) {
                let line = document()
                    .create_element("span")
                    .map_err(|_| "Stacked text measurement is unavailable")?;
                crate::style::css(line.clone(), super::alignment::STACKED_LINE_CSS.into());
                line.set_text_content(Some(&cluster));
                probe
                    .append_child(&line)
                    .map_err(|_| "Stacked text measurement is unavailable")?;
            }
        } else {
            probe.set_text_content(Some(text));
        }
        let r = probe.get_bounding_client_rect();
        let (w, h) = if cell.style.rotation == 255 {
            (r.width(), r.height())
        } else {
            rotated_size(r.width(), r.height(), cell.style.rotation)
        };
        // Text the Grid places with whole-pixel advances is fitted with the same advances: a
        // browser that reports fractional outline advances (Linux) would otherwise fit a column
        // a few pixels narrower than the text the Grid paints.
        let w = if rows || cell.style.rotation == 255 {
            w
        } else {
            cell.shown()
                .and_then(|mut shown| {
                    shown.style = fitted;
                    text_advances(&shown, text, 1.)
                })
                .map_or(w, |advances| advances.iter().sum())
        };
        let pixels = if rows {
            h + 4. + script_row_extra(&cell.style)
        } else {
            w + padding + indent + icon
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
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_971p_6__super_and_subscript_draw_smaller_and_off_the_baseline_in_both_painters() {
        let plain = TextStyle { pixels: 12., family: Some("Arial".into()), ..Default::default() };
        let up = TextStyle { script: Some("superscript".into()), ..plain.clone() };
        let down = TextStyle { script: Some("subscript".into()), ..plain.clone() };
        assert_eq!((drawn_pixels(&plain), drawn_pixels(&up), drawn_pixels(&down)), (12., 8., 8.));
        // The DOM declaration and the canvas font, which also measures widths, take the same smaller size, scaled by zoom.
        assert!(font_css(&up, None, false, 2.).contains("font-size:16px"), "{}", font_css(&up, None, false, 2.));
        assert!(canvas_font(&up, None, false, 2.).contains(" 16px "), "{}", canvas_font(&up, None, false, 2.));
        assert!(font_css(&plain, None, false, 2.).contains("font-size:24px"));
        // A superscript rises by a third of the stored size, a subscript falls by an eighth; plain text stays on the baseline.
        assert_eq!((script_shift(&plain, 1.), script_shift(&up, 1.), script_shift(&down, 1.)), (0., -4., 1.5));
        assert_eq!(script_shift(&up, 2.), -8.);
    }
    /// Excel Q1 (excel-answers q1.txt): row heights of whole-cell superscript and subscript against plain text.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_971p_6__a_script_row_asks_for_the_height_excel_measured() {
        let extra = |points: f64, script: &str| {
            let style = TextStyle { pixels: points * 4. / 3., script: Some(script.into()), ..Default::default() };
            script_row_extra(&style)
        };
        // Calibri 11, 20, 36: rows 15 / 17.25 / 18, 26.25 / 29.25 / 30.75, 46.5 / 52.5 / 54 pt = 20 / 23 / 24, 35 / 39 / 41, 62 / 70 / 72 px.
        assert_eq!([extra(11., "superscript"), extra(20., "superscript"), extra(36., "superscript")], [3., 4., 8.]);
        assert_eq!([extra(11., "subscript"), extra(20., "subscript"), extra(36., "subscript")], [4., 6., 10.]);
        // Arial 11: 14.25 / 16.5 pt (a superscript, 3 px more); its subscript row (18.75 pt, 6 px) is not fitted.
        assert_eq!(script_row_extra(&TextStyle { pixels: 11. * 4. / 3., ..Default::default() }), 0.);
    }
}
