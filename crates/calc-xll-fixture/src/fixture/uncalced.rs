//! Status 64 probes for dc-6332.11 (answer 13, D.1 and D.2). Opt-in: registered only when
//! `uncalced-tests.enable` sits beside the module or `DNA_XLL_UNCALCED_TESTS` is set.
//!
//! Every entry appends one record to a log that `MyUcLog()` returns and `MyUcReset` clears, so
//! the same case list (`crates/calc-xll-reference/fixture/x17-uncalced-cases.json`) compares
//! Excel's order of entries, statuses and frees with the hosts' as one string. Each registration
//! has its own export (Excel keeps one registration per module and procedure).
//!
//! - `MyUcRead(ref)` (`QU`), `MyUcReadSafe` (`QU$`), `MyUcReadMacro` (`QU#`): xlCoerce of the
//!   reference into a sentinel result; logs `name:status:output` (`untouched` when the result
//!   operand still holds the sentinel) and returns `status|output`.
//! - `MyUcValue(x)` (`QQ`) and `MyUcValueSafe` (`QQ$`): log the prepared value argument.
//! - `MyUcOwned(ref)` (`QU`): the same read, then returns a DLLFree number 777; the fixture's
//!   xlAutoFree12 logs `free` when it gets that pointer back.
//! - `MyUcParent(ref)` (`QU`): calls xlUDF("MyUcRead", ref) and logs the outer status and output.
//! - `MyUcAgain(ref)` (`QU`): after a status 64 it calls xlCoerce on the same reference again
//!   and then xlfCaller, against the documented contract, and logs both statuses.
//! - `MyUcMixed(ref, x)` (`QUQ`): logs the prepared value argument, then reads the reference.
//! - `MyUcLate(x)` (`BB`): the producer; logs `late:x` when x differs from its last call.
//! - `MyUcThreads()` (`Q`): `main` or `worker` per logged entry, in order.
use super::boundary::{free_result, output, sentinel};
use super::*;

static LOG: Mutex<Vec<String>> = Mutex::new(Vec::new());
static THREADS: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());
static OPEN_THREAD: AtomicUsize = AtomicUsize::new(0);
static OWNED: Mutex<Vec<usize>> = Mutex::new(Vec::new());
static LATE: Mutex<Option<u64>> = Mutex::new(None);

pub(super) unsafe fn register_all(module: &XlOper12) -> bool {
    OPEN_THREAD.store(unsafe { GetCurrentThreadId() } as usize, Ordering::Release);
    [
        ("MyUcReset", "J#", "MyUcReset"),
        ("MyUcLog", "Q", "MyUcLog"),
        ("MyUcThreads", "Q", "MyUcThreads"),
        ("MyUcLate", "BB", "MyUcLate"),
        ("MyUcRead", "QU", "MyUcRead"),
        ("MyUcReadSafe", "QU$", "MyUcReadSafe"),
        ("MyUcReadMacro", "QU#", "MyUcReadMacro"),
        ("MyUcValue", "QQ", "MyUcValue"),
        ("MyUcValueSafe", "QQ$", "MyUcValueSafe"),
        ("MyUcOwned", "QU", "MyUcOwned"),
        ("MyUcParent", "QU", "MyUcParent"),
        ("MyUcAgain", "QU", "MyUcAgain"),
        ("MyUcMixed", "QUQ", "MyUcMixed"),
    ]
    .into_iter()
    .all(|(procedure, signature, name)| unsafe { register(module, procedure, signature, name) })
}

fn log(line: String) {
    let thread = if unsafe { GetCurrentThreadId() } as usize == OPEN_THREAD.load(Ordering::Acquire)
    {
        "main"
    } else {
        "worker"
    };
    LOG.lock().unwrap().push(line);
    THREADS.lock().unwrap().push(thread);
}

/// xlCoerce of `reference` with no type operand into a sentinel result: (status, output).
unsafe fn read(reference: *mut XlOper12) -> (i32, String) {
    let mut result = sentinel();
    let code = unsafe { call(XL_COERCE, &[reference], &mut result) };
    let shown = unsafe { output(&result) };
    if code == SUCCESS {
        unsafe { free_result(&mut result) };
    }
    (code, shown)
}
unsafe fn logged_read(name: &str, reference: *mut XlOper12) -> *mut XlOper12 {
    let (code, shown) = unsafe { read(reference) };
    log(format!("{name}:{code}:{shown}"));
    returned_text(&format!("{code}|{shown}"))
}

