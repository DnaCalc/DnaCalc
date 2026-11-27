//! The page's end of the workbook wire, with no carrier and no toolkit.
//!
//! One [`Client`] owns everything the page knows about the workbook session: the requests in
//! flight, the tile fragments of the frame being received, the frame store, the typed watches, the
//! one edit draft and the one commit in progress. A carrier hands it packets ([`Client::deliver`])
//! and sends what it queues ([`Client::take_outbox`]). The shell and every view work through it, so
//! a second view needs no new state in the shell and a second carrier needs no new logic.
//!
//! It is plain data and methods. `deliver` and every command return a [`Changes`] saying what
//! moved, so a toolkit binding (dna-kit's `Session`) can update exactly those signals.

use crate::commands::InputMode;
use crate::workbook::FrameStore;
use crate::workbook_tiles::TileFrames;
use crate::Live;
use calc_skin::grid::{Area, Key, Sheet};
use calc_skin::query::{self, Query};
use calc_skin::{
    Answer, Batch, Budget, Change, Envelope, Frame, Message, ModelChange, Operation, Packet,
    Preview, PreviewDepth, Refusal, Revision, Selection, SourceRef, Stamp, Target, WatchId,
    WorkbookId, WorkbookReply, WorkbookRequest,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::marker::PhantomData;

/// The budget of an ordinary page request.
pub const PAGE_BUDGET: Budget = Budget {
    cells: 4096,
    nodes: 4096,
    edges: 4096,
    bytes: 4 * 1024 * 1024,
    millis: 100,
};

/// A watch the page holds, valid from the call that made it. The host's identity for it arrives
/// later and never leaves the client.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WatchKey(u64);

#[derive(Debug)]
pub struct Watch<Q> {
    key: WatchKey,
    query: PhantomData<fn() -> Q>,
}

impl<Q> Clone for Watch<Q> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<Q> Copy for Watch<Q> {}

impl<Q> Watch<Q> {
    pub fn key(&self) -> WatchKey {
        self.key
    }
}

/// The one text draft. A view switch does not end it and nothing commits it implicitly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditSession {
    pub at: Key,
    pub text: String,
    /// `Enter` when typing replaced the cell, `Edit` when the existing entry was opened.
    pub mode: InputMode,
    workbook: WorkbookId,
    base: Revision,
    original: String,
}

/// Where a change stands between the person's intent and the workbook.
#[derive(Clone, Debug, PartialEq)]
pub enum Seam {
    Idle,
    /// Waiting for the host to confirm the selection the change was made against.
    Queued,
    Previewing,
    /// The preview was requested for review or carries warnings: the person applies or cancels.
    Review(Preview),
    Committing,
}

/// What a call changed. Empty when nothing a binding shows moved.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Changes {
    pub watches: BTreeSet<WatchKey>,
    pub stamp: bool,
    pub selection: bool,
    pub edit: bool,
    pub committed: bool,
    pub seam: bool,
    pub notice: bool,
}

impl Changes {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    fn merge(&mut self, other: Self) {
        self.watches.extend(other.watches);
        self.stamp |= other.stamp;
        self.selection |= other.selection;
        self.edit |= other.edit;
        self.committed |= other.committed;
        self.seam |= other.seam;
        self.notice |= other.notice;
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Submission {
    Draft,
    Direct,
    Review,
}

enum Pending {
    Quiet,
    Open,
    Watch(WatchKey),
    Select { client_seq: u64 },
    Preview,
    Commit { edit: bool },
    Run { notice: Option<String> },
    Resolve,
    Navigate,
}

struct Slot {
    id: Option<WatchId>,
}

#[derive(Default)]
pub struct Client {
    next_request: u64,
    next_watch: u64,
    next_batch: u64,
    pending: BTreeMap<u64, Pending>,
    outbox: VecDeque<Packet>,
    tiles: TileFrames,
    store: FrameStore,
    slots: BTreeMap<WatchKey, Slot>,
    /// Watches dropped before the host named them: unwatched the moment it does.
    orphans: BTreeSet<WatchKey>,
    edit: Option<EditSession>,
    held: BTreeMap<Key, EditSession>,
    seam: Option<(Seam, Batch, Submission)>,
    notice: String,
    last_refusal: Option<Refusal>,
}

impl Client {
    /// The packets to send, in order. The carrier calls this after every call into the client.
    pub fn take_outbox(&mut self) -> Vec<Packet> {
        self.outbox.drain(..).collect()
    }

