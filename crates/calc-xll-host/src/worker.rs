//! Only owned values and request messages cross this boundary. The evaluator and its
//! borrowed reader remain on the main lane; native calls and their frees use one worker.
use crate::native::{Module, Registration, State, XllFunction};
use calc_func::UserFunctionContext;
use calc_value::{CalcValue, CellKey, ReferenceLike};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::Ordering,
        mpsc::{self, Receiver, Sender, TryRecvError},
    },
    thread::JoinHandle,
};

type Job = Box<dyn FnOnce() + Send>;
pub(crate) struct Pool {
    sender: Option<Sender<Job>>,
    threads: Vec<JoinHandle<()>>,
}
impl Pool {
    pub fn capacity(&self) -> usize {
        self.threads.len()
    }
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel::<Job>();
        let receiver = Arc::new(Mutex::new(receiver));
        let count = std::thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(1)
            .clamp(1, 4);
        let threads = (0..count)
            .map(|index| {
                let receiver = receiver.clone();
                std::thread::Builder::new()
                    .name(format!("xll-calculation-{index}"))
                    .spawn(move || {
                        loop {
                            let job = receiver.lock().unwrap().recv();
                            match job {
                                Ok(job) => job(),
                                Err(_) => break,
                            }
                        }
                    })
                    .expect("create native calculation worker")
            })
            .collect();
        Self {
            sender: Some(sender),
            threads,
        }
    }
}
impl Drop for Pool {
    fn drop(&mut self) {
        self.sender.take();
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}
type Reply = Sender<Result<CalcValue, String>>;
type ReadReply = Sender<Result<calc_func::ReferenceRead, String>>;
enum Request {
    /// A typed read: the owner answers `Pending` for a producer not yet calculated (status 64).
    /// `true` asks for the typed read; `false` for the plain value read, answered as `Value`.
    Read(ReferenceLike, bool, ReadReply),
    ArgumentRead(ReferenceLike, bool, ReadReply),
    Reevaluate(calc_func::UserReevaluationReason, Sender<Result<(), String>>),
    Worksheet(String, Vec<CalcValue>, &'static str, Reply),
    Refuse(&'static str, &'static str),
    Volatile(bool),
    Done(Result<CalcValue, String>, Vec<String>),
}
struct Reader {
    sender: Sender<Request>,
    caller: Option<CellKey>,
}
impl Reader {
    fn read(
        &mut self,
        request: impl FnOnce(ReadReply) -> Request,
    ) -> Result<calc_func::ReferenceRead, String> {
        let (reply, result) = mpsc::channel();
        self.sender
            .send(request(reply))
            .map_err(|_| "XLL reader disconnected")?;
        result.recv().map_err(|_| "XLL reader disconnected")?
    }
}
/// A plain read's answer: the owner answers it with a value or a fault, never `Pending`.
fn value_read(read: calc_func::ReferenceRead) -> Result<CalcValue, String> {
    match read {
        calc_func::ReferenceRead::Value(value) => Ok(value),
        calc_func::ReferenceRead::Pending => Err("XLL plain read answered as pending".into()),
    }
}
impl UserFunctionContext for Reader {
    fn caller(&self) -> Option<CellKey> {
        self.caller
    }
    fn set_volatile(&mut self, value: bool) {
        let _ = self.sender.send(Request::Volatile(value));
    }
    fn refuse(&mut self, reason: &'static str) -> String {
        self.refuse_by(reason, "dc-snz.2")
    }
    fn refuse_by(&mut self, reason: &'static str, owner: &'static str) -> String {
        let _ = self.sender.send(Request::Refuse(reason, owner));
        format!("{owner}: {reason}")
    }
    fn values_of(&mut self, reference: &ReferenceLike) -> Result<CalcValue, String> {
        self.read(|reply| Request::Read(reference.clone(), false, reply))
            .and_then(value_read)
    }
    fn argument_values_of(&mut self, reference: &ReferenceLike) -> Result<CalcValue, String> {
        self.read(|reply| Request::ArgumentRead(reference.clone(), false, reply))
            .and_then(value_read)
    }
    fn read_reference(
        &mut self,
        reference: &ReferenceLike,
    ) -> Result<calc_func::ReferenceRead, String> {
        self.read(|reply| Request::Read(reference.clone(), true, reply))
    }
    fn read_argument(
        &mut self,
        reference: &ReferenceLike,
    ) -> Result<calc_func::ReferenceRead, String> {
        self.read(|reply| Request::ArgumentRead(reference.clone(), true, reply))
    }
    fn request_reevaluation(&mut self, reason: calc_func::UserReevaluationReason) -> Result<(), String> {
        let (reply, result) = mpsc::channel();
        self.sender
            .send(Request::Reevaluate(reason, reply))
            .map_err(|_| "XLL reader disconnected")?;
        result.recv().map_err(|_| "XLL reader disconnected")?
    }
    fn worksheet_function(
        &mut self,
        name: &str,
        arguments: &[CalcValue],
    ) -> Result<CalcValue, String> {
        self.worksheet_function_for(name, arguments, "dc-snz.2")
    }
    fn worksheet_function_for(
        &mut self,
        name: &str,
        arguments: &[CalcValue],
        owner: &'static str,
    ) -> Result<CalcValue, String> {
        let (reply, result) = mpsc::channel();
        self.sender
            .send(Request::Worksheet(
                name.to_owned(),
                arguments.to_vec(),
                owner,
                reply,
            ))
            .map_err(|_| "XLL reader disconnected")?;
        result.recv().map_err(|_| "XLL reader disconnected")?
    }
}

pub(crate) fn submit(
    function: &XllFunction,
    pool: &Pool,
    arguments: &[CalcValue],
    caller: Option<CellKey>,
    has_context: bool,
) -> Result<PoolCall, String> {
    let (sender, receiver) = mpsc::channel();
    let info = function.registration.info.clone();
    let uses = function.registration.uses.get();
    let (sheets, sheet_names, active_sheet, hwnd, closed, main_thread, allocations) = {
        let state = function.state.borrow();
        (
            state.sheets.clone(),
            state.sheet_names.clone(),
            state.active_sheet,
            state.hwnd,
            state.closed,
            state.thread,
            state.allocations.clone(),
        )
    };
    if main_thread != unsafe { crate::native::GetCurrentThreadId() } {
        return Err("dc-snz.18: invocation submitted outside owning main lane".into());
    }
    let arguments = arguments.to_vec();
    // The owner keeps both native modules resident from enqueue through the
    // physical return, including when the evaluator drops its activation.
    let mut pending = crate::callbacks::owned_pending(&function.state, &function.registration)?;
    pending.completion.forwarded.store(true, Ordering::Release);
    let completion = pending.completion.clone();
    pool.sender
        .as_ref()
        .unwrap()
        .send(Box::new(move || {
            let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                if !completion.active.load(Ordering::Acquire) {
                    return Err("dc-snz.23: canceled before native worker entry".into());
                }
                // Acquire separate Windows module leases on this thread. No Rc, RefCell,
                // arena pointer, libffi interface or evaluator borrow was sent from the STA.
                let module = Module::loaded(info.module.to_string_lossy().as_ref())?;
                let origin = Module::loaded(info.origin.to_string_lossy().as_ref())?;
                let state = Rc::new(RefCell::new(State {
                    modules: Vec::new(),
                    registrations: BTreeMap::new(),
                    names: BTreeMap::new(),
                    next_id: 1,
                    sheets,
                    sheet_names,
                    active_sheet,
                    events: BTreeMap::new(),
                    // The host's one registry: a callback result kept past this return stays
                    // valid until the add-in frees it, as in Excel (x13 MyRetainPool).
                    allocations,
                    diagnostics: Vec::new(),
                    hwnd,
                    thread: unsafe { crate::native::GetCurrentThreadId() },
                    closed,
                    status_message: None,
                    workers: None,
                    pending_async: BTreeMap::new(),
                    async_continuations: false,
                    rtd: None,
                }));
                let cif = crate::invoke::call_interface(&info.signature);
                let registration = Rc::new(Registration {
                    info,
                    module,
                    origin,
                    uses: Cell::new(uses),
                    cif,
                });
                let adapter = XllFunction {
                    early: false,
                    parallel: 1,
                    state: state.clone(),
                    registration,
                    bound: false,
                };
                let mut reader = Reader {
                    sender: sender.clone(),
                    caller,
                };
                let result = adapter.invoke_inline(
                    &arguments,
                    if has_context { Some(&mut reader) } else { None },
                );
                let mut state = state.borrow_mut();
                Ok((result, std::mem::take(&mut state.diagnostics)))
            }));
            let (result, diagnostics) = match run {
                Ok(Ok(value)) => value,
                Ok(Err(why)) => (Err(why), Vec::new()),
                Err(_) => (Err("dc-snz.18: native worker panicked".into()), Vec::new()),
            };
            completion.physical_returned.store(true, Ordering::Release);
            let _ = sender.send(Request::Done(result, diagnostics));
        }))
        .map_err(|_| "native calculation pool stopped")?;
    pending.entered = true;
    Ok(PoolCall {
        state: function.state.clone(),
        receiver,
        _pending: Some(pending),
        finished: false,
    })
}

