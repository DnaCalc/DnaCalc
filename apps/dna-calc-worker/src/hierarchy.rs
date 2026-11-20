//! The layouts of Excel 2016's hierarchy charts (dc-zjsg.6): a treemap's nested rectangles (the squarified algorithm
//! of Bruls, Huizing and van Wijk) and a sunburst's arcs. The hierarchy is the series' category levels (leaf names,
//! then each level above, a parent named at its first point holding until the next) with the values summed up it.
//! Excel's own tile order and spacing are not measured (ASSUMED until an Excel side-by-side).

use calc_host::SeriesView;
use calc_host::chart::text_of;

/// A node of the hierarchy: a leaf has the index of its point.
#[derive(Debug, PartialEq)]
pub struct Tile {
    pub name: String,
    pub value: f64,
    pub point: Option<usize>,
    pub children: Vec<Tile>,
}

impl Tile {
    fn total(&self) -> f64 {
        if self.children.is_empty() { self.value } else { self.children.iter().map(Tile::total).sum() }
    }
}

/// The hierarchy of `series`: its outermost level's names first; points without a positive value are left out.
pub fn hierarchy(series: &SeriesView) -> Vec<Tile> {
    let filled: Vec<Vec<String>> = series
        .outer_levels
        .iter()
        .map(|level| {
            let mut current = String::new();
            (0..series.values.len())
                .map(|j| {
                    if let Some(Some(name)) = level.get(j) {
                        current = name.clone();
                    }
                    current.clone()
                })
                .collect()
        })
        .collect();
    let mut roots: Vec<Tile> = Vec::new();
    for (j, value) in series.values.iter().enumerate() {
        let Some(value) = value.filter(|v| v.is_finite() && *v > 0.0) else { continue };
        let leaf = series.categories.get(j).cloned().flatten().map(|v| text_of(&v)).unwrap_or_else(|| (j + 1).to_string());
        let mut path: Vec<String> = filled.iter().rev().map(|level| level[j].clone()).collect();
        path.push(leaf);
        let mut siblings = &mut roots;
        for (depth, name) in path.iter().enumerate() {
            let last = depth + 1 == path.len();
            let at = if last { None } else { siblings.iter().position(|t| &t.name == name && t.point.is_none()) };
            let at = match at {
                Some(at) => at,
                None => {
                    siblings.push(Tile { name: name.clone(), value: if last { value } else { 0.0 }, point: last.then_some(j), children: Vec::new() });
                    siblings.len() - 1
                }
            };
            siblings = &mut siblings[at].children;
        }
    }
    roots
}

/// A rectangle: left, top, width, height.
pub type Rect = (f64, f64, f64, f64);

/// `tiles` (by size, largest first) laid out in `rect` as squarified rectangles: rows along the shorter side, a row
/// closed when adding the next tile would make the worst aspect ratio worse.
pub fn squarify(tiles: &[&Tile], rect: Rect) -> Vec<Rect> {
    let total: f64 = tiles.iter().map(|t| t.total()).sum();
    let mut out = Vec::with_capacity(tiles.len());
    if total <= 0.0 || rect.2 <= 0.0 || rect.3 <= 0.0 {
        return vec![(rect.0, rect.1, 0.0, 0.0); tiles.len()];
    }
    let scale = rect.2 * rect.3 / total;
    let (mut x, mut y, mut w, mut h) = rect;
    let mut i = 0;
    while i < tiles.len() {
        let side = w.min(h);
        let area = |t: &Tile| t.total() * scale;
        let worst = |row: &[&Tile]| {
            let sum: f64 = row.iter().map(|t| area(t)).sum();
            let (max, min) = row.iter().fold((0.0f64, f64::INFINITY), |(a, b), t| (a.max(area(t)), b.min(area(t))));
            ((side * side * max) / (sum * sum)).max((sum * sum) / (side * side * min))
        };
        let mut end = i + 1;
        while end < tiles.len() && worst(&tiles[i..=end]) <= worst(&tiles[i..end]) {
            end += 1;
        }
        let row = &tiles[i..end];
        let sum: f64 = row.iter().map(|t| area(t)).sum();
        let thickness = sum / side;
        let mut offset = 0.0;
        for t in row {
            let length = area(t) / thickness;
            out.push(if w >= h { (x, y + offset, thickness, length) } else { (x + offset, y, length, thickness) });
            offset += length;
        }
        if w >= h {
            x += thickness;
            w -= thickness;
        } else {
            y += thickness;
            h -= thickness;
        }
        i = end;
    }
    out
}

