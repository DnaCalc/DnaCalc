//! The atlas skin: the workbook's map. Its structure (each sheet with its used area, its inputs and
//! its formula cells) and its calculation (the mode, the authored revision, the undo and redo
//! depth, the last timings, and every formula cell counted by provenance: calculated, refused,
//! the file's stored value, stale) with the refusals grouped by reason. Navigation uses the shared selection through worker messages.
//!
//! A formula cell is one Excel counts as holding a formula (calc-host's `holds_formula`: legacy
//! array members and data-table cells included, a dynamic array's spilled cells not), the set a
//! corpus comparison's inventory holds. The counts follow the corpus rule: a refused formula is a
//! failure, never left out, so the atlas shows calculated against all formula cells.
//!
//! Ported from DnaTreeCalc (commit bf6dfd7) `src/dnacalc-stage-atlas/src/lib.rs` (the structure
//! map and the calculation panel, each derived from the workbook on every change and never
//! blank), over calc-host's projections (the worker census), asked of the
//! workbook's server after every change (dc-p96). The defined-name catalogue is not listed.

use crate::controller::Controller;
use crate::skin::{Skin, SkinContext, SkinId};
use crate::wire;
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

const REASON_PAGE: usize = 30;
fn reason_rows(census: &dna_calc_model::projection::Census, page: usize) -> Vec<(String, usize)> {
    let mut reasons: Vec<_> = census
        .refusals
        .iter()
        .map(|(reason, n)| (reason.clone(), *n))
        .collect();
    reasons.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    reasons
        .into_iter()
        .skip(page.saturating_mul(REASON_PAGE))
        .take(REASON_PAGE)
        .collect()
}

