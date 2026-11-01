//! Translate Format Cells through calc-host; previews use the grid's value formatter.
use crate::{cells, wire, workbook::Workbook};
use calc_host::value::{CellKey, GridArea};
use dna_calc_model::format::{BorderPatch, FormatPatch, FormatSelection};

pub fn host_patch(p: FormatPatch) -> calc_host::FormatPatch {
    calc_host::FormatPatch {
        number_format: p.number_format,
        horizontal: p.horizontal,
        vertical: p.vertical,
        wrap: p.wrap,
        rotation: p.rotation,
        indent: p.indent,
        font_name: p.font_name,
        font_size: p.font_size,
        bold: p.bold,
        italic: p.italic,
        underline: p.underline,
        strike: p.strike,
        font_color: p.font_color,
        fill: p.fill,
        borders: p.borders.map(|b| calc_host::BorderPatch {
            left: b.left,
            right: b.right,
            top: b.top,
            bottom: b.bottom,
            style: b.style,
            color: b.color,
        }),
    }
}
fn properties(host: &calc_host::HostState, at: CellKey) -> FormatPatch {
    let table = host.styles();
    let xf = table
        .cell_formats
        .get(host.cell_style(at) as usize)
        .cloned()
        .unwrap_or_default();
    let parent = xf
        .xf_id
        .and_then(|id| table.cell_style_formats.get(id as usize));
    let font = table
        .fonts
        .get(
            xf.font_id
                .or_else(|| parent.and_then(|p| p.font_id))
                .unwrap_or(0) as usize,
        )
        .cloned()
        .unwrap_or_default();
    let fill = table.fills.get(
        xf.fill_id
            .or_else(|| parent.and_then(|p| p.fill_id))
            .unwrap_or(0) as usize,
    );
    let border = table.borders.get(
        xf.border_id
            .or_else(|| parent.and_then(|p| p.border_id))
            .unwrap_or(0) as usize,
    );
    let a = host.cell_alignment(at).cloned().unwrap_or_default();
    let rotation = a
        .text_rotation
        .as_deref()
        .unwrap_or("0")
        .parse::<i16>()
        .ok()
        .and_then(|v| match v {
            0..=90 => Some(v),
            91..=180 => Some(90 - v),
            _ => None,
        });
    let rgb = |c: &calc_host::StyleColor| cells::style_color(host, c);
    let borders = border
        .and_then(|b| {
            let edges = [&b.left, &b.right, &b.top, &b.bottom];
            let first = edges
                .iter()
                .filter_map(|e| e.as_ref())
                .find(|e| e.style.is_some());
            let style = first.and_then(|e| e.style.clone()).unwrap_or("thin".into());
            let color = first
                .and_then(|e| e.color.as_ref())
                .and_then(rgb)
                .unwrap_or("#000000".into());
            edges
                .iter()
                .filter_map(|e| e.as_ref())
                .filter(|e| e.style.is_some())
                .all(|e| {
                    e.style.as_ref() == Some(&style)
                        && e.color.as_ref().and_then(rgb).unwrap_or("#000000".into()) == color
                })
                .then(|| BorderPatch {
                    left: b.left.as_ref().is_some_and(|e| e.style.is_some()),
                    right: b.right.as_ref().is_some_and(|e| e.style.is_some()),
                    top: b.top.as_ref().is_some_and(|e| e.style.is_some()),
                    bottom: b.bottom.as_ref().is_some_and(|e| e.style.is_some()),
                    style,
                    color,
                })
        })
        .or_else(|| {
            border.is_none().then(|| BorderPatch {
                style: "thin".into(),
                color: "#000000".into(),
                ..Default::default()
            })
        });
    FormatPatch {
        number_format: Some(host.number_format(at).unwrap_or("General").into()),
        horizontal: Some(a.horizontal.unwrap_or("general".into())),
        vertical: Some(a.vertical.unwrap_or("bottom".into())),
        wrap: Some(a.wrap_text),
        rotation,
        indent: a.indent.as_deref().unwrap_or("0").parse().ok(),
        font_name: Some(font.name.unwrap_or("Calibri".into())),
        font_size: Some(font.size.unwrap_or("11".into())),
        bold: Some(font.bold),
        italic: Some(font.italic),
        underline: Some(font.underline.as_deref().is_some_and(|v| v != "none")),
        strike: Some(font.strike),
        font_color: Some(font.color.as_ref().and_then(rgb).unwrap_or_default()),
        fill: match fill {
            None => Some(String::new()),
            Some(f) if matches!(f.pattern_type.as_deref(), Some("none") | None) => {
                Some(String::new())
            }
            Some(f) if f.pattern_type.as_deref() == Some("solid") => {
                f.fg_color.as_ref().and_then(rgb)
            }
            _ => None,
        },
        borders,
    }
}
pub fn selection(book: &Workbook, areas: Vec<GridArea>) -> Result<FormatSelection, String> {
    let keys = calc_host::format_cells(&areas)?;
    let first = *keys.first().ok_or("Select cells to format")?;
    if keys
        .iter()
        .any(|at| book.host().model().sheet_by_id(at.sheet()).is_none())
    {
        return Err("A selected sheet is unavailable".into());
    }
    let mut common = properties(book.host(), first);
    let mut styles = std::collections::BTreeSet::new();
    styles.insert(book.host().cell_style(first));
    for at in keys
        .iter()
        .skip(1)
        .filter(|at| styles.insert(book.host().cell_style(**at)))
    {
        let next = properties(book.host(), *at);
        macro_rules! intersect {($($field:ident),*)=>{$(if common.$field!=next.$field {common.$field=None;})*};}
        intersect!(
            number_format,
            horizontal,
            vertical,
            wrap,
            rotation,
            indent,
            font_name,
            font_size,
            bold,
            italic,
            underline,
            strike,
            font_color,
            fill,
            borders
        );
    }
    Ok(FormatSelection {
        revision: book.host().revision().get(),
        areas: areas.into_iter().map(wire::Area::of).collect(),
        common,
        sample: preview(book, first, FormatPatch::default())?,
    })
}
pub fn preview(book: &Workbook, at: CellKey, patch: FormatPatch) -> Result<wire::Cell, String> {
    let patch = host_patch(patch);
    patch.validate()?;
    let host = book.host();
    let cell = book.cell(at).ok_or("The preview cell is unavailable")?;
    let mut table = (**host.styles()).clone();
    let index = calc_host::patched_style(&mut table, host.cell_style(at), &patch)?;
    let original = book.shown(&cell);
    let mut shown = cells::shown(
        &cell,
        book.profile(),
        table
            .number_format_code(index)
            .map(|code| cells::CellFormat {
                code,
                date1904: host.declared_date1904(),
                palette: book.cell_format(at).and_then(|f| f.palette),
            }),
    );
    shown.conditional = None;
    shown.style = crate::autofit::text_style_for(host, at, original.style.width, &table, index);
    Ok(wire::cell_of(
        &shown,
        cells::input_text(cell.input()),
        "Format preview of the first selected cell".into(),
        &cell,
    ))
}
