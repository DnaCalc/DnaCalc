//! Blocking client of the touchstone/1 answer broker (see `tools/excel-service/touchstone/protocol.py`).
//!
//! Questions are typed function calls; the broker answers them from its pool of Excel engines.  This
//! crate speaks the same length-prefixed JSON frames as the Python client over std sockets: the Linux
//! relay's unix socket (`~/.excel-service/excel.sock`) or the Windows broker's local TCP endpoint
//! (`endpoint.json`: port and token).  No async runtime: one reader thread per connection routes the
//! answer frames to the request that asked.
//!
//! ```no_run
//! use touchstone_client::{Arg, Client};
//! let client = Client::connect()?;
//! let reply = client.ask("ERF.PRECISE", vec![vec![Arg::number_bits(0x3e47bc15e9263272)]])?;
//! println!("{:?} on Excel build {:?}", reply.rows[0], reply.env_id);
//! # Ok::<(), touchstone_client::Error>(())
//! ```
//!
//! Native only: it refuses to compile for wasm32. After a connection drop the client resends
//! unfinished requests with their original IDs and piece numbers. A running broker can replay
//! those pieces from its answer cache. A broker restart loses that cache and can recalculate them.

#[cfg(target_arch = "wasm32")]
compile_error!("touchstone-client is native-only and must not be built for wasm32");

use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::os::unix::net::UnixStream;

pub const VERSION: &str = "touchstone/1";
const FRAME_BUDGET: usize = 40 * 1024;
const MAX_FRAME: usize = 64 * 1024 * 1024;
const SOCKET_TIMEOUT: Duration = Duration::from_secs(10);
const RECONNECT_WINDOW: Duration = Duration::from_secs(90);

/// One typed argument of a call.
#[derive(Clone, Debug, PartialEq)]
pub enum Arg {
    /// The binary64 bits of a number.
    Number(u64),
    /// Text through a worksheet cell.
    Text(String),
    /// Text as a formula literal.
    TextLiteral(String),
    Boolean(bool),
    /// A worksheet error such as `#N/A`.
    Error(String),
    Blank,
    /// A numeric array (row-major bits) of `rows` x `cols`, for the regression and ETS functions.
    Array {
        rows: usize,
        cols: usize,
        bits: Vec<u64>,
    },
}

impl Arg {
    pub fn number(value: f64) -> Arg {
        Arg::Number(value.to_bits())
    }
    pub fn number_bits(bits: u64) -> Arg {
        Arg::Number(bits)
    }
    fn token(&self) -> String {
        match self {
            Arg::Number(_) => "n".into(),
            Arg::Text(_) => "s".into(),
            Arg::TextLiteral(_) => "S".into(),
            Arg::Boolean(_) => "b".into(),
            Arg::Error(_) => "e".into(),
            Arg::Blank => "z".into(),
            Arg::Array { rows, cols, .. } => format!("a{rows}x{cols}"),
        }
    }
    fn push_values(&self, out: &mut Vec<Value>) {
        match self {
            Arg::Number(bits) => out.push(Value::String(format!("{bits:016x}"))),
            Arg::Text(s) | Arg::TextLiteral(s) => out.push(Value::String(s.clone())),
            Arg::Boolean(b) => out.push(Value::Bool(*b)),
            Arg::Error(code) => out.push(Value::String(code.clone())),
            Arg::Blank => out.push(Value::Null),
            Arg::Array { bits, .. } => {
                out.extend(bits.iter().map(|b| Value::String(format!("{b:016x}"))))
            }
        }
    }
    fn width(&self) -> usize {
        match self {
            Arg::Array { rows, cols, .. } => rows * cols,
            _ => 1,
        }
    }
}

/// One typed answer.
#[derive(Clone, Debug, PartialEq)]
pub enum Answer {
    Number(u64),
    Text(String),
    Boolean(bool),
    Error(String),
    Blank,
    /// A rectangular matrix of typed cells (array input echo or spilled array answer).
    Array {
        rows: usize,
        cols: usize,
        cells: Vec<Answer>,
    },
    /// Excel crashed on this call when run alone; recorded, never retried.
    Crashed {
        detail: String,
    },
    /// Excel changed this input on entry, so its reading is not of the input asked for.
    Unread {
        reason: String,
    },
}

