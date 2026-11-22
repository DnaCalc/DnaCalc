//! Browser adapter for the Playground's number-to-text explorer.

#[cfg(target_arch = "wasm32")]
use calc_func::{RoundingPath, inspect_number_text};
#[cfg(target_arch = "wasm32")]
use serde_json::json;
#[cfg(target_arch = "wasm32")]
use std::cmp::Ordering;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

fn main() {}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn inspect_number(input: &str) -> Option<String> {
    let number: f64 = input.trim().parse().ok()?;
    let inspection = inspect_number_text(number)?;
    let rounding = inspection.rounding.map(|step| {
        let path = match step.path {
            RoundingPath::IntegerSplit { whole } => json!({ "whole": whole }),
            RoundingPath::Scaled {
                decimal_shift,
                significand,
                right_shift,
                corrected,
            } => json!({
                "decimalShift": decimal_shift,
                "significand": hex(significand),
                "rightShift": right_shift,
                "corrected": corrected,
            }),
        };
        let decision = match step.decision {
            Ordering::Greater => "up",
            Ordering::Equal => "keep",
            Ordering::Less => "down",
        };
        json!({
            "path": path,
            "binaryExponent": step.binary_exponent,
            "fraction": hex(step.fraction),
            "places": step.places,
            "half": step.half,
            "sum": hex(step.sum),
            "carry": step.carry,
            "truncated": step.truncated_digits,
            "inputExponent": step.input_exponent,
            "nextDigit": step.next_digit,
            "exactTie": step.exact_tie,
            "decision": decision,
            "rounded": step.rounded_digits,
            "roundedExponent": step.rounded_exponent,
        })
    });
    Some(
        json!({
            "bits": hex(number.to_bits()),
            "subnormal": number.is_subnormal(),
            "text": inspection.text,
            "rounding": rounding,
        })
        .to_string(),
    )
}

#[cfg(target_arch = "wasm32")]
fn hex(bits: u64) -> String {
    format!("0x{bits:016X}")
}
