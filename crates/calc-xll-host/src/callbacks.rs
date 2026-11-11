use crate::native::{Module, Registration, RegistrationInfo, Shared, current_sheet};
use calc_func::UserFunctionContext;
use calc_value::{CalcValue, CoreRef, ExcelText, ReferenceLike, WorksheetErrorCode};
use calc_xll_abi::*;
use std::{
    cell::Cell,
    collections::{BTreeSet, HashMap},
    rc::Rc,
    sync::{
        Arc, Condvar, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

/// Bytes xlStack keeps back from the measured headroom: Excel's 87,119-byte shortfall below the
/// caller's own headroom, less about 2 KiB of host frames between the add-in and this measure.
const XL_STACK_RESERVE: usize = 85_000;
/// What kind of entry a frame belongs to. The kind decides which callbacks are served.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FrameKind {
    /// A registered function or command entered from a cell, a command or `Application.Run`.
    Entry,
    /// xlAutoOpen, xlAutoClose and the other lifecycle exports.
    Lifecycle,
    /// xlAutoFree12 releasing a DLLFree result: only xlFree is served.
    FreeOnly,
}
pub(crate) struct Frame<'a> {
    pub kind: FrameKind,
    pub state: Shared,
    pub module: Rc<Module>,
    pub origin: Rc<Module>,
    pub context: Option<&'a mut dyn UserFunctionContext>,
    pub signature: Option<XllSignature>,
    pub provider: Option<crate::RtdProvider>,
    pub rtd_subscriptions: BTreeSet<calc_func::RtdLease>,
    /// Callback results Excel keeps valid until the calling function returns (xlfCall).
    pub transient: Vec<Arena>,
    /// Set when a callback answered status 64 (`xlretUncalced`) in this attempt: the attempt's
    /// return is discarded (answer 13 B.5), and from then on only xlFree is served.
    pub uncalced: bool,
}
impl Frame<'_> {
    pub fn lifecycle(state: Shared, module: Rc<Module>) -> Self {
        Self {
            kind: FrameKind::Lifecycle,
            state,
            origin: module.clone(),
            module,
            context: None,
            signature: None,
            provider: None,
            rtd_subscriptions: BTreeSet::new(),
            transient: Vec::new(),
            uncalced: false,
        }
    }
    /// The frame xlAutoFree12 runs in: the registration's modules, the calling formula's
    /// context (so a refused callback refuses that formula), no signature.
    pub fn free_only<'b>(
        state: Shared,
        module: Rc<Module>,
        origin: Rc<Module>,
        context: Option<&'b mut dyn UserFunctionContext>,
    ) -> Frame<'b> {
        Frame {
            kind: FrameKind::FreeOnly,
            state,
            module,
            origin,
            context,
            signature: None,
            provider: None,
            rtd_subscriptions: BTreeSet::new(),
            transient: Vec::new(),
            uncalced: false,
        }
    }
}

/// What one callback answers: its exact status, what happens to the caller's result operand,
/// and whether the calling formula becomes a typed refusal. `dispatch` applies it in one place.
pub(crate) struct CallbackReply {
    pub status: i32,
    pub write: ResultWrite,
    pub refusal: Option<String>,
}
pub(crate) enum ResultWrite {
    /// The result operand is not touched.
    Unchanged,
    /// All 32 bytes are zeroed (xlAsyncReturn's accepted completion, Excel20527).
    ZeroAll,
    /// A result operand built by the host; its allocation is already registered.
    Operand(XlOper12),
    /// A failed callback inside a frame: Excel 16.0.20527 writes an xltypeErr into the result
    /// operand: #VALUE! for statuses 2, 32 and 128 (x13 boundary batch, every failed row in the
    /// normal, `#`, `$` and xlAutoOpen contexts), the source's own error when xlCoerce fails on
    /// an error value (x14). Other statuses leave it untouched.
    Error(i32),
}
impl CallbackReply {
    fn status(status: i32) -> Self {
        Self {
            status,
            write: if matches!(status, INVALID_FUNCTION | FAILED | NOT_THREAD_SAFE) {
                ResultWrite::Error(15)
            } else {
                ResultWrite::Unchanged
            },
            refusal: None,
        }
    }
    fn operand(oper: XlOper12) -> Self {
        Self {
            status: SUCCESS,
            write: ResultWrite::Operand(oper),
            refusal: None,
        }
    }
    /// The first status 64 of an attempt: #VALUE! in the result operand (x17), no refusal.
    fn uncalced() -> Self {
        Self {
            status: UNCALCED,
            write: ResultWrite::Error(15),
            refusal: None,
        }
    }
    fn refused(status: i32, why: String) -> Self {
        Self {
            refusal: Some(why),
            ..Self::status(status)
        }
    }
    unsafe fn apply(self, frame: Option<&mut Frame<'_>>, result: *mut XlOper12) -> i32 {
        if let Some(frame) = frame {
            if let Some(why) = &self.refusal {
                refuse(frame, why);
            }
            if result.is_null()
                && let ResultWrite::Operand(oper) = &self.write
            {
                // Nobody receives this operand, so nobody can free it.
                free(&frame.state, oper);
            }
        }
        if !result.is_null() {
            match self.write {
                ResultWrite::Unchanged => {}
                ResultWrite::ZeroAll => unsafe {
                    std::ptr::write_bytes(result.cast::<u8>(), 0, std::mem::size_of::<XlOper12>());
                },
                ResultWrite::Operand(oper) => unsafe { result.write_unaligned(oper) },
                ResultWrite::Error(code) => unsafe {
                    result.write_unaligned(XlOper12::error(code));
                },
            }
        }
        self.status
    }
}
thread_local! { static FRAME:Cell<*mut Frame<'static>>=const{Cell::new(std::ptr::null_mut())}; }
/// The cell (zero-based row and column) the native worksheet call running on this thread was
/// entered for; `None` outside a call or when the call has no cell caller. This is what
/// `Application.Caller` of the COM projection answers while an add-in's code runs.
pub fn running_caller() -> Option<(u32, u32)> {
    let pointer = FRAME.with(|f| f.get());
    if pointer.is_null() {
        return None;
    }
    // SAFETY: a frame outlives its guard and never leaves its thread (see `enter`).
    let frame = unsafe { &*pointer };
    let cell = frame.context.as_ref().and_then(|context| context.caller())?;
    Some((cell.row().checked_sub(1)?, cell.column().checked_sub(1)?))
}
pub(crate) struct FrameGuard(*mut Frame<'static>);
impl Drop for FrameGuard {
    fn drop(&mut self) {
        FRAME.with(|f| f.set(self.0));
    }
}
/// The caller must keep the frame alive and exclusively accessible through callbacks until
/// the returned guard is dropped. A frame and its guard never leave their owning thread.
pub(crate) unsafe fn enter(frame: &mut Frame<'_>) -> FrameGuard {
    let ptr = frame as *mut Frame<'_> as *mut Frame<'static>;
    FrameGuard(FRAME.with(|f| f.replace(ptr)))
}

pub(crate) struct Completion {
    pub result: Mutex<Option<Result<CalcValue, String>>>,
    pub ready: Condvar,
    pub forwarded: AtomicBool,
    pub active: AtomicBool,
    pub physical_returned: AtomicBool,
}
pub(crate) struct PendingOwner {
    pub completion: Arc<Completion>,
    pub registration: Rc<Registration>,
}
static ASYNC: OnceLock<Mutex<HashMap<usize, Arc<Completion>>>> = OnceLock::new();
static NEXT_ASYNC: AtomicUsize = AtomicUsize::new(1);
fn asynchronous() -> &'static Mutex<HashMap<usize, Arc<Completion>>> {
    ASYNC.get_or_init(|| Mutex::new(HashMap::new()))
}
pub(crate) struct Pending {
    pub key: usize,
    pub completion: Arc<Completion>,
    state: std::rc::Weak<std::cell::RefCell<crate::native::State>>,
    pub entered: bool,
}
impl Drop for Pending {
    fn drop(&mut self) {
        if let Some(state) = self.state.upgrade() {
            let lock = self.completion.result.lock().unwrap();
            let outstanding =
                self.entered && !self.completion.physical_returned.load(Ordering::Acquire);
            self.completion.active.store(false, Ordering::Release);
            drop(lock);
            if outstanding {
                // Revocation ends authorization, not execution in native code.
                // Keep the registration/module lease and a refusing C API tombstone
                // until that task attempts its return (or the process exits).
                return;
            }
            state.borrow_mut().pending_async.remove(&self.key);
        }
        cancel(self.key);
    }
}
pub(crate) fn pending() -> Result<Pending, String> {
    let completion = Arc::new(Completion {
        result: Mutex::new(None),
        ready: Condvar::new(),
        forwarded: AtomicBool::new(false),
        active: AtomicBool::new(true),
        physical_returned: AtomicBool::new(false),
    });
    // Never reuse an address/token: a late completion must not become a later call's result.
    let key = NEXT_ASYNC
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
        .map_err(|_| "dc-snz.2: async handle space exhausted")?;
    asynchronous()
        .lock()
        .unwrap()
        .insert(key, completion.clone());
    Ok(Pending {
        key,
        completion,
        state: std::rc::Weak::new(),
        entered: false,
    })
}
pub(crate) fn owned_pending(
    state: &Shared,
    registration: &Rc<Registration>,
) -> Result<Pending, String> {
    let mut pending = pending()?;
    pending.state = Rc::downgrade(state);
    state.borrow_mut().pending_async.insert(
        pending.key,
        PendingOwner {
            completion: pending.completion.clone(),
            registration: registration.clone(),
        },
    );
    Ok(pending)
}
pub(crate) fn wait(key: usize, completion: &Completion) -> Result<CalcValue, String> {
    let lock = completion
        .result
        .lock()
        .map_err(|_| "async result lock poisoned")?;
    let (mut lock, status) = completion
        .ready
        .wait_timeout_while(lock, Duration::from_secs(10), |r| r.is_none())
        .map_err(|_| "async result lock poisoned")?;
    completion.active.store(false, Ordering::Release);
    if status.timed_out() && lock.is_none() {
        return Err("dc-snz.2: XLL async completion timed out".into());
    }
    asynchronous().lock().unwrap().remove(&key);
    lock.take().ok_or("missing async completion")?
}
pub(crate) fn cancel(key: usize) {
    let held = asynchronous().lock().unwrap().remove(&key);
    if let Some(held) = held {
        let _lock = held.result.lock().unwrap();
        held.active.store(false, Ordering::Release);
    }
}

/// Main executable's MdCallBack12 export forwards here. No registry is global: the active
/// invocation supplies it. Only asynchronous completion handles cross the thread boundary.
/// # Safety
/// Arguments must be live ABI operands for the duration of the call; result must be writable
/// when nonnull. An untrusted native DLL cannot be isolated by this in-process adapter.
pub unsafe fn dispatch(
    function: i32,
    count: i32,
    args: *const *mut XlOper12,
    result: *mut XlOper12,
) -> i32 {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
        if !(0..=255).contains(&count) {
            return INVALID_COUNT;
        }
        if count > 0 && args.is_null() {
            return INVALID_OPER;
        }
        let pointers: Vec<_> = (0..count as usize)
            .map(|i| args.add(i).read_unaligned())
            .collect();
        let args = pointers.as_slice();
        if args.iter().any(|a| a.is_null()) {
            return INVALID_OPER;
        }
        if function == XL_ASYNC_RETURN {
            return async_return(args, result);
        }
        FRAME.with(|active| {
            let ptr = active.get();
            if ptr.is_null() {
                return FAILED;
            }
            let frame = &mut *ptr;
            if frame.kind == FrameKind::FreeOnly && function != XL_FREE_FN {
                // Excel 16.0.20527 terminated when xlAutoFree12 called xlStack, xlGetName,
                // xlfCaller and SUM (x13 batch, MyFreeFrame(2)); xlFree there returns 0. The host
                // cannot crash in Excel's place: it fails the callback and refuses the formula.
                return CallbackReply::refused(
                    FAILED,
                    format!("dc-6332.13: callback {function} from xlAutoFree12 terminates Excel 16.0.20527"),
                )
                .apply(Some(frame), result);
            }
            if frame.uncalced && function != XL_FREE_FN {
                // After status 64 an add-in may only free and return (Microsoft's xlretUncalced
                // contract). Excel 16.0.20527 answers anything else with 64 again and leaves the
                // result operand untouched (x17 MyUcAgain: a second xlCoerce and xlfCaller).
                return CallbackReply::status(UNCALCED).apply(Some(frame), result);
            }
            if matches!(function, XLF_CALL | XLF_REGISTER_ID) {
                return crate::call::dispatch(frame, function, args, result);
            }
            let reply = answer(frame, function, args, result.is_null());
            reply.apply(Some(frame), result)
        })
    }))
    .unwrap_or(FAILED)
}

