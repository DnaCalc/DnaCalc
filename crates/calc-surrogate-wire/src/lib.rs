//! The add-in surrogate's message envelope (docs/architecture/ADDIN_SURROGATE.md section 4).
//!
//! A surrogate is a second process that runs a session and every add-in it loads; the process the
//! person started (the *controller*) keeps the window and the page. Every message between the two
//! is one [`Frame`]: a fixed [`Header`] (the envelope) and the payload bytes of the application's
//! existing seam message. This crate moves envelopes and bytes and never decodes a payload: the
//! application decodes it after the checks here have passed.
//!
//! The envelope carries
//! - `incarnation`: 128 random bits chosen when the surrogate starts, and the controller's start
//!   counter ([`Incarnation`]); never a process ID, which a restarted surrogate can share;
//! - `request`: for a request its ID, for a reply the ID of the request it answers;
//! - `parent`: for a request the surrogate makes while a controller request is open (a file picker
//!   during an open), the ID of that open request;
//! - `sequence`: per direction, strictly increasing from 1; a gap or a repeat ends the connection;
//! - `kind`: the payload type ([`Kind`]), checked against what the request expects before the
//!   payload is read.
//!
//! Decoding checks, in order, the header (magic, version, flow), the size against the limit
//! (before any payload byte is buffered), the incarnation (a frame from another incarnation is
//! refused, changes no state and does not count in the sequence), the sequence, and then the kind
//! and the request or parent the frame names. [`ControllerLink`] and [`SurrogateLink`] are the two
//! ends; both are byte-in, byte-out state machines, so the platform's pipes and threads (in
//! `calc-platform-desktop`) and the tests drive the same code.
//!
//! What crosses is owned data only: the header is numbers, the payload is bytes. There is no field
//! for a pointer, a handle or an HWND, and a payload is the serialised seam message, never memory.
//! Validation cannot protect against memory corruption inside a sending surrogate: containment of a
//! corrupt surrogate is ending it (section 6).

use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// The first four bytes of every frame.
pub const MAGIC: [u8; 4] = *b"DNSG";
/// The envelope's version.
pub const VERSION: u16 = 1;
/// The bytes of a header.
pub const HEADER_LEN: usize = 60;

/// What an invocation whose result was not accepted reports when the surrogate stopped (section 6),
/// exactly.
pub const UNKNOWN_OUTCOME: &str = "The add-in process stopped before this result was accepted. External work may have completed. Retrying will run the add-in again.";

/// One run of a surrogate: random bits chosen when it starts and the controller's start counter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Incarnation {
    pub nonce: [u8; 16],
    pub start: u32,
}

impl Incarnation {
    pub fn new(nonce: [u8; 16], start: u32) -> Self {
        Incarnation { nonce, start }
    }

    /// From the random bits as one number (most significant byte first).
    pub fn from_parts(bits: u128, start: u32) -> Self {
        Incarnation {
            nonce: bits.to_be_bytes(),
            start,
        }
    }

    /// As text, for a command line (`--surrogate <hex>.<start>`).
    pub fn to_text(&self) -> String {
        let mut text = String::with_capacity(32 + 11);
        for byte in self.nonce {
            text.push_str(&format!("{byte:02x}"));
        }
        text.push('.');
        text.push_str(&self.start.to_string());
        text
    }

    /// From [`Incarnation::to_text`].
    pub fn parse(text: &str) -> Option<Self> {
        let (hex, start) = text.split_once('.')?;
        if hex.len() != 32 || !hex.is_ascii() {
            return None;
        }
        let bits = u128::from_str_radix(hex, 16).ok()?;
        Some(Self::from_parts(bits, start.parse().ok()?))
    }
}

/// What a frame is: a request (from either end), the reply to one, or an event (a message an end
/// sends on its own, answered by nothing).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flow {
    Request,
    Reply,
    Event,
}

impl Flow {
    fn code(self) -> u8 {
        match self {
            Flow::Request => 1,
            Flow::Reply => 2,
            Flow::Event => 3,
        }
    }

