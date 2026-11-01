//! One open workbook as DNA Calc holds it: the calc-host session every skin drives, the document's
//! name, whether it changed since it was opened or saved, and the deltas its subscriptions
//! received, waiting for the skins to take them.
//!
//! Nothing here reaches the engine or the document except through [`HostSession`]: opening is
//! [`HostSession::open`], every change is a [`SkinIntent`], every value read is a projection
//! ([`calc_host::project`]) or a subscription's [`HostDelta`], and saving is
//! [`HostSession::save`]. A subscription's sink only queues its deltas ([`Workbook::take_deltas`]),
//! so a skin applies them after the intent returns, never while the session is still working.
//!
//! The application's document lifecycle is ported from DnaTreeCalc (commit bf6dfd7)
//! `src/dnacalc-app/src/document.rs` (`DocumentController`: the document's name, open from bytes,
//! save to bytes, the dirty mark and the status line) onto calc-host; the legacy dispatcher, the
//! in-memory demo workbook and the desktop file bridge are left behind.

use crate::cells;
use calc_host::value::{CellKey, GridArea, SheetId};
use calc_host::{
    CalcMode, FormatProfile, HostDelta, HostOptions, HostProgress, HostSession, HostState,
    IntentError, IntentReceipt, Interest, MemorySource, OpenPolicy, PackageRead, ProjectedCell,
    ProjectionRequest, SavePolicy, SaveReceipt, SkinIntent, SubscriptionId,
};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

/// The locale DNA Calc reads entries and shows numbers in.
pub const LOCALE: &str = "en-US";

/// The name a new workbook gets until it is saved under another.
pub const NEW_WORKBOOK_NAME: &str = "Book1.xlsx";

/// The options every workbook is opened with: the en-US locale profile, and the calculation mode
/// the package declares.
pub fn host_options() -> HostOptions {
    HostOptions {
        locale: FormatProfile::for_culture(LOCALE),
        ..HostOptions::default()
    }
}

/// The linear memory reserved before a workbook is opened in the browser, per byte of its
/// package, and at most (dc-26q). The allocator a wasm module brings (dlmalloc) grows linear memory
/// in small steps as the open allocates, and Chrome's memory growth is slow enough that, before
/// this reservation, it took about half of the default tariff cap workbook's open (4.2 of 8.4
/// seconds in a CPU profile). One allocation of the expected size, freed at once, grows the
/// memory in one step and leaves it to the allocator. That workbook (5.3 MB) used 276 MB.
pub const OPEN_RESERVE_PER_BYTE: usize = 64;
pub const OPEN_RESERVE_MAX: usize = 1 << 30;

/// Grows the browser's linear memory for opening a package of `bytes` bytes
/// ([`OPEN_RESERVE_PER_BYTE`]); nothing when the memory cannot grow that far, and nothing off the
/// browser, whose allocator gets memory from the system.
pub fn reserve_for_open(bytes: usize) {
    #[cfg(target_arch = "wasm32")]
    {
        let want = bytes
            .saturating_mul(OPEN_RESERVE_PER_BYTE)
            .min(OPEN_RESERVE_MAX);
        let mut reserve: Vec<u8> = Vec::new();
        let _ = reserve.try_reserve_exact(want);
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = bytes;
    }
}

/// Why a workbook did not open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenRefused(pub String);

pub use dna_calc_model::projection::SheetTab;

/// An open workbook.
pub struct Workbook {
    session: HostSession,
    name: String,
    changed: bool,
    inbox: Rc<RefCell<Vec<HostDelta>>>,
    /// The workbook's redefined palette entries 1 to 56, read once at open; `None` when it
    /// redefines none.
    palette: Option<Vec<Option<String>>>,
    /// How long opening recalculated the workbook, in milliseconds of the browser's clock;
    /// `None` for manual mode or when the admitted calculation is still pending.
    calculated_on_open: Option<f64>,
    /// Each sheet's conditional formats, prepared for the values as they stand; emptied when an
    /// intent commits an edit or recalculates (dc-6yw).
    display_metrics:
        RefCell<std::collections::BTreeMap<SheetId, Rc<dna_calc_model::geometry::GridMetrics>>>,
    conditional: RefCell<std::collections::BTreeMap<SheetId, Rc<calc_host::ConditionalFormats>>>,
}

