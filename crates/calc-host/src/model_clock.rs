//! An in-process RTD clock driven by elapsed time supplied by the workbook owner.

use crate::{
    FormulaResult, HostState, RtdConnection, RtdConnector, RtdLease, TopicKey,
    value::{CalcValue, CellKey},
};
use calc_doc_model::CellInput;
use calc_fml::reference::column_letters;
use std::{cell::RefCell, collections::BTreeMap, rc::Rc, time::Duration};

const PROG_ID: &str = "dnacalc.clock";
const MAX_TICK: u64 = 1 << 53;

#[derive(Debug)]
struct Controls {
    period: Duration,
    running: bool,
    step: u64,
    reset: u64,
}

impl Controls {
    fn read(args: &[String]) -> Option<Self> {
        let [model, kind, tail @ ..] = args else {
            return None;
        };
        if model.is_empty() || kind != "tick" || tail.len() > 4 {
            return None;
        }
        let arg = |index| {
            tail.get(index)
                .map(String::as_str)
                .filter(|text| !text.is_empty())
        };
        let rate = arg(0).unwrap_or("1").parse::<f64>().ok()?;
        let period = Duration::try_from_secs_f64(rate.recip()).ok()?;
        if period.is_zero() {
            return None;
        }
        let running = match arg(1).unwrap_or("TRUE") {
            "TRUE" => true,
            "FALSE" => false,
            _ => return None,
        };
        let step = arg(2).unwrap_or("0").parse().ok()?;
        let reset = arg(3).unwrap_or("0").parse().ok()?;
        Some(Self {
            period,
            running,
            step,
            reset,
        })
    }
}

#[derive(Debug)]
struct Model {
    caller: CellKey,
    topic: TopicKey,
    controls: Controls,
    lease: RtdLease,
    sequence: u64,
    tick: u64,
    elapsed: Duration,
}

impl Model {
    fn value(&self) -> CalcValue {
        CalcValue::number(self.tick as f64)
    }

    fn connection(&self) -> RtdConnection {
        RtdConnection {
            lease: self.lease,
            provisional: (self.sequence == 1).then(|| (self.sequence, self.value())),
        }
    }
}

#[derive(Debug, Default)]
struct State {
    last_lease: u64,
    models: BTreeMap<String, Model>,
}

/// A sequenced value ready for admission through the workbook RTD session.
pub struct ModelClockTick {
    /// The subscribed RTD topic.
    pub topic: TopicKey,
    /// The connection that owns this value.
    pub lease: RtdLease,
    /// The next sequence number on this lease.
    pub sequence: u64,
    /// The tick value reported by the source.
    pub value: CalcValue,
}

/// Worksheet cells used by the guarded clock balance.
///
/// The scalar state carries the reset count, tick, current balance and previous balance.
/// Keeping them together makes the old balance available without relying on the order in
/// which Excel evaluates several cells in a circular component.
pub struct ClockBalance {
    /// The self-referencing state formula.
    pub state: CellKey,
    /// The RTD clock formula.
    pub tick: CellKey,
    /// The control that starts a new sequence at tick zero.
    pub reset: CellKey,
    /// The balance at the start of a sequence.
    pub opening: CellKey,
    /// The amount to debit on the next tick, with a lower balance limit of zero.
    pub debit: CellKey,
}

