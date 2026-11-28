//! Format Cells: Number, Alignment, Font, Border, Fill and Protection, as Excel's dialog has them.
//!
//! The dialog opens showing what the selected cells share and commits only what was changed, as
//! one `ModelChange::FormatCells`: one workbook edit and one Undo.

use super::number_format::{self as number, Choices};
use crate::dialog::{self, Choice, Commit, Family, Field, FieldId, Kind, Spec, State, Tab, Target, Value};
use crate::{Live, Session, ViewCommand};
use calc_skin::{
    BorderLine, CellFormatFacts, CellFormatting, EdgeStroke, FillPattern, FormatColor, HorizontalAlignment,
    ModelChange, RangeBorders, TextDirection, TextOrientation, UnderlineStyle, VerticalAlignment, ViewId, query,
};
use leptos::prelude::*;

pub const OPEN: &str = "classic.format-cells";
pub const OPEN_NUMBER: &str = "classic.format-cells-number";
pub const OPEN_ALIGNMENT: &str = "classic.format-cells-alignment";
pub const OPEN_FONT: &str = "classic.format-cells-font";

pub static FAMILY: Family = Family {
    id: "format-cells",
    command: OPEN,
    label: "Format Cells",
    chords: &["Ctrl+1"],
    when: "mode:ready && writable",
    target: Target::selection,
    spec,
    begin,
    poll,
    end,
    enabled,
    shown,
    changed,
    press,
    custom,
    commit,
};

const HORIZONTAL: [(HorizontalAlignment, &str, &str); 8] = [
    (HorizontalAlignment::General, "general", "General"),
    (HorizontalAlignment::Left, "left", "Left (Indent)"),
    (HorizontalAlignment::Center, "center", "Center"),
    (HorizontalAlignment::Right, "right", "Right (Indent)"),
    (HorizontalAlignment::Fill, "fill", "Fill"),
    (HorizontalAlignment::Justify, "justify", "Justify"),
    (HorizontalAlignment::CenterAcrossSelection, "center-across", "Center Across Selection"),
    (HorizontalAlignment::Distributed, "distributed", "Distributed (Indent)"),
];

const VERTICAL: [(VerticalAlignment, &str, &str); 5] = [
    (VerticalAlignment::Top, "top", "Top"),
    (VerticalAlignment::Center, "center", "Center"),
    (VerticalAlignment::Bottom, "bottom", "Bottom"),
    (VerticalAlignment::Justify, "justify", "Justify"),
    (VerticalAlignment::Distributed, "distributed", "Distributed"),
];

const DIRECTION: [(TextDirection, &str, &str); 3] = [
    (TextDirection::Context, "context", "Context"),
    (TextDirection::LeftToRight, "ltr", "Left-to-Right"),
    (TextDirection::RightToLeft, "rtl", "Right-to-Left"),
];

const UNDERLINE: [(UnderlineStyle, &str, &str); 5] = [
    (UnderlineStyle::None, "none", "None"),
    (UnderlineStyle::Single, "single", "Single"),
    (UnderlineStyle::Double, "double", "Double"),
    (UnderlineStyle::SingleAccounting, "single-accounting", "Single Accounting"),
    (UnderlineStyle::DoubleAccounting, "double-accounting", "Double Accounting"),
];

const FONT_STYLES: [&str; 4] = ["Regular", "Italic", "Bold", "Bold Italic"];
const FONT_SIZES: [&str; 16] = [
    "8", "9", "10", "11", "12", "14", "16", "18", "20", "22", "24", "26", "28", "36", "48", "72",
];
/// Fonts the list offers. A workbook's own font is always shown in the box above the list.
const FONTS: [&str; 14] = [
    "Aptos Narrow",
    "Arial",
    "Calibri",
    "Cambria",
    "Candara",
    "Consolas",
    "Courier New",
    "Georgia",
    "Segoe UI",
    "Tahoma",
    "Times New Roman",
    "Trebuchet MS",
    "Verdana",
    "Wingdings",
];

const LINES: [(BorderLine, &str); 14] = [
    (BorderLine::None, "None"),
    (BorderLine::Hair, "Hair"),
    (BorderLine::Dotted, "Dotted"),
    (BorderLine::DashDotDot, "Dash Dot Dot"),
    (BorderLine::DashDot, "Dash Dot"),
    (BorderLine::Dashed, "Dashed"),
    (BorderLine::Thin, "Thin"),
    (BorderLine::MediumDashDotDot, "Medium Dash Dot Dot"),
    (BorderLine::SlantDashDot, "Slant Dash Dot"),
    (BorderLine::MediumDashDot, "Medium Dash Dot"),
    (BorderLine::MediumDashed, "Medium Dashed"),
    (BorderLine::Medium, "Medium"),
    (BorderLine::Thick, "Thick"),
    (BorderLine::Double, "Double"),
];

const PATTERNS: [(FillPattern, &str); 19] = [
    (FillPattern::None, "None"),
    (FillPattern::Solid, "Solid"),
    (FillPattern::Gray75, "75% Gray"),
    (FillPattern::Gray50, "50% Gray"),
    (FillPattern::Gray25, "25% Gray"),
    (FillPattern::Gray125, "12.5% Gray"),
    (FillPattern::Gray0625, "6.25% Gray"),
    (FillPattern::HorizontalStripe, "Horizontal Stripe"),
    (FillPattern::VerticalStripe, "Vertical Stripe"),
    (FillPattern::ReverseDiagonalStripe, "Reverse Diagonal Stripe"),
    (FillPattern::DiagonalStripe, "Diagonal Stripe"),
    (FillPattern::DiagonalCrosshatch, "Diagonal Crosshatch"),
    (FillPattern::ThickDiagonalCrosshatch, "Thick Diagonal Crosshatch"),
    (FillPattern::ThinHorizontalStripe, "Thin Horizontal Stripe"),
    (FillPattern::ThinVerticalStripe, "Thin Vertical Stripe"),
    (FillPattern::ThinReverseDiagonalStripe, "Thin Reverse Diagonal Stripe"),
    (FillPattern::ThinDiagonalStripe, "Thin Diagonal Stripe"),
    (FillPattern::ThinHorizontalCrosshatch, "Thin Horizontal Crosshatch"),
    (FillPattern::ThinDiagonalCrosshatch, "Thin Diagonal Crosshatch"),
];

