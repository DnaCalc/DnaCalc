//! Boundary probes for dc-6332.10 (answer 13, D.3, D.9 sentinel pass and the D.22 rows 16385,
//! 16391, 16403). Opt-in: registered only when `boundary-tests.enable` sits beside the module or
//! `DNA_XLL_BOUNDARY_TESTS` is set, so the X1 registrations and their cases are unchanged.
//!
//! Every probe reports in one text result what the callback did, so the same case list runs in
//! Excel and in the hosts and the answers compare as strings:
//! Each registration has its own export: Excel 16.0.20527 keeps one registration per module and
//! procedure, so a second `xlfRegister` of one export with other type text changes the first
//! name's registration too (measured here: three names of one export all ran as `QB$`).
//! - `MyBoundary(fn)` (`QB`), `MyBoundaryMacro` (`QB#`) and `MyBoundaryPool` (`QB$`) build the
//!   operand vector of the X5 permission matrix for callback `fn` inside the registered frame,
//!   fills the result operand with 0xA5 bytes first and reports `code|thread|output`, where the
//!   output is `untouched` when every byte is still the sentinel.
//! - `MyFreeProbe(kind)` (`QB#`) gets one callback result, reports its raw type bits (XLFree?),
//!   calls xlFree on that operand and reports which of its three value words and its type changed, then frees
//!   the same operand again only when the first free nulled its pointer word.
//! - `MyFreeFrame(kind)` (`QB`) returns a DLLFree number; the fixture's xlAutoFree12 then makes
//!   no callback (kind 0), only xlFree (kind 1) or also xlStack, xlGetName, xlfCaller and SUM
//!   (kind 2; Excel 16.0.20527 terminated during this one) and keeps the codes for
//!   `MyFreeFrameResult(kind)`.
//! - `MyRetain(step)` (`QB`) and `MyRetainPool(step)` (`QB$`): step 1 keeps an xlGetName result
//!   across the return, step 2 reads it in a later entry and frees it there.
//! - `MyBoundaryOpen()` reports the same callbacks as answered inside xlAutoOpen.
//! - Operands (answer 13 D.4 baseline, D.5): `MyCoerce(value, type)` (`QQB#`) and
//!   `MyCoerceRef(ref, type)` (`QUB#`) report xlCoerce's status and output for a target type
//!   mask; `MyUnits` (`QQ`) and `MyUnitsCounted` (`QD%`) report the UTF-16 code units they
//!   receive; `MyOddText(kind)` returns text with an embedded NUL (1) or a lone surrogate (2);
//!   `MySwapRows(array)` (`QQ`) swaps its input's first two rows in place and returns the new
//!   top-left; `MyNestedName()` (`Q#`) compares xlGetName before and after a nested xlUDF.
//!
//! The fixture shares calc-xll-abi with the host, so its layouts are no independent witness:
//! these probes measure behaviour, and Excel's answers are the oracle.
use super::*;

#[link(name = "user32")]
unsafe extern "system" {
    fn GetWindowThreadProcessId(window: *mut c_void, process: *mut u32) -> u32;
    fn GetClassNameW(window: *mut c_void, name: *mut u16, capacity: i32) -> i32;
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentProcessId() -> u32;
    fn GetCurrentThreadStackLimits(low: *mut usize, high: *mut usize);
}
const SENTINEL: u8 = 0xa5;
static OPEN_THREAD: AtomicUsize = AtomicUsize::new(0);
static OPEN_RESULTS: Mutex<String> = Mutex::new(String::new());
static FREE_FRAME_POINTER: AtomicUsize = AtomicUsize::new(0);
static FREE_FRAME_RESULT: Mutex<String> = Mutex::new(String::new());
static FREE_FRAME_KIND: AtomicUsize = AtomicUsize::new(0);
/// A retained callback result, kept as its raw bytes so the static is `Send`.
static RETAINED: Mutex<Option<[u8; 28]>> = Mutex::new(None);

