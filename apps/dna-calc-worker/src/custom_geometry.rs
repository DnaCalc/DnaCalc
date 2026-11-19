//! A custom geometry (`a:custGeom`) as the page draws it (dc-zjsg.2): its adjust values and guides worked
//! out by the shape guide formulas of ECMA-376 Part 1 (20.1.9.11), then its paths as segments with their
//! points as fractions of the shape's box. Arcs become cubic pieces of at most a quarter turn; quadratic
//! curves become cubic ones.

use calc_host::{CustomGeometry, GeometryCommand, GeometryPath};
use dna_calc_model::drawn_shapes::{DrawnPath, Seg};
use std::collections::HashMap;
use std::sync::OnceLock;

/// The preset geometries of ECMA-376 Part 1 as `presets/make-presets.py` writes them from the standard's
/// presetShapeDefinitions.xml.
const PRESET_TABLE: &str = include_str!("presets/preset-shapes.txt");

fn presets() -> &'static HashMap<&'static str, CustomGeometry> {
    static TABLE: OnceLock<HashMap<&'static str, CustomGeometry>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = HashMap::new();
        let mut current: Option<(&str, CustomGeometry)> = None;
        let pair = |a: Option<&str>, b: Option<&str>| (a.unwrap_or("0").to_owned(), b.unwrap_or("0").to_owned());
        for line in PRESET_TABLE.lines().filter(|l| !l.starts_with('#') && !l.is_empty()) {
            if let Some(name) = line.strip_prefix('@') {
                if let Some((n, g)) = current.take() {
                    table.insert(n, g);
                }
                current = Some((name, CustomGeometry::default()));
                continue;
            }
            let Some((_, g)) = current.as_mut() else { continue };
            let (tag, rest) = line.split_once(' ').unwrap_or((line, ""));
            let mut words = rest.split_whitespace();
            match tag {
                "a" | "g" => {
                    let (name, fmla) = rest.split_once(' ').unwrap_or((rest, "val 0"));
                    let list = if tag == "a" { &mut g.adjust } else { &mut g.guides };
                    list.push((name.to_owned(), fmla.to_owned()));
                }
                "r" => g.text_rect = Some([0, 1, 2, 3].map(|_| words.next().unwrap_or("0").to_owned())),
                "p" => {
                    fn field(w: Option<&str>) -> Option<&str> {
                        w.filter(|w| *w != "-")
                    }
                    let (w, h, fill, stroke) = (field(words.next()), field(words.next()), field(words.next()), field(words.next()));
                    g.paths.push(GeometryPath {
                        width: w.and_then(|v| v.parse().ok()),
                        height: h.and_then(|v| v.parse().ok()),
                        fill: fill.map(str::to_owned),
                        stroke: !matches!(stroke, Some("false" | "0")),
                        commands: Vec::new(),
                    });
                }
                _ => {
                    let Some(path) = g.paths.last_mut() else { continue };
                    let mut p = || pair(words.next(), words.next());
                    path.commands.push(match tag {
                        "M" => GeometryCommand::Move(p()),
                        "L" => GeometryCommand::Line(p()),
                        "A" => GeometryCommand::Arc([0, 1, 2, 3].map(|_| words.next().unwrap_or("0").to_owned())),
                        "Q" => GeometryCommand::Quad([p(), p()]),
                        "C" => GeometryCommand::Cubic([p(), p(), p()]),
                        _ => GeometryCommand::Close,
                    });
                }
            }
        }
        if let Some((n, g)) = current.take() {
            table.insert(n, g);
        }
        table
    })
}

/// The preset `name` as a custom geometry, its adjust values replaced by the shape's own (`adjust`, 100000ths);
/// `None` for a name the standard does not define.
pub(crate) fn preset(name: &str, adjust: &[(String, i64)]) -> Option<CustomGeometry> {
    let mut geometry = presets().get(name)?.clone();
    for (name, value) in adjust {
        if let Some(slot) = geometry.adjust.iter_mut().find(|(n, _)| n == name) {
            slot.1 = format!("val {value}");
        }
    }
    Some(geometry)
}

/// 60,000ths of a degree, the unit of a guide's angle.
const DEGREE: f64 = 60_000.0;

struct Guides {
    values: HashMap<String, f64>,
}

