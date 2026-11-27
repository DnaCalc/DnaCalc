//! Ordered workbook frames and the optimistic cursor connection.

use crate::Live;
use calc_skin::{Answer, Event, Frame, Selection, SelectionCause, Stamp, Update, WatchId};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default)]
pub struct FrameStore {
    stamp: Option<Stamp>,
    selection: Option<Selection>,
    shown_selection: Option<Selection>,
    pending: Option<u64>,
    next_client_seq: u64,
    watches: BTreeMap<WatchId, Live<Answer<Value>>>,
}

impl FrameStore {
    pub fn stamp(&self) -> Option<Stamp> {
        self.stamp
    }

    pub fn selection(&self) -> Option<&Selection> {
        self.shown_selection.as_ref()
    }

    pub fn confirmed_selection(&self) -> Option<&Selection> {
        if self.pending.is_some() {
            None
        } else {
            self.selection.as_ref()
        }
    }

    pub fn select(&mut self, selection: Selection) -> u64 {
        self.next_client_seq += 1;
        self.pending = Some(self.next_client_seq);
        self.shown_selection = Some(selection);
        self.next_client_seq
    }

    pub fn refuse_selection(&mut self, client_seq: u64) {
        if self.pending == Some(client_seq) {
            self.pending = None;
            self.shown_selection.clone_from(&self.selection);
        }
    }

    pub fn watch(&mut self, watch: WatchId) {
        self.watches.insert(watch, Live::Loading);
    }

    pub fn unwatch(&mut self, watch: WatchId) {
        self.watches.remove(&watch);
    }

    pub fn live(&self, watch: WatchId) -> Option<&Live<Answer<Value>>> {
        self.watches.get(&watch)
    }

    pub fn apply(&mut self, frame: Frame) -> bool {
        if let Some(stamp) = self.stamp {
            if frame.stamp.workbook == stamp.workbook
                && (frame.stamp.model < stamp.model
                    || frame.stamp.view < stamp.view
                    || frame.stamp.calc < stamp.calc)
            {
                return false;
            }
            if frame.stamp.workbook != stamp.workbook {
                self.selection = None;
                self.shown_selection = None;
                self.pending = None;
                self.watches
                    .values_mut()
                    .for_each(|watch| *watch = Live::Loading);
            }
        }
        for update in frame.updates {
            let (watch, live) = match update {
                Update::Ready {
                    watch,
                    coverage,
                    body,
                } => (
                    watch,
                    Live::Ready(Answer {
                        stamp: frame.stamp,
                        coverage,
                        body,
                    }),
                ),
                Update::Tiles { watch, .. } => (
                    watch,
                    Live::Refused(calc_skin::Refusal::Transport(
                        "Missing tile fragments".into(),
                    )),
                ),
                Update::Outdated { watch, .. } => (watch, Live::Outdated),
                Update::Refused { watch, reason } => (watch, Live::Refused(reason)),
            };
            if let Some(old) = self.watches.get_mut(&watch) {
                *old = live;
            }
        }
        for event in frame.events {
            if let Event::Selection { selection, cause } = event {
                if self
                    .selection
                    .as_ref()
                    .is_some_and(|confirmed| confirmed.seq >= selection.seq)
                {
                    continue;
                }
                self.selection = Some(selection.clone());
                match cause {
                    SelectionCause::ClientEcho { client_seq }
                        if self.pending != Some(client_seq) =>
                    {
                        if self.pending.is_none() {
                            self.shown_selection = Some(selection);
                        }
                    }
                    _ => {
                        self.pending = None;
                        self.shown_selection = Some(selection);
                    }
                }
            }
        }
        self.stamp = Some(frame.stamp);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use calc_skin::{CalcGeneration, Revision, SourceRef, WorkbookId, grid::Key};

    fn selection(seq: u64, column: u32) -> Selection {
        Selection {
            seq,
            target: SourceRef::Workbook,
            active: Some(Key(0, 0, 1, column)),
            grouped: Vec::new(),
            item: None,
        }
    }

    fn frame(selection: Selection, cause: SelectionCause) -> Frame {
        Frame {
            stamp: Stamp {
                workbook: WorkbookId(1),
                model: Revision(0),
                view: Revision(0),
                calc: CalcGeneration(0),
            },
            events: vec![Event::Selection { selection, cause }],
            updates: Vec::new(),
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn macro_selection_supersedes_outstanding_mouse_moves_and_old_echoes() {
        let mut state = FrameStore::default();
        state.apply(frame(selection(1, 1), SelectionCause::Open));
        let first = state.select(selection(1, 2));
        let later = state.select(selection(1, 3));
        state.apply(frame(
            selection(2, 2),
            SelectionCause::ClientEcho { client_seq: first },
        ));
        assert_eq!(state.selection().unwrap().active.unwrap().3, 3);
        assert!(state.confirmed_selection().is_none());
        state.apply(frame(
            selection(4, 4),
            SelectionCause::Macro {
                procedure: "SetFocus".into(),
            },
        ));
        state.apply(frame(
            selection(3, 3),
            SelectionCause::ClientEcho { client_seq: later },
        ));
        assert_eq!(state.confirmed_selection().unwrap().active.unwrap().3, 4);
    }
}
