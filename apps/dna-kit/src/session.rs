//! The page's workbook session as signals.
//!
//! [`Session`] is the one handle the shell and every view hold. It binds the runtime's
//! [`Client`] (the requests in flight, the frame store, the typed watches, the one draft, the
//! seam) to Leptos signals, and it holds the one command table and the one keymap. A carrier is a
//! send function and a call to [`Session::deliver`]; nothing else on the page talks to the wire.
//!
//! Reads are reactive: a component that calls [`Session::active`] runs again when the selection
//! moves and at no other time, and a component that reads a watch runs again when a frame touches
//! that watch.

use crate::view::ViewCommand;
use calc_skin::grid::Key;
use calc_skin::query::{self, Query};
use calc_skin::{
    Answer, Batch, Operation, Packet, Refusal, Selection, Stamp, Target, ViewId, WorkbookSummary,
};
use dna_skin_runtime::Live;
use dna_skin_runtime::client::{Changes, Client, EditSession, Seam, Watch, WatchKey};
use dna_skin_runtime::commands::{
    Chord, CommandError, CommandId, CommandSpec, CommandTable, Context, InputMode, Invocation,
    KeyInput, Keymap, Layer, Resolution, When,
};
use leptos::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

/// Where the keyboard focus is when a key arrives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    /// The stage: a view's body. A printable key starts a draft at the active cell.
    Stage,
    /// The session's editor (the formula bar, an in-cell editor). Text goes to the editor.
    Editor,
    /// Another text field (the Name Box, the search field). Only chords with Alt reach the
    /// keymap, so typing and the field's own Enter, arrows and Ctrl+Z stay the field's.
    Field,
}

struct Inner {
    client: RefCell<Client>,
    send: Box<dyn Fn(Packet)>,
    ticks: RefCell<BTreeMap<WatchKey, ArcRwSignal<u64>>>,
    commands: RefCell<CommandTable>,
    keymap: RefCell<Keymap>,
    summary: Cell<Option<Watch<query::Summary>>>,
    /// The watch on the active cell's entry, which the editor shows and F2 opens.
    entry: Cell<Option<(Key, Watch<query::Formula>)>>,
    /// A cell whose entry was asked to open before it arrived.
    opening: Cell<Option<Key>>,
    notebook_profile: RefCell<Option<ViewId>>,
    after_commit: RefCell<Option<Box<dyn FnOnce()>>>,
}

#[derive(Clone, Copy)]
pub struct Session {
    inner: StoredValue<Rc<Inner>, LocalStorage>,
    stamp: RwSignal<u64>,
    selection: RwSignal<u64>,
    edit: RwSignal<u64>,
    seam: RwSignal<u64>,
    notice: RwSignal<u64>,
    view: RwSignal<ViewId>,
}

fn tick(signal: impl Update<Value = u64>) {
    signal.update(|count| *count = count.wrapping_add(1));
}

impl Session {
    /// A session whose packets leave through `send`. `view` is the view on stage at the start.
    pub fn new(view: ViewId, send: impl Fn(Packet) + 'static) -> Self {
        let session = Self {
            inner: StoredValue::new_local(Rc::new(Inner {
                client: RefCell::default(),
                send: Box::new(send),
                ticks: RefCell::default(),
                commands: RefCell::default(),
                keymap: RefCell::default(),
                summary: Cell::new(None),
                entry: Cell::new(None),
                opening: Cell::new(None),
                notebook_profile: RefCell::new(None),
                after_commit: RefCell::new(None),
            })),
            stamp: RwSignal::new(0),
            selection: RwSignal::new(0),
            edit: RwSignal::new(0),
            seam: RwSignal::new(0),
            notice: RwSignal::new(0),
            view: RwSignal::new(view),
        };
        let summary = session.watch(query::Summary);
        session.inner().summary.set(Some(summary));
        session
    }

    fn inner(self) -> Rc<Inner> {
        self.inner.get_value()
    }