fn atlas_view(controller: Controller) -> impl IntoView {
    let atlas = RwSignal::new(None::<wire::Atlas>);
    let failure = RwSignal::new(String::new());
    let epoch = StoredValue::new(0u64);
    Effect::new(move |_| {
        controller.revision.track();
        controller.summary.track();
        epoch.update_value(|n| *n += 1);
        let requested = epoch.get_value();
        atlas.set(None);
        failure.set(String::new());
        controller.ask_result(wire::Request::Atlas, move |reply| {
            if epoch.try_get_value() != Some(requested) {
                return;
            }
            match reply {
                Ok(wire::Reply::Atlas(found)) => {
                    atlas.try_set(Some(found));
                }
                Err(error) => {
                    failure.try_set(format!("Workbook overview unavailable: {error}"));
                }
                _ => {}
            }
        });
    });
    let census = Memo::new(move |_| {
        atlas.with(|a| a.as_ref().map(|a| a.census.clone()).unwrap_or_default())
    });
    let reason_page = controller.view_field::<dna_calc_model::AtlasState, _>(
        SkinId::Atlas,
        |s| &s.reason_page,
        |s| &mut s.reason_page,
    );
    let selected = controller.view_field::<dna_calc_model::AtlasState, _>(
        SkinId::Atlas,
        |s| &s.selected,
        |s| &mut s.selected,
    );
    let cell_page = controller.view_field::<dna_calc_model::AtlasState, _>(
        SkinId::Atlas,
        |s| &s.cell_page,
        |s| &mut s.cell_page,
    );
    Effect::new(move |_| {
        let revision = controller.revision.get();
        controller.update_view::<dna_calc_model::AtlasState>(SkinId::Atlas, |state| {
            state.observe_revision(revision)
        });
    });
    let affected = RwSignal::new(None::<wire::RefusalCells>);
    let affected_error = RwSignal::new(String::new());
    let cell_epoch = StoredValue::new(0u64);
    Effect::new(move |_| {
        if atlas.with(Option::is_none) {
            return;
        }
        let count = census.with(|c| c.refusals.len());
        reason_page.update(|p| *p = (*p).min(count.saturating_sub(1) / REASON_PAGE));
    });
    Effect::new(move |_| {
        let revision = controller.revision.get();
        let reason = selected.get();
        let page = cell_page.get();
        cell_epoch.update_value(|n| *n += 1);
        let requested = cell_epoch.get_value();
        affected.set(None);
        affected_error.set(String::new());
        let Some(reason) = reason else {
            return;
        };
        controller.ask_result(
            wire::Request::AtlasRefusals {
                reason,
                page: page as u64,
            },
            move |reply| {
                if cell_epoch.try_get_value() != Some(requested)
                    || controller.revision.try_get_untracked() != Some(revision)
                {
                    return;
                }
                match reply {
                    Ok(wire::Reply::AtlasRefusals(found)) => {
                        let last = found.total.saturating_sub(1) / 50;
                        if found.page > last {
                            cell_page.try_set(last);
                        } else {
                            affected.try_set(Some(found));
                        }
                    }
                    Err(error) => {
                        affected_error.try_set(format!("Affected cells unavailable: {error}"));
                    }
                    _ => {}
                }
            },
        );
    });
    let jump = move |at, skin| controller.reveal(at, skin);
    let sheets = move || {
        census.with(|c|c.sheets.iter().map(|s| {
        let at=s.identity.and_then(|s|calc_value::CellKey::new(s.id(),1,1).ok());
        let hidden=s.identity.is_some_and(|id|controller.summary.with(|v|v.hidden_sheets.iter().any(|(s,_,_)|*s==id)));
        view!{<tr data-dnacalc="atlas-sheet" data-sheet=s.name.clone()>
            <td><button data-dnacalc="atlas-sheet-jump" disabled=hidden on:click=move |_|if let Some(at)=at{jump(at, SkinId::Grid);}>{s.name.clone()}</button>{hidden.then_some(" (hidden; unhide in Sheet tools)")}</td>
            <td>{s.used.clone().unwrap_or_else(||"empty".into())}</td><td>{s.inputs}</td><td>{s.formulas}</td><td>{s.calculated}</td><td>{s.refused}</td><td>{s.file_cached}</td><td>{s.stale}</td><td>{s.pending()}</td>
        </tr>}
    }).collect_view())
    };
    let totals = move || {
        if atlas.with(Option::is_none) {
            return "Counting the workbook's cells".into();
        }
        census.with(|c| {
            let f=c.formulas();let n=c.calculated();
            let rate=if f==0 {"no formula cells".into()}else{format!("{:.2}% calculated",100.0*n as f64/f as f64)};
            format!("{f} formula cells: {n} calculated, {} refused, {} file cache, {} stale, {} pending; {rate}",c.refused(),c.file_cached(),c.stale(),c.pending())
        })
    };
    let reasons = move || {
        reason_rows(&census.get(),reason_page.get()).into_iter().map(|(reason,n)| {
        let select_reason=reason.clone();let detail_reason=reason.clone();let pressed_reason=reason.clone();
        view!{<li data-dnacalc="atlas-refusal">
            <button data-dnacalc="atlas-reason" data-reason=reason.clone() aria-expanded=move ||(selected.get().as_ref()==Some(&pressed_reason)).to_string() on:click=move |_|batch(||{selected.set(Some(select_reason.clone()));cell_page.set(0);})>{format!("{n} {} — {reason}",if n==1{"cell"}else{"cells"})}</button>
            {move ||(selected.get().as_ref()==Some(&detail_reason)).then(|| {
                let found=affected.get();
                let total=found.as_ref().map_or(0,|p|p.total);
                let cells=found.map(|p|p.cells.into_iter().map(|(key,label)| {
                    let at=key.cell();
                    let hidden=at.is_some_and(|at|controller.summary.with(|v|v.hidden_sheets.iter().any(|(s,_,_)|s.id()==at.sheet())));
                    let label=if hidden{format!("{label} (hidden; unhide in Sheet tools)")}else{label};
                    view!{<li data-dnacalc="atlas-affected-cell"><span class="dc-mono">{label}</span><button data-dnacalc="atlas-open-grid" disabled=hidden on:click=move |_|if let Some(at)=at{jump(at, SkinId::Grid);}>"Open in Grid"</button></li>}
                }).collect_view());
                view!{<div class="dc-atlas-affected"><p role="status" data-dnacalc="atlas-affected-count">{if affected.with(Option::is_none){"Loading affected cells…".into()}else{format!("{total} affected {} · page {} of {}",if total==1{"cell"}else{"cells"},cell_page.get()+1,total.div_ceil(50).max(1))}}</p><p role="status">{move ||affected_error.get()}</p><ul>{cells}</ul><div class="dc-atlas-pages"><button data-dnacalc="atlas-cells-previous" disabled=move ||cell_page.get()==0 on:click=move |_|cell_page.update(|p|*p=p.saturating_sub(1))>"Previous cells"</button><button data-dnacalc="atlas-cells-next" disabled=move ||affected.with(|p|p.as_ref().is_none_or(|p|(cell_page.get()+1).saturating_mul(50)>=p.total)) on:click=move |_|cell_page.update(|p|*p+=1)>"Next cells"</button></div></div>}
            })}
        </li>}
    }).collect_view()
    };
    let hud = move || {
        let wire::Atlas {
            mode,
            revision,
            undo,
            redo,
            intents,
            name,
            changed,
            memory,
            ..
        } = atlas.get().unwrap_or_default();
        let timings = controller.timings.get();
        let ms = |value: Option<f64>| {
            value.map_or_else(|| "none".to_owned(), |ms| format!("{ms:.0} ms"))
        };
        view! {
            <ul data-dnacalc="atlas-hud">
                <li>{format!("Workbook {name}{}", if changed { ", changed since opened or saved" } else { "" })}</li>
                <li data-dnacalc="atlas-mode">{format!("Calculation: {mode}")}</li>
                <li>{format!("Authored revision {revision}; {undo} steps to undo, {redo} to redo; {intents} intents logged")}</li>
                <li data-dnacalc="atlas-memory">{match (memory, crate::controller::memory_bytes()) {
                    (Some(worker), Some(page)) if crate::port::in_worker() => format!("Memory: {:.0} MB of wasm linear memory in the calculation worker, {:.0} MB on the page", worker / 1_048_576.0, page / 1_048_576.0),
                    (_, Some(page)) => format!("Memory: {:.0} MB of wasm linear memory", page / 1_048_576.0),
                    _ => "Memory: not in a browser".to_owned(),
                }}</li>
                <li data-dnacalc="atlas-timings">{format!("Open and first calculation: {}; last intent (an edit, undo, selection or recalculation): {}", ms(timings.open_ms), ms(timings.last_intent_ms))}</li>
            </ul>
        }
    };

    view! {
        <section class="dc-atlas" data-skin="atlas" aria-label="Workbook overview">
            <h2>"Workbook overview"</h2>
            <p role="status" data-dnacalc="atlas-totals">{totals}</p>
            <p class="dc-atlas-agreement" data-dnacalc="atlas-agreement">"Calculated coverage counts formulas DNA Calc evaluated. Agreement with Excel has not been measured in this view."</p>
            <p role="status">{move ||failure.get()}</p>
            {move ||(atlas.with(Option::is_some)&&census.with(|c|c.formulas()==0)).then(||view!{<p data-dnacalc="atlas-empty">"No formulas yet. Open a workbook, or choose a sheet and enter a formula in Grid."</p>})}
            <h2>"Sheets"</h2>
            <div class="dc-atlas-table" tabindex="0" role="region" aria-label="Sheet calculation counts; scroll horizontally for all columns"><table><thead><tr><th>"Sheet"</th><th>"Used area"</th><th>"Inputs"</th><th>"Formulas"</th><th>"Calculated"</th><th>"Refused"</th><th>"File cache"</th><th>"Stale"</th><th>"Pending"</th></tr></thead><tbody>{sheets}</tbody></table></div>
            <h2>"What needs attention"</h2><p>"Choose a refusal reason, then open an affected cell to inspect or edit its source. Cached and stale values need Calculate. Pending formulas have no published value."</p>
            <p data-dnacalc="atlas-reason-count">{move ||census.with(|c|{if atlas.with(Option::is_none){return "Counting refusal reasons…".into();}let total=c.refusals.len();if total==0 {"0 refusal reasons".into()}else{format!("Reasons {}–{} of {total} · up to 30 per page; most affected first",reason_page.get()*REASON_PAGE+1,((reason_page.get()+1)*REASON_PAGE).min(total))}})}</p>
            <ul class="dc-refusals">{reasons}</ul>
            <div class="dc-atlas-pages"><button data-dnacalc="atlas-reasons-previous" disabled=move ||reason_page.get()==0 on:click=move |_|reason_page.update(|p|*p=p.saturating_sub(1))>"Previous reasons"</button><button data-dnacalc="atlas-reasons-next" disabled=move ||census.with(|c|(reason_page.get()+1)*REASON_PAGE>=c.refusals.len()) on:click=move |_|reason_page.update(|p|*p+=1)>"Next reasons"</button></div>
            {move ||(atlas.with(Option::is_some)&&census.with(|c|c.refusals.is_empty())).then(||view!{<p data-dnacalc="atlas-no-refusals">"No formula is refused."</p>})}
            <details data-dnacalc="atlas-diagnostics"><summary>"Calculation diagnostics"</summary>{hud}</details>
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn every_reason_is_reachable_in_count_then_text_order() {
        let mut census = dna_calc_model::projection::Census::default();
        for n in 0..33 {
            census.refusals.insert(format!("reason {n:02}"), 1);
        }
        census.refusals.insert("most affected".into(), 60);
        let first = reason_rows(&census, 0);
        let rest = reason_rows(&census, 1);
        assert_eq!(first.len(), 30);
        assert_eq!(rest.len(), 4);
        assert_eq!(first[0], ("most affected".into(), 60));
        let all: std::collections::BTreeSet<_> =
            first.into_iter().chain(rest).map(|(r, _)| r).collect();
        assert_eq!(all, census.refusals.keys().cloned().collect());
        assert!(reason_rows(&census, usize::MAX).is_empty());
    }
}
