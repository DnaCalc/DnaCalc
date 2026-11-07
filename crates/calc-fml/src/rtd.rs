//! The RTD admission rules as a pure function (dc-snz.23, design milestone 1).
//!
//! CalcFml keeps no RTD state: no field, static or cache lives here. The [`RtdLedger`] is a value
//! the owner (CalcHost, later OneCalc's session) holds; [`admit`] takes one ledger and one event
//! and returns the next ledger with the verdict. A ledger clone is cheap (its tables are `Arc`s)
//! and old ledgers stay valid for work that holds them. The ledger is readable from any thread:
//! it implements [`UserExternalInputs`], whose cut is `Send + Sync`.

use calc_func::value::CalcValue;
use calc_func::{UserExternalInputs, UserInputKey};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// What admitting one event did. `ProviderChanged` is decided by the owner before it asks, since
/// it alone knows which callables are bound.
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

/// One event for the ledger. A subscription is never recycled: retiring keeps a tombstone.
#[derive(Clone, Debug, PartialEq)]
pub enum RtdEvent {
    /// A value with its sequence for a subscription.
    Update {
        key: UserInputKey,
        sequence: u64,
        value: CalcValue,
    },
    /// The subscription is disconnected.
    Retire { key: UserInputKey },
}

/// The latest admitted value and sequence per key, the tombstones and the view version.
#[derive(Clone, Debug, Default)]
pub struct RtdLedger {
    version: u64,
    values: Arc<BTreeMap<UserInputKey, (u64, CalcValue)>>,
    retired: Arc<BTreeSet<UserInputKey>>,
}

impl RtdLedger {
    /// The admitted sequence and value of `key`, if any.
    pub fn get(&self, key: &UserInputKey) -> Option<&(u64, CalcValue)> {
        self.values.get(key)
    }
    /// Whether `key` has been retired.
    pub fn is_retired(&self, key: &UserInputKey) -> bool {
        self.retired.contains(key)
    }
    /// The number of admitted subscriptions.
    pub fn len(&self) -> usize {
        self.values.len()
    }
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
    fn advanced(&self) -> u64 {
        self.version
            .checked_add(1)
            .expect("RTD input view exhausted")
    }
}

impl UserExternalInputs for RtdLedger {
    fn version(&self) -> u64 {
        self.version
    }
    fn rtd_value(&self, key: UserInputKey) -> Option<(u64, CalcValue)> {
        self.values.get(&key).cloned()
    }
}

/// Admit one event. The returned ledger equals the argument unless the verdict is `Accepted`.
pub fn admit(ledger: &RtdLedger, event: RtdEvent) -> (RtdLedger, RtdDisposition) {
    match event {
        RtdEvent::Update {
            key,
            sequence,
            value,
        } => {
            if value.is_missing() || sequence == 0 {
                return (ledger.clone(), RtdDisposition::InvalidValue);
            }
            if ledger.retired.contains(&key) {
                return (ledger.clone(), RtdDisposition::Retired);
            }
            if let Some((held, old)) = ledger.values.get(&key) {
                if sequence < *held {
                    return (ledger.clone(), RtdDisposition::StaleSequence);
                }
                if sequence == *held {
                    let verdict = if *old == value {
                        RtdDisposition::Duplicate
                    } else {
                        RtdDisposition::ConflictingSequence
                    };
                    return (ledger.clone(), verdict);
                }
            }
            let mut next = ledger.clone();
            next.version = ledger.advanced();
            Arc::make_mut(&mut next.values).insert(key, (sequence, value));
            (next, RtdDisposition::Accepted)
        }
        RtdEvent::Retire { key } => {
            if ledger.retired.contains(&key) {
                return (ledger.clone(), RtdDisposition::Retired);
            }
            let mut next = ledger.clone();
            next.version = ledger.advanced();
            Arc::make_mut(&mut next.retired).insert(key);
            Arc::make_mut(&mut next.values).remove(&key);
            (next, RtdDisposition::Accepted)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use calc_func::UserCallableIdentity;

    fn key(subscription: u64) -> UserInputKey {
        UserInputKey {
            callable: UserCallableIdentity {
                table: 1,
                slot: 0,
                generation: 1,
            },
            subscription,
        }
    }
    fn update(subscription: u64, sequence: u64, value: f64) -> RtdEvent {
        RtdEvent::Update {
            key: key(subscription),
            sequence,
            value: CalcValue::number(value),
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn admission_orders_sequences_and_keeps_old_ledgers() {
        let empty = RtdLedger::default();
        let (one, verdict) = admit(&empty, update(7, 1, 20.0));
        assert_eq!(verdict, RtdDisposition::Accepted);
        assert!(empty.is_empty(), "the argument ledger is untouched");
        assert_eq!(one.version(), 1);
        assert_eq!(one.rtd_value(key(7)), Some((1, CalcValue::number(20.0))));

        let (same, verdict) = admit(&one, update(7, 1, 20.0));
        assert_eq!(verdict, RtdDisposition::Duplicate);
        assert_eq!(same.version(), 1);
        assert_eq!(
            admit(&one, update(7, 1, 999.0)).1,
            RtdDisposition::ConflictingSequence
        );
        let (two, verdict) = admit(&one, update(7, 2, 40.0));
        assert_eq!(verdict, RtdDisposition::Accepted);
        assert_eq!(admit(&two, update(7, 1, 20.0)).1, RtdDisposition::StaleSequence);
        assert_eq!(
            one.rtd_value(key(7)),
            Some((1, CalcValue::number(20.0))),
            "an older ledger stays valid for work that holds it"
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn invalid_values_and_retirement() {
        let empty = RtdLedger::default();
        assert_eq!(admit(&empty, update(7, 0, 1.0)).1, RtdDisposition::InvalidValue);
        let missing = RtdEvent::Update {
            key: key(7),
            sequence: 1,
            value: CalcValue::missing(),
        };
        assert_eq!(admit(&empty, missing).1, RtdDisposition::InvalidValue);

        let (one, _) = admit(&empty, update(7, 1, 20.0));
        let (gone, verdict) = admit(&one, RtdEvent::Retire { key: key(7) });
        assert_eq!(verdict, RtdDisposition::Accepted);
        assert_eq!(gone.rtd_value(key(7)), None);
        assert!(gone.is_retired(&key(7)));
        assert_eq!(
            admit(&gone, RtdEvent::Retire { key: key(7) }).1,
            RtdDisposition::Retired
        );
        assert_eq!(admit(&gone, update(7, 99, 1.0)).1, RtdDisposition::Retired);
        // Another subscription is unaffected.
        assert_eq!(admit(&gone, update(8, 1, 1.0)).1, RtdDisposition::Accepted);
    }
}