    /// Reads the client without subscribing to anything.
    fn peek<T>(self, read: impl FnOnce(&Client) -> T) -> T {
        read(&self.inner().client.borrow())
    }

    /// One call into the client: sends what it queued, then tells the signals what moved. The
    /// client is released before anything is sent, so a carrier may answer inside `send`.
    fn update(self, call: impl FnOnce(&mut Client) -> Changes) {
        let inner = self.inner();
        let mut changes = call(&mut inner.client.borrow_mut());
        if changes.selection || changes.stamp {
            self.aim_entry(&inner);
        }
        self.flush(&inner);
        if let Some(at) = inner.opening.get() {
            self.open_entry(&inner, at, &mut changes);
            self.flush(&inner);
        }
        let continuation = (changes.seam && self.peek(|client| matches!(client.seam(), Seam::Idle)))
            .then(|| inner.after_commit.borrow_mut().take())
            .flatten();
        let touched: Vec<ArcRwSignal<u64>> = {
            let ticks = inner.ticks.borrow();
            changes.watches.iter().filter_map(|key| ticks.get(key).cloned()).collect()
        };
        touched.into_iter().for_each(tick);
        for (moved, signal) in [
            (changes.stamp, self.stamp),
            (changes.selection, self.selection),
            (changes.edit, self.edit),
            (changes.seam, self.seam),
            (changes.notice, self.notice),
        ] {
            if moved {
                tick(signal);
            }
        }
        if changes.committed {
            if let Some(continuation) = continuation {
                continuation();
            }
        }
    }

    fn flush(self, inner: &Inner) {
        loop {
            let packets = inner.client.borrow_mut().take_outbox();
            if packets.is_empty() {
                return;
            }
            packets.into_iter().for_each(|packet| (inner.send)(packet));
        }
    }

    /// Keeps the entry watch on the active cell.
    fn aim_entry(self, inner: &Inner) {
        let mut client = inner.client.borrow_mut();
        let active = client.active();
        let held = inner.entry.get();
        if held.map(|(at, _)| at) == active {
            return;
        }
        if let Some((_, watch)) = held {
            client.unwatch(watch);
            inner.ticks.borrow_mut().remove(&watch.key());
        }
        inner.entry.set(active.map(|at| {
            let watch = client.watch(query::Formula { at });
            inner.ticks.borrow_mut().insert(watch.key(), ArcRwSignal::new(0));
            (at, watch)
        }));
    }

    /// Opens the entry of `at` for editing once it is known. The request is dropped when the
    /// selection has moved on.
    fn open_entry(self, inner: &Inner, at: Key, changes: &mut Changes) {
        let mut client = inner.client.borrow_mut();
        let entry = match inner.entry.get() {
            Some((held, watch)) if held == at && client.active() == Some(at) => client.read(watch),
            _ => {
                inner.opening.set(None);
                return;
            }
        };
        match entry {
            Live::Ready(answer) => {
                inner.opening.set(None);
                let began = if self.notebook_profile(&self.view.get_untracked()) && client.held_edit(at).is_some() {
                    client.resume_edit(at)
                } else {
                    client.begin_edit(at, None, &answer.body.input)
                };
                changes.edit |= began.edit;
                changes.selection |= began.selection;
            }
            Live::Refused(reason) => {
                inner.opening.set(None);
                changes.notice |= client.refuse(reason).notice;
            }
            Live::Idle | Live::Loading | Live::Outdated => (),
        }
    }

    // The carrier side

    pub fn deliver(self, packet: Packet) {
        self.update(|client| client.deliver(packet));
    }

    /// Shows a failure the carrier met (a worker that would not start, a file that would not read).
    pub fn refuse(self, reason: Refusal) {
        self.update(|client| client.refuse(reason));
    }

    // Reactive reads

    pub fn stamp(self) -> Option<Stamp> {
        self.stamp.track();
        self.peek(Client::stamp)
    }

