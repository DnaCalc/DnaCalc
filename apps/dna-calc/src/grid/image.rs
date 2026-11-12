//! Pictures use the same cell-image request in the browser and desktop workbook transports.
use super::{
    geometry::{GridMetrics, Viewport},
    panes::Panes,
    render_plan::RenderPlan,
};
use crate::style::css;
use crate::{controller::Controller, wire};
use leptos::prelude::*;
use std::{cell::Cell, rc::Rc};

/// Draw pictures above either grid painter. Ignore replies after the view has gone.
pub(super) fn cells(
    controller: Controller,
    plan: &RenderPlan,
    panes: Panes,
    metrics: &GridMetrics,
    viewport: &Viewport,
) -> impl IntoView + use<> {
    plan.cells.iter().filter(|cell| cell.shown.kind == "image").map(|cell| {
        let rect = cell.anchor;
        let style = format!(
            "position:absolute;left:{}px;top:{}px;width:{}px;height:{}px;overflow:hidden;display:flex;align-items:center;justify-content:center;pointer-events:none{}",
            rect.x, rect.y, rect.w, rect.h, panes.anchor_clip_style(cell, metrics, viewport)
        );
        let image = RwSignal::new(None::<(String, wire::CellImage)>);
        let alive = Rc::new(Cell::new(true));
        let callback_alive = alive.clone();
        let held = StoredValue::new_local(alive);
        controller.call(wire::Request::CellImage { at: wire::Key::of(cell.shown.at) }, None, move |reply, bytes| {
            let (Ok(wire::Reply::CellImage(Some(info))), Some(bytes)) = (reply, bytes) else { return };
            if !callback_alive.get() { return; }
            let src = data_url(&info.content_type, &bytes);
            image.set(Some((src, info)));
        });
        on_cleanup(move || {
            held.with_value(|alive| alive.set(false));
        });
        let scale = plan.scale;
        view! {
            <div data-dnacalc="cell-image" data-cell=cell.address.clone() use:css=style>
                {move || image.get().map(|(src, info)| {
                    let image_style = match info.sizing {
                        0 => "width:100%;height:100%;object-fit:contain".to_owned(),
                        1 => "width:100%;height:100%;object-fit:fill".to_owned(),
                        2 => format!("max-width:none;max-height:none;transform:scale({scale})"),
                        3 => format!("max-width:none;max-height:none;width:{};height:{}", dimension(info.width, scale), dimension(info.height, scale)),
                        _ => String::new(),
                    };
                    view! { <img src=src alt=info.alt_text.unwrap_or_default() use:css=format!("flex:none;{image_style}") /> }
                })}
            </div>
        }
    }).collect_view()
}

fn dimension(value: Option<f64>, scale: f64) -> String {
    value
        .filter(|n| *n > 0.0 && n.is_finite())
        .map_or_else(|| "auto".into(), |n| format!("{}px", n * scale))
}

// Site and desktop policies allow data images. Encoding locally never makes a network request.
fn data_url(content_type: &str, bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = format!("data:{content_type};base64,");
    out.reserve(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let a = chunk[0];
        let b = chunk.get(1).copied().unwrap_or(0);
        let c = chunk.get(2).copied().unwrap_or(0);
        out.push(TABLE[(a >> 2) as usize] as char);
        out.push(TABLE[(((a & 3) << 4) | (b >> 4)) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(((b & 15) << 2) | (c >> 6)) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(c & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}
