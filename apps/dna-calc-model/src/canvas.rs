//! Pure canvas trace and existing SVG geometry.
//! Extracted from DNA Calc; retains the DnaTreeCalc bf6dfd7
//! `src/dnatreecalc-skins/src/flow.rs` helpers ported there.
use calc_value::{CellKey, GridArea};
use std::collections::{BTreeMap, BTreeSet};
/// The most chips one column shows; the rest are counted.
pub const PER_COLUMN: usize = 12;
/// The deepest trace either way.
pub const MAX_DEPTH: u8 = 6;

pub const COLUMN_WIDTH: f64 = 230.0;
pub const CHIP_WIDTH: f64 = 200.0;
pub const ROW_HEIGHT: f64 = 124.0;
pub const CHIP_HEIGHT: f64 = 110.0;

/// A chip: one cell, or a range a formula reads whole.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Node {
    Cell(CellKey),
    Area(GridArea),
}

impl Node {
    fn key(&self) -> (u8, CellKey, CellKey) {
        match self {
            Node::Cell(at) => (0, *at, *at),
            Node::Area(area) => (1, area.first(), area.last()),
        }
    }
}

impl PartialOrd for Node {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Node {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.key().cmp(&other.key())
    }
}

/// The trace around a cell: the chips of each column (0 the cell, negative its inputs at that
/// depth, positive its readers), the wires from input to reader, and how many chips each column
/// leaves out beyond [`PER_COLUMN`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Trace {
    pub columns: BTreeMap<i32, Vec<Node>>,
    pub wires: Vec<(Node, Node)>,
    pub hidden: BTreeMap<i32, usize>,
}

impl Trace {
    /// Where a chip is: its column and its place in it.
    pub fn place(&self, node: Node) -> Option<(i32, usize)> {
        self.columns.iter().find_map(|(column, nodes)| {
            nodes
                .iter()
                .position(|n| *n == node)
                .map(|row| (*column, row))
        })
    }
}

/// The trace from `center`: `back` steps of what formulas read (a range of one cell is that cell;
/// a larger range is one chip, which is not followed further) and `forward` steps of the formulas
/// that read a cell. Each chip appears once, in the first column it is reached in; a column holds
/// at most `per_column` chips (the rest are counted, and not followed).
pub fn trace(
    center: CellKey,
    back: u8,
    forward: u8,
    per_column: usize,
    precedents: impl Fn(CellKey) -> Option<Vec<GridArea>>,
    dependents: impl Fn(CellKey) -> Vec<CellKey>,
) -> Trace {
    let mut out = Trace::default();
    out.columns.insert(0, vec![Node::Cell(center)]);
    let mut seen: BTreeSet<Node> = BTreeSet::from([Node::Cell(center)]);
    // Places a chip reached at `column`: whether it is shown, and whether it was reached first here.
    let place = |out: &mut Trace, seen: &mut BTreeSet<Node>, column: i32, node: Node| {
        if !seen.insert(node) {
            return (out.place(node).is_some(), false);
        }
        let nodes = out.columns.entry(column).or_default();
        if nodes.len() < per_column {
            nodes.push(node);
            (true, true)
        } else {
            *out.hidden.entry(column).or_default() += 1;
            (false, true)
        }
    };
    let mut frontier = vec![center];
    for level in 1..=i32::from(back) {
        let mut next = Vec::new();
        for reader in &frontier {
            for area in precedents(*reader).unwrap_or_default() {
                let node = if area.cell_count() == 1 {
                    Node::Cell(area.first())
                } else {
                    Node::Area(area)
                };
                let (shown, fresh) = place(&mut out, &mut seen, -level, node);
                if shown {
                    out.wires.push((node, Node::Cell(*reader)));
                    if let (true, Node::Cell(cell)) = (fresh, node) {
                        next.push(cell);
                    }
                }
            }
        }
        frontier = next;
    }
    let mut frontier = vec![center];
    for level in 1..=i32::from(forward) {
        let mut next = Vec::new();
        for read in &frontier {
            for reader in dependents(*read) {
                let node = Node::Cell(reader);
                let (shown, fresh) = place(&mut out, &mut seen, level, node);
                if shown {
                    out.wires.push((Node::Cell(*read), node));
                    if fresh {
                        next.push(reader);
                    }
                }
            }
        }
        frontier = next;
    }
    out.wires.sort();
    out.wires.dedup();
    out
}

/// The chip's position on the plane, its top-left corner, with the leftmost column at 0.
pub fn position(trace: &Trace, column: i32, row: usize) -> (f64, f64) {
    let first = trace.columns.keys().next().copied().unwrap_or(0);
    (
        f64::from(column - first) * COLUMN_WIDTH,
        row as f64 * ROW_HEIGHT,
    )
}

/// The wires as SVG markup: numbers and fixed names only, never text from a cell.
pub fn wires_svg(trace: &Trace) -> String {
    let mut paths = String::new();
    for (from, to) in &trace.wires {
        let (Some((c1, r1)), Some((c2, r2))) = (trace.place(*from), trace.place(*to)) else {
            continue;
        };
        let (x1, y1) = position(trace, c1, r1);
        let (x2, y2) = position(trace, c2, r2);
        let (x1, y1) = (x1 + CHIP_WIDTH, y1 + CHIP_HEIGHT / 2.0);
        let y2 = y2 + CHIP_HEIGHT / 2.0;
        let middle = (x1 + x2) / 2.0;
        paths.push_str(&format!(
            "<path d=\"M {x1:.1} {y1:.1} C {middle:.1} {y1:.1} {middle:.1} {y2:.1} {x2:.1} {y2:.1}\"/>"
        ));
    }
    let (width, height) = plane_size(trace);
    format!("<svg width=\"{width:.0}\" height=\"{height:.0}\">{paths}</svg>")
}

pub fn plane_size(trace: &Trace) -> (f64, f64) {
    let columns = trace.columns.len().max(1) as f64;
    let rows = trace
        .columns
        .values()
        .map(Vec::len)
        .max()
        .unwrap_or(1)
        .max(1) as f64;
    (
        (columns - 1.0) * COLUMN_WIDTH + CHIP_WIDTH,
        rows * ROW_HEIGHT + 64.0,
    )
}
