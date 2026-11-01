//! A workbook's style table as its styles part declares it: number formats, fonts, fills,
//! borders, cell formats (`cellXfs`, which a cell's style index names), cell style formats,
//! named cell styles, differential formats and the colour tables. Document-only metadata: no
//! calculation reads it, and a save writes the styles part back as it was stored.
//!
//! Attribute values the model does not interpret (sizes, colours, alignment keywords) are kept as
//! the file's text. A child element or attribute without a field here is named in
//! [`StyleTable::unmodelled`], so its absence from the table is visible; its bytes stay in the
//! styles part.

/// A number format a styles part declares (`numFmt`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct NumberFormat {
    pub id: u32,
    pub code: String,
}

/// A colour reference: by RGB, by legacy index, by theme colour with a tint, or automatic.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct StyleColor {
    pub rgb: Option<String>,
    pub indexed: Option<String>,
    pub theme: Option<String>,
    pub tint: Option<String>,
    pub auto: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Font {
    pub name: Option<String>,
    pub size: Option<String>,
    pub color: Option<StyleColor>,
    pub family: Option<String>,
    pub scheme: Option<String>,
    pub bold: bool,
    pub italic: bool,
    pub underline: Option<String>,
    pub strike: bool,
    /// Child elements without a field here, by local name.
    pub other_children: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Fill {
    pub pattern_type: Option<String>,
    pub fg_color: Option<StyleColor>,
    pub bg_color: Option<StyleColor>,
    /// Child elements without a field here (a gradient fill's), by local name.
    pub other_children: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct BorderEdge {
    pub style: Option<String>,
    pub color: Option<StyleColor>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Border {
    pub left: Option<BorderEdge>,
    pub right: Option<BorderEdge>,
    pub top: Option<BorderEdge>,
    pub bottom: Option<BorderEdge>,
    pub diagonal: Option<BorderEdge>,
    pub diagonal_up: bool,
    pub diagonal_down: bool,
    pub other_children: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Alignment {
    pub horizontal: Option<String>,
    pub vertical: Option<String>,
    pub text_rotation: Option<String>,
    pub wrap_text: bool,
    pub shrink_to_fit: bool,
    pub indent: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Protection {
    pub locked: Option<bool>,
    pub hidden: Option<bool>,
}

/// A cell format (`xf`): the indexes of its number format, font, fill and border, the cell style
/// format it is based on, and its flags.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct CellFormat {
    pub number_format_id: Option<u32>,
    pub font_id: Option<u32>,
    pub fill_id: Option<u32>,
    pub border_id: Option<u32>,
    pub xf_id: Option<u32>,
    pub quote_prefix: bool,
    pub pivot_button: bool,
    pub apply_number_format: bool,
    pub apply_font: bool,
    pub apply_fill: bool,
    pub apply_border: bool,
    pub apply_alignment: bool,
    pub apply_protection: bool,
    pub alignment: Option<Alignment>,
    pub protection: Option<Protection>,
}

/// A named cell style (`cellStyle`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CellStyle {
    pub name: String,
    pub xf_id: u32,
    pub builtin_id: Option<u32>,
    pub custom_builtin: bool,
    pub hidden: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TableStyles {
    pub count: Option<u32>,
    pub default_table_style: Option<String>,
    pub default_pivot_style: Option<String>,
}

/// A differential format (`dxf`), which conditional formats and table styles name.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct DifferentialFormat {
    pub number_format: Option<NumberFormat>,
    pub font: Option<Font>,
    pub fill: Option<Fill>,
    pub border: Option<Border>,
    pub alignment: Option<Alignment>,
    pub protection: Option<Protection>,
}

/// The styles part's colour tables.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct WorkbookColors {
    pub indexed: Vec<StyleColor>,
    pub most_recently_used: Vec<StyleColor>,
}

/// A workbook's style table. A cell's style index names an entry of [`StyleTable::cell_formats`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct StyleSources {
    pub fonts: std::collections::BTreeMap<u32, u32>,
    pub fills: std::collections::BTreeMap<u32, u32>,
    pub borders: std::collections::BTreeMap<u32, u32>,
    pub cell_formats: std::collections::BTreeMap<u32, u32>,
}

/// Imported records followed by authored records, with their source for lossless derivation.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct StyleTable {
    /// Worksheet style assignments, co-committed with the records they name.
    pub runs: StyleRuns,
    pub sources: StyleSources,
    pub number_formats: Vec<NumberFormat>,
    pub fonts: Vec<Font>,
    pub fills: Vec<Fill>,
    pub borders: Vec<Border>,
    pub cell_style_formats: Vec<CellFormat>,
    pub cell_formats: Vec<CellFormat>,
    pub cell_styles: Vec<CellStyle>,
    pub differential_formats: Vec<DifferentialFormat>,
    pub table_styles: Option<TableStyles>,
    pub colors: WorkbookColors,
    /// Elements of the styles part without a place in this table, each as a path of local names
    /// (`styleSheet/extLst`); their bytes stay in the part.
    pub unmodelled: Vec<String>,
}

