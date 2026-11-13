//! Loopback browser carrier for the application's native workbook Server (dc-icq4.4).
//! The URL is a per-run capability: never include request paths in diagnostics.
use base64::{Engine, engine::general_purpose::STANDARD};
use calc_platform_desktop::assets::{self, Assets};
use dna_calc_protocol::{self as wire, Event, FromServer};
use dna_calc_worker::server::{self, Server};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::{Ipv4Addr, TcpListener, TcpStream},
    path::Path,
    sync::Arc,
    time::Duration,
};
use tungstenite::{Message, WebSocket, protocol::Role};

const BRIDGE: &str = include_str!("../../dna-calc/web/desktop-bridge.js");
const MAX_HEADER: usize = 16 * 1024;
const STACK: usize = 256 * 1024 * 1024;

#[cfg(embedded_page)]
const PAGE: &[u8] = include_bytes!(env!("CALC_PAGE_BLOCK"));
#[cfg(not(embedded_page))]
const PAGE: &[u8] = &[];

pub fn run(args: &[String], out: &mut dyn Write) -> Result<(), String> {
    let mut port = 0;
    let mut directory = None;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--port" => {
                port = args
                    .next()
                    .ok_or("--port needs a number")?
                    .parse::<u16>()
                    .map_err(|_| "--port needs a number from 0 to 65535")?;
            }
            "--assets" => directory = Some(args.next().ok_or("--assets needs a directory")?),
            _ => return Err("expected --port <n> or --assets <dir>".into()),
        }
    }
    let page = match directory {
        Some(dir) => read_page(Path::new(dir))?,
        None if !PAGE.is_empty() => Assets::parse(PAGE)?,
        None => {
            return Err(concat!(
                "this build carries no page; give --assets DIR (cargo xtask app bundle calc), ",
                "or build with DNACALC_CALC_PAGE naming a packed page"
            )
            .into());
        }
    };
    let runner = Runner::bind(page, port)?;
    writeln!(
        out,
        "Open {} in your browser; Ctrl+C stops the server.",
        runner.url()
    )
    .map_err(|e| e.to_string())?;
    out.flush().map_err(|e| e.to_string())?;
    runner.run()
}

fn read_page(dir: &Path) -> Result<Assets, String> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(|e| format!("cannot read page directory: {e}"))? {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.file_type().map_err(|e| e.to_string())?.is_file() {
            let name = entry
                .file_name()
                .to_str()
                .ok_or("page name is not UTF-8")?
                .to_owned();
            files.push((
                name,
                std::fs::read(entry.path()).map_err(|e| e.to_string())?,
            ));
        }
    }
    Assets::parse(&assets::pack(&files))
}

/// One listener; each WebSocket owns its own Server channels and releases them on disconnect.
pub struct Runner {
    listener: TcpListener,
    policy: Arc<Policy>,
}

struct Policy {
    authority: String,
    origin: String,
    prefix: String,
    page: Assets,
}