pub(super) unsafe fn register_all(module: &XlOper12) -> bool {
    OPEN_THREAD.store(unsafe { GetCurrentThreadId() } as usize, Ordering::Release);
    // xlGetInst (16391) inside xlAutoOpen only with a second marker: a host that refuses it
    // there refuses the whole load, which would hide every other probe.
    let open_inst = std::env::var_os("DNA_XLL_BOUNDARY_OPEN_INST").is_some()
        || std::path::Path::new(&*SELF_PATH.lock().unwrap())
            .parent()
            .is_some_and(|directory| directory.join("boundary-open.enable").is_file());
    let open = if open_inst {
        vec![XL_STACK, 16391, 16403, 16392]
    } else {
        vec![XL_STACK, 16403, 16392]
    };
    *OPEN_RESULTS.lock().unwrap() = open
        .iter()
        .map(|&function| format!("{function}:{}", unsafe { probe(function) }))
        .collect::<Vec<_>>()
        .join(";");
    [
        ("MyBoundary", "QB", "MyBoundary"),
        ("MyBoundaryMacro", "QB#", "MyBoundaryMacro"),
        ("MyBoundaryPool", "QB$", "MyBoundaryPool"),
        ("MyFreeProbe", "QB#", "MyFreeProbe"),
        ("MyFreeFrame", "QB", "MyFreeFrame"),
        ("MyFreeFrameResult", "QB", "MyFreeFrameResult"),
        ("MyRetain", "QB", "MyRetain"),
        ("MyRetainPool", "QB$", "MyRetainPool"),
        ("MyBoundaryOpen", "Q", "MyBoundaryOpen"),
        ("MyBoundarySink", "J", "MyBoundarySink"),
        ("MyCoerce", "QQB#", "MyCoerce"),
        ("MyCoerceRef", "QUB#", "MyCoerceRef"),
        ("MyUnits", "QQ", "MyUnits"),
        ("MyUnitsCounted", "QD%", "MyUnitsCounted"),
        ("MyOddText", "QB", "MyOddText"),
        ("MySwapRows", "QQ", "MySwapRows"),
        ("MyNestedName", "Q#", "MyNestedName"),
    ]
    .into_iter()
    .all(|(procedure, signature, name)| {
        let registered = unsafe { register(module, procedure, signature, name) };
        if !registered {
            note(&format!("registration failed: {name} {signature}"));
        }
        registered
    })
}
/// Appends one line to `boundary.log` beside the module: what failed while opening.
fn note(line: &str) {
    use std::io::Write;
    let path = SELF_PATH.lock().unwrap().clone();
    if let Some(directory) = std::path::Path::new(&path).parent()
        && let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(directory.join("boundary.log"))
    {
        let _ = writeln!(file, "{line}");
    }
}

/// A result operand whose 28 defined bytes (value and type) are the sentinel. The four bytes of
/// tail padding are not part of the ABI and are never compared.
fn sentinel() -> XlOper12 {
    XlOper12 {
        value: XlValue12 {
            storage: [SENTINEL; 24],
        },
        xltype: u32::from_ne_bytes([SENTINEL; 4]),
    }
}
/// The value and type bytes of an operand, without the tail padding.
fn bytes(oper: &XlOper12) -> [u8; 28] {
    let mut raw = [0u8; 28];
    unsafe {
        std::ptr::copy_nonoverlapping((oper as *const XlOper12).cast::<u8>(), raw.as_mut_ptr(), 28);
    }
    raw
}
fn thread() -> &'static str {
    if unsafe { GetCurrentThreadId() } as usize == OPEN_THREAD.load(Ordering::Acquire) {
        "main"
    } else {
        "worker"
    }
}
fn main_module() -> usize {
    unsafe { GetModuleHandleW(std::ptr::null()) as usize }
}
fn own_module() -> usize {
    let mut handle = std::ptr::null_mut();
    // GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | UNCHANGED_REFCOUNT
    unsafe {
        GetModuleHandleExW(6, own_module as *const () as *const u16, &mut handle);
    }
    handle as usize
}
fn handle_name(value: usize) -> String {
    if value == 0 {
        "null".into()
    } else if value == main_module() {
        "main".into()
    } else if value == own_module() {
        "self".into()
    } else {
        "other".into()
    }
}