    pub fn selection(self) -> Option<Selection> {
        self.selection.track();
        self.peek(|client| client.selection().cloned())
    }

    pub fn active(self) -> Option<Key> {
        self.selection.track();
        self.peek(Client::active)
    }

    pub fn edit(self) -> Option<EditSession> {
        self.edit.track();
        self.peek(|client| client.edit().cloned())
    }

    pub fn mode(self) -> InputMode {
        self.edit.track();
        let view = self.view();
        self.mode_in(&view)
    }

    fn mode_in(self, view: &ViewId) -> InputMode {
        match self.peek(Client::mode) {
            InputMode::Ready if self.notebook_profile(view) => InputMode::NotebookCommand,
            mode => mode,
        }
    }

    fn notebook_profile(self, view: &ViewId) -> bool {
        self.inner().notebook_profile.borrow().as_ref() == Some(view)
    }

    /// A command-mode view keeps an unrun Excel edit when Escape returns to its stage.
    pub fn enable_notebook_keys(self, view: ViewId) {
        self.inner().notebook_profile.replace(Some(view));
    }

    pub fn held_edit(self, at: Key) -> Option<EditSession> {
        self.edit.track();
        self.peek(|client| client.held_edit(at).cloned())
    }

    pub fn held_edits(self) -> usize {
        self.edit.track();
        self.peek(Client::held_edits)
    }

    /// Bring a retained draft back to the shared editor with its original base.
    pub fn resume_held(self, at: Key) -> bool {
        if self.edit().is_some() || !matches!(self.seam(), Seam::Idle) || self.held_edit(at).is_none() {
            return false;
        }
        self.update(|client| {
            let mut changes = client.select_cell(at);
            changes.edit = client.resume_edit(at).edit;
            changes
        });
        true
    }

    pub fn revert_held(self, at: Key) {
        self.update(|client| client.revert_held(at));
    }

    pub fn seam(self) -> Seam {
        self.seam.track();
        self.peek(|client| client.seam().clone())
    }

    pub fn notice(self) -> String {
        self.notice.track();
        self.peek(|client| client.notice().to_owned())
    }

    pub fn summary(self) -> Option<WorkbookSummary> {
        let watch = self.inner().summary.get()?;
        match self.read(watch) {
            Live::Ready(answer) => Some(answer.body),
            _ => None,
        }
    }

    /// What the editor shows: the draft while there is one, otherwise the active cell's entry.
    pub fn entry(self) -> String {
        if let Some(edit) = self.edit() {
            return edit.text;
        }
        self.selection.track();
        match self.inner().entry.get() {
            Some((_, watch)) => match self.read(watch) {
                Live::Ready(answer) => answer.body.input,
                _ => String::new(),
            },
            None => String::new(),
        }
    }

    // Watches

    pub fn watch<Q: Query>(self, query: Q) -> Watch<Q> {
        let inner = self.inner();
        let watch = inner.client.borrow_mut().watch(query);
        inner.ticks.borrow_mut().insert(watch.key(), ArcRwSignal::new(0));
        self.flush(&inner);
        watch
    }

    pub fn unwatch<Q>(self, watch: Watch<Q>) {
        let inner = self.inner();
        inner.client.borrow_mut().unwatch(watch);
        inner.ticks.borrow_mut().remove(&watch.key());
        self.flush(&inner);
    }

    /// The watch's answer, typed. The caller runs again when a frame touches the watch.
    pub fn read<Q: Query>(self, watch: Watch<Q>) -> Live<Answer<Q::Answer>> {
        let inner = self.inner();
        if let Some(signal) = inner.ticks.borrow().get(&watch.key()) {
            signal.track();
        }
        inner.client.borrow().read(watch)
    }

