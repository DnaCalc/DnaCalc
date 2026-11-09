//! The result's formatting and conditional formatting: what the archived DnaOneCalc showed in its
//! Formatting and Conditional Formatting panels, over the shared crates.
//!
//! The person authors a [`CellStyle`] (font, fill, borders, alignment: the result's own look) and a
//! list of [`CfRuleSpec`] rules. A rule becomes the [`calc_doc::ConditionalRule`] DNA Calc's grid
//! reads from a workbook, and [`calc_doc::look`] gives what it shows: the colour scale, data bar,
//! icon set, rank, average, duplicate and unique rules are the functions calc-host paints a sheet
//! with; a rule that holds a formula (`cellIs`, text, blank, error and date rules, `expression`)
//! is tested by the session's evaluator, the formula placed at each cell of the result as Excel
//! places a rule's formula at each cell of its range. A scalar result is a range of one cell and
//! an array result is a range of its own cells, as if the person had selected the spilled area.
//!
//! Nothing here computes the formula's value; it lays a look over a value that was calculated.

use crate::session::{InputValue, Session};
use calc_doc::look::{self, Applied, Appearance, CellLook, LookColor, RuleStats};
use calc_doc::{ColorScale, ConditionalRule, DataBar, IconSet, Threshold};
use calc_doc::model::StyleColor;
use calc_func::value::{CalcArray, CalcValue, CellKey, CoreRef};
use serde::{Deserialize, Serialize};

/// The most cells whose formula-held rules are tested; the aggregate rules cover every cell shown.
pub const MAX_TESTED_CELLS: usize = 5_000;

/// The most cells of the result placed in the session that tests a formula-held rule.
pub const MAX_PLACED_CELLS: usize = 20_000;

/// The Formatting panel: how the result is drawn. An empty text or a false flag leaves the
/// default.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellStyle {
    #[serde(default)]
    pub font_family: String,
    /// Points; 0 leaves the default.
    #[serde(default)]
    pub font_size: u32,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
    #[serde(default)]
    pub underline: bool,
    #[serde(default)]
    pub strike: bool,
    /// `#rrggbb` or empty.
    #[serde(default)]
    pub text_color: String,
    #[serde(default)]
    pub fill: String,
    /// `general`, `left`, `center` or `right`; empty is `general`.
    #[serde(default)]
    pub horizontal: String,
    #[serde(default)]
    pub wrap: bool,
    #[serde(default)]
    pub border_top: bool,
    #[serde(default)]
    pub border_bottom: bool,
    #[serde(default)]
    pub border_left: bool,
    #[serde(default)]
    pub border_right: bool,
    #[serde(default)]
    pub border_color: String,
}

impl CellStyle {
    /// Whether the style changes nothing.
    pub fn is_default(&self) -> bool {
        *self == CellStyle::default()
    }

    /// The CSS declarations that draw the style (a colour that is not `#rrggbb` is left out).
    pub fn css(&self) -> String {
        let mut css = String::new();
        let mut put = |name: &str, value: String| {
            css.push_str(name);
            css.push(':');
            css.push_str(&value);
            css.push(';');
        };
        let family: String = self
            .font_family
            .chars()
            .filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_'))
            .collect();
        if !family.trim().is_empty() {
            put("font-family", format!("\"{}\",sans-serif", family.trim()));
        }
        if self.font_size > 0 {
            put("font-size", format!("{}px", self.font_size.min(96) * 4 / 3));
        }
        if self.bold {
            put("font-weight", "700".into());
        }
        if self.italic {
            put("font-style", "italic".into());
        }
        let decoration = match (self.underline, self.strike) {
            (true, true) => "underline line-through",
            (true, false) => "underline",
            (false, true) => "line-through",
            (false, false) => "",
        };
        if !decoration.is_empty() {
            put("text-decoration", decoration.into());
        }
        if let Some(color) = hex_color(&self.text_color) {
            put("color", color);
        }
        if let Some(color) = hex_color(&self.fill) {
            put("background-color", color);
        }
        if matches!(self.horizontal.as_str(), "left" | "center" | "right") {
            put("text-align", self.horizontal.clone());
        }
        if self.wrap {
            put("white-space", "normal".into());
        }
        let border = format!("1px solid {}", hex_color(&self.border_color).unwrap_or_else(|| "#000000".into()));
        for (on, side) in [
            (self.border_top, "top"),
            (self.border_bottom, "bottom"),
            (self.border_left, "left"),
            (self.border_right, "right"),
        ] {
            if on {
                put(&format!("border-{side}"), border.clone());
            }
        }
        css
    }

    /// The style in a few words, for the effective display.
    pub fn describe(&self) -> Vec<String> {
        let mut lines = Vec::new();
        let mut font = Vec::new();
        if !self.font_family.trim().is_empty() {
            font.push(self.font_family.trim().to_owned());
        }
        if self.font_size > 0 {
            font.push(format!("{} pt", self.font_size));
        }
        for (on, name) in [
            (self.bold, "bold"),
            (self.italic, "italic"),
            (self.underline, "underline"),
            (self.strike, "strikethrough"),
        ] {
            if on {
                font.push(name.to_owned());
            }
        }
        if let Some(color) = hex_color(&self.text_color) {
            font.push(format!("colour {color}"));
        }
        if !font.is_empty() {
            lines.push(format!("Font: {}", font.join(", ")));
        }
        if let Some(color) = hex_color(&self.fill) {
            lines.push(format!("Fill: {color}"));
        }
        let borders: Vec<&str> = [
            (self.border_top, "top"),
            (self.border_bottom, "bottom"),
            (self.border_left, "left"),
            (self.border_right, "right"),
        ]
        .into_iter()
        .filter_map(|(on, name)| on.then_some(name))
        .collect();
        if !borders.is_empty() {
            lines.push(format!("Borders: {}", borders.join(", ")));
        }
        let mut alignment = Vec::new();
        if matches!(self.horizontal.as_str(), "left" | "center" | "right") {
            alignment.push(self.horizontal.clone());
        }
        if self.wrap {
            alignment.push("wrapped".to_owned());
        }
        if !alignment.is_empty() {
            lines.push(format!("Alignment: {}", alignment.join(", ")));
        }
        lines
    }
}

