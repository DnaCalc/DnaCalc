use crate::native::{Module, Registration, RegistrationInfo, Shared, current_sheet};
use calc_func::UserFunctionContext;
use calc_value::{CalcValue, CoreRef, ExcelText, ReferenceLike, WorksheetErrorCode};
use calc_xll_abi::*;
use std::{
    cell::Cell,
    collections::HashMap,
    rc::Rc,
    sync::{
        Arc, Condvar, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

pub(crate) struct Frame<'a> {
    pub state: Shared,
    pub module: Rc<Module>,
    pub origin: Rc<Module>,
    pub context: Option<&'a mut dyn UserFunctionContext>,
    pub signature: Option<XllSignature>,
}
impl Frame<'_> {
    pub fn lifecycle(state: Shared, module: Rc<Module>) -> Self {
        Self {
            state,
            origin: module.clone(),
            module,
            context: None,
            signature: None,
        }
    }
}
thread_local! { static FRAME:Cell<*mut Frame<'static>>=const{Cell::new(std::ptr::null_mut())}; }
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
            if function == 16403 {
                if count != 0 {
                    return INVALID_COUNT;
                }
                if !result.is_null() {
                    result.write_unaligned(XlOper12 {
                        value: XlValue12 {
                            big_data: XlBigData {
                                data: crate::native::GetModuleHandleW(std::ptr::null()).cast(),
                                byte_count: 0,
                            },
                        },
                        xltype: XL_BIG_DATA,
                    });
                }
                return SUCCESS;
            }
            if matches!(function, 16388 | 16389) {
                return sheet_callback(frame, function, args, result);
            }
            match run(frame, function, args) {
                Ok(Some(value)) => {
                    if result.is_null() {
                        return SUCCESS;
                    }
                    match publish(frame, function, args, &value, result) {
                        Ok(()) => SUCCESS,
                        Err(why) => {
                            refuse(frame, &why);
                            FAILED
                        }
                    }
                }
                Ok(None) => SUCCESS,
                Err((code, why)) => {
                    refuse(frame, &why);
                    code
                }
            }
        })
    }))
    .unwrap_or(FAILED)
}

