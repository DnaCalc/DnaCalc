//! The styles part read into a [`StyleTable`]: number formats, fonts, fills, borders, cell style
//! formats, cell formats, cell styles, differential formats, table styles and the colour tables.
//! An element without a place in the table is named in [`StyleTable::unmodelled`]; the part's
//! bytes stay in the package as stored.

use super::{Events, LoadError, MAIN_NAMESPACE, read_xml};
use crate::opc::Package;
use crate::xml::{XmlElement, XmlEvent};
use calc_doc_model::{
    Alignment, Border, BorderEdge, CellFormat, CellStyle, DifferentialFormat, Fill, Font,
    NumberFormat, Protection, StyleColor, StyleTable, TableStyles,
};

fn text(element: &XmlElement, name: &str) -> Option<String> {
    element.attribute(name).map(str::to_owned)
}

fn number(element: &XmlElement, name: &str) -> Option<u32> {
    element
        .attribute(name)
        .and_then(|value| value.trim().parse().ok())
}

fn flag(element: &XmlElement, name: &str) -> Option<bool> {
    element
        .attribute(name)
        .map(|value| matches!(value.trim(), "1" | "true" | "TRUE" | "True"))
}

fn color(element: &XmlElement) -> StyleColor {
    StyleColor {
        rgb: text(element, "rgb"),
        indexed: text(element, "indexed"),
        theme: text(element, "theme"),
        tint: text(element, "tint"),
        auto: flag(element, "auto").unwrap_or(false),
    }
}

fn number_format(element: &XmlElement) -> Option<NumberFormat> {
    Some(NumberFormat {
        id: number(element, "numFmtId")?,
        code: text(element, "formatCode").unwrap_or_default(),
    })
}

fn font_child(font: &mut Font, element: &XmlElement) {
    let on = || flag(element, "val").unwrap_or(true);
    match element.name.local.as_str() {
        "b" => font.bold = on(),
        "i" => font.italic = on(),
        "strike" => font.strike = on(),
        "u" => font.underline = Some(text(element, "val").unwrap_or_else(|| "single".to_owned())),
        "sz" => font.size = text(element, "val"),
        "name" => font.name = text(element, "val"),
        "family" => font.family = text(element, "val"),
        "scheme" => font.scheme = text(element, "val"),
        "color" => font.color = Some(color(element)),
        other => font.other_children.push(other.to_owned()),
    }
}

fn fill_child(fill: &mut Fill, element: &XmlElement) {
    match element.name.local.as_str() {
        "patternFill" => fill.pattern_type = text(element, "patternType"),
        "fgColor" => fill.fg_color = Some(color(element)),
        "bgColor" => fill.bg_color = Some(color(element)),
        other => fill.other_children.push(other.to_owned()),
    }
}

fn border_start(element: &XmlElement) -> Border {
    Border {
        diagonal_up: flag(element, "diagonalUp").unwrap_or(false),
        diagonal_down: flag(element, "diagonalDown").unwrap_or(false),
        ..Border::default()
    }
}

fn edge<'b>(border: &'b mut Border, name: &str) -> Option<&'b mut Option<BorderEdge>> {
    Some(match name {
        "left" | "start" => &mut border.left,
        "right" | "end" => &mut border.right,
        "top" => &mut border.top,
        "bottom" => &mut border.bottom,
        "diagonal" => &mut border.diagonal,
        _ => return None,
    })
}

/// A child of a border at `depth` below it (1 for an edge, 2 for an edge's colour).
fn border_child(border: &mut Border, edge_name: &mut Option<String>, element: &XmlElement) {
    let local = element.name.local.as_str();
    if let Some(slot) = edge(border, local) {
        *slot = Some(BorderEdge {
            style: text(element, "style"),
            color: None,
        });
        *edge_name = Some(local.to_owned());
    } else if local == "color" {
        if let Some(name) = edge_name.as_deref()
            && let Some(Some(slot)) = edge(border, name)
        {
            slot.color = Some(color(element));
        }
    } else {
        border.other_children.push(local.to_owned());
    }
}

