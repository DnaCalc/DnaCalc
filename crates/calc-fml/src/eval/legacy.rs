//! Where a legacy formula takes one value by implicit intersection, and where it lifts.
//!
//! A legacy formula (see [`super::FormulaForm::Legacy`]) evaluates a range or an array that meets
//! a single-value place (a parameter that takes one value, or an operator's operand) by the place
//! the argument holds in the formula. Excel 365 shows the result as an `@` it writes into the
//! formula's `Formula2` text. Measured on Excel 16.0 through COM on 2026-10-23, each formula
//! entered through `Range.Formula` in row 2 over A1:A3 = 1, 2, 3 and B1:B3 = 10, 20, 30, then read
//! back through `Range.Formula2` and `Range.Value2`:
//!
//! | entered | `Formula2` | value |
//! |---|---|---|
//! | `=A1:A3*1` | `=@A1:A3*1` | 2 |
//! | `={1,2}*2` | `=@{1,2}*2` | 2 |
//! | `=ABS({-1,-2})` | `=ABS(@{-1,-2})` | 1 |
//! | `=IFERROR(A1:A3,0)` | `=IFERROR(@A1:A3,0)` | 2 |
//! | `=SUM(A1:A3*2)` | `=SUM(@A1:A3*2)` | 4 |
//! | `=SUM(ABS(A1:A3))` | `=SUM(ABS(@A1:A3))` | 2 |
//! | `=SUM({1,2}*2)` | unchanged | 6 |
//! | `=SUM(ABS({-1,-2}))` | unchanged | 3 |
//! | `=SUM({1,2}*A1:A2)` | `=SUM({1,2}*@A1:A2)` | 6 |
//! | `=SUM(IF(TRUE,A1:A3*1))` | `=SUM(IF(TRUE,@A1:A3*1))` | 2 |
//! | `=SUM(IF(TRUE,A1:A3))` | unchanged | 6 |
//! | `=COUNT(A1:A3*1)`, `=MAX(A1:A3*1)`, `=AND(A1:A3>0)` | `@` before `A1:A3` | 1, 2, TRUE |
//! | `=MATCH(2,A1:A3*1,0)`, `=VLOOKUP(2,A1:B3*1,2,0)` | `@` before the range | #N/A, #VALUE! |
//! | `=SUMPRODUCT(A1:A3*2)`, `=SUMPRODUCT((A1:A3>1)*B1:B3)` | unchanged | 12, 50 |
//! | `=SUMPRODUCT(ABS(A1:A3))` | unchanged | 6 |
//! | `=SUMPRODUCT(IF(A1:A3>1,1,0))`, `=SUMPRODUCT(IFERROR(A1:A3,0))` | `@` before `A1:A3` | 1, 2 |
//! | `=LOOKUP(2,1/(A1:A3=2),B1:B3)` | unchanged | 20 |
//! | `=INDEX(A1:A3*2,2)`, `=SUM(INDEX(A1:A3*2,0))` | `@` before INDEX at the root only | 4, 12 |
//! | `=ROWS(A1:A3*1)` | unchanged | 3 |
//!
//! From these rows a place is one of four kinds ([`Place`]). The formula's root, and every
//! single-value parameter and operand on the way down from it, is a [`Place::Value`]: a range
//! there gives the cell implicit intersection picks, and an array its first element. A parameter
//! that takes a reference and does not spill (SUM's, MATCH's lookup array) makes a
//! [`Place::Aggregate`] below it: a range is intersected there, while an array lifts. An array
//! parameter (SUMPRODUCT's, LOOKUP's vectors, INDEX's array, ROWS's) makes a [`Place::Array`] below
//! it, where both lift, down through operators and single-value parameters of eager functions;
//! IF, IFERROR and the other lazy functions intersect a range there again. Every other place is
//! [`Place::Unknown`], where a range or an array that meets a single-value place is refused,
//! since the measurements above do not say what Excel does there.

use super::Admission;
use calc_func::ParameterClass;

/// The kind of place an argument holds in a legacy formula. See the module documentation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Place {
    Value,
    Aggregate,
    Array,
    Unknown,
}

/// The parameters that take a reference and do not spill, by function: a range expression
/// written there is intersected. Each is either measured (see the module documentation) or of the
/// family of one that is (the aggregates' number and value lists).
const AGGREGATE: &[(&str, &[usize])] = &[
    ("AND", ALL),
    ("AVERAGE", ALL),
    ("AVERAGEA", ALL),
    ("COUNT", ALL),
    ("COUNTA", ALL),
    ("HLOOKUP", &[1]),
    ("MATCH", &[1]),
    ("MAX", ALL),
    ("MAXA", ALL),
    ("MIN", ALL),
    ("MINA", ALL),
    ("OR", ALL),
    ("PRODUCT", ALL),
    ("SUM", ALL),
    ("SUMSQ", ALL),
    ("VLOOKUP", &[1]),
];

