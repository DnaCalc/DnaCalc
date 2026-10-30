//! The workbook's server (dc-p96): one open [`Workbook`] and the page's subscriptions to it,
//! answering [`Request`]s and turning calc-host's deltas into the cells the page shows
//! ([`Event::Cells`]). The calculation worker runs one ([`crate::worker`]); until every skin
//! reads through the messages, the page runs one in process ([`crate::port`]).
//!
//! A cell crosses as the page shows it: the server works out its text in its number format and
//! its conditional look ([`Workbook::shown`]), its input as it would be edited, and the words for
//! where its value came from. A chart crosses drawn: the server lays it out with the sheet's
//! sizes at 100% and draws it ([`crate::grid::chart::chart_svg`]).

use crate::cells;
use crate::wire::{
    self, Area, Cell, Cells, Chart, ChartPlace, Event, Geometry, Intent, Interest, Key, Outcome,
    Reply, Request, Sheet, Summary, Validation,
};
use crate::workbook::Workbook;
use calc_host::value::{CellKey, GridArea};
use calc_host::{
    Axis, AxisChange, AxisEdit, CalcMode, CellEdit, ProjectedValue, Selection, SkinIntent,
    SubscriptionId,
};
use std::collections::BTreeMap;

/// What the server answers to one request: the reply (or why there is none) and the bytes that
/// travel beside it.
pub struct Answer {
    pub reply: Result<Reply, String>,
    pub bytes: Option<Vec<u8>>,
}

pub struct Server {
    book: Workbook,
    /// The page's subscriptions: its number for each, and calc-host's.
    subscriptions: BTreeMap<u64, SubscriptionId>,
    events: Vec<Event>,
}

impl Default for Server {
    fn default() -> Self {
        Server::new()
    }
}

impl Server {
    /// A server over a new, empty workbook.
    pub fn new() -> Server {
        Server {
            book: Workbook::new_blank(),
            subscriptions: BTreeMap::new(),
            events: Vec::new(),
        }
    }

    /// The open workbook (the in-process transport lets the skins not yet moved to the messages
    /// read it directly).
    pub fn book(&self) -> &Workbook {
        &self.book
    }

    pub fn book_mut(&mut self) -> &mut Workbook {
        &mut self.book
    }

    /// The events since the last call, in order.
    pub fn take_events(&mut self) -> Vec<Event> {
        self.collect();
        std::mem::take(&mut self.events)
    }