fn cell_format(element: &XmlElement) -> CellFormat {
    let on = |name| flag(element, name).unwrap_or(false);
    CellFormat {
        number_format_id: number(element, "numFmtId"),
        font_id: number(element, "fontId"),
        fill_id: number(element, "fillId"),
        border_id: number(element, "borderId"),
        xf_id: number(element, "xfId"),
        quote_prefix: on("quotePrefix"),
        pivot_button: on("pivotButton"),
        apply_number_format: on("applyNumberFormat"),
        apply_font: on("applyFont"),
        apply_fill: on("applyFill"),
        apply_border: on("applyBorder"),
        apply_alignment: on("applyAlignment"),
        apply_protection: on("applyProtection"),
        alignment: None,
        protection: None,
    }
}

fn alignment(element: &XmlElement) -> Alignment {
    Alignment {
        horizontal: text(element, "horizontal"),
        vertical: text(element, "vertical"),
        text_rotation: text(element, "textRotation"),
        wrap_text: flag(element, "wrapText").unwrap_or(false),
        shrink_to_fit: flag(element, "shrinkToFit").unwrap_or(false),
        indent: text(element, "indent"),
    }
}

fn protection(element: &XmlElement) -> Protection {
    Protection {
        locked: flag(element, "locked"),
        hidden: flag(element, "hidden"),
    }
}

/// What the element being read belongs to.
enum Open {
    Font(Font),
    Fill(Fill),
    Border(Border, Option<String>),
    /// A cell format, and whether it is of `cellXfs` (not `cellStyleXfs`).
    Format(CellFormat, bool),
    Differential(DifferentialFormat),
    /// A font, fill or border of the differential format open below it.
    DifferentialFont(Font),
    DifferentialFill(Fill),
    DifferentialBorder(Border, Option<String>),
}

/// The differential format open nearest the end of `open`.
fn differential(open: &mut [Option<Open>]) -> Option<&mut DifferentialFormat> {
    open.iter_mut().rev().flatten().find_map(|item| match item {
        Open::Differential(format) => Some(format),
        _ => None,
    })
}

