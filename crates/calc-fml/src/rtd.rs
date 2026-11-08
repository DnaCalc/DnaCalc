//! The RTD admission rules as a pure function (dc-snz.23, design milestones 1 and 2).
//!
//! CalcFml keeps no RTD state: no field, static or cache lives here. The [`RtdLedger`] is a value
//! the owner (CalcHost, later OneCalc's session) holds; [`admit`] takes one ledger and one event
//! and returns the next ledger with the verdict. A ledger clone is cheap (its tables are `Arc`s)
//! and old ledgers stay valid for work that holds them. The ledger is readable from any thread:
//! it implements [`RtdCut`], whose cut is `Send + Sync`.
//!
//! The ledger is keyed by Excel's topic identity ([`TopicKey`]). Each topic holds the entry of its
//! latest connection incarnation ([`RtdLease`]); a lease above the held one replaces it and starts
//! a new sequence, and retiring an old lease keeps the newer entry.

use calc_func::value::{CalcValue, CellKey};
use calc_func::{RtdConnector, RtdCut, RtdEntry, RtdLease, RtdTopicRead};
pub use calc_func::{RtdRead, TopicKey};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// What admitting one event did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RtdDisposition {
    Retired,
    Accepted,
    Duplicate,
    StaleSequence,
    ConflictingSequence,
    ProviderChanged,
    InvalidValue,
}

/// One event for the ledger. A lease is never recycled: retiring keeps a tombstone.
#[derive(Clone, Debug, PartialEq)]
pub enum RtdEvent {
    /// A value with its sequence for a topic's connection incarnation. A lease above the held one
    /// is a new incarnation and starts a new sequence.
    Update {
        topic: TopicKey,
        lease: RtdLease,
        sequence: u64,
        value: CalcValue,
    },
    /// The incarnation is disconnected.
    Retire { topic: TopicKey, lease: RtdLease },
}

/// The latest admitted entry per topic, the tombstones and the view version.
#[derive(Clone, Debug, Default)]
pub struct RtdLedger {
    version: u64,
    entries: Arc<BTreeMap<TopicKey, RtdEntry>>,
    retired: Arc<BTreeSet<(TopicKey, RtdLease)>>,
}

impl RtdLedger {
    /// The admitted entry of `topic`, if any.
    pub fn entry(&self, topic: &TopicKey) -> Option<&RtdEntry> {
        self.entries.get(topic)
    }
    /// Whether this incarnation of `topic` has been retired.
    pub fn is_retired(&self, topic: &TopicKey, lease: RtdLease) -> bool {
        self.retired.contains(&(topic.clone(), lease))
    }
    /// The number of topics with an admitted entry.
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    fn advanced(&self) -> u64 {
        self.version
            .checked_add(1)
            .expect("RTD input view exhausted")
    }
}

impl RtdCut for RtdLedger {
    fn version(&self) -> u64 {
        self.version
    }
    fn get(&self, topic: &TopicKey) -> Option<RtdEntry> {
        self.entries.get(topic).cloned()
    }
}

/// Admit one event. The returned ledger equals the argument unless the verdict is `Accepted`.
pub fn admit(ledger: &RtdLedger, event: RtdEvent) -> (RtdLedger, RtdDisposition) {
    match event {
        RtdEvent::Update {
            topic,
            lease,
            sequence,
            value,
        } => {
            if value.is_missing() || sequence == 0 {
                return (ledger.clone(), RtdDisposition::InvalidValue);
            }
            if ledger.is_retired(&topic, lease) {
                return (ledger.clone(), RtdDisposition::Retired);
            }
            if let Some(held) = ledger.entries.get(&topic) {
                if lease < held.lease {
                    return (ledger.clone(), RtdDisposition::StaleSequence);
                }
                if lease == held.lease {
                    if sequence < held.sequence {
                        return (ledger.clone(), RtdDisposition::StaleSequence);
                    }
                    if sequence == held.sequence {
                        let verdict = if held.value == value {
                            RtdDisposition::Duplicate
                        } else {
                            RtdDisposition::ConflictingSequence
                        };
                        return (ledger.clone(), verdict);
                    }
                }
            }
            let mut next = ledger.clone();
            next.version = ledger.advanced();
            Arc::make_mut(&mut next.entries).insert(
                topic,
                RtdEntry {
                    lease,
                    sequence,
                    value,
                },
            );
            (next, RtdDisposition::Accepted)
        }
        RtdEvent::Retire { topic, lease } => {
            if ledger.is_retired(&topic, lease) {
                return (ledger.clone(), RtdDisposition::Retired);
            }
            let mut next = ledger.clone();
            next.version = ledger.advanced();
            // A newer incarnation of the topic keeps its entry.
            if ledger
                .entries
                .get(&topic)
                .is_some_and(|held| held.lease == lease)
            {
                Arc::make_mut(&mut next.entries).remove(&topic);
            }
            Arc::make_mut(&mut next.retired).insert((topic, lease));
            (next, RtdDisposition::Accepted)
        }
    }
}