    fn from_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(Flow::Request),
            2 => Some(Flow::Reply),
            3 => Some(Flow::Event),
            _ => None,
        }
    }
}

/// A payload type. The application numbers its own below [`Kind::RESERVED`]; the kinds from
/// there up belong to the envelope.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Kind(pub u16);

impl Kind {
    /// The first kind the envelope keeps for itself.
    pub const RESERVED: u16 = 0xff00;
    /// The surrogate's first frame: it is up, under the incarnation it was started with.
    pub const HELLO: Kind = Kind(0xff01);
    /// The controller's answer to a surrogate request it will not serve (its parent is not the
    /// request being waited on): the surrogate's code gets a refusal instead of waiting forever.
    pub const DECLINED: Kind = Kind(0xff02);

    pub fn reserved(self) -> bool {
        self.0 >= Self::RESERVED
    }
}

/// The envelope of one frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    pub incarnation: Incarnation,
    pub flow: Flow,
    pub kind: Kind,
    pub sequence: u64,
    /// For a request its ID; for a reply the request it answers; 0 for an event.
    pub request: u64,
    /// For a request made while another is open, that request's ID; otherwise 0.
    pub parent: u64,
    /// The payload's bytes.
    pub length: u32,
}

/// A whole frame: its envelope and its payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub header: Header,
    pub payload: Vec<u8>,
}

/// The bytes of a frame: the header as given, then the payload. [`Outbound::frame`] sets
/// `header.length` to the payload's; a test can write any header.
pub fn encode(header: &Header, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_LEN + payload.len());
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.push(header.flow.code());
    out.push(0);
    out.extend_from_slice(&header.kind.0.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&header.incarnation.nonce);
    out.extend_from_slice(&header.incarnation.start.to_le_bytes());
    out.extend_from_slice(&header.sequence.to_le_bytes());
    out.extend_from_slice(&header.request.to_le_bytes());
    out.extend_from_slice(&header.parent.to_le_bytes());
    out.extend_from_slice(&header.length.to_le_bytes());
    debug_assert_eq!(out.len(), HEADER_LEN);
    out.extend_from_slice(payload);
    out
}

/// Why a connection ends. After one, the end that saw it takes nothing more: the supervisor ends
/// the surrogate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WireError {
    /// The first bytes are not [`MAGIC`].
    Magic,
    /// An envelope version this build does not read.
    Version(u16),
    /// A flow the envelope does not define.
    Flow(u8),
    /// A reserved header field is not zero.
    Reserved,
    /// A payload longer than the limit (seen in the header, before the payload).
    TooLarge { length: usize, limit: usize },
    /// A frame out of sequence: a gap or a repeat.
    Sequence { expected: u64, got: u64 },
    /// A reply whose kind is not the one its request expects.
    Kind { request: u64, expected: Kind, got: Kind },
    /// A frame before the surrogate's hello, or a second hello.
    BeforeHello,
    /// A frame whose flow or kind this end never takes (a reserved kind, a reply to nothing the
    /// protocol allows).
    Unexpected { flow: Flow, kind: Kind },
    /// The link failed earlier, or the surrogate stopped.
    Closed,
}

impl std::fmt::Display for WireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WireError::Magic => write!(f, "the add-in process sent bytes that are no frame"),
            WireError::Version(v) => write!(f, "the add-in process sent envelope version {v}"),
            WireError::Flow(code) => write!(f, "the add-in process sent flow {code}"),
            WireError::Reserved => write!(f, "the add-in process set a reserved envelope field"),
            WireError::TooLarge { length, limit } => write!(
                f,
                "the add-in process sent a message of {length} bytes (the limit is {limit})"
            ),
            WireError::Sequence { expected, got } => write!(
                f,
                "the add-in process sent message {got} where {expected} was due"
            ),
            WireError::Kind {
                request,
                expected,
                got,
            } => write!(
                f,
                "the add-in process answered request {request} with kind {} instead of {}",
                got.0, expected.0
            ),
            WireError::BeforeHello => write!(f, "the add-in process did not start properly"),
            WireError::Unexpected { flow, kind } => write!(
                f,
                "the add-in process sent an unexpected {flow:?} of kind {}",
                kind.0
            ),
            WireError::Closed => write!(f, "the add-in process is not running"),
        }
    }
}