impl Answer {
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Answer::Number(bits) => Some(f64::from_bits(*bits)),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub enum Priority {
    Interactive,
    Bulk,
}

/// A question: one function over many calls.
#[derive(Debug)]
pub struct Request {
    pub function: String,
    pub rows: Vec<Vec<Arg>>,
    /// `(row, column)` of the one output of a regression function.
    pub result_index: Option<(u32, u32)>,
    pub priority: Option<Priority>,
    pub echo_inputs: bool,
}

impl Request {
    pub fn new(function: &str, rows: Vec<Vec<Arg>>) -> Request {
        Request {
            function: function.to_string(),
            rows,
            result_index: None,
            priority: None,
            echo_inputs: false,
        }
    }
}

/// The answers to a request, in the order of its calls, and the environment that produced them.
#[derive(Debug)]
pub struct Reply {
    pub rows: Vec<Answer>,
    /// The inputs Excel read back, when `echo_inputs` asked for them.
    pub inputs: Option<Vec<Vec<Answer>>>,
    /// A representative `env_id` for the request (for single-worker compatibility).
    pub env_id: Option<String>,
    /// Representative environment entry for `env_id`.
    pub env: Option<Value>,
    /// The exact `env_id` of the worker that answered each call, in request order.
    pub env_by_row: Vec<Option<String>>,
    /// Every distinct worker environment seen in this reply, keyed by `env_id`.
    pub envs: BTreeMap<String, Value>,
    /// Calls answered per worker name (e.g. `w1`, `w2`, `w3`).
    pub worker_calls: BTreeMap<String, usize>,
    pub seconds: f64,
}

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    /// The broker refused or failed the request: a code (`refused`, `excel-crashed`, ...) and detail.
    Service {
        code: String,
        detail: String,
    },
    Connection(String),
    Timeout,
    Protocol(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "{e}"),
            Error::Service { code, detail } => write!(f, "{code}: {detail}"),
            Error::Connection(m) => write!(f, "connection lost: {m}"),
            Error::Timeout => write!(f, "timed out"),
            Error::Protocol(m) => write!(f, "protocol error: {m}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Error {
        Error::Io(e)
    }
}

// ---- frames -----------------------------------------------------------------------------------

fn pack(kind: u8, body: &Value) -> Vec<u8> {
    let text = serde_json::to_vec(body).expect("a JSON value serialises");
    let mut out = Vec::with_capacity(text.len() + 5);
    out.extend_from_slice(&((text.len() + 1) as u32).to_be_bytes());
    out.push(kind);
    out.extend_from_slice(&text);
    out
}

/// Parses whole frames off the front of `buffer`; a compressed frame (type byte >= 0x80) is refused:
/// local connections are not compressed, and this crate has no zlib.
fn take_frames(buffer: &mut Vec<u8>) -> Result<Vec<(u8, Value)>, Error> {
    let mut frames = Vec::new();
    let mut at = 0;
    while buffer.len() - at >= 4 {
        let size = u32::from_be_bytes(buffer[at..at + 4].try_into().unwrap()) as usize;
        if size == 0 || size > MAX_FRAME {
            return Err(Error::Protocol(format!("frame of {size} bytes")));
        }
        if buffer.len() - at - 4 < size {
            break;
        }
        let kind = buffer[at + 4];
        if kind & 0x80 != 0 {
            return Err(Error::Protocol(
                "compressed frame on a local connection".into(),
            ));
        }
        let body: Value = serde_json::from_slice(&buffer[at + 5..at + 4 + size])
            .map_err(|e| Error::Protocol(e.to_string()))?;
        frames.push((kind, body));
        at += 4 + size;
    }
    buffer.drain(..at);
    Ok(frames)
}

enum Stream {
    Tcp(TcpStream),
    #[cfg(unix)]
    Unix(UnixStream),
}

impl Stream {
    fn try_clone(&self) -> io::Result<Stream> {
        Ok(match self {
            Stream::Tcp(s) => Stream::Tcp(s.try_clone()?),
            #[cfg(unix)]
            Stream::Unix(s) => Stream::Unix(s.try_clone()?),
        })
    }

    fn set_timeouts(&self, read: Option<Duration>, write: Option<Duration>) -> io::Result<()> {
        match self {
            Stream::Tcp(s) => {
                s.set_read_timeout(read)?;
                s.set_write_timeout(write)
            }
            #[cfg(unix)]
            Stream::Unix(s) => {
                s.set_read_timeout(read)?;
                s.set_write_timeout(write)
            }
        }
    }

