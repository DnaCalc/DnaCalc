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
pub struct StyleTable {
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
