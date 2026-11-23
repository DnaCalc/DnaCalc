//! Measured built-in layers for captured worksheet pivot layouts.
//! These layers are presentation only; cell formats and saved pivot records remain unchanged.
use crate::HostState;
use calc_doc_model::{BorderEdge, StyleColor};
use calc_value::CellKey;

/// The supported pivot style additions, before conditional formatting.
#[derive(Clone, Debug, Default)]
pub struct PivotStyleLook {
    pub bold: Option<bool>,
    pub font_color: Option<StyleColor>,
    pub fill: Option<StyleColor>,
    /// Left, top, bottom, right. An absent edge leaves the cell's own edge in place.
    pub borders: [Option<BorderEdge>; 4],
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Preset {
    Light1,
    Light8,
    Light16,
    Light21,
    Medium1,
    Medium2,
    Medium9,
    Medium14,
    Dark1,
    Dark6,
    Dark11,
}

fn theme(index: u32, tint: &str) -> StyleColor {
    StyleColor {
        theme: Some(index.to_string()),
        tint: Some(tint.into()),
        ..Default::default()
    }
}
fn rgb(color: &str) -> StyleColor {
    StyleColor {
        rgb: Some(color.into()),
        ..Default::default()
    }
}
fn pivot_tint(host: &HostState, index: u32, percent: i8) -> Option<StyleColor> {
    let base = calc_doc::look::rgb_triple(&theme(index, "0"), host.loaded().theme_colors())?;
    let (r, g, b) = calc_doc::look::pivot_style_tint(base, percent);
    Some(rgb(&format!("FF{r:02X}{g:02X}{b:02X}")))
}
fn edge(style: &str, color: StyleColor) -> BorderEdge {
    BorderEdge {
        style: Some(style.into()),
        color: Some(color),
    }
}
fn flag(style: &calc_doc_model::pivot::PivotStyle, name: &str) -> Option<bool> {
    match style.attributes.get(name).map(String::as_str)? {
        "1" | "true" => Some(true),
        "0" | "false" => Some(false),
        _ => None,
    }
}

// Excel's displayed format spelling omits escapes for these captured literal characters.
// Escaped dollars and commas remain; unmeasured escape and padding syntax is refused.
fn display_number_code(code: &str) -> Option<(String, bool)> {
    let mut result = String::new();
    let mut chars = code.chars();
    let mut quoted = false;
    let mut bracketed = false;
    let mut changed = false;
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            let next = chars.next()?;
            if quoted || bracketed {
                return None;
            }
            match next {
                '-' | '(' | ')' | ' ' => changed = true,
                '$' | ',' => result.push('\\'),
                _ => return None,
            }
            result.push(next);
        } else {
            if !quoted && !bracketed && matches!(ch, '_' | '*') {
                return None;
            }
            if ch == '"' && !bracketed {
                quoted = !quoted;
            }
            if !quoted && ch == '[' {
                if bracketed {
                    return None;
                }
                bracketed = true;
            } else if !quoted && ch == ']' {
                if !bracketed {
                    return None;
                }
                bracketed = false;
            }
            result.push(ch);
        }
    }
    (!quoted && !bracketed).then_some((result, changed))
}

