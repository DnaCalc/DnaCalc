//! The evaluation trace panel both apps show (dc-wr9 for DNA OneCalc, dc-0f1k.7 for DNA Calc): a
//! button that asks the owner for the formula's trace ([`crate::parts::trace`] over the owner's
//! evaluator), the steps it listed innermost first, Previous and Next to walk them, and the step
//! shown highlighted in the formula field. A trace belongs to what it was asked about: once the
//! owner's identity for the formula and its inputs differs, the panel says so and lists nothing.

use crate::formula_field::FormulaAssist;
use calc_fml::syntax::TextSpan;
use dna_calc_model::parts::Trace;
use leptos::prelude::*;

/// The trace last listed, the identity it was asked about, and the step shown.
#[derive(Clone, Copy)]
pub struct TracePanel {
    run: RwSignal<Option<(String, Trace)>>,
    at: RwSignal<usize>,
}

impl Default for TracePanel {
    fn default() -> Self {
        Self::new()
    }
}

impl TracePanel {
    pub fn new() -> Self {
        TracePanel {
            run: RwSignal::new(None),
            at: RwSignal::new(0),
        }
    }

    /// The step at `index`, shown and highlighted in the formula field of `assist`.
    fn show_step(self, assist: FormulaAssist, index: usize) {
        self.run.with_untracked(|run| {
            if let Some((_, trace)) = run
                && let Some(step) = trace.steps.get(index)
            {
                assist.focus().set(Some(TextSpan::new(step.start, step.len)));
                self.at.set(index);
            }
        });
    }

    /// The panel. `identity` names what a trace depends on (the formula text and the inputs it
    /// reads); `start` asks the owner for the trace and hands it to the callback it is given.
    pub fn view(
        self,
        assist: FormulaAssist,
        identity: impl Fn() -> String + Copy + 'static,
        start: impl Fn(Box<dyn FnOnce(Trace)>) + Copy + 'static,
    ) -> impl IntoView {
        let panel = self;
        // The closure is the owner's and stays on this thread; the reactive closures below hold
        // only its handle.
        let identity = StoredValue::new_local(identity);
        let begin = move |_| {
            let asked = identity.with_value(|identity| identity());
            start(Box::new(move |found: Trace| {
                let first = found.steps.first().map(|s| TextSpan::new(s.start, s.len));
                panel.run.set(Some((asked, found)));
                panel.at.set(0);
                assist.focus().set(first);
            }));
        };
        let fresh = move || {
            panel
                .run
                .with(|run| run.as_ref().is_some_and(|(asked, _)| *asked == identity.with_value(|identity| identity())))
        };
        view! {
            <section class="oc-trace" data-onecalc="trace-panel" aria-labelledby="oc-trace-title">
                <header><h2 id="oc-trace-title">"Evaluation trace"</h2>
                    <button type="button" data-onecalc="trace-start" on:click=begin>"Trace the formula"</button>
                    <button type="button" data-onecalc="trace-previous" on:click=move |_| panel.show_step(assist, panel.at.get_untracked().saturating_sub(1))>"Previous step"</button>
                    <button type="button" data-onecalc="trace-next" on:click=move |_| panel.show_step(assist, panel.at.get_untracked() + 1)>"Next step"</button>
                </header>
                <p class="oc-prov" role="status" data-onecalc="trace-status">{move || if panel.run.with(Option::is_none) {
                    "Select part of the formula and press F9 to see its value; Escape restores the formula. Or trace the whole formula, part by part.".to_owned()
                } else if !fresh() {
                    "The formula or its inputs changed since this trace. Trace it again.".to_owned()
                } else {
                    panel.run.with(|run| run.as_ref().map(|(_, t)| format!("Step {} of {}", panel.at.get() + 1, t.steps.len())).unwrap_or_default())
                }}</p>
                <ol class="oc-trace-steps" data-onecalc="trace-steps">{move || {
                    if !fresh() { return Vec::new(); }
                    let current = panel.at.get();
                    panel.run.with(|run| run.as_ref().map(|(_, t)| t.steps.iter().enumerate().map(|(index, step)| {
                        let class = format!(
                            "oc-depth-{}{}",
                            step.depth.min(8),
                            if index == current { " oc-trace-current" } else { "" }
                        );
                        let (text, kind, value) = (step.text.clone(), step.kind.clone(), step.value.clone());
                        view! { <li class=class data-onecalc="trace-step" on:click=move |_| panel.show_step(assist, index)>
                            <code data-onecalc="trace-part">{text}</code>" = "<span data-onecalc="trace-value" data-kind=kind>{value}</span>
                        </li> }
                    }).collect::<Vec<_>>()).unwrap_or_default())
                }}</ol>
                <p class="oc-prov" data-onecalc="trace-note">{move || panel.run.with(|run| run.as_ref().map(|(_, t)| t.note.clone()).unwrap_or_default())}</p>
            </section>
        }
    }
}
