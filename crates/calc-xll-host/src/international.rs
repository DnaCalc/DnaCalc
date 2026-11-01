//! GET.WORKSPACE(37): English formula syntax with the current Windows regional
//! settings. This desktop host currently has one English interface language.
use calc_value::{ArrayShape, CalcArray, CalcValue, ExcelText};

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetLocaleInfoEx(name: *const u16, kind: u32, output: *mut u16, count: i32) -> i32;
}

fn regional(kind: u32) -> Result<String, String> {
    let mut buffer = [0u16; 128];
    let count = unsafe {
        GetLocaleInfoEx(
            std::ptr::null(),
            kind,
            buffer.as_mut_ptr(),
            buffer.len() as i32,
        )
    };
    if count <= 0 {
        return Err(format!(
            "dc-snz.20: Windows regional setting {kind}: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(String::from_utf16_lossy(&buffer[..count as usize - 1]))
}
fn number(kind: u32) -> Result<u32, String> {
    regional(kind)?
        .parse()
        .map_err(|_| format!("invalid Windows regional number {kind}"))
}
fn text(value: &str) -> CalcValue {
    CalcValue::text(ExcelText::from_interop_assignment(value))
}
fn num(value: u32) -> CalcValue {
    CalcValue::number(f64::from(value))
}

pub(crate) fn settings() -> Result<CalcValue, String> {
    let decimal = regional(0x0e)?;
    // Excel's legacy international array uses an ordinary space for NBSP grouping.
    let thousands = regional(0x0f)?.replace('\u{a0}', " ");
    let positive = number(0x1b)?;
    let negative = number(0x1c)?;
    let negative_style = match negative {
        0 | 4 | 14 | 15 => 0,
        1 | 5 | 8 | 9 => 1,
        2 | 6 | 12 | 13 => 2,
        3 | 7 | 10 | 11 => 3,
        _ => return Err("dc-snz.20: unknown Windows currency format".into()),
    };
    let date_order = number(0x21)?;
    let array_column = if decimal == "," { "\\" } else { "," };
    let mut values = vec![
        num(1),
        num(number(0x05)?),
        text(&decimal),
        text(&thousands),
        text(&regional(0x0c)?),
        text("R"),
        text("C"),
        text("r"),
        text("c"),
        text("["),
        text("]"),
        text("{"),
        text("}"),
        text(array_column),
        text(";"),
        text("@"),
        text(&regional(0x1d)?),
        text(&regional(0x1e)?),
        text("y"),
        text("m"),
        text("d"),
        text("h"),
        text("m"),
        text("s"),
        text(&regional(0x14)?),
        text("General"),
        num(number(0x19)?),
        num(negative_style),
        num(number(0x11)?),
        num(3),
        num(3),
        num(date_order),
    ];
    values.extend(
        [
            number(0x23)? != 0,        // 24-hour clock
            false,                     // English worksheet function names
            number(0x0d)? == 0,        // metric
            positive == 3,             // postfix currency symbol preceded by a space
            matches!(positive, 0 | 2), // currency before amount
            negative_style != 0,
            true,
            true, // currency trailing/leading zeros
            number(0x27)? != 0,
            number(0x26)? != 0,
            number(0x24)? != 0,
            date_order == 0,
            number(0x25)? != 0, // time leading zero
        ]
        .into_iter()
        .map(CalcValue::logical),
    );
    values.push(text("en-US"));
    Ok(CalcValue::array(
        CalcArray::new(
            ArrayShape {
                rows: 1,
                cols: values.len(),
            },
            values,
        )
        .ok_or("invalid international settings shape")?,
    ))
}
