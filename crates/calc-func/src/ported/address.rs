//! ADDRESS, ported from OxFunc (crates/oxfunc_core/src/functions/reference_metadata_family.rs at
//! 6fca3518): the coordinate conversion, the sheet name's quoting by Excel's name grammar
//! (address_name_classes.rs) and the address text in A1 or R1C1 form.

use crate::value::{ExcelText, WorksheetErrorCode};

use super::address_name_classes;

pub(crate) fn address_integer(number: f64) -> i64 {
    // On ADDRESS's admitted integer range, Excel's observed conversion is
    // floor(RN53(RN64(number + 2^31)) - 2^31). The two rounding steps admit
    // the lower neighbor of each integer through these inclusive thresholds.
    // This reduced comparison avoids platform-dependent extended precision.
    // W111 retains exact-bit boundary probes for both thresholds and all axes.
    let upper = number.ceil();
    let threshold = if upper > 0.0 {
        2.0_f64.powi(-22) + 2.0_f64.powi(-33)
    } else {
        2.0_f64.powi(-23) + 2.0_f64.powi(-34)
    };
    if upper - number <= threshold {
        upper as i64
    } else {
        number.floor() as i64
    }
}

fn address_name_character(unit: u16, initial: bool) -> bool {
    if unit < 128 {
        let byte = unit as u8;
        byte.is_ascii_alphabetic()
            || matches!(byte, b'_' | b'\\')
            || (!initial && (byte.is_ascii_digit() || matches!(byte, b'.' | b'?')))
    } else if unit < 256 {
        // Excel's legacy name classes are distinct from Unicode alphabetic
        // categories (for example multiplication and division signs qualify).
        matches!(unit, 0xA1 | 0xA4 | 0xA7 | 0xA8 | 0xAA | 0xAD | 0xAF..=0xBA | 0xBC..=0xFF)
    } else if (0xD800..=0xDFFF).contains(&unit) {
        // Classification is by UTF-16 unit, even for isolated surrogates.
        !initial || unit <= 0xDBFF
    } else {
        let ranges = if initial {
            address_name_classes::NAME_START
        } else {
            address_name_classes::NAME_CONTINUE
        };
        let index = ranges.partition_point(|&(_, end)| end < unit);
        ranges.get(index).is_some_and(|&(start, _)| start <= unit)
    }
}

fn address_name_is_reference(units: &[u16]) -> bool {
    use super::a1_refs::{A1ReferenceNotation, parse_a1_reference};
    // Reference tokens are ASCII. Lossy conversion here affects no emitted
    // text and cannot turn a non-ASCII unit into an ASCII reference character.
    let name = String::from_utf16_lossy(units).to_ascii_uppercase();
    if let Some(reference) = parse_a1_reference(&name) {
        if reference.notation == A1ReferenceNotation::Rect
            && reference.end_row <= 1_048_576
            && reference.end_col <= 16_384
        {
            return true;
        }
    }
    if matches!(name.as_str(), "R" | "C" | "RC" | "TRUE" | "FALSE") {
        return true;
    }
    // Defined-name ambiguity uses a leading R1C1 axis token. A dot immediately
    // after its digits instead makes a dotted name token. The suffix need not
    // itself form a reference (R1foo and C1R0 still require quoting).
    let (rest, limit) = if let Some(rest) = name.strip_prefix("RC") {
        (rest, 16_384)
    } else if let Some(rest) = name.strip_prefix('R') {
        (rest, 1_048_576)
    } else if let Some(rest) = name.strip_prefix('C') {
        (rest, 16_384)
    } else {
        return false;
    };
    let count = rest.bytes().take_while(u8::is_ascii_digit).count();
    count > 0
        && !rest[count..].starts_with('.')
        && rest[..count]
            .parse::<u32>()
            .is_ok_and(|number| (1..=limit).contains(&number))
}

