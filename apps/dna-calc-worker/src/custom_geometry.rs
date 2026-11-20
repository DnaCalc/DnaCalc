//! A custom geometry (`a:custGeom`) as the page draws it (dc-zjsg.2): its adjust values and guides worked
//! out by the shape guide formulas of ECMA-376 Part 1 (20.1.9.11), then its paths as segments with their
//! points as fractions of the shape's box. Arcs become cubic pieces of at most a quarter turn; quadratic
//! curves become cubic ones.

use calc_host::preset_geometry::{DEGREE, Guides};
pub(crate) use calc_host::preset_geometry::preset;
use calc_host::{CustomGeometry, GeometryCommand};
use dna_calc_model::drawn_shapes::{DrawnPath, Seg};

/// The paths of `geometry` for a shape `w` by `h` EMU, points as fractions of its box, and its text rectangle
/// `[l, t, r, b]` as fractions (the whole box when the file gives none).
pub(crate) fn custom_paths(geometry: &CustomGeometry, w: f64, h: f64) -> (Vec<DrawnPath>, [f64; 4]) {
    let (w, h) = (if w > 0.0 { w } else { 1.0 }, if h > 0.0 { h } else { 1.0 });
    let guides = Guides::of(geometry, w, h);
    let paths = geometry
        .paths
        .iter()
        .map(|path| {
            // A path with a coordinate space of its own is scaled from it to the box; otherwise its points
            // are in the shape's.
            let sx = path.width.filter(|&v| v > 0).map_or(w, |v| v as f64);
            let sy = path.height.filter(|&v| v > 0).map_or(h, |v| v as f64);
            let point = |(x, y): &(String, String)| (guides.value(x) / sx, guides.value(y) / sy);
            let mut segments = Vec::new();
            let mut current = (0.0, 0.0);
            let mut start = (0.0, 0.0);
            for command in &path.commands {
                match command {
                    GeometryCommand::Move(p) => {
                        current = point(p);
                        start = current;
                        segments.push(Seg::Move(current.0, current.1));
                    }
                    GeometryCommand::Line(p) => {
                        current = point(p);
                        segments.push(Seg::Line(current.0, current.1));
                    }
                    GeometryCommand::Quad([q, p]) => {
                        let (q, p) = (point(q), point(p));
                        let c1 = (current.0 + 2.0 / 3.0 * (q.0 - current.0), current.1 + 2.0 / 3.0 * (q.1 - current.1));
                        let c2 = (p.0 + 2.0 / 3.0 * (q.0 - p.0), p.1 + 2.0 / 3.0 * (q.1 - p.1));
                        segments.push(Seg::Cubic(c1, c2, p));
                        current = p;
                    }
                    GeometryCommand::Cubic([a, b, p]) => {
                        let p = point(p);
                        segments.push(Seg::Cubic(point(a), point(b), p));
                        current = p;
                    }
                    GeometryCommand::Arc([wr, hr, st, sw]) => {
                        let (wr, hr) = (guides.value(wr) / sx, guides.value(hr) / sy);
                        let (st, sw) = (guides.value(st) / DEGREE, guides.value(sw) / DEGREE);
                        current = arc(&mut segments, current, wr, hr, st, sw);
                    }
                    GeometryCommand::Close => {
                        segments.push(Seg::Close);
                        current = start;
                    }
                }
            }
            DrawnPath { segments, fill: path.fill.clone().unwrap_or_else(|| "norm".into()), stroke: path.stroke }
        })
        .collect();
    let text = geometry.text_rect.as_ref().map_or([0.0, 0.0, 1.0, 1.0], |[l, t, r, b]| {
        [guides.value(l) / w, guides.value(t) / h, guides.value(r) / w, guides.value(b) / h]
    });
    (paths, text)
}

/// The parametric angle (radians) of the point of an ellipse with radii `wr`, `hr` seen at `degrees`.
fn parametric(wr: f64, hr: f64, degrees: f64) -> f64 {
    let a = degrees.to_radians();
    (wr * a.sin()).atan2(hr * a.cos())
}

