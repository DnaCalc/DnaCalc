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

use crate::editor::{
    BracketPair, Popup, PopupKey, bracket_pair_at, expects_reference, insert_call, popup_key,
};
use calc_fml::assist::{self, CallSite, HighlightRun, TokenRole};
use calc_fml::parse_formula;
use calc_fml::syntax::TextSpan;
use calc_func::{FunctionCallTarget, help};
use leptos::prelude::*;
use std::rc::Rc;

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

/// What an owner gives the field to evaluate a selected part (F9): the part's start and length in
/// UTF-16 code units of the text, and the continuation for the formula text that gives its value,
/// or the reason there is none.
pub type PartEvaluator = Rc<dyn Fn(usize, usize, Box<dyn FnOnce(Result<String, String>)>)>;

/// The text before an F9 replaced a part, to restore with Escape.
#[derive(Clone, Debug, PartialEq, Eq)]
struct PartUndo {
    before: String,
    after: String,
    start: usize,
    end: usize,
}

/// Point mode: the grid the field points into, and the cell a first arrow key moves from.
#[derive(Clone)]
struct PointConfig {
    rows: u32,
    columns: u32,
    origin: Rc<dyn Fn() -> (u32, u32)>,
}

/// The reference point mode last wrote: where it is in the text, and the cells it spans.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Pointing {
    span: TextSpan,
    anchor: (u32, u32),
    cursor: (u32, u32),
}

/// The handles of the state only some owners use.
#[derive(Clone, Copy)]
struct Ext {
    scope: RwSignal<assist::Scope>,
    focus: RwSignal<Option<TextSpan>>,
    part_status: RwSignal<String>,
    part: StoredValue<Option<PartEvaluator>, LocalStorage>,
    part_undo: RwSignal<Option<PartUndo>>,
    point: StoredValue<Option<PointConfig>, LocalStorage>,
    pointing: RwSignal<Option<Pointing>>,
    point_armed: RwSignal<bool>,
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
    /// The state only some owners use (completion scope, F9, point mode, the trace focus), behind
    /// one handle: views copy this struct into every closure, so it stays small.
    ext: StoredValue<Ext, LocalStorage>,
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
        let scope = RwSignal::new(assist::Scope::default());
        let call = Memo::new(move |_| {
            let caret = caret.get();
            scope.with(|scope| {
                text.with(|text| assist::call_in(&parse_formula(text), caret, scope))
            })
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
            ext: StoredValue::new_local(Ext {
                scope,
                focus: RwSignal::new(None),
                part_status: RwSignal::new(String::new()),
                part: StoredValue::new_local(None),
                part_undo: RwSignal::new(None),
                point: StoredValue::new_local(None),
                pointing: RwSignal::new(None),
                point_armed: RwSignal::new(true),
            }),
            call,
        }
    }

    fn ext(self) -> Ext {
        self.ext.get_value()
    }

    /// The caller's defined names and registered functions, offered beside the catalogue; the
    /// owner sets it (DNA Calc from the workbook, OneCalc from its session).
    pub fn scope(self) -> RwSignal<assist::Scope> {
        self.ext().scope
    }

    /// The part of the formula a trace step or an evaluated selection points at, drawn behind
    /// the text.
    pub fn focus(self) -> RwSignal<Option<TextSpan>> {
        self.ext().focus
    }

    /// What the last F9 said: the part evaluated, or why it was not.
    pub fn part_status(self) -> RwSignal<String> {
        self.ext().part_status
    }

    fn part(self) -> StoredValue<Option<PartEvaluator>, LocalStorage> {
        self.ext().part
    }

    fn part_undo(self) -> RwSignal<Option<PartUndo>> {
        self.ext().part_undo
    }

    fn point(self) -> StoredValue<Option<PointConfig>, LocalStorage> {
        self.ext().point
    }

    fn pointing(self) -> RwSignal<Option<Pointing>> {
        self.ext().pointing
    }

