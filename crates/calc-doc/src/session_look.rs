//! The look a formula session keeps in its file (dc-0f1k.20): ordinary Excel cell formats on the
//! formula's cell and the cells its result spills into, and conditional formats over the same
//! cells. No custom XML: Excel opens the file and shows the same formats and rules.
//!
//! [`SessionLook`] is plain data. [`SessionFormat`] becomes one cell format (`xf`) with its font,
//! fill and borders; each [`SessionRule`] becomes one `cfRule` with its `dxf`. The rule formulas
//! are the file's: relative to the top-left cell of the look's area, as `cfRule` stores them (the
//! application translates from its own origin). Colours are `#rrggbb` text. What the standard
//! `cfRule` cannot hold (a data bar without a gradient is an Excel 2010 extension) is refused with
//! a reason, never dropped.

use crate::session::SessionRefusal;
use crate::{ColorScale, ConditionalRule, DataBar, IconSet, Threshold};
use calc_doc_model::value::{CellKey, GridArea};
use calc_doc_model::{
    Alignment, Border, BorderEdge, CellFormat, ConditionalRuleSource, DifferentialFormat, Fill,
    Font, NumberFormat, StyleColor, StyleRun, StyleTable,
};
use std::sync::Arc;

/// The cell format a session's result carries. An empty text, a zero size and a false flag leave
/// the default, as the Formatting panel does.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionFormat {
    pub font_family: String,
    /// Points; 0 is the default.
    pub font_size: u32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    /// `#rrggbb` or empty.
    pub text_color: String,
    pub fill: String,
    /// `left`, `center`, `right` or empty (general).
    pub horizontal: String,
    pub wrap: bool,
    pub border_top: bool,
    pub border_bottom: bool,
    pub border_left: bool,
    pub border_right: bool,
    pub border_color: String,
}

impl SessionFormat {
    /// Whether the format changes nothing.
    pub fn is_default(&self) -> bool {
        *self == SessionFormat::default()
    }
}

/// A rule's differential format (`dxf`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionDxf {
    pub fill: Option<String>,
    pub font_color: Option<String>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub number_format: Option<String>,
}

impl SessionDxf {
    fn is_empty(&self) -> bool {
        *self == SessionDxf::default()
    }
}

/// One conditional-format rule and the differential format it applies.
#[derive(Clone, Debug, PartialEq)]
pub struct SessionRule {
    /// The rule as `cfRule` holds it. Its `dxf` index is the file's and is not used here.
    pub rule: ConditionalRule,
    pub dxf: Option<SessionDxf>,
}

/// The formats and rules over the cells `rows` x `columns` from (`row`, `column`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SessionLook {
    pub row: u32,
    pub column: u32,
    pub rows: u32,
    pub columns: u32,
    pub format: Option<SessionFormat>,
    pub rules: Vec<SessionRule>,
}

fn write_refused(detail: impl std::fmt::Display) -> SessionRefusal {
    SessionRefusal::Write {
        detail: detail.to_string(),
    }
}

fn rgb(text: &str) -> Option<StyleColor> {
    let hex = text.strip_prefix('#')?;
    (hex.len() == 6 && hex.bytes().all(|b| b.is_ascii_hexdigit())).then(|| StyleColor {
        rgb: Some(format!("FF{}", hex.to_ascii_uppercase())),
        ..Default::default()
    })
}

/// `#rrggbb` of an `rgb` colour (`AARRGGBB` or `RRGGBB`); a theme or indexed colour has none here.
fn hex_of(color: &StyleColor) -> Option<String> {
    let value = color.rgb.as_deref()?;
    let hex = match value.len() {
        8 => &value[2..],
        6 => value,
        _ => return None,
    };
    hex.bytes()
        .all(|b| b.is_ascii_hexdigit())
        .then(|| format!("#{}", hex.to_ascii_lowercase()))
}

fn escape(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
    out
}