impl Guides {
    /// The built-in names of a `w` by `h` shape (ECMA-376 Part 1, 20.1.10.56's table of shape guides).
    fn new(w: f64, h: f64) -> Self {
        let (ss, ls) = (w.min(h), w.max(h));
        let mut values = HashMap::new();
        for (name, value) in [
            ("w", w),
            ("h", h),
            ("l", 0.0),
            ("t", 0.0),
            ("r", w),
            ("b", h),
            ("hc", w / 2.0),
            ("vc", h / 2.0),
            ("ss", ss),
            ("ls", ls),
            ("cd2", 10_800_000.0),
            ("cd4", 5_400_000.0),
            ("cd8", 2_700_000.0),
            ("3cd4", 16_200_000.0),
            ("3cd8", 8_100_000.0),
            ("5cd8", 13_500_000.0),
            ("7cd8", 18_900_000.0),
        ] {
            values.insert(name.to_owned(), value);
        }
        for d in [2.0, 3.0, 4.0, 5.0, 6.0, 8.0, 10.0, 12.0, 32.0] {
            values.insert(format!("wd{d}"), w / d);
            values.insert(format!("hd{d}"), h / d);
        }
        for d in [2.0, 4.0, 6.0, 8.0, 16.0, 32.0] {
            values.insert(format!("ssd{d}"), ss / d);
        }
        Guides { values }
    }

    /// A number or a guide's name; a name not defined is 0.
    fn value(&self, token: &str) -> f64 {
        token.trim().parse::<f64>().ok().unwrap_or_else(|| self.values.get(token.trim()).copied().unwrap_or(0.0))
    }

    /// A guide formula (`*/ w 1 2`, `val 5000`, `pin 0 adj 50000`, ...).
    fn formula(&self, fmla: &str) -> f64 {
        let mut parts = fmla.split_whitespace();
        let op = parts.next().unwrap_or("val");
        let args: Vec<f64> = parts.map(|a| self.value(a)).collect();
        let a = |i: usize| args.get(i).copied().unwrap_or(0.0);
        let angle = |v: f64| (v / DEGREE).to_radians();
        let at2 = |x: f64, y: f64| y.atan2(x).to_degrees() * DEGREE;
        match op {
            "*/" => if a(2) == 0.0 { 0.0 } else { a(0) * a(1) / a(2) },
            "+-" => a(0) + a(1) - a(2),
            "+/" => if a(2) == 0.0 { 0.0 } else { (a(0) + a(1)) / a(2) },
            "?:" => if a(0) > 0.0 { a(1) } else { a(2) },
            "abs" => a(0).abs(),
            "at2" => at2(a(0), a(1)),
            "cat2" => a(0) * a(2).atan2(a(1)).cos(),
            "cos" => a(0) * angle(a(1)).cos(),
            "max" => a(0).max(a(1)),
            "min" => a(0).min(a(1)),
            "mod" => (a(0) * a(0) + a(1) * a(1) + a(2) * a(2)).sqrt(),
            "pin" => if a(1) < a(0) { a(0) } else if a(1) > a(2) { a(2) } else { a(1) },
            "sat2" => a(0) * a(2).atan2(a(1)).sin(),
            "sin" => a(0) * angle(a(1)).sin(),
            "sqrt" => a(0).max(0.0).sqrt(),
            "tan" => a(0) * angle(a(1)).tan(),
            _ => a(0),
        }
    }

    fn define(&mut self, list: &[(String, String)]) {
        for (name, fmla) in list {
            let value = self.formula(fmla);
            self.values.insert(name.clone(), value);
        }
    }
}

/// The paths of `geometry` for a shape `w` by `h` EMU, points as fractions of its box, and its text rectangle
/// `[l, t, r, b]` as fractions (the whole box when the file gives none).
pub(crate) fn custom_paths(geometry: &CustomGeometry, w: f64, h: f64) -> (Vec<DrawnPath>, [f64; 4]) {
    let (w, h) = (if w > 0.0 { w } else { 1.0 }, if h > 0.0 { h } else { 1.0 });
    let mut guides = Guides::new(w, h);
    guides.define(&geometry.adjust);
    guides.define(&geometry.guides);
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
        assert_eq!(presets().len(), 186);
        for (name, geometry) in presets() {
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