/// One arc of a sunburst: its ring (0 the innermost), start and end angle in turns (0 to 1 from twelve o'clock,
/// clockwise), the tile it draws and the index of the top-level tile it belongs to.
#[derive(Debug, PartialEq)]
pub struct Arc<'a> {
    pub ring: usize,
    pub from: f64,
    pub to: f64,
    pub tile: &'a Tile,
    pub root: usize,
}

/// The sunburst arcs of `roots`: each tile's arc as wide as its total, parents and children ordered descending by
/// total (Excel-measured: dw3 Windows item 7), while each parent keeps its data-order `root` colour index.
pub fn arcs(roots: &[Tile]) -> Vec<Arc<'_>> {
    fn walk<'a>(tiles: &'a [Tile], ring: usize, from: f64, to: f64, root: Option<usize>, out: &mut Vec<Arc<'a>>) {
        let total: f64 = tiles.iter().map(Tile::total).sum();
        if total <= 0.0 {
            return;
        }
        let mut order: Vec<(usize, &Tile)> = tiles.iter().enumerate().collect();
        order.sort_by(|a, b| b.1.total().total_cmp(&a.1.total()));
        let mut at = from;
        for (i, t) in order {
            let span = (to - from) * t.total() / total;
            let root = root.unwrap_or(i);
            out.push(Arc { ring, from: at, to: at + span, tile: t, root });
            walk(&t.children, ring + 1, at, at + span, Some(root), out);
            at += span;
        }
    }
    let mut out = Vec::new();
    walk(roots, 0, 0.0, 1.0, None, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(name: &str, value: f64, point: usize) -> Tile {
        Tile { name: name.into(), value, point: Some(point), children: Vec::new() }
    }

    /// The squarified layout fills the rectangle, each tile its share of the area.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn squarified_tiles_fill_the_rectangle() {
        // The example of the paper: 6, 6, 4, 3, 2, 2, 1 in 6 x 4.
        let tiles: Vec<Tile> = [6.0, 6.0, 4.0, 3.0, 2.0, 2.0, 1.0].iter().enumerate().map(|(i, v)| leaf("t", *v, i)).collect();
        let refs: Vec<&Tile> = tiles.iter().collect();
        let rects = squarify(&refs, (0.0, 0.0, 6.0, 4.0));
        let area: f64 = rects.iter().map(|r| r.2 * r.3).sum();
        assert!((area - 24.0).abs() < 1e-9);
        for (r, t) in rects.iter().zip(&tiles) {
            assert!((r.2 * r.3 - t.value).abs() < 1e-9, "{r:?} {}", t.value);
            assert!(r.0 >= -1e-9 && r.1 >= -1e-9 && r.0 + r.2 <= 6.0 + 1e-9 && r.1 + r.3 <= 4.0 + 1e-9);
        }
        // The first row is the 6 and 6 down the left side (3 x 2 each).
        assert!((rects[0].2 - 3.0).abs() < 1e-9 && (rects[0].3 - 2.0).abs() < 1e-9, "{:?}", rects[0]);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn sunburst_arcs_nest_by_ring() {
        let roots = vec![
            Tile { name: "A".into(), value: 0.0, point: None, children: vec![leaf("a1", 1.0, 0), leaf("a2", 2.0, 1)] },
            Tile { name: "B".into(), value: 0.0, point: None, children: vec![leaf("b1", 3.0, 2)] },
        ];
        let arcs = arcs(&roots);
        let spans: Vec<(usize, f64, f64, usize)> = arcs.iter().map(|a| (a.ring, a.from, a.to, a.root)).collect();
        // A and B tie at total 3 (data order kept); inside A, a2 (2) precedes a1 (1).
        assert_eq!(spans, [(0, 0.0, 0.5, 0), (1, 0.0, 1.0 / 3.0, 0), (1, 1.0 / 3.0, 0.5, 0), (0, 0.5, 1.0, 1), (1, 0.5, 1.0, 1)]);
    }
}
