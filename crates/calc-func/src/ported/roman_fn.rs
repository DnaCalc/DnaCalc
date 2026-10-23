use crate::value::ExcelText;
use crate::value::WorksheetErrorCode;
const THOUSANDS: [&str; 4] = ["", "M", "MM", "MMM"];
const HUNDREDS: [&str; 10] = ["", "C", "CC", "CCC", "CD", "D", "DC", "DCC", "DCCC", "CM"];
const TENS: [&str; 10] = ["", "X", "XX", "XXX", "XL", "L", "LX", "LXX", "LXXX", "XC"];
const ONES: [&str; 10] = ["", "I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX"];

fn standard_roman(number: u32) -> String {
    let thousands = THOUSANDS[(number / 1000) as usize];
    let hundreds = HUNDREDS[((number % 1000) / 100) as usize];
    let tens = TENS[((number % 100) / 10) as usize];
    let ones = ONES[(number % 10) as usize];
    format!("{thousands}{hundreds}{tens}{ones}")
}

fn with_standard_suffix(prefix: &str, remainder: u32) -> String {
    if remainder == 0 {
        prefix.to_string()
    } else {
        format!("{prefix}{}", standard_roman(remainder))
    }
}

fn roman_form1_under_100(number: u32) -> String {
    match number {
        45..=49 => with_standard_suffix("VL", number - 45),
        95..=99 => with_standard_suffix("VC", number - 95),
        _ => standard_roman(number),
    }
}

fn roman_form2_under_100(number: u32) -> String {
    match number {
        49 => "IL".to_string(),
        45..=48 => with_standard_suffix("VL", number - 45),
        99 => "IC".to_string(),
        95..=98 => with_standard_suffix("VC", number - 95),
        _ => standard_roman(number),
    }
}

fn roman_form1_under_1000(number: u32) -> String {
    match number {
        0..=99 => roman_form1_under_100(number),
        100..=399 => format!(
            "{}{}",
            "C".repeat((number / 100) as usize),
            roman_form1_under_100(number % 100)
        ),
        400..=449 => format!("CD{}", roman_form1_under_100(number - 400)),
        450..=499 => format!("LD{}", roman_form1_under_100(number - 450)),
        500..=899 => {
            let remainder = number - 500;
            format!(
                "D{}{}",
                "C".repeat((remainder / 100) as usize),
                roman_form1_under_100(remainder % 100)
            )
        }
        900..=949 => format!("CM{}", roman_form1_under_100(number - 900)),
        950..=999 => format!("LM{}", roman_form1_under_100(number - 950)),
        _ => unreachable!("number out of range"),
    }
}

fn roman_form2_under_1000(number: u32) -> String {
    match number {
        0..=99 => roman_form2_under_100(number),
        100..=399 => format!(
            "{}{}",
            "C".repeat((number / 100) as usize),
            roman_form2_under_100(number % 100)
        ),
        400..=489 => format!("CD{}", roman_form2_under_100(number - 400)),
        490..=499 => format!("XD{}", standard_roman(number - 490)),
        500..=899 => {
            let remainder = number - 500;
            format!(
                "D{}{}",
                "C".repeat((remainder / 100) as usize),
                roman_form2_under_100(remainder % 100)
            )
        }
        900..=989 => format!("CM{}", roman_form2_under_100(number - 900)),
        990..=999 => format!("XM{}", standard_roman(number - 990)),
        _ => unreachable!("number out of range"),
    }
}

fn roman_form3_under_1000(number: u32) -> String {
    match number {
        0..=99 => roman_form2_under_100(number),
        100..=399 => format!(
            "{}{}",
            "C".repeat((number / 100) as usize),
            roman_form2_under_100(number % 100)
        ),
        400..=489 => format!("CD{}", roman_form2_under_100(number - 400)),
        490..=494 => format!("XD{}", standard_roman(number - 490)),
        495..=499 => format!("VD{}", standard_roman(number - 495)),
        500..=899 => {
            let remainder = number - 500;
            format!(
                "D{}{}",
                "C".repeat((remainder / 100) as usize),
                roman_form2_under_100(remainder % 100)
            )
        }
        900..=989 => format!("CM{}", roman_form2_under_100(number - 900)),
        990..=994 => format!("XM{}", standard_roman(number - 990)),
        995..=999 => format!("VM{}", standard_roman(number - 995)),
        _ => unreachable!("number out of range"),
    }
}

