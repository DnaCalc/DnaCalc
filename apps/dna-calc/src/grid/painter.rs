//! Canvas2D paints the same committed plan as DOM. Interaction, semantic rows, headers,
//! charts, review controls and the live editor stay in the shared grid adapter.
use super::{
    alignment, autofit,
    geometry::GridMetrics,
    panes::{self, Panes},
    render_plan::{PlannedCell, RenderPlan},
};
use crate::cells::Provenance;
use leptos::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d as Context, HtmlCanvasElement};

const STORAGE: &str = "dna-grid-painter";
pub(super) fn initial_canvas() -> bool {
    let query = window().location().search().unwrap_or_default();
    if let Some(value) = query
        .trim_start_matches('?')
        .split('&')
        .find_map(|part| part.strip_prefix("renderer="))
    {
        return value == "canvas";
    }
    window()
        .session_storage()
        .ok()
        .flatten()
        .and_then(|s| s.get_item(STORAGE).ok().flatten())
        .as_deref()
        == Some("canvas")
}
pub(super) fn remember(canvas: bool) {
    if let Ok(Some(storage)) = window().session_storage() {
        let _ = storage.set_item(STORAGE, if canvas { "canvas" } else { "dom" });
    }
}
fn font(cell: &PlannedCell, scale: f64) -> String {
    let mut display_style = cell.shown.style.clone();
    display_style.pixels = cell.paint_pixels;
    let style = &display_style;
    let conditional = cell.shown.conditional.as_ref();
    let italic = style.italic
        || conditional.is_some_and(|c| c.italic)
        || matches!(
            cell.shown.provenance,
            Provenance::FileCache | Provenance::Refused
        );
    autofit::canvas_font(style, conditional, italic, scale)
}
fn rect(ctx: &Context, r: super::geometry::CellRect) {
    ctx.rect(r.x, r.y, r.w, r.h);
}
fn token(style: &web_sys::CssStyleDeclaration, name: &str) -> String {
    style
        .get_property_value(name)
        .unwrap_or_default()
        .trim()
        .into()
}
fn width(ctx: &Context, text: &str) -> f64 {
    ctx.measure_text(text).map_or(0., |m| m.width())
}

