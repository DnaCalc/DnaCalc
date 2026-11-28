//! The frame every Classic dialog is drawn in, and the commands that open and close it.
//!
//! A dialog family is a [`Family`]: its fields, what it reads from the workbook before it shows
//! them, and the one typed change its OK commits. The frame owns everything else: the modal
//! element, the tab strip, the access keys, Enter and Escape, the alert line and the probe.
//! A session shows one dialog at a time, as Excel does.

use super::model::{Choice, Field, FieldId, Focus, KeyAction, Kind, Spec, State, Value, caption};
use crate::{Live, Seam, Session, ViewCommand};
use calc_skin::grid::{Area, Key, Sheet};
use calc_skin::{Batch, Change, ModelChange, Operation, Revision, SourceRef, ViewId, WorkbookId};
use dna_skin_runtime::commands::KeyInput;
use leptos::prelude::*;
use leptos::wasm_bindgen::JsCast;

pub const ACCEPT: &str = "dialog.accept";
pub const CANCEL: &str = "dialog.cancel";

/// What a dialog acts on: the selection it was opened over, at the revision it read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub workbook: WorkbookId,
    pub revision: Revision,
    pub sheet: Sheet,
    pub areas: Vec<Area>,
    pub active: Key,
}

impl Target {
    /// The selected worksheet cells.
    pub fn selection(session: Session) -> Result<Self, String> {
        let stamp = session.stamp().ok_or("The workbook is unavailable.")?;
        let selection = session.selection().ok_or("Select worksheet cells first.")?;
        let SourceRef::Cells { sheet, areas } = selection.target else {
            return Err("Select worksheet cells first.".into());
        };
        let first = areas.first().ok_or("Select worksheet cells first.")?;
        let holds = |area: &Area, at: Key| {
            area.0.0 == at.0 && area.0.1 == at.1 && (area.0.2..=area.1.2).contains(&at.2) && (area.0.3..=area.1.3).contains(&at.3)
        };
        let active = selection
            .active
            .filter(|at| areas.iter().any(|area| holds(area, *at)))
            .unwrap_or(first.0);
        Ok(Self {
            workbook: stamp.workbook,
            revision: stamp.model,
            sheet,
            areas,
            active,
        })
    }

    /// One workbook change at the revision the dialog read.
    pub fn batch(&self, label: &str, change: ModelChange) -> Batch {
        Batch {
            id: String::new(),
            workbook: Some(self.workbook),
            base: Some(self.revision),
            expects: Vec::new(),
            selection_seq: None,
            label: Some(label.into()),
            changes: vec![Change::Model(change)],
        }
    }
}

/// What OK does.
#[derive(Clone, Debug, PartialEq)]
pub enum Commit {
    /// Nothing was changed: the dialog closes.
    Nothing,
    /// One workbook edit, committed at once and undone in one step.
    Edit(Batch),
    /// One workbook edit that the review seam shows before it is applied.
    Review(Batch),
    Run(Operation, Option<String>),
    /// Move the selection to an address, as Go To does.
    GoTo(String),
}

/// One Excel dialog on the frame.
pub struct Family {
    /// The dialog's name in the page and the probe: `format-cells`.
    pub id: &'static str,
    /// The command that opens it.
    pub command: &'static str,
    pub label: &'static str,
    pub chords: &'static [&'static str],
    /// When the command can run: `mode:ready && writable`.
    pub when: &'static str,
    pub target: fn(Session) -> Result<Target, String>,
    pub spec: fn(&Target) -> Spec,
    /// Starts what the dialog reads from the workbook.
    pub begin: fn(Session, &Target),
    /// Fills the fields once the workbook has answered.
    pub poll: fn(Session, &Target, &mut State) -> Live<()>,
    pub end: fn(Session),
    pub enabled: fn(&State, FieldId) -> bool,
    pub shown: fn(&State, FieldId) -> bool,
    /// A field changed: the fields that depend on it follow.
    pub changed: fn(&mut State, FieldId),
    /// A button of the dialog was pressed.
    pub press: fn(&mut State, FieldId),
    /// Draws a [`Kind::Custom`] field.
    pub custom: fn(Session, FieldId) -> AnyView,
    pub commit: fn(&State, &Target) -> Result<Commit, String>,
}