impl Runner {
    pub fn bind(page: Assets, port: u16) -> Result<Self, String> {
        if page.get("index.html").is_none() {
            return Err("the page has no index.html".into());
        }
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port)).map_err(|e| e.to_string())?;
        let authority = listener
            .local_addr()
            .map_err(|e| e.to_string())?
            .to_string();
        let mut secret = [0u8; 32];
        getrandom::fill(&mut secret)
            .map_err(|_| "the operating system could not supply a random URL")?;
        let token = secret
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        Ok(Self {
            listener,
            policy: Arc::new(Policy {
                origin: format!("http://{authority}"),
                authority,
                prefix: format!("/{token}/"),
                page,
            }),
        })
    }

    /// The per-run browser capability. The caller displays it to the local user only.
    pub fn url(&self) -> String {
        format!("{}{}", self.policy.origin, self.policy.prefix)
    }

    pub fn run(self) -> Result<(), String> {
        for stream in self.listener.incoming() {
            let stream = stream.map_err(|e| e.to_string())?;
            let policy = self.policy.clone();
            std::thread::Builder::new()
                .name("dnacalc-browser".into())
                .stack_size(STACK)
                .spawn(move || {
                    // A disconnected or malformed client affects only its own session.
                    let _ = connection(stream, &policy);
                })
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

fn response(
    stream: &mut TcpStream,
    status: &str,
    kind: &str,
    body: &[u8],
    policy: &Policy,
) -> std::io::Result<()> {
    let csp = format!(
        concat!(
            "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self'; ",
            "img-src 'self' data:; connect-src 'self' ws://{}/; worker-src 'self'; ",
            "object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'"
        ),
        policy.authority
    );
    write!(
        stream,
        concat!(
            "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {length}\r\n",
            "Connection: close\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\n",
            "X-Content-Type-Options: nosniff\r\nCross-Origin-Resource-Policy: same-origin\r\n",
            "Content-Security-Policy: {csp}\r\n\r\n"
        ),
        status = status,
        kind = kind,
        length = body.len(),
        csp = csp
    )?;
    stream.write_all(body)
}

fn connection(mut stream: TcpStream, policy: &Policy) -> Result<(), String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(30)))
        .map_err(|e| e.to_string())?;
    stream.set_nodelay(true).map_err(|e| e.to_string())?;
    // Read only the HTTP header: the WebSocket decoder must see all subsequent frame bytes.
    let mut header = Vec::new();
    while !header.ends_with(b"\r\n\r\n") && header.len() < MAX_HEADER {
        let mut byte = [0];
        stream
            .read_exact(&mut byte)
            .map_err(|_| "incomplete HTTP header")?;
        header.push(byte[0]);
    }
    let mut slots = [httparse::EMPTY_HEADER; 64];
    let mut request = httparse::Request::new(&mut slots);
    let complete = request
        .parse(&header)
        .is_ok_and(|status| status.is_complete());
    let get = |name: &str| {
        let mut values = request
            .headers
            .iter()
            .filter(|h| h.name.eq_ignore_ascii_case(name));
        let value = values
            .next()
            .and_then(|h| std::str::from_utf8(h.value).ok());
        if values.next().is_some() { None } else { value }
    };
    let path = request.path.unwrap_or("");
    let name = path.strip_prefix(&policy.prefix);
    let origin_count = request
        .headers
        .iter()
        .filter(|h| h.name.eq_ignore_ascii_case("origin"))
        .count();
    let allowed = complete
        && request.method == Some("GET")
        && request.version == Some(1)
        && get("host") == Some(policy.authority.as_str())
        && name.is_some()
        && (origin_count == 0 || get("origin") == Some(policy.origin.as_str()))
        && !matches!(get("sec-fetch-site"), Some("cross-site"));
    if !allowed {
        return response(
            &mut stream,
            "403 Forbidden",
            "text/plain",
            b"Forbidden",
            policy,
        )
        .map_err(|e| e.to_string());
    }
    let name = name.unwrap().split('?').next().unwrap_or("");
    if name == "socket" {
        if get("origin") != Some(policy.origin.as_str()) {
            return response(
                &mut stream,
                "403 Forbidden",
                "text/plain",
                b"Forbidden",
                policy,
            )
            .map_err(|e| e.to_string());
        }
        let mut upgrade = tungstenite::http::Request::builder()
            .method("GET")
            .uri(path);
        for h in request.headers.iter() {
            upgrade = upgrade.header(h.name, h.value);
        }
        let upgrade = upgrade.body(()).map_err(|_| "invalid WebSocket header")?;
        let Ok(accepted) = tungstenite::handshake::server::create_response(&upgrade) else {
            return response(
                &mut stream,
                "400 Bad Request",
                "text/plain",
                b"Invalid WebSocket handshake",
                policy,
            )
            .map_err(|e| e.to_string());
        };
        tungstenite::handshake::server::write_response(&mut stream, &accepted)
            .map_err(|e| e.to_string())?;
        stream
            .set_read_timeout(Some(Duration::from_millis(2)))
            .map_err(|e| e.to_string())?;
        // Workbook bytes have the application's own admission rules; the carrier must not
        // impose a smaller file limit merely because base64 expands a WebSocket frame.
        let config = tungstenite::protocol::WebSocketConfig::default()
            .max_message_size(None)
            .max_frame_size(None);
        let socket = WebSocket::from_raw_socket(stream, Role::Server, Some(config));
        return worker(socket);
    }
    let name = if name.is_empty() { "index.html" } else { name };
    let injected;
    let body =
        if name == "desktop-bridge.js" {
            BRIDGE.as_bytes()
        } else if let Some(body) = policy.page.get(name) {
            if name == "index.html" {
                let html = std::str::from_utf8(body).map_err(|_| "index.html is not UTF-8")?;
                injected = html.replacen("<script type=\"module\"", concat!(
                "<script src=\"desktop-bridge.js\" data-dnacalc-socket=\"socket\"></script>\n",
                "<script type=\"module\""
            ), 1);
                injected.as_bytes()
            } else {
                body
            }
        } else {
            return response(
                &mut stream,
                "404 Not Found",
                "text/plain",
                b"Not found",
                policy,
            )
            .map_err(|e| e.to_string());
        };
    response(
        &mut stream,
        "200 OK",
        assets::content_type(name),
        body,
        policy,
    )
    .map_err(|e| e.to_string())
}

