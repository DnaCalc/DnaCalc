//! Alignment of the existing cell text span. Workbook facts arrive through calc-host;
//! geometry and overflow remain in the grid render plan.
use super::{autofit, render_plan::PlannedCell};
use crate::cells::ConditionalShown;

pub(super) const STACKED_LINE_CSS: &str = "display:block;flex:none;height:1em;line-height:1;white-space:pre;overflow:hidden";

pub struct TextLine {
    pub text: String,
    pub segments: Option<Vec<String>>,
    pub advances: Option<Vec<f64>>,
    pub css: String,
}
impl TextLine {
    fn plain(text: String) -> Self {
        Self {
            text,
            segments: None,
            advances: None,
            css: String::new(),
        }
    }
}

pub struct TextLayout {
    pub container: String,
    pub span: String,
    pub icon: String,
    pub lines: Vec<TextLine>,
}

pub fn horizontal(cell: &PlannedCell) -> &str {
    match cell.shown.style.horizontal.as_deref() {
        Some("general") | None if cell.shown.style.indent > 0 => "left",
        Some("general") | None => match cell.shown.kind {
            "number" => "right",
            "logical" | "error" => "center",
            _ if cell.shown.provenance == crate::cells::Provenance::Refused => "center",
            _ => "left",
        },
        Some(value) => value,
    }
}

