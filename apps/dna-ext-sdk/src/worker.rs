use crate::{Bindings, ExtensionError, InstalledExtension, MAX_MESSAGE, Output, validate_module};
use js_sys::{Array, Function, Object, Reflect, Uint8Array, WebAssembly};
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use web_sys::{
    DedicatedWorkerGlobalScope, ErrorEvent, MessageEvent, Window, Worker, WorkerOptions, WorkerType,
};

const WORKER_URL: &str = "./extension-worker.js";
const RUN_LIMIT_MS: i32 = 5_000;

type Delivery = Rc<RefCell<Option<Box<dyn FnOnce(Result<Output, ExtensionError>)>>>>;

/// Owns a single render. Dropping it cancels superseded work.
pub struct RenderJob {
    worker: Worker,
    window: Window,
    timeout: i32,
    _message: Closure<dyn FnMut(MessageEvent)>,
    _error: Closure<dyn FnMut(ErrorEvent)>,
    _deadline: Closure<dyn FnMut()>,
}

impl Drop for RenderJob {
    fn drop(&mut self) {
        self.window.clear_timeout_with_handle(self.timeout);
        self.worker.set_onmessage(None);
        self.worker.set_onerror(None);
        self.worker.terminate();
    }
}

fn worker_error(error: JsValue) -> ExtensionError {
    ExtensionError::Worker(error.as_string().unwrap_or_else(|| format!("{error:?}")))
}

fn finish(worker: &Worker, delivery: &Delivery, result: Result<Output, ExtensionError>) {
    worker.terminate();
    let callback = delivery.borrow_mut().take();
    if let Some(callback) = callback {
        callback(result);
    }
}

impl InstalledExtension {
    /// Send only the declared bindings and the authenticated module to a dedicated Worker.
    pub fn render(
        &self,
        bindings: &Bindings,
        complete: impl FnOnce(Result<Output, ExtensionError>) + 'static,
    ) -> Result<RenderJob, ExtensionError> {
        let input = self.encode_bindings(bindings)?;
        let window = web_sys::window()
            .ok_or_else(|| ExtensionError::Worker("A page is required.".into()))?;
        let options = WorkerOptions::new();
        options.set_type(WorkerType::Module);
        let worker = Worker::new_with_options(WORKER_URL, &options).map_err(worker_error)?;
        let delivery: Delivery = Rc::new(RefCell::new(Some(Box::new(complete))));
        let message = Closure::<dyn FnMut(MessageEvent)>::new({
            let worker = worker.clone();
            let delivery = delivery.clone();
            let extension = self.clone();
            move |event: MessageEvent| {
                let response = Array::from(&event.data());
                let text = response.get(1).as_string();
                let result = match (response.get(0).as_bool(), text) {
                    (Some(true), Some(text)) => extension.decode_output(text.as_bytes()),
                    (Some(false), Some(reason)) => Err(ExtensionError::Worker(reason)),
                    _ => Err(ExtensionError::Worker(
                        "Invalid extension worker response.".into(),
                    )),
                };
                finish(&worker, &delivery, result);
            }
        });
        let error = Closure::<dyn FnMut(ErrorEvent)>::new({
            let worker = worker.clone();
            let delivery = delivery.clone();
            move |event: ErrorEvent| {
                finish(
                    &worker,
                    &delivery,
                    Err(ExtensionError::Worker(event.message())),
                );
            }
        });
        let deadline = Closure::<dyn FnMut()>::new({
            let worker = worker.clone();
            move || {
                finish(
                    &worker,
                    &delivery,
                    Err(ExtensionError::Worker("Extension render timed out.".into())),
                )
            }
        });
        worker.set_onmessage(Some(message.as_ref().unchecked_ref()));
        worker.set_onerror(Some(error.as_ref().unchecked_ref()));
        let timeout = match window.set_timeout_with_callback_and_timeout_and_arguments_0(
            deadline.as_ref().unchecked_ref(),
            RUN_LIMIT_MS,
        ) {
            Ok(timeout) => timeout,
            Err(error) => {
                worker.terminate();
                return Err(worker_error(error));
            }
        };
        let job = RenderJob {
            worker,
            window,
            timeout,
            _message: message,
            _error: error,
            _deadline: deadline,
        };
        let payload = Array::of2(&Uint8Array::from(self.module()), &JsValue::from_str(&input));
        job.worker.post_message(&payload).map_err(worker_error)?;
        Ok(job)
    }
}

