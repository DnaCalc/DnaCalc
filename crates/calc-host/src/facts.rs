//! The host's workbook facts that CELL and INFO read (dc-uar): one snapshot per recalculation of
//! the file the workbook was opened from, each cell's format, the columns' widths, the sheets'
//! windows, the calculation mode and the application, given to the engine before each
//! recalculation begins ([`calc_engine::EngineState::set_workbook_facts`]).
//!
//! The file and the application are what the application states ([`HostState::set_file`],
//! [`HostState::set_application`]); calc-host reads no file system and no platform of its own,
//! so until they are set CELL("filename") and INFO("release") and the like are refused. The
//! Formats and windows retain their imported facts with authored overrides; physical column
//! widths come from the current model, including structural moves and authored dimensions.

use crate::state::HostState;
use calc_doc::{CalcMode, LoadedSheet};
use calc_doc_model::value::{CellKey, SheetId};
use calc_doc_model::{AxisEdit, ModelRoot, StyleTable};
use calc_fml::{
    ApplicationFacts, CalculationMode, CellFormatFacts, ColumnWidth, HorizontalAlignment,
    WorkbookFacts, WorkbookFile,
};
use std::sync::Arc;

/// One snapshot of the facts (see the module documentation).
#[derive(Debug)]
pub(crate) struct HostFacts {
    pub(crate) model: Arc<ModelRoot>,
    pub(crate) styles: Arc<StyleTable>,
    pub(crate) sheets: Arc<[LoadedSheet]>,
    pub(crate) axis_edits: Arc<[AxisEdit]>,
    pub(crate) file: Option<WorkbookFile>,
    pub(crate) application: Option<ApplicationFacts>,
    pub(crate) calc_mode: CalcMode,
    pub(crate) active_sheet: Option<SheetId>,
}

/// Where the cell at `at` was when the workbook was opened, through the rows and columns
/// `edits` inserted and deleted since ([`HostState::opened_position`]).
pub(crate) fn opened_position(edits: &[AxisEdit], at: CellKey) -> Option<(u32, u32)> {
    let (mut row, mut column) = (at.row(), at.column());
    for edit in edits.iter().rev().filter(|edit| edit.sheet() == at.sheet()) {
        match edit.axis() {
            calc_doc_model::Axis::Rows => row = calc_doc_model::source_position(*edit, row)?,
            calc_doc_model::Axis::Columns => {
                column = calc_doc_model::source_position(*edit, column)?
            }
        }
    }
    Some((row, column))
}

/// The maximum digit width, in pixels, of a font Excel uses as a workbook's default font, which
/// a column width in characters counts in: measured for the fonts of Excel's default styles
/// (Calibri 11 and Aptos Narrow 11 give a default column of 8.43 characters, 64 pixels).
fn maximum_digit_width(styles: &StyleTable) -> Option<f64> {
    let font = styles
        .cell_style_formats
        .first()
        .and_then(|format| format.font_id)
        .and_then(|id| styles.fonts.get(usize::try_from(id).ok()?))
        .or_else(|| styles.fonts.first());
    let (name, size) = match font {
        Some(font) => (font.name.as_deref()?, font.size.as_deref()?),
        // A styles part with no font: Excel's default, Calibri 11.
        None => ("Calibri", "11"),
    };
    let size: f64 = size.trim().parse().ok()?;
    match (name, size) {
        ("Calibri" | "Aptos Narrow", 11.0) => Some(7.0),
        _ => None,
    }
}

/// The width in characters Excel shows for a file's column width `width` (characters of the
/// maximum digit width `mdw` with the cell padding): the width in whole pixels, less the 5 pixels
/// of padding, in hundredths of a character (ECMA-376 Part 1, 18.3.1.13).
fn shown_characters(width: f64, mdw: f64) -> f64 {
    let pixels = (((256.0 * width + (128.0 / mdw).trunc()) / 256.0) * mdw).trunc();
    (((pixels - 5.0) / mdw * 100.0 + 0.5).trunc() / 100.0).max(0.0)
}

/// The one alignment lookup for workbook facts and visual consumers. Cell records override
/// their named style; a missing alignment inherits the named style's record.
pub(crate) fn alignment(styles: &StyleTable, style: u32) -> Option<&calc_doc_model::Alignment> {
    let format = styles.cell_formats.get(usize::try_from(style).ok()?)?;
    format.alignment.as_ref().or_else(|| {
        format
            .xf_id
            .and_then(|id| styles.cell_style_formats.get(id as usize))
            .and_then(|parent| parent.alignment.as_ref())
    })
}

impl HostFacts {
    fn sheet(&self, id: SheetId) -> Option<&LoadedSheet> {
        self.sheets.iter().find(|sheet| sheet.id() == id)
    }

    /// The cell format index of the cell at `at` ([`HostState::cell_style`]).
    fn style(&self, at: CellKey) -> u32 {
        if let Some(format) = self.model.definitions().cell_format(at) {
            return format;
        }
        match (
            self.sheet(at.sheet()),
            opened_position(&self.axis_edits, at),
        ) {
            (Some(sheet), Some((row, column))) => sheet.style(row, column),
            _ => 0,
        }
    }
}

