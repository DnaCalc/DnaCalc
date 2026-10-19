//! Typed values: their canonical JSON form and the type-aware comparison every verdict uses.
//!
//! A value is one member naming its kind: `{"number": "<16 hex digits>"}` (the binary64 bits),
//! `{"text": "<4 hex digits per UTF-16 code unit>"}`, `{"logical": true}`, `{"error": "div0"}`
//! (the wire names calc-doc-model uses), `{"empty": true}`, or
//! `{"array": {"rows": r, "columns": c, "cells": [<scalar>, ...]}}` in row-major order. A number
//! is never written in decimal, so reading a value never rounds.
//!
//! Two values are equal only when they have the same kind and: the same bits for numbers (so +0
//! and -0 differ, and so do two NaNs with different payloads), the same code units for text, the
//! same logical, the same error code, or the same shape and equal cells for arrays.

use crate::DecodeError;
use crate::json::{self, Node, Out};
use crate::value::{ArrayShape, CalcArray, CalcValue, CoreValue, ExcelText, WorksheetErrorCode};

/// How a calculated value differs from the expectation it was compared with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MismatchClass {
    /// Both numbers with different bits; `ulp` counts the representable doubles between them when
    /// both are finite (0 for +0 against -0), and is `None` when either is infinite or NaN.
    Number { ulp: Option<u64> },
    /// Both text, differing in at least one UTF-16 code unit or in length.
    Text,
    /// Both logicals, differing.
    Logical,
    /// Both worksheet errors, with different codes.
    Error {
        expected: WorksheetErrorCode,
        actual: WorksheetErrorCode,
    },
    /// Different kinds of value (for example a number expected and an error calculated).
    Kind {
        expected: &'static str,
        actual: &'static str,
    },
    /// Both arrays, with different shapes.
    ArrayShape {
        expected: ArrayShape,
        actual: ArrayShape,
    },
    /// Both arrays of one shape; the first cell in row-major order that differs, 0-based.
    ArrayCell {
        row: usize,
        column: usize,
        class: Box<MismatchClass>,
    },
}

impl MismatchClass {
    pub(crate) fn out(&self) -> Out {
        match self {
            MismatchClass::Number { ulp } => Out::Object(vec![
                ("class", Out::text("number")),
                ("ulp", ulp.map_or(Out::Null, Out::Unsigned)),
            ]),
            MismatchClass::Text => Out::Object(vec![("class", Out::text("text"))]),
            MismatchClass::Logical => Out::Object(vec![("class", Out::text("logical"))]),
            MismatchClass::Error { expected, actual } => Out::Object(vec![
                ("class", Out::text("error")),
                ("expected", Out::text(error_name(*expected))),
                ("actual", Out::text(error_name(*actual))),
            ]),
            MismatchClass::Kind { expected, actual } => Out::Object(vec![
                ("class", Out::text("kind")),
                ("expected", Out::text(*expected)),
                ("actual", Out::text(*actual)),
            ]),
            MismatchClass::ArrayShape { expected, actual } => Out::Object(vec![
                ("class", Out::text("array_shape")),
                ("expected", shape_out(*expected)),
                ("actual", shape_out(*actual)),
            ]),
            MismatchClass::ArrayCell { row, column, class } => Out::Object(vec![
                ("class", Out::text("array_cell")),
                ("row", Out::Unsigned(*row as u64)),
                ("column", Out::Unsigned(*column as u64)),
                ("cell", class.out()),
            ]),
        }
    }
}

fn shape_out(shape: ArrayShape) -> Out {
    Out::Object(vec![
        ("rows", Out::Unsigned(shape.rows as u64)),
        ("columns", Out::Unsigned(shape.cols as u64)),
    ])
}

