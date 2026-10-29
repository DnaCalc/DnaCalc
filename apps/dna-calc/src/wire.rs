//! The messages between the page and the calculation worker (dc-p96).
//!
//! The workbook's session moves off the page's main thread into a dedicated web worker
//! ([`crate::worker`]); the page keeps what its skins show and talks to the worker through
//! [`crate::client`]. Every message is one JSON text (serde_json) carried by `postMessage`. Both
//! sides are the same wasm build, so the messages are not versioned; they are tested to read back
//! as they were written ([`decode`] of [`encode`]).
//!
//! - The worker's first message, once its module and wasm are loaded, is
//!   [`FromWorker::Started`]; the page sends nothing before it.
//! - The page sends [`ToWorker`]: a request with an id the page chose.
//! - The worker answers each request once, with [`FromWorker::Reply`] under the same id, or
//!   [`FromWorker::Failed`] when it could not.

use serde::{Deserialize, Serialize, de::DeserializeOwned};

/// The page to the worker.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToWorker {
    pub id: u64,
    pub request: Request,
}

/// What the page asks of the worker.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Request {
    /// An answer, to show the worker is there and to time a round trip.
    Ping,
}

/// What the worker answers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Reply {
    /// The answer to [`Request::Ping`]: the package version the worker runs.
    Pong { version: String },
}

/// The worker to the page.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum FromWorker {
    /// The worker is loaded and listening.
    Started,
    /// The answer to the request `id`.
    Reply { id: u64, reply: Reply },
    /// The request `id` (or, with no id, a message that was not a request) could not be served.
    Failed { id: Option<u64>, message: String },
}

/// A message as the JSON text `postMessage` carries.
pub fn encode<T: Serialize>(message: &T) -> String {
    serde_json::to_string(message).expect("a wire message always encodes")
}

/// A message from its JSON text.
pub fn decode<T: DeserializeOwned>(text: &str) -> Result<T, String> {
    serde_json::from_str(text).map_err(|e| format!("not a message: {e}"))
}

/// The worker's answer to a request: what it serves without a workbook.
pub fn serve(request: &Request) -> Reply {
    match request {
        Request::Ping => Reply::Pong {
            version: env!("CARGO_PKG_VERSION").to_owned(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn messages_read_back_as_written() {
        let request = ToWorker {
            id: 7,
            request: Request::Ping,
        };
        assert_eq!(decode::<ToWorker>(&encode(&request)), Ok(request));
        for message in [
            FromWorker::Started,
            FromWorker::Reply {
                id: 7,
                reply: serve(&Request::Ping),
            },
            FromWorker::Failed {
                id: None,
                message: "no".to_owned(),
            },
        ] {
            assert_eq!(decode::<FromWorker>(&encode(&message)), Ok(message));
        }
        assert!(decode::<ToWorker>("{\"id\":1}").is_err());
    }
}
