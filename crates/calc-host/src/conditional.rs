//! What a sheet's conditional formats show (dc-6yw): for a cell, the fill, the font colour and
//! weight, the number format, the data bar or the icon its rules give it, as Excel applies them.
//!
//! [`HostState::conditional_formats`] prepares a sheet's rules once for the current revision: each
//! rule's formulas compiled for detached evaluation ([`calc_engine::DetachedFormula`]: CalcFml's
//! one evaluator, reading the workbook as the engine published it, never a second route), and the
//! statistics a rule reads over its whole areas (the numbers for a colour scale, a data bar, an
//! icon set, a top or bottom rank and an average; the values for duplicates). [`ConditionalFormats::look`]
//! then gives one cell's look.
//!
//! The rules apply in priority order (`priority`, lowest first, across every block that holds
//! the cell). A rule that holds sets the parts of the look a rule before it left unset, and one
//! with stop-if-true ends the cell's rules. A `cellIs` rule compares the cell with its formulas
//! through a formula the host writes (`A1>(5)` at the block's top-left cell), so the comparison is
//! the evaluator's, as Excel's comparison operators compare; an `expression` rule and the text,
//! blank, error and date rules, for which Excel writes the formula, hold when their formula gives
//! TRUE or a non-zero number. A formula that is refused, or that reads a cell whose value nothing
//! calculated, does not hold, and is counted ([`ConditionalFormats::refused`]).
//!
//! Colours: an `rgb` colour as it is; a theme colour from the workbook's theme with its tint
//! applied as Windows' HLS arithmetic does (luminance in 0 to 240, measured on Excel 16.0.20430:
//! accent1 156082 at tint 0.6 shows as 83CCEB); an indexed colour by its index, for the caller's
//! palette ([`LookColor::Indexed`]).
//!
//! Not modelled yet, each leaving the cell as it is: `aboveAverage` with `stdDev`, rules that only
//! Excel 2010's extension holds, a colour scale's or data bar's `formula` thresholds that give no
//! number, and after a row or column is inserted or deleted in the session, a rule's formulas are
//! not rewritten (its areas move, its formulas keep their text; the save rewrites them).

use crate::state::HostState;
use crate::{ProjectedValue, ProjectionRequest, project};
use calc_doc::{ConditionalRule, Threshold};
use calc_doc_model::StyleColor;
use calc_doc_model::value::{CalcValue, CellKey, CoreRef, GridArea, SheetId};
use calc_engine::DetachedFormula;
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
    fn is_empty(&self) -> bool {
        *self == CellLook::default()
    }
}

/// A differential format's look (a rule's `dxfId`).
#[derive(Clone, Debug, Default, PartialEq)]
struct DxfLook {
    fill: Option<LookColor>,
    font_color: Option<LookColor>,
    bold: Option<bool>,
    italic: Option<bool>,
    underline: Option<bool>,
    strike: Option<bool>,
    number_format: Option<String>,
}

/// What a rule reads over its whole areas.
#[derive(Clone, Debug, Default)]
struct Stats {
    /// The numbers, ascending.
    numbers: Vec<f64>,
    /// How often each value occurs (blanks left out), by a key equal for values Excel takes as
    /// duplicates.
    counts: BTreeMap<String, usize>,
}

/// One rule, prepared.
struct Prepared {
    rule: ConditionalRule,
    areas: Vec<GridArea>,
    /// The formula whose truth the rule tests, compiled at the block's origin.
    condition: Option<Result<DetachedFormula, ()>>,
    /// Each threshold's formula, for colour scales, data bars and icon sets.
    threshold_formulas: Vec<Option<DetachedFormula>>,
    stats: Option<Stats>,
    dxf: DxfLook,
}

/// A sheet's conditional formats, prepared for one revision ([`HostState::conditional_formats`]).
pub struct ConditionalFormats {
    sheet: SheetId,
    rules: Vec<Prepared>,
    refused: std::cell::Cell<usize>,
}

impl ConditionalFormats {
    /// The sheet the formats are of.
    pub fn sheet(&self) -> SheetId {
        self.sheet
    }

    /// How many rules the sheet has.
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// How many rule evaluations so far did not hold because their formula was refused.
    pub fn refused(&self) -> usize {
        self.refused.get()
    }

