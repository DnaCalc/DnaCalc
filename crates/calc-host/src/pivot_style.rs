//! Measured Light16 and Medium9 layers for the one-row, one-column worksheet layout.
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

fn theme(index: u32, tint: &str) -> StyleColor {
    StyleColor { theme: Some(index.to_string()), tint: Some(tint.into()), ..Default::default() }
}
fn rgb(color: &str) -> StyleColor {
    StyleColor { rgb: Some(color.into()), ..Default::default() }
}
fn edge(color: StyleColor) -> BorderEdge {
    BorderEdge { style: Some("thin".into()), color: Some(color) }
}
fn flag(style: &calc_doc_model::pivot::PivotStyle, name: &str) -> Option<bool> {
    match style.attributes.get(name).map(String::as_str)? {
        "1" | "true" => Some(true),
        "0" | "false" => Some(false),
        _ => None,
    }
}

impl HostState {
    /// The supported preset's style layer for this cell. Unsupported shapes and area formats
    /// leave the stored appearance alone until their precedence is modelled.
    pub fn pivot_style_look(&self, at: CellKey) -> Option<PivotStyleLook> {
        let state = self.model().definitions().pivots();
        let mut tables = state.sheet(at.sheet())?.tables.iter().filter(|table| table.area.contains(at));
        let table = tables.next()?;
        if tables.next().is_some() || table.row_fields.len() != 1 || table.column_fields.len() != 1
            || table.row_fields[0] < 0 || table.column_fields[0] < 0 || table.data_fields.len() != 1
            || !table.page_fields.is_empty() || !table.filters.is_empty() || !table.formats.is_empty()
            || !table.unsupported.is_empty() { return None; }
        let style = table.style.as_ref()?;
        let light = match style.attributes.get("name")?.as_str() {
            "PivotStyleLight16" => true,
            "PivotStyleMedium9" => false,
            _ => return None,
        };
        let header = flag(style, "showColHeaders")?;
        let _row_headers = flag(style, "showRowHeaders")?;
        let rows = flag(style, "showRowStripes")?;
        let columns = flag(style, "showColStripes")?;
        let _last_column = flag(style, "showLastColumn")?;
        if rows && columns { return None; }
        // The captured neutral stripe colours are established for a white light-background
        // theme. A differently coloured background needs its own measured tint behaviour.
        if light && (rows || columns) && self.resolved_style_color(&StyleColor {
            theme: Some("0".into()), ..Default::default()
        }).is_none_or(|color| !color.eq_ignore_ascii_case("#ffffff")) { return None; }
        let first = table.area.first();
        let data_row = first.row().checked_add(table.location.get("firstDataRow")?.parse::<u32>().ok()?)?;
        let data_col = first.column().checked_add(table.location.get("firstDataCol")?.parse::<u32>().ok()?)?;
        if data_row <= first.row() || data_col <= first.column() || data_row > table.area.last().row()
            || data_col > table.area.last().column()
            || table.row_items.len() != (table.area.last().row() - data_row + 1) as usize
            || table.col_items.len() != (table.area.last().column() - data_col + 1) as usize
            || table.row_items.last().is_none_or(|item| item.kind != "grand")
            || table.col_items.last().is_none_or(|item| item.kind != "grand")
            || table.row_items[..table.row_items.len() - 1].iter().any(|item| item.kind != "data")
            || table.col_items[..table.col_items.len() - 1].iter().any(|item| item.kind != "data") {
            return None;
        }
        let styles = self.styles();
        let xf = styles.cell_formats.get(self.cell_style(at) as usize)?;
        let parent = xf.xf_id.and_then(|id| styles.cell_style_formats.get(id as usize));
        let font = xf.font_id.or_else(|| parent.and_then(|p| p.font_id)).unwrap_or(0);
        let fill = xf.fill_id.or_else(|| parent.and_then(|p| p.fill_id)).unwrap_or(0);
        let border = xf.border_id.or_else(|| parent.and_then(|p| p.border_id)).unwrap_or(0);
        let mut look = PivotStyleLook::default();
        let is_header = at.row() < data_row && header;
        let is_total = at.row() == table.area.last().row();
        if font == 0 && (is_header || is_total) { look.bold = Some(true); }
        if !light && font == 0 && is_header { look.font_color = Some(theme(0, "0")); }
        if fill == 0 {
            if light && (is_header || is_total) { look.fill = Some(theme(4, "0.7999816888943144")); }
            else if !light && is_header { look.fill = Some(theme(4, "0")); }
            else if light && at.row() >= data_row && at.row() < table.area.last().row() {
                let row_band = rows && (at.row() - data_row) % 2 == 0;
                let col_band = columns && at.column() >= data_col && at.column() < table.area.last().column()
                    && (at.column() - data_col) % 2 == 0;
                if row_band || col_band { look.fill = Some(rgb("FFD9D9D9")); }
            }
        }
        if light && border == 0 {
            // The horizontal theme edge needs the preset's exact tint conversion; leave it
            // unresolved until that conversion reproduces the captured DisplayFormat colour.
            if columns && at.row() >= data_row {
                let vertical = edge(rgb("FFBFBFBF"));
                if at.column() >= data_col { look.borders[0] = Some(vertical.clone()); }
                if at.column() < table.area.last().column() { look.borders[3] = Some(vertical); }
            }
        }
        Some(look)
    }
}