/// Bootstrap that runs in the dedicated Worker, outside the page.
pub fn start_worker() -> Result<(), JsValue> {
    let scope: DedicatedWorkerGlobalScope = js_sys::global().dyn_into()?;
    let message = Closure::<dyn FnMut(MessageEvent)>::new({
        let scope = scope.clone();
        move |event: MessageEvent| {
            let payload = Array::from(&event.data());
            let result = payload
                .get(0)
                .dyn_into::<Uint8Array>()
                .ok()
                .zip(payload.get(1).as_string())
                .ok_or_else(|| "Invalid extension worker input.".to_owned())
                .and_then(|(module, input)| {
                    if module.length() as usize > crate::MAX_MODULE || input.len() > MAX_MESSAGE {
                        return Err("Extension worker input is too large.".into());
                    }
                    invoke(&module.to_vec(), &input).map_err(|error| error.to_string())
                });
            let response = match result {
                Ok(output) => Array::of2(&JsValue::TRUE, &JsValue::from_str(&output)),
                Err(error) => Array::of2(&JsValue::FALSE, &JsValue::from_str(&error)),
            };
            let _ = scope.post_message(&response);
        }
    });
    scope.set_onmessage(Some(message.as_ref().unchecked_ref()));
    message.forget();
    Ok(())
}

fn invoke(module: &[u8], input: &str) -> Result<String, ExtensionError> {
    validate_module(module)?;
    let module = WebAssembly::Module::new(&Uint8Array::from(module)).map_err(worker_error)?;
    let instance = WebAssembly::Instance::new(&module, &Object::new()).map_err(worker_error)?;
    let exports = instance.exports();
    let export =
        |name: &str| Reflect::get(&exports, &JsValue::from_str(name)).map_err(worker_error);
    let function = |name: &str| export(name)?.dyn_into::<Function>().map_err(worker_error);
    let memory = export("memory")?
        .dyn_into::<WebAssembly::Memory>()
        .map_err(worker_error)?;
    let len = input.len() as u32;
    let allocate = function("dna_alloc")?;
    let address = number(
        allocate
            .call1(&JsValue::UNDEFINED, &len.into())
            .map_err(worker_error)?,
    )?;
    memory_range(&memory, address, len)?.copy_from(input.as_bytes());
    let render = function("dna_render")?;
    let output = number(
        render
            .call2(&JsValue::UNDEFINED, &address.into(), &len.into())
            .map_err(worker_error)?,
    )?;
    let output_len = number(
        function("dna_output_len")?
            .call0(&JsValue::UNDEFINED)
            .map_err(worker_error)?,
    )?;
    let bytes = memory_range(&memory, output, output_len)?.to_vec();
    String::from_utf8(bytes)
        .map_err(|_| ExtensionError::Worker("Extension output must be UTF-8.".into()))
}

fn number(value: JsValue) -> Result<u32, ExtensionError> {
    value
        .as_f64()
        .filter(|n| n.fract() == 0.0 && (0.0..=u32::MAX as f64).contains(n))
        .map(|n| n as u32)
        .ok_or_else(|| ExtensionError::Worker("Invalid extension memory address.".into()))
}

fn memory_range(
    memory: &WebAssembly::Memory,
    start: u32,
    len: u32,
) -> Result<Uint8Array, ExtensionError> {
    let buffer = Uint8Array::new(&memory.buffer());
    let end = start
        .checked_add(len)
        .filter(|end| *end <= buffer.length() && len as usize <= MAX_MESSAGE)
        .ok_or_else(|| ExtensionError::Worker("Extension memory range is out of bounds.".into()))?;
    Ok(buffer.subarray(start, end))
}
