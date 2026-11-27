//! Frozen grid tile atoms over the shared binary codec.
pub use calc_skin::tiles::TILE_TAG;
pub use dna_calc_model::tiles::*;

pub fn encode_tiles(batch: &TileBatch) -> Result<Vec<u8>, String> {
    calc_skin::tiles::encode_tiles(batch)
}

pub fn decode_tiles(bytes: &[u8]) -> Result<TileBatch, String> {
    calc_skin::tiles::decode_tiles(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Cell, FormatShown, Geometry, Key, Provenance, Sheet};

    struct Reader<'a>(&'a [u8]);

    impl<'a> Reader<'a> {
        fn u32(&mut self) -> Result<u32, String> {
            let (value, rest) = self.0.split_at(4);
            self.0 = rest;
            Ok(u32::from_le_bytes(value.try_into().unwrap()))
        }

        fn text(&mut self) -> Result<(), String> {
            let len = self.u32()? as usize;
            self.0 = &self.0[len..];
            Ok(())
        }
    }

    fn write_u32(bytes: &mut Vec<u8>, value: usize) {
        bytes.extend_from_slice(&(value as u32).to_le_bytes());
    }

    fn write_key(bytes: &mut Vec<u8>, key: Key) {
        for field in [key.0, key.1, key.2, key.3] {
            bytes.extend_from_slice(&field.to_le_bytes());
        }
    }
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
            spill: Some(crate::SpillView::Filled(crate::Area(
                Key(0, 0, 1, 1),
                Key(0, 0, 3, 2),
            ))),
            exact: None,
            number_as_text: false,
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
        assert!(bytes.starts_with(b"DNATILE2"));
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