impl Family {
    pub fn no_read(_: Session, _: &Target) {}
    pub fn ready(_: Session, _: &Target, _: &mut State) -> Live<()> {
        Live::Ready(())
    }
    pub fn no_end(_: Session) {}
    pub fn always(_: &State, _: FieldId) -> bool {
        true
    }
    pub fn unlinked(_: &mut State, _: FieldId) {}
    pub fn no_custom(_: Session, _: FieldId) -> AnyView {
        ().into_any()
    }
}

#[derive(Clone, Copy)]
struct Open {
    family: &'static Family,
}

#[derive(Clone, Copy, Default)]
struct Host {
    family: RwSignal<Option<Open>>,
    target: RwSignal<Option<Target>>,
    state: RwSignal<Option<State>>,
    ready: RwSignal<bool>,
    /// Why the dialog cannot show the selection.
    failure: RwSignal<Option<String>>,
    /// The tab each dialog showed when it last closed: Excel reopens a dialog on it.
    last_tab: RwSignal<Vec<(&'static str, &'static str)>>,
    element: NodeRef<leptos::html::Dialog>,
}

impl Host {
    fn family(self) -> Option<&'static Family> {
        self.family.get_untracked().map(|open| open.family)
    }

    fn usable(self, state: &State, id: FieldId) -> bool {
        self.family().is_some_and(|family| (family.shown)(state, id) && (family.enabled)(state, id))
    }

    fn change(self, id: FieldId, value: Value) {
        let Some(family) = self.family() else { return };
        self.state.update(|state| {
            if let Some(state) = state {
                state.set(id, value);
                (family.changed)(state, id);
            }
        });
    }

    fn press(self, id: FieldId) {
        let Some(family) = self.family() else { return };
        self.state.update(|state| {
            if let Some(state) = state {
                state.message = None;
                (family.press)(state, id);
            }
        });
    }

    fn switch(self, id: FieldId) {
        let Some(family) = self.family() else { return };
        self.state.update(|state| {
            if let Some(state) = state {
                state.switch(id);
                (family.changed)(state, id);
            }
        });
    }
}

/// The open commands of these families, with OK and Cancel.
pub fn commands(view: &ViewId, session: Session, families: &'static [&'static Family]) -> Vec<ViewCommand> {
    let host = session.local::<Host>();
    let mut commands: Vec<_> = families
        .iter()
        .copied()
        .map(|family| {
            ViewCommand::new(view, family.command, family.label, family.chords, move |_| {
                if !matches!(session.seam(), Seam::Idle) {
                    return Err("Finish reviewing the current edit first.".into());
                }
                let target = (family.target)(session)?;
                close(session);
                let mut state = State::new((family.spec)(&target), []);
                if let Some((_, tab)) = host.last_tab.get_untracked().into_iter().find(|(id, _)| *id == family.id)
                    && let Some(index) = state.spec.tabs.iter().position(|candidate| candidate.id == tab)
                {
                    state.show(index);
                }
                host.state.set(Some(state));
                host.ready.set(false);
                host.failure.set(None);
                (family.begin)(session, &target);
                host.target.set(Some(target));
                host.family.set(Some(Open { family }));
                session.dismiss_key_tips();
                poll(session);
                Ok(())
            })
            .when(family.when)
        })
        .collect();
    commands.push(ViewCommand::new(view, ACCEPT, "OK", &[], move |_| accept(session)));
    commands.push(ViewCommand::new(view, CANCEL, "Cancel", &[], move |_| {
        close(session);
        Ok(())
    }));
    commands
}

/// Fills the open dialog from the workbook's answer when it has arrived.
pub fn poll(session: Session) {
    let host = session.local::<Host>();
    let (Some(family), Some(target)) = (host.family(), host.target.get_untracked()) else {
        return;
    };
    if host.ready.get_untracked() || host.failure.with_untracked(Option::is_some) {
        return;
    }
    let mut outcome = Live::Loading;
    host.state.update(|state| {
        if let Some(state) = state {
            outcome = (family.poll)(session, &target, state);
            if outcome == Live::Ready(()) {
                state.settle();
            }
        }
    });
    match outcome {
        Live::Ready(()) => host.ready.set(true),
        Live::Refused(reason) => host.failure.set(Some(crate::refusal_text(reason))),
        Live::Idle | Live::Loading | Live::Outdated => {}
    }
}

