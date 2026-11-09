//! Atomic cell formatting through the ordinary authored command and history route.
use crate::{CellEdit, HostState};
use calc_doc_model::{
    Border, BorderEdge, CellFormat, Fill, Font, NumberFormat, StyleColor, StyleTable,
};
use calc_value::{CellKey, GridArea};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// Built-in dates follow the existing regional date profile. The en-ZA ID 37
/// follows the Excel format reading retained by dc-971p.43; the workbook stores IDs.
pub(crate) fn builtin_formats(profile: Option<&crate::FormatProfile>) -> BTreeMap<u32, String> {
    let mut codes = BTreeMap::new();
    if let Some(profile) = profile {
        let date = profile.short_date_pattern.to_ascii_lowercase();
        codes.insert(14, date.clone());
        codes.insert(22, format!("{date} h:mm"));
        if profile.id == calc_func::locale::LocaleId::EnZa {
            codes.insert(37, "#,##0;-#,##0".to_owned());
        }
    }
    codes
}

#[cfg(test)]
mod regional_format_tests {
    use super::*;
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn builtins_follow_the_profile_and_declared_codes_win() {
        let mut table = StyleTable::default();
        table.cell_formats = [14, 22, 37]
            .into_iter()
            .map(|id| CellFormat {
                number_format_id: Some(id),
                ..Default::default()
            })
            .collect();
        for (locale, date) in [
            ("en-US", "m/d/yyyy"),
            ("en-GB", "dd/mm/yyyy"),
            ("en-ZA", "yyyy/mm/dd"),
            ("de-DE", "dd.mm.yyyy"),
        ] {
            let profile = crate::FormatProfile::for_culture(locale).unwrap();
            let codes = builtin_formats(Some(&profile));
            let code = |style| {
                table.number_format_code_with(style, |id| codes.get(&id).map(String::as_str))
            };
            assert_eq!(code(0), Some(date));
            assert_eq!(code(1), Some(format!("{date} h:mm").as_str()));
            if locale == "en-ZA" {
                let profile = profile.with_separators(".", " ").unwrap();
                let system = calc_func::locale::DateSystem::System1900;
                assert_eq!(
                    calc_func::cell_text(
                        &profile,
                        system,
                        &calc_value::CalcValue::number(45234.567),
                        code(0).unwrap()
                    ),
                    Ok("2023/11/04".into())
                );
                assert_eq!(
                    calc_func::cell_text(
                        &profile,
                        system,
                        &calc_value::CalcValue::number(45234.567),
                        code(2).unwrap()
                    ),
                    Ok("45 235".into())
                );
                assert_eq!(
                    calc_func::cell_text(
                        &profile,
                        system,
                        &calc_value::CalcValue::number(-45234.567),
                        code(2).unwrap()
                    ),
                    Ok("-45 235".into())
                );
            }
        }
        let profile = crate::FormatProfile::for_culture("en-ZA").unwrap();
        let codes = builtin_formats(Some(&profile));
        table.number_formats.push(NumberFormat {
            id: 14,
            code: "yyyy-mm".into(),
        });
        assert_eq!(
            table.number_format_code_with(0, |id| codes.get(&id).map(String::as_str)),
            Some("yyyy-mm")
        );
        assert_eq!(table.cell_formats[0].number_format_id, Some(14));
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn entered_cells_resolve_locale_builtins_through_the_application_host() {
        use crate::{
            HostOptions, HostSession, MemorySource, OpenPolicy, ProjectedValue, ProjectionRequest,
            SkinIntent,
        };
        let profile = crate::FormatProfile::for_culture("en-ZA")
            .unwrap()
            .with_separators(".", " ")
            .unwrap();
        let mut book = HostSession::open(
            Arc::new(MemorySource::new(crate::blank_package())),
            &OpenPolicy::default(),
            HostOptions {
                locale: Some(profile),
                ..Default::default()
            },
        )
        .unwrap();
        let sheet = book.host().sheets()[0].id();
        let at = [
            CellKey::new(sheet, 5, 7).unwrap(),
            CellKey::new(sheet, 7, 7).unwrap(),
        ];
        let mut table = (**book.host().styles()).clone();
        let first = table.cell_formats.len() as u32;
        table
            .cell_formats
            .extend([14, 37].into_iter().map(|id| CellFormat {
                number_format_id: Some(id),
                apply_number_format: true,
                ..Default::default()
            }));
        let receipt = book.dispatch(SkinIntent::Edit {
            edits: vec![
                CellEdit::Styles(Arc::new(table)),
                CellEdit::SetFormat {
                    at: at[0],
                    format: Some(first),
                },
                CellEdit::SetFormat {
                    at: at[1],
                    format: Some(first + 1),
                },
            ],
        });
        assert!(receipt.accepted(), "{:?}", receipt.error);
        for at in at {
            let receipt = book.dispatch(SkinIntent::EnterCell {
                at,
                text: "45234.567".into(),
            });
            assert!(receipt.accepted(), "{:?}", receipt.error);
        }
        let projected =
            crate::project(book.host(), &ProjectionRequest { cells: at.to_vec() }).unwrap();
        for (cell, expected) in projected.cells().iter().zip(["2023/11/04", "45 235"]) {
            let ProjectedValue::Literal(value) = cell.value() else {
                panic!("Entered value must retain literal provenance")
            };
            assert_eq!(value.as_number(), Some(45234.567));
            let code = book.host().number_format(cell.at()).unwrap();
            assert_eq!(
                calc_func::cell_text(
                    book.host().options().locale.as_ref().unwrap(),
                    calc_func::locale::DateSystem::System1900,
                    value,
                    code
                ),
                Ok(expected.into())
            );
        }
        assert_eq!(
            book.host().styles().cell_formats[first as usize].number_format_id,
            Some(14)
        );
        assert_eq!(
            book.host().styles().cell_formats[first as usize + 1].number_format_id,
            Some(37)
        );
    }
}

/// None leaves the property alone; empty colours mean automatic font colour or no fill.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FormatPatch {
    pub number_format: Option<String>,
    pub horizontal: Option<String>,
    pub vertical: Option<String>,
    pub wrap: Option<bool>,
    pub shrink: Option<bool>,
    pub rotation: Option<i16>,
    pub indent: Option<u16>,
    pub font_name: Option<String>,
    pub font_size: Option<String>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub underline_style: Option<String>,
    pub strike: Option<bool>,
    pub font_color: Option<String>,
    pub fill: Option<String>,
    pub borders: Option<BorderPatch>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BorderPatch {
    pub left: bool,
    pub right: bool,
    pub top: bool,
    pub bottom: bool,
    pub style: String,
    pub color: String,
}

pub(crate) fn color(value: &str) -> Result<Option<StyleColor>, String> {
    if value.is_empty() {
        return Ok(None);
    }
    let rgb = value.strip_prefix('#').unwrap_or(value);
    if rgb.len() != 6 || !rgb.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Use a six-digit RGB colour".into());
    }
    Ok(Some(StyleColor {
        rgb: Some(format!("FF{}", rgb.to_ascii_uppercase())),
        ..Default::default()
    }))
}
impl FormatPatch {
    pub fn validate(&self) -> Result<(), String> {
        if self.number_format.as_ref().is_some_and(|s| {
            s.is_empty() || s.encode_utf16().count() > 255 || s.chars().any(char::is_control)
        }) {
            return Err("Use a number format of 1 to 255 characters".into());
        }
        if self.horizontal.as_deref().is_some_and(|s| {
            !matches!(
                s,
                "general"
                    | "left"
                    | "center"
                    | "right"
                    | "fill"
                    | "justify"
                    | "centerContinuous"
                    | "distributed"
            )
        }) {
            return Err("Choose a horizontal alignment".into());
        }
        if self
            .vertical
            .as_deref()
            .is_some_and(|s| !matches!(s, "top" | "center" | "bottom" | "justify" | "distributed"))
        {
            return Err("Choose a vertical alignment".into());
        }
        if self
            .rotation
            .is_some_and(|n| n != 255 && !(-90..=90).contains(&n))
            || self.indent.is_some_and(|n| n > 250)
        {
            return Err(
                "Rotation must be -90 to 90 degrees or stacked text; indent 0 to 250".into(),
            );
        }
        if self.font_name.as_ref().is_some_and(|s| {
            s.trim().is_empty() || s.encode_utf16().count() > 255 || s.chars().any(char::is_control)
        }) {
            return Err("Use a font family of 1 to 255 characters".into());
        }
        if self.font_size.as_ref().is_some_and(|s| {
            s.parse::<f64>()
                .map_or(true, |n| !n.is_finite() || !(1.0..=409.0).contains(&n))
        }) {
            return Err("Font size must be 1 to 409 points".into());
        }
        if self
            .underline_style
            .as_deref()
            .is_some_and(|v| !matches!(v, "none" | "single" | "double"))
        {
            return Err("Choose no underline, single or double underline".into());
        }
        if let (Some(on), Some(kind)) = (self.underline, self.underline_style.as_deref()) {
            if on != (kind != "none") {
                return Err("The underline choices disagree".into());
            }
        }
        for c in [&self.font_color, &self.fill].into_iter().flatten() {
            color(c)?;
        }
        if let Some(b) = &self.borders {
            if !matches!(
                b.style.as_str(),
                "thin"
                    | "medium"
                    | "thick"
                    | "double"
                    | "dotted"
                    | "dashed"
                    | "dashDot"
                    | "dashDotDot"
                    | "mediumDashed"
                    | "mediumDashDot"
                    | "mediumDashDotDot"
                    | "slantDashDot"
                    | "hair"
            ) {
                return Err("Choose a border line style".into());
            }
            color(&b.color)?;
        }
        Ok(())
    }
}
fn defaults(table: &mut StyleTable) {
    if table.fonts.is_empty() {
        table.fonts.push(Font {
            name: Some("Calibri".into()),
            size: Some("11".into()),
            ..Default::default()
        });
    }
    if table.fills.is_empty() {
        table.fills.extend([
            Fill {
                pattern_type: Some("none".into()),
                ..Default::default()
            },
            Fill {
                pattern_type: Some("gray125".into()),
                ..Default::default()
            },
        ]);
    }
    if table.borders.is_empty() {
        table.borders.push(Border::default());
    }
    if table.cell_style_formats.is_empty() {
        table.cell_style_formats.push(CellFormat::default());
    }
    if table.cell_formats.is_empty() {
        table.cell_formats.push(CellFormat {
            font_id: Some(0),
            fill_id: Some(0),
            border_id: Some(0),
            xf_id: Some(0),
            ..Default::default()
        });
    }
}