/// The most topic strings a call can carry: a worksheet function takes 255 arguments, two of
/// them the ProgID and the server.
pub const MAX_TOPIC_STRINGS: usize = 253;

/// The built-in `RTD(progid, server, topic1, ...)`, registered by a host that has an RTD
/// connector ([`crate::CallableTable::register_rtd`]). It coerces its arguments to text as Excel
/// does and reads the topic through [`read_topic`], the rule the `xlfRtd` callback uses too, so
/// a typed `=RTD()` and an add-in's call with equal arguments are one topic. Local servers only:
/// a non-empty server name is a typed refusal. Excel's `#N/A` when no server can be started is
/// the answer to a connector failure; an error argument is returned as that error.
#[derive(Debug)]
pub struct RtdFunction;

pub(crate) const RTD_PROVIDER: &str = "calc-fml/rtd";

impl calc_func::UserFunction for RtdFunction {
    fn call(&self, _: &[CalcValue]) -> Result<CalcValue, String> {
        Err("dc-snz.23: RTD reads through the evaluator's connector".into())
    }
    fn call_with_context(
        &self,
        arguments: &[CalcValue],
        context: &mut dyn calc_func::UserFunctionContext,
    ) -> Result<CalcValue, String> {
        use calc_func::value::WorksheetErrorCode;
        let mut values = Vec::with_capacity(arguments.len());
        for argument in arguments {
            values.push(match argument.as_reference() {
                Some(reference) => context.argument_values_of(reference)?,
                None => argument.clone(),
            });
        }
        if values.len() < 3 {
            return Err(context.refuse_by("RTD without a ProgID, a server and a topic", "dc-snz.23"));
        }
        if values.len() - 2 > MAX_TOPIC_STRINGS {
            return Ok(CalcValue::error(WorksheetErrorCode::Value));
        }
        for value in &values {
            if let Some(error) = value.as_error() {
                return Ok(CalcValue::error(error));
            }
            if value.as_array().is_some() {
                return Err(context.refuse_by("RTD over an array argument", "dc-snz.23"));
            }
        }
        // Text as Excel coerces an argument to text: the one catalogue's CONCAT.
        let mut text = |value: &CalcValue| -> Result<Option<String>, String> {
            if value.is_missing() || value.is_empty() {
                return Ok(Some(String::new()));
            }
            let joined = context.worksheet_function("CONCAT", std::slice::from_ref(value))?;
            Ok(joined.as_text().map(|text| text.to_string_lossy()))
        };
        let mut texts = Vec::with_capacity(values.len());
        for value in &values {
            match text(value)? {
                Some(text) => texts.push(text),
                None => return Ok(CalcValue::error(WorksheetErrorCode::Value)),
            }
        }
        if !texts[1].is_empty() {
            return Err(context.refuse_by("RTD from a named (remote) server", "dc-snz.23"));
        }
        let topic = TopicKey::new(&texts[0], "", &texts[2..]);
        match context.read_topic(&topic, true) {
            Ok(seen) => Ok(match seen.value {
                // Excel shows an empty RTD value as zero.
                Some((_, value)) if value.is_empty() => CalcValue::number(0.0),
                Some((_, value)) => value,
                None => CalcValue::error(WorksheetErrorCode::NA),
            }),
            // No server could be started for the ProgID (or it failed): Excel's #N/A.
            Err(why) if why.starts_with(calc_func::RTD_CONNECT_FAILED) => {
                Ok(CalcValue::error(WorksheetErrorCode::NA))
            }
            Err(why) => Err(why),
        }
    }
}

/// Why [`read_topic`] could not read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RtdReadError {
    /// The evaluation has no admitted input cut.
    NoCut,
    /// The evaluation has no owner connector (the browser has none).
    NoConnector,
    /// The connector failed to connect.
    Connect(String),
}

