//! The view contract: what a workbook view is to the shell, and how it registers.
//!
//! A view is a crate that implements [`View`] and registers it with `install(&mut Registry)`. It
//! is given a [`Session`] and nothing else: no shell type, no carrier, no other view. What it
//! reads comes through the session's typed watches, what it changes goes through the session's
//! one seam, and what a person can ask it to do is a command.

use crate::session::Session;
use calc_skin::query::Census;
use calc_skin::{Selection, Target, ViewId};
use dna_skin_runtime::commands::{CommandId, CommandSpec, Invocation, When};
use leptos::prelude::*;
use std::rc::Rc;

/// What the shell shows about a view before it mounts it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ViewSpec {
    pub id: ViewId,
    /// The name in the switcher and in "Undid … (Model map)".
    pub title: &'static str,
    /// The keymap profile the view's keys follow (`excel-365-win-en-us`, `jupyter`).
    pub profile: &'static str,
}

/// Whether a view can show a workbook, decided from counts before anything is mounted.
#[derive(Clone, Debug, PartialEq)]
pub enum Fit {
    Whole,
    Simplified {
        shown: u64,
        total: u64,
        explanation: String,
    },
    /// The view will not mount. The shell shows the reason and the ways forward, each a command.
    Declined {
        reason: String,
        alternatives: Vec<WayForward>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct WayForward {
    pub label: String,
    pub command: Invocation,
}

/// Where the one selection is in a view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Located {
    /// The view shows exactly the selected thing.
    Exact,
    /// The view shows something coarser that contains it; the Strip says so with this label.
    Within { label: String },
    /// The selection is not in this view. The shell offers "Reveal in Classic".
    Absent,
}

/// A command a view contributes, with the keys its profile gives it.
///
/// A view's commands are registered once, when the shell starts, and each can run only while its
/// view is on stage. A handler works on the [`Session`] (the selection, the draft, the seam) and
/// never on the mounted component, so a key, a button, a skin's sequence and an agent's request
/// do the same thing whether or not the view has drawn yet.
pub struct ViewCommand {
    pub spec: CommandSpec,
    pub chords: Vec<(When, &'static str)>,
    pub run: Rc<dyn Fn(&Invocation) -> Result<(), String>>,
}

impl ViewCommand {
    /// A command that can run only in `view`, with keys bound under the same condition.
    pub fn new(
        view: &ViewId,
        id: &'static str,
        label: &'static str,
        chords: &[&'static str],
        run: impl Fn(&Invocation) -> Result<(), String> + 'static,
    ) -> Self {
        Self::under(When::view(&view.0), id, label, chords, run)
    }

    /// A command available from any view, for an instrument that can be docked anywhere.
    pub fn global(
        id: &'static str,
        label: &'static str,
        chords: &[&'static str],
        run: impl Fn(&Invocation) -> Result<(), String> + 'static,
    ) -> Self {
        Self::under(When::ALWAYS, id, label, chords, run)
    }

    fn under(
        here: When,
        id: &'static str,
        label: &'static str,
        chords: &[&'static str],
        run: impl Fn(&Invocation) -> Result<(), String> + 'static,
    ) -> Self {
        Self {
            spec: CommandSpec {
                id: CommandId::new(id),
                label: label.into(),
                enabled: here.clone(),
            },
            chords: chords.iter().map(|chord| (here.clone(), *chord)).collect(),
            run: Rc::new(run),
        }
    }

    /// Adds a condition to the command and its keys: `mode:ready` for grid motion, `writable`
    /// for a command that edits.
    pub fn when(mut self, when: &str) -> Self {
        let extra = When::parse(when).expect("a built-in condition parses");
        self.spec.enabled = self.spec.enabled.and(extra.clone());
        for (condition, _) in &mut self.chords {
            *condition = condition.clone().and(extra.clone());
        }
        self
    }
}

/// What a view hands back when it mounts.
pub struct MountedView {
    pub body: AnyView,
    /// Where a selection is in this view. The shell asks before it says "Reveal in Classic".
    pub locate: Box<dyn Fn(&Selection) -> Located>,
    /// Brings a target into sight without changing the selection.
    pub reveal: Box<dyn Fn(&Target)>,
}

impl MountedView {
    /// A view that shows every cell where it is, as a sheet grid does.
    pub fn exact(body: impl IntoAny) -> Self {
        Self {
            body: body.into_any(),
            locate: Box::new(|_| Located::Exact),
            reveal: Box::new(|_| ()),
        }
    }
}

/// The shell's shared panel positions. A view decides which instrument to mount in a slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DockSide {
    Right,
    Bottom,
}

impl DockSide {
    pub fn label(self) -> &'static str {
        match self {
            Self::Right => "right",
            Self::Bottom => "bottom",
        }
    }
}

pub struct DockSpec {
    pub command: &'static str,
    pub label: &'static str,
    pub mount: fn(Session, DockSide) -> AnyView,
}

pub trait View: 'static {
    fn spec(&self) -> ViewSpec;

    /// Checked from the workbook's counts before `mount`. The default takes any workbook.
    fn fit(&self, _census: &Census) -> Fit {
        Fit::Whole
    }

    /// The view's commands and their keys. Called once per session, before any mount.
    fn commands(&self, _session: Session) -> Vec<ViewCommand> {
        Vec::new()
    }

    fn dock(&self) -> Option<DockSpec> {
        None
    }

    /// Cancels an open view-local interaction when the shell has no edit or review to cancel.
    fn cancel_interaction(&self, _session: Session) -> bool {
        false
    }

    fn mount(&self, session: Session) -> MountedView;
}

impl crate::Registry {
    /// Registers a workbook view. A second view with the same identity is a programming error.
    pub fn view(&mut self, view: impl View) {
        let id = view.spec().id;
        assert!(
            self.views.iter().all(|known| known.spec().id != id),
            "view {} registered twice",
            id.0
        );
        self.views.push(Rc::new(view));
    }

    /// The registered views in registration order: the order of the switcher.
    pub fn views(&self) -> Vec<ViewSpec> {
        self.views.iter().map(|view| view.spec()).collect()
    }

    pub fn find(&self, id: &ViewId) -> Option<Rc<dyn View>> {
        self.views
            .iter()
            .find(|view| view.spec().id == *id)
            .cloned()
    }

    /// Registers every view's commands and keys with the session. The shell calls this once.
    pub fn install_commands(&self, session: Session) {
        for view in &self.views {
            session.install(view.commands(session));
        }
    }
}