fn threshold_xml(out: &mut String, threshold: &Threshold) {
    out.push_str(&format!("<cfvo type=\"{}\"", escape(&threshold.kind)));
    if let Some(value) = &threshold.value {
        out.push_str(&format!(" val=\"{}\"", escape(value)));
    }
    if !threshold.gte {
        out.push_str(" gte=\"0\"");
    }
    out.push_str("/>");
}

fn color_xml(out: &mut String, color: &StyleColor) -> Result<(), SessionRefusal> {
    let value = color
        .rgb
        .as_deref()
        .ok_or_else(|| write_refused("a rule colour must be an RGB colour"))?;
    out.push_str(&format!("<color rgb=\"{}\"/>", escape(value)));
    Ok(())
}

/// The worksheet fragment holding one rule over `sqref`.
fn rule_source(
    sqref: &str,
    rule: &ConditionalRule,
    dxf: Option<u32>,
    priority: i64,
) -> Result<String, SessionRefusal> {
    if rule.unread {
        return Err(write_refused(
            "a rule with fields this version does not write",
        ));
    }
    let mut attributes = format!(" type=\"{}\"", escape(&rule.kind));
    if let Some(id) = dxf {
        attributes.push_str(&format!(" dxfId=\"{id}\""));
    }
    attributes.push_str(&format!(" priority=\"{priority}\""));
    if rule.stop_if_true {
        attributes.push_str(" stopIfTrue=\"1\"");
    }
    if let Some(operator) = &rule.operator {
        attributes.push_str(&format!(" operator=\"{}\"", escape(operator)));
    }
    if let Some(text) = &rule.text {
        attributes.push_str(&format!(" text=\"{}\"", escape(text)));
    }
    if let Some(period) = &rule.time_period {
        attributes.push_str(&format!(" timePeriod=\"{}\"", escape(period)));
    }
    if rule.kind == "top10" {
        attributes.push_str(&format!(
            " rank=\"{}\" percent=\"{}\" bottom=\"{}\"",
            rule.rank.unwrap_or(10),
            u8::from(rule.percent),
            u8::from(rule.bottom)
        ));
    }
    if rule.kind == "aboveAverage" {
        attributes.push_str(&format!(
            " aboveAverage=\"{}\"",
            u8::from(rule.above_average)
        ));
        if rule.equal_average {
            attributes.push_str(" equalAverage=\"1\"");
        }
    }
    let mut body = String::new();
    for formula in &rule.formulas {
        body.push_str(&format!("<formula>{}</formula>", escape(formula)));
    }
    if let Some(ColorScale { thresholds, colors }) = &rule.color_scale {
        body.push_str("<colorScale>");
        thresholds.iter().for_each(|t| threshold_xml(&mut body, t));
        for color in colors {
            color_xml(&mut body, color)?;
        }
        body.push_str("</colorScale>");
    }
    if let Some(DataBar {
        thresholds,
        color,
        min_length,
        max_length,
        gradient,
        show_value,
    }) = &rule.data_bar
    {
        if !gradient {
            return Err(write_refused(
                "a data bar without a gradient is an Excel 2010 extension that this version does not write; use the gradient bar",
            ));
        }
        body.push_str(&format!(
            "<dataBar minLength=\"{min_length}\" maxLength=\"{max_length}\"{}>",
            if *show_value { "" } else { " showValue=\"0\"" }
        ));
        thresholds.iter().for_each(|t| threshold_xml(&mut body, t));
        color_xml(&mut body, color)?;
        body.push_str("</dataBar>");
    }
    if let Some(IconSet {
        name,
        thresholds,
        reverse,
        show_value,
    }) = &rule.icon_set
    {
        body.push_str(&format!(
            "<iconSet iconSet=\"{}\"{}{}>",
            escape(name),
            if *reverse { " reverse=\"1\"" } else { "" },
            if *show_value { "" } else { " showValue=\"0\"" }
        ));
        thresholds.iter().for_each(|t| threshold_xml(&mut body, t));
        body.push_str("</iconSet>");
    }
    Ok(format!(
        "<worksheet xmlns=\"{ns}\"><conditionalFormatting sqref=\"{sqref}\"><cfRule{attributes}>{body}</cfRule></conditionalFormatting></worksheet>",
        ns = crate::load::MAIN_NAMESPACE,
    ))
}