/// The open dialog's name, when one is open.
pub fn open(session: Session) -> Option<&'static str> {
    session.local::<Host>().family.get().map(|open| open.family.id)
}

/// Reads the open dialog, tracking its changes.
pub fn read<T>(session: Session, read: impl FnOnce(&State) -> T) -> Option<T> {
    session.local::<Host>().state.with(|state| state.as_ref().map(read))
}

/// The open dialog as it is now, without tracking it.
pub fn state(session: Session) -> Option<State> {
    session.local::<Host>().state.get_untracked()
}

/// What the open dialog acts on.
pub fn target(session: Session) -> Option<Target> {
    session.local::<Host>().target.get_untracked()
}

/// Shows a tab of the open dialog by its name.
pub fn show(session: Session, tab: &str) {
    session.local::<Host>().state.update(|state| {
        if let Some(state) = state
            && let Some(index) = state.spec.tabs.iter().position(|candidate| candidate.id == tab)
        {
            state.show(index);
        }
    });
}

/// Whether the open dialog has read the workbook and can be used.
pub fn is_ready(session: Session) -> bool {
    session.local::<Host>().ready.get_untracked()
}

/// Sets a field as the person does, so the fields that depend on it follow.
pub fn change(session: Session, id: FieldId, value: Value) {
    session.local::<Host>().change(id, value);
}

/// Presses a button of the open dialog.
pub fn press(session: Session, id: FieldId) {
    session.local::<Host>().press(id);
}

/// Gives the open dialog a key. The values follow; the page moves the focus.
pub fn key(session: Session, input: &KeyInput, focus: Focus) -> KeyAction {
    let host = session.local::<Host>();
    let Some(action) = host.state.with_untracked(|state| {
        state.as_ref().map(|state| state.key(input, focus, &|id| host.usable(state, id)))
    }) else {
        return KeyAction::Keep;
    };
    match &action {
        KeyAction::Accept => {
            let _ = accept(session);
        }
        KeyAction::Cancel => {
            close(session);
        }
        KeyAction::Show(tab) => host.state.update(|state| {
            if let Some(state) = state {
                state.show(*tab);
            }
        }),
        KeyAction::Switch(id) => host.switch(id),
        KeyAction::Choose(id, value) => host.change(id, Value::Choice(Some(value.clone()))),
        KeyAction::Press(id) => host.press(id),
        KeyAction::Keep | KeyAction::Focus(_) => {}
    }
    action
}

/// OK: commits the dialog's one change, or says why it cannot.
pub fn accept(session: Session) -> Result<(), String> {
    let host = session.local::<Host>();
    let (Some(family), Some(target)) = (host.family(), host.target.get_untracked()) else {
        return Err("No dialog is open.".into());
    };
    if !host.ready.get_untracked() {
        return Err("The dialog is still reading the workbook.".into());
    }
    let state = host.state.get_untracked().ok_or("No dialog is open.")?;
    let commit = match (family.commit)(&state, &target) {
        Ok(commit) => commit,
        Err(message) => {
            host.state.update(|state| {
                if let Some(state) = state {
                    state.message = Some(message.clone());
                }
            });
            return Err(message);
        }
    };
    if !matches!(commit, Commit::Nothing | Commit::GoTo(_)) {
        if !matches!(session.seam(), Seam::Idle) {
            return Err("Finish reviewing the current edit first.".into());
        }
        if !session.stamp().is_some_and(|stamp| stamp.workbook == target.workbook && stamp.model == target.revision) {
            let message = format!("The workbook changed. Reopen {}.", family.label);
            host.state.update(|state| {
                if let Some(state) = state {
                    state.message = Some(message.clone());
                }
            });
            return Err(message);
        }
    }
    close(session);
    match commit {
        Commit::Nothing => {}
        Commit::Edit(batch) => session.submit(batch),
        Commit::Review(batch) => session.submit_for_review(batch),
        Commit::Run(operation, notice) => session.operate(operation, notice),
        Commit::GoTo(address) => session.go_to(address),
    }
    Ok(())
}

