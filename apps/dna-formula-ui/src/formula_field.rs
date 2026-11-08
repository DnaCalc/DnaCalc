//! The formula field, shared by DNA OneCalc's surface and DNA Calc's formula bar: a text field with
//! calc-fml's syntax highlighting behind it, the bracket pair at the caret, the completion popup of
//! calc-func's function names, signature help for the call at the caret, and the function help
//! panel. Nothing here calculates: the assistance is calc-fml's ([`calc_fml::assist`]) and the help
//! text calc-func's ([`calc_func::help`]).
//!
//! [`FormulaAssist`] holds the field's state as signals over a text signal its owner keeps, so an
//! owner can set the text (DNA Calc shows the active cell's formula in it) and read it back. Each
//! part is its own view, placed by the owner: [`FormulaAssist::field`], [`FormulaAssist::popup`],
//! [`FormulaAssist::signature`] and [`FormulaAssist::help`]. Every element a person uses carries a
//! `data-onecalc` attribute naming its role, in both applications.
//!
//! Moved out of DNA OneCalc's surface (D3, `apps/dna-onecalc/src/surface.rs`) into this library,
//! which both apps depend on. It was ported there from DnaTreeCalc's
//! Leptos formula skin (`src/dnacalc-formula-skin-leptos/src/lib.rs` at DnaTreeCalc c68ecaa: the
//! editor, completion list, signature and help elements and their CSS) and DnaOneCalc's editor (at
//! 61e3979: the highlighted overlay behind the text field from
//! `src/dnaonecalc-host/src/ui/editor/render_projection.rs`, whose runs must reproduce the field's
//! text exactly so the caret does not drift), so DNA Calc uses the same field (dc-rif).
//!
//! No reactive effect writes a signal: the popup is reconciled in the event handlers that change
//! the text or the caret, so a panic can only come from a memo or a view, where the browser smoke
//! test sees it.

use crate::editor::{BracketPair, Popup, PopupKey, bracket_pair_at, insert_call, popup_key};
use calc_fml::assist::{self, CallSite, HighlightRun, TokenRole};
use calc_fml::parse_formula;
use calc_fml::syntax::TextSpan;
use calc_func::{FunctionCallTarget, help};
use leptos::prelude::*;

/// A text field can edit either an ordinary signal or one field of a portable model.
/// The binding holds reactive accessors; the owning application retains the text.
#[derive(Clone, Copy)]
pub struct TextBinding {
    read: Signal<String>,
    write: Callback<String>,
}
impl TextBinding {
    pub fn new(read: Signal<String>, write: impl Fn(String) + Send + Sync + 'static) -> Self {
        Self {
            read,
            write: Callback::new(write),
        }
    }
    pub fn get(self) -> String {
        self.read.get()
    }
    pub fn get_untracked(self) -> String {
        self.read.get_untracked()
    }
    pub fn with<U>(self, f: impl FnOnce(&String) -> U) -> U {
        self.read.with(f)
    }
    pub fn track(self) {
        self.read.track();
    }
    pub fn set(self, text: String) {
        self.write.run(text);
    }
}
impl From<RwSignal<String>> for TextBinding {
    fn from(text: RwSignal<String>) -> Self {
        Self::new(text.into(), move |next| text.set(next))
    }
}