    fn shutdown(&self) {
        let _ = match self {
            Stream::Tcp(s) => s.shutdown(Shutdown::Both),
            #[cfg(unix)]
            Stream::Unix(s) => s.shutdown(Shutdown::Both),
        };
    }
}

impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Stream::Tcp(s) => s.read(buf),
            #[cfg(unix)]
            Stream::Unix(s) => s.read(buf),
        }
    }
}

impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Stream::Tcp(s) => s.write(buf),
            #[cfg(unix)]
            Stream::Unix(s) => s.write(buf),
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        match self {
            Stream::Tcp(s) => s.flush(),
            #[cfg(unix)]
            Stream::Unix(s) => s.flush(),
        }
    }
}

// ---- grouping ---------------------------------------------------------------------------------

struct Group {
    function: String,
    key: String,
    n: usize,
    values: Vec<Value>,
    positions: Vec<usize>,
    index: Option<(u32, u32)>,
    width: usize,
}

fn group_rows(request: &Request) -> Vec<Group> {
    let mut order: Vec<String> = Vec::new();
    let mut groups: HashMap<String, Group> = HashMap::new();
    for (position, row) in request.rows.iter().enumerate() {
        let key = row.iter().map(Arg::token).collect::<Vec<_>>().join(",");
        let group = groups.entry(key.clone()).or_insert_with(|| {
            order.push(key.clone());
            Group {
                function: request.function.clone(),
                key: key.clone(),
                n: 0,
                values: Vec::new(),
                positions: Vec::new(),
                index: request.result_index,
                width: row.iter().map(Arg::width).sum(),
            }
        });
        group.n += 1;
        for arg in row {
            arg.push_values(&mut group.values);
        }
        group.positions.push(position);
    }
    order
        .into_iter()
        .map(|k| groups.remove(&k).unwrap())
        .collect()
}

fn group_json(group: &Group, start: usize, n: usize) -> Value {
    let mut g = json!({
        "f": group.function,
        "k": group.key,
        "n": n,
        "v": group.values[start * group.width..(start + n) * group.width].to_vec(),
    });
    if let Some((r, c)) = group.index {
        g["x"] = json!([r, c]);
    }
    let positions = &group.positions[start..start + n];
    if positions.windows(2).all(|w| w[1] == w[0] + 1) {
        g["o"] = json!(positions[0]);
    } else {
        g["i"] = json!(positions);
    }
    g
}

/// Cuts a request into submit frames under `FRAME_BUDGET` bytes.
fn submit_frames(id: &str, request: &Request) -> Vec<Value> {
    let groups = group_rows(request);
    let mut frames: Vec<Vec<Value>> = Vec::new();
    let mut current: Vec<Value> = Vec::new();
    let mut size = 0usize;
    for group in &groups {
        let per_call = (group.width * 20).max(8);
        let initial_step = (FRAME_BUDGET / per_call).max(1);
        let mut pending: std::collections::VecDeque<(usize, usize)> =
            std::collections::VecDeque::new();
        let mut start = 0;
        while start < group.n {
            let n = initial_step.min(group.n - start);
            pending.push_back((start, n));
            start += n;
        }
        while let Some((off, n)) = pending.pop_front() {
            let part = group_json(group, off, n);
            let encoded = serde_json::to_vec(&part).map_or(0, |v| v.len());
            if encoded > FRAME_BUDGET && n > 1 {
                let half = (n / 2).max(1);
                pending.push_front((off + half, n - half));
                pending.push_front((off, half));
                continue;
            }
            if !current.is_empty() && size + encoded > FRAME_BUDGET {
                frames.push(std::mem::take(&mut current));
                size = 0;
            }
            current.push(part);
            size += encoded;
            if size >= FRAME_BUDGET {
                frames.push(std::mem::take(&mut current));
                size = 0;
            }
        }
    }
    if !current.is_empty() || frames.is_empty() {
        frames.push(current);
    }
    let count = frames.len();
    frames
        .into_iter()
        .enumerate()
        .map(|(seq, groups)| {
            let mut frame =
                json!({"id": id, "seq": seq, "last": seq + 1 == count, "groups": groups});
            if seq == 0 {
                let mut opts = json!({"echo": request.echo_inputs, "order": "any", "calls": request.rows.len()});
                match request.priority {
                    Some(Priority::Interactive) => opts["priority"] = json!("interactive"),
                    Some(Priority::Bulk) => opts["priority"] = json!("bulk"),
                    None => {}
                }
                frame["opts"] = opts;
            }
            frame
        })
        .collect()
}