/// A `#rrggbb` colour, lower-cased; `None` for anything else.
pub fn hex_color(text: &str) -> Option<String> {
    let text = text.trim();
    let digits = text.strip_prefix('#')?;
    (digits.len() == 6 && digits.chars().all(|c| c.is_ascii_hexdigit()))
        .then(|| format!("#{}", digits.to_ascii_lowercase()))
}

impl CellStyle {
    /// The style as the session file's cell format.
    pub fn session_format(&self) -> calc_doc::SessionFormat {
        let color = |text: &str| hex_color(text).unwrap_or_default();
        calc_doc::SessionFormat {
            font_family: self.font_family.trim().to_owned(),
            font_size: self.font_size,
            bold: self.bold,
            italic: self.italic,
            underline: self.underline,
            strike: self.strike,
            text_color: color(&self.text_color),
            fill: color(&self.fill),
            horizontal: if matches!(self.horizontal.as_str(), "left" | "center" | "right") {
                self.horizontal.clone()
            } else {
                String::new()
            },
            wrap: self.wrap,
            border_top: self.border_top,
            border_bottom: self.border_bottom,
            border_left: self.border_left,
            border_right: self.border_right,
            border_color: color(&self.border_color),
        }
    }

    /// The style a file's cell format gives the panel.
    pub fn of_session_format(format: &calc_doc::SessionFormat) -> CellStyle {
        CellStyle {
            font_family: format.font_family.clone(),
            font_size: format.font_size,
            bold: format.bold,
            italic: format.italic,
            underline: format.underline,
            strike: format.strike,
            text_color: format.text_color.clone(),
            fill: format.fill.clone(),
            horizontal: format.horizontal.clone(),
            wrap: format.wrap,
            border_top: format.border_top,
            border_bottom: format.border_bottom,
            border_left: format.border_left,
            border_right: format.border_right,
            border_color: format.border_color.clone(),
        }
    }
}