    /// Answers `request`; `bytes` are those that travel beside it (an open's package).
    pub fn handle(&mut self, request: Request, bytes: Option<Vec<u8>>) -> Answer {
        let mut out = None;
        let reply = (|| -> Result<Reply, String> {
            match request {
                Request::Ping => Ok(Reply::Pong {
                    version: env!("CARGO_PKG_VERSION").to_owned(),
                }),
                Request::New => {
                    self.install(Workbook::new_blank());
                    Ok(self.done(true, None, 0.0))
                }
                Request::Open { name } => {
                    let started = crate::controller::now_ms();
                    let bytes = bytes.unwrap_or_default();
                    crate::workbook::reserve_for_open(bytes.len());
                    match Workbook::open(&name, bytes) {
                        Ok(book) => {
                            let elapsed = crate::controller::now_ms() - started;
                            let calculated = book.calculated_on_open();
                            self.install(book);
                            Ok(Reply::Done {
                                outcome: self.outcome(true, None, elapsed),
                                opened: Some((elapsed, calculated)),
                                saved: None,
                            })
                        }
                        Err(refused) => Ok(Reply::Done {
                            outcome: Outcome {
                                error: Some(refused.0),
                                ..self.outcome(false, None, 0.0)
                            },
                            opened: None,
                            saved: None,
                        }),
                    }
                }
                Request::Save => {
                    let name = self.book.save_name();
                    match self.book.save() {
                        Ok((bytes, _)) => {
                            out = Some(bytes);
                            Ok(Reply::Done {
                                outcome: self.outcome(true, None, 0.0),
                                opened: None,
                                saved: Some(name),
                            })
                        }
                        Err(failure) => Ok(Reply::Done {
                            outcome: Outcome {
                                error: Some(failure),
                                ..self.outcome(false, None, 0.0)
                            },
                            opened: None,
                            saved: None,
                        }),
                    }
                }
                Request::Intent(intent) => self.intent(intent),
                Request::Subscribe { id, interest } => {
                    let interest = host_interest(interest)?;
                    match self.book.subscribe(interest) {
                        Ok(host) => {
                            self.subscriptions.insert(id, host);
                            Ok(Reply::Subscribed)
                        }
                        Err(error) => Err(error.to_string()),
                    }
                }
                Request::SetInterest { id, interest } => {
                    let interest = host_interest(interest)?;
                    let subscription =
                        *self.subscriptions.get(&id).ok_or("no such subscription")?;
                    self.book.dispatch(SkinIntent::SetInterest {
                        subscription,
                        interest,
                    });
                    Ok(Reply::Subscribed)
                }
                Request::Unsubscribe { id } => {
                    if let Some(host) = self.subscriptions.remove(&id) {
                        self.book.unsubscribe(host);
                    }
                    Ok(Reply::Subscribed)
                }
                Request::Geometry(sheet) => Ok(Reply::Geometry(
                    self.book
                        .host()
                        .sheet_geometry(sheet.id())
                        .map(|g| Geometry::of(&g))
                        .unwrap_or_default(),
                )),
                Request::UsedArea(sheet) => Ok(Reply::UsedArea(
                    self.book.used_area(sheet.id()).map(Area::of),
                )),
                Request::Charts(sheet) => Ok(Reply::Charts(self.charts(sheet))),
                Request::Atlas => {
                    let census = self.book.census();
                    let session = self.book.session();
                    let (undo, redo) = session.history_depth();
                    Ok(Reply::Atlas(wire::Atlas {
                        mode: match session.calc_mode() {
                            CalcMode::Automatic => "automatic",
                            CalcMode::AutomaticExceptTables => "automatic except tables",
                            CalcMode::Manual => "manual",
                        }
                        .to_owned(),
                        revision: self.book.host().revision().get(),
                        undo,
                        redo,
                        intents: session.intent_log().len(),
                        name: self.book.name().to_owned(),
                        changed: self.book.changed(),
                        memory: crate::controller::memory_bytes(),
                        census,
                    }))
                }
                Request::Outline { shown, open } => Ok(Reply::Outline(self.outline(&shown, &open))),
                Request::Dependents(at) => {
                    let at = cell(at)?;
                    Ok(Reply::Keys(
                        self.book
                            .host()
                            .dependents(at)
                            .into_iter()
                            .map(Key::of)
                            .collect(),
                    ))
                }
                Request::Canvas {
                    center,
                    back,
                    forward,
                } => Ok(Reply::Canvas(crate::canvas::view_model(
                    &self.book,
                    cell(center)?,
                    back,
                    forward,
                ))),
                Request::Ledger {
                    sheet,
                    filter,
                    text,
                    sort,
                    page,
                    active,
                } => Ok(Reply::Ledger(crate::ledger::view_model(
                    &self.book,
                    sheet.map(Sheet::id),
                    &filter,
                    &text,
                    &sort,
                    page as usize,
                    active.and_then(Key::cell),
                ))),
                Request::Validation(at) => {
                    let at = cell(at)?;
                    Ok(Reply::Validation(self.book.validation(at).map(|rule| {
                        Validation {
                            dropdown: rule.dropdown,
                            prompt: rule.prompt,
                            items: self.book.validation_items(at),
                        }
                    })))
                }
            }
        })();
        self.collect();
        Answer { reply, bytes: out }
    }

    /// A new workbook in place of the open one: the page's subscriptions end with it.
    fn install(&mut self, book: Workbook) {
        self.book = book;
        self.subscriptions.clear();
        self.events.clear();
    }

    fn summary(&self) -> Summary {
        let iteration = self.book.host().model().definitions().iteration();
        let circular = self.book.host().circular_cells();
        Summary {
            name: self.book.name().to_owned(),
            changed: self.book.changed(),
            undo: self.book.session().history_depth().0,
            redo: self.book.session().history_depth().1,
            sheets: self
                .book
                .sheets()
                .into_iter()
                .map(|tab| (Sheet::of(tab.id), tab.name))
                .collect(),
            manual: self.book.session().calc_mode() == CalcMode::Manual,
            revision: self.book.host().revision().get(),
            iteration: iteration.enabled(),
            max_iterations: iteration.max_iterations(),
            max_change: iteration.max_change(),
            circular: circular.first().copied().map(Key::of),
            circular_count: circular.len(),
        }
    }

    fn outcome(&self, accepted: bool, error: Option<String>, elapsed_ms: f64) -> Outcome {
        let selection = self.book.session().selection();
        Outcome {
            accepted,
            committed: false,
            recalculated: false,
            error,
            recalc: None,
            note: None,
            elapsed_ms,
            selection: selection
                .active
                .map(|at| (Key::of(at), selection.areas.first().copied().map(Area::of))),
            summary: self.summary(),
        }
    }

    fn done(&self, accepted: bool, error: Option<String>, elapsed_ms: f64) -> Reply {
        Reply::Done {
            outcome: self.outcome(accepted, error, elapsed_ms),
            opened: None,
            saved: None,
        }
    }

