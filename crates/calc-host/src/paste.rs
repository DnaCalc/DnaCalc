//! Paste preparation and revision-bound plans. Preview tokens belong to the transport;
//! only the host can construct a plan and the host checks its revision when applying it.
use crate::value::{CellKey, GridArea};
use crate::{CellEdit, CopySnapshot, HostState, PasteMode};

pub enum PasteSource<'a> {
    Snapshot {
        snapshot: &'a CopySnapshot,
        cut: bool,
    },
    Fill {
        from: GridArea,
        down: bool,
        series: bool,
    },
    External(Vec<Vec<String>>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PastePlan {
    scope: Option<u64>,
    revision: u64,
    target: GridArea,
    replaces: u64,
    edits: Vec<CellEdit>,
}
impl PastePlan {
    pub fn target(&self) -> GridArea {
        self.target
    }
    pub fn replaces(&self) -> u64 {
        self.replaces
    }
    pub(crate) fn into_edits(self, host: &HostState) -> Result<Vec<CellEdit>, String> {
        if self.scope != Some(host.command_scope) || self.revision != host.revision().get() {
            return Err("The workbook changed after the preview; paste again".into());
        }
        Ok(self.edits)
    }

    /// Replay deliberately applies a recorded command to another open host. Only that route
    /// can rebind a plan; its original revision check and prepared edits remain intact.
    pub(crate) fn for_replay(mut self, host: &HostState, recorded_scope: u64) -> Self {
        self.scope = (self.scope == Some(recorded_scope)).then_some(host.command_scope);
        self
    }
}

pub fn prepare_paste(
    host: &HostState,
    source: PasteSource<'_>,
    to: GridArea,
    mode: PasteMode,
) -> Result<PastePlan, String> {
    let (edits, target) = match source {
        PasteSource::Snapshot { snapshot, cut } => {
            if cut && mode != PasteMode::All {
                return Err(
                    "Paste Special is unavailable for a cut; copy the cells instead".to_owned(),
                );
            }
            let target = snapshot.target(to).map_err(|e| e.to_string())?;
            let edits = snapshot
                .edits(host, to, mode, cut, None)
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
        PasteSource::External(rows) => {
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
                    if mode == PasteMode::Values
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
    for edit in &edits {
        if let CellEdit::Set { at, input } = edit {
            if let Err(failure) = host.check_entry(*at, Some(input)) {
                return Err(failure.to_string());
            }
        }
    }
    let replaces = if mode == PasteMode::Formats {
        0
    } else {
        host.model().cells(target).into_iter().flatten().count() as u64
    };
    Ok(PastePlan {
        scope: Some(host.command_scope),
        revision: host.revision().get(),
        target,
        replaces,
        edits,
    })
}

pub fn matrix_shape(rows: &[Vec<String>]) -> Result<(u32, u32), String> {
    let width = rows.first().map_or(0, Vec::len);
    if width == 0 || rows.is_empty() || rows.iter().any(|row| row.len() != width) {
        return Err("Paste a rectangular table".to_owned());
    }
    if rows.len() as u64 * width as u64 > crate::COPY_LIMIT {
        return Err("The clipboard table is too large".to_owned());
    }
    Ok((rows.len() as u32, width as u32))
}