pub fn layout(
    cell: &PlannedCell,
    conditional: &ConditionalShown,
    italic: bool,
    zoom: f64,
) -> TextLayout {
    let mut display_style=cell.shown.style.clone();
    display_style.pixels=cell.paint_pixels;
    let style = &display_style;
    let horizontal = horizontal(cell);
    let vertical = style.vertical.as_deref().unwrap_or("bottom");
    let indent = autofit::indent_pixels(style, Some(conditional), italic, zoom);
    let (left_pad, right_pad) = super::render_plan::text_insets(zoom);
    let (mut left, right) = match horizontal {
        "right" => (left_pad, right_pad + indent),
        "distributed" => (left_pad + indent, right_pad + indent),
        "left" => (left_pad + indent, right_pad),
        _ => (left_pad, right_pad),
    };
    let icon = if conditional.icon.is_some() {
        20. * zoom
    } else {
        0.
    };
    left += icon;
    let available = (cell.rect.w - left - right).max(1.);
    let justify = match horizontal {
        "right" => "flex-end",
        "center" | "centerContinuous" => "center",
        _ => "flex-start",
    };
    let align = match vertical {
        "top" | "justify" => "flex-start",
        "center" => "center",
        _ => "flex-end",
    };
    let inset = if style.rotation == 0 && !style.wrap && !style.shrink
        && !matches!(horizontal, "justify" | "distributed") && !cell.shown.text.contains('\n')
    { zoom } else { 0. };
    let top = if vertical == "top" { inset } else { 0. };
    let bottom = if vertical == "bottom" { inset + cell.baseline_inset } else { 0. };
    let mut out = TextLayout {
        container: format!(
            ";padding-left:{left}px;padding-right:{right}px;padding-top:{top}px;padding-bottom:{bottom}px;justify-content:{justify};align-items:{align}"
        ),
        span: "display:block;flex-shrink:0;white-space:pre".into(),
        icon: format!(
            "position:absolute;left:{}px;{};width:{icon}px;box-sizing:border-box",
            cell.anchor.x - cell.rect.x + left_pad,
            match vertical {
                "top" | "justify" => "top:0",
                "center" | "distributed" => "top:50%;transform:translateY(-50%)",
                _ => "bottom:0",
            }
        ),
        lines: vec![TextLine::plain(if conditional.hide_value {
            String::new()
        } else {
            cell.shown.text.clone()
        })],
    };
    let font = autofit::font_css(style, Some(conditional), italic, zoom);
    if style.rotation == 255 {
        out.lines = graphemes(&out.lines[0].text).into_iter().map(|text|TextLine {
            text, segments: None, advances: None,
            css: STACKED_LINE_CSS.into(),
        }).collect();
        out.span.push_str(";display:flex;flex-direction:column;align-items:center");
        return out;
    }
    if style.rotation != 0 {
        if let Ok((w, h)) = autofit::probe(&font, &out.lines[0].text) {
            let (w, h) = autofit::rotated_size(w, h, style.rotation);
            let x = match horizontal {
                "right" => cell.rect.w - right - w / 2.,
                "center" | "centerContinuous" => cell.anchor.x - cell.rect.x + cell.anchor.w / 2.,
                _ => left + w / 2.,
            };
            let y = match vertical {
                "top" => h / 2.,
                "center" => cell.rect.h / 2.,
                _ => cell.rect.h - h / 2.,
            };
            out.span = format!(
                "position:absolute;left:{x}px;top:{y}px;white-space:pre;transform:translate(-50%,-50%) rotate({}deg);transform-origin:center",
                style.rotation
            );
        }
        return out;
    }
    if matches!(horizontal, "center" | "centerContinuous") {
        // A neighbour may clip just one side. Keep the centre over the physical source cell.
        let offset =
            cell.anchor.x + cell.anchor.w / 2. - cell.rect.x - (left + cell.rect.w - right) / 2.;
        out.span
            .push_str(&format!(";position:relative;left:{offset}px"));
    }
    if horizontal == "fill"
        && !out.lines[0].text.is_empty()
        && let Ok((width, _)) = autofit::probe(&font, &out.lines[0].text)
        && width > 0.
    {
        let repeats = (available / width).floor().max(1.) as usize;
        // Display repetition is bounded independently of the authored text.
        out.lines[0].text = out.lines[0]
            .text
            .repeat(repeats.min(16_384 / out.lines[0].text.len().max(1)).max(1));
    }
    let wrap = style.wrap || matches!(horizontal, "justify" | "distributed");
    if wrap {
        out.span.push_str(&format!(
            ";width:{available}px;white-space:pre-wrap;overflow-wrap:anywhere"
        ));
    }
    if horizontal == "justify" {
        out.span
            .push_str(";text-align:justify;text-align-last:left");
    } else if horizontal == "distributed" {
        out.span
            .push_str(";text-align:justify;text-align-last:justify;text-justify:inter-character");
    } else if style.wrap {
        out.span.push_str(&format!(
            ";text-align:{}",
            match horizontal {
                "right" => "right",
                "center" | "centerContinuous" => "center",
                _ => "left",
            }
        ));
    }
    let vertical_spread = matches!(vertical, "justify" | "distributed");
    if vertical_spread || horizontal == "distributed" {
        if let Ok(lines) = wrapped_lines(
            &font,
            &out.lines[0].text,
            if wrap { available } else { f64::MAX },
        ) {
            out.lines = lines.into_iter().map(TextLine::plain).collect();
        }
        let distributed = vertical == "distributed";
        let height = if vertical_spread && (distributed || out.lines.len() > 1) {
            "height:100%;"
        } else {
            ""
        };
        out.span.push_str(&format!(
            ";{height}display:flex;flex-direction:column;justify-content:{}",
            if distributed {
                "space-around"
            } else {
                "space-between"
            }
        ));
        let count = out.lines.len();
        for (index, line) in out.lines.iter_mut().enumerate() {
            line.css =
                "display:block;flex:none;height:1em;line-height:1;white-space:pre;overflow:hidden"
                    .into();
            if horizontal == "distributed" {
                let segments = distributed_segments(&line.text);
                line.css.push_str(if segments.len() > 1 {
                    ";display:flex;justify-content:space-between"
                } else {
                    ";display:flex;justify-content:center"
                });
                line.segments = Some(segments);
            } else if horizontal == "justify" && index + 1 < count {
                line.css
                    .push_str(";text-align:justify;text-align-last:justify");
            }
        }
    }
    // Excel's single-line top/bottom placement uses the font's measured line box.
    // Keep centred and multi-line placement independent of this single-line correction.
    if !wrap && !out.lines[0].text.contains('\n') && matches!(vertical, "top" | "bottom")
        && let Some((ascent, descent)) = autofit::font_box(style, Some(conditional), italic, zoom)
    {
        let extra = (ascent + descent - style.pixels * zoom) / 2.;
        let shift = if vertical == "top" { extra } else { -extra };
        out.span.push_str(&format!(";position:relative;top:{shift}px"));
    }
    if let Some(advances) = autofit::text_advances(&cell.shown, &out.lines[0].text, zoom) {
        out.lines[0].segments = Some(graphemes(&out.lines[0].text));
        out.lines[0].advances = Some(advances);
        out.lines[0].css.push_str(";display:inline-flex;align-items:baseline");
        out.lines[0].css.push_str(&autofit::segmented_decoration(style, conditional, italic, zoom));
    }
    out
}