/// Cancel. Returns whether a dialog was open.
pub fn close(session: Session) -> bool {
    let host = session.local::<Host>();
    let Some(family) = host.family() else {
        return false;
    };
    if let Some(element) = host.element.get_untracked() {
        element.close();
    }
    (family.end)(session);
    if let Some(tab) = host.state.with_untracked(|state| {
        state.as_ref().and_then(|state| state.spec.tabs.get(state.tab).map(|tab| tab.id))
    }) {
        host.last_tab.update(|last| {
            last.retain(|(id, _)| *id != family.id);
            last.push((family.id, tab));
        });
    }
    host.family.set(None);
    host.target.set(None);
    host.state.set(None);
    host.ready.set(false);
    host.failure.set(None);
    session.dismiss_key_tips();
    true
}

/// What the page's test probe reports about the open dialog.
pub fn probe(session: Session) -> serde_json::Value {
    let host = session.local::<Host>();
    let Some(mut dialog) = host.state.with_untracked(|state| state.as_ref().map(State::probe)) else {
        return serde_json::Value::Null;
    };
    dialog["ready"] = host.ready.get_untracked().into();
    dialog["failure"] = host.failure.get_untracked().into();
    dialog
}

/// The open dialog, drawn. A view mounts this once.
pub fn mount(session: Session) -> impl IntoView {
    let host = session.local::<Host>();
    let shown = Memo::new(move |_| {
        let stamp = session.stamp()?;
        let open = host.family.get()?;
        host.target.with(|target| target.as_ref().is_some_and(|target| target.workbook == stamp.workbook))
            .then_some(open.family.id)
    });
    move || shown.get().and_then(|_| host.family()).map(|family| frame(session, host, family))
}

fn label(text: &'static str) -> impl IntoView {
    marked(text.to_owned())
}

/// A caption with its access key underlined.
fn marked(text: String) -> impl IntoView {
    let (plain, key) = caption(&text);
    let at = key.and_then(|_| {
        // The mark is before the first unescaped ampersand's letter.
        let mut index = 0;
        let mut letters = text.chars().peekable();
        while let Some(letter) = letters.next() {
            if letter == '&' {
                match letters.next() {
                    Some('&') => index += 1,
                    Some(_) => return Some(index),
                    None => return None,
                }
            } else {
                index += 1;
            }
        }
        None
    });
    match at {
        Some(at) => {
            let letters: Vec<char> = plain.chars().collect();
            let before: String = letters[..at].iter().collect();
            let key: String = letters[at..=at].iter().collect();
            let after: String = letters[at + 1..].iter().collect();
            view! { <span class="dna-dialog-caption">{before}<u>{key}</u>{after}</span> }.into_any()
        }
        None => view! { <span class="dna-dialog-caption">{plain}</span> }.into_any(),
    }
}

fn key_input(event: &leptos::ev::KeyboardEvent) -> KeyInput {
    KeyInput {
        key: event.key(),
        ctrl: event.ctrl_key(),
        alt: event.alt_key(),
        shift: event.shift_key(),
        meta: event.meta_key(),
        alt_graph: event.get_modifier_state("AltGraph"),
        composing: event.is_composing(),
    }
}

fn focus_of(event: &leptos::ev::KeyboardEvent, state: &State) -> Focus {
    let Some(element) = event.target().and_then(|target| target.dyn_into::<leptos::web_sys::Element>().ok()) else {
        return Focus::Frame;
    };
    if element.get_attribute("role").as_deref() == Some("tab") {
        return Focus::Tabs;
    }
    let named = element
        .closest("[data-field]")
        .ok()
        .flatten()
        .and_then(|field| field.get_attribute("data-field"));
    match named.and_then(|name| state.field(&name).map(|field| field.id)) {
        Some(id) => Focus::Field(id),
        None => Focus::Frame,
    }
}

fn focus(host: Host, selector: String) {
    let Some(dialog) = host.element.get_untracked() else { return };
    if let Ok(Some(element)) = dialog.query_selector(&selector)
        && let Ok(element) = element.dyn_into::<leptos::web_sys::HtmlElement>()
    {
        let _ = element.focus();
    }
}

