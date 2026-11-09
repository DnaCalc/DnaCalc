//! Volatility probe registrations (lane vol-1, `docs/architecture/VOLATILITY.md`). Opt-in: the
//! fixture registers them only when `volatility-tests.enable` sits beside the module or
//! `DNA_XLL_VOLATILITY_TESTS` is set, so the existing X1 registrations and tests are unchanged.
//!
//! Every procedure here is monomorphic and does the same thing: it logs one entry per call
//! (name, call number since the last reset, caller cell, the type of each pointer argument) and
//! then runs a per-name script. Script characters: `T` calls xlfVolatile(TRUE), `F`
//! xlfVolatile(FALSE), `N` xlfVolatile() with no argument, `C` xlCoerce on each pointer argument
//! (version-12 opers only); each call logs its Excel12 status code. `|` separates the parts of
//! one script, one part per call: call k runs part k, and calls past the last part repeat it.
//! A script starting with `*` cycles through its parts instead. The result is always 1.
//! `VolCfg(name, script)` replaces a name's script, `VolReset()` clears the log, `VolResetCalls()` also the per-name call numbers,
//! `VolLog()` returns the log. The scripts are defaults; none of this models Excel.
use super::*;

// xlfVolatile (XLCALL.H). Measured: Excel answers xlretSuccess from a `#` registration and from a
// call outside a cell, and xlretInvXlfn (2) from a worksheet call of any other registration.
const XLF_VOLATILE: i32 = 237;

static CONFIG: Mutex<BTreeMap<String, String>> = Mutex::new(BTreeMap::new());
static COUNTS: Mutex<BTreeMap<String, usize>> = Mutex::new(BTreeMap::new());
static LOG: Mutex<Vec<String>> = Mutex::new(Vec::new());

unsafe fn caller_text() -> String {
    let mut result = XlOper12::default();
    let code = unsafe { call(XLF_CALLER, &[], &mut result) };
    if code != SUCCESS {
        return format!("nocaller{code}");
    }
    let text = unsafe {
        match result.xltype & TYPE_MASK {
            XL_SREF => {
                let r = result.value.single_reference.rectangle;
                format!("{},{}", r.row_first + 1, r.col_first + 1)
            }
            XL_REF => {
                let r = (*result.value.reference.table).rectangles[0];
                format!("{},{}", r.row_first + 1, r.col_first + 1)
            }
            other => format!("caller_type{other}"),
        }
    };
    unsafe { call(XL_FREE_FN, &[&mut result], std::ptr::null_mut()) };
    text
}

/// Type of a pointer argument: `wide` is a version-12 oper (type at offset 24), else a version-4
/// oper (type at offset 8).
unsafe fn arg_type(wide: bool, pointer: *mut u8) -> u32 {
    if pointer.is_null() {
        return 0xffff;
    }
    unsafe {
        if wide {
            std::ptr::read_unaligned(pointer.add(24).cast::<u32>()) & 0xffff
        } else {
            u32::from(std::ptr::read_unaligned(pointer.add(8).cast::<u16>()))
        }
    }
}

unsafe fn run(
    name: &str,
    default: &str,
    pointers: &[(bool, *mut u8)],
    numbers: &[f64],
) -> *mut XlOper12 {
    let number = {
        let mut counts = COUNTS.lock().unwrap();
        let n = counts.entry(name.to_owned()).or_insert(0);
        *n += 1;
        *n
    };
    let script = CONFIG
        .lock()
        .unwrap()
        .get(name)
        .cloned()
        .unwrap_or_else(|| default.to_owned());
    let (cycle, body) = match script.strip_prefix('*') {
        Some(rest) => (true, rest),
        None => (false, script.as_str()),
    };
    let parts: Vec<&str> = body.split('|').collect();
    let part = if cycle {
        parts[(number - 1) % parts.len()]
    } else {
        parts[(number - 1).min(parts.len() - 1)]
    };
    let mut entry = format!("{name}#{number}@{}", unsafe { caller_text() });
    for (wide, pointer) in pointers {
        entry.push_str(&format!(" t{:x}", unsafe { arg_type(*wide, *pointer) }));
    }
    for value in numbers {
        entry.push_str(&format!(" n{value}"));
    }
    entry.push(':');
    for step in part.chars() {
        match step {
            'T' | 'F' => {
                let mut flag = XlOper12::boolean(step == 'T');
                let mut result = XlOper12::default();
                let code = unsafe { call(XLF_VOLATILE, &[&mut flag], &mut result) };
                entry.push_str(&format!("{step}={code},"));
            }
            'N' => {
                let mut result = XlOper12::default();
                let code = unsafe { call(XLF_VOLATILE, &[], &mut result) };
                entry.push_str(&format!("N={code},"));
            }
            'C' => {
                for (wide, pointer) in pointers {
                    if *wide && !pointer.is_null() {
                        let mut result = XlOper12::default();
                        let mut kind = XlOper12::integer(1);
                        let code = unsafe {
                            call(
                                XL_COERCE,
                                &[*pointer as *mut XlOper12, &mut kind],
                                &mut result,
                            )
                        };
                        entry.push_str(&format!("C={code}/{:x},", result.xltype & TYPE_MASK));
                        unsafe { call(XL_FREE_FN, &[&mut result], std::ptr::null_mut()) };
                    }
                }
            }
            _ => {}
        }
    }
    LOG.lock().unwrap().push(entry);
    returned(XlOper12::number(1.0))
}