#[unsafe(no_mangle)]
pub extern "system" fn MyUcReset() -> i32 {
    LOG.lock().unwrap().clear();
    THREADS.lock().unwrap().clear();
    1
}
#[unsafe(no_mangle)]
pub extern "system" fn MyUcLog() -> *mut XlOper12 {
    returned_text(&LOG.lock().unwrap().join(";"))
}
#[unsafe(no_mangle)]
pub extern "system" fn MyUcThreads() -> *mut XlOper12 {
    returned_text(&THREADS.lock().unwrap().join(";"))
}
/// The producer logs only a changed value: Excel 16.0.20527 sometimes calculates it a second time
/// with the same value within or after `Range.Calculate` (x17 runs 1 and 2, in different groups
/// per run), which says nothing about the readers.
#[unsafe(no_mangle)]
pub extern "system" fn MyUcLate(x: f64) -> f64 {
    let mut last = LATE.lock().unwrap();
    if *last != Some(x.to_bits()) {
        *last = Some(x.to_bits());
        drop(last);
        log(format!("late:{x}"));
    }
    x
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyUcRead(reference: *mut XlOper12) -> *mut XlOper12 {
    unsafe { logged_read("read", reference) }
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyUcReadSafe(reference: *mut XlOper12) -> *mut XlOper12 {
    unsafe { logged_read("safe", reference) }
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyUcReadMacro(reference: *mut XlOper12) -> *mut XlOper12 {
    unsafe { logged_read("macro", reference) }
}
unsafe fn logged_value(name: &str, value: *mut XlOper12) -> *mut XlOper12 {
    let shown = unsafe { output(&*value) };
    log(format!("{name}:{shown}"));
    returned_text(&format!("{name}|{shown}"))
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyUcValue(value: *mut XlOper12) -> *mut XlOper12 {
    unsafe { logged_value("value", value) }
}
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyUcValueSafe(value: *mut XlOper12) -> *mut XlOper12 {
    unsafe { logged_value("valuesafe", value) }
}
/// D.2: a DLLFree result returned after the read, whatever its status.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyUcOwned(reference: *mut XlOper12) -> *mut XlOper12 {
    let (code, shown) = unsafe { read(reference) };
    log(format!("owned:{code}:{shown}"));
    let result = Box::into_raw(Box::new(XlOper12 {
        value: XlValue12 { number: 777. },
        xltype: XL_NUM | DLL_FREE,
    }));
    OWNED.lock().unwrap().push(result as usize);
    result
}
/// Called by xlAutoFree12 first; true when `value` was MyUcOwned's result.
pub(super) unsafe fn free_owned(value: *mut XlOper12) -> bool {
    let mut owned = OWNED.lock().unwrap();
    let Some(index) = owned.iter().position(|&p| p == value as usize) else {
        return false;
    };
    owned.remove(index);
    drop(owned);
    log("free".into());
    drop(unsafe { Box::from_raw(value) });
    true
}
/// D.2: a nested xlUDF whose child reads the pending reference.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyUcParent(reference: *mut XlOper12) -> *mut XlOper12 {
    let mut arena = Arena::new();
    let mut name = arena.text("MyUcRead").unwrap();
    let mut result = sentinel();
    let code = unsafe { call(XL_UDF, &[&mut name, reference], &mut result) };
    let shown = unsafe { output(&result) };
    if code == SUCCESS {
        unsafe { free_result(&mut result) };
    }
    log(format!("parent:{code}:{shown}"));
    returned_text(&format!("{code}|{shown}"))
}
/// Contract boundary: two more callbacks after a status 64.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyUcAgain(reference: *mut XlOper12) -> *mut XlOper12 {
    let (code, shown) = unsafe { read(reference) };
    let mut line = format!("again:{code}:{shown}");
    if code == UNCALCED {
        let (second, shown) = unsafe { read(reference) };
        line.push_str(&format!("|coerce={second}:{shown}"));
        let mut caller = sentinel();
        let third = unsafe { call(XLF_CALLER, &[], &mut caller) };
        line.push_str(&format!("|caller={third}:{}", unsafe { output(&caller) }));
        if third == SUCCESS {
            unsafe { free_result(&mut caller) };
        }
    }
    log(line);
    returned_text(&format!("{code}|{shown}"))
}
/// D.1's mixed call: a reference and a value argument naming the same pending cell.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn MyUcMixed(reference: *mut XlOper12, value: *mut XlOper12) -> *mut XlOper12 {
    let prepared = unsafe { output(&*value) };
    let (code, shown) = unsafe { read(reference) };
    log(format!("mixed:{prepared}:{code}:{shown}"));
    returned_text(&format!("{code}|{shown}"))
}