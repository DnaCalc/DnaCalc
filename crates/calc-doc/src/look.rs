//! What a conditional-format rule gives a value (dc-6yw), the part of the evaluation that needs no
//! workbook: the look types, the colour arithmetic, the statistics a rule reads over its range and
//! the aggregate rules (colour scale, data bar, icon set, top and bottom, above and below average,
//! duplicate and unique values).
//!
//! calc-host applies these to a sheet's cells through the engine's detached formulas, DNA Calc's
//! grid painting them; DNA OneCalc applies the same functions to the cells of a formula's result.
//! Moved out of calc-host's `conditional` module so that OneCalc, which depends on neither
//! calc-host nor calc-engine, runs the code DNA Calc runs. Rules that hold a formula (a `cellIs`
//! comparison, text, blank, error and date rules, `expression`) are tested by the caller with its
//! own evaluator; [`condition_formula`] gives the formula text calc-host writes for a `cellIs` rule.
//!
//! Colours: an `rgb` colour as it is; a theme colour from the workbook's theme with its tint
//! applied as Windows' HLS arithmetic does (luminance in 0 to 240, measured on Excel 16.0.20430:
//! accent1 156082 at tint 0.6 shows as 83CCEB); an indexed colour by its index, for the caller's
//! palette ([`LookColor::Indexed`]).

use crate::{ConditionalRule, Threshold};
use calc_doc_model::StyleColor;
use calc_doc_model::value::{CalcValue, CoreRef};
use std::collections::BTreeMap;

/// A colour of a look: `#rrggbb`, or an index into the workbook's palette (`indexed`), which the
/// caller resolves with the palette it shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LookColor {
    Rgb(String),
    Indexed(u32),
}

/// A data bar in a cell: how much of the cell's width it fills (0 to 1), its colour, whether it
/// is a gradient, and whether the cell's value is shown beside it.
#[derive(Clone, Debug, PartialEq)]
pub struct BarLook {
    pub fraction: f64,
    pub color: Option<LookColor>,
    pub gradient: bool,
    pub show_value: bool,
}

/// An icon of an icon set: the set's name (`3Arrows`, ...), the icon's index in it (0 is the
/// lowest band's), and whether the value is shown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IconLook {
    pub set: String,
    pub index: usize,
    pub show_value: bool,
}

/// What a cell's conditional formats give it; a part left `None` is the cell's own.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CellLook {
    pub fill: Option<LookColor>,
    pub font_color: Option<LookColor>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub strike: Option<bool>,
    pub number_format: Option<String>,
    pub bar: Option<BarLook>,
    pub icon: Option<IconLook>,
}

impl CellLook {
    /// Whether no rule changed anything.
    pub fn is_empty(&self) -> bool {
        *self == CellLook::default()
    }
}

/// A differential format's look (a rule's `dxfId`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Appearance {
    pub fill: Option<LookColor>,
    pub font_color: Option<LookColor>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub strike: Option<bool>,
    pub number_format: Option<String>,
}

/// What a rule reads over its whole areas.
#[derive(Clone, Debug, Default)]
pub struct RuleStats {
    /// The numbers, ascending.
    pub numbers: Vec<f64>,
    /// How often each value occurs (blanks left out), by a key equal for values Excel takes as
    /// duplicates.
    pub counts: BTreeMap<String, usize>,
}

impl RuleStats {
    /// Counts one value of the range.
    pub fn add(&mut self, value: &CalcValue) {
        if let Some(n) = number_of(value) {
            self.numbers.push(n);
        }
        if let Some(key) = value_key(value) {
            *self.counts.entry(key).or_default() += 1;
        }
    }

    /// Orders the numbers; call once after the last [`RuleStats::add`].
    pub fn finish(&mut self) {
        self.numbers.sort_by(f64::total_cmp);
    }
}

