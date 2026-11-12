//! Tile wire v2: little-endian header and address/index records, preceded by a bounded JSON
//! dictionary of presentation atoms and geometry. Controls use the separate JSON envelope.
//! Dictionaries never reference a previous fragment, so reset does not leave dangling ids.
use crate::{decode, encode};
use dna_calc_model::projection::{Cell, Geometry, Key, Sheet};
pub use dna_calc_model::tiles::*;
use std::collections::BTreeMap;

pub const TILE_TAG: &str = "dnacalc-tiles-v2";
const MAGIC: &[u8; 8] = b"DNATILE2";

pub fn encode_tiles(batch: &TileBatch) -> Result<Vec<u8>, String> {
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
        atom.at = Key(0, 0, 1, 1);
        let text = encode(&atom);
        let next = dictionary.len() as u32;
        let index = *indexes.entry(text.clone()).or_insert_with(|| {
            dictionary.push(text);
            next
        });
        records.push((cell.at, index));
    }
    // Definitions precede every use. Lengths are checked before decoder allocation.
    write_u32(&mut out, dictionary.len());
    for atom in dictionary {
        write_text(&mut out, &atom);
    }
    write_text(&mut out, &encode(&batch.geometry));
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
pub fn decode_tiles(bytes: &[u8]) -> Result<TileBatch, String> {
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
        1 => Some(crate::Publication {
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
    let mut dictionary: Vec<Cell> = Vec::with_capacity(count);
    let mut dictionary_bytes = 0usize;
    for _ in 0..count {
        let atom: Cell = decode(r.text()?)?;
        dictionary_bytes = dictionary_bytes.saturating_add(atom.retained_bytes());
        if dictionary_bytes > MAX_TILE_BYTES {
            return Err("Decoded tile dictionary exceeds its retention limit".into());
        }
        dictionary.push(atom);
    }
    let geometry: Vec<(Sheet, Geometry)> = decode(r.text()?)?;
    let mut retained = geometry
        .iter()
        .fold(512usize, |n, (_, g)| n.saturating_add(geometry_bytes(g)));
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
        cell.at = at;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FormatShown, Provenance};
    fn sample() -> TileBatch {
        let c = Cell {
            at: Key(0, 0, 1, 1),
            text: "0.30000000000000004 🧬\n\"".into(),
            kind: "number".into(),
            provenance: Provenance::Calculated,
            formula: true,
            style: Default::default(),
            format: FormatShown::General,
            color: None,
            conditional: None,
            input: "=0.1+0.2".into(),
            why: "calculated".into(),
            owner: None,
            error_meaning: None,
            exact: None,
        };
        let mut second = c.clone();
        second.at.2 = 2;
        TileBatch {
            stamp: TileStamp {
                calculation_through: None,
                id: 7,
                workbook: 8,
                transaction: u64::MAX,
                base: 9007199254740992,
                next: 9007199254740993,
                revision: 17,
                geometry_version: 17,
                style_version: 17,
                dictionary: u64::MAX,
                reset: true,
            },
            fragment: 3,
            final_fragment: true,
            cells: vec![c, second],
            removed: vec![Key(0, 0, 3, 1)],
            geometry: vec![(
                Sheet(0, 0),
                Geometry {
                    default_row_height: Some(1. / 3.),
                    ..Default::default()
                },
            )],
        }
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn binary_records_keep_u64_ids_text_and_geometry_exact_and_share_dictionary_atoms() {
        let batch = sample();
        let bytes = encode_tiles(&batch).unwrap();
        assert_eq!(decode_tiles(&bytes), Ok(batch));
        // Fixed header then one dictionary atom shared by the two address records.
        assert_eq!(u32::from_le_bytes(bytes[86..90].try_into().unwrap()), 1);
        assert!(bytes.starts_with(MAGIC));
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn truncation_unknown_versions_flags_counts_addresses_and_dictionary_references_are_refused() {
        let bytes = encode_tiles(&sample()).unwrap();
        for n in 0..bytes.len() {
            assert!(decode_tiles(&bytes[..n]).is_err(), "accepted prefix {n}");
        }
        let mut bad = bytes.clone();
        bad[0] = 0;
        assert!(decode_tiles(&bad).is_err());
        let mut bad = bytes.clone();
        bad[85] = 4;
        assert!(decode_tiles(&bad).is_err());
        let mut bad = bytes.clone();
        bad[86..90].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode_tiles(&bad).is_err());
        let mut bad = bytes.clone();
        bad.push(0);
        assert!(decode_tiles(&bad).is_err());
        let mut r = Reader(&bytes[86..]);
        let atoms = r.u32().unwrap();
        for _ in 0..atoms {
            r.text().unwrap();
        }
        r.text().unwrap();
        let start = bytes.len() - r.0.len();
        // count, sheet slot/generation, row, column, dictionary reference
        let mut bad = bytes.clone();
        bad[start + 20..start + 24].copy_from_slice(&999u32.to_le_bytes());
        assert!(decode_tiles(&bad).is_err());
        let mut bad = bytes.clone();
        bad[start + 12..start + 16].copy_from_slice(&0u32.to_le_bytes());
        assert!(decode_tiles(&bad).is_err());
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn calculation_publication_survives_the_binary_tile_route() {
        let mut batch = sample();
        batch.stamp.calculation_through = Some(crate::Publication {
            domain: u64::MAX - 1,
            command: u64::MAX - 2,
            sequence: 7,
        });
        let bytes = encode_tiles(&batch).unwrap();
        assert_eq!(decode_tiles(&bytes), Ok(batch));
        let mut bad = bytes;
        bad[80] = 2;
        assert!(decode_tiles(&bad).is_err());
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dictionary_fanout_is_charged_before_expanding_the_references() {
        let mut batch = sample();
        batch.cells.truncate(1);
        batch.cells[0].text = "x".repeat(200_000);
        batch.removed.clear();
        let mut bytes = encode_tiles(&batch).unwrap();
        let mut reader = Reader(&bytes[86..]);
        assert_eq!(reader.u32().unwrap(), 1);
        reader.text().unwrap();
        reader.text().unwrap();
        let records = bytes.len() - reader.0.len();
        bytes.truncate(records);
        write_u32(&mut bytes, MAX_TILE_RECORDS);
        for row in 1..=MAX_TILE_RECORDS as u32 {
            write_key(&mut bytes, Key(0, 0, row, 1));
            bytes.extend_from_slice(&0u32.to_le_bytes());
        }
        write_u32(&mut bytes, 0);
        assert!(bytes.len() < MAX_TILE_BYTES);
        // Three expanded records already exceed the retained budget. Refuse there even
        // when the rest of the declared record sequence has not arrived: checking only
        // after decoding the entire batch would instead read on and report truncation.
        assert_eq!(
            decode_tiles(&bytes[..records + 4 + 3 * 20]).unwrap_err(),
            "Decoded tile exceeds its retention limit"
        );
        assert_eq!(
            decode_tiles(&bytes).unwrap_err(),
            "Decoded tile exceeds its retention limit"
        );
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn byte_and_retained_record_budgets_apply_before_encoding_or_decoding() {
        assert!(decode_tiles(&vec![0; MAX_TILE_BYTES + 1]).is_err());
        let mut batch = sample();
        batch.cells[0].text = "x".repeat(MAX_TILE_BYTES);
        assert!(encode_tiles(&batch).is_err());
        let mut batch = sample();
        batch.removed = vec![Key(0, 0, 3, 1); MAX_TILE_RECORDS];
        assert!(encode_tiles(&batch).is_err());
    }
}
