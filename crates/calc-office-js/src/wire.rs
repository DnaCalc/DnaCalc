//! The batch wire protocol. Values are lossless: `-0.0` keeps its bits, text keeps unpaired
//! surrogates, and Empty, Null and Error stay apart (the client adapter alone folds them into
//! Office.js's `any[][]`).

use calc_object_model::{AutoArray, AutoValue};
use calc_value::ExcelText;
pub use dna_calc_protocol::automation::*;

pub fn to_wire(
    value: &AutoValue,
    handle_of: &mut dyn FnMut(&calc_object_model::ObjectRef) -> u64,
) -> WireValue {
    match value {
        AutoValue::Empty => WireValue::Empty,
        AutoValue::Null => WireValue::Null,
        AutoValue::Missing => WireValue::Missing,
        AutoValue::Bool(v) => WireValue::Bool { v: *v },
        AutoValue::I2(v) => WireValue::I4 { v: i32::from(*v) },
        AutoValue::I4(v) => WireValue::I4 { v: *v },
        AutoValue::R4(v) => WireValue::r8(f64::from(*v)),
        AutoValue::R8(v) | AutoValue::Date(v) => WireValue::r8(*v),
        AutoValue::Currency(v) => WireValue::r8(*v as f64 / 10_000.0),
        AutoValue::Text(text) => {
            let units = text.utf16_code_units();
            match String::from_utf16(units) {
                Ok(v) => WireValue::Text { v },
                Err(_) => WireValue::Text16 {
                    units: units.to_vec(),
                },
            }
        }
        AutoValue::Error(v) => WireValue::Error { v: *v },
        AutoValue::Object(object) => WireValue::Object {
            h: handle_of(object),
        },
        AutoValue::Foreign(h) => WireValue::Object { h: *h },
        AutoValue::Array(array) => WireValue::Array {
            rows: array.rows,
            cols: array.columns,
            v: array.values.iter().map(|v| to_wire(v, handle_of)).collect(),
        },
    }
}

pub fn from_wire(
    value: &WireValue,
    object_of: &dyn Fn(u64) -> Option<calc_object_model::ObjectRef>,
) -> Result<AutoValue, String> {
    Ok(match value {
        WireValue::Empty => AutoValue::Empty,
        WireValue::Null => AutoValue::Null,
        WireValue::Missing => AutoValue::Missing,
        WireValue::Bool { v } => AutoValue::Bool(*v),
        WireValue::I4 { v } => AutoValue::I4(*v),
        WireValue::R8 { bits } => AutoValue::R8(f64::from_bits(
            bits.len()
                .eq(&16)
                .then(|| u64::from_str_radix(bits, 16).ok())
                .flatten()
                .ok_or_else(|| format!("bad r8 bits {bits:?}"))?,
        )),
        WireValue::Text { v } => AutoValue::text(v),
        WireValue::Text16 { units } => {
            AutoValue::Text(ExcelText::from_utf16_code_units(units.clone()))
        }
        WireValue::Error { v } => AutoValue::Error(*v),
        WireValue::Object { h } => {
            AutoValue::Object(object_of(*h).ok_or_else(|| format!("unknown handle {h}"))?)
        }
        WireValue::Array { rows, cols, v } => {
            if (*rows as usize).checked_mul(*cols as usize) != Some(v.len()) {
                return Err("array size does not match rows x cols".into());
            }
            AutoValue::Array(Box::new(AutoArray {
                rows: *rows,
                columns: *cols,
                lower: (1, 1),
                values: v
                    .iter()
                    .map(|x| from_wire(x, object_of))
                    .collect::<Result<_, _>>()?,
            }))
        }
    })
}