/// `formula` with every relative A1 reference moved by (`rows`, `columns`): the formula a rule
/// holds, written for A1, as a file stores it for the range that starts `rows` below and
/// `columns` right of A1 (a negative amount moves it back). Text in quotes is left as it is, and
/// so are function names, defined names and whole-row and whole-column references.
pub fn shift_formula(formula: &str, rows: i64, columns: i64) -> Result<String, String> {
    let chars: Vec<char> = formula.chars().collect();
    let mut out = String::with_capacity(formula.len() + 8);
    let mut i = 0;
    let is_word = |c: char| c.is_alphanumeric() || matches!(c, '_' | '.');
    while i < chars.len() {
        let c = chars[i];
        if c == '"' || c == '\'' {
            // A text constant or a quoted sheet name: copied whole (a doubled quote stays inside).
            out.push(c);
            i += 1;
            while i < chars.len() {
                out.push(chars[i]);
                if chars[i] == c {
                    if chars.get(i + 1) == Some(&c) {
                        out.push(c);
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                i += 1;
            }
            continue;
        }
        let boundary = i == 0 || !is_word(chars[i - 1]);
        if boundary && (c == '$' || c.is_ascii_alphabetic()) {
            let mut j = i;
            let col_abs = chars[j] == '$';
            if col_abs {
                j += 1;
            }
            let letters_start = j;
            while j < chars.len() && chars[j].is_ascii_alphabetic() {
                j += 1;
            }
            let letters = &chars[letters_start..j];
            let mut k = j;
            let row_abs = chars.get(k) == Some(&'$');
            if row_abs {
                k += 1;
            }
            let digits_start = k;
            while k < chars.len() && chars[k].is_ascii_digit() {
                k += 1;
            }
            let digits = &chars[digits_start..k];
            let next_is_word = chars.get(k).is_some_and(|&n| is_word(n) || n == '(' || n == '!');
            if (1..=3).contains(&letters.len()) && !digits.is_empty() && !next_is_word {
                let column = letters
                    .iter()
                    .fold(0u64, |n, c| n * 26 + u64::from(c.to_ascii_uppercase() as u8 - b'A' + 1));
                let row: u64 = digits.iter().collect::<String>().parse().unwrap_or(u64::MAX);
                if (1..=16_384).contains(&column) && (1..=1_048_576).contains(&row) {
                    let column = if col_abs { column as i64 } else { column as i64 + columns };
                    let row = if row_abs { row as i64 } else { row as i64 + rows };
                    if !(1..=16_384).contains(&column) || !(1..=1_048_576).contains(&row) {
                        return Err(format!("a reference in {formula} would leave the sheet"));
                    }
                    if col_abs {
                        out.push('$');
                    }
                    out.push_str(&crate::file::column_letters(column as u32));
                    if row_abs {
                        out.push('$');
                    }
                    out.push_str(&row.to_string());
                    i = k;
                    continue;
                }
            }
            // Not a cell reference: copy the word whole.
            let mut w = i;
            while w < chars.len() && (is_word(chars[w]) || (w == i && chars[w] == '$')) {
                w += 1;
            }
            let end = w.max(i + 1);
            out.extend(&chars[i..end]);
            i = end;
            continue;
        }
        out.push(c);
        i += 1;
    }
    Ok(out)
}

/// One conditional-format rule as the Conditional Formatting panel holds it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CfRuleSpec {
    /// `cellIs`, `text`, `blanks`, `errors`, `dates`, `expression`, `rank`, `average`,
    /// `duplicates`, `uniques`, `colorScale`, `dataBar` or `iconSet`.
    pub kind: String,
    /// The comparison (`greaterThan`, ...), the text test (`containsText`, `notContainsText`,
    /// `beginsWith`, `endsWith`), `top` or `bottom` for a rank, `above` or `below` for an average,
    /// `blanks` or `noBlanks`, `errors` or `noErrors`, and the date period (`today`, ...).
    #[serde(default)]
    pub operator: String,
    /// A comparison's value or formula, a text, or an expression.
    #[serde(default)]
    pub operand1: String,
    /// The second value of `between` and `notBetween`.
    #[serde(default)]
    pub operand2: String,
    /// A rank's count.
    #[serde(default = "ten")]
    pub rank: u32,
    #[serde(default)]
    pub percent: bool,
    /// An average rule: equal to the average counts too.
    #[serde(default)]
    pub equal: bool,
    #[serde(default)]
    pub stop: bool,
    /// The look of a rule that tests a condition: colours `#rrggbb` or empty, and the format code.
    #[serde(default)]
    pub fill: String,
    #[serde(default)]
    pub font_color: String,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
    #[serde(default)]
    pub underline: bool,
    #[serde(default)]
    pub strike: bool,
    #[serde(default)]
    pub number_format: String,
    /// A colour scale: low, middle (empty for a two-colour scale) and high colours.
    #[serde(default = "scale_low")]
    pub low: String,
    #[serde(default = "scale_mid")]
    pub mid: String,
    #[serde(default = "scale_high")]
    pub high: String,
    /// A data bar's colour and whether it is a gradient.
    #[serde(default = "bar_color")]
    pub bar_color: String,
    #[serde(default = "yes")]
    pub gradient: bool,
    /// An icon set's name (`3Arrows`, ...) and whether its icons run in reverse.
    #[serde(default = "icon_set")]
    pub icon_set: String,
    #[serde(default)]
    pub reverse: bool,
}

fn ten() -> u32 {
    10
}
fn yes() -> bool {
    true
}
fn scale_low() -> String {
    "#f8696b".into()
}
fn scale_mid() -> String {
    "#ffeb84".into()
}
fn scale_high() -> String {
    "#63be7b".into()
}
fn bar_color() -> String {
    "#638ec6".into()
}
fn icon_set() -> String {
    "3Arrows".into()
}

impl CfRuleSpec {
    /// A new rule of `kind` with the defaults Excel's dialogs seed.
    pub fn new(kind: &str) -> CfRuleSpec {
        let mut spec = CfRuleSpec {
            kind: kind.to_owned(),
            operator: String::new(),
            operand1: String::new(),
            operand2: String::new(),
            rank: 10,
            percent: false,
            equal: false,
            stop: false,
            fill: String::new(),
            font_color: String::new(),
            bold: false,
            italic: false,
            underline: false,
            strike: false,
            number_format: String::new(),
            low: scale_low(),
            mid: scale_mid(),
            high: scale_high(),
            bar_color: bar_color(),
            gradient: true,
            icon_set: icon_set(),
            reverse: false,
        };
        spec.operator = match kind {
            "cellIs" => "greaterThan",
            "text" => "containsText",
            "blanks" => "blanks",
            "errors" => "errors",
            "dates" => "today",
            "rank" => "top",
            "average" => "above",
            _ => "",
        }
        .to_owned();
        if !matches!(kind, "colorScale" | "dataBar" | "iconSet") {
            // Excel's "Light Red Fill with Dark Red Text".
            spec.fill = "#ffc7ce".into();
            spec.font_color = "#9c0006".into();
        }
        spec
    }

    /// A few words naming the rule, for the panel and the effective display.
    pub fn title(&self) -> String {
        let o1 = self.operand1.trim();
        match self.kind.as_str() {
            "cellIs" => match self.operator.as_str() {
                "between" | "notBetween" => format!(
                    "Cell value {} {o1} and {}",
                    if self.operator == "between" { "between" } else { "not between" },
                    self.operand2.trim()
                ),
                op => format!(
                    "Cell value {} {o1}",
                    match op {
                        "lessThan" => "<",
                        "lessThanOrEqual" => "<=",
                        "equal" => "=",
                        "notEqual" => "<>",
                        "greaterThanOrEqual" => ">=",
                        _ => ">",
                    }
                ),
            },
            "text" => format!("Text {} \"{o1}\"", match self.operator.as_str() {
                "notContainsText" => "does not contain",
                "beginsWith" => "begins with",
                "endsWith" => "ends with",
                _ => "contains",
            }),
            "blanks" => if self.operator == "noBlanks" { "Cell is not blank" } else { "Cell is blank" }.to_owned(),
            "errors" => if self.operator == "noErrors" { "Cell has no error" } else { "Cell has an error" }.to_owned(),
            "dates" => format!("Date is {}", self.operator),
            "expression" => format!("Formula {o1}"),
            "rank" => format!(
                "{} {}{}",
                if self.operator == "bottom" { "Bottom" } else { "Top" },
                self.rank,
                if self.percent { "%" } else { "" }
            ),
            "average" => format!(
                "{} average{}",
                if self.operator == "below" { "Below" } else { "Above" },
                if self.equal { " or equal" } else { "" }
            ),
            "duplicates" => "Duplicate values".to_owned(),
            "uniques" => "Unique values".to_owned(),
            "colorScale" => "Colour scale".to_owned(),
            "dataBar" => "Data bar".to_owned(),
            "iconSet" => format!("Icon set {}", self.icon_set),
            other => other.to_owned(),
        }
    }

    /// The rule as DNA Calc's grid reads one, and the look it gives when it holds. A formula the
    /// rule needs but lacks is the error.
    pub fn rule(&self, priority: i64) -> Result<(ConditionalRule, Appearance), String> {
        let mut rule = ConditionalRule {
            unread: false,
            kind: String::new(),
            priority,
            stop_if_true: self.stop,
            dxf: None,
            operator: None,
            formulas: Vec::new(),
            text: None,
            rank: None,
            percent: false,
            bottom: false,
            above_average: true,
            equal_average: false,
            std_dev: None,
            time_period: None,
            color_scale: None,
            data_bar: None,
            icon_set: None,
            extension_id: None,
        };
        let need = |text: &str, what: &str| -> Result<String, String> {
            let text = text.trim();
            if text.is_empty() {
                Err(format!("{} needs {what}", self.title()))
            } else {
                Ok(text.trim_start_matches('=').to_owned())
            }
        };
        let quoted = |text: &str| format!("\"{}\"", text.replace('"', "\"\""));
        match self.kind.as_str() {
            "cellIs" => {
                rule.kind = "cellIs".into();
                rule.operator = Some(self.operator.clone());
                rule.formulas.push(need(&self.operand1, "a value")?);
                if matches!(self.operator.as_str(), "between" | "notBetween") {
                    rule.formulas.push(need(&self.operand2, "a second value")?);
                }
            }
            "text" => {
                let text = self.operand1.as_str();
                if text.is_empty() {
                    return Err(format!("{} needs a text", self.title()));
                }
                let q = quoted(text);
                rule.kind = self.operator.clone();
                rule.text = Some(text.to_owned());
                rule.formulas.push(match self.operator.as_str() {
                    "notContainsText" => format!("ISERROR(SEARCH({q},A1))"),
                    "beginsWith" => format!("LEFT(A1,LEN({q}))={q}"),
                    "endsWith" => format!("RIGHT(A1,LEN({q}))={q}"),
                    _ => {
                        rule.kind = "containsText".into();
                        format!("NOT(ISERROR(SEARCH({q},A1)))")
                    }
                });
            }
            "blanks" => {
                let not = self.operator == "noBlanks";
                rule.kind = if not { "notContainsBlanks" } else { "containsBlanks" }.into();
                rule.formulas.push(format!("LEN(TRIM(A1)){}0", if not { ">" } else { "=" }));
            }
            "errors" => {
                let not = self.operator == "noErrors";
                rule.kind = if not { "notContainsErrors" } else { "containsErrors" }.into();
                rule.formulas.push(if not { "NOT(ISERROR(A1))" } else { "ISERROR(A1)" }.into());
            }
            "dates" => {
                rule.kind = "timePeriod".into();
                rule.time_period = Some(self.operator.clone());
                rule.formulas.push(
                    match self.operator.as_str() {
                        "yesterday" => "FLOOR(A1,1)=TODAY()-1",
                        "tomorrow" => "FLOOR(A1,1)=TODAY()+1",
                        "last7Days" => "AND(TODAY()-FLOOR(A1,1)<=6,FLOOR(A1,1)<=TODAY())",
                        "thisWeek" => "AND(TODAY()-ROUNDDOWN(A1,0)<=WEEKDAY(TODAY())-1,ROUNDDOWN(A1,0)-TODAY()<=7-WEEKDAY(TODAY()))",
                        "lastWeek" => "AND(TODAY()-ROUNDDOWN(A1,0)>=(WEEKDAY(TODAY())),TODAY()-ROUNDDOWN(A1,0)<(WEEKDAY(TODAY())+7))",
                        "nextWeek" => "AND(ROUNDDOWN(A1,0)-TODAY()>(7-WEEKDAY(TODAY())),ROUNDDOWN(A1,0)-TODAY()<(15-WEEKDAY(TODAY())))",
                        "thisMonth" => "AND(MONTH(A1)=MONTH(TODAY()),YEAR(A1)=YEAR(TODAY()))",
                        "lastMonth" => "AND(MONTH(A1)=MONTH(EDATE(TODAY(),0-1)),YEAR(A1)=YEAR(EDATE(TODAY(),0-1)))",
                        "nextMonth" => "AND(MONTH(A1)=MONTH(EDATE(TODAY(),0+1)),YEAR(A1)=YEAR(EDATE(TODAY(),0+1)))",
                        _ => "FLOOR(A1,1)=TODAY()",
                    }
                    .to_owned(),
                );
            }
            "expression" => {
                rule.kind = "expression".into();
                rule.formulas.push(need(&self.operand1, "a formula")?);
            }
            "rank" => {
                if self.rank == 0 || (self.percent && self.rank > 100) {
                    return Err(format!("{} needs a count of 1 or more (at most 100 percent)", self.title()));
                }
                rule.kind = "top10".into();
                rule.rank = Some(self.rank);
                rule.percent = self.percent;
                rule.bottom = self.operator == "bottom";
            }
            "average" => {
                rule.kind = "aboveAverage".into();
                rule.above_average = self.operator != "below";
                rule.equal_average = self.equal;
            }
            "duplicates" => rule.kind = "duplicateValues".into(),
            "uniques" => rule.kind = "uniqueValues".into(),
            "colorScale" => {
                rule.kind = "colorScale".into();
                let mut thresholds = vec![threshold("min", None)];
                let mut colors = vec![self.low.as_str()];
                if hex_color(&self.mid).is_some() {
                    thresholds.push(threshold("percentile", Some("50")));
                    colors.push(self.mid.as_str());
                }
                thresholds.push(threshold("max", None));
                colors.push(self.high.as_str());
                let colors = colors
                    .into_iter()
                    .map(|c| style_color(c).ok_or_else(|| format!("{} needs colours as #rrggbb", self.title())))
                    .collect::<Result<Vec<_>, _>>()?;
                rule.color_scale = Some(ColorScale { thresholds, colors });
            }
            "dataBar" => {
                rule.kind = "dataBar".into();
                let color = style_color(&self.bar_color)
                    .ok_or_else(|| format!("{} needs a colour as #rrggbb", self.title()))?;
                rule.data_bar = Some(DataBar {
                    thresholds: vec![threshold("min", None), threshold("max", None)],
                    color,
                    min_length: 0,
                    max_length: 100,
                    gradient: self.gradient,
                    show_value: true,
                });
            }
            "iconSet" => {
                rule.kind = "iconSet".into();
                let count = self
                    .icon_set
                    .chars()
                    .next()
                    .and_then(|c| c.to_digit(10))
                    .filter(|n| (3..=5).contains(n))
                    .ok_or_else(|| format!("{} is not an icon set", self.icon_set))?
                    as usize;
                let thresholds = (0..count)
                    .map(|i| threshold("percent", Some(&((100 * i + count / 2) / count).to_string())))
                    .collect();
                rule.icon_set = Some(IconSet {
                    name: self.icon_set.clone(),
                    thresholds,
                    reverse: self.reverse,
                    show_value: true,
                });
            }
            other => return Err(format!("{other} is not a rule kind")),
        }
        let appearance = Appearance {
            fill: self.look_color(&self.fill),
            font_color: self.look_color(&self.font_color),
            bold: self.bold.then_some(true),
            italic: self.italic.then_some(true),
            underline: self.underline.then_some(true),
            strike: self.strike.then_some(true),
            number_format: (!self.number_format.trim().is_empty()).then(|| self.number_format.trim().to_owned()),
        };
        Ok((rule, appearance))
    }

    fn look_color(&self, text: &str) -> Option<LookColor> {
        hex_color(text).map(LookColor::Rgb)
    }

    /// The rule as the session file holds it, for the range that starts `rows` below and
    /// `columns` right of A1: its formulas moved there, its look as a differential format.
    pub fn session_rule(&self, priority: i64, rows: i64, columns: i64) -> Result<calc_doc::SessionRule, String> {
        let (mut rule, appearance) = self.rule(priority)?;
        for formula in &mut rule.formulas {
            *formula = shift_formula(formula, rows, columns)?;
        }
        let hex = |color: &Option<LookColor>| match color {
            Some(LookColor::Rgb(hex)) => Some(hex.clone()),
            _ => None,
        };
        let dxf = calc_doc::SessionDxf {
            fill: hex(&appearance.fill),
            font_color: hex(&appearance.font_color),
            bold: appearance.bold == Some(true),
            italic: appearance.italic == Some(true),
            underline: appearance.underline == Some(true),
            strike: appearance.strike == Some(true),
            number_format: appearance.number_format.clone(),
        };
        let visual = matches!(self.kind.as_str(), "colorScale" | "dataBar" | "iconSet");
        Ok(calc_doc::SessionRule { rule, dxf: (!visual).then_some(dxf) })
    }

    /// The panel's rule a file's rule is. `rows` and `columns` are the amounts the file's formulas
    /// are moved back by. A rule the panel cannot hold is refused with its reason.
    pub fn of_session_rule(rule: &calc_doc::SessionRule, rows: i64, columns: i64) -> Result<CfRuleSpec, String> {
        let r = &rule.rule;
        let back = |index: usize| -> Result<String, String> {
            r.formulas
                .get(index)
                .map(|f| shift_formula(f, -rows, -columns))
                .unwrap_or_else(|| Err(format!("the {} rule has no formula", r.kind)))
        };
        let refuse = |what: &str| Err(format!("a {} rule {what}", r.kind));
        let mut spec = CfRuleSpec::new(match r.kind.as_str() {
            "cellIs" => "cellIs",
            "containsText" | "notContainsText" | "beginsWith" | "endsWith" => "text",
            "containsBlanks" | "notContainsBlanks" => "blanks",
            "containsErrors" | "notContainsErrors" => "errors",
            "timePeriod" => "dates",
            "expression" => "expression",
            "top10" => "rank",
            "aboveAverage" => "average",
            "duplicateValues" => "duplicates",
            "uniqueValues" => "uniques",
            "colorScale" => "colorScale",
            "dataBar" => "dataBar",
            "iconSet" => "iconSet",
            other => return Err(format!("a {other} rule is not one the panel holds")),
        });
        spec.stop = r.stop_if_true;
        match r.kind.as_str() {
            "cellIs" => {
                spec.operator = r.operator.clone().unwrap_or_default();
                if !matches!(
                    spec.operator.as_str(),
                    "between" | "notBetween" | "equal" | "notEqual" | "greaterThan" | "lessThan"
                        | "greaterThanOrEqual" | "lessThanOrEqual"
                ) {
                    return refuse("with a comparison the panel does not offer");
                }
                spec.operand1 = back(0)?;
                if matches!(spec.operator.as_str(), "between" | "notBetween") {
                    spec.operand2 = back(1)?;
                }
            }
            "containsText" | "notContainsText" | "beginsWith" | "endsWith" => {
                spec.operator = r.kind.clone();
                spec.operand1 = r.text.clone().ok_or_else(|| format!("a {} rule has no text", r.kind))?;
            }
            "containsBlanks" | "notContainsBlanks" => {
                spec.operator = if r.kind == "containsBlanks" { "blanks" } else { "noBlanks" }.into()
            }
            "containsErrors" | "notContainsErrors" => {
                spec.operator = if r.kind == "containsErrors" { "errors" } else { "noErrors" }.into()
            }
            "timePeriod" => spec.operator = r.time_period.clone().unwrap_or_default(),
            "expression" => spec.operand1 = back(0)?,
            "top10" => {
                spec.operator = if r.bottom { "bottom" } else { "top" }.into();
                spec.rank = r.rank.unwrap_or(10);
                spec.percent = r.percent;
            }
            "aboveAverage" => {
                if r.std_dev.is_some_and(|n| n > 0) {
                    return refuse("by standard deviations");
                }
                spec.operator = if r.above_average { "above" } else { "below" }.into();
                spec.equal = r.equal_average;
            }
            "colorScale" => {
                let Some(scale) = &r.color_scale else { return refuse("without its scale") };
                let kinds: Vec<&str> = scale.thresholds.iter().map(|t| t.kind.as_str()).collect();
                let mid = scale.thresholds.get(1).and_then(|t| t.value.as_deref());
                let ok = match kinds.as_slice() {
                    ["min", "max"] => true,
                    ["min", "percentile", "max"] => mid == Some("50"),
                    _ => false,
                };
                if !ok || scale.colors.len() != scale.thresholds.len() {
                    return refuse("with thresholds other than lowest, 50th percentile and highest");
                }
                let color = |c: &StyleColor| {
                    c.rgb.as_deref().filter(|v| v.len() == 8).map(|v| format!("#{}", v[2..].to_ascii_lowercase()))
                };
                let colors: Option<Vec<String>> = scale.colors.iter().map(color).collect();
                let Some(colors) = colors else { return refuse("with a colour that is not RGB") };
                spec.low = colors[0].clone();
                spec.mid = if colors.len() == 3 { colors[1].clone() } else { String::new() };
                spec.high = colors[colors.len() - 1].clone();
            }
            "dataBar" => {
                let Some(bar) = &r.data_bar else { return refuse("without its bar") };
                let kinds: Vec<&str> = bar.thresholds.iter().map(|t| t.kind.as_str()).collect();
                if kinds != ["min", "max"] || bar.min_length != 0 || bar.max_length != 100 || !bar.show_value {
                    return refuse("with thresholds, lengths or a hidden value the panel does not offer");
                }
                let Some(hex) = bar.color.rgb.as_deref().filter(|v| v.len() == 8) else {
                    return refuse("with a colour that is not RGB");
                };
                spec.bar_color = format!("#{}", hex[2..].to_ascii_lowercase());
                spec.gradient = bar.gradient;
            }
            "iconSet" => {
                let Some(set) = &r.icon_set else { return refuse("without its icons") };
                let mine = CfRuleSpec { icon_set: set.name.clone(), ..CfRuleSpec::new("iconSet") };
                let (expected, _) = mine.rule(1).map_err(|e| format!("a {} rule: {e}", r.kind))?;
                let same = expected.icon_set.as_ref().is_some_and(|e| {
                    e.thresholds.len() == set.thresholds.len()
                        && e.thresholds.iter().zip(&set.thresholds).all(|(a, b)| a.kind == b.kind && a.value == b.value && a.gte == b.gte)
                });
                if !same || !set.show_value {
                    return refuse("with thresholds or a hidden value the panel does not offer");
                }
                spec.icon_set = set.name.clone();
                spec.reverse = set.reverse;
            }
            _ => {}
        }
        if let Some(dxf) = &rule.dxf {
            spec.fill = dxf.fill.clone().unwrap_or_default();
            spec.font_color = dxf.font_color.clone().unwrap_or_default();
            spec.bold = dxf.bold;
            spec.italic = dxf.italic;
            spec.underline = dxf.underline;
            spec.strike = dxf.strike;
            spec.number_format = dxf.number_format.clone().unwrap_or_default();
        } else if !matches!(spec.kind.as_str(), "colorScale" | "dataBar" | "iconSet") {
            spec.fill.clear();
            spec.font_color.clear();
        }
        Ok(spec)
    }
}

fn threshold(kind: &str, value: Option<&str>) -> Threshold {
    Threshold {
        kind: kind.to_owned(),
        value: value.map(str::to_owned),
        gte: true,
    }
}

fn style_color(text: &str) -> Option<StyleColor> {
    let hex = hex_color(text)?;
    Some(StyleColor {
        rgb: Some(format!("FF{}", &hex[1..])),
        indexed: None,
        theme: None,
        tint: None,
        auto: false,
    })
}

/// The glyph and colour of an icon of a set. Unicode stand-ins for Excel's artwork, as the archived
/// DnaOneCalc used.
pub fn icon_glyph(set: &str, index: usize) -> (&'static str, &'static str) {
    let count = set.chars().next().and_then(|c| c.to_digit(10)).unwrap_or(3) as usize;
    let index = index.min(count.saturating_sub(1));
    const RED: &str = "#d03030";
    const AMBER: &str = "#e0a000";
    const GREEN: &str = "#2f9e44";
    const GREY: &str = "#707070";
    let three = |a: [(&'static str, &'static str); 3]| a[index.min(2)];
    if set.contains("Arrows") {
        let all = [("\u{2193}", RED), ("\u{2198}", AMBER), ("\u{2192}", AMBER), ("\u{2197}", GREEN), ("\u{2191}", GREEN)];
        let grey = set.contains("Gray");
        let pick = match count {
            3 => [0, 2, 4][index.min(2)],
            4 => [0, 1, 3, 4][index.min(3)],
            _ => index.min(4),
        };
        let (glyph, color) = all[pick];
        return (glyph, if grey { GREY } else { color });
    }
    if set.contains("TrafficLights") || set.contains("RedToBlack") {
        if count == 4 {
            return [("\u{25CF}", "#202020"), ("\u{25CF}", RED), ("\u{25CF}", AMBER), ("\u{25CF}", GREEN)][index.min(3)];
        }
        return three([("\u{25CF}", RED), ("\u{25CF}", AMBER), ("\u{25CF}", GREEN)]);
    }
    if set.contains("Signs") {
        return three([("\u{25C6}", RED), ("\u{25B2}", AMBER), ("\u{25CF}", GREEN)]);
    }
    if set.contains("Symbols") {
        return three([("\u{2717}", RED), ("!", AMBER), ("\u{2713}", GREEN)]);
    }
    if set.contains("Flags") {
        return three([("\u{2691}", RED), ("\u{2691}", AMBER), ("\u{2691}", GREEN)]);
    }
    if set.contains("Quarters") {
        return [("\u{25CB}", GREY), ("\u{25D4}", GREY), ("\u{25D1}", GREY), ("\u{25D5}", GREY), ("\u{25CF}", GREY)][index.min(4)];
    }
    // Ratings: bars of growing height.
    [("\u{2581}", GREY), ("\u{2583}", GREY), ("\u{2585}", GREY), ("\u{2587}", GREY), ("\u{2588}", GREY)][index.min(4)]
}

/// What the rules gave a result: a look for each cell of its window, and a line for each rule.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Outcome {
    /// Row by row over the window; an empty look is a cell no rule changed.
    pub looks: Vec<Vec<CellLook>>,
    /// One line per rule: what it did, or why it did nothing.
    pub notes: Vec<String>,
    /// How many cells at least one rule changed.
    pub changed: usize,
    /// How many tests were refused (a formula that did not compile or evaluate).
    pub refused: usize,
    /// Why the first one was.
    pub first_refusal: Option<String>,
}