impl std::error::Error for WireError {}

/// A frame dropped without ending the connection: it changes no state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// From another incarnation (a surrogate that was replaced, or one never started).
    Incarnation { got: Incarnation },
    /// A reply to a request the controller stopped waiting for: a late result, not admitted.
    Late { request: u64 },
    /// A reply to a request that is not open.
    NotOpen { request: u64 },
    /// A surrogate request whose parent is not the request the controller is waiting on.
    Parent { request: u64, parent: u64 },
}

/// Why an end will not send what it was asked to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SendError {
    /// The link failed, or has not started (no hello yet).
    NotReady,
    /// A reserved kind used as an application's.
    ReservedKind(Kind),
    /// A reply or callback answer to a request that is not open on this end.
    NotOpen(u64),
    /// A surrogate request with no controller request open to be its parent.
    NoParent,
}

impl std::fmt::Display for SendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SendError::NotReady => write!(f, "the add-in process is not running"),
            SendError::ReservedKind(kind) => write!(f, "kind {} is the envelope's", kind.0),
            SendError::NotOpen(request) => write!(f, "request {request} is not open"),
            SendError::NoParent => write!(f, "no request is open to ask under"),
        }
    }
}

impl std::error::Error for SendError {}

/// Splits bytes into frames, checking each header before its payload is buffered.
#[derive(Debug)]
pub struct Decoder {
    buffer: Vec<u8>,
    limit: usize,
    failed: Option<WireError>,
}

impl Decoder {
    /// A decoder that refuses payloads over `limit` bytes.
    pub fn new(limit: usize) -> Self {
        Decoder {
            buffer: Vec::new(),
            limit,
            failed: None,
        }
    }

    pub fn push(&mut self, bytes: &[u8]) {
        if self.failed.is_none() {
            self.buffer.extend_from_slice(bytes);
        }
    }

    /// The next whole frame, `None` while one is incomplete.
    pub fn next_frame(&mut self) -> Result<Option<Frame>, WireError> {
        if let Some(error) = &self.failed {
            return Err(error.clone());
        }
        match self.take() {
            Err(error) => {
                self.failed = Some(error.clone());
                self.buffer.clear();
                Err(error)
            }
            ok => ok,
        }
    }

    fn take(&mut self) -> Result<Option<Frame>, WireError> {
        let b = &self.buffer;
        // The magic is checked as soon as its bytes are here.
        let have = b.len().min(4);
        if b[..have] != MAGIC[..have] {
            return Err(WireError::Magic);
        }
        if b.len() < HEADER_LEN {
            return Ok(None);
        }
        let u16_at = |at: usize| u16::from_le_bytes([b[at], b[at + 1]]);
        let u32_at = |at: usize| u32::from_le_bytes(b[at..at + 4].try_into().expect("4 bytes"));
        let u64_at = |at: usize| u64::from_le_bytes(b[at..at + 8].try_into().expect("8 bytes"));
        let version = u16_at(4);
        if version != VERSION {
            return Err(WireError::Version(version));
        }
        let flow = Flow::from_code(b[6]).ok_or(WireError::Flow(b[6]))?;
        if b[7] != 0 || u16_at(10) != 0 {
            return Err(WireError::Reserved);
        }
        let length = u32_at(56) as usize;
        if length > self.limit {
            return Err(WireError::TooLarge {
                length,
                limit: self.limit,
            });
        }
        if b.len() < HEADER_LEN + length {
            return Ok(None);
        }
        let header = Header {
            incarnation: Incarnation {
                nonce: b[12..28].try_into().expect("16 bytes"),
                start: u32_at(28),
            },
            flow,
            kind: Kind(u16_at(8)),
            sequence: u64_at(32),
            request: u64_at(40),
            parent: u64_at(48),
            length: length as u32,
        };
        let payload = b[HEADER_LEN..HEADER_LEN + length].to_vec();
        self.buffer.drain(..HEADER_LEN + length);
        Ok(Some(Frame { header, payload }))
    }
}