/// Whether a rule of this `type` is one [`apply_aggregate`] decides from the range's statistics.
pub fn is_aggregate(kind: &str) -> bool {
    matches!(
        kind,
        "colorScale"
            | "dataBar"
            | "iconSet"
            | "top10"
            | "aboveAverage"
            | "duplicateValues"
            | "uniqueValues"
    )
}

/// How an aggregate rule fared on one value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Applied {
    /// The rule holds for the value; the look carries what it gave.
    Held,
    /// The rule does not hold (or the value or a threshold is not a number).
    NotHeld,
    /// The rule's bound could not be calculated; it does not hold and the caller counts it.
    Refused,
}

/// Applies one aggregate rule to `value`, setting what `look` leaves unset. `None` when the rule is
/// not an aggregate one ([`is_aggregate`]). `stats` are the range's; `average` is the bound of an
/// `aboveAverage` rule, calculated by the caller; `formula_threshold` gives the number of the
/// `index`th threshold whose value is a formula or a number written as one.
pub fn apply_aggregate(
    rule: &ConditionalRule,
    appearance: &Appearance,
    stats: Option<&RuleStats>,
    average: Option<Result<f64, ()>>,
    value: Option<&CalcValue>,
    theme: &[String],
    formula_threshold: &dyn Fn(usize) -> Option<f64>,
    look: &mut CellLook,
) -> Option<Applied> {
    let number = value.and_then(number_of);
    let held = |held: bool| {
        if held {
            Applied::Held
        } else {
            Applied::NotHeld
        }
    };
    Some(match rule.kind.as_str() {
        "colorScale" => {
            let (Some(scale), Some(n), Some(stats)) = (rule.color_scale.as_ref(), number, stats)
            else {
                return Some(Applied::NotHeld);
            };
            let points: Vec<f64> = scale
                .thresholds
                .iter()
                .enumerate()
                .filter_map(|(i, t)| threshold_value(t, stats, || formula_threshold(i)))
                .collect();
            let colors: Vec<Option<(u8, u8, u8)>> =
                scale.colors.iter().map(|c| rgb_triple(c, theme)).collect();
            if points.len() != scale.thresholds.len() || colors.len() < points.len() {
                return Some(Applied::NotHeld);
            }
            let Some(color) = interpolate(&points, &colors, n) else {
                return Some(Applied::NotHeld);
            };
            if look.fill.is_none() {
                look.fill = Some(LookColor::Rgb(hex(color)));
            }
            Applied::Held
        }
        "dataBar" => {
            let (Some(bar), Some(n), Some(stats)) = (rule.data_bar.as_ref(), number, stats) else {
                return Some(Applied::NotHeld);
            };
            let lo = bar
                .thresholds
                .first()
                .and_then(|t| threshold_value(t, stats, || formula_threshold(0)));
            let hi = bar
                .thresholds
                .get(1)
                .and_then(|t| threshold_value(t, stats, || formula_threshold(1)));
            let (Some(lo), Some(hi)) = (lo, hi) else {
                return Some(Applied::NotHeld);
            };
            let share = if hi > lo {
                ((n - lo) / (hi - lo)).clamp(0.0, 1.0)
            } else {
                1.0
            };
            let (min, max) = (
                f64::from(bar.min_length) / 100.0,
                f64::from(bar.max_length) / 100.0,
            );
            if look.bar.is_none() {
                look.bar = Some(BarLook {
                    fraction: min + (max - min) * share,
                    color: resolve_color(&bar.color, theme),
                    gradient: bar.gradient,
                    show_value: bar.show_value,
                });
            }
            Applied::Held
        }
        "iconSet" => {
            let (Some(set), Some(n), Some(stats)) = (rule.icon_set.as_ref(), number, stats) else {
                return Some(Applied::NotHeld);
            };
            let mut index = 0;
            for (i, t) in set.thresholds.iter().enumerate().skip(1) {
                let Some(limit) = threshold_value(t, stats, || formula_threshold(i)) else {
                    return Some(Applied::NotHeld);
                };
                if n > limit || (t.gte && n == limit) {
                    index = i;
                }
            }
            if set.reverse {
                index = set.thresholds.len().saturating_sub(1) - index;
            }
            if look.icon.is_none() {
                look.icon = Some(IconLook {
                    set: set.name.clone(),
                    index,
                    show_value: set.show_value,
                });
            }
            Applied::Held
        }
        "top10" => {
            let (Some(n), Some(stats)) = (number, stats) else {
                return Some(Applied::NotHeld);
            };
            let count = stats.numbers.len();
            let rank = rule.rank.unwrap_or(10) as usize;
            let take = if rule.percent {
                (count * rank / 100).max(1)
            } else {
                rank.max(1)
            }
            .min(count);
            if take == 0 {
                return Some(Applied::NotHeld);
            }
            let in_rank = if rule.bottom {
                n <= stats.numbers[take - 1]
            } else {
                n >= stats.numbers[count - take]
            };
            held(in_rank && dress(look, appearance))
        }
        "aboveAverage" => {
            let Some(n) = number else {
                return Some(Applied::NotHeld);
            };
            let Some(Ok(bound)) = average else {
                return Some(Applied::Refused);
            };
            let passes = match (rule.above_average, rule.equal_average) {
                (true, false) => n > bound,
                (true, true) => n >= bound,
                (false, false) => n < bound,
                (false, true) => n <= bound,
            };
            held(passes && dress(look, appearance))
        }
        "duplicateValues" | "uniqueValues" => {
            let (Some(value), Some(stats)) = (value, stats) else {
                return Some(Applied::NotHeld);
            };
            let Some(key) = value_key(value) else {
                return Some(Applied::NotHeld);
            };
            let count = stats.counts.get(&key).copied().unwrap_or(0);
            let duplicate = if rule.kind == "duplicateValues" {
                count > 1
            } else {
                count == 1
            };
            held(duplicate && dress(look, appearance))
        }
        _ => return None,
    })
}