/// The wire name of each worksheet error code (the names calc-doc-model's wire format uses).
pub(crate) fn error_name(code: WorksheetErrorCode) -> &'static str {
    match code {
        WorksheetErrorCode::Null => "null",
        WorksheetErrorCode::Div0 => "div0",
        WorksheetErrorCode::Value => "value",
        WorksheetErrorCode::Ref => "ref",
        WorksheetErrorCode::Name => "name",
        WorksheetErrorCode::Num => "num",
        WorksheetErrorCode::NA => "na",
        WorksheetErrorCode::Busy => "busy",
        WorksheetErrorCode::GettingData => "getting_data",
        WorksheetErrorCode::Spill => "spill",
        WorksheetErrorCode::Calc => "calc",
        WorksheetErrorCode::Field => "field",
        WorksheetErrorCode::Blocked => "blocked",
        WorksheetErrorCode::Connect => "connect",
    }
}

/// The literal Excel writes for each worksheet error code in a saved cell cache (`<v>` of a cell
/// with `t="e"`).
pub(crate) fn error_literal(code: WorksheetErrorCode) -> &'static str {
    match code {
        WorksheetErrorCode::Null => "#NULL!",
        WorksheetErrorCode::Div0 => "#DIV/0!",
        WorksheetErrorCode::Value => "#VALUE!",
        WorksheetErrorCode::Ref => "#REF!",
        WorksheetErrorCode::Name => "#NAME?",
        WorksheetErrorCode::Num => "#NUM!",
        WorksheetErrorCode::NA => "#N/A",
        WorksheetErrorCode::Busy => "#BUSY!",
        WorksheetErrorCode::GettingData => "#GETTING_DATA",
        WorksheetErrorCode::Spill => "#SPILL!",
        WorksheetErrorCode::Calc => "#CALC!",
        WorksheetErrorCode::Field => "#FIELD!",
        WorksheetErrorCode::Blocked => "#BLOCKED!",
        WorksheetErrorCode::Connect => "#CONNECT!",
    }
}

pub(crate) fn error_from_name(name: &str) -> Option<WorksheetErrorCode> {
    WorksheetErrorCode::all()
        .into_iter()
        .find(|code| error_name(*code) == name)
}

pub(crate) fn error_from_literal(literal: &str) -> Option<WorksheetErrorCode> {
    WorksheetErrorCode::all()
        .into_iter()
        .find(|code| error_literal(*code) == literal)
}

/// The name of a value's kind, as reports and mismatch classes write it.
pub(crate) fn kind_name(core: &CoreValue) -> &'static str {
    match core {
        CoreValue::Number(_) => "number",
        CoreValue::Text(_) => "text",
        CoreValue::Logical(_) => "logical",
        CoreValue::Error(_) => "error",
        CoreValue::Empty => "empty",
        CoreValue::Missing => "missing",
        CoreValue::Array(_) => "array",
        CoreValue::Reference(_) => "reference",
    }
}

const VALUE_KINDS: [&str; 6] = ["number", "text", "logical", "error", "empty", "array"];