macro_rules! export {
    ($ident:ident, $name:literal, $default:literal, ()) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "system" fn $ident() -> *mut XlOper12 {
            unsafe { run($name, $default, &[], &[]) }
        }
    };
    ($ident:ident, $name:literal, $default:literal, (wide)) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "system" fn $ident(a: *mut u8) -> *mut XlOper12 {
            unsafe { run($name, $default, &[(true, a)], &[]) }
        }
    };
    ($ident:ident, $name:literal, $default:literal, (narrow)) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "system" fn $ident(a: *mut u8) -> *mut XlOper12 {
            unsafe { run($name, $default, &[(false, a)], &[]) }
        }
    };
    ($ident:ident, $name:literal, $default:literal, (num)) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "system" fn $ident(a: f64) -> *mut XlOper12 {
            unsafe { run($name, $default, &[], &[a]) }
        }
    };
    ($ident:ident, $name:literal, $default:literal, (wide, num)) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "system" fn $ident(a: *mut u8, b: f64) -> *mut XlOper12 {
            unsafe { run($name, $default, &[(true, a)], &[b]) }
        }
    };
    ($ident:ident, $name:literal, $default:literal, (num, wide)) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "system" fn $ident(a: f64, b: *mut u8) -> *mut XlOper12 {
            unsafe { run($name, $default, &[(true, b)], &[a]) }
        }
    };
    ($ident:ident, $name:literal, $default:literal, (wide, wide)) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "system" fn $ident(a: *mut u8, b: *mut u8) -> *mut XlOper12 {
            unsafe { run($name, $default, &[(true, a), (true, b)], &[]) }
        }
    };
    ($ident:ident, $name:literal, $default:literal, (narrow, narrow)) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "system" fn $ident(a: *mut u8, b: *mut u8) -> *mut XlOper12 {
            unsafe { run($name, $default, &[(false, a), (false, b)], &[]) }
        }
    };
}

// One row builds the export and its registration: (export, name, signature, default script, shape).
macro_rules! table {
    ($(($ident:ident, $name:literal, $sig:literal, $default:literal, $shape:tt)),* $(,)?) => {
        $(export!($ident, $name, $default, $shape);)*
        const REGISTRATION_TABLE: &[(&str, &str, &str)] = &[$((stringify!($ident), $name, $sig)),*];
    };
}

