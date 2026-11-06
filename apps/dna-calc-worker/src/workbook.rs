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

// A stable clock snapshot belongs to opening or Calculate Now. Sampling during
// filtering, selection, saves or continuations would invalidate values they are reading or
// preserving. Scoped, rebuild, automatic edit and VBA passes keep the last sampled clock.
fn sample_browser_clock(session: &mut HostSession) {
    #[cfg(target_arch = "wasm32")]
    session.set_clock(dna_formula_ui::web::serial_clock());
    #[cfg(not(target_arch = "wasm32"))]
    let _ = session; // Keep a native embedding's explicitly supplied clock.
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

/// A template creates an unpathed workbook from its file stem, even when the supplied name
/// contains a desktop path. The server owns the session counter for that stem.
pub(crate) fn template_basename(name: &str) -> &str {
    let file = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let stem = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
    if stem.is_empty() { "Book" } else { stem }
}

/// Why a workbook did not open.
#[derive(Debug)]
pub enum OpenRefused {
    Package(calc_host::OpenFailure),
    Vba(calc_host_vba::FileProjectError),
}
impl std::fmt::Display for OpenRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self { Self::Package(error) => error.fmt(f), Self::Vba(error) => error.fmt(f) }
    }
}
impl std::error::Error for OpenRefused {}

/// A caller can preserve a file's project without compiling or executing it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VbaOpenPolicy { Run, Preserve }

pub use dna_calc_model::projection::SheetTab;