impl HostState {
    /// A measured DisplayFormat number code for the captured value-field layouts. This is
    /// separate from the cell format used to render text: Excel reports General on these grand
    /// total cells even when their displayed text uses the value-field format.
    pub fn pivot_display_number_format(&self, at: CellKey) -> Option<String> {
        let definitions = self.model().definitions();
        if definitions.cell_format(at).is_some()
            || !definitions.conditional_rules(at.sheet()).is_empty()
        {
            return None;
        }
        let state = definitions.pivots();
        let sheet = state.sheet(at.sheet())?;
        if !state.caches.issues.is_empty() || !sheet.issues.is_empty() {
            return None;
        }
        let mut tables = sheet.tables.iter().filter(|table| table.area.contains(at));
        let table = tables.next()?;
        if tables.next().is_some()
            || table.row_fields.iter().any(|field| *field < 0)
            || !table.page_fields.is_empty()
            || !table.filters.is_empty()
            || !table.formats.is_empty()
            || !table.unsupported.is_empty()
        {
            return None;
        }
        let cache = state.caches.caches.get(&table.cache_id)?;
        if cache.source_type != "worksheet"
            || !cache.unsupported.is_empty()
            || !cache.calculated_items.is_empty()
            || cache
                .fields
                .iter()
                .any(|field| field.group.is_some() || !field.formula_items.is_empty())
        {
            return None;
        }
        let first = table.area.first();
        let header: u32 = table.location.get("firstHeaderRow")?.parse().ok()?;
        let row_offset: u32 = table.location.get("firstDataRow")?.parse().ok()?;
        let col_offset: u32 = table.location.get("firstDataCol")?.parse().ok()?;
        let styles = self.styles();
        let xf = styles.cell_formats.get(self.cell_style(at) as usize)?;
        let parent = xf
            .xf_id
            .and_then(|id| styles.cell_style_formats.get(id as usize));
        let cell_id = xf
            .number_format_id
            .or_else(|| parent.and_then(|p| p.number_format_id))
            .unwrap_or(0);
        // In the captured date-axis layout Excel reports General for the header's
        // DisplayFormat while retaining the date format for its visible label text.
        if table.row_fields.len() == 2
            && table.row_fields[0] != table.row_fields[1]
            && table.column_fields.len() == 1
            && table.column_fields[0] >= 0
            && !table.row_fields.contains(&table.column_fields[0])
            && table.data_fields.len() == 1
            && header == 1
            && row_offset == 2
            && col_offset == 1
            && at.row() == first.row().checked_add(1)?
        {
            let style = table.style.as_ref()?;
            if style.attributes.get("name")?.as_str() != "PivotStyleMedium9"
                || !flag(style, "showRowHeaders")?
                || !flag(style, "showColHeaders")?
                || !flag(style, "showRowStripes")?
                || !flag(style, "showColStripes")?
            {
                return None;
            }
            let field = cache.fields.get(table.column_fields[0] as usize)?;
            if field.group.is_some()
                || !field.formula_items.is_empty()
                || field.items.is_empty()
                || field
                    .items
                    .iter()
                    .any(|item| !matches!(item, calc_doc_model::pivot::PivotItem::Date(_)))
            {
                return None;
            }
            let id: u32 = field.attributes.get("numFmtId")?.parse().ok()?;
            if id != cell_id {
                return None;
            }
            display_number_code(self.number_format(at)?)?;
            let columns = table
                .area
                .last()
                .column()
                .checked_sub(first.column().checked_add(col_offset)?)?
                .checked_add(1)?;
            if table.col_items.len() != columns as usize
                || table
                    .col_items
                    .last()
                    .is_none_or(|item| item.kind != "grand")
                || table.col_items[..table.col_items.len() - 1]
                    .iter()
                    .any(|item| item.kind != "data")
            {
                return None;
            }
            let col =
                at.column()
                    .checked_sub(first.column().checked_add(col_offset)?)? as usize;
            let item = table.col_items.get(col)?;
            if !matches!(item.kind.as_str(), "data" | "grand") {
                return None;
            }
            return Some("General".into());
        }
        if table.row_fields.len() != 1 {
            return None;
        }
        let style = table.style.as_ref()?;
        if style.attributes.get("name")?.as_str() != "PivotStyleLight16"
            || !flag(style, "showRowHeaders")?
            || !flag(style, "showColHeaders")?
            || flag(style, "showRowStripes")?
            || flag(style, "showColStripes")?
        {
            return None;
        }
        let data_fields = table.data_fields.len();
        let shape = match data_fields {
            1 => {
                table.column_fields.len() == 1
                    && table.column_fields[0] >= 0
                    && table.column_fields[0] != table.row_fields[0]
            }
            2 => {
                table.column_fields.len() == 2
                    && table.column_fields[0] >= 0
                    && table.column_fields[0] != table.row_fields[0]
                    && table.column_fields[1] == -2
            }
            _ => false,
        };
        if !shape {
            return None;
        }
        if header != 1 || row_offset != data_fields as u32 + 1 || col_offset != 1 {
            return None;
        }
        let row = at.row().checked_sub(first.row().checked_add(row_offset)?)? as usize;
        let col = at
            .column()
            .checked_sub(first.column().checked_add(col_offset)?)? as usize;
        let row_count = table
            .area
            .last()
            .row()
            .checked_sub(first.row().checked_add(row_offset)?)?
            .checked_add(1)?;
        let col_count = table
            .area
            .last()
            .column()
            .checked_sub(first.column().checked_add(col_offset)?)?
            .checked_add(1)?;
        if table.row_items.len() != row_count as usize
            || table.col_items.len() != col_count as usize
            || table
                .row_items
                .last()
                .is_none_or(|item| item.kind != "grand")
            || table.row_items[..table.row_items.len() - 1]
                .iter()
                .any(|item| item.kind != "data")
        {
            return None;
        }
        if data_fields == 1 {
            if table
                .col_items
                .last()
                .is_none_or(|item| item.kind != "grand")
                || table.col_items[..table.col_items.len() - 1]
                    .iter()
                    .any(|item| item.kind != "data" || item.data_index != 0)
            {
                return None;
            }
        } else {
            if table.col_items.len() < 2 || table.col_items.len() % 2 != 0 {
                return None;
            }
            let pairs = table.col_items.len() / 2;
            if table
                .col_items
                .chunks_exact(2)
                .enumerate()
                .any(|(index, pair)| {
                    let kind = if index + 1 == pairs { "grand" } else { "data" };
                    pair[0].kind != kind
                        || pair[1].kind != kind
                        || pair[0].data_index != 0
                        || pair[1].data_index != 1
                })
            {
                return None;
            }
        }
        let row_item = table.row_items.get(row)?;
        if !matches!(row_item.kind.as_str(), "data" | "grand") {
            return None;
        }
        let column = table.col_items.get(col)?;
        if !matches!(column.kind.as_str(), "data" | "grand") {
            return None;
        }
        let field = table.data_fields.get(column.data_index as usize)?;
        let id: u32 = field.attributes.get("numFmtId")?.parse().ok()?;
        if cell_id != id {
            return None;
        }
        let (code, changed) = display_number_code(self.number_format(at)?)?;
        if row_item.kind == "grand" {
            Some("General".into())
        } else {
            changed.then_some(code)
        }
    }

