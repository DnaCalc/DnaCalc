use crate::{State, button, cell_label, invoke, result, status};
use calc_skin::grid::{Area, Key};
use calc_skin::{Answer, Interest, LiveTopic, Provenance, RawValue, WorkbookId, query};
use dna_kit::{Session, ViewCommand};
use leptos::prelude::*;
use std::collections::VecDeque;

const RETAINED: usize = 1200;
const DISPLAYED: usize = 120;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Signal {
    Cell(Key),
    Topic {
        lease: u64,
        progid: String,
        server: String,
        strings: Vec<String>,
    },
}

impl Signal {
    fn topic(topic: &LiveTopic) -> Self {
        Self::Topic {
            lease: topic.lease,
            progid: topic.progid.clone(),
            server: topic.server.clone(),
            strings: topic.strings.clone(),
        }
    }

    fn label(&self, session: Session) -> String {
        match self {
            Self::Cell(at) => cell_label(session, *at),
            Self::Topic {
                progid, strings, ..
            } => format!("{progid} · {}", strings.join(" / ")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sequence {
    Cell,
    Topic { published: u64, received: Option<u64> },
}

#[derive(Clone, Debug)]
struct Sample {
    sequence: Sequence,
    value: RawValue,
    time: u64,
}

#[derive(Debug)]
struct Series {
    target: Signal,
    samples: VecDeque<Sample>,
    observed: u64,
    coalesced: u64,
    previous_receipt: Option<u64>,
    last_admission: Option<u64>,
}

#[derive(Default)]
pub(crate) struct Recording {
    workbook: Option<WorkbookId>,
    series: Vec<Series>,
    selected: Option<usize>,
    capturing: bool,
    replay: Option<usize>,
}

impl Recording {
    fn select(&mut self, workbook: WorkbookId, target: Signal) {
        if self.workbook != Some(workbook) {
            *self = Self {
                workbook: Some(workbook),
                ..Self::default()
            };
        }
        self.replay = None;
        let index = self
            .series
            .iter()
            .position(|series| series.target == target)
            .unwrap_or_else(|| {
                self.series.push(Series {
                    target,
                    samples: VecDeque::new(),
                    observed: 0,
                    coalesced: 0,
                    previous_receipt: None,
                    last_admission: None,
                });
                self.series.len() - 1
            });
        self.selected = Some(index);
        self.series[index].previous_receipt = None;
    }

    fn selected(&self) -> Option<&Series> {
        self.selected.and_then(|index| self.series.get(index))
    }

    fn take_sample(&mut self, sequence: Sequence, value: RawValue) {
        let Some(series) = self.selected.and_then(|index| self.series.get_mut(index)) else {
            return;
        };
        let repeated =
            series
                .samples
                .back()
                .is_some_and(|sample| match (sample.sequence, sequence) {
                    (Sequence::Cell, Sequence::Cell) => sample.value == value,
                    (Sequence::Topic { published: previous, .. }, Sequence::Topic { published: current, .. }) => previous == current,
                    _ => false,
                });
        if repeated {
            if series.previous_receipt.is_none()
                && let Sequence::Topic { received, .. } = sequence
            {
                series.previous_receipt = received;
            }
            return;
        }
        if let Sequence::Topic { received, .. } = sequence {
            if let Some((previous, received)) = series.previous_receipt.zip(received) {
                series.coalesced = series.coalesced.saturating_add(
                    received.saturating_sub(previous).saturating_sub(1),
                );
            }
            series.previous_receipt = received;
        }
        if series.samples.len() == RETAINED {
            series.samples.pop_front();
            self.replay = self.replay.map(|position| position.saturating_sub(1));
        }
        series.samples.push_back(Sample {
            sequence,
            value,
            time: now(),
        });
        series.observed += 1;
    }
}

fn now() -> u64 {
    let (seconds, nanos) = uuid::Timestamp::now(uuid::NoContext).to_unix();
    seconds.saturating_mul(1000) + u64::from(nanos / 1_000_000)
}

fn sampled_cell(answer: Answer<calc_skin::CellWindow>, at: Key) -> Option<(Sequence, RawValue)> {
    let cell = answer.body.cells.into_iter().find(|cell| cell.at == at)?;
    matches!(
        cell.shown.provenance,
        Provenance::Literal | Provenance::Calculated { .. } | Provenance::External { .. }
    )
    .then_some((Sequence::Cell, cell.shown.value))
}

pub(crate) fn commands(session: Session) -> Vec<ViewCommand> {
    let recording = session.local::<State>().recording;
    if cfg!(target_arch = "wasm32") {
        let target = Memo::new(move |_| {
            let workbook = session.stamp()?.workbook;
            recording.with(|recording| {
                let series = recording.selected().filter(|_| recording.workbook == Some(workbook))?;
                let capturing = recording.capturing;
                (capturing || matches!(series.target, Signal::Topic { .. }))
                    .then(|| (series.target.clone(), capturing))
            })
        });
        let cell = session.following::<query::Cells>();
        let sources =
            session.following_with_budget::<query::SourceInventory>(crate::INSTRUMENT_BUDGET);
        Effect::new(move |_| {
            let target = target.get();
            let selected_cell = match target {
                Some((Signal::Cell(at), _)) => Some(at),
                _ => None,
            };
            let cells = cell.at(selected_cell.map(|at| query::Cells {
                interest: Interest::Area(Area(at, at)),
            }));
            let topics = sources
                .at(matches!(target, Some((Signal::Topic { .. }, _))).then_some(query::SourceInventory));
            let sample = match target {
                Some((Signal::Cell(at), _)) => result(cells, "")
                    .ok()
                    .and_then(|answer| sampled_cell(answer, at)),
                Some((target, capturing)) => result(topics, "").ok().and_then(|answer| {
                    let topic = answer.body.topics.into_iter().find(|topic| Signal::topic(topic) == target)?;
                    if let Some(time) = topic.receipt.and_then(|receipt| receipt.last_unix_ms) {
                        recording.update(|recording| {
                            if let Some(series) = recording.selected.and_then(|index| recording.series.get_mut(index)) {
                                series.last_admission = Some(time);
                            }
                        });
                    }
                    capturing.then_some((Sequence::Topic {
                        published: topic.sequence,
                        received: topic.receipt.map(|receipt| receipt.received),
                    }, topic.last.value))
                }),
                None => None,
            };
            if let Some((sequence, value)) = sample {
                recording.update(|recording| recording.take_sample(sequence, value));
            }
        });
    }
    let select = move |signal: Signal| -> Result<(), String> {
        let workbook = session.stamp().ok_or("Open a workbook first.")?.workbook;
        recording.update(|recording| recording.select(workbook, signal));
        Ok(())
    };
    vec![
        ViewCommand::global(
            "inspector.pin-signal",
            "Watch selected cell",
            &[],
            move |_| {
                select(Signal::Cell(
                    session.active().ok_or("Select a cell to watch.")?,
                ))
            },
        ),
        ViewCommand::global(
            "inspector.watch-topic",
            "Watch RTD topic",
            &[],
            move |invocation| {
                let (lease, progid, server, strings) =
                    serde_json::from_value(invocation.args.clone())
                        .map_err(|_| "Select an RTD topic.")?;
                select(Signal::Topic {
                    lease,
                    progid,
                    server,
                    strings,
                })
            },
        ),
        ViewCommand::global("inspector.record", "Record signal", &[], move |_| {
            let workbook = session.stamp().ok_or("Open a workbook first.")?.workbook;
            if !recording.with_untracked(|recording| {
                recording.workbook == Some(workbook) && recording.selected.is_some()
            }) {
                select(Signal::Cell(
                    session
                        .active()
                        .ok_or("Select a cell or topic to record.")?,
                ))?;
            }
            recording.update(|recording| {
                recording.capturing = true;
                recording.replay = None;
            });
            Ok(())
        }),
        ViewCommand::global(
            "inspector.pause-capture",
            "Pause signal capture",
            &[],
            move |_| {
                recording.update(|recording| {
                    recording.capturing = false;
                    if let Some(series) = recording.selected.and_then(|index| recording.series.get_mut(index)) {
                        series.previous_receipt = None;
                    }
                });
                Ok(())
            },
        ),
        ViewCommand::global(
            "inspector.clear-capture",
            "Clear signal recording",
            &[],
            move |_| {
                recording.update(|recording| {
                    if let Some(series) = recording
                        .selected
                        .and_then(|index| recording.series.get_mut(index))
                    {
                        series.samples.clear();
                        series.observed = 0;
                        series.coalesced = 0;
                        series.previous_receipt = None;
                    }
                    recording.replay = None;
                });
                Ok(())
            },
        ),
        ViewCommand::global(
            "inspector.replay",
            "Replay signal",
            &[],
            move |invocation| {
                let position: Option<usize> = serde_json::from_value(invocation.args.clone())
                    .map_err(|_| "Choose a recorded sample.")?;
                recording.update(|recording| {
                    recording.replay = position.filter(|position| {
                        recording
                            .selected()
                            .is_some_and(|series| *position < series.samples.len())
                    });
                });
                Ok(())
            },
        ),
    ]
}

fn values(samples: &[Sample]) -> impl Iterator<Item = f64> + '_ {
    samples.iter().filter_map(|sample| match sample.value {
        RawValue::Number { bits } => Some(f64::from_bits(bits)).filter(|value| value.is_finite()),
        _ => None,
    })
}

fn sample_range(samples: &[Sample]) -> Option<(f64, f64)> {
    let mut values = values(samples);
    let first = values.next()?;
    Some(values.fold((first, first), |(min, max), value| {
        (min.min(value), max.max(value))
    }))
}

pub(crate) fn recording(
    session: Session,
    topics: dna_kit::Following<query::SourceInventory>,
) -> impl IntoView {
    let recording = session.local::<State>().recording;
    let time = RwSignal::new(now());
    #[cfg(target_arch = "wasm32")]
    if let Ok(timer) = leptos::leptos_dom::helpers::set_interval_with_handle(
        move || time.set(now()),
        std::time::Duration::from_secs(1),
    ) {
        on_cleanup(move || timer.clear());
    }
    view! {
        <section class="dna-inspector-card">
            <h2>"Signal recorder"</h2>
            <div class="dna-inspector-actions">
                {button(session, "inspector.pin-signal", "Watch selected cell")}
                {button(session, "inspector.record", "Record")}
                {button(session, "inspector.pause-capture", "Pause capture")}
                {button(session, "inspector.clear-capture", "Clear")}
            </div>
            <label hidden=move || recording.with(|recording| recording.selected().is_none_or(|series| series.samples.is_empty()))>
                "Replay sample"
                <input
                    type="range" aria-label="Replay sample" min=0
                    max=move || recording.with(|recording| recording.selected().map_or(0, |series| series.samples.len().saturating_sub(1)))
                    prop:value=move || recording.with(|recording| recording.replay.unwrap_or_else(|| {
                        recording.selected().map_or(0, |series| series.samples.len().saturating_sub(1))
                    }))
                    on:input=move |event| {
                        if let Ok(position) = event_target_value(&event).parse::<usize>() {
                            crate::run(session, "inspector.replay", serde_json::json!(position));
                        }
                    }
                />
            </label>
            <Show when=move || recording.with(|recording| recording.replay.is_some())>
                {button(session, "inspector.replay", "Follow latest")}
            </Show>
            {move || recording.with(|recording| {
                if session.stamp().map(|stamp| stamp.workbook) != recording.workbook {
                    return status("Choose a cell or RTD topic to observe.".into());
                }
                let Some(series) = recording.selected() else { return status("Choose a cell or RTD topic to observe.".into()); };
                let end = recording.replay.map_or(series.samples.len(), |position| position + 1);
                let start = end.saturating_sub(DISPLAYED);
                let samples: Vec<_> = series.samples.range(start..end).cloned().collect();
                let base_time = series.samples.front().map_or(0, |sample| sample.time);
                let last = samples.last().map(|sample| sample.value.display());
                let label = series.target.label(session);
                let count = series.samples.len();
                let observed = series.observed;
                let capture = if recording.capturing { "Recording" } else { "Capture paused" };
                let telemetry = if matches!(series.target, Signal::Topic { .. }) {
                    let answer = result(topics.at(Some(query::SourceInventory)), "").ok();
                    let last_sample = samples.last().map(|sample| sample.time);
                    answer.map(|answer| {
                        let source = answer.body.topics.into_iter().find(|topic| Signal::topic(topic) == series.target);
                        let receipt = source.as_ref().and_then(|topic| topic.receipt);
                        let received_rate = receipt.and_then(|receipt| rate(receipt.timed_received,
                            receipt.first_unix_ms, receipt.last_unix_ms));
                        let sample_rate = rate(samples.len() as u64,
                            samples.first().map(|sample| sample.time), last_sample);
                        let coalesced = series.coalesced;
                        view! {
                            <p data-inspector="topic-coverage">
                                {receipt.map(|receipt| format!("{} received in this lease · ", receipt.received))}
                                {format!("{observed} observed · {coalesced} coalesced")}
                            </p>
                            {received_rate.map(|rate| view! { <p>{format!("Received {rate:.2}/s")}</p> })}
                            {sample_rate.map(|rate| view! { <p>{format!("Sampled {rate:.2}/s in the displayed window")}</p> })}
                            {(source.is_none()).then(|| {
                                series.last_admission.map(|received| format!("Disconnected · stale {} s",
                                    time.get().saturating_sub(received) / 1000))
                                    .unwrap_or_else(|| "Disconnected · stale age unavailable".into())
                            })}
                        }
                    })
                } else { None };
                view! {
                    <p><strong>{label}</strong>" · " {capture}</p>
                    <p data-inspector="recording-count">{format!("{observed} observed · {count} retained")}</p>
                    {telemetry}
                    {last.map(|last| view! { <p data-inspector="recording-last">"Last " {last}</p> })}
                    {sample_range(&samples).map(|(min, max)| {
                        let series = plot(&samples, min, max);
                        view! {
                            <p>{format!("Min {min} · Max {max}")}</p>
                            {series}
                        }
                    })}
                    {(!samples.is_empty()).then(|| view! {
                        <details open=true><summary>{format!("Displayed samples {}–{end}", start + 1)}</summary>
                            <table><thead><tr><th scope="col">"Elapsed (ms)"</th><th scope="col">"Value"</th></tr></thead>
                            <tbody>{samples.into_iter().map(|sample| view! {
                                <tr><td>{sample.time.saturating_sub(base_time)}</td><td>{sample.value.display()}</td></tr>
                            }).collect_view()}</tbody></table>
                        </details>
                    })}
                }.into_any()
            })}
        </section>
    }
}

fn rate(count: u64, first: Option<u64>, last: Option<u64>) -> Option<f64> {
    let elapsed = last?.checked_sub(first?)?;
    (count > 1 && elapsed > 0).then(|| (count - 1) as f64 * 1000.0 / elapsed as f64)
}

pub(crate) fn topic_button(session: Session, topic: &LiveTopic) -> impl IntoView + use<> {
    let args = serde_json::json!([topic.lease, topic.progid, topic.server, topic.strings]);
    invoke(session, "inspector.watch-topic", args, "Watch topic".into())
}

fn plot(samples: &[Sample], min: f64, max: f64) -> impl IntoView + use<> {
    let scale = min.abs().max(max.abs()).max(f64::MIN_POSITIVE);
    let bounds = (min / scale, max / scale);
    let start = samples.first().map_or(0, |sample| sample.time);
    let duration = samples
        .last()
        .map_or(0, |sample| sample.time.saturating_sub(start));
    let points = samples
        .iter()
        .enumerate()
        .filter_map(|(index, sample)| {
            let RawValue::Number { bits } = sample.value else {
                return None;
            };
            let value = f64::from_bits(bits);
            if !value.is_finite() {
                return None;
            }
            let x = if duration == 0 {
                index as f64 / samples.len().saturating_sub(1).max(1) as f64
            } else {
                (sample.time.saturating_sub(start) as f64 / duration as f64).min(1.)
            };
            let y = if bounds.1 == bounds.0 {
                0.5
            } else {
                (value / scale - bounds.0) / (bounds.1 - bounds.0)
            };
            let left = format!("{}%", 2. + x * 96.);
            let bottom = format!("{}%", 4. + y * 92.);
            Some(
                view! { <span style:left=left style:bottom=bottom title=value.to_string()></span> },
            )
        })
        .collect_view();
    view! { <div class="dna-inspector-plot" role="img" aria-label="Recorded values by elapsed time">{points}</div> }
}

pub(crate) fn pinned(session: Session) -> impl IntoView {
    let recording = session.local::<State>().recording;
    let cells = session.following::<query::Cells>();
    let at = Memo::new(move |_| {
        let workbook = session.stamp()?.workbook;
        recording.with(|recording| {
            if recording.workbook != Some(workbook) {
                return None;
            }
            match &recording.selected()?.target {
                Signal::Cell(at) => Some(*at),
                Signal::Topic { .. } => None,
            }
        })
    });
    view! {
        <Show when=move || at.get().is_some()>
            <details class="dna-inspector-card dna-inspector-pinned" open=true>
                <summary>"Pinned Watch"</summary>
                {move || {
                    let at = at.get();
                    let cells = cells.at(at.map(|at| query::Cells { interest: Interest::Area(Area(at, at)) }));
                    match result(cells, "Choose a cell to watch.") {
                        Ok(answer) => answer.body.cells.into_iter().find(|cell| Some(cell.at) == at).map(|cell| {
                            let display = match cell.shown.provenance {
                                Provenance::AsSaved => format!("{} · As saved", cell.text),
                                Provenance::Stale { .. } => format!("{} · Stale", cell.text),
                                Provenance::Refused { .. } => "Unavailable".into(),
                                _ => cell.text,
                            };
                            view! { <p>{crate::reveal(session, cell.at)} " " {display}</p> }.into_any()
                        }).unwrap_or_else(|| status("The pinned cell is empty.".into())),
                        Err(message) => status(message),
                    }
                }}
            </details>
        </Show>
    }
}