/// An open workbook.
pub struct Workbook {
    session: HostSession,
    name: String,
    changed: bool,
    unsaved: bool,
    output_format: calc_host::SpreadsheetFormat,
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
        Self::open_with_project_policy(name, bytes, budget, VbaOpenPolicy::Run)
    }

    pub fn open_with_project_policy(
        name: &str,
        bytes: Vec<u8>,
        budget: calc_host::WorkBudget,
        vba: VbaOpenPolicy,
    ) -> Result<Workbook, OpenRefused> {
        Self::open_with_template_mode(name, bytes, budget, vba, false)
    }

    pub fn open_with_template_mode(name:&str, bytes:Vec<u8>, budget:calc_host::WorkBudget, vba:VbaOpenPolicy, edit_template:bool) -> Result<Workbook,OpenRefused> {
        Self::open_with_template_counter(name,bytes,budget,vba,edit_template,1)
    }

    pub(crate) fn open_with_template_counter(name:&str, bytes:Vec<u8>, budget:calc_host::WorkBudget, vba:VbaOpenPolicy, edit_template:bool, template_counter:u64) -> Result<Workbook,OpenRefused> {
        let memory=MemorySource::new(bytes);
        let source: Arc<dyn PackageRead> = Arc::new(memory.clone().named(name));
        let mut session = HostSession::open(source, &OpenPolicy::default(), host_options())
            .map_err(OpenRefused::Package)?;
        let input_format=session.host().loaded().spreadsheet_format();
        let unsaved=input_format.is_template() && !edit_template;
        let output_format=if unsaved {if input_format.macro_enabled(){calc_host::SpreadsheetFormat::Xlsm}else{calc_host::SpreadsheetFormat::Xlsx}}else{input_format};
        let name=if unsaved {format!("{}{template_counter}.{}",template_basename(name),output_format.extension())}else{name.to_owned()};
        if unsaved {
            session=HostSession::open(Arc::new(memory.named(name.as_str())), &OpenPolicy::default(), host_options()).map_err(OpenRefused::Package)?;
        }
        sample_browser_clock(&mut session);
        session.set_calculation_budget(budget);
        if vba == VbaOpenPolicy::Run {
            calc_host_vba::activate_file_project(&mut session, &name, calc_vba::DEFAULT_STEP_BUDGET, None)
                .map_err(OpenRefused::Vba)?;
        }
        let mut workbook = Workbook {
            session,
            name,
            changed: false,
            unsaved,
            output_format,
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
        let stem=self.name.rsplit_once('.').map_or(self.name.as_str(), |(stem,_)|stem);
        format!("{stem}.{}",self.output_format.extension())
    }
    pub fn unsaved(&self)->bool {self.unsaved}
    pub fn output_format(&self)->calc_host::SpreadsheetFormat {self.output_format}
    pub fn saved_as(&mut self,name:String,format:calc_host::SpreadsheetFormat) {
        self.session.set_file_identity(&name,format.macro_enabled());
        self.name=name;self.output_format=format;self.unsaved=false;
        self.conditional.borrow_mut().clear();self.display_metrics.borrow_mut().clear();
        if self.session.calc_mode()!=CalcMode::Manual {self.session.dispatch(SkinIntent::Recalculate);}
    }
    pub fn save_policy(&self)->SavePolicy {SavePolicy{format:calc_host::SaveFormat::Spreadsheet(self.output_format),..Default::default()}}

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

    pub(crate) fn begin_name_calculation(&mut self, id: calc_host::value::NameId,
        version: u64, caller: CellKey) -> Result<calc_host::NameCalculation, String> {
        self.session.begin_name_calculation(id, version, caller)
    }
    pub(crate) fn advance_name_calculation(&mut self, work: &mut calc_host::NameCalculation,
        quantum: usize) -> calc_host::NameCalculationProgress {
        work.advance(&mut self.session, quantum)
    }

    /// Native composition installs XLL/VBA/WIT adapters through one callable table.
    pub fn set_user_functions(&mut self, functions: calc_host::CallableTable) {
        self.session.set_user_functions(functions);
    }
    pub fn update_rtd(
        &mut self,
        provider: calc_host::CallableHandle,
        subscription: u64,
        sequence: u64,
        value: calc_host::value::CalcValue,
    ) -> calc_host::RtdDisposition {
        self.session
            .update_rtd(provider, subscription, sequence, value)
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
        self.session.set_workbook_event_handler(None);
        Ok(())
    }
    pub fn load_vba_project(&mut self, modules: &[crate::wire::VbaModule], max_steps: u64) -> Result<(), String> {
        self.load_vba_project_with_backend(modules, max_steps, None)
    }
    pub fn load_vba_project_with_backend(&mut self, modules: &[crate::wire::VbaModule], max_steps: u64, backend: Option<calc_vba::Backend>) -> Result<(), String> {
        let modules = modules.iter().map(|module| calc_host_vba::ModuleSource {
            name: module.name.clone(),
            source: module.source.clone(),
            target: match module.target {
                crate::wire::VbaModuleTarget::Standard => calc_host_vba::ModuleTarget::Standard,
                crate::wire::VbaModuleTarget::Workbook => calc_host_vba::ModuleTarget::Workbook,
                crate::wire::VbaModuleTarget::Worksheet(sheet) => calc_host_vba::ModuleTarget::Worksheet(sheet.id()),
            },
        }).collect::<Vec<_>>();
        let owner = calc_host_vba::prepare(self.session.host(), &self.name, &modules, max_steps, backend)?;
        owner.install(&mut self.session);
        Ok(())
    }
    pub fn set_vba_events_enabled(&mut self, enabled: bool) {
        self.session.set_workbook_events_enabled(enabled);
    }
    pub fn take_workbook_event_failures(&mut self) -> calc_host::WorkbookEventFailures {
        self.session.take_workbook_event_failures()
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
        if matches!(&intent, SkinIntent::Recalculate) {
            sample_browser_clock(&mut self.session);
            self.conditional.borrow_mut().clear();
        }
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
    pub fn begin_calculated_save(&mut self) -> Result<calc_host::CalculatingSave, String> {
        self.session
            .prepare_calculated_save(&self.save_policy())
            .map_err(|error| error.to_string())
    }
    pub fn begin_ordinary_save(
        &mut self,
        calculate_before_save: bool,
        policy: &SavePolicy,
    ) -> Result<calc_host::OrdinarySave, String> {
        self.begin_ordinary_save_operation(calculate_before_save,policy,false)
    }
    pub fn begin_ordinary_save_operation(&mut self,calculate_before_save:bool,policy:&SavePolicy,save_as:bool)->Result<calc_host::OrdinarySave,String> {
        let active_sheet = self
            .session
            .selection()
            .active
            .map(|at| at.sheet())
            .or_else(|| self.host().sheets().first().map(|s| s.id()))
            .ok_or("No active worksheet")?;
        self.session
            .begin_save_with_context(
                policy,
                calc_host::ExcelSaveContext {
                    operation: if save_as {calc_host::SaveOperation::SaveAsDialog}else{calc_host::SaveOperation::Save},
                    calculate_before_save,
                    active_sheet,
                    disabled_sheets: &[],
                    before_save: None,
                },
            )
            .map_err(|error| error.to_string())
    }
    pub fn advance_ordinary_save(
        &mut self,
        save: &mut calc_host::OrdinarySave,
        budget: calc_host::WorkBudget,
    ) -> calc_host::CalculatedSaveProgress {
        save.advance(&mut self.session, budget)
    }
    pub fn cancel_ordinary_save(&mut self, mut save: calc_host::OrdinarySave) {
        save.cancel(&mut self.session);
    }
    pub fn finish_ordinary_save(
        &mut self,
        save: calc_host::OrdinarySave,
    ) -> Result<(Vec<u8>, SaveReceipt), String> {
        let prepared = save.finish().map_err(|error| error.to_string())?;
        self.write_prepared_save(prepared)
    }
    pub fn finish_calculated_save(
        &mut self,
        save: calc_host::CalculatingSave,
    ) -> Result<(Vec<u8>, SaveReceipt), String> {
        let prepared = self
            .session
            .finish_calculated_save(save)
            .map_err(|error| error.to_string())?;
        self.write_prepared_save(prepared)
    }
    fn write_prepared_save(
        &mut self,
        prepared: calc_host::PreparedSave,
    ) -> Result<(Vec<u8>, SaveReceipt), String> {
        let revision = prepared.pin().model;
        let mut bytes = Vec::new();
        let receipt = prepared
            .write(&mut bytes)
            .map_err(|error| error.to_string())?;
        if self.host().revision() == revision {
            self.changed = false;
            self.unsaved = false;
        }
        Ok((bytes, receipt))
    }
    pub fn cancel_calculated_save(
        &mut self,
        mut save: calc_host::CalculatingSave,
    ) -> Result<(), String> {
        save.cancel();
        match self.session.finish_calculated_save(save) {
            Ok(_) | Err(calc_host::CalculatedSaveError::Cancelled) => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    }

    /// Saves the current revision as a package's bytes; the workbook is unchanged since.
    pub fn save(&mut self) -> Result<(Vec<u8>, SaveReceipt), String> {
        let mut bytes = Vec::new();
        let policy=self.save_policy();
        let receipt = self
            .session
            .save(&policy, &mut bytes)
            .map_err(|failure| failure.to_string())?;
        self.changed = false;
        self.unsaved = false;
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

#[cfg(test)]
mod template_tests {
    use super::*;
    fn convert(bytes:&[u8],kind:calc_host::SpreadsheetFormat)->Vec<u8> {
        let doc=calc_doc::open(Arc::new(MemorySource::new(bytes.to_vec())),&Default::default()).unwrap();
        let mut output=Vec::new();
        calc_doc::write(calc_doc::prepare_save(&doc.snapshot(),&SavePolicy{format:calc_host::SaveFormat::Spreadsheet(kind),..Default::default()}).unwrap(),&mut output).unwrap();output
    }
    #[cfg_attr(not(target_arch="wasm32"),test)]
    #[cfg_attr(target_arch="wasm32",wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_bctq__opening_template_creates_unsaved_workbook_while_editing_preserves_file_kind() {
        use calc_host::SpreadsheetFormat as F;
        let template=convert(include_bytes!("../../../crates/calc-doc/tests/fixtures/dc_5sr/features.xlsx"),F::Xltx);
        let mut created=Workbook::open("personal.xltx",template.clone()).unwrap();
        assert_eq!(created.name(),"personal1.xlsx");assert!(created.unsaved());assert_eq!(created.save_name(),"personal1.xlsx");
        assert_eq!(created.host().model().definitions().workbook_name(),Some("personal1.xlsx"));
        assert!(created.calculated_on_open().is_some());
        let (output,_)=created.save().unwrap();assert!(!created.unsaved());
        let reopened=Workbook::open("personal1.xlsx",output).unwrap();assert_eq!(reopened.host().loaded().spreadsheet_format(),F::Xlsx);
        assert_eq!(reopened.sheets().len(),created.sheets().len());
        let mut editing=Workbook::open_with_template_mode("personal.xltx",template,calc_host::WorkBudget::UNLIMITED,VbaOpenPolicy::Preserve,true).unwrap();
        assert_eq!(editing.name(),"personal.xltx");assert!(!editing.unsaved());assert_eq!(editing.output_format(),F::Xltx);
        let (saved,_)=editing.save().unwrap();assert_eq!(Workbook::open("personal.xltx",saved).unwrap().output_format(),F::Xlsx);
    }
    #[cfg_attr(not(target_arch="wasm32"),test)]
    #[cfg_attr(target_arch="wasm32",wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_bctq__macro_template_retains_actual_vba_and_rejects_macro_free_output() {
        use calc_host::SpreadsheetFormat as F;
        let original=include_bytes!("../../../crates/calc-doc/tests/fixtures/vba_udf_names/local.xlsm");
        let template=convert(original,F::Xltm);
        let mut book=Workbook::open("macro.xltm",template).unwrap();
        assert_eq!(book.name(),"macro1.xlsm");assert!(book.unsaved());
        let source=calc_doc::open(Arc::new(MemorySource::new(original.to_vec())),&Default::default()).unwrap();
        let (saved,_)=book.save().unwrap();
        let output=calc_doc::open(Arc::new(MemorySource::new(saved)),&Default::default()).unwrap();
        for part in source.package().parts().iter().filter(|p|p.name().ends_with(".bin")) {
            assert_eq!(source.package().read_part(part.name()).unwrap(),output.package().read_part(part.name()).unwrap());
        }
        for kind in [F::Xlsx,F::Xltx] {
            let save=book.begin_ordinary_save(false,&SavePolicy {format:calc_host::SaveFormat::Spreadsheet(kind),..Default::default()}).unwrap();
            assert_eq!(save.progress(),calc_host::CalculatedSaveProgress::Failed);
            assert!(book.finish_ordinary_save(save).unwrap_err().contains("would be lost"));
        }
    }
}

#[cfg(test)]
#[allow(dead_code)] // The shared fixture kit also serves other test targets.
#[path = "../../../crates/calc-doc/tests/cdoc_m1_1_support/mod.rs"]
mod existing_clock_fixture_kit;

#[cfg(test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn browser_clock_open_and_explicit_calculate_preserve_manual_file_caches() {
    use existing_clock_fixture_kit::{RawEntry, build_zip, parse_zip};
    use calc_host::ProjectedValue;
    let original = calc_host::blank_package();
    let archive = parse_zip(&original);
    let value = |book: &Workbook, row| {
        let at = CellKey::new(book.host().sheets()[0].id(), row, 1).unwrap();
        calc_host::project(book.host(), &ProjectionRequest { cells: vec![at] }).unwrap().cells()[0].value().clone()
    };
    let check_calculated = |book: &Workbook, first: Option<f64>, last: Option<f64>| {
        let offset = if book.host().declared_date1904() { 1462.0 } else { 0.0 };
        let ProjectedValue::Calculated { value: now, .. } = value(book, 1) else { panic!("NOW must be calculated: {:?}", value(book, 1)); };
        let ProjectedValue::Calculated { value: today, .. } = value(book, 2) else { panic!("TODAY must be calculated: {:?}", value(book, 2)); };
        let now = now.as_number().expect("NOW produces a numeric clock value");
        assert_eq!(today.as_number(), Some(now.floor()));
        if let (Some(first), Some(last)) = (first, last) { assert!((first.min(last)..=first.max(last)).contains(&(now + offset)), "{first} <= {now} + {offset} <= {last}"); }
        else { assert_eq!(now, 45000.25 - offset); }
    };
    for date1904 in [false, true] {
        for manual in [false, true] {
            let entries = archive.entries.iter().map(|part| {
                let payload = part.payload(&original);
                let bytes = if part.method == 0 { payload.to_vec() } else { miniz_oxide::inflate::decompress_to_vec(payload).unwrap() };
                let bytes = match part.name.as_str() {
                    "xl/workbook.xml" => String::from_utf8(bytes).unwrap().replace("<sheets>", &format!("<workbookPr date1904=\"{}\"/><sheets>", u8::from(date1904))).replace("<calcPr", &format!("<calcPr calcMode=\"{}\"", if manual { "manual" } else { "auto" })).into_bytes(),
                    "xl/worksheets/sheet1.xml" => String::from_utf8(bytes).unwrap().replace("<sheetData/>", "<sheetData><row r=\"1\"><c r=\"A1\"><f>NOW()</f><v>1.25</v></c></row><row r=\"2\"><c r=\"A2\"><f>TODAY()</f><v>1</v></c></row></sheetData>").into_bytes(),
                    _ => bytes,
                };
                RawEntry::stored(&part.name, &bytes)
            }).collect::<Vec<_>>();
            let bytes = build_zip(&entries);
            let first = dna_formula_ui::web::serial_clock();
            let mut book = Workbook::open("clock.xlsx", bytes).unwrap();
            let last = dna_formula_ui::web::serial_clock();
            assert_eq!(book.host().declared_date1904(), date1904);
            if manual {
                assert!(matches!(value(&book, 1), ProjectedValue::FileCached(cache) if cache.value().to_value().as_number() == Some(1.25)));
                let clock = book.host().entry_environment().now;
                let (saved, _) = book.save().unwrap();
                assert_eq!(book.host().entry_environment().now, clock);
                let reopened = Workbook::open("clock.xlsx", saved).unwrap();
                assert!(matches!(value(&reopened, 1), ProjectedValue::FileCached(cache) if cache.value().to_value().as_number() == Some(1.25)));
            } else if first.is_some() { check_calculated(&book, first, last); }
            else { assert!(matches!(value(&book, 1), ProjectedValue::NotModeled { .. })); }
            book.session.set_clock(Some(45000.25));
            let first = dna_formula_ui::web::serial_clock();
            assert!(book.dispatch(SkinIntent::Recalculate).accepted());
            let last = dna_formula_ui::web::serial_clock();
            check_calculated(&book, first, last);
            let clock = book.host().entry_environment().now;
            let before = value(&book, 1);
            let refused = book.dispatch(SkinIntent::Calculate { scope: calc_host::CalcScope::Workbook, selection: calc_host::EvalSelection::FormulaEntry });
            assert!(!refused.accepted());
            assert_eq!(book.host().entry_environment().now, clock);
            assert_eq!(value(&book, 1), before);
        }
    }
}
