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
    workbook: u64,
    commands: calc_host::CommandJournal<(Result<Reply,String>, Option<Vec<u8>>)>,
    validation_pending: Option<(u64, u64, Selection, SkinIntent)>,
    book: Workbook,
    /// The page's subscriptions: its number for each, and calc-host's.
    subscriptions: BTreeMap<u64, SubscriptionId>,
    events: Vec<Event>,
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
            workbook: 0,
            commands: calc_host::CommandJournal::new(128,16*1024*1024),
            validation_pending: None,
            book,
            subscriptions: BTreeMap::new(),
            events: Vec::new(),
            snapshots: BTreeMap::new(),
            next_clip: 1,
            clipboard_session: crate::clipboard::session_nonce(),
            paste_plan: None,
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
        if let Request::Command(envelope) = request {return self.command(envelope,bytes);}
        if request.is_command() {return Answer {reply:Err("A workbook command needs an identity and base revision".into()),bytes:None};}
        self.handle_admitted(request,bytes)
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
                Request::Summary=>Ok(Reply::Summary(self.summary())),
                Request::Command(_)=>Err("Nested command envelope".into()),
                Request::Find(options) => {
                    Ok(Reply::Found(crate::search::find(&self.book, &options)))
                }
                Request::SelectionStats => Ok(Reply::SelectionStats(
                    crate::search::selection_stats(&self.book, self.book.session().selection()),
                )),
                Request::CanEdit(key) => {
                    Ok(Reply::CanEdit(cell(key).and_then(|at| self.can_edit(at))))
                }
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
                    let text = crate::clipboard::encode_tsv(&rows);
                    let html = crate::clipboard::encode_html(&rows, token, &self.clipboard_session);
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
                Request::Geometry(sheet) => {
                    let mut geometry = self
                        .book
                        .host()
                        .sheet_geometry(sheet.id())
                        .map(|g| Geometry::of(&g))
                        .unwrap_or_default();
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
                    Ok(Reply::Geometry(geometry))
                }
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
                        memory: crate::controller::memory_bytes(),
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
    fn install(&mut self, book: Workbook) {
        self.validation_pending = None;
        self.workbook = self.workbook.checked_add(1).expect("workbook incarnations exhausted");
        self.book = book;
        self.subscriptions.clear();
        self.events.clear();
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
        let plan = calc_host::prepare_paste(self.book.host(), source, to, mode.host())?;
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
            workbook:self.workbook,
            session:self.clipboard_session.clone(),
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
        if host.model().sheet_by_id(target.sheet()).is_none() {
            return Err("The sheet is unavailable".to_owned());
        }
        let sheet = host
            .loaded()
            .sheet(target.sheet())
            .ok_or("The sheet is unavailable")?;
        if sheet.protected() {
            return Err("Fitting a protected sheet is unavailable here".to_owned());
        }
        if host.merged_areas(target.sheet()).iter().any(|merge| {
            merge.first().row() <= target.last().row()
                && merge.last().row() >= target.first().row()
                && merge.first().column() <= target.last().column()
                && merge.last().column() >= target.first().column()
        }) {
            return Err(
                "AutoFit with merged cells is unavailable; choose an unmerged range".to_owned(),
            );
        }
        let metrics = crate::grid::sheet_metrics(host.sheet_geometry(target.sheet()).as_ref());
        let mut keys = std::collections::BTreeSet::new();
        for occupied in host.occupied_areas(target.sheet()) {
            let top = target.first().row().max(occupied.first().row());
            let left = target.first().column().max(occupied.first().column());
            let bottom = target.last().row().min(occupied.last().row());
            let right = target.last().column().min(occupied.last().column());
            if top > bottom || left > right {
                continue;
            }
            if u64::from(bottom - top + 1) * u64::from(right - left + 1)
                > crate::grid::autofit::FIT_CELLS as u64
            {
                return Err("Fit at most 10000 occupied cells at once".to_owned());
            }
            for row in top..=bottom {
                for column in left..=right {
                    if metrics.width(column) == 0.0 || metrics.height(row) == 0.0 {
                        continue;
                    }
                    keys.insert(CellKey::new(target.sheet(), row, column).unwrap());
                    if keys.len() > crate::grid::autofit::FIT_CELLS {
                        return Err("Fit at most 10000 occupied cells at once".to_owned());
                    }
                }
            }
        }
        let projected = calc_host::project(
            host,
            &calc_host::ProjectionRequest {
                cells: keys.into_iter().collect(),
            },
        )
        .map_err(|e| e.to_string())?;
        let mut cells = Vec::new();
        for p in projected.cells() {
            let shown = self.book.shown(p);
            if let Some(reason) = &shown.style.unsupported {
                return Err(reason.clone());
            }
            cells.push(Cell::of(
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
                    crate::grid::selection::Navigation::Column
                        | crate::grid::selection::Navigation::Row
                ) && selection.areas.len() > 1
                {
                    return Ok(self.done(false,Some("Axis expansion for disjoint selections is not modelled; select one rectangle".to_owned()),0.0));
                }
                if extend
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
                    crate::grid::selection::Navigation::All
                        | crate::grid::selection::Navigation::Column
                        | crate::grid::selection::Navigation::Row
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
                    crate::grid::selection::Navigation::Edge(_)
                        | crate::grid::selection::Navigation::All
                ) {
                    host.occupied_areas(at.sheet())
                } else {
                    Vec::new()
                };
                let mut used = if matches!(nav, crate::grid::selection::Navigation::UsedEnd) {
                    host.used_area(at.sheet())
                } else {
                    None
                };
                if matches!(nav, crate::grid::selection::Navigation::UsedEnd) {
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
                let metrics = crate::grid::sheet_metrics(host.sheet_geometry(at.sheet()).as_ref());
                SkinIntent::Select(crate::grid::selection::navigate(
                    selection,
                    nav,
                    extend,
                    &metrics,
                    f64::from(page_height),
                    &occupied,
                    used,
                ))
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
        let started = crate::controller::now_ms();
        let receipt = self.book.dispatch(intent);
        if rule_error_is_route
            && let Some(calc_host::IntentError::Workbook(reason)) = &receipt.error
        {
            return Err(reason.clone());
        }
        let elapsed = crate::controller::now_ms() - started;
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
                let shown = self.book.shown(projected);
                if projected.input().is_none()
                    && projected.value() == &ProjectedValue::Empty
                    && shown.conditional.is_none()
                {
                    emptied.push(Key::of(projected.at()));
                    continue;
                }
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
            crate::tree::outline_keys(host, sheet.id(), crate::notebook::PAGE, reveal);
        linked.extend(
            keys.into_iter()
                .filter(|at| !placed.iter().any(|a| a.contains(*at)))
                .map(|at| (None, Some(GridArea::cell(at)), false)),
        );
        let total = linked.len();
        let pages = total.div_ceil(crate::notebook::PAGE).max(1);
        let page = placement
            .and_then(|id| linked.iter().position(|(p, _, _)| p.as_deref() == Some(id)))
            .or_else(|| {
                reveal.and_then(|at| {
                    linked
                        .iter()
                        .position(|(_, a, _)| a.is_some_and(|a| a.contains(at)))
                })
            })
            .map(|i| i / crate::notebook::PAGE)
            .unwrap_or(requested)
            .min(pages - 1);
        let blocks = linked
            .into_iter()
            .skip(page * crate::notebook::PAGE)
            .take(crate::notebook::PAGE)
            .map(|(placement, anchor, prose)| {
                let cell = anchor.and_then(|a| self.book.cell(a.first())).map(|p| {
                    Cell::of(
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

    fn can_edit(&self, at: CellKey) -> Result<(), String> {
        if self.book.host().model().sheet_by_id(at.sheet()).is_none() {
            return Err("The worksheet is unavailable".into());
        }
        if self
            .book
            .host()
            .loaded()
            .sheet(at.sheet())
            .is_some_and(|s| s.protected() || s.is_macro_sheet())
        {
            return Err("Editing protected or macro worksheets is unavailable".into());
        }
        Ok(())
    }

    fn notebook_link(
        &mut self,
        target: Area,
        prose: bool,
        text: Option<String>,
    ) -> Result<Reply, String> {
        use calc_host::{BlockId, BlockKind, PlacementId, SkinLayoutEdit, Spot, ViewKind};
        let target = area(target)?;
        if text.is_some() {
            self.can_edit(target.first())?;
        }
        let host = self.book.host();
        let layout = host.model().layout();
        if text.is_some()
            && (target.first() != target.last()
                || host
                    .occupied_areas(target.sheet())
                    .iter()
                    .any(|area| area.contains(target.first())))
        {
            return Err("Insert into one empty cell, or link existing cells without replacing their contents".into());
        }
        let kind = if prose {
            BlockKind::Prose
        } else {
            BlockKind::Calculation
        };
        if prose
            && host
                .model()
                .cells(target)
                .into_iter()
                .flatten()
                .any(|(_, c)| matches!(c, calc_host::CellInput::Formula(_)))
        {
            return Err(
                "Prose uses visible worksheet text; a formula is a calculation block".into(),
            );
        }
        if prose
            && host.occupied_areas(target.sheet()).iter().any(|a| {
                a.first() != a.last()
                    && a.first().row() <= target.last().row()
                    && a.last().row() >= target.first().row()
                    && a.first().column() <= target.last().column()
                    && a.last().column() >= target.first().column()
            })
        {
            return Err("Prose links use worksheet text, not array values".into());
        }
        let entry = text.map(|text| if prose { format!("'{text}") } else { text });
        let mut edits = Vec::new();
        let block = if let Some(block) = layout
            .blocks()
            .iter()
            .find(|b| b.anchor() == Some(target) && b.kind() == kind)
        {
            block.id()
        } else {
            let id = BlockId::new(
                layout
                    .blocks()
                    .iter()
                    .map(|b| b.id().get())
                    .max()
                    .unwrap_or(0)
                    .checked_add(1)
                    .ok_or("Block identity space exhausted")?,
            )
            .ok_or("Block identity space exhausted")?;
            edits.push(CellEdit::Layout(SkinLayoutEdit::CreateBlock {
                block: id,
                kind,
                anchor: target,
            }));
            id
        };
        let id = PlacementId::new(
            ViewKind::ALL
                .iter()
                .flat_map(|v| layout.view(*v))
                .map(|p| p.id().get())
                .max()
                .unwrap_or(0)
                .checked_add(1)
                .ok_or("Placement identity space exhausted")?,
        )
        .ok_or("Placement identity space exhausted")?;
        let position = std::num::NonZeroU32::new(
            u32::try_from(layout.view(ViewKind::Notebook).len() + 1)
                .map_err(|_| "Too many notebook links")?,
        )
        .ok_or("Notebook position is zero")?;
        edits.push(CellEdit::Layout(SkinLayoutEdit::AddPlacement {
            placement: id,
            block,
            spot: Spot::Notebook { position },
        }));
        if let Some(text) = entry {
            let layout = edits
                .into_iter()
                .filter_map(|e| {
                    if let CellEdit::Layout(layout) = e {
                        Some(layout)
                    } else {
                        None
                    }
                })
                .collect();
            self.dispatch_entry(
                SkinIntent::EnterCellAndLayout {
                    at: target.first(),
                    text,
                    layout,
                },
                false,
            )
        } else {
            self.dispatch_entry(SkinIntent::Edit { edits }, false)
        }
    }

    fn notebook_order(&mut self, id: String, direction: i8) -> Result<Reply, String> {
        use calc_host::{PlacementId, SkinLayoutEdit, Spot, ViewKind};
        let id = PlacementId::parse(&id).ok_or("Invalid notebook placement")?;
        let layout = self.book.host().model().layout();
        let list = layout.view(ViewKind::Notebook);
        let index = list
            .iter()
            .position(|p| p.id() == id)
            .ok_or("Notebook link no longer exists")?;
        let mut edits = Vec::new();
        if direction == 0 {
            let block = list[index].block();
            edits.push(CellEdit::Layout(SkinLayoutEdit::RemovePlacement {
                placement: id,
            }));
            if ViewKind::ALL
                .iter()
                .flat_map(|v| layout.view(*v))
                .filter(|p| p.block() == block)
                .count()
                == 1
            {
                edits.push(CellEdit::Layout(SkinLayoutEdit::RemoveBlock { block }));
            }
        } else {
            let sheet = layout
                .block(list[index].block())
                .and_then(|b| b.anchor())
                .map(|a| a.sheet());
            let peer = if direction < 0 {
                (0..index).rev().find(|i| {
                    layout
                        .block(list[*i].block())
                        .and_then(|b| b.anchor())
                        .map(|a| a.sheet())
                        == sheet
                })
            } else {
                (index + 1..list.len()).find(|i| {
                    layout
                        .block(list[*i].block())
                        .and_then(|b| b.anchor())
                        .map(|a| a.sheet())
                        == sheet
                })
            };
            let Some(peer) = peer else {
                return Ok(self.done(true, None, 0.));
            };
            edits.push(CellEdit::Layout(SkinLayoutEdit::MovePlacement {
                placement: id,
                spot: Spot::Notebook {
                    position: std::num::NonZeroU32::new(peer as u32 + 1).unwrap(),
                },
            }));
        }
        self.dispatch_entry(SkinIntent::Edit { edits }, false)
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
                Cell::of(
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
                Cell::of(
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
        assert!(server.execute_current(enter(at, "original"), None).reply.is_ok());
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
        let Reply::Done { outcome, .. } =
            server.execute_current(Request::Intent(intent), None).reply.unwrap()
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
