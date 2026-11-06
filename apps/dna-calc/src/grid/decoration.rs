//! The two painters share the line weight and dash vocabulary of stored cell borders.
use dna_calc_model::projection::BorderShown;

pub struct Line {
    pub width: f64,
    pub css: &'static str,
    pub dash: &'static [f64],
    pub double: bool,
}
// The captured 75/100/150% views use the same 96-DPI pen widths.
pub fn line(edge: &BorderShown, _scale: f64) -> Line {
    let (width, css, dash, double) = match edge.style.as_str() {
        "hair" => (1., "dotted", &[1., 1.][..], false),
        "medium" => (2., "solid", &[][..], false),
        "thick" => (3., "solid", &[][..], false),
        "double" => (3., "double", &[][..], true),
        "dotted" => (1., "dotted", &[2., 2.][..], false),
        "dashed" => (1., "dashed", &[3., 1.][..], false),
        "mediumDashed" => (2., "dashed", &[9., 3.][..], false),
        "dashDot" => (1., "dashed", &[9., 3., 3., 3.][..], false),
        "dashDotDot" => (1., "dashed", &[9., 3., 3., 3., 3., 3.][..], false),
        "mediumDashDot" => (2., "dashed", &[9., 3., 3., 3.][..], false),
        "mediumDashDotDot" | "slantDashDot" => (2., "dashed", &[9., 3., 3., 3., 3., 3.][..], false),
        _ => (1., "solid", &[][..], false),
    };
    Line {
        width,
        css,
        dash,
        double,
    }
}
pub fn css(cell: &dna_calc_model::render_plan::PlannedCell, scale: f64) -> String {
    let mut css = String::new();
    for (side, edge) in ["left", "right", "top", "bottom"].into_iter().zip(&cell.borders) {
        if let Some(edge) = edge {
            let line = line(edge, scale);
            if cell.row_span == 1 && cell.col_span == 1 && !line.dash.is_empty() {
                continue; // The SVG and Canvas strokes use the same physical dash lengths.
            }
            css.push_str(&format!(
                ";border-{side}:{}px {} {}",
                line.width,
                line.css,
                edge.color.as_deref().unwrap_or("#000000")
            ));
        }
    }
    css
}
/// Thin/medium pens end at the boundary. Thick and double pens straddle its last pixel.
/// Keep the original anchor for text, hit testing and merged fragment clipping.
pub fn border_rect(cell: &dna_calc_model::render_plan::PlannedCell, scale: f64) -> dna_calc_model::geometry::CellRect {
    let mut rect = cell.anchor;
    if cell.row_span == 1 && cell.col_span == 1 {
        let widths = cell.borders.each_ref().map(|e|e.as_ref().map_or(0.,|e|line(e,scale).width));
        let leading = |width:f64| if width == 3. {2.}else{width};
        let trailing = |width:f64| if width == 3. {1.}else{0.};
        let left = leading(widths[0]);
        let top = leading(widths[2]);
        rect.x -= left; rect.w += left;
        rect.y -= top; rect.h += top;
        rect.w += trailing(widths[1]);
        rect.h += trailing(widths[3]);
    }
    rect
}

/// Selection pens are two display pixels at every zoom: outside the leading edges,
/// and across the final grid pixel on the trailing edges.
pub fn selection_rect(mut rect:dna_calc_model::geometry::CellRect)->dna_calc_model::geometry::CellRect {
    rect.x-=2.;rect.y-=2.;rect.w+=3.;rect.h+=3.;rect
}

/// Shade a selected pane fragment without tinting its active cell or its text.
pub fn selection_shades(rect:dna_calc_model::geometry::CellRect, active:dna_calc_model::geometry::CellRect)->Vec<dna_calc_model::geometry::CellRect> {
    use dna_calc_model::geometry::CellRect;
    let Some(hole)=super::panes::intersection(rect,active)else{return vec![rect]};
    [CellRect{x:rect.x,y:rect.y,w:rect.w,h:hole.y-rect.y},
     CellRect{x:rect.x,y:hole.y+hole.h,w:rect.w,h:rect.y+rect.h-hole.y-hole.h},
     CellRect{x:rect.x,y:hole.y,w:hole.x-rect.x,h:hole.h},
     CellRect{x:hole.x+hole.w,y:hole.y,w:rect.x+rect.w-hole.x-hole.w,h:hole.h}]
        .into_iter().filter(|r|r.w>0.&&r.h>0.).collect()
}