/// One rule prepared over the result's cells.
struct Prepared {
    title: String,
    rule: ConditionalRule,
    appearance: Appearance,
    stats: Option<RuleStats>,
    average: Option<Result<f64, ()>>,
    /// The formula tested at each cell, written for A1.
    condition: Option<String>,
    held: usize,
}

/// Applies `specs` to the cells `rows` x `cols` from the top left of `array` (the result's
/// window), ranges over all of `array`. `session` supplies the evaluator and the clock for the
/// formula-held rules: a scratch copy of its environment, with the result's cells placed from A1.
pub fn evaluate(
    specs: &[CfRuleSpec],
    array: &CalcArray,
    rows: usize,
    cols: usize,
    environment: Option<calc_fml::CalcEnvironment>,
) -> Outcome {
    let shape = array.shape();
    let mut outcome = Outcome {
        looks: vec![vec![CellLook::default(); cols]; rows],
        ..Outcome::default()
    };
    let mut prepared = Vec::new();
    let mut order: Vec<(i64, usize)> = Vec::new();
    for (index, spec) in specs.iter().enumerate() {
        match spec.rule(index as i64 + 1) {
            Ok((rule, appearance)) => {
                order.push((rule.priority, prepared.len()));
                prepared.push(Prepared {
                    title: format!("{}. {}", index + 1, spec.title()),
                    rule,
                    appearance,
                    stats: None,
                    average: None,
                    condition: None,
                    held: 0,
                });
            }
            Err(why) => outcome.notes.push(format!("{}. {why}; the rule is not applied", index + 1)),
        }
    }
    if prepared.is_empty() {
        return outcome;
    }
    // The range statistics, over every cell of the result.
    let needs_stats = |p: &Prepared| look::is_aggregate(&p.rule.kind) && p.rule.kind != "aboveAverage";
    let mut stats = RuleStats::default();
    if prepared.iter().any(needs_stats) {
        for r in 0..shape.rows {
            for c in 0..shape.cols {
                if let Some(value) = array.get(r, c) {
                    stats.add(value);
                }
            }
        }
        stats.finish();
    }
    // The session that tests formulas: the result's cells from A1.
    let placed = shape.rows * shape.cols <= MAX_PLACED_CELLS;
    let mut scratch = None;
    let formula_rules = prepared.iter().any(|p| !look::is_aggregate(&p.rule.kind) || p.rule.kind == "aboveAverage");
    if formula_rules && placed {
        let mut session = Session::new("Sheet1").expect("Sheet1 is a sheet name");
        session.set_environment(environment);
        // The formula cell is outside the placed cells: the evaluator never reads its own cell.
        let _ = session.set_formula_cell(&look::a1(shape.rows as u32 + 2, shape.cols as u32 + 2));
        let first_sheet = session.sheets().next().map(|(_, sheet)| sheet);
        if let Some(sheet) = first_sheet {
            for r in 0..shape.rows {
                for c in 0..shape.cols {
                    let Some(value) = array.get(r, c) else { continue };
                    let Ok(cell) = CellKey::new(sheet, r as u32 + 1, c as u32 + 1) else { continue };
                    let _ = session.set_input_at(cell, input_of(value));
                }
            }
        }
        scratch = Some(session);
    }
    let range = format!("$A$1:${}${}", look::a1(1, shape.cols as u32).trim_end_matches(char::is_numeric), shape.rows);
    for p in &mut prepared {
        match p.rule.kind.as_str() {
            "colorScale" | "dataBar" | "iconSet" | "top10" | "duplicateValues" | "uniqueValues" => {
                p.stats = Some(stats.clone());
            }
            "aboveAverage" => {
                p.average = Some(match scratch.as_mut() {
                    Some(session) => {
                        session.set_formula_cell("A1").ok();
                        let result = session.calculate_text(&format!("={}", look::average_formula(&p.rule, &range)));
                        match result.result.as_ref().and_then(look::number_of) {
                            Some(n) => Ok(n),
                            None => Err(()),
                        }
                    }
                    None => Err(()),
                });
            }
            "cellIs" => p.condition = look::condition_formula(&p.rule, "A1"),
            _ => p.condition = p.rule.formulas.first().cloned(),
        }
    }
    order.sort();
    let mut tested = 0usize;
    let mut skipped = false;
    for r in 0..rows {
        for c in 0..cols {
            let Some(value) = array.get(r, c) else { continue };
            let mut cell_look = CellLook::default();
            for &(_, i) in &order {
                let p = &mut prepared[i];
                let held = if look::is_aggregate(&p.rule.kind) {
                    let applied = look::apply_aggregate(
                        &p.rule,
                        &p.appearance,
                        p.stats.as_ref(),
                        p.average.clone(),
                        Some(value),
                        &[],
                        &|_| None,
                        &mut cell_look,
                    );
                    match applied {
                        Some(Applied::Held) => true,
                        Some(Applied::Refused) => {
                            outcome.refused += 1;
                            false
                        }
                        _ => false,
                    }
                } else {
                    match (&p.condition, scratch.as_mut()) {
                        (Some(text), Some(session)) if tested < MAX_TESTED_CELLS => {
                            // The formula is written for A1 and tested for the cell (r, c), whose
                            // value the session holds: it is entered in a cell beside the result,
                            // shifted by a whole number of columns (ROW() is then the cell's own) or
                            // of rows when the formula asks for COLUMN() only, and compiled for A1
                            // shifted the same way, so that its relative references land on (r, c).
                            let upper = text.to_ascii_uppercase();
                            let by_rows = upper.contains("COLUMN(") && !upper.contains("ROW(");
                            let (shift_rows, shift_cols) = if by_rows {
                                (shape.rows as u32 + 1, 0)
                            } else {
                                (0, shape.cols as u32 + 1)
                            };
                            let at = look::a1(r as u32 + 1 + shift_rows, c as u32 + 1 + shift_cols);
                            let origin = look::a1(1 + shift_rows, 1 + shift_cols);
                            if session.set_formula_cell(&at).is_err() {
                                false
                            } else {
                                tested += 1;
                                let result = session.calculate_relative(&format!("={text}"), &origin);
                                match result.result {
                                    Some(value) if look::truth(&value) => look::dress(&mut cell_look, &p.appearance),
                                    Some(_) => false,
                                    None => {
                                        outcome.refused += 1;
                                        if outcome.first_refusal.is_none() {
                                            outcome.first_refusal = result
                                                .diagnostics
                                                .first()
                                                .map(|d| format!("={text}: {}", crate::display::diagnostic(session, d)));
                                        }
                                        false
                                    }
                                }
                            }
                        }
                        (Some(_), Some(_)) => {
                            skipped = true;
                            false
                        }
                        _ => false,
                    }
                };
                if held {
                    p.held += 1;
                    if p.rule.stop_if_true {
                        break;
                    }
                }
            }
            if !cell_look.is_empty() {
                outcome.changed += 1;
            }
            outcome.looks[r][c] = cell_look;
        }
    }
    let total = rows * cols;
    for p in &prepared {
        let note = if formula_rule(&p.rule) && scratch.is_none() {
            format!(
                "{}: not applied, the result has more than {MAX_PLACED_CELLS} cells and a formula rule is tested against placed cells",
                p.title
            )
        } else {
            format!("{}: holds for {} of {total} cell(s)", p.title, p.held)
        };
        outcome.notes.push(note);
    }
    if skipped {
        outcome.notes.push(format!(
            "Formula rules were tested for the first {MAX_TESTED_CELLS} cells of the result only."
        ));
    }
    if outcome.refused > 0 {
        outcome.notes.push(format!(
            "{} test(s) could not be calculated and did not hold; a rule formula may be wrong{}.",
            outcome.refused,
            outcome.first_refusal.as_ref().map(|why| format!(" ({why})")).unwrap_or_default()
        ));
    }
    outcome
}

