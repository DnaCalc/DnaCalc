//! Format Cells: the cell format a view reads and the typed change it commits.
//!
//! One type serves both directions. In a change, `None` leaves a property as it is. In the facts a
//! host returns for a selection, `None` says the selected cells differ in that property, which
//! is what Excel's dialog shows as a greyed box or an empty list.

use crate::RawValue;
use crate::grid::Key;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HorizontalAlignment {
    General,
    Left,
    Center,
    Right,
    Fill,
    Justify,
    CenterAcrossSelection,
    Distributed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerticalAlignment {
    Top,
    Center,
    Bottom,
    Justify,
    Distributed,
}

/// The Alignment tab's Orientation: degrees from -90 to 90, or text stacked top to bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextOrientation {
    Degrees(i16),
    Stacked,
}

/// The Alignment tab's Text direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextDirection {
    Context,
    LeftToRight,
    RightToLeft,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnderlineStyle {
    None,
    Single,
    Double,
    SingleAccounting,
    DoubleAccounting,
}

/// A colour choice in the dialog. `Automatic` is the Font tab's Automatic, the Border tab's
/// Automatic and the Fill tab's No Color.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FormatColor {
    Automatic,
    Rgb([u8; 3]),
}

/// The Border tab's line styles, in the order of its Style box, after None.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BorderLine {
    None,
    Hair,
    Dotted,
    DashDotDot,
    DashDot,
    Dashed,
    Thin,
    MediumDashDotDot,
    SlantDashDot,
    MediumDashDot,
    MediumDashed,
    Medium,
    Thick,
    Double,
}

impl BorderLine {
    pub const ALL: [Self; 14] = [
        Self::None,
        Self::Hair,
        Self::Dotted,
        Self::DashDotDot,
        Self::DashDot,
        Self::Dashed,
        Self::Thin,
        Self::MediumDashDotDot,
        Self::SlantDashDot,
        Self::MediumDashDot,
        Self::MediumDashed,
        Self::Medium,
        Self::Thick,
        Self::Double,
    ];

    /// The workbook file's name for the line (`ST_BorderStyle`).
    pub fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Hair => "hair",
            Self::Dotted => "dotted",
            Self::DashDotDot => "dashDotDot",
            Self::DashDot => "dashDot",
            Self::Dashed => "dashed",
            Self::Thin => "thin",
            Self::MediumDashDotDot => "mediumDashDotDot",
            Self::SlantDashDot => "slantDashDot",
            Self::MediumDashDot => "mediumDashDot",
            Self::MediumDashed => "mediumDashed",
            Self::Medium => "medium",
            Self::Thick => "thick",
            Self::Double => "double",
        }
    }

    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|line| line.name() == name)
    }
}

/// One border line: its style and colour. A line of `None` is no border.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EdgeStroke {
    pub line: BorderLine,
    pub color: FormatColor,
}

impl EdgeStroke {
    pub const NONE: Self = Self {
        line: BorderLine::None,
        color: FormatColor::Automatic,
    };
}

/// The eight borders of the Border tab, for each selected area as a whole: its four outside
/// edges, the edges between its cells, and the two diagonals of every cell.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RangeBorders {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left: Option<EdgeStroke>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right: Option<EdgeStroke>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top: Option<EdgeStroke>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bottom: Option<EdgeStroke>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inside_horizontal: Option<EdgeStroke>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inside_vertical: Option<EdgeStroke>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagonal_down: Option<EdgeStroke>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagonal_up: Option<EdgeStroke>,
}

impl RangeBorders {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// These borders with both diagonals stated.
    pub fn with_diagonals(self, stroke: EdgeStroke) -> Self {
        Self {
            diagonal_down: Some(stroke),
            diagonal_up: Some(stroke),
            ..self
        }
    }
}