    /// Typing arms point mode (Excel's Enter mode); a click or a caret key disarms it (Edit mode).
    fn point_armed(self) -> RwSignal<bool> {
        self.ext().point_armed
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
            self.part_undo().set(None);
            self.pointing().set(None);
            self.point_armed().set(true);
            self.focus().set(None);
        }
        self.caret.set(at);
        if self.composing.get_untracked() {
            return;
        }
        let completion = self
            .scope()
            .with_untracked(|scope| assist::complete_in(&text, at, scope));
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
        self.field_inner(diagnostics, on_key, rows, placeholder, cell_role, label, false)
    }

    /// The same field with the completion list and the signature help drawn as a pop-up anchored
    /// at the caret, inside the field, as Excel draws them under its formula bar. The owner then
    /// places neither [`FormulaAssist::popup`] nor [`FormulaAssist::signature`]. The keys are the
    /// field's own: arrows choose, Tab or Enter accept, Escape closes.
    pub fn anchored_field(
        self,
        diagnostics: Signal<Vec<TextSpan>>,
        on_key: Option<Callback<leptos::ev::KeyboardEvent>>,
        rows: u32,
        placeholder: &'static str,
    ) -> impl IntoView {
        self.field_inner(diagnostics, on_key, rows, placeholder, None, "Formula".into(), true)
    }

    /// An invisible copy of the text up to the caret, laid out exactly as the field lays out its
    /// own, and a zero-width anchor after it: the pop-up is positioned from the anchor, so it
    /// follows the caret through wrapped lines without measuring anything.
    fn caret_overlay(self) -> impl IntoView {
        let (text, caret) = (self.text, self.caret);
        let before = move || {
            let at = caret.get();
            text.with(|text| {
                let mut units = 0;
                text.chars()
                    .take_while(|c| {
                        let more = units < at;
                        units += c.len_utf16();
                        more
                    })
                    .collect::<String>()
            })
        };
        view! {
            <div class="oc-mirror" data-onecalc="caret-overlay">
                <span aria-hidden="true">{before}</span>
                <div class="oc-anchor">
                    <div class="oc-pop-down">{self.popup()}</div>
                    <div class="oc-sig-up">{self.signature()}</div>
                </div>
            </div>
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn field_inner(
        self,
        diagnostics: Signal<Vec<TextSpan>>,
        on_key: Option<Callback<leptos::ev::KeyboardEvent>>,
        rows: u32,
        placeholder: &'static str,
        cell_role: Option<&'static str>,
        label: String,
        anchored: bool,
    ) -> impl IntoView {
        // The highlight and the bracket pair are this field's own: kept out of the struct, which
        // views copy into every closure (DNA Calc's command bar mounts near the wasm stack's size).
        let (caret, text_binding) = (self.caret, self.text);
        let runs = Memo::new(move |_| text_binding.with(|text| assist::highlight(text)));
        let brackets = Memo::new(move |_| {
            let caret = caret.get();
            text_binding.with(|text| bracket_pair_at(text, caret))
        });
        let highlight = move || {
            let diagnostics = diagnostics.get();
            let pair = brackets.get();
            let focus = self.focus().get();
            runs.with(|runs| {
                runs.iter()
                    .map(|run| {
                        let mut classes = run_classes(run, &diagnostics, pair);
                        if let Some(focus) = focus
                            && focus.start < run.span.end()
                            && run.span.start < focus.end()
                        {
                            classes.push_str(" oc-focus");
                        }
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
                        // A key that types or moves the caret by more than an arrow ends point
                        // mode; a modifier, or a key the browser does not name (Firefox sends
                        // "Unidentified" when a driver releases the modifiers), does not.
                        let key = ev.key();
                        if key.chars().count() == 1
                            || matches!(
                                key.as_str(),
                                "Backspace" | "Delete" | "Enter" | "Home" | "End" | "PageUp" | "PageDown" | "Tab" | "Insert"
                            )
                        {
                            self.pointing().set(None);
                        }
                        if ev.key() == "F9" && !ev.ctrl_key() && !ev.alt_key() && !ev.meta_key() {
                            ev.prevent_default();
                            ev.stop_propagation();
                            self.evaluate_selection();
                            return;
                        }
                        if ev.key() == "Escape" && self.restore_part() {
                            ev.prevent_default();
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
                        // The anchored pop-up (DNA OneCalc) takes Enter too, as it takes Tab: the
                        // field there has no worksheet Enter to commit. A modified Enter is the
                        // owner's (Ctrl+Shift+Enter enters an array formula).
                        if anchored
                            && ev.key() == "Enter"
                            && !(ev.ctrl_key() || ev.meta_key() || ev.alt_key() || ev.shift_key())
                            && self.popup.with_untracked(Popup::is_open)
                        {
                            ev.prevent_default();
                            ev.stop_propagation();
                            self.accept();
                            return;
                        }
                        if self.popup_keydown(&ev) || self.point_keydown(&ev) {
                            ev.stop_propagation();
                        } else {
                            if matches!(
                                ev.key().as_str(),
                                "ArrowUp" | "ArrowDown" | "ArrowLeft" | "ArrowRight" | "Home" | "End" | "PageUp" | "PageDown"
                            ) {
                                self.point_armed().set(false);
                            }
                            if let Some(on_key) = on_key {
                                on_key.run(ev);
                            }
                        }
                    }
                    on:keyup=move |ev: leptos::ev::KeyboardEvent| {
                        if !matches!(ev.key().as_str(), "ArrowDown" | "ArrowUp" | "Enter" | "Tab" | "Escape") {
                            self.sync_from_field(false)
                        }
                    }
                    on:click=move |_| {
                        self.pointing().set(None);
                        self.point_armed().set(false);
                        self.sync_from_field(false)
                    }
                ></textarea>
                {anchored.then(|| self.caret_overlay())}
            </div>
        }
    }

    /// The field's selection in UTF-16 code units.
    pub fn selection(self) -> (usize, usize) {
        match self.field.get_untracked() {
            Some(field) => (
                field.selection_start().ok().flatten().unwrap_or(0) as usize,
                field.selection_end().ok().flatten().unwrap_or(0) as usize,
            ),
            None => {
                let at = self.caret.get_untracked();
                (at, at)
            }
        }
    }

    /// Lets F9 evaluate the selected part, through the owner's evaluator.
    pub fn enable_part_evaluation(
        self,
        evaluate: impl Fn(usize, usize, Box<dyn FnOnce(Result<String, String>)>) + 'static,
    ) {
        self.part().set_value(Some(Rc::new(evaluate)));
    }

    /// F9: replaces the selected part with its value, selected, and keeps the text before it for
    /// Escape. Without a selection it says to select one.
    pub fn evaluate_selection(self) {
        let Some(evaluate) = self.part().try_get_value().flatten() else {
            return;
        };
        let (start, end) = self.selection();
        if start >= end {
            self.part_status()
                .set("Select the part of the formula to evaluate, then press F9.".to_owned());
            return;
        }
        let before = self.text.get_untracked();
        let units: Vec<u16> = before.encode_utf16().collect();
        let end = end.min(units.len());
        let part = String::from_utf16_lossy(&units[start..end]);
        evaluate(
            start,
            end - start,
            Box::new(move |answer| match answer {
                Ok(literal) => {
                    let mut out = units[..start].to_vec();
                    out.extend(literal.encode_utf16());
                    let stop = out.len();
                    out.extend_from_slice(&units[end..]);
                    let after = String::from_utf16_lossy(&out);
                    self.place(after.clone(), stop);
                    self.part_undo().set(Some(PartUndo {
                        before,
                        after,
                        start,
                        end,
                    }));
                    if let Some(field) = self.field.get_untracked() {
                        let _ = field.set_selection_range(start as u32, stop as u32);
                    }
                    self.part_status().set(format!(
                        "{} evaluated to {literal}. Escape restores the formula.",
                        part.trim()
                    ));
                }
                Err(reason) => self.part_status().set(reason),
            }),
        );
    }

    /// Escape after F9: puts the text back as it was and selects the part again. `false` when no
    /// F9 is waiting to be undone (or the text was edited since).
    pub fn restore_part(self) -> bool {
        let Some(undo) = self.part_undo().get_untracked() else {
            return false;
        };
        self.part_undo().set(None);
        if self.text.get_untracked() != undo.after {
            return false;
        }
        self.place(undo.before, undo.end);
        if let Some(field) = self.field.get_untracked() {
            let _ = field.set_selection_range(undo.start as u32, undo.end as u32);
        }
        self.part_status().set("Formula restored.".to_owned());
        true
    }

    /// Lets arrow keys and clicks on the owner's grid insert and replace references at the caret.
    /// The grid is `rows` by `columns`; `origin` gives the cell a first arrow key moves from.
    pub fn enable_point_mode(self, rows: u32, columns: u32, origin: impl Fn() -> (u32, u32) + 'static) {
        self.point().set_value(Some(PointConfig {
            rows,
            columns,
            origin: Rc::new(origin),
        }));
    }

    /// The cells (first row, first column, last row, last column) the pointed reference spans.
    pub fn pointed_cells(self) -> Option<(u32, u32, u32, u32)> {
        self.pointing().get().map(|p| {
            (
                p.anchor.0.min(p.cursor.0),
                p.anchor.1.min(p.cursor.1),
                p.anchor.0.max(p.cursor.0),
                p.anchor.1.max(p.cursor.1),
            )
        })
    }

    /// An arrow key in point mode: moves the pointed cell (Shift extends a range), starting a
    /// reference where the caret expects one. `true` when it took the key.
    fn point_keydown(self, ev: &leptos::ev::KeyboardEvent) -> bool {
        if ev.ctrl_key() || ev.meta_key() || ev.alt_key() {
            return false;
        }
        let (row, column) = match ev.key().as_str() {
            "ArrowUp" => (-1i64, 0i64),
            "ArrowDown" => (1, 0),
            "ArrowLeft" => (0, -1),
            "ArrowRight" => (0, 1),
            _ => return false,
        };
        let Some(config) = self.point().try_get_value().flatten() else {
            return false;
        };
        let text = self.text.get_untracked();
        let (start, end) = self.selection();
        let (anchor, cursor, span) = match self.pointing().get_untracked() {
            Some(p) => (p.anchor, p.cursor, Some(p.span)),
            None if self.point_armed().get_untracked()
                && !ev.shift_key()
                && expects_reference(&text, start, end) =>
            {
                let origin = (config.origin)();
                (origin, origin, Some(TextSpan::new(start, end - start)))
            }
            None => return false,
        };
        let moved = (
            (i64::from(cursor.0) + row).clamp(1, i64::from(config.rows)) as u32,
            (i64::from(cursor.1) + column).clamp(1, i64::from(config.columns)) as u32,
        );
        let anchor = if ev.shift_key() { anchor } else { moved };
        ev.prevent_default();
        self.point_to(&text, span, anchor, moved);
        true
    }

    /// A click on a cell of the owner's grid while the field is being edited: inserts the cell's
    /// reference where the caret expects one, or replaces the reference point mode wrote (Shift
    /// extends it to a range). `true` when the click was taken, so the owner keeps the focus in
    /// the field.
    pub fn point_click(self, row: u32, column: u32, extend: bool) -> bool {
        let Some(field) = self.field.get_untracked() else {
            return false;
        };
        if self.point().try_get_value().flatten().is_none() {
            return false;
        }
        let focused = leptos::prelude::document()
            .active_element()
            .is_some_and(|active| {
                let element: &web_sys::Element = field.as_ref();
                active.is_same_node(Some(element))
            });
        if !focused {
            return false;
        }
        let text = self.text.get_untracked();
        let (start, end) = self.selection();
        let cell = (row, column);
        match self.pointing().get_untracked() {
            Some(p) => {
                let anchor = if extend { p.anchor } else { cell };
                self.point_to(&text, Some(p.span), anchor, cell);
                true
            }
            None if expects_reference(&text, start, end) => {
                self.point_to(&text, Some(TextSpan::new(start, end - start)), cell, cell);
                true
            }
            None => false,
        }
    }

    /// Writes the reference for the cells `anchor` to `cursor` over `span` of `text`, with the
    /// caret after it, and remembers where it is.
    fn point_to(self, text: &str, span: Option<TextSpan>, anchor: (u32, u32), cursor: (u32, u32)) {
        let span = span.unwrap_or_else(|| TextSpan::new(self.caret.get_untracked(), 0));
        let address = |(row, column): (u32, u32)| crate::display::address(row, column);
        let reference = if anchor == cursor {
            address(cursor)
        } else {
            let first = (anchor.0.min(cursor.0), anchor.1.min(cursor.1));
            let last = (anchor.0.max(cursor.0), anchor.1.max(cursor.1));
            format!("{}:{}", address(first), address(last))
        };
        let units: Vec<u16> = text.encode_utf16().collect();
        let start = span.start.min(units.len());
        let end = span.end().min(units.len()).max(start);
        let mut out = units[..start].to_vec();
        out.extend(reference.encode_utf16());
        let stop = out.len();
        out.extend_from_slice(&units[end..]);
        self.popup.set(Popup::Hidden);
        self.place(String::from_utf16_lossy(&out), stop);
        self.pointing().set(Some(Pointing {
            span: TextSpan::new(start, stop - start),
            anchor,
            cursor,
        }));
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
                            let label = proposal.label().to_owned();
                            let signature = match (&proposal.target, &proposal.descriptor) {
                                (Some(target), _) => target.signature_help().display,
                                (None, Some(descriptor)) => descriptor.signature(),
                                (None, None) => proposal.kind_name().to_owned(),
                            };
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
                None => match &site.registered {
                    Some(descriptor) => {
                        let (before, current, after) = descriptor.signature_pieces(site.active_argument);
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
                },
            };
            Some(view)
        }
    }

    /// The help panel of the popup's selected function, or else of the call the caret is in.
    pub fn help(self) -> impl IntoView {
        let (popup, call) = (self.popup, self.call);
        move || {
            let selected = popup.with(Popup::selected);
            let described = selected
                .as_ref()
                .and_then(|proposal| proposal.descriptor.clone())
                .or_else(|| {
                    call.with(|site| {
                        site.as_ref()
                            .filter(|site| site.target.is_none())
                            .and_then(|site| site.registered.clone())
                    })
                });
            if let Some(descriptor) = described {
                return Some(registered_help_view(descriptor).into_any());
            }
            let target = selected
                .and_then(|proposal| proposal.target)
                .or_else(|| call.with(|site| site.as_ref().and_then(|site| site.target)))?;
            Some(help_view(target).into_any())
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

/// The function help panel of a registered function: what its provider documented.
pub fn registered_help_view(descriptor: calc_fml::CallableDescriptor) -> impl IntoView {
    let signature = descriptor.signature();
    let description = if descriptor.description.is_empty() {
        "No description from the add-in.".to_owned()
    } else {
        descriptor.description.clone()
    };
    let category = if descriptor.category.is_empty() {
        "no category".to_owned()
    } else {
        format!("category {}", descriptor.category)
    };
    let mut traits = Vec::new();
    if descriptor.volatile {
        traits.push("volatile");
    }
    if descriptor.thread_safe {
        traits.push("thread-safe");
    }
    if descriptor.asynchronous {
        traits.push("asynchronous");
    }
    let traits = if traits.is_empty() { String::new() } else { format!("; {}", traits.join(", ")) };
    let parameters = descriptor
        .parameters
        .iter()
        .map(|parameter| {
            let text = if parameter.description.is_empty() {
                parameter.name.clone()
            } else {
                format!("{}: {}", parameter.name, parameter.description)
            };
            view! { <li data-onecalc="help-parameter">{text}</li> }
        })
        .collect_view();
    view! {
        <aside class="oc-help" data-onecalc="function-help">
            <h3 data-onecalc="help-name">{descriptor.name.clone()}</h3>
            <code data-onecalc="help-signature">{signature}</code>
            <p data-onecalc="help-description">{description}</p>
            <ul data-onecalc="help-parameters">{parameters}</ul>
            <p data-onecalc="help-status">{format!("Registered function from {}; {category}{traits}.", descriptor.origin)}</p>
        </aside>
    }
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
        calc_func::Availability::AbsentOnWindows => "Not in Excel for Windows (the name gives #NAME?)",
        calc_func::Availability::MacroSheetOnly => "Macro sheets only (refused in a worksheet formula)",
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
