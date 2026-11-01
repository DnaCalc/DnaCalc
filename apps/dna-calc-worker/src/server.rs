//! The workbook's server (dc-p96): one open [`Workbook`] and the page's subscriptions to it,
//! answering [`Request`]s and turning calc-host's deltas into the cells the page shows
//! ([`Event::Tiles`]). The calculation worker owns one ([`crate::worker`]); native composition
//! injects the same route through [`crate::transport::InProcessTransport`].
//!
//! A cell crosses as the page shows it: the server works out its text in its number format and
//! its conditional look ([`Workbook::shown`]), its input as it would be edited, and the words for
//! where its value came from. A chart crosses drawn: the server lays it out with the sheet's
//! sizes at 100% and draws it ([`crate::chart::chart_svg`]).

use crate::cells;
use crate::wire::{
    self, Area, Chart, ChartPlace, Event, Intent, Interest, Key, Outcome, Reply, Request, Sheet,
    Summary, Validation,
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
    calculation_budget: calc_host::WorkBudget,
    workbook: u64,
    commands: calc_host::CommandJournal<(Result<Reply, String>, Option<Vec<u8>>)>,
    validation_pending: Option<(u64, u64, Selection, SkinIntent)>,
    book: Workbook,
    /// The page's subscriptions: its number for each, and calc-host's.
    subscriptions: BTreeMap<u64, SubscriptionId>,
    subscription_interests: BTreeMap<u64, calc_host::Interest>,
    last_subscription: Option<u64>,
    events: Vec<Event>,
    deferred_calculation: Vec<Event>,
    tile_streams: BTreeMap<u64, crate::publication::TileStream>,
    next_publication: u64,
    snapshots: BTreeMap<u64, calc_host::CopySnapshot>,
    next_clip: u64,
    clipboard_session: String,
    paste_plan: Option<(u64, calc_host::PastePlan)>,
}

impl Default for Server {
    fn default() -> Self {
        Server::new()
    }
}

impl Server {
    /// A server over a new, empty workbook.
    pub fn new() -> Server {
        let book = Workbook::new_blank();
        Server {
            calculation_budget: calc_host::WorkBudget::UNLIMITED,
            workbook: 0,
            commands: calc_host::CommandJournal::new(128, 16 * 1024 * 1024),
            validation_pending: None,
            book,
            subscriptions: BTreeMap::new(),
            subscription_interests: BTreeMap::new(),
            last_subscription: None,
            events: Vec::new(),
            deferred_calculation: Vec::new(),
            tile_streams: BTreeMap::new(),
            next_publication: 1,
            snapshots: BTreeMap::new(),
            next_clip: 1,
            clipboard_session: crate::services::session_nonce(),
            paste_plan: None,
        }
    }

    pub fn set_calculation_budget(&mut self, budget: calc_host::WorkBudget) {
        self.calculation_budget = budget;
        self.book.set_calculation_budget(budget);
    }
    pub fn has_pending_calculation(&self) -> bool {
        self.deferred_calculation.len() < wire::MAX_PENDING_REQUESTS - 2
            && self.book.has_pending_calculation()
    }
    pub fn advance_calculation(&mut self) -> calc_host::HostProgress {
        let progress = self.book.advance_calculation();
        self.collect();
        progress
    }

    /// A provider posts its return to this owner. User code resumes in a later budgeted task.
    pub fn complete_async(
        &mut self,
        token: &calc_host::AsyncToken,
        value: calc_host::value::CalcValue,
    ) -> calc_host::AsyncDisposition {
        let disposition = self.book.complete_async(token, value);
        self.collect();
        disposition
    }

    /// Changes whenever this server installs a replacement workbook.
    pub fn workbook_id(&self) -> u64 {
        self.workbook
    }