/// The sending half of one direction: stamps the incarnation and the next sequence number.
#[derive(Debug)]
pub struct Outbound {
    incarnation: Incarnation,
    next: u64,
}

impl Outbound {
    pub fn new(incarnation: Incarnation) -> Self {
        Outbound {
            incarnation,
            next: 1,
        }
    }

    /// The bytes of the next frame.
    pub fn frame(
        &mut self,
        flow: Flow,
        kind: Kind,
        request: u64,
        parent: u64,
        payload: &[u8],
    ) -> Vec<u8> {
        let header = Header {
            incarnation: self.incarnation,
            flow,
            kind,
            sequence: self.next,
            request,
            parent,
            length: u32::try_from(payload.len()).unwrap_or(u32::MAX),
        };
        self.next += 1;
        encode(&header, payload)
    }

    /// Continues from `sequence` (a test that forges one end's frames).
    pub fn skip_to(&mut self, sequence: u64) {
        self.next = sequence;
    }
}

/// The receiving half of one direction: frames of this incarnation, in sequence.
#[derive(Debug)]
struct Inbound {
    decoder: Decoder,
    incarnation: Incarnation,
    last: u64,
}

enum Checked {
    Frame(Frame),
    Refused(Refusal),
}

impl Inbound {
    fn new(incarnation: Incarnation, limit: usize) -> Self {
        Inbound {
            decoder: Decoder::new(limit),
            incarnation,
            last: 0,
        }
    }

    fn next(&mut self) -> Result<Option<Checked>, WireError> {
        let Some(frame) = self.decoder.next_frame()? else {
            return Ok(None);
        };
        // Another incarnation's frame is no part of this connection: refused, and uncounted.
        if frame.header.incarnation != self.incarnation {
            return Ok(Some(Checked::Refused(Refusal::Incarnation {
                got: frame.header.incarnation,
            })));
        }
        let expected = self.last + 1;
        if frame.header.sequence != expected {
            return Err(WireError::Sequence {
                expected,
                got: frame.header.sequence,
            });
        }
        self.last = expected;
        Ok(Some(Checked::Frame(frame)))
    }
}

/// What the controller takes from the surrogate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Incoming {
    /// The surrogate is up.
    Hello,
    /// The answer to an open request.
    Reply {
        request: u64,
        kind: Kind,
        payload: Vec<u8>,
    },
    /// A request for a controller service (a dialog, a file) made while `parent` is open: the
    /// controller answers it with [`ControllerLink::answer_callback`] while it waits.
    Callback {
        request: u64,
        parent: u64,
        kind: Kind,
        payload: Vec<u8>,
    },
    /// A message the surrogate sent on its own (a real-time value arrived).
    Event { kind: Kind, payload: Vec<u8> },
    /// A frame dropped, with why.
    Refused(Refusal),
}

#[derive(Debug, PartialEq, Eq)]
enum State {
    Starting,
    Ready,
    Failed,
}

/// The controller's end of a link to one surrogate incarnation.
#[derive(Debug)]
pub struct ControllerLink {
    out: Outbound,
    inbound: Inbound,
    state: State,
    next_request: u64,
    /// The requests waiting for a reply, with the kind each reply must have.
    open: BTreeMap<u64, Kind>,
    /// The requests the controller stopped waiting for: their results are not admitted.
    cancelled: BTreeSet<u64>,
    /// The surrogate's requests the controller has not answered yet.
    callbacks: BTreeSet<u64>,
    /// The surrogate's requests refused for their parent, not yet declined.
    refused: BTreeSet<u64>,
}