/// Reactive access to a small value owned by the application model.
type StateMutation<T> = Box<dyn Fn(&mut dyn FnMut(&mut T)) + Send + Sync>;
pub struct StateBinding<T: Send + Sync + 'static> {
    read: Signal<T>,
    write: Callback<T>,
    mutate: StoredValue<StateMutation<T>>,
}
impl<T: Send + Sync> Copy for StateBinding<T> {}
impl<T: Send + Sync> Clone for StateBinding<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T: Clone + Send + Sync + 'static> StateBinding<T> {
    pub fn new(read: Signal<T>, write: impl Fn(T) + Send + Sync + 'static) -> Self {
        let write = Callback::new(write);
        Self {
            read,
            write,
            mutate: StoredValue::new(Box::new(move |change| {
                let mut value = read.get_untracked();
                change(&mut value);
                write.run(value);
            })),
        }
    }
    pub fn new_with_update(
        read: Signal<T>,
        write: impl Fn(T) + Send + Sync + 'static,
        mutate: impl Fn(&mut dyn FnMut(&mut T)) + Send + Sync + 'static,
    ) -> Self {
        Self {
            read,
            write: Callback::new(write),
            mutate: StoredValue::new(Box::new(mutate)),
        }
    }
    pub fn with<U>(self, f: impl FnOnce(&T) -> U) -> U {
        self.read.with(f)
    }
    pub fn with_untracked<U>(self, f: impl FnOnce(&T) -> U) -> U {
        self.read.with_untracked(f)
    }
    pub fn try_update<U>(self, f: impl FnOnce(&mut T) -> U) -> Option<U> {
        let mut f = Some(f);
        let mut result = None;
        self.mutate.try_with_value(|mutate| {
            mutate(&mut |value| {
                result = Some(f.take().expect("one state update")(value));
            })
        });
        result
    }
    pub fn update(self, f: impl FnOnce(&mut T)) {
        self.try_update(f);
    }
    pub fn get(self) -> T {
        self.read.get()
    }
    pub fn get_untracked(self) -> T {
        self.read.get_untracked()
    }
    pub fn set(self, value: T) {
        self.write.run(value)
    }
    pub fn track(self) {
        self.read.track()
    }
}
/// The journal remains with its owner; edits do not clone the journal to change one entry.
pub trait HistoryStore: Send + Sync {
    fn available(&self, redo: bool) -> bool;
    fn start(&self, original: String, text: String, end: usize);
    fn record(&self, text: String, caret: usize);
    fn step(&self, redo: bool) -> Option<(String, usize)>;
}
#[derive(Clone, Copy)]
pub struct HistoryBinding(StoredValue<Box<dyn HistoryStore>>);
impl HistoryBinding {
    pub fn new(store: impl HistoryStore + 'static) -> Self {
        Self(StoredValue::new(Box::new(store)))
    }
    fn available(self, redo: bool) -> bool {
        self.0.with_value(|h| h.available(redo))
    }
    fn start(self, original: String, text: String, end: usize) {
        self.0.with_value(|h| h.start(original, text, end))
    }
    fn record(self, text: String, caret: usize) {
        self.0.with_value(|h| h.record(text, caret))
    }
    fn step(self, redo: bool) -> Option<(String, usize)> {
        self.0.try_with_value(|h| h.step(redo)).flatten()
    }
}
pub struct AssistBindings {
    pub caret: StateBinding<usize>,
    pub dismissed: StateBinding<Option<String>>,
    pub composing: StateBinding<bool>,
    pub readonly: StateBinding<bool>,
    pub history: HistoryBinding,
    pub popup: StateBinding<Popup>,
}
struct LocalHistory(RwSignal<dna_calc_model::EditorState>);
impl HistoryStore for LocalHistory {
    fn available(&self, redo: bool) -> bool {
        self.0.with_untracked(|s| s.history.available(redo))
    }
    fn start(&self, original: String, text: String, end: usize) {
        self.0.update(|s| s.history.start(original, text, end));
    }
    fn record(&self, text: String, caret: usize) {
        self.0.update(|s| s.history.record(text, caret));
    }
    fn step(&self, redo: bool) -> Option<(String, usize)> {
        self.0.try_update(|s| s.history.step(redo)).flatten()
    }
}

/// The formula field's state over its owner's text signal. Every part is a signal, a memo or a node
/// reference, so the whole is `Copy` and its views can be placed anywhere in the owner's view.
#[derive(Clone, Copy)]
pub struct FormulaAssist {
    /// The field's text, kept by the owner.
    pub text: TextBinding,
    /// The caret, in UTF-16 code units.
    pub caret: StateBinding<usize>,
    pub popup: StateBinding<Popup>,
    /// The prefix whose completion the person dismissed, which stays hidden until it changes.
    dismissed: StateBinding<Option<String>>,
    /// The text field.
    pub field: NodeRef<leptos::html::Textarea>,
    pub composing: StateBinding<bool>,
    pub readonly: StateBinding<bool>,
    history: HistoryBinding,
    runs: Memo<Vec<HighlightRun>>,
    brackets: Memo<Option<BracketPair>>,
    /// The function call the caret is in.
    pub call: Memo<Option<CallSite>>,
}