fn roman_form4_under_1000(number: u32) -> String {
    match number {
        499 => "ID".to_string(),
        999 => "IM".to_string(),
        0..=99 => roman_form2_under_100(number),
        100..=399 => format!(
            "{}{}",
            "C".repeat((number / 100) as usize),
            roman_form2_under_100(number % 100)
        ),
        400..=489 => format!("CD{}", roman_form2_under_100(number - 400)),
        490..=494 => format!("XD{}", standard_roman(number - 490)),
        495..=498 => format!("VD{}", standard_roman(number - 495)),
        500..=899 => {
            let remainder = number - 500;
            format!(
                "D{}{}",
                "C".repeat((remainder / 100) as usize),
                roman_form2_under_100(remainder % 100)
            )
        }
        900..=989 => format!("CM{}", roman_form2_under_100(number - 900)),
        990..=994 => format!("XM{}", standard_roman(number - 990)),
        995..=998 => format!("VM{}", standard_roman(number - 995)),
        _ => unreachable!("number out of range"),
    }
}

fn roman_by_form(number: u32, form: i32) -> String {
    let thousands = THOUSANDS[(number / 1000) as usize];
    let remainder = number % 1000;
    let tail = match form {
        0 => standard_roman(remainder),
        1 => roman_form1_under_1000(remainder),
        2 => roman_form2_under_1000(remainder),
        3 => roman_form3_under_1000(remainder),
        4 => roman_form4_under_1000(remainder),
        _ => unreachable!("validated before dispatch"),
    };
    format!("{thousands}{tail}")
}

pub fn roman_kernel(number: f64, form: i32) -> Result<ExcelText, WorksheetErrorCode> {
    let number = number.trunc();
    if !number.is_finite() || !(0.0..=3999.0).contains(&number) || !(0..=4).contains(&form) {
        return Err(WorksheetErrorCode::Value);
    }

    let roman = roman_by_form(number as u32, form);
    Ok(ExcelText::from_utf16_code_units(
        roman.encode_utf16().collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn as_string(text: &ExcelText) -> String {
        text.to_string_lossy()
    }

    #[test]
    fn roman_kernel_matches_excel_probe_rows() {
        let cases = [
            (499.0, 0, "CDXCIX"),
            (499.0, 1, "LDVLIV"),
            (499.0, 2, "XDIX"),
            (499.0, 3, "VDIV"),
            (499.0, 4, "ID"),
            (0.0, 0, ""),
            (3999.0, 0, "MMMCMXCIX"),
            (3999.0, 4, "MMMIM"),
        ];

        for (number, form, expected) in cases {
            let got = roman_kernel(number, form)
                .unwrap_or_else(|e| panic!("roman_kernel({number}, {form}) returned error {e:?}"));
            assert_eq!(as_string(&got), expected);
        }
    }

    #[test]
    fn roman_kernel_matches_excel_simplification_boundaries() {
        let cases = [
            (45.0, 1, "VL"),
            (49.0, 2, "IL"),
            (95.0, 1, "VC"),
            (99.0, 2, "IC"),
            (490.0, 1, "LDXL"),
            (490.0, 2, "XD"),
            (495.0, 3, "VD"),
            (499.0, 4, "ID"),
            (990.0, 1, "LMXL"),
            (990.0, 2, "XM"),
            (995.0, 3, "VM"),
            (999.0, 4, "IM"),
        ];

        for (number, form, expected) in cases {
            let got = roman_kernel(number, form).unwrap();
            assert_eq!(as_string(&got), expected);
        }
    }

    #[test]
    fn roman_kernel_truncates_numeric_inputs_and_rejects_domain_violations() {
        assert_eq!(as_string(&roman_kernel(499.9, 0).unwrap()), "CDXCIX");
        assert_eq!(as_string(&roman_kernel(499.0, 1).unwrap()), "LDVLIV");
        assert_eq!(roman_kernel(-1.0, 0), Err(WorksheetErrorCode::Value));
        assert_eq!(roman_kernel(4000.0, 0), Err(WorksheetErrorCode::Value));
        assert_eq!(roman_kernel(499.0, -1), Err(WorksheetErrorCode::Value));
        assert_eq!(roman_kernel(499.0, 5), Err(WorksheetErrorCode::Value));
    }
}