pub(crate) struct PoolCall {
    // Cancel, disconnect replies, then release the owner. Drop order matters.
    _pending: Option<crate::callbacks::Pending>,
    receiver: Receiver<Request>,
    state: crate::native::Shared,
    finished: bool,
}
impl std::fmt::Debug for PoolCall {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PoolCall")
            .field("finished", &self.finished)
            .finish()
    }
}
impl PoolCall {
    fn service(
        &mut self,
        request: Request,
        context: &mut Option<&mut dyn UserFunctionContext>,
    ) -> Option<Result<CalcValue, String>> {
        match request {
            Request::ArgumentRead(reference, typed, reply) => {
                let result = context
                    .as_mut()
                    .ok_or("XLL has no formula reader".to_owned())
                    .and_then(|c| {
                        if typed {
                            c.read_argument(&reference)
                        } else {
                            c.argument_values_of(&reference)
                                .map(calc_func::ReferenceRead::Value)
                        }
                    });
                let _ = reply.send(result);
            }
            Request::Read(reference, typed, reply) => {
                let result = context
                    .as_mut()
                    .ok_or("XLL has no formula reader".to_owned())
                    .and_then(|c| {
                        if typed {
                            c.read_reference(&reference)
                        } else {
                            c.values_of(&reference).map(calc_func::ReferenceRead::Value)
                        }
                    });
                let _ = reply.send(result);
            }
            Request::Reevaluate(reason, reply) => {
                let result = context
                    .as_mut()
                    .ok_or("XLL has no formula reader".to_owned())
                    .and_then(|c| c.request_reevaluation(reason));
                let _ = reply.send(result);
            }
            Request::Worksheet(name, arguments, owner, reply) => {
                let result = context
                    .as_mut()
                    .ok_or("XLL has no formula reader".to_owned())
                    .and_then(|c| c.worksheet_function_for(&name, &arguments, owner));
                let _ = reply.send(result);
            }
            Request::Refuse(reason, owner) => {
                if let Some(c) = context {
                    c.refuse_by(reason, owner);
                }
            }
            Request::Volatile(value) => {
                if let Some(c) = context {
                    c.set_volatile(value);
                }
            }
            Request::Done(result, diagnostics) => {
                self.state.borrow_mut().diagnostics.extend(diagnostics);
                if let Err(why) = &result
                    && let Some(context) = context
                {
                    crate::native::refuse_native_error(*context, why);
                }
                self.finished = true;
                return Some(result);
            }
        }
        None
    }
}