impl FormulaAssist {
    /// The field's state over `text`, with the caret at the start and the popup hidden.
    pub fn new(text: impl Into<TextBinding>) -> FormulaAssist {
        let text = text.into();
        let state = RwSignal::new(dna_calc_model::EditorState::default());
        let popup = RwSignal::new(Popup::default());
        Self::new_with_state(
            text,
            AssistBindings {
                caret: StateBinding::new(
                    Signal::derive(move || state.with(|s| s.caret)),
                    move |v| state.update(|s| s.caret = v),
                ),
                dismissed: StateBinding::new(
                    Signal::derive(move || state.with(|s| s.dismissed.clone())),
                    move |v| state.update(|s| s.dismissed = v),
                ),
                composing: StateBinding::new(
                    Signal::derive(move || state.with(|s| s.composing)),
                    move |v| state.update(|s| s.composing = v),
                ),
                readonly: StateBinding::new(
                    Signal::derive(move || state.with(|s| s.readonly)),
                    move |v| state.update(|s| s.readonly = v),
                ),
                history: HistoryBinding::new(LocalHistory(state)),
                popup: StateBinding::new_with_update(
                    popup.into(),
                    move |p| popup.set(p),
                    move |f| {
                        popup.try_update(|p| f(p));
                    },
                ),
            },
        )
    }
    pub fn new_with_state(text: impl Into<TextBinding>, state: AssistBindings) -> FormulaAssist {
        let text = text.into();
        let AssistBindings {
            caret,
            dismissed,
            composing,
            readonly,
            history,
            popup,
        } = state;
        history.start(text.get_untracked(), text.get_untracked(), 0);
        let runs = Memo::new(move |_| text.with(|text| assist::highlight(text)));
        let brackets = Memo::new(move |_| {
            let caret = caret.get();
            text.with(|text| bracket_pair_at(text, caret))
        });
        let call = Memo::new(move |_| {
            let caret = caret.get();
            text.with(|text| assist::call_at(&parse_formula(text), caret))
        });
        FormulaAssist {
            text,
            caret,
            popup,
            dismissed,
            field: NodeRef::new(),
            composing,
            readonly,
            history,
            runs,
            brackets,
            call,
        }
    }

    /// Another field over the same text, completion, caret and local undo session.
    pub fn history_available(self, redo: bool) -> bool {
        self.text.track();
        self.caret.track();
        self.history.available(redo)
    }

    pub fn another_field(self) -> Self {
        Self {
            field: NodeRef::new(),
            ..self
        }
    }

    pub fn start_edit(self, original: String) {
        let text = self.text.get_untracked();
        let end = text.encode_utf16().count();
        self.caret.set(end);
        self.history.start(original, text, end);
        self.composing.set(false);
        self.reset();
    }

    /// Reads the field's text and caret after an edit or a caret move, and reconciles the popup.
    pub fn sync_from_field(self, text_changed: bool) {
        let Some(field) = self.field.get_untracked() else {
            return;
        };
        let text = field.value();
        let at = field
            .selection_start()
            .ok()
            .flatten()
            .map_or(text.encode_utf16().count(), |at| at as usize);
        if text_changed {
            self.text.set(text.clone());
            self.history.record(text.clone(), at);
        }
        self.caret.set(at);
        if self.composing.get_untracked() {
            return;
        }
        let completion = assist::complete(&text, at);
        let prefix_now = completion.as_ref().map(|c| c.prefix.clone());
        if self.dismissed.get_untracked() != prefix_now {
            self.dismissed.set(None);
        }
        let dismissed_prefix = self.dismissed.get_untracked();
        self.popup
            .update(|popup| popup.sync(completion, dismissed_prefix.as_deref()));
    }

