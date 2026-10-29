//! The canvas skin: the active cell's calculation laid out as a flow. The cell sits in the middle
//! column; the cells and ranges its formula reads are columns to its left, as deep as asked, and
//! the formulas that read it columns to its right, with a wire from each input to the formula
//! that reads it (inputs left, results right). `[` reaches one step further back and `]` one step
//! further forward (the buttons do the same); Escape returns to one step each way. A click on a
//! cell's chip makes it the active cell, and the canvas follows: it is centred on the workbook's
//! active cell wherever that is moved. A panel beside it shows the cell itself: what was
//! entered, its value and where that came from, its number format, and what it reads and what
//! reads it. F9 recalculates.
//!
//! The graph is calc-host's ([`calc_host::HostState::precedents`] and
//! [`calc_host::HostState::dependents`], the engine's own edges); the skin changes nothing but the
//! selection and, with F9, recalculates.
//!
//! Ported from DnaTreeCalc (commit bf6dfd7) `src/dnatreecalc-skins/src/flow.rs`: the layered
//! columns of chips with inputs left and results right, the SVG wires drawn from the fixed chip
//! grid (`wires_svg`), the trace grown back and forward by depth (`trace_keys`, the `[` and `]`
//! keys) and F9; and the selected-node inspector of `spine_widgets.rs` (`NodeInspector`: the
//! value, the format and the precedent and dependent counts). Left behind: the layout of every
//! node of the tree at once by longest chain (`compute_layout`, `rank_of`), which over an
//! Excel workbook of a hundred thousand formulas shows nothing legible, the reading-head sweep in
//! the engine's evaluation order and the derivation explain panel, whose reads calc-host does not
//! offer.

use crate::cells::{self, Provenance};
use crate::controller::Controller;
use crate::skin::{Skin, SkinContext, SkinId};
use crate::style::css;
use calc_host::value::{CellKey, GridArea};
use leptos::prelude::*;
use std::collections::{BTreeMap, BTreeSet};

/// The most chips one column shows; the rest are counted.
pub const PER_COLUMN: usize = 12;
/// The deepest trace either way.
pub const MAX_DEPTH: u8 = 6;

const COLUMN_WIDTH: f64 = 230.0;
const CHIP_WIDTH: f64 = 200.0;
const ROW_HEIGHT: f64 = 70.0;
const CHIP_HEIGHT: f64 = 58.0;

#[derive(Clone, Copy, Debug, Default)]
pub struct CanvasSkin;

impl Skin for CanvasSkin {
    fn id(&self) -> SkinId {
        SkinId::Canvas
    }

    fn title(&self) -> &'static str {
        "Canvas"
    }

    fn mount(&self, ctx: SkinContext) -> AnyView {
        canvas_view(ctx.controller).into_any()
    }
}

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
fn position(trace: &Trace, column: i32, row: usize) -> (f64, f64) {
    let first = trace.columns.keys().next().copied().unwrap_or(0);
    (
        f64::from(column - first) * COLUMN_WIDTH,
        row as f64 * ROW_HEIGHT,
    )
}

/// The wires as SVG markup: numbers and fixed names only, never text from a cell.
fn wires_svg(trace: &Trace) -> String {
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

fn plane_size(trace: &Trace) -> (f64, f64) {
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
        rows * ROW_HEIGHT + 20.0,
    )
}