pub(crate) fn refuse_frame(frame: &mut Frame<'_>, why: &str) {
    refuse(frame, why)
}
fn refuse(frame: &mut Frame<'_>, why: &str) {
    if let Some(context) = &mut frame.context {
        context.refuse_by(
            "an XLL callback is not modelled",
            if why.starts_with("dc-snz.18:") {
                "dc-snz.18"
            } else if why.starts_with("dc-snz.23:") {
                "dc-snz.23"
            } else if why.starts_with("dc-gh22:") {
                "dc-gh22"
            } else if why.starts_with("dc-6332.13:") {
                "dc-6332.13"
            } else {
                "dc-snz.2"
            },
        );
    }
    frame.state.borrow_mut().refusal(why);
}
fn err(why: impl Into<String>) -> (i32, String) {
    (FAILED, why.into())
}
unsafe fn sheet_callback(
    frame: &mut Frame<'_>,
    function: i32,
    args: &[*mut XlOper12],
) -> CallbackReply {
    if args.len() > 1 || (function == XL_SHEET_NM && args.len() != 1) {
        return CallbackReply::status(INVALID_COUNT);
    }
    let selected = current_sheet(&frame.context).or(frame.state.borrow().active_sheet);
    let sheet = if let Some(arg) = args.first() {
        let oper = unsafe { (*arg).read_unaligned() };
        match oper.xltype & TYPE_MASK {
            XL_SREF => selected,
            XL_REF => frame
                .state
                .borrow()
                .sheets
                .decode(unsafe { oper.value.reference.sheet }),
            XL_STR if function == XL_SHEET_ID => {
                let value = unsafe { from_oper(*arg, &frame.state.borrow().sheets, selected) };
                let Ok(name) = value.and_then(|v| text(&v).map_err(|(_, why)| why)) else {
                    return CallbackReply::status(INVALID_OPER);
                };
                frame
                    .state
                    .borrow()
                    .sheet_names
                    .iter()
                    .find_map(|(s, n)| n.eq_ignore_ascii_case(&name).then_some(*s))
            }
            _ => return CallbackReply::status(INVALID_OPER),
        }
    } else {
        selected
    };
    let Some(sheet) = sheet else {
        return CallbackReply::status(FAILED);
    };
    if function == XL_SHEET_ID {
        let id = frame.state.borrow_mut().sheets.encode(sheet);
        CallbackReply::operand(XlOper12 {
            value: XlValue12 {
                reference: XlReference12 {
                    table: std::ptr::null_mut(),
                    sheet: id,
                },
            },
            xltype: XL_REF,
        })
    } else {
        let name = frame.state.borrow().sheet_names.get(&sheet).cloned();
        let Some(name) = name else {
            return CallbackReply::status(FAILED);
        };
        let value = CalcValue::text(ExcelText::from_interop_assignment(&name));
        match unsafe { publish(frame, function, args, &value) } {
            Ok(oper) => CallbackReply::operand(oper),
            Err(_) => CallbackReply::status(FAILED),
        }
    }
}
fn text(value: &CalcValue) -> Result<String, (i32, String)> {
    match value.core_ref() {
        CoreRef::Text(t) => String::from_utf16(t.utf16_code_units())
            .map_err(|_| err("invalid UTF-16 callback text")),
        _ => Err(err("callback requires text")),
    }
}
pub(crate) fn allocation_key(oper: &XlOper12) -> Option<usize> {
    unsafe {
        match oper.xltype & TYPE_MASK {
            XL_STR => Some(oper.value.string as usize),
            XL_MULTI => Some(oper.value.array.values as usize),
            XL_REF => Some(oper.value.reference.table as usize),
            _ => None,
        }
    }
}
/// The result operand for a callback's value. Str/Multi/Ref storage is registered as a
/// callback allocation until the add-in calls xlFree on it (or its module is removed).
unsafe fn publish(
    frame: &mut Frame<'_>,
    function: i32,
    args: &[*mut XlOper12],
    value: &CalcValue,
) -> Result<XlOper12, String> {
    if function == XL_GET_HWND {
        return Ok(XlOper12 {
            value: XlValue12 {
                integer: frame.state.borrow().hwnd as i32,
            },
            xltype: XL_INT,
        });
    }
    let integer_coercion = function == XL_COERCE
        && args.get(1).is_some_and(|p| {
            let kind = unsafe { (*p).read_unaligned() };
            match kind.xltype & TYPE_MASK {
                XL_INT => unsafe { kind.value.integer == XL_INT as i32 },
                XL_NUM => unsafe { kind.value.number == XL_INT as f64 },
                _ => false,
            }
        });
    if function == XL_STACK || function == XL_EVENT_REGISTER || integer_coercion {
        return Ok(XlOper12::integer(value.as_number().unwrap_or(0.) as i32));
    }
    if function == XLF_CALLER {
        if let Some(cell) = frame.context.as_ref().and_then(|c| c.caller()) {
            return Ok(XlOper12 {
                value: XlValue12 {
                    single_reference: XlSRef12 {
                        count: 1,
                        rectangle: XlRef12 {
                            row_first: (cell.row() - 1) as i32,
                            row_last: (cell.row() - 1) as i32,
                            col_first: (cell.column() - 1) as i32,
                            col_last: (cell.column() - 1) as i32,
                        },
                    },
                },
                xltype: XL_SREF,
            });
        }
    }
    let mut arena = Arena::new();
    let oper = arena.oper(value, &mut frame.state.borrow_mut().sheets)?;
    if let Some(key) = allocation_key(&oper) {
        let allocations = frame.state.borrow().allocations.clone();
        allocations.lock().unwrap().insert(
            key,
            crate::native::CallbackAllocation {
                owner: frame.origin.handle as usize,
                _arena: arena,
            },
        );
    }
    Ok(oper)
}
/// Release the host storage behind one operand, if the host allocated it.
pub(crate) fn free(state: &Shared, oper: &XlOper12) {
    if let Some(key) = allocation_key(oper) {
        let allocations = state.borrow().allocations.clone();
        allocations.lock().unwrap().remove(&key);
    }
}
/// xlFree on the caller's own operand, as Excel 16.0.20527 does it (x13 batch, MyFreeProbe):
/// the storage is released, the pointer word of a Str/Multi operand becomes null, the type
/// becomes xltypeMissing for every kind (Num, SRef and Ref too), and the other value bytes are
/// kept. A second xlFree of the same operand then succeeds and releases nothing. The Ref table
/// pointer is nulled as the documentation says; the batch only saw Refs with a null table.
unsafe fn free_operand(state: &Shared, pointer: *mut XlOper12) {
    let oper = unsafe { pointer.read_unaligned() };
    free(state, &oper);
    unsafe {
        if matches!(oper.xltype & TYPE_MASK, XL_STR | XL_MULTI | XL_REF) {
            pointer.cast::<usize>().write_unaligned(0);
        }
        std::ptr::addr_of_mut!((*pointer).xltype).write_unaligned(XL_MISSING);
    }
}