    pub fn stamp(&self) -> Option<Stamp> {
        self.store.stamp()
    }

    /// The selection to show: the optimistic one while the host has not answered.
    pub fn selection(&self) -> Option<&Selection> {
        self.store.selection()
    }

    pub fn active(&self) -> Option<Key> {
        self.selection().and_then(|selection| selection.active)
    }

    pub fn edit(&self) -> Option<&EditSession> {
        self.edit.as_ref()
    }

    pub fn held_edit(&self, at: Key) -> Option<&EditSession> {
        self.held.get(&at)
    }

    pub fn held_edits(&self) -> usize {
        self.held.len()
    }

    pub fn revert_held(&mut self, at: Key) -> Changes {
        Changes {
            edit: self.held.remove(&at).is_some(),
            ..Changes::default()
        }
    }

    pub fn seam(&self) -> &Seam {
        self.seam.as_ref().map_or(&Seam::Idle, |(seam, ..)| seam)
    }

    pub fn mode(&self) -> InputMode {
        self.edit.as_ref().map_or(InputMode::Ready, |edit| edit.mode)
    }

    /// The Strip's message: the last refusal or the last undo notice. Empty when there is none.
    pub fn notice(&self) -> &str {
        &self.notice
    }

    pub fn last_refusal(&self) -> Option<&Refusal> {
        self.last_refusal.as_ref()
    }

    fn busy(&self) -> bool {
        matches!(self.seam(), Seam::Previewing | Seam::Committing)
    }

    fn send(&mut self, request: WorkbookRequest, bytes: Option<Vec<u8>>, pending: Pending) {
        self.next_request += 1;
        self.pending.insert(self.next_request, pending);
        self.outbox.push_back(Packet {
            envelope: Envelope::new(self.next_request, Message::WorkbookRequest(request)),
            bytes,
        });
    }

    fn say(&mut self, text: impl Into<String>) -> Changes {
        let text = text.into();
        let notice = self.notice != text || self.last_refusal.take().is_some();
        self.notice = text;
        Changes {
            notice,
            ..Changes::default()
        }
    }

    pub fn refuse(&mut self, reason: Refusal) -> Changes {
        let text = refusal_text(reason.clone());
        let notice = self.notice != text || self.last_refusal.as_ref() != Some(&reason);
        self.notice = text;
        self.last_refusal = Some(reason);
        Changes { notice, ..Changes::default() }
    }

    // Watches

    pub fn watch<Q: Query>(&mut self, query: Q) -> Watch<Q> {
        self.watch_with_budget(query, PAGE_BUDGET)
    }

    pub fn watch_with_budget<Q: Query>(&mut self, query: Q, budget: Budget) -> Watch<Q> {
        self.next_watch += 1;
        let key = WatchKey(self.next_watch);
        let query = query.request();
        self.slots.insert(key, Slot { id: None });
        self.send(
            WorkbookRequest::Watch {
                query,
                budget,
            },
            None,
            Pending::Watch(key),
        );
        Watch {
            key,
            query: PhantomData,
        }
    }

    pub fn unwatch<Q>(&mut self, watch: Watch<Q>) {
        let Some(slot) = self.slots.remove(&watch.key) else {
            return;
        };
        match slot.id {
            Some(id) => {
                self.store.unwatch(id);
                self.send(WorkbookRequest::Unwatch(id), None, Pending::Quiet);
            }
            None => {
                self.orphans.insert(watch.key);
            }
        }
    }

    /// The watch's current answer, typed. `Loading` until the first frame names it.
    pub fn read<Q: Query>(&self, watch: Watch<Q>) -> Live<Answer<Q::Answer>> {
        let live = self
            .slots
            .get(&watch.key)
            .and_then(|slot| slot.id)
            .and_then(|id| self.store.live(id));
        match live {
            None | Some(Live::Loading) => Live::Loading,
            Some(Live::Idle) => Live::Idle,
            Some(Live::Outdated) => Live::Outdated,
            Some(Live::Refused(reason)) => Live::Refused(reason.clone()),
            Some(Live::Ready(answer)) => match Q::decode(answer.clone()) {
                Ok(answer) => Live::Ready(answer),
                Err(reason) => Live::Refused(reason),
            },
        }
    }

    // Selection and navigation