table![
    // Ordinary registration; the script decides.
    (vx_plain, "XPlain", "Q", "", ()),
    (vx_true, "XTrue", "Q", "T", ()),
    (vx_false, "XFalse", "Q", "F", ()),
    (vx_tf, "XTF", "Q", "TF", ()),
    (vx_ft, "XFT", "Q", "FT", ()),
    (vx_ttf, "XTTF", "Q", "TTF", ()),
    (vx_seq, "XSeq", "Q", "*T|F", ()),
    (vx_noarg, "XNoArg", "Q", "N", ()),
    // '!' registration.
    (vb_bang, "Bang", "Q!", "", ()),
    (vb_clear, "BangClear", "Q!", "F", ()),
    (vb_clear_once, "BangClearOnce", "Q!", "F|", ()),
    (vb_true, "BangTrue", "Q!", "T", ()),
    (vb_ft, "BangFT", "Q!", "FT", ()),
    (vb_seq, "BangSeq", "Q!", "*F|T", ()),
    // '#' without arguments, '$' thread-safe.
    (vh_hash, "Hash", "Q#", "", ()),
    (vh_hash_false, "HashFalse", "Q#", "F", ()),
    (vh_hash_true, "HashTrue", "Q#", "T", ()),
    (vd_dollar, "Dollar", "Q$", "", ()),
    (vd_dollar_true, "DollarT", "Q$", "T", ()),
    (vd_dollar_false, "DollarF", "Q$", "F", ()),
    // '#' with one argument of each kind; G* are the same without '#'; *C coerce their argument.
    (vh_r, "Hr", "QR#", "", (narrow)),
    (vh_p, "Hp", "QP#", "", (narrow)),
    (vh_u, "Hu", "QU#", "", (wide)),
    (vh_q, "Hq", "QQ#", "", (wide)),
    (vh_b, "Hb", "QB#", "", (num)),
    (vh_ub, "Hub", "QUB#", "", (wide, num)),
    (vh_bu, "Hbu", "QBU#", "", (num, wide)),
    (vh_uc, "HuC", "QU#", "C", (wide)),
    (vh_qc, "HqC", "QQ#", "C", (wide)),
    (vh_ubc, "HubC", "QUB#", "C", (wide, num)),
    (vh_buc, "HbuC", "QBU#", "C", (num, wide)),
    (vh_uf, "HuF", "QU#", "F", (wide)),
    (vh_ut, "HuT", "QU#", "T", (wide)),
    (vh_rr, "Hrr", "QRR#", "", (narrow, narrow)),
    (vh_uu, "Huu", "QUU#", "", (wide, wide)),
    (vg_r, "Gr", "QR", "", (narrow)),
    (vg_p, "Gp", "QP", "", (narrow)),
    (vg_u, "Gu", "QU", "", (wide)),
    (vg_q, "Gq", "QQ", "", (wide)),
    (vg_b, "Gb", "QB", "", (num)),
    (vg_ub, "Gub", "QUB", "", (wide, num)),
    (vg_uc, "GuC", "QU", "C", (wide)),
    (vg_uf, "GuF", "QU", "F", (wide)),
    (vg_ut, "GuT", "QU", "T", (wide)),
    // '#' (the only class where Excel accepts xlfVolatile): scripts and the '!' companion.
    (vh_tf, "HTF", "Q#", "TF", ()),
    (vh_ft, "HFT", "Q#", "FT", ()),
    (vh_ttf, "HTTF", "Q#", "TTF", ()),
    (vh_seq, "HSeq", "Q#", "*T|F", ()),
    (vh_noarg, "HNoArg", "Q#", "N", ()),
    (vh_once_f, "HOnceF", "Q#", "F|", ()),
    (vh_once_t, "HOnceT", "Q#", "T|", ()),
    (vk_bh, "BangHash", "Q!#", "", ()),
    (vk_bhc, "BangHashClear", "Q!#", "F", ()),
    (vk_bhco, "BangHashClearOnce", "Q!#", "F|", ()),
    (vk_bht, "BangHashTrue", "Q!#", "T", ()),
    (vk_bhft, "BangHashFT", "Q!#", "FT", ()),
    (vk_bhseq, "BangHashSeq", "Q!#", "*F|T", ()),
    (vh_utf, "HuTF", "QU#", "TF", (wide)),
    (vh_uft, "HuFT", "QU#", "FT", (wide)),
    (vh_useq, "HuSeq", "QU#", "*T|F", (wide)),
    (vh_uoncef, "HuOnceF", "QU#", "F|", (wide)),
    // '#' with a '!' or '$' companion.
    (vk_bu, "BangHu", "QU!#", "", (wide)),
    (vk_buf, "BangHuF", "QU!#", "F", (wide)),
    (vk_bg, "BangGu", "QU!", "", (wide)),
    (vk_dollar_u, "DollarU", "QU$", "", (wide)),
    (vk_dollar_ut, "DollarUT", "QU$", "T", (wide)),
];