/// The one rule for reading a topic, for the built-in `RTD` function and the `xlfRtd` callback
/// alike. The owner's connector joins or starts the connection (a topic already connected is
/// joined without a second ConnectData, and `caller` becomes one more reader). The value is the
/// cut's admitted one; while the connection's provisional value is newer than the cut's (or the
/// cut holds nothing for this lease), that value is shown at once, as Excel shows ConnectData's
/// return, and the read is recorded at its sequence, so the result is not certified until the
/// owner admits that sequence. Pure: it holds no state and calls nothing but the connector.
pub fn read_topic(
    cut: Option<&dyn RtdCut>,
    connector: Option<&dyn RtdConnector>,
    topic: &TopicKey,
    caller: CellKey,
    declared: bool,
) -> Result<(RtdTopicRead, RtdRead), RtdReadError> {
    let cut = cut.ok_or(RtdReadError::NoCut)?;
    let connector = connector.ok_or(RtdReadError::NoConnector)?;
    let connection = connector
        .connect(topic, caller, declared)
        .map_err(RtdReadError::Connect)?;
    let lease = connection.lease;
    let admitted = cut.get(topic).filter(|entry| entry.lease == lease);
    let value = match (connection.provisional, admitted) {
        (Some(provisional), Some(held)) if provisional.0 > held.sequence => Some(provisional),
        (Some(provisional), None) => Some(provisional),
        (_, Some(held)) => Some((held.sequence, held.value)),
        (None, None) => None,
    };
    let read = RtdRead {
        topic: topic.clone(),
        lease,
        sequence: value.as_ref().map_or(0, |(sequence, _)| *sequence),
    };
    Ok((RtdTopicRead { lease, value }, read))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn topic(name: &str) -> TopicKey {
        TopicKey::new("Prog.Id", "", &[name.to_owned()])
    }
    fn update(name: &str, lease: u64, sequence: u64, value: f64) -> RtdEvent {
        RtdEvent::Update {
            topic: topic(name),
            lease: RtdLease(lease),
            sequence,
            value: CalcValue::number(value),
        }
    }
    fn entry(lease: u64, sequence: u64, value: f64) -> Option<RtdEntry> {
        Some(RtdEntry {
            lease: RtdLease(lease),
            sequence,
            value: CalcValue::number(value),
        })
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn admission_orders_sequences_and_keeps_old_ledgers() {
        let empty = RtdLedger::default();
        let (one, verdict) = admit(&empty, update("a", 1, 1, 20.0));
        assert_eq!(verdict, RtdDisposition::Accepted);
        assert!(empty.is_empty(), "the argument ledger is untouched");
        assert_eq!(one.version(), 1);
        assert_eq!(one.get(&topic("a")), entry(1, 1, 20.0));

        let (same, verdict) = admit(&one, update("a", 1, 1, 20.0));
        assert_eq!(verdict, RtdDisposition::Duplicate);
        assert_eq!(same.version(), 1);
        assert_eq!(
            admit(&one, update("a", 1, 1, 999.0)).1,
            RtdDisposition::ConflictingSequence
        );
        let (two, verdict) = admit(&one, update("a", 1, 2, 40.0));
        assert_eq!(verdict, RtdDisposition::Accepted);
        assert_eq!(
            admit(&two, update("a", 1, 1, 20.0)).1,
            RtdDisposition::StaleSequence
        );
        assert_eq!(
            one.get(&topic("a")),
            entry(1, 1, 20.0),
            "an older ledger stays valid for work that holds it"
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn invalid_values_and_retirement() {
        let empty = RtdLedger::default();
        assert_eq!(
            admit(&empty, update("a", 1, 0, 1.0)).1,
            RtdDisposition::InvalidValue
        );
        let missing = RtdEvent::Update {
            topic: topic("a"),
            lease: RtdLease(1),
            sequence: 1,
            value: CalcValue::missing(),
        };
        assert_eq!(admit(&empty, missing).1, RtdDisposition::InvalidValue);

        let (one, _) = admit(&empty, update("a", 1, 1, 20.0));
        let retire = RtdEvent::Retire {
            topic: topic("a"),
            lease: RtdLease(1),
        };
        let (gone, verdict) = admit(&one, retire.clone());
        assert_eq!(verdict, RtdDisposition::Accepted);
        assert_eq!(gone.get(&topic("a")), None);
        assert!(gone.is_retired(&topic("a"), RtdLease(1)));
        assert_eq!(admit(&gone, retire).1, RtdDisposition::Retired);
        assert_eq!(
            admit(&gone, update("a", 1, 99, 1.0)).1,
            RtdDisposition::Retired
        );
        // Another topic is unaffected, and a new incarnation of the topic starts again.
        assert_eq!(
            admit(&gone, update("b", 1, 1, 1.0)).1,
            RtdDisposition::Accepted
        );
        let (again, verdict) = admit(&gone, update("a", 2, 1, 5.0));
        assert_eq!(verdict, RtdDisposition::Accepted);
        assert_eq!(again.get(&topic("a")), entry(2, 1, 5.0));
    }

    #[derive(Debug)]
    struct Connector(Option<(u64, f64)>, u64);
    impl RtdConnector for Connector {
        fn connect(
            &self,
            _topic: &TopicKey,
            _caller: CellKey,
            _declared: bool,
        ) -> Result<calc_func::RtdConnection, String> {
            Ok(calc_func::RtdConnection {
                lease: RtdLease(self.1),
                provisional: self.0.map(|(sequence, value)| (sequence, CalcValue::number(value))),
            })
        }
    }
    fn caller() -> CellKey {
        CellKey::new(calc_func::value::SheetId::from_parts(0, 0), 1, 1).unwrap()
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn read_topic_shows_the_provisional_value_and_reads_it_at_its_sequence() {
        let empty = RtdLedger::default();
        let t = topic("a");
        // A new connection: ConnectData's value shows at once, read at sequence 1.
        let (seen, read) =
            read_topic(Some(&empty), Some(&Connector(Some((1, 7.0)), 3)), &t, caller(), false).unwrap();
        assert_eq!(seen.lease, RtdLease(3));
        assert_eq!(seen.value, Some((1, CalcValue::number(7.0))));
        assert_eq!(read, RtdRead { topic: t.clone(), lease: RtdLease(3), sequence: 1 });
        // Once the owner admitted sequence 1, the admitted value is used.
        let (admitted, _) = admit(&empty, update("a", 3, 1, 7.0));
        let (seen, read) = read_topic(
            Some(&admitted),
            Some(&Connector(Some((1, 7.0)), 3)),
            &t,
            caller(),
            false,
        )
        .unwrap();
        assert_eq!(seen.value, Some((1, CalcValue::number(7.0))));
        assert_eq!(read.sequence, 1);
        // A joined topic with nothing provisional and nothing admitted has no value.
        let (seen, read) =
            read_topic(Some(&empty), Some(&Connector(None, 3)), &t, caller(), false).unwrap();
        assert_eq!(seen.value, None);
        assert_eq!(read.sequence, 0);
        // A cut entry of another incarnation is not this lease's.
        let (old, _) = admit(&empty, update("a", 2, 9, 1.0));
        let (seen, _) = read_topic(Some(&old), Some(&Connector(None, 3)), &t, caller(), false).unwrap();
        assert_eq!(seen.value, None);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_topic_key_is_excels_identity_as_measured() {
        let key = |progid: &str, server: &str, strings: &[&str]| {
            TopicKey::new(
                progid,
                server,
                &strings.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>(),
            )
        };
        // The ProgID compares without regard to ASCII case; topic strings compare exactly.
        assert_eq!(key("Dna.Prog", "", &["a"]), key("DNA.PROG", "", &["a"]));
        assert_ne!(key("Dna.Prog", "", &["abc"]), key("Dna.Prog", "", &["ABC"]));
        assert_ne!(key("Dna.Prog", "", &["t"]), key("Dna.Prog", "", &[" t"]));
        // Two strings are not one.
        assert_ne!(key("Dna.Prog", "", &["x", "y"]), key("Dna.Prog", "", &["x"]));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn read_topic_refuses_without_a_cut_or_a_connector() {
        let empty = RtdLedger::default();
        let t = topic("a");
        assert_eq!(
            read_topic(None, Some(&Connector(None, 1)), &t, caller(), false).unwrap_err(),
            RtdReadError::NoCut
        );
        assert_eq!(
            read_topic(Some(&empty), None, &t, caller(), false).unwrap_err(),
            RtdReadError::NoConnector
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_new_incarnation_replaces_the_old_and_its_retirement_keeps_the_new_entry() {
        let (one, _) = admit(&RtdLedger::default(), update("a", 1, 7, 20.0));
        // The replacement connects first: lease 2 starts at sequence 1 and replaces lease 1.
        let (two, verdict) = admit(&one, update("a", 2, 1, 30.0));
        assert_eq!(verdict, RtdDisposition::Accepted);
        assert_eq!(two.get(&topic("a")), entry(2, 1, 30.0));
        // A late value of the old incarnation is stale.
        assert_eq!(
            admit(&two, update("a", 1, 8, 99.0)).1,
            RtdDisposition::StaleSequence
        );
        // Retiring the old incarnation afterwards keeps the new entry.
        let (after, verdict) = admit(
            &two,
            RtdEvent::Retire {
                topic: topic("a"),
                lease: RtdLease(1),
            },
        );
        assert_eq!(verdict, RtdDisposition::Accepted);
        assert_eq!(after.get(&topic("a")), entry(2, 1, 30.0));
        assert_eq!(
            admit(&after, update("a", 1, 9, 1.0)).1,
            RtdDisposition::Retired
        );
    }
}