    /// A watch that follows a query which changes: the cells under a viewport, the geometry of
    /// the sheet on show. It is dropped with the component that made it.
    pub fn following<Q: Query>(self) -> Following<Q> {
        let held = StoredValue::new_local(None::<(Q, Watch<Q>)>);
        on_cleanup(move || {
            if let Some(Some((_, watch))) = held.try_get_value() {
                self.unwatch(watch);
            }
        });
        Following {
            session: self,
            held,
        }
    }

    // Selection, the draft and the seam

    pub fn select(self, selection: Selection) {
        self.update(|client| client.select(selection));
    }

    pub fn select_cell(self, at: Key) {
        self.update(|client| client.select_cell(at));
    }

    pub fn go_to(self, address: String) {
        self.update(|client| {
            client.go_to(address);
            Changes::default()
        });
    }

    pub fn navigate(self, target: Target) {
        self.update(|client| {
            client.navigate(target);
            Changes::default()
        });
    }

    /// Starts the draft at `at`: `Some(text)` replaces the entry (typing over a cell), `None`
    /// opens the entry the cell has, as soon as the host has said what it is.
    pub fn begin_edit(self, at: Key, text: Option<String>) {
        match text {
            Some(text) => self.update(|client| client.begin_edit(at, Some(text), "")),
            None => {
                self.inner().opening.set(Some(at));
                self.update(|client| client.select_cell(at));
            }
        }
    }

    /// The editor's text changed. With no draft this starts one at the active cell.
    pub fn edit_text(self, text: String) {
        self.update(|client| match (client.edit().is_some(), client.active()) {
            (true, _) => client.edit_text(text),
            (false, Some(at)) => {
                let mut changes = client.begin_edit(at, Some(text), "");
                changes.edit = true;
                changes
            }
            (false, None) => Changes::default(),
        });
    }

    pub fn cancel(self) {
        self.inner().opening.set(None);
        if self.notebook_profile(&self.view.get_untracked()) {
            self.update(Client::hold_edit);
        } else {
            self.update(Client::cancel);
        }
    }

    pub fn commit_edit(self) {
        self.update(Client::commit_edit);
    }

    /// Runs a navigation or other follow-up only after the carrier confirms the edit.
    pub fn commit_edit_then(self, after: impl FnOnce() + 'static) {
        if !matches!(self.seam(), Seam::Idle) || self.edit().is_none() {
            return;
        }
        self.inner().after_commit.replace(Some(Box::new(after)));
        self.commit_edit();
    }

    /// Sends a change through the seam: preview, then commit when the preview is clean.
    pub fn submit(self, batch: Batch) {
        self.update(|client| client.submit(batch));
    }

    pub fn apply_review(self) {
        self.update(Client::apply_review);
    }

    pub fn operate(self, operation: Operation, notice: Option<String>) {
        self.update(|client| {
            client.run(operation, notice);
            Changes::default()
        });
    }

    pub fn open(self, name: String, bytes: Vec<u8>) {
        self.update(|client| client.open(name, bytes));
    }

    // Views

    pub fn view(self) -> ViewId {
        self.view.get()
    }

    pub fn show(self, view: ViewId) {
        let previous = self.view.get_untracked();
        if previous != view {
            if self.notebook_profile(&previous) && self.edit().is_some() && matches!(self.seam(), Seam::Idle) {
                self.cancel();
            }
            self.view.set(view);
        }
    }

    // Commands and keys

    pub fn context(self) -> Context {
        let summary = self.inner().summary.get();
        let view = self.view.get_untracked();
        let mode = self.mode_in(&view);
        self.peek(|client| Context {
            view,
            mode,
            read_only: summary.is_some_and(|watch| match client.read(watch) {
                Live::Ready(answer) => answer.body.capabilities.read_only,
                _ => false,
            }),
        })
    }

    pub fn register(
        self,
        spec: CommandSpec,
        handler: impl Fn(&Invocation) -> Result<(), String> + 'static,
    ) {
        self.inner().commands.borrow_mut().register(spec, handler);
    }

    pub fn bind(self, layer: Layer, when: When, chord: &str, command: CommandId) {
        self.inner()
            .keymap
            .borrow_mut()
            .bind(layer, when, chord, command);
    }