/// The number a threshold stands for over the rule's statistics; `formula` gives the number of a
/// threshold written as a formula (or a `num` that is not a number).
pub fn threshold_value(
    threshold: &Threshold,
    stats: &RuleStats,
    formula: impl Fn() -> Option<f64>,
) -> Option<f64> {
    let (lo, hi) = (*stats.numbers.first()?, *stats.numbers.last()?);
    let given = || {
        threshold
            .value
            .as_deref()
            .and_then(|v| v.trim().parse::<f64>().ok())
    };
    match threshold.kind.as_str() {
        "min" | "autoMin" => Some(lo),
        "max" | "autoMax" => Some(hi),
        "num" => given().or_else(formula),
        "percent" => Some(lo + (hi - lo) * given()? / 100.0),
        "percentile" => Some(percentile(&stats.numbers, given()? / 100.0)),
        "formula" => formula(),
        _ => None,
    }
}

/// Sets what the differential format gives and the look leaves unset; true.
pub fn dress(look: &mut CellLook, appearance: &Appearance) -> bool {
    fn set<T: Clone>(slot: &mut Option<T>, value: &Option<T>) {
        if slot.is_none() {
            slot.clone_from(value);
        }
    }
    set(&mut look.fill, &appearance.fill);
    set(&mut look.font_color, &appearance.font_color);
    set(&mut look.bold, &appearance.bold);
    set(&mut look.italic, &appearance.italic);
    set(&mut look.underline, &appearance.underline);
    set(&mut look.strike, &appearance.strike);
    set(&mut look.number_format, &appearance.number_format);
    true
}

/// Whether a formula's value holds: TRUE, or a number that is not 0.
pub fn truth(value: &CalcValue) -> bool {
    match value.core_ref() {
        CoreRef::Logical(b) => b,
        CoreRef::Number(n) => n != 0.0,
        _ => false,
    }
}