pub(super) fn paint(
    canvas: &HtmlCanvasElement,
    plan: &RenderPlan,
    panes: Panes,
    metrics: &GridMetrics,
) {
    // The painter fills every pixel, and only an opaque context gets the LCD text
    // antialiasing the DOM painter and Excel's raster have.
    let options = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&options, &"alpha".into(), &false.into());
    let Some(ctx) = canvas
        .get_context_with_context_options("2d", &options)
        .ok()
        .flatten()
        .and_then(|v| v.dyn_into::<Context>().ok())
    else {
        return;
    };
    let dpr = window().device_pixel_ratio().max(1.);
    let (w, h) = (
        (plan.viewport.width * dpr).ceil() as u32,
        (plan.viewport.height * dpr).ceil() as u32,
    );
    if canvas.width() != w {
        canvas.set_width(w)
    }
    if canvas.height() != h {
        canvas.set_height(h)
    }
    let _ = ctx.set_transform(dpr, 0., 0., dpr, 0., 0.);
    let Some(style) = window().get_computed_style(canvas).ok().flatten() else {
        return;
    };
    let paper = token(&style, "--dna-paper");
    ctx.set_fill_style_str(&paper);
    ctx.fill_rect(0., 0., plan.viewport.width, plan.viewport.height);
    ctx.set_fill_style_str(&token(&style, "--dc-gridline"));
    for col in &plan.col_headers {
        ctx.fill_rect(col.rect.x + col.rect.w - 1., 0., 1., plan.viewport.height)
    }
    for row in &plan.row_headers {
        ctx.fill_rect(0., row.rect.y + row.rect.h - 1., plan.viewport.width, 1.)
    }
    for cell in &plan.cells {
        let Some(clip) = cell.clip.or_else(|| {
            panes::intersection(
                cell.rect,
                panes.bounds(metrics, &plan.viewport, cell.row, cell.col),
            )
        }) else {
            continue;
        };
        ctx.save();
        ctx.begin_path();
        rect(&ctx, clip);
        ctx.clip();
        let conditional = cell.shown.conditional.clone().unwrap_or_default();
        let background = conditional
            .fill
            .as_deref()
            .or(cell.shown.style.fill.as_deref())
            .map(str::to_owned)
            .or_else(|| match cell.shown.provenance {
                Provenance::Stale => Some(token(&style, "--dna-amber-soft")),
                Provenance::Refused => Some(token(&style, "--dna-paper")),
                _ if cell.row_span > 1 || cell.col_span > 1 => Some(paper.clone()),
                _ => None,
            });
        if let Some(fill) = background {
            ctx.set_fill_style_str(&fill);
            ctx.fill_rect(cell.anchor.x, cell.anchor.y, cell.anchor.w, cell.anchor.h)
        }
        if conditional.fill.is_none()
            && let Some(brush) = &cell.shown.style.brush
            && let Some((texture, _)) =
                super::decoration::texture(brush, cell.anchor.w, cell.anchor.h, plan.scale)
        {
            if matches!(brush, dna_calc_model::projection::FillShown::Pattern { .. }) {
                if let Ok(Some(pattern)) =
                    ctx.create_pattern_with_html_canvas_element(&texture, "repeat")
                {
                    ctx.save();
                    // Keep the source mask's discrete foreground/background pixels at
                    // fractional scroll offsets; gradients keep interpolation below.
                    ctx.set_image_smoothing_enabled(false);
                    let _ = ctx.translate(cell.anchor.x, cell.anchor.y);
                    let unit = 8. / f64::from(texture.width());
                    let _ = ctx.scale(unit, unit);
                    ctx.set_fill_style_canvas_pattern(&pattern);
                    ctx.fill_rect(0., 0., cell.anchor.w / unit, cell.anchor.h / unit);
                    ctx.restore();
                }
            } else {
                let _ = ctx.draw_image_with_html_canvas_element_and_dw_and_dh(
                    &texture,
                    cell.anchor.x,
                    cell.anchor.y,
                    cell.anchor.w,
                    cell.anchor.h,
                );
            }
        }
        if (cell.row_span > 1 || cell.col_span > 1)
            && conditional.fill.is_none()
            && cell.shown.style.fill.is_none()
            && cell.shown.style.brush.is_none()
        {
            ctx.set_fill_style_str(&token(&style, "--dc-gridline"));
            ctx.fill_rect(
                cell.rect.x + cell.rect.w - 1.0,
                cell.rect.y,
                1.0,
                cell.rect.h,
            );
            ctx.fill_rect(
                cell.rect.x,
                cell.rect.y + cell.rect.h - 1.0,
                cell.rect.w,
                1.0,
            );
        }
        if cell.shown.provenance == Provenance::Refused {
            ctx.set_stroke_style_str(&token(&style, "--dna-red-soft"));
            ctx.set_line_width(6.);
            let mut x = cell.rect.x - cell.rect.h;
            while x < cell.rect.x + cell.rect.w {
                ctx.begin_path();
                ctx.move_to(x, cell.rect.y + cell.rect.h);
                ctx.line_to(x + cell.rect.h, cell.rect.y);
                ctx.stroke();
                x += 12.;
            }
        }
        if let Some((share, color, gradient)) = &conditional.bar {
            if *gradient {
                let g = ctx.create_linear_gradient(
                    cell.rect.x,
                    0.,
                    cell.rect.x + cell.rect.w * share,
                    0.,
                );
                let _ = g.add_color_stop(0., color);
                let _ = g.add_color_stop(1., "#ffffff");
                ctx.set_fill_style_canvas_gradient(&g);
            } else {
                ctx.set_fill_style_str(color)
            }
            ctx.fill_rect(
                cell.rect.x,
                cell.rect.y + cell.rect.h * 0.14,
                cell.rect.w * share,
                cell.rect.h * 0.72,
            );
        }
        ctx.restore();
    }
    // Paint all glyphs after all physical backgrounds. An adjacent styled blank must not
    // erase an earlier cell's overflowing text, and merged fragments use their own clip.
    for cell in &plan.cells {
        let Some(clip) = panes.text_clip(cell, metrics, &plan.viewport) else {
            continue;
        };
        ctx.save();
        ctx.begin_path();
        rect(&ctx, clip);
        ctx.clip();
        let conditional = cell.shown.conditional.clone().unwrap_or_default();
        let color = conditional
            .font_color
            .as_ref()
            .or(cell.shown.color.as_ref())
            .or(cell.shown.style.font_color.as_ref())
            .cloned()
            .unwrap_or_else(|| {
                token(
                    &style,
                    match cell.shown.provenance {
                        Provenance::Refused => "--dna-red-ink",
                        Provenance::FileCache => "--dna-ink-3",
                        Provenance::Stale => "--dna-prov-ext",
                        _ if cell.shown.kind == "error" => "--dna-red-ink",
                        _ => "--dna-ink",
                    },
                )
            });
        ctx.set_fill_style_str(&color);
        ctx.set_stroke_style_str(&color);
        ctx.set_font(&font(cell, plan.scale));
        text(&ctx, cell, plan.scale);
        ctx.restore();
    }
    for cell in &plan.cells {
        let r = cell.anchor;
        let expanded = cell.row_span == 1 && cell.col_span == 1;
        let border_rect = super::decoration::border_rect(cell, plan.scale);
        let Some(clip) = panes::intersection(
            border_rect,
            (if expanded { None } else { cell.clip })
                .unwrap_or_else(|| panes.bounds(metrics, &plan.viewport, cell.row, cell.col)),
        ) else {
            continue;
        };
        ctx.save();
        ctx.begin_path();
        rect(&ctx, clip);
        ctx.clip();
        let shift = super::decoration::dash_shift(metrics);
        for line in super::decoration::edges(cell, plan.scale, shift)
            .into_iter()
            .chain(super::decoration::strokes(cell, plan.scale, shift))
        {
            ctx.set_stroke_style_str(&line.color);
            ctx.set_line_width(line.width);
            let dashes = js_sys::Array::new();
            for n in line.dash {
                dashes.push(&wasm_bindgen::JsValue::from_f64(n));
            }
            let _ = ctx.set_line_dash(&dashes);
            ctx.set_line_dash_offset(line.phase);
            ctx.begin_path();
            ctx.move_to(r.x + line.x1, r.y + line.y1);
            ctx.line_to(r.x + line.x2, r.y + line.y2);
            ctx.stroke();
        }
        ctx.restore();
    }
    // Inspection is metadata on the real surface, not another value source. A later rAF is
    // the benchmark's paint opportunity; this marker only says draw calls have completed.
    let _ = canvas.set_attribute(
        "data-draw-revision",
        &plan.publication.map_or(0, |s| s.next).to_string(),
    );
    let _ = canvas.set_attribute(
        "data-draw-presentation",
        &plan.presentation.epoch.to_string(),
    );
}
fn text(ctx: &Context, cell: &PlannedCell, scale: f64) {
    let mut display_style = cell.shown.style.clone();
    display_style.pixels = cell.paint_pixels;
    let s = &display_style;
    let c = cell.shown.conditional.clone().unwrap_or_default();
    if c.hide_value {
        return;
    }
    let italic = matches!(
        cell.shown.provenance,
        Provenance::FileCache | Provenance::Refused
    );
    let indent = autofit::indent_pixels(s, Some(&c), italic, scale);
    let horizontal = alignment::horizontal(cell);
    let vertical = s.vertical.as_deref().unwrap_or("bottom");
    let (left_pad, right_pad) = super::render_plan::text_insets(scale);
    let icon = if c.icon.is_some() { 20. * scale } else { 0. };
    let (left, right) = match horizontal {
        "right" => (left_pad + icon, right_pad + indent),
        "distributed" => (left_pad + icon + indent, right_pad + indent),
        "left" => (left_pad + icon + indent, right_pad),
        _ => (left_pad + icon, right_pad),
    };
    let available = (cell.rect.w - left - right).max(1.);
    let mut shown = cell.shown.text.clone();
    if horizontal == "fill" && s.rotation != 255 && !shown.is_empty() {
        let w = width(ctx, &shown);
        if w > 0. {
            shown = shown.repeat(
                ((available / w).floor().max(1.) as usize)
                    .min(16_384 / shown.len().max(1))
                    .max(1),
            );
        }
    }
    let wrap = s.wrap || matches!(horizontal, "justify" | "distributed");
    let stacked = s.rotation == 255;
    let lines = if stacked {
        alignment::graphemes(&shown)
    } else if wrap && s.rotation == 0 {
        alignment::wrapped_lines(
            &autofit::font_css(s, Some(&c), italic, scale),
            &shown,
            available,
        )
        .unwrap_or_else(|_| vec![shown.clone()])
    } else {
        shown.split('\n').map(str::to_owned).collect()
    };
    let line_height = s.pixels * scale;
    let height = line_height * lines.len() as f64;
    let inset = if s.rotation == 0 && !wrap && !s.shrink && !shown.contains('\n') {
        scale
    } else {
        0.
    };
    let top = match vertical {
        "top" => cell.rect.y + inset,
        "justify" => cell.rect.y,
        "center" => cell.rect.y + (cell.rect.h - height) / 2.,
        "bottom" => cell.rect.y + cell.rect.h - height - inset - cell.baseline_inset,
        _ => cell.rect.y + cell.rect.h - height,
    };
    ctx.set_text_align("left");
    ctx.set_text_baseline("alphabetic");
    let measure = ctx.measure_text("Mg").ok();
    let (ascent, descent) = measure
        .as_ref()
        .map_or((line_height * 0.8, line_height * 0.2), |m| {
            (m.font_bounding_box_ascent(), m.font_bounding_box_descent())
        });
    let baseline = if ascent.is_finite() && descent.is_finite() {
        (line_height - ascent - descent) / 2. + ascent
    } else {
        line_height * 0.8
    };
    let shift = if s.rotation == 0
        && !wrap
        && !shown.contains('\n')
        && ascent.is_finite()
        && descent.is_finite()
    {
        let extra = (ascent + descent - line_height) / 2.;
        match vertical {
            "top" => extra,
            "bottom" => -extra,
            _ => 0.,
        }
    } else {
        0.
    };
    let double = autofit::double_underline(s, Some(&c));
    let integer = autofit::integer_text(&cell.shown, &shown);
    let advance = |line: &str| -> f64 {
        if integer {
            alignment::graphemes(line)
                .into_iter()
                .map(|part| {
                    autofit::integer_advance(s, Some(&c), italic, scale, &part, width(ctx, &part))
                })
                .sum()
        } else {
            width(ctx, line)
        }
    };
    let decorate = |x: f64, y: f64, advance: f64| {
        if c.underline || c.strike || s.underline || s.strike {
            ctx.set_line_width(scale);
            let stroke = if double {
                autofit::underline_width(s.pixels, scale)
            } else {
                scale
            };
            ctx.set_line_width(stroke);
            let under = baseline + stroke;
            for offset in [
                (c.underline || s.underline).then_some(under),
                double.then_some(under + 2. * stroke),
                (c.strike || s.strike).then_some(line_height / 2.),
            ]
            .into_iter()
            .flatten()
            {
                ctx.begin_path();
                ctx.move_to(x, y + offset);
                ctx.line_to(x + advance, y + offset);
                ctx.stroke();
            }
        }
    };
    let block_width = lines
        .iter()
        .map(|line| advance(line.trim_end_matches('\n')))
        .fold(0., f64::max);
    for (i, line) in lines.iter().enumerate() {
        let line = line.trim_end_matches('\n');
        let w = advance(line);
        let x = if stacked {
            let left_edge = match horizontal {
                "right" => cell.rect.x + cell.rect.w - right - block_width,
                "center" | "centerContinuous" => cell.anchor.x + (cell.anchor.w - block_width) / 2.,
                _ => cell.rect.x + left,
            };
            left_edge + (block_width - w) / 2.
        } else {
            match horizontal {
                "right" => cell.rect.x + cell.rect.w - right - w,
                "center" | "centerContinuous" => cell.anchor.x + cell.anchor.w / 2. - w / 2.,
                _ => cell.rect.x + left,
            }
        };
        let y = shift
            + if stacked {
                top + i as f64 * line_height
            } else {
                match vertical {
                    "distributed" => {
                        cell.rect.y
                            + (cell.rect.h - height) / (lines.len() as f64 * 2.)
                            + i as f64 * (cell.rect.h / lines.len() as f64)
                    }
                    "justify" if lines.len() > 1 => {
                        cell.rect.y
                            + i as f64 * (cell.rect.h - line_height) / (lines.len() - 1) as f64
                    }
                    _ => top + i as f64 * line_height,
                }
            };
        ctx.save();
        if s.rotation != 0 && !stacked {
            let (rw, rh) = autofit::rotated_size(block_width, height, s.rotation);
            let cx = match horizontal {
                "right" => cell.rect.x + cell.rect.w - right - rw / 2.,
                "center" | "centerContinuous" => cell.anchor.x + cell.anchor.w / 2.,
                _ => cell.rect.x + left + rw / 2.,
            };
            let cy = match vertical {
                "top" => cell.rect.y + rh / 2.,
                "center" => cell.rect.y + cell.rect.h / 2.,
                _ => cell.rect.y + cell.rect.h - rh / 2.,
            };
            let _ = ctx.translate(cx, cy);
            let _ = ctx.rotate(f64::from(s.rotation).to_radians());
            let decoration_x = -block_width / 2.;
            let decoration_y = -height / 2. + i as f64 * line_height;
            let _ = ctx.fill_text(line, decoration_x, decoration_y + baseline);
            decorate(decoration_x, decoration_y, w);
        } else if !stacked
            && (horizontal == "distributed" || (horizontal == "justify" && i + 1 < lines.len()))
        {
            let parts = if horizontal == "distributed" {
                alignment::distributed_segments(line)
            } else {
                line.split_inclusive(char::is_whitespace)
                    .map(str::to_owned)
                    .collect()
            };
            let used: f64 = parts.iter().map(|p| width(ctx, p)).sum();
            let gap = if parts.len() > 1 {
                (available - used) / (parts.len() - 1) as f64
            } else {
                0.
            };
            let mut px = if parts.len() == 1 && horizontal == "distributed" {
                cell.rect.x + left + (available - used) / 2.
            } else {
                cell.rect.x + left
            };
            for part in parts {
                let advance = width(ctx, &part);
                let _ = ctx.fill_text(&part, px, y + baseline);
                if horizontal == "distributed" {
                    // DOM distributes separate grapheme spans. Their decorations stay
                    // beneath each span, including a single centred grapheme.
                    decorate(px, y, advance);
                }
                px += advance + gap;
            }
            if horizontal == "justify" {
                decorate(x, y, w);
            }
        } else if integer {
            // Use the nearest baseline pixel; an exact half-pixel belongs to the
            // preceding pixel, as in the DOM line box. Other fractions round normally.
            let paint_y = if matches!(vertical, "top" | "bottom") {
                (y + baseline - 0.5).ceil() - baseline
            } else {
                y
            };
            let mut pen = x;
            for part in alignment::graphemes(line) {
                let _ = ctx.fill_text(&part, pen, paint_y + baseline);
                pen +=
                    autofit::integer_advance(s, Some(&c), italic, scale, &part, width(ctx, &part));
            }
            decorate(x, paint_y, w);
        } else {
            let _ = ctx.fill_text(line, x, y + baseline);
            decorate(x, y, w);
        }
        ctx.restore();
    }
}

