//! Projection: each requested cell's authored input and its value with the value's provenance
//! (plan 2.2's ValueProvenance). A formula's result is projected as calculated only when the
//! engine calculated it and no edit since has changed the formula or anything it reads, directly
//! or through other formulas; the calculation it carries names the revision it read, which may
//! be earlier than the current one.

use crate::recalc::RecalcFailed;
use crate::state::HostState;
use calc_doc_model::value::{CalcValue, CellKey, SheetId};
use calc_doc_model::{AuthoredRevision, CellInput, FileCached};
use calc_engine::{Calculation, FormulaResult, NotModeled};
use std::fmt;

/// The cells a projection reads, in the order the page returns them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectionRequest {
    pub cells: Vec<CellKey>,
}

/// Why a formula cell's result is stale.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StaleCause {
    /// The formula has no result that stands (an edit changed it or what it reads, or added it, or
    /// the package cached no result for it), and no recalculation of the current revision has
    /// failed: none has been attempted since the last commit, or one is in progress and none
    /// before it of the same revision failed. Every commit returns each cell whose cause was
    /// RecalcFailed to this cause, because the next recalculation is of the revision the commit
    /// made.
    AwaitingRecalc,
    /// The last recalculation was of the current revision and published nothing
    /// ([`ProjectionPage::recalc_failure`] says why); the cell is among the results it would have
    /// calculated, and no edit has committed since. The cause stands while a later recalculation
    /// of the same revision is in progress, until one publishes.
    RecalcFailed,
    /// The result last published for the cell is the package's cache for the formula the package
    /// holds there, and an edit committed since the workbook was opened changed a cell's input or
    /// formula. Every node is pending from open until the first publication, and calc_engine
    /// exposes no closure of one change (formulas are bound only when a recalculation begins), so
    /// the host cannot say which file caches an edit reached: it gives this cause to every file
    /// cache of a formula the package still holds there once such an edit commits, whether the
    /// edit reached it or not, until the first recalculation publishes. Each cell with this cause
    /// is such a file cache, labelled stale without reach information; those among them that no
    /// edit reached are the conservative broadening. Exposing an edit's reach from open would
    /// retire this cause; no bead owns that yet. A session in automatic mode recalculates after
    /// the first commit, so the cause lasts only until then.
    FileCacheUnverified,
}

/// A formula cell whose value is not calculated for the current revision: what was last
/// published for it, if anything, and why that no longer stands.
#[derive(Clone, Debug, PartialEq)]
pub struct StaleValue {
    last_value: Option<CalcValue>,
    last_calculation: Option<Calculation>,
    file_cache: Option<FileCached>,
    cause: StaleCause,
}

impl StaleValue {
    /// The value the engine last calculated for the cell; `None` when its last result was a
    /// refusal or it has none.
    pub fn last_value(&self) -> Option<&CalcValue> {
        self.last_value.as_ref()
    }

    /// The calculation that published the cell's last result; `None` when none has.
    pub fn last_calculation(&self) -> Option<&Calculation> {
        self.last_calculation.as_ref()
    }

    /// The package's cached result, when that is what was last published for the cell; nothing
    /// here calculated it, and it no longer stands.
    pub fn file_cache(&self) -> Option<&FileCached> {
        self.file_cache.as_ref()
    }

    pub fn cause(&self) -> StaleCause {
        self.cause
    }
}

/// A cell's value and the value's provenance.
#[derive(Clone, Debug, PartialEq)]
pub enum ProjectedValue {
    /// The cell holds no input.
    Empty,
    /// The cell's input is a literal, which is its value.
    Literal(CalcValue),
    /// The result the package stored for the cell's formula. Nothing here calculated it, and no
    /// edit since the workbook was opened has changed a cell's input or formula.
    FileCached(FileCached),
    /// The value the engine calculated for the cell's formula, which still stands: no edit since
    /// `calculation` has changed the formula or anything it reads. `calculation` names the
    /// revision whose inputs it read, which may be earlier than the current revision.
    Calculated {
        value: CalcValue,
        calculation: Calculation,
    },
    /// The engine refused the cell's formula, and the refusal still stands as a calculated value
    /// would; it has no value.
    NotModeled {
        refusal: NotModeled,
        calculation: Calculation,
    },
    /// The cell's formula has no result that stands at the current revision.
    Stale(StaleValue),
}

/// One projected cell.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectedCell {
    at: CellKey,
    input: Option<CellInput>,
    value: ProjectedValue,
}

impl ProjectedCell {
    /// An empty cell: no input, no value.
    pub(crate) fn empty(at: CellKey) -> ProjectedCell {
        ProjectedCell {
            at,
            input: None,
            value: ProjectedValue::Empty,
        }
    }

    pub fn at(&self) -> CellKey {
        self.at
    }

    /// The cell's authored input at the current revision; `None` for an empty cell.
    pub fn input(&self) -> Option<&CellInput> {
        self.input.as_ref()
    }

    pub fn value(&self) -> &ProjectedValue {
        &self.value
    }
}

