//! Subscription publications, independent of a painter and of calculation completion.
//!
//! These receipts close a projection of already installed host results. They are not engine
//! AdmissionReceipt/CompletionReceipt or a claim that a workbook is fresh. Engine publication
//! identities travel alongside stream identities without upgrading individual cell provenance.
use crate::projection::{Cell, Geometry, Key, Sheet};
use serde::{Deserialize, Serialize};

pub const MAX_SUBSCRIPTIONS: usize = 32;
pub const MAX_TILE_CELLS: usize = 65_536;
/// A delta can remove the old root and install a disjoint new root.
pub const MAX_PUBLICATION_RECORDS: usize = MAX_TILE_CELLS * 2;
pub const MAX_PUBLICATION_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_TILE_BYTES: usize = 512 * 1024;
pub const MAX_TILE_RECORDS: usize = 256;
pub const MAX_PENDING_REQUESTS: usize = 256;

/// A stream-local publication identity. `workbook` changes on replacement. `next` counts
/// publications for this subscription, independently of host deltas coalesced under pressure.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TileStamp {
    /// Latest engine publication incorporated by this root, possibly followed by authored
    /// edits. This correlates delivery; cell provenance still determines freshness.
    #[serde(default)]
    pub calculation_through: Option<crate::projection::Publication>,
    pub id: u64,
    pub workbook: u64,
    pub transaction: u64,
    pub base: u64,
    pub next: u64,
    pub revision: u64,
    /// Geometry and styles share this publication's root epoch. They can change without an
    /// authored revision changing, for example when conditional formatting recalculates.
    pub geometry_version: u64,
    pub style_version: u64,
    /// Dictionaries are local to a publication and are released with it.
    pub dictionary: u64,
    pub reset: bool,
}

/// One bounded binary fragment. Dictionary definitions precede its address/index records on
/// the wire. Removals are explicit. No fragment is observable until the transaction closes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TileBatch {
    pub stamp: TileStamp,
    pub fragment: u32,
    pub final_fragment: bool,
    /// Geometry travels in the first fragment and commits with its cells.
    pub geometry: Vec<(Sheet, Geometry)>,
    pub cells: Vec<Cell>,
    pub removed: Vec<Key>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TileBarrier {
    pub id: u64,
    pub workbook: u64,
    pub transaction: u64,
    pub through: u64,
}
impl TileStamp {
    pub fn barrier(self) -> TileBarrier {
        TileBarrier {
            id: self.id,
            workbook: self.workbook,
            transaction: self.transaction,
            through: self.next,
        }
    }
}

impl Cell {
    /// Conservative wire/retention budget, including JSON escaping of dictionary strings.
    pub fn retained_bytes(&self) -> usize {
        let mut strings = vec![
            self.text.as_str(),
            self.kind.as_str(),
            self.input.as_str(),
            self.why.as_str(),
        ];
        strings.extend(
            [
                self.color.as_deref(),
                self.owner.as_deref(),
                self.error_meaning.as_deref(),
                self.style.family.as_deref(),
                self.style.horizontal.as_deref(),
                self.style.vertical.as_deref(),
                self.style.unsupported.as_deref(),
            ]
            .into_iter()
            .flatten(),
        );
        if let Some(c) = &self.conditional {
            strings.extend(
                [
                    c.fill.as_deref(),
                    c.font_color.as_deref(),
                    c.bar.as_ref().map(|b| b.1.as_str()),
                    c.icon.as_ref().map(|i| i.0.as_str()),
                ]
                .into_iter()
                .flatten(),
            );
        }
        strings.into_iter().fold(2048usize, |n, s| {
            n.saturating_add(s.len().saturating_mul(6))
        })
    }
}
impl TileBatch {
    pub fn retained_bytes(&self) -> usize {
        self.cells
            .iter()
            .fold(512usize, |n, c| n.saturating_add(c.retained_bytes()))
            .saturating_add(self.removed.len().saturating_mul(16))
            .saturating_add(
                self.geometry
                    .iter()
                    .fold(0usize, |n, (_, g)| n.saturating_add(geometry_bytes(g))),
            )
    }
}
pub fn geometry_bytes(g: &Geometry) -> usize {
    1024usize
        .saturating_add(g.coverage.len().saturating_mul(64))
        .saturating_add(
            g.rows
                .len()
                .saturating_add(g.columns.len())
                .saturating_mul(128),
        )
        .saturating_add(g.merges.len().saturating_mul(128))
        .saturating_add(
            g.edit_refusal
                .as_ref()
                .map_or(0, |s| s.len().saturating_mul(6)),
        )
}