fn a1(row: u32, column: u32) -> String {
    format!("{}{row}", crate::load::address::column_letters(column))
}

fn dxf_of(
    dxf: &SessionDxf,
    existing: &[DifferentialFormat],
) -> Result<DifferentialFormat, SessionRefusal> {
    let color = |text: &str, what: &str| {
        rgb(text).ok_or_else(|| write_refused(format!("the {what} colour {text:?} is not #rrggbb")))
    };
    let mut out = DifferentialFormat::default();
    if dxf.bold || dxf.italic || dxf.underline || dxf.strike || dxf.font_color.is_some() {
        out.font = Some(Font {
            bold: dxf.bold,
            italic: dxf.italic,
            underline: dxf.underline.then(|| "single".to_owned()),
            strike: dxf.strike,
            color: dxf
                .font_color
                .as_deref()
                .map(|c| color(c, "font"))
                .transpose()?,
            ..Default::default()
        });
    }
    if let Some(fill) = &dxf.fill {
        // Excel stores a conditional fill's colour as the pattern's background.
        out.fill = Some(Fill {
            bg_color: Some(color(fill, "fill")?),
            ..Default::default()
        });
    }
    if let Some(code) = &dxf.number_format {
        // A differential format names its own number format; ids are the file's custom ones.
        let known = existing.iter().filter_map(|d| d.number_format.as_ref());
        let id = known
            .clone()
            .find(|f| &f.code == code)
            .map(|f| f.id)
            .unwrap_or_else(|| known.map(|f| f.id).max().map_or(164, |m| m + 1).max(164));
        out.number_format = Some(NumberFormat {
            id,
            code: code.clone(),
        });
    }
    Ok(out)
}

/// The cell format `format` as records added to `table`: the index of the new cell format.
fn add_format(table: &mut StyleTable, format: &SessionFormat) -> Result<u32, SessionRefusal> {
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
    let color = |text: &str, what: &str| -> Result<Option<StyleColor>, SessionRefusal> {
        if text.is_empty() {
            Ok(None)
        } else {
            rgb(text)
                .map(Some)
                .ok_or_else(|| write_refused(format!("the {what} colour {text:?} is not #rrggbb")))
        }
    };
    let mut xf = CellFormat {
        xf_id: Some(0),
        ..Default::default()
    };
    let base = table.fonts[0].clone();
    let mut font = base.clone();
    if !format.font_family.is_empty() {
        font.name = Some(format.font_family.clone());
        // A named font is not the theme's.
        font.scheme = None;
        font.family = None;
    }
    if format.font_size > 0 {
        font.size = Some(format.font_size.to_string());
    }
    font.bold = format.bold;
    font.italic = format.italic;
    font.underline = format.underline.then(|| "single".to_owned());
    font.strike = format.strike;
    if let Some(c) = color(&format.text_color, "text")? {
        font.color = Some(c);
    }
    xf.font_id = Some(0);
    if font != base {
        table.fonts.push(font);
        xf.font_id = Some(table.fonts.len() as u32 - 1);
        xf.apply_font = true;
    }
    xf.fill_id = Some(0);
    if let Some(c) = color(&format.fill, "fill")? {
        table.fills.push(Fill {
            pattern_type: Some("solid".into()),
            fg_color: Some(c),
            bg_color: Some(StyleColor {
                indexed: Some("64".into()),
                ..Default::default()
            }),
            ..Default::default()
        });
        xf.fill_id = Some(table.fills.len() as u32 - 1);
        xf.apply_fill = true;
    }
    xf.border_id = Some(0);
    if format.border_top || format.border_bottom || format.border_left || format.border_right {
        let edge_color = color(&format.border_color, "border")?.unwrap_or_else(|| StyleColor {
            rgb: Some("FF000000".into()),
            ..Default::default()
        });
        let edge = |on: bool| {
            on.then(|| BorderEdge {
                style: Some("thin".into()),
                color: Some(edge_color.clone()),
            })
        };
        table.borders.push(Border {
            left: edge(format.border_left),
            right: edge(format.border_right),
            top: edge(format.border_top),
            bottom: edge(format.border_bottom),
            ..Default::default()
        });
        xf.border_id = Some(table.borders.len() as u32 - 1);
        xf.apply_border = true;
    }
    if !format.horizontal.is_empty() || format.wrap {
        xf.alignment = Some(Alignment {
            horizontal: (!format.horizontal.is_empty()).then(|| format.horizontal.clone()),
            wrap_text: format.wrap,
            ..Default::default()
        });
        xf.apply_alignment = true;
    }
    table.cell_formats.push(xf);
    Ok(table.cell_formats.len() as u32 - 1)
}