use dna_calc_model::projection::FillShown;
/// A fixed tile for the OOXML pattern vocabulary. Patterns keep physical pixel size at zoom.
fn pattern(name: &str, x: u32, y: u32) -> bool {
    let (x, y) = (x % 8, y % 8);
    match name {
        "darkGray" => (x + y) % 2 == 0 || y % 2 == 0,
        "mediumGray" => (x + y) % 2 == 0,
        "lightGray" => x % 2 == 0 && y % 2 == 0,
        "gray125" => (x + 4 * (y % 2)) % 8 == 0,
        "gray0625" => y % 2 == 0 && (x + 2 * (y % 4)) % 8 == 0,
        "darkHorizontal" => y % 4 < 2,
        "lightHorizontal" => y % 4 == 0,
        "darkVertical" => x % 4 < 2,
        "lightVertical" => x % 4 == 0,
        "darkDown" => (x + 8 - y) % 8 < 3,
        "lightDown" => (x + 8 - y) % 8 == 0,
        "darkUp" => (x + y) % 8 < 3,
        "lightUp" => (x + y) % 8 == 0,
        "darkGrid" => x % 8 < 2 || y % 8 < 2,
        "lightGrid" => x % 4 == 0 || y % 4 == 0,
        "darkTrellis" => (x + y) % 8 < 2 || (x + 8 - y) % 8 < 2,
        "lightTrellis" => (x + y) % 8 == 0 || (x + 8 - y) % 8 == 0,
        _ => false,
    }
}
fn rgb(text: &str) -> [u8; 3] {
    let text = text.strip_prefix('#').unwrap_or(text);
    if text.len() != 6 {
        return [0, 0, 0];
    }
    [0, 2, 4].map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap_or(0))
}
fn mix(a: [u8; 3], b: [u8; 3], t: f64) -> [u8; 3] {
    [0, 1, 2].map(|i| (f64::from(a[i]) * (1. - t) + f64::from(b[i]) * t).round() as u8)
}
fn sample_gradient(stops: &[(f64, [u8; 3])], t: f64) -> [u8; 3] {
    if stops.is_empty() {
        return [0, 0, 0];
    }
    if t < stops[0].0 {
        return stops[0].1;
    }
    for pair in stops.windows(2) {
        if t < pair[1].0 {
            return mix(
                pair[0].1,
                pair[1].1,
                ((t - pair[0].0) / (pair[1].0 - pair[0].0)).clamp(0., 1.),
            );
        }
    }
    stops.last().unwrap().1
}
fn gradient_position(kind: &str, degree: f64, r: [f64; 4], x: f64, y: f64, w: f64, h: f64) -> f64 {
    if kind == "path" {
        let distance = |p: f64, a: f64, b: f64| {
            if p < a {
                (a - p) / a.max(f64::EPSILON)
            } else if p > b {
                (p - b) / (1. - b).max(f64::EPSILON)
            } else {
                0.
            }
        };
        distance(x / w, r[0], r[1])
            .max(distance(y / h, r[2], r[3]))
            .clamp(0., 1.)
    } else {
        let (sin, cos) = degree.to_radians().sin_cos();
        (((x - w / 2.) * cos + (y - h / 2.) * sin) / (w * cos.abs() + h * sin.abs()) + 0.5)
            .clamp(0., 1.)
    }
}

