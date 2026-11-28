use crate::{
    Answer, Batch, Budget, FormulaAnswer, FormulaOperation, FormulaQuery, Frame, Operation,
    Preview, PreviewDepth, Refusal, Selection, Stamp, Target, WatchId, WorkbookQuery,
};
use serde::{Deserialize, Serialize};
use std::{cell::RefCell, collections::VecDeque, rc::Rc};

/// Version of the control-message schema. Increment when an incompatible message changes.
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Message {
    WorkbookRequest(WorkbookRequest),
    WorkbookReply(WorkbookReply),
    Frame(Frame),
    Tile(crate::tiles::TileStamp),
    Failure(Refusal),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum WorkbookRequest {
    Open {
        name: String,
    },
    Ask {
        query: WorkbookQuery,
        budget: Budget,
    },
    Watch {
        query: WorkbookQuery,
        budget: Budget,
    },
    Unwatch(WatchId),
    AcknowledgeTiles {
        transaction: u64,
    },
    ResetTiles,
    Select {
        selection: Selection,
        client_seq: u64,
    },
    Navigate(Target),
    Preview {
        batch: Batch,
        depth: PreviewDepth,
    },
    PreviewProposal {
        proposal: crate::ProposalId,
        units: Vec<crate::UnitId>,
        depth: PreviewDepth,
    },
    Commit(Batch),
    Run(Operation),
    FormulaQuery(FormulaQuery),
    FormulaOperation(FormulaOperation),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum WorkbookReply {
    Answer(Answer<serde_json::Value>),
    Watching(WatchId),
    Preview(Preview),
    Stamp(Stamp),
    Accepted,
    Formula(FormulaAnswer),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Envelope {
    pub schema: u32,
    pub request: u64,
    pub message: Message,
}

impl Envelope {
    pub fn new(request: u64, message: Message) -> Self {
        Self {
            schema: SCHEMA_VERSION,
            request,
            message,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Packet {
    pub envelope: Envelope,
    pub bytes: Option<Vec<u8>>,
}

pub trait Carrier {
    fn send(&self, packet: &Packet) -> Result<(), Refusal>;
    fn receive(&self) -> Result<Option<Packet>, Refusal>;
}

struct Encoded {
    control: Vec<u8>,
    bytes: Option<Vec<u8>>,
}

type Queue = Rc<RefCell<VecDeque<Encoded>>>;

/// A bounded, FIFO carrier with separate control and binary payloads. Uses the same JSON control
/// encoding as remote carriers so in-process callers exercise the wire representation.
pub struct InProcess {
    incoming: Queue,
    outgoing: Queue,
    capacity: usize,
}

impl InProcess {
    pub fn pair(capacity: usize) -> (Self, Self) {
        let front = Queue::default();
        let back = Queue::default();
        (
            Self {
                incoming: front.clone(),
                outgoing: back.clone(),
                capacity,
            },
            Self {
                incoming: back,
                outgoing: front,
                capacity,
            },
        )
    }
}

impl Carrier for InProcess {
    fn send(&self, packet: &Packet) -> Result<(), Refusal> {
        let mut outgoing = self.outgoing.borrow_mut();
        if outgoing.len() >= self.capacity {
            return Err(Refusal::Transport("carrier queue is full".into()));
        }
        let control = serde_json::to_vec(&packet.envelope)
            .map_err(|error| Refusal::Transport(error.to_string()))?;
        outgoing.push_back(Encoded {
            control,
            bytes: packet.bytes.clone(),
        });
        Ok(())
    }

    fn receive(&self) -> Result<Option<Packet>, Refusal> {
        let Some(Encoded { control, bytes }) = self.incoming.borrow_mut().pop_front() else {
            return Ok(None);
        };
        decode(&control, bytes).map(Some)
    }
}

pub fn decode(control: &[u8], bytes: Option<Vec<u8>>) -> Result<Packet, Refusal> {
    #[derive(Deserialize)]
    struct Header {
        schema: u32,
    }
    let parse_error = |error: serde_json::Error| Refusal::Transport(error.to_string());
    let Header { schema } = serde_json::from_slice(control).map_err(parse_error)?;
    if schema != SCHEMA_VERSION {
        return Err(Refusal::SchemaMismatch {
            expected: SCHEMA_VERSION,
            received: schema,
        });
    }
    let envelope = serde_json::from_slice(control).map_err(parse_error)?;
    Ok(Packet { envelope, bytes })
}