/// Theme attributes and system colour changes invalidate raster colours too. Ownership is
/// scoped to the pane; leaving a skin disconnects the observer and the media listener.
pub(super) fn watch_colors() -> RwSignal<u64> {
    let revision = RwSignal::new(0u64);
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::closure::Closure;
        let changed =
            Closure::<dyn FnMut(js_sys::Array)>::new(move |_| revision.update(|n| *n += 1));
        if let Ok(observer) = web_sys::MutationObserver::new(changed.as_ref().unchecked_ref()) {
            let opts = web_sys::MutationObserverInit::new();
            opts.set_attributes(true);
            opts.set_attribute_filter(&js_sys::Array::of1(&"data-dna-theme".into()));
            if let Some(root) = document().document_element() {
                let _ = observer.observe_with_options(&root, &opts);
            }
            let held = StoredValue::new_local((observer, changed));
            on_cleanup(move || held.with_value(|(observer, _)| observer.disconnect()));
        }
        if let Ok(Some(media)) = window().match_media("(prefers-color-scheme: dark)") {
            let changed = Closure::<dyn FnMut()>::new(move || revision.update(|n| *n += 1));
            let _ =
                media.add_event_listener_with_callback("change", changed.as_ref().unchecked_ref());
            let held = StoredValue::new_local((media, changed));
            on_cleanup(move || {
                held.with_value(|(media, changed)| {
                    let _ = media.remove_event_listener_with_callback(
                        "change",
                        changed.as_ref().unchecked_ref(),
                    );
                })
            });
        }
    }
    revision
}
