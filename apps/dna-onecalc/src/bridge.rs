//! The page's side of the desktop bridge: a [`Link`] whose session is the native one behind the
//! WebView2 page.
//!
//! The desktop exe hosts this same page in WebView2 and keeps the session on the native thread
//! that owns add-in calls. A request goes out as one string through `chrome.webview.postMessage`
//! (the platform's string bridge, `calc-platform-desktop`'s `WebView`), as the JSON of a [`Call`]; the
//! host answers with one string, the JSON of an [`Answer`] with the same id. Answers arrive in the
//! order the requests were made. A browser has no `chrome.webview`, so [`connect`] finds nothing
//! there and the surface runs over a [`crate::port::LocalLink`].

use crate::port::{Link, Reply, Request};
use serde::{Deserialize, Serialize};

/// A request on the wire: its id and the request.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Call {
    pub id: u64,
    pub request: Request,
}

/// A message the host sends on its own, with no call to answer: something changed on its side that
/// the page should look at again (`rtd`: a real-time value arrived).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub event: String,
}

/// An answer on the wire: the id of the call it answers and the reply.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Answer {
    pub id: u64,
    pub reply: Reply,
}

#[cfg(target_arch = "wasm32")]
mod page {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::collections::BTreeMap;
    use std::rc::Rc;
    use wasm_bindgen::JsCast;
    use wasm_bindgen::JsValue;
    use wasm_bindgen::closure::Closure;

    type Waiting = Rc<RefCell<BTreeMap<u64, Box<dyn FnOnce(Reply)>>>>;
    type Events = Rc<RefCell<Option<Rc<dyn Fn(String)>>>>;

    /// A link to the native session behind WebView2.
    pub struct BridgeLink {
        host: JsValue,
        next: Cell<u64>,
        waiting: Waiting,
        events: Events,
        _listener: Closure<dyn FnMut(JsValue)>,
    }

    /// The bridge, when the page runs in the desktop exe's WebView2.
    pub fn connect() -> Option<BridgeLink> {
        let window = web_sys::window()?;
        let chrome = js_sys::Reflect::get(&window, &JsValue::from_str("chrome")).ok()?;
        if chrome.is_undefined() || chrome.is_null() {
            return None;
        }
        let host = js_sys::Reflect::get(&chrome, &JsValue::from_str("webview")).ok()?;
        if host.is_undefined() || host.is_null() {
            return None;
        }
        let waiting: Waiting = Rc::new(RefCell::new(BTreeMap::new()));
        let inbox = Rc::clone(&waiting);
        let events: Events = Rc::new(RefCell::new(None));
        let event_inbox = Rc::clone(&events);
        let listener = Closure::<dyn FnMut(JsValue)>::new(move |event: JsValue| {
            let Some(text) = js_sys::Reflect::get(&event, &JsValue::from_str("data"))
                .ok()
                .and_then(|data| data.as_string())
            else {
                return;
            };
            let Ok(answer) = serde_json::from_str::<Answer>(&text) else {
                if let Ok(event) = serde_json::from_str::<Event>(&text) {
                    let handler = event_inbox.borrow().clone();
                    if let Some(handler) = handler {
                        handler(event.event);
                    }
                }
                return;
            };
            let done = inbox.borrow_mut().remove(&answer.id);
            if let Some(done) = done {
                done(answer.reply);
            }
        });
        let add = js_sys::Reflect::get(&host, &JsValue::from_str("addEventListener")).ok()?;
        let add: js_sys::Function = add.dyn_into().ok()?;
        add.call2(&host, &JsValue::from_str("message"), listener.as_ref())
            .ok()?;
        Some(BridgeLink {
            host,
            next: Cell::new(1),
            waiting,
            events,
            _listener: listener,
        })
    }

    impl Link for BridgeLink {
        fn call(&self, request: Request, done: Box<dyn FnOnce(Reply)>) {
            let id = self.next.get();
            self.next.set(id + 1);
            let text = match serde_json::to_string(&Call { id, request }) {
                Ok(text) => text,
                Err(error) => {
                    done(Reply::Refused {
                        message: format!("the request could not be encoded: {error}"),
                    });
                    return;
                }
            };
            self.waiting.borrow_mut().insert(id, done);
            let post = js_sys::Reflect::get(&self.host, &JsValue::from_str("postMessage"))
                .ok()
                .and_then(|post| post.dyn_into::<js_sys::Function>().ok());
            let posted =
                post.and_then(|post| post.call1(&self.host, &JsValue::from_str(&text)).ok());
            if posted.is_none()
                && let Some(done) = self.waiting.borrow_mut().remove(&id)
            {
                done(Reply::Refused {
                    message: "the desktop host did not take the request".to_owned(),
                });
            }
        }

        fn native_files(&self) -> bool {
            true
        }

        fn on_event(&self, handler: Rc<dyn Fn(String)>) {
            *self.events.borrow_mut() = Some(handler);
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use page::{BridgeLink, connect};

/// A browser has no desktop bridge.
#[cfg(not(target_arch = "wasm32"))]
pub fn connect() -> Option<NoBridge> {
    None
}

/// Never constructed off the browser: [`connect`] finds no bridge there.
#[cfg(not(target_arch = "wasm32"))]
pub enum NoBridge {}

#[cfg(not(target_arch = "wasm32"))]
impl Link for NoBridge {
    fn call(&self, _request: Request, _done: Box<dyn FnOnce(Reply)>) {
        match *self {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shown::Authored;

    /// The envelope and every new message survive the JSON the bridge carries.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn calls_and_answers_round_trip_through_the_wire_text() {
        let calls = vec![
            Call {
                id: 1,
                request: Request::Evaluate(Authored::example_sum()),
            },
            Call {
                id: 2,
                request: Request::Open {
                    name: "a.xlsx".into(),
                    bytes: (0u8..=255).collect(),
                    zone: Some(3),
                },
            },
            Call {
                id: 3,
                request: Request::SaveNative,
            },
        ];
        for call in calls {
            let text = serde_json::to_string(&call).unwrap();
            assert_eq!(serde_json::from_str::<Call>(&text).unwrap(), call);
        }
        let event = Event {
            event: "rtd".to_owned(),
        };
        let text = serde_json::to_string(&event).unwrap();
        assert_eq!(serde_json::from_str::<Event>(&text).unwrap(), event);
        // An event is no answer, and an answer is no event: the page tells them apart by parsing.
        assert!(serde_json::from_str::<Answer>(&text).is_err());
        let answer = serde_json::to_string(&Answer {
            id: 1,
            reply: Reply::Accepted,
        })
        .unwrap();
        assert!(serde_json::from_str::<Event>(&answer).is_err());
        let mut port = crate::port::LocalPort::new(crate::Session::new("Sheet1").unwrap());
        use crate::port::SessionPort;
        for request in [
            Request::Evaluate(Authored::example_sum()),
            Request::Save,
            Request::New,
            Request::Open {
                name: "x".into(),
                bytes: b"not a workbook".to_vec(),
                zone: None,
            },
        ] {
            let answer = Answer {
                id: 7,
                reply: port.send(request),
            };
            let text = serde_json::to_string(&answer).unwrap();
            assert_eq!(serde_json::from_str::<Answer>(&text).unwrap(), answer);
        }
    }
}