    /// What the conditional formats give the cell `at`, reading `host` (the revision they were
    /// prepared for); `None` when no rule changes its look.
    pub fn look(&self, host: &HostState, at: CellKey) -> Option<CellLook> {
        if at.sheet() != self.sheet {
            return None;
        }
        let mut look = CellLook::default();
        let mut value: Option<Option<CalcValue>> = None;
        for prepared in &self.rules {
            if !prepared.areas.iter().any(|area| area.contains(at)) {
                continue;
            }
            let value = value.get_or_insert_with(|| cell_value(host, at));
            let held = self.apply(host, prepared, at, value.as_ref(), &mut look);
            if held && prepared.rule.stop_if_true {
                break;
            }
        }
        (!look.is_empty()).then_some(look)
    }

    /// Applies one rule to the cell, setting what the look leaves unset; whether the rule held.
    fn apply(
        &self,
        host: &HostState,
        prepared: &Prepared,
        at: CellKey,
        value: Option<&CalcValue>,
        look: &mut CellLook,
    ) -> bool {
        let rule = &prepared.rule;
        let number = value.and_then(number_of);
        match rule.kind.as_str() {
            "colorScale" => {
                let (Some(scale), Some(n), Some(stats)) =
                    (rule.color_scale.as_ref(), number, prepared.stats.as_ref())
                else {
                    return false;
                };
                let points: Vec<f64> = scale
                    .thresholds
                    .iter()
                    .enumerate()
                    .filter_map(|(i, t)| self.threshold(host, prepared, i, t, stats, at))
                    .collect();
                let colors: Vec<Option<(u8, u8, u8)>> =
                    scale.colors.iter().map(|c| rgb_triple(host, c)).collect();
                if points.len() != scale.thresholds.len() || colors.len() < points.len() {
                    return false;
                }
                let Some(color) = interpolate(&points, &colors, n) else {
                    return false;
                };
                if look.fill.is_none() {
                    look.fill = Some(LookColor::Rgb(hex(color)));
                }
                true
            }
            "dataBar" => {
                let (Some(bar), Some(n), Some(stats)) =
                    (rule.data_bar.as_ref(), number, prepared.stats.as_ref())
                else {
                    return false;
                };
                let lo = bar
                    .thresholds
                    .first()
                    .and_then(|t| self.threshold(host, prepared, 0, t, stats, at));
                let hi = bar
                    .thresholds
                    .get(1)
                    .and_then(|t| self.threshold(host, prepared, 1, t, stats, at));
                let (Some(lo), Some(hi)) = (lo, hi) else {
                    return false;
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
                        color: resolve(host, &bar.color),
                        gradient: bar.gradient,
                        show_value: bar.show_value,
                    });
                }
                true
            }
            "iconSet" => {
                let (Some(set), Some(n), Some(stats)) =
                    (rule.icon_set.as_ref(), number, prepared.stats.as_ref())
                else {
                    return false;
                };
                let mut index = 0;
                for (i, t) in set.thresholds.iter().enumerate().skip(1) {
                    let Some(limit) = self.threshold(host, prepared, i, t, stats, at) else {
                        return false;
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
                true
            }
            "top10" => {
                let (Some(n), Some(stats)) = (number, prepared.stats.as_ref()) else {
                    return false;
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
                    return false;
                }
                let held = if rule.bottom {
                    n <= stats.numbers[take - 1]
                } else {
                    n >= stats.numbers[count - take]
                };
                held && dress(look, &prepared.dxf)
            }
            "aboveAverage" => {
                let (Some(n), Some(stats)) = (number, prepared.stats.as_ref()) else {
                    return false;
                };
                if rule.std_dev.is_some() || stats.numbers.is_empty() {
                    return false;
                }
                let mean = stats.numbers.iter().sum::<f64>() / stats.numbers.len() as f64;
                let held = match (rule.above_average, rule.equal_average) {
                    (true, false) => n > mean,
                    (true, true) => n >= mean,
                    (false, false) => n < mean,
                    (false, true) => n <= mean,
                };
                held && dress(look, &prepared.dxf)
            }
            "duplicateValues" | "uniqueValues" => {
                let (Some(value), Some(stats)) = (value, prepared.stats.as_ref()) else {
                    return false;
                };
                let Some(key) = value_key(value) else {
                    return false;
                };
                let count = stats.counts.get(&key).copied().unwrap_or(0);
                let held = if rule.kind == "duplicateValues" {
                    count > 1
                } else {
                    count == 1
                };
                held && dress(look, &prepared.dxf)
            }
            _ => {
                let Some(condition) = &prepared.condition else {
                    return false;
                };
                let Ok(formula) = condition else {
                    self.refused.set(self.refused.get() + 1);
                    return false;
                };
                match formula.evaluate(&host.engine, &host.model, at) {
                    Ok(result) if truth(&result) => dress(look, &prepared.dxf),
                    Ok(_) => false,
                    Err(_) => {
                        self.refused.set(self.refused.get() + 1);
                        false
                    }
                }
            }
        }
    }