impl StyleTable {
    /// The number format code of the cell format at `style`: a format the table declares, or one
    /// of the built-in formats every workbook has; `None` when the cell format does not exist or
    /// names a format neither declares.
    pub fn number_format_code(&self, style: u32) -> Option<&str> {
        let id = self
            .cell_formats
            .get(usize::try_from(style).ok()?)?
            .number_format_id
            .unwrap_or(0);
        self.number_formats
            .iter()
            .find(|format| format.id == id)
            .map(|format| format.code.as_str())
            .or_else(|| builtin_number_format(id))
    }
}

/// The code of a built-in number format, by ID, as ECMA-376 Part 1, 18.8.30 lists them for the
/// en-US locale; `None` for an ID it does not list.
pub fn builtin_number_format(id: u32) -> Option<&'static str> {
    Some(match id {
        0 => "General",
        1 => "0",
        2 => "0.00",
        3 => "#,##0",
        4 => "#,##0.00",
        9 => "0%",
        10 => "0.00%",
        11 => "0.00E+00",
        12 => "# ?/?",
        13 => "# ??/??",
        14 => "m/d/yyyy",
        15 => "d-mmm-yy",
        16 => "d-mmm",
        17 => "mmm-yy",
        18 => "h:mm AM/PM",
        19 => "h:mm:ss AM/PM",
        20 => "h:mm",
        21 => "h:mm:ss",
        22 => "m/d/yyyy h:mm",
        37 => "#,##0 ;(#,##0)",
        38 => "#,##0 ;[Red](#,##0)",
        39 => "#,##0.00;(#,##0.00)",
        40 => "#,##0.00;[Red](#,##0.00)",
        45 => "mm:ss",
        46 => "[h]:mm:ss",
        47 => "mmss.0",
        48 => "##0.0E+0",
        49 => "@",
        _ => return None,
    })
}

/// One uniform format over an inclusive rectangle. Blank positions need no cell records.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StyleRun {
    pub area: calc_value::GridArea,
    pub format: u32,
}