impl std::fmt::Debug for Workbook {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Workbook")
            .field("name", &self.name)
            .field("changed", &self.changed)
            .field("session", &self.session)
            .finish()
    }
}

impl Workbook {
    /// A new workbook: calc-host's blank package, one empty sheet.
    pub fn new_blank() -> Workbook {
        Workbook::open(NEW_WORKBOOK_NAME, calc_host::blank_package())
            .expect("the blank package opens")
    }

    /// Opens a package's bytes. Unless the package declares manual calculation, the workbook is
    /// then recalculated, so every formula shows what the engine calculates for it (or its
    /// refusal) rather than the value the file stored.
    pub fn open(name: &str, bytes: Vec<u8>) -> Result<Workbook, OpenRefused> {
        Self::open_with_budget(name, bytes, calc_host::WorkBudget::UNLIMITED)
    }

    pub fn open_with_budget(
        name: &str,
        bytes: Vec<u8>,
        budget: calc_host::WorkBudget,
    ) -> Result<Workbook, OpenRefused> {
        let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(bytes));
        let mut session = HostSession::open(source, &OpenPolicy::default(), host_options())
            .map_err(|failure| OpenRefused(failure.to_string()))?;
        session.set_calculation_budget(budget);
        let mut workbook = Workbook {
            session,
            name: name.to_owned(),
            changed: false,
            inbox: Rc::default(),
            calculated_on_open: None,
            palette: None,
            conditional: RefCell::default(),
            display_metrics: RefCell::default(),
        };
        let palette: Vec<Option<String>> = (1..=56u8)
            .map(|index| workbook.host().palette_color(index))
            .collect();
        if palette.iter().any(Option::is_some) {
            workbook.palette = Some(palette);
        }
        if workbook.session.calc_mode() != CalcMode::Manual {
            let started = crate::services::now_ms();
            workbook.session.dispatch(SkinIntent::Recalculate);
            if !workbook.session.has_pending_calculation() {
                workbook.calculated_on_open = Some(crate::services::now_ms() - started);
            }
        }
        Ok(workbook)
    }

    /// How long opening recalculated the workbook, in milliseconds (0 off the browser); `None`
    /// for a file that calculates manually, whose formulas show the values the file stored, as
    /// file caches, until Calculate.
    pub fn calculated_on_open(&self) -> Option<f64> {
        self.calculated_on_open
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// The name a save is offered under: the workbook's, but an Excel binary workbook's (`.xlsb`),
    /// which is saved as SpreadsheetML (dc-5sr), with `.xlsm` when it is macro-enabled and
    /// `.xlsx` otherwise.
    pub fn save_name(&self) -> String {
        match self.name.rsplit_once('.') {
            Some((stem, extension)) if extension.eq_ignore_ascii_case("xlsb") => {
                let macros = self.host().model().definitions().macro_enabled();
                format!("{stem}.{}", if macros { "xlsm" } else { "xlsx" })
            }
            _ => self.name.clone(),
        }
    }

    /// Whether an edit committed since the workbook was opened or last saved.
    pub fn changed(&self) -> bool {
        self.changed
    }

    pub fn host(&self) -> &HostState {
        self.session.host()
    }

    pub fn session(&self) -> &HostSession {
        &self.session
    }

    /// Native composition installs XLL/VBA/WIT adapters through one callable table.
    pub fn set_user_functions(&mut self, functions: calc_host::CallableTable) {
        self.session.set_user_functions(functions);
    }
    pub fn complete_async(
        &mut self,
        token: &calc_host::AsyncToken,
        value: calc_host::value::CalcValue,
    ) -> calc_host::AsyncDisposition {
        self.session.complete_async(token, value)
    }

    pub fn load_vba_module(
        &mut self,
        name: &str,
        source: &str,
        max_steps: u64,
    ) -> Result<(), String> {
        if source.len() > 1_048_576 {
            return Err("VBA module source exceeds one MiB".into());
        }
        let functions = calc_vba::load_module(name, source, max_steps)?;
        self.session.set_user_functions(functions);
        Ok(())
    }
    pub fn set_calculation_budget(&mut self, budget: calc_host::WorkBudget) {
        self.session.set_calculation_budget(budget);
    }
    pub fn has_pending_calculation(&self) -> bool {
        self.session.has_pending_calculation()
    }
    pub fn take_calculation_events(&mut self) -> Vec<calc_host::CalcEvent> {
        self.session.take_calculation_events()
    }
    pub fn cancel_calculation(&mut self) {
        self.session.cancel_calculation();
    }
    pub fn advance_calculation(&mut self) -> HostProgress {
        let progress = self.session.advance_calculation();
        if matches!(progress, HostProgress::Published(_)) {
            self.conditional.borrow_mut().clear();
            self.display_metrics.borrow_mut().clear();
        }
        progress
    }

    pub fn profile(&self) -> Option<&FormatProfile> {
        self.session.host().options().locale.as_ref()
    }

    /// The sheets, in workbook order.
    pub fn sheets(&self) -> Vec<SheetTab> {
        self.host()
            .sheets()
            .iter()
            .map(|entry| SheetTab {
                id: entry.id(),
                name: entry.name().text().to_string_lossy(),
            })
            .collect()
    }

    /// Carries out an intent; an intent that committed an edit marks the workbook changed.
    pub fn dispatch(&mut self, intent: SkinIntent) -> IntentReceipt {
        let receipt = self.session.dispatch(intent);
        if receipt.committed.is_some() {
            self.changed = true;
        }
        if receipt.committed.is_some() || receipt.recalc.is_some() {
            self.conditional.borrow_mut().clear();
            self.display_metrics.borrow_mut().clear();
        }
        receipt
    }

    pub fn display_metrics(&self, sheet: SheetId) -> Rc<dna_calc_model::geometry::GridMetrics> {
        let mut cache = self.display_metrics.borrow_mut();
        Rc::clone(cache.entry(sheet).or_insert_with(|| {
            Rc::new(dna_calc_model::geometry::sheet_metrics(
                self.host()
                    .sheet_geometry(sheet)
                    .as_ref()
                    .map(crate::wire::geometry_of)
                    .as_ref(),
            ))
        }))
    }
    /// The conditional formats of a sheet, prepared once for the values as they stand.
    pub fn conditional_formats(&self, sheet: SheetId) -> Rc<calc_host::ConditionalFormats> {
        Rc::clone(
            self.conditional
                .borrow_mut()
                .entry(sheet)
                .or_insert_with(|| Rc::new(self.host().conditional_formats(sheet))),
        )
    }

    /// A palette colour as `#rrggbb`: the eight basic colours (0 to 7), the workbook's palette
    /// (8 to 63, its entries 1 to 56), the system's foreground (64) and background (65).
    fn indexed_color(&self, index: u32) -> Option<String> {
        cells::indexed_color(self.host(), index)
    }

    fn look_color(&self, color: &Option<calc_host::LookColor>) -> Option<String> {
        match color.as_ref()? {
            calc_host::LookColor::Rgb(text) => Some(text.clone()),
            calc_host::LookColor::Indexed(index) => self.indexed_color(*index),
        }
    }

    /// A projected cell as a skin shows it, in its number format and with what its sheet's
    /// conditional formats give it (dc-6yw): a rule's number format in place of the cell's, its
    /// fill, font, data bar and icon.
    pub fn shown(&self, cell: &calc_host::ProjectedCell) -> cells::CellShown {
        let at = cell.at();
        let formats = self.conditional_formats(at.sheet());
        let look = if formats.rule_count() == 0 {
            None
        } else {
            formats.look(self.host(), at)
        };
        let Some(look) = look else {
            let mut shown = cells::shown(cell, self.profile(), self.cell_format(at));
            shown.style = crate::autofit::text_style(
                self.host(),
                at,
                self.display_metrics(at.sheet()).width(at.column()),
            );
            return shown;
        };
        let format = match look.number_format.as_deref() {
            Some(code) => Some(cells::CellFormat {
                code,
                date1904: self.host().declared_date1904(),
                palette: self.palette.as_deref(),
            }),
            None => self.cell_format(at),
        };
        let mut shown = cells::shown(cell, self.profile(), format);
        shown.style = crate::autofit::text_style(
            self.host(),
            at,
            self.display_metrics(at.sheet()).width(at.column()),
        );
        let font_color = self.look_color(&look.font_color);
        let bar = look.bar.as_ref().map(|bar| {
            (
                bar.fraction,
                self.look_color(&bar.color)
                    .unwrap_or_else(|| "#638ec6".to_owned()),
                bar.gradient,
            )
        });
        let hide_value = look.bar.as_ref().is_some_and(|bar| !bar.show_value)
            || look.icon.as_ref().is_some_and(|icon| !icon.show_value);
        shown.conditional = Some(cells::ConditionalShown {
            fill: self.look_color(&look.fill),
            font_color,
            bold: look.bold.unwrap_or(false),
            italic: look.italic.unwrap_or(false),
            underline: look.underline.unwrap_or(false),
            strike: look.strike.unwrap_or(false),
            bar,
            icon: look
                .icon
                .as_ref()
                .map(|icon| (icon.set.clone(), icon.index)),
            hide_value,
        });
        shown
    }

    /// The data validation rule of a cell (dc-6yw): its drop-down and input message.
    pub fn validation(&self, at: CellKey) -> Option<calc_host::ValidationRule> {
        self.session.host().validation(at)
    }

    /// The items a cell's list drop-down offers, as text to enter (dc-6yw).
    pub fn validation_items(&self, at: CellKey) -> Vec<String> {
        let Some(items) = self.session.host().validation_list(at) else {
            return Vec::new();
        };
        items
            .iter()
            .map(calc_host::chart::text_of)
            .filter(|text| !text.is_empty())
            .collect()
    }

    pub fn validation_failure(
        &self,
        at: CellKey,
        text: &str,
    ) -> Option<calc_host::ValidationFailure> {
        let host = self.session.host();
        let input = calc_host::interpret_entry_in(text, Some(&host.entry_environment())).ok()?;
        host.check_entry(at, input.as_ref()).err()
    }

    /// What a warning or information rule of the cell says about `text` as its entry, which the
    /// session takes (a stop rule's refusal comes back as the intent's error).
    pub fn validation_note(&self, at: CellKey, text: &str) -> Option<calc_host::ValidationFailure> {
        let host = self.session.host();
        let input = calc_host::interpret_entry_in(text, Some(&host.entry_environment())).ok()?;
        host.check_entry(at, input.as_ref())
            .err()
            .filter(|failure| failure.style != calc_host::ErrorStyle::Stop)
    }

    /// Text typed into a cell, read as Excel reads a cell entry: one undoable edit.
    pub fn enter(&mut self, at: CellKey, text: &str) -> IntentReceipt {
        self.dispatch(SkinIntent::EnterCell {
            at,
            text: text.to_owned(),
        })
    }

    /// Registers a subscription whose deltas wait in the workbook's inbox.
    pub fn subscribe(&mut self, interest: Interest) -> Result<SubscriptionId, IntentError> {
        let inbox = Rc::clone(&self.inbox);
        self.session.subscribe(
            interest,
            Box::new(move |delta: &HostDelta| inbox.borrow_mut().push(delta.clone())),
        )
    }

    pub fn unsubscribe(&mut self, id: SubscriptionId) -> bool {
        self.session.unsubscribe(id)
    }

    /// The deltas received since the last call, in the order they were delivered.
    pub fn take_deltas(&mut self) -> Vec<HostDelta> {
        std::mem::take(&mut *self.inbox.borrow_mut())
    }

    /// The cells requested, projected (empty for a sheet the workbook does not hold).
    pub fn project(&self, cells: Vec<CellKey>) -> Vec<ProjectedCell> {
        calc_host::project(self.host(), &ProjectionRequest { cells })
            .map(|page| page.cells().to_vec())
            .unwrap_or_default()
    }

    /// One cell, projected.
    pub fn cell(&self, at: CellKey) -> Option<ProjectedCell> {
        self.project(vec![at]).into_iter().next()
    }

    /// The number format of the cell at `at`, with the workbook's date system.
    pub fn cell_format(&self, at: CellKey) -> Option<cells::CellFormat<'_>> {
        Some(cells::CellFormat {
            code: self.host().number_format(at)?,
            date1904: self.host().declared_date1904(),
            palette: self.palette.as_deref(),
        })
    }

    /// The used area of a sheet ([`HostState::used_area`]).
    pub fn used_area(&self, sheet: SheetId) -> Option<GridArea> {
        self.host().used_area(sheet)
    }

    /// Saves the current revision as a package's bytes; the workbook is unchanged since.
    pub fn save(&mut self) -> Result<(Vec<u8>, SaveReceipt), String> {
        let mut bytes = Vec::new();
        let receipt = self
            .session
            .save(&SavePolicy::default(), &mut bytes)
            .map_err(|failure| failure.to_string())?;
        self.changed = false;
        Ok((bytes, receipt))
    }

    /// The non-empty cells of a sheet as they stand, projected: what a subscription to the whole
    /// sheet is first given ([`Interest::Sheet`]), taken through a subscription that ends here.
    pub fn sheet_cells(&mut self, sheet: SheetId) -> Vec<ProjectedCell> {
        let waiting = self.take_deltas();
        let cells = match self.subscribe(Interest::Sheet(sheet)) {
            Ok(id) => {
                let delta = self.take_deltas();
                self.unsubscribe(id);
                delta.into_iter().flat_map(|delta| delta.cells).collect()
            }
            Err(_) => Vec::new(),
        };
        self.inbox.borrow_mut().extend(waiting);
        cells
    }

    /// The workbook's formula cells by provenance, per sheet, and the refusals by reason. A
    /// formula cell is one Excel counts as holding a formula ([`HostState::holds_formula`]: legacy
    /// array members and data-table cells included, a dynamic array's spilled cells not), the set
    /// `cargo xtask compare`'s inventory holds.
    pub fn census(&mut self) -> Census {
        let mut census = Census::default();
        for tab in self.sheets() {
            let mut sheet = SheetCensus {
                used: self
                    .used_area(tab.id)
                    .map(|area| cells::area_label(self.host(), area)),
                identity: Some(crate::wire::Sheet::of(tab.id)),
                name: tab.name,
                ..SheetCensus::default()
            };
            for cell in self.sheet_cells(tab.id) {
                if cell.input().is_some() {
                    sheet.inputs += 1;
                }
                let shown = cells::shown(&cell, None, None);
                if !self.host().holds_formula(cell.at()) {
                    continue;
                }
                sheet.formulas += 1;
                match shown.provenance {
                    cells::Provenance::Calculated => sheet.calculated += 1,
                    cells::Provenance::Refused => {
                        sheet.refused += 1;
                        if let calc_host::ProjectedValue::NotModeled { refusal, .. } = cell.value()
                        {
                            let reason = cells::refusal_text(self.host(), refusal);
                            *census.refusals.entry(reason).or_default() += 1;
                        }
                    }
                    cells::Provenance::FileCache => sheet.file_cached += 1,
                    cells::Provenance::Stale => sheet.stale += 1,
                    cells::Provenance::Empty | cells::Provenance::Literal => {}
                }
            }
            census.sheets.push(sheet);
        }
        census
    }

    /// Project through the same host route as the census; no cached value becomes a result.
    pub fn refusal_cells(&self, reason: &str, page: usize) -> crate::wire::RefusalCells {
        let mut found = crate::wire::RefusalCells {
            page,
            ..Default::default()
        };
        let start = page.saturating_mul(50);
        for tab in self.sheets() {
            for cell in self.project(self.host().formula_cells(tab.id).into_iter().collect()) {
                let calc_host::ProjectedValue::NotModeled { refusal, .. } = cell.value() else {
                    continue;
                };
                if cells::refusal_text(self.host(), refusal) != reason {
                    continue;
                }
                if found.total >= start && found.cells.len() < 50 {
                    found.cells.push((
                        crate::wire::Key::of(cell.at()),
                        cells::cell_label(self.host(), cell.at()),
                    ));
                }
                found.total += 1;
            }
        }
        found
    }

    /// The outcome of the last recalculation an intent ran, in words.
    pub fn recalc_text(progress: &HostProgress) -> String {
        match progress {
            HostProgress::Current => "nothing to recalculate".to_owned(),
            HostProgress::Cancelled => "calculation cancelled".to_owned(),
            HostProgress::Manual => "manual calculation: press Calculate".to_owned(),
            HostProgress::Calculating { .. } => "calculating".to_owned(),
            HostProgress::Waiting { .. } => "waiting for a provider".to_owned(),
            HostProgress::Published(delta) if delta.completion().is_none() => {
                "calculating".to_owned()
            }
            HostProgress::Published(_) => "recalculated".to_owned(),
            HostProgress::Failed(failure) => format!("the recalculation failed: {failure:?}"),
        }
    }
}

pub use dna_calc_model::projection::{Census, SheetCensus};
