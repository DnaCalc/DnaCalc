//! Only owned values and request messages cross this boundary. The evaluator and its
//! borrowed reader remain on the main lane; native calls and their frees use one worker.
use crate::native::{Module, Registration, State, XllFunction};
use calc_func::UserFunctionContext;
use calc_value::{CalcValue, CellKey, ReferenceLike};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, HashMap},
    rc::Rc,
    sync::{
        Arc, Mutex,
        mpsc::{self, Sender},
    },
    thread::JoinHandle,
};

type Job = Box<dyn FnOnce() + Send>;
pub(crate) struct Pool {
    sender: Option<Sender<Job>>,
    threads: Vec<JoinHandle<()>>,
}
impl Pool {
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
enum Request {
    Read(ReferenceLike, Reply),
    ArgumentRead(ReferenceLike, Reply),
    Worksheet(String, Vec<CalcValue>, &'static str, Reply),
    Refuse(&'static str, &'static str),
    Volatile(bool),
    Done(Result<CalcValue, String>, Vec<String>),
}
struct Reader {
    sender: Sender<Request>,
    caller: Option<CellKey>,
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
        let (reply, result) = mpsc::channel();
        self.sender
            .send(Request::Read(reference.clone(), reply))
            .map_err(|_| "XLL reader disconnected")?;
        result.recv().map_err(|_| "XLL reader disconnected")?
    }
    fn argument_values_of(&mut self, reference: &ReferenceLike) -> Result<CalcValue, String> {
        let (reply, result) = mpsc::channel();
        self.sender
            .send(Request::ArgumentRead(reference.clone(), reply))
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

pub(crate) fn invoke(
    function: &XllFunction,
    pool: &Pool,
    arguments: &[CalcValue],
    mut context: Option<&mut dyn UserFunctionContext>,
) -> Result<CalcValue, String> {
    let (sender, receiver) = mpsc::channel();
    let info = function.registration.info.clone();
    let uses = function.registration.uses.get();
    let (sheets, sheet_names, active_sheet, hwnd, closed, main_thread) = {
        let state = function.state.borrow();
        (
            state.sheets.clone(),
            state.sheet_names.clone(),
            state.active_sheet,
            state.hwnd,
            state.closed,
            state.thread,
        )
    };
    if main_thread != unsafe { crate::native::GetCurrentThreadId() } {
        return Err("dc-snz.18: invocation submitted outside owning main lane".into());
    }
    let caller = context.as_ref().and_then(|c| c.caller());
    let has_context = context.is_some();
    let arguments = arguments.to_vec();
    pool.sender
        .as_ref()
        .unwrap()
        .send(Box::new(move || {
            let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
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
                    allocations: HashMap::new(),
                    diagnostics: Vec::new(),
                    hwnd,
                    thread: unsafe { crate::native::GetCurrentThreadId() },
                    closed,
                    status_message: None,
                    workers: None,
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
                    state: state.clone(),
                    registration,
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
                if !state.allocations.is_empty() {
                    return Err("dc-snz.18: worker retained callback storage".into());
                }
                Ok((result, std::mem::take(&mut state.diagnostics)))
            }));
            let (result, diagnostics) = match run {
                Ok(Ok(value)) => value,
                Ok(Err(why)) => (Err(why), Vec::new()),
                Err(_) => (Err("dc-snz.18: native worker panicked".into()), Vec::new()),
            };
            let _ = sender.send(Request::Done(result, diagnostics));
        }))
        .map_err(|_| "native calculation pool stopped")?;
    loop {
        let request = receiver.recv().map_err(|_| "native worker disconnected")?;
        match request {
            Request::ArgumentRead(reference, reply) => {
                let result = context
                    .as_mut()
                    .ok_or("XLL has no formula reader".to_owned())
                    .and_then(|c| c.argument_values_of(&reference));
                let _ = reply.send(result);
            }
            Request::Read(reference, reply) => {
                let result = context
                    .as_mut()
                    .ok_or("XLL has no formula reader".to_owned())
                    .and_then(|c| c.values_of(&reference));
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
                if let Some(c) = &mut context {
                    c.refuse_by(reason, owner);
                }
            }
            Request::Volatile(value) => {
                if let Some(c) = &mut context {
                    c.set_volatile(value);
                }
            }
            Request::Done(result, diagnostics) => {
                function.state.borrow_mut().diagnostics.extend(diagnostics);
                return result;
            }
        }
    }
}