/// The requested cells at the current revision.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectionPage {
    authored: AuthoredRevision,
    engine: AuthoredRevision,
    recalc_failure: Option<RecalcFailed>,
    cells: Vec<ProjectedCell>,
}

impl ProjectionPage {
    /// The authored revision the inputs were read from.
    pub fn authored_revision(&self) -> AuthoredRevision {
        self.authored
    }

    /// The revision the engine state is at, which the values were read from.
    pub fn engine_revision(&self) -> AuthoredRevision {
        self.engine
    }

    /// Why the last recalculation published nothing, when none has published since.
    pub fn recalc_failure(&self) -> Option<&RecalcFailed> {
        self.recalc_failure.as_ref()
    }

    /// The requested cells, in request order.
    pub fn cells(&self) -> &[ProjectedCell] {
        &self.cells
    }
}

/// [`project`] was asked for a cell of a sheet the current revision does not hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ProjectionError {
    pub sheet: SheetId,
}

impl fmt::Display for ProjectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "the workbook holds no sheet {:?}", self.sheet)
    }
}

impl std::error::Error for ProjectionError {}

/// Each requested cell's authored input and value. A formula cell's value is its published
/// result:
///
/// - a calculated value or a refusal is projected as [`ProjectedValue::Calculated`] or
///   [`ProjectedValue::NotModeled`] unless the engine lists the cell as stale
///   ([`calc_engine::EngineState::stale`]: pending, or reading a pending formula, directly or
///   through others), and then as [`ProjectedValue::Stale`] with that last result;
/// - a file cache is projected as [`ProjectedValue::FileCached`] only while no edit since the
///   workbook was opened has changed a cell's input or formula. After one has, it is
///   [`ProjectedValue::Stale`]: with the stale cause of its formula when that is not the formula
///   the package holds in the cell, and otherwise with [`StaleCause::FileCacheUnverified`];
/// - a formula with no published result is [`ProjectedValue::Stale`].
///
/// A stale cause is [`StaleCause::RecalcFailed`] when the last recalculation was of the current
/// revision and failed, and [`StaleCause::AwaitingRecalc`] otherwise.
pub fn project(
    host: &HostState,
    request: &ProjectionRequest,
) -> Result<ProjectionPage, ProjectionError> {
    if let Some(at) = request
        .cells
        .iter()
        .find(|at| host.model.sheet_by_id(at.sheet()).is_none())
    {
        return Err(ProjectionError { sheet: at.sheet() });
    }
    let stale = host
        .engine
        .stale(&host.model)
        .expect("the host keeps its authored revision and its engine state at one revision");
    let cause = match &host.last_failure {
        Some((revision, _)) if *revision == host.model.revision() => StaleCause::RecalcFailed,
        _ => StaleCause::AwaitingRecalc,
    };
    let cells = request
        .cells
        .iter()
        .map(|&at| {
            let input = host.model.cell(at).cloned();
            let value = match &input {
                None => filled_value(host, at, &stale, cause),
                Some(CellInput::Number(number)) => {
                    ProjectedValue::Literal(CalcValue::number(number.get()))
                }
                Some(CellInput::Text(text)) => {
                    ProjectedValue::Literal(CalcValue::text(text.text().clone()))
                }
                Some(CellInput::Logical(logical)) => {
                    ProjectedValue::Literal(CalcValue::logical(*logical))
                }
                Some(CellInput::Error(code)) => ProjectedValue::Literal(CalcValue::error(*code)),
                Some(input @ CellInput::Formula(_)) => {
                    let is_stale = stale.binary_search(&at).is_ok();
                    // Whether a file cache is the package's result for the formula now in the
                    // cell: the loaded revision held that formula there.
                    let formula_as_opened = host.loaded.model().cell(at) == Some(input);
                    let file_cache_stands = !host.inputs_changed_since_open;
                    formula_value(
                        host.engine.result(at),
                        is_stale,
                        cause,
                        formula_as_opened,
                        file_cache_stands,
                    )
                }
            };
            ProjectedCell { at, input, value }
        })
        .collect();
    Ok(ProjectionPage {
        authored: host.model.revision(),
        engine: host.engine.revision(),
        recalc_failure: host
            .last_failure
            .as_ref()
            .map(|(_, failure)| failure.clone()),
        cells,
    })
}

