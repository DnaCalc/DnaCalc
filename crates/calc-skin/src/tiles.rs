//! Bounded, independent binary tile fragments shared by the two workbook pages.
use crate::grid::{Area, Key, Sheet};
use crate::{RawValue, WorkbookCell};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::collections::BTreeMap;

pub trait TileCell: Clone + Serialize + DeserializeOwned {
    fn key(&self) -> Key;
    fn set_key(&mut self, at: Key);
    fn retained_bytes(&self) -> usize;
}

pub trait TileGeometry: Serialize + DeserializeOwned {
    fn retained_bytes(&self) -> usize;
}

impl TileCell for WorkbookCell {
    fn key(&self) -> Key {
        self.at
    }

    fn set_key(&mut self, at: Key) {
        self.at = at;
    }

    fn retained_bytes(&self) -> usize {
        let value = match &self.shown.value {
            RawValue::Text(units) => units.len().saturating_mul(2),
            RawValue::Error(code) => code.len(),
            RawValue::Other { kind, display } => kind.len().saturating_add(display.len()),
            _ => 0,
        };
        let source = match &self.shown.provenance {
            crate::Provenance::External { source, .. } => source.len(),
            crate::Provenance::Refused { owner, reason } => {
                owner.len().saturating_add(reason.len())
            }
            _ => 0,
        };
        512usize
            .saturating_add(self.input.len())
            .saturating_add(self.text.len())
            .saturating_add(value)
            .saturating_add(source)
    }
}

impl TileGeometry for Area {
    fn retained_bytes(&self) -> usize {
        128
    }
}

/// Identity of an observed host calculation publication.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Publication {
    pub domain: u64,
    pub command: u64,
    pub sequence: u64,
}