/// xlStack's number against this thread's own stack headroom (both differ per call depth and
/// per process), as the shortfall in 16 KiB steps: Excel 16.0.20527 answered 87,119 bytes
/// less than the fixture's own headroom in every context (main, worker, xlAutoOpen), `5x16KiB`.
fn stack(value: i32) -> String {
    let (mut low, mut high) = (0usize, 0usize);
    unsafe { GetCurrentThreadStackLimits(&mut low, &mut high) };
    let here = &low as *const usize as usize;
    let headroom = here.saturating_sub(low) as i64;
    let shortfall = headroom - i64::from(value);
    if (0..1 << 20).contains(&shortfall) {
        format!("int:headroom-{}x16KiB", shortfall / 16384)
    } else {
        format!("int:{value}|own_headroom={headroom}")
    }
}
/// xlGetHwnd's 32-bit handle, named by its process and window class (the value differs per run).
fn window(low: i32) -> String {
    let handle = low as u32 as usize as *mut c_void;
    let mut process = 0;
    let thread = unsafe { GetWindowThreadProcessId(handle, &mut process) };
    if thread == 0 {
        return "hwnd:invalid".into();
    }
    let mut name = [0u16; 64];
    let length = unsafe { GetClassNameW(handle, name.as_mut_ptr(), 64) }.max(0) as usize;
    format!(
        "hwnd:process={}|class={}",
        if process == unsafe { GetCurrentProcessId() } {
            "self"
        } else {
            "other"
        },
        String::from_utf16_lossy(&name[..length])
    )
}
/// What a callback left in its result operand. Pointer values and paths that differ between
/// processes are named by their relation (`@self`, `main`), never printed.
unsafe fn output(oper: &XlOper12) -> String {
    let raw = bytes(oper);
    if raw.iter().all(|&b| b == SENTINEL) {
        return "untouched".into();
    }
    let kind = oper.xltype & TYPE_MASK;
    let flags = oper.xltype & !TYPE_MASK;
    let value = unsafe {
        match kind {
            XL_BIG_DATA => format!(
                "data={}|count={}",
                handle_name(oper.value.big_data.data as usize),
                oper.value.big_data.byte_count
            ),
            XL_INT if oper.value.integer as u32 == main_module() as u32 => "main-low32".into(),
            XL_STR => {
                let text = counted_text(oper.value.string);
                if text == *SELF_PATH.lock().unwrap() {
                    "@self".into()
                } else if let Some((_, sheet)) =
                    text.strip_prefix('[').and_then(|t| t.split_once(']'))
                {
                    // The workbook's name depends on the session (Book1, Book2, ...).
                    format!("[workbook]{sheet}")
                } else {
                    text
                }
            }
            XL_SREF => {
                let r = oper.value.single_reference.rectangle;
                format!(
                    "count={}|{},{},{},{}",
                    oper.value.single_reference.count,
                    r.row_first,
                    r.row_last,
                    r.col_first,
                    r.col_last
                )
            }
            XL_REF => {
                let table = oper.value.reference.table;
                if table.is_null() {
                    "table=null".into()
                } else {
                    format!("areas={}", (*table).count)
                }
            }
            XL_NUM | XL_BOOL | XL_ERR | XL_INT | XL_MULTI | XL_NIL | XL_MISSING => {
                describe_value(oper)
            }
            _ => format!(
                "raw={}",
                raw.iter().map(|b| format!("{b:02x}")).collect::<String>()
            ),
        }
    };
    format!("{}{}:{value}", type_name(kind), flag_text(flags))
}
fn flag_text(flags: u32) -> String {
    let rest = flags & !(XL_FREE | DLL_FREE);
    if rest == 0 {
        String::new()
    } else {
        format!("+bits{rest:#x}")
    }
}

