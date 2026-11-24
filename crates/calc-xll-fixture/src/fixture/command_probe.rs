//! AF-6 C2: call one Excel12 command from a registered XLL macro, with exact operands.
use super::*;
use std::{fs, path::PathBuf, sync::OnceLock};

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
    let ordinal: i32 = lines.next()?.parse().ok()?;
    if !matches!(
        ordinal,
        17 | 47 | 49..=52 | 54 | 55 | 61 | 96 | 97 | 109 | 110 | 122 | 127 | 141
    ) {
        return None;
    }
    let mut arena = Arena::new();
    let mut operands = Vec::new();
    let mut references = Vec::new();
    for line in lines {
        let (kind, value) = line.split_once('\t')?;
        let operand = match kind {
            "bool" => XlOper12::boolean(value.parse().ok()?),
            "number" => XlOper12::number(value.parse().ok()?),
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
    let code = unsafe { call(0x8000 | ordinal, &arguments, &mut result) };
    let kind = result.xltype & TYPE_MASK;
    let value = match kind {
        XL_BOOL => (unsafe { result.value.integer } != 0).to_string(),
        XL_NUM => format!("0x{:016x}", unsafe { result.value.number }.to_bits()),
        XL_ERR | XL_INT => unsafe { result.value.integer }.to_string(),
        _ => String::new(),
    };
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
    fs::write(
        path.join("command-probe-result.txt"),
        format!("{code}\t{kind}\t{value}"),
    )
    .ok()
}