pub const MAX_SUBSCRIPTIONS: usize = 32;
pub const MAX_TILE_CELLS: usize = 65_536;
/// A delta can remove the old root and install a disjoint new root.
pub const MAX_PUBLICATION_RECORDS: usize = MAX_TILE_CELLS * 2;
/// Sized so that MAX_TILE_CELLS ordinary cells fit: each is charged CELL_BYTES plus its text.
pub const MAX_PUBLICATION_BYTES: usize = 80 * 1024 * 1024;
/// What a Cell holds inline, rounded up (896 bytes on 64-bit native). Heap text is added on top.
pub const CELL_BYTES: usize = 1024;
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
    pub calculation_through: Option<Publication>,
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
pub struct TileBatch<C, G> {
    pub stamp: TileStamp,
    pub fragment: u32,
    pub final_fragment: bool,
    /// Geometry travels in the first fragment and commits with its cells.
    pub geometry: Vec<(Sheet, G)>,
    pub cells: Vec<C>,
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

impl<C: TileCell, G: TileGeometry> TileBatch<C, G> {
    pub fn retained_bytes(&self) -> usize {
        self.cells
            .iter()
            .fold(512usize, |n, c| n.saturating_add(c.retained_bytes()))
            .saturating_add(self.removed.len().saturating_mul(16))
            .saturating_add(
                self.geometry
                    .iter()
                    .fold(0usize, |n, (_, g)| n.saturating_add(g.retained_bytes())),
            )
    }
}
pub const TILE_TAG: &str = "dnacalc-tiles-v2";
const MAGIC: &[u8; 8] = b"DNATILE2";

pub fn encode_tiles<C: TileCell, G: TileGeometry>(
    batch: &TileBatch<C, G>,
) -> Result<Vec<u8>, String> {
    if batch.cells.len() + batch.removed.len() > MAX_TILE_RECORDS
        || batch.retained_bytes() > MAX_TILE_BYTES
    {
        return Err("The tile fragment exceeds its bounded budget".into());
    }
    let mut out = MAGIC.to_vec();
    let s = batch.stamp;
    for n in [
        s.id,
        s.workbook,
        s.transaction,
        s.base,
        s.next,
        s.revision,
        s.geometry_version,
        s.style_version,
        s.dictionary,
    ] {
        out.extend_from_slice(&n.to_le_bytes());
    }
    out.push(u8::from(s.calculation_through.is_some()));
    if let Some(unit) = s.calculation_through {
        for n in [unit.domain, unit.command, unit.sequence] {
            out.extend_from_slice(&n.to_le_bytes());
        }
    }
    out.extend_from_slice(&batch.fragment.to_le_bytes());
    out.push(u8::from(s.reset) | (u8::from(batch.final_fragment) << 1));
    let mut dictionary = Vec::new();
    let mut indexes = BTreeMap::new();
    let mut records = Vec::new();
    for cell in &batch.cells {
        let mut atom = cell.clone();
        atom.set_key(Key(0, 0, 1, 1));
        let text = encode(&atom)?;
        let next = dictionary.len() as u32;
        let index = *indexes.entry(text.clone()).or_insert_with(|| {
            dictionary.push(text);
            next
        });
        records.push((cell.key(), index));
    }
    // Definitions precede every use. Lengths are checked before decoder allocation.
    write_u32(&mut out, dictionary.len());
    for atom in dictionary {
        write_text(&mut out, &atom);
    }
    write_text(&mut out, &encode(&batch.geometry)?);
    write_u32(&mut out, records.len());
    for (at, index) in records {
        write_key(&mut out, at);
        out.extend_from_slice(&index.to_le_bytes());
    }
    write_u32(&mut out, batch.removed.len());
    for &at in &batch.removed {
        write_key(&mut out, at);
    }
    if out.len() > MAX_TILE_BYTES {
        return Err("Encoded tile fragment exceeds its byte limit".into());
    }
    Ok(out)
}
fn write_u32(out: &mut Vec<u8>, n: usize) {
    out.extend_from_slice(&(n as u32).to_le_bytes());
}
fn write_text(out: &mut Vec<u8>, s: &str) {
    write_u32(out, s.len());
    out.extend_from_slice(s.as_bytes());
}
fn write_key(out: &mut Vec<u8>, at: Key) {
    for n in [at.0, at.1, at.2, at.3] {
        out.extend_from_slice(&n.to_le_bytes());
    }
}

struct Reader<'a>(&'a [u8]);
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        if n > self.0.len() {
            return Err("Truncated tile fragment".into());
        }
        let (out, rest) = self.0.split_at(n);
        self.0 = rest;
        Ok(out)
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn count(&mut self, limit: usize) -> Result<usize, String> {
        let n = self.u32()? as usize;
        if n > limit {
            return Err("Tile count exceeds its bounded budget".into());
        }
        Ok(n)
    }
    fn text(&mut self) -> Result<&'a str, String> {
        let n = self.count(MAX_TILE_BYTES)?;
        std::str::from_utf8(self.take(n)?).map_err(|_| "Tile dictionary is not UTF-8".into())
    }
    fn key(&mut self) -> Result<Key, String> {
        let at = Key(self.u32()?, self.u32()?, self.u32()?, self.u32()?);
        at.cell().ok_or("Tile address is outside the worksheet")?;
        Ok(at)
    }
}
pub fn decode_tiles<C: TileCell, G: TileGeometry>(bytes: &[u8]) -> Result<TileBatch<C, G>, String> {
    if bytes.len() > MAX_TILE_BYTES {
        return Err("Tile fragment exceeds its byte limit".into());
    }
    let mut r = Reader(bytes);
    if r.take(8)? != MAGIC {
        return Err("Unknown tile wire version".into());
    }
    let mut stamp = TileStamp {
        calculation_through: None,
        id: r.u64()?,
        workbook: r.u64()?,
        transaction: r.u64()?,
        base: r.u64()?,
        next: r.u64()?,
        revision: r.u64()?,
        geometry_version: r.u64()?,
        style_version: r.u64()?,
        dictionary: r.u64()?,
        reset: false,
    };
    stamp.calculation_through = match r.take(1)?[0] {
        0 => None,
        1 => Some(Publication {
            domain: r.u64()?,
            command: r.u64()?,
            sequence: r.u64()?,
        }),
        _ => return Err("Unknown calculation publication marker".into()),
    };
    let fragment = r.u32()?;
    let flags = r.take(1)?[0];
    if flags & !3 != 0 {
        return Err("Unknown tile flags".into());
    }
    stamp.reset = flags & 1 != 0;
    let count = r.count(MAX_TILE_RECORDS)?;
    let mut dictionary: Vec<C> = Vec::with_capacity(count);
    let mut dictionary_bytes = 0usize;
    for _ in 0..count {
        let atom: C = decode_atom(r.text()?)?;
        dictionary_bytes = dictionary_bytes.saturating_add(atom.retained_bytes());
        if dictionary_bytes > MAX_TILE_BYTES {
            return Err("Decoded tile dictionary exceeds its retention limit".into());
        }
        dictionary.push(atom);
    }
    let geometry: Vec<(Sheet, G)> = decode_atom(r.text()?)?;
    let mut retained = geometry
        .iter()
        .fold(512usize, |n, (_, g)| n.saturating_add(g.retained_bytes()));
    if retained > MAX_TILE_BYTES {
        return Err("Decoded tile geometry exceeds its retention limit".into());
    }
    let count = r.count(MAX_TILE_RECORDS)?;
    let mut cells = Vec::with_capacity(count);
    for _ in 0..count {
        let at = r.key()?;
        let index = r.u32()? as usize;
        let atom = dictionary
            .get(index)
            .ok_or("Undefined tile dictionary reference")?;
        // A small dictionary can fan out into large records. Charge each reference before
        // cloning its strings, rather than allocating the entire expansion and checking later.
        retained = retained.saturating_add(atom.retained_bytes());
        if retained > MAX_TILE_BYTES {
            return Err("Decoded tile exceeds its retention limit".into());
        }
        let mut cell = atom.clone();
        cell.set_key(at);
        cells.push(cell);
    }
    let count = r.count(MAX_TILE_RECORDS - cells.len())?;
    if retained.saturating_add(count.saturating_mul(16)) > MAX_TILE_BYTES {
        return Err("Decoded tile removals exceed their retention limit".into());
    }
    let mut removed = Vec::with_capacity(count);
    for _ in 0..count {
        removed.push(r.key()?);
    }
    if !r.0.is_empty() {
        return Err("Trailing bytes in tile fragment".into());
    }
    let batch = TileBatch {
        stamp,
        fragment,
        final_fragment: flags & 2 != 0,
        geometry,
        cells,
        removed,
    };
    if batch.retained_bytes() > MAX_TILE_BYTES {
        return Err("Decoded tile exceeds its retention limit".into());
    }
    Ok(batch)
}

fn encode(value: &impl Serialize) -> Result<String, String> {
    serde_json::to_string(value).map_err(|error| error.to_string())
}

fn decode_atom<T: DeserializeOwned>(text: &str) -> Result<T, String> {
    serde_json::from_str(text).map_err(|error| error.to_string())
}