/// The eight border buttons, in the dialog's order, with what each sets.
const EDGES: [(FieldId, &str); 8] = [
    ("border.top", "Top"),
    ("border.inside-horizontal", "Inside horizontal"),
    ("border.bottom", "Bottom"),
    ("border.diagonal-up", "Diagonal up"),
    ("border.left", "Left"),
    ("border.inside-vertical", "Inside vertical"),
    ("border.right", "Right"),
    ("border.diagonal-down", "Diagonal down"),
];

const PROTECTION_NOTE: &str = "Locking cells or hiding formulas has no effect until you protect the worksheet \
    (Review tab, Protect group, Protect Sheet button).";

fn named<T: Copy + PartialEq>(table: &[(T, &'static str, &'static str)]) -> Vec<Choice> {
    table.iter().map(|(_, value, label)| Choice::new(*value, *label)).collect()
}

fn spec(_: &Target) -> Spec {
    let color = |automatic| Kind::Color { automatic };
    Spec {
        id: "format-cells",
        title: "Format Cells",
        accept: "OK",
        tabs: vec![
            Tab {
                id: "number",
                label: "Number",
                fields: vec![
                    Field::new("number.category", "&Category:", Kind::List(Choice::plain(&number::CATEGORIES), 12)),
                    Field::new("number.sample", "Sample", Kind::Custom),
                    Field::new("number.decimals", "&Decimal places:", Kind::Spin { min: 0, max: 30 }),
                    Field::new("number.separator", "&Use 1000 Separator (,)", Kind::Check),
                    Field::new("number.symbol", "&Symbol:", Kind::DropDown(vec![
                        Choice::new("none", "None"),
                        Choice::new("$", "$"),
                    ])),
                    Field::new("number.negative", "&Negative numbers:", Kind::List(Vec::new(), 4)),
                    Field::new("number.code", "&Type:", Kind::Edit { max_length: 255 }),
                    Field::new("number.type", "&Type:", Kind::List(Vec::new(), 8)),
                    Field::new("number.locale", "&Locale (location):", Kind::DropDown(vec![
                        Choice::new("en-US", "English (United States)"),
                    ])),
                ],
            },
            Tab {
                id: "alignment",
                label: "Alignment",
                fields: vec![
                    Field::new("align.horizontal", "&Horizontal:", Kind::DropDown(named(&HORIZONTAL))).in_group("Text alignment"),
                    Field::new("align.indent", "&Indent:", Kind::Spin { min: 0, max: 250 }).in_group("Text alignment"),
                    Field::new("align.vertical", "&Vertical:", Kind::DropDown(named(&VERTICAL))).in_group("Text alignment"),
                    Field::new("align.justify-distributed", "&Justify distributed", Kind::Check).in_group("Text alignment"),
                    Field::new("align.wrap", "&Wrap text", Kind::Check).in_group("Text control"),
                    Field::new("align.shrink", "Shrin&k to fit", Kind::Check).in_group("Text control"),
                    Field::new("align.merge", "&Merge cells", Kind::Check).in_group("Text control"),
                    Field::new("align.direction", "&Text direction:", Kind::DropDown(named(&DIRECTION))).in_group("Right-to-left"),
                    Field::new("align.stacked", "Vertical text", Kind::Check).in_group("Orientation"),
                    Field::new("align.degrees", "&Degrees", Kind::Spin { min: -90, max: 90 }).in_group("Orientation"),
                ],
            },
            Tab {
                id: "font",
                label: "Font",
                fields: vec![
                    Field::new("font.name", "&Font:", Kind::Combo(Choice::plain(&FONTS))),
                    Field::new("font.style", "F&ont style:", Kind::Combo(Choice::plain(&FONT_STYLES))),
                    Field::new("font.size", "&Size:", Kind::Combo(Choice::plain(&FONT_SIZES))),
                    Field::new("font.underline", "&Underline:", Kind::DropDown(named(&UNDERLINE))),
                    Field::new("font.color", "&Color:", color("Automatic")),
                    Field::new("font.normal", "&Normal font", Kind::Check),
                    Field::new("font.strikethrough", "Stri&kethrough", Kind::Check).in_group("Effects"),
                    Field::new("font.superscript", "Sup&erscript", Kind::Check).in_group("Effects"),
                    Field::new("font.subscript", "Su&bscript", Kind::Check).in_group("Effects"),
                    Field::new("font.preview", "Preview", Kind::Custom),
                ],
            },
            Tab {
                id: "border",
                label: "Border",
                fields: {
                    let lines = LINES.iter().map(|(line, label)| Choice::new(line.name(), *label)).collect();
                    let mut fields = vec![
                        Field::new("border.style", "&Style:", Kind::List(lines, 14)).in_group("Line"),
                        Field::new("border.color", "&Color:", color("Automatic")).in_group("Line"),
                        Field::new("border.preset-none", "&None", Kind::Button).in_group("Presets"),
                        Field::new("border.preset-outline", "&Outline", Kind::Button).in_group("Presets"),
                        Field::new("border.preset-inside", "&Inside", Kind::Button).in_group("Presets"),
                    ];
                    fields.extend(EDGES.iter().map(|(id, label)| Field::new(id, label, Kind::Toggle).in_group("Border")));
                    fields.push(Field::new("border.preview", "", Kind::Custom).in_group("Border"));
                    fields
                },
            },
            Tab {
                id: "fill",
                label: "Fill",
                fields: vec![
                    Field::new("fill.color", "Background &Color:", color("No Color")),
                    Field::new("fill.pattern-color", "P&attern Color:", color("Automatic")),
                    Field::new("fill.pattern", "&Pattern Style:", Kind::DropDown(
                        PATTERNS.iter().map(|(pattern, label)| Choice::new(pattern.name(), *label)).collect(),
                    )),
                    Field::new("fill.effects", "F&ill Effects...", Kind::Button),
                    Field::new("fill.sample", "Sample", Kind::Custom),
                ],
            },
            Tab {
                id: "protection",
                label: "Protection",
                fields: vec![
                    Field::new("protect.locked", "&Locked", Kind::Check),
                    Field::new("protect.hidden", "H&idden", Kind::Check),
                    Field::new("protect.note", PROTECTION_NOTE, Kind::Note),
                ],
            },
        ],
    }
}