    pub fn select(&mut self, selection: Selection) -> Changes {
        if self.selection().is_some_and(|shown| {
            shown.target == selection.target && shown.active == selection.active
        }) {
            return Changes::default();
        }
        let client_seq = self.store.select(selection.clone());
        self.send(
            WorkbookRequest::Select {
                selection,
                client_seq,
            },
            None,
            Pending::Select { client_seq },
        );
        Changes {
            selection: true,
            ..Changes::default()
        }
    }

    pub fn select_cell(&mut self, at: Key) -> Changes {
        self.select(Selection {
            seq: 0,
            target: cell_source(at),
            active: Some(at),
            grouped: Vec::new(),
            item: None,
        })
    }

    /// Goes to what an address names: the Name Box and the command search.
    pub fn go_to(&mut self, address: String) {
        self.send(
            WorkbookRequest::Ask {
                query: query::Resolve { address }.request(),
                budget: PAGE_BUDGET,
            },
            None,
            Pending::Resolve,
        );
    }

    pub fn navigate(&mut self, target: Target) {
        self.send(WorkbookRequest::Navigate(target), None, Pending::Navigate);
    }

    // The draft and the seam

    /// Starts a draft at `at`. `text` replaces the entry (typing into a cell); `None` opens
    /// `current`, the cell's entry, for editing.
    pub fn begin_edit(&mut self, at: Key, text: Option<String>, current: &str) -> Changes {
        let Some(stamp) = self.stamp() else {
            return Changes::default();
        };
        if self.busy() {
            return Changes::default();
        }
        let mut changes = if self.active() == Some(at) {
            Changes::default()
        } else {
            self.select_cell(at)
        };
        self.edit = Some(EditSession {
            at,
            mode: if text.is_some() {
                InputMode::Enter
            } else {
                InputMode::Edit
            },
            text: text.unwrap_or_else(|| current.to_owned()),
            workbook: stamp.workbook,
            base: stamp.model,
            original: current.to_owned(),
        });
        changes.edit = true;
        changes
    }

    pub fn edit_text(&mut self, text: String) -> Changes {
        match &mut self.edit {
            Some(edit) if edit.text != text => {
                edit.text = text;
                Changes {
                    edit: true,
                    ..Changes::default()
                }
            }
            _ => Changes::default(),
        }
    }

    /// Esc: drops the draft and any change waiting or under review. A change the host is already
    /// working on cannot be cancelled here.
    pub fn cancel(&mut self) -> Changes {
        if self.busy() {
            return Changes::default();
        }
        let mut changes = self.say("");
        changes.edit = self.edit.take().is_some();
        changes.seam = self.seam.take().is_some();
        changes
    }

    /// Restore a held draft without rebasing it on changes since it was written.
    pub fn resume_edit(&mut self, at: Key) -> Changes {
        if self.busy() {
            return Changes::default();
        }
        let Some(draft) = self.held.remove(&at) else {
            return Changes::default();
        };
        self.edit = Some(draft);
        Changes { edit: true, ..Changes::default() }
    }

    /// Return to command mode, retaining unsent text against its original revision.
    pub fn hold_edit(&mut self) -> Changes {
        if self.busy() {
            return Changes::default();
        }
        let Some(draft) = self.edit.take() else {
            return self.cancel();
        };
        let mut changes = self.cancel();
        if draft.text != draft.original {
            self.held.insert(draft.at, draft);
        }
        changes.edit = true;
        changes
    }

    /// Enter: sends the draft through the seam.
    pub fn commit_edit(&mut self) -> Changes {
        let Some(edit) = self.edit.clone() else {
            return Changes::default();
        };
        if !matches!(self.seam(), Seam::Idle | Seam::Queued) {
            return Changes::default();
        }
        let batch = Batch {
            id: String::new(),
            workbook: Some(edit.workbook),
            base: Some(edit.base),
            expects: vec![cell_source(edit.at)],
            selection_seq: None,
            label: None,
            changes: vec![Change::Model(ModelChange::Enter {
                at: edit.at,
                text: edit.text,
            })],
        };
        self.start(batch, Submission::Draft)
    }

    /// Sends any batch through the seam: preview, then commit when the preview is clean, or
    /// [`Seam::Review`] when it carries warnings. The client fills in the batch identity and the
    /// selection it was made against.
    pub fn submit(&mut self, batch: Batch) -> Changes {
        self.submit_batch(batch, Submission::Direct)
    }