/// A value in its canonical form; arrays only when `allow_array`.
fn decode_scalar_or_array(
    node: &Node,
    path: &str,
    allow_array: bool,
) -> Result<CalcValue, DecodeError> {
    let (kind, inner) = json::one_member(node, path, &VALUE_KINDS)?;
    let at = format!("{path}.{kind}");
    match kind {
        "number" => {
            let digits = json::text(inner, &at)?;
            let bits = json::parse_bits(digits).ok_or_else(|| {
                DecodeError::new(
                    &at,
                    format!("{digits:?} is not 16 lowercase hex digits (the binary64 bits)"),
                )
            })?;
            Ok(CalcValue::number(f64::from_bits(bits)))
        }
        "text" => {
            let digits = json::text(inner, &at)?;
            let units = json::parse_units(digits).ok_or_else(|| {
                DecodeError::new(&at, "is not 4 lowercase hex digits per UTF-16 code unit")
            })?;
            Ok(CalcValue::text(ExcelText::from_utf16_code_units(units)))
        }
        "logical" => Ok(CalcValue::logical(json::boolean(inner, &at)?)),
        "error" => {
            let name = json::text(inner, &at)?;
            let code = error_from_name(name)
                .ok_or_else(|| DecodeError::new(&at, format!("{name:?} names no error code")))?;
            Ok(CalcValue::error(code))
        }
        "empty" => {
            if !json::boolean(inner, &at)? {
                return Err(DecodeError::new(&at, "must be true"));
            }
            Ok(CalcValue::empty())
        }
        _ => {
            if !allow_array {
                return Err(DecodeError::new(
                    &at,
                    "is an array inside an array; array cells are scalars",
                ));
            }
            let fields = json::Fields::new(inner, &at, &["rows", "columns", "cells"])?;
            let rows = json::unsigned(fields.required("rows")?, &fields.at("rows"))?;
            let columns = json::unsigned(fields.required("columns")?, &fields.at("columns"))?;
            let cells_at = fields.at("cells");
            let cells = json::array(fields.required("cells")?, &cells_at)?
                .iter()
                .enumerate()
                .map(|(i, cell)| decode_scalar_or_array(cell, &format!("{cells_at}[{i}]"), false))
                .collect::<Result<Vec<_>, _>>()?;
            let shape = ArrayShape {
                rows: usize::try_from(rows)
                    .map_err(|_| DecodeError::new(&fields.at("rows"), "is too large"))?,
                cols: usize::try_from(columns)
                    .map_err(|_| DecodeError::new(&fields.at("columns"), "is too large"))?,
            };
            let count = cells.len();
            let array = CalcArray::new(shape, cells).ok_or_else(|| {
                DecodeError::new(
                    &at,
                    format!(
                        "declares {rows} rows by {columns} columns (neither may be 0) and holds {count} cells"
                    ),
                )
            })?;
            Ok(CalcValue::array(array))
        }
    }
}

pub(crate) fn decode_value(node: &Node, path: &str) -> Result<CalcValue, DecodeError> {
    decode_scalar_or_array(node, path, true)
}

/// A value in its canonical form. A kind the canonical form cannot hold (an omitted argument or a
/// reference, which no cell holds) is written as `{"unrepresentable": "<kind>"}`.
pub(crate) fn value_out(value: &CalcValue) -> Out {
    let one = |kind: &'static str, inner: Out| Out::Object(vec![(kind, inner)]);
    match value.core() {
        CoreValue::Number(number) => one("number", Out::text(json::bits_text(number.to_bits()))),
        CoreValue::Text(text) => one("text", Out::text(json::units_text(text.utf16_code_units()))),
        CoreValue::Logical(logical) => one("logical", Out::Bool(*logical)),
        CoreValue::Error(code) => one("error", Out::text(error_name(*code))),
        CoreValue::Empty => one("empty", Out::Bool(true)),
        CoreValue::Array(array) => one(
            "array",
            Out::Object(vec![
                ("rows", Out::Unsigned(array.shape().rows as u64)),
                ("columns", Out::Unsigned(array.shape().cols as u64)),
                (
                    "cells",
                    Out::Array(array.iter_row_major().map(value_out).collect()),
                ),
            ]),
        ),
        other @ (CoreValue::Missing | CoreValue::Reference(_)) => {
            one("unrepresentable", Out::text(kind_name(other)))
        }
    }
}

/// The position of `bits` on the line of binary64 values in numeric order, with +0 and -0 both
/// at 0, so that the difference of two positions counts the representable doubles between them.
fn ordered(bits: u64) -> i128 {
    let magnitude = i128::from(bits & 0x7fff_ffff_ffff_ffff);
    if bits >> 63 == 1 {
        -magnitude
    } else {
        magnitude
    }
}

/// The number of representable doubles between two finite numbers; `None` when either is
/// infinite or NaN.
pub(crate) fn ulp_distance(a: f64, b: f64) -> Option<u64> {
    if !a.is_finite() || !b.is_finite() {
        return None;
    }
    u64::try_from((ordered(a.to_bits()) - ordered(b.to_bits())).unsigned_abs()).ok()
}