/// The array parameters, by function: a range expression written there lifts. FILTER's include
/// is measured by the catalogue oracle's capture (F3 dc-728): `=_xlfn._xlws.FILTER(Data!A1:A3,
/// Data!A1:A3>1)` entered as a legacy formula gives 5, the first of FILTER's rows {5;3}.
const ARRAY: &[(&str, &[usize])] = &[
    ("COLUMNS", &[0]),
    ("FILTER", &[1]),
    ("INDEX", &[0]),
    ("LOOKUP", &[1, 2]),
    ("ROWS", &[0]),
    ("SUMPRODUCT", ALL),
];

/// Every parameter of a function.
const ALL: &[usize] = &[usize::MAX];

fn listed(table: &[(&str, &[usize])], spelling: &str, index: usize) -> bool {
    table.iter().any(|(name, positions)| {
        name.eq_ignore_ascii_case(spelling) && (*positions == ALL || positions.contains(&index))
    })
}

impl Place {
    /// How a call made at this place admits a range or an array given to a single-value
    /// parameter.
    pub(super) fn admission(self) -> Admission {
        match self {
            Place::Value => Admission::IntersectAll,
            Place::Aggregate => Admission::IntersectReferences,
            Place::Array => Admission::Lift,
            Place::Unknown => Admission::Refuse,
        }
    }

    /// The place of an operator's operand made at this place: the same place.
    pub(super) fn operand(self) -> Place {
        self
    }

    /// The place of argument `index` of a call of the catalogue function `spelling`, made at this
    /// place, whose parameter there is of `class`; `lazy` when the function's arguments are
    /// lazy (IF, IFERROR, IFNA, CHOOSE).
    pub(super) fn argument(
        self,
        spelling: &str,
        index: usize,
        class: Option<ParameterClass>,
        lazy: bool,
    ) -> Place {
        if self == Place::Unknown {
            return Place::Unknown;
        }
        if lazy {
            // IF's, IFERROR's and CHOOSE's arguments keep the place of the call, except in an
            // array place, where a range at their single-value parameters is intersected again.
            return match self {
                Place::Array => Place::Aggregate,
                other => other,
            };
        }
        match class {
            Some(ParameterClass::Value) => self,
            Some(ParameterClass::ValueOrReference | ParameterClass::Reference) | None => {
                if listed(ARRAY, spelling, index) {
                    match self {
                        Place::Value | Place::Aggregate => Place::Array,
                        _ => Place::Unknown,
                    }
                } else if listed(AGGREGATE, spelling, index) {
                    match self {
                        Place::Value | Place::Aggregate => Place::Aggregate,
                        _ => Place::Unknown,
                    }
                } else {
                    Place::Unknown
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn places_follow_the_measured_rows() {
        let r = Some(ParameterClass::ValueOrReference);
        let v = Some(ParameterClass::Value);
        // =SUM(A1:A3*2): intersected below SUM.
        assert_eq!(Place::Value.argument("SUM", 0, r, false), Place::Aggregate);
        assert_eq!(Place::Aggregate.admission(), Admission::IntersectReferences);
        // =SUMPRODUCT(ABS(A1:A3)): lifts below SUMPRODUCT, through ABS.
        let array = Place::Value.argument("SUMPRODUCT", 0, r, false);
        assert_eq!(array, Place::Array);
        assert_eq!(array.argument("ABS", 0, v, false), Place::Array);
        // =SUMPRODUCT(IF(A1:A3>1,1,0)): IF intersects again.
        assert_eq!(array.argument("IF", 0, v, true), Place::Aggregate);
        // =ABS({-1,-2}) at the root: intersected.
        assert_eq!(Place::Value.argument("ABS", 0, v, false), Place::Value);
        assert_eq!(Place::Value.admission(), Admission::IntersectAll);
        // A parameter not measured: refused.
        assert_eq!(
            Place::Value.argument("XLOOKUP", 1, r, false),
            Place::Unknown
        );
        assert_eq!(Place::Unknown.admission(), Admission::Refuse);
        // A function below an array place that is not measured there.
        assert_eq!(array.argument("SUM", 0, r, false), Place::Unknown);
        assert_eq!(Place::Value.argument("lookup", 2, r, false), Place::Array);
    }
}
