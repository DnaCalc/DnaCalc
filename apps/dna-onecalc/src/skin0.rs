//! Formula-only composition for standalone and embedded pages.

use crate::port::{Link, Reply, Request};
use calc_skin::{FormulaAnswer, FormulaLink, FormulaPort, FormulaQuery, Refusal};
use dna_formula_ui::web::serial_clock;
use leptos::prelude::*;
use std::{cell::RefCell, rc::Rc};
use web_sys::HtmlElement;

struct Local(RefCell<calc_formula_host::FormulaHost>);

impl FormulaPort for Local {
    fn ask(&self, query: FormulaQuery, complete: Box<dyn FnOnce(Result<FormulaAnswer, Refusal>)>) {
        let answer = self.0.borrow_mut().handle(query);
        complete(answer);
    }
}

struct Native(Box<dyn Link>);

impl FormulaPort for Native {
    fn ask(&self, query: FormulaQuery, complete: Box<dyn FnOnce(Result<FormulaAnswer, Refusal>)>) {
        self.0.call(
            Request::SkinFormula(query),
            Box::new(move |reply| {
                let answer = match reply {
                    Reply::SkinFormula(answer) => answer,
                    Reply::Refused { message } => Err(Refusal::Transport(message)),
                    _ => Err(Refusal::Transport("Unexpected formula reply".into())),
                };
                complete(answer);
            }),
        );
    }
}

pub fn fragment(parent: &HtmlElement) -> Option<String> {
    let location = web_sys::window()?.location();
    let fragment = parent
        .get_attribute("data-link")
        .or_else(|| location.hash().ok());
    if let Some(link) = fragment.as_deref().and_then(FormulaLink::from_fragment) {
        return Some(link.0);
    }
    let query = location.search().ok()?;
    query
        .trim_start_matches('?')
        .split('&')
        .any(|param| param == "skin=0")
        .then(String::new)
}

pub fn mount(parent: HtmlElement, initial_formula: String) {
    let port: Rc<dyn FormulaPort> = match crate::bridge::connect() {
        Some(link) => Rc::new(Native(Box::new(link))),
        None => Rc::new(Local(RefCell::default())),
    };
    let mut registry = dna_kit::Registry::default();
    dna_view_explorer::install(&mut registry);
    let handle = mount_to(parent, move || {
        dna_shell::formula(&registry, port, initial_formula, serial_clock)
    });
    std::mem::forget(handle);
}