#[derive(Clone, Copy, Default)]
struct Reading {
    watch: RwSignal<Option<dna_skin_runtime::client::Watch<query::CellFormatFor>>>,
}

fn begin(session: Session, target: &Target) {
    let reading = session.local::<Reading>();
    reading.watch.set(Some(session.watch(query::CellFormatFor {
        areas: target.areas.clone(),
        active: target.active,
    })));
}

fn end(session: Session) {
    if let Some(watch) = session.local::<Reading>().watch.get_untracked() {
        session.unwatch(watch);
    }
    session.local::<Reading>().watch.set(None);
}

fn poll(session: Session, target: &Target, state: &mut State) -> Live<()> {
    let Some(watch) = session.local::<Reading>().watch.get() else {
        return Live::Idle;
    };
    match session.read(watch) {
        Live::Ready(answer) if answer.stamp.workbook == target.workbook && answer.stamp.model == target.revision => {
            fill(state, target, &answer.body);
            Live::Ready(())
        }
        Live::Refused(reason) => Live::Refused(reason),
        _ => Live::Loading,
    }
}

fn color_value(color: Option<FormatColor>) -> Value {
    Value::Choice(color.map(|color| match color {
        FormatColor::Automatic => "auto".to_owned(),
        FormatColor::Rgb([red, green, blue]) => format!("{red:02X}{green:02X}{blue:02X}"),
    }))
}

fn color_of(value: &str) -> Option<FormatColor> {
    if value == "auto" {
        return Some(FormatColor::Automatic);
    }
    let rgb = u32::from_str_radix(value, 16).ok().filter(|_| value.len() == 6)?;
    let [_, red, green, blue] = rgb.to_be_bytes();
    Some(FormatColor::Rgb([red, green, blue]))
}

fn stroke_value(stroke: Option<EdgeStroke>) -> Value {
    Value::Choice(stroke.map(|stroke| {
        if stroke.line == BorderLine::None {
            return "none".to_owned();
        }
        let Value::Choice(Some(color)) = color_value(Some(stroke.color)) else { unreachable!("a stated colour") };
        format!("{}:{color}", stroke.line.name())
    }))
}

fn stroke_of(value: &str) -> Option<EdgeStroke> {
    if value == "none" {
        return Some(EdgeStroke::NONE);
    }
    let (line, color) = value.split_once(':')?;
    Some(EdgeStroke {
        line: BorderLine::named(line)?,
        color: color_of(color)?,
    })
}

fn table_value<T: Copy + PartialEq>(table: &[(T, &'static str, &'static str)], value: Option<T>) -> Value {
    Value::Choice(value.and_then(|value| table.iter().find(|(item, ..)| *item == value).map(|(_, name, _)| (*name).to_owned())))
}

fn table_item<T: Copy>(table: &[(T, &'static str, &'static str)], name: Option<&str>) -> Option<T> {
    let name = name?;
    table.iter().find(|(_, value, _)| *value == name).map(|(item, ..)| *item)
}

/// Puts the choices of a format code into the Number tab.
fn show_number(state: &mut State, choices: &Choices, code: Option<&str>) {
    state.set_choice("number.category", choices.category);
    state.set_text("number.decimals", choices.decimals.to_string());
    state.set_flag("number.separator", choices.separator);
    state.set_choice("number.symbol", if choices.dollar { "$" } else { "none" });
    state.set_choice("number.negative", choices.negative);
    list_number(state, choices);
    match choices.kind {
        Some(kind) => state.set_choice("number.type", kind),
        None => state.set("number.type", Value::Choice(code.map(str::to_owned))),
    }
    if let Some(code) = choices.code().or_else(|| code.map(str::to_owned)) {
        state.set_text("number.code", code);
    }
}

/// The lists that follow the category and its options.
fn list_number(state: &mut State, choices: &Choices) {
    let labels = choices.negative_labels();
    state.set_choices(
        "number.negative",
        number::NEGATIVES.iter().zip(labels).map(|(value, label)| Choice::new(*value, label)).collect(),
    );
    let types = if choices.category == number::CUSTOM {
        let mut codes: Vec<String> = number::CUSTOM_CODES.iter().map(|code| (*code).to_owned()).collect();
        if let Some(Value::Text(extra)) = state.value("meta.custom-formats").cloned() {
            for code in extra.lines() {
                if !codes.iter().any(|known| known == code) {
                    codes.push(code.to_owned());
                }
            }
        }
        codes.into_iter().map(|code| Choice::new(code.clone(), code)).collect()
    } else {
        number::types(choices.category).iter().map(|(code, label)| Choice::new(*code, *label)).collect()
    };
    state.set_choices("number.type", types);
}

/// The Number tab's choices as the fields hold them now.
fn number_choices(state: &State) -> Option<Choices> {
    let category = number::CATEGORIES.into_iter().find(|category| Some(*category) == state.choice("number.category"))?;
    let kind = state.choice("number.type").and_then(|code| {
        number::types(category).iter().find(|(kind, _)| *kind == code).map(|(kind, _)| *kind)
    });
    Some(Choices {
        category,
        decimals: state.number("number.decimals").and_then(|n| u8::try_from(n).ok()).filter(|n| *n <= 30)?,
        separator: state.flag("number.separator") == Some(true),
        dollar: state.choice("number.symbol") == Some("$"),
        negative: number::NEGATIVES.into_iter().find(|n| Some(*n) == state.choice("number.negative")).unwrap_or(number::NEGATIVES[0]),
        kind,
    })
}

