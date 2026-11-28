//! AF-6 C2: call one Excel12 command or file-context function from a registered XLL macro.
use super::*;
use std::{fmt::Write as _, fs, path::PathBuf, sync::OnceLock};

static DIRECTORY: OnceLock<PathBuf> = OnceLock::new();

pub(super) unsafe fn register_probe(module: &XlOper12, directory: PathBuf) -> bool {
    let mut arena = Arena::new();
    let mut module = *module;
    let mut procedure = arena.text("MyCommandProbe").unwrap();
    let mut signature = arena.text("J").unwrap();
    let mut name = arena.text("MyCommandProbe").unwrap();
    let mut arguments = arena.text("").unwrap();
    let mut macro_type = XlOper12::integer(2);
    let mut result = XlOper12::default();
    let code = unsafe {
        call(
            XLF_REGISTER,
            &[
                &mut module,
                &mut procedure,
                &mut signature,
                &mut name,
                &mut arguments,
                &mut macro_type,
            ],
            &mut result,
        )
    };
    let registered = code == SUCCESS && result.xltype & TYPE_MASK == XL_NUM;
    if registered {
        REGISTRATIONS
            .lock()
            .unwrap()
            .push((unsafe { result.value.number }, "MyCommandProbe".into()));
    }
    if code == SUCCESS {
        unsafe {
            call(XL_FREE_FN, &[&mut result], std::ptr::null_mut());
        }
    }
    registered && DIRECTORY.set(directory).is_ok()
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyCommandProbe() -> i32 {
    match unsafe { probe() } {
        Some(()) => 1,
        None => 0,
    }
}

unsafe fn probe() -> Option<()> {
    let path = DIRECTORY.get()?;
    let request = fs::read_to_string(path.join("command-probe-request.txt")).ok()?;
    let mut lines = request.lines();
    let request_number = lines.next()?;
    let function = if let Some(number) = request_number.strip_prefix("function ") {
        let number = number.parse().ok()?;
        if !matches!(number, 132..=139 | 166 | 186 | 188) {
            return None;
        }
        number
    } else {
        let ordinal: i32 = request_number.parse().ok()?;
        if !matches!(
            ordinal,
            17 | 47 | 49..=55 | 61 | 96 | 97 | 109 | 110 | 122 | 127 | 141 | 386
        ) {
            return None;
        }
        XL_COMMAND | ordinal
    };
    let mut arena = Arena::new();
    let mut operands = Vec::new();
    let mut references = Vec::new();
    for line in lines {
        let (kind, value) = line.split_once('\t')?;
        let operand = match kind {
            "bool" => XlOper12::boolean(value.parse().ok()?),
            "number" => XlOper12::number(value.parse().ok()?),
            "i2" => XlOper12::integer(i32::from(value.parse::<i16>().ok()?)),
            "miss" if value.is_empty() => XlOper12 {
                xltype: XL_MISSING,
                ..XlOper12::default()
            },
            "text" => arena.text(value).ok()?,
            "reference" => {
                let mut text = arena.text(value).ok()?;
                let mut a1 = XlOper12::boolean(false);
                let mut reference = XlOper12::default();
                // xlfTextref, with R1C1 chosen through its Boolean argument.
                let code = unsafe { call(147, &[&mut text, &mut a1], &mut reference) };
                if code != SUCCESS || reference.xltype & TYPE_MASK != XL_REF {
                    fs::write(
                        path.join("command-probe-result.txt"),
                        format!(
                            "reference setup failed: code={code}, xltype={}",
                            reference.xltype
                        ),
                    )
                    .ok()?;
                    return Some(());
                }
                references.push(reference);
                reference
            }
            _ => return None,
        };
        operands.push(operand);
    }
    let arguments: Vec<_> = operands.iter_mut().map(|value| value as *mut _).collect();
    let mut result = XlOper12::default();
    let code = unsafe { call(function, &arguments, &mut result) };
    let xltype = result.xltype;
    let kind = xltype & TYPE_MASK;
    let value = unsafe { result_value(&result) };
    if code == SUCCESS {
        unsafe {
            call(XL_FREE_FN, &[&mut result], std::ptr::null_mut());
        }
    }
    for reference in &mut references {
        unsafe {
            call(XL_FREE_FN, &[reference], std::ptr::null_mut());
        }
    }
    let mut report = format!("{code}\t{kind}\t{value}");
    if function & XL_COMMAND == 0 {
        write!(report, "\t{xltype}").ok()?;
    }
    fs::write(path.join("command-probe-result.txt"), report).ok()
}

// Strings use counted UTF-16, and arrays include each element's type and payload in row order.
unsafe fn result_value(result: &XlOper12) -> String {
    match result.xltype & TYPE_MASK {
        XL_BOOL => (unsafe { result.value.integer } != 0).to_string(),
        XL_NUM => format!("0x{:016x}", unsafe { result.value.number }.to_bits()),
        XL_ERR | XL_INT => unsafe { result.value.integer }.to_string(),
        XL_STR => {
            let ptr = unsafe { result.value.string };
            if ptr.is_null() {
                return "null".into();
            }
            let len = usize::from(unsafe { *ptr });
            let text = unsafe { std::slice::from_raw_parts(ptr.add(1), len) };
            let mut hex = String::with_capacity(6 + text.len() * 4);
            hex.push_str("utf16:");
            for unit in text {
                write!(hex, "{unit:04x}").unwrap();
            }
            hex
        }
        XL_MULTI => {
            let array = unsafe { result.value.array };
            let count = usize::try_from(array.rows)
                .ok()
                .zip(usize::try_from(array.columns).ok())
                .and_then(|(rows, columns)| rows.checked_mul(columns))
                .unwrap_or(0);
            let mut text = format!("{}x{}", array.rows, array.columns);
            if array.values.is_null() && count != 0 {
                text.push_str(";null");
                return text;
            }
            for index in 0..count {
                let item = unsafe { &*array.values.add(index) };
                let kind = item.xltype & TYPE_MASK;
                let value = unsafe { result_value(item) };
                write!(text, ";{kind}:{value}").unwrap();
            }
            text
        }
        _ => String::new(),
    }
}