fn post(
    socket: &mut WebSocket<TcpStream>,
    channel: u32,
    text: &str,
    bytes: Option<&[u8]>,
) -> Result<(), String> {
    let prefix = if channel == 0 {
        String::new()
    } else {
        format!("@{channel}\u{2}")
    };
    let message = match bytes {
        Some(bytes) => format!("{prefix}{text}\u{1}{}", STANDARD.encode(bytes)),
        None => format!("{prefix}{text}"),
    };
    socket
        .send(Message::Text(message.into()))
        .map_err(|_| "browser socket disconnected".into())
}

fn events(
    socket: &mut WebSocket<TcpStream>,
    channel: u32,
    server: &mut Server,
) -> Result<(), String> {
    for event in server.take_events() {
        match event {
            Event::Tiles(batch) => post(
                socket,
                channel,
                wire::TILE_TAG,
                Some(&wire::encode_tiles(&batch)?),
            )?,
            event => post(
                socket,
                channel,
                &wire::encode(&FromServer::Event(event)),
                None,
            )?,
        }
    }
    Ok(())
}

fn worker(mut socket: WebSocket<TcpStream>) -> Result<(), String> {
    let mut channels = BTreeMap::<u32, Server>::new();
    loop {
        match socket.read() {
            Ok(Message::Text(text)) => {
                let (channel, text) = match text.strip_prefix('@') {
                    Some(text) => {
                        let (channel, text) = text.split_once('\u{2}').ok_or("invalid channel")?;
                        (channel.parse::<u32>().map_err(|_| "invalid channel")?, text)
                    }
                    None => (0, text.as_str()),
                };
                if text == "hello" {
                    let mut server = Server::new();
                    server.set_calculation_budget(calc_host::WorkBudget::evaluations(64));
                    channels.insert(channel, server);
                    post(
                        &mut socket,
                        channel,
                        &wire::encode(&FromServer::Started),
                        None,
                    )?;
                } else if text == "bye" {
                    channels.remove(&channel);
                } else if let Some(server) = channels.get_mut(&channel) {
                    let (text, bytes) = match text.split_once('\u{1}') {
                        Some((text, bytes)) => (
                            text,
                            Some(STANDARD.decode(bytes).map_err(|_| "invalid binary frame")?),
                        ),
                        None => (text, None),
                    };
                    let (reply, bytes) = server::answer_text(server, text, bytes);
                    events(&mut socket, channel, server)?;
                    post(&mut socket, channel, &reply, bytes.as_deref())?;
                }
            }
            Ok(Message::Close(_)) | Err(tungstenite::Error::ConnectionClosed) => return Ok(()),
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Ok(Message::Ping(_)) | Ok(Message::Pong(_)) => {
                socket.flush().map_err(|_| "browser socket disconnected")?;
            }
            _ => return Err("invalid browser socket message".into()),
        }
        for (&channel, server) in &mut channels {
            if server.has_pending_calculation() {
                server.advance_calculation();
                events(&mut socket, channel, server)?;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tungstenite::client::IntoClientRequest;

    fn runner() -> Runner {
        Runner::bind(
            Assets::parse(&assets::pack(&[
                (
                    "index.html".into(),
                    b"<script type=\"module\" src=\"boot.js\"></script>".to_vec(),
                ),
                ("boot.js".into(), b"// boot".to_vec()),
            ]))
            .unwrap(),
            0,
        )
        .unwrap()
    }

    fn start(runner: &Runner) -> std::thread::JoinHandle<()> {
        let listener = runner.listener.try_clone().unwrap();
        let policy = runner.policy.clone();
        std::thread::Builder::new()
            .stack_size(STACK)
            .spawn(move || {
                let (stream, _) = listener.accept().unwrap();
                let _ = connection(stream, &policy);
            })
            .unwrap()
    }

    fn http(runner: &Runner, path: &str, host: &str, extra: &str) -> String {
        let thread = start(runner);
        let mut stream = TcpStream::connect(runner.listener.local_addr().unwrap()).unwrap();
        write!(stream, "GET {path} HTTP/1.1\r\nHost: {host}\r\n{extra}\r\n").unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        thread.join().unwrap();
        response
    }

    /// Native-only: real loopback HTTP, including DNS rebinding and cross-origin refusals.
    #[test]
    fn http_requires_the_token_and_exact_host_and_origin_without_cors() {
        let runner = runner();
        assert!(runner.listener.local_addr().unwrap().ip().is_loopback());
        let p = &runner.policy;
        for (path, host, extra) in [
            ("/wrong/", p.authority.as_str(), "".to_owned()),
            (p.prefix.as_str(), "attacker.example", String::new()),
            (
                p.prefix.as_str(),
                p.authority.as_str(),
                "Origin: https://attacker.example\r\n".into(),
            ),
            (
                p.prefix.as_str(),
                p.authority.as_str(),
                "Origin: null\r\n".into(),
            ),
            (
                p.prefix.as_str(),
                p.authority.as_str(),
                "Sec-Fetch-Site: cross-site\r\n".into(),
            ),
            (
                p.prefix.as_str(),
                p.authority.as_str(),
                format!("Origin: {}\r\nOrigin: {}\r\n", p.origin, p.origin),
            ),
            (
                p.prefix.as_str(),
                p.authority.as_str(),
                format!("Host: {}\r\n", p.authority),
            ),
        ] {
            let response = http(&runner, path, host, &extra);
            assert!(response.starts_with("HTTP/1.1 403"));
            assert!(
                !response
                    .to_ascii_lowercase()
                    .contains("access-control-allow")
            );
        }
        let response = http(&runner, &p.prefix, &p.authority, "");
        assert!(response.starts_with("HTTP/1.1 200"));
        assert!(response.contains("data-dnacalc-socket=\"socket\""));
        assert!(response.contains(&format!("connect-src 'self' ws://{}/", p.authority)));
        assert!(response.contains("Referrer-Policy: no-referrer"));
        assert!(
            !response
                .to_ascii_lowercase()
                .contains("access-control-allow")
        );
        let response = http(
            &runner,
            &format!("{}boot.js", p.prefix),
            &p.authority,
            &format!("Origin: {}\r\n", p.origin),
        );
        assert!(response.starts_with("HTTP/1.1 200"));
        assert!(response.ends_with("// boot"));
        assert!(
            http(
                &runner,
                &format!("{}../AGENTS.md", p.prefix),
                &p.authority,
                ""
            )
            .starts_with("HTTP/1.1 404")
        );
    }

    /// Native-only: WebSocket authentication uses the actual handshake, before any worker exists.
    #[test]
    fn websocket_refuses_wrong_token_and_cross_origin_and_missing_origin() {
        let runner = runner();
        for (path, origin) in [
            (
                "/wrong/socket".to_owned(),
                Some(runner.policy.origin.as_str()),
            ),
            (
                format!("{}socket", runner.policy.prefix),
                Some("https://attacker.example"),
            ),
            (format!("{}socket", runner.policy.prefix), None),
        ] {
            let thread = start(&runner);
            let mut request = format!("ws://{}{path}", runner.policy.authority)
                .into_client_request()
                .unwrap();
            if let Some(origin) = origin {
                request
                    .headers_mut()
                    .insert("Origin", origin.parse().unwrap());
            }
            let result = tungstenite::connect(request);
            assert!(
                matches!(result, Err(tungstenite::Error::Http(ref reply)) if reply.status() == 403)
            );
            thread.join().unwrap();
        }
    }

    fn receive(
        socket: &mut WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>>,
        channel: u32,
        id: u64,
    ) -> FromServer {
        loop {
            let Message::Text(message) = socket.read().unwrap() else {
                continue;
            };
            let text = if channel == 0 {
                message.as_str()
            } else {
                message.strip_prefix(&format!("@{channel}\u{2}")).unwrap()
            };
            if let Ok(message) = wire::decode::<FromServer>(text) {
                if matches!(message, FromServer::Reply { id: found, .. } | FromServer::Failed { id: Some(found), .. } if found == id)
                {
                    return message;
                }
            }
        }
    }

    /// Native-only: the real socket carries channel framing, workbook bytes and native replies.
    #[test]
    fn websocket_channels_open_workbook_bytes_and_answer_with_the_native_server() {
        let runner = runner();
        let thread = start(&runner);
        let mut request = format!(
            "ws://{}{}socket",
            runner.policy.authority, runner.policy.prefix
        )
        .into_client_request()
        .unwrap();
        request
            .headers_mut()
            .insert("Origin", runner.policy.origin.parse().unwrap());
        let (mut socket, _) = tungstenite::connect(request)
            .unwrap_or_else(|_| panic!("local WebSocket handshake failed"));
        for (channel, prefix) in [(0, ""), (3, "@3\u{2}")] {
            socket
                .send(Message::Text(format!("{prefix}hello").into()))
                .unwrap();
            let Message::Text(started) = socket.read().unwrap() else {
                panic!("expected text");
            };
            assert!(started.as_str() == format!("{prefix}{}", wire::encode(&FromServer::Started)));
            let ping = wire::encode(&wire::ToServer {
                id: 1,
                request: wire::Request::Ping,
            });
            socket
                .send(Message::Text(format!("{prefix}{ping}").into()))
                .unwrap();
            assert!(matches!(
                receive(&mut socket, channel, 1),
                FromServer::Reply {
                    reply: wire::Reply::Pong { .. },
                    ..
                }
            ));
            let summary = wire::encode(&wire::ToServer {
                id: 2,
                request: wire::Request::Summary,
            });
            socket
                .send(Message::Text(format!("{prefix}{summary}").into()))
                .unwrap();
            let FromServer::Reply {
                reply: wire::Reply::Summary(summary),
                ..
            } = receive(&mut socket, channel, 2)
            else {
                panic!("expected workbook summary");
            };
            let open = wire::encode(&wire::ToServer {
                id: 3,
                request: wire::Request::Command(wire::CommandEnvelope {
                    session: summary.session,
                    id: 1,
                    workbook: summary.workbook,
                    base_revision: summary.revision,
                    source: "page".into(),
                    undo_group: None,
                    resolved_selection: None,
                    request: Box::new(wire::Request::Open {
                        name: "cut-base.xlsx".into(),
                    }),
                }),
            });
            let bytes =
                include_bytes!("../../../crates/calc-host/tests/fixtures/dc_gf0/cut-base.xlsx");
            socket
                .send(Message::Text(
                    format!("{prefix}{open}\u{1}{}", STANDARD.encode(bytes)).into(),
                ))
                .unwrap();
            let reply = receive(&mut socket, channel, 3);
            assert!(
                matches!(reply, FromServer::Reply { reply: wire::Reply::Done { ref outcome, .. }, .. } if outcome.error.is_none())
            );
            socket
                .send(Message::Text(format!("{prefix}bye").into()))
                .unwrap();
        }
        socket.close(None).unwrap();
        thread.join().unwrap();
    }

    /// Native-only: command-line refusals and the optional page block are native executable inputs.
    #[test]
    fn serve_rejects_options_and_reads_a_packaged_page() {
        for args in [
            vec!["--port", "65536"],
            vec!["--port"],
            vec!["--host", "0.0.0.0"],
            vec!["book.xlsx"],
        ] {
            let args = args.into_iter().map(str::to_owned).collect::<Vec<_>>();
            assert!(run(&args, &mut Vec::new()).is_err());
        }
        if !PAGE.is_empty() {
            assert!(Assets::parse(PAGE).unwrap().get("index.html").is_some());
        }
    }
}