#[derive(Clone)]
struct Texture {
    brush: FillShown,
    w: u32,
    h: u32,
    scale: f64,
    width: f64,
    height: f64,
    canvas: web_sys::HtmlCanvasElement,
    url: String,
}
thread_local! {static TEXTURES:std::cell::RefCell<std::collections::VecDeque<Texture>>=Default::default();}
/// One bounded texture is shared by DOM and Canvas, including path-gradient interpolation.
/// Cached pixels follow visible physical cells and never the size of a selected range.
pub fn texture(
    brush: &FillShown,
    width: f64,
    height: f64,
    scale: f64,
) -> Option<(web_sys::HtmlCanvasElement, String)> {
    use wasm_bindgen::{Clamped, JsCast};
    let (width, height) = if matches!(brush, FillShown::Pattern { .. }) {
        (8., 8.)
    } else {
        (width, height)
    };
    let dpr = leptos::prelude::window().device_pixel_ratio().max(1.);
    let (w, h) = (
        (width * dpr).ceil().clamp(1., 1024.) as u32,
        (height * dpr).ceil().clamp(1., 1024.) as u32,
    );
    if let Some(hit) = TEXTURES.with(|cache| {
        cache
            .borrow()
            .iter()
            .find(|t| {
                t.brush == *brush
                    && t.w == w
                    && t.h == h
                    && t.scale == scale
                    && t.width == width
                    && t.height == height
            })
            .cloned()
    }) {
        return Some((hit.canvas, hit.url));
    }
    let canvas = leptos::prelude::document()
        .create_element("canvas")
        .ok()?
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .ok()?;
    canvas.set_width(w);
    canvas.set_height(h);
    let ctx = canvas
        .get_context("2d")
        .ok()??
        .dyn_into::<web_sys::CanvasRenderingContext2d>()
        .ok()?;
    let stops = match brush {
        FillShown::Gradient { stops, .. } => {
            stops.iter().map(|(p, c)| (*p, rgb(c))).collect::<Vec<_>>()
        }
        _ => Vec::new(),
    };
    let mut pixels = vec![255u8; (w * h * 4) as usize];
    let colors = match brush {
        FillShown::Pattern {
            foreground,
            background,
            ..
        } => Some((rgb(foreground), rgb(background))),
        _ => None,
    };
    for y in 0..h {
        for x in 0..w {
            let color = match brush {
                FillShown::Pattern { name, .. } => {
                    let (fg, bg) = colors.unwrap();
                    if pattern(
                        name,
                        (f64::from(x) * width / f64::from(w)).floor() as u32,
                        (f64::from(y) * height / f64::from(h)).floor() as u32,
                    ) {
                        fg
                    } else {
                        bg
                    }
                }
                FillShown::Gradient {
                    kind,
                    degree,
                    rectangle,
                    ..
                } => sample_gradient(
                    &stops,
                    gradient_position(
                        kind,
                        *degree,
                        *rectangle,
                        (f64::from(x) + 0.5) * width / f64::from(w),
                        (f64::from(y) + 0.5) * height / f64::from(h),
                        width,
                        height,
                    ),
                ),
            };
            let i = ((y * w + x) * 4) as usize;
            pixels[i..i + 3].copy_from_slice(&color);
        }
    }
    let data = web_sys::ImageData::new_with_u8_clamped_array_and_sh(Clamped(&pixels), w, h).ok()?;
    ctx.put_image_data(&data, 0., 0.).ok()?;
    let url = canvas.to_data_url().ok()?;
    TEXTURES.with(|cache| {
        let mut cache = cache.borrow_mut();
        while cache.len() >= 64
            || cache
                .iter()
                .map(|t| u64::from(t.w) * u64::from(t.h))
                .sum::<u64>()
                + u64::from(w) * u64::from(h)
                > 2_000_000
        {
            if cache.pop_front().is_none() {
                break;
            }
        }
        cache.push_back(Texture {
            brush: brush.clone(),
            w,
            h,
            scale,
            width,
            height,
            canvas: canvas.clone(),
            url: url.clone(),
        });
    });
    Some((canvas, url))
}