/// A value's number, when it is one.
pub fn number_of(value: &CalcValue) -> Option<f64> {
    match value.core_ref() {
        CoreRef::Number(n) => Some(n),
        _ => None,
    }
}

/// The key under which Excel takes two values as duplicates: a number by its value, a text
/// regardless of case, a logical, an error; `None` for a blank.
pub fn value_key(value: &CalcValue) -> Option<String> {
    match value.core_ref() {
        CoreRef::Number(n) => Some(format!("n{n:?}")),
        CoreRef::Text(t) => Some(format!("t{}", t.to_string_lossy().to_lowercase())),
        CoreRef::Logical(b) => Some(format!("b{b}")),
        CoreRef::Error(e) => Some(format!("e{e:?}")),
        _ => None,
    }
}

/// PERCENTILE.INC of ascending `numbers` at `k` (0 to 1).
pub fn percentile(numbers: &[f64], k: f64) -> f64 {
    let k = k.clamp(0.0, 1.0);
    let position = k * (numbers.len() - 1) as f64;
    let below = position.floor() as usize;
    let above = (below + 1).min(numbers.len() - 1);
    numbers[below] + (numbers[above] - numbers[below]) * (position - below as f64)
}

/// The colour a scale gives `n`: its position over the whole scale (from the first threshold to
/// the last), then its place between the thresholds around it, each channel moved from the lower
/// colour toward the upper by that share and truncated toward the lower. Fitted to Excel
/// 16.0.20430's colours for all twenty cells of the fixture's two scales (cf-excel.txt, E and F),
/// where rounding misses eleven and truncating the share within the segment misses two.
pub fn interpolate(
    points: &[f64],
    colors: &[Option<(u8, u8, u8)>],
    n: f64,
) -> Option<(u8, u8, u8)> {
    let last = points.len() - 1;
    let (lo, hi) = (points[0], points[last]);
    if n <= lo {
        return colors[0];
    }
    if n >= hi {
        return colors[last];
    }
    let position = (n - lo) / (hi - lo);
    let marks: Vec<f64> = points.iter().map(|x| (x - lo) / (hi - lo)).collect();
    let i = (0..last).find(|i| position >= marks[*i] && position <= marks[i + 1])?;
    let (a, b) = (colors[i]?, colors[i + 1]?);
    let span = marks[i + 1] - marks[i];
    let t = if span > 0.0 {
        (position - marks[i]) / span
    } else {
        0.0
    };
    let mix = |x: u8, y: u8| (f64::from(x) + ((f64::from(y) - f64::from(x)) * t).trunc()) as u8;
    Some((mix(a.0, b.0), mix(a.1, b.1), mix(a.2, b.2)))
}

/// `#rrggbb`.
pub fn hex((r, g, b): (u8, u8, u8)) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// A colour as an RGB triple, when it is an rgb or theme colour.
pub fn rgb_triple(color: &StyleColor, theme: &[String]) -> Option<(u8, u8, u8)> {
    match resolve_color(color, theme)? {
        LookColor::Rgb(text) => {
            let n = u32::from_str_radix(text.trim_start_matches('#'), 16).ok()?;
            Some(((n >> 16) as u8, (n >> 8) as u8, n as u8))
        }
        LookColor::Indexed(_) => None,
    }
}