/// The X5 permission matrix's operand vector for one callback (owned.cpp `permission`), plus the
/// D.22 rows. Built inside the registered frame; the caller frees nothing it did not get back.
unsafe fn probe(function: i32) -> String {
    let mut arena = Arena::new();
    let mut caller = sentinel();
    let mut caller_code = None;
    let mut operands: Vec<XlOper12> = match function {
        4 => vec![XlOper12::number(19.), XlOper12::number(23.)],
        XL_COERCE => vec![XlOper12::number(19.)],
        237 => vec![XlOper12::boolean(false)],
        186 => vec![XlOper12::number(2.)],
        XLF_EVALUATE => vec![arena.text("1+1").unwrap()],
        32890 => vec![
            XlOper12::boolean(true),
            arena.text("X5 permission").unwrap(),
        ],
        // ECHO(TRUE): a harmless command, from a worksheet function.
        32909 => vec![XlOper12::boolean(true)],
        XL_UDF => vec![
            arena.text("MyAdd").unwrap(),
            XlOper12::number(1.),
            XlOper12::number(2.),
        ],
        XLF_SET_NAME => vec![arena.text("MyBoundaryNoSuchName").unwrap()],
        16401 => vec![arena.text("MyBoundarySink").unwrap(), XlOper12::integer(1)],
        16389 => {
            caller_code = Some(unsafe { call(XLF_CALLER, &[], &mut caller) });
            vec![caller]
        }
        _ => Vec::new(),
    };
    let pointers = operands
        .iter_mut()
        .map(|o| o as *mut XlOper12)
        .collect::<Vec<_>>();
    let mut result = sentinel();
    let code = unsafe { call(function, &pointers, &mut result) };
    let shown = if function == 16392 && code == SUCCESS && result.xltype & TYPE_MASK == XL_INT {
        window(unsafe { result.value.integer })
    } else if function == XL_STACK && code == SUCCESS && result.xltype & TYPE_MASK == XL_INT {
        stack(unsafe { result.value.integer })
    } else {
        unsafe { output(&result) }
    };
    let mut text = format!("{code}|{}|{shown}", thread());
    if code == SUCCESS {
        unsafe { free_result(&mut result) };
    }
    if function == 16401 && code == SUCCESS {
        // Never leave a registered handler behind in an Excel session: unregister at once.
        let mut nil = XlOper12::default();
        let mut event = XlOper12::integer(1);
        let mut cleared = sentinel();
        let undo = unsafe { call(16401, &[&mut nil, &mut event], &mut cleared) };
        text.push_str(&format!("|unregister={undo}"));
        if undo == SUCCESS {
            unsafe { free_result(&mut cleared) };
        }
    }
    if caller_code == Some(SUCCESS) {
        unsafe { free_result(&mut caller) };
    }
    text
}
unsafe fn free_result(oper: &mut XlOper12) {
    unsafe {
        call(XL_FREE_FN, &[oper], std::ptr::null_mut());
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyBoundary(function: f64) -> *mut XlOper12 {
    let text = unsafe { probe(function as i32) };
    returned_text(&text)
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyBoundaryMacro(function: f64) -> *mut XlOper12 {
    let text = unsafe { probe(function as i32) };
    returned_text(&text)
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyBoundaryPool(function: f64) -> *mut XlOper12 {
    let text = unsafe { probe(function as i32) };
    returned_text(&text)
}
#[unsafe(no_mangle)]
pub extern "system" fn MyBoundaryOpen() -> *mut XlOper12 {
    let text = OPEN_RESULTS.lock().unwrap().clone();
    returned_text(&text)
}
#[unsafe(no_mangle)]
pub extern "system" fn MyBoundarySink() -> i32 {
    0
}

/// D.3: raw bits of one callback result, then what xlFree does to the caller's operand.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyFreeProbe(kind: f64) -> *mut XlOper12 {
    let mut arena = Arena::new();
    let (function, mut operands): (i32, Vec<XlOper12>) = match kind as i32 {
        1 => (XL_GET_NAME, vec![]),
        2 => (XLF_CALLER, vec![]),
        3 => (
            XL_COERCE,
            vec![XlOper12::number(7.), XlOper12::integer(XL_STR as i32)],
        ),
        4 => (
            XL_COERCE,
            vec![XlOper12::number(7.), XlOper12::integer(XL_MULTI as i32)],
        ),
        5 => (16388, vec![]),
        6 => (4, vec![XlOper12::number(19.), XlOper12::number(23.)]),
        7 => (XLF_EVALUATE, vec![arena.text("\"ab\"&\"c\"").unwrap()]),
        _ => return returned(XlOper12::error(15)),
    };
    let pointers = operands
        .iter_mut()
        .map(|o| o as *mut XlOper12)
        .collect::<Vec<_>>();
    let mut result = sentinel();
    let code = unsafe { call(function, &pointers, &mut result) };
    if code != SUCCESS {
        return returned_text(&format!("{code}|{}", unsafe { output(&result) }));
    }
    let raw_type = result.xltype;
    let before = bytes(&result);
    let first = unsafe { call(XL_FREE_FN, &[&mut result], std::ptr::null_mut()) };
    let after = bytes(&result);
    let words = [0..8, 8..16, 16..24, 24..28]
        .into_iter()
        .map(|range| {
            if before[range.clone()] == after[range.clone()] {
                "same"
            } else if after[range].iter().all(|&b| b == 0) {
                "zero"
            } else {
                "changed"
            }
        })
        .collect::<Vec<_>>()
        .join(",");
    // A second free on the same operand is safe only when the first one cleared its pointer.
    let pointer_type = matches!(raw_type & TYPE_MASK, XL_STR | XL_MULTI | XL_REF);
    let again = if !pointer_type || after[..8].iter().all(|&b| b == 0) {
        unsafe { call(XL_FREE_FN, &[&mut result], std::ptr::null_mut()) }.to_string()
    } else {
        "skipped".into()
    };
    returned_text(&format!(
        "{code}|{:#06x}|free={first}|words={words}|type_after={:#06x}|again={again}",
        raw_type, result.xltype
    ))
}

/// The DLLFree result whose xlAutoFree12 runs the free-frame probes.
#[unsafe(no_mangle)]
pub extern "system" fn MyFreeFrame(kind: f64) -> *mut XlOper12 {
    FREE_FRAME_RESULT.lock().unwrap().clear();
    FREE_FRAME_KIND.store(kind as usize, Ordering::Release);
    let result = Box::into_raw(Box::new(XlOper12 {
        value: XlValue12 { number: 42. },
        xltype: XL_NUM | DLL_FREE,
    }));
    FREE_FRAME_POINTER.store(result as usize, Ordering::Release);
    result
}
#[unsafe(no_mangle)]
/// The argument only keeps the case formulas distinct; it names the kind just run.
pub extern "system" fn MyFreeFrameResult(_kind: f64) -> *mut XlOper12 {
    let text = FREE_FRAME_RESULT.lock().unwrap().clone();
    returned_text(if text.is_empty() { "not freed" } else { &text })
}
/// Called by xlAutoFree12 first; true when `value` was the free-frame probe's own result.
pub(super) unsafe fn free_frame(value: *mut XlOper12) -> bool {
    if value.is_null()
        || FREE_FRAME_POINTER
            .compare_exchange(value as usize, 0, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
    {
        return false;
    }
    let kind = FREE_FRAME_KIND.load(Ordering::Acquire);
    let mut parts = vec![format!("thread={}", thread())];
    if kind >= 2 {
        for function in [XL_STACK, XL_GET_NAME, XLF_CALLER, 4] {
            parts.push(format!("{function}:{}", unsafe { probe(function) }));
        }
    }
    if kind >= 1 {
        let mut nothing = XlOper12::default();
        parts.push(format!("16384:{}", unsafe {
            call(XL_FREE_FN, &[&mut nothing], std::ptr::null_mut())
        }));
    }
    *FREE_FRAME_RESULT.lock().unwrap() = parts.join(";");
    drop(unsafe { Box::from_raw(value) });
    true
}

/// D.3: a callback allocation kept across the return of one entry and released in a later one.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyRetain(step: f64) -> *mut XlOper12 {
    unsafe { retain(step) }
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyRetainPool(step: f64) -> *mut XlOper12 {
    unsafe { retain(step) }
}
unsafe fn retain(step: f64) -> *mut XlOper12 {
    let mut retained = RETAINED.lock().unwrap();
    let text = if step == 1. {
        if retained.is_some() {
            "already kept".to_owned()
        } else {
            let mut result = sentinel();
            let code = unsafe { call(XL_GET_NAME, &[], &mut result) };
            if code == SUCCESS {
                *retained = Some(bytes(&result));
            }
            format!("{code}|{}|kept", thread())
        }
    } else {
        match retained.take() {
            None => "nothing kept".to_owned(),
            Some(raw) => {
                let mut oper = XlOper12::default();
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        raw.as_ptr(),
                        (&mut oper as *mut XlOper12).cast::<u8>(),
                        28,
                    );
                }
                let read = unsafe { output(&oper) };
                let code = unsafe { call(XL_FREE_FN, &[&mut oper], std::ptr::null_mut()) };
                format!("{}|read={read}|free={code}", thread())
            }
        }
    };
    drop(retained);
    returned_text(&text)
}

/// D.4 baseline: xlCoerce of a value or a reference to a target type mask.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyCoerce(value: *mut XlOper12, target: f64) -> *mut XlOper12 {
    unsafe { coerce(value, target) }
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyCoerceRef(value: *mut XlOper12, target: f64) -> *mut XlOper12 {
    unsafe { coerce(value, target) }
}
unsafe fn coerce(value: *mut XlOper12, target: f64) -> *mut XlOper12 {
    let mut kind = XlOper12::integer(target as i32);
    let mut result = sentinel();
    let code = unsafe { call(XL_COERCE, &[value, &mut kind], &mut result) };
    let text = format!("{code}|{}", unsafe { output(&result) });
    if code == SUCCESS {
        unsafe { free_result(&mut result) };
    }
    returned_text(&text)
}
fn units(units: &[u16]) -> String {
    let list = units.iter().map(|u| format!("{u:04x}")).collect::<Vec<_>>();
    format!("len={}|{}", units.len(), list.join(","))
}
/// D.5: the code units of a text operand as received (no decoding).
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyUnits(value: *mut XlOper12) -> *mut XlOper12 {
    let oper = unsafe { value.read_unaligned() };
    let text = if oper.xltype & TYPE_MASK == XL_STR {
        let pointer = unsafe { oper.value.string };
        let length = usize::from(unsafe { *pointer });
        units(unsafe { std::slice::from_raw_parts(pointer.add(1), length) })
    } else {
        format!("type={}", type_name(oper.xltype))
    };
    returned_text(&text)
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyUnitsCounted(value: *const u16) -> *mut XlOper12 {
    let length = usize::from(unsafe { *value });
    let text = units(unsafe { std::slice::from_raw_parts(value.add(1), length) });
    returned_text(&text)
}
/// D.5: returned text that a lossy UTF-8 round trip would change.
#[unsafe(no_mangle)]
pub extern "system" fn MyOddText(kind: f64) -> *mut XlOper12 {
    let body: &[u16] = match kind as i32 {
        1 => &[0x61, 0x00, 0x62],
        2 => &[0x61, 0xd83d, 0x62],
        _ => &[0x61],
    };
    TEXT.with(|p| unsafe {
        let buffer = &mut *p.get();
        buffer.clear();
        buffer.push(body.len() as u16);
        buffer.extend_from_slice(body);
        returned(XlOper12 {
            value: XlValue12 {
                string: buffer.as_mut_ptr(),
            },
            xltype: XL_STR,
        })
    })
}
/// D.5 aliasing: the add-in mutates its own input array (as xlOil's xloSort does). The
/// engine's value must stay as it was; only this call's copy changes.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MySwapRows(value: *mut XlOper12) -> *mut XlOper12 {
    let oper = unsafe { value.read_unaligned() };
    if oper.xltype & TYPE_MASK != XL_MULTI {
        return returned(XlOper12::error(15));
    }
    let array = unsafe { oper.value.array };
    if array.rows < 2 || array.columns < 1 {
        return returned(XlOper12::error(15));
    }
    let columns = array.columns as usize;
    unsafe {
        for column in 0..columns {
            let top = array.values.add(column);
            let next = array.values.add(columns + column);
            let swap = top.read_unaligned();
            top.write_unaligned(next.read_unaligned());
            next.write_unaligned(swap);
        }
        returned(array.values.read_unaligned())
    }
}
/// The origin restores after a nested registered call: xlGetName before and after xlUDF.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyNestedName() -> *mut XlOper12 {
    let name = || {
        let mut result = sentinel();
        let code = unsafe { call(XL_GET_NAME, &[], &mut result) };
        let text = format!("{code}|{}", unsafe { output(&result) });
        if code == SUCCESS {
            unsafe { free_result(&mut result) };
        }
        text
    };
    let before = name();
    let mut arena = Arena::new();
    let mut child = arena.text("MyBoundaryMacro").unwrap();
    let mut argument = XlOper12::number(4.);
    let mut result = sentinel();
    let code = unsafe { call(XL_UDF, &[&mut child, &mut argument], &mut result) };
    let nested = format!("{code}|{}", unsafe { output(&result) });
    if code == SUCCESS {
        unsafe { free_result(&mut result) };
    }
    let after = name();
    returned_text(&format!(
        "before={before}|nested={nested}|same={}",
        before == after
    ))
}