fn fill(state: &mut State, target: &Target, facts: &CellFormatFacts) {
    let common = &facts.common;
    state.set("meta.custom-formats", Value::Text(facts.custom_formats.join("\n")));
    state.set("meta.normal-font", Value::Text(format!("{}\n{}", facts.normal_font.0, facts.normal_font.1)));
    state.set("meta.sample", Value::Text(facts.sample_text.clone()));
    let deep = |rows: bool| {
        target.areas.iter().any(|area| if rows { area.0.2 != area.1.2 } else { area.0.3 != area.1.3 })
    };
    state.set("meta.rows", Value::Flag(Some(deep(true))));
    state.set("meta.columns", Value::Flag(Some(deep(false))));

    match &common.number_format {
        Some(code) => show_number(state, &Choices::of(code), Some(code)),
        None => {
            // A selection of several formats shows no category until one is chosen.
            show_number(state, &Choices::defaults(number::GENERAL), None);
            state.set("number.category", Value::Choice(None));
            state.set_text("number.code", "");
        }
    }

    state.set("align.horizontal", table_value(&HORIZONTAL, common.horizontal));
    state.set("align.vertical", table_value(&VERTICAL, common.vertical));
    state.set_text("align.indent", common.indent.map(|indent| indent.to_string()).unwrap_or_default());
    state.set("align.justify-distributed", Value::Flag(Some(common.justify_distributed.unwrap_or(false))));
    state.set("align.wrap", Value::Flag(common.wrap));
    state.set("align.shrink", Value::Flag(common.shrink));
    state.set("align.merge", Value::Flag(common.merge));
    state.set("align.direction", table_value(&DIRECTION, Some(common.direction.unwrap_or(TextDirection::Context))));
    state.set("align.stacked", Value::Flag(Some(common.orientation == Some(TextOrientation::Stacked))));
    state.set_text("align.degrees", match common.orientation {
        Some(TextOrientation::Degrees(degrees)) => degrees.to_string(),
        Some(TextOrientation::Stacked) => "0".into(),
        None => String::new(),
    });

    state.set_text("font.name", common.font_name.clone().unwrap_or_default());
    state.set_text("font.style", match (common.bold, common.italic) {
        (Some(false), Some(false)) => "Regular",
        (Some(false), Some(true)) => "Italic",
        (Some(true), Some(false)) => "Bold",
        (Some(true), Some(true)) => "Bold Italic",
        _ => "",
    });
    state.set_text("font.size", common.font_size.clone().unwrap_or_default());
    state.set("font.underline", table_value(&UNDERLINE, common.underline));
    state.set("font.color", color_value(common.font_color));
    state.set("font.strikethrough", Value::Flag(common.strikethrough));
    state.set("font.superscript", Value::Flag(common.superscript));
    state.set("font.subscript", Value::Flag(common.subscript));
    let normal = common.font_name.as_deref() == Some(facts.normal_font.0.as_str())
        && common.font_size.as_deref() == Some(facts.normal_font.1.as_str())
        && (common.bold, common.italic) == (Some(false), Some(false))
        && common.underline == Some(UnderlineStyle::None)
        && common.font_color == Some(FormatColor::Automatic)
        && (common.strikethrough, common.superscript, common.subscript) == (Some(false), Some(false), Some(false));
    state.set("font.normal", Value::Flag(Some(normal)));

    // Excel opens the Border tab with the thin line and the automatic colour chosen.
    state.set_choice("border.style", BorderLine::Thin.name());
    state.set("border.color", color_value(Some(FormatColor::Automatic)));
    let RangeBorders { left, right, top, bottom, inside_horizontal, inside_vertical, diagonal_down, diagonal_up } = common.borders;
    for (id, stroke) in [
        ("border.top", top),
        ("border.inside-horizontal", inside_horizontal),
        ("border.bottom", bottom),
        ("border.diagonal-up", diagonal_up),
        ("border.left", left),
        ("border.inside-vertical", inside_vertical),
        ("border.right", right),
        ("border.diagonal-down", diagonal_down),
    ] {
        state.set(id, stroke_value(stroke));
    }

    state.set("fill.color", color_value(common.fill_color));
    state.set("fill.pattern-color", color_value(common.pattern_color));
    state.set("fill.pattern", Value::Choice(common.pattern.map(|pattern| pattern.name().to_owned())));
    state.set("protect.locked", Value::Flag(common.locked));
    state.set("protect.hidden", Value::Flag(common.hidden));
}

fn category(state: &State) -> &str {
    state.choice("number.category").unwrap_or("")
}

fn shown(state: &State, id: FieldId) -> bool {
    let category = category(state);
    match id {
        "number.decimals" => matches!(category, number::NUMBER | number::CURRENCY | number::ACCOUNTING | number::PERCENTAGE | number::SCIENTIFIC),
        "number.separator" => category == number::NUMBER,
        "number.symbol" => matches!(category, number::CURRENCY | number::ACCOUNTING),
        "number.negative" => matches!(category, number::NUMBER | number::CURRENCY),
        "number.type" => matches!(category, number::DATE | number::TIME | number::FRACTION | number::SPECIAL | number::CUSTOM),
        "number.code" => category == number::CUSTOM,
        "number.locale" => matches!(category, number::DATE | number::TIME | number::SPECIAL),
        _ => true,
    }
}

fn enabled(state: &State, id: FieldId) -> bool {
    match id {
        // Indent applies to the three alignments Excel names with "(Indent)".
        "align.indent" => matches!(state.choice("align.horizontal"), Some("left" | "right" | "distributed")),
        "align.justify-distributed" => state.choice("align.horizontal") == Some("distributed"),
        "align.shrink" => state.flag("align.wrap") != Some(true),
        "align.degrees" => state.flag("align.stacked") != Some(true),
        "border.inside-horizontal" => state.flag("meta.rows") == Some(true),
        "border.inside-vertical" => state.flag("meta.columns") == Some(true),
        "border.preset-inside" => state.flag("meta.rows") == Some(true) || state.flag("meta.columns") == Some(true),
        _ => true,
    }
}