/// Reads the styles part `part`.
pub(crate) fn read_styles(package: &Package, part: &str) -> Result<StyleTable, LoadError> {
    let xml = read_xml(package, part)?;
    let mut events = Events::new(&xml);
    events.root(
        MAIN_NAMESPACE,
        "styleSheet",
        "the document element is not a SpreadsheetML styleSheet",
    )?;
    let mut table = StyleTable::default();
    // The local names of the open elements below the document element.
    let mut path: Vec<String> = Vec::new();
    // What the element at each depth of `path` opened, when it opened something.
    let mut open: Vec<Option<Open>> = Vec::new();
    loop {
        let event = match events.next()? {
            Some(event) => event,
            None => return Err(events.invalid("the part ends inside styleSheet")),
        };
        match event {
            XmlEvent::Text(_) => {}
            XmlEvent::End(_) if path.is_empty() => break,
            XmlEvent::End(_) => {
                path.pop();
                let Some(Some(finished)) = open.pop() else {
                    continue;
                };
                match finished {
                    Open::Font(font) => table.fonts.push(font),
                    Open::Fill(fill) => table.fills.push(fill),
                    Open::Border(border, _) => table.borders.push(border),
                    Open::Format(format, true) => table.cell_formats.push(format),
                    Open::Format(format, false) => table.cell_style_formats.push(format),
                    Open::Differential(format) => table.differential_formats.push(format),
                    Open::DifferentialFont(font) => {
                        if let Some(format) = differential(&mut open) {
                            format.font = Some(font);
                        }
                    }
                    Open::DifferentialFill(fill) => {
                        if let Some(format) = differential(&mut open) {
                            format.fill = Some(fill);
                        }
                    }
                    Open::DifferentialBorder(border, _) => {
                        if let Some(format) = differential(&mut open) {
                            format.border = Some(border);
                        }
                    }
                }
            }
            XmlEvent::Start(element) => {
                let local = String::from(element.name.local.clone());
                let parent = path.last().map(String::as_str);
                // The innermost open item, which a child element adds to.
                let current = open.iter_mut().rev().flatten().next();
                let mut opened = None;
                match (parent, local.as_str(), current) {
                    (Some("numFmts"), "numFmt", _) => {
                        table.number_formats.extend(number_format(&element));
                    }
                    (Some("fonts"), "font", _) => opened = Some(Open::Font(Font::default())),
                    (Some("fills"), "fill", _) => opened = Some(Open::Fill(Fill::default())),
                    (Some("borders"), "border", _) => {
                        opened = Some(Open::Border(border_start(&element), None));
                    }
                    (Some("cellXfs"), "xf", _) => {
                        opened = Some(Open::Format(cell_format(&element), true));
                    }
                    (Some("cellStyleXfs"), "xf", _) => {
                        opened = Some(Open::Format(cell_format(&element), false));
                    }
                    (Some("cellStyles"), "cellStyle", _) => table.cell_styles.push(CellStyle {
                        name: text(&element, "name").unwrap_or_default(),
                        xf_id: number(&element, "xfId").unwrap_or(0),
                        builtin_id: number(&element, "builtinId"),
                        custom_builtin: flag(&element, "customBuiltin").unwrap_or(false),
                        hidden: flag(&element, "hidden").unwrap_or(false),
                    }),
                    (Some("dxfs"), "dxf", _) => {
                        opened = Some(Open::Differential(DifferentialFormat::default()));
                    }
                    (None, "tableStyles", _) => {
                        table.table_styles = Some(TableStyles {
                            count: number(&element, "count"),
                            default_table_style: text(&element, "defaultTableStyle"),
                            default_pivot_style: text(&element, "defaultPivotStyle"),
                        });
                    }
                    (Some("indexedColors"), "rgbColor", _) => {
                        table.colors.indexed.push(color(&element));
                    }
                    (Some("mruColors"), "color", _) => {
                        table.colors.most_recently_used.push(color(&element));
                    }
                    (
                        None,
                        "numFmts" | "fonts" | "fills" | "borders" | "cellStyleXfs" | "cellXfs"
                        | "cellStyles" | "dxfs" | "colors",
                        _,
                    ) => {}
                    (Some("colors"), "indexedColors" | "mruColors", _) => {}
                    (_, _, Some(Open::Font(font))) => font_child(font, &element),
                    (_, _, Some(Open::Fill(fill))) => fill_child(fill, &element),
                    (_, _, Some(Open::Border(border, edge_name))) => {
                        border_child(border, edge_name, &element);
                    }
                    (_, "alignment", Some(Open::Format(format, _))) => {
                        format.alignment = Some(alignment(&element));
                    }
                    (_, "protection", Some(Open::Format(format, _))) => {
                        format.protection = Some(protection(&element));
                    }
                    (_, _, Some(Open::Differential(format))) => match local.as_str() {
                        "numFmt" => format.number_format = number_format(&element),
                        "alignment" => format.alignment = Some(alignment(&element)),
                        "protection" => format.protection = Some(protection(&element)),
                        "font" => opened = Some(Open::DifferentialFont(Font::default())),
                        "fill" => opened = Some(Open::DifferentialFill(Fill::default())),
                        "border" => {
                            opened = Some(Open::DifferentialBorder(border_start(&element), None));
                        }
                        _ => table.unmodelled.push(path_of(&path, &local)),
                    },
                    (_, _, Some(Open::DifferentialFont(font))) => font_child(font, &element),
                    (_, _, Some(Open::DifferentialFill(fill))) => fill_child(fill, &element),
                    (_, _, Some(Open::DifferentialBorder(border, edge_name))) => {
                        border_child(border, edge_name, &element);
                    }
                    _ => table.unmodelled.push(path_of(&path, &local)),
                }
                path.push(local);
                open.push(opened);
            }
        }
    }
    events.end()?;
    Ok(table)
}

fn path_of(path: &[String], local: &str) -> String {
    let mut out = String::from("styleSheet");
    for name in path.iter().map(String::as_str).chain([local]) {
        out.push('/');
        out.push_str(name);
    }
    out
}
