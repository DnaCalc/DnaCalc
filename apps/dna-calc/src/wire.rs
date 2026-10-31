//! The current JSON wire codec and worker conversion adapters. Contracts live in the model.
pub use dna_calc_model::projection::*;
use serde::{Serialize,de::DeserializeOwned};

    pub fn geometry_of(geometry: &calc_host::SheetGeometry) -> Geometry {
        Geometry {
            top_left: None,
            pane_top_left: None,
            active_cell: None,
            active_pane: 0,
            split: None,
            frozen_rows: 0,
            frozen_columns: 0,
            split_window: false,
            default_row_height: geometry.default_row_height,
            default_column_width: geometry.default_column_width,
            base_column_width: geometry.base_column_width,
            columns: geometry
                .columns
                .iter()
                .map(|span| (span.first, span.last, span.width, span.hidden))
                .collect(),
            rows: geometry
                .rows
                .iter()
                .map(|(&row, format)| (row, format.height, format.hidden))
                .collect(),
        }
    }


    pub fn host_paste_mode(mode: PasteMode) -> calc_host::PasteMode {
        match mode {
            PasteMode::All => calc_host::PasteMode::All,
            PasteMode::Values => calc_host::PasteMode::Values,
            PasteMode::Formulas => calc_host::PasteMode::Formulas,
            PasteMode::Formats => calc_host::PasteMode::Formats,
        }
    }

    pub fn cell_of(
        shown: &CellShown,
        input: String,
        why: String,
        projected: &calc_host::ProjectedCell,
    ) -> Cell {
        Cell {
            style: shown.style.clone(),
            at: Key::of(shown.at),
            text: shown.text.clone(),
            kind: shown.kind.to_owned(),
            provenance: shown.provenance,
            formula: shown.formula,
            format: shown.format,
            color: shown.color.clone(),
            conditional: shown.conditional.clone(),
            input,
            why,
            owner: crate::cells::refusal_owner(projected),
            error_meaning: crate::cells::error_meaning(projected),
        }
    }


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
        for message in [
            FromServer::Started,
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
            FromServer::Event(Event::Cells(Cells {
                id: 1,
                seq: 2,
                reset: false,
                cells: vec![cell.clone()],
                emptied: vec![at],
            })),
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
