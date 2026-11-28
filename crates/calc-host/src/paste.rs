//! Paste preparation and revision-bound plans. Preview tokens belong to the transport;
//! only the host can construct a plan and the host checks its revision when applying it.
use crate::value::{CellKey, CoreRef, GridArea, WorksheetErrorCode};
use crate::{
    CellEdit, CellInput, CopySnapshot, FiniteNumber, HostState, PasteMode, ProjectedValue,
    ProjectionRequest,
};
use std::collections::{HashMap, HashSet};

/// Input to the host paste planner.
pub enum PasteSource<'a> {
    /// Paste an immutable range captured by the host.
    Snapshot {
        /// Captured authored content, styles, and calculated values.
        snapshot: &'a CopySnapshot,
        /// Move the source cells when true.
        cut: bool,
    },
    /// Extend an existing range into adjacent cells.
    Fill {
        /// Range providing the seed cells.
        from: GridArea,
        /// Fill down when true and right when false.
        down: bool,
        /// Extend a series instead of repeating the seed pattern.
        series: bool,
    },
    /// Paste a matrix of external cell-entry strings.
    External(Vec<Vec<String>>),
    /// Infer values from examples on the worksheet.
    FlashFill,
}

/// Arithmetic requested by Excel's Paste Special operation field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PasteArithmetic {
    /// Add the source to the destination.
    Add,
    /// Subtract the source from the destination.
    Subtract,
    /// Multiply the destination by the source.
    Multiply,
    /// Divide the destination by the source.
    Divide,
}

/// Independent cell options applied to a prepared copy.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PasteCellOptions {
    /// Preserve the destination cell and its format when the source is blank.
    pub skip_blanks: bool,
    /// Combine copied and destination numbers instead of replacing the destination.
    pub arithmetic: Option<PasteArithmetic>,
}

/// Why the host could not prepare the requested Paste Special cell edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PasteOptionsRefusal {
    /// Either the open workbook or its revision has changed.
    WorkbookChanged,
    /// The destination could not be projected at the prepared revision.
    ProjectionUnavailable,
    /// The arithmetic case lacks observed Excel behavior.
    NotModelled(&'static str),
}

/// A prepared paste bound to the workbook and revision inspected during preview.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PastePlan {
    coordinates: Option<calc_fml::reference::StructuralEdit>,
    scope: Option<u64>,
    revision: u64,
    target: GridArea,
    replaces: u64,
    select_target: bool,
    edits: Vec<CellEdit>,
}
impl PastePlan {
    /// Select the destination as part of this paste's undoable transaction.
    pub fn with_target_selection(mut self) -> Self {
        self.select_target = true;
        self
    }
    pub(crate) fn selection(&self) -> Option<crate::Selection> {
        self.select_target.then(|| crate::Selection {
            active: Some(self.target.first()),
            areas: vec![self.target],
        })
    }
    /// A bounded sample from the actual literal text edits prepared by this plan.
    pub fn text_preview(&self) -> Vec<(CellKey, String)> {
        self.edits
            .iter()
            .filter_map(|e| match e {
                CellEdit::Set {
                    at,
                    input: crate::CellInput::Text(t),
                } => Some((*at, String::from_utf16_lossy(t.text().utf16_code_units()))),
                _ => None,
            })
            .take(8)
            .collect()
    }
    pub(crate) fn edits_mut(&mut self) -> &mut Vec<CellEdit> {
        &mut self.edits
    }
    /// Apply Paste Special's cell options before previewing or committing a copied range.
    pub fn with_cell_options(
        mut self,
        host: &HostState,
        options: PasteCellOptions,
    ) -> Result<Self, PasteOptionsRefusal> {
        if !self.matches_host(host) {
            return Err(PasteOptionsRefusal::WorkbookChanged);
        }
        if options.skip_blanks {
            // Excel skips a blank source cell whole, including its formatting.
            let blanks: Vec<_> = self
                .edits
                .iter()
                .filter_map(|edit| match edit {
                    CellEdit::Clear { area } => Some(*area),
                    _ => None,
                })
                .collect();
            self.edits.retain(|edit| match edit {
                CellEdit::Clear { .. } => false,
                CellEdit::SetFormat { at, .. } => !blanks.iter().any(|area| area.contains(*at)),
                _ => true,
            });
        }
        if let Some(arithmetic) = options.arithmetic {
            apply_paste_arithmetic(host, &mut self.edits, arithmetic)?;
        }
        Ok(self)
    }
    /// Return the destination after any expansion of the selected range.
    pub fn target(&self) -> GridArea {
        self.target
    }
    /// Return the number of occupied destination cells for overwrite confirmation.
    /// Format-only and Flash Fill plans report zero.
    pub fn replaces(&self) -> u64 {
        self.replaces
    }
    /// Take the prepared edits for a host transaction at the revision inspected by the planner.
    pub fn into_edits(self, host: &HostState) -> Result<Vec<CellEdit>, String> {
        if !self.matches_host(host) {
            return Err("The workbook changed after the preview; paste again".into());
        }
        Ok(self.edits)
    }

