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
        atomic::{AtomicUsize, Ordering},
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
}
static ASYNC: OnceLock<Mutex<HashMap<usize, Arc<Completion>>>> = OnceLock::new();
static NEXT_ASYNC: AtomicUsize = AtomicUsize::new(1);
fn asynchronous() -> &'static Mutex<HashMap<usize, Arc<Completion>>> {
    ASYNC.get_or_init(|| Mutex::new(HashMap::new()))
}
pub(crate) struct Pending {
    pub key: usize,
    pub completion: Arc<Completion>,
}
impl Drop for Pending {
    fn drop(&mut self) {
        cancel(self.key);
    }
}
pub(crate) fn pending() -> Result<Pending, String> {
    let completion = Arc::new(Completion {
        result: Mutex::new(None),
        ready: Condvar::new(),
    });
    // Never reuse an address/token: a late completion must not become a later call's result.
    let key = NEXT_ASYNC
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
        .map_err(|_| "dc-snz.2: async handle space exhausted")?;
    asynchronous()
        .lock()
        .unwrap()
        .insert(key, completion.clone());
    Ok(Pending { key, completion })
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
    asynchronous().lock().unwrap().remove(&key);
    if status.timed_out() && lock.is_none() {
        return Err("dc-snz.2: XLL async completion timed out".into());
    }
    lock.take().ok_or("missing async completion")?
}
pub(crate) fn cancel(key: usize) {
    asynchronous().lock().unwrap().remove(&key);
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
        let args = if count == 0 {
            &[][..]
        } else {
            std::slice::from_raw_parts(args, count as usize)
        };
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
            if matches!(function, 16388 | 16389) {
                return sheet_callback(frame, function, args, result);
            }
            match run(frame, function, args) {
                Ok(Some(value)) => {
                    if result.is_null() {
                        return SUCCESS;
                    }
                    match publish(frame, function, &value, result) {
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
        context.refuse_by("an XLL callback is not modelled", "dc-snz.2");
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
        let oper = unsafe { &**arg };
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
            *result = XlOper12 {
                value: XlValue12 {
                    reference: XlReference12 {
                        table: std::ptr::null_mut(),
                        sheet: id,
                    },
                },
                xltype: XL_REF,
            };
        }
    } else {
        let name = frame.state.borrow().sheet_names.get(&sheet).cloned();
        let Some(name) = name else {
            return FAILED;
        };
        let value = CalcValue::text(ExcelText::from_interop_assignment(&name));
        if unsafe { publish(frame, function, &value, result) }.is_err() {
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
    value: &CalcValue,
    result: *mut XlOper12,
) -> Result<(), String> {
    if function == 16392 {
        unsafe {
            *result = XlOper12 {
                value: XlValue12 {
                    integer: frame.state.borrow().hwnd as i32,
                },
                xltype: XL_INT,
            };
        }
        return Ok(());
    }
    if function == XL_STACK {
        unsafe {
            *result = XlOper12::integer(value.as_number().unwrap_or(0.) as i32);
        }
        return Ok(());
    }
    if function == XLF_CALLER {
        if let Some(cell) = frame.context.as_ref().and_then(|c| c.caller()) {
            unsafe {
                *result = XlOper12 {
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
                };
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
        *result = oper;
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
    let handle = unsafe { &*args[0] };
    if handle.xltype & TYPE_MASK != XL_BIG_DATA {
        return INVALID_ASYNC;
    }
    let key = unsafe { handle.value.big_data.data as usize };
    let completion = asynchronous().lock().unwrap().get(&key).cloned();
    let Some(completion) = completion else {
        return INVALID_ASYNC;
    };
    // Async completions are values, never references to another thread's sheet context.
    let value = unsafe { from_oper(args[1], &SheetMap::default(), None) };
    let mut lock = completion.result.lock().unwrap();
    if lock.is_some() {
        return INVALID_ASYNC;
    }
    *lock = Some(value);
    completion.ready.notify_one();
    // Microsoft documents a Boolean result. The X0/X1 direct-callback traces on this Excel
    // build leave type zero here; that observed difference is recorded separately.
    if !result.is_null() {
        unsafe {
            *result = XlOper12::boolean(true);
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
            free(&frame.state, unsafe { &**pointer });
        }
        return Ok(None);
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
            frame.state.borrow_mut().status_message = values.get(1).map(text).transpose()?;
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
            if values[0].as_number() != Some(2.) {
                return Err((
                    INVALID_FUNCTION,
                    "dc-snz.2: GET.WORKSPACE item not modelled".into(),
                ));
            }
            CalcValue::text(ExcelText::from_interop_assignment("16.0"))
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
            let context = frame.context.as_mut().ok_or_else(|| {
                err(format!(
                    "dc-snz.2: worksheet callback {name} outside a formula"
                ))
            })?;
            context
                .worksheet_function_for(name, &values, "dc-snz.2")
                .map_err(err)?
        }
    };
    Ok(Some(value))
}

fn coerce(value: CalcValue, kind: u32) -> Result<CalcValue, (i32, String)> {
    let matches = match kind {
        XL_NUM => value.as_number().is_some(),
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
        (CoreRef::Logical(b), XL_NUM) => Ok(CalcValue::number(f64::from(u8::from(b)))),
        (CoreRef::Empty, XL_NUM) => Ok(CalcValue::number(0.)),
        (CoreRef::Number(n), XL_BOOL) => Ok(CalcValue::logical(n != 0.)),
        (_, XL_MULTI) if value.as_reference().is_none() => {
            Ok(CalcValue::array(calc_value::CalcArray::from_scalar(value)))
        }
        _ => Err((FAILED, "dc-snz.2: xlCoerce conversion not modelled".into())),
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
    // This form is only valid for commands; the native procedure returns void.
    let signature = XllSignature::parse(if type_text.is_empty() && macro_type == 2 {
        ">"
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
}
