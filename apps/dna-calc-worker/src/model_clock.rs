//! An in-process RTD clock driven by elapsed time supplied by the workbook owner.

use calc_host::{
    RtdConnection, RtdConnector, RtdLease, TopicKey,
    value::{CalcValue, CellKey},
};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc, time::Duration};

const PROG_ID: &str = "dnacalc.clock";
const MAX_TICK: u64 = 1 << 53;

#[derive(Debug)]
struct Controls {
    period: Duration,
    running: bool,
    step: u64,
}

impl Controls {
    fn read(args: &[String]) -> Option<Self> {
        let [model, kind, tail @ ..] = args else {
            return None;
        };
        if model.is_empty() || kind != "tick" || tail.len() > 3 {
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
        Some(Self {
            period,
            running,
            step,
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

pub(crate) struct Tick {
    pub topic: TopicKey,
    pub lease: RtdLease,
    pub sequence: u64,
    pub value: CalcValue,
}

#[derive(Debug, Default)]
pub(crate) struct ModelClock {
    state: RefCell<State>,
    fallback: RefCell<Option<Rc<dyn RtdConnector>>>,
}

impl ModelClock {
    pub fn set_fallback(&self, fallback: Rc<dyn RtdConnector>) {
        *self.fallback.borrow_mut() = Some(fallback);
    }

    pub fn reset(&self) {
        self.state.borrow_mut().models.clear();
    }

    /// Hold elapsed time while the workbook is busy. Publish one due tick per model at a time.
    pub fn advance(&self, elapsed: Duration, workbook_idle: bool) -> Vec<Tick> {
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
                Some(Tick {
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
                let tick = model.tick
                    + u64::from(model.tick < MAX_TICK && model.controls.step != controls.step);
                let elapsed = if model.controls.running == controls.running
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