/// Sparse authored formatting. Later assignments override only their
/// rectangle. The existing immutable style-table command commits these with derived records.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct StyleRuns {
    runs: Vec<StyleRun>,
}
impl StyleRuns {
    /// Import ordered source records without repeatedly searching already distinct points.
    pub fn from_layers(runs: impl IntoIterator<Item = StyleRun>) -> Self {
        Self {
            runs: runs.into_iter().collect(),
        }
    }
    pub fn iter(&self) -> impl Iterator<Item = StyleRun> + '_ {
        self.runs.iter().copied()
    }
    pub fn get(&self, at: calc_value::CellKey) -> Option<u32> {
        self.runs
            .iter()
            .rev()
            .find(|run| run.area.contains(at))
            .map(|run| run.format)
    }
    pub fn assign(&mut self, run: StyleRun) {
        self.runs.retain(|old| {
            !(run.area.contains(old.area.first()) && run.area.contains(old.area.last()))
        });
        // Keep whole-axis defaults below later finite exceptions, so structural edits still
        // format new positions at the worksheet edge. Never sort overlapping assignments.
        self.runs.push(run);
    }

    /// The uniform regions of an area, applying sources in order over the workbook default.
    /// Cost follows stored style boundaries, never the number of cells in a blank rectangle.
    pub fn regions(
        area: calc_value::GridArea,
        sources: impl IntoIterator<Item = StyleRun>,
    ) -> Vec<StyleRun> {
        let mut regions = Self {
            runs: vec![StyleRun { area, format: 0 }],
        };
        for (index, run) in sources.into_iter().enumerate() {
            if let Some(part) = style_intersection(area, run.area) {
                regions.runs = regions
                    .runs
                    .drain(..)
                    .flat_map(|old| {
                        subtract_style_area(old.area, part)
                            .into_iter()
                            .map(move |area| StyleRun { area, ..old })
                    })
                    .collect();
                regions.runs.push(StyleRun { area: part, ..run });
            }
            if index % 64 == 63 {
                regions.coalesce();
            }
        }
        regions.coalesce();
        regions.runs
    }
    fn coalesce(&mut self) {
        // Merge complete adjacent strips only; partial overlap never erases a format boundary.
        loop {
            self.runs.sort_by_key(|r| {
                (
                    r.format,
                    r.area.sheet(),
                    r.area.first().row(),
                    r.area.last().row(),
                    r.area.first().column(),
                )
            });
            let mut changed = false;
            let mut next: Vec<StyleRun> = Vec::new();
            for run in self.runs.drain(..) {
                if let Some(last) = next.last_mut()
                    && last.format == run.format
                    && last.area.sheet() == run.area.sheet()
                    && last.area.first().row() == run.area.first().row()
                    && last.area.last().row() == run.area.last().row()
                    && last.area.last().column() + 1 == run.area.first().column()
                {
                    last.area =
                        calc_value::GridArea::new(last.area.first(), run.area.last()).unwrap();
                    changed = true;
                } else {
                    next.push(run);
                }
            }
            next.sort_by_key(|r| {
                (
                    r.format,
                    r.area.sheet(),
                    r.area.first().column(),
                    r.area.last().column(),
                    r.area.first().row(),
                )
            });
            self.runs = Vec::new();
            for run in next {
                if let Some(last) = self.runs.last_mut()
                    && last.format == run.format
                    && last.area.sheet() == run.area.sheet()
                    && last.area.first().column() == run.area.first().column()
                    && last.area.last().column() == run.area.last().column()
                    && last.area.last().row() + 1 == run.area.first().row()
                {
                    last.area =
                        calc_value::GridArea::new(last.area.first(), run.area.last()).unwrap();
                    changed = true;
                } else {
                    self.runs.push(run);
                }
            }
            if !changed {
                break;
            }
        }
    }
    /// Structural edits retain column/row defaults across the whole perpendicular axis.
    /// An inserted band inherits the formatting immediately before it, as existing cell reads do.
    pub fn moved(&self, edit: crate::AxisEdit) -> Self {
        use calc_value::{CellKey, GridArea, MAX_COLUMNS, MAX_ROWS};
        let mut next = Self::default();
        for run in self.iter() {
            if run.area.sheet() != edit.sheet() {
                next.runs.push(run);
                continue;
            }
            let (r1, c1, r2, c2) = (
                run.area.first().row(),
                run.area.first().column(),
                run.area.last().row(),
                run.area.last().column(),
            );
            let full = match edit.axis() {
                crate::Axis::Rows => r1 == 1 && r2 == MAX_ROWS,
                crate::Axis::Columns => c1 == 1 && c2 == MAX_COLUMNS,
            };
            if full {
                next.runs.push(run);
                continue;
            }
            if let Some(area) = crate::moved_area(edit, run.area) {
                next.runs.push(StyleRun { area, ..run });
            }
            let before = edit.first().saturating_sub(1);
            let inherits = match edit.axis() {
                crate::Axis::Rows => (r1..=r2).contains(&before),
                crate::Axis::Columns => (c1..=c2).contains(&before),
            };
            if edit.change() == crate::AxisChange::Insert && inherits {
                let (a, b) = match edit.axis() {
                    crate::Axis::Rows => ((edit.first(), c1), (edit.last(), c2)),
                    crate::Axis::Columns => ((r1, edit.first()), (r2, edit.last())),
                };
                let area = GridArea::new(
                    CellKey::new(edit.sheet(), a.0, a.1).unwrap(),
                    CellKey::new(edit.sheet(), b.0, b.1).unwrap(),
                )
                .unwrap();
                next.runs.push(StyleRun { area, ..run });
            }
        }
        next
    }
    pub fn without_sheet(&self, sheet: calc_value::SheetId) -> Self {
        Self {
            runs: self
                .runs
                .iter()
                .filter(|r| r.area.sheet() != sheet)
                .copied()
                .collect(),
        }
    }
}
pub fn style_intersection(
    a: calc_value::GridArea,
    b: calc_value::GridArea,
) -> Option<calc_value::GridArea> {
    if a.sheet() != b.sheet() {
        return None;
    }
    let first = calc_value::CellKey::new(
        a.sheet(),
        a.first().row().max(b.first().row()),
        a.first().column().max(b.first().column()),
    )
    .ok()?;
    let last = calc_value::CellKey::new(
        a.sheet(),
        a.last().row().min(b.last().row()),
        a.last().column().min(b.last().column()),
    )
    .ok()?;
    if first.row() > last.row() || first.column() > last.column() {
        return None;
    }
    calc_value::GridArea::new(first, last).ok()
}
fn subtract_style_area(
    area: calc_value::GridArea,
    cut: calc_value::GridArea,
) -> Vec<calc_value::GridArea> {
    let Some(hit) = style_intersection(area, cut) else {
        return vec![area];
    };
    let (r1, c1, r2, c2) = (
        area.first().row(),
        area.first().column(),
        area.last().row(),
        area.last().column(),
    );
    let (a, b, c, d) = (
        hit.first().row(),
        hit.first().column(),
        hit.last().row(),
        hit.last().column(),
    );
    [
        (r1, c1, a - 1, c2),
        (c + 1, c1, r2, c2),
        (a, c1, c, b - 1),
        (a, d + 1, c, c2),
    ]
    .into_iter()
    .filter(|(r, c, a, b)| r <= a && c <= b)
    .filter_map(|(r, c, a, b)| {
        calc_value::GridArea::new(
            calc_value::CellKey::new(area.sheet(), r, c).ok()?,
            calc_value::CellKey::new(area.sheet(), a, b).ok()?,
        )
        .ok()
    })
    .collect()
}