    fn intent(&mut self, intent: Intent) -> Result<Reply, String> {
        let intent = host_intent(intent)?;
        // What a warning or information rule says about an entry, worked out before it is taken.
        let note = match &intent {
            SkinIntent::EnterCell { at, text } => {
                self.book.validation_note(*at, text).map(|failure| {
                    let kind = match failure.style {
                        calc_host::ErrorStyle::Warning => "Warning",
                        _ => "Information",
                    };
                    (kind.to_owned(), failure.to_string())
                })
            }
            _ => None,
        };
        let started = crate::controller::now_ms();
        let receipt = self.book.dispatch(intent);
        let elapsed = crate::controller::now_ms() - started;
        let mut outcome = self.outcome(
            receipt.accepted(),
            receipt.error.as_ref().map(ToString::to_string),
            elapsed,
        );
        outcome.committed = receipt.committed.is_some();
        outcome.recalculated = receipt.recalc.is_some();
        outcome.recalc = receipt.recalc.as_ref().map(Workbook::recalc_text);
        outcome.note = note.filter(|_| receipt.accepted());
        Ok(Reply::Done {
            outcome,
            opened: None,
            saved: None,
        })
    }

    /// calc-host's deltas since the last call, as the page's subscriptions' cells.
    fn collect(&mut self) {
        let deltas = self.book.take_deltas();
        if deltas.is_empty() {
            return;
        }
        let ids: BTreeMap<SubscriptionId, u64> = self
            .subscriptions
            .iter()
            .map(|(&page, &host)| (host, page))
            .collect();
        for delta in deltas {
            let Some(&id) = ids.get(&delta.subscription) else {
                continue;
            };
            let mut cells = Vec::new();
            let mut emptied = Vec::new();
            for projected in &delta.cells {
                if projected.input().is_none() && projected.value() == &ProjectedValue::Empty {
                    emptied.push(Key::of(projected.at()));
                    continue;
                }
                let shown = self.book.shown(projected);
                cells.push(Cell::of(
                    &shown,
                    cells::input_text(projected.input()),
                    cells::provenance_text(self.book.host(), projected),
                    projected,
                ));
            }
            self.events.push(Event::Cells(Cells {
                id,
                seq: delta.seq,
                reset: delta.reset,
                cells,
                emptied,
            }));
        }
    }

    /// The tree's outline ([`crate::tree::outline_sheet`] per sheet) with the open sheets' shown
    /// cells.
    fn outline(&self, shown: &[(Sheet, u64)], open: &[Sheet]) -> wire::Outline {
        let host = self.book.host();
        let names = host
            .defined_names()
            .iter()
            .map(|name| {
                let formula = name.formula.strip_prefix('=').unwrap_or(&name.formula);
                format!("{} = {formula}", name.name)
            })
            .collect();
        let mut sheets = Vec::new();
        let mut wanted = Vec::new();
        for tab in self.book.sheets() {
            let sheet = Sheet::of(tab.id);
            let limit = shown
                .iter()
                .find(|(s, _)| *s == sheet)
                .map_or(crate::tree::ROW_PAGE, |(_, n)| *n as usize);
            let outline = crate::tree::outline_sheet(
                tab.id,
                tab.name,
                host.input_cells(tab.id).into_iter(),
                limit,
            );
            if open.contains(&sheet) {
                for (_, row) in &outline.rows {
                    wanted.extend(row.iter().take(crate::tree::ROW_CELLS).copied());
                }
            }
            sheets.push(wire::OutlineSheet {
                sheet,
                name: outline.name,
                cells: outline.cells as u64,
                rows: outline
                    .rows
                    .iter()
                    .map(|(row, cells)| (*row, cells.iter().copied().map(Key::of).collect()))
                    .collect(),
                more_rows: outline.more_rows as u64,
            });
        }
        let cells = self
            .book
            .project(wanted)
            .iter()
            .map(|projected| {
                Cell::of(
                    &self.book.shown(projected),
                    cells::input_text(projected.input()),
                    cells::provenance_text(host, projected),
                    projected,
                )
            })
            .collect();
        wire::Outline {
            workbook: self.book.name().to_owned(),
            names,
            sheets,
            cells,
        }
    }