unsafe fn async_return(args: &[*mut XlOper12], result: *mut XlOper12) -> i32 {
    if args.len() != 2 {
        return INVALID_COUNT;
    }
    let handle = unsafe { args[0].read_unaligned() };
    if handle.xltype & TYPE_MASK != XL_BIG_DATA {
        return INVALID_ASYNC;
    }
    let key = unsafe { handle.value.big_data.data as usize };
    let completion = asynchronous().lock().unwrap().get(&key).cloned();
    let Some(completion) = completion else {
        return INVALID_ASYNC;
    };
    let mut lock = completion.result.lock().unwrap();
    completion.physical_returned.store(true, Ordering::Release);
    if lock.is_some() || !completion.active.load(Ordering::Acquire) {
        return INVALID_ASYNC;
    }
    // Async completions are values, never references to another thread's sheet context.
    // A revoked handle never dereferences the supplied result operand.
    let value = unsafe { from_oper(args[1], &SheetMap::default(), None) };
    *lock = Some(value);
    completion.ready.notify_one();
    // Excel20527 clears all result bytes on successful completion, including
    // an operand initially containing a number.
    let reply = CallbackReply {
        status: SUCCESS,
        write: ResultWrite::ZeroAll,
        refusal: None,
    };
    unsafe { reply.apply(None, result) }
}

/// Permission denials measured on Excel 16.0.20527: Excel's own answers to a worksheet
/// function's callback, not missing host capabilities. The add-in sees the status (and #VALUE!
/// in its result operand); the formula keeps the add-in's result and is never a refusal.
///
/// | callback | normal | `#` | `$` | evidence |
/// |---|---|---|---|---|
/// | 237 xlfVolatile, 186 GET.WORKSPACE | 2 | served | 2 | X5 matrix, x13 |
/// | 257 EVALUATE | served | served | 128 | X5, x13 |
/// | 32890 MESSAGE | 2 | served (FALSE) | 128 | X5, x13 |
/// | other commands (`0x8000`, ECHO 32909) | 2 | refused: Excel runs them | 128 | x13 |
/// | 255 xlUDF | served | served or refused (cached-read change) | 32 | x13 |
/// | 88 SET.NAME | 2 | served | 2 | x13 |
/// | 16391 xlGetInst | 32 | 32 | 32 | x13 (also 32 in xlAutoOpen) |
///
/// Still refused from `$` because Excel's answer is unmeasured or not modelled: 16401 event
/// registration (Excel: 0), 379 RTD, 149 REGISTER, 201 UNREGISTER (see `run`).
fn measured_denial(frame: &Frame<'_>, function: i32) -> Option<i32> {
    if function == XL_GET_INST {
        return Some(FAILED);
    }
    let signature = frame.signature.as_ref()?;
    let (macro_equivalent, thread_safe) = (signature.macro_equivalent, signature.thread_safe);
    match function {
        XLF_VOLATILE | XLF_GET_WORKSPACE if !macro_equivalent => Some(INVALID_FUNCTION),
        XLF_EVALUATE if thread_safe => Some(NOT_THREAD_SAFE),
        f if f & XL_COMMAND != 0 && thread_safe => Some(NOT_THREAD_SAFE),
        f if f & XL_COMMAND != 0 && !macro_equivalent => Some(INVALID_FUNCTION),
        XL_UDF if thread_safe => Some(FAILED),
        XLF_SET_NAME if !macro_equivalent => Some(INVALID_FUNCTION),
        _ => None,
    }
}

/// The reply to one callback inside a frame (xlfCall and REGISTER.ID excepted, `call.rs`).
unsafe fn answer(
    frame: &mut Frame<'_>,
    function: i32,
    args: &[*mut XlOper12],
    no_result: bool,
) -> CallbackReply {
    if let Some(status) = measured_denial(frame, function) {
        return CallbackReply::status(status);
    }
    if function == XL_GET_INST_PTR {
        if !args.is_empty() {
            return CallbackReply::status(INVALID_COUNT);
        }
        return CallbackReply::operand(XlOper12 {
            value: XlValue12 {
                big_data: XlBigData {
                    data: unsafe { crate::native::GetModuleHandleW(std::ptr::null()) }.cast(),
                    byte_count: 0,
                },
            },
            xltype: XL_BIG_DATA,
        });
    }
    if matches!(function, XL_SHEET_ID | XL_SHEET_NM) {
        return unsafe { sheet_callback(frame, function, args) };
    }
    if function == XL_COERCE {
        return unsafe { coerce_callback(frame, args, no_result) };
    }
    match run(frame, function, args) {
        Ok(Some(_)) if no_result => CallbackReply::status(SUCCESS),
        Ok(Some(value)) => match unsafe { publish(frame, function, args, &value) } {
            Ok(oper) => CallbackReply::operand(oper),
            Err(why) => CallbackReply::refused(FAILED, why),
        },
        Ok(None) => CallbackReply::status(SUCCESS),
        // A nested call (xlUDF) whose own attempt got status 64: Excel 16.0.20527 answers the
        // parent's xlUDF with 64 and #VALUE! and enters both again after the producer (x17
        // MyUcParent). The parent's attempt is now unwound too.
        Err((UNCALCED, _)) => {
            frame.uncalced = true;
            CallbackReply::uncalced()
        }
        Err((code, why)) => CallbackReply::refused(code, why),
    }
}