    /// The supported preset's style layer for this cell. Unsupported shapes and area formats
    /// leave the stored appearance alone until their precedence is modelled.
    pub fn pivot_style_look(&self, at: CellKey) -> Option<PivotStyleLook> {
        let state = self.model().definitions().pivots();
        let mut tables = state
            .sheet(at.sheet())?
            .tables
            .iter()
            .filter(|table| table.area.contains(at));
        let table = tables.next()?;
        let column_layout = table.row_fields.len() == 1
            && table.row_fields[0] >= 0
            && table.column_fields.len() == 1
            && table.column_fields[0] >= 0
            && table.data_fields.len() == 1;
        let row_layout = table.row_fields.len() == 2
            && table.row_fields.iter().all(|field| *field >= 0)
            && table.row_fields[0] != table.row_fields[1]
            && table.column_fields.as_slice() == [-2]
            && table.data_fields.len() == 2;
        if tables.next().is_some()
            || (!column_layout && !row_layout)
            || !table.page_fields.is_empty()
            || !table.filters.is_empty()
            || !table.formats.is_empty()
            || !table.unsupported.is_empty()
        {
            return None;
        }
        let style = table.style.as_ref()?;
        let preset = match style.attributes.get("name")?.as_str() {
            "PivotStyleLight1" => Preset::Light1,
            "PivotStyleLight8" => Preset::Light8,
            "PivotStyleLight16" => Preset::Light16,
            "PivotStyleLight21" => Preset::Light21,
            "PivotStyleMedium1" => Preset::Medium1,
            "PivotStyleMedium2" => Preset::Medium2,
            "PivotStyleMedium9" => Preset::Medium9,
            "PivotStyleMedium14" => Preset::Medium14,
            "PivotStyleDark1" => Preset::Dark1,
            "PivotStyleDark6" => Preset::Dark6,
            "PivotStyleDark11" => Preset::Dark11,
            _ => return None,
        };
        let light = matches!(
            preset,
            Preset::Light1 | Preset::Light8 | Preset::Light16 | Preset::Light21
        );
        let dark = matches!(preset, Preset::Dark1 | Preset::Dark6 | Preset::Dark11);
        let header = flag(style, "showColHeaders")?;
        let row_headers = flag(style, "showRowHeaders")?;
        let rows = flag(style, "showRowStripes")?;
        let columns = flag(style, "showColStripes")?;
        let _last_column = flag(style, "showLastColumn")?;
        if rows && columns || row_layout && (rows || columns || !header || !row_headers) {
            return None;
        }
        if !matches!(preset, Preset::Light16 | Preset::Medium9) && (rows || columns) {
            return None;
        }
        if dark && (!header || !row_headers) {
            return None;
        }
        // The fixed neutral colours below are measured with a white background and black text.
        let neutral = matches!(
            preset,
            Preset::Light1 | Preset::Light8 | Preset::Medium1 | Preset::Dark1 | Preset::Dark11
        );
        if (neutral || preset == Preset::Light16 && (rows || columns))
            && self
                .resolved_style_color(&StyleColor {
                    theme: Some("0".into()),
                    ..Default::default()
                })
                .is_none_or(|color| !color.eq_ignore_ascii_case("#ffffff"))
        {
            return None;
        }
        if neutral
            && self
                .resolved_style_color(&StyleColor {
                    theme: Some("1".into()),
                    ..Default::default()
                })
                .is_none_or(|color| !color.eq_ignore_ascii_case("#000000"))
        {
            return None;
        }
        // The measured Dark11 subtotal tint differs from the shared conversion.
        if preset == Preset::Dark11
            && self
                .resolved_style_color(&theme(6, "0"))
                .is_none_or(|color| !color.eq_ignore_ascii_case("#196b24"))
        {
            return None;
        }
        let first = table.area.first();
        let data_row = first
            .row()
            .checked_add(table.location.get("firstDataRow")?.parse::<u32>().ok()?)?;
        let data_col = first
            .column()
            .checked_add(table.location.get("firstDataCol")?.parse::<u32>().ok()?)?;
        let header_offset = table.location.get("firstHeaderRow")?.parse::<u32>().ok()?;
        if column_layout
            && (header_offset != 1 || data_row - first.row() != 2 || data_col - first.column() != 1)
            || row_layout
                && (header_offset != 0
                    || data_row - first.row() != 1
                    || data_col - first.column() != 1)
        {
            return None;
        }
        if data_row <= first.row()
            || data_col <= first.column()
            || data_row > table.area.last().row()
            || data_col > table.area.last().column()
            || table.row_items.len() != (table.area.last().row() - data_row + 1) as usize
            || table.col_items.len() != (table.area.last().column() - data_col + 1) as usize
            || table
                .row_items
                .last()
                .is_none_or(|item| item.kind != "grand")
            || table.row_items[..table.row_items.len() - 1]
                .iter()
                .any(|item| item.kind != "data")
            || column_layout
                && (table
                    .col_items
                    .last()
                    .is_none_or(|item| item.kind != "grand")
                    || table.col_items[..table.col_items.len() - 1]
                        .iter()
                        .any(|item| item.kind != "data"))
            || row_layout
                && (table.col_items.len() != 2
                    || table.col_items.iter().enumerate().any(|(index, item)| {
                        item.kind != "data" || item.data_index as usize != index
                    })
                    || table.row_items[..table.row_items.len() - 1]
                        .iter()
                        .any(|item| {
                            (item.repeat as usize)
                                .checked_add(item.indexes.len())
                                .is_none_or(|depth| depth == 0 || depth > table.row_fields.len())
                        }))
        {
            return None;
        }
        let styles = self.styles();
        let xf = styles.cell_formats.get(self.cell_style(at) as usize)?;
        let parent = xf
            .xf_id
            .and_then(|id| styles.cell_style_formats.get(id as usize));
        let font = xf
            .font_id
            .or_else(|| parent.and_then(|p| p.font_id))
            .unwrap_or(0);
        let fill = xf
            .fill_id
            .or_else(|| parent.and_then(|p| p.fill_id))
            .unwrap_or(0);
        let border = xf
            .border_id
            .or_else(|| parent.and_then(|p| p.border_id))
            .unwrap_or(0);
        let mut look = PivotStyleLook::default();
        let row = at.row();
        let last = table.area.last();
        let is_header = row < data_row && header;
        let is_total = row == last.row();
        let subtotal_at = |row| {
            row_layout && (data_row..last.row()).contains(&row) && {
                let item = &table.row_items[(row - data_row) as usize];
                (item.repeat as usize)
                    .checked_add(item.indexes.len())
                    .is_some_and(|depth| depth < table.row_fields.len())
            }
        };
        let is_subtotal = subtotal_at(row);
        let muted = matches!(preset, Preset::Medium1 | Preset::Medium2);
        if font == 0
            && (is_total
                || is_header && (!muted || column_layout && at == first)
                || is_subtotal && !muted)
        {
            look.bold = Some(true);
        }
        if !light
            && font == 0
            && (is_header || preset == Preset::Medium1 && is_subtotal || dark && is_total)
        {
            look.font_color = Some(theme(0, "0"));
        }
        if fill == 0 {
            if preset == Preset::Light16 && (is_header || is_total) {
                look.fill = Some(theme(4, "0.7999816888943144"));
            } else if preset == Preset::Light21 && (is_header || is_total) {
                look.fill = Some(theme(9, "0.7999816888943144"));
            } else if preset == Preset::Light1 && is_total {
                look.fill = Some(theme(0, "0"));
            } else if preset == Preset::Light8 && is_subtotal {
                look.fill = Some(rgb("FFD9D9D9"));
            } else if preset == Preset::Dark1 {
                look.fill = Some(rgb(if is_header || is_total {
                    "FF808080"
                } else if is_subtotal {
                    "FFD9D9D9"
                } else {
                    "FFBFBFBF"
                }));
            } else if preset == Preset::Dark6 {
                look.fill = Some(theme(
                    8,
                    if is_header || is_total {
                        "-0.499984740745262"
                    } else if is_subtotal {
                        "0.7999816888943144"
                    } else {
                        "0.5999938962981048"
                    },
                ));
            } else if preset == Preset::Dark11 {
                look.fill = Some(if is_header || is_total {
                    rgb("FF404040")
                } else if is_subtotal {
                    rgb("FF83E28E")
                } else {
                    theme(6, "0.7999816888943144")
                });
            } else if preset == Preset::Medium1 && is_header {
                look.fill = Some(rgb("FF808080"));
            } else if preset == Preset::Medium1 && is_subtotal {
                look.fill = Some(rgb("FFA6A6A6"));
            } else if preset == Preset::Medium2 && is_header {
                look.fill = Some(theme(4, "-0.249977111117893"));
            } else if matches!(preset, Preset::Medium9 | Preset::Medium14) && is_header {
                look.fill = Some(theme(if preset == Preset::Medium14 { 9 } else { 4 }, "0"));
            } else if matches!(preset, Preset::Medium9 | Preset::Medium14) && is_subtotal {
                look.fill = Some(theme(
                    if preset == Preset::Medium14 { 9 } else { 4 },
                    "0.7999816888943144",
                ));
            } else if preset == Preset::Light16
                && at.row() >= data_row
                && at.row() < table.area.last().row()
            {
                let row_band = rows && (at.row() - data_row) % 2 == 0;
                let col_band = columns
                    && at.column() >= data_col
                    && at.column() < table.area.last().column()
                    && (at.column() - data_col) % 2 == 0;
                if row_band || col_band {
                    look.fill = Some(rgb("FFD9D9D9"));
                }
            }
        }
        if border == 0 {
            match preset {
                Preset::Light16 => {
                    let rule_below = |row| {
                        (header && row + 1 == data_row) || row + 1 == last.row() || subtotal_at(row)
                    };
                    for (side, upper_row) in [(1, row - 1), (2, row)] {
                        if rule_below(upper_row) {
                            look.borders[side] =
                                pivot_tint(self, 4, 40).map(|color| edge("thin", color));
                        }
                    }
                    if columns && row >= data_row {
                        let vertical = edge("thin", rgb("FFBFBFBF"));
                        if at.column() >= data_col {
                            look.borders[0] = Some(vertical.clone());
                        }
                        if at.column() < last.column() {
                            look.borders[3] = Some(vertical);
                        }
                    }
                }
                Preset::Dark6 => {
                    let rule_below = |upper_row| {
                        if !(first.row()..last.row()).contains(&upper_row) {
                            return None;
                        }
                        let tint = if upper_row + 1 < data_row {
                            -50
                        } else if upper_row + 1 == data_row {
                            0
                        } else if subtotal_at(upper_row) || subtotal_at(upper_row + 1) {
                            40
                        } else {
                            80
                        };
                        pivot_tint(self, 8, tint).map(|color| edge("thin", color))
                    };
                    look.borders[1] = rule_below(row - 1);
                    look.borders[2] = rule_below(row);
                }
                Preset::Dark11 => {
                    let outer = edge("medium", rgb("FF808080"));
                    for (side, on_perimeter) in [
                        at.column() == first.column(),
                        row == first.row(),
                        is_total,
                        at.column() == last.column(),
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        if on_perimeter {
                            look.borders[side] = Some(outer.clone());
                        }
                    }
                    if subtotal_at(row - 1) {
                        look.borders[1] =
                            pivot_tint(self, 6, 80).map(|color| edge("medium", color));
                    }
                    if is_subtotal {
                        look.borders[2] =
                            pivot_tint(self, 6, 80).map(|color| edge("medium", color));
                    }
                }
                _ => {}
            }
        }
        Some(look)
    }
}

#[cfg(test)]
mod tests {
    use super::display_number_code;

    /// Format constructs without captured spelling evidence must not acquire an inferred override.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_8wp_12__unmeasured_number_code_syntax_is_refused() {
        for code in [r"0*\-", r"0_)", r"0\a", "0\\", "[Red", "\"unfinished"] {
            assert!(display_number_code(code).is_none(), "{code}");
        }
    }
}