fn answer_from(letter: char, value: &Value) -> Answer {
    match letter {
        'n' => Answer::Number(u64::from_str_radix(value.as_str().unwrap_or("0"), 16).unwrap_or(0)),
        't' => Answer::Text(value.as_str().unwrap_or("").to_string()),
        'b' => Answer::Boolean(value.as_bool().unwrap_or(false)),
        'e' => Answer::Error(value.as_str().unwrap_or("").to_string()),
        'z' => Answer::Blank,
        'm' => {
            let rows = value["r"].as_u64().unwrap_or(0) as usize;
            let cols = value["c"].as_u64().unwrap_or(0) as usize;
            let ty: Vec<char> = value["ty"].as_str().unwrap_or("").chars().collect();
            let vals = value["v"].as_array().cloned().unwrap_or_default();
            let cells = ty
                .into_iter()
                .zip(vals.iter())
                .map(|(l, v)| answer_from(l, v))
                .collect();
            Answer::Array { rows, cols, cells }
        }
        'x' => Answer::Crashed {
            detail: value
                .get("detail")
                .and_then(Value::as_str)
                .unwrap_or("excel-crashed")
                .to_string(),
        },
        _ => Answer::Unread {
            reason: value
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or("unread")
                .to_string(),
        },
    }
}

fn client_lane() -> Option<String> {
    if let Ok(lane) = std::env::var("DNACALC_LANE")
        && !lane.is_empty()
    {
        return Some(lane);
    }
    let log = std::env::var("DNACALC_AGENT_LOG").ok()?;
    let (_, path) = log.split_once(':')?;
    let normalized = path.replace('\\', "/");
    let parts: Vec<&str> = normalized.split('/').collect();
    if parts.len() >= 3 && parts[parts.len() - 3] == "codex-runs" {
        Some(parts[parts.len() - 2].to_string())
    } else {
        None
    }
}

// ---- connection -------------------------------------------------------------------------------

struct Slot {
    frames: Arc<[Vec<u8>]>,
    answers: Vec<Option<Answer>>,
    inputs: Option<Vec<Option<Vec<Answer>>>>,
    env_id: Option<String>,
    env_by_row: Vec<Option<String>>,
    worker_calls: BTreeMap<String, usize>,
    sender: Sender<Result<Reply, Error>>,
    started: std::time::Instant,
    echo: bool,
}

struct Shared {
    // None while reconnecting; Err once the client is closed.
    writer: Mutex<Result<Option<Stream>, String>>,
    slots: Mutex<HashMap<String, Slot>>,
    envs: Mutex<HashMap<String, Value>>,
}

pub struct Client {
    shared: Arc<Shared>,
    next: Mutex<u64>,
}

enum Connector {
    Endpoint(PathBuf),
    #[cfg(unix)]
    Unix(PathBuf),
}

impl Connector {
    fn open(&self, name: &str, shared: &Shared) -> Result<Stream, Error> {
        let (mut stream, token) = match self {
            #[cfg(unix)]
            Self::Unix(path) => (Stream::Unix(UnixStream::connect(path)?), None),
            Self::Endpoint(path) => {
                let text = std::fs::read_to_string(path)?;
                let info: Value =
                    serde_json::from_str(&text).map_err(|e| Error::Protocol(e.to_string()))?;
                let host = info["host"].as_str().unwrap_or("127.0.0.1");
                let port = info["port"]
                    .as_u64()
                    .and_then(|port| u16::try_from(port).ok())
                    .ok_or_else(|| Error::Protocol("endpoint has no valid port".into()))?;
                let stream = TcpStream::connect((host, port))?;
                stream.set_nodelay(true)?;
                (
                    Stream::Tcp(stream),
                    info["token"].as_str().map(str::to_string),
                )
            }
        };
        stream.set_timeouts(Some(SOCKET_TIMEOUT), Some(SOCKET_TIMEOUT))?;
        let mut hello = json!({"v": VERSION, "name": name, "token": token});
        if let Some(lane) = client_lane() {
            hello["lane"] = json!(lane);
        }
        stream.write_all(&pack(b'H', &hello))?;
        loop {
            let mut header = [0; 4];
            stream.read_exact(&mut header)?;
            let size = u32::from_be_bytes(header) as usize;
            if !(1..=MAX_FRAME).contains(&size) {
                return Err(Error::Protocol(format!("frame of {size} bytes")));
            }
            let mut frame = header.to_vec();
            frame.resize(4 + size, 0);
            stream.read_exact(&mut frame[4..])?;
            let (kind, body) = take_frames(&mut frame)?.pop().expect("one complete frame");
            if kind == b'H' && body["v"] == VERSION {
                if let Some(envs) = body["envs"].as_object() {
                    shared.envs.lock().unwrap().extend(envs.clone());
                }
                stream.set_timeouts(None, Some(SOCKET_TIMEOUT))?;
                return Ok(stream);
            }
            if kind != b'E' {
                return Err(Error::Protocol("expected hello from the service".into()));
            }
            route(shared, kind, body);
        }
    }
}