#[cfg(test)]
mod run_tests {
    use super::*;
    use calc_value::{CellKey, GridArea, MAX_COLUMNS, MAX_ROWS, SheetId};
    fn area(s: SheetId, a: u32, b: u32, c: u32, d: u32) -> GridArea {
        GridArea::new(
            CellKey::new(s, a, b).unwrap(),
            CellKey::new(s, c, d).unwrap(),
        )
        .unwrap()
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn overlapping_regions_match_all_positions_and_never_bridge_an_empty_intersection() {
        let s = SheetId::from_parts(0, 0);
        let mut runs = StyleRuns::default();
        let mut expected = [[0u32; 8]; 8];
        let mut seed = 41u32;
        for step in 1..=40 {
            let mut next = || {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                seed % 8 + 1
            };
            let a = area(s, next(), next(), next(), next());
            let format = step % 5;
            runs.assign(StyleRun { area: a, format });
            for r in a.first().row()..=a.last().row() {
                for c in a.first().column()..=a.last().column() {
                    expected[(r - 1) as usize][(c - 1) as usize] = format;
                }
            }
            let regions = StyleRuns::regions(area(s, 1, 1, 8, 8), runs.iter());
            assert_eq!(regions.iter().map(|r| r.area.cell_count()).sum::<u64>(), 64);
            for r in 1..=8 {
                for c in 1..=8 {
                    let at = CellKey::new(s, r, c).unwrap();
                    assert_eq!(
                        runs.get(at).unwrap_or(0),
                        expected[(r - 1) as usize][(c - 1) as usize]
                    );
                    let hits: Vec<_> = regions.iter().filter(|r| r.area.contains(at)).collect();
                    assert_eq!(hits.len(), 1);
                    assert_eq!(hits[0].format, expected[(r - 1) as usize][(c - 1) as usize]);
                }
            }
        }
        assert!(style_intersection(area(s, 1, 1, 1, 2), area(s, 3, 4, 4, 5)).is_none());
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn whole_axis_defaults_survive_edge_changes_below_finite_exceptions() {
        let s = SheetId::from_parts(0, 0);
        let mut runs = StyleRuns::default();
        runs.assign(StyleRun {
            area: area(s, 1, 2, MAX_ROWS, 2),
            format: 1,
        });
        runs.assign(StyleRun {
            area: area(s, 5, 1, 5, MAX_COLUMNS),
            format: 2,
        });
        runs.assign(StyleRun {
            area: area(s, 5, 2, 5, 2),
            format: 3,
        });
        let edit =
            crate::AxisEdit::new(s, crate::Axis::Rows, crate::AxisChange::Delete, 7, 1).unwrap();
        let moved = runs.moved(edit);
        assert_eq!(moved.get(CellKey::new(s, MAX_ROWS, 2).unwrap()), Some(1));
        let edit =
            crate::AxisEdit::new(s, crate::Axis::Columns, crate::AxisChange::Delete, 1, 1).unwrap();
        let moved = moved.moved(edit);
        assert_eq!(moved.get(CellKey::new(s, 5, MAX_COLUMNS).unwrap()), Some(2));
        assert_eq!(moved.get(CellKey::new(s, 5, 1).unwrap()), Some(3));
        assert_eq!(moved.get(CellKey::new(s, MAX_ROWS, 1).unwrap()), Some(1));
        assert!(moved.iter().count() < 8);
    }
}