    /// Registers a view's commands with their profile keys.
    pub fn install(self, commands: Vec<ViewCommand>) {
        for command in commands {
            let id = command.spec.id.clone();
            let run = command.run;
            self.register(command.spec, move |invocation| run(invocation));
            for (when, chord) in command.chords {
                self.bind(Layer::View, when, chord, id.clone());
            }
        }
    }

    /// Runs a command. Every route ends here: a key, a Deck button, the command search, a skin's
    /// sequence, a view request from a macro or an agent. A refusal is shown on the Strip.
    pub fn run(self, invocation: &Invocation) -> Result<(), CommandError> {
        let inner = self.inner();
        let handler = inner
            .commands
            .borrow()
            .handler(&invocation.command, &self.context());
        let result = handler.and_then(|run| run(invocation).map_err(CommandError::Refused));
        if let Err(error) = &result {
            let text = match error {
                CommandError::Unknown(id) => format!("There is no command {id}."),
                CommandError::NotNow(id) => {
                    let commands = inner.commands.borrow();
                    let label = commands.spec(id).map_or(id.as_str(), |spec| &spec.label);
                    format!("{label} is not available now.")
                }
                CommandError::Refused(text) => text.clone(),
            };
            self.update(|client| client.refuse(Refusal::Invalid { reason: text }));
        }
        result
    }

    pub fn command(self, id: &'static str) -> Result<(), CommandError> {
        self.run(&Invocation::of(CommandId::new(id)))
    }

    /// A key press. `true` when the session took it and the browser must not.
    pub fn key(self, input: &KeyInput, focus: Focus) -> bool {
        if focus == Focus::Field && !(input.alt && !input.alt_graph) {
            return false;
        }
        let context = self.context();
        let resolution = self.inner().keymap.borrow().resolve(input, &context);
        match resolution {
            Resolution::Command(command) => {
                let _ = self.run(&Invocation::of(command));
                true
            }
            Resolution::Text | Resolution::Unbound => {
                let typed = (focus == Focus::Stage && context.mode == InputMode::Ready)
                    .then(|| input.text())
                    .flatten()
                    .zip(self.peek(Client::active));
                match typed {
                    Some((text, at)) => {
                        self.begin_edit(at, Some(text.to_owned()));
                        true
                    }
                    None => false,
                }
            }
        }
    }

    /// The keys a command answers to now, for a button's hint and the search list.
    pub fn chords_for(self, command: &CommandId) -> Vec<Chord> {
        self.inner()
            .keymap
            .borrow()
            .chords_for(command, &self.context())
    }

    /// The commands that can run now and match `text`.
    pub fn search(self, text: &str) -> Vec<CommandSpec> {
        self.inner()
            .commands
            .borrow()
            .search(text, &self.context())
            .into_iter()
            .cloned()
            .collect()
    }
}

/// A watch that follows a changing query. See [`Session::following`].
pub struct Following<Q: Query> {
    session: Session,
    held: StoredValue<Option<(Q, Watch<Q>)>, LocalStorage>,
}

impl<Q: Query> Clone for Following<Q> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<Q: Query> Copy for Following<Q> {}

impl<Q: Query> Following<Q> {
    /// The answer to `query`, re-aiming the watch when the query is not the one held. `None`
    /// drops the watch.
    pub fn at(&self, query: Option<Q>) -> Live<Answer<Q::Answer>> {
        let held = self.held.get_value();
        if held.as_ref().map(|(held, _)| held) != query.as_ref() {
            if let Some((_, watch)) = held {
                self.session.unwatch(watch);
            }
            self.held.set_value(query.map(|query| {
                let watch = self.session.watch(query.clone());
                (query, watch)
            }));
        }
        match self.held.get_value() {
            Some((_, watch)) => self.session.read(watch),
            None => Live::Idle,
        }
    }
}
