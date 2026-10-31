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
//! The worker projects the workbook graph (dc-p96); the skin changes nothing but the selection and, with F9,
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

use crate::controller::Controller;
use crate::skin::{Skin, SkinContext, SkinId};
use crate::style::css;
use crate::wire;
#[cfg(test)]
use calc_value::{CellKey, GridArea};
use leptos::prelude::*;

pub use dna_calc_model::canvas::*;
#[derive(Clone, Copy, Debug, Default)]
pub struct CanvasSkin;

impl Skin for CanvasSkin {
    fn id(&self) -> SkinId {
        SkinId::Canvas
    }

    fn title(&self) -> &'static str {
        "Canvas"
    }

    fn focus_target(&self) -> &'static str {
        "[data-dnacalc=canvas]"
    }

    fn mount(&self, ctx: SkinContext) -> AnyView {
        canvas_view(ctx.controller).into_any()
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
fn canvas_view(controller: Controller) -> impl IntoView {
    let back = controller.view_field::<dna_calc_model::CanvasState, _>(
        SkinId::Canvas,
        |s| &s.back,
        |s| &mut s.back,
    );
    let forward = controller.view_field::<dna_calc_model::CanvasState, _>(
        SkinId::Canvas,
        |s| &s.forward,
        |s| &mut s.forward,
    );
    let zoom = controller.view_field::<dna_calc_model::CanvasState, _>(
        SkinId::Canvas,
        |s| &s.zoom,
        |s| &mut s.zoom,
    );
    let traced = RwSignal::new(None::<wire::Canvas>);
    let range = RwSignal::new(None::<wire::Chip>);
    let loading = RwSignal::new(true);
    let failure = RwSignal::new(String::new());
    let refresh = RwSignal::new(0u64);
    let epoch = StoredValue::new(0u64);
    let focus_center = controller.view_field::<dna_calc_model::CanvasState, _>(
        SkinId::Canvas,
        |s| &s.pending_focus,
        |s| &mut s.pending_focus,
    );
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
                        focus_center.set_value(None);
                        #[cfg(target_arch = "wasm32")]
                        set_timeout(
                            move || {
                                center_view();
                                if let Some(ticket) = focus {
                                    controller.focus_projection(
                                        SkinId::Canvas,
                                        "[data-dnacalc=canvas-node][data-center=true]".into(),
                                        ticket,
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
    let grow = move |signal: crate::model_adapter::ModelField<u8>, by: i8| {
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
        _ => {}
    };
    let open_source = move |reference: String| controller.reveal_reference(reference, SkinId::Grid);
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
                if let Some(at)=selected_chip.cell.and_then(|k|k.cell()) {let ticket=controller.focus_snapshot();controller.navigate_to(at,move |ok|{if ok&&focus_center.try_update_value(|v|*v=Some(ticket)).is_some(){refresh.try_update(|n|*n+=1);}});}
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
    use calc_value::{IdAllocator, SheetId};

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