impl calc_func::UserFunctionInvocation for PoolCall {
    fn resume(
        &mut self,
        context: &mut dyn UserFunctionContext,
        quantum: u64,
    ) -> Result<calc_func::UserFunctionStep, String> {
        if self.finished {
            return Err("native pool activation already completed".into());
        }
        let mut context = Some(context);
        for _ in 0..quantum.clamp(1, 64) {
            match self.receiver.try_recv() {
                Ok(request) => {
                    if let Some(result) = self.service(request, &mut context) {
                        return result.map(calc_func::UserFunctionStep::Complete);
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    return Err(context
                        .as_mut()
                        .unwrap()
                        .refuse_by("native worker disconnected", "dc-snz.23"));
                }
            }
        }
        Ok(calc_func::UserFunctionStep::Suspended)
    }
}

pub(crate) fn invoke(
    function: &XllFunction,
    pool: &Pool,
    arguments: &[CalcValue],
    mut context: Option<&mut dyn UserFunctionContext>,
) -> Result<CalcValue, String> {
    let mut call = submit(
        function,
        pool,
        arguments,
        context.as_ref().and_then(|c| c.caller()),
        context.is_some(),
    )?;
    loop {
        let request = call
            .receiver
            .recv()
            .map_err(|_| "native worker disconnected")?;
        if let Some(result) = call.service(request, &mut context) {
            return result;
        }
    }
}
