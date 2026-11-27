//! The preset shape geometries of ECMA-376 Part 1 (dc-zjsg.2, dc-zjsg.5) and the shape guide formulas (20.1.9.11)
//! that work out a geometry's guides, paths, text rectangle and connection sites. The table is the standard's
//! presetShapeDefinitions.xml as `presets/make-presets.py` writes it; the page's drawing and the host's connector
//! routing read the same geometry here.

use crate::{CustomGeometry, GeometryCommand, GeometryPath};
use std::collections::HashMap;
use std::sync::OnceLock;

/// The preset geometries of ECMA-376 Part 1 as `presets/make-presets.py` writes them from the standard's
/// presetShapeDefinitions.xml.
const PRESET_TABLE: &str = match std::str::from_utf8(include_bytes!("presets/preset-shapes.txt")) {
    Ok(table) => table,
    Err(_) => panic!("preset shape data must be UTF-8"),
};

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
                "x" => g.sites.push(pair(words.next(), words.next())),
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
pub fn preset(name: &str, adjust: &[(String, i64)]) -> Option<CustomGeometry> {
    let mut geometry = presets().get(name)?.clone();
    for (name, value) in adjust {
        if let Some(slot) = geometry.adjust.iter_mut().find(|(n, _)| n == name) {
            slot.1 = format!("val {value}");
        }
    }
    Some(geometry)
}

/// 60,000ths of a degree, the unit of a guide's angle.
pub const DEGREE: f64 = 60_000.0;

/// A shape's guide values: the built-in names, then its adjust values and guides worked out in order.
pub struct Guides {
    values: HashMap<String, f64>,
}

impl Guides {
    /// The guides of `geometry` for a shape `w` by `h` (any unit; EMU from the file).
    pub fn of(geometry: &CustomGeometry, w: f64, h: f64) -> Self {
        let mut guides = Guides::new(w, h);
        guides.define(&geometry.adjust);
        guides.define(&geometry.guides);
        guides
    }

    /// The built-in names of a `w` by `h` shape (ECMA-376 Part 1, 20.1.10.56's table of shape guides).
    pub fn new(w: f64, h: f64) -> Self {
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
    pub fn value(&self, token: &str) -> f64 {
        token.trim().parse::<f64>().ok().unwrap_or_else(|| self.values.get(token.trim()).copied().unwrap_or(0.0))
    }

    /// A guide formula (`*/ w 1 2`, `val 5000`, `pin 0 adj 50000`, ...).
    pub fn formula(&self, fmla: &str) -> f64 {
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

    pub fn define(&mut self, list: &[(String, String)]) {
        for (name, fmla) in list {
            let value = self.formula(fmla);
            self.values.insert(name.clone(), value);
        }
    }
}


/// The preset names the table defines.
pub fn names() -> impl Iterator<Item = &'static str> {
    presets().keys().copied()
}

/// The connection sites of `geometry` for a shape `w` by `h`, as fractions of its box, in the file's order (a
/// connector's `stCxn idx` and `endCxn idx` count them from 0).
pub fn connection_sites(geometry: &CustomGeometry, w: f64, h: f64) -> Vec<(f64, f64)> {
    let (w, h) = (if w > 0.0 { w } else { 1.0 }, if h > 0.0 { h } else { 1.0 });
    let guides = Guides::of(geometry, w, h);
    geometry.sites.iter().map(|(x, y)| (guides.value(x) / w, guides.value(y) / h)).collect()
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zjsg_5__connection_sites_come_from_the_standards_definitions() {
        // A rectangle's four sites: top, left, bottom, right middles.
        let rect = preset("rect", &[]).unwrap();
        assert_eq!(connection_sites(&rect, 200.0, 100.0), [(0.5, 0.0), (0.0, 0.5), (0.5, 1.0), (1.0, 0.5)]);
        // An isosceles triangle has six; its apex is the first.
        let triangle = preset("triangle", &[]).unwrap();
        let sites = connection_sites(&triangle, 100.0, 100.0);
        assert_eq!(sites.len(), 6);
        assert_eq!(sites[0], (0.5, 0.0));
        assert_eq!(names().count(), 186);
    }
}
