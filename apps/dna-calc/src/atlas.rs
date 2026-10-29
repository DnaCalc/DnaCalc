//! The atlas skin: the workbook's map. Its structure (each sheet with its used area, its inputs and
//! its formula cells) and its calculation (the mode, the authored revision, the undo and redo
//! depth, the last timings, and every formula cell counted by provenance: calculated, refused,
//! the file's stored value, stale) with the refusals grouped by reason. The skin only reads: it
//! dispatches no intent.
//!
//! A formula cell is one Excel counts as holding a formula (calc-host's `holds_formula`: legacy
//! array members and data-table cells included, a dynamic array's spilled cells not), the set a
//! corpus comparison's inventory holds. The counts follow the corpus rule: a refused formula is a
//! failure, never left out, so the atlas shows calculated against all formula cells.
//!
//! Ported from DnaTreeCalc (commit bf6dfd7) `src/dnacalc-stage-atlas/src/lib.rs` (the structure
//! map and the calculation panel, each derived from the workbook on every change and never
//! blank), over calc-host's projections ([`crate::workbook::Workbook::census`]). The defined-name
//! catalogue is not listed: calc-host does not expose the names yet.

use crate::controller::Controller;
use crate::skin::{Skin, SkinContext, SkinId};
use calc_host::CalcMode;
use leptos::prelude::*;

#[derive(Clone, Copy, Debug, Default)]
pub struct AtlasSkin;

impl Skin for AtlasSkin {
    fn id(&self) -> SkinId {
        SkinId::Atlas
    }

    fn title(&self) -> &'static str {
        "Atlas"
    }

    fn mount(&self, ctx: SkinContext) -> AnyView {
        atlas_view(ctx.controller).into_any()
    }
}

fn mode_name(mode: CalcMode) -> &'static str {
    match mode {
        CalcMode::Automatic => "automatic",
        CalcMode::AutomaticExceptTables => "automatic except tables",
        CalcMode::Manual => "manual",
    }
}

fn atlas_view(controller: Controller) -> impl IntoView {
    let census = Memo::new(move |_| {
        controller.revision.track();
        controller.update_untracked(|book| book.census())
    });
    let sheets = move || {
        census.with(|census| {
            census
                .sheets
                .iter()
                .map(|sheet| {
                    view! {
                        <tr data-dnacalc="atlas-sheet" data-sheet=sheet.name.clone()>
                            <td>{sheet.name.clone()}</td>
                            <td>{sheet.used.clone().unwrap_or_else(|| "empty".to_owned())}</td>
                            <td>{sheet.inputs}</td>
                            <td>{sheet.formulas}</td>
                            <td>{sheet.calculated}</td>
                            <td>{sheet.refused}</td>
                            <td>{sheet.file_cached}</td>
                            <td>{sheet.stale}</td>
                        </tr>
                    }
                })
                .collect_view()
        })
    };
    let totals = move || {
        census.with(|census| {
            let formulas = census.formulas();
            let calculated = census.calculated();
            let rate = if formulas == 0 {
                "no formula cells".to_owned()
            } else {
                format!("{:.2}% calculated", 100.0 * calculated as f64 / formulas as f64)
            };
            format!(
                "{formulas} formula cells: {calculated} calculated, {} refused, {} file cache, {} stale; {rate}",
                census.refused(),
                census.file_cached(),
                census.stale()
            )
        })
    };
    let refusals = move || {
        census.with(|census| {
            let mut reasons: Vec<(String, usize)> =
                census.refusals.iter().map(|(reason, n)| (reason.clone(), *n)).collect();
            reasons.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            reasons
                .into_iter()
                .take(30)
                .map(|(reason, n)| view! { <li data-dnacalc="atlas-refusal">{format!("{n} x {reason}")}</li> })
                .collect_view()
        })
    };
    let hud = move || {
        let (mode, revision, (undo, redo), intents, name, changed) = controller.with(|book| {
            let session = book.session();
            (
                session.calc_mode(),
                book.host().revision().get(),
                session.history_depth(),
                session.intent_log().len(),
                book.name().to_owned(),
                book.changed(),
            )
        });
        let timings = controller.timings.get();
        let ms = |value: Option<f64>| {
            value.map_or_else(|| "none".to_owned(), |ms| format!("{ms:.0} ms"))
        };
        view! {
            <ul data-dnacalc="atlas-hud">
                <li>{format!("Workbook {name}{}", if changed { ", changed since opened or saved" } else { "" })}</li>
                <li data-dnacalc="atlas-mode">{format!("Calculation: {}", mode_name(mode))}</li>
                <li>{format!("Authored revision {revision}; {undo} steps to undo, {redo} to redo; {intents} intents logged")}</li>
                <li data-dnacalc="atlas-memory">{crate::controller::memory_bytes().map_or_else(|| "Memory: not in a browser".to_owned(), |bytes| format!("Memory: {:.0} MB of wasm linear memory", bytes / 1_048_576.0))}</li>
                <li data-dnacalc="atlas-timings">{format!("Open and first calculation: {}; last intent (an edit, undo, selection or recalculation): {}", ms(timings.open_ms), ms(timings.last_intent_ms))}</li>
            </ul>
        }
    };

    view! {
        <section class="dc-atlas" data-skin="atlas">
            <h2>"Calculation"</h2>
            {hud}
            <p data-dnacalc="atlas-totals">{totals}</p>
            <h2>"Sheets"</h2>
            <table>
                <thead>
                    <tr>
                        <th>"Sheet"</th><th>"Used area"</th><th>"Inputs"</th><th>"Formulas"</th>
                        <th>"Calculated"</th><th>"Refused"</th><th>"File cache"</th><th>"Stale"</th>
                    </tr>
                </thead>
                <tbody>{sheets}</tbody>
            </table>
            <h2>"Refusals by reason"</h2>
            <ul class="dc-refusals">{refusals}</ul>
            {move || census.with(|census| census.refusals.is_empty()).then(|| view! { <p data-dnacalc="atlas-no-refusals">"No formula is refused."</p> })}
        </section>
    }
}