/// The operations that give the session's cells their look, over `table` (the blank workbook's
/// styles) and the sheet.
pub(crate) fn look_operations(
    look: &SessionLook,
    table: &StyleTable,
    sheet: calc_doc_model::value::SheetId,
) -> Result<Vec<calc_doc_model::WorkbookOp>, SessionRefusal> {
    use calc_doc_model::WorkbookOp;
    let cell = |row: u32, column: u32| {
        CellKey::new(sheet, row, column)
            .map_err(|_| write_refused("the look's cells are outside the sheet"))
    };
    if look.rows == 0 || look.columns == 0 {
        return Err(write_refused("the look covers no cells"));
    }
    let area = GridArea::new(
        cell(look.row, look.column)?,
        cell(look.row + look.rows - 1, look.column + look.columns - 1)?,
    )
    .map_err(|_| write_refused("the look's cells are outside the sheet"))?;
    let mut table = table.clone();
    let mut operations = Vec::new();
    if let Some(format) = look.format.as_ref().filter(|f| !f.is_default()) {
        let index = add_format(&mut table, format)?;
        table.runs.assign(StyleRun {
            area,
            format: index,
        });
    }
    let mut sources = Vec::new();
    let sqref = if look.rows * look.columns == 1 {
        a1(look.row, look.column)
    } else {
        format!(
            "{}:{}",
            a1(look.row, look.column),
            a1(look.row + look.rows - 1, look.column + look.columns - 1)
        )
    };
    for (index, rule) in look.rules.iter().enumerate() {
        let dxf = match rule.dxf.as_ref().filter(|d| !d.is_empty()) {
            Some(dxf) => {
                let converted = dxf_of(dxf, &table.differential_formats)?;
                let id = table
                    .differential_formats
                    .iter()
                    .position(|d| *d == converted)
                    .unwrap_or_else(|| {
                        table.differential_formats.push(converted);
                        table.differential_formats.len() - 1
                    });
                Some(id as u32)
            }
            None => None,
        };
        let priority = if rule.rule.priority > 0 {
            rule.rule.priority
        } else {
            index as i64 + 1
        };
        let source = rule_source(&sqref, &rule.rule, dxf, priority)?;
        sources.push(ConditionalRuleSource {
            source: source.into_bytes().into(),
            areas: vec![area],
            priority,
            stop: rule.rule.stop_if_true,
            formulas: rule.rule.formulas.clone(),
            threshold_formulas: threshold_formulas(&rule.rule),
            authored: true,
        });
    }
    if table != StyleTable::default() {
        operations.push(WorkbookOp::SetStyles(Arc::new(table)));
    }
    if !sources.is_empty() {
        operations.push(WorkbookOp::SetConditionalRules {
            sheet,
            rules: sources,
        });
    }
    Ok(operations)
}

/// The formula thresholds of a rule in file order (colour scale, data bar, icon set).
fn threshold_formulas(rule: &ConditionalRule) -> Vec<String> {
    let mut out = Vec::new();
    let mut take = |thresholds: &[Threshold]| {
        for t in thresholds {
            if t.kind == "formula" {
                if let Some(v) = &t.value {
                    out.push(v.clone());
                }
            }
        }
    };
    if let Some(s) = &rule.color_scale {
        take(&s.thresholds);
    }
    if let Some(b) = &rule.data_bar {
        take(&b.thresholds);
    }
    if let Some(i) = &rule.icon_set {
        take(&i.thresholds);
    }
    out
}

