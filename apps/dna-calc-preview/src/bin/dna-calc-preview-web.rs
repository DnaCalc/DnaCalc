#[cfg(target_arch = "wasm32")]
mod browser {
    use calc_skin::{Packet, Refusal, ViewId, decode};
    use dna_kit::Session;
    use dna_shell::workbook::workbook;
    use leptos::prelude::*;
    use std::cell::RefCell;
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
            // The carrier: packets out to the worker, packets in to the session. The session's
            // client owns the requests in flight and the tile frames.
            let failed = Rc::new(RefCell::new(None::<Refusal>));
            let state = Session::new(ViewId(dna_view_classic::ID.into()), {
                let worker = worker.clone();
                let failed = failed.clone();
                move |packet: Packet| {
                    let sent = serde_json::to_string(&packet.envelope)
                        .map_err(|error| Refusal::Transport(error.to_string()))
                        .and_then(|text| {
                            let send = match packet.bytes {
                                Some(bytes) => {
                                    let bytes = js_sys::Uint8Array::from(bytes.as_slice()).buffer();
                                    worker.post_message_with_transfer(&js_sys::Array::of2(&text.into(), &bytes), &js_sys::Array::of1(&bytes))
                                }
                                None => worker.post_message(&text.into()),
                            };
                            send.map_err(|error| Refusal::Transport(format!("Could not reach the workbook worker: {error:?}")))
                        });
                    if let Err(reason) = sent {
                        *failed.borrow_mut() = Some(reason);
                    }
                }
            });
            let receive = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
                let data = event.data();
                let (control, bytes) = if js_sys::Array::is_array(&data) {
                    let packet = js_sys::Array::from(&data);
                    (packet.get(0).as_string(), Some(js_sys::Uint8Array::new(&packet.get(1)).to_vec()))
                } else {
                    (data.as_string(), None)
                };
                let Some(text) = control else { return };
                match decode(text.as_bytes(), bytes) {
                    Ok(packet) => state.deliver(packet),
                    Err(reason) => state.refuse(reason),
                }
                // A send that failed inside the call above is reported once the session is free.
                if let Some(reason) = failed.borrow_mut().take() {
                    state.refuse(reason);
                }
            });
            worker.set_onmessage(Some(receive.as_ref().unchecked_ref()));
            receive.forget();
            let fault = Closure::<dyn FnMut(ErrorEvent)>::new(move |event: ErrorEvent| {
                state.refuse(Refusal::Transport(event.message()));
            });
            worker.set_onerror(Some(fault.as_ref().unchecked_ref()));
            fault.forget();
            let probe = Closure::<dyn FnMut() -> JsValue>::new(move || {
                js_sys::JSON::parse(&dna_shell::workbook::probe(state).to_string()).unwrap_or(JsValue::NULL)
            });
            let _ = js_sys::Reflect::set(&window, &JsValue::from_str("__dnaProbe"), probe.as_ref());
            probe.forget();
            let mut registry = dna_kit::Registry::default();
            dna_view_classic::install(&mut registry);
            dna_view_explorer::install_docked(&mut registry);
            dna_view_inspector::install(&mut registry);
            dna_view_notebook::install(&mut registry);
            dna_view_map::install(&mut registry);
            dna_view_app::install(&mut registry);
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
                                    Ok(bytes) => dna_kit::workbook_open::request(state, file.name(), js_sys::Uint8Array::new(&bytes).to_vec()),
                                    Err(_) => state.refuse(Refusal::Transport("Could not read the workbook file".into())),
                                }
                            });
                        }
                    }/>
                </label>
            }.into_any();
            let chrome = dna_shell::workbook::install(&registry, state);
            workbook(&registry, state, chrome, open)
        }).forget();
        Ok(())
    }
}

fn main() {}
