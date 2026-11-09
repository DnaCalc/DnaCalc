//! The random source RAND, RANDBETWEEN and RANDARRAY draw from during a recalculation.
//!
//! Ported from OxCalc `src/oxcalc-core/src/grid/machine/workbook_nodes.rs`
//! (`WorkbookVolatileRandomProvider`, `splitmix64`, `fnv1a64`) at commit 1bd514ef. Each formula's
//! draws are a `splitmix64` stream whose start mixes the state's seed, the recalculation's tick
//! and the formula's cell, so the values a formula draws do not depend on the order formulas are
//! evaluated in, and the next recalculation draws new ones.
//!
//! A formula that waits for a cell it read keeps its stream with its retained evaluation, which
//! resumes at the waiting instruction (docs/architecture/EVALUATOR_SUSPENSION.md): every draw
//! happens once and none is repeated. A formula the engine evaluates again from its start (the
//! fallback, when a wait cannot be taken in place and no user function has run) starts a new
//! stream of the same key, and so draws the same values again; nothing it drew before is kept.

use calc_fml::RandomSource;
use calc_value::CellKey;

/// One formula's stream of uniform numbers in [0, 1) for one recalculation.
#[derive(Debug)]
pub(crate) struct RandomStream {
    state: u64,
}

impl RandomStream {
    pub(crate) fn new(seed: u64, tick: u64, at: CellKey) -> Self {
        let mut key = [0u8; 20];
        key[..4].copy_from_slice(&at.sheet().slot().to_le_bytes());
        key[4..8].copy_from_slice(&at.sheet().generation().to_le_bytes());
        key[8..12].copy_from_slice(&at.row().to_le_bytes());
        key[12..16].copy_from_slice(&at.column().to_le_bytes());
        RandomStream {
            state: splitmix64(seed ^ splitmix64(tick)) ^ splitmix64(fnv1a64(&key)),
        }
    }
}

impl RandomSource for RandomStream {
    fn next_unit(&mut self) -> f64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        // The top 53 bits of the mixed output, as a double in [0, 1).
        ((splitmix64(self.state) >> 11) as f64) * (1.0 / ((1_u64 << 53) as f64))
    }
}

/// Actual unit draws of one explicit evaluation, in ordinal order. Its enclosing
/// invocation supplies the draw key; omitted samples still execute normally.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RandomDrawEvidence {
    pub samples: Vec<u64>,
    pub omitted: u64,
}
pub const MAX_RECORDED_RANDOM_DRAWS: usize = 1024;

pub(crate) struct RecordedRandom {
    stream: RandomStream,
    evidence: RandomDrawEvidence,
}
impl RecordedRandom {
    pub(crate) fn new(stream: RandomStream) -> Self {
        Self {
            stream,
            evidence: RandomDrawEvidence::default(),
        }
    }
    pub(crate) fn into_evidence(self) -> RandomDrawEvidence {
        self.evidence
    }
}
impl RandomSource for RecordedRandom {
    fn next_unit(&mut self) -> f64 {
        let value = self.stream.next_unit();
        if self.evidence.samples.len() < MAX_RECORDED_RANDOM_DRAWS {
            self.evidence.samples.push(value.to_bits());
        } else {
            self.evidence.omitted = self
                .evidence
                .omitted
                .checked_add(1)
                .expect("random evidence count exhausted");
        }
        value
    }
}

/// One `splitmix64` finalizing mix: a bijection on `u64`.
const fn splitmix64(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The FNV-1a 64-bit hash of `bytes`.
const fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xCBF2_9CE4_8422_2325;
    let mut i = 0;
    while i < bytes.len() {
        hash ^= bytes[i] as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
        i += 1;
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use calc_value::{IdAllocator, SheetId};

    fn draws(seed: u64, tick: u64, at: CellKey) -> Vec<f64> {
        let mut stream = RandomStream::new(seed, tick, at);
        (0..4).map(|_| stream.next_unit()).collect()
    }

    /// A formula's stream depends on the seed, the tick and its cell only: the same three give the
    /// same draws, another tick or another cell others, and every draw lies in [0, 1).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn streams_are_keyed_by_seed_tick_and_cell() {
        let mut sheets: IdAllocator<SheetId> = IdAllocator::new();
        let sheet = sheets.issue().unwrap();
        let a1 = CellKey::new(sheet, 1, 1).unwrap();
        let a2 = CellKey::new(sheet, 2, 1).unwrap();
        assert_eq!(draws(7, 1, a1), draws(7, 1, a1));
        assert_ne!(draws(7, 1, a1), draws(7, 2, a1));
        assert_ne!(draws(7, 1, a1), draws(7, 1, a2));
        assert_ne!(draws(7, 1, a1), draws(8, 1, a1));
        assert!(draws(7, 1, a1).iter().all(|unit| (0.0..1.0).contains(unit)));
    }
}