impl ControllerLink {
    /// The end for the surrogate started as `incarnation`, refusing payloads over `limit` bytes.
    pub fn new(incarnation: Incarnation, limit: usize) -> Self {
        ControllerLink {
            out: Outbound::new(incarnation),
            inbound: Inbound::new(incarnation, limit),
            state: State::Starting,
            next_request: 1,
            open: BTreeMap::new(),
            cancelled: BTreeSet::new(),
            callbacks: BTreeSet::new(),
            refused: BTreeSet::new(),
        }
    }

    pub fn incarnation(&self) -> Incarnation {
        self.out.incarnation
    }

    /// Whether the surrogate said hello and nothing has failed since.
    pub fn ready(&self) -> bool {
        self.state == State::Ready
    }

    /// The requests waiting for a reply.
    pub fn open(&self) -> Vec<u64> {
        self.open.keys().copied().collect()
    }

    /// A request: its ID and the bytes to write. `expects` is the kind its reply must have.
    pub fn request(
        &mut self,
        kind: Kind,
        expects: Kind,
        payload: Vec<u8>,
    ) -> Result<(u64, Vec<u8>), SendError> {
        if self.state != State::Ready {
            return Err(SendError::NotReady);
        }
        for k in [kind, expects] {
            if k.reserved() {
                return Err(SendError::ReservedKind(k));
            }
        }
        let request = self.next_request;
        self.next_request += 1;
        self.open.insert(request, expects);
        Ok((
            request,
            self.out.frame(Flow::Request, kind, request, 0, &payload),
        ))
    }

    /// The answer to the surrogate's request `request` ([`Incoming::Callback`]).
    pub fn answer_callback(
        &mut self,
        request: u64,
        kind: Kind,
        payload: Vec<u8>,
    ) -> Result<Vec<u8>, SendError> {
        if self.state != State::Ready {
            return Err(SendError::NotReady);
        }
        if kind.reserved() {
            return Err(SendError::ReservedKind(kind));
        }
        if !self.callbacks.remove(&request) {
            return Err(SendError::NotOpen(request));
        }
        Ok(self.out.frame(Flow::Reply, kind, request, 0, &payload))
    }

    /// The answer that declines the surrogate's request `request`: one refused for its parent
    /// ([`Refusal::Parent`]), or a callback the controller will not serve.
    pub fn decline_callback(&mut self, request: u64) -> Result<Vec<u8>, SendError> {
        if self.state != State::Ready {
            return Err(SendError::NotReady);
        }
        if !self.refused.remove(&request) && !self.callbacks.remove(&request) {
            return Err(SendError::NotOpen(request));
        }
        Ok(self.out.frame(Flow::Reply, Kind::DECLINED, request, 0, &[]))
    }

    /// An event for the surrogate.
    pub fn event(&mut self, kind: Kind, payload: Vec<u8>) -> Result<Vec<u8>, SendError> {
        if self.state != State::Ready {
            return Err(SendError::NotReady);
        }
        if kind.reserved() {
            return Err(SendError::ReservedKind(kind));
        }
        Ok(self.out.frame(Flow::Event, kind, 0, 0, &payload))
    }

    /// The controller stops waiting for `request` (its time ran out). The native code goes on; a
    /// result that arrives later is refused as [`Refusal::Late`], and a callback under it as
    /// [`Refusal::Parent`].
    pub fn cancel(&mut self, request: u64) {
        if self.open.remove(&request).is_some() {
            self.cancelled.insert(request);
        }
    }

    /// The surrogate stopped (it ended, or the supervisor ended it): the link takes nothing more,
    /// and every open request is returned once, for the unknown-outcome message
    /// ([`UNKNOWN_OUTCOME`]). Nothing is sent again.
    pub fn stopped(&mut self) -> Vec<u64> {
        self.state = State::Failed;
        self.cancelled.clear();
        self.callbacks.clear();
        self.refused.clear();
        std::mem::take(&mut self.open).into_keys().collect()
    }

    /// Bytes read from the surrogate.
    pub fn receive(&mut self, bytes: &[u8]) {
        self.inbound.decoder.push(bytes);
    }