    /// The number a threshold stands for over the rule's statistics.
    fn threshold(
        &self,
        host: &HostState,
        prepared: &Prepared,
        index: usize,
        threshold: &Threshold,
        stats: &Stats,
        at: CellKey,
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
            "num" => given().or_else(|| {
                let formula = prepared.threshold_formulas.get(index)?.as_ref()?;
                number_of(&formula.evaluate(&host.engine, &host.model, at).ok()?)
            }),
            "percent" => Some(lo + (hi - lo) * given()? / 100.0),
            "percentile" => Some(percentile(&stats.numbers, given()? / 100.0)),
            "formula" => {
                let formula = prepared.threshold_formulas.get(index)?.as_ref()?;
                number_of(&formula.evaluate(&host.engine, &host.model, at).ok()?)
            }
            _ => None,
        }
    }
}

/// Sets what the differential format gives and the look leaves unset; true.
fn dress(look: &mut CellLook, dxf: &DxfLook) -> bool {
    fn set<T: Clone>(slot: &mut Option<T>, value: &Option<T>) {
        if slot.is_none() {
            slot.clone_from(value);
        }
    }
    set(&mut look.fill, &dxf.fill);
    set(&mut look.font_color, &dxf.font_color);
    set(&mut look.bold, &dxf.bold);
    set(&mut look.italic, &dxf.italic);
    set(&mut look.underline, &dxf.underline);
    set(&mut look.strike, &dxf.strike);
    set(&mut look.number_format, &dxf.number_format);
    true
}

/// Whether a formula's value holds: TRUE, or a number that is not 0.
fn truth(value: &CalcValue) -> bool {
    match value.core_ref() {
        CoreRef::Logical(b) => b,
        CoreRef::Number(n) => n != 0.0,
        _ => false,
    }
}

fn number_of(value: &CalcValue) -> Option<f64> {
    match value.core_ref() {
        CoreRef::Number(n) => Some(n),
        _ => None,
    }
}

/// The key under which Excel takes two values as duplicates: a number by its value, a text
/// regardless of case, a logical, an error; `None` for a blank.
fn value_key(value: &CalcValue) -> Option<String> {
    match value.core_ref() {
        CoreRef::Number(n) => Some(format!("n{n:?}")),
        CoreRef::Text(t) => Some(format!("t{}", t.to_string_lossy().to_lowercase())),
        CoreRef::Logical(b) => Some(format!("b{b}")),
        CoreRef::Error(e) => Some(format!("e{e:?}")),
        _ => None,
    }
}