fn formula_rule(rule: &ConditionalRule) -> bool {
    !look::is_aggregate(&rule.kind) || rule.kind == "aboveAverage"
}

/// A result cell as the session's input for it.
fn input_of(value: &CalcValue) -> InputValue {
    match value.core_ref() {
        CoreRef::Number(n) => InputValue::number(n).unwrap_or_else(|_| InputValue::blank()),
        CoreRef::Text(t) => InputValue::text(&t.to_string_lossy()).unwrap_or_else(|_| InputValue::blank()),
        CoreRef::Logical(b) => InputValue::logical(b),
        CoreRef::Error(e) => InputValue::error(e),
        _ => InputValue::blank(),
    }
}

/// How one result cell is drawn beyond its text: what the rules, the number format's colour and
/// the Formatting panel give it, as plain data that crosses a port.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LookWire {
    /// `#rrggbb` or empty.
    #[serde(default)]
    pub fill: String,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
    #[serde(default)]
    pub underline: bool,
    #[serde(default)]
    pub strike: bool,
    /// A data bar: the share of the cell's width it fills (0 to 1).
    #[serde(default)]
    pub bar: Option<f64>,
    #[serde(default)]
    pub bar_color: String,
    #[serde(default)]
    pub gradient: bool,
    /// An icon: its set, its index in the set, its glyph and its colour.
    #[serde(default)]
    pub icon_set: String,
    #[serde(default)]
    pub icon_index: Option<usize>,
    #[serde(default)]
    pub icon_glyph: String,
    #[serde(default)]
    pub icon_color: String,
    /// The text a number format a rule gave shows in place of the cell's own.
    #[serde(default)]
    pub text: Option<String>,
}

