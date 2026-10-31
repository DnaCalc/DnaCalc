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
//! [`calc_host::HostState::dependents`], the engine's own edges), traced by the workbook's server
//! ([`view_model`], dc-p96); the skin changes nothing but the selection and, with F9,
//! recalculates.
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
use crate::wire;
use crate::workbook::Workbook;
use calc_host::value::{CellKey, GridArea};
use leptos::prelude::*;
use std::collections::{BTreeMap, BTreeSet};

/// The most chips one column shows; the rest are counted.
pub const PER_COLUMN: usize = 12;
/// The deepest trace either way.
pub const MAX_DEPTH: u8 = 6;

const COLUMN_WIDTH: f64 = 230.0;
const CHIP_WIDTH: f64 = 200.0;
const ROW_HEIGHT: f64 = 124.0;
const CHIP_HEIGHT: f64 = 110.0;

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
        canvas_view(ctx.controller, ctx.active_skin).into_any()
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
        rows * ROW_HEIGHT + 64.0,
    )
}

/// The canvas around `center` as the page draws it (the workbook's server works it out, dc-p96):
/// the trace, each chip's text, the wires and the centre cell's inspector.
pub fn view_model(book: &Workbook, center: CellKey, back: u8, forward: u8) -> wire::Canvas {
    let host = book.host();
    let profile = book.profile().cloned();
    let trace = trace(
        center,
        back,
        forward,
        PER_COLUMN,
        |at| host.precedents(at),
        |at| host.dependents(at),
    );
    let center_sheet = center.sheet();
    let mut chips = Vec::new();
    let mut hidden = Vec::new();
    for (column, nodes) in &trace.columns {
        for (row, node) in nodes.iter().enumerate() {
            let (x, y) = position(&trace, *column, row);
            let chip = match *node {
                Node::Cell(at) => {
                    let label = if at.sheet() == center_sheet {
                        cells::address(at.row(), at.column())
                    } else {
                        cells::cell_label(host, at)
                    };
                    let (input, value, provenance) = match book.cell(at) {
                        Some(cell) => {
                            let shown = cells::shown(&cell, profile.as_ref(), book.cell_format(at));
                            let value = if shown.provenance == Provenance::Refused {
                                "refused".to_owned()
                            } else {
                                shown.text
                            };
                            (cells::input_text(cell.input()), value, shown.provenance)
                        }
                        None => (String::new(), String::new(), Provenance::Empty),
                    };
                    wire::Chip {
                        column: *column,
                        x,
                        y,
                        reference: cells::cell_label(host, at),
                        sheet: wire::Sheet::of(at.sheet()),
                        label,
                        cell: Some(wire::Key::of(at)),
                        input,
                        value,
                        provenance,
                        count: 1,
                    }
                }
                Node::Area(area) => wire::Chip {
                    column: *column,
                    x,
                    y,
                    label: if area.sheet() == center_sheet {
                        format!(
                            "{}:{}",
                            cells::address(area.first().row(), area.first().column()),
                            cells::address(area.last().row(), area.last().column())
                        )
                    } else {
                        cells::area_label(host, area)
                    },
                    reference: cells::area_label(host, area),
                    sheet: wire::Sheet::of(area.sheet()),
                    cell: None,
                    input: String::new(),
                    value: String::new(),
                    provenance: Provenance::Empty,
                    count: area.cell_count(),
                },
            };
            chips.push(chip);
        }
        if let Some(count) = trace.hidden.get(column) {
            let (x, y) = position(&trace, *column, nodes.len());
            hidden.push((*column, x, y, *count));
        }
    }
    let (width, height) = plane_size(&trace);
    let cell = book.cell(center);
    let (input, value, why, reason_is_detail) = match &cell {
        Some(cell) => {
            let shown = cells::shown(cell, profile.as_ref(), book.cell_format(center));
            (
                cells::input_text(cell.input()),
                shown.text,
                match cells::error_meaning(cell) {
                    Some(meaning) => format!("{}; {meaning}", cells::provenance_text(host, cell)),
                    None => cells::provenance_text(host, cell),
                },
                shown.provenance == Provenance::Calculated && shown.kind != "error",
            )
        }
        None => (String::new(), String::new(), "empty".to_owned(), false),
    };
    wire::Canvas {
        wires_svg: wires_svg(&trace),
        chips,
        hidden,
        width,
        height,
        inspector: wire::Inspector {
            label: cells::cell_label(host, center),
            input,
            value,
            why,
            reason_is_detail,
            format: host.number_format(center).unwrap_or("General").to_owned(),
            reads: host
                .precedents(center)
                .unwrap_or_default()
                .into_iter()
                .map(|area| cells::area_label(host, area))
                .collect(),
            read_by: host
                .dependents(center)
                .into_iter()
                .map(|cell| cells::cell_label(host, cell))
                .collect(),
        },
    }
}

