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
    validation_pending: Option<(u64, u64, Selection, SkinIntent)>,
    sheet_keys: std::collections::BTreeSet<calc_host::SheetKey>,
    book: Workbook,
    /// The page's subscriptions: its number for each, and calc-host's.
    subscriptions: BTreeMap<u64, SubscriptionId>,
    events: Vec<Event>,
    snapshots: BTreeMap<u64, calc_host::CopySnapshot>,
    next_clip: u64,
    clipboard_session: String,
    paste_plan: Option<(u64, u64, Vec<CellEdit>)>,
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
            validation_pending: None,
            sheet_keys: book.host().sheets().iter().map(|s| s.key()).collect(),
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
        let mut out = None;
        let reply = (|| -> Result<Reply, String> {
            match request {
                Request::Find(options) => {
                    Ok(Reply::Found(search_selection(&options).and_then(
                        |selection| crate::search::find(&self.book, &selection, &options),
                    )))
                }
                Request::SelectionStats => Ok(Reply::SelectionStats(
                    crate::search::selection_stats(&self.book, self.book.session().selection()),
                )),
                Request::CanEdit(key) => Ok(Reply::CanEdit(
                    key.cell()
                        .ok_or_else(|| "The cell is unavailable".to_owned())
                        .and_then(|at| {
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
                                return Err(
                                    "Editing protected or macro worksheets is unavailable".into()
                                );
                            }
                            Ok(())
                        }),
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
                    geometry.split_window = self
                        .book
                        .host()
                        .loaded()
                        .sheet(sheet.id())
                        .is_some_and(|s| s.view().split_window);
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
        self.sheet_keys = book.host().sheets().iter().map(|s| s.key()).collect();
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
        let (edits, target) = match source {
            PasteSource::Internal { token, cut } => {
                let snapshot = self
                    .snapshots
                    .get(&token)
                    .ok_or("The copied snapshot expired; copy again")?;
                if cut && mode != wire::PasteMode::All {
                    return Err(
                        "Paste Special is unavailable for a cut; copy the cells instead".to_owned(),
                    );
                }
                let target = snapshot.target(to).map_err(|e| e.to_string())?;
                let edits = snapshot
                    .edits(self.book.host(), to, mode.host(), cut, None)
                    .map_err(|e| e.to_string())?;
                (edits, target)
            }
            PasteSource::Fill { from, down, series } => {
                let from = area(from)?;
                let snapshot = calc_host::CopySnapshot::capture(self.book.host(), from)
                    .map_err(|e| e.to_string())?;
                let edits = snapshot
                    .fill_edits(self.book.host(), to, down, series)
                    .map_err(|e| e.to_string())?;
                (edits, to)
            }
            PasteSource::External(rows) => {
                if mode == wire::PasteMode::Formats {
                    return Err("External table formats are not modelled; copy formats within this workbook".to_owned());
                }
                let (height, width) = crate::clipboard::matrix_shape(&rows)?;
                let target = if to.cell_count() == 1 {
                    GridArea::new(
                        to.first(),
                        CellKey::new(
                            to.sheet(),
                            to.first().row() + height - 1,
                            to.first().column() + width - 1,
                        )
                        .map_err(|_| "The paste reaches past the grid")?,
                    )
                    .unwrap()
                } else {
                    to
                };
                if target.row_count() % height != 0 || target.column_count() % width != 0 {
                    return Err(
                        "The paste area must be a whole multiple of the table's size".to_owned(),
                    );
                }
                calc_host::check_destination(self.book.host(), target, mode.host())
                    .map_err(|e| e.to_string())?;
                let mut edits = Vec::new();
                for row in target.first().row()..=target.last().row() {
                    for col in target.first().column()..=target.last().column() {
                        let at = CellKey::new(target.sheet(), row, col).unwrap();
                        let text = &rows[((row - target.first().row()) % height) as usize]
                            [((col - target.first().column()) % width) as usize];
                        let input = calc_host::interpret_entry_in(
                            text,
                            Some(&self.book.host().entry_environment()),
                        )
                        .map_err(|e| e.to_string())?;
                        if mode == wire::PasteMode::Values
                            && matches!(input, Some(calc_host::CellInput::Formula(_)))
                        {
                            return Err("External formula text has no calculated value; choose Formulas or All".to_owned());
                        }
                        edits.push(match input {
                            Some(input) => CellEdit::Set { at, input },
                            None => CellEdit::Clear {
                                area: GridArea::cell(at),
                            },
                        });
                    }
                }
                (edits, target)
            }
        };
        for edit in &edits {
            if let CellEdit::Set { at, input } = edit {
                if let Err(failure) = self.book.host().check_entry(*at, Some(input)) {
                    return Err(failure.to_string());
                }
            }
        }
        let replaces = if mode == wire::PasteMode::Formats {
            0
        } else {
            self.book
                .host()
                .model()
                .cells(target)
                .into_iter()
                .flatten()
                .count() as u64
        };
        let id = self.next_clip;
        self.next_clip += 1;
        self.paste_plan = Some((id, self.book.host().revision().get(), edits));
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
        let styles = host.loaded().styles();
        for at in &keys {
            let style = host.cell_style(*at);
            let format = styles.cell_formats.get(style as usize);
            if format.is_none() && style != 0 {
                return Err("The cell style is unavailable".to_owned());
            }
            if format
                .into_iter()
                .chain(format.and_then(|f| {
                    f.xf_id
                        .and_then(|i| styles.cell_style_formats.get(i as usize))
                }))
                .any(|format| {
                    format.font_id.unwrap_or(0) != 0
                        || format.alignment.as_ref().is_some_and(|a| {
                            a.wrap_text
                                || a.shrink_to_fit
                                || a.text_rotation.as_deref().is_some_and(|n| n != "0")
                                || a.indent.as_deref().is_some_and(|n| n != "0")
                                || matches!(
                                    a.horizontal.as_deref(),
                                    Some("fill" | "justify" | "distributed")
                                )
                        })
                })
            {
                return Err("AutoFit of custom fonts, wrapping, rotation or indentation is unavailable here".to_owned());
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
            if shown.text.contains(['\n', '\r']) {
                return Err("AutoFit of multiline cells is unavailable here".to_owned());
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

    fn axis_format_edits(
        &self,
        sheet: Sheet,
        rows: bool,
        first: u32,
        count: u32,
        size_bits: Option<u64>,
        hidden: Option<bool>,
    ) -> Result<Vec<CellEdit>, String> {
        let host = self.book.host();
        let loaded = host
            .loaded()
            .sheet(sheet.id())
            .ok_or("The sheet is unavailable")?;
        if loaded.protected() {
            return Err("Dimensions on a protected sheet are not available here".to_owned());
        }
        let axis = if rows {
            calc_host::Axis::Rows
        } else {
            calc_host::Axis::Columns
        };
        let last = first
            .checked_add(count)
            .and_then(|n| n.checked_sub(1))
            .filter(|n| first >= 1 && count > 0 && *n <= axis.size())
            .ok_or("The dimensions are outside the worksheet")?;
        if count > 16_384 {
            return Err("Change at most 16384 rows or columns at once".to_owned());
        }
        if rows
            && host
                .model()
                .definitions()
                .hidden_rows(sheet.id(), first, last)
                .any(|(_, why)| why == calc_host::RowHidden::Filtered)
        {
            return Err("Rows hidden by a filter cannot be resized or shown here".to_owned());
        }
        let size = size_bits
            .map(|b| crate::grid::axis::stored_size(rows, f64::from_bits(b)))
            .transpose()
            .map_err(str::to_owned)?;
        let mut edits = Vec::new();
        for index in first..=last {
            let old = host
                .model()
                .definitions()
                .axis_layouts()
                .get(sheet.id(), axis, index);
            let mut new = old;
            if let Some(size) = size {
                new = match size {
                    None => old.with_hidden(true),
                    Some(n) => {
                        calc_host::AxisFormat::new(Some(n), false, !rows).expect("a checked size")
                    }
                };
            }
            if let Some(hidden) = hidden {
                new = if !hidden && new.size() == Some(0.0) {
                    calc_host::AxisFormat::default()
                } else {
                    new.with_hidden(hidden)
                };
            }
            if old != new {
                edits.push(CellEdit::AxisFormat(
                    calc_host::AxisFormatEdit::new(sheet.id(), axis, index, new)
                        .expect("checked band indices"),
                ));
            }
        }
        Ok(edits)
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

    fn edit_conditional(
        &self,
        sheet: calc_host::value::SheetId,
        revision: u64,
        index: Option<u32>,
        change: wire::ConditionalChange,
    ) -> Result<Vec<CellEdit>, String> {
        let host = self.book.host();
        if revision != host.revision().get() {
            return Err("Rules changed; reopen the current rule and try again".into());
        }
        if host.loaded().sheet(sheet).is_some_and(|s| s.protected()) {
            return Err("Worksheet protection prevents conditional rule edits".into());
        }
        if host
            .loaded()
            .sheet(sheet)
            .is_some_and(|s| s.conditional_formats().iter().any(|b| b.unread_container))
        {
            return Err("This worksheet has additional conditional-rule metadata; editing its rules is unavailable and the file is preserved (dc-971p.8)".into());
        }
        let mut rules = host.model().definitions().conditional_rules(sheet).to_vec();
        let ranges = |text: &str| -> Result<Vec<GridArea>, String> {
            let at = CellKey::new(sheet, 1, 1).unwrap();
            let mut areas = Vec::new();
            for token in text.split_whitespace() {
                match host.resolve_reference_text(at, token) {
                    calc_host::ReferenceTarget::Area(a) if a.sheet() == sheet => areas.push(a),
                    _ => {
                        return Err(
                            "Applies to needs space-separated ranges on this worksheet".into()
                        );
                    }
                }
            }
            if areas.is_empty() || areas.len() > 128 {
                return Err("Choose between 1 and 128 ranges".into());
            }
            Ok(areas)
        };
        if let wire::ConditionalChange::Add {
            kind,
            applies,
            color,
        } = change
        {
            let mut source = calc_host::new_conditional_rule(
                ranges(&applies)?,
                &kind,
                color.trim_start_matches('#'),
            )?;
            source.priority = rules
                .iter()
                .map(|r| r.priority)
                .max()
                .unwrap_or(0)
                .saturating_add(1);
            rules.push(source);
        } else {
            let i = index
                .map(|i| i as usize)
                .filter(|i| *i < rules.len())
                .ok_or("Select a rule first")?;
            let parsed = calc_host::conditional_rule(&rules[i])?;
            match change {
                wire::ConditionalChange::Update {
                    applies,
                    formulas,
                    stop,
                } => {
                    let areas = ranges(&applies)?;
                    if parsed.extension_id.is_some() {
                        return Err("Editing extension-backed rules is unavailable; the imported rule is preserved (dc-971p.8)".into());
                    }
                    let mut formulas = formulas;
                    if formulas != rules[i].formulas {
                        if !matches!(parsed.kind.as_str(), "cellIs" | "expression")
                            || formulas.len() != rules[i].formulas.len()
                        {
                            return Err(
                                "This rule condition cannot be edited yet (dc-971p.8)".into()
                            );
                        }
                        for formula in &mut formulas {
                            let text = calc_host::FormulaText::new(
                                calc_host::value::ExcelText::from_utf16_code_units(
                                    formula
                                        .trim()
                                        .trim_start_matches('=')
                                        .encode_utf16()
                                        .collect(),
                                ),
                            )
                            .map_err(|e| e.to_string())?;
                            let form = calc_host::file_form_with_links(
                                &text,
                                host.model().definitions().names(),
                                &host.model().definitions().external_file_names(),
                            )
                            .map_err(|e| format!("Conditional formula is unavailable: {e:?}"))?;
                            *formula = String::from_utf16_lossy(&form.text)
                                .trim_start_matches('=')
                                .to_owned();
                        }
                    }
                    rules[i].areas = areas;
                    rules[i].formulas = formulas;
                    rules[i].stop = stop
                        && !matches!(parsed.kind.as_str(), "colorScale" | "dataBar" | "iconSet");
                    rules[i].authored = true;
                }
                wire::ConditionalChange::Remove => {
                    if parsed.extension_id.is_some() {
                        return Err("Removing extension-backed rules is unavailable; the imported rule is preserved (dc-971p.8)".into());
                    }
                    rules.remove(i);
                }
                wire::ConditionalChange::Move { up } => {
                    let mut order = (0..rules.len()).collect::<Vec<_>>();
                    order.sort_by_key(|&n| rules[n].priority);
                    let old = order.iter().position(|&n| n == i).unwrap();
                    let destination = if up {
                        old.saturating_sub(1)
                    } else {
                        (old + 1).min(order.len() - 1)
                    };
                    order.swap(old, destination);
                    for (rank, index) in order.into_iter().enumerate() {
                        rules[index].priority = rank as i64 + 1;
                        rules[index].authored = true;
                    }
                }
                _ => unreachable!(),
            }
        }
        let mut order = (0..rules.len()).collect::<Vec<_>>();
        order.sort_by_key(|&n| rules[n].priority);
        for (rank, index) in order.into_iter().enumerate() {
            rules[index].priority = rank as i64 + 1;
        }
        Ok(vec![CellEdit::ConditionalRules { sheet, rules }])
    }

    fn intent(&mut self, intent: Intent) -> Result<Reply, String> {
        if let Intent::AcceptValidation { token } = intent {
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

        let intent = match intent {
            Intent::Replace {
                options,
                replacement,
                revision,
                only,
            } => {
                if revision != self.book.host().revision().get() {
                    return Ok(self.done(
                        false,
                        Some("The workbook changed; run Find again before replacing".into()),
                        0.0,
                    ));
                }
                let prepared = search_selection(&options).and_then(|selection| {
                    crate::search::replace_edits(
                        &self.book,
                        &selection,
                        &options,
                        &replacement,
                        only.map(cell).transpose()?,
                    )
                });
                match prepared {
                    Ok(edits) if !edits.is_empty() => SkinIntent::Edit { edits },
                    Ok(_) => return Ok(self.done(false, Some("No entries changed".into()), 0.0)),
                    Err(error) => return Ok(self.done(false, Some(error), 0.0)),
                }
            }
            Intent::EditConditional {
                sheet,
                revision,
                index,
                change,
            } => match self.edit_conditional(sheet.id(), revision, index, change) {
                Ok(edits) => SkinIntent::Edit { edits },
                Err(message) => return Ok(self.done(false, Some(message), 0.0)),
            },
            Intent::ShiftCells {
                area: a,
                down,
                insert,
            } => SkinIntent::Edit {
                edits: calc_host::shift_cell_edits(self.book.host(), area(a)?, down, insert)?,
            },
            Intent::EditSheet { sheet, change } => {
                use calc_host::{SheetEdit, SheetKey, SheetName, SheetVisibility};
                use wire::SheetChange as C;
                let model = self.book.host().model();
                let id = sheet.map(Sheet::id);
                let require = || {
                    id.filter(|id| model.sheet_by_id(*id).is_some())
                        .ok_or_else(|| "The worksheet is unavailable".to_owned())
                };
                let name = |text: String| -> Result<SheetName, String> {
                    if text.eq_ignore_ascii_case("History") {
                        return Err(
                            "History is reserved by Excel; choose another worksheet name"
                                .to_owned(),
                        );
                    }
                    SheetName::new(calc_host::value::ExcelText::from_utf16_code_units(
                        text.encode_utf16().collect(),
                    ))
                    .map_err(|e| e.to_string())
                };
                let edit = match change {
                    C::Add { name: text } => {
                        let key = (1u32..)
                            .map(SheetKey::new)
                            .find(|key| {
                                !self.sheet_keys.contains(key)
                                    && model.sheets().iter().all(|s| s.key() != *key)
                            })
                            .ok_or("No sheet keys remain")?;
                        self.sheet_keys.insert(key);
                        SheetEdit::Insert {
                            key,
                            name: name(text)?,
                            position: std::num::NonZeroU32::new(model.sheets().len() as u32 + 1)
                                .unwrap(),
                        }
                    }
                    C::Rename { name: text } => SheetEdit::Rename {
                        sheet: require()?,
                        name: name(text)?,
                    },
                    C::Move { position } => {
                        if position == 0 || position as usize > model.sheets().len() {
                            return Err("Choose a worksheet position in the workbook".to_owned());
                        }
                        SheetEdit::Move {
                            sheet: require()?,
                            position: std::num::NonZeroU32::new(position).unwrap(),
                        }
                    }
                    C::Remove => SheetEdit::Remove { sheet: require()? },
                    C::Hide | C::Show => {
                        let sheet = require()?;
                        if model.sheet_by_id(sheet).unwrap().visibility()
                            == SheetVisibility::VeryHidden
                        {
                            return Err(
                                "This sheet is VeryHidden; unhiding it is unavailable here"
                                    .to_owned(),
                            );
                        }
                        SheetEdit::SetVisibility {
                            sheet,
                            visibility: if change == C::Hide {
                                SheetVisibility::Hidden
                            } else {
                                SheetVisibility::Visible
                            },
                        }
                    }
                };
                SkinIntent::Edit {
                    edits: vec![CellEdit::Sheet(edit)],
                }
            }
            Intent::FitContents {
                sheet,
                rows,
                revision,
                sizes,
            } => {
                if revision != self.book.host().revision().get() {
                    return Err("The workbook changed while fitting; try again".to_owned());
                }
                if sizes.len() > 16384 {
                    return Err("Fit at most 16384 rows or columns at once".to_owned());
                }
                let axis = if rows { Axis::Rows } else { Axis::Columns };
                let mut edits = Vec::new();
                let mut seen = std::collections::BTreeSet::new();
                for (index, size) in sizes {
                    if !seen.insert(index) {
                        return Err("A fitted band was repeated".to_owned());
                    }
                    let hidden = self
                        .book
                        .host()
                        .model()
                        .definitions()
                        .axis_layouts()
                        .get(sheet.id(), axis, index)
                        .hidden();
                    edits.extend(self.axis_format_edits(
                        sheet,
                        rows,
                        index,
                        1,
                        Some(size),
                        Some(hidden),
                    )?);
                }
                if edits.is_empty() {
                    return Ok(self.done(true, None, 0.0));
                }
                SkinIntent::Edit { edits }
            }

            Intent::AxisFormat {
                sheet,
                rows,
                first,
                count,
                size_bits,
                hidden,
            } => {
                let edits = self.axis_format_edits(sheet, rows, first, count, size_bits, hidden)?;
                if edits.is_empty() {
                    return Ok(self.done(true, None, 0.0));
                }
                SkinIntent::Edit { edits }
            }
            Intent::FreezePanes {
                sheet,
                rows,
                columns,
            } => {
                if self.book.host().model().sheet_by_id(sheet.id()).is_none() {
                    return Err("The sheet is unavailable".to_owned());
                }
                let panes = calc_host::FrozenPanes::new(rows, columns);
                if panes.is_none() && (rows != 0 || columns != 0) {
                    return Err("The frozen boundary is outside the worksheet".to_owned());
                }
                if panes
                    == self
                        .book
                        .host()
                        .model()
                        .definitions()
                        .frozen_panes(sheet.id())
                {
                    return Ok(self.done(true, None, 0.0));
                }
                SkinIntent::Edit {
                    edits: vec![CellEdit::FrozenPanes {
                        sheet: sheet.id(),
                        panes,
                    }],
                }
            }
            Intent::ApplyPaste { id } => {
                let Some((held, revision, edits)) = self.paste_plan.take() else {
                    return Err("The paste preview expired; paste again".to_owned());
                };
                if held != id || revision != self.book.host().revision().get() {
                    return Err("The workbook changed after the preview; paste again".to_owned());
                }
                if edits.is_empty() {
                    return Ok(self.done(true, None, 0.0));
                }
                SkinIntent::Edit { edits }
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
            other => host_intent(other)?,
        };
        self.dispatch_entry(intent, false)
    }

    fn dispatch_entry(&mut self, intent: SkinIntent, confirmed: bool) -> Result<Reply, String> {
        if !confirmed {
            self.validation_pending = None;
            let targets = match &intent {
                SkinIntent::EnterCell { at, text } => vec![(*at, text.as_str())],
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

/// The page's intent as calc-host's.
fn host_intent(intent: Intent) -> Result<SkinIntent, String> {
    Ok(match intent {
        Intent::ShiftCells { .. } => return Err("cell shifts need the workbook worker".to_owned()),
        Intent::EditSheet { .. } => {
            return Err("sheet commands need the workbook worker".to_owned());
        }
        Intent::FitContents { .. } => return Err("fitting needs the workbook worker".to_owned()),
        Intent::AxisFormat { .. } => return Err("dimensions need the workbook worker".to_owned()),
        Intent::FreezePanes { .. } => return Err("pane editing needs the workbook view".to_owned()),
        Intent::ApplyPaste { .. } => return Err("paste needs its worker preview".to_owned()),
        Intent::Replace { .. }
        | Intent::EditConditional { .. }
        | Intent::AcceptValidation { .. } => {
            return Err("Validation decisions are handled by the worker".into());
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
        Intent::Navigate { .. } => {
            return Err("navigation requires the workbook selection".to_owned());
        }
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
        assert!(server.handle(enter(at, "original"), None).reply.is_ok());
        let Reply::FitContents { revision, cells } = server
            .handle(
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
                .handle(enter(CellKey::new(sheet, 2, 1).unwrap(), "later"), None)
                .reply
                .is_ok()
        );
        let answer = server.handle(
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
            .handle(
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
        let answer = server.handle(
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

fn search_selection(options: &wire::FindOptions) -> Result<Selection, String> {
    let areas = options
        .areas
        .iter()
        .copied()
        .map(area)
        .collect::<Result<Vec<_>, _>>()?;
    if areas.len() > 1024 {
        return Err("Search supports at most1024 selection areas".into());
    }
    Ok(Selection {
        active: areas.first().map(|a| a.first()),
        areas,
    })
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
                .handle(
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
            .handle(
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
        server.handle(Request::New, None).reply.unwrap();
        let sheet = server.book.host().sheets()[0].id();
        let anchor = CellKey::new(sheet, 1, 3).unwrap();
        server
            .handle(
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
