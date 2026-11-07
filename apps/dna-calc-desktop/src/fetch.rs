//! The desktop's fetch route for WEBSERVICE and IMAGE. `calc_func::fetch_following` holds every
//! rule (redirects, statuses, bodies); this only runs a fetch on its own thread over WinHTTP and
//! tells the UI thread, which hands the reply to the server. There is no cross-origin restriction.
use crate::winhttp;
use calc_func::{FetchKind, FetchReply, FetchRequest, FetchRoute, fetch_following};
use std::{
    cell::Cell,
    fmt,
    sync::{Arc, Mutex},
};
use windows_sys::Win32::{
    System::Threading::GetCurrentThreadId,
    UI::WindowsAndMessaging::{PostThreadMessageW, WM_APP},
};

/// Posted to the UI thread when a fetch has finished.
pub const WM_FETCH: u32 = WM_APP + 2;

type Done = Arc<Mutex<Vec<(u64, FetchKind, FetchReply)>>>;

pub struct ThreadedFetch {
    next: Cell<u64>,
    thread: u32,
    done: Done,
}

impl fmt::Debug for ThreadedFetch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ThreadedFetch").finish_non_exhaustive()
    }
}

impl ThreadedFetch {
    /// A route whose replies are announced to the calling thread.
    pub fn new() -> Self {
        Self {
            next: Cell::new(0),
            thread: unsafe { GetCurrentThreadId() },
            done: Done::default(),
        }
    }

    /// The finished fetches, oldest first.
    pub fn take(&self) -> Vec<(u64, FetchKind, FetchReply)> {
        std::mem::take(&mut *self.done.lock().unwrap_or_else(|e| e.into_inner()))
    }
}

impl FetchRoute for ThreadedFetch {
    fn begin(&self, request: FetchRequest) -> u64 {
        let id = self.next.get() + 1;
        self.next.set(id);
        let (done, thread) = (self.done.clone(), self.thread);
        std::thread::spawn(move || {
            let reply = fetch_following(request.kind, &request.url, &mut |url| winhttp::get(url));
            done.lock()
                .unwrap_or_else(|e| e.into_inner())
                .push((id, request.kind, reply));
            unsafe { PostThreadMessageW(thread, WM_FETCH, 0, 0) };
        });
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dna_calc_worker::{
        server::Server,
        wire::{Intent, Key, Request},
    };
    use std::{
        io::{Read, Write},
        net::TcpListener,
        rc::Rc,
        time::{Duration, Instant},
    };

    /// A local server: `/text`, `/redir` (to `/text`), `/gone` (404) and `/slow-empty` (204).
    fn serve() -> u16 {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let mut head = [0u8; 2048];
                let read = stream.read(&mut head).unwrap_or(0);
                let head = String::from_utf8_lossy(&head[..read]).into_owned();
                let path = head.split_whitespace().nth(1).unwrap_or("/").to_owned();
                let (status, extra, body) = match path.as_str() {
                    "/text" => ("200 OK", "", "caf\u{e9} ok"),
                    "/redir" => ("302 Found", "Location: /text\r\n", ""),
                    "/gone" => ("404 Not Found", "", "no"),
                    _ => ("204 No Content", "", ""),
                };
                let reply = format!(
                    "HTTP/1.1 {status}\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(reply.as_bytes());
            }
        });
        port
    }

    fn text_of(server: &Server, at: calc_value::CellKey) -> String {
        let page = calc_host::project(
            server.book().host(),
            &calc_host::ProjectionRequest { cells: vec![at] },
        )
        .unwrap();
        match page.cells()[0].value() {
            calc_host::ProjectedValue::Calculated { value, .. } => crate::value_json(value).to_string(),
            other => format!("{other:?}"),
        }
    }

    /// Native-only: a real GET over WinHTTP to a local server, through the shared redirect rules
    /// and the server's async path, with no cross-origin policy in between (dc-fetch-f0tp).
    #[test]
    fn webservice_fetches_over_winhttp_and_follows_redirects() {
        let port = serve();
        let mut server = Server::new();
        let route = Rc::new(ThreadedFetch::new());
        server.set_fetch_route(route.clone());
        let sheet = server.book().host().sheets()[0].id();
        let cells: Vec<_> = (1..=4).map(|row| calc_value::CellKey::new(sheet, row, 2).unwrap()).collect();
        let urls = ["text", "redir", "gone", "nowhere-204"];
        for (cell, url) in cells.iter().zip(urls) {
            server
                .execute_current(
                    Request::Intent(Intent::EnterCell {
                        at: Key::of(*cell),
                        text: format!("=WEBSERVICE(\"http://127.0.0.1:{port}/{url}\")"),
                    }),
                    None,
                )
                .reply
                .unwrap();
        }
        // Run until every cell has settled: nothing calculating, nothing awaiting a reply. WEBSERVICE
        // is volatile, so a reply can start the next round.
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut answered = 0;
        let mut quiet = 0;
        while quiet < 20 && Instant::now() < deadline {
            let mut busy = false;
            while server.has_pending_calculation() {
                server.advance_calculation();
                busy = true;
            }
            for (request, kind, reply) in route.take() {
                answered += 1;
                server.complete_fetch(request, reply.into_value(kind));
                busy = true;
            }
            busy |= !server.pending_async().is_empty();
            quiet = if busy { 0 } else { quiet + 1 };
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(answered >= 4, "every fetch replied");
        assert_eq!(text_of(&server, cells[0]), "\"caf\u{e9} ok\"");
        assert_eq!(text_of(&server, cells[1]), "\"caf\u{e9} ok\"", "a 302 is followed");
        assert_eq!(text_of(&server, cells[2]), "{\"error\":\"#VALUE!\"}", "status 404 is #VALUE!");
        assert_eq!(text_of(&server, cells[3]), "\"\"", "status 204 is empty text");
    }
}