    /// The next message from the surrogate, `None` when the bytes so far hold no whole frame. An
    /// error ends the link: the caller ends the surrogate.
    pub fn next(&mut self) -> Result<Option<Incoming>, WireError> {
        if self.state == State::Failed {
            return Err(WireError::Closed);
        }
        let result = self.take();
        if result.is_err() {
            self.state = State::Failed;
        }
        result
    }

    fn take(&mut self) -> Result<Option<Incoming>, WireError> {
        let frame = match self.inbound.next()? {
            None => return Ok(None),
            Some(Checked::Refused(refusal)) => return Ok(Some(Incoming::Refused(refusal))),
            Some(Checked::Frame(frame)) => frame,
        };
        let Header {
            flow,
            kind,
            request,
            parent,
            ..
        } = frame.header;
        let hello = flow == Flow::Event && kind == Kind::HELLO;
        match (&self.state, hello) {
            (State::Starting, true) => {
                self.state = State::Ready;
                return Ok(Some(Incoming::Hello));
            }
            (State::Starting, false) | (_, true) => return Err(WireError::BeforeHello),
            _ => {}
        }
        if kind.reserved() {
            return Err(WireError::Unexpected { flow, kind });
        }
        match flow {
            Flow::Reply => {
                if let Some(expected) = self.open.get(&request).copied() {
                    if expected != kind {
                        return Err(WireError::Kind {
                            request,
                            expected,
                            got: kind,
                        });
                    }
                    self.open.remove(&request);
                    Ok(Some(Incoming::Reply {
                        request,
                        kind,
                        payload: frame.payload,
                    }))
                } else if self.cancelled.remove(&request) {
                    Ok(Some(Incoming::Refused(Refusal::Late { request })))
                } else {
                    Ok(Some(Incoming::Refused(Refusal::NotOpen { request })))
                }
            }
            Flow::Request => {
                // A surrogate asks only under the request the controller is waiting on.
                if !self.open.contains_key(&parent) {
                    self.refused.insert(request);
                    return Ok(Some(Incoming::Refused(Refusal::Parent { request, parent })));
                }
                self.callbacks.insert(request);
                Ok(Some(Incoming::Callback {
                    request,
                    parent,
                    kind,
                    payload: frame.payload,
                }))
            }
            Flow::Event => Ok(Some(Incoming::Event {
                kind,
                payload: frame.payload,
            })),
        }
    }
}

/// What the surrogate takes from the controller.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SurrogateIncoming {
    /// A controller request: answer it with [`SurrogateLink::reply`].
    Request {
        request: u64,
        kind: Kind,
        payload: Vec<u8>,
    },
    /// The controller's answer to the surrogate's own request.
    CallbackReply {
        request: u64,
        kind: Kind,
        payload: Vec<u8>,
    },
    /// The controller will not serve the surrogate's request `request` ([`Kind::DECLINED`]).
    Declined { request: u64 },
    /// An event from the controller.
    Event { kind: Kind, payload: Vec<u8> },
    /// A frame dropped, with why.
    Refused(Refusal),
}

/// The surrogate's end of the link.
#[derive(Debug)]
pub struct SurrogateLink {
    out: Outbound,
    inbound: Inbound,
    failed: bool,
    /// The controller requests taken and not yet answered, oldest first.
    serving: VecDeque<u64>,
    next_callback: u64,
    /// The surrogate's requests waiting for the controller, with the kind each answer must have.
    callbacks: BTreeMap<u64, Kind>,
}

impl SurrogateLink {
    pub fn new(incarnation: Incarnation, limit: usize) -> Self {
        SurrogateLink {
            out: Outbound::new(incarnation),
            inbound: Inbound::new(incarnation, limit),
            failed: false,
            serving: VecDeque::new(),
            next_callback: 1,
            callbacks: BTreeMap::new(),
        }
    }

    pub fn incarnation(&self) -> Incarnation {
        self.out.incarnation
    }

    /// The surrogate's first frame.
    pub fn hello(&mut self) -> Vec<u8> {
        self.out.frame(Flow::Event, Kind::HELLO, 0, 0, &[])
    }