fn changed(state: &mut State, id: FieldId) {
    match id {
        "number.category" => {
            if let Some(category) = number::CATEGORIES.into_iter().find(|name| Some(*name) == state.choice("number.category")) {
                let code = state.text("number.code").to_owned();
                let choices = Choices::defaults(category);
                // Custom keeps the code the cells have, as Excel's Type box does.
                show_number(state, &choices, (category == number::CUSTOM && !code.is_empty()).then_some(code.as_str()));
            }
        }
        "number.type" if category(state) == number::CUSTOM => {
            if let Some(code) = state.choice("number.type").map(str::to_owned) {
                state.set_text("number.code", code);
            }
        }
        "number.decimals" | "number.separator" | "number.symbol" | "number.negative" | "number.type" => {
            if let Some(choices) = number_choices(state) {
                list_number(state, &choices);
                if let Some(code) = choices.code() {
                    state.set_text("number.code", code);
                }
            }
        }
        "align.horizontal" => {
            if !enabled(state, "align.indent") && state.text("align.indent") != "0" {
                state.set_text("align.indent", "0");
            }
        }
        "font.normal" => {
            if state.flag("font.normal") == Some(true) {
                let normal = state.value("meta.normal-font").cloned();
                if let Some(Value::Text(normal)) = normal
                    && let Some((name, size)) = normal.split_once('\n')
                {
                    state.set_text("font.name", name);
                    state.set_text("font.size", size);
                }
                state.set_text("font.style", "Regular");
                state.set_choice("font.underline", "none");
                state.set_choice("font.color", "auto");
                for effect in ["font.strikethrough", "font.superscript", "font.subscript"] {
                    state.set_flag(effect, false);
                }
            }
        }
        "font.superscript" if state.flag(id) == Some(true) => state.set_flag("font.subscript", false),
        "font.subscript" if state.flag(id) == Some(true) => state.set_flag("font.superscript", false),
        _ => {}
    }
    if id.starts_with("font.") && id != "font.normal" && state.flag("font.normal") == Some(true) {
        state.set_flag("font.normal", false);
    }
}

/// The line the Border tab would draw now: its Style and Color.
fn pen(state: &State) -> String {
    match state.choice("border.style") {
        None | Some("none") => "none".into(),
        Some(line) => format!("{line}:{}", state.choice("border.color").unwrap_or("auto")),
    }
}

fn press(state: &mut State, id: FieldId) {
    let pen = pen(state);
    match id {
        "border.preset-none" => {
            for (edge, _) in EDGES {
                state.set_choice(edge, "none");
            }
        }
        "border.preset-outline" => {
            for edge in ["border.top", "border.bottom", "border.left", "border.right"] {
                state.set_choice(edge, pen.clone());
            }
        }
        "border.preset-inside" => {
            for edge in ["border.inside-horizontal", "border.inside-vertical"] {
                if enabled(state, edge) {
                    state.set_choice(edge, pen.clone());
                }
            }
        }
        "fill.effects" => {
            state.message = Some("Fill Effects (gradient fills) is not available yet (dc-s2xup).".into());
        }
        edge if EDGES.iter().any(|(known, _)| *known == edge) => {
            // A click draws the chosen line, and takes away a line that is already the chosen one.
            let next = if state.choice(edge) == Some(pen.as_str()) { "none".to_owned() } else { pen };
            state.set_choice(edge, next);
        }
        _ => {}
    }
}

/// The change the dialog's fields make: every field that no longer holds what it opened with.
pub fn formatting(state: &State) -> Result<CellFormatting, String> {
    let mut format = CellFormatting::default();
    if state.changed("number.code") {
        let code = state.text("number.code");
        if code.is_empty() {
            return Err("Enter a number format code in the Type box.".into());
        }
        format.number_format = Some(code.to_owned());
    } else if state.changed("number.decimals") && state.number("number.decimals").is_none_or(|n| !(0..=30).contains(&n)) {
        return Err("Decimal places must be a whole number from 0 to 30.".into());
    }
    if state.changed("align.horizontal") {
        format.horizontal = table_item(&HORIZONTAL, state.choice("align.horizontal"));
    }
    if state.changed("align.vertical") {
        format.vertical = table_item(&VERTICAL, state.choice("align.vertical"));
    }
    if state.changed("align.indent") {
        let indent = state.number("align.indent").and_then(|n| u16::try_from(n).ok()).filter(|n| *n <= 250);
        format.indent = Some(indent.ok_or("Indent must be a whole number from 0 to 250.")?);
    }
    if state.changed("align.justify-distributed") {
        format.justify_distributed = state.flag("align.justify-distributed");
    }
    if state.changed("align.wrap") {
        format.wrap = state.flag("align.wrap");
    }
    if state.changed("align.shrink") {
        format.shrink = state.flag("align.shrink");
    }
    if state.changed("align.merge") {
        format.merge = state.flag("align.merge");
    }
    if state.changed("align.direction") {
        format.direction = table_item(&DIRECTION, state.choice("align.direction"));
    }
    if state.changed("align.stacked") || state.changed("align.degrees") {
        format.orientation = Some(if state.flag("align.stacked") == Some(true) {
            TextOrientation::Stacked
        } else {
            let degrees = state.number("align.degrees").and_then(|n| i16::try_from(n).ok()).filter(|n| (-90..=90).contains(n));
            TextOrientation::Degrees(degrees.ok_or("Degrees must be a whole number from -90 to 90.")?)
        });
    }
    if state.changed("font.name") {
        let name = state.text("font.name").trim();
        if name.is_empty() {
            return Err("Choose a font.".into());
        }
        format.font_name = Some(name.to_owned());
    }
    if state.changed("font.style") {
        let style = state.text("font.style").trim().to_ascii_lowercase();
        let (bold, italic) = match style.as_str() {
            "regular" => (false, false),
            "italic" => (false, true),
            "bold" => (true, false),
            "bold italic" => (true, true),
            _ => return Err("Choose a font style from the list: Regular, Italic, Bold or Bold Italic.".into()),
        };
        format.bold = Some(bold);
        format.italic = Some(italic);
    }
    if state.changed("font.size") {
        let size = state.text("font.size").trim();
        if !size.parse::<f64>().is_ok_and(|points| (1.0..=409.0).contains(&points)) {
            return Err("Font size must be between 1 and 409 points.".into());
        }
        format.font_size = Some(size.to_owned());
    }
    if state.changed("font.underline") {
        format.underline = table_item(&UNDERLINE, state.choice("font.underline"));
    }
    if state.changed("font.color") {
        format.font_color = state.choice("font.color").and_then(color_of);
    }
    if state.changed("font.strikethrough") {
        format.strikethrough = state.flag("font.strikethrough");
    }
    if state.changed("font.superscript") {
        format.superscript = state.flag("font.superscript");
    }
    if state.changed("font.subscript") {
        format.subscript = state.flag("font.subscript");
    }
    let edge = |id: &str| if state.changed(id) { state.choice(id).and_then(stroke_of) } else { None };
    format.borders = RangeBorders {
        left: edge("border.left"),
        right: edge("border.right"),
        top: edge("border.top"),
        bottom: edge("border.bottom"),
        inside_horizontal: edge("border.inside-horizontal"),
        inside_vertical: edge("border.inside-vertical"),
        diagonal_down: edge("border.diagonal-down"),
        diagonal_up: edge("border.diagonal-up"),
    };
    if state.changed("fill.color") {
        format.fill_color = state.choice("fill.color").and_then(color_of);
    }
    if state.changed("fill.pattern") {
        format.pattern = state.choice("fill.pattern").and_then(FillPattern::named);
    }
    // Excel keeps a pattern colour only with a pattern that has marks to colour.
    let patterned = !matches!(state.choice("fill.pattern"), None | Some("none" | "solid"));
    if state.changed("fill.pattern-color") && patterned {
        format.pattern_color = state.choice("fill.pattern-color").and_then(color_of);
    }
    if state.changed("protect.locked") {
        format.locked = state.flag("protect.locked");
    }
    if state.changed("protect.hidden") {
        format.hidden = state.flag("protect.hidden");
    }
    Ok(format)
}