#[cfg(test)]
mod brush_tests {
    use super::*;
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn selection_shading_excludes_the_active_cell_in_each_fragment() {
        use dna_calc_model::geometry::CellRect;
        let rect=CellRect{x:10.,y:20.,w:100.,h:80.};
        for active in [CellRect{x:30.,y:40.,w:20.,h:15.},CellRect{x:0.,y:0.,w:20.,h:30.},CellRect{x:200.,y:200.,w:20.,h:20.}] {
            let shades=selection_shades(rect,active);
            let hole=super::super::panes::intersection(rect,active).map_or(0.,|r|r.w*r.h);
            assert_eq!(shades.iter().map(|r|r.w*r.h).sum::<f64>(),rect.w*rect.h-hole);
            for (i,r) in shades.iter().enumerate() {
                assert!(super::super::panes::intersection(*r,active).is_none());
                for other in &shades[i+1..] {assert!(super::super::panes::intersection(*r,*other).is_none());}
            }
        }
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn captured_pen_extents_keep_their_physical_width_at_each_zoom() {
        use dna_calc_model::{geometry::{CellRect,GridMetrics,Viewport},projection::{CellShown,FormatShown,Provenance},render_plan::build_render_plan};
        let mut ids=calc_value::IdAllocator::<calc_value::SheetId>::new();
        let shown=CellShown{at:calc_value::CellKey::new(ids.issue().unwrap(),1,1).unwrap(),style:Default::default(),text:String::new(),kind:"",provenance:Provenance::Empty,formula:false,format:FormatShown::General,color:None,conditional:None};
        let mut cell=build_render_plan(&[shown],&GridMetrics::default(),&Viewport{scroll_x:0.,scroll_y:0.,width:500.,height:300.},10,10).cells.remove(0);
        cell.anchor=CellRect{x:30.,y:40.,w:100.,h:30.};
        for scale in [0.75,1.,1.5] {
            for (style,low,high,width) in [("thin",-1.,0.,1.),("medium",-2.,0.,2.),("thick",-2.,1.,3.),("double",-2.,1.,2.)] {
                cell.borders=[Some(BorderShown{style:style.into(),color:Some("#154c94".into())}),None,None,None];
                let strokes=edges(&cell,scale);
                let lo=strokes.iter().map(|s|s.x1-s.width/2.).fold(f64::INFINITY,f64::min);
                let hi=strokes.iter().map(|s|s.x1+s.width/2.).fold(f64::NEG_INFINITY,f64::max);
                assert_eq!((lo,hi,strokes.iter().map(|s|s.width).sum::<f64>()),(low,high,width),"{style} at {scale}");
            }
        }
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn patterns_have_distinct_ink_and_background_and_documented_grey_density() {
        let count = |name: &str| {
            (0..8)
                .flat_map(|y| (0..8).map(move |x| (x, y)))
                .filter(|&(x, y)| pattern(name, x, y))
                .count()
        };
        for name in [
            "darkGray",
            "mediumGray",
            "lightGray",
            "gray125",
            "gray0625",
            "darkHorizontal",
            "lightHorizontal",
            "darkVertical",
            "lightVertical",
            "darkDown",
            "lightDown",
            "darkUp",
            "lightUp",
            "darkGrid",
            "lightGrid",
            "darkTrellis",
            "lightTrellis",
        ] {
            assert!((1..64).contains(&count(name)), "{name}");
        }
        assert_eq!(count("gray0625"), 4);
        assert_eq!(count("gray125"), 8);
        assert_eq!(count("lightGray"), 16);
        assert_eq!(count("mediumGray"), 32);
        assert_eq!(count("darkGray"), 48);
        assert_eq!(count("lightGrid"), 28);
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn linear_and_rectangular_gradients_use_stops_and_asymmetric_convergence() {
        let rect = [0.2, 0.8, 0.3, 0.7];
        let stops = [(0., [255, 0, 0]), (1., [0, 0, 255])];
        let p = |x, y| gradient_position("linear", 90., rect, x, y, 200., 100.);
        assert_eq!(sample_gradient(&stops, p(100., 0.)), [255, 0, 0]);
        assert_eq!(sample_gradient(&stops, p(100., 100.)), [0, 0, 255]);
        assert_eq!(sample_gradient(&stops, p(100., 50.)), [128, 0, 128]);
        let p = |x, y| gradient_position("path", 0., rect, x, y, 200., 100.);
        assert_eq!(p(100., 50.), 0.);
        assert_eq!(p(0., 50.), 1.);
        assert_eq!(p(200., 50.), 1.);
        assert_eq!(p(100., 0.), 1.);
        assert_eq!(p(100., 100.), 1.);
        assert!((p(20., 50.) - 0.5).abs() < 1e-12);
        assert!((p(180., 50.) - 0.5).abs() < 1e-12);
        assert_eq!(
            sample_gradient(
                &[
                    (0., [255, 0, 0]),
                    (0.5, [255, 0, 0]),
                    (0.5, [0, 0, 255]),
                    (1., [0, 0, 255])
                ],
                0.5
            ),
            [0, 0, 255]
        );
    }
}

pub struct Stroke {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
    pub width: f64,
    pub color: String,
    pub dash: Vec<f64>,
    pub phase: f64,
}

/// Axis-aligned edges in cell-local coordinates. Shared sides use the same dash phase
/// even when the neighbouring cell's perpendicular pens extend a different distance.
pub fn edges(cell: &dna_calc_model::render_plan::PlannedCell, scale: f64) -> Vec<Stroke> {
    let mut out = Vec::new();
    let unmerged = cell.row_span == 1 && cell.col_span == 1;
    let rect = border_rect(cell,scale);
    let (left,top,right,bottom)=(rect.x-cell.anchor.x,rect.y-cell.anchor.y,
        rect.x+rect.w-cell.anchor.x,rect.y+rect.h-cell.anchor.y);
    for (side,edge) in cell.borders.iter().enumerate() {
        let Some(edge)=edge else {continue};
        let pen=line(edge,scale);
        let offsets = if unmerged {
            if pen.double {vec![-1.5,0.5]} else {vec![if pen.width==3. {-0.5}else{-pen.width/2.}]}
        } else if pen.double {vec![0.5,2.5]}else{vec![pen.width/2.]};
        for offset in offsets {
            let (x1,y1,x2,y2)=match side {
                0 => (offset,top,offset,bottom),
                1 => {let x=cell.anchor.w+if unmerged {offset}else{-offset};(x,top,x,bottom)},
                2 => (left,offset,right,offset),
                _ => {let y=cell.anchor.h+if unmerged {offset}else{-offset};(left,y,right,y)},
            };
            let phase=if side<2 {cell.anchor.y+y1}else{cell.anchor.x+x1};
            if edge.style=="slantDashDot" && unmerged {
                // The captured medium slant has staggered cuts across two one-pixel lanes.
                // Their union has no empty gap; a two-pixel dashed stroke loses that shape.
                for (lane,dash) in [(-0.5,vec![11.,1.,5.,1.,5.,1.]),(0.5,vec![10.,2.,4.,2.,4.,2.])] {
                    let (dx,dy)=if side<2 {(lane,0.)}else{(0.,lane)};
                    out.push(Stroke{x1:x1+dx,y1:y1+dy,x2:x2+dx,y2:y2+dy,width:1.,color:edge.color.clone().unwrap_or_else(||"#000000".into()),dash,phase});
                }
            } else {
                out.push(Stroke{x1,y1,x2,y2,width:if pen.double {1.}else{pen.width},
                    color:edge.color.clone().unwrap_or_else(||"#000000".into()),dash:pen.dash.to_vec(),phase});
            }
        }
    }
    out
}
/// Extra strokes use cell-local coordinates, so frozen-pane fragments share identical geometry.
pub fn strokes(cell: &dna_calc_model::render_plan::PlannedCell, scale: f64) -> Vec<Stroke> {
    let mut out = Vec::new();
    for segment in &cell.boundary {
        let line = line(&segment.edge, scale);
        for offset in if line.double {
            vec![0.5, 2.5]
        } else {
            vec![line.width / 2.]
        } {
            let (dx, dy) = match segment.side {
                0 => (offset, 0.),
                1 => (-offset, 0.),
                2 => (0., offset),
                _ => (0., -offset),
            };
            out.push(Stroke {
                x1: segment.x1 + dx,
                y1: segment.y1 + dy,
                x2: segment.x2 + dx,
                y2: segment.y2 + dy,
                width: if line.double { 1. } else { line.width },
                color: segment
                    .edge
                    .color
                    .clone()
                    .unwrap_or_else(|| "#000000".into()),
                dash: line.dash.to_vec(),
                phase: if segment.side<2 {cell.anchor.y+segment.y1}else{cell.anchor.x+segment.x1},
            });
        }
    }
    for (up, edge) in cell.shown.style.diagonal.iter().enumerate() {
        let Some(edge) = edge else {
            continue;
        };
        let line = line(edge, scale);
        let (x1, y1, x2, y2) = if up == 0 {
            (0., cell.anchor.h, cell.anchor.w, 0.)
        } else {
            (0., 0., cell.anchor.w, cell.anchor.h)
        };
        let length = (cell.anchor.w * cell.anchor.w + cell.anchor.h * cell.anchor.h)
            .sqrt()
            .max(1.);
        for offset in if line.double {
            vec![-1., 1.]
        } else {
            vec![0.]
        } {
            let (dx, dy) = (-(y2 - y1) / length * offset, (x2 - x1) / length * offset);
            out.push(Stroke {
                x1: x1 + dx,
                y1: y1 + dy,
                x2: x2 + dx,
                y2: y2 + dy,
                width: if line.double { 1. } else { line.width },
                color: edge.color.clone().unwrap_or_else(|| "#000000".into()),
                dash: line.dash.to_vec(),
                phase: 0.,
            });
        }
    }
    out
}