pub(crate) fn quote_sheet_text_if_needed(
    sheet_text: &ExcelText,
) -> Result<Vec<u16>, WorksheetErrorCode> {
    let units = sheet_text.utf16_code_units();
    let escaped_len = units.len() + units.iter().filter(|&&unit| unit == 39).count();
    if units.len() > 255 || escaped_len > 256 {
        return Err(WorksheetErrorCode::Value);
    }
    let simple_token = |token: &[u16]| {
        token
            .iter()
            .enumerate()
            .all(|(index, &unit)| address_name_character(unit, index == 0))
    };
    let simple = if units.is_empty() {
        true
    } else if units[0] == 91 {
        units
            .iter()
            .position(|&unit| unit == 93)
            .is_some_and(|end| {
                end > 1
                    && units[1..end]
                        .iter()
                        .all(|&unit| address_name_character(unit, false))
                    && simple_token(&units[end + 1..])
            })
    } else {
        simple_token(units) && !address_name_is_reference(units)
    };
    if simple {
        return Ok(units.to_vec());
    }
    let mut quoted = Vec::with_capacity(escaped_len + 2);
    quoted.push(39);
    for &unit in units {
        quoted.push(unit);
        if unit == 39 {
            quoted.push(39);
        }
    }
    quoted.push(39);
    Ok(quoted)
}

struct AddressBuffer(Vec<u16>);

impl AddressBuffer {
    fn literal(&mut self, text: &str) {
        let remaining = 258usize.saturating_sub(self.0.len());
        self.0.extend(text.encode_utf16().take(remaining));
    }

    fn integer(&mut self, number: i64) {
        // Signs are literal fragments; magnitude digits append atomically.
        if number < 0 {
            self.literal("-");
        }
        let digits = number.unsigned_abs().to_string();
        if digits.len() <= 258usize.saturating_sub(self.0.len()) {
            self.literal(&digits);
        }
    }
}

fn column_label_from_index(mut col: usize) -> Option<String> {
    if col == 0 {
        return None;
    }
    let mut chars = Vec::new();
    while col > 0 {
        let rem = (col - 1) % 26;
        chars.push((b'A' + rem as u8) as char);
        col = (col - 1) / 26;
    }
    chars.reverse();
    Some(chars.into_iter().collect())
}

pub(crate) fn format_address_body(
    row: i64,
    col: i64,
    abs_num: usize,
    a1_style: bool,
    sheet_text: Option<&ExcelText>,
) -> Result<ExcelText, WorksheetErrorCode> {
    if !(1..=4).contains(&abs_num) {
        return Err(WorksheetErrorCode::Value);
    }
    let row_relative = !a1_style && matches!(abs_num, 3 | 4);
    let col_relative = !a1_style && matches!(abs_num, 2 | 4);
    let valid_coordinate = |value: i64, limit: i64, relative: bool| {
        if relative {
            (-limit + 1..limit).contains(&value)
        } else {
            (1..=limit).contains(&value)
        }
    };
    if !valid_coordinate(row, 1_048_576, row_relative)
        || !valid_coordinate(col, 16_384, col_relative)
    {
        return Err(WorksheetErrorCode::Value);
    }
    let prefix = match sheet_text {
        Some(sheet) => {
            let mut prefix = quote_sheet_text_if_needed(sheet)?;
            prefix.push(33);
            prefix
        }
        None => Vec::new(),
    };
    // The sheet prefix is constructed before the bounded body append. An
    // admitted escaped name can make the prefix itself 259 UTF-16 units long.
    let mut address = AddressBuffer(prefix);
    if a1_style {
        let col_part = column_label_from_index(col as usize)
            .ok_or(WorksheetErrorCode::Value)?;
        if matches!(abs_num, 1 | 3) {
            address.literal("$");
        }
        address.literal(&col_part);
        if matches!(abs_num, 1 | 2) {
            address.literal("$");
        }
        address.integer(row);
    } else {
        for (label, number, relative) in [("R", row, row_relative), ("C", col, col_relative)] {
            address.literal(label);
            if !relative || number != 0 {
                if relative {
                    address.literal("[");
                }
                address.integer(number);
                if relative {
                    address.literal("]");
                }
            }
        }
    }
    Ok(ExcelText::from_utf16_code_units(address.0))
}