    /// Accepts the popup's selected proposal into the text, with the caret after it.
    pub fn accept(self) {
        let text = self.text.get_untracked();
        let Some((next, at)) = self.popup.try_update(|popup| popup.accept(&text)).flatten() else {
            return;
        };
        self.place(next, at);
    }

    /// Inserts a call of `name` at the field's selection (a click in a function list).
    pub fn insert_function(self, name: &'static str) {
        let text = self.text.get_untracked();
        let (start, end) = match self.field.get_untracked() {
            Some(field) => (
                field.selection_start().ok().flatten().unwrap_or(0) as usize,
                field.selection_end().ok().flatten().unwrap_or(0) as usize,
            ),
            None => (0, 0),
        };
        let (next, at) = insert_call(&text, start, end, name);
        self.popup.set(Popup::Hidden);
        self.place(next, at);
    }

    /// Sets the text and puts the caret at `at`, in the field and in the signals.
    pub fn place(self, next: String, at: usize) {
        self.history.record(next.clone(), at);
        self.restore(next, at);
    }

    /// Reflects an accepted model edit into the mounted field without writing its journal again.
    pub fn reflect(self, text: &str, at: usize) {
        if let Some(field) = self.field.get_untracked() {
            field.set_value(text);
            let _ = field.focus();
            let _ = field.set_selection_range(at as u32, at as u32);
        }
    }

    fn restore(self, next: String, at: usize) {
        self.reflect(&next, at);
        self.text.set(next);
        self.caret.set(at);
    }

    pub fn undo(self, redo: bool) {
        if self.readonly.get_untracked() || self.composing.get_untracked() {
            return;
        }
        if let Some((text, at)) = self.history.step(redo) {
            self.restore(text, at);
            self.sync_from_field(false);
        }
    }

    /// Closes the popup and keeps it closed until the prefix changes.
    pub fn dismiss(self) {
        let prefix = self.popup.with_untracked(|popup| match popup {
            Popup::Open { completion, .. } => Some(completion.prefix.clone()),
            Popup::Hidden => None,
        });
        self.dismissed.set(prefix);
        self.popup.set(Popup::Hidden);
    }

    /// Hides the popup without remembering a dismissal (the owner replaced the text).
    pub fn reset(self) {
        self.dismissed.set(None);
        self.popup.set(Popup::Hidden);
    }

    /// Takes a key the open popup uses (arrows, Enter, Tab, Escape); `true` when it took it.
    pub fn popup_keydown(self, ev: &leptos::ev::KeyboardEvent) -> bool {
        if ev.ctrl_key() || ev.meta_key() || ev.alt_key() || ev.shift_key() {
            return false;
        }
        if !self.popup.with_untracked(Popup::is_open) {
            return false;
        }
        let Some(key) = popup_key(&ev.key()) else {
            return false;
        };
        ev.prevent_default();
        match key {
            PopupKey::Next => self.popup.update(|popup| popup.move_selection(1)),
            PopupKey::Previous => self.popup.update(|popup| popup.move_selection(-1)),
            PopupKey::Accept => self.accept(),
            PopupKey::Dismiss => self.dismiss(),
        }
        true
    }

    /// The editor: the highlighted runs behind a transparent text field. `diagnostics` are the
    /// spans of the text to underline. A key the popup does not take goes to `on_key`.
    pub fn field(
        self,
        diagnostics: Signal<Vec<TextSpan>>,
        on_key: Option<Callback<leptos::ev::KeyboardEvent>>,
        rows: u32,
        placeholder: &'static str,
    ) -> impl IntoView {
        self.field_named(diagnostics, on_key, rows, placeholder, None)
    }

    /// The same field with an application role for an in-cell editor.
    pub fn field_named(
        self,
        diagnostics: Signal<Vec<TextSpan>>,
        on_key: Option<Callback<leptos::ev::KeyboardEvent>>,
        rows: u32,
        placeholder: &'static str,
        cell_role: Option<&'static str>,
    ) -> impl IntoView {
        self.field_labelled(
            diagnostics,
            on_key,
            rows,
            placeholder,
            cell_role,
            "Formula".into(),
        )
    }