fn canvas_view(controller: Controller) -> impl IntoView {
    let back = RwSignal::new(1u8);
    let forward = RwSignal::new(1u8);
    let traced = Memo::new(move |_| {
        controller.revision.track();
        let center = controller.active_key()?;
        let (back, forward) = (back.get(), forward.get());
        Some(controller.with_untracked(|book| {
            let host = book.host();
            trace(
                center,
                back,
                forward,
                PER_COLUMN,
                |at| host.precedents(at),
                |at| host.dependents(at),
            )
        }))
    });

    let grow = move |signal: RwSignal<u8>, by: i8| {
        signal.update(|depth| *depth = depth.saturating_add_signed(by).clamp(0, MAX_DEPTH));
    };
    let on_key = move |ev: leptos::ev::KeyboardEvent| match ev.key().as_str() {
        "[" => {
            ev.prevent_default();
            grow(back, 1);
        }
        "]" => {
            ev.prevent_default();
            grow(forward, 1);
        }
        "Escape" => {
            back.set(1);
            forward.set(1);
        }
        "F9" => {
            ev.prevent_default();
            controller.recalculate();
        }
        _ => {}
    };

    let chips = move || {
        let Some(trace) = traced.get() else {
            return ().into_any();
        };
        let center_sheet = trace
            .columns
            .get(&0)
            .and_then(|nodes| nodes.first())
            .and_then(|node| match node {
                Node::Cell(at) => Some(at.sheet()),
                Node::Area(_) => None,
            });
        let (width, height) = plane_size(&trace);
        let svg = wires_svg(&trace);
        let mut chips = Vec::new();
        for (column, nodes) in &trace.columns {
            for (row, node) in nodes.iter().enumerate() {
                let (x, y) = position(&trace, *column, row);
                let style = format!(
                    "left:{x:.0}px;top:{y:.0}px;width:{CHIP_WIDTH:.0}px;height:{CHIP_HEIGHT:.0}px"
                );
                let view = match *node {
                    Node::Cell(at) => {
                        let (label, input, value, provenance) = controller.with_untracked(|book| {
                            let host = book.host();
                            let label = if Some(at.sheet()) == center_sheet {
                                cells::address(at.row(), at.column())
                            } else {
                                cells::cell_label(host, at)
                            };
                            let profile = book.profile().cloned();
                            match book.cell(at) {
                                Some(cell) => {
                                    let shown =
                                        cells::shown(&cell, profile.as_ref(), book.cell_format(at));
                                    let value = if shown.provenance == Provenance::Refused {
                                        "refused".to_owned()
                                    } else {
                                        shown.text
                                    };
                                    (
                                        label,
                                        cells::input_text(cell.input()),
                                        value,
                                        shown.provenance,
                                    )
                                }
                                None => (label, String::new(), String::new(), Provenance::Empty),
                            }
                        });
                        let go = move |_| controller.go_to(at);
                        let node_label = label.clone();
                        view! {
                            <div class="dc-node" data-dnacalc="canvas-node" data-kind="cell" data-node=node_label
                                data-column=column.to_string() data-center=(*column == 0).to_string()
                                data-provenance=provenance.name() use:css=style on:click=go>
                                <b>{label}</b>
                                <div class="dc-f">{input}</div>
                                <div class="dc-v" data-dnacalc="canvas-value">{value}</div>
                            </div>
                        }
                        .into_any()
                    }
                    Node::Area(area) => {
                        let label = controller.with_untracked(|book| {
                            if Some(area.sheet()) == center_sheet {
                                format!(
                                    "{}:{}",
                                    cells::address(area.first().row(), area.first().column()),
                                    cells::address(area.last().row(), area.last().column())
                                )
                            } else {
                                cells::area_label(book.host(), area)
                            }
                        });
                        let count = area.cell_count();
                        let node_label = label.clone();
                        view! {
                            <div class="dc-node" data-dnacalc="canvas-node" data-kind="area" data-node=node_label
                                data-column=column.to_string() data-center="false" use:css=style>
                                <b>{label}</b>
                                <div class="dc-f">{format!("a range of {count} cells")}</div>
                            </div>
                        }
                        .into_any()
                    }
                };
                chips.push(view);
            }
            if let Some(hidden) = trace.hidden.get(column) {
                let (x, y) = position(&trace, *column, nodes.len());
                chips.push(
                    view! {
                        <div class="dc-hidden" data-dnacalc="canvas-hidden" use:css=format!("left:{x:.0}px;top:{y:.0}px")>
                            {format!("{hidden} more not shown")}
                        </div>
                    }
                    .into_any(),
                );
            }
        }
        view! {
            <div class="dc-plane" use:css=format!("width:{width:.0}px;height:{height:.0}px")>
                <div inner_html=svg></div>
                {chips}
            </div>
        }
        .into_any()
    };

    let inspector = move || {
        controller.revision.track();
        let Some(at) = controller.active_key() else {
            return ().into_any();
        };
        let (label, input, value, why, format, reads, read_by) =
            controller.with_untracked(|book| {
                let host = book.host();
                let profile = book.profile().cloned();
                let cell = book.cell(at);
                let (input, value, why) = match &cell {
                    Some(cell) => {
                        let shown = cells::shown(cell, profile.as_ref(), book.cell_format(at));
                        (
                            cells::input_text(cell.input()),
                            shown.text,
                            cells::provenance_text(host, cell),
                        )
                    }
                    None => (String::new(), String::new(), "empty".to_owned()),
                };
                let reads: Vec<String> = host
                    .precedents(at)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|area| cells::area_label(host, area))
                    .collect();
                let read_by: Vec<String> = host
                    .dependents(at)
                    .into_iter()
                    .map(|cell| cells::cell_label(host, cell))
                    .collect();
                (
                    cells::cell_label(host, at),
                    input,
                    value,
                    why,
                    host.number_format(at).unwrap_or("General").to_owned(),
                    reads,
                    read_by,
                )
            });
        let list = |items: Vec<String>| {
            if items.is_empty() {
                "nothing".to_owned()
            } else if items.len() > 20 {
                format!("{} and {} more", items[..20].join(", "), items.len() - 20)
            } else {
                items.join(", ")
            }
        };
        let (reads_count, read_by_count) = (reads.len(), read_by.len());
        view! {
            <dl data-dnacalc="canvas-inspector">
                <dt>"Cell"</dt><dd data-dnacalc="canvas-cell">{label}</dd>
                <dt>"Entered"</dt><dd>{input}</dd>
                <dt>"Value"</dt><dd data-dnacalc="canvas-inspector-value">{value}</dd>
                <dt>"Where the value came from"</dt><dd>{why}</dd>
                <dt>"Number format"</dt><dd>{format}</dd>
                <dt>{format!("Reads ({reads_count})")}</dt><dd data-dnacalc="canvas-reads">{list(reads)}</dd>
                <dt>{format!("Read by ({read_by_count})")}</dt><dd data-dnacalc="canvas-read-by">{list(read_by)}</dd>
            </dl>
        }
        .into_any()
    };

    view! {
        <section class="dc-canvas" data-skin="canvas" data-dnacalc="canvas" tabindex="0" on:keydown=on_key>
            <div class="dc-stage">
                <div class="dc-bar">
                    <button type="button" data-dnacalc="canvas-back-more" on:click=move |_| grow(back, 1)>"[ reads further"</button>
                    <button type="button" data-dnacalc="canvas-back-less" on:click=move |_| grow(back, -1)>"fewer"</button>
                    <span data-dnacalc="canvas-depth">{move || format!("{} back, {} forward", back.get(), forward.get())}</span>
                    <button type="button" data-dnacalc="canvas-forward-less" on:click=move |_| grow(forward, -1)>"fewer"</button>
                    <button type="button" data-dnacalc="canvas-forward-more" on:click=move |_| grow(forward, 1)>"read by further ]"</button>
                </div>
                {chips}
            </div>
            <aside>{inspector}</aside>
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use calc_host::value::{IdAllocator, SheetId};

    fn sheet() -> SheetId {
        let mut ids: IdAllocator<SheetId> = IdAllocator::new();
        ids.issue().unwrap()
    }

    /// A1 = 1, B1 = A1*2, C1 = SUM(A1:B1), D1 = C1+B1, E1 = SUM(A1:A9).
    fn graph(
        s: SheetId,
    ) -> (
        impl Fn(CellKey) -> Option<Vec<GridArea>>,
        impl Fn(CellKey) -> Vec<CellKey>,
    ) {
        let at = move |c: u32| CellKey::new(s, 1, c).unwrap();
        let area = move |a: CellKey, b: CellKey| GridArea::new(a, b).unwrap();
        let precedents = move |cell: CellKey| match cell.column() {
            2 => Some(vec![area(at(1), at(1))]),
            3 => Some(vec![area(at(1), at(2))]),
            4 => Some(vec![area(at(3), at(3)), area(at(2), at(2))]),
            5 => Some(vec![area(at(1), CellKey::new(s, 9, 1).unwrap())]),
            _ => None,
        };
        let dependents = move |cell: CellKey| match cell.column() {
            1 => vec![at(2), at(3), at(5)],
            2 => vec![at(3), at(4)],
            3 => vec![at(4)],
            _ => Vec::new(),
        };
        (precedents, dependents)
    }

    /// Ported from flow.rs `trace_includes_precedents_and_dependents_to_depth` and
    /// `layout_ranks_inputs_left_and_results_right`: inputs in the columns to the left, readers
    /// to the right, as deep as asked.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn the_trace_puts_inputs_left_and_readers_right() {
        let s = sheet();
        let at = |c: u32| CellKey::new(s, 1, c).unwrap();
        let (precedents, dependents) = graph(s);
        let traced = trace(at(4), 1, 1, PER_COLUMN, &precedents, &dependents);
        assert_eq!(traced.columns.get(&0), Some(&vec![Node::Cell(at(4))]));
        assert_eq!(
            traced.columns.get(&-1),
            Some(&vec![Node::Cell(at(3)), Node::Cell(at(2))])
        );
        assert_eq!(traced.columns.get(&1), None, "nothing reads D1");
        // Two steps back: C1's range A1:B1 is one chip; B1's A1 is in it, not a cell of its own.
        let traced = trace(at(4), 2, 0, PER_COLUMN, &precedents, &dependents);
        let area = GridArea::new(at(1), at(2)).unwrap();
        assert_eq!(
            traced.columns.get(&-2),
            Some(&vec![Node::Area(area), Node::Cell(at(1))])
        );
        assert!(
            traced
                .wires
                .contains(&(Node::Area(area), Node::Cell(at(3))))
        );
        assert!(
            traced
                .wires
                .contains(&(Node::Cell(at(1)), Node::Cell(at(2))))
        );
        // Forward from A1: B1, C1 and E1 read it; then D1 reads B1 and C1, once.
        let traced = trace(at(1), 0, 2, PER_COLUMN, &precedents, &dependents);
        assert_eq!(
            traced.columns.get(&1),
            Some(&vec![
                Node::Cell(at(2)),
                Node::Cell(at(3)),
                Node::Cell(at(5))
            ])
        );
        assert_eq!(traced.columns.get(&2), Some(&vec![Node::Cell(at(4))]));
        assert!(
            traced
                .wires
                .contains(&(Node::Cell(at(2)), Node::Cell(at(4))))
        );
        assert!(
            traced
                .wires
                .contains(&(Node::Cell(at(3)), Node::Cell(at(4))))
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_full_column_counts_what_it_leaves_out() {
        let s = sheet();
        let at = |c: u32| CellKey::new(s, 1, c).unwrap();
        let (precedents, dependents) = graph(s);
        let traced = trace(at(1), 0, 1, 2, &precedents, &dependents);
        assert_eq!(traced.columns.get(&1).map(Vec::len), Some(2));
        assert_eq!(traced.hidden.get(&1), Some(&1));
        // The wires reach only chips that are shown.
        assert!(
            traced
                .wires
                .iter()
                .all(|(from, to)| traced.place(*from).is_some() && traced.place(*to).is_some())
        );
        assert!(wires_svg(&traced).starts_with("<svg "));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_literal_has_only_readers() {
        let s = sheet();
        let at = |c: u32| CellKey::new(s, 1, c).unwrap();
        let (precedents, dependents) = graph(s);
        let traced = trace(at(1), 3, 0, PER_COLUMN, &precedents, &dependents);
        assert_eq!(traced.columns.len(), 1);
        assert!(traced.wires.is_empty());
    }
}