/// The format of the cell format `index`, for a file's formatted cell.
pub(crate) fn read_format(table: &StyleTable, index: u32) -> Option<SessionFormat> {
    let xf = table.cell_formats.get(index as usize)?;
    let mut format = SessionFormat::default();
    let default_font = table.fonts.first();
    if let Some(font) = xf.font_id.and_then(|id| table.fonts.get(id as usize)) {
        if Some(font) != default_font {
            if font.name.as_ref() != default_font.and_then(|f| f.name.as_ref()) {
                format.font_family = font.name.clone().unwrap_or_default();
            }
            if font.size.as_ref() != default_font.and_then(|f| f.size.as_ref()) {
                format.font_size = font
                    .size
                    .as_deref()
                    .and_then(|s| s.parse::<f64>().ok())
                    .map_or(0, |s| s.round() as u32);
            }
            format.bold = font.bold;
            format.italic = font.italic;
            format.underline = font.underline.as_deref().is_some_and(|u| u != "none");
            format.strike = font.strike;
            if font.color != default_font.and_then(|f| f.color.clone()) {
                format.text_color = font.color.as_ref().and_then(hex_of).unwrap_or_default();
            }
        }
    }
    if let Some(fill) = xf.fill_id.and_then(|id| table.fills.get(id as usize)) {
        if fill.pattern_type.as_deref() == Some("solid") {
            format.fill = fill.fg_color.as_ref().and_then(hex_of).unwrap_or_default();
        }
    }
    if let Some(border) = xf.border_id.and_then(|id| table.borders.get(id as usize)) {
        let on = |edge: &Option<BorderEdge>| {
            edge.as_ref()
                .is_some_and(|e| e.style.as_deref().is_some_and(|s| s != "none"))
        };
        format.border_top = on(&border.top);
        format.border_bottom = on(&border.bottom);
        format.border_left = on(&border.left);
        format.border_right = on(&border.right);
        format.border_color = [&border.top, &border.bottom, &border.left, &border.right]
            .into_iter()
            .filter(|e| on(e))
            .find_map(|e| e.as_ref().and_then(|e| e.color.as_ref()).and_then(hex_of))
            .unwrap_or_default();
    }
    if let Some(alignment) = &xf.alignment {
        format.horizontal = alignment
            .horizontal
            .as_deref()
            .filter(|h| matches!(*h, "left" | "center" | "right"))
            .unwrap_or_default()
            .to_owned();
        format.wrap = alignment.wrap_text;
    }
    (!format.is_default()).then_some(format)
}

/// The differential format of a rule, as the session holds one.
pub(crate) fn read_dxf(table: &StyleTable, index: u32) -> Option<SessionDxf> {
    let d = table.differential_formats.get(index as usize)?;
    let dxf = SessionDxf {
        fill: d.fill.as_ref().and_then(|fill| {
            let color = match fill.pattern_type.as_deref() {
                Some("solid") => fill.fg_color.as_ref().or(fill.bg_color.as_ref()),
                _ => fill.bg_color.as_ref().or(fill.fg_color.as_ref()),
            }?;
            hex_of(color)
        }),
        font_color: d
            .font
            .as_ref()
            .and_then(|f| f.color.as_ref())
            .and_then(hex_of),
        bold: d.font.as_ref().is_some_and(|f| f.bold),
        italic: d.font.as_ref().is_some_and(|f| f.italic),
        underline: d
            .font
            .as_ref()
            .and_then(|f| f.underline.as_ref())
            .is_some_and(|u| u != "none"),
        strike: d.font.as_ref().is_some_and(|f| f.strike),
        number_format: d.number_format.as_ref().map(|n| n.code.clone()),
    };
    Some(dxf)
}
