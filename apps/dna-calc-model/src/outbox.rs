//! Serial application command submission. The model retains operations and their origin;
//! adapters retain callbacks and deliver only the envelope returned here.
use crate::{
    Model,
    projection::{Area, CommandEnvelope, Key, Request},
};
use std::collections::VecDeque;

struct Submission {
    focus: crate::FocusTicket,
    id: u64,
    generation: u64,
    source: String,
    request: Request,
    bytes: Option<Vec<u8>>,
}
#[derive(Default)]
pub struct CommandQueue {
    pending: VecDeque<Submission>,
    active: Option<u64>,
}
pub enum CommandDelivery {
    Ready {
        focus: crate::FocusTicket,
        envelope: CommandEnvelope,
        bytes: Option<Vec<u8>>,
    },
    Refused {
        id: u64,
        focus: crate::FocusTicket,
        reason: String,
    },
}
impl CommandDelivery {
    pub fn focus(&self) -> crate::FocusTicket {
        match self {
            Self::Ready { focus, .. } | Self::Refused { focus, .. } => *focus,
        }
    }
    pub fn id(&self) -> u64 {
        match self {
            Self::Ready { envelope, .. } => envelope.id,
            Self::Refused { id, .. } => *id,
        }
    }
}
impl Model {
    pub fn submit_command(
        &mut self,
        request: Request,
        bytes: Option<Vec<u8>>,
    ) -> Result<u64, String> {
        let focus = self
            .runtime
            .callback_focus
            .unwrap_or_else(|| self.focus.snapshot());
        self.submit_command_with_focus(request, bytes, focus)
    }
    pub fn submit_command_with_focus(
        &mut self,
        request: Request,
        bytes: Option<Vec<u8>>,
        focus: crate::FocusTicket,
    ) -> Result<u64, String> {
        if !request.is_command() {
            return Err("This operation is a query, not a workbook command".into());
        }
        if self.commands.pending.len() + usize::from(self.commands.active.is_some())
            >= crate::tiles::MAX_PENDING_REQUESTS
        {
            return Err(
                "The workbook command queue is full; wait for the current operation".into(),
            );
        }
        let id = self.issue_command();
        self.commands.pending.push_back(Submission {
            focus,
            id,
            generation: self.runtime.generation,
            source: self.active_skin.clone().unwrap_or_else(|| "shell".into()),
            request,
            bytes,
        });
        Ok(id)
    }
    /// Base revision and selection are resolved after the preceding receipt is admitted by
    /// the adapter. An operation from a replaced workbook cannot acquire the new identity.
    pub fn take_command(&mut self) -> Option<CommandDelivery> {
        if self.commands.active.is_some() {
            return None;
        }
        let next = self.commands.pending.pop_front()?;
        self.commands.active = Some(next.id);
        if next.generation != self.runtime.generation {
            return Some(CommandDelivery::Refused {
                id: next.id,
                focus: next.focus,
                reason: "This command belongs to a workbook that has been replaced".into(),
            });
        }
        Some(CommandDelivery::Ready {
            focus: next.focus,
            envelope: CommandEnvelope {
                id: next.id,
                session: self.runtime.summary.session.clone(),
                workbook: self.runtime.summary.workbook,
                base_revision: self.runtime.summary.revision,
                source: next.source,
                undo_group: None,
                resolved_selection: self.confirmed_selection.as_ref().map(|(at, areas)| {
                    (Key::of(*at), areas.iter().copied().map(Area::of).collect())
                }),
                request: Box::new(next.request),
            },
            bytes: next.bytes,
        })
    }
    pub fn finish_command(&mut self, id: u64) {
        assert_eq!(
            self.commands.active,
            Some(id),
            "only the active command may release submission"
        );
        self.commands.active = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_stalled_command_bounds_later_submissions_without_losing_accepted_work() {
        let mut model = Model::default();
        let mut accepted = Vec::new();
        for _ in 0..crate::tiles::MAX_PENDING_REQUESTS {
            accepted.push(
                model
                    .submit_command(Request::Intent(crate::projection::Intent::Undo), None)
                    .unwrap(),
            );
        }
        let first = model.take_command().unwrap();
        assert_eq!(first.id(), accepted[0]);
        assert!(
            model
                .submit_command(Request::Intent(crate::projection::Intent::Redo), None)
                .unwrap_err()
                .contains("queue is full")
        );
        model.finish_command(first.id());
        let next = model
            .submit_command(Request::Intent(crate::projection::Intent::Redo), None)
            .unwrap();
        accepted.push(next);
        for &expected in &accepted[1..] {
            let command = model.take_command().unwrap();
            assert_eq!(command.id(), expected);
            model.finish_command(command.id());
        }
        assert!(model.take_command().is_none());
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn queued_commands_resolve_after_prior_receipt_and_keep_their_workbook() {
        let mut model = Model::default();
        model.runtime.summary = crate::projection::Summary {
            session: "s".into(),
            workbook: 3,
            revision: 7,
            ..Default::default()
        }
        .into();
        let first = model
            .submit_command(Request::Intent(crate::projection::Intent::Undo), None)
            .unwrap();
        let second = model
            .submit_command(Request::Intent(crate::projection::Intent::Redo), None)
            .unwrap();
        let CommandDelivery::Ready { envelope, .. } = model.take_command().unwrap() else {
            panic!("first command refused")
        };
        assert_eq!((envelope.id, envelope.base_revision), (first, 7));
        assert!(model.take_command().is_none());
        model.runtime.summary.revision = 8;
        model.finish_command(first);
        let CommandDelivery::Ready { envelope, .. } = model.take_command().unwrap() else {
            panic!("second command refused")
        };
        assert_eq!((envelope.id, envelope.base_revision), (second, 8));
        model.finish_command(second);
        let old = model
            .submit_command(Request::Intent(crate::projection::Intent::Undo), None)
            .unwrap();
        model.runtime.generation += 1;
        assert!(matches!(model.take_command(),Some(CommandDelivery::Refused {id,..}) if id==old));
        model.finish_command(old);
        assert!(model.take_command().is_none());
    }
}
