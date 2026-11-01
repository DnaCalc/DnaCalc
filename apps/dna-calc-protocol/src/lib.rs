//! JSON controls and bounded binary tile records shared by page and worker.
pub mod tiles;
pub use dna_calc_model::projection::*;
use serde::{Serialize, de::DeserializeOwned};
pub use tiles::*;
/// A message as the JSON text `postMessage` carries.
pub fn encode<T: Serialize>(message: &T) -> String {
    serde_json::to_string(message).expect("a wire message always encodes")
}

/// A message from its JSON text.
pub fn decode<T: DeserializeOwned>(text: &str) -> Result<T, String> {
    serde_json::from_str(text).map_err(|e| format!("not a message: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn messages_read_back_as_written() {
        let at = Key(3, 1, 7, 2);
        let request = ToServer {
            id: 7,
            request: Request::Intent(Intent::EnterCell {
                at,
                text: "=A1+0.1".to_owned(),
            }),
        };
        assert_eq!(decode::<ToServer>(&encode(&request)), Ok(request));
        let cell = Cell {
            style: Default::default(),
            at,
            text: "0.30000000000000004".to_owned(),
            kind: "number".to_owned(),
            provenance: Provenance::Calculated,
            formula: true,
            format: FormatShown::General,
            color: None,
            conditional: Some(ConditionalShown {
                bar: Some((0.25, "#638ec6".to_owned(), true)),
                ..ConditionalShown::default()
            }),
            input: "=A1+0.1".to_owned(),
            why: "calculated".to_owned(),
            owner: None,
            error_meaning: None,
        };
        let stamp = TileStamp {
            calculation_through: None,
            id: 1,
            workbook: 0,
            transaction: 2,
            base: 1,
            next: 2,
            revision: 3,
            geometry_version: 3,
            style_version: 3,
            dictionary: 2,
            reset: false,
        };
        let tiles = TileBatch {
            stamp,
            fragment: 0,
            final_fragment: true,
            geometry: vec![(Sheet(3, 1), Geometry::default())],
            cells: vec![cell.clone()],
            removed: vec![at],
        };
        assert_eq!(decode_tiles(&encode_tiles(&tiles).unwrap()), Ok(tiles));
        for message in [
            FromServer::Started,
            FromServer::Event(Event::TileAdmission(stamp)),
            FromServer::Event(Event::TileCompletion(stamp.barrier())),
            FromServer::Reply {
                id: 7,
                reply: Reply::Pong {
                    version: "0.1.0".to_owned(),
                },
            },
            FromServer::Failed {
                id: None,
                message: "no".to_owned(),
            },
        ] {
            assert_eq!(decode::<FromServer>(&encode(&message)), Ok(message));
        }
        assert_eq!(cell.shown().map(|shown| shown.kind), Some("number"));
        assert!(decode::<ToServer>("{\"id\":1}").is_err());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn numbers_cross_exactly() {
        let geometry = Geometry {
            frozen_rows: 1,
            frozen_columns: 2,
            split_window: false,
            default_row_height: Some(0.1 + 0.2),
            default_column_width: Some(8.43),
            base_column_width: None,
            columns: vec![(1, 3, Some(1.0 / 3.0), false)],
            rows: vec![(2, Some(f64::MIN_POSITIVE), true)],
            ..Default::default()
        };
        let back: Reply = decode(&encode(&Reply::Geometry(geometry.clone()))).unwrap();
        assert_eq!(back, Reply::Geometry(geometry));
    }
}

/// A message's text, and the bytes beside it (`[text, ArrayBuffer]`).
#[cfg(target_arch = "wasm32")]
pub fn unpack(data: wasm_bindgen::JsValue) -> Option<(String, Option<Vec<u8>>)> {
    if let Some(text) = data.as_string() {
        return Some((text, None));
    }
    let pair: js_sys::Array = wasm_bindgen::JsCast::dyn_into(data).ok()?;
    let text = pair.get(0).as_string()?;
    let bytes = js_sys::Uint8Array::new(&pair.get(1)).to_vec();
    Some((text, Some(bytes)))
}