    /// The reply to the controller request `request`.
    pub fn reply(
        &mut self,
        request: u64,
        kind: Kind,
        payload: Vec<u8>,
    ) -> Result<Vec<u8>, SendError> {
        if kind.reserved() {
            return Err(SendError::ReservedKind(kind));
        }
        let Some(at) = self.serving.iter().position(|&r| r == request) else {
            return Err(SendError::NotOpen(request));
        };
        self.serving.remove(at);
        Ok(self.out.frame(Flow::Reply, kind, request, 0, &payload))
    }

    /// A request for a controller service, under the controller request being served (the newest
    /// one taken): its ID and the bytes to write. `expects` is the kind its answer must have.
    pub fn callback(
        &mut self,
        kind: Kind,
        expects: Kind,
        payload: Vec<u8>,
    ) -> Result<(u64, Vec<u8>), SendError> {
        for k in [kind, expects] {
            if k.reserved() {
                return Err(SendError::ReservedKind(k));
            }
        }
        let parent = *self.serving.back().ok_or(SendError::NoParent)?;
        let request = self.next_callback;
        self.next_callback += 1;
        self.callbacks.insert(request, expects);
        Ok((
            request,
            self.out.frame(Flow::Request, kind, request, parent, &payload),
        ))
    }

    /// A message to the controller on the surrogate's own initiative.
    pub fn event(&mut self, kind: Kind, payload: Vec<u8>) -> Vec<u8> {
        self.out.frame(Flow::Event, kind, 0, 0, &payload)
    }

    pub fn receive(&mut self, bytes: &[u8]) {
        self.inbound.decoder.push(bytes);
    }

    pub fn next(&mut self) -> Result<Option<SurrogateIncoming>, WireError> {
        if self.failed {
            return Err(WireError::Closed);
        }
        let result = self.take();
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    fn take(&mut self) -> Result<Option<SurrogateIncoming>, WireError> {
        let frame = match self.inbound.next()? {
            None => return Ok(None),
            Some(Checked::Refused(refusal)) => {
                return Ok(Some(SurrogateIncoming::Refused(refusal)));
            }
            Some(Checked::Frame(frame)) => frame,
        };
        let Header {
            flow,
            kind,
            request,
            ..
        } = frame.header;
        if flow == Flow::Reply && kind == Kind::DECLINED {
            if self.callbacks.remove(&request).is_none() {
                return Ok(Some(SurrogateIncoming::Refused(Refusal::NotOpen { request })));
            }
            return Ok(Some(SurrogateIncoming::Declined { request }));
        }
        if kind.reserved() {
            return Err(WireError::Unexpected { flow, kind });
        }
        match flow {
            Flow::Request => {
                self.serving.push_back(request);
                Ok(Some(SurrogateIncoming::Request {
                    request,
                    kind,
                    payload: frame.payload,
                }))
            }
            Flow::Reply => {
                let Some(expected) = self.callbacks.get(&request).copied() else {
                    return Ok(Some(SurrogateIncoming::Refused(Refusal::NotOpen { request })));
                };
                if expected != kind {
                    return Err(WireError::Kind {
                        request,
                        expected,
                        got: kind,
                    });
                }
                self.callbacks.remove(&request);
                Ok(Some(SurrogateIncoming::CallbackReply {
                    request,
                    kind,
                    payload: frame.payload,
                }))
            }
            Flow::Event => Ok(Some(SurrogateIncoming::Event {
                kind,
                payload: frame.payload,
            })),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn an_incarnation_round_trips_as_command_line_text() {
        let i = Incarnation::from_parts(0xfedc_ba98_7654_3210_0123_4567_89ab_cdef, 12);
        let text = i.to_text();
        assert_eq!(text, "fedcba98765432100123456789abcdef.12");
        assert_eq!(Incarnation::parse(&text), Some(i));
        for bad in ["", "00.1", "zz", &format!("{}x.1", &text[..31])] {
            assert_eq!(Incarnation::parse(bad), None, "{bad}");
        }
    }
}