fn formula_value(
    result: Option<&FormulaResult>,
    is_stale: bool,
    cause: StaleCause,
    formula_as_opened: bool,
    file_cache_stands: bool,
) -> ProjectedValue {
    let stale = |last_value: Option<&CalcValue>, last_calculation: Option<&Calculation>| {
        ProjectedValue::Stale(StaleValue {
            last_value: last_value.cloned(),
            last_calculation: last_calculation.cloned(),
            file_cache: None,
            cause,
        })
    };
    match result {
        Some(FormulaResult::FileCached(cache)) if file_cache_stands => {
            ProjectedValue::FileCached(cache.clone())
        }
        // An edit replaced the formula, or removed it and authored another: the cache is the
        // result of a formula the cell no longer holds. A cache stays published at the cell until
        // the next publication, whatever the edits.
        Some(FormulaResult::FileCached(cache)) if !formula_as_opened => {
            ProjectedValue::Stale(StaleValue {
                last_value: None,
                last_calculation: None,
                file_cache: Some(cache.clone()),
                cause,
            })
        }
        Some(FormulaResult::FileCached(cache)) => ProjectedValue::Stale(StaleValue {
            last_value: None,
            last_calculation: None,
            file_cache: Some(cache.clone()),
            cause: StaleCause::FileCacheUnverified,
        }),
        Some(FormulaResult::Calculated { value, calculation }) if is_stale => {
            stale(Some(value), Some(calculation))
        }
        Some(FormulaResult::Calculated { value, calculation }) => ProjectedValue::Calculated {
            value: value.clone(),
            calculation: calculation.clone(),
        },
        Some(FormulaResult::NotModeled { calculation, .. }) if is_stale => {
            stale(None, Some(calculation))
        }
        Some(FormulaResult::NotModeled {
            refusal,
            calculation,
        }) => ProjectedValue::NotModeled {
            refusal: refusal.clone(),
            calculation: calculation.clone(),
        },
        // A formula cell without a published result was added since the last publication, so it
        // is pending.
        None => stale(None, None),
    }
}

/// The value of a cell that holds no input: empty, unless the model reads it as not available
/// ([`calc_doc_model::Definitions::not_available`]).
///
/// - A cell the engine's published spill of an array formula fills beside its anchor
///   ([`calc_engine::EngineState::spill_covering`]) projects the spilled value as calculated, with
///   the anchor's calculation, while the anchor's result stands, and as stale with that value
///   otherwise.
/// - Another cell of the file's area of an array formula projects the anchor's standing: what the
///   file cached for the cell while the anchor's result is a file cache that stands, the anchor's
///   refusal once the engine refused the anchor, empty once the engine calculated the anchor (its
///   result no longer fills the cell), and stale otherwise, with the cell's file cache.
/// - An unloaded cell (a rich value, a data table's cell) projects what the file cached for it
///   while no edit has changed an input since open, and stale with that cache after one has.
fn filled_value(
    host: &HostState,
    at: CellKey,
    stale: &[CellKey],
    cause: StaleCause,
) -> ProjectedValue {
    let cache = host
        .loaded
        .sheet(at.sheet())
        .and_then(|sheet| sheet.value_cache(at.row(), at.column()))
        .cloned();
    let file_cache_stands = !host.inputs_changed_since_open;
    let stale_cache = |cache: Option<FileCached>, cause| {
        ProjectedValue::Stale(StaleValue {
            last_value: None,
            last_calculation: None,
            file_cache: cache,
            cause,
        })
    };
    let definitions = host.model.definitions();
    // L3 (dc-m3u): a data table's cell projects the result the engine calculated for it, as a
    // formula's does; before its first calculation, what the file cached.
    if definitions.data_table_containing(at).is_some()
        && let Some(result) = host.engine.result(at)
    {
        return formula_value(
            Some(result),
            stale.binary_search(&at).is_ok(),
            cause,
            true,
            file_cache_stands,
        );
    }
    if definitions.unloaded(at).is_some() {
        return match cache {
            Some(cache) if file_cache_stands => ProjectedValue::FileCached(cache),
            cache => stale_cache(cache, StaleCause::FileCacheUnverified),
        };
    }
    if let Some(spill) = host
        .engine
        .spill_covering(at)
        .filter(|spill| spill.anchor() != at)
        && let Some(FormulaResult::Calculated { calculation, .. }) =
            host.engine.result(spill.anchor())
    {
        let value = spill.value_at(at).clone();
        return if stale.binary_search(&spill.anchor()).is_ok() {
            ProjectedValue::Stale(StaleValue {
                last_value: Some(value),
                last_calculation: Some(calculation.clone()),
                file_cache: cache,
                cause,
            })
        } else {
            ProjectedValue::Calculated {
                value,
                calculation: calculation.clone(),
            }
        };
    }
    let Some(array) = definitions
        .array_containing(at)
        .filter(|array| array.anchor() != at)
    else {
        return ProjectedValue::Empty;
    };
    let anchor = array.anchor();
    let anchor_stale = stale.binary_search(&anchor).is_ok();
    match host.engine.result(anchor) {
        Some(FormulaResult::Calculated { .. }) if !anchor_stale => ProjectedValue::Empty,
        Some(FormulaResult::FileCached(_)) if file_cache_stands => match cache {
            Some(cache) => ProjectedValue::FileCached(cache),
            None => ProjectedValue::Empty,
        },
        Some(FormulaResult::FileCached(_)) => stale_cache(cache, StaleCause::FileCacheUnverified),
        Some(FormulaResult::NotModeled {
            refusal,
            calculation,
        }) if !anchor_stale => ProjectedValue::NotModeled {
            refusal: refusal.clone(),
            calculation: calculation.clone(),
        },
        _ => stale_cache(cache, cause),
    }
}
