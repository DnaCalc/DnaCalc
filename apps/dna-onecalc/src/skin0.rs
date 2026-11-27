//! Formula-only composition for standalone and embedded pages.

use crate::port::{Link, Reply, Request};
use calc_skin::{
    FormulaLink, FormulaOperation, FormulaPort, FormulaQuery, FormulaReply, Refusal, Trial,
};
use dna_formula_ui::web::serial_clock;
use leptos::prelude::*;
use std::{cell::RefCell, rc::Rc};
use web_sys::{HtmlElement, Storage};

const DRAFT: &str = "dna-formula-explorer:1";

struct Local(RefCell<calc_formula_host::FormulaHost>);

impl FormulaPort for Local {
    fn ask(&self, query: FormulaQuery, complete: FormulaReply) {
        let answer = self.0.borrow_mut().handle(query);
        complete(answer);
    }

    fn operate(&self, operation: FormulaOperation, complete: FormulaReply) {
        let answer = self.0.borrow_mut().operate(operation);
        complete(answer);
    }
}

struct Native(Box<dyn Link>);

impl Native {
    fn request(&self, request: Request, complete: FormulaReply) {
        self.0.call(
            request,
            Box::new(move |reply| {
                complete(match reply {
                    Reply::SkinFormula(answer) => answer,
                    Reply::Refused { message } => Err(Refusal::Transport(message)),
                    _ => Err(Refusal::Transport("Unexpected formula reply".into())),
                });
            }),
        );
    }
}

impl FormulaPort for Native {
    fn ask(&self, query: FormulaQuery, complete: FormulaReply) {
        self.request(Request::SkinFormula(query), complete);
    }

    fn operate(&self, operation: FormulaOperation, complete: FormulaReply) {
        self.request(Request::SkinOperation(operation), complete);
    }
}

fn fragment(parent: &HtmlElement) -> Option<FormulaLink> {
    let location = web_sys::window()?.location();
    let fragment = parent
        .get_attribute("data-link")
        .or_else(|| location.hash().ok())?;
    FormulaLink::from_fragment(&fragment)
}

fn storage() -> Option<Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

fn restore() -> Option<Trial> {
    let contents = storage()?.get_item(DRAFT).ok().flatten()?;
    let mut draft: Trial = serde_json::from_str(&contents).ok()?;
    draft.caller = None;
    draft.now = None;
    Some(draft)
}

fn persist(mut draft: Trial) {
    draft.caller = None;
    draft.now = None;
    if let (Some(storage), Ok(text)) = (storage(), serde_json::to_string(&draft)) {
        let _ = storage.set_item(DRAFT, &text);
    }
}

pub fn mount(parent: HtmlElement) {
    let port: Rc<dyn FormulaPort> = match crate::bridge::connect() {
        Some(link) => Rc::new(Native(Box::new(link))),
        None => Rc::new(Local(RefCell::default())),
    };
    let standalone = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.body())
        .is_some_and(|body| body.has_attribute("data-onecalc-standalone"));
    let link = fragment(&parent);
    let auto_preview = link.is_some();
    let draft = match link {
        Some(FormulaLink(formula)) => Trial {
            formula,
            ..Trial::default()
        },
        None if standalone => restore().unwrap_or_default(),
        None => Trial::default(),
    };
    // A linked example should leave a saved scratchpad available on returning to the bare URL.
    let save = (standalone && !auto_preview).then_some(persist as fn(Trial));
    let mut registry = dna_kit::Registry::default();
    dna_view_explorer::install(&mut registry);
    let handle = mount_to(parent, move || {
        dna_shell::formula(&registry, port, draft, auto_preview, save, serial_clock)
    });
    std::mem::forget(handle);
}
