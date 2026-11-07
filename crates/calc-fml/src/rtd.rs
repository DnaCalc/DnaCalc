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

use calc_func::value::CalcValue;
use calc_func::{RtdCut, RtdEntry, RtdLease};
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
