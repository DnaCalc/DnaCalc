//! Admission and bounded receipt retention for a serial command source. Admission happens
//! before execution, including validation and calculation; an old identity never starts again.
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandStamp {
    pub id: u64,
    pub workbook: u64,
    pub base_revision: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandRefusal {
    IdentityReused,
    ReceiptExpired,
    StaleWorkbook,
    StaleRevision { base: u64, current: u64 },
}
impl std::fmt::Display for CommandRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IdentityReused => {
                f.write_str("This command identity was already used for another command")
            }
            Self::ReceiptExpired => {
                f.write_str("This command identity has expired; it will not run again")
            }
            Self::StaleWorkbook => {
                f.write_str("This command belongs to a workbook that has been replaced")
            }
            Self::StaleRevision { base, current } => write!(
                f,
                "This command applies to revision {base}; the workbook is at revision {current}"
            ),
        }
    }
}
/// One journal per serial command source, retained across workbook replacement. Only the most
/// recent receipts are retained; the high watermark prevents an evicted identity applying twice.
/// This is an in-memory delivery guarantee, not a recovery log or an effect replay mechanism.
pub struct CommandJournal<T> {
    high: u64,
    receipts: BTreeMap<u64, (String, T, usize)>,
    capacity: usize,
    retained: usize,
    max_bytes: usize,
}
impl<T: Clone> CommandJournal<T> {
    pub fn new(capacity: usize, max_bytes: usize) -> Self {
        assert!(capacity > 0);
        Self {
            high: 0,
            receipts: BTreeMap::new(),
            capacity,
            retained: 0,
            max_bytes,
        }
    }
    pub fn next_id(&self) -> u64 {
        self.high
            .checked_add(1)
            .expect("command identities exhausted")
    }
    pub fn admit(
        &mut self,
        stamp: CommandStamp,
        workbook: u64,
        revision: u64,
        payload: &str,
    ) -> Result<Option<T>, CommandRefusal> {
        if let Some((previous, receipt, _)) = self.receipts.get(&stamp.id) {
            return if previous == payload {
                Ok(Some(receipt.clone()))
            } else {
                Err(CommandRefusal::IdentityReused)
            };
        }
        if stamp.id <= self.high {
            return Err(CommandRefusal::ReceiptExpired);
        }
        // Consume the identity even if revision validation refuses it. A later delivery may
        // never turn the same refused command into an effect after the workbook changes.
        self.high = stamp.id;
        if stamp.workbook != workbook {
            return Err(CommandRefusal::StaleWorkbook);
        }
        if stamp.base_revision != revision {
            return Err(CommandRefusal::StaleRevision {
                base: stamp.base_revision,
                current: revision,
            });
        }
        Ok(None)
    }
    pub fn remember(&mut self, id: u64, payload: String, receipt: T, receipt_bytes: usize) {
        let weight = payload.len().saturating_add(receipt_bytes);
        if weight > self.max_bytes {
            return;
        }
        self.retained = self.retained.saturating_add(weight);
        if let Some((_, _, old)) = self.receipts.insert(id, (payload, receipt, weight)) {
            self.retained -= old;
        }
        while self.receipts.len() > self.capacity || self.retained > self.max_bytes {
            if let Some((_, (_, _, weight))) = self.receipts.pop_first() {
                self.retained -= weight;
            }
        }
    }
}