    /// The open workbook, available to native composition and tests.
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
        if let Request::Command(envelope) = request {
            return self.command(envelope, bytes);
        }
        if request.is_command() {
            return Answer {
                reply: Err("A workbook command needs an identity and base revision".into()),
                bytes: None,
            };
        }
        self.handle_admitted(request, bytes)
    }

    /// Immediate clients resolve their revision and target immediately before delivery. Remote
    /// clients construct an envelope at submission and retain it unchanged for any retry.
    pub fn envelope(&self, request: Request) -> Request {
        Request::Command(wire::CommandEnvelope {
            session: self.clipboard_session.clone(),
            id: self.commands.next_id(),
            workbook: self.workbook,
            base_revision: self.book.host().revision().get(),
            source: "headless".into(),
            undo_group: None,
            resolved_selection: self.current_selection(),
            request: Box::new(request),
        })
    }
    pub fn execute_current(&mut self, request: Request, bytes: Option<Vec<u8>>) -> Answer {
        let request = if request.is_command() {
            self.envelope(request)
        } else {
            request
        };
        self.handle(request, bytes)
    }
    fn current_selection(&self) -> Option<(Key, Vec<Area>)> {
        let selection = self.book.session().selection();
        selection.active.map(|at| {
            (
                Key::of(at),
                selection.areas.iter().copied().map(Area::of).collect(),
            )
        })
    }
    fn command(&mut self, envelope: wire::CommandEnvelope, bytes: Option<Vec<u8>>) -> Answer {
        use std::hash::{Hash, Hasher};
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        bytes.hash(&mut hash);
        if envelope.session != self.clipboard_session {
            return Answer {
                reply: Err("This command belongs to another workbook session".into()),
                bytes: None,
            };
        }
        let payload = wire::encode(&(envelope.clone(), hash.finish()));
        let stamp = calc_host::CommandStamp {
            id: envelope.id,
            workbook: envelope.workbook,
            base_revision: envelope.base_revision,
        };
        let admission = self.commands.admit(
            stamp,
            self.workbook,
            self.book.host().revision().get(),
            &payload,
        );
        match admission {
            Ok(Some((reply, bytes))) => return Answer { reply, bytes },
            Err(reason) => {
                return Answer {
                    reply: Err(reason.to_string()),
                    bytes: None,
                };
            }
            Ok(None) => {}
        }
        let answer = if !envelope.request.is_command() {
            Answer {
                reply: Err("A command envelope must contain one workbook command".into()),
                bytes: None,
            }
        } else if self.deferred_calculation.len() >= wire::MAX_PENDING_REQUESTS - 2 {
            Answer {
                reply: Err("Calculation delivery is waiting for tile acknowledgements".into()),
                bytes: None,
            }
        } else if envelope.resolved_selection != self.current_selection() {
            Answer {reply:Err("The command's resolved selection has changed; submit it again for the current target".into()),bytes:None}
        } else {
            self.handle_admitted(*envelope.request, bytes)
        };
        let reply_bytes = wire::encode(&answer.reply)
            .len()
            .saturating_add(answer.bytes.as_ref().map_or(0, Vec::len));
        self.commands.remember(
            stamp.id,
            payload,
            (answer.reply.clone(), answer.bytes.clone()),
            reply_bytes,
        );
        answer
    }
    fn handle_admitted(&mut self, request: Request, bytes: Option<Vec<u8>>) -> Answer {
        let mut out = None;
        let reply = (|| -> Result<Reply, String> {
            match request {
                Request::FormatSelection(areas) => Ok(Reply::FormatSelection(
                    areas
                        .into_iter()
                        .map(area)
                        .collect::<Result<Vec<_>, _>>()
                        .and_then(|areas| crate::format::selection(&self.book, areas)),
                )),
                Request::PreviewFormat {
                    at,
                    revision,
                    patch,
                } => Ok(Reply::FormatPreview(
                    if revision != self.book.host().revision().get() {
                        Err("The workbook changed; reopen Format Cells".into())
                    } else {
                        cell(at).and_then(|at| crate::format::preview(&self.book, at, patch))
                    },
                )),
                Request::Summary => Ok(Reply::Summary(self.summary())),
                Request::Command(_) => Err("Nested command envelope".into()),
                Request::Find(options) => {
                    Ok(Reply::Found(crate::search::find(&self.book, &options)))
                }
                Request::SelectionStats => Ok(Reply::SelectionStats(
                    crate::search::selection_stats(&self.book, self.book.session().selection()),
                )),
                Request::CanEdit(key) => Ok(Reply::CanEdit(
                    cell(key).and_then(|at| calc_host::can_edit(self.book.host(), at)),
                )),
                Request::FitContents { area: target } => self.fit_contents(area(target)?),
                Request::Capture { area: source, keep } => {
                    let from = area(source)?;
                    let snapshot = calc_host::CopySnapshot::capture(self.book.host(), from)
                        .map_err(|e| e.to_string())?;
                    let token = self.next_clip;
                    self.next_clip += 1;
                    let cells: Vec<_> = (from.first().row()..=from.last().row())
                        .flat_map(|row| {
                            (from.first().column()..=from.last().column())
                                .map(move |col| CellKey::new(from.sheet(), row, col).unwrap())
                        })
                        .collect();
                    let page = calc_host::project(
                        self.book.host(),
                        &calc_host::ProjectionRequest { cells },
                    )
                    .map_err(|e| e.to_string())?;
                    let rows: Vec<Vec<_>> = page
                        .cells()
                        .chunks(from.column_count() as usize)
                        .map(|row| row.iter().map(|cell| self.book.shown(cell).text).collect())
                        .collect();
                    let text = dna_calc_model::clipboard::encode_tsv(&rows);
                    let html = dna_calc_model::clipboard::encode_html(
                        &rows,
                        token,
                        &self.clipboard_session,
                    );
                    self.snapshots.retain(|id, _| Some(*id) == keep);
                    self.snapshots.insert(token, snapshot);
                    Ok(Reply::Clipboard(wire::Clipboard {
                        token,
                        session: self.clipboard_session.clone(),
                        area: source,
                        revision: self.book.host().revision().get(),
                        text,
                        html,
                    }))
                }
                Request::PreparePaste { source, to, mode } => {
                    self.prepare_paste(source, area(to)?, mode)
                }
                Request::Ping => Ok(Reply::Pong {
                    version: env!("CARGO_PKG_VERSION").to_owned(),
                }),
                Request::New => {
                    self.install(Workbook::new_blank());
                    Ok(self.done(true, None, 0.0))
                }
                Request::Open { name } => {
                    let started = crate::services::now_ms();
                    let bytes = bytes.unwrap_or_default();
                    crate::workbook::reserve_for_open(bytes.len());
                    match Workbook::open_with_budget(&name, bytes, self.calculation_budget) {
                        Ok(book) => {
                            let elapsed = crate::services::now_ms() - started;
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
                Request::LoadVbaModule {
                    name,
                    source,
                    max_steps,
                } => {
                    self.book.load_vba_module(&name, &source, max_steps)?;
                    self.collect();
                    Ok(self.done(true, None, 0.0))
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
                Request::Subscribe {
                    workbook,
                    id,
                    interest,
                } => {
                    if workbook != self.workbook {
                        return Err("This subscription belongs to a replaced workbook".into());
                    }
                    if self.last_subscription.is_some_and(|last| id <= last) {
                        return Err(
                            "Subscription identities must increase and cannot be reused".into()
                        );
                    }
                    if self.subscriptions.contains_key(&id) {
                        return Err("Subscription identity is already in use".into());
                    }
                    if self.subscriptions.len() >= wire::MAX_SUBSCRIPTIONS {
                        return Err("Too many tile subscriptions".into());
                    }
                    let interest = host_interest(interest)?;
                    match self.book.subscribe(interest.clone()) {
                        Ok(host) => {
                            self.last_subscription = Some(id);
                            self.subscription_interests.insert(id, interest);
                            self.subscriptions.insert(id, host);
                            self.tile_streams.insert(id, Default::default());
                            Ok(Reply::Subscribed)
                        }
                        Err(error) => Err(error.to_string()),
                    }
                }
                Request::SetInterest {
                    workbook,
                    id,
                    interest,
                } => {
                    if workbook != self.workbook {
                        return Err("This interest belongs to a replaced workbook".into());
                    }
                    let interest = host_interest(interest)?;
                    let subscription =
                        *self.subscriptions.get(&id).ok_or("no such subscription")?;
                    let receipt = self.book.dispatch(SkinIntent::SetInterest {
                        subscription,
                        interest: interest.clone(),
                    });
                    if let Some(error) = receipt.error {
                        return Err(error.to_string());
                    }
                    self.subscription_interests.insert(id, interest);
                    Ok(Reply::Subscribed)
                }
                Request::TileAck(barrier) => {
                    if barrier.workbook == self.workbook {
                        if let Some(stream) = self.tile_streams.get_mut(&barrier.id) {
                            stream.acknowledge(barrier);
                        }
                    }
                    Ok(Reply::Subscribed)
                }
                Request::ResetTiles { workbook, id } => {
                    if workbook != self.workbook {
                        return Err("This reset belongs to a replaced workbook".into());
                    }
                    let failed = self
                        .tile_streams
                        .get(&id)
                        .ok_or("No such tile subscription")?
                        .failed();
                    self.retire_tile_events(id);
                    if failed {
                        // A budget failure discarded the retained root. Only a host snapshot
                        // can repair it; later deltas cannot reconstruct untouched cells.
                        let subscription =
                            *self.subscriptions.get(&id).ok_or("No such subscription")?;
                        let interest = self
                            .subscription_interests
                            .get(&id)
                            .ok_or("No such interest")?
                            .clone();
                        let receipt = self.book.dispatch(SkinIntent::SetInterest {
                            subscription,
                            interest,
                        });
                        if let Some(error) = receipt.error {
                            return Err(error.to_string());
                        }
                    }
                    self.tile_streams.get_mut(&id).unwrap().reset();
                    Ok(Reply::Subscribed)
                }
                Request::Unsubscribe { workbook, id } => {
                    if workbook != self.workbook {
                        return Ok(Reply::Subscribed);
                    }
                    self.retire_tile_events(id);
                    self.subscription_interests.remove(&id);
                    self.tile_streams.remove(&id);
                    if let Some(host) = self.subscriptions.remove(&id) {
                        self.book.unsubscribe(host);
                    }
                    Ok(Reply::Subscribed)
                }
                Request::Geometry(sheet) => Ok(Reply::Geometry(self.geometry(sheet))),
                Request::UsedArea(sheet) => Ok(Reply::UsedArea(
                    self.book.used_area(sheet.id()).map(Area::of),
                )),
                Request::ResolveReference { anchor, text } => {
                    let at = cell(anchor)?;
                    let resolved = match self.book.host().resolve_reference_text(at, text.trim()) {
                        calc_host::ReferenceTarget::Area(area) => {
                            if self
                                .book
                                .host()
                                .model()
                                .sheet_by_id(area.sheet())
                                .is_some_and(|s| {
                                    s.visibility() != calc_host::SheetVisibility::Visible
                                })
                            {
                                Err("Unhide this worksheet before navigating to it".to_owned())
                            } else {
                                Ok(Area::of(area))
                            }
                        }
                        calc_host::ReferenceTarget::Invalid => Err(format!(
                            "{text:?} is not a valid address, range or reference name"
                        )),
                        calc_host::ReferenceTarget::NotModelled => Err(format!(
                            "The reference {text:?} is not modelled; use an explicit sheet address or range"
                        )),
                    };
                    Ok(Reply::ResolvedReference(resolved))
                }
                Request::Review(at) => Ok(Reply::Review(self.review(cell(at)?))),
                Request::ReviewMarkers(sheet) => {
                    let review = self.book.host().sheet_review(sheet.id());
                    let mut markers: BTreeMap<Key, String> = BTreeMap::new();
                    for note in review.notes {
                        markers.insert(Key::of(note.at), "Note".into());
                    }
                    for link in review.links {
                        markers
                            .entry(Key::of(link.area.first()))
                            .and_modify(|s| s.push_str(" and link"))
                            .or_insert_with(|| "Link".into());
                    }
                    Ok(Reply::ReviewMarkers(
                        markers.into_iter().take(5000).collect(),
                    ))
                }
                Request::Charts(sheet) => Ok(Reply::Charts(self.charts(sheet))),
                Request::AtlasRefusals { reason, page } => Ok(Reply::AtlasRefusals(
                    self.book
                        .refusal_cells(&reason, usize::try_from(page).unwrap_or(usize::MAX)),
                )),
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
                        memory: crate::services::memory_bytes(),
                        census,
                    }))
                }
                Request::Outline {
                    shown,
                    open,
                    reveal,
                    query,
                } => Ok(Reply::Outline(self.outline(
                    &shown,
                    &open,
                    reveal.and_then(|k| k.cell()),
                    &query,
                ))),
                Request::Notebook {
                    sheet,
                    page,
                    reveal,
                    placement,
                } => Ok(Reply::Notebook(self.notebook(
                    sheet,
                    usize::try_from(page).unwrap_or(usize::MAX),
                    reveal.and_then(|k| k.cell()),
                    placement.as_deref(),
                ))),
                Request::BlockDetail(target) => {
                    Ok(Reply::BlockDetail(self.block_detail(area(target)?)))
                }
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
    fn install(&mut self, mut book: Workbook) {
        let interrupted = self.book.has_pending_calculation();
        if interrupted {
            self.book.cancel_calculation();
        }
        self.collect();
        self.release_calculation_events(true);
        book.set_calculation_budget(self.calculation_budget);
        self.validation_pending = None;
        self.workbook = self
            .workbook
            .checked_add(1)
            .expect("workbook incarnations exhausted");
        self.book = book;
        self.subscriptions.clear();
        self.subscription_interests.clear();
        self.last_subscription = None;
        self.events
            .retain(|event| matches!(event, Event::Calculation { .. }));
        self.tile_streams.clear();
        self.snapshots.clear();
        self.paste_plan = None;
    }

    fn prepare_paste(
        &mut self,
        source: wire::PasteSource,
        to: GridArea,
        mode: wire::PasteMode,
    ) -> Result<Reply, String> {
        use wire::PasteSource;
        self.paste_plan = None;
        let source = match source {
            PasteSource::Internal { token, cut } => calc_host::PasteSource::Snapshot {
                snapshot: self
                    .snapshots
                    .get(&token)
                    .ok_or("The copied snapshot expired; copy again")?,
                cut,
            },
            PasteSource::Fill { from, down, series } => calc_host::PasteSource::Fill {
                from: area(from)?,
                down,
                series,
            },
            PasteSource::External(rows) => calc_host::PasteSource::External(rows),
        };
        let plan =
            calc_host::prepare_paste(self.book.host(), source, to, wire::host_paste_mode(mode))?;
        let target = plan.target();
        let replaces = plan.replaces();
        let id = self.next_clip;
        self.next_clip += 1;
        self.paste_plan = Some((id, plan));
        Ok(Reply::PastePreview(wire::PastePreview {
            id,
            area: Area::of(target),
            replaces,
        }))
    }

    fn summary(&self) -> Summary {
        let iteration = self.book.host().model().definitions().iteration();
        let circular = self.book.host().circular_cells();
        Summary {
            workbook: self.workbook,
            session: self.clipboard_session.clone(),
            initial_cell: self
                .book
                .host()
                .sheets()
                .iter()
                .find(|s| s.visibility() == calc_host::SheetVisibility::Visible)
                .and_then(|s| {
                    let (row, col) = self.book.host().sheet_view(s.id())?.active_cell?;
                    CellKey::new(s.id(), row, col).ok().map(Key::of)
                }),
            name: self.book.name().to_owned(),
            changed: self.book.changed(),
            undo: self.book.session().history_depth().0,
            redo: self.book.session().history_depth().1,
            sheets: self
                .book
                .host()
                .sheets()
                .iter()
                .filter(|s| s.visibility() == calc_host::SheetVisibility::Visible)
                .map(|s| (Sheet::of(s.id()), s.name().text().to_string_lossy()))
                .collect(),
            sheet_order: self
                .book
                .host()
                .sheets()
                .iter()
                .map(|s| Sheet::of(s.id()))
                .collect(),
            hidden_sheets: self
                .book
                .host()
                .sheets()
                .iter()
                .filter(|s| s.visibility() != calc_host::SheetVisibility::Visible)
                .map(|s| {
                    (
                        Sheet::of(s.id()),
                        s.name().text().to_string_lossy(),
                        s.visibility() == calc_host::SheetVisibility::VeryHidden,
                    )
                })
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
            notebook_placement: None,
            validation: None,
            accepted,
            committed: false,
            recalculated: false,
            error,
            recalc: None,
            note: None,
            elapsed_ms,
            selection: selection.active.map(|at| {
                (
                    Key::of(at),
                    selection.areas.iter().copied().map(Area::of).collect(),
                )
            }),
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

    fn fit_contents(&self, target: GridArea) -> Result<Reply, String> {
        let host = self.book.host();
        let metrics = dna_calc_model::geometry::sheet_metrics(
            host.sheet_geometry(target.sheet())
                .as_ref()
                .map(wire::geometry_of)
                .as_ref(),
        );
        let keys = calc_host::autofit_cells(host, target, |at| {
            metrics.width(at.column()) != 0. && metrics.height(at.row()) != 0.
        })?;
        let projected = calc_host::project(host, &calc_host::ProjectionRequest { cells: keys })
            .map_err(|e| e.to_string())?;
        let mut cells = Vec::new();
        for p in projected.cells() {
            let shown = self.book.shown(p);
            if let Some(reason) = &shown.style.unsupported {
                return Err(reason.clone());
            }
            cells.push(wire::cell_of(
                &shown,
                cells::input_text(p.input()),
                cells::provenance_text(host, p),
                p,
            ));
        }
        Ok(Reply::FitContents {
            revision: host.revision().get(),
            cells,
        })
    }

    fn review(&self, at: CellKey) -> wire::Review {
        let host = self.book.host();
        let source = host.model().definitions().conditional_rules(at.sheet());
        let explanations: BTreeMap<_, _> = self
            .book
            .conditional_formats(at.sheet())
            .explain(host, at)
            .into_iter()
            .collect();
        let mut rules = source
            .iter()
            .enumerate()
            .filter_map(|(index, s)| {
                let r = calc_host::conditional_rule(s).ok()?;
                Some(wire::Rule {
                    index: index as u32,
                    kind: r.kind.clone(),
                    priority: s.priority,
                    applies: s
                        .areas
                        .iter()
                        .map(|a| {
                            format!(
                                "{}:{}",
                                dna_formula_ui::display::column_letters(a.first().column())
                                    + &a.first().row().to_string(),
                                dna_formula_ui::display::column_letters(a.last().column())
                                    + &a.last().row().to_string()
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(" "),
                    formulas: s.formulas.clone(),
                    stop: s.stop,
                    state: explanations
                        .get(&index)
                        .cloned()
                        .unwrap_or_else(|| "Rule unavailable".into()),
                    editable: r.extension_id.is_none(),
                    formula_editable: matches!(r.kind.as_str(), "expression" | "cellIs"),
                    visual: matches!(r.kind.as_str(), "colorScale" | "dataBar" | "iconSet"),
                })
            })
            .collect::<Vec<_>>();
        rules.sort_by_key(|r| r.priority);
        let review = host.sheet_review(at.sheet());
        let count = review.notes.len() + review.links.len();
        let mut unsupported = review.unsupported;
        if host
            .loaded()
            .sheet(at.sheet())
            .is_some_and(|s| s.conditional_formats().iter().any(|b| b.unread_container))
        {
            unsupported.push("Additional rule metadata is preserved; conditional-rule editing is unavailable on this worksheet (dc-971p.8)".into());
        }

        if count > 5000 {
            unsupported.push(
                "Only the first 5000 review markers are drawn; cell details remain available here"
                    .into(),
            );
        }
        let validation=host.validation(at).map(|v|format!("{} validation · {:?}. Custom rules and formula-entry result checks are unavailable (dc-6yw); loaded rules are preserved.",v.kind,v.style));
        wire::Review {
            at: Some(Key::of(at)),
            revision: host.revision().get(),
            rules,
            notes: review
                .notes
                .into_iter()
                .filter(|n| n.at == at)
                .map(|n| (n.author, n.text))
                .collect(),
            links: review
                .links
                .into_iter()
                .filter(|l| l.area.contains(at))
                .map(|l| wire::ReviewLink {
                    display: l.display.clone().unwrap_or_else(|| {
                        l.target
                            .clone()
                            .or_else(|| l.location.clone())
                            .unwrap_or_else(|| "Unavailable link".into())
                    }),
                    target: l.target,
                    location: l.location,
                    tooltip: l.tooltip.unwrap_or_default(),
                })
                .collect(),
            unsupported,
            validation,
        }
    }

    fn intent(&mut self, intent: Intent) -> Result<Reply, String> {
        let intent = match intent {
            Intent::FormatCells {
                areas,
                revision,
                patch,
            } => {
                if revision != self.book.host().revision().get() {
                    return Err("The workbook changed; reopen Format Cells".into());
                }
                SkinIntent::Command(calc_host::WorkbookCommand::FormatCells {
                    areas: areas.into_iter().map(area).collect::<Result<Vec<_>, _>>()?,
                    patch: crate::format::host_patch(patch),
                })
            }
            Intent::AcceptValidation { token } => {
                let (held, revision, selection, intent) = self
                    .validation_pending
                    .take()
                    .ok_or("This validation decision has expired")?;
                if held != token
                    || revision != self.book.host().revision().get()
                    || &selection != self.book.session().selection()
                {
                    return Err("The workbook or selection changed; enter the draft again".into());
                }
                return self.dispatch_entry(intent, true);
            }
            Intent::NotebookLink {
                anchor,
                prose,
                text,
            } => return self.notebook_link(anchor, prose, text),
            Intent::NotebookOrder {
                placement,
                direction,
            } => return self.notebook_order(placement, direction),

            Intent::Replace {
                options,
                replacement,
                revision,
                only,
            } => SkinIntent::Command(calc_host::WorkbookCommand::Replace {
                options: crate::search::host_options(&options)?,
                replacement,
                revision,
                only: only.map(cell).transpose()?,
            }),
            Intent::EditConditional {
                sheet,
                revision,
                index,
                change,
            } => SkinIntent::Command(calc_host::WorkbookCommand::Conditional {
                sheet: sheet.id(),
                revision,
                index,
                change: conditional_change(change),
            }),
            Intent::ShiftCells {
                area: a,
                down,
                insert,
            } => SkinIntent::Command(calc_host::WorkbookCommand::ShiftCells {
                area: area(a)?,
                down,
                insert,
            }),
            Intent::EditSheet { sheet, change } => {
                SkinIntent::Command(calc_host::WorkbookCommand::Sheet {
                    sheet: sheet.map(Sheet::id),
                    change: sheet_change(change),
                })
            }
            Intent::FitContents {
                sheet,
                rows,
                revision,
                sizes,
            } => SkinIntent::Command(calc_host::WorkbookCommand::FitContents {
                sheet: sheet.id(),
                rows,
                revision,
                sizes,
            }),
            Intent::AxisFormat {
                sheet,
                rows,
                first,
                count,
                size_bits,
                hidden,
            } => SkinIntent::Command(calc_host::WorkbookCommand::AxisFormat {
                sheet: sheet.id(),
                rows,
                first,
                count,
                size_bits,
                hidden,
            }),
            Intent::FreezePanes {
                sheet,
                rows,
                columns,
            } => SkinIntent::Command(calc_host::WorkbookCommand::FreezePanes {
                sheet: sheet.id(),
                rows,
                columns,
            }),
            Intent::ApplyPaste { id } => {
                let Some((held, plan)) = self.paste_plan.take() else {
                    return Err("The paste preview expired; paste again".into());
                };
                if held != id {
                    return Err("The workbook changed after the preview; paste again".into());
                }
                SkinIntent::Command(calc_host::WorkbookCommand::Paste(plan))
            }
            Intent::Navigate {
                nav,
                extend,
                page_height,
            } => {
                let selection = self.book.session().selection();
                let at = selection.active.ok_or("no active cell")?;
                if matches!(
                    nav,
                    dna_calc_model::selection::Navigation::Column
                        | dna_calc_model::selection::Navigation::Row
                ) && selection.areas.len() > 1
                {
                    return Ok(self.done(false,Some("Axis expansion for disjoint selections is not modelled; select one rectangle".to_owned()),0.0));
                }
                let merges = self.book.host().merged_areas(at.sheet());
                let active_merge = merges.iter().copied().find(|a| a.contains(at));
                if extend
                    && active_merge.is_none()
                    && selection.areas.first().is_some_and(|area| {
                        (at.row() != area.first().row() && at.row() != area.last().row())
                            || (at.column() != area.first().column()
                                && at.column() != area.last().column())
                    })
                {
                    return Ok(self.done(false,Some("Range extension from an interior active cell awaits Excel measurement; select an anchor corner first".to_owned()),0.0));
                }
                let host = self.book.host();
                if matches!(
                    nav,
                    dna_calc_model::selection::Navigation::All
                        | dna_calc_model::selection::Navigation::Column
                        | dna_calc_model::selection::Navigation::Row
                ) && host
                    .model()
                    .definitions()
                    .tables()
                    .iter()
                    .any(|table| table.area.contains(at))
                {
                    return Ok(self.done(false,Some("Table selection stages are not modelled; use the name box to select an explicit range".to_owned()),0.0));
                }
                let occupied = if matches!(
                    nav,
                    dna_calc_model::selection::Navigation::Edge(_)
                        | dna_calc_model::selection::Navigation::All
                ) {
                    host.occupied_areas(at.sheet())
                } else {
                    Vec::new()
                };
                let mut used = if matches!(nav, dna_calc_model::selection::Navigation::UsedEnd) {
                    host.used_area(at.sheet())
                } else {
                    None
                };
                if matches!(nav, dna_calc_model::selection::Navigation::UsedEnd) {
                    for (key, _) in host
                        .model()
                        .definitions()
                        .cell_formats()
                        .filter(|(key, _)| key.sheet() == at.sheet())
                    {
                        used = Some(match used {
                            None => GridArea::cell(key),
                            Some(area) => GridArea::new(
                                CellKey::new(
                                    at.sheet(),
                                    area.first().row().min(key.row()),
                                    area.first().column().min(key.column()),
                                )
                                .expect("bounded format"),
                                CellKey::new(
                                    at.sheet(),
                                    area.last().row().max(key.row()),
                                    area.last().column().max(key.column()),
                                )
                                .expect("bounded format"),
                            )
                            .expect("one sheet"),
                        });
                    }
                }
                let metrics = dna_calc_model::geometry::sheet_metrics(
                    host.sheet_geometry(at.sheet())
                        .as_ref()
                        .map(wire::geometry_of)
                        .as_ref(),
                );
                let mut selection = selection.clone();
                let mut nav = nav;
                if !extend && let Some(merged) = merges.iter().copied().find(|a| a.contains(at)) {
                    use dna_calc_model::selection::{NavAction, Navigation};
                    if let Navigation::Walk {
                        horizontal,
                        reverse,
                    } = nav
                        && selection.areas.as_slice() == [merged]
                    {
                        nav = Navigation::Step(match (horizontal, reverse) {
                            (true, false) => NavAction::Right,
                            (true, true) => NavAction::Left,
                            (false, false) => NavAction::Down,
                            (false, true) => NavAction::Up,
                        });
                    }
                    let edge = match nav {
                        Navigation::Step(NavAction::Down | NavAction::CommitDown) => {
                            Some((merged.last().row(), merged.first().column()))
                        }
                        Navigation::Step(
                            NavAction::Right | NavAction::NextColumn | NavAction::CommitRight,
                        ) => Some((merged.first().row(), merged.last().column())),
                        Navigation::Step(
                            NavAction::Up | NavAction::Left | NavAction::PrevColumn,
                        ) => Some((merged.first().row(), merged.first().column())),
                        _ => None,
                    };
                    if let Some((row, col)) = edge {
                        selection.active =
                            Some(CellKey::new(at.sheet(), row, col).expect("merge edge"));
                    }
                }
                let mut next = if extend
                    && active_merge.is_some()
                    && !matches!(
                        nav,
                        dna_calc_model::selection::Navigation::Row
                            | dna_calc_model::selection::Navigation::Column
                            | dna_calc_model::selection::Navigation::All
                    ) {
                    dna_calc_model::selection::extend_merged(
                        &selection,
                        active_merge.unwrap(),
                        nav,
                        &metrics,
                        f64::from(page_height),
                        &occupied,
                        used,
                    )
                } else {
                    dna_calc_model::selection::navigate(
                        &selection,
                        nav,
                        extend,
                        &metrics,
                        f64::from(page_height),
                        &occupied,
                        used,
                    )
                };
                // Walk visits each merged identity once within a larger selected rectangle.
                // Jump to the covered run's edge instead of snapping back to its anchor.
                if !extend
                    && let dna_calc_model::selection::Navigation::Walk {
                        horizontal,
                        reverse,
                    } = nav
                {
                    let mut visited = std::collections::BTreeSet::new();
                    while let Some(target) = next.active {
                        let Some(merge) = merges
                            .iter()
                            .find(|a| a.contains(target) && a.first() != target)
                        else {
                            break;
                        };
                        if !visited.insert(target) {
                            return Err("Merged selection navigation did not advance".into());
                        }
                        let area = next
                            .areas
                            .iter()
                            .find(|a| a.contains(target))
                            .copied()
                            .unwrap_or_else(|| GridArea::cell(target));
                        let full_line = if horizontal {
                            area.first().column() == merge.first().column()
                                && area.last().column() == merge.last().column()
                        } else {
                            area.first().row() == merge.first().row()
                                && area.last().row() == merge.last().row()
                        };
                        if reverse
                            && (full_line
                                || if horizontal {
                                    target.row() == merge.first().row()
                                } else {
                                    target.column() == merge.first().column()
                                })
                        {
                            next.active = Some(merge.first());
                            break;
                        }
                        let edge = if full_line {
                            merge.last()
                        } else if horizontal {
                            CellKey::new(
                                target.sheet(),
                                target.row(),
                                if reverse {
                                    merge.first().column()
                                } else {
                                    merge.last().column()
                                },
                            )
                            .expect("merged walk column")
                        } else {
                            CellKey::new(
                                target.sheet(),
                                if reverse {
                                    merge.first().row()
                                } else {
                                    merge.last().row()
                                },
                                target.column(),
                            )
                            .expect("merged walk row")
                        };
                        next.active = Some(edge);
                        next = dna_calc_model::selection::navigate(
                            &next,
                            nav,
                            false,
                            &metrics,
                            f64::from(page_height),
                            &occupied,
                            used,
                        );
                    }
                }
                SkinIntent::Select(next)
            }
            Intent::EnterCell { at, text } => SkinIntent::EnterCell {
                at: cell(at)?,
                text,
            },
            Intent::EnterSelection { text } => SkinIntent::EnterSelection { text },
            Intent::Clear { areas } => SkinIntent::Edit {
                edits: areas
                    .into_iter()
                    .map(|a| Ok(CellEdit::Clear { area: area(a)? }))
                    .collect::<Result<_, String>>()?,
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
            Intent::Calculate { scope, selection } => SkinIntent::Calculate {
                scope: match scope {
                    wire::CalculationScope::Workbook => calc_host::CalcScope::Workbook,
                    wire::CalculationScope::Sheet(sheet) => calc_host::CalcScope::Sheet(sheet.id()),
                    wire::CalculationScope::Range(areas) => calc_host::CalcScope::Range(
                        areas
                            .into_iter()
                            .map(|area| area.area().ok_or("The calculation range is invalid"))
                            .collect::<Result<Vec<_>, _>>()?,
                    ),
                },
                selection: match selection {
                    wire::CalculationSelection::DirtyAndVolatile => {
                        calc_host::EvalSelection::DirtyAndVolatile
                    }
                    wire::CalculationSelection::ForceAll => calc_host::EvalSelection::ForceAll,
                },
            },
            Intent::CalculateRange { areas } => SkinIntent::CalculateRange {
                areas: areas
                    .into_iter()
                    .map(|area| area.area().ok_or("The calculation range is invalid"))
                    .collect::<Result<Vec<_>, _>>()?,
            },
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
            Intent::Select { active, areas } => SkinIntent::Select(Selection {
                active: Some(cell(active)?),
                areas: areas.into_iter().map(area).collect::<Result<_, _>>()?,
            }),
        };
        let intent = if let SkinIntent::Select(mut selection) = intent {
            if let Some(at) = selection.active {
                let merges = self.book.host().merged_areas(at.sheet());
                if let Some(merged) = merges.iter().find(|a| a.contains(at)) {
                    selection.active = Some(merged.first());
                }
                for area in &mut selection.areas {
                    loop {
                        let before = *area;
                        for merge in &merges {
                            if area.intersection(*merge).is_some() {
                                *area = GridArea::new(
                                    CellKey::new(
                                        area.sheet(),
                                        area.first().row().min(merge.first().row()),
                                        area.first().column().min(merge.first().column()),
                                    )
                                    .expect("merge start"),
                                    CellKey::new(
                                        area.sheet(),
                                        area.last().row().max(merge.last().row()),
                                        area.last().column().max(merge.last().column()),
                                    )
                                    .expect("merge end"),
                                )
                                .expect("one sheet");
                            }
                        }
                        if *area == before {
                            break;
                        }
                    }
                }
            }
            SkinIntent::Select(selection)
        } else {
            intent
        };
        self.dispatch_entry(intent, false)
    }

    fn dispatch_entry(&mut self, intent: SkinIntent, confirmed: bool) -> Result<Reply, String> {
        if !confirmed {
            self.validation_pending = None;
            let targets = match &intent {
                SkinIntent::EnterCell { at, text }
                | SkinIntent::EnterCellAndLayout { at, text, .. } => vec![(*at, text.as_str())],
                SkinIntent::EnterSelection { text } => {
                    let selection = self.book.session().selection();
                    if !selection.areas.iter().any(|a| {
                        self.book
                            .host()
                            .loaded()
                            .sheet(a.sheet())
                            .is_some_and(|s| !s.validations().is_empty())
                    }) {
                        Vec::new()
                    } else {
                        if selection.areas.iter().map(|a| a.cell_count()).sum::<u64>() > 10000 {
                            return Ok(self.done(
                                false,
                                Some("Validate at most 10000 selected cells at once".into()),
                                0.0,
                            ));
                        }
                        selection
                            .areas
                            .iter()
                            .flat_map(|a| {
                                (a.first().row()..=a.last().row()).flat_map(move |r| {
                                    (a.first().column()..=a.last().column())
                                        .map(move |c| CellKey::new(a.sheet(), r, c).unwrap())
                                })
                            })
                            .map(|at| (at, text.as_str()))
                            .collect()
                    }
                }
                _ => Vec::new(),
            };
            let mut failures = targets
                .iter()
                .filter_map(|(at, text)| self.book.validation_failure(*at, text))
                .collect::<Vec<_>>();
            failures.sort_by_key(|f| match f.style {
                calc_host::ErrorStyle::Stop => 0,
                calc_host::ErrorStyle::Warning => 1,
                _ => 2,
            });
            if let Some(failure) = failures.first() {
                let token = self.next_clip;
                self.next_clip += 1;
                let style = match failure.style {
                    calc_host::ErrorStyle::Stop => "Stop",
                    calc_host::ErrorStyle::Warning => "Warning",
                    _ => "Information",
                };
                let message = if failures.len() > 1 {
                    format!(
                        "{} ({} selected cells reject this entry)",
                        failure.message,
                        failures.len()
                    )
                } else {
                    failure.message.clone()
                };
                let decision = wire::ValidationDecision {
                    token,
                    style: style.into(),
                    title: failure.title.clone(),
                    message,
                };
                if failure.style != calc_host::ErrorStyle::Stop {
                    self.validation_pending = Some((
                        token,
                        self.book.host().revision().get(),
                        self.book.session().selection().clone(),
                        intent,
                    ));
                }
                let mut outcome = self.outcome(false, Some(failure.to_string()), 0.0);
                outcome.validation = Some(decision);
                return Ok(Reply::Done {
                    outcome,
                    opened: None,
                    saved: None,
                });
            }
        }
        // What a warning or information rule says about an entry, worked out before it is taken.
        let note = match &intent {
            SkinIntent::EnterCell { at, text }
            | SkinIntent::EnterCellAndLayout { at, text, .. } => {
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
        let locate = |edit: &calc_host::SkinLayoutEdit| match edit {
            calc_host::SkinLayoutEdit::AddPlacement {
                placement,
                spot: calc_host::Spot::Notebook { .. },
                ..
            } => Some(placement.to_string()),
            _ => None,
        };
        let notebook_placement = match &intent {
            SkinIntent::EnterCellAndLayout { layout, .. } => layout.iter().find_map(locate),
            SkinIntent::Edit { edits } => edits.iter().find_map(|e| {
                if let CellEdit::Layout(edit) = e {
                    locate(edit)
                } else {
                    None
                }
            }),
            _ => None,
        };
        // Preserve the request's response shape while the host owns all command rules.
        let rule_error_is_route = matches!(
            &intent,
            SkinIntent::Command(
                calc_host::WorkbookCommand::Sheet { .. }
                    | calc_host::WorkbookCommand::FitContents { .. }
                    | calc_host::WorkbookCommand::AxisFormat { .. }
                    | calc_host::WorkbookCommand::FreezePanes { .. }
                    | calc_host::WorkbookCommand::Paste(_)
                    | calc_host::WorkbookCommand::ShiftCells { .. }
            )
        );
        let started = crate::services::now_ms();
        let receipt = self.book.dispatch(intent);
        if rule_error_is_route
            && let Some(calc_host::IntentError::Workbook(reason)) = &receipt.error
        {
            return Err(reason.clone());
        }
        let elapsed = crate::services::now_ms() - started;
        let mut outcome = self.outcome(
            receipt.accepted(),
            receipt.error.as_ref().map(ToString::to_string),
            elapsed,
        );
        outcome.notebook_placement = notebook_placement.filter(|_| receipt.accepted());
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

    fn push_calculation_event(&mut self, event: calc_host::CalcEvent, terminal: bool) {
        use calc_host::CalcEvent as E;
        let scope = |scope: calc_host::CalcScope| match scope {
            calc_host::CalcScope::Workbook => wire::CalculationScope::Workbook,
            calc_host::CalcScope::Sheet(sheet) => wire::CalculationScope::Sheet(Sheet::of(sheet)),
            calc_host::CalcScope::Range(areas) => {
                wire::CalculationScope::Range(areas.into_iter().map(Area::of).collect())
            }
        };
        let receipt = match event {
            E::AwaitingAsync(token) => wire::CalculationEvent::AwaitingAsync {
                domain: token.invocation.pass.command.domain_incarnation,
                command: token.invocation.pass.command.sequence,
                pass: token.invocation.pass.ordinal,
                token: token.id,
                at: Key::of(token.target.locus),
                request: token.request,
            },
            E::AsyncReturned { token, disposition } => wire::CalculationEvent::AsyncReturned {
                token,
                disposition: match disposition {
                    calc_host::AsyncDisposition::Accepted => wire::AsyncReturnDisposition::Accepted,
                    calc_host::AsyncDisposition::Duplicate => {
                        wire::AsyncReturnDisposition::Duplicate
                    }
                    calc_host::AsyncDisposition::Rejected(reason) => {
                        wire::AsyncReturnDisposition::Rejected(format!("{reason:?}"))
                    }
                },
            },
            E::Committed { revision } => wire::CalculationEvent::Committed {
                revision: revision.get(),
            },
            E::Admitted(r) => wire::CalculationEvent::Admitted {
                domain: r.command.domain_incarnation,
                command: r.command.sequence,
                pass: r.pass.ordinal,
                revision: r.admitted.model.get(),
                scope: scope(r.scope),
                selection: match r.selection {
                    calc_host::EvalSelection::DirtyAndVolatile => {
                        wire::CalculationSelection::DirtyAndVolatile
                    }
                    calc_host::EvalSelection::ForceAll => wire::CalculationSelection::ForceAll,
                },
                range_post_state: match r.range_post_state {
                    calc_host::RangePostState::Satisfied => {
                        wire::CalculationRangePostState::Satisfied
                    }
                    calc_host::RangePostState::StillRequired => {
                        wire::CalculationRangePostState::StillRequired
                    }
                },
                predecessor: r.predecessor.map(|id| id.sequence),
            },
            E::Yielded { pass, remaining } => wire::CalculationEvent::Yielded {
                domain: pass.command.domain_incarnation,
                command: pass.command.sequence,
                pass: pass.ordinal,
                remaining,
            },
            E::Superseded(r) => {
                let by = match r.outcome {
                    calc_host::CompletionOutcome::Superseded { by } => by.sequence,
                    _ => unreachable!("a supersession receipt names its successor"),
                };
                wire::CalculationEvent::Superseded {
                    domain: r.command.domain_incarnation,
                    command: r.command.sequence,
                    by,
                    revision: r.admitted.model.get(),
                }
            }
            E::Reused {
                at,
                computed_by,
                certified_for,
            } => wire::CalculationEvent::Reused {
                at: Key::of(at),
                computed_by: computed_by.sequence,
                certified_for: certified_for.sequence,
            },
            E::Rejected { at } => wire::CalculationEvent::Rejected { at: Key::of(at) },
            E::Published(unit) => wire::CalculationEvent::Published(wire::Publication {
                domain: unit.command.domain_incarnation,
                command: unit.command.sequence,
                sequence: unit.sequence,
            }),
            E::Completed(r) => wire::CalculationEvent::Completed {
                domain: r.command.domain_incarnation,
                command: r.command.sequence,
                revision: r.admitted.model.get(),
                outcome: match r.outcome {
                    calc_host::CompletionOutcome::Complete => wire::CalculationOutcome::Complete,
                    calc_host::CompletionOutcome::Incomplete => {
                        wire::CalculationOutcome::Incomplete
                    }
                    calc_host::CompletionOutcome::Cancelled => wire::CalculationOutcome::Cancelled,
                    calc_host::CompletionOutcome::Failed => wire::CalculationOutcome::Failed,
                    calc_host::CompletionOutcome::Superseded { .. } => {
                        wire::CalculationOutcome::Superseded
                    }
                },
                required: r.coverage.required,
                calculated: r.coverage.calculated,
                refused: r.coverage.refused,
                held: r.coverage.held,
                uncertified: r.coverage.uncertified,
                boundary_cached: r.coverage.boundary_cached,
                imported_cached: r.coverage.imported_cached,
            },
        };
        let event = Event::Calculation {
            workbook: self.workbook,
            receipt,
            summary: None,
            failed_projections: Vec::new(),
        };
        if terminal {
            self.deferred_calculation.push(event);
        } else {
            self.events.push(event);
        }
    }

    fn retire_tile_events(&mut self, id: u64) {
        self.events.retain(|event| match event {
            Event::TileAdmission(stamp) => stamp.id != id,
            Event::Tiles(batch) => batch.stamp.id != id,
            Event::TileCompletion(barrier) => barrier.id != id,
            Event::ProjectionFailed { id: failed, .. } => *failed != id,
            Event::Calculation { .. } => true,
        });
    }

    fn geometry(&self, sheet: Sheet) -> wire::Geometry {
        let mut geometry = self
            .book
            .host()
            .sheet_geometry(sheet.id())
            .map(|g| wire::geometry_of(&g))
            .unwrap_or_default();
        geometry.merges = self
            .book
            .host()
            .merged_areas(sheet.id())
            .into_iter()
            .map(wire::Area::of)
            .collect();
        geometry.edit_refusal = calc_host::can_edit(
            self.book.host(),
            CellKey::new(sheet.id(), 1, 1).expect("sheet origin"),
        )
        .err();
        if let Some(panes) = self
            .book
            .host()
            .model()
            .definitions()
            .frozen_panes(sheet.id())
        {
            geometry.frozen_rows = panes.rows();
            geometry.frozen_columns = panes.columns();
        }
        if let Some(view) = self.book.host().sheet_view(sheet.id()) {
            geometry.top_left = view.top_left;
            geometry.pane_top_left = view.pane_top_left;
            geometry.active_cell = view.active_cell;
            geometry.active_pane = view.active_pane;
            geometry.split = view.split;
            geometry.split_window = view.split_window;
        }
        geometry
    }

    /// Adapt installed host deltas into bounded projection publications. There is no new
    /// evaluation here. Geometry-only revisions also publish, keeping value/style/layout roots
    /// related even when no displayed text changed.
    fn collect(&mut self) {
        let mut terminal = Vec::new();
        for event in self.book.take_calculation_events() {
            if matches!(
                event,
                calc_host::CalcEvent::Published(_) | calc_host::CalcEvent::Completed(_)
            ) {
                terminal.push(event);
            } else {
                self.push_calculation_event(event, false);
            }
        }
        let deltas = self.book.take_deltas();
        let revision = self.book.host().revision().get();
        let geometry: Vec<_> = self
            .book
            .sheets()
            .into_iter()
            .map(|s| {
                let sheet = Sheet::of(s.id);
                (sheet, self.geometry(sheet))
            })
            .collect();
        let ids: BTreeMap<SubscriptionId, u64> =
            self.subscriptions.iter().map(|(&p, &h)| (h, p)).collect();
        for delta in deltas {
            let Some(&id) = ids.get(&delta.subscription) else {
                continue;
            };
            let mut cells = Vec::new();
            let mut removed = Vec::new();
            let mut retained = 0usize;
            let mut oversized = false;
            for projected in &delta.cells {
                if cells.len() + removed.len() >= wire::MAX_PUBLICATION_RECORDS {
                    oversized = true;
                    break;
                }
                let shown = self.book.shown(projected);
                if projected.input().is_none()
                    && projected.value() == &ProjectedValue::Empty
                    && shown.conditional.is_none()
                    && self.book.host().cell_style(projected.at()) == 0
                {
                    retained = retained.saturating_add(16);
                    if retained > wire::MAX_PUBLICATION_BYTES - wire::MAX_TILE_BYTES {
                        oversized = true;
                        break;
                    }
                    removed.push(Key::of(projected.at()));
                    continue;
                }
                let cell = wire::cell_of(
                    &shown,
                    cells::input_text(projected.input()),
                    cells::provenance_text(self.book.host(), projected),
                    projected,
                );
                retained = retained.saturating_add(cell.retained_bytes());
                if cells.len() >= wire::MAX_TILE_CELLS
                    || retained > wire::MAX_PUBLICATION_BYTES - wire::MAX_TILE_BYTES
                {
                    oversized = true;
                    break;
                }
                cells.push(cell);
            }
            if oversized {
                if let Some(stream) = self.tile_streams.get_mut(&id) {
                    stream.reject();
                    stream.revision = revision;
                }
                self.events.push(Event::ProjectionFailed {
                    id,
                    reason: "This projection exceeds the tile budget; narrow its visible area"
                        .into(),
                });
                continue;
            }
            let publication = match &delta.cause {
                calc_host::DeltaCause::Recalculated { publication, .. } => {
                    Some(wire::Publication {
                        domain: publication.command.domain_incarnation,
                        command: publication.command.sequence,
                        sequence: publication.sequence,
                    })
                }
                _ => None,
            };
            if let Some(stream) = self.tile_streams.get_mut(&id) {
                if let Err(reason) = stream.update(
                    delta.reset,
                    cells,
                    removed,
                    revision,
                    geometry.clone(),
                    publication,
                ) {
                    self.events.push(Event::ProjectionFailed { id, reason });
                }
            }
        }
        for (&id, stream) in &mut self.tile_streams {
            if stream.revision != revision {
                if let Err(reason) = stream.update(
                    false,
                    Vec::new(),
                    Vec::new(),
                    revision,
                    geometry.clone(),
                    None,
                ) {
                    self.events.push(Event::ProjectionFailed { id, reason });
                }
            }
            let transaction = self.next_publication;
            self.next_publication = self
                .next_publication
                .checked_add(1)
                .expect("Tile transaction identities exhausted");
            match stream.publish(id, self.workbook, transaction) {
                Ok(events) => self.events.extend(events),
                Err(reason) => self.events.push(Event::ProjectionFailed { id, reason }),
            }
        }
        for event in terminal {
            self.push_calculation_event(event, true);
        }
        if self
            .tile_streams
            .values()
            .all(|stream| !stream.has_pending())
        {
            self.release_calculation_events(false);
        }
    }

    /// Receipt frontiers stay immutable. The summary describes current host state at
    /// delivery, including same-revision changes such as mode switches and successful saves.
    fn release_calculation_events(&mut self, closing: bool) {
        if self.deferred_calculation.is_empty() {
            return;
        }
        let current = self.summary();
        let failed: Vec<_> = self
            .tile_streams
            .iter()
            .filter_map(|(&id, stream)| {
                (stream.failed() || closing && stream.has_pending()).then_some(id)
            })
            .collect();
        for event in &mut self.deferred_calculation {
            if let Event::Calculation {
                summary,
                failed_projections,
                ..
            } = event
            {
                *summary = Some(current.clone());
                *failed_projections = failed.clone();
            }
        }
        self.events.append(&mut self.deferred_calculation);
    }

    fn notebook(
        &self,
        sheet: Sheet,
        requested: usize,
        reveal: Option<CellKey>,
        placement: Option<&str>,
    ) -> wire::Notebook {
        use calc_host::{BlockKind, ViewKind};
        let host = self.book.host();
        let layout = host.model().layout();
        let mut linked = Vec::new();
        let mut placed = Vec::new();
        for placement in layout.view(ViewKind::Notebook) {
            if let Some(block) = layout.block(placement.block()) {
                let anchor = block.anchor();
                if anchor.is_none_or(|a| a.sheet() == sheet.id()) {
                    if let Some(a) = anchor {
                        placed.push(a);
                    }
                    linked.push((
                        Some(placement.id().to_string()),
                        anchor,
                        block.kind() == BlockKind::Prose,
                    ));
                }
            }
        }
        let reveal = reveal.filter(|at| {
            host.occupied_areas(sheet.id())
                .iter()
                .any(|a| a.contains(*at))
        });
        let (keys, omitted) =
            crate::tree::outline_keys(host, sheet.id(), wire::NOTEBOOK_PAGE, reveal);
        linked.extend(
            keys.into_iter()
                .filter(|at| !placed.iter().any(|a| a.contains(*at)))
                .map(|at| (None, Some(GridArea::cell(at)), false)),
        );
        let total = linked.len();
        let pages = total.div_ceil(wire::NOTEBOOK_PAGE).max(1);
        let page = placement
            .and_then(|id| linked.iter().position(|(p, _, _)| p.as_deref() == Some(id)))
            .or_else(|| {
                reveal.and_then(|at| {
                    linked
                        .iter()
                        .position(|(_, a, _)| a.is_some_and(|a| a.contains(at)))
                })
            })
            .map(|i| i / wire::NOTEBOOK_PAGE)
            .unwrap_or(requested)
            .min(pages - 1);
        let blocks = linked
            .into_iter()
            .skip(page * wire::NOTEBOOK_PAGE)
            .take(wire::NOTEBOOK_PAGE)
            .map(|(placement, anchor, prose)| {
                let cell = anchor.and_then(|a| self.book.cell(a.first())).map(|p| {
                    wire::cell_of(
                        &self.book.shown(&p),
                        cells::input_text(p.input()),
                        cells::provenance_text(host, &p),
                        &p,
                    )
                });
                wire::NotebookBlock {
                    placement,
                    anchor: anchor.map(Area::of),
                    prose,
                    cell,
                }
            })
            .collect();
        wire::Notebook {
            total: total as u64,
            page: page as u64,
            omitted,
            blocks,
        }
    }

    fn notebook_link(
        &mut self,
        target: Area,
        prose: bool,
        text: Option<String>,
    ) -> Result<Reply, String> {
        let intent =
            calc_host::prepare_notebook_link(self.book.host(), area(target)?, prose, text)?;
        self.dispatch_entry(intent, false)
    }
    fn notebook_order(&mut self, id: String, direction: i8) -> Result<Reply, String> {
        let id = calc_host::PlacementId::parse(&id).ok_or("Invalid notebook placement")?;
        match calc_host::prepare_notebook_order(self.book.host(), id, direction)? {
            Some(intent) => self.dispatch_entry(intent, false),
            None => Ok(self.done(true, None, 0.)),
        }
    }
    fn block_detail(&self, linked: GridArea) -> wire::BlockDetail {
        let host = self.book.host();
        let at = linked.first();
        let owner = host.spill_owner(at);
        let blocked = host.blocked_spill_area(at);
        let area = blocked
            .or_else(|| owner.and_then(|a| host.spill_area(a)))
            .unwrap_or(linked);
        let (blockers, more) = host.spill_blockers(at, 32);
        let keys = (area.first().row()..=area.last().row())
            .flat_map(|row| {
                (area.first().column()..=area.last().column())
                    .map(move |col| CellKey::new(area.sheet(), row, col).unwrap())
            })
            .take(32)
            .collect::<Vec<_>>();
        let preview = self
            .book
            .project(keys)
            .iter()
            .map(|p| {
                wire::cell_of(
                    &self.book.shown(p),
                    cells::input_text(p.input()),
                    cells::provenance_text(host, p),
                    p,
                )
            })
            .collect::<Vec<_>>();
        let label = cells::area_label(host, area);
        let summary = if blocked.is_some() {
            format!(
                "{} cannot spill into {label}. {} blocking areas shown{}; preview is current worksheet contents.",
                cells::cell_label(host, at),
                blockers.len(),
                if more { " (more not shown)" } else { "" }
            )
        } else if let Some(owner) = owner {
            format!(
                "Array anchor {}; actual range {label}. Showing {} of {} cells.",
                cells::cell_label(host, owner),
                preview.len(),
                area.cell_count()
            )
        } else {
            format!(
                "Worksheet source {label}. Showing {} of {} cells.",
                preview.len(),
                area.cell_count()
            )
        };
        wire::BlockDetail {
            summary,
            anchor: owner.map(Key::of),
            cells: preview,
            blockers: blockers.into_iter().map(Area::of).collect(),
        }
    }

    /// The tree's outline ([`crate::tree::outline_sheet`] per sheet) with the open sheets' shown
    /// cells.
    fn outline(
        &self,
        shown: &[(Sheet, u64)],
        open: &[Sheet],
        reveal: Option<CellKey>,
        query: &str,
    ) -> wire::Outline {
        let query = query.trim().to_lowercase();
        let mut omitted = 0;
        let host = self.book.host();
        let names = host
            .defined_names()
            .iter()
            .filter(|name| {
                !host
                    .model()
                    .layout()
                    .blocks()
                    .iter()
                    .any(|b| b.locator().as_str() == name.name)
            })
            .map(|name| {
                let formula = name.formula.strip_prefix('=').unwrap_or(&name.formula);
                format!("{} = {formula}", name.name)
            })
            .filter(|text| query.is_empty() || text.to_lowercase().contains(&query))
            .collect();
        let mut sheets = Vec::new();
        let mut wanted = Vec::new();
        for tab in self.book.sheets() {
            let sheet = Sheet::of(tab.id);
            let limit = shown
                .iter()
                .find(|(s, _)| *s == sheet)
                .map_or(crate::tree::ROW_PAGE, |(_, n)| *n as usize);
            let (keys, hidden) = crate::tree::outline_keys(host, tab.id, limit, reveal);
            omitted += hidden;
            let keys: Vec<_> = keys
                .into_iter()
                .filter(|at| {
                    query.is_empty()
                        || format!(
                            "{}!{} {}",
                            tab.name,
                            cells::address(at.row(), at.column()),
                            cells::input_text(host.model().cell(*at))
                        )
                        .to_lowercase()
                        .contains(&query)
                })
                .collect();
            if !query.is_empty() && keys.is_empty() {
                continue;
            }
            let mut outline =
                crate::tree::outline_sheet(tab.id, tab.name, keys.iter().copied(), limit);
            if query.is_empty()
                && let Some(at) = reveal.filter(|at| at.sheet() == tab.id)
            {
                if !outline.rows.iter().any(|(row, _)| *row == at.row()) {
                    outline.rows.push((
                        at.row(),
                        keys.iter()
                            .copied()
                            .filter(|key| key.row() == at.row())
                            .collect(),
                    ));
                    outline.rows.sort_by_key(|(row, _)| *row);
                    outline.more_rows = outline.more_rows.saturating_sub(1);
                }
            }
            if open.contains(&sheet) {
                for (_, row) in &outline.rows {
                    wanted.extend(row.iter().take(crate::tree::ROW_CELLS).copied());
                    if let Some(at) = reveal.filter(|at| row.contains(at)) {
                        wanted.push(at);
                    }
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
        wanted.sort();
        wanted.dedup();
        let cells = self
            .book
            .project(wanted)
            .iter()
            .map(|projected| {
                wire::cell_of(
                    &self.book.shown(projected),
                    cells::input_text(projected.input()),
                    cells::provenance_text(host, projected),
                    projected,
                )
            })
            .collect();
        wire::Outline {
            omitted: omitted as u64,
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
        let m = dna_calc_model::geometry::sheet_metrics(
            geometry.as_ref().map(wire::geometry_of).as_ref(),
        );
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
                        crate::chart::chart_svg(&view, width, height)
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
        Interest::Areas(areas) => {
            calc_host::Interest::Areas(areas.into_iter().map(area).collect::<Result<_, _>>()?)
        }
        Interest::Sheet(sheet) => calc_host::Interest::Sheet(sheet.id()),
        Interest::Selection => calc_host::Interest::Selection,
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

#[cfg(test)]
mod fit_tests {
    use super::*;
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn fitted_contents_come_from_projection_and_a_stale_measurement_cannot_edit() {
        let mut server = Server::new();
        let sheet = server.book.host().sheets()[0].id();
        let at = CellKey::new(sheet, 1, 1).unwrap();
        let enter = |at, text: &str| {
            Request::Intent(Intent::EnterCell {
                at: Key::of(at),
                text: text.to_owned(),
            })
        };
        assert!(
            server
                .execute_current(enter(at, "original"), None)
                .reply
                .is_ok()
        );
        let Reply::FitContents { revision, cells } = server
            .execute_current(
                Request::FitContents {
                    area: Area::of(GridArea::cell(at)),
                },
                None,
            )
            .reply
            .unwrap()
        else {
            panic!("the content reply")
        };
        assert_eq!(cells.len(), 1);
        assert_eq!(cells[0].text, "original");
        assert!(
            server
                .execute_current(enter(CellKey::new(sheet, 2, 1).unwrap(), "later"), None)
                .reply
                .is_ok()
        );
        let answer = server.execute_current(
            Request::Intent(Intent::FitContents {
                sheet: Sheet::of(sheet),
                rows: false,
                revision,
                sizes: vec![(1, 20f64.to_bits())],
            }),
            None,
        );
        assert!(answer.reply.unwrap_err().contains("changed while fitting"));
        assert_eq!(
            server
                .book
                .host()
                .model()
                .definitions()
                .axis_layouts()
                .get(sheet, Axis::Columns, 1)
                .size(),
            None
        );
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn fitting_refuses_a_large_spill_before_expanding_or_projecting_its_cells() {
        let mut server = Server::new();
        let sheet = server.book.host().sheets()[0].id();
        let at = CellKey::new(sheet, 1, 1).unwrap();
        let reply = server
            .execute_current(
                Request::Intent(Intent::EnterCell {
                    at: Key::of(at),
                    text: "=SEQUENCE(10001)".to_owned(),
                }),
                None,
            )
            .reply
            .unwrap();
        assert!(matches!(
            reply,
            Reply::Done {
                outcome: Outcome { accepted: true, .. },
                ..
            }
        ));
        let target = GridArea::new(at, CellKey::new(sheet, 10001, 1).unwrap()).unwrap();
        let answer = server.execute_current(
            Request::FitContents {
                area: Area::of(target),
            },
            None,
        );
        assert!(answer.reply.unwrap_err().contains("10000 occupied"));
        assert_eq!(
            server
                .book
                .host()
                .model()
                .definitions()
                .axis_layouts()
                .get(sheet, Axis::Columns, 1)
                .size(),
            None
        );
    }
}

#[cfg(test)]
mod outline_tests {
    use super::*;
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn outline_search_reveal_and_spill_windows_use_the_application_server() {
        let mut server = Server::new();
        let data = include_bytes!("../../../crates/calc-host/tests/fixtures/dc_gf0/cut-base.xlsx");
        assert!(
            server
                .execute_current(
                    Request::Open {
                        name: "cut-base.xlsx".into()
                    },
                    Some(data.to_vec())
                )
                .reply
                .is_ok()
        );
        let sheet = server.book.host().sheets()[0].id();
        let distant = CellKey::new(sheet, 10000, 52).unwrap();
        server
            .execute_current(
                Request::Intent(Intent::EnterCell {
                    at: Key::of(distant),
                    text: "distant source".into(),
                }),
                None,
            )
            .reply
            .unwrap();
        let outline = server.outline(&[], &[Sheet::of(sheet)], Some(distant), "distant source");
        assert_eq!(outline.cells.len(), 1);
        assert_eq!(outline.cells[0].at, Key::of(distant));
        assert_eq!(outline.cells[0].input, "distant source");
        server.execute_current(Request::New, None).reply.unwrap();
        let sheet = server.book.host().sheets()[0].id();
        let anchor = CellKey::new(sheet, 1, 3).unwrap();
        server
            .execute_current(
                Request::Intent(Intent::EnterCell {
                    at: Key::of(anchor),
                    text: "=SEQUENCE(1000,2)".into(),
                }),
                None,
            )
            .reply
            .unwrap();
        let outline = server.outline(&[], &[Sheet::of(sheet)], Some(anchor), "");
        assert_eq!(outline.omitted, 1900);
        assert_eq!(outline.cells.len(), 100);
        let last = outline
            .cells
            .iter()
            .find(|c| c.at == Key::of(CellKey::new(sheet, 50, 4).unwrap()))
            .unwrap();
        assert_eq!(last.text, "100");
        assert!(last.input.is_empty());
        assert_eq!(last.provenance, cells::Provenance::Calculated);
    }
}

#[cfg(test)]
mod notebook_tests {
    use super::*;
    fn apply(server: &mut Server, intent: Intent) {
        let Reply::Done { outcome, .. } = server
            .execute_current(Request::Intent(intent), None)
            .reply
            .unwrap()
        else {
            panic!("an edit receipt")
        };
        assert!(outcome.accepted, "{:?}", outcome.error);
    }
    fn page(server: &mut Server, sheet: calc_host::value::SheetId, n: u64) -> wire::Notebook {
        let Reply::Notebook(page) = server
            .execute_current(
                Request::Notebook {
                    sheet: Sheet::of(sheet),
                    page: n,
                    reveal: None,
                    placement: None,
                },
                None,
            )
            .reply
            .unwrap()
        else {
            panic!("a notebook page")
        };
        page
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn newly_linked_duplicate_is_revealed_by_placement_after_the_first_page() {
        let mut server = Server::new();
        let sheet = server.book.host().sheets()[0].id();
        let at = CellKey::new(sheet, 1, 1).unwrap();
        apply(
            &mut server,
            Intent::EnterCell {
                at: Key::of(at),
                text: "7".into(),
            },
        );
        let mut last = None;
        for _ in 0..45 {
            let Reply::Done { outcome, .. } = server
                .execute_current(
                    Request::Intent(Intent::NotebookLink {
                        anchor: Area::of(GridArea::cell(at)),
                        prose: false,
                        text: None,
                    }),
                    None,
                )
                .reply
                .unwrap()
            else {
                panic!("link receipt")
            };
            assert!(outcome.accepted);
            last = outcome.notebook_placement;
        }
        let Reply::Notebook(page) = server
            .execute_current(
                Request::Notebook {
                    sheet: Sheet::of(sheet),
                    page: 0,
                    reveal: Some(Key::of(at)),
                    placement: last.clone(),
                },
                None,
            )
            .reply
            .unwrap()
        else {
            panic!("notebook page")
        };
        assert_eq!(page.page, 1);
        assert_eq!(page.blocks.len(), 5);
        assert_eq!(page.blocks.last().unwrap().placement, last);
        assert_eq!(page.blocks.last().unwrap().cell.as_ref().unwrap().text, "7");
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn links_prose_order_structural_moves_and_orphans_round_trip_without_moving_sources() {
        let mut server = Server::new();
        let sheet = server.book.host().sheets()[0].id();
        let key = |row, col| CellKey::new(sheet, row, col).unwrap();
        apply(
            &mut server,
            Intent::NotebookLink {
                anchor: Area::of(GridArea::cell(key(1, 1))),
                prose: true,
                text: Some("=visible prose".into()),
            },
        );
        let first = page(&mut server, sheet, 0);
        assert_eq!(first.blocks.len(), 1);
        assert!(first.blocks[0].prose);
        assert_eq!(
            first.blocks[0].cell.as_ref().unwrap().text,
            "=visible prose"
        );
        assert!(!first.blocks[0].cell.as_ref().unwrap().formula);
        apply(
            &mut server,
            Intent::NotebookLink {
                anchor: Area::of(GridArea::cell(key(2, 1))),
                prose: false,
                text: Some("=3*7".into()),
            },
        );
        let id = page(&mut server, sheet, 0).blocks[1]
            .placement
            .clone()
            .unwrap();
        apply(
            &mut server,
            Intent::NotebookOrder {
                placement: id.clone(),
                direction: -1,
            },
        );
        assert_eq!(
            page(&mut server, sheet, 0).blocks[0]
                .cell
                .as_ref()
                .unwrap()
                .text,
            "21"
        );
        assert_eq!(
            server
                .book
                .shown(&server.book.cell(key(1, 1)).unwrap())
                .text,
            "=visible prose"
        );
        apply(&mut server, Intent::Undo);
        assert!(page(&mut server, sheet, 0).blocks[0].prose);
        apply(&mut server, Intent::Redo);
        apply(
            &mut server,
            Intent::EditAxis {
                sheet: Sheet::of(sheet),
                rows: true,
                insert: true,
                start: 1,
                count: 1,
            },
        );
        assert_eq!(
            page(&mut server, sheet, 0).blocks[0]
                .anchor
                .unwrap()
                .area()
                .unwrap()
                .first(),
            key(3, 1)
        );
        let bytes = server.book.save().unwrap().0;
        server
            .execute_current(
                Request::Open {
                    name: "notebook.xlsx".into(),
                },
                Some(bytes),
            )
            .reply
            .unwrap();
        let sheet = server.book.host().sheets()[0].id();
        let reopened = page(&mut server, sheet, 0);
        assert_eq!(reopened.blocks[0].placement.as_deref(), Some(id.as_str()));
        assert_eq!(reopened.blocks[0].cell.as_ref().unwrap().text, "21");
        assert_eq!(
            reopened.blocks[1].cell.as_ref().unwrap().text,
            "=visible prose"
        );
        apply(
            &mut server,
            Intent::EditAxis {
                sheet: Sheet::of(sheet),
                rows: true,
                insert: false,
                start: 3,
                count: 1,
            },
        );
        assert!(page(&mut server, sheet, 0).blocks[0].anchor.is_none());
        let bytes = server.book.save().unwrap().0;
        server
            .execute_current(
                Request::Open {
                    name: "orphan.xlsx".into(),
                },
                Some(bytes),
            )
            .reply
            .unwrap();
        let sheet = server.book.host().sheets()[0].id();
        let orphan = page(&mut server, sheet, 0);
        assert!(orphan.blocks[0].anchor.is_none());
        assert_eq!(
            orphan.blocks[1].cell.as_ref().unwrap().text,
            "=visible prose"
        );
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn bounded_pages_and_current_array_details_do_not_turn_caches_into_calculated_values() {
        let mut server = Server::new();
        let sheet = server.book.host().sheets()[0].id();
        let key = |row, col| CellKey::new(sheet, row, col).unwrap();
        for row in 1..=85 {
            apply(
                &mut server,
                Intent::EnterCell {
                    at: Key::of(key(row, 1)),
                    text: row.to_string(),
                },
            );
        }
        let first = page(&mut server, sheet, 0);
        assert_eq!(first.total, 85);
        assert_eq!(first.blocks.len(), 40);
        assert_eq!(page(&mut server, sheet, 2).blocks.len(), 5);
        assert!(server.subscriptions.is_empty());
        apply(
            &mut server,
            Intent::EnterCell {
                at: Key::of(key(2, 4)),
                text: "99".into(),
            },
        );
        apply(
            &mut server,
            Intent::EnterCell {
                at: Key::of(key(1, 3)),
                text: "=SEQUENCE(3,2)".into(),
            },
        );
        let detail = server.block_detail(GridArea::cell(key(1, 3)));
        assert_eq!(detail.blockers, vec![Area::of(GridArea::cell(key(2, 4)))]);
        assert!(detail.summary.contains("cannot spill"));
        assert_eq!(detail.cells[3].text, "99");
        apply(
            &mut server,
            Intent::Clear {
                areas: vec![Area::of(GridArea::cell(key(2, 4)))],
            },
        );
        assert!(
            server
                .notebook_link(
                    Area::of(GridArea::cell(key(2, 4))),
                    false,
                    Some("overwrite".into())
                )
                .is_err()
        );
        assert!(
            server
                .notebook_link(Area::of(GridArea::cell(key(2, 4))), true, None)
                .is_err()
        );
        let detail = server.block_detail(GridArea::cell(key(2, 4)));
        assert_eq!(detail.anchor, Some(Key::of(key(1, 3))));
        assert!(detail.blockers.is_empty());
        assert_eq!(detail.cells.len(), 6);
        assert_eq!(detail.cells[5].text, "6");
        apply(&mut server, Intent::SetCalcMode { manual: true });
        apply(
            &mut server,
            Intent::EnterCell {
                at: Key::of(key(2, 4)),
                text: "99".into(),
            },
        );
        assert!(
            server
                .block_detail(GridArea::cell(key(1, 3)))
                .blockers
                .is_empty()
        );
    }
}

fn sheet_change(change: wire::SheetChange) -> calc_host::SheetChange {
    use calc_host::SheetChange as H;
    use wire::SheetChange as W;
    match change {
        W::Add { name } => H::Add { name },
        W::Rename { name } => H::Rename { name },
        W::Move { position } => H::Move { position },
        W::Remove => H::Remove,
        W::Hide => H::Hide,
        W::Show => H::Show,
    }
}
fn conditional_change(change: wire::ConditionalChange) -> calc_host::ConditionalChange {
    use calc_host::ConditionalChange as H;
    use wire::ConditionalChange as W;
    match change {
        W::Add {
            kind,
            applies,
            color,
        } => H::Add {
            kind,
            applies,
            color,
        },
        W::Update {
            applies,
            formulas,
            stop,
        } => H::Update {
            applies,
            formulas,
            stop,
        },
        W::Move { up } => H::Move { up },
        W::Remove => H::Remove,
    }
}

#[cfg(test)]
mod tile_tests {
    use super::*;
    use dna_calc_model::{TileApply, ViewMirror};
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_blank_formatted_cell_survives_the_wire_and_renders_until_undo() {
        let mut server = Server::new();
        let sheet = server.book().sheets()[0].id;
        let at = CellKey::new(sheet, 2, 2).unwrap();
        subscribe(&mut server, 1);
        let mut mirror = ViewMirror::default();
        let barrier = receive(&mut mirror, projection_events(&mut server));
        server
            .handle(Request::TileAck(barrier), None)
            .reply
            .unwrap();
        let answer = server.execute_current(
            Request::Intent(Intent::FormatCells {
                areas: vec![Area::of(GridArea::cell(at))],
                revision: server.book().host().revision().get(),
                patch: dna_calc_model::format::FormatPatch {
                    fill: Some("#ffeedd".into()),
                    ..Default::default()
                },
            }),
            None,
        );
        assert!(
            matches!(
                answer.reply,
                Ok(Reply::Done {
                    outcome: Outcome { accepted: true, .. },
                    ..
                })
            ),
            "{:?}",
            answer.reply
        );
        let barrier = receive(&mut mirror, projection_events(&mut server));
        let cell = mirror
            .cell(at)
            .expect("style-only cell is a projected identity");
        assert_eq!(cell.provenance, cells::Provenance::Empty);
        assert!(cell.text.is_empty());
        assert!(cell.input.is_empty());
        assert_eq!(cell.style.fill.as_deref(), Some("#ffeedd"));
        assert_eq!(
            render(&mirror, &server)
                .cells
                .iter()
                .find(|c| c.address == "B2")
                .unwrap()
                .shown
                .style
                .fill
                .as_deref(),
            Some("#ffeedd")
        );
        server
            .handle(Request::TileAck(barrier), None)
            .reply
            .unwrap();
        server
            .execute_current(Request::Intent(Intent::Undo), None)
            .reply
            .unwrap();
        let barrier = receive(&mut mirror, projection_events(&mut server));
        assert!(mirror.cell(at).is_none());
        server
            .handle(Request::TileAck(barrier), None)
            .reply
            .unwrap();
        server
            .execute_current(Request::Intent(Intent::Redo), None)
            .reply
            .unwrap();
        receive(&mut mirror, projection_events(&mut server));
        assert_eq!(
            mirror.cell(at).unwrap().style.fill.as_deref(),
            Some("#ffeedd")
        );
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_subscription_to_a_covered_merge_cell_observes_its_source_anchor_and_edits() {
        let mut server = Server::new();
        server
            .execute_current(
                Request::Open {
                    name: "features.xlsx".into(),
                },
                Some(
                    include_bytes!("../../../crates/calc-doc/tests/fixtures/dc_5sr/features.xlsx")
                        .to_vec(),
                ),
            )
            .reply
            .unwrap();
        let sheet = server.book().sheets()[0].id;
        let anchor = CellKey::new(sheet, 1, 8).unwrap();
        let covered = CellKey::new(sheet, 1, 9).unwrap();
        let enter_anchor = |server: &mut Server, text: &str| {
            let Reply::Done { outcome, .. } = server
                .execute_current(
                    Request::Intent(Intent::EnterCell {
                        at: Key::of(anchor),
                        text: text.into(),
                    }),
                    None,
                )
                .reply
                .unwrap()
            else {
                panic!("entry receipt")
            };
            assert!(outcome.accepted, "{:?}", outcome.error);
        };
        enter_anchor(&mut server, "merge source");
        server.take_events();
        let workbook = server.workbook_id();
        server
            .execute_current(
                Request::Subscribe {
                    workbook,
                    id: 1,
                    interest: Interest::Area(Area::of(GridArea::cell(covered))),
                },
                None,
            )
            .reply
            .unwrap();
        let mut mirror = ViewMirror::default();
        let barrier = receive(&mut mirror, projection_events(&mut server));
        assert_eq!(mirror.cell(anchor).unwrap().text, "merge source");
        assert_eq!(
            mirror.cell(anchor).unwrap().provenance,
            cells::Provenance::Literal
        );
        assert_eq!(
            mirror.geometry(sheet).unwrap().merges,
            [Area::of(GridArea::new(anchor, covered).unwrap())]
        );
        server
            .handle(Request::TileAck(barrier), None)
            .reply
            .unwrap();
        enter_anchor(&mut server, "changed source");
        receive(&mut mirror, projection_events(&mut server));
        assert_eq!(mirror.cell(anchor).unwrap().text, "changed source");
    }
    fn projection_events(server: &mut Server) -> Vec<Event> {
        server
            .take_events()
            .into_iter()
            .filter(|e| !matches!(e, Event::Calculation { .. }))
            .collect()
    }
    fn enter(server: &mut Server, text: &str) {
        let sheet = server.book().sheets()[0].id;
        let answer = server.execute_current(
            Request::Intent(Intent::EnterCell {
                at: Key::of(CellKey::new(sheet, 1, 1).unwrap()),
                text: text.into(),
            }),
            None,
        );
        assert!(
            matches!(
                answer.reply,
                Ok(Reply::Done {
                    outcome: Outcome { accepted: true, .. },
                    ..
                })
            ),
            "{:?}",
            answer.reply
        );
    }
    fn subscribe(server: &mut Server, id: u64) {
        let sheet = server.book().sheets()[0].id;
        assert!(
            server
                .handle(
                    Request::Subscribe {
                        workbook: server.workbook_id(),
                        id,
                        interest: Interest::Sheet(Sheet::of(sheet))
                    },
                    None
                )
                .reply
                .is_ok()
        );
    }
    fn render(mirror: &ViewMirror, server: &Server) -> dna_calc_model::render_plan::RenderPlan {
        let sheet = server.book().sheets()[0].id;
        let shown: Vec<_> = mirror.cells().filter_map(|(_, c)| c.shown()).collect();
        let metrics = dna_calc_model::geometry::sheet_metrics(mirror.geometry(sheet));
        let view = dna_calc_model::geometry::Viewport {
            width: 800.,
            height: 16_000.,
            ..Default::default()
        };
        let mut plan = dna_calc_model::render_plan::build_render_plan_with(
            &shown,
            &metrics,
            &view,
            1000,
            20,
            &dna_calc_model::render_plan::PlanOptions {
                geometry: mirror.geometry(sheet),
                ..Default::default()
            },
        );
        plan.attach_semantics(
            mirror,
            sheet,
            &[GridArea::cell(CellKey::new(sheet, 1, 1).unwrap())],
            (1, 1),
        );
        plan
    }
    fn receive(mirror: &mut ViewMirror, events: Vec<Event>) -> wire::TileBarrier {
        let mut barrier = None;
        assert!(matches!(events.first(), Some(Event::TileAdmission(_))));
        assert!(matches!(events.last(), Some(Event::TileCompletion(_))));
        let original = mirror
            .cells()
            .map(|(at, c)| (*at, c.clone()))
            .collect::<Vec<_>>();
        for event in events {
            match event {
                Event::TileAdmission(stamp) => {
                    mirror.admit(stamp).unwrap();
                }
                Event::Tiles(batch) => {
                    let batch = wire::decode_tiles(&wire::encode_tiles(&batch).unwrap()).unwrap();
                    assert_eq!(mirror.stage(&batch), Ok(TileApply::Staged));
                    assert_eq!(
                        mirror
                            .cells()
                            .map(|(at, c)| (*at, c.clone()))
                            .collect::<Vec<_>>(),
                        original
                    );
                }
                Event::TileCompletion(b) => {
                    assert_eq!(mirror.complete(b), Ok(TileApply::Applied(b)));
                    barrier = Some(b);
                }
                Event::ProjectionFailed { reason, .. } => panic!("{reason}"),
                Event::Calculation { .. } => {
                    unreachable!("projection helper removes calculation controls")
                }
            }
        }
        barrier.expect("the completion barrier")
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn fragmented_spills_shrink_atomically_through_the_application_codec() {
        let mut server = Server::new();
        enter(&mut server, "=SEQUENCE(600)");
        subscribe(&mut server, 11);
        let events = projection_events(&mut server);
        assert!(
            events
                .iter()
                .filter(|e| matches!(e, Event::Tiles(_)))
                .count()
                >= 3
        );
        let mut mirror = ViewMirror::default();
        let barrier = receive(&mut mirror, events);
        assert_eq!(mirror.cells().count(), 600);
        let plan = render(&mirror, &server);
        assert_eq!(plan.cells.len(), 600);
        assert_eq!(plan.publication, mirror.stamp());
        assert_eq!(plan.cells[599].shown.text, "600");
        assert_eq!(
            plan.cells[599].shown.provenance,
            wire::Provenance::Calculated
        );
        assert!(server.handle(Request::TileAck(barrier), None).reply.is_ok());
        enter(&mut server, "=SEQUENCE(2)");
        let events = projection_events(&mut server);
        assert!(
            events
                .iter()
                .any(|e| matches!(e,Event::Tiles(b) if !b.removed.is_empty()))
        );
        receive(&mut mirror, events);
        assert_eq!(mirror.cells().count(), 2);
        let plan = render(&mirror, &server);
        assert_eq!(plan.cells.len(), 2);
        assert_eq!(plan.cells[1].shown.text, "2");
        assert!(plan.semantic_rows[0].cells[0].selected);
        let cleared = plan
            .semantic_rows
            .iter()
            .flat_map(|r| &r.cells)
            .find(|c| c.address == "A3")
            .unwrap();
        assert_eq!(cleared.provenance, "empty");
        assert!(cleared.text.is_empty());
        assert_eq!(
            mirror
                .cells()
                .map(|(_, c)| c.text.as_str())
                .collect::<Vec<_>>(),
            ["1", "2"]
        );
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_slow_subscriber_keeps_one_flight_and_coalesces_missed_revisions_into_a_reset() {
        let mut server = Server::new();
        enter(&mut server, "1");
        subscribe(&mut server, 12);
        let mut mirror = ViewMirror::default();
        let first = receive(&mut mirror, projection_events(&mut server));
        for n in 2..=75 {
            enter(&mut server, &n.to_string());
            assert!(projection_events(&mut server).is_empty());
        }
        let mut forged = first;
        forged.transaction += 1;
        server.handle(Request::TileAck(forged), None);
        assert!(projection_events(&mut server).is_empty());
        server.handle(Request::TileAck(first), None);
        let events = projection_events(&mut server);
        assert!(
            matches!(events.first(),Some(Event::TileAdmission(s)) if s.reset && s.base==first.through)
        );
        let latest = receive(&mut mirror, events);
        assert_eq!(mirror.cells().next().unwrap().1.text, "75");
        assert_eq!(latest.through, first.through + 1);
        server.handle(Request::TileAck(first), None); // duplicate ack cannot release the new flight
        enter(&mut server, "76");
        assert!(projection_events(&mut server).is_empty());
        server.handle(Request::TileAck(latest), None);
        receive(&mut mirror, projection_events(&mut server));
        assert_eq!(mirror.cells().next().unwrap().1.text, "76");
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn lost_fragments_recover_by_explicit_reset_and_old_workbook_acks_cannot_release_new_flights() {
        let mut server = Server::new();
        enter(&mut server, "=SEQUENCE(300)");
        subscribe(&mut server, 13);
        let events = projection_events(&mut server);
        let Event::TileAdmission(stamp) = events[0] else {
            panic!("admission")
        };
        let mut mirror = ViewMirror::default();
        mirror.admit(stamp).unwrap();
        assert!(mirror.complete(stamp.barrier()).is_err());
        server.handle(
            Request::ResetTiles {
                workbook: server.workbook_id(),
                id: 13,
            },
            None,
        );
        let reset = receive(&mut mirror, projection_events(&mut server));
        assert_eq!(mirror.cells().count(), 300);
        let old = reset;
        assert!(server.execute_current(Request::New, None).reply.is_ok());
        subscribe(&mut server, 13);
        let mut replacement = ViewMirror::default();
        let new = receive(&mut replacement, projection_events(&mut server));
        assert_ne!(old.workbook, new.workbook);
        server.handle(Request::TileAck(old), None);
        enter(&mut server, "42");
        assert!(projection_events(&mut server).is_empty());
        server.handle(Request::TileAck(new), None);
        receive(&mut replacement, projection_events(&mut server));
        assert_eq!(replacement.cells().next().unwrap().1.text, "42");
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn subscription_budget_is_enforced_and_unsubscribe_releases_it() {
        let mut server = Server::new();
        for id in 0..wire::MAX_SUBSCRIPTIONS as u64 {
            subscribe(&mut server, id);
        }
        let sheet = server.book().sheets()[0].id;
        let workbook = server.workbook_id();
        let request = || Request::Subscribe {
            workbook,
            id: 100,
            interest: Interest::Sheet(Sheet::of(sheet)),
        };
        assert!(
            server
                .handle(request(), None)
                .reply
                .unwrap_err()
                .contains("Too many")
        );
        server.handle(Request::Unsubscribe { workbook, id: 0 }, None);
        assert!(server.handle(request(), None).reply.is_ok());
        assert!(!server.tile_streams.contains_key(&0));
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn delayed_controls_cannot_reach_a_replacement_or_reuse_a_retired_identity() {
        let mut server = Server::new();
        subscribe(&mut server, 1);
        let old = server.workbook_id();
        server.execute_current(Request::New, None).reply.unwrap();
        let current = server.workbook_id();
        assert!(current > old);
        subscribe(&mut server, 1);
        let sheet = server.book().sheets()[0].id;
        let interest = Interest::Sheet(Sheet::of(sheet));
        assert!(
            server
                .handle(
                    Request::SetInterest {
                        workbook: old,
                        id: 1,
                        interest: interest.clone()
                    },
                    None
                )
                .reply
                .is_err()
        );
        assert!(
            server
                .handle(
                    Request::ResetTiles {
                        workbook: old,
                        id: 1
                    },
                    None
                )
                .reply
                .is_err()
        );
        assert!(
            server
                .handle(
                    Request::Subscribe {
                        workbook: old,
                        id: 2,
                        interest: interest.clone()
                    },
                    None
                )
                .reply
                .is_err()
        );
        server
            .handle(
                Request::Unsubscribe {
                    workbook: old,
                    id: 1,
                },
                None,
            )
            .reply
            .unwrap();
        assert!(server.subscriptions.contains_key(&1));
        let mut mirror = ViewMirror::default();
        let barrier = receive(&mut mirror, projection_events(&mut server));
        assert_eq!(barrier.workbook, current);
        server
            .handle(
                Request::Unsubscribe {
                    workbook: current,
                    id: 1,
                },
                None,
            )
            .reply
            .unwrap();
        assert!(
            server
                .handle(
                    Request::Subscribe {
                        workbook: current,
                        id: 1,
                        interest: interest.clone()
                    },
                    None
                )
                .reply
                .unwrap_err()
                .contains("cannot be reused")
        );
        assert!(
            server
                .handle(
                    Request::Subscribe {
                        workbook: current,
                        id: 2,
                        interest
                    },
                    None
                )
                .reply
                .is_ok()
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn retiring_subscriptions_discards_undelivered_frames() {
        let mut server = Server::new();
        for id in 0..200 {
            subscribe(&mut server, id);
            assert!(server.events.iter().any(|e| matches!(e, Event::Tiles(_))));
            let controls: Vec<_> = server
                .events
                .iter()
                .filter(|e| matches!(e, Event::Calculation { .. }))
                .cloned()
                .collect();
            server
                .handle(
                    Request::Unsubscribe {
                        workbook: server.workbook_id(),
                        id,
                    },
                    None,
                )
                .reply
                .unwrap();
            assert_eq!(server.take_events(), controls);
            assert!(server.tile_streams.is_empty());
            assert!(server.subscription_interests.is_empty());
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn recalculation_versions_presentation_roots_without_an_authored_revision() {
        let mut server = Server::new();
        enter(&mut server, "=RAND()");
        server.take_events();
        subscribe(&mut server, 11);
        let mut mirror = ViewMirror::default();
        let first = receive(&mut mirror, projection_events(&mut server));
        let previous = mirror.stamp().unwrap();
        server.handle(Request::TileAck(first), None).reply.unwrap();
        server.take_events();
        server
            .execute_current(Request::Intent(Intent::Recalculate), None)
            .reply
            .unwrap();
        receive(&mut mirror, projection_events(&mut server));
        let current = mirror.stamp().unwrap();
        assert_eq!(current.revision, previous.revision);
        assert!(current.transaction > previous.transaction);
        assert_eq!(current.geometry_version, current.transaction);
        assert_eq!(current.style_version, current.transaction);
        assert_ne!(current.style_version, previous.style_version);
        let cell = mirror.cells().next().unwrap().1;
        assert_eq!(cell.input, "=RAND()");
        assert_eq!(cell.provenance, wire::Provenance::Calculated);
        assert_eq!(cell.kind, "number");
        assert!((0.0..1.0).contains(&cell.text.parse::<f64>().unwrap()));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn calculation_completion_waits_for_its_projection_under_backpressure() {
        let mut server = Server::new();
        enter(&mut server, "1");
        server.take_events();
        subscribe(&mut server, 11);
        let mut mirror = ViewMirror::default();
        let barrier = receive(&mut mirror, projection_events(&mut server));
        enter(&mut server, "=1+1");
        let held = server.take_events();
        let command = held
            .iter()
            .find_map(|e| match e {
                Event::Calculation {
                    receipt: wire::CalculationEvent::Admitted { command, .. },
                    ..
                } => Some(*command),
                _ => None,
            })
            .unwrap();
        assert!(!held.iter().any(|e| matches!(
            e,
            Event::Tiles(_)
                | Event::TileCompletion(_)
                | Event::Calculation {
                    receipt: wire::CalculationEvent::Completed { .. },
                    ..
                }
        )));
        assert!(server.handle(Request::TileAck(barrier), None).reply.is_ok());
        let released = server.take_events();
        let unit = released
            .iter()
            .find_map(|e| match e {
                Event::TileAdmission(stamp) => stamp.calculation_through,
                _ => None,
            })
            .unwrap();
        assert_eq!(unit.command, command);
        let tiles = released
            .iter()
            .position(|e| matches!(e, Event::TileCompletion(_)))
            .unwrap();
        let completed = released
            .iter()
            .position(|e| {
                matches!(e, Event::Calculation {
            receipt: wire::CalculationEvent::Completed { command: id, .. }, .. } if *id == command)
            })
            .unwrap();
        assert!(tiles < completed);
        receive(
            &mut mirror,
            released
                .into_iter()
                .filter(|e| !matches!(e, Event::Calculation { .. }))
                .collect(),
        );
        let sheet = server.book().sheets()[0].id;
        assert_eq!(
            mirror
                .cell(CellKey::new(sheet, 1, 1).unwrap())
                .unwrap()
                .text,
            "2"
        );
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn an_oversized_calculation_reports_projection_failure_without_a_tile_barrier() {
        let mut server = Server::new();
        subscribe(&mut server, 11);
        let mut mirror = ViewMirror::default();
        let barrier = receive(&mut mirror, projection_events(&mut server));
        assert!(server.handle(Request::TileAck(barrier), None).reply.is_ok());
        server.take_events();
        let sheet = server.book().sheets()[0].id;
        let first = CellKey::new(sheet, 1, 1).unwrap();
        let area = GridArea::new(first, CellKey::new(sheet, 100, 1).unwrap()).unwrap();
        let selected = server.execute_current(
            Request::Intent(Intent::Select {
                active: Key::of(first),
                areas: vec![Area::of(area)],
            }),
            None,
        );
        assert!(selected.reply.is_ok());
        let result = server.execute_current(
            Request::Intent(Intent::EnterSelection {
                text: "=REPT(\"x\",32767)".into(),
            }),
            None,
        );
        assert!(matches!(
            result.reply,
            Ok(Reply::Done {
                outcome: Outcome { accepted: true, .. },
                ..
            })
        ));
        let events = server.take_events();
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::ProjectionFailed { id: 11, .. }))
        );
        assert!(!events.iter().any(|e| matches!(e, Event::TileCompletion(_))));
        assert!(events.iter().any(|e| matches!(e,Event::Calculation { receipt: wire::CalculationEvent::Completed { outcome: wire::CalculationOutcome::Complete, calculated: 100, .. }, failed_projections, .. } if failed_projections == &[11])));
        assert!(server.tile_streams[&11].failed());
        assert!(mirror.cells().next().is_none());
        let marker = CellKey::new(sheet, 1, 2).unwrap();
        server
            .execute_current(
                Request::Intent(Intent::EnterCell {
                    at: Key::of(marker),
                    text: "preserved".into(),
                }),
                None,
            )
            .reply
            .unwrap();
        let deleted = GridArea::new(
            CellKey::new(sheet, 2, 1).unwrap(),
            CellKey::new(sheet, 100, 1).unwrap(),
        )
        .unwrap();
        server
            .execute_current(
                Request::Intent(Intent::Clear {
                    areas: vec![Area::of(deleted)],
                }),
                None,
            )
            .reply
            .unwrap();
        assert!(server.tile_streams[&11].failed());
        assert!(
            !server
                .take_events()
                .iter()
                .any(|e| matches!(e, Event::Tiles(_) | Event::TileCompletion(_)))
        );
        mirror.invalidate();
        server
            .handle(
                Request::ResetTiles {
                    workbook: server.workbook_id(),
                    id: 11,
                },
                None,
            )
            .reply
            .unwrap();
        receive(&mut mirror, projection_events(&mut server));
        assert!(!server.tile_streams[&11].failed());
        assert!(!mirror.needs_reset());
        assert_eq!(mirror.cells().count(), 2);
        assert_eq!(mirror.cell(first).unwrap().text.len(), 32767);
        assert_eq!(mirror.cell(marker).unwrap().text, "preserved");
        assert!(mirror.cell(CellKey::new(sheet, 2, 1).unwrap()).is_none());
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_delayed_manual_completion_retains_its_original_revision_after_an_edit() {
        let mut server = Server::new();
        assert!(
            server
                .execute_current(Request::Intent(Intent::SetCalcMode { manual: true }), None)
                .reply
                .is_ok()
        );
        enter(&mut server, "=1+1");
        server.take_events();
        subscribe(&mut server, 11);
        let mut mirror = ViewMirror::default();
        let barrier = receive(&mut mirror, projection_events(&mut server));
        let sheet = server.book().sheets()[0].id;
        let at = CellKey::new(sheet, 1, 1).unwrap();
        assert!(
            server
                .execute_current(
                    Request::Intent(Intent::CalculateRange {
                        areas: vec![Area::of(GridArea::cell(at))]
                    }),
                    None
                )
                .reply
                .is_ok()
        );
        let original_revision = server.book().host().revision().get();
        let held = server.take_events();
        assert!(held.iter().any(|e| matches!(
            e,
            Event::Calculation {
                receipt: wire::CalculationEvent::Admitted { .. },
                ..
            }
        )));
        enter(&mut server, "9");
        let after_edit = server.take_events();
        assert!(!after_edit.iter().any(|e| matches!(
            e,
            Event::Calculation {
                receipt: wire::CalculationEvent::Admitted { .. },
                ..
            }
        )));
        assert!(server.book().host().revision().get() > original_revision);
        assert!(server.handle(Request::TileAck(barrier), None).reply.is_ok());
        let events = server.take_events();
        assert!(events.iter().any(|e| matches!(e,Event::Calculation {
            receipt: wire::CalculationEvent::Completed { revision, .. }, summary: Some(summary), ..
        } if *revision == original_revision && summary.revision == server.book().host().revision().get())));
        receive(
            &mut mirror,
            events
                .into_iter()
                .filter(|e| !matches!(e, Event::Calculation { .. }))
                .collect(),
        );
        assert_eq!(mirror.cell(at).unwrap().text, "9");
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn delayed_controls_keep_current_mode_and_saved_status_at_the_same_revision() {
        let mut server = Server::new();
        enter(&mut server, "1");
        server.take_events();
        subscribe(&mut server, 11);
        let mut mirror = ViewMirror::default();
        let barrier = receive(&mut mirror, projection_events(&mut server));
        enter(&mut server, "=1+1");
        let revision = server.book().host().revision().get();
        let held = server.take_events();
        assert!(held.iter().any(|e| matches!(
            e,
            Event::Calculation {
                receipt: wire::CalculationEvent::Admitted { .. },
                ..
            }
        )));
        assert!(
            server
                .execute_current(Request::Intent(Intent::SetCalcMode { manual: true }), None)
                .reply
                .is_ok()
        );
        assert!(server.execute_current(Request::Save, None).reply.is_ok());
        assert_eq!(server.book().host().revision().get(), revision);
        assert!(server.summary().manual);
        assert!(!server.summary().changed);
        assert!(server.handle(Request::TileAck(barrier), None).reply.is_ok());
        let events = server.take_events();
        let mut completed = 0;
        for event in events {
            if let Event::Calculation {
                receipt,
                summary: Some(summary),
                ..
            } = event
            {
                assert_eq!(summary.revision, revision);
                assert!(summary.manual);
                assert!(!summary.changed);
                if let wire::CalculationEvent::Completed {
                    revision: admitted, ..
                } = receipt
                {
                    assert_eq!(admitted, revision);
                    completed += 1;
                }
            }
        }
        assert_eq!(completed, 1);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn removal_pressure_coalesces_to_the_complete_root_and_never_admits_an_oversized_delta() {
        let mut server = Server::new();
        enter(&mut server, "retained");
        subscribe(&mut server, 11);
        let mut mirror = ViewMirror::default();
        let first = receive(&mut mirror, projection_events(&mut server));
        server.handle(Request::TileAck(first), None);
        let marker = mirror.cells().next().unwrap().1.clone();
        let sheet = Sheet(marker.at.0, marker.at.1);
        let geometry = vec![(sheet, server.geometry(sheet))];
        let revision = server.book().host().revision().get();

        // Individually legal disjoint removal lists exceed the pending record budget together.
        // The untouched marker must survive replacing those operations with the current root.
        let split = wire::MAX_PUBLICATION_RECORDS / 2;
        let removed = |start: usize, count: usize| {
            (start..start + count)
                .map(|row| Key(sheet.0, sheet.1, row as u32, 1))
                .collect()
        };
        {
            let stream = server.tile_streams.get_mut(&11).unwrap();
            stream
                .update(
                    false,
                    Vec::new(),
                    removed(2, split),
                    revision,
                    geometry.clone(),
                    None,
                )
                .unwrap();
            stream
                .update(
                    false,
                    Vec::new(),
                    removed(2 + split, split + 1),
                    revision,
                    geometry.clone(),
                    None,
                )
                .unwrap();
        }
        server.collect();
        let events = projection_events(&mut server);
        assert!(matches!(events.first(), Some(Event::TileAdmission(s)) if s.reset));
        let second = receive(&mut mirror, events);
        assert_eq!(mirror.cells().count(), 1);
        assert_eq!(mirror.cell(marker.at.cell().unwrap()), Some(&marker));
        server.handle(Request::TileAck(second), None);

        // Byte pressure also resets a pending delta before its count limit. Every fragment
        // and the aggregate still pass the shipping codec and atomic mirror route.
        let mut large = marker.clone();
        large.text = "x".repeat(16_384);
        large.input = large.text.clone();
        let geometry_bytes = geometry
            .iter()
            .map(|(_, g)| wire::geometry_bytes(g))
            .sum::<usize>();
        let count = (wire::MAX_PUBLICATION_BYTES
            - wire::MAX_TILE_BYTES
            - geometry_bytes
            - marker.retained_bytes())
            / large.retained_bytes();
        let cells = (0..count)
            .map(|row| {
                let mut cell = large.clone();
                cell.at = Key(sheet.0, sheet.1, row as u32 + 1, 2);
                cell
            })
            .collect();
        assert!(count + 16_384 < wire::MAX_PUBLICATION_RECORDS);
        {
            let stream = server.tile_streams.get_mut(&11).unwrap();
            stream
                .update(false, cells, Vec::new(), revision, geometry.clone(), None)
                .unwrap();
            stream
                .update(
                    false,
                    Vec::new(),
                    removed(2, 16_384),
                    revision,
                    geometry.clone(),
                    None,
                )
                .unwrap();
        }
        server.collect();
        let events = projection_events(&mut server);
        assert!(matches!(events.first(), Some(Event::TileAdmission(s)) if s.reset));
        let (bytes, records) = events
            .iter()
            .fold((0, 0), |(bytes, records), event| match event {
                Event::Tiles(batch) => (
                    bytes + batch.retained_bytes(),
                    records + batch.cells.len() + batch.removed.len(),
                ),
                _ => (bytes, records),
            });
        assert!(bytes <= wire::MAX_PUBLICATION_BYTES);
        assert!(records <= wire::MAX_PUBLICATION_RECORDS);
        let third = receive(&mut mirror, events);
        assert_eq!(mirror.cells().count(), count + 1);
        assert_eq!(mirror.cell(marker.at.cell().unwrap()), Some(&marker));
        assert_eq!(
            mirror
                .cell(Key(sheet.0, sheet.1, count as u32, 2).cell().unwrap())
                .unwrap()
                .text,
            large.text
        );
        server.handle(Request::TileAck(third), None);

        // An individual oversized list is refused before retaining it or advancing delivery.
        let stream = server.tile_streams.get_mut(&11).unwrap();
        let error = stream
            .update(
                false,
                Vec::new(),
                removed(2, wire::MAX_PUBLICATION_RECORDS + 1),
                revision,
                geometry,
                None,
            )
            .unwrap_err();
        assert!(error.contains("delta exceeds"));
        assert!(stream.failed());
        assert!(!stream.has_pending());
        assert!(
            stream
                .publish(11, first.workbook, third.transaction + 1)
                .unwrap()
                .is_empty()
        );
        assert_eq!(mirror.cells().count(), count + 1);
        assert_eq!(mirror.stamp().unwrap().barrier(), third);
    }
}
