#[cfg(target_arch = "wasm32")]
mod browser {
    use calc_skin::{Envelope, Message, Refusal, decode};
    use dna_shell::workbook::{Complete, Exchange, WorkbookFrame, workbook};
    use dna_skin_runtime::workbook_tiles::TileFrames;
    use leptos::prelude::*;
    use std::cell::{Cell, RefCell};
    use std::collections::BTreeMap;
    use std::rc::Rc;
    use wasm_bindgen::{JsCast, JsValue, closure::Closure, prelude::wasm_bindgen};
    use web_sys::{ErrorEvent, MessageEvent, Worker, WorkerOptions, WorkerType};

    #[wasm_bindgen(start)]
    pub fn start() -> Result<(), JsValue> {
        let window = web_sys::window().ok_or("No browser window")?;
        let document = window.document().ok_or("No document")?;
        let root = document
            .get_element_by_id("dna-classic")
            .ok_or("No workbook mount")?;
        let script = js_sys::Reflect::get(&window, &JsValue::from_str("__dnaClassicWorker"))?
            .as_string()
            .ok_or("No workbook worker URL")?;
        let options = WorkerOptions::new();
        options.set_type(WorkerType::Module);
        let worker = Worker::new_with_options(&script, &options)?;
        let ready = document.document_element().ok_or("No document root")?;
        let root: web_sys::HtmlElement = root.dyn_into()?;
        root.set_text_content(None);
        leptos::mount::mount_to(root, move || {
            let pending = Rc::new(RefCell::new(BTreeMap::<u64, Complete>::new()));
            let sequence = Cell::new(1);
            let exchange: Exchange = Rc::new({
                let pending = pending.clone();
                let worker = worker.clone();
                move |request, bytes, complete| {
                    let id = sequence.get();
                    sequence.set(id + 1);
                    let envelope = Envelope::new(id, Message::WorkbookRequest(request));
                    let result = serde_json::to_string(&envelope)
                        .map_err(|error| Refusal::Transport(error.to_string()))
                        .and_then(|text| {
                            let send = match bytes {
                                Some(bytes) => {
                                    let bytes = js_sys::Uint8Array::from(bytes.as_slice()).buffer();
                                    worker.post_message_with_transfer(&js_sys::Array::of2(&text.into(), &bytes), &js_sys::Array::of1(&bytes))
                                }
                                None => worker.post_message(&text.into()),
                            };
                            send.map_err(|error| Refusal::Transport(format!("Could not reach the workbook worker: {error:?}")))
                        });
                    match result {
                        Ok(()) => { pending.borrow_mut().insert(id, complete); }
                        Err(reason) => complete(Err(reason)),
                    }
                }
            });
            let state = WorkbookFrame::new(exchange);
            let mut tiles = TileFrames::default();
            let receive = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
                let data = event.data();
                let (control, bytes) = if js_sys::Array::is_array(&data) {
                    let packet = js_sys::Array::from(&data);
                    (packet.get(0).as_string(), Some(js_sys::Uint8Array::new(&packet.get(1)).to_vec()))
                } else {
                    (data.as_string(), None)
                };
                let Some(text) = control else { return };
                let packet = match decode(text.as_bytes(), bytes) {
                    Ok(packet) => packet,
                    Err(reason) => { state.error(reason); return; }
                };
                match packet.envelope.message {
                    Message::Frame(frame) => match tiles.finish(frame) {
                        Ok((frame, transaction)) => {
                            state.receive(frame);
                            if let Some(transaction) = transaction {
                                state.acknowledge_tiles(transaction);
                            }
                        }
                        Err(reason) => {
                            state.error(reason);
                            state.reset_tiles();
                        }
                    },
                    Message::Tile(stamp) => {
                        let result = packet.bytes.ok_or_else(|| Refusal::Transport("Tile data is missing".into()))
                            .and_then(|bytes| tiles.fragment(stamp, &bytes));
                        if let Err(reason) = result { state.error(reason); }
                    }
                    Message::WorkbookReply(reply) => {
                        let complete = pending.borrow_mut().remove(&packet.envelope.request);
                        if let Some(complete) = complete { complete(Ok(reply)); }
                    }
                    Message::Failure(reason) => {
                        let complete = pending.borrow_mut().remove(&packet.envelope.request);
                        if let Some(complete) = complete {
                            complete(Err(reason));
                        } else {
                            state.error(reason);
                        }
                    }
                    Message::WorkbookRequest(_) => state.error(Refusal::Transport("Unexpected workbook request".into())),
                }
            });
            worker.set_onmessage(Some(receive.as_ref().unchecked_ref()));
            receive.forget();
            let fault = Closure::<dyn FnMut(ErrorEvent)>::new(move |event: ErrorEvent| {
                state.error(Refusal::Transport(event.message()));
            });
            worker.set_onerror(Some(fault.as_ref().unchecked_ref()));
            fault.forget();
            let probe = Closure::<dyn FnMut() -> JsValue>::new(move || {
                js_sys::JSON::parse(&state.probe().to_string()).unwrap_or(JsValue::NULL)
            });
            let _ = js_sys::Reflect::set(&window, &JsValue::from_str("__dnaProbe"), probe.as_ref());
            probe.forget();
            let mut registry = dna_kit::Registry::default();
            dna_view_classic::install(&mut registry);
            let _ = ready.set_attribute("data-dnacalc-preview-ready", "true");
            let open = view! {
                <label class="dna-book-open">
                    "Open workbook"
                    <input type="file" accept=".xlsx,.xlsm,.xlsb,.xls" data-dnacalc="open-file" on:change=move |event| {
                        let input: web_sys::HtmlInputElement = event_target(&event);
                        let file = input.files().and_then(|files| files.item(0));
                        input.set_value("");
                        if let Some(file) = file {
                            wasm_bindgen_futures::spawn_local(async move {
                                match wasm_bindgen_futures::JsFuture::from(file.array_buffer()).await {
                                    Ok(bytes) => state.open(file.name(), js_sys::Uint8Array::new(&bytes).to_vec()),
                                    Err(_) => state.error(Refusal::Transport("Could not read the workbook file".into())),
                                }
                            });
                        }
                    }/>
                </label>
            }.into_any();
            workbook(&registry, state, open)
        }).forget();
        Ok(())
    }
}

fn main() {}