    /// Previews the change through the same seam and waits for an explicit Apply.
    pub fn submit_for_review(&mut self, batch: Batch) -> Changes {
        self.submit_batch(batch, Submission::Review)
    }

    fn submit_batch(&mut self, batch: Batch, submission: Submission) -> Changes {
        if !matches!(self.seam(), Seam::Idle) {
            return self.say("Finish the change in progress first.");
        }
        self.start(batch, submission)
    }

    fn start(&mut self, mut batch: Batch, submission: Submission) -> Changes {
        let Some(selection) = self.store.confirmed_selection().cloned() else {
            // A keystroke can outrun the host's echo of the selection it was typed at.
            self.seam = Some((Seam::Queued, batch, submission));
            return Changes {
                seam: true,
                ..Changes::default()
            };
        };
        if submission == Submission::Draft && selection.active != self.edit.as_ref().map(|e| e.at) {
            self.seam = None;
            let mut changes = self.refuse(Refusal::StaleSelection);
            changes.seam = true;
            return changes;
        }
        self.next_batch += 1;
        batch.id = self.next_batch.to_string();
        batch.selection_seq = Some(selection.seq);
        self.send(
            WorkbookRequest::Preview {
                batch: batch.clone(),
                depth: PreviewDepth::WritesOnly,
            },
            None,
            Pending::Preview,
        );
        self.seam = Some((Seam::Previewing, batch, submission));
        Changes {
            seam: true,
            ..Changes::default()
        }
    }

    /// Applies the change under review.
    pub fn apply_review(&mut self) -> Changes {
        match self.seam.take() {
            Some((Seam::Review(_), batch, submission)) => self.commit(batch, submission),
            other => {
                self.seam = other;
                Changes::default()
            }
        }
    }

    fn commit(&mut self, batch: Batch, submission: Submission) -> Changes {
        self.send(
            WorkbookRequest::Commit(batch.clone()),
            None,
            Pending::Commit { edit: submission == Submission::Draft },
        );
        self.seam = Some((Seam::Committing, batch, submission));
        Changes {
            seam: true,
            ..Changes::default()
        }
    }

    // Operations

    /// Runs a session operation. `notice` is what the Strip says when it succeeds
    /// ("Undid · Entered Sheet1!B2").
    pub fn run(&mut self, operation: Operation, notice: Option<String>) {
        self.send(
            WorkbookRequest::Run(operation),
            None,
            Pending::Run { notice },
        );
    }

    pub fn open(&mut self, name: String, bytes: Vec<u8>) -> Changes {
        if self.edit.is_some() || !matches!(self.seam(), Seam::Idle) {
            return self.say("Finish or cancel the cell edit before opening a workbook.");
        }
        self.send(WorkbookRequest::Open { name }, Some(bytes), Pending::Open);
        Changes::default()
    }

    // The carrier side

    pub fn deliver(&mut self, packet: Packet) -> Changes {
        let request = packet.envelope.request;
        match packet.envelope.message {
            Message::Frame(frame) => match self.tiles.finish(frame) {
                Ok((frame, transaction)) => {
                    if let Some(transaction) = transaction {
                        self.send(
                            WorkbookRequest::AcknowledgeTiles { transaction },
                            None,
                            Pending::Quiet,
                        );
                    }
                    self.frame(frame)
                }
                Err(reason) => {
                    self.send(WorkbookRequest::ResetTiles, None, Pending::Quiet);
                    self.refuse(reason)
                }
            },
            Message::Tile(stamp) => {
                let result = packet
                    .bytes
                    .ok_or_else(|| Refusal::Transport("Tile data is missing".into()))
                    .and_then(|bytes| self.tiles.fragment(stamp, &bytes));
                match result {
                    Ok(()) => Changes::default(),
                    Err(reason) => self.refuse(reason),
                }
            }
            Message::WorkbookReply(reply) => match self.pending.remove(&request) {
                Some(pending) => self.reply(pending, Ok(reply)),
                None => Changes::default(),
            },
            Message::Failure(reason) => match self.pending.remove(&request) {
                Some(pending) => self.reply(pending, Err(reason)),
                None => self.refuse(reason),
            },
            Message::WorkbookRequest(_) => {
                self.refuse(Refusal::Transport("Unexpected workbook request".into()))
            }
        }
    }