fn frame(session: Session, host: Host, family: &'static Family) -> impl IntoView {
    super::show_modal(host.element);
    Effect::new(move |_| poll(session));
    let spec: Spec = host.state.with_untracked(|state| state.as_ref().map(|state| state.spec.clone())).unwrap_or(Spec {
        id: family.id,
        title: family.label,
        tabs: Vec::new(),
        accept: "OK",
    });
    let keydown = move |event: leptos::ev::KeyboardEvent| {
        // A modal dialog owns the keyboard: the shell and the grid see none of its keys.
        event.stop_propagation();
        let Some(at) = host.state.with_untracked(|state| state.as_ref().map(|state| focus_of(&event, state))) else {
            return;
        };
        let action = key(session, &key_input(&event), at);
        match &action {
            KeyAction::Keep => return,
            KeyAction::Show(_) if at == Focus::Tabs => {
                if let Some(tab) = read(session, |state| state.spec.tabs[state.tab].id) {
                    focus(host, format!("[data-dialog-tab='{tab}']"));
                }
            }
            KeyAction::Focus(id) | KeyAction::Switch(id) | KeyAction::Press(id) => {
                focus(host, format!("[data-field='{id}']"));
            }
            KeyAction::Choose(id, value) => focus(host, format!("[data-field='{id}'] [value='{value}']")),
            KeyAction::Show(_) | KeyAction::Accept | KeyAction::Cancel => {}
        }
        event.prevent_default();
    };
    let tabbed = spec.tabs.len() > 1;
    let strip = tabbed.then(|| {
        let tabs = spec.tabs.iter().enumerate().map(|(index, tab)| {
            let id = tab.id;
            let selected = move || read(session, |state| state.tab == index).unwrap_or(false);
            view! {
                <button type="button" role="tab" data-dialog-tab=id aria-controls=format!("dna-dialog-panel-{id}")
                    aria-selected=move || if selected() { "true" } else { "false" }
                    tabindex=move || if selected() { "0" } else { "-1" }
                    on:click=move |_| host.state.update(|state| {
                        if let Some(state) = state { state.show(index); }
                    })>{tab.label}</button>
            }
        }).collect_view();
        view! { <div class="dna-dialog-tabs" role="tablist" aria-label=spec.title>{tabs}</div> }
    });
    let panels = spec.tabs.iter().enumerate().map(|(index, tab)| {
        let id = tab.id;
        let mut groups: Vec<(&'static str, Vec<Field>)> = Vec::new();
        for field in &tab.fields {
            match groups.last_mut() {
                Some((group, fields)) if *group == field.group => fields.push(field.clone()),
                _ => groups.push((field.group, vec![field.clone()])),
            }
        }
        let body = groups.into_iter().map(|(group, fields)| {
            let controls = fields.into_iter().map(|field| control(session, host, family, field)).collect_view();
            if group.is_empty() {
                view! { <div class="dna-dialog-fields">{controls}</div> }.into_any()
            } else {
                view! { <fieldset class="dna-dialog-group"><legend>{group}</legend>{controls}</fieldset> }.into_any()
            }
        }).collect_view();
        view! {
            <div class="dna-dialog-panel" id=format!("dna-dialog-panel-{id}") data-dialog-panel=id
                role=if tabbed { "tabpanel" } else { "group" }
                hidden=move || read(session, |state| state.tab != index).unwrap_or(true)>{body}</div>
        }
    }).collect_view();
    let waiting = move || {
        if let Some(reason) = host.failure.get() {
            return Some(reason);
        }
        (!host.ready.get()).then(|| "Reading the selection…".to_owned())
    };
    view! {
        <dialog class="dna-workbook-dialog dna-dialog" node_ref=host.element data-dnacalc="dialog" data-dialog=family.id
            data-dna-action-field aria-labelledby="dna-dialog-title"
            data-dialog-ready=move || if host.ready.get() { "true" } else { "false" }
            on:cancel=move |event: leptos::ev::Event| { event.prevent_default(); close(session); }
            on:keydown=keydown
            on:keyup=move |event: leptos::ev::KeyboardEvent| event.stop_propagation()>
            <h2 id="dna-dialog-title">{spec.title}</h2>
            <form on:submit=move |event: leptos::ev::SubmitEvent| {
                event.prevent_default();
                let _ = accept(session);
            }>
                {strip}
                <fieldset class="dna-dialog-body" disabled=move || !host.ready.get()>{panels}</fieldset>
                <p class="dna-dialog-status" role="status" data-dnacalc="dialog-status">{waiting}</p>
                <p class="dna-dialog-message" role="alert" data-dnacalc="dialog-message">
                    {move || read(session, |state| state.message.clone()).flatten()}
                </p>
                <footer>
                    <button type="submit" data-dnacalc="dialog-ok" data-command=ACCEPT
                        disabled=move || !host.ready.get()>{spec.accept}</button>
                    <button type="button" data-dnacalc="dialog-cancel" data-command=CANCEL
                        on:click=move |_| { close(session); }>"Cancel"</button>
                </footer>
            </form>
        </dialog>
    }
}

fn options(choices: Memo<Vec<Choice>>, chosen: impl Fn() -> Option<String> + Copy + Send + Sync + 'static) -> impl IntoView {
    move || {
        let current = chosen();
        choices.get().into_iter().map(|choice| {
            let selected = current.as_deref() == Some(choice.value.as_str());
            let (text, _) = caption(&choice.label);
            view! { <option value=choice.value selected=selected>{text}</option> }
        }).collect_view()
    }
}

/// The standard colours a colour box lists before More Colors.
pub const STANDARD_COLORS: [(&str, &str); 12] = [
    ("000000", "Black"),
    ("FFFFFF", "White"),
    ("C00000", "Dark Red"),
    ("FF0000", "Red"),
    ("FFC000", "Orange"),
    ("FFFF00", "Yellow"),
    ("92D050", "Light Green"),
    ("00B050", "Green"),
    ("00B0F0", "Light Blue"),
    ("0070C0", "Blue"),
    ("002060", "Dark Blue"),
    ("7030A0", "Purple"),
];

fn control(session: Session, host: Host, family: &'static Family, field: Field) -> AnyView {
    let id = field.id;
    let enabled = move || read(session, |state| (family.enabled)(state, id)).unwrap_or(false);
    let hidden = move || !read(session, |state| (family.shown)(state, id)).unwrap_or(false);
    let text = move || read(session, |state| state.text(id).to_owned()).unwrap_or_default();
    let chosen = move || read(session, |state| state.choice(id).map(str::to_owned)).flatten();
    let choices = Memo::new(move |_| {
        read(session, |state| match state.field(id).map(|field| &field.kind) {
            Some(Kind::Options(list) | Kind::DropDown(list) | Kind::List(list, _) | Kind::Combo(list)) => list.clone(),
            _ => Vec::new(),
        })
        .unwrap_or_default()
    });
    let pick = move |event: leptos::ev::Event| host.change(id, Value::Choice(Some(event_target_value(&event))));
    let class = format!("dna-dialog-field dna-dialog-{}", match field.kind {
        Kind::Check => "check",
        Kind::Options(_) => "options",
        Kind::DropDown(_) => "drop-down",
        Kind::List(..) => "list",
        Kind::Edit { .. } => "edit",
        Kind::Spin { .. } => "spin",
        Kind::Combo(_) => "combo",
        Kind::Color { .. } => "color",
        Kind::Button => "button",
        Kind::Toggle => "toggle",
        Kind::Custom => "custom",
        Kind::Note => "note",
    });
    match field.kind {
        Kind::Check => view! {
            <label class=class hidden=hidden>
                <input type="checkbox" data-field=id disabled=move || !enabled()
                    prop:checked=move || read(session, |state| state.flag(id) == Some(true)).unwrap_or(false)
                    prop:indeterminate=move || read(session, |state| state.flag(id).is_none()).unwrap_or(false)
                    on:click=move |_| host.switch(id)/>
                {label(field.label)}
            </label>
        }.into_any(),
        Kind::Options(_) => view! {
            <fieldset class=class data-field=id role="radiogroup" hidden=hidden disabled=move || !enabled()>
                {(!field.label.is_empty()).then(|| view! { <legend>{label(field.label)}</legend> })}
                {move || choices.get().into_iter().map(|choice| {
                    let value = choice.value.clone();
                    let selected = chosen().as_deref() == Some(choice.value.as_str());
                    view! {
                        <label>
                            <input type="radio" name=id value=choice.value.clone() prop:checked=selected
                                on:change=move |_| host.change(id, Value::Choice(Some(value.clone())))/>
                            {marked(choice.label)}
                        </label>
                    }
                }).collect_view()}
            </fieldset>
        }.into_any(),
        Kind::DropDown(_) => view! {
            <label class=class hidden=hidden>{label(field.label)}
                <select data-field=id disabled=move || !enabled() on:change=pick>{options(choices, chosen)}</select>
            </label>
        }.into_any(),
        Kind::List(_, rows) => view! {
            <label class=class hidden=hidden>{label(field.label)}
                <select data-field=id size=rows.to_string() disabled=move || !enabled() on:change=pick>
                    {options(choices, chosen)}
                </select>
            </label>
        }.into_any(),
        Kind::Edit { max_length } => view! {
            <label class=class hidden=hidden>{label(field.label)}
                <input type="text" data-field=id maxlength=max_length.to_string() disabled=move || !enabled()
                    prop:value=text on:input=move |event| host.change(id, Value::Text(event_target_value(&event)))/>
            </label>
        }.into_any(),
        Kind::Spin { min, max } => view! {
            <label class=class hidden=hidden>{label(field.label)}
                <input type="number" data-field=id min=min.to_string() max=max.to_string() step="1"
                    disabled=move || !enabled() prop:value=text
                    on:input=move |event| host.change(id, Value::Text(event_target_value(&event)))/>
            </label>
        }.into_any(),
        Kind::Combo(_) => view! {
            <label class=class hidden=hidden>{label(field.label)}
                <input type="text" data-field=id maxlength="255" disabled=move || !enabled() prop:value=text
                    on:input=move |event| host.change(id, Value::Text(event_target_value(&event)))/>
                <select size="6" data-field-list=id aria-label=caption(field.label).0 disabled=move || !enabled()
                    on:change=move |event| host.change(id, Value::Text(event_target_value(&event)))>
                    {options(choices, move || Some(text()))}
                </select>
            </label>
        }.into_any(),
        Kind::Color { automatic } => {
            let listed = move || {
                let current = chosen();
                let custom = current.clone().filter(|value| {
                    value != "auto" && !STANDARD_COLORS.iter().any(|(rgb, _)| rgb == value)
                });
                let entry = |value: String, name: String| {
                    let selected = current.as_deref() == Some(value.as_str());
                    view! { <option value=value selected=selected>{name}</option> }
                };
                let mut entries = vec![entry("auto".into(), automatic.into())];
                entries.extend(STANDARD_COLORS.iter().map(|(rgb, name)| entry((*rgb).into(), (*name).into())));
                entries.extend(custom.map(|rgb| entry(rgb.clone(), format!("#{rgb}"))));
                if current.is_none() {
                    // A selection of several colours shows an empty box until one is chosen.
                    entries.insert(0, view! { <option value=String::new() selected=true>{String::new()}</option> });
                }
                entries.collect_view()
            };
            view! {
                <label class=class hidden=hidden>{label(field.label)}
                    <select data-field=id disabled=move || !enabled()
                        on:change=move |event| {
                            let value = event_target_value(&event);
                            if !value.is_empty() { host.change(id, Value::Choice(Some(value))); }
                        }>{listed}</select>
                    <input type="color" data-field-more=id aria-label="More Colors" disabled=move || !enabled()
                        prop:value=move || chosen().filter(|value| value != "auto").map_or_else(|| "#000000".to_owned(), |rgb| format!("#{rgb}"))
                        on:input=move |event| {
                            let value = event_target_value(&event);
                            host.change(id, Value::Choice(Some(value.trim_start_matches('#').to_ascii_uppercase())));
                        }/>
                </label>
            }.into_any()
        }
        Kind::Button => view! {
            <button type="button" class=class data-field=id hidden=hidden disabled=move || !enabled()
                on:click=move |_| host.press(id)>{label(field.label)}</button>
        }.into_any(),
        Kind::Toggle => view! {
            <button type="button" class=class data-field=id hidden=hidden disabled=move || !enabled()
                aria-pressed=move || match chosen().as_deref() {
                    None => "mixed",
                    Some("none") => "false",
                    Some(_) => "true",
                }
                on:click=move |_| host.press(id)>{label(field.label)}</button>
        }.into_any(),
        Kind::Custom => view! {
            <div class=class data-field=id hidden=hidden>
                {(!field.label.is_empty()).then(|| label(field.label))}
                {(family.custom)(session, id)}
            </div>
        }.into_any(),
        Kind::Note => view! { <p class=class data-field=id hidden=hidden>{field.label}</p> }.into_any(),
    }
}