/// Resolve one style without changing the table's imported records. Derivations retain the
/// source index so the codec can copy unknown XML fields into the appended record.
pub fn patched_style(
    table: &mut StyleTable,
    index: u32,
    patch: &FormatPatch,
) -> Result<u32, String> {
    patch.validate()?;
    defaults(table);
    let original = table
        .cell_formats
        .get(index as usize)
        .ok_or("The source cell format is unavailable")?
        .clone();
    let mut xf = original.clone();
    let parent = xf
        .xf_id
        .and_then(|id| table.cell_style_formats.get(id as usize))
        .cloned()
        .unwrap_or_default();
    if let Some(code) = &patch.number_format {
        let id = (0..=163)
            .find(|id| {
                calc_doc_model::builtin_number_format(*id).is_some_and(|c| {
                    c == code
                        || c.eq_ignore_ascii_case("general") && code.eq_ignore_ascii_case("general")
                })
            })
            .or_else(|| {
                table
                    .number_formats
                    .iter()
                    .find(|n| &n.code == code)
                    .map(|n| n.id)
            });
        let id = match id {
            Some(id) => id,
            None => {
                let id = table
                    .number_formats
                    .iter()
                    .map(|n| n.id)
                    .max()
                    .unwrap_or(163)
                    .max(163)
                    .checked_add(1)
                    .ok_or("No number format IDs remain")?;
                table.number_formats.push(NumberFormat {
                    id,
                    code: code.clone(),
                });
                id
            }
        };
        xf.number_format_id = Some(id);
        xf.apply_number_format = true;
    }
    let mut a = xf
        .alignment
        .clone()
        .or(parent.alignment)
        .unwrap_or_default();
    let before = a.clone();
    if let Some(v) = &patch.horizontal {
        a.horizontal = Some(v.clone());
    }
    if let Some(v) = &patch.vertical {
        a.vertical = Some(v.clone());
    }
    if let Some(v) = patch.wrap {
        a.wrap_text = v;
    }
    if let Some(v) = patch.shrink {
        a.shrink_to_fit = v;
    }
    if let Some(v) = patch.rotation {
        a.text_rotation = Some(if v < 0 { 90 - v } else { v }.to_string());
    }
    if let Some(v) = patch.indent {
        a.indent = Some(v.to_string());
    }
    if a != before {
        xf.alignment = Some(a);
        xf.apply_alignment = true;
    }
    let source = xf.font_id.or(parent.font_id).unwrap_or(0);
    let mut font = table
        .fonts
        .get(source as usize)
        .ok_or("The source font is unavailable")?
        .clone();
    let before = font.clone();
    if let Some(v) = &patch.font_name {
        font.name = Some(v.clone());
        font.scheme = None;
    }
    if let Some(v) = &patch.font_size {
        font.size = Some(v.clone());
    }
    if let Some(v) = patch.bold {
        font.bold = v;
    }
    if let Some(v) = patch.italic {
        font.italic = v;
    }
    if let Some(v) = patch.underline {
        font.underline = v.then(|| "single".into());
    }
    if let Some(v) = &patch.underline_style {
        font.underline = (v != "none").then(|| v.clone());
    }
    if let Some(v) = patch.strike {
        font.strike = v;
    }
    if let Some(v) = &patch.font_color {
        font.color = color(v)?;
    }
    if font != before {
        let id = u32::try_from(table.fonts.len()).map_err(|_| "Too many fonts")?;
        table.fonts.push(font);
        table.sources.fonts.insert(id, source);
        xf.font_id = Some(id);
        xf.apply_font = true;
    }
    if let Some(v) = &patch.fill {
        let source = xf.fill_id.or(parent.fill_id).unwrap_or(0);
        let before = table
            .fills
            .get(source as usize)
            .ok_or("The source fill is unavailable")?;
        let fill = Fill {
            gradient: None,
            pattern_type: Some(if v.is_empty() { "none" } else { "solid" }.into()),
            fg_color: color(v)?,
            bg_color: None,
            other_children: Vec::new(),
        };
        if &fill != before {
            let id = table.fills.len() as u32;
            table.fills.push(fill);
            table.sources.fills.insert(id, source);
            xf.fill_id = Some(id);
            xf.apply_fill = true;
        }
    }
    if let Some(v) = &patch.borders {
        let source = xf.border_id.or(parent.border_id).unwrap_or(0);
        let mut border = table
            .borders
            .get(source as usize)
            .ok_or("The source border is unavailable")?
            .clone();
        let before = border.clone();
        let edge = |on| -> Result<Option<BorderEdge>, String> {
            Ok(if on {
                Some(BorderEdge {
                    style: Some(v.style.clone()),
                    color: color(&v.color)?,
                })
            } else {
                None
            })
        };
        border.left = edge(v.left)?;
        border.right = edge(v.right)?;
        border.top = edge(v.top)?;
        border.bottom = edge(v.bottom)?;
        if border != before {
            let id = table.borders.len() as u32;
            table.borders.push(border);
            table.sources.borders.insert(id, source);
            xf.border_id = Some(id);
            xf.apply_border = true;
        }
    }
    if xf == original {
        return Ok(index);
    }
    let id = u32::try_from(table.cell_formats.len()).map_err(|_| "Too many cell formats")?;
    table.cell_formats.push(xf);
    table.sources.cell_formats.insert(id, index);
    Ok(id)
}