    fn matches_host(&self, host: &HostState) -> bool {
        self.scope == Some(host.command_scope) && self.revision == host.revision().get()
    }

    pub(crate) fn coordinates(&self) -> Option<calc_fml::reference::StructuralEdit> {
        self.coordinates.clone()
    }

    /// Replay deliberately applies a recorded command to another open host. Only that route
    /// can rebind a plan; its original revision check and prepared edits remain intact.
    pub(crate) fn for_replay(mut self, host: &HostState, recorded_scope: u64) -> Self {
        self.scope = (self.scope == Some(recorded_scope)).then_some(host.command_scope);
        self
    }
}

fn apply_paste_arithmetic(
    host: &HostState,
    edits: &mut [CellEdit],
    operation: PasteArithmetic,
) -> Result<(), PasteOptionsRefusal> {
    let refuse = PasteOptionsRefusal::NotModelled;
    let cells = edits
        .iter()
        .filter_map(|edit| match edit {
            CellEdit::Set { at, .. } => Some(*at),
            CellEdit::Clear { area } => Some(area.first()),
            _ => None,
        })
        .collect();
    let page = crate::project(host, &ProjectionRequest { cells })
        .map_err(|_| PasteOptionsRefusal::ProjectionUnavailable)?;
    let mut current = HashMap::new();
    let mut dest_empty = HashSet::new();
    for cell in page.cells() {
        let number = match cell.value() {
            ProjectedValue::Empty => {
                dest_empty.insert(cell.at());
                0.0
            }
            ProjectedValue::Literal(value) => match value.core_ref() {
                CoreRef::Number(number) => number,
                _ => return Err(refuse("onto a destination cell that is not a number")),
            },
            _ => return Err(refuse("onto a formula destination")),
        };
        current.insert(cell.at(), number);
    }
    for edit in edits {
        let (at, source) = match edit {
            CellEdit::Set { at, input } => match input {
                CellInput::Number(source) => (*at, source.get()),
                _ => return Err(refuse("with a source that is not a number")),
            },
            CellEdit::Clear { area } => {
                let at = area.first();
                if dest_empty.contains(&at) {
                    continue;
                }
                (at, 0.0)
            }
            _ => continue,
        };
        let dest = current[&at];
        let result = match operation {
            PasteArithmetic::Add => dest + source,
            PasteArithmetic::Subtract => dest - source,
            PasteArithmetic::Multiply => dest * source,
            PasteArithmetic::Divide if source == 0.0 => {
                *edit = CellEdit::Set {
                    at,
                    input: CellInput::Error(WorksheetErrorCode::Div0),
                };
                continue;
            }
            PasteArithmetic::Divide => dest / source,
        };
        *edit = CellEdit::Set {
            at,
            input: FiniteNumber::new(result)
                .map(CellInput::Number)
                .unwrap_or(CellInput::Error(WorksheetErrorCode::Num)),
        };
    }
    Ok(())
}

/// Validate the source and destination and prepare a paste in its original orientation.
pub fn prepare_paste(
    host: &HostState,
    source: PasteSource<'_>,
    to: GridArea,
    mode: PasteMode,
) -> Result<PastePlan, String> {
    prepare_paste_oriented(host, source, to, mode, false)
}