    pub fn field_labelled(
        self,
        diagnostics: Signal<Vec<TextSpan>>,
        on_key: Option<Callback<leptos::ev::KeyboardEvent>>,
        rows: u32,
        placeholder: &'static str,
        cell_role: Option<&'static str>,
        label: String,
    ) -> impl IntoView {
        let highlight = move || {
            let diagnostics = diagnostics.get();
            let pair = self.brackets.get();
            self.runs.with(|runs| {
                runs.iter()
                    .map(|run| {
                        let classes = run_classes(run, &diagnostics, pair);
                        let role = role_name(run.role);
                        let text = run.text.clone();
                        view! { <span class=classes data-role=role>{text}</span> }
                    })
                    .collect_view()
            })
        };
        let text = self.text;
        view! {
            <div class="oc-editor">
                <pre data-onecalc="highlight" aria-hidden="true">{highlight}"\n"</pre>
                <textarea
                    data-onecalc="formula"
                    data-dnacalc=cell_role
                    node_ref=self.field
                    rows=rows.to_string()
                    spellcheck="false"
                    autocomplete="off"
                    aria-label=label
                    placeholder=placeholder
                    prop:value=move || text.get()
                    prop:readOnly=move || self.readonly.get()
                    on:input=move |_| self.sync_from_field(true)
                    on:compositionstart=move |_| {
                        self.composing.set(true);
                        self.reset();
                    }
                    on:compositionend=move |_| {
                        self.composing.set(false);
                        self.sync_from_field(true);
                    }
                    on:keydown=move |ev: leptos::ev::KeyboardEvent| {
                        if self.readonly.get_untracked() {
                            ev.prevent_default();
                            ev.stop_propagation();
                            return;
                        }
                        if self.composing.get_untracked() || ev.is_composing() || ev.key_code() == 229 {
                            ev.stop_propagation();
                            return;
                        }
                        let ctrl = ev.ctrl_key() || ev.meta_key();
                        if ctrl && !ev.alt_key() && matches!(ev.key().to_ascii_lowercase().as_str(), "z" | "y") {
                            ev.prevent_default();
                            ev.stop_propagation();
                            let redo = ev.key().eq_ignore_ascii_case("y") || ev.shift_key();
                            self.undo(redo);
                            return;
                        }
                        if self.popup_keydown(&ev) {
                            ev.stop_propagation();
                        } else if let Some(on_key) = on_key {
                            on_key.run(ev);
                        }
                    }
                    on:keyup=move |ev: leptos::ev::KeyboardEvent| {
                        if !matches!(ev.key().as_str(), "ArrowDown" | "ArrowUp" | "Enter" | "Tab" | "Escape") {
                            self.sync_from_field(false)
                        }
                    }
                    on:click=move |_| self.sync_from_field(false)
                ></textarea>
            </div>
        }
    }

    /// The completion popup, while it is open.
    pub fn popup(self) -> impl IntoView {
        let popup = self.popup;
        move || {
            popup.with(|state| match state {
                Popup::Hidden => None,
                Popup::Open {
                    completion,
                    selected,
                } => {
                    let selected = *selected;
                    let items = completion
                        .proposals
                        .iter()
                        .enumerate()
                        .map(|(index, proposal)| {
                            let label = proposal.label();
                            let signature = proposal.target.signature_help().display;
                            let class = if index == selected { "oc-sel" } else { "" };
                            view! {
                                <li
                                    class=class
                                    data-onecalc="completion"
                                    role="option"
                                    aria-selected=if index == selected { "true" } else { "false" }
                                    on:mousedown=move |ev| {
                                        ev.prevent_default();
                                        popup.update(|popup| popup.select(index));
                                        self.accept();
                                    }
                                >
                                    <span data-onecalc="completion-label">{label}</span>
                                    <small>{signature}</small>
                                </li>
                            }
                        })
                        .collect_view();
                    Some(view! { <ul class="oc-pop" data-onecalc="completions" role="listbox">{items}</ul> })
                }
            })
        }
    }