    fn frame(&mut self, frame: Frame) -> Changes {
        let before = (self.stamp(), self.selection().cloned());
        let touched: BTreeSet<WatchId> = frame
            .updates
            .iter()
            .map(|update| match update {
                calc_skin::Update::Ready { watch, .. }
                | calc_skin::Update::Tiles { watch, .. }
                | calc_skin::Update::Outdated { watch, .. }
                | calc_skin::Update::Refused { watch, .. } => *watch,
            })
            .collect();
        if !self.store.apply(frame) {
            return Changes::default();
        }
        let mut changes = Changes {
            stamp: before.0 != self.stamp(),
            selection: before.1.as_ref() != self.selection(),
            ..Changes::default()
        };
        let another_workbook = before
            .0
            .zip(self.stamp())
            .is_some_and(|(old, new)| old.workbook != new.workbook);
        if another_workbook {
            // Nothing drafted against the old workbook can be applied to the new one.
            changes.edit = self.edit.take().is_some() || !self.held.is_empty();
            self.held.clear();
            changes.seam = self.seam.take().is_some();
            changes.watches.extend(self.slots.keys().copied());
        } else {
            changes.watches.extend(
                self.slots
                    .iter()
                    .filter(|(_, slot)| slot.id.is_some_and(|id| touched.contains(&id)))
                    .map(|(key, _)| *key),
            );
        }
        if matches!(self.seam(), Seam::Queued) && self.store.confirmed_selection().is_some() {
            let (_, batch, submission) = self.seam.take().expect("a queued change");
            changes.merge(self.start(batch, submission));
        }
        changes
    }

    fn reply(&mut self, pending: Pending, result: Result<WorkbookReply, Refusal>) -> Changes {
        match (pending, result) {
            (Pending::Quiet, _) => Changes::default(),
            (Pending::Watch(key), Ok(WorkbookReply::Watching(id))) => {
                if self.orphans.remove(&key) {
                    self.send(WorkbookRequest::Unwatch(id), None, Pending::Quiet);
                } else if let Some(slot) = self.slots.get_mut(&key) {
                    slot.id = Some(id);
                    self.store.watch(id);
                }
                Changes::default()
            }
            (Pending::Watch(key), Err(reason)) => {
                self.orphans.remove(&key);
                self.refuse(reason)
            }
            (Pending::Select { client_seq }, Err(reason)) => {
                self.store.refuse_selection(client_seq);
                let mut changes = self.refuse(reason);
                changes.selection = true;
                changes
            }
            (Pending::Preview, result) => self.previewed(result),
            (Pending::Commit { edit }, Ok(WorkbookReply::Stamp(_))) => {
                self.seam = None;
                let mut changes = self.say("");
                changes.seam = true;
                changes.edit = edit && self.edit.take().is_some();
                changes.committed = true;
                changes
            }
            (Pending::Commit { .. }, Err(reason)) => {
                // The draft stays: the person corrects it or cancels.
                self.seam = None;
                let mut changes = self.refuse(reason);
                changes.seam = true;
                changes
            }
            (Pending::Run { notice }, Ok(WorkbookReply::Stamp(_))) => {
                self.say(notice.unwrap_or_default())
            }
            (Pending::Resolve, Ok(WorkbookReply::Answer(answer))) => {
                match query::Resolve::decode(answer) {
                    Ok(answer) => {
                        self.navigate(Target::Source(answer.body));
                        Changes::default()
                    }
                    Err(reason) => self.refuse(reason),
                }
            }
            (Pending::Open, Ok(WorkbookReply::Stamp(_))) => self.say(""),
            (Pending::Select { .. } | Pending::Navigate, Ok(_)) => Changes::default(),
            (_, Err(reason)) => self.refuse(reason),
            (_, Ok(_)) => self.refuse(Refusal::Transport("Unexpected workbook reply".into())),
        }
    }

