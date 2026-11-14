//! Range.Value's E6 type selection. This does not render or coerce a cell's value.
//! CELL's format classifier is private to CalcFunc and has different measured rules (for
//! example yyyy-mm-dd is CELL class G). Keep undecidable Automation classes as refusals.
use super::*;
use calc_doc_model::{AxisEdit, StyleTable};
use calc_func::locale::FormatProfile;
use std::collections::BTreeMap;

pub(super) struct Formats {
    sheets: Arc<[calc_doc::LoadedSheet]>,
    axis_edits: Vec<AxisEdit>,
    builtin: BTreeMap<u32, String>,
    profile: Option<FormatProfile>,
}

impl Formats {
    pub(super) fn new(host: &crate::HostState) -> Self {
        Self {
            sheets: host.loaded().shared_sheets(),
            axis_edits: host.axis_edits.clone(),
            builtin: host.builtin_formats.clone(),
            profile: host.entry_environment().profile,
        }
    }
    fn code<'a>(&'a self, styles: &'a StyleTable, id: u32) -> Option<&'a str> {
        // A file's declared code wins over the session's regional built-in meaning.
        styles
            .number_formats
            .iter()
            .find(|f| f.id == id)
            .map(|f| f.code.as_str())
            .or_else(|| self.builtin.get(&id).map(String::as_str))
            .or_else(|| calc_doc_model::builtin_number_format(id))
    }
    pub(super) fn number(&self, model: &ModelRoot, at: CellKey, number: f64) -> Reply {
        let styles = model.definitions().styles();
        let style = model
            .definitions()
            .cell_format(at)
            .or_else(|| styles.runs.get(at))
            .unwrap_or_else(|| {
                crate::facts::imported_style(
                    self.sheets.iter().find(|s| s.id() == at.sheet()),
                    &self.axis_edits,
                    at,
                )
            });
        let format = styles
            .cell_formats
            .get(style as usize)
            .ok_or_else(unknown)?;
        let code = self
            .code(styles, format.number_format_id.unwrap_or(0))
            .ok_or_else(unknown)?;
        let class = class(code, number)?;
        if matches!(class, Class::Date) {
            // OLE Automation DATE's finite range; E6 preserves the serial's bits.
            if !number.is_finite() || number <= -657_435. || number >= 2_958_466. {
                return Err(overflow());
            }
            return Ok(AutoValue::Date(number));
        }
        if matches!(class, Class::Time) {
            return Ok(AutoValue::R8(number));
        }
        // E6 A11's accounting code escapes the dollar and grouping comma; the locale's
        // Currency style in A13 does not. Require the session's Currency style lineage
        // as well as its actual code; a currency-looking token alone is not enough.
        if let Some(style) = format.xf_id.and_then(|xf| {
            styles
                .cell_styles
                .iter()
                .find(|s| s.xf_id == xf && s.builtin_id == Some(4))
        }) {
            let base = styles
                .cell_style_formats
                .get(style.xf_id as usize)
                .ok_or_else(unknown)?;
            let base_code = self
                .code(styles, base.number_format_id.unwrap_or(0))
                .ok_or_else(unknown)?;
            if code == base_code {
                if style.custom_builtin || self.profile.is_none() {
                    return Err(unknown());
                }
                return currency(number);
            }
        }
        if matches!(class, Class::Currency) {
            return Err(unknown());
        }
        // A non-default XF with no named-style record could be the lost Currency
        // lineage. The number-format string alone cannot distinguish it from accounting.
        if format
            .xf_id
            .is_some_and(|xf| xf != 0 && !styles.cell_styles.iter().any(|s| s.xf_id == xf))
        {
            return Err(unknown());
        }
        Ok(AutoValue::R8(number))
    }
}
fn unknown() -> AutoError {
    AutoError::not_modelled(
        "Range.Value format class is not known from the session; use Value2",
        "dc-1nbp.6",
    )
}
/// Round the exact binary64 value to 1/10000 units without first rounding its scaled
/// value to binary64. That intermediate loses CY units above 2^53 scaled units.
fn currency(number: f64) -> Reply {
    if !number.is_finite() || number.abs() >= 1_000_000_000_000_000. {
        return Err(overflow());
    }
    let bits = number.to_bits();
    let negative = bits >> 63 != 0;
    let exponent = ((bits >> 52) & 0x7ff) as i32;
    let fraction = bits & ((1u64 << 52) - 1);
    let (significand, shift) = if exponent == 0 {
        (fraction, 1074)
    } else {
        (fraction | (1u64 << 52), 1075 - exponent)
    };
    // The finite magnitude bound above implies shift >= 3 and a product below 2^67.
    let product = u128::from(significand) * 10_000;
    let magnitude = if shift >= 128 {
        0
    } else {
        let shift = shift as u32;
        let whole = product >> shift;
        let remainder = product & ((1u128 << shift) - 1);
        whole + u128::from(remainder >= (1u128 << (shift - 1)))
    };
    let limit = (i64::MAX as u128) + u128::from(negative);
    if magnitude > limit {
        return Err(overflow());
    }
    let signed = if negative {
        -(magnitude as i128)
    } else {
        magnitude as i128
    };
    Ok(AutoValue::Currency(
        i64::try_from(signed).map_err(|_| overflow())?,
    ))
}
fn overflow() -> AutoError {
    // Excel errors carry EXCEPINFO; Status would incorrectly return DISP_E_OVERFLOW directly.
    AutoError::excel(0x8002_000Au32 as i32, "")
}
enum Class {
    Number,
    Date,
    Time,
    Currency,
}