fn commit(state: &State, target: &Target) -> Result<Commit, String> {
    let format = formatting(state)?;
    if format.is_empty() {
        return Ok(Commit::Nothing);
    }
    Ok(Commit::Edit(target.batch("Format Cells", ModelChange::FormatCells {
        areas: target.areas.clone(),
        format: Box::new(format),
    })))
}

fn css_color(value: Option<&str>, automatic: &'static str) -> String {
    match value {
        Some(rgb) if rgb != "auto" && rgb.len() == 6 => format!("#{rgb}"),
        _ => automatic.to_owned(),
    }
}

fn custom(session: Session, id: FieldId) -> AnyView {
    match id {
        "number.sample" => {
            let sample = session.following::<query::FormatSampleFor>();
            let text = move || {
                let code = dialog::read(session, |state| {
                    state.changed("number.code").then(|| state.text("number.code").to_owned()).filter(|code| !code.is_empty())
                })
                .flatten();
                let opened = dialog::read(session, |state| state.text("meta.sample").to_owned()).unwrap_or_default();
                let Some((code, target)) = code.zip(dialog::target(session)) else {
                    sample.at(None);
                    return opened;
                };
                match sample.at(Some(query::FormatSampleFor { at: target.active, number_format: code })) {
                    Live::Ready(answer) => answer.body.text,
                    Live::Refused(_) => String::new(),
                    _ => opened,
                }
            };
            view! { <output class="dna-dialog-sample" data-dnacalc="format-sample">{text}</output> }.into_any()
        }
        "font.preview" => {
            let style = move || {
                dialog::read(session, |state| {
                    let face = state.text("font.style").to_ascii_lowercase();
                    let underline = !matches!(state.choice("font.underline"), None | Some("none"));
                    let strike = state.flag("font.strikethrough") == Some(true);
                    let line = match (underline, strike) {
                        (true, true) => "underline line-through",
                        (true, false) => "underline",
                        (false, true) => "line-through",
                        (false, false) => "none",
                    };
                    format!(
                        "font-family:'{}';font-size:{}pt;font-weight:{};font-style:{};text-decoration:{};color:{};vertical-align:{}",
                        state.text("font.name").replace(['\'', ';'], ""),
                        state.text("font.size").parse::<f64>().map_or(11.0, |size| size.clamp(1.0, 409.0)),
                        if face.contains("bold") { "bold" } else { "normal" },
                        if face.contains("italic") { "italic" } else { "normal" },
                        line,
                        css_color(state.choice("font.color"), "currentColor"),
                        match (state.flag("font.superscript"), state.flag("font.subscript")) {
                            (Some(true), _) => "super",
                            (_, Some(true)) => "sub",
                            _ => "baseline",
                        },
                    )
                })
                .unwrap_or_default()
            };
            view! { <output class="dna-dialog-sample" data-dnacalc="format-font-preview"><span style=style>"AaBbCcYyZz"</span></output> }.into_any()
        }
        "fill.sample" => {
            let style = move || {
                dialog::read(session, |state| {
                    format!("background:{}", css_color(state.choice("fill.color"), "transparent"))
                })
                .unwrap_or_default()
            };
            view! { <output class="dna-dialog-sample dna-dialog-fill-sample" data-dnacalc="format-fill-sample" style=style></output> }.into_any()
        }
        "border.preview" => {
            // The diagram draws each of the eight borders the dialog now holds.
            let edge = move |id: &'static str| dialog::read(session, |state| state.choice(id).map(str::to_owned)).flatten();
            let drawn = move |id: &'static str| match edge(id) {
                None => "mixed".to_owned(),
                Some(value) => value,
            };
            view! {
                <div class="dna-dialog-border-preview" data-dnacalc="format-border-preview" aria-hidden="true">
                    {EDGES.iter().map(|(id, _)| {
                        let id = *id;
                        view! { <i data-edge=id.trim_start_matches("border.") data-line=move || drawn(id)></i> }
                    }).collect_view()}
                    <span>"Text"</span>
                </div>
            }.into_any()
        }
        _ => ().into_any(),
    }
}

