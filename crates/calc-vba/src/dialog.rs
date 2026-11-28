//! Explicit answers for two VBA dialogs during a headless command.

use calcvba_oxir::OxTy;
use calcvba_runtime::Variant;
use std::{cell::RefCell, collections::BTreeMap};

/// Exact prompt text and the response the operator supplies for each dialog.
#[derive(Default)]
pub struct DialogAnswers {
    /// VBA button result by `MsgBox` prompt.
    pub msgbox: BTreeMap<String, i32>,
    /// Text response by `InputBox` prompt.
    pub inputbox: BTreeMap<String, String>,
}

/// The response that the command supplied to VBA.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DialogAnswer {
    /// A VBA `MsgBox` button result.
    Button(i32),
    /// Text typed into a VBA `InputBox`.
    Text(String),
}

/// One dialog attempted during the command. A missing answer leaves the call unavailable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DialogCall {
    /// The VBA runtime member name.
    pub member: &'static str,
    /// The prompt, if it could be read as VBA text.
    pub prompt: Option<String>,
    /// The explicit answer that was used.
    pub answer: Option<DialogAnswer>,
}

scoped_tls_hkt::scoped_thread_local!(static ACTIVE: for<'a> (
    &'a DialogAnswers, &'a RefCell<Vec<DialogCall>>
));

impl DialogAnswers {
    /// Supply answers for this call and return a record of each dialog it reached.
    pub fn capture<T>(&self, call: impl FnOnce() -> T) -> (T, Vec<DialogCall>) {
        let attempts = RefCell::new(Vec::new());
        let result = ACTIVE.set((self, &attempts), call);
        (result, attempts.into_inner())
    }
}

pub(crate) fn msgbox(prompt: &Variant, style: &Variant) -> Option<Variant> {
    answer("MsgBox", prompt, |answers, prompt| {
        let choice = *answers.msgbox.get(prompt)?;
        let style = super::runtime::coerce_argument(style.clone(), &OxTy::Long)
            .ok()?
            .as_i32()?;
        // The low nibble selects the buttons. MS-VBAL MsgBox lists the available results.
        let buttons: &[i32] = match style & 0xf {
            0 => &[1],
            1 => &[1, 2],
            2 => &[3, 4, 5],
            3 => &[6, 7, 2],
            4 => &[6, 7],
            5 => &[4, 2],
            _ => &[],
        };
        buttons
            .contains(&choice)
            .then_some(DialogAnswer::Button(choice))
    })
}

pub(crate) fn inputbox(prompt: &Variant) -> Option<Variant> {
    answer("InputBox", prompt, |answers, prompt| {
        answers
            .inputbox
            .get(prompt)
            .cloned()
            .map(DialogAnswer::Text)
    })
}

fn answer(
    member: &'static str,
    prompt: &Variant,
    lookup: impl FnOnce(&DialogAnswers, &str) -> Option<DialogAnswer>,
) -> Option<Variant> {
    if !ACTIVE.is_set() {
        return None;
    }
    ACTIVE.with(|(answers, attempts)| {
        let prompt = super::runtime::coerce_argument(prompt.clone(), &OxTy::Str)
            .ok()
            .and_then(|value| value.as_bstr().map(|text| text.to_string()));
        let answer = prompt.as_deref().and_then(|text| lookup(answers, text));
        let result = match &answer {
            Some(DialogAnswer::Button(value)) => Some(Variant::from_i32(*value)),
            Some(DialogAnswer::Text(value)) => Some(Variant::from_string(value.as_str())),
            None => None,
        };
        attempts.borrow_mut().push(DialogCall {
            member,
            prompt,
            answer,
        });
        result
    })
}