    /// The signature help line of the call the caret is in.
    pub fn signature(self) -> impl IntoView {
        let call = self.call;
        move || {
            let site = call.get()?;
            let view = match site.target {
                Some(target) => {
                    let (before, current, after) = signature_pieces(target, site.active_argument);
                    view! {
                        <div class="oc-sig" data-onecalc="signature">
                            {before}
                            <strong data-onecalc="active-parameter">{current}</strong>
                            {after}
                        </div>
                    }
                    .into_any()
                }
                None => view! {
                    <div class="oc-sig oc-err" data-onecalc="signature">
                        {format!("{} is not a function in calc-func's catalogue", site.callee)}
                    </div>
                }
                .into_any(),
            };
            Some(view)
        }
    }

    /// The help panel of the popup's selected function, or else of the call the caret is in.
    pub fn help(self) -> impl IntoView {
        let (popup, call) = (self.popup, self.call);
        move || {
            let target = popup
                .with(Popup::selected)
                .map(|proposal| proposal.target)
                .or_else(|| call.with(|site| site.as_ref().and_then(|site| site.target)))?;
            Some(help_view(target))
        }
    }
}

/// A highlighted run as the overlay shows it: its classes.
pub fn run_classes(
    run: &HighlightRun,
    diagnostics: &[TextSpan],
    brackets: Option<BracketPair>,
) -> String {
    let mut classes = String::from(match run.role {
        TokenRole::Function => "oc-r-function",
        TokenRole::Name => "oc-r-name",
        TokenRole::Number => "oc-r-number",
        TokenRole::Text => "oc-r-text",
        TokenRole::Error => "oc-r-error",
        TokenRole::Operator => "oc-r-operator",
        TokenRole::Delimiter => "oc-r-delimiter",
        TokenRole::Whitespace => "oc-r-space",
        TokenRole::Unknown => "oc-r-unknown",
    });
    let span = run.span;
    let overlaps = |d: &TextSpan| {
        if d.len == 0 {
            d.start >= span.start && d.start < span.end()
        } else {
            d.start < span.end() && span.start < d.end()
        }
    };
    if diagnostics.iter().any(overlaps) {
        classes.push_str(" oc-diag");
    }
    if let Some(pair) = brackets
        && span.len == 1
        && (span.start == pair.open || span.start == pair.close)
    {
        classes.push_str(" oc-bracket");
    }
    classes
}

fn role_name(role: TokenRole) -> &'static str {
    match role {
        TokenRole::Function => "function",
        TokenRole::Name => "name",
        TokenRole::Number => "number",
        TokenRole::Text => "text",
        TokenRole::Error => "error",
        TokenRole::Operator => "operator",
        TokenRole::Delimiter => "delimiter",
        TokenRole::Whitespace => "whitespace",
        TokenRole::Unknown => "unknown",
    }
}

/// How many arguments a function takes, in words.
pub fn arity_text(target: FunctionCallTarget<'_>) -> String {
    let arity = target.arity();
    match (arity.min, arity.max) {
        (0, 0) => "takes no arguments".to_owned(),
        (min, max) if min == max => {
            format!("takes {min} argument{}", if min == 1 { "" } else { "s" })
        }
        (min, max) => format!("takes {min} to {max} arguments"),
    }
}

/// The signature help line's pieces: the text before the active parameter, the active one, and
/// the rest.
pub fn signature_pieces(target: FunctionCallTarget<'_>, active: usize) -> (String, String, String) {
    let help = target.signature_help();
    let count = help.parameters.len();
    let repeats = help.parameters.last().is_some_and(|p| p.repeats);
    let active = if active >= count && repeats {
        count - 1
    } else {
        active
    };
    let label = |index: usize| {
        let parameter = help.parameters[index];
        if parameter.optional {
            format!("[{}]", parameter.name)
        } else {
            parameter.name.to_owned()
        }
    };
    let mut before = format!("{}(", target.spelling());
    let mut current = String::new();
    let mut after = String::new();
    for index in 0..count {
        let text = label(index);
        let separator = if index > 0 { ", " } else { "" };
        if index < active {
            before.push_str(separator);
            before.push_str(&text);
        } else if index == active {
            before.push_str(separator);
            current = text;
        } else {
            after.push_str(separator);
            after.push_str(&text);
        }
    }
    if repeats {
        after.push_str(", ...");
    }
    after.push(')');
    (before, current, after)
}