    fn previewed(&mut self, result: Result<WorkbookReply, Refusal>) -> Changes {
        let Some((Seam::Previewing, batch, submission)) = self.seam.take() else {
            return Changes::default();
        };
        let refusal = match result {
            Ok(WorkbookReply::Preview(preview)) => match preview.refusal.clone() {
                Some(reason) => reason,
                None if submission != Submission::Review && preview.warnings.is_empty() => {
                    return self.commit(batch, submission);
                }
                None => {
                    self.seam = Some((Seam::Review(preview), batch, submission));
                    return Changes {
                        seam: true,
                        ..Changes::default()
                    };
                }
            },
            Ok(_) => Refusal::Transport("Expected a preview reply".into()),
            Err(reason) => reason,
        };
        let mut changes = self.refuse(refusal);
        changes.seam = true;
        changes
    }
}

pub fn cell_source(at: Key) -> SourceRef {
    SourceRef::Cells {
        sheet: Sheet(at.0, at.1),
        areas: vec![Area(at, at)],
    }
}

/// A refusal in the words the Strip shows.
pub fn refusal_text(reason: Refusal) -> String {
    match reason {
        Refusal::Excel { reason }
        | Refusal::Invalid { reason }
        | Refusal::Validation { reason, .. }
        | Refusal::Transport(reason) => reason,
        Refusal::Conflict { .. } => {
            "The workbook changed. Check the cell and start the edit again.".into()
        }
        Refusal::StaleSelection => "The selection changed during this edit.".into(),
        Refusal::ReadOnly => "This workbook is read-only.".into(),
        Refusal::Protected(_) => "The worksheet is protected.".into(),
        Refusal::NotModelled { what, owner } => format!("{what} is unavailable ({owner})"),
        Refusal::OverBudget { .. } => "Choose a smaller worksheet area.".into(),
        reason => format!("{reason:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use calc_skin::query::{CalcActivity, CalcMode, CalcState, CalcStatus};
    use calc_skin::{CalcGeneration, Coverage, Event, SelectionCause, Update};

    const A1: Key = Key(0, 0, 1, 1);
    const B2: Key = Key(0, 0, 2, 2);

    fn stamp(workbook: u64, model: u64) -> Stamp {
        Stamp {
            workbook: WorkbookId(workbook),
            model: Revision(model),
            view: Revision(0),
            calc: CalcGeneration(0),
        }
    }

    fn selection(seq: u64, at: Key) -> Selection {
        Selection {
            seq,
            target: cell_source(at),
            active: Some(at),
            grouped: Vec::new(),
            item: None,
        }
    }

    fn frame(stamp: Stamp, events: Vec<Event>, updates: Vec<Update>) -> Packet {
        Packet {
            envelope: Envelope::new(
                0,
                Message::Frame(Frame {
                    stamp,
                    events,
                    updates,
                }),
            ),
            bytes: None,
        }
    }

    fn reply(request: u64, reply: WorkbookReply) -> Packet {
        Packet {
            envelope: Envelope::new(request, Message::WorkbookReply(reply)),
            bytes: None,
        }
    }

    fn failure(request: u64, reason: Refusal) -> Packet {
        Packet {
            envelope: Envelope::new(request, Message::Failure(reason)),
            bytes: None,
        }
    }

    /// The requests the client queued, with their request numbers.
    fn sent(client: &mut Client) -> Vec<(u64, WorkbookRequest)> {
        client
            .take_outbox()
            .into_iter()
            .map(|packet| match packet.envelope.message {
                Message::WorkbookRequest(request) => (packet.envelope.request, request),
                other => panic!("the client sent {other:?}"),
            })
            .collect()
    }

    /// A client on workbook 1 with the host's selection at A1.
    fn opened() -> Client {
        let mut client = Client::default();
        client.deliver(frame(
            stamp(1, 0),
            vec![Event::Selection {
                selection: selection(1, A1),
                cause: SelectionCause::Open,
            }],
            Vec::new(),
        ));
        client
    }

    fn preview(warnings: &[&str], refusal: Option<Refusal>) -> WorkbookReply {
        WorkbookReply::Preview(Preview {
            base: stamp(1, 0),
            writes: Vec::new(),
            presentation: Vec::new(),
            warnings: warnings.iter().map(|w| (*w).to_owned()).collect(),
            refusal,
        })
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_watch_reads_its_typed_answer_and_reports_only_its_own_changes() {
        let mut client = opened();
        let calc = client.watch(CalcState);
        let other = client.watch(query::History);
        let requests = sent(&mut client);
        assert!(matches!(
            requests[0].1,
            WorkbookRequest::Watch {
                query: calc_skin::WorkbookQuery::CalcState,
                ..
            }
        ));
        assert_eq!(client.read(calc), Live::Loading);
        client.deliver(reply(requests[0].0, WorkbookReply::Watching(WatchId(7))));
        client.deliver(reply(requests[1].0, WorkbookReply::Watching(WatchId(8))));
        let status = CalcStatus {
            mode: CalcMode::Automatic,
            activity: CalcActivity::Pending,
            circular: None,
        };
        let changes = client.deliver(frame(
            stamp(1, 0),
            Vec::new(),
            vec![Update::Ready {
                watch: WatchId(7),
                coverage: Coverage::Complete,
                body: CalcState::encode(&status).unwrap(),
            }],
        ));
        assert_eq!(changes.watches, BTreeSet::from([calc.key()]));
        assert!(!changes.selection && !changes.stamp);
        assert!(matches!(client.read(calc), Live::Ready(answer) if answer.body == status));
        assert_eq!(client.read(other), Live::Loading);

        // A body of the wrong shape is a refusal, never a default value.
        client.deliver(frame(
            stamp(1, 0),
            Vec::new(),
            vec![Update::Ready {
                watch: WatchId(8),
                coverage: Coverage::Complete,
                body: serde_json::json!({"mode": "Manual"}),
            }],
        ));
        assert!(matches!(client.read(other), Live::Refused(Refusal::Transport(_))));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_watch_dropped_before_the_host_names_it_is_unwatched_when_it_does() {
        let mut client = opened();
        let early = client.watch(CalcState);
        let request = sent(&mut client)[0].0;
        client.unwatch(early);
        assert!(sent(&mut client).is_empty());
        client.deliver(reply(request, WorkbookReply::Watching(WatchId(3))));
        assert_eq!(
            sent(&mut client)
                .into_iter()
                .map(|(_, r)| r)
                .collect::<Vec<_>>(),
            [WorkbookRequest::Unwatch(WatchId(3))]
        );
        assert_eq!(client.read(early), Live::Loading);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_clean_edit_previews_then_commits_and_keeps_the_draft_when_refused() {
        let mut client = opened();
        let changes = client.begin_edit(A1, Some("=21*2".into()), "old");
        assert!(changes.edit && !changes.selection);
        assert_eq!(client.mode(), InputMode::Enter);
        client.edit_text("=21*2+0".into());
        client.commit_edit();
        assert_eq!(client.seam(), &Seam::Previewing);
        let (request, preview_request) = sent(&mut client).pop().unwrap();
        let WorkbookRequest::Preview { batch, depth } = preview_request else {
            panic!("expected a preview")
        };
        assert_eq!(depth, PreviewDepth::WritesOnly);
        assert_eq!(batch.selection_seq, Some(1));
        assert_eq!(batch.base, Some(Revision(0)));
        assert_eq!(batch.expects, [cell_source(A1)]);
        // A second Enter while the host works changes nothing.
        assert!(client.commit_edit().is_empty());
        assert!(client.cancel().is_empty());

        client.deliver(reply(request, preview(&[], None)));
        assert_eq!(client.seam(), &Seam::Committing);
        let (request, commit) = sent(&mut client).pop().unwrap();
        assert_eq!(commit, WorkbookRequest::Commit(batch));
        let changes = client.deliver(failure(request, Refusal::ReadOnly));
        assert!(changes.seam && changes.notice && !changes.edit);
        assert_eq!(client.notice(), "This workbook is read-only.");
        assert_eq!(client.edit().unwrap().text, "=21*2+0");

        client.commit_edit();
        let request = sent(&mut client).pop().unwrap().0;
        client.deliver(reply(request, preview(&[], None)));
        let request = sent(&mut client).pop().unwrap().0;
        let changes = client.deliver(reply(request, WorkbookReply::Stamp(stamp(1, 1))));
        assert!(changes.edit && changes.seam);
        assert!(client.edit().is_none());
        assert_eq!((client.seam(), client.notice()), (&Seam::Idle, ""));
        assert_eq!(client.mode(), InputMode::Ready);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_preview_with_warnings_waits_for_the_person() {
        let mut client = opened();
        client.begin_edit(A1, None, "=A2");
        assert_eq!(client.mode(), InputMode::Edit);
        assert_eq!(client.edit().unwrap().text, "=A2");
        client.commit_edit();
        let request = sent(&mut client).pop().unwrap().0;
        client.deliver(reply(request, preview(&["Closes a cycle"], None)));
        assert!(matches!(client.seam(), Seam::Review(p) if p.warnings == ["Closes a cycle"]));
        assert!(sent(&mut client).is_empty(), "nothing commits unasked");
        client.apply_review();
        assert!(matches!(
            sent(&mut client).pop().unwrap().1,
            WorkbookRequest::Commit(_)
        ));

        // A refusal inside the preview ends the attempt and keeps the draft.
        let mut client = opened();
        client.begin_edit(A1, Some("x".into()), "");
        client.commit_edit();
        let request = sent(&mut client).pop().unwrap().0;
        client.deliver(reply(request, preview(&[], Some(Refusal::ReadOnly))));
        assert_eq!(client.seam(), &Seam::Idle);
        assert!(client.edit().is_some());
        // Esc drops it.
        assert!(client.cancel().edit);
        assert!(client.edit().is_none());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn enter_typed_ahead_of_the_selection_echo_waits_for_it_and_lands_on_that_cell() {
        let mut client = opened();
        client.select_cell(B2);
        let (select, _) = sent(&mut client).pop().unwrap();
        client.begin_edit(B2, Some("7".into()), "");
        client.commit_edit();
        assert_eq!(client.seam(), &Seam::Queued);
        assert!(sent(&mut client).is_empty(), "no preview before the echo");
        client.deliver(reply(select, WorkbookReply::Accepted));
        let changes = client.deliver(frame(
            stamp(1, 0),
            vec![Event::Selection {
                selection: selection(2, B2),
                cause: SelectionCause::ClientEcho { client_seq: 1 },
            }],
            Vec::new(),
        ));
        assert!(changes.seam);
        let (_, request) = sent(&mut client).pop().unwrap();
        assert!(matches!(
            request,
            WorkbookRequest::Preview { batch, .. } if batch.selection_seq == Some(2)
        ));

        // A macro moves the selection while the draft waits: the edit refuses, it does not land
        // on the macro's cell.
        let mut client = opened();
        client.select_cell(B2);
        sent(&mut client);
        client.begin_edit(B2, Some("7".into()), "");
        client.commit_edit();
        client.deliver(frame(
            stamp(1, 0),
            vec![Event::Selection {
                selection: selection(5, A1),
                cause: SelectionCause::Macro {
                    procedure: "Jump".into(),
                },
            }],
            Vec::new(),
        ));
        assert_eq!(client.seam(), &Seam::Idle);
        assert_eq!(client.notice(), "The selection changed during this edit.");
        assert!(sent(&mut client).is_empty());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn another_workbook_drops_the_draft_and_marks_every_watch() {
        let mut client = opened();
        let calc = client.watch(CalcState);
        let request = sent(&mut client)[0].0;
        client.deliver(reply(request, WorkbookReply::Watching(WatchId(1))));
        client.begin_edit(A1, Some("x".into()), "");
        assert!(client.open("b.xlsx".into(), vec![1]).notice);
        assert!(sent(&mut client).is_empty(), "an open never discards a draft");
        let changes = client.deliver(frame(stamp(2, 0), Vec::new(), Vec::new()));
        assert!(changes.edit && changes.stamp);
        assert_eq!(changes.watches, BTreeSet::from([calc.key()]));
        assert!(client.edit().is_none());
        assert_eq!(client.read(calc), Live::Loading);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn an_address_resolves_then_navigates_and_an_undo_reports_what_it_undid() {
        let mut client = opened();
        client.go_to("B2".into());
        let (request, _) = sent(&mut client).pop().unwrap();
        client.deliver(reply(
            request,
            WorkbookReply::Answer(Answer {
                stamp: stamp(1, 0),
                coverage: Coverage::Complete,
                body: serde_json::to_value(cell_source(B2)).unwrap(),
            }),
        ));
        assert_eq!(
            sent(&mut client).pop().unwrap().1,
            WorkbookRequest::Navigate(Target::Source(cell_source(B2)))
        );
        client.run(Operation::Undo, Some("Undid · Entered Sheet1!B2".into()));
        let (request, _) = sent(&mut client).pop().unwrap();
        assert!(client.deliver(reply(request, WorkbookReply::Stamp(stamp(1, 2)))).notice);
        assert_eq!(client.notice(), "Undid · Entered Sheet1!B2");
        client.run(Operation::Redo, None);
        let (request, _) = sent(&mut client).pop().unwrap();
        client.deliver(failure(
            request,
            Refusal::Invalid {
                reason: "Nothing to redo".into(),
            },
        ));
        assert_eq!(client.notice(), "Nothing to redo");
    }
}