/// `a:arcTo` from `from`: the arc of the ellipse with radii `wr`, `hr` from the angle `start` through `sweep`
/// (degrees, clockwise), as cubic pieces of at most 90 degrees; returns the arc's end.
fn arc(segments: &mut Vec<Seg>, from: (f64, f64), wr: f64, hr: f64, start: f64, sweep: f64) -> (f64, f64) {
    if wr == 0.0 || hr == 0.0 || sweep == 0.0 {
        return from;
    }
    let t0 = parametric(wr, hr, start);
    let centre = (from.0 - wr * t0.cos(), from.1 - hr * t0.sin());
    let pieces = (sweep.abs() / 90.0).ceil().max(1.0) as usize;
    let step = sweep / pieces as f64;
    let mut t1 = t0;
    let mut end = from;
    for i in 1..=pieces {
        let mut t2 = parametric(wr, hr, start + step * i as f64);
        // The parametric angle moves the way the sweep does, by less than a half turn a piece.
        while sweep > 0.0 && t2 < t1 {
            t2 += std::f64::consts::TAU;
        }
        while sweep < 0.0 && t2 > t1 {
            t2 -= std::f64::consts::TAU;
        }
        let k = 4.0 / 3.0 * ((t2 - t1) / 4.0).tan();
        let p1 = (centre.0 + wr * t1.cos(), centre.1 + hr * t1.sin());
        let p2 = (centre.0 + wr * t2.cos(), centre.1 + hr * t2.sin());
        let c1 = (p1.0 - k * wr * t1.sin(), p1.1 + k * hr * t1.cos());
        let c2 = (p2.0 + k * wr * t2.sin(), p2.1 - k * hr * t2.cos());
        segments.push(Seg::Cubic(c1, c2, p2));
        end = p2;
        t1 = t2;
    }
    end
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;
    use calc_host::GeometryPath;

    fn pt(x: &str, y: &str) -> (String, String) {
        (x.into(), y.into())
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zjsg_2__every_ecma_376_preset_is_in_the_table_and_draws() {
        // 187 entries in the standard's file, upDownArrow twice.
        assert_eq!(calc_host::preset_geometry::names().count(), 186);
        for name in calc_host::preset_geometry::names() {
            let geometry = &preset(name, &[]).unwrap();
            let (paths, text) = custom_paths(geometry, 100_000.0, 60_000.0);
            assert!(!paths.is_empty() && paths.iter().all(|p| !p.segments.is_empty()), "{name}");
            assert!(paths.iter().flat_map(|p| &p.segments).all(|s| match *s {
                Seg::Move(x, y) | Seg::Line(x, y) => x.is_finite() && y.is_finite(),
                Seg::Cubic(a, b, c) => [a, b, c].iter().all(|p| p.0.is_finite() && p.1.is_finite()),
                Seg::Close => true,
            }), "{name}");
            assert!(text.iter().all(|v| v.is_finite()), "{name}");
        }
        // A five-pointed star: ten points, its top at the middle of the top edge.
        let (paths, text) = custom_paths(&preset("star5", &[]).unwrap(), 100.0, 100.0);
        assert_eq!(paths[0].segments.iter().filter(|s| matches!(s, Seg::Line(..))).count(), 9);
        assert!(paths[0].segments.contains(&Seg::Line(0.5, 0.0)));
        assert!(text[0] > 0.2 && text[2] < 0.8);
        // The shape's own adjust value replaces the preset's.
        let thin = preset("star5", &[("adj".into(), 10_000)]).unwrap();
        assert_eq!(thin.adjust[0], ("adj".to_owned(), "val 10000".to_owned()));
        assert!(preset("noSuchShape", &[]).is_none());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zjsg_2__guide_formulas_follow_ecma_376() {
        let g = Guides::new(200.0, 100.0);
        assert_eq!(g.formula("*/ w 1 4"), 50.0);
        assert_eq!(g.formula("+- hc 10 vc"), 60.0);
        assert_eq!(g.formula("+/ w h 3"), 100.0);
        assert_eq!(g.formula("?: -1 5 7"), 7.0);
        assert_eq!(g.formula("pin 0 120000 50000"), 50000.0);
        assert_eq!(g.formula("val ss"), 100.0);
        assert_eq!(g.formula("mod 3 4 0"), 5.0);
        assert!((g.formula("at2 1 1") - 2_700_000.0).abs() < 1e-6);
        assert!((g.formula("cos 100 cd4")).abs() < 1e-9);
        assert!((g.formula("sin 100 cd4") - 100.0).abs() < 1e-9);
        assert_eq!(g.formula("max wd2 hd3"), 100.0);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zjsg_2__a_custom_path_scales_from_its_own_space_and_arcs_become_curves() {
        let geometry = CustomGeometry {
            adjust: Vec::new(),
            guides: vec![("half".into(), "*/ w 1 2".into())],
            text_rect: Some(["l".into(), "t".into(), "half".into(), "b".into()]),
            sites: Vec::new(),
            paths: vec![
                GeometryPath {
                    width: Some(10),
                    height: Some(20),
                    fill: None,
                    stroke: true,
                    commands: vec![
                        GeometryCommand::Move(pt("0", "0")),
                        GeometryCommand::Line(pt("10", "0")),
                        GeometryCommand::Quad([pt("10", "20"), pt("0", "20")]),
                        GeometryCommand::Close,
                    ],
                },
                GeometryPath {
                    width: None,
                    height: None,
                    fill: Some("none".into()),
                    stroke: true,
                    commands: vec![
                        // From the right of a circle of radius 50 half way round, clockwise, to its left.
                        GeometryCommand::Move(pt("100", "50")),
                        GeometryCommand::Arc(["50".into(), "50".into(), "0".into(), "cd2".into()]),
                    ],
                },
            ],
        };
        let (paths, text) = custom_paths(&geometry, 100.0, 100.0);
        assert_eq!(text, [0.0, 0.0, 0.5, 1.0]);
        assert_eq!(paths[0].fill, "norm");
        assert_eq!(paths[0].segments[1], Seg::Line(1.0, 0.0));
        let Seg::Cubic(c1, c2, end) = paths[0].segments[2] else { panic!("a cubic") };
        assert!((c1.0 - 1.0).abs() < 1e-9 && (c1.1 - 2.0 / 3.0).abs() < 1e-9 && (c2.0 - 2.0 / 3.0).abs() < 1e-9 && (c2.1 - 1.0).abs() < 1e-9);
        assert_eq!(end, (0.0, 1.0));
        // Two quarter pieces, through the bottom (clockwise in a y-down space) to the left.
        assert_eq!(paths[1].fill, "none");
        assert_eq!(paths[1].segments.len(), 3);
        let Seg::Cubic(_, _, mid) = paths[1].segments[1] else { panic!("a cubic") };
        assert!((mid.0 - 0.5).abs() < 1e-9 && (mid.1 - 1.0).abs() < 1e-9, "{mid:?}");
        let Seg::Cubic(_, _, end) = paths[1].segments[2] else { panic!("a cubic") };
        assert!(end.0.abs() < 1e-9 && (end.1 - 0.5).abs() < 1e-9, "{end:?}");
    }
}