impl ClockBalance {
    /// Ordinary A1 formulas for state, current balance, and previous balance, on one sheet.
    pub fn formulas(&self) -> Option<[String; 3]> {
        let [state, tick, reset, opening, debit] =
            [self.state, self.tick, self.reset, self.opening, self.debit];
        if [tick, reset, opening, debit]
            .iter()
            .any(|at| at.sheet() != state.sheet())
        {
            return None;
        }
        let [state, tick, reset, opening, debit] = [state, tick, reset, opening, debit]
            .map(|at| format!("{}{}", column_letters(at.column()), at.row()));
        let first = format!(r#"FIND("|",{state})"#);
        let second = format!(r#"FIND("|",{state},{first}+1)"#);
        let prior_stamp = format!(r#"IFERROR(LEFT({state},{first}-1),"")"#);
        let current = format!("--MID({state},{first}+1,{second}-{first}-1)");
        let previous = format!("--MID({state},{second}+1,32767)");
        let stamp = format!(r#"{reset}&":"&{tick}"#);
        let initial = format!(r#"{reset}&":0|"&{opening}&"|"&{opening}"#);
        let next = format!(r#"{stamp}&"|"&MAX(0,{current}-{debit})&"|"&{current}"#);
        let previous_stamp = format!(r#"{reset}&":"&({tick}-1)"#);
        let state_formula = format!(
            "IF({prior_stamp}={stamp},{state},IF({tick}=0,{initial},IF({prior_stamp}={previous_stamp},{next},{state})))"
        );
        Some([state_formula, current, previous])
    }
}

/// The in-process provider for the `DnaCalc.Clock` RTD topic.
#[derive(Debug, Default)]
pub struct ModelClock {
    state: RefCell<State>,
    fallback: RefCell<Option<Rc<dyn RtdConnector>>>,
}

impl ModelClock {
    /// Retain the carrier's provider for topics outside the local clock.
    pub fn set_fallback(&self, fallback: Rc<dyn RtdConnector>) {
        *self.fallback.borrow_mut() = Some(fallback);
    }

    /// Drop the models from a workbook that was replaced.
    pub fn reset(&self) {
        self.state.borrow_mut().models.clear();
    }

    /// The clock cell and tick backing a just-calculated, guarded balance.
    ///
    /// The caller checks iteration evidence for a prior-pass self-read. Require the exact
    /// guard, a current local lease, and agreement between the state, clock cell and ledger.
    pub fn observed_balance(&self, host: &HostState, at: CellKey) -> Option<(CellKey, u64)> {
        let CellInput::Formula(formula) = host.model().cell(at)? else {
            return None;
        };
        let formula = formula.rendered();
        let formula = String::from_utf16(formula.utf16_code_units()).ok()?;
        if !formula.starts_with("IF(IFERROR(LEFT(") {
            return None;
        }
        let mut sites = Vec::new();
        for (_, target) in host.scalar_reference_sites(at, &format!("={formula}")) {
            if !sites.contains(&target) {
                sites.push(target);
            }
        }
        let [state, reset, tick, opening, debit] = sites.as_slice() else {
            return None;
        };
        if *state != at {
            return None;
        }
        let [guard, ..] = (ClockBalance {
            state: *state,
            tick: *tick,
            reset: *reset,
            opening: *opening,
            debit: *debit,
        })
        .formulas()?;
        if formula != guard {
            return None;
        }
        let state = self.state.borrow();
        let model = state.models.values().find(|model| model.caller == *tick)?;
        let entry = host.rtd_inputs.entry(&model.topic)?;
        let clock = model.value();
        if entry.lease != model.lease || entry.sequence != model.sequence || entry.value != clock {
            return None;
        }
        let clock_result = host.engine.result(*tick)?;
        let FormulaResult::Calculated { value, calculation } = clock_result else {
            return None;
        };
        if *value != clock
            || calculation.held_by_circular_rule()
            || calculation.reads_external_links()
            || !clock_result.rtd_reads().iter().any(|read| {
                read.topic == model.topic
                    && read.lease == model.lease
                    && read.sequence == model.sequence
            })
        {
            return None;
        }
        let Some(FormulaResult::Calculated { value, calculation }) = host.engine.result(at) else {
            return None;
        };
        if calculation.held_by_circular_rule() || calculation.reads_external_links() {
            return None;
        }
        let content = value.as_text()?.to_string_lossy();
        let (stamp, _) = content.split_once('|')?;
        (stamp == format!("{}:{}", model.controls.reset, model.tick)).then_some((*tick, model.tick))
    }

    /// Hold elapsed time while the workbook is busy. Publish one due tick per model at a time.
    pub fn advance(&self, elapsed: Duration, workbook_idle: bool) -> Vec<ModelClockTick> {
        let mut state = self.state.borrow_mut();
        state
            .models
            .values_mut()
            .filter_map(|model| {
                if !model.controls.running || model.tick == MAX_TICK {
                    return None;
                }
                model.elapsed = model.elapsed.saturating_add(elapsed);
                if !workbook_idle || model.elapsed < model.controls.period {
                    return None;
                }
                model.elapsed -= model.controls.period;
                model.tick += 1;
                model.sequence += 1;
                Some(ModelClockTick {
                    topic: model.topic.clone(),
                    lease: model.lease,
                    sequence: model.sequence,
                    value: model.value(),
                })
            })
            .collect()
    }
}

impl RtdConnector for ModelClock {
    fn connect(
        &self,
        topic: &TopicKey,
        caller: CellKey,
        declared: bool,
    ) -> Result<RtdConnection, String> {
        if topic.progid != PROG_ID || !topic.server.is_empty() {
            return self
                .fallback
                .borrow()
                .clone()
                .ok_or_else(|| "RTD server unavailable".to_owned())?
                .connect(topic, caller, declared);
        }
        let controls = Controls::read(&topic.strings).ok_or("Invalid DnaCalc.Clock topic")?;
        let name = &topic.strings[0];
        let mut state = self.state.borrow_mut();
        let (tick, elapsed) = match state.models.get(name) {
            Some(model) if model.topic == *topic => return Ok(model.connection()),
            Some(model) if model.caller != caller => {
                return Err("DnaCalc.Clock model already has a controlling cell".into());
            }
            Some(model) => {
                let reset = model.controls.reset != controls.reset;
                let tick = if reset {
                    0
                } else {
                    model.tick
                        + u64::from(model.tick < MAX_TICK && model.controls.step != controls.step)
                };
                let elapsed = if !reset
                    && model.controls.running == controls.running
                    && model.controls.period == controls.period
                {
                    model.elapsed
                } else {
                    Duration::ZERO
                };
                (tick, elapsed)
            }
            None => (0, Duration::ZERO),
        };
        state.last_lease = state
            .last_lease
            .checked_add(1)
            .ok_or("DnaCalc.Clock leases exhausted")?;
        let model = Model {
            caller,
            topic: topic.clone(),
            controls,
            lease: RtdLease(state.last_lease),
            sequence: 1,
            tick,
            elapsed,
        };
        let connection = model.connection();
        state.models.insert(name.clone(), model);
        Ok(connection)
    }
}