impl LookWire {
    /// The wire form of a look, with the colour a number format gave the text (`format_color`) in
    /// place of the rules' font colour, as Excel's number-format colours take precedence over the
    /// font's. `None` for a cell nothing changes.
    pub fn of(look: &CellLook, format_color: Option<&str>, text: Option<String>) -> Option<LookWire> {
        let rgb = |color: &Option<LookColor>| match color {
            Some(LookColor::Rgb(text)) => text.clone(),
            _ => String::new(),
        };
        let mut wire = LookWire {
            fill: rgb(&look.fill),
            color: rgb(&look.font_color),
            bold: look.bold.unwrap_or(false),
            italic: look.italic.unwrap_or(false),
            underline: look.underline.unwrap_or(false),
            strike: look.strike.unwrap_or(false),
            text,
            ..LookWire::default()
        };
        if let Some(color) = format_color {
            wire.color = color.to_owned();
        }
        if let Some(bar) = &look.bar {
            wire.bar = Some(bar.fraction.clamp(0.0, 1.0));
            wire.bar_color = match &bar.color {
                Some(LookColor::Rgb(text)) => text.clone(),
                _ => "#638ec6".to_owned(),
            };
            wire.gradient = bar.gradient;
        }
        if let Some(icon) = &look.icon {
            let (glyph, color) = icon_glyph(&icon.set, icon.index);
            wire.icon_set = icon.set.clone();
            wire.icon_index = Some(icon.index);
            wire.icon_glyph = glyph.to_owned();
            wire.icon_color = color.to_owned();
        }
        (wire != LookWire::default()).then_some(wire)
    }