impl WorkbookFacts for HostFacts {
    fn cell_format(&self, cell: CellKey) -> Option<CellFormatFacts> {
        let style = self.style(cell);
        let number_format = self.styles.number_format_code(style)?.to_owned();
        let format = self.styles.cell_formats.get(usize::try_from(style).ok()?);
        let id = format
            .and_then(|format| format.number_format_id)
            .unwrap_or(0);
        let builtin = (!self
            .styles
            .number_formats
            .iter()
            .any(|declared| declared.id == id))
        .then_some(id);
        let horizontal = match alignment(&self.styles, style)
            .and_then(|alignment| alignment.horizontal.as_deref())
            .map(str::trim)
        {
            None | Some("general") => HorizontalAlignment::General,
            Some("left") => HorizontalAlignment::Left,
            Some("center") => HorizontalAlignment::Center,
            Some("right") => HorizontalAlignment::Right,
            Some("fill") => HorizontalAlignment::Fill,
            Some("justify") => HorizontalAlignment::Justify,
            Some("centerContinuous") => HorizontalAlignment::CenterContinuous,
            Some("distributed") => HorizontalAlignment::Distributed,
            Some(_) => return None,
        };
        let locked = format
            .and_then(|format| format.protection.as_ref())
            .and_then(|protection| protection.locked)
            .unwrap_or(true);
        Some(CellFormatFacts {
            number_format,
            builtin,
            horizontal,
            locked,
        })
    }

    fn column_width(&self, sheet: SheetId, column: u32) -> Option<ColumnWidth> {
        let mdw = maximum_digit_width(&self.styles)?;
        let geometry = self.sheet(sheet)?.geometry();
        let format = self.model.definitions().axis_layouts().get(
            sheet,
            calc_doc_model::Axis::Columns,
            column,
        );
        let default_characters = || match geometry.default_column_width {
            Some(width) => shown_characters(width, mdw),
            None => {
                // The default column: the base width's digits and the padding, rounded up to a
                // multiple of 8 pixels (8 characters of 7 pixels give 64 pixels, 8.43 characters).
                let base = geometry.base_column_width.unwrap_or(8.0);
                let pixels = ((base * mdw + 5.0) / 8.0).ceil() * 8.0;
                (((pixels - 5.0) / mdw * 100.0 + 0.5).trunc() / 100.0).max(0.0)
            }
        };
        Some(if format.hidden() {
            ColumnWidth {
                characters: 0.0,
                default: false,
            }
        } else {
            match format.size() {
                Some(width) => ColumnWidth {
                    characters: shown_characters(width, mdw),
                    default: !format.custom(),
                },
                None => ColumnWidth {
                    characters: default_characters(),
                    default: true,
                },
            }
        })
    }

    fn file(&self) -> Option<WorkbookFile> {
        self.file.clone()
    }

    fn active_cell(&self) -> Option<CellKey> {
        let sheet = self.active_sheet?;
        let (row, column) = self.sheet(sheet)?.view().active_cell.unwrap_or((1, 1));
        CellKey::new(sheet, row, column).ok()
    }

    fn top_left_cell(&self, sheet: SheetId) -> Option<CellKey> {
        let (row, column) = self.sheet(sheet)?.view().top_left.unwrap_or((1, 1));
        CellKey::new(sheet, row, column).ok()
    }

    fn calculation_mode(&self) -> Option<CalculationMode> {
        Some(match self.calc_mode {
            CalcMode::Automatic => CalculationMode::Automatic,
            CalcMode::AutomaticExceptTables => CalculationMode::AutomaticExceptTables,
            CalcMode::Manual => CalculationMode::Manual,
        })
    }

    fn application(&self) -> Option<ApplicationFacts> {
        self.application.clone()
    }
}

impl HostState {
    /// The snapshot of the workbook facts at the current revision (see the module documentation).
    pub(crate) fn workbook_facts(&self) -> Arc<dyn WorkbookFacts> {
        Arc::new(HostFacts {
            model: Arc::clone(&self.model),
            styles: Arc::clone(self.loaded.styles()),
            sheets: self.loaded.shared_sheets(),
            axis_edits: self.axis_edits.clone().into(),
            file: self.file.clone(),
            application: self.application.clone(),
            calc_mode: self.calc_mode(),
            active_sheet: self.loaded.active_sheet(),
        })
    }

    /// Sets the file the workbook was opened from, which CELL("filename") and a reference to
    /// another sheet in CELL("address") name: its folder (with its trailing separator) and file
    /// name; a folder of `None` for a workbook never saved. `None` refuses them again.
    pub fn set_file(&mut self, file: Option<WorkbookFile>) {
        self.file = file;
    }

    /// Sets the facts about the application and its platform that INFO reads (`release`,
    /// `osversion`, `system`, `directory`); `None` refuses them again.
    pub fn set_application(&mut self, application: Option<ApplicationFacts>) {
        self.application = application;
    }
}

#[cfg(test)]
mod tests {
    use super::shown_characters;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn alignment_resolves_one_record_for_cell_facts_and_visual_consumers() {
        use calc_doc_model::{Alignment, CellFormat, StyleTable};
        let inherited = Alignment {
            horizontal: Some("right".into()),
            vertical: Some("top".into()),
            indent: Some("2".into()),
            ..Default::default()
        };
        let direct = Alignment {
            horizontal: Some("center".into()),
            ..Default::default()
        };
        let table = StyleTable {
            cell_style_formats: vec![CellFormat {
                alignment: Some(inherited.clone()),
                ..Default::default()
            }],
            cell_formats: vec![
                CellFormat {
                    xf_id: Some(0),
                    ..Default::default()
                },
                CellFormat {
                    xf_id: Some(0),
                    alignment: Some(direct.clone()),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        assert_eq!(super::alignment(&table, 0), Some(&inherited));
        assert_eq!(super::alignment(&table, 1), Some(&direct));
        assert_eq!(super::alignment(&table, 9), None);
    }

    /// File widths Excel wrote for column widths set in characters (Calibri 11, 7 pixels).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn file_widths_show_as_excel_shows_them() {
        assert_eq!(shown_characters(20.7109375, 7.0), 20.0);
        assert_eq!(shown_characters(9.140625, 7.0), 8.43);
        assert_eq!(shown_characters(0.0, 7.0), 0.0);
    }
}