/// A style colour as a look's colour; `theme` is the workbook's twelve theme colours as `RRGGBB`
/// in the colour scheme's order (empty without a theme part).
pub fn resolve_color(color: &StyleColor, theme: &[String]) -> Option<LookColor> {
    if let Some(rgb) = color.rgb.as_deref() {
        let rgb = rgb.trim();
        let digits = rgb.get(rgb.len().checked_sub(6)?..)?;
        if digits.chars().all(|c| c.is_ascii_hexdigit()) {
            let tint = color
                .tint
                .as_deref()
                .and_then(|t| t.trim().parse::<f64>().ok());
            let n = u32::from_str_radix(digits, 16).ok()?;
            let triple = ((n >> 16) as u8, (n >> 8) as u8, n as u8);
            return Some(LookColor::Rgb(hex(match tint {
                Some(t) if t != 0.0 => tinted(triple, t),
                _ => triple,
            })));
        }
    }
    if let Some(index) = color
        .theme
        .as_deref()
        .and_then(|t| t.trim().parse::<usize>().ok())
    {
        // Theme colour indexes count lt1 and dk1 (and lt2 and dk2) the other way round from the
        // scheme's order.
        let slot = match index {
            0 => 1,
            1 => 0,
            2 => 3,
            3 => 2,
            other => other,
        };
        let base = theme.get(slot)?;
        let n = u32::from_str_radix(base, 16).ok()?;
        let triple = ((n >> 16) as u8, (n >> 8) as u8, n as u8);
        let tint = color
            .tint
            .as_deref()
            .and_then(|t| t.trim().parse::<f64>().ok())
            .unwrap_or(0.0);
        return Some(LookColor::Rgb(hex(if tint == 0.0 {
            triple
        } else {
            tinted(triple, tint)
        })));
    }
    if let Some(index) = color
        .indexed
        .as_deref()
        .and_then(|t| t.trim().parse::<u32>().ok())
    {
        return Some(LookColor::Indexed(index));
    }
    None
}

/// A colour with a tint applied as Windows' HLS arithmetic does (HLSMAX 240, ColorRGBToHLS and
/// ColorHLSToRGB): the luminance moved toward black (`tint` < 0) or white.
pub fn tinted((r, g, b): (u8, u8, u8), tint: f64) -> (u8, u8, u8) {
    const HLSMAX: i64 = 240;
    const RGBMAX: i64 = 255;
    let (r, g, b) = (i64::from(r), i64::from(g), i64::from(b));
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = ((max + min) * HLSMAX + RGBMAX) / (2 * RGBMAX);
    let (h, s) = if max == min {
        (HLSMAX * 2 / 3, 0)
    } else {
        let s = if l <= HLSMAX / 2 {
            ((max - min) * HLSMAX + (max + min) / 2) / (max + min)
        } else {
            ((max - min) * HLSMAX + (2 * RGBMAX - max - min) / 2) / (2 * RGBMAX - max - min)
        };
        let part = |c: i64| ((max - c) * (HLSMAX / 6) + (max - min) / 2) / (max - min);
        let (rd, gd, bd) = (part(r), part(g), part(b));
        let mut h = if r == max {
            bd - gd
        } else if g == max {
            HLSMAX / 3 + rd - bd
        } else {
            2 * HLSMAX / 3 + gd - rd
        };
        if h < 0 {
            h += HLSMAX;
        }
        if h > HLSMAX {
            h -= HLSMAX;
        }
        (h, s)
    };
    let lf = l as f64;
    let l = if tint < 0.0 {
        (lf * (1.0 + tint)).round() as i64
    } else {
        (lf * (1.0 - tint) + (HLSMAX as f64 - HLSMAX as f64 * (1.0 - tint))).round() as i64
    }
    .clamp(0, HLSMAX);
    if s == 0 {
        let v = (l * RGBMAX / HLSMAX) as u8;
        return (v, v, v);
    }
    let m2 = if l <= HLSMAX / 2 {
        (l * (HLSMAX + s) + HLSMAX / 2) / HLSMAX
    } else {
        l + s - (l * s + HLSMAX / 2) / HLSMAX
    };
    let m1 = 2 * l - m2;
    let hue = |mut h: i64| {
        if h < 0 {
            h += HLSMAX;
        }
        if h > HLSMAX {
            h -= HLSMAX;
        }
        if h < HLSMAX / 6 {
            m1 + ((m2 - m1) * h + HLSMAX / 12) / (HLSMAX / 6)
        } else if h < HLSMAX / 2 {
            m2
        } else if h < HLSMAX * 2 / 3 {
            m1 + ((m2 - m1) * (HLSMAX * 2 / 3 - h) + HLSMAX / 12) / (HLSMAX / 6)
        } else {
            m1
        }
    };
    let channel = |c: i64| ((c * RGBMAX + HLSMAX / 2) / HLSMAX).clamp(0, 255) as u8;
    (
        channel(hue(h + HLSMAX / 3)),
        channel(hue(h)),
        channel(hue(h - HLSMAX / 3)),
    )
}

