#[cfg(target_arch = "wasm32")]
mod worker {
    use calc_host::{
        FormatProfile, HostOptions, HostSession, MemorySource, OpenPolicy, WorkBudget,
        blank_package,
    };
    use calc_skin::{Envelope, Message, Origin, Packet, Refusal, ViewId, WritePolicy, decode};
    use calc_skin_host::{Connection, SkinHost};
    use std::{rc::Rc, sync::Arc, time::Duration};
    use wasm_bindgen::{JsCast, JsValue, closure::Closure, prelude::wasm_bindgen};
    use web_sys::{DedicatedWorkerGlobalScope, MessageEvent};

    fn post(scope: &DedicatedWorkerGlobalScope, envelope: Envelope) -> Result<(), JsValue> {
        let text = serde_json::to_string(&envelope).map_err(|error| error.to_string())?;
        scope.post_message(&text.into())
    }

    fn frames(scope: &DedicatedWorkerGlobalScope, port: &Connection) -> Result<(), JsValue> {
        for Packet { envelope, bytes } in
            port.next_packets().map_err(|error| format!("{error:?}"))?
        {
            match bytes {
                Some(bytes) => {
                    let control =
                        serde_json::to_string(&envelope).map_err(|error| error.to_string())?;
                    let bytes = js_sys::Uint8Array::from(bytes.as_slice()).buffer();
                    let message = js_sys::Array::of2(&control.into(), &bytes);
                    scope.post_message_with_transfer(&message, &js_sys::Array::of1(&bytes))?;
                }
                None => post(scope, envelope)?,
            }
        }
        Ok(())
    }

    fn advance(scope: DedicatedWorkerGlobalScope, host: Rc<SkinHost>, port: Rc<Connection>) {
        let task = Closure::once_into_js({
            let scope = scope.clone();
            move || {
                let more = host.advance();
                if let Err(error) = frames(&scope, &port) {
                    wasm_bindgen::throw_val(error);
                }
                if more {
                    advance(scope, host, port);
                }
            }
        });
        if let Err(error) =
            scope.set_timeout_with_callback_and_timeout_and_arguments_0(task.unchecked_ref(), 0)
        {
            wasm_bindgen::throw_val(error);
        }
    }

    fn clock(
        scope: &DedicatedWorkerGlobalScope,
        host: Rc<SkinHost>,
        port: Rc<Connection>,
    ) -> Result<(), JsValue> {
        let Some(time) = scope.performance() else {
            return Ok(());
        };
        host.enable_model_clock();
        let mut last = time.now();
        let drive = Closure::<dyn FnMut()>::new({
            let scope = scope.clone();
            move || {
                let now = time.now();
                let elapsed = Duration::from_secs_f64((now - last).max(0.0) / 1000.0);
                last = now;
                if host.advance_model_clock(elapsed) {
                    advance(scope.clone(), host.clone(), port.clone());
                }
            }
        });
        scope.set_interval_with_callback_and_timeout_and_arguments_0(
            drive.as_ref().unchecked_ref(),
            50,
        )?;
        drive.forget();
        Ok(())
    }

    #[wasm_bindgen(start)]
    pub fn start() -> Result<(), JsValue> {
        let scope: DedicatedWorkerGlobalScope = js_sys::global().dyn_into()?;
        let locale = scope
            .navigator()
            .language()
            .as_deref()
            .and_then(FormatProfile::for_culture);
        let mut session = HostSession::open(
            Arc::new(MemorySource::new(blank_package())),
            &OpenPolicy::default(),
            HostOptions {
                locale,
                ..HostOptions::default()
            },
        )
        .map_err(|error| format!("{error:?}"))?;
        session.set_calculation_budget(WorkBudget::evaluations(64));
        let host = Rc::new(SkinHost::new(session));
        let port = Rc::new(host.connect(
            Origin::User {
                view: ViewId("classic".into()),
            },
            WritePolicy::Commit,
        ));
        clock(&scope, host.clone(), port.clone())?;
        let receive = Closure::<dyn FnMut(MessageEvent)>::new({
            let scope = scope.clone();
            move |event: MessageEvent| {
                let result = (|| {
                    let data = event.data();
                    let (control, bytes) = if js_sys::Array::is_array(&data) {
                        let packet = js_sys::Array::from(&data);
                        (
                            packet.get(0).as_string(),
                            Some(js_sys::Uint8Array::new(&packet.get(1)).to_vec()),
                        )
                    } else {
                        (data.as_string(), None)
                    };
                    let packet = control
                        .ok_or_else(|| Refusal::Transport("Expected a control message".into()))
                        .and_then(|text| decode(text.as_bytes(), bytes));
                    let reply = match packet {
                        Ok(packet) => port.handle(packet).envelope,
                        Err(reason) => Envelope::new(0, Message::Failure(reason)),
                    };
                    post(&scope, reply)?;
                    frames(&scope, &port)?;
                    if host.with_session(calc_skin::SelectionCause::Open, |session| {
                        session.has_pending_calculation()
                    }) {
                        advance(scope.clone(), host.clone(), port.clone());
                    }
                    Ok::<(), JsValue>(())
                })();
                if let Err(error) = result {
                    wasm_bindgen::throw_val(error);
                }
            }
        });
        scope.set_onmessage(Some(receive.as_ref().unchecked_ref()));
        receive.forget();
        Ok(())
    }
}

fn main() {}
