//! One-shot content fitting for the grid's current, unwrapped text presentation.
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

/// Measure the same CSS font and conditional weight/style the visible grid uses, at 100%.
/// Text is assigned as textContent; geometry declarations go through the CSS object model.
pub fn measure(rows: bool, cells: &[wire::Cell]) -> Result<Vec<(u32, u64)>, String> {
    use leptos::prelude::*;
    use wasm_bindgen::JsCast;
    let layer = document()
        .query_selector(".dc-layer")
        .map_err(|_| "The grid is unavailable")?
        .ok_or("Return to the grid before fitting contents")?;
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
        probe.set_text_content(Some(text));
        let bold = conditional.is_some_and(|c| c.bold);
        let italic = conditional.is_some_and(|c| c.italic)
            || matches!(
                cell.provenance,
                crate::cells::Provenance::FileCache | crate::cells::Provenance::Refused
            );
        crate::style::css(
            probe.clone().unchecked_into(),
            format!(
                "font-size:13px;font-weight:{};font-style:{}",
                if bold { 700 } else { 400 },
                if italic { "italic" } else { "normal" }
            ),
        );
        let rect = probe.get_bounding_client_rect();
        let index = if rows { cell.at.2 } else { cell.at.3 };
        let pixels = if rows {
            rect.height() + 4.0
        } else {
            let visible = rect.width()
                + 8.0
                + if conditional.is_some_and(|c| c.icon.is_some()) {
                    20.0
                } else {
                    0.0
                };
            if cell.kind == "number" {
                visible.max(text.chars().count() as f64 * 7.0 + 5.0)
            } else {
                visible
            }
        };
        samples.push((index, pixels));
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