fn run(
    frame: &mut Frame<'_>,
    function: i32,
    args: &[*mut XlOper12],
) -> Result<Option<CalcValue>, (i32, String)> {
    if function == XL_FREE_FN {
        for &pointer in args {
            unsafe { free_operand(&frame.state, pointer) };
        }
        return Ok(None);
    }
    // Measured denials were answered before `run` (`measured_denial`); what reaches here from a
    // worksheet function is either served or a capability the host does not model.
    if frame.signature.is_some() && function & XL_COMMAND != 0 && function != XLC_MESSAGE {
        // Only `#` entries get here; Excel 16.0.20527 runs ECHO for them (x13: 0, FALSE).
        return Err((
            INVALID_FUNCTION,
            "dc-snz.18: commands from a macro-equivalent worksheet function are not modelled"
                .into(),
        ));
    }
    if frame.signature.as_ref().is_some_and(|s| s.thread_safe) && function == XL_EVENT_REGISTER {
        return Err((
            NOT_THREAD_SAFE,
            "dc-snz.18: event registration from calculation workers is not modelled".into(),
        ));
    }
    if frame.signature.as_ref().is_some_and(|s| s.thread_safe) && function == XLF_RTD {
        return Err((
            NOT_THREAD_SAFE,
            "dc-snz.23: RTD requires the owning STA".into(),
        ));
    }
    if frame.signature.as_ref().is_some_and(|s| s.thread_safe)
        && matches!(function, XLF_REGISTER | XLF_UNREGISTER)
    {
        return Err((
            NOT_THREAD_SAFE,
            format!("callback {function} from a thread-safe function"),
        ));
    }
    let values = {
        let state = frame.state.borrow();
        args.iter()
            .map(|p| {
                unsafe { from_oper(*p, &state.sheets, current_sheet(&frame.context)) }.map_err(err)
            })
            .collect::<Result<Vec<_>, _>>()?
    };
    let require = |n: usize| {
        if args.len() == n {
            Ok(())
        } else {
            Err((
                INVALID_COUNT,
                format!("callback {function}: expected {n} arguments"),
            ))
        }
    };
    let value = match function {
        XLC_MESSAGE => {
            if !(1..=2).contains(&args.len()) {
                return Err((INVALID_COUNT, "MESSAGE expects one or two arguments".into()));
            }
            let show = values[0]
                .as_logical()
                .unwrap_or_else(|| values[0].as_number().is_some_and(|n| n != 0.));
            frame.state.borrow_mut().status_message = if show {
                values.get(1).map(text).transpose()?
            } else {
                None
            };
            // Excel 16.0.20527 answers MESSAGE from a `#` function with status 0 and FALSE (x13).
            CalcValue::logical(false)
        }
        XL_GET_NAME => {
            require(0)?;
            CalcValue::text(ExcelText::from_interop_assignment(
                frame.module.path.to_string_lossy().as_ref(),
            ))
        }
        XL_ABORT => {
            if args.len() > 1 {
                return Err((INVALID_COUNT, "xlAbort expects zero or one argument".into()));
            }
            CalcValue::logical(false)
        }
        XL_STACK => {
            require(0)?;
            let mut low = 0usize;
            let mut high = 0usize;
            unsafe {
                crate::native::GetCurrentThreadStackLimits(&mut low, &mut high);
            }
            let current = &low as *const usize as usize;
            if !(low..high).contains(&current) {
                return Err(err("cannot establish current stack bounds"));
            }
            // Excel 16.0.20527 answers the caller's raw headroom less 87,119 bytes in every
            // context (main thread, worker, xlAutoOpen; x13 batch): no 64 KiB cap. The host
            // measures here, below the add-in's frame, so it keeps the rest of that reserve.
            CalcValue::number(
                current
                    .saturating_sub(low)
                    .saturating_sub(XL_STACK_RESERVE)
                    .min(i32::MAX as usize) as f64,
            )
        }
        XLF_CALLER => {
            require(0)?;
            match frame.context.as_ref().and_then(|c| c.caller()) {
                Some(c) => CalcValue::reference(ReferenceLike::cell(c)),
                None => CalcValue::error(WorksheetErrorCode::Ref),
            }
        }
        XLF_GET_WORKSPACE => {
            require(1)?;
            match values[0].as_number() {
                Some(2.) => CalcValue::text(ExcelText::from_interop_assignment("16.0")),
                Some(37.) => crate::international::settings().map_err(err)?,
                _ => {
                    return Err((
                        INVALID_FUNCTION,
                        "dc-snz.2: GET.WORKSPACE item not modelled".into(),
                    ));
                }
            }
        }
        XL_GET_HWND => {
            require(0)?;
            CalcValue::number(frame.state.borrow().hwnd as f64)
        }
        XL_RUNNING_ON_CLUSTER => {
            require(0)?;
            CalcValue::logical(false)
        }
        XLF_VOLATILE => {
            if args.len() > 1 {
                return Err((
                    INVALID_COUNT,
                    "VOLATILE expects zero or one argument".into(),
                ));
            }
            let volatile = values
                .first()
                .map(|v| {
                    v.as_logical()
                        .unwrap_or(v.as_number().is_some_and(|n| n != 0.))
                })
                .unwrap_or(true);
            if let Some(context) = &mut frame.context {
                context.set_volatile(volatile);
            }
            CalcValue::logical(true)
        }
        XLF_REGISTER => return register(frame, &values).map(Some),
        XL_EVENT_REGISTER => {
            require(2)?;
            let event = values[1]
                .as_number()
                .ok_or_else(|| err("event ID requires a number"))? as i32;
            if !matches!(event, 1 | 2) {
                return Err((INVALID_OPER, "unknown XLL event".into()));
            }
            let key = (frame.origin.handle as usize, event);
            if values[0].is_empty() || values[0].is_missing() {
                frame.state.borrow_mut().events.remove(&key);
            } else {
                let address = frame.module.address(&text(&values[0])?).map_err(err)?;
                frame.state.borrow_mut().events.insert(key, address);
            }
            CalcValue::number(1.)
        }
        XLF_UNREGISTER => {
            require(1)?;
            let id = values[0]
                .as_number()
                .ok_or_else(|| err("UNREGISTER requires a register ID"))?
                as u64;
            let state = frame.state.borrow();
            match state.registrations.get(&id) {
                Some(r) if r.uses.get() > 0 => {
                    r.uses.set(r.uses.get() - 1);
                    CalcValue::logical(true)
                }
                _ => CalcValue::error(WorksheetErrorCode::Value),
            }
        }
        XLF_SET_NAME => {
            if !(1..=2).contains(&args.len()) {
                return Err((
                    INVALID_COUNT,
                    "SET.NAME expects one or two arguments".into(),
                ));
            }
            if args.len() == 2 && !values[1].is_missing() {
                return Err((INVALID_FUNCTION, "dc-snz.2: SET.NAME assignment".into()));
            }
            frame
                .state
                .borrow_mut()
                .names
                .remove(&calc_value::fold_workbook_name(&text(&values[0])?));
            CalcValue::logical(true)
        }
        XLF_REFTEXT => {
            // REFTEXT uses the same ADDRESS kernel and qualifier formatter as
            // worksheet formulas and the COM Range projection.
            require(2)?;
            let Some(reference) = values[0].as_reference() else {
                return Ok(Some(CalcValue::error(WorksheetErrorCode::Value)));
            };
            let [area] = reference.areas() else {
                return Err(err("dc-snz.2: multi-area REFTEXT is not modelled"));
            };
            let style = coerce(values[1].clone(), XL_BOOL)?;
            let name = frame
                .state
                .borrow()
                .sheet_names
                .get(&area.sheet())
                .cloned()
                .ok_or_else(|| err("REFTEXT sheet has no name"))?;
            let qualifier = if let Some((book, sheet)) =
                name.strip_prefix('[').and_then(|s| s.split_once(']'))
            {
                calc_fml::reference::format_workbook_sheet_qualifier(book, sheet)
            } else {
                return Err(err("dc-snz.2: REFTEXT needs a workbook-qualified sheet"));
            };
            let address = |cell: calc_value::CellKey| -> Result<String, (i32, String)> {
                let value = worksheet_value(
                    "ADDRESS",
                    &[
                        CalcValue::number(f64::from(cell.row())),
                        CalcValue::number(f64::from(cell.column())),
                        CalcValue::number(1.),
                        style.clone(),
                    ],
                )
                .map_err(err)?;
                text(&value)
            };
            let mut result = format!("{qualifier}!{}", address(area.first())?);
            if area.first() != area.last() {
                result.push(':');
                result.push_str(&address(area.last())?);
            }
            CalcValue::text(ExcelText::from_interop_assignment(&result))
        }
        XLF_EVALUATE => {
            require(1)?;
            let expression = text(&values[0])?;
            let name = calc_value::fold_workbook_name(&expression);
            let registered = frame.state.borrow().names.get(&name).copied();
            if let Some(id) = registered {
                CalcValue::number(id as f64)
            } else if let Some(context) = frame.context.as_mut() {
                context
                    .evaluate_constant_expression(&expression, "dc-snz.2")
                    .map_err(err)?
            } else {
                return Err(err(
                    "dc-snz.2: EVALUATE expression requires an owning formula invocation",
                ));
            }
        }
        XL_UDF => {
            if args.is_empty() {
                return Err((INVALID_COUNT, "xlUDF requires an ID or name".into()));
            }
            let registration = {
                let state = frame.state.borrow();
                let id = if let Some(id) = values[0].as_number() {
                    Some(id as u64)
                } else {
                    state
                        .names
                        .get(&calc_value::fold_workbook_name(&text(&values[0])?))
                        .copied()
                };
                id.and_then(|id| state.registrations.get(&id).cloned())
            };
            let Some(registration) = registration else {
                return Ok(Some(CalcValue::error(WorksheetErrorCode::Name)));
            };
            if registration.uses.get() == 0 {
                return Ok(Some(CalcValue::error(WorksheetErrorCode::Name)));
            }
            if registration.info.macro_type == 2 && frame.signature.is_some() {
                return Err((
                    INVALID_FUNCTION,
                    "dc-snz.18: worksheet functions cannot invoke registered commands".into(),
                ));
            }
            if frame.context.is_some()
                && frame.signature.as_ref().is_some_and(|s| s.macro_equivalent)
                    != registration.info.signature.macro_equivalent
            {
                return Err((INVALID_FUNCTION,
                    "dc-snz.18: nested registered calls that change cached-read mode are not modelled".into()));
            }
            let adapter = crate::native::XllFunction {
                early: false,
                parallel: 1,
                state: frame.state.clone(),
                registration,
                bound: false,
            };
            match &mut frame.context {
                Some(context) => adapter.invoke(&values[1..], Some(&mut **context)),
                None => adapter.invoke(&values[1..], None),
            }
            .map_err(|why| {
                if why == crate::invoke::UNCALCED_RETURN {
                    (UNCALCED, why)
                } else {
                    err(why)
                }
            })?
        }
        XLF_RTD => {
            if values.len() < 3 {
                return Err((INVALID_COUNT, "RTD needs a ProgID, server and topic".into()));
            }
            let _provider = frame
                .provider
                .ok_or_else(|| err("dc-snz.23: RTD requires an admitted callable identity"))?;
            let context = frame
                .context
                .as_mut()
                .ok_or_else(|| err("dc-snz.23: RTD requires an admitted input view"))?;
            let mut resolved = Vec::with_capacity(values.len());
            for value in &values {
                resolved.push(match value.as_reference() {
                    Some(reference) => context.argument_values_of(reference).map_err(err)?,
                    None => value.clone(),
                });
            }
            // The one rule for both RTD routes (calc_func::read_rtd_call): topics coerce to text
            // as the typed RTD() does (the number 1 and the text "1" are one topic, measured),
            // the owner's connector joins or starts the topic, and the value shows at once, as
            // ConnectData's own while it awaits admission; the read stays uncertified until the
            // owner admits that sequence.
            let read = calc_func::read_rtd_call(&mut **context, &resolved, false).map_err(err)?;
            if let Some(lease) = read.lease {
                frame.rtd_subscriptions.insert(lease);
            }
            read.value
        }
        // Ftab numbers come from the existing file-formula table. Dispatch is through the
        // evaluator's ordinary worksheet route, with its current numeric and read policy.
        other => {
            let name = crate::invoke::worksheet_name(other).ok_or_else(|| {
                (
                    INVALID_FUNCTION,
                    format!("dc-snz.2: unsupported C API callback {other}"),
                )
            })?;
            if let Some(context) = frame.context.as_mut() {
                context
                    .worksheet_function_for(name, &values, "dc-snz.2")
                    .map_err(err)?
            } else {
                // Lifecycle callbacks still use the ordinary kernel. Without an
                // evaluator there are no cell, workbook, random or clock services.
                worksheet_value(name, &values).map_err(err)?
            }
        }
    };
    Ok(Some(value))
}