/// How `actual` differs from `expected`, or `None` when they are equal (see the module
/// documentation for what equal means). A callable compares as its worksheet-visible core.
pub(crate) fn compare(expected: &CalcValue, actual: &CalcValue) -> Option<MismatchClass> {
    match (expected.core(), actual.core()) {
        (CoreValue::Number(e), CoreValue::Number(a)) => {
            (e.to_bits() != a.to_bits()).then(|| MismatchClass::Number {
                ulp: ulp_distance(*e, *a),
            })
        }
        (CoreValue::Text(e), CoreValue::Text(a)) => {
            (e.utf16_code_units() != a.utf16_code_units()).then_some(MismatchClass::Text)
        }
        (CoreValue::Logical(e), CoreValue::Logical(a)) => {
            (e != a).then_some(MismatchClass::Logical)
        }
        (CoreValue::Error(e), CoreValue::Error(a)) => (e != a).then_some(MismatchClass::Error {
            expected: *e,
            actual: *a,
        }),
        (CoreValue::Empty, CoreValue::Empty) => None,
        (CoreValue::Array(e), CoreValue::Array(a)) => {
            if e.shape() != a.shape() {
                return Some(MismatchClass::ArrayShape {
                    expected: e.shape(),
                    actual: a.shape(),
                });
            }
            let columns = e.shape().cols;
            e.iter_row_major()
                .zip(a.iter_row_major())
                .enumerate()
                .find_map(|(i, (e, a))| {
                    compare(e, a).map(|class| MismatchClass::ArrayCell {
                        row: i / columns,
                        column: i % columns,
                        class: Box::new(class),
                    })
                })
        }
        (e, a) => Some(MismatchClass::Kind {
            expected: kind_name(e),
            actual: kind_name(a),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<CalcValue, DecodeError> {
        decode_value(&json::parse(text.as_bytes(), "value").unwrap(), "value")
    }

    fn number(bits: u64) -> CalcValue {
        CalcValue::number(f64::from_bits(bits))
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn canonical_values_round_trip_exactly() {
        for text in [
            r#"{"number":"8000000000000000"}"#,
            r#"{"number":"7ff8000000000001"}"#,
            r#"{"text":"0041d83d"}"#,
            r#"{"logical":false}"#,
            r#"{"error":"getting_data"}"#,
            r#"{"empty":true}"#,
            r#"{"array":{"rows":1,"columns":2,"cells":[{"number":"3ff0000000000000"},{"error":"na"}]}}"#,
        ] {
            let value = parse(text).unwrap();
            assert_eq!(value_out(&value).compact(), text, "round trip of {text}");
        }
        for code in WorksheetErrorCode::all() {
            assert_eq!(error_from_name(error_name(code)), Some(code));
            assert_eq!(error_from_literal(error_literal(code)), Some(code));
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn non_canonical_values_are_refused() {
        for text in [
            r#"{"number":1}"#,
            r#"{"number":"1"}"#,
            r#"{"text":"A"}"#,
            r##"{"error":"#DIV/0!"}"##,
            r#"{"empty":false}"#,
            r#"{"number":"3ff0000000000000","text":""}"#,
            r#"{"array":{"rows":2,"columns":1,"cells":[{"empty":true}]}}"#,
            r#"{"array":{"rows":0,"columns":0,"cells":[]}}"#,
            r#"{"array":{"rows":1,"columns":1,"cells":[{"array":{"rows":1,"columns":1,"cells":[{"empty":true}]}}]}}"#,
            r#"{"date":"2026"}"#,
        ] {
            assert!(parse(text).is_err(), "{text} was accepted");
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn comparison_is_type_aware_and_bit_exact() {
        let one = 1.0f64.to_bits();
        assert_eq!(compare(&number(one), &number(one)), None);
        assert_eq!(
            compare(&number(one), &number(one + 3)),
            Some(MismatchClass::Number { ulp: Some(3) })
        );
        assert_eq!(
            compare(&number(0), &number(1 << 63)),
            Some(MismatchClass::Number { ulp: Some(0) })
        );
        assert_eq!(
            compare(&number(1), &number(1 | (1 << 63))),
            Some(MismatchClass::Number { ulp: Some(2) })
        );
        assert_eq!(
            compare(&number(one), &CalcValue::number(f64::INFINITY)),
            Some(MismatchClass::Number { ulp: None })
        );
        assert_eq!(
            compare(
                &CalcValue::number(2.0),
                &CalcValue::error(WorksheetErrorCode::Num)
            ),
            Some(MismatchClass::Kind {
                expected: "number",
                actual: "error"
            })
        );
        assert_eq!(
            compare(
                &CalcValue::error(WorksheetErrorCode::Div0),
                &CalcValue::error(WorksheetErrorCode::Value)
            ),
            Some(MismatchClass::Error {
                expected: WorksheetErrorCode::Div0,
                actual: WorksheetErrorCode::Value
            })
        );
        assert_eq!(
            compare(
                &CalcValue::error(WorksheetErrorCode::Name),
                &CalcValue::error(WorksheetErrorCode::Name)
            ),
            None
        );
        // An error against a non-error is a kind mismatch; the members name which side holds it.
        assert_eq!(
            compare(
                &CalcValue::error(WorksheetErrorCode::Div0),
                &CalcValue::number(1.0)
            ),
            Some(MismatchClass::Kind {
                expected: "error",
                actual: "number"
            })
        );
        // A calculated value of a kind no cell holds (an omitted argument) is still compared.
        assert_eq!(
            compare(&CalcValue::number(1.0), &CalcValue::missing()),
            Some(MismatchClass::Kind {
                expected: "number",
                actual: "missing"
            })
        );
        assert_eq!(
            compare(
                &CalcValue::error(WorksheetErrorCode::Value),
                &CalcValue::missing()
            ),
            Some(MismatchClass::Kind {
                expected: "error",
                actual: "missing"
            })
        );
        let text = |s: &str| CalcValue::text(ExcelText::from_interop_assignment(s));
        assert_eq!(compare(&text("a"), &text("a")), None);
        assert_eq!(compare(&text("a"), &text("A")), Some(MismatchClass::Text));
        assert_eq!(
            compare(&CalcValue::logical(true), &CalcValue::logical(false)),
            Some(MismatchClass::Logical)
        );
        assert_eq!(
            compare(&CalcValue::empty(), &CalcValue::number(0.0)),
            Some(MismatchClass::Kind {
                expected: "empty",
                actual: "number"
            })
        );
        assert_eq!(
            compare(&CalcValue::number(1.0), &CalcValue::logical(true)),
            Some(MismatchClass::Kind {
                expected: "number",
                actual: "logical"
            })
        );
        assert_eq!(
            compare(&text("1"), &CalcValue::number(1.0)),
            Some(MismatchClass::Kind {
                expected: "text",
                actual: "number"
            })
        );
        let row =
            |values: Vec<CalcValue>| CalcValue::array(CalcArray::from_rows(vec![values]).unwrap());
        let column = |values: Vec<CalcValue>| {
            CalcValue::array(
                CalcArray::from_rows(values.into_iter().map(|v| vec![v]).collect()).unwrap(),
            )
        };
        let a = row(vec![CalcValue::number(1.0), CalcValue::number(2.0)]);
        assert_eq!(compare(&a, &a.clone()), None);
        assert_eq!(
            compare(
                &a,
                &column(vec![CalcValue::number(1.0), CalcValue::number(2.0)])
            ),
            Some(MismatchClass::ArrayShape {
                expected: ArrayShape { rows: 1, cols: 2 },
                actual: ArrayShape { rows: 2, cols: 1 }
            })
        );
        assert_eq!(
            compare(
                &a,
                &row(vec![CalcValue::number(1.0), CalcValue::logical(true)])
            ),
            Some(MismatchClass::ArrayCell {
                row: 0,
                column: 1,
                class: Box::new(MismatchClass::Kind {
                    expected: "number",
                    actual: "logical"
                })
            })
        );
    }
}