fn label_parts(label: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut quoted = false;
    let mut chars = label.char_indices().peekable();
    while let Some((i, ch)) = chars.next() {
        if ch == '\'' {
            if quoted && chars.peek().is_some_and(|(_, next)| *next == '\'') {
                chars.next();
            } else {
                quoted = !quoted;
            }
        } else if !quoted && matches!(ch, '!' | ':') {
            let end = i + ch.len_utf8();
            parts.push(label[start..end].to_owned());
            start = end;
        }
    }
    if start < label.len() {
        parts.push(label[start..].to_owned());
    }
    parts
}

fn related_summary(items: &[String]) -> String {
    if items.is_empty() {
        return "nothing".into();
    }
    let shown = items
        .iter()
        .take(20)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    if items.len() > 20 {
        format!("{shown}; {} more not shown", items.len() - 20)
    } else {
        shown
    }
}
fn zoom_step(current: u16, by: i16) -> u16 {
    current.saturating_add_signed(by).clamp(50, 200)
}
fn pan(dx: i32, dy: i32) {
    #[cfg(target_arch = "wasm32")]
    if let Some(view) = web_sys::window().and_then(|w| w.document()).and_then(|d| {
        d.query_selector("[data-dnacalc=canvas-viewport]")
            .ok()
            .flatten()
    }) {
        view.set_scroll_left(view.scroll_left().saturating_add(dx));
        view.set_scroll_top(view.scroll_top().saturating_add(dy));
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = (dx, dy);
}
fn center_view() {
    #[cfg(target_arch = "wasm32")]
    if let Some(document) = web_sys::window().and_then(|w| w.document()) {
        if let (Ok(Some(view)), Ok(Some(node))) = (
            document.query_selector("[data-dnacalc=canvas-viewport]"),
            document.query_selector("[data-dnacalc=canvas-node][data-center=true]"),
        ) {
            let v = view.get_bounding_client_rect();
            let n = node.get_bounding_client_rect();
            pan(
                (n.x() - v.x() + n.width() / 2.0 - v.width() / 2.0) as i32,
                (n.y() - v.y() + n.height() / 2.0 - v.height() / 2.0) as i32,
            );
        }
    }
}
fn canvas_view(controller: Controller, active_skin: RwSignal<Option<String>>) -> impl IntoView {
    let back = RwSignal::new(1u8);
    let forward = RwSignal::new(1u8);
    let zoom = RwSignal::new(100u16);
    let traced = RwSignal::new(None::<wire::Canvas>);
    let range = RwSignal::new(None::<wire::Chip>);
    let loading = RwSignal::new(true);
    let failure = RwSignal::new(String::new());
    let refresh = RwSignal::new(0u64);
    let epoch = StoredValue::new(0u64);
    let focus_center = StoredValue::new(false);
    Effect::new(move |_| {
        let revision = controller.revision.get();
        let generation = controller.generation.get();
        refresh.track();
        let (back, forward) = (back.get(), forward.get());
        epoch.update_value(|n| *n += 1);
        let asked = epoch.get_value();
        traced.set(None);
        range.set(None);
        failure.set(String::new());
        let Some(center) = controller.active_key() else {
            loading.set(false);
            failure.set("Choose a cell in Grid or enter its address above.".into());
            return;
        };
        loading.set(true);
        controller.ask_result(
            wire::Request::Canvas {
                center: wire::Key::of(center),
                back,
                forward,
            },
            move |reply| {
                if epoch.try_get_value() != Some(asked)
                    || controller.revision.try_get_untracked() != Some(revision)
                    || controller.generation.try_get_untracked() != Some(generation)
                {
                    return;
                }
                match reply {
                    Ok(wire::Reply::Canvas(found)) => {
                        traced.try_set(Some(found));
                        loading.try_set(false);
                        let focus = focus_center.get_value();
                        focus_center.set_value(false);
                        #[cfg(target_arch = "wasm32")]
                        set_timeout(
                            move || {
                                center_view();
                                if focus {
                                    crate::app::focus(
                                        "[data-dnacalc=canvas-node][data-center=true]",
                                    );
                                }
                            },
                            std::time::Duration::from_millis(20),
                        );
                        #[cfg(not(target_arch = "wasm32"))]
                        let _ = focus;
                    }
                    Err(error) => {
                        failure.try_set(format!("Dependency view unavailable: {error}"));
                        loading.try_set(false);
                    }
                    _ => {}
                }
            },
        );
    });
    let grow = move |signal: RwSignal<u8>, by: i8| {
        signal.update(|d| *d = d.saturating_add_signed(by).clamp(0, MAX_DEPTH))
    };
    let reset = move || {
        back.set(1);
        forward.set(1);
        zoom.set(100);
        range.set(None);
        #[cfg(target_arch = "wasm32")]
        set_timeout(center_view, std::time::Duration::from_millis(20));
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
        "Escape" => reset(),
        "F9" => {
            ev.prevent_default();
            controller.recalculate();
        }
        _ => {}
    };
    let open_source = move |reference: String| {
        controller.go_to_text(reference, move |ok| {
            if ok {
                active_skin.set(Some("grid".into()));
                #[cfg(target_arch = "wasm32")]
                set_timeout(
                    move || crate::app::focus("[data-dnacalc=grid][data-grid-active=true]"),
                    std::time::Duration::from_millis(20),
                );
            }
        });
    };
    let chips = move || {
        let Some(canvas) = traced.get() else {
            return ().into_any();
        };
        let nodes=canvas.chips.into_iter().map(|chip| {
            let style=format!("left:{:.0}px;top:{:.0}px;width:{CHIP_WIDTH:.0}px;height:{CHIP_HEIGHT:.0}px",chip.x,chip.y);
            let is_range=chip.cell.is_none();
            let hidden=controller.summary.with(|s|!s.sheets.iter().any(|(id,_)|id.id()==chip.sheet.id()));
            let reference=chip.reference.clone();
            let column=chip.column;
            let selected=Memo::new(move |_|range.with(|chosen|if is_range{chosen.as_ref().is_some_and(|r|r.reference==reference)}else{chosen.is_none()&&column==0}));
            let accessible=format!("Inspect {}{}",chip.reference,if is_range {" range"}else{""});
            let address=label_parts(&chip.reference).into_iter().map(|part|view!{<span>{part}</span><wbr/>}).collect_view();
            let formula=chip.input.starts_with('=').then(||chip.input.clone());
            let summary=if is_range {format!("{} cells · range summary",chip.count)}else{chip.value.clone()};
            let state=if is_range {"terminal range"}else{chip.provenance.name()};
            let description=format!("{summary}; {state}");
            let selected_chip=chip.clone();
            let click=move |_| {
                if hidden&&!is_range {failure.set("This source is on a hidden sheet. Unhide it in Sheet tools before navigation.".into());return;}
                if let Some(at)=selected_chip.cell.and_then(|k|k.cell()) {controller.navigate_to(at,move |ok|{if ok&&focus_center.try_update_value(|v|*v=true).is_some(){refresh.try_update(|n|*n+=1);}});}
                else {range.set(Some(selected_chip.clone()));}
            };
            view!{<button type="button" class="dc-node" data-dnacalc="canvas-node" data-kind={if is_range{"area"}else{"cell"}} data-node=chip.label data-column=chip.column.to_string() data-center=(chip.column==0).to_string() data-selected=move ||selected.get().to_string() data-provenance=chip.provenance.name() aria-label=accessible aria-description=description aria-disabled=(hidden&&!is_range).to_string() title={if hidden{"Hidden sheet: unhide it in Sheet tools before source navigation."}else{""}} aria-pressed=move ||selected.get().to_string() use:css=style on:click=click>
                <b>{address}</b>{formula.map(|f|view!{<span class="dc-f" data-dnacalc="canvas-formula">{f}</span>})}<span class="dc-v" data-dnacalc="canvas-value">{summary}</span><span class="dc-node-state" data-dnacalc="canvas-state">{state}</span>
            </button>}
        }).collect_view();
        let omissions=canvas.hidden.into_iter().map(|(column,x,y,count)|view!{<div class="dc-hidden" data-dnacalc="canvas-hidden" use:css=format!("left:{x:.0}px;top:{y:.0}px;width:{CHIP_WIDTH:.0}px")>{format!("{count} additional {} at depth {} not shown",if column<0{"inputs"}else{"readers"},column.unsigned_abs())}</div>}).collect_view();
        let scale = f64::from(zoom.get()) / 100.0;
        view!{<div class="dc-plane-space" use:css=format!("width:{:.0}px;height:{:.0}px",canvas.width*scale,canvas.height*scale)><div class="dc-plane" use:css=format!("width:{:.0}px;height:{:.0}px;transform:scale({scale});transform-origin:0 0",canvas.width,canvas.height)><div inner_html=canvas.wires_svg></div>{nodes}{omissions}</div></div>}.into_any()
    };
    let inspector = move || {
        if let Some(chip) = range.get() {
            let reference = chip.reference.clone();
            let hidden = controller
                .summary
                .with(|s| !s.sheets.iter().any(|(id, _)| id.id() == chip.sheet.id()));
            return view!{<div data-dnacalc="canvas-range"><h3>{chip.reference}</h3><p>{format!("{} cells. A terminal range summary; its cells are not expanded into this trace.",chip.count)}</p><button data-dnacalc="canvas-show-source" disabled=hidden on:click=move |_|open_source(reference.clone())>"Show source in Grid"</button><p>{if hidden{"Hidden sheet: unhide it in Sheet tools before source navigation."}else{"The active formula remains selected until you open this range in Grid."}}</p></div>}.into_any();
        }
        let Some(canvas) = traced.get() else {
            return ().into_any();
        };
        let wire::Inspector {
            label,
            input,
            value,
            why,
            reason_is_detail,
            format,
            reads,
            read_by,
        } = canvas.inspector;
        let source = label.clone();
        let (reads_count, read_by_count) = (reads.len(), read_by.len());
        let formula = input.starts_with('=').then_some(input);
        let visible_reason =
            (!reason_is_detail).then(|| view! {<p data-dnacalc="canvas-reason">{why.clone()}</p>});
        let detail_reason =
            reason_is_detail.then(|| view! {<p data-dnacalc="canvas-reason">{why}</p>});
        view!{<div data-dnacalc="canvas-inspector"><h3 data-dnacalc="canvas-cell">{label}</h3>{formula.map(|f|view!{<pre class="dc-inspector-formula">{f}</pre>})}<p class="dc-inspector-value" data-dnacalc="canvas-inspector-value">{value}</p>{visible_reason}<button data-dnacalc="canvas-show-source" on:click=move |_|open_source(source.clone())>"Show source in Grid"</button><details data-dnacalc="canvas-details"><summary>"Calculation, format and dependency details"</summary>{detail_reason}<dl><dt>"Number format"</dt><dd>{format}</dd><dt>{format!("Reads ({reads_count})")}</dt><dd data-dnacalc="canvas-reads">{related_summary(&reads)}</dd><dt>{format!("Read by ({read_by_count})")}</dt><dd data-dnacalc="canvas-read-by">{related_summary(&read_by)}</dd></dl></details></div>}.into_any()
    };
    view! {
        <section class="dc-canvas" data-skin="canvas" data-dnacalc="canvas" tabindex="0" on:keydown=on_key>
            <div class="dc-stage"><div class="dc-bar" role="group" aria-label="Dependency trace depth">
                <button data-dnacalc="canvas-back-more" disabled={move ||back.get()>=MAX_DEPTH} on:click=move |_|grow(back,1)>"More inputs ["</button><button data-dnacalc="canvas-back-less" aria-label="Fewer input levels" disabled=move ||back.get()==0 on:click=move |_|grow(back,-1)>"Fewer inputs"</button>
                <span data-dnacalc="canvas-depth">{move ||format!("{} back, {} forward",back.get(),forward.get())}</span>
                <button data-dnacalc="canvas-forward-less" aria-label="Fewer reader levels" disabled=move ||forward.get()==0 on:click=move |_|grow(forward,-1)>"Fewer readers"</button><button data-dnacalc="canvas-forward-more" disabled={move ||forward.get()>=MAX_DEPTH} on:click=move |_|grow(forward,1)>"More readers ]"</button>
            </div><div class="dc-bar" role="group" aria-label="Dependency view position and zoom">
                <button data-dnacalc="canvas-pan-left" aria-label="Pan dependency view left" on:click=move |_|pan(-180,0)>"←"</button><button data-dnacalc="canvas-pan-right" aria-label="Pan dependency view right" on:click=move |_|pan(180,0)>"→"</button><button aria-label="Pan dependency view up" on:click=move |_|pan(0,-140)>"↑"</button><button aria-label="Pan dependency view down" on:click=move |_|pan(0,140)>"↓"</button>
                <button data-dnacalc="canvas-zoom-out" aria-label="Zoom dependency view out" disabled=move ||zoom.get()==50 on:click=move |_|zoom.update(|z|*z=zoom_step(*z,-10))>"−"</button><span data-dnacalc="canvas-zoom">{move ||format!("{}%",zoom.get())}</span><button data-dnacalc="canvas-zoom-in" aria-label="Zoom dependency view in" disabled=move ||zoom.get()==200 on:click=move |_|zoom.update(|z|*z=zoom_step(*z,10))>"+"</button><button data-dnacalc="canvas-center" aria-label="Center active cell" on:click=move |_|center_view()>"Center cell"</button><button data-dnacalc="canvas-reset" aria-label="Reset dependency view" on:click=move |_|reset()>"Reset"</button>
            </div><details class="dc-canvas-guide"><summary>"About this dependency view"</summary><p class="dc-canvas-contract">{format!("Dependency inspector · inputs left, readers right · up to {MAX_DEPTH} levels and {PER_COLUMN} nodes per column. Ranges are summaries; cell edits use the formula field above.")}</p></details>
            <p role="status" data-dnacalc="canvas-status">{move ||if loading.get(){"Loading dependencies…".into()}else{failure.get()}}</p>
            {move ||(!loading.get()&&!failure.get().is_empty()).then(||view!{<button data-dnacalc="canvas-retry" on:click=move |_|refresh.update(|n|*n+=1)>"Retry dependency view"</button>})}
            <div class="dc-canvas-viewport" data-dnacalc="canvas-viewport" tabindex="0" role="region" aria-label="Dependency chips; scroll or use pan controls">{chips}</div></div>
            <aside aria-label="Dependency selection">{inspector}</aside>
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use calc_host::value::{IdAllocator, SheetId};

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn labels_break_at_reference_boundaries_and_zoom_is_bounded() {
        assert_eq!(
            label_parts("'Long Sheet'!A1:Z99"),
            vec!["'Long Sheet'!", "A1:", "Z99"]
        );
        assert_eq!(
            label_parts("'Long!Sheet'' name'!A1:Z99"),
            vec!["'Long!Sheet'' name'!", "A1:", "Z99"]
        );
        let labels = (1..=25).map(|n| format!("A{n}")).collect::<Vec<_>>();
        let summary = related_summary(&labels);
        assert!(summary.contains("A20; 5 more not shown"));
        assert!(!summary.contains("A21"));
        assert_eq!(zoom_step(50, -10), 50);
        assert_eq!(zoom_step(200, 10), 200);
        assert_eq!(zoom_step(100, 10), 110);
    }
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