/// xlCoerce as Excel 16.0.20527 answers it (x14 operand batch, `x14-operand-cases.json`): the
/// second operand is a mask of acceptable types, not one type. A reference is returned as it
/// is when the mask allows Ref, read otherwise (a multi-area reference reads as #VALUE!); an
/// array is kept when the mask allows Multi, else its top-left element is used; a value whose
/// kind the mask allows is returned unchanged; otherwise the first possible conversion in the
/// order Num, Str, Bool, Int, Multi, Err is taken. Measured failures are status 32 with the
/// source's error (an error value) or #VALUE! (a failed conversion) in the result operand, and
/// never refuse the formula.
unsafe fn coerce_callback(
    frame: &mut Frame<'_>,
    args: &[*mut XlOper12],
    no_result: bool,
) -> CallbackReply {
    if !(1..=2).contains(&args.len()) {
        return CallbackReply::refused(
            INVALID_COUNT,
            "xlCoerce expects one or two arguments".into(),
        );
    }
    let values = {
        let state = frame.state.borrow();
        match args
            .iter()
            .map(|p| unsafe { from_oper(*p, &state.sheets, current_sheet(&frame.context)) })
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(values) => values,
            Err(why) => return CallbackReply::refused(FAILED, why),
        }
    };
    let mask = match values.get(1) {
        None => None,
        Some(kind) => match kind.as_number() {
            Some(n) if n >= 0. && n.fract() == 0. && n <= f64::from(u16::MAX) => Some(n as u32),
            _ => {
                return CallbackReply::refused(
                    FAILED,
                    "dc-snz.2: xlCoerce with a non-numeric type operand is not modelled".into(),
                );
            }
        },
    };
    if mask.is_some_and(|m| {
        m & !(XL_NUM | XL_STR | XL_BOOL | XL_REF | XL_ERR | XL_MULTI | XL_INT) != 0
    }) {
        return CallbackReply::refused(
            FAILED,
            "dc-snz.2: xlCoerce to Missing, Nil, SRef or flow types is not modelled".into(),
        );
    }
    let mut value = values[0].clone();
    if let Some(reference) = value.as_reference() {
        if mask.is_some_and(|m| m & XL_REF != 0) {
            return reply_value(frame, args, value, false, no_result);
        }
        value = if reference.areas().len() > 1 {
            CalcValue::error(WorksheetErrorCode::Value)
        } else {
            let read = match frame.context.as_mut() {
                Some(context) => context.read_reference(reference),
                None => {
                    return CallbackReply::refused(
                        FAILED,
                        "dc-snz.2: xlCoerce reference outside formula".into(),
                    );
                }
            };
            match read {
                Ok(calc_func::ReferenceRead::Value(value)) => value,
                // Status 64: the producer is not calculated yet. The read has asked the engine to
                // evaluate this formula again after it; the add-in must now return. Excel
                // 16.0.20527 writes #VALUE! into the result operand (x17, every row), and nothing
                // is refused.
                Ok(calc_func::ReferenceRead::Pending) => {
                    frame.uncalced = true;
                    return CallbackReply::uncalced();
                }
                Err(why) => return CallbackReply::refused(FAILED, why),
            }
        };
    }
    let Some(mask) = mask else {
        return reply_value(frame, args, value, false, no_result);
    };
    match coerce_mask(value, mask) {
        Ok((value, integer)) => reply_value(frame, args, value, integer, no_result),
        Err(Coerced::Failed(code)) => CallbackReply {
            status: FAILED,
            write: ResultWrite::Error(code),
            refusal: None,
        },
        Err(Coerced::NotModelled(why)) => CallbackReply::refused(FAILED, why),
    }
}
enum Coerced {
    /// Excel's status 32 with this error in the result operand.
    Failed(i32),
    NotModelled(String),
}
fn reply_value(
    frame: &mut Frame<'_>,
    args: &[*mut XlOper12],
    value: CalcValue,
    integer: bool,
    no_result: bool,
) -> CallbackReply {
    if no_result {
        return CallbackReply::status(SUCCESS);
    }
    if integer {
        return CallbackReply::operand(XlOper12::integer(value.as_number().unwrap_or(0.) as i32));
    }
    match unsafe { publish(frame, XL_COERCE, &args[..1], &value) } {
        Ok(oper) => CallbackReply::operand(oper),
        Err(why) => CallbackReply::refused(FAILED, why),
    }
}
/// One value against a type mask; the bool says the answer is an xltypeInt.
fn coerce_mask(value: CalcValue, mask: u32) -> Result<(CalcValue, bool), Coerced> {
    if value.as_array().is_some() && mask & XL_MULTI != 0 {
        return Ok((value, false));
    }
    let value = match value.as_array() {
        Some(array) => array.get(0, 0).cloned().unwrap_or_else(CalcValue::empty),
        None => value,
    };
    let kind = match value.core_ref() {
        CoreRef::Number(_) => XL_NUM,
        CoreRef::Text(_) => XL_STR,
        CoreRef::Logical(_) => XL_BOOL,
        CoreRef::Error(_) => XL_ERR,
        _ => 0,
    };
    if kind != 0 && mask & kind != 0 {
        return Ok((value, false));
    }
    if let CoreRef::Error(code) = value.core_ref() {
        // An error converts only into an array holding it (x14: #N/A and #VALUE! to Multi).
        if mask & XL_MULTI != 0 {
            return Ok((
                CalcValue::array(calc_value::CalcArray::from_scalar(value)),
                false,
            ));
        }
        return Err(Coerced::Failed(error_operand_code(code)));
    }
    for target in [XL_NUM, XL_STR, XL_BOOL, XL_INT, XL_MULTI, XL_ERR] {
        if mask & target == 0 {
            continue;
        }
        if target == XL_ERR {
            // Any non-error value coerces to #VALUE! with status 0.
            return Ok((CalcValue::error(WorksheetErrorCode::Value), false));
        }
        match coerce(value.clone(), target) {
            Ok(converted) => return Ok((converted, target == XL_INT)),
            Err((_, why)) if why.starts_with("dc-snz.20:") => {
                return Err(Coerced::NotModelled(why));
            }
            Err(_) => continue,
        }
    }
    // Every allowed conversion failed (or the mask holds Ref alone for a value).
    Err(Coerced::Failed(15))
}
/// The XLOPER12 error code of a worksheet error (#VALUE! for one without an Excel 12 code).
fn error_operand_code(code: WorksheetErrorCode) -> i32 {
    calc_xll_abi::error_number(code).unwrap_or(15)
}
pub(crate) fn coerce(value: CalcValue, kind: u32) -> Result<CalcValue, (i32, String)> {
    let matches = match kind {
        XL_NUM => value.as_number().is_some(),
        XL_INT => false,
        XL_STR => matches!(value.core_ref(), CoreRef::Text(_)),
        XL_BOOL => value.as_logical().is_some(),
        XL_MULTI => value.as_array().is_some(),
        XL_REF => value.as_reference().is_some(),
        _ => false,
    };
    if matches {
        return Ok(value);
    }
    match (value.core_ref(), kind) {
        (CoreRef::Text(t), XL_NUM | XL_INT) => {
            let environment = calc_func::locale::CalcEnvironment {
                compatibility_version: 1,
                date_system: calc_func::locale::DateSystem::System1900,
                profile: calc_func::locale::FormatProfile::for_culture("en-US"),
                now: None,
            };
            let number = calc_func::read_number_text(t.utf16_code_units(), &environment)
                .map_err(|_| err("dc-snz.20: xlCoerce text parsing is not modelled"))?
                .ok_or_else(|| err("xlCoerce text is not a number"))?;
            coerce(CalcValue::number(number), kind)
        }
        (CoreRef::Number(n), XL_INT) if n >= i32::MIN as f64 && n <= i32::MAX as f64 => {
            Ok(CalcValue::number(n.trunc()))
        }
        (CoreRef::Logical(b), XL_NUM | XL_INT) => Ok(CalcValue::number(f64::from(u8::from(b)))),
        (CoreRef::Empty | CoreRef::Missing, XL_NUM | XL_INT) => Ok(CalcValue::number(0.)),
        (CoreRef::Number(n), XL_BOOL) => Ok(CalcValue::logical(n != 0.)),
        (CoreRef::Empty | CoreRef::Missing, XL_BOOL) => Ok(CalcValue::logical(false)),
        (CoreRef::Text(t), XL_BOOL) => {
            let text = t.to_string_lossy();
            if text.eq_ignore_ascii_case("TRUE") {
                Ok(CalcValue::logical(true))
            } else if text.eq_ignore_ascii_case("FALSE") {
                Ok(CalcValue::logical(false))
            } else {
                Err((FAILED, "xlCoerce text is not a logical".into()))
            }
        }
        (CoreRef::Number(_) | CoreRef::Logical(_) | CoreRef::Empty | CoreRef::Missing, XL_STR) => {
            // Use the worksheet's existing number-to-text rules, including its
            // refusal for an unmodelled rounding case, rather than Rust display.
            worksheet_value("CONCATENATE", &[value]).map_err(err)
        }
        (_, XL_MULTI) if value.as_reference().is_none() => {
            Ok(CalcValue::array(calc_value::CalcArray::from_scalar(value)))
        }
        _ => Err((FAILED, "dc-snz.2: xlCoerce conversion not modelled".into())),
    }
}