pub(crate) fn prepare(
    host: &HostState,
    areas: &[GridArea],
    patch: &FormatPatch,
) -> Result<Vec<CellEdit>, String> {
    patch.validate()?;
    if areas.is_empty() {
        return Err("Select cells to format".into());
    }
    for sheet in areas.iter().map(|a| a.sheet()).collect::<BTreeSet<_>>() {
        crate::can_edit(host, CellKey::new(sheet, 1, 1).unwrap())?;
    }
    if patch == &FormatPatch::default() {
        return Ok(Vec::new());
    }
    let mut table = (**host.styles()).clone();
    let mut mapped = BTreeMap::new();
    let mut edits = Vec::new();
    for area in areas {
        let mut changes = Vec::new();
        for run in format_regions(host, *area)? {
            let next = if let Some(id) = mapped.get(&run.format) {
                *id
            } else {
                let id = patched_style(&mut table, run.format, patch)?;
                mapped.insert(run.format, id);
                id
            };
            changes.push((run, next));
        }
        if !changes.iter().any(|(run, next)| run.format != *next) {
            continue;
        }
        let whole_axis = (area.first().row() == 1 && area.last().row() == calc_value::MAX_ROWS)
            || (area.first().column() == 1 && area.last().column() == calc_value::MAX_COLUMNS);
        let baseline = if whole_axis {
            let id = if let Some(id) = mapped.get(&0) {
                *id
            } else {
                let id = patched_style(&mut table, 0, patch)?;
                mapped.insert(0, id);
                id
            };
            table.runs.assign(calc_doc_model::StyleRun {
                area: *area,
                format: id,
            });
            Some(id)
        } else {
            None
        };
        for (run, next) in changes {
            if baseline.map_or(next != run.format, |base| next != base) {
                table.runs.assign(calc_doc_model::StyleRun {
                    format: next,
                    ..run
                });
            }
        }
    }
    for (at, old) in host
        .model()
        .definitions()
        .cell_formats()
        .filter(|(at, _)| areas.iter().any(|a| a.contains(*at)))
    {
        let next = if let Some(id) = mapped.get(&old) {
            *id
        } else {
            let id = patched_style(&mut table, old, patch)?;
            mapped.insert(old, id);
            id
        };
        if old != next {
            edits.push(CellEdit::SetFormat {
                at,
                format: Some(next),
            });
        }
    }
    if &table != host.styles().as_ref() {
        edits.insert(0, CellEdit::Styles(Arc::new(table)));
    }
    Ok(edits)
}