fn class(code: &str, number: f64) -> Result<Class, AutoError> {
    // Split sections only outside literals/brackets. Conditional sections need their own
    // measured Automation classification and are intentionally refused below.
    let mut quoted = false;
    let mut bracket = false;
    let mut escaped = false;
    let mut start = 0;
    let mut sections = Vec::new();
    for (i, ch) in code.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if !quoted && matches!(ch, '\\' | '_' | '*') {
            escaped = true;
            continue;
        }
        if ch == '"' {
            quoted = !quoted;
        }
        if !quoted {
            if ch == '[' {
                bracket = true;
            }
            if ch == ']' {
                bracket = false;
            }
            if ch == ';' && !bracket {
                sections.push(&code[start..i]);
                start = i + 1;
            }
        }
    }
    if quoted || bracket || escaped {
        return Err(unknown());
    }
    sections.push(&code[start..]);
    if sections.len() > 4 {
        return Err(unknown());
    }
    // Refuse conditional codes even when their condition is in an unselected section.
    if sections
        .iter()
        .any(|s| s.contains("[<") || s.contains("[>") || s.contains("[="))
    {
        return Err(unknown());
    }
    let selected = if number < 0. && sections.len() > 1 {
        1
    } else if number == 0. && sections.len() > 2 {
        2
    } else {
        0
    };
    let code = sections[selected].trim().to_ascii_lowercase();
    if code == "general" || code == "@" || code.is_empty() {
        return Ok(Class::Number);
    }
    let code = code.replace("am/pm", "").replace("a/p", "");
    let mut chars = code.chars();
    let (mut date, mut month, mut time, mut currency) = (false, false, false, false);
    while let Some(ch) = chars.next() {
        match ch {
            '"' => {
                if !chars.by_ref().any(|c| c == '"') {
                    return Err(unknown());
                }
            }
            '\\' | '_' | '*' => {
                chars.next().ok_or_else(unknown)?;
            }
            '[' => {
                let mut token = String::new();
                loop {
                    match chars.next() {
                        Some(']') => break,
                        Some(c) => token.push(c),
                        None => return Err(unknown()),
                    }
                }
                if token.contains("f800") || token.contains("f400") {
                    return Err(unknown());
                }
                if matches!(token.as_str(), "h" | "hh" | "m" | "mm" | "s" | "ss") {
                    time = true;
                } else if !(token.starts_with('$')
                    || matches!(
                        token.as_str(),
                        "black"
                            | "blue"
                            | "cyan"
                            | "green"
                            | "magenta"
                            | "red"
                            | "white"
                            | "yellow"
                    )
                    || token
                        .strip_prefix("color")
                        .is_some_and(|v| v.parse::<u8>().is_ok_and(|v| (1..=56).contains(&v))))
                {
                    return Err(unknown());
                }
            }
            '$' => currency = true,
            'y' | 'd' => date = true,
            'm' => month = true,
            'h' | 's' => time = true,
            // Scientific notation's E is numeric, never a date's era token.
            'e' if chars.as_str().starts_with(['+', '-']) => (),
            c if c.is_ascii_alphabetic() => return Err(unknown()),
            _ => (),
        }
    }
    Ok(if date || (month && !time) {
        Class::Date
    } else if time {
        Class::Time
    } else if currency {
        Class::Currency
    } else {
        Class::Number
    })
}