/// Asynchronous registrations (`>EX...`): the script runs in the entry (before the handle is
/// returned); with `late` it runs on the worker thread after the async return, outside any call.
unsafe fn run_async(
    name: &str,
    default: &str,
    value: *const f64,
    handle: *const XlOper12,
    late: bool,
) {
    let value = unsafe { *value };
    if !late {
        unsafe { run(name, default, &[], &[value]) };
    }
    let handle_data = unsafe { (*handle).value.big_data };
    let pointer = handle_data.data as usize;
    let byte_count = handle_data.byte_count;
    let name = name.to_owned();
    let worker = std::thread::spawn(move || {
        let mut handle = XlOper12 {
            value: XlValue12 {
                big_data: XlBigData {
                    data: pointer as *mut u8,
                    byte_count,
                },
            },
            xltype: XL_BIG_DATA,
        };
        let mut answer = XlOper12::number(value + 1.);
        let mut result = XlOper12::default();
        let code = unsafe { call(XL_ASYNC_RETURN, &[&mut handle, &mut answer], &mut result) };
        if late {
            let mut flag = XlOper12::boolean(true);
            let mut r2 = XlOper12::default();
            let status = unsafe { call(XLF_VOLATILE, &[&mut flag], &mut r2) };
            LOG.lock()
                .unwrap()
                .push(format!("{name}#late:asyncreturn={code},T={status},"));
        }
    });
    WORKERS.lock().unwrap().push(worker);
}
macro_rules! async_export {
    ($ident:ident, $name:literal, $default:literal, $late:literal) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "system" fn $ident(value: *const f64, handle: *const XlOper12) {
            unsafe { run_async($name, $default, value, handle, $late) }
        }
    };
}
async_export!(va_plain, "AsyncPlain", "", false);
async_export!(va_true, "AsyncT", "T", false);
async_export!(va_false, "AsyncF", "F", false);
async_export!(va_bang, "AsyncBang", "", false);
async_export!(va_bang_false, "AsyncBangF", "F", false);
async_export!(va_dollar_true, "AsyncDollarT", "T", false);
async_export!(va_dollar_false, "AsyncDollarF", "F", false);
async_export!(va_late, "AsyncLate", "", true);
const ASYNC_TABLE: &[(&str, &str, &str)] = &[
    ("va_plain", "AsyncPlain", ">EX"),
    ("va_true", "AsyncT", ">EX"),
    ("va_false", "AsyncF", ">EX"),
    ("va_bang", "AsyncBang", ">EX!"),
    ("va_bang_false", "AsyncBangF", ">EX!"),
    ("va_dollar_true", "AsyncDollarT", ">EX$"),
    ("va_dollar_false", "AsyncDollarF", ">EX$"),
    ("va_late", "AsyncLate", ">EX"),
];

/// Diagnostic: Excel12(n, TRUE) for n in from..=to, listing the numbers whose status is not 2
/// (xlretInvXlfn). Used to look for VOLATILE's number before the `#` finding; kept for the record.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn VolScan(from: f64, to: f64) -> *mut XlOper12 {
    let mut out = String::new();
    for n in (from as i32)..=(to as i32) {
        let mut flag = XlOper12::boolean(true);
        let mut result = XlOper12::default();
        let code = unsafe { call(n, &[&mut flag], &mut result) };
        if code != 2 {
            out.push_str(&format!("{n}:{code}/{:x} ", result.xltype & TYPE_MASK));
        }
    }
    returned_text(&out)
}

/// Diagnostic: Excel12(n, TRUE) with n from the argument, logged; the cell's volatility shows
/// whether n is VOLATILE.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn XVolN(n: f64) -> *mut XlOper12 {
    let mut flag = XlOper12::boolean(true);
    let mut result = XlOper12::default();
    let code = unsafe { call(n as i32, &[&mut flag], &mut result) };
    LOG.lock().unwrap().push(format!(
        "XVolN#{n}@{}:status={code}/{:x},",
        unsafe { caller_text() },
        result.xltype & TYPE_MASK
    ));
    returned(XlOper12::number(1.0))
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn VolReset() -> *mut XlOper12 {
    LOG.lock().unwrap().clear();
    returned(XlOper12::number(0.0))
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn VolResetCalls() -> *mut XlOper12 {
    COUNTS.lock().unwrap().clear();
    LOG.lock().unwrap().clear();
    returned(XlOper12::number(0.0))
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn VolLog() -> *mut XlOper12 {
    let text = LOG.lock().unwrap().join(";");
    let text: String = text.chars().take(30000).collect();
    returned_text(&text)
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn VolCfg(name: *const u16, script: *const u16) -> *mut XlOper12 {
    let name = String::from_utf16_lossy(&unsafe { read_wide(name) });
    let script = String::from_utf16_lossy(&unsafe { read_wide(script) });
    CONFIG.lock().unwrap().insert(name, script);
    returned(XlOper12::number(1.0))
}

pub(super) unsafe fn register_all(module: &XlOper12) -> bool {
    for (procedure, name, signature) in REGISTRATION_TABLE {
        if !unsafe { register(module, procedure, signature, name) } {
            return false;
        }
    }
    for (procedure, name, signature) in ASYNC_TABLE {
        if !unsafe { register(module, procedure, signature, name) } {
            return false;
        }
    }
    for (procedure, signature) in [
        ("VolReset", "Q"),
        ("VolResetCalls", "Q"),
        ("VolLog", "Q"),
        ("VolCfg", "QC%C%"),
        ("VolScan", "QBB"),
        ("XVolN", "QB"),
    ] {
        if !unsafe { register(module, procedure, signature, procedure) } {
            return false;
        }
    }
    true
}
