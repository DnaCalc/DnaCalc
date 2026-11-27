//! The shared application frame in its level-zero configuration.

use calc_skin::FormulaPort;
use dna_kit::{FormulaContext, Registry};
use leptos::prelude::*;
use std::rc::Rc;

pub fn formula(
    registry: &Registry,
    port: Rc<dyn FormulaPort>,
    initial_formula: String,
    clock: fn() -> Option<f64>,
) -> impl IntoView + use<> {
    let status = RwSignal::new("Ready");
    let stage = registry.mount_formula(FormulaContext {
        port,
        initial_formula,
        clock,
        status,
    });
    view! {
        <div class="dna-fe-frame" data-skin0="frame">
            <header class="dna-fe-mast">
                <span class="dna-fe-mark" aria-hidden="true"><i></i><i></i></span>
                <span>"DNA Calc"</span>
                <span class="dna-fe-mast-title">"Formula Explorer"</span>
            </header>
            <main class="dna-fe-stage">{stage}</main>
            <footer class="dna-fe-strip" role="status">{move || status.get()}</footer>
        </div>
    }
}