/// The same revision-bound planner with rows and columns exchanged at the copy boundary.
pub fn prepare_paste_oriented(
    host: &HostState,
    source: PasteSource<'_>,
    to: GridArea,
    mode: PasteMode,
    transpose: bool,
) -> Result<PastePlan, String> {
    let flash = matches!(&source, PasteSource::FlashFill);
    let mode = if flash { PasteMode::Values } else { mode };
    if transpose && matches!(&source, PasteSource::Fill { .. } | PasteSource::FlashFill) {
        return Err(crate::CopyRefusal::TransposeFill.to_string());
    }
    // Excel entry alerts apply to direct/Values entry; whole-cell copy and fill can introduce
    // invalid values which Circle Invalid Data then audits against the copied rules.
    let validate_entries =
        matches!(&source, PasteSource::External(_)) || mode.base() == PasteMode::Values;
    let coordinates = match &source {
        PasteSource::Snapshot {
            snapshot,
            cut: true,
        } => Some(crate::copy::move_coordinates(
            host,
            snapshot.area(),
            to.first(),
        )),
        _ => None,
    };
    let (edits, target) = match source {
        PasteSource::Snapshot { snapshot, cut } => {
            if cut && mode.base() != PasteMode::All {
                return Err(
                    "Paste Special is unavailable for a cut; copy the cells instead".to_owned(),
                );
            }
            let target = snapshot
                .target_oriented(to, transpose)
                .map_err(|e| e.to_string())?;
            let edits = snapshot
                .edits_oriented(host, to, mode, cut, None, transpose)
                .map_err(|e| e.to_string())?;
            (edits, target)
        }
        PasteSource::Fill { from, down, series } => {
            let snapshot = crate::CopySnapshot::capture(host, from).map_err(|e| e.to_string())?;
            let edits = snapshot
                .fill_edits(host, to, down, series)
                .map_err(|e| e.to_string())?;
            (edits, to)
        }
        PasteSource::FlashFill => (crate::flash_fill::prepare(host, to)?, to),
        PasteSource::External(rows) => {
            let rows = if transpose {
                let (height, width) = matrix_shape(&rows)?;
                (0..width as usize)
                    .map(|column| {
                        (0..height as usize)
                            .map(|row| rows[row][column].clone())
                            .collect()
                    })
                    .collect()
            } else {
                rows
            };
            if mode == PasteMode::Formats {
                return Err(
                    "External table formats are not modelled; copy formats within this workbook"
                        .to_owned(),
                );
            }
            let (height, width) = matrix_shape(&rows)?;
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
            crate::check_destination(host, target, mode).map_err(|e| e.to_string())?;
            let mut edits = Vec::new();
            for row in target.first().row()..=target.last().row() {
                for col in target.first().column()..=target.last().column() {
                    let at = CellKey::new(target.sheet(), row, col).unwrap();
                    let text = &rows[((row - target.first().row()) % height) as usize]
                        [((col - target.first().column()) % width) as usize];
                    let input = crate::interpret_entry_in(text, Some(&host.entry_environment()))
                        .map_err(|e| e.to_string())?;
                    if mode.base() == PasteMode::Values
                        && matches!(input, Some(crate::CellInput::Formula(_)))
                    {
                        return Err(
                            "External formula text has no calculated value; choose Formulas or All"
                                .to_owned(),
                        );
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
    if transpose
        && host
            .model()
            .definitions()
            .tables()
            .iter()
            .any(|t| t.area.intersection(target).is_some())
    {
        return Err(crate::CopyRefusal::TransposeTable.to_string());
    }
    for edit in edits.iter().filter(|_| validate_entries) {
        if let CellEdit::Set { at, input } = edit {
            if let Err(failure) = host.check_entry(*at, Some(input)) {
                return Err(failure.to_string());
            }
        }
    }
    let replaces = if flash || !mode.writes_contents() {
        0
    } else {
        host.model().cells(target).into_iter().flatten().count() as u64
    };
    Ok(PastePlan {
        coordinates,
        scope: Some(host.command_scope),
        revision: host.revision().get(),
        target,
        replaces,
        select_target: false,
        edits,
    })
}

pub use calc_value::matrix_shape;