impl Shared {
    fn close(&self, reason: String) {
        let mut writer = self.writer.lock().unwrap();
        if let Ok(Some(stream)) = std::mem::replace(&mut *writer, Err(reason.clone())) {
            stream.shutdown();
        }
        for (_, slot) in self.slots.lock().unwrap().drain() {
            let _ = slot.sender.send(Err(Error::Connection(reason.clone())));
        }
    }

    fn replay(&self, mut stream: Stream) {
        let mut writer = self.writer.lock().unwrap();
        let Ok(connection) = &mut *writer else {
            stream.shutdown();
            return;
        };
        let pending: Vec<_> = self
            .slots
            .lock()
            .unwrap()
            .values_mut()
            .map(|slot| {
                // The new stream supplies the entire reply, from cache or a restarted broker.
                slot.answers.fill(None);
                if let Some(inputs) = &mut slot.inputs {
                    inputs.fill(None);
                }
                slot.env_id = None;
                slot.env_by_row.fill(None);
                slot.worker_calls.clear();
                Arc::clone(&slot.frames)
            })
            .collect();
        if write_pieces(&mut stream, pending.iter().flat_map(|pieces| pieces.iter())).is_ok() {
            *connection = Some(stream);
        }
    }

    fn reconnect(&self, connector: &Connector, name: &str) -> Result<Stream, String> {
        let started = Instant::now();
        let mut delay = Duration::from_millis(50);
        loop {
            thread::sleep(delay);
            if self.writer.lock().unwrap().is_err() {
                return Err("the client was dropped".into());
            }
            match connector.open(name, self) {
                Ok(stream) => return Ok(stream),
                Err(error) if started.elapsed() >= RECONNECT_WINDOW => {
                    return Err(error.to_string());
                }
                Err(_) => delay = (delay * 2).min(Duration::from_secs(2)),
            }
        }
    }

    fn read(&self, stream: &mut Stream) -> Result<(), Error> {
        let mut buffer = Vec::new();
        let mut chunk = vec![0; 1 << 16];
        loop {
            let n = stream.read(&mut chunk)?;
            if n == 0 {
                return Ok(());
            }
            buffer.extend_from_slice(&chunk[..n]);
            for (kind, body) in take_frames(&mut buffer)? {
                route(self, kind, body);
            }
        }
    }
}

fn write_pieces<'a>(
    stream: &mut Stream,
    pieces: impl IntoIterator<Item = &'a Vec<u8>>,
) -> io::Result<()> {
    for piece in pieces {
        if let Err(error) = stream.write_all(piece) {
            stream.shutdown();
            return Err(error);
        }
    }
    Ok(())
}

impl Drop for Client {
    fn drop(&mut self) {
        self.shared.close("the client was dropped".into());
    }
}

/// A request in flight.
pub struct Handle {
    receiver: Receiver<Result<Reply, Error>>,
}

impl Handle {
    /// Waits for the whole answer.
    pub fn wait(self) -> Result<Reply, Error> {
        self.receiver
            .recv()
            .map_err(|_| Error::Connection("the client was dropped".into()))?
    }
    pub fn wait_timeout(self, timeout: Duration) -> Result<Reply, Error> {
        match self.receiver.recv_timeout(timeout) {
            Ok(result) => result,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Err(Error::Timeout),
            Err(_) => Err(Error::Connection("the client was dropped".into())),
        }
    }
}

