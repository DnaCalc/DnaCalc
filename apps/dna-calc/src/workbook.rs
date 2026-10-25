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

/// Why a workbook did not open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenRefused(pub String);

/// One sheet as the skins list it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SheetTab {
    pub id: SheetId,
    pub name: String,
}

/// An open workbook.
pub struct Workbook {
    session: HostSession,
    name: String,
    changed: bool,
    inbox: Rc<RefCell<Vec<HostDelta>>>,
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
        let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(bytes));
        let session = HostSession::open(source, &OpenPolicy::default(), host_options())
            .map_err(|failure| OpenRefused(failure.to_string()))?;
        let mut workbook = Workbook {
            session,
            name: name.to_owned(),
            changed: false,
            inbox: Rc::default(),
        };
        if workbook.session.calc_mode() != CalcMode::Manual {
            workbook.session.dispatch(SkinIntent::Recalculate);
        }
        Ok(workbook)
    }

    pub fn name(&self) -> &str {
        &self.name
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
        receipt
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

    /// The workbook's formula cells by provenance, per sheet, and the refusals by reason.
    pub fn census(&mut self) -> Census {
        let mut census = Census::default();
        for tab in self.sheets() {
            let mut sheet = SheetCensus {
                used: self
                    .used_area(tab.id)
                    .map(|area| cells::area_label(self.host(), area)),
                name: tab.name,
                ..SheetCensus::default()
            };
            for cell in self.sheet_cells(tab.id) {
                if cell.input().is_some() {
                    sheet.inputs += 1;
                }
                let shown = cells::shown(&cell, None);
                if !shown.formula {
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

    /// The outcome of the last recalculation an intent ran, in words.
    pub fn recalc_text(progress: &HostProgress) -> String {
        match progress {
            HostProgress::Current => "nothing to recalculate".to_owned(),
            HostProgress::Manual => "manual calculation: press Calculate".to_owned(),
            HostProgress::Calculating { .. } => "calculating".to_owned(),
            HostProgress::Published(_) => "recalculated".to_owned(),
            HostProgress::Failed(failure) => format!("the recalculation failed: {failure:?}"),
        }
    }
}

/// One sheet's formula cells by provenance.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SheetCensus {
    pub name: String,
    /// The used area's label, when the sheet holds anything.
    pub used: Option<String>,
    pub inputs: usize,
    pub formulas: usize,
    pub calculated: usize,
    pub refused: usize,
    pub file_cached: usize,
    pub stale: usize,
}

/// The workbook's formula cells by provenance, per sheet, and the refusals by reason.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Census {
    pub sheets: Vec<SheetCensus>,
    pub refusals: std::collections::BTreeMap<String, usize>,
}

impl Census {
    pub fn formulas(&self) -> usize {
        self.sheets.iter().map(|sheet| sheet.formulas).sum()
    }

    pub fn calculated(&self) -> usize {
        self.sheets.iter().map(|sheet| sheet.calculated).sum()
    }

    pub fn refused(&self) -> usize {
        self.sheets.iter().map(|sheet| sheet.refused).sum()
    }

    pub fn file_cached(&self) -> usize {
        self.sheets.iter().map(|sheet| sheet.file_cached).sum()
    }

    pub fn stale(&self) -> usize {
        self.sheets.iter().map(|sheet| sheet.stale).sum()
    }
}