/// Current uniform styles, including imported axis defaults and existing style points.
/// This visits stored records and their boundaries, not every cell of the requested rectangle.
pub fn format_regions(
    host: &HostState,
    area: GridArea,
) -> Result<Vec<calc_doc_model::StyleRun>, String> {
    use calc_doc_model::{StyleRun, StyleRuns};
    if host.model().sheet_by_id(area.sheet()).is_none() {
        return Err("A selected sheet is unavailable".into());
    }
    let mut layers = Vec::new();
    if let Some(sheet) = host.loaded().sheet(area.sheet()) {
        layers.extend(sheet.default_styles());
        // One record per cell is how a file holds a formatted rectangle: join them into
        // rectangles before the region search, whose cost grows with its sources (dc-0f1k.11.3).
        layers.extend(StyleRuns::coalesced_cells(
            area.sheet(),
            sheet.explicit_styles().filter(|&(r, c, format)| {
                format != sheet.default_style(r, c)
                    && (!host.axis_edits.is_empty()
                        || area.contains(CellKey::new(area.sheet(), r, c).unwrap()))
            }),
        ));
    }
    let mut imported = StyleRuns::from_layers(layers);
    for edit in &host.axis_edits {
        imported = imported.moved(*edit);
    }
    let sources = imported.iter().chain(host.styles().runs.iter()).chain(
        host.model()
            .definitions()
            .cell_formats()
            .map(|(at, format)| StyleRun {
                area: GridArea::cell(at),
                format,
            }),
    );
    Ok(StyleRuns::regions(area, sources))
}
/// Representative cells for exact mixed-property inspection, one per actual style in the areas.
pub fn format_samples(host: &HostState, areas: &[GridArea]) -> Result<Vec<CellKey>, String> {
    if areas.is_empty() {
        return Err("Select cells to format".into());
    }
    let mut styles = BTreeMap::new();
    for area in areas {
        for run in format_regions(host, *area)? {
            styles.entry(run.format).or_insert(run.area.first());
        }
    }
    Ok(styles.into_values().collect())
}