/// The Fill tab's Pattern Style, in the workbook file's names (`ST_PatternType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FillPattern {
    None,
    Solid,
    Gray75,
    Gray50,
    Gray25,
    Gray125,
    Gray0625,
    HorizontalStripe,
    VerticalStripe,
    ReverseDiagonalStripe,
    DiagonalStripe,
    DiagonalCrosshatch,
    ThickDiagonalCrosshatch,
    ThinHorizontalStripe,
    ThinVerticalStripe,
    ThinReverseDiagonalStripe,
    ThinDiagonalStripe,
    ThinHorizontalCrosshatch,
    ThinDiagonalCrosshatch,
}

impl FillPattern {
    pub const ALL: [Self; 19] = [
        Self::None,
        Self::Solid,
        Self::Gray75,
        Self::Gray50,
        Self::Gray25,
        Self::Gray125,
        Self::Gray0625,
        Self::HorizontalStripe,
        Self::VerticalStripe,
        Self::ReverseDiagonalStripe,
        Self::DiagonalStripe,
        Self::DiagonalCrosshatch,
        Self::ThickDiagonalCrosshatch,
        Self::ThinHorizontalStripe,
        Self::ThinVerticalStripe,
        Self::ThinReverseDiagonalStripe,
        Self::ThinDiagonalStripe,
        Self::ThinHorizontalCrosshatch,
        Self::ThinDiagonalCrosshatch,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Solid => "solid",
            Self::Gray75 => "darkGray",
            Self::Gray50 => "mediumGray",
            Self::Gray25 => "lightGray",
            Self::Gray125 => "gray125",
            Self::Gray0625 => "gray0625",
            Self::HorizontalStripe => "darkHorizontal",
            Self::VerticalStripe => "darkVertical",
            Self::ReverseDiagonalStripe => "darkDown",
            Self::DiagonalStripe => "darkUp",
            Self::DiagonalCrosshatch => "darkGrid",
            Self::ThickDiagonalCrosshatch => "darkTrellis",
            Self::ThinHorizontalStripe => "lightHorizontal",
            Self::ThinVerticalStripe => "lightVertical",
            Self::ThinReverseDiagonalStripe => "lightDown",
            Self::ThinDiagonalStripe => "lightUp",
            Self::ThinHorizontalCrosshatch => "lightGrid",
            Self::ThinDiagonalCrosshatch => "lightTrellis",
        }
    }

    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|pattern| pattern.name() == name)
    }
}

/// Every property the six tabs of Format Cells set.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CellFormatting {
    /// Number: the format code, as Custom shows it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub number_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub horizontal: Option<HorizontalAlignment>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vertical: Option<VerticalAlignment>,
    /// Indent, 0 to 250.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indent: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub justify_distributed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wrap: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shrink: Option<bool>,
    /// Merge cells: each selected area becomes one merged cell, or its merged cells are split.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merge: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<TextDirection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orientation: Option<TextOrientation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_name: Option<String>,
    /// Size in points as the Size box holds it: `11`, `10.5`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_size: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bold: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub italic: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub underline: Option<UnderlineStyle>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_color: Option<FormatColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strikethrough: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub superscript: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscript: Option<bool>,
    #[serde(skip_serializing_if = "RangeBorders::is_empty")]
    pub borders: RangeBorders,
    /// Background Color; `Automatic` is No Color.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fill_color: Option<FormatColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<FillPattern>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern_color: Option<FormatColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locked: Option<bool>,
    /// Protection's Hidden: the formula is not shown while the sheet is protected.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hidden: Option<bool>,
}

impl CellFormatting {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// What Format Cells shows when it opens over a selection.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CellFormatFacts {
    /// The properties every selected cell shares; a missing one differs across the selection.
    pub common: CellFormatting,
    /// The active cell, whose value the Number tab's Sample formats.
    pub active: Key,
    pub sample: RawValue,
    /// The Sample under the active cell's current number format.
    pub sample_text: String,
    /// The workbook's custom number format codes, for the Custom category's list.
    pub custom_formats: Vec<String>,
    /// The Normal style's font and size, which the Font tab's Normal font box restores.
    pub normal_font: (String, String),
    pub date1904: bool,
}

/// One number format applied to the Sample value.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormatSample {
    pub text: String,
}