/// Format Cells on a named tab, as the ribbon's dialog launchers and Ctrl+Shift+F open it.
pub fn commands(view: &ViewId, session: Session) -> Vec<ViewCommand> {
    let on_tab = |id: &'static str, label: &'static str, chords: &[&'static str], tab: &'static str| {
        ViewCommand::new(view, id, label, chords, move |_| {
            session.command(OPEN).map_err(|error| format!("{error:?}"))?;
            dialog::show(session, tab);
            Ok(())
        })
        .when(FAMILY.when)
    };
    vec![
        on_tab(OPEN_NUMBER, "Format Cells: Number", &[], "number"),
        on_tab(OPEN_ALIGNMENT, "Format Cells: Alignment", &[], "alignment"),
        on_tab(OPEN_FONT, "Format Cells: Font", &["Ctrl+Shift+F", "Ctrl+Shift+P"], "font"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use calc_skin::grid::{Area, Key, Sheet};
    use calc_skin::{RawValue, Revision, WorkbookId};

    fn target() -> Target {
        Target {
            workbook: WorkbookId(1),
            revision: Revision(4),
            sheet: Sheet(0, 1),
            areas: vec![Area(Key(0, 1, 2, 2), Key(0, 1, 4, 3))],
            active: Key(0, 1, 2, 2),
        }
    }

    fn facts() -> CellFormatFacts {
        CellFormatFacts {
            common: CellFormatting {
                number_format: Some("General".into()),
                horizontal: Some(HorizontalAlignment::General),
                vertical: Some(VerticalAlignment::Bottom),
                indent: Some(0),
                justify_distributed: None,
                wrap: Some(false),
                shrink: Some(false),
                merge: Some(false),
                direction: None,
                orientation: Some(TextOrientation::Degrees(0)),
                font_name: Some("Calibri".into()),
                font_size: Some("11".into()),
                bold: Some(false),
                italic: Some(false),
                underline: Some(UnderlineStyle::None),
                font_color: Some(FormatColor::Automatic),
                strikethrough: Some(false),
                superscript: Some(false),
                subscript: Some(false),
                borders: RangeBorders {
                    left: Some(EdgeStroke::NONE),
                    right: Some(EdgeStroke::NONE),
                    top: Some(EdgeStroke::NONE),
                    bottom: Some(EdgeStroke::NONE),
                    inside_horizontal: Some(EdgeStroke::NONE),
                    inside_vertical: Some(EdgeStroke::NONE),
                    diagonal_down: Some(EdgeStroke::NONE),
                    diagonal_up: Some(EdgeStroke::NONE),
                },
                fill_color: Some(FormatColor::Automatic),
                pattern: Some(FillPattern::None),
                pattern_color: Some(FormatColor::Automatic),
                locked: Some(true),
                hidden: Some(false),
            },
            active: Key(0, 1, 2, 2),
            sample: RawValue::Number { bits: 1234.5f64.to_bits() },
            sample_text: "1234.5".into(),
            custom_formats: vec!["0.00 \"kg\"".into()],
            normal_font: ("Calibri".into(), "11".into()),
            date1904: false,
        }
    }

    fn opened(facts: &CellFormatFacts) -> State {
        let target = target();
        let mut state = State::new(spec(&target), []);
        fill(&mut state, &target, facts);
        state.settle();
        state
    }

    fn set(state: &mut State, id: FieldId, value: Value) {
        state.set(id, value);
        changed(state, id);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn the_dialog_has_excels_six_tabs_and_opens_on_the_selections_format() {
        let state = opened(&facts());
        let tabs: Vec<_> = state.spec.tabs.iter().map(|tab| tab.label).collect();
        assert_eq!(tabs, ["Number", "Alignment", "Font", "Border", "Fill", "Protection"]);
        assert_eq!(state.choice("number.category"), Some("General"));
        assert_eq!(state.choice("align.horizontal"), Some("general"));
        assert_eq!(state.choice("align.vertical"), Some("bottom"));
        assert_eq!(state.choice("align.direction"), Some("context"));
        assert_eq!((state.text("font.name"), state.text("font.style"), state.text("font.size")), ("Calibri", "Regular", "11"));
        assert_eq!(state.flag("font.normal"), Some(true));
        assert_eq!(state.choice("border.style"), Some("thin"));
        assert_eq!(state.choice("fill.color"), Some("auto"));
        assert_eq!((state.flag("protect.locked"), state.flag("protect.hidden")), (Some(true), Some(false)));
        // OK with nothing changed changes nothing.
        assert_eq!(commit(&state, &target()), Ok(Commit::Nothing));
        // General has no options; a category shows only its own.
        for id in ["number.decimals", "number.separator", "number.symbol", "number.negative", "number.type", "number.code"] {
            assert!(!shown(&state, id), "{id}");
        }
        assert!(!enabled(&state, "align.indent"));
        assert!(enabled(&state, "border.inside-horizontal") && enabled(&state, "border.inside-vertical"));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_category_and_its_options_compose_the_code_the_dialog_commits() {
        let mut state = opened(&facts());
        set(&mut state, "number.category", Value::Choice(Some("Number".into())));
        assert_eq!(state.text("number.code"), "0.00");
        assert!(shown(&state, "number.decimals") && shown(&state, "number.separator") && shown(&state, "number.negative"));
        set(&mut state, "number.separator", Value::Flag(Some(true)));
        set(&mut state, "number.decimals", Value::Text("1".into()));
        set(&mut state, "number.negative", Value::Choice(Some("red-parentheses".into())));
        assert_eq!(state.text("number.code"), "#,##0.0_);[Red](#,##0.0)");
        let Some(Kind::List(negatives, _)) = state.field("number.negative").map(|field| field.kind.clone()) else {
            panic!("the negative numbers list");
        };
        assert_eq!(negatives[2].label, "(1,234.1)");
        assert_eq!(formatting(&state).unwrap(), CellFormatting {
            number_format: Some("#,##0.0_);[Red](#,##0.0)".into()),
            ..Default::default()
        });

        set(&mut state, "number.category", Value::Choice(Some("Date".into())));
        assert_eq!(state.text("number.code"), "m/d/yyyy");
        set(&mut state, "number.type", Value::Choice(Some("[$-409]d-mmm-yy;@".into())));
        assert_eq!(state.text("number.code"), "[$-409]d-mmm-yy;@");
        // Custom keeps the current code, lists the workbook's own codes, and takes typed codes.
        set(&mut state, "number.category", Value::Choice(Some("Custom".into())));
        assert_eq!(state.text("number.code"), "[$-409]d-mmm-yy;@");
        let Some(Kind::List(codes, _)) = state.field("number.type").map(|field| field.kind.clone()) else {
            panic!("the custom list");
        };
        assert_eq!(codes[0].value, "General");
        assert_eq!(codes.last().unwrap().value, "0.00 \"kg\"");
        set(&mut state, "number.type", Value::Choice(Some("0.00 \"kg\"".into())));
        assert_eq!(formatting(&state).unwrap().number_format.as_deref(), Some("0.00 \"kg\""));
        set(&mut state, "number.code", Value::Text(String::new()));
        assert!(formatting(&state).is_err());
        // Back on the format the cells have, the Number tab changes nothing.
        set(&mut state, "number.category", Value::Choice(Some("General".into())));
        assert_eq!(formatting(&state).unwrap(), CellFormatting::default());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn only_changed_fields_are_committed_and_linked_fields_follow() {
        let mut state = opened(&facts());
        set(&mut state, "align.horizontal", Value::Choice(Some("left".into())));
        assert!(enabled(&state, "align.indent"));
        set(&mut state, "align.indent", Value::Text("2".into()));
        set(&mut state, "align.wrap", Value::Flag(Some(true)));
        assert!(!enabled(&state, "align.shrink"), "Wrap text disables Shrink to fit");
        set(&mut state, "align.degrees", Value::Text("-45".into()));
        set(&mut state, "font.style", Value::Text("Bold Italic".into()));
        assert_eq!(state.flag("font.normal"), Some(false));
        set(&mut state, "font.superscript", Value::Flag(Some(true)));
        set(&mut state, "font.subscript", Value::Flag(Some(true)));
        assert_eq!(state.flag("font.superscript"), Some(false), "Subscript clears Superscript");
        set(&mut state, "font.color", Value::Choice(Some("C00000".into())));
        set(&mut state, "fill.color", Value::Choice(Some("FFFF00".into())));
        set(&mut state, "fill.pattern-color", Value::Choice(Some("0070C0".into())));
        set(&mut state, "protect.hidden", Value::Flag(Some(true)));
        let format = formatting(&state).unwrap();
        assert_eq!(format, CellFormatting {
            horizontal: Some(HorizontalAlignment::Left),
            indent: Some(2),
            wrap: Some(true),
            orientation: Some(TextOrientation::Degrees(-45)),
            bold: Some(true),
            italic: Some(true),
            subscript: Some(true),
            font_color: Some(FormatColor::Rgb([0xC0, 0, 0])),
            fill_color: Some(FormatColor::Rgb([0xFF, 0xFF, 0])),
            // No pattern is chosen, so the pattern colour is not part of the change.
            hidden: Some(true),
            ..Default::default()
        });
        let Commit::Edit(batch) = commit(&state, &target()).unwrap() else { panic!("an edit") };
        assert_eq!(batch.base, Some(Revision(4)));
        assert_eq!(batch.label.as_deref(), Some("Format Cells"));
        // Switching away from an indenting alignment returns the indent to zero.
        set(&mut state, "align.horizontal", Value::Choice(Some("center".into())));
        assert_eq!(state.text("align.indent"), "0");
        // Normal font restores the Normal style's font and clears the effects.
        set(&mut state, "font.normal", Value::Flag(Some(true)));
        assert_eq!((state.text("font.style"), state.choice("font.color"), state.flag("font.subscript")), ("Regular", Some("auto"), Some(false)));
        for (id, entry) in [("font.size", "0"), ("font.size", "410"), ("align.degrees", "91"), ("font.style", "Heavy")] {
            let mut state = opened(&facts());
            set(&mut state, id, Value::Text(entry.into()));
            assert!(formatting(&state).is_err(), "{id} {entry}");
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn border_presets_and_buttons_draw_with_the_chosen_line_and_toggle_it_off() {
        let mut state = opened(&facts());
        press(&mut state, "border.preset-outline");
        assert_eq!(state.choice("border.left"), Some("thin:auto"));
        assert_eq!(state.choice("border.inside-vertical"), Some("none"));
        set(&mut state, "border.style", Value::Choice(Some("double".into())));
        set(&mut state, "border.color", Value::Choice(Some("C00000".into())));
        press(&mut state, "border.preset-inside");
        press(&mut state, "border.top");
        assert_eq!(state.choice("border.top"), Some("double:C00000"), "a different line replaces the edge");
        press(&mut state, "border.top");
        assert_eq!(state.choice("border.top"), Some("none"), "the same line takes it away");
        let borders = formatting(&state).unwrap().borders;
        let thin = EdgeStroke { line: BorderLine::Thin, color: FormatColor::Automatic };
        let double = EdgeStroke { line: BorderLine::Double, color: FormatColor::Rgb([0xC0, 0, 0]) };
        assert_eq!(borders, RangeBorders {
            left: Some(thin),
            right: Some(thin),
            bottom: Some(thin),
            inside_horizontal: Some(double),
            inside_vertical: Some(double),
            ..Default::default()
        });
        press(&mut state, "border.preset-none");
        assert!(formatting(&state).unwrap().borders.is_empty(), "the borders are as they opened");
        // One row has no inside horizontal line to draw.
        let mut row = target();
        row.areas = vec![Area(Key(0, 1, 2, 2), Key(0, 1, 2, 5))];
        let mut state = State::new(spec(&row), []);
        fill(&mut state, &row, &facts());
        state.settle();
        assert!(!enabled(&state, "border.inside-horizontal") && enabled(&state, "border.preset-inside"));
        press(&mut state, "border.preset-inside");
        assert_eq!(state.choice("border.inside-horizontal"), Some("none"));
        assert_eq!(state.choice("border.inside-vertical"), Some("thin:auto"));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_mixed_selection_opens_greyed_and_commits_only_what_is_set() {
        let mut mixed = facts();
        mixed.common.number_format = None;
        mixed.common.wrap = None;
        mixed.common.bold = None;
        mixed.common.font_size = None;
        mixed.common.borders.top = None;
        mixed.common.fill_color = None;
        let mut state = opened(&mixed);
        assert_eq!(state.choice("number.category"), None);
        assert_eq!(state.value("align.wrap"), Some(&Value::Flag(None)));
        assert_eq!((state.text("font.style"), state.text("font.size")), ("", ""));
        assert_eq!(state.choice("border.top"), None);
        assert_eq!(state.choice("fill.color"), None);
        assert_eq!(formatting(&state).unwrap(), CellFormatting::default());
        state.switch("align.wrap");
        state.switch("align.wrap");
        assert_eq!(formatting(&state).unwrap(), CellFormatting { wrap: Some(false), ..Default::default() });
        state.switch("align.wrap");
        assert_eq!(formatting(&state).unwrap(), CellFormatting::default(), "greyed again leaves wrapping as it is");
        set(&mut state, "number.category", Value::Choice(Some("Percentage".into())));
        assert_eq!(formatting(&state).unwrap().number_format.as_deref(), Some("0.00%"));
    }
}