/// PERCENTILE.INC of ascending `numbers` at `k` (0 to 1).
fn percentile(numbers: &[f64], k: f64) -> f64 {
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
fn interpolate(points: &[f64], colors: &[Option<(u8, u8, u8)>], n: f64) -> Option<(u8, u8, u8)> {
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

fn hex((r, g, b): (u8, u8, u8)) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// A colour as an RGB triple, when it is an rgb or theme colour.
fn rgb_triple(host: &HostState, color: &StyleColor) -> Option<(u8, u8, u8)> {
    match resolve(host, color)? {
        LookColor::Rgb(text) => {
            let n = u32::from_str_radix(text.trim_start_matches('#'), 16).ok()?;
            Some(((n >> 16) as u8, (n >> 8) as u8, n as u8))
        }
        LookColor::Indexed(_) => None,
    }
}

/// A style colour as a look's colour.
pub(crate) fn resolve(host: &HostState, color: &StyleColor) -> Option<LookColor> {
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
    if let Some(theme) = color
        .theme
        .as_deref()
        .and_then(|t| t.trim().parse::<usize>().ok())
    {
        // Theme colour indexes count lt1 and dk1 (and lt2 and dk2) the other way round from the
        // scheme's order.
        let slot = match theme {
            0 => 1,
            1 => 0,
            2 => 3,
            3 => 2,
            other => other,
        };
        let base = host.loaded.theme_colors().get(slot)?;
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
fn tinted((r, g, b): (u8, u8, u8), tint: f64) -> (u8, u8, u8) {
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

/// The value a rule reads of a cell: its literal, or its formula's calculated or file-cached
/// value; `None` for a blank cell, and for a formula with no value (refused or stale).
pub(crate) fn cell_value(host: &HostState, at: CellKey) -> Option<CalcValue> {
    let page = project(host, &ProjectionRequest { cells: vec![at] }).ok()?;
    projected_value(page.cells().first()?.value())
}

pub(crate) fn projected_value(value: &ProjectedValue) -> Option<CalcValue> {
    match value {
        ProjectedValue::Literal(value) => Some(value.clone()),
        ProjectedValue::Calculated { value, .. } => Some(value.clone()),
        ProjectedValue::FileCached(cache) => Some(cache.value().to_value()),
        _ => None,
    }
}

/// The address of a cell as a relative A1 reference (`B3`).
fn a1(at: CellKey) -> String {
    let mut column = at.column();
    let mut letters = Vec::new();
    while column > 0 {
        letters.push(b'A' + ((column - 1) % 26) as u8);
        column = (column - 1) / 26;
    }
    letters.reverse();
    format!("{}{}", String::from_utf8_lossy(&letters), at.row())
}

/// The formula a `cellIs` rule tests at the block's origin, its operator applied to the origin
/// cell and the rule's formulas.
fn cell_is_formula(rule: &ConditionalRule, origin: CellKey) -> Option<String> {
    let cell = a1(origin);
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

impl HostState {
    /// The conditional formats of `sheet`, prepared for the current revision (see
    /// [`crate::conditional`]); empty for a sheet the file gave none.
    pub fn conditional_formats(&self, sheet: SheetId) -> ConditionalFormats {
        let mut rules = Vec::new();
        if let Some(loaded) = self.loaded.sheet(sheet) {
            for block in loaded.conditional_formats() {
                let areas = self.current_areas(sheet, &block.areas);
                let Some(origin) = areas.first().map(|area| area.first()) else {
                    continue;
                };
                for rule in &block.rules {
                    rules.push(self.prepare(rule, &areas, origin));
                }
            }
        }
        rules.sort_by_key(|prepared| prepared.rule.priority);
        ConditionalFormats {
            sheet,
            rules,
            refused: std::cell::Cell::new(0),
        }
    }

    /// Areas the file states for `sheet`, as (top, left, bottom, right), where the session's row
    /// and column insertions and deletions have moved them; an area all of whose cells were
    /// deleted is left out.
    pub(crate) fn current_areas(
        &self,
        sheet: SheetId,
        areas: &[(u32, u32, u32, u32)],
    ) -> Vec<GridArea> {
        use calc_fml::reference::{EditAxis, StructuralEdit};
        let moves: Vec<StructuralEdit> = self
            .axis_edits
            .iter()
            .filter(|edit| edit.sheet() == sheet)
            .map(|edit| {
                let axis = match edit.axis() {
                    calc_doc_model::Axis::Rows => EditAxis::Rows,
                    calc_doc_model::Axis::Columns => EditAxis::Columns,
                };
                let (first, count) = (edit.first(), edit.count());
                match edit.change() {
                    calc_doc_model::AxisChange::Insert => StructuralEdit::Insert {
                        sheet,
                        axis,
                        first,
                        count,
                    },
                    calc_doc_model::AxisChange::Delete => StructuralEdit::Delete {
                        sheet,
                        axis,
                        first,
                        count,
                    },
                }
            })
            .collect();
        areas
            .iter()
            .filter_map(|&(top, left, bottom, right)| {
                let mut area = GridArea::new(
                    CellKey::new(sheet, top, left).ok()?,
                    CellKey::new(sheet, bottom, right).ok()?,
                )
                .ok()?;
                for edit in &moves {
                    area = edit.area(area)?;
                }
                Some(area)
            })
            .collect()
    }

    fn prepare(&self, rule: &ConditionalRule, areas: &[GridArea], origin: CellKey) -> Prepared {
        let compile =
            |text: &str| DetachedFormula::compile(&self.model, text, origin).map_err(|_| ());
        let condition = match rule.kind.as_str() {
            "cellIs" => Some(match cell_is_formula(rule, origin) {
                Some(text) => compile(&text),
                None => Err(()),
            }),
            "colorScale" | "dataBar" | "iconSet" | "top10" | "aboveAverage" | "duplicateValues"
            | "uniqueValues" => None,
            _ => Some(match rule.formulas.first() {
                Some(text) => compile(text),
                None => Err(()),
            }),
        };
        let thresholds: Vec<&Threshold> = rule
            .color_scale
            .iter()
            .flat_map(|s| s.thresholds.iter())
            .chain(rule.data_bar.iter().flat_map(|b| b.thresholds.iter()))
            .chain(rule.icon_set.iter().flat_map(|s| s.thresholds.iter()))
            .collect();
        let threshold_formulas = thresholds
            .iter()
            .map(|t| match (t.kind.as_str(), t.value.as_deref()) {
                ("formula" | "num", Some(text)) if text.trim().parse::<f64>().is_err() => {
                    DetachedFormula::compile(&self.model, text.trim_start_matches('='), origin).ok()
                }
                _ => None,
            })
            .collect();
        let stats = (condition.is_none()).then(|| self.stats(areas));
        let dxf = rule
            .dxf
            .and_then(|i| self.loaded.styles().differential_formats.get(i as usize))
            .map(|d| DxfLook {
                fill: d.fill.as_ref().and_then(|fill| {
                    let color = match fill.pattern_type.as_deref() {
                        Some("solid") => fill.fg_color.as_ref().or(fill.bg_color.as_ref()),
                        _ => fill.bg_color.as_ref().or(fill.fg_color.as_ref()),
                    }?;
                    resolve(self, color)
                }),
                font_color: d
                    .font
                    .as_ref()
                    .and_then(|font| font.color.as_ref())
                    .and_then(|color| resolve(self, color)),
                bold: d.font.as_ref().and_then(|f| f.bold.then_some(true)),
                italic: d.font.as_ref().and_then(|f| f.italic.then_some(true)),
                underline: d
                    .font
                    .as_ref()
                    .and_then(|f| f.underline.as_ref().map(|u| u != "none")),
                strike: d.font.as_ref().and_then(|f| f.strike.then_some(true)),
                number_format: d.number_format.as_ref().map(|n| n.code.clone()),
            })
            .unwrap_or_default();
        Prepared {
            rule: rule.clone(),
            areas: areas.to_vec(),
            condition,
            threshold_formulas,
            stats,
            dxf,
        }
    }

    /// The numbers and the value counts over `areas`' occupied cells.
    fn stats(&self, areas: &[GridArea]) -> Stats {
        let mut cells: Vec<CellKey> = Vec::new();
        for area in areas {
            cells.extend(
                self.model
                    .cells(*area)
                    .into_iter()
                    .flatten()
                    .map(|(at, _)| at),
            );
        }
        cells.sort();
        cells.dedup();
        let mut stats = Stats::default();
        if let Ok(page) = project(self, &ProjectionRequest { cells }) {
            for cell in page.cells() {
                let Some(value) = projected_value(cell.value()) else {
                    continue;
                };
                if let Some(n) = number_of(&value) {
                    stats.numbers.push(n);
                }
                if let Some(key) = value_key(&value) {
                    *stats.counts.entry(key).or_default() += 1;
                }
            }
        }
        stats.numbers.sort_by(f64::total_cmp);
        stats
    }
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
}