    /// The sheet's charts, laid out with its sizes at 100% and drawn.
    fn charts(&self, sheet: Sheet) -> Vec<Chart> {
        let host = self.book.host();
        let geometry = host.sheet_geometry(sheet.id());
        let m = crate::grid::sheet_metrics(geometry.as_ref());
        let corner = |c: &calc_host::ChartCorner| {
            let x = m.col_start(c.cell.column()) + c.dx;
            let y = m.row_start(c.cell.row()) + c.dy;
            (x, y)
        };
        host.charts(sheet.id())
            .0
            .into_iter()
            .map(|view| {
                let (place, width, height) = match view.place {
                    calc_host::ChartPlace::Cells { from, to } => {
                        let ((x0, y0), (x1, y1)) = (corner(&from), corner(&to));
                        (
                            ChartPlace::Cells {
                                from: (Key::of(from.cell), from.dx, from.dy),
                                to: (Key::of(to.cell), to.dx, to.dy),
                            },
                            x1 - x0,
                            y1 - y0,
                        )
                    }
                    calc_host::ChartPlace::Sized {
                        from,
                        width,
                        height,
                    } => (
                        ChartPlace::Sized {
                            from: (Key::of(from.cell), from.dx, from.dy),
                            width,
                            height,
                        },
                        width,
                        height,
                    ),
                    calc_host::ChartPlace::Absolute {
                        x,
                        y,
                        width,
                        height,
                    } => (
                        ChartPlace::Absolute {
                            x,
                            y,
                            width,
                            height,
                        },
                        width,
                        height,
                    ),
                };
                let kind = view
                    .plots
                    .first()
                    .map(|plot| format!("{:?}", plot.kind).to_lowercase())
                    .unwrap_or_default();
                Chart {
                    name: view.name.clone(),
                    kind,
                    place,
                    svg: if width > 0.0 && height > 0.0 {
                        crate::grid::chart::chart_svg(&view, width, height)
                    } else {
                        String::new()
                    },
                }
            })
            .collect()
    }
}

fn cell(at: Key) -> Result<CellKey, String> {
    at.cell().ok_or_else(|| "not a cell".to_owned())
}

fn area(area: Area) -> Result<GridArea, String> {
    area.area().ok_or_else(|| "not an area".to_owned())
}

fn host_interest(interest: Interest) -> Result<calc_host::Interest, String> {
    Ok(match interest {
        Interest::Area(a) => calc_host::Interest::Area(area(a)?),
        Interest::Sheet(sheet) => calc_host::Interest::Sheet(sheet.id()),
        Interest::Selection => calc_host::Interest::Selection,
    })
}

/// The page's intent as calc-host's.
fn host_intent(intent: Intent) -> Result<SkinIntent, String> {
    Ok(match intent {
        Intent::EnterCell { at, text } => SkinIntent::EnterCell {
            at: cell(at)?,
            text,
        },
        Intent::Clear { area: a } => SkinIntent::Edit {
            edits: vec![CellEdit::Clear { area: area(a)? }],
        },
        Intent::EditAxis {
            sheet,
            rows,
            insert,
            start,
            count,
        } => {
            let axis = if rows { Axis::Rows } else { Axis::Columns };
            let change = if insert {
                AxisChange::Insert
            } else {
                AxisChange::Delete
            };
            SkinIntent::EditAxis(
                AxisEdit::new(sheet.id(), axis, change, start, count)
                    .map_err(|e| format!("{e:?}"))?,
            )
        }
        Intent::Copy { from, to, cut } => SkinIntent::Copy {
            from: area(from)?,
            to: area(to)?,
            cut,
        },
        Intent::Undo => SkinIntent::Undo,
        Intent::Redo => SkinIntent::Redo,
        Intent::Recalculate => SkinIntent::Recalculate,
        Intent::SetCalcMode { manual } => SkinIntent::SetCalcMode(if manual {
            CalcMode::Manual
        } else {
            CalcMode::Automatic
        }),
        Intent::SetIteration {
            enabled,
            max_iterations,
            max_change_bits,
        } => SkinIntent::Edit {
            edits: vec![calc_host::CellEdit::SetIteration(
                calc_host::IterationSettings::new(
                    enabled,
                    max_iterations,
                    f64::from_bits(max_change_bits),
                ),
            )],
        },
        Intent::Select { active, area: a } => SkinIntent::Select(Selection {
            active: Some(cell(active)?),
            areas: vec![area(a)?],
        }),
    })
}

/// Answers a message's text as the worker does: the reply, or the reason there is none.
pub fn answer_text(
    server: &mut Server,
    text: &str,
    bytes: Option<Vec<u8>>,
) -> (String, Option<Vec<u8>>) {
    use wire::{FromServer, ToServer};
    match wire::decode::<ToServer>(text) {
        Ok(ToServer { id, request }) => {
            let answer = server.handle(request, bytes);
            let message = match answer.reply {
                Ok(reply) => FromServer::Reply { id, reply },
                Err(message) => FromServer::Failed {
                    id: Some(id),
                    message,
                },
            };
            (wire::encode(&message), answer.bytes)
        }
        Err(message) => (
            wire::encode(&FromServer::Failed { id: None, message }),
            None,
        ),
    }
}