impl Client {
    /// The relay's unix socket when present, else the Windows broker's local endpoint.
    pub fn connect() -> Result<Client, Error> {
        #[cfg(unix)]
        {
            let path = match std::env::var_os("TOUCHSTONE_SOCKET") {
                Some(p) => PathBuf::from(p),
                None => PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
                    .join(".excel-service")
                    .join("excel.sock"),
            };
            if path.exists() {
                return Client::connect_unix(&path);
            }
        }
        let endpoint = std::env::var_os("TOUCHSTONE_ENDPOINT")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(r"C:\Work\DnaCalc\codex-runs\excel-service-v2\endpoint.json")
            });
        Client::connect_endpoint(&endpoint)
    }

    #[cfg(unix)]
    pub fn connect_unix(path: &Path) -> Result<Client, Error> {
        Client::start(Connector::Unix(std::path::absolute(path)?))
    }

    /// The broker's local endpoint file: `{"host", "port", "token"}`.
    pub fn connect_endpoint(path: &Path) -> Result<Client, Error> {
        Client::start(Connector::Endpoint(std::path::absolute(path)?))
    }

    fn start(connector: Connector) -> Result<Client, Error> {
        let shared = Arc::new(Shared {
            writer: Mutex::new(Ok(None)),
            slots: Mutex::new(HashMap::new()),
            envs: Mutex::new(HashMap::new()),
        });
        let name = format!("rust-{}", std::process::id());
        let mut reader = connector.open(&name, &shared)?;
        *shared.writer.lock().unwrap() = Ok(Some(reader.try_clone()?));
        let reader_shared = Arc::clone(&shared);
        thread::spawn(move || {
            let mut result = reader_shared.read(&mut reader);
            loop {
                if let Err(Error::Protocol(error)) = result {
                    reader_shared.close(error);
                    break;
                }
                {
                    let mut writer = reader_shared.writer.lock().unwrap();
                    let Ok(connection) = &mut *writer else {
                        break;
                    };
                    if let Some(stream) = connection.take() {
                        stream.shutdown();
                    }
                }
                let stream = reader_shared
                    .reconnect(&connector, &name)
                    .and_then(|stream| {
                        reader = stream.try_clone().map_err(|error| error.to_string())?;
                        Ok(stream)
                    });
                let stream = match stream {
                    Ok(stream) => stream,
                    Err(reason) => {
                        reader_shared.close(reason);
                        break;
                    }
                };
                // A large replay can fill both socket buffers. Read replies while sending it.
                result = thread::scope(|scope| {
                    scope.spawn(|| reader_shared.replay(stream));
                    let result = reader_shared.read(&mut reader);
                    reader.shutdown();
                    result
                });
            }
        });
        Ok(Client {
            shared,
            next: Mutex::new(0),
        })
    }

    /// Sends a request and returns at once.
    pub fn submit(&self, request: Request) -> Result<Handle, Error> {
        let id = {
            let mut next = self.next.lock().unwrap();
            *next += 1;
            format!(
                "{:016x}{:08x}{:08x}",
                nanos() as u64,
                std::process::id(),
                *next as u32
            )
        };
        let frames: Arc<[Vec<u8>]> = submit_frames(&id, &request)
            .iter()
            .map(|frame| pack(b'S', frame))
            .collect();
        let (sender, receiver) = channel();
        let calls = request.rows.len();
        let mut writer = self.shared.writer.lock().unwrap();
        let connection = writer
            .as_mut()
            .map_err(|reason| Error::Connection(reason.clone()))?;
        self.shared.slots.lock().unwrap().insert(
            id,
            Slot {
                frames: Arc::clone(&frames),
                answers: vec![None; calls],
                inputs: if request.echo_inputs {
                    Some(vec![None; calls])
                } else {
                    None
                },
                env_id: None,
                env_by_row: vec![None; calls],
                worker_calls: BTreeMap::new(),
                sender,
                started: Instant::now(),
                echo: request.echo_inputs,
            },
        );
        if let Some(stream) = connection
            && write_pieces(stream, frames.iter()).is_err()
        {
            *connection = None;
        }
        Ok(Handle { receiver })
    }

    /// One question, blocking until it is answered.
    pub fn ask(&self, function: &str, rows: Vec<Vec<Arg>>) -> Result<Reply, Error> {
        self.submit(Request::new(function, rows))?.wait()
    }

    /// Many questions at once; the replies come back in request order.
    pub fn ask_many(&self, requests: Vec<Request>) -> Vec<Result<Reply, Error>> {
        let handles: Vec<Result<Handle, Error>> =
            requests.into_iter().map(|r| self.submit(r)).collect();
        handles
            .into_iter()
            .map(|h| h.and_then(Handle::wait))
            .collect()
    }

    /// The environment entry (Excel version, build, CPU, calculation settings) for an `env_id`.
    pub fn env(&self, env_id: &str) -> Option<Value> {
        self.shared.envs.lock().unwrap().get(env_id).cloned()
    }
}

