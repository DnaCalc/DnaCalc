//! RTD inputs in a OneCalc session (milestone 6, dc-snz.23).
//!
//! A session with an owner-supplied [`calc_fml::RtdConnector`] (the desktop application's, over
//! the loaded add-ins' RTD servers) evaluates its one formula with the rules calc-fml defines
//! for every host: the built-in `RTD` and an add-in's `xlfRtd` read a topic through the admitted
//! inputs, or connect it through the connector, whose first value is shown at once and certified
//! only when the session admits it as that sequence. The session owns what calc-fml holds no
//! state for: the ledger of admitted inputs ([`calc_fml::rtd::RtdLedger`]), the topics the
//! formula read, and the leases the formula stopped reading. It has no engine and no dependents:
//! when an admitted input is one the formula read, the owner calculates the formula again.

use calc_fml::rtd::{RtdDisposition, RtdEvent, RtdLedger};
use calc_fml::{RtdConnection, RtdConnector, RtdLease, RtdRead, TopicKey};
use calc_func::value::{CalcValue, CellKey};
use std::cell::RefCell;
use std::rc::Rc;

/// One RTD value the session admitted: its topic, lease and sequence, and the value.
#[derive(Clone, Debug, PartialEq)]
pub struct RtdAdmission {
    pub topic: TopicKey,
    pub lease: RtdLease,
    pub sequence: u64,
    pub value: CalcValue,
}

type Connected = Rc<RefCell<Vec<(TopicKey, RtdLease, u64, CalcValue)>>>;

/// The session's RTD state.
#[derive(Debug, Default)]
pub(crate) struct SessionRtd {
    connector: Option<Rc<dyn RtdConnector>>,
    pub(crate) ledger: RtdLedger,
    connected: Connected,
    reads: Vec<RtdRead>,
    invocations: std::collections::BTreeSet<u64>,
    unread: Vec<(TopicKey, RtdLease)>,
    admissions: Vec<RtdAdmission>,
}

/// The owner's connector with a record of every provisional value it returned, which the session
/// admits (as their sequence) once the evaluation has finished.
pub(crate) struct Recording {
    inner: Rc<dyn RtdConnector>,
    queue: Connected,
}

impl std::fmt::Debug for Recording {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Recording").finish_non_exhaustive()
    }
}

impl RtdConnector for Recording {
    fn connect(
        &self,
        topic: &TopicKey,
        caller: CellKey,
        declared: bool,
    ) -> Result<RtdConnection, String> {
        let connection = self.inner.connect(topic, caller, declared)?;
        if let Some((sequence, value)) = &connection.provisional {
            self.queue
                .borrow_mut()
                .push((topic.clone(), connection.lease, *sequence, value.clone()));
        }
        Ok(connection)
    }
}

impl SessionRtd {
    pub(crate) fn set_connector(&mut self, connector: Rc<dyn RtdConnector>) {
        self.connector = Some(connector);
    }

    pub(crate) fn has_connector(&self) -> bool {
        self.connector.is_some()
    }

    pub(crate) fn connector(&self) -> Option<Rc<dyn RtdConnector>> {
        self.connector.clone()
    }

    /// The connector for one evaluation, recording what it connects.
    pub(crate) fn recording(&self) -> Option<Recording> {
        self.connector.as_ref().map(|inner| Recording {
            inner: inner.clone(),
            queue: self.connected.clone(),
        })
    }

    /// An evaluation of the session's formula ended having read `reads`: the values its
    /// connections returned are admitted (before anything is shown as certified), and the leases
    /// the formula held before and no longer reads are noted for the owner to disconnect.
    pub(crate) fn finish(&mut self, reads: Vec<RtdRead>, invocations: std::collections::BTreeSet<u64>) {
        self.invocations = invocations;
        let connected = std::mem::take(&mut *self.connected.borrow_mut());
        for (topic, lease, sequence, value) in connected {
            self.admit(topic, lease, sequence, value);
        }
        let previous = std::mem::replace(&mut self.reads, reads);
        for read in previous {
            let still = self
                .reads
                .iter()
                .any(|now| now.topic == read.topic && now.lease == read.lease);
            let held = (read.topic, read.lease);
            if !still && !self.unread.contains(&held) {
                self.unread.push(held);
            }
        }
    }

    pub(crate) fn admit(
        &mut self,
        topic: TopicKey,
        lease: RtdLease,
        sequence: u64,
        value: CalcValue,
    ) -> RtdDisposition {
        let admitted = value.clone();
        let (next, disposition) = calc_fml::rtd::admit(
            &self.ledger,
            RtdEvent::Update {
                topic: topic.clone(),
                lease,
                sequence,
                value,
            },
        );
        if disposition == RtdDisposition::Accepted {
            self.ledger = next;
            self.admissions.push(RtdAdmission {
                topic,
                lease,
                sequence,
                value: admitted,
            });
        }
        disposition
    }

    pub(crate) fn retire(&mut self, topic: TopicKey, lease: RtdLease) -> RtdDisposition {
        let (next, disposition) = calc_fml::rtd::admit(
            &self.ledger,
            RtdEvent::Retire {
                topic: topic.clone(),
                lease,
            },
        );
        if disposition == RtdDisposition::Accepted {
            self.ledger = next;
        }
        self.unread.retain(|held| held != &(topic.clone(), lease));
        self.reads
            .retain(|read| !(read.topic == topic && read.lease == lease));
        disposition
    }

    pub(crate) fn invocations(&self) -> &std::collections::BTreeSet<u64> {
        &self.invocations
    }

    pub(crate) fn reads(&self) -> &[RtdRead] {
        &self.reads
    }

    pub(crate) fn take_unread(&mut self) -> Vec<(TopicKey, RtdLease)> {
        std::mem::take(&mut self.unread)
    }

    pub(crate) fn take_admissions(&mut self) -> Vec<RtdAdmission> {
        std::mem::take(&mut self.admissions)
    }
}