    /// The CSS declarations that draw the look.
    pub fn css(&self) -> String {
        let mut css = String::new();
        if !self.fill.is_empty() {
            css.push_str(&format!("background-color:{};", self.fill));
        }
        if !self.color.is_empty() {
            css.push_str(&format!("color:{};", self.color));
        }
        if self.bold {
            css.push_str("font-weight:700;");
        }
        if self.italic {
            css.push_str("font-style:italic;");
        }
        match (self.underline, self.strike) {
            (true, true) => css.push_str("text-decoration:underline line-through;"),
            (true, false) => css.push_str("text-decoration:underline;"),
            (false, true) => css.push_str("text-decoration:line-through;"),
            (false, false) => {}
        }
        if let Some(fraction) = self.bar {
            let percent = (fraction * 1000.0).round() / 10.0;
            let color = &self.bar_color;
            let paint = if self.gradient {
                format!("linear-gradient(to right,{color},{color}33)")
            } else {
                format!("linear-gradient({color},{color})")
            };
            css.push_str(&format!(
                "background-image:{paint};background-size:{percent}% 100%;background-repeat:no-repeat;"
            ));
        }
        css
    }

    /// The look in a few words, for the effective display and the smoke checks.
    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        if !self.fill.is_empty() {
            parts.push(format!("fill {}", self.fill));
        }
        if !self.color.is_empty() {
            parts.push(format!("font colour {}", self.color));
        }
        for (on, name) in [
            (self.bold, "bold"),
            (self.italic, "italic"),
            (self.underline, "underline"),
            (self.strike, "strikethrough"),
        ] {
            if on {
                parts.push(name.to_owned());
            }
        }
        if let Some(bar) = self.bar {
            parts.push(format!("data bar {:.0}%", bar * 100.0));
        }
        if let Some(index) = self.icon_index {
            parts.push(format!("icon {} of {}", index + 1, self.icon_set));
        }
        if let Some(text) = &self.text {
            parts.push(format!("shown as {text}"));
        }
        parts.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_style_draws_as_css_and_a_bad_colour_is_left_out() {
        let style = CellStyle {
            bold: true,
            fill: "#FFC7CE".into(),
            text_color: "red".into(),
            border_bottom: true,
            horizontal: "right".into(),
            ..CellStyle::default()
        };
        let css = style.css();
        assert!(css.contains("font-weight:700;"));
        assert!(css.contains("background-color:#ffc7ce;"));
        assert!(!css.contains("color:red"));
        assert!(css.contains("border-bottom:1px solid #000000;"));
        assert!(css.contains("text-align:right;"));
        assert!(CellStyle::default().is_default());
    }
}