/// The address of a cell as a relative A1 reference (`B3`).
pub fn a1(row: u32, column: u32) -> String {
    let mut column = column;
    let mut letters = Vec::new();
    while column > 0 {
        letters.push(b'A' + ((column - 1) % 26) as u8);
        column = (column - 1) / 26;
    }
    letters.reverse();
    format!("{}{}", String::from_utf8_lossy(&letters), row)
}

/// The formula a `cellIs` rule tests at the block's origin (`origin` as the cell's A1 address),
/// its operator applied to the origin cell and the rule's formulas, so that the comparison is the
/// evaluator's, as Excel's comparison operators compare.
pub fn condition_formula(rule: &ConditionalRule, origin: &str) -> Option<String> {
    let cell = origin;
    let f = |i: usize| rule.formulas.get(i).map(|f| format!("({f})"));
    Some(match rule.operator.as_deref()? {
        "lessThan" => format!("{cell}<{}", f(0)?),
        "lessThanOrEqual" => format!("{cell}<={}", f(0)?),
        "equal" => format!("{cell}={}", f(0)?),
        "notEqual" => format!("{cell}<>{}", f(0)?),
        "greaterThanOrEqual" => format!("{cell}>={}", f(0)?),
        "greaterThan" => format!("{cell}>{}", f(0)?),
        "between" => format!("AND({cell}>={},{cell}<={})", f(0)?, f(1)?),
        "notBetween" => format!("OR({cell}<{},{cell}>{})", f(0)?, f(1)?),
        _ => return None,
    })
}

/// The formula whose value is the bound of an `aboveAverage` rule over `ranges` (absolute A1
/// ranges joined by commas), with its standard-deviation offset.
pub fn average_formula(rule: &ConditionalRule, ranges: &str) -> String {
    let mut bound = format!("AVERAGE({ranges})");
    if let Some(deviation) = rule.std_dev {
        bound.push_str(&format!(
            "{}({deviation}*STDEVP({ranges}))",
            if rule.above_average { "+" } else { "-" }
        ));
    }
    bound
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_tint_follows_windows_hls_arithmetic() {
        // Excel 16.0.20430: accent1 156082 at tint 0.59999389629810485 shows as 83CCEB.
        assert_eq!(
            hex(tinted((0x15, 0x60, 0x82), 0.59999389629810485)),
            "#83cceb"
        );
        assert_eq!(hex(tinted((0, 0, 0), 0.5)), hex(tinted((0, 0, 0), 0.5)));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_scale_interpolates_each_channel_and_rounds() {
        let colors = [
            Some((0xf8, 0x69, 0x6b)),
            Some((0xff, 0xeb, 0x84)),
            Some((0x63, 0xbe, 0x7b)),
        ];
        let points = [1.0, 5.5, 10.0];
        assert_eq!(
            interpolate(&points, &colors, 1.0).map(hex).as_deref(),
            Some("#f8696b")
        );
        assert_eq!(
            interpolate(&points, &colors, 9.0).map(hex).as_deref(),
            Some("#86c97e")
        );
        assert_eq!(
            interpolate(&points, &colors, 10.0).map(hex).as_deref(),
            Some("#63be7b")
        );
        assert_eq!(percentile(&[1.0, 2.0, 3.0, 4.0], 0.5), 2.5);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn addresses_are_a1() {
        assert_eq!(a1(3, 2), "B3");
        assert_eq!(a1(1, 27), "AA1");
    }
}
