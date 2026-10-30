//! Driving the engine's recalculation of the current revision. A recalculation is not part of an
//! edit's transaction (plan 3.3): a committed edit stays whatever the recalculation does.

use crate::state::HostState;
use calc_doc::CalcMode;
use calc_engine::{
    ReadObserver, RecalcFailure, RecalcProgress, StartError, ValueDelta, WorkBudget,
};
use std::fmt;

/// Why a recalculation published nothing. The committed edits stay, the published generation is
/// unchanged, and [`crate::project`] labels every calculated result or refusal the recalculation
/// would have replaced as stale ([`crate::StaleCause::RecalcFailed`] until an edit commits, then
/// [`crate::StaleCause::AwaitingRecalc`]) until one publishes. A failure does not make a file
/// cache stale: one stays a file cache while no edit since the workbook was opened has changed
/// an input or a formula.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecalcFailed {
    /// The engine refused to begin it ([`calc_engine::begin_recalc`]), such as for a circular
    /// reference among the formulas it would calculate.
    Refused(StartError),
    /// It began and failed before publishing ([`calc_engine::step`] and
    /// [`calc_engine::finish`]), such as when the caller's read observer stopped it.
    Failed(RecalcFailure),
}

impl fmt::Display for RecalcFailed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RecalcFailed::Refused(error) => write!(f, "the recalculation was refused: {error}"),
            RecalcFailed::Failed(failure) => write!(f, "the recalculation failed: {failure}"),
        }
    }
}

impl std::error::Error for RecalcFailed {}

/// Where the workbook's calculation stands after a [`step`].
#[derive(Clone, Debug, PartialEq)]
pub enum HostProgress {
    /// The published generation calculated the current revision and no formula is pending:
    /// nothing was done.
    Current,
    /// The workbook calculates manually ([`crate::CalcMode::Manual`]) and no recalculation was
    /// asked for ([`calculate`]): nothing was done, and the formulas an edit reached project as
    /// stale.
    Manual,
    /// A recalculation of the current revision is in progress with `remaining` formulas to
    /// evaluate; the next step continues it.
    Calculating { remaining: usize },
    /// A recalculation published a generation: what changed.
    Published(ValueDelta),
    /// A recalculation published nothing. The next step begins another.
    Failed(RecalcFailed),
}

/// Recalculates the current revision, evaluating at most `budget` formulas in this call, under
/// the host's numeric mode, with `observer` shown every run each evaluation reads.
///
/// A step with no recalculation in progress begins one ([`calc_engine::begin_recalc`]) unless the
/// published generation is of the current revision and no formula is pending. It then evaluates
/// ([`calc_engine::step`]) and, once nothing remains or the recalculation failed, publishes or
/// collects the failure ([`calc_engine::finish`]). A product caller that observes nothing passes
/// [`calc_engine::Unobserved`]; an observer can only watch reads or stop the recalculation, which
/// then fails ([`calc_engine::RecalcFailure::ReadFailed`]).
pub fn step(
    host: &mut HostState,
    budget: WorkBudget,
    observer: &mut dyn ReadObserver,
) -> HostProgress {
    let mut work = match host.work.take() {
        Some(work) => work,
        None => {
            let current = host.model.revision();
            let requested = std::mem::take(&mut host.calculate_requested);
            // L3 (dc-m3u): automatic except tables leaves data tables until a recalculation is
            // asked for.
            let deferred = host.calc_mode() == CalcMode::AutomaticExceptTables && !requested;
            host.engine.set_tables_deferred(deferred);
            if !requested {
                if host.calc_mode() == CalcMode::Manual {
                    return HostProgress::Manual;
                }
                if host.published == Some(current) && !host.engine.needs_recalc(&host.model) {
                    return HostProgress::Current;
                }
            }
            // dc-uar: the facts CELL and INFO read, as of this revision.
            let facts = host.workbook_facts();
            host.engine.set_workbook_facts(Some(facts));
            match calc_engine::begin_recalc(
                &mut host.engine,
                &host.model,
                host.options.numeric.clone(),
            ) {
                Ok(work) => work,
                Err(error) => return failed(host, RecalcFailed::Refused(error)),
            }
        }
    };
    match calc_engine::step(&host.engine, &host.model, &mut work, budget, observer) {
        RecalcProgress::Pending { remaining } => {
            host.work = Some(work);
            HostProgress::Calculating { remaining }
        }
        RecalcProgress::Complete | RecalcProgress::Failed => {
            match calc_engine::finish(&mut host.engine, work) {
                Ok(delta) => {
                    host.published = Some(delta.revision());
                    host.last_failure = None;
                    HostProgress::Published(delta)
                }
                Err(failure) => failed(host, RecalcFailed::Failed(failure)),
            }
        }
    }
}

/// Asks for a recalculation, as Excel's Calculate Now (F9) does: the next [`step`] begins one
/// whatever the calculation mode, and even when nothing is pending, so that it calculates the
/// pending formulas, the formulas that call a volatile function and every formula that reads
/// them. A recalculation already in progress continues; the one asked for begins after it.
pub fn calculate(host: &mut HostState) {
    host.calculate_requested = true;
}

/// Asks for a full recalculation, as Excel's `CalculateFull` (Ctrl+Alt+F9) does: every formula
/// is made pending and the next [`step`] begins a recalculation that calculates every formula,
/// whatever the calculation mode. The compiled plans and the dependency graph are kept, so
/// nothing is compiled again. A recalculation in progress is cancelled, since it would clear the
/// nodes it calculates from pending when it published; the full one replaces it.
pub fn calculate_full(host: &mut HostState) {
    host.work = None;
    host.engine.mark_all_pending();
    host.calculate_requested = true;
}

fn failed(host: &mut HostState, failure: RecalcFailed) -> HostProgress {
    // A recalculation in progress is always of the current revision: a commit cancels it.
    host.last_failure = Some((host.model.revision(), failure.clone()));
    HostProgress::Failed(failure)
}