/// Space words when there are several and character clusters in a single word.
/// Keep the original text content, including spaces, inside the visual spans. Segmenter keeps
/// combining marks and emoji sequences together; no second copy of the cell value is stored.
pub(super) fn distributed_segments(text: &str) -> Vec<String> {
    let mut words = Vec::<String>::new();
    for word in text.split_inclusive(char::is_whitespace) {
        if word.trim().is_empty() && !words.is_empty() {
            words.last_mut().unwrap().push_str(word);
        } else {
            words.push(word.into());
        }
    }
    if words.iter().filter(|s| !s.trim().is_empty()).count() > 1 {
        return words;
    }
    graphemes(text)
}

/// Shared clusters for stacked text, distributed alignment and Canvas painting.
pub(super) fn graphemes(text: &str) -> Vec<String> {
    use wasm_bindgen::JsCast;
    let options = js_sys::Intl::SegmenterOptions::new();
    options.set_granularity(js_sys::Intl::SegmenterGranularity::Grapheme);
    let segmenter = js_sys::Intl::Segmenter::new(&js_sys::Array::new(), options.as_ref());
    let segments = segmenter.segment(text);
    js_sys::try_iter(segments.as_ref())
        .ok()
        .flatten()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|value| {
            value
                .unchecked_into::<js_sys::Intl::SegmentData>()
                .segment()
                .into()
        })
        .collect()
}

/// Let the browser place graphemes and wrap words in the actual font, then keep its line
/// boundaries when spreading those lines vertically. The probe never changes workbook data.
pub(super) fn wrapped_lines(font: &str, text: &str, width: f64) -> Result<Vec<String>, String> {
    use leptos::prelude::*;
    use wasm_bindgen::JsCast;
    let node = document()
        .create_element("span")
        .map_err(|_| "Text measurement is unavailable")?
        .dyn_into::<web_sys::HtmlElement>()
        .map_err(|_| "Text measurement is unavailable")?;
    node.set_class_name("dc-fit-measure");
    let width = if width.is_finite() && width < 1e10 {
        format!("width:{width}px;white-space:pre-wrap;overflow-wrap:anywhere")
    } else {
        "white-space:pre".into()
    };
    crate::style::css(node.clone().unchecked_into(), format!("{font};{width}"));
    node.set_text_content(Some(text));
    document()
        .body()
        .ok_or("No document")?
        .append_child(&node)
        .map_err(|_| "Text measurement is unavailable")?;
    let result = (|| {
        let Some(child) = node.first_child() else {
            return Ok(vec![String::new()]);
        };
        let range = document()
            .create_range()
            .map_err(|_| "Text measurement is unavailable")?;
        let (mut offset, mut top) = (0, None::<f64>);
        let mut lines = vec![String::new()];
        for ch in text.chars() {
            range
                .set_start(&child, offset)
                .map_err(|_| "Text measurement is unavailable")?;
            offset += ch.len_utf16() as u32;
            range
                .set_end(&child, offset)
                .map_err(|_| "Text measurement is unavailable")?;
            let y = range.get_bounding_client_rect().top();
            if top.is_some_and(|t| (t - y).abs() > 0.5) {
                lines.push(String::new());
            }
            lines.last_mut().unwrap().push(ch);
            top = Some(y);
        }
        Ok(lines)
    })();
    node.remove();
    result
}