fn nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

fn route(shared: &Shared, kind: u8, body: Value) {
    match kind {
        b'E' => {
            if let Some(id) = body["env_id"].as_str() {
                shared
                    .envs
                    .lock()
                    .unwrap()
                    .insert(id.to_string(), body["env"].clone());
            }
        }
        b'A' => {
            let id = body["id"].as_str().unwrap_or("").to_string();
            let mut slots = shared.slots.lock().unwrap();
            let Some(slot) = slots.get_mut(&id) else {
                return;
            };
            let frame_env = body["env"].as_str().map(str::to_string);
            if let Some(env) = &frame_env {
                slot.env_id = Some(env.clone());
            }
            let worker_name = body["w"].as_str().map(str::to_string);
            for g in body["g"].as_array().into_iter().flatten() {
                let ty: Vec<char> = g["ty"].as_str().unwrap_or("").chars().collect();
                let values = g["v"].as_array().cloned().unwrap_or_default();
                let positions: Vec<usize> = match (g["o"].as_u64(), g["i"].as_array()) {
                    (Some(o), _) => (o as usize..o as usize + ty.len()).collect(),
                    (None, Some(list)) => list
                        .iter()
                        .filter_map(|p| p.as_u64().map(|p| p as usize))
                        .collect(),
                    _ => Vec::new(),
                };
                if let Some(worker) = &worker_name {
                    *slot.worker_calls.entry(worker.clone()).or_insert(0) += positions.len();
                }
                for (k, position) in positions.iter().enumerate() {
                    if let (Some(&letter), Some(value)) = (ty.get(k), values.get(k)) {
                        if *position < slot.answers.len() {
                            slot.answers[*position] = Some(answer_from(letter, value));
                            if let Some(env) = &frame_env {
                                slot.env_by_row[*position] = Some(env.clone());
                            }
                        }
                    }
                }
                if slot.echo {
                    if let (Some(ity), Some(iv)) = (g["ity"].as_str(), g["iv"].as_array()) {
                        let letters: Vec<char> = ity.chars().collect();
                        let width = if ty.is_empty() {
                            0
                        } else {
                            letters.len() / ty.len()
                        };
                        if let Some(inputs) = slot.inputs.as_mut() {
                            for (k, position) in positions.iter().enumerate() {
                                let cells = (0..width)
                                    .filter_map(|c| {
                                        Some(answer_from(
                                            *letters.get(k * width + c)?,
                                            iv.get(k * width + c)?,
                                        ))
                                    })
                                    .collect();
                                if *position < inputs.len() {
                                    inputs[*position] = Some(cells);
                                }
                            }
                        }
                    }
                }
            }
        }
        b'D' => {
            let id = body["id"].as_str().unwrap_or("").to_string();
            let Some(slot) = shared.slots.lock().unwrap().remove(&id) else {
                return;
            };
            let seconds = slot.started.elapsed().as_secs_f64();
            let known_envs = shared.envs.lock().unwrap();
            let env = slot
                .env_id
                .as_ref()
                .and_then(|e| known_envs.get(e).cloned());
            let mut envs = BTreeMap::new();
            for row_env in slot.env_by_row.iter().flatten() {
                if !envs.contains_key(row_env)
                    && let Some(entry) = known_envs.get(row_env)
                {
                    envs.insert(row_env.clone(), entry.clone());
                }
            }
            if let Some(id) = &slot.env_id
                && !envs.contains_key(id)
                && let Some(entry) = known_envs.get(id)
            {
                envs.insert(id.clone(), entry.clone());
            }
            drop(known_envs);
            let missing = slot.answers.iter().any(Option::is_none);
            let result = if missing {
                Err(Error::Protocol(
                    "the service finished without answering every call".into(),
                ))
            } else {
                Ok(Reply {
                    rows: slot.answers.into_iter().map(Option::unwrap).collect(),
                    inputs: slot
                        .inputs
                        .map(|v| v.into_iter().map(|c| c.unwrap_or_default()).collect()),
                    env_id: slot.env_id,
                    env,
                    env_by_row: slot.env_by_row,
                    envs,
                    worker_calls: slot.worker_calls,
                    seconds,
                })
            };
            let _ = slot.sender.send(result);
        }
        b'X' => {
            let id = body["id"].as_str().unwrap_or("").to_string();
            let Some(slot) = shared.slots.lock().unwrap().remove(&id) else {
                return;
            };
            let _ = slot.sender.send(Err(Error::Service {
                code: body["code"].as_str().unwrap_or("error").to_string(),
                detail: body["detail"].as_str().unwrap_or("").to_string(),
            }));
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_group_by_shape_and_keep_positions() {
        let request = Request::new(
            "F",
            vec![
                vec![Arg::number_bits(1)],
                vec![Arg::Text("a".into())],
                vec![Arg::number_bits(2)],
                vec![Arg::TextLiteral("b".into())],
            ],
        );
        let groups = group_rows(&request);
        let keys: Vec<&str> = groups.iter().map(|g| g.key.as_str()).collect();
        assert_eq!(keys, ["n", "s", "S"]);
        assert_eq!(groups[0].positions, [0, 2]);
        let json = group_json(&groups[0], 0, 2);
        assert_eq!(json["i"], json!([0, 2]));
        assert_eq!(json["v"], json!(["0000000000000001", "0000000000000002"]));
        assert_eq!(group_json(&groups[1], 0, 1)["o"], json!(1));
    }

    #[test]
    fn a_big_request_is_cut_into_small_frames_and_the_last_is_flagged() {
        let rows = vec![vec![Arg::number(1.0)]; 20000];
        let frames = submit_frames(&"a".repeat(32), &Request::new("ABS", rows));
        assert!(frames.len() > 3);
        assert!(
            frames
                .iter()
                .all(|f| serde_json::to_vec(f).unwrap().len() < 64 * 1024)
        );
        assert_eq!(frames.last().unwrap()["last"], json!(true));
        assert_eq!(frames[0]["opts"]["calls"], json!(20000));
        let calls: u64 = frames
            .iter()
            .flat_map(|f| f["groups"].as_array().unwrap())
            .map(|g| g["n"].as_u64().unwrap())
            .sum();
        assert_eq!(calls, 20000);
    }

    #[test]
    fn arrays_flatten_inline_with_their_shape_in_the_key() {
        let a = Arg::Array {
            rows: 2,
            cols: 2,
            bits: vec![1, 2, 3, 4],
        };
        let mut request = Request::new("LINEST", vec![vec![a.clone(), Arg::Boolean(true)]]);
        request.result_index = Some((4, 2));
        let groups = group_rows(&request);
        assert_eq!(groups[0].key, "a2x2,b");
        assert_eq!(groups[0].values.len(), 5);
        assert_eq!(group_json(&groups[0], 0, 1)["x"], json!([4, 2]));
    }

    #[test]
    fn frames_survive_arbitrary_chunking() {
        let mut data = pack(b'A', &json!({"id": "x", "g": []}));
        data.extend(pack(b'D', &json!({"id": "x"})));
        let mut buffer = Vec::new();
        let mut seen = Vec::new();
        for byte in data {
            buffer.push(byte);
            seen.extend(
                take_frames(&mut buffer)
                    .unwrap()
                    .into_iter()
                    .map(|(k, _)| k),
            );
        }
        assert_eq!(seen, [b'A', b'D']);
    }

    #[test]
    fn matrix_and_crashed_answers_decode_cleanly() {
        let matrix = answer_from(
            'm',
            &json!({"r": 2, "c": 1, "ty": "nn", "v": ["c059000000000000", "404e000000000000"]}),
        );
        assert_eq!(
            matrix,
            Answer::Array {
                rows: 2,
                cols: 1,
                cells: vec![
                    Answer::Number(0xc059000000000000),
                    Answer::Number(0x404e000000000000),
                ],
            }
        );
        let crashed = answer_from('x', &json!({"detail": "worker exited"}));
        assert_eq!(
            crashed,
            Answer::Crashed {
                detail: "worker exited".into()
            }
        );
    }
}