/// The function help panel for one function.
pub fn help_view(target: FunctionCallTarget<'static>) -> impl IntoView {
    let signature = target.signature_help().display;
    let status = match target.implementation() {
        calc_func::Implementation::Held => "Held".to_owned(),
        calc_func::Implementation::Binding => "Formula binding".to_owned(),
        calc_func::Implementation::NotHeld { owner } => format!("Not held; owned by {owner}"),
        calc_func::Implementation::Deferred { class } => format!("Deferred ({class:?}); owned by {}", class.owner()),
    };
    let availability = match target.availability() {
        calc_func::Availability::Unspecified => "Availability not established",
        calc_func::Availability::ListsInCellsBeta => "Insider Beta; workbook compatibility version 3",
    };
    let description = help::description(target)
        .unwrap_or("No description in calc-func's help text.")
        .to_owned();
    // Microsoft's official description is Microsoft's text: show where it comes from.
    let source = help::help(target)
        .filter(|h| h.microsoft.is_some())
        .and_then(|h| h.source)
        .map(|url| {
            view! {
                <p class="oc-help-source" data-onecalc="help-source">
                    "Source: "
                    <a href=url target="_blank" rel="noopener noreferrer">"Microsoft"</a>
                </p>
            }
        });
    view! {
        <aside class="oc-help" data-onecalc="function-help">
            <h3 data-onecalc="help-name">{target.spelling()}</h3>
            <code data-onecalc="help-signature">{signature}</code>
            <p data-onecalc="help-description">{description}</p>
            <p data-onecalc="help-status">{status}"; "{availability}</p>
            {source}
            <p>{format!("{}; in calc-func's catalogue.", arity_text(target))}</p>
        </aside>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use calc_func::{Notation, catalogue};

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn signature_pieces_mark_the_active_parameter() {
        let sum = catalogue()
            .resolve_surface(Notation::Call, "SUM")
            .expect("SUM");
        let (before, current, after) = signature_pieces(sum, 0);
        assert_eq!(
            format!("{before}<{current}>{after}"),
            "SUM(<number1>, [number2], ...)"
        );
        // Past the named parameters, the repeating last one stays active.
        let (before, current, after) = signature_pieces(sum, 5);
        assert_eq!(
            format!("{before}<{current}>{after}"),
            "SUM(number1, <[number2]>, ...)"
        );
        let pi = catalogue()
            .resolve_surface(Notation::Call, "PI")
            .expect("PI");
        let (before, current, after) = signature_pieces(pi, 0);
        assert_eq!(format!("{before}<{current}>{after}"), "PI(<>)");
        assert_eq!(arity_text(pi), "takes no arguments");
        assert_eq!(arity_text(sum), "takes 1 to 255 arguments");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn highlight_classes_mark_diagnostics_and_brackets() {
        let runs = assist::highlight("=SUM(1,2)");
        let pair = bracket_pair_at("=SUM(1,2)", 4);
        let classes: Vec<String> = runs.iter().map(|run| run_classes(run, &[], pair)).collect();
        assert_eq!(classes[1], "oc-r-function");
        assert_eq!(classes[2], "oc-r-delimiter oc-bracket");
        assert_eq!(classes[6], "oc-r-delimiter oc-bracket");
        let runs = assist::highlight("=Sheet9!A1+1");
        let classes: Vec<String> = runs
            .iter()
            .map(|run| run_classes(run, &[TextSpan::new(1, 9)], None))
            .collect();
        assert!(classes[1].ends_with("oc-diag"));
        assert!(!classes[classes.len() - 1].ends_with("oc-diag"));
    }
}