fn refuse(frame: &mut Frame<'_>, why: &str) {
    if let Some(context) = &mut frame.context {
        context.refuse_by(
            "an XLL callback is not modelled",
            if why.starts_with("dc-snz.18:") {
                "dc-snz.18"
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
    result: *mut XlOper12,
) -> i32 {
    if args.len() > 1 || (function == 16389 && args.len() != 1) {
        return INVALID_COUNT;
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
            XL_STR if function == 16388 => {
                let value = unsafe { from_oper(*arg, &frame.state.borrow().sheets, selected) };
                let Ok(name) = value.and_then(|v| text(&v).map_err(|(_, why)| why)) else {
                    return INVALID_OPER;
                };
                frame
                    .state
                    .borrow()
                    .sheet_names
                    .iter()
                    .find_map(|(s, n)| n.eq_ignore_ascii_case(&name).then_some(*s))
            }
            _ => return INVALID_OPER,
        }
    } else {
        selected
    };
    let Some(sheet) = sheet else {
        return FAILED;
    };
    if result.is_null() {
        return SUCCESS;
    }
    if function == 16388 {
        let id = frame.state.borrow_mut().sheets.encode(sheet);
        unsafe {
            result.write_unaligned(XlOper12 {
                value: XlValue12 {
                    reference: XlReference12 {
                        table: std::ptr::null_mut(),
                        sheet: id,
                    },
                },
                xltype: XL_REF,
            });
        }
    } else {
        let name = frame.state.borrow().sheet_names.get(&sheet).cloned();
        let Some(name) = name else {
            return FAILED;
        };
        let value = CalcValue::text(ExcelText::from_interop_assignment(&name));
        if unsafe { publish(frame, function, args, &value, result) }.is_err() {
            return FAILED;
        }
    }
    SUCCESS
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
unsafe fn publish(
    frame: &mut Frame<'_>,
    function: i32,
    args: &[*mut XlOper12],
    value: &CalcValue,
    result: *mut XlOper12,
) -> Result<(), String> {
    if function == 16392 {
        unsafe {
            result.write_unaligned(XlOper12 {
                value: XlValue12 {
                    integer: frame.state.borrow().hwnd as i32,
                },
                xltype: XL_INT,
            });
        }
        return Ok(());
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
    if function == XL_STACK || function == 16401 || integer_coercion {
        unsafe {
            result.write_unaligned(XlOper12::integer(value.as_number().unwrap_or(0.) as i32));
        }
        return Ok(());
    }
    if function == XLF_CALLER {
        if let Some(cell) = frame.context.as_ref().and_then(|c| c.caller()) {
            unsafe {
                result.write_unaligned(XlOper12 {
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
            return Ok(());
        }
    }
    let mut arena = Arena::new();
    let oper = arena.oper(value, &mut frame.state.borrow_mut().sheets)?;
    if let Some(key) = allocation_key(&oper) {
        frame.state.borrow_mut().allocations.insert(
            key,
            crate::native::CallbackAllocation {
                owner: frame.origin.handle as usize,
                _arena: arena,
            },
        );
    }
    unsafe {
        result.write_unaligned(oper);
    }
    Ok(())
}
pub(crate) fn free(state: &Shared, oper: &XlOper12) {
    if let Some(key) = allocation_key(oper) {
        state.borrow_mut().allocations.remove(&key);
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
    // Microsoft documents a Boolean result. The X0/X1 direct-callback traces on this Excel
    // build leave type zero here; that observed difference is recorded separately.
    if !result.is_null() {
        unsafe {
            result.write_unaligned(XlOper12::boolean(true));
        }
    }
    SUCCESS
}

fn run(
    frame: &mut Frame<'_>,
    function: i32,
    args: &[*mut XlOper12],
) -> Result<Option<CalcValue>, (i32, String)> {
    if function == XL_FREE_FN {
        for pointer in args {
            free(&frame.state, &unsafe { (*pointer).read_unaligned() });
        }
        return Ok(None);
    }
    if frame.signature.is_some() && function & 0x8000 != 0 {
        return Err((
            INVALID_FUNCTION,
            "dc-snz.18: worksheet functions cannot call commands".into(),
        ));
    }
    if frame.signature.as_ref().is_some_and(|s| s.thread_safe) && function == 16401 {
        return Err((
            NOT_THREAD_SAFE,
            "dc-snz.18: event registration from calculation workers is not modelled".into(),
        ));
    }
    if frame.signature.as_ref().is_some_and(|s| s.thread_safe)
        && matches!(function, XL_UDF | XLF_EVALUATE)
    {
        return Err((
            NOT_THREAD_SAFE,
            "dc-snz.18: nested registered calls from calculation workers are not modelled".into(),
        ));
    }
    if frame.signature.as_ref().is_some_and(|s| s.thread_safe)
        && matches!(function, XLF_REGISTER | XLF_UNREGISTER | XLF_SET_NAME | 186)
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
        32890 => {
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
            CalcValue::logical(true)
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
            CalcValue::number(current.saturating_sub(low).min(i32::MAX as usize) as f64)
        }
        XLF_CALLER => {
            require(0)?;
            match frame.context.as_ref().and_then(|c| c.caller()) {
                Some(c) => CalcValue::reference(ReferenceLike::cell(c)),
                None => CalcValue::error(WorksheetErrorCode::Ref),
            }
        }
        XL_COERCE => {
            if !(1..=2).contains(&args.len()) {
                return Err((
                    INVALID_COUNT,
                    "xlCoerce expects one or two arguments".into(),
                ));
            }
            let mut value = values[0].clone();
            if let Some(reference) = value.as_reference() {
                if args.len() == 2 && values[1].as_number() == Some(f64::from(XL_REF)) {
                    return Ok(Some(value));
                }
                value = frame
                    .context
                    .as_mut()
                    .ok_or_else(|| err("dc-snz.2: xlCoerce reference outside formula"))?
                    .values_of(reference)
                    .map_err(err)?;
            }
            if let Some(kind) = values.get(1).and_then(CalcValue::as_number) {
                value = coerce(value, kind as u32)?;
            }
            value
        }
        186 => {
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
        16392 => {
            require(0)?;
            CalcValue::number(frame.state.borrow().hwnd as f64)
        }
        16402 => {
            require(0)?;
            CalcValue::logical(false)
        }
        237 => {
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
        16401 => {
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
        XLF_EVALUATE => {
            require(1)?;
            let name = calc_value::fold_workbook_name(&text(&values[0])?);
            match frame.state.borrow().names.get(&name) {
                Some(id) => CalcValue::number(*id as f64),
                None => {
                    return Err((
                        INVALID_FUNCTION,
                        "dc-snz.2: EVALUATE outside registered names is not modelled".into(),
                    ));
                }
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
                state: frame.state.clone(),
                registration,
            };
            match &mut frame.context {
                Some(context) => adapter.invoke(&values[1..], Some(&mut **context)),
                None => adapter.invoke(&values[1..], None),
            }
            .map_err(err)?
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

fn coerce(value: CalcValue, kind: u32) -> Result<CalcValue, (i32, String)> {
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
        (CoreRef::Logical(b), XL_NUM) => Ok(CalcValue::number(f64::from(u8::from(b)))),
        (CoreRef::Empty, XL_NUM) => Ok(CalcValue::number(0.)),
        (CoreRef::Number(n), XL_BOOL) => Ok(CalcValue::logical(n != 0.)),
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