fn worksheet_value(name: &str, values: &[CalcValue]) -> Result<CalcValue, String> {
    struct Given<'a>(&'a [CalcValue]);
    impl calc_func::PreparedArguments for Given<'_> {
        fn count(&self) -> usize {
            self.0.len()
        }
        fn argument(&mut self, index: usize) -> CalcValue {
            self.0[index].clone()
        }
    }
    let target = calc_func::catalogue()
        .resolve_surface(calc_func::Notation::Call, name)
        .map_err(|e| format!("dc-snz.20: worksheet callback {name}: {e:?}"))?;
    calc_func::invoke(
        target,
        &mut Given(values),
        &mut calc_func::EvalServices::new(&calc_value::NumericMode::Fast),
        &mut calc_func::FunctionCallScratch::new(),
    )
    .map_err(|e| format!("dc-snz.20: worksheet callback {name}: {e:?}"))
}

/// The text of an optional `xlfRegister` argument: empty when the add-in left it out, passed
/// something that is not text, or gave a number (a category may be one; it is kept as written).
fn optional_text(value: Option<&CalcValue>) -> String {
    match value.map(CalcValue::core_ref) {
        Some(CoreRef::Text(t)) => String::from_utf16_lossy(t.utf16_code_units()),
        Some(CoreRef::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

/// The documentation arguments of `xlfRegister`: argument names (5th), category (7th), shortcut
/// (8th), help topic (9th), function help (10th) and one help text per argument (11th on).
fn registration_docs(values: &[CalcValue]) -> crate::native::RegistrationDocs {
    let names = optional_text(values.get(4));
    crate::native::RegistrationDocs {
        argument_names: if names.trim().is_empty() {
            Vec::new()
        } else {
            names.split(',').map(|n| n.trim().to_owned()).collect()
        },
        category: optional_text(values.get(6)),
        shortcut: optional_text(values.get(7)),
        help_topic: optional_text(values.get(8)),
        function_help: optional_text(values.get(9)),
        argument_help: values
            .iter()
            .skip(10)
            .map(|value| optional_text(Some(value)))
            .collect(),
    }
}

fn register(frame: &mut Frame<'_>, values: &[CalcValue]) -> Result<CalcValue, (i32, String)> {
    if values.len() < 4 {
        return Err((INVALID_COUNT, "xlfRegister requires four arguments".into()));
    }
    if frame.signature.is_some() {
        return Err((
            INVALID_FUNCTION,
            "dc-snz.2: registration from a worksheet function".into(),
        ));
    }
    let module_name = text(&values[0])?;
    let module = Module::loaded(&module_name).map_err(err)?;
    let procedure = if let Some(n) = values[1].as_number() {
        format!("#{}", n as u16)
    } else {
        text(&values[1])?
    };
    // Resolve now, on every registration, including re-registration of an export xlOil rewrites.
    let address = if let Some(n) = values[1].as_number() {
        if !(1. ..=65535.).contains(&n) || n.fract() != 0. {
            return Ok(CalcValue::error(WorksheetErrorCode::Value));
        }
        module.ordinal(n as u16)
    } else {
        module.address(&procedure)
    }
    .map_err(err)?;
    let type_text = text(&values[2])?;
    let name = text(&values[3])?;
    let macro_type = values.get(5).and_then(CalcValue::as_number).unwrap_or(1.) as i32;
    // Excel-DNA registers its zero-argument commands with an empty type string.
    // This form is only valid for commands. Excel reads its native Boolean return,
    // including a zero value from the .NET 8 wrappers; it is not unconditional success.
    let signature = XllSignature::parse(if type_text.is_empty() && macro_type == 2 {
        "A"
    } else {
        &type_text
    })
    .map_err(err)?;
    if !matches!(macro_type, 0..=2) || (macro_type == 2 && signature.thread_safe) {
        return Ok(CalcValue::error(WorksheetErrorCode::Value));
    }
    let mut state = frame.state.borrow_mut();
    let key = calc_value::fold_workbook_name(&name);
    let existing = state
        .registrations
        .values()
        .find(|r| {
            r.module.handle == module.handle
                && r.info.address == address
                && r.info.type_text == type_text
                && r.info.name == name
                && r.info.macro_type == macro_type
                && r.origin.handle == frame.module.handle
        })
        .cloned();
    let id = if let Some(existing) = existing {
        existing.uses.set(
            existing
                .uses
                .get()
                .checked_add(1)
                .ok_or_else(|| err("XLL registration use count overflow"))?,
        );
        existing.info.id
    } else {
        let id = state.next_id;
        state.next_id += 1;
        let info = RegistrationInfo {
            id,
            name: name.clone(),
            procedure,
            type_text,
            signature,
            address,
            module: module.path.clone(),
            origin: frame.module.path.clone(),
            use_count: 1,
            macro_type,
            docs: registration_docs(values),
        };
        let cif = crate::invoke::call_interface(&info.signature);
        state.registrations.insert(
            id,
            Rc::new(Registration {
                info,
                module,
                uses: Cell::new(1),
                origin: frame.module.clone(),
                cif,
            }),
        );
        id
    };
    state.names.insert(key, id);
    Ok(CalcValue::number(id as f64))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn text_value(text: &str) -> CalcValue {
        CalcValue::text(ExcelText::from_utf16_code_units(
            text.encode_utf16().collect::<Vec<_>>(),
        ))
    }

    /// Native-only (shares the module's native tests): the documentation arguments of xlfRegister
    /// (argument names, category, shortcut, help topic, function help and one help per argument)
    /// reach the registration and its descriptor; an add-in that leaves them out gets arg1, arg2.
    #[test]
    fn xlf_register_documentation_becomes_the_descriptor() {
        let values = vec![
            text_value("Add.xll"),
            text_value("MyAdd"),
            text_value("BBB"),
            text_value("MyAdd"),
            text_value("first, second"),
            CalcValue::number(1.),
            text_value("My math"),
            CalcValue::missing(),
            text_value("help.chm!1"),
            text_value("Adds two numbers."),
            text_value("The first number."),
            text_value("The second number."),
        ];
        let docs = registration_docs(&values);
        assert_eq!(docs.argument_names, ["first", "second"]);
        assert_eq!(
            (docs.category.as_str(), docs.shortcut.as_str()),
            ("My math", "")
        );
        assert_eq!(docs.help_topic, "help.chm!1");
        assert_eq!(docs.function_help, "Adds two numbers.");
        assert_eq!(
            docs.argument_help,
            ["The first number.", "The second number."]
        );
        let info = |docs| RegistrationInfo {
            id: 1,
            name: "MyAdd".into(),
            procedure: "MyAdd".into(),
            type_text: "BBB".into(),
            signature: XllSignature::parse("BBB$").unwrap(),
            address: 0,
            module: std::path::PathBuf::from("Add.xll"),
            origin: std::path::PathBuf::from("Add.xll"),
            use_count: 1,
            macro_type: 1,
            docs,
        };
        let descriptor = info(docs).descriptor();
        assert_eq!(descriptor.signature(), "MyAdd(first, second)");
        assert_eq!(descriptor.parameters[1].description, "The second number.");
        assert_eq!(
            (descriptor.origin.as_str(), descriptor.category.as_str()),
            ("xll Add.xll", "My math")
        );
        assert!(descriptor.thread_safe && !descriptor.volatile && !descriptor.asynchronous);
        // Left out, or only the names: the other fields are empty and unnamed arguments are numbered.
        let bare = info(registration_docs(&values[..4])).descriptor();
        assert_eq!(bare.signature(), "MyAdd(arg1, arg2)");
        assert_eq!(
            (bare.description.as_str(), bare.category.as_str()),
            ("", "")
        );
        let partial = info(registration_docs(&values[..5])).descriptor();
        assert_eq!(partial.signature(), "MyAdd(first, second)");
        // A category given as a number is kept as written.
        let numbered = registration_docs(&[
            text_value("m"),
            text_value("p"),
            text_value("B"),
            text_value("f"),
            CalcValue::missing(),
            CalcValue::number(1.),
            CalcValue::number(14.),
        ]);
        assert_eq!(numbered.category, "14");
    }

    /// Native-only: constructs native module leases and the same callable projection
    /// used by the P08 XPure DLL; no permission is inferred from a `$` registration.
    #[test]
    fn p08_early_execution_requires_an_explicit_compatible_registration_grant() {
        let host = crate::native::XllHost::new(0);
        let module = Module::loaded("kernel32.dll").unwrap();
        for (signature, allowed) in [
            ("BB$", true),
            ("BB", false),
            ("BB$!", false),
            ("QQ#", false),
        ] {
            let registration = Rc::new(Registration {
                info: RegistrationInfo {
                    id: 1,
                    name: "XPure".into(),
                    procedure: "XPure".into(),
                    type_text: signature.into(),
                    signature: XllSignature::parse(signature).unwrap(),
                    address: 0,
                    module: module.path.clone(),
                    origin: module.path.clone(),
                    use_count: 1,
                    macro_type: 1,
                    docs: Default::default(),
                },
                module: module.clone(),
                origin: module.clone(),
                uses: std::cell::Cell::new(1),
                cif: None,
            });
            host.state
                .borrow_mut()
                .registrations
                .insert(1, registration);
            host.state
                .borrow_mut()
                .names
                .insert(calc_value::fold_workbook_name("XPure"), 1);
            let ordinary = host.functions().unwrap();
            assert!(
                !ordinary
                    .bindings()
                    .permits_early_execution(ordinary.bind(None, "XPure").unwrap())
            );
            let granted = host.functions_with_early_execution(&[1]);
            assert_eq!(granted.is_ok(), allowed);
            if let Ok(functions) = granted {
                assert!(
                    functions
                        .bindings()
                        .permits_early_execution(functions.bind(None, "XPure").unwrap())
                );
            }
        }
        assert!(host.functions_with_early_execution(&[99]).is_err());
        let visible = host.state.borrow().registrations.get(&1).unwrap().clone();
        let mut info = visible.info.clone();
        info.id = 2;
        let shadowed = Rc::new(Registration {
            info,
            module: visible.module.clone(),
            origin: visible.origin.clone(),
            uses: std::cell::Cell::new(1),
            cif: None,
        });
        host.state
            .borrow_mut()
            .registrations
            .insert(2, shadowed.clone());
        // An identical older registration can become visible again after re-registration.
        assert_eq!(host.active_registration_id("xpure"), Some(1));
        host.state
            .borrow_mut()
            .names
            .insert(calc_value::fold_workbook_name("XPure"), 2);
        assert_eq!(host.active_registration_id("XPure"), Some(2));
        shadowed.uses.set(0);
        assert_eq!(host.active_registration_id("XPure"), None);
    }
    /// Native-only: the same dispatcher the executable exports accepts xlw's packed
    /// argument pointers, operands and result buffer, and reports the real module handle.
    #[test]
    fn packed_callback_coercion_and_instance_handle() {
        let host = crate::XllHost::new(0);
        let module = Module::loaded(std::env::current_exe().unwrap().to_str().unwrap()).unwrap();
        let mut frame = Frame::lifecycle(host.state.clone(), module);
        let _guard = unsafe { enter(&mut frame) };
        let mut arena = Arena::new();
        let mut bytes = vec![0u8; 97];
        let input = unsafe { bytes.as_mut_ptr().add(1).cast::<XlOper12>() };
        let kind = unsafe { input.add(1) };
        let result = unsafe { input.add(2) };
        let mut pointers = vec![0u8; 2 * std::mem::size_of::<usize>() + 1];
        let args = unsafe { pointers.as_mut_ptr().add(1).cast::<*mut XlOper12>() };
        unsafe {
            input.write_unaligned(arena.text("16.0").unwrap());
            kind.write_unaligned(XlOper12::integer(XL_INT as i32));
            args.write_unaligned(input);
            args.add(1).write_unaligned(kind);
            assert_eq!(dispatch(XL_COERCE, 2, args, result), SUCCESS);
            let value = result.read_unaligned();
            assert_eq!(value.xltype, XL_INT);
            assert_eq!(value.value.integer, 16);
            assert_eq!(dispatch(16403, 0, std::ptr::null(), result), SUCCESS);
            let value = result.read_unaligned();
            assert_eq!(value.xltype, XL_BIG_DATA);
            assert_eq!(value.value.big_data.byte_count, 0);
            assert_eq!(
                value.value.big_data.data,
                crate::native::GetModuleHandleW(std::ptr::null()).cast()
            );
        }
        assert!(host.diagnostics().is_empty());
    }
    /// Native-only: xlFree changes the caller's operand as Excel 16.0.20527 does (x13 batch):
    /// the string pointer becomes null, the type xltypeMissing, the rest stays; a second free of
    /// the same operand succeeds and releases nothing.
    #[test]
    fn xl_free_nulls_the_pointer_and_marks_the_operand_missing() {
        let host = crate::XllHost::new(0);
        let module = Module::loaded(std::env::current_exe().unwrap().to_str().unwrap()).unwrap();
        let mut frame = Frame::lifecycle(host.state.clone(), module);
        let _guard = unsafe { enter(&mut frame) };
        let mut result = XlOper12::default();
        unsafe {
            assert_eq!(
                dispatch(XL_GET_NAME, 0, std::ptr::null(), &mut result),
                SUCCESS
            );
        }
        assert_eq!(result.xltype, XL_STR);
        assert_eq!(host.live_callback_allocations(), 1);
        let before: [u8; 24] = unsafe { result.value.storage };
        let mut args = [&mut result as *mut XlOper12];
        for _ in 0..2 {
            assert_eq!(
                unsafe { dispatch(XL_FREE_FN, 1, args.as_mut_ptr(), std::ptr::null_mut()) },
                SUCCESS
            );
            let after: [u8; 24] = unsafe { result.value.storage };
            assert_eq!(after[..8], [0; 8]);
            assert_eq!(after[8..], before[8..]);
            assert_eq!(result.xltype, XL_MISSING);
            assert_eq!(host.live_callback_allocations(), 0);
        }
        // A number keeps its bits and becomes Missing too.
        let mut number = XlOper12::number(42.);
        let mut args = [&mut number as *mut XlOper12];
        assert_eq!(
            unsafe { dispatch(XL_FREE_FN, 1, args.as_mut_ptr(), std::ptr::null_mut()) },
            SUCCESS
        );
        assert_eq!(
            (number.xltype, unsafe { number.value.number }),
            (XL_MISSING, 42.)
        );
        assert!(host.diagnostics().is_empty());
    }
    /// Native-only: inside xlAutoFree12 only xlFree is served; any other callback fails with a
    /// typed refusal, because Excel 16.0.20527 terminated there (x13 MyFreeFrame(2)).
    #[test]
    fn free_only_frame_serves_only_xl_free() {
        let host = crate::XllHost::new(0);
        let module = Module::loaded(std::env::current_exe().unwrap().to_str().unwrap()).unwrap();
        let mut frame = Frame::free_only(host.state.clone(), module.clone(), module, None);
        let _guard = unsafe { enter(&mut frame) };
        let mut nothing = XlOper12::default();
        let mut args = [&mut nothing as *mut XlOper12];
        assert_eq!(
            unsafe { dispatch(XL_FREE_FN, 1, args.as_mut_ptr(), std::ptr::null_mut()) },
            SUCCESS
        );
        assert!(host.diagnostics().is_empty());
        let mut result = XlOper12::default();
        assert_eq!(
            unsafe { dispatch(XL_STACK, 0, std::ptr::null(), &mut result) },
            FAILED
        );
        assert_eq!(result.xltype, XL_ERR);
        assert_eq!(host.diagnostics().len(), 1);
        assert!(host.diagnostics()[0].starts_with("dc-6332.13:"));
    }
    /// Native-only: a measured denial answers Excel's status and writes #VALUE! into the result
    /// operand (x13: every failed 2/32/128 row), without refusing; xlGetInst answers 32.
    #[test]
    fn measured_denials_write_value_error_without_refusing() {
        let host = crate::XllHost::new(0);
        let module = Module::loaded(std::env::current_exe().unwrap().to_str().unwrap()).unwrap();
        let mut frame = Frame::lifecycle(host.state.clone(), module);
        frame.kind = FrameKind::Entry;
        frame.signature = Some(XllSignature::parse("QB").unwrap());
        let _guard = unsafe { enter(&mut frame) };
        let mut flag = XlOper12::boolean(false);
        let mut args = [&mut flag as *mut XlOper12];
        for (function, count, status) in [
            (XLF_VOLATILE, 1, INVALID_FUNCTION),
            (XLF_SET_NAME, 1, INVALID_FUNCTION),
            (XL_COMMAND | 141, 1, INVALID_FUNCTION),
            (XL_GET_INST, 0, FAILED),
        ] {
            let mut result = XlOper12::number(7.);
            let code = unsafe { dispatch(function, count, args.as_mut_ptr(), &mut result) };
            assert_eq!(code, status, "{function}");
            assert_eq!(
                (result.xltype, unsafe { result.value.integer }),
                (XL_ERR, 15),
                "{function}"
            );
        }
        assert!(host.diagnostics().is_empty());
    }
    fn handle(key: usize) -> XlOper12 {
        XlOper12 {
            value: XlValue12 {
                big_data: XlBigData {
                    data: key as *mut u8,
                    byte_count: 0,
                },
            },
            xltype: XL_BIG_DATA,
        }
    }

    /// Native-only: verifies native async handle lifetime and rejection without an XLL process.
    #[test]
    fn dropped_preparation_invalidates_its_handle_without_reuse() {
        let pending = pending().unwrap();
        let key = pending.key;
        drop(pending);
        let mut h = handle(key);
        let mut value = XlOper12::number(3.);
        assert_eq!(
            unsafe { async_return(&[&mut h, &mut value], std::ptr::null_mut()) },
            INVALID_ASYNC
        );
        let next = super::pending().unwrap();
        assert!(next.key > key);
    }
    /// Native-only: only one background completion is admitted, copied, then invalidated.
    #[test]
    fn duplicate_and_completed_async_handles_are_rejected() {
        let pending = pending().unwrap();
        let key = pending.key;
        let mut h = handle(key);
        let mut value = XlOper12::number(3.);
        assert_eq!(
            unsafe { async_return(&[&mut h, &mut value], std::ptr::null_mut()) },
            SUCCESS
        );
        assert_eq!(
            unsafe { async_return(&[&mut h, &mut value], std::ptr::null_mut()) },
            INVALID_ASYNC
        );
        assert_eq!(
            wait(key, &pending.completion).unwrap(),
            CalcValue::number(3.)
        );
        assert_eq!(
            unsafe { async_return(&[&mut h, &mut value], std::ptr::null_mut()) },
            INVALID_ASYNC
        );
    }

    /// Native-only: exercises the executable callback route with the operands measured in Excel.
    #[test]
    fn async_return_clears_the_result_operand_and_keeps_the_completion_value() {
        for sentinel in [None, Some(13.25f64)] {
            let mut result_bytes = [0xa5u8; std::mem::size_of::<XlOper12>()];
            let result = result_bytes.as_mut_ptr().cast::<XlOper12>();
            if let Some(value) = sentinel {
                result_bytes[..8].copy_from_slice(&value.to_ne_bytes());
                unsafe { std::ptr::addr_of_mut!((*result).xltype).write_unaligned(XL_NUM) };
            } else {
                result_bytes.fill(0);
            }
            let request = pending().unwrap();
            let mut h = handle(request.key);
            let mut value = XlOper12::number(42.);
            let args = [&mut h as *mut XlOper12, &mut value];
            assert_eq!(
                unsafe { dispatch(XL_ASYNC_RETURN, 2, args.as_ptr(), result) },
                SUCCESS
            );
            assert_eq!(result_bytes, [0; std::mem::size_of::<XlOper12>()]);
            assert_eq!(
                wait(request.key, &request.completion).unwrap(),
                CalcValue::number(42.)
            );
        }
    }

    /// Native-only: revoking an owned handle must retain code while its native task can still run.
    #[test]
    fn cancelled_native_call_keeps_its_module_lease_until_a_late_return() {
        let host = crate::native::XllHost::new(0);
        let module = crate::native::Module::loaded("kernel32.dll").unwrap();
        let origin = crate::native::Module::loaded("ntdll.dll").unwrap();
        host.state.borrow_mut().modules.push(module.clone());
        let registration = Rc::new(Registration {
            info: RegistrationInfo {
                id: 1,
                name: "lease-test".into(),
                procedure: "unused".into(),
                type_text: ">EX".into(),
                signature: XllSignature::parse(">EX").unwrap(),
                address: 0,
                module: module.path.clone(),
                origin: origin.path.clone(),
                use_count: 1,
                macro_type: 1,
                docs: Default::default(),
            },
            module: module.clone(),
            origin,
            uses: std::cell::Cell::new(1),
            cif: None,
        });
        let lease = Rc::downgrade(&registration);
        let mut pending = owned_pending(&host.state, &registration).unwrap();
        pending.entered = true;
        let key = pending.key;
        drop(registration);
        drop(pending);
        assert!(lease.upgrade().is_some());
        assert!(
            host.unload(&module.path)
                .unwrap_err()
                .contains("outstanding native async handles")
        );
        assert!(
            host.close()
                .unwrap_err()
                .contains("outstanding native async handles")
        );
        let mut handle = handle(key);
        let mut result = XlOper12::number(42.);
        assert_eq!(
            unsafe { async_return(&[&mut handle, &mut result], std::ptr::null_mut()) },
            INVALID_ASYNC
        );
        assert!(host.take_async_returns().is_empty());
        assert!(lease.upgrade().is_none());
        host.close().unwrap();
    }
}
