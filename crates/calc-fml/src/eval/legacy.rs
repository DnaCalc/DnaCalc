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
//! L3 (dc-m3u) measured more parameters the same way on 2026-10-23 (Excel 16.0.20430, `Range.Formula`
//! entry in rows 5 to 91 over the same A1:B3, `Range.Formula2` read back). Excel wrote no `@` before
//! the range expression at these array parameters: XLOOKUP's lookup and return arrays
//! (`=XLOOKUP(2,A1:A3*1,B1:B3)` 20, `=XLOOKUP(2,A1:A3,B1:B3*1)` 20), XMATCH's lookup array (2),
//! MMULT's (`=@MMULT(A1:A3*1,1)` 1), IRR's values (#NUM!), CORREL's, SLOPE's, INTERCEPT's,
//! FORECAST's known values, SUMX2MY2's, SUMXMY2's, COVAR's, PEARSON's, RSQ's and STEYX's arrays (1,
//! 10, 0, 20, -1386, 1134, 6.666667, 1, 1, 0), FREQUENCY's (`=@FREQUENCY(A1:A3*1,2)` 2) and MODE's
//! numbers (#N/A). It wrote `@` before the range expression, intersected, at these: MEDIAN's,
//! STDEV's, VAR's, STDEVP's, VARP's, MINA's, GEOMEAN's, HARMEAN's, DEVSQ's, AVEDEV's, KURT's and
//! SKEW's numbers, LARGE's and SMALL's array, PERCENTILE's, QUARTILE's, PERCENTRANK's and
//! TRIMMEAN's array, CONCAT's and TEXTJOIN's texts, NPV's values and TRANSPOSE's array
//! (`=@TRANSPOSE(@A1:A3*1)`); each is #VALUE! in those rows, below A1:A3. dc-10y measured every
//! parameter of the catalogue that takes a reference the same way on 2026-10-25 ([`AGGREGATE`]),
//! CHISQ.TEST's among them (no `@`: its ranges lift).
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
/// written there is intersected. dc-10y measured every parameter of the catalogue that takes a
/// reference, at its first positions (up to four), on Excel 16.0.20430 through COM on 2026-10-25:
/// the call with `A1:A3*1` there and 1 at its other arguments, entered through `Range.Formula`,
/// `Range.Formula2` read back; an `@` written before `A1:A3` puts the parameter here, none puts it
/// in [`ARRAY`]. A function of 250 parameters or more whose measured positions all agree takes
/// every position ([`ALL`]). A parameter whose call Excel refused to enter (one that takes only a
/// reference) is in neither.
const AGGREGATE: &[(&str, &[usize])] = &[
    ("AGGREGATE", &[3]),
    ("AND", ALL),
    ("AVEDEV", ALL),
    ("AVERAGE", ALL),
    ("AVERAGEA", ALL),
    ("BYCOL", &[1]),
    ("BYROW", &[1]),
    ("CHOOSECOLS", &[1, 2]),
    ("CHOOSEROWS", &[1, 2]),
    ("CONCAT", ALL),
    ("COUNT", ALL),
    ("COUNTA", ALL),
    ("DEVSQ", ALL),
    ("FILTER", &[2]),
    ("FVSCHEDULE", &[1]),
    ("GCD", ALL),
    ("GEOMEAN", ALL),
    ("HARMEAN", ALL),
    ("HLOOKUP", &[1]),
    ("IMPRODUCT", ALL),
    ("IMSUM", ALL),
    ("ISEVEN", &[0]),
    ("ISODD", &[0]),
    ("ISREF", &[0]),
    ("KURT", ALL),
    ("LARGE", &[0]),
    ("LCM", ALL),
    ("MAKEARRAY", &[2]),
    ("MAP", &[1, 2]),
    ("MATCH", &[1]),
    ("MAX", ALL),
    ("MAXA", ALL),
    ("MEDIAN", ALL),
    ("MIN", ALL),
    ("MINA", ALL),
    ("MULTINOMIAL", ALL),
    ("N", &[0]),
    ("NETWORKDAYS", &[2]),
    ("NPV", ALL),
    ("OR", ALL),
    ("PERCENTILE", &[0]),
    ("PERCENTILE.EXC", &[0]),
    ("PERCENTILE.INC", &[0]),
    ("PERCENTRANK", &[0]),
    ("PERCENTRANK.EXC", &[0]),
    ("PERCENTRANK.INC", &[0]),
    ("PRODUCT", ALL),
    ("QUARTILE", &[0]),
    ("QUARTILE.EXC", &[0]),
    ("QUARTILE.INC", &[0]),
    ("REDUCE", &[0, 2]),
    ("SCAN", &[0, 2]),
    ("SERIESSUM", &[3]),
    ("SHEET", &[0]),
    ("SKEW", ALL),
    ("SKEW.P", ALL),
    ("SMALL", &[0]),
    ("SORTBY", &[2]),
    ("STDEV", ALL),
    ("STDEV.P", ALL),
    ("STDEV.S", ALL),
    ("STDEVA", ALL),
    ("STDEVP", ALL),
    ("STDEVPA", ALL),
    ("SUM", ALL),
    ("SUMSQ", ALL),
    ("TEXTAFTER", &[1]),
    ("TEXTBEFORE", &[1]),
    ("TEXTJOIN", ALL),
    ("TEXTSPLIT", &[1, 2]),
    ("TRANSPOSE", &[0]),
    ("TRIMMEAN", &[0]),
    ("TYPE", &[0]),
    ("VAR", ALL),
    ("VAR.P", ALL),
    ("VAR.S", ALL),
    ("VARA", ALL),
    ("VARP", ALL),
    ("VARPA", ALL),
    ("VLOOKUP", &[1]),
    ("WORKDAY", &[2]),
    ("XIRR", &[0, 1]),
    ("XLOOKUP", &[3]),
    ("XNPV", &[1, 2]),
    ("XOR", ALL),
    ("Z.TEST", &[0]),
    ("ZTEST", &[0]),
];

/// The array parameters, by function: a range expression written there lifts (measured as
/// [`AGGREGATE`] says). FILTER's include, for one, was measured by the catalogue oracle's capture
/// too (F3 dc-728): `=_xlfn._xlws.FILTER(Data!A1:A3,Data!A1:A3>1)` entered as a legacy formula
/// gives 5, the first of FILTER's rows {5;3}.
const ARRAY: &[(&str, &[usize])] = &[
    ("AGGREGATE", &[2]),
    ("ARRAYTOTEXT", &[0]),
    ("BYCOL", &[0]),
    ("BYROW", &[0]),
    ("CHISQ.TEST", &[0, 1]),
    ("CHITEST", &[0, 1]),
    ("CHOOSECOLS", &[0]),
    ("CHOOSEROWS", &[0]),
    ("COLUMNS", &[0]),
    ("CORREL", &[0, 1]),
    ("COVAR", &[0, 1]),
    ("COVARIANCE.P", &[0, 1]),
    ("COVARIANCE.S", &[0, 1]),
    ("DROP", &[0]),
    ("EXPAND", &[0]),
    ("F.TEST", &[0, 1]),
    ("FILTER", &[0, 1]),
    ("FORECAST", &[1, 2]),
    ("FORECAST.LINEAR", &[1, 2]),
    ("FREQUENCY", &[0, 1]),
    ("FTEST", &[0, 1]),
    ("GROWTH", &[0, 1]),
    ("HSTACK", ALL),
    ("INDEX", &[0]),
    ("INTERCEPT", &[0, 1]),
    ("IRR", &[0]),
    ("LINEST", &[0, 1]),
    ("LOGEST", &[0, 1]),
    ("LOOKUP", &[1, 2]),
    ("MAP", &[0]),
    ("MDETERM", &[0]),
    ("MINVERSE", &[0]),
    ("MIRR", &[0]),
    ("MMULT", &[0, 1]),
    ("MODE", ALL),
    ("MODE.MULT", ALL),
    ("MODE.SNGL", ALL),
    ("PEARSON", &[0, 1]),
    ("PROB", &[0, 1]),
    ("REDUCE", &[1]),
    ("ROWS", &[0]),
    ("RSQ", &[0, 1]),
    ("SCAN", &[1]),
    ("SHEETS", &[0]),
    ("SLOPE", &[0, 1]),
    ("SORT", &[0, 1]),
    ("SORTBY", &[0, 1]),
    ("STEYX", &[0, 1]),
    ("SUMPRODUCT", ALL),
    ("SUMX2MY2", &[0, 1]),
    ("SUMX2PY2", &[0, 1]),
    ("SUMXMY2", &[0, 1]),
    ("T.TEST", &[0, 1]),
    ("TAKE", &[0]),
    ("TOCOL", &[0]),
    ("TOROW", &[0]),
    ("TREND", &[0, 1]),
    ("TTEST", &[0, 1]),
    ("UNIQUE", &[0]),
    ("VSTACK", ALL),
    ("WRAPCOLS", &[0]),
    ("WRAPROWS", &[0]),
    ("XLOOKUP", &[1, 2]),
    ("XMATCH", &[1]),
];

/// The value parameters a legacy formula intersects wherever the call is, an array place
/// included, by function. dc-b1l measured FIELDVALUE in the catalogue oracle's capture (Excel
/// 16.0.20430, 2026-10-27): Excel loads `=ROWS(FIELDVALUE({1,2;3,4},"x"))` as
/// `=ROWS(FIELDVALUE(@{1,2;3,4},"x"))`, #FIELD!, and `=ROWS(FIELDVALUE(Data!A1:A3,"x"))` with `@`
/// before the range, where the `*` operand of `=ROWS(A1:A3*1)` lifts.
const SINGLE: &[(&str, &[usize])] = &[("FIELDVALUE", ALL)];

/// The array parameters that lift wherever the call is, an array place included (dc-h9u). Measured
/// in Excel 16.0.20527 through COM on 2026-10-30, `Range.Formula` entry in rows 1 to 36 over
/// A101:A103 = 1, 2, 3 and E101:E103 = East, West, East, `Range.Formula2` read back: Excel wrote no
/// `@` in `=ROWS(GROUPBY(E101:E103&"",A101:A103*1,SUM))` (3),
/// `=ROWS(GROUPBY(E101:E103,A101:A103,SUM,,,,A101:A103>1))` (3),
/// `=SUMPRODUCT(GROUPBY(E101:E103,A101:A103*1,SUM,,0))` (6) or
/// `=ROWS(PIVOTBY(E101:E103&"",D101:D103&"",A101:A103*1,SUM,,,,,,A101:A103>1))` (4), and the
/// catalogue oracle's `=ROWS(GROUPBY(...,{1;2;3;4;5;6}>2))` rows give the filtered groups.
const NESTED_ARRAY: &[(&str, &[usize])] = &[("GROUPBY", &[0, 1, 6]), ("PIVOTBY", &[0, 1, 2, 9])];

/// The parameters whose range expression a legacy formula intersects wherever the call is, an
/// array place included, while an array there lifts (dc-o9r). Measured as [`NESTED_ARRAY`] says:
/// `=SUMPRODUCT(TRANSPOSE(A101:A103*1))` is written back `=SUMPRODUCT(TRANSPOSE(@A101:A103*1))`,
/// #VALUE!, and `=ROWS(TRANSPOSE(A101:A103*1))` likewise, while `=SUMPRODUCT(TRANSPOSE({1,2,3}*1))`
/// keeps no `@`, 6, and `=ROWS(TRANSPOSE({1,2,3}*1))` is 3.
const AGGREGATE_ANYWHERE: &[(&str, &[usize])] = &[("TRANSPOSE", &[0])];

/// The parameters at which a legacy formula intersects an argument whose value is a reference
/// (Excel writes `@` before it), outside an array place or (`true`) everywhere (dc-o9r, dc-b1l).
/// Measured as [`NESTED_ARRAY`] says: `=TRANSPOSE(A101:A103)` is written back
/// `=@TRANSPOSE(@A101:A103)` and gives #VALUE!, and `=ROWS(TRANSPOSE(A101:A103))`,
/// `=SUMPRODUCT(TRANSPOSE(A101:A103))`, `=SUM(TRANSPOSE(OFFSET(A101,0,0,3,1)))`,
/// `=SUM(TRANSPOSE(Rng))`, `=INDEX(TRANSPOSE(D101:D103),1,3)` and
/// `=HLOOKUP("b",TRANSPOSE(D101:D103),1,FALSE)` each take `@` before the reference, #VALUE! below
/// it; `=TYPE(A101:A103)` is `=TYPE(@A101:A103)`, 16, and `TYPE(@OFFSET(A101,0,0,3,1))`,
/// `TYPE(@Rng)`, `TYPE(@A:A)` (1, in row 3) likewise, while `=SUMPRODUCT(TYPE(A101:A103))` keeps
/// no `@`, 64, and `=ROWS(TYPE(A101:A103))` is 1. `=TRANSPOSE({1,2,3})` and `=TYPE((A101,A102))`
/// (a union) take no `@` before the argument.
const REFERENCE_INTERSECTED: &[(&str, usize, bool)] = &[("TRANSPOSE", 0, true), ("TYPE", 0, false)];

/// Whether a legacy formula intersects a reference given as argument `index` of a call of
/// `spelling` made at `place` (see [`REFERENCE_INTERSECTED`]).
pub(super) fn intersects_reference(place: Place, spelling: &str, index: usize) -> bool {
    REFERENCE_INTERSECTED
        .iter()
        .any(|(name, position, everywhere)| {
            name.eq_ignore_ascii_case(spelling)
                && *position == index
                && (*everywhere || place != Place::Array)
        })
}

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
            Some(ParameterClass::Value) if listed(SINGLE, spelling, index) => Place::Value,
            Some(ParameterClass::Value) => self,
            Some(ParameterClass::ValueOrReference | ParameterClass::Reference) | None => {
                if listed(NESTED_ARRAY, spelling, index) {
                    Place::Array
                } else if listed(AGGREGATE_ANYWHERE, spelling, index) {
                    Place::Aggregate
                } else if listed(ARRAY, spelling, index) {
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
        // dc-10y: CHISQ.TEST's ranges lift (no `@` written); a parameter not measured (a
        // function absent from both tables) is refused.
        assert_eq!(
            Place::Value.argument("CHISQ.TEST", 0, r, false),
            Place::Array
        );
        assert_eq!(
            Place::Value.argument("NO.SUCH.FUNCTION", 0, r, false),
            Place::Unknown
        );
        // L3: XLOOKUP's lookup array lifts; MEDIAN's numbers are intersected.
        assert_eq!(Place::Value.argument("XLOOKUP", 1, r, false), Place::Array);
        assert_eq!(Place::Value.argument("MEDIAN", 0, r, false), Place::Aggregate);
        assert_eq!(Place::Unknown.admission(), Admission::Refuse);
        // A function below an array place that is not measured there.
        assert_eq!(array.argument("SUM", 0, r, false), Place::Unknown);
        assert_eq!(Place::Value.argument("lookup", 2, r, false), Place::Array);
        // dc-b1l: FIELDVALUE's value parameters are intersected below ROWS's array place.
        assert_eq!(array.argument("FIELDVALUE", 0, v, false), Place::Value);
        assert_eq!(array.argument("FIELDVALUE", 1, v, false), Place::Value);
        // dc-h9u: GROUPBY's filter lifts below ROWS's array place; dc-o9r: TRANSPOSE's range
        // expression is intersected there, and TRANSPOSE's and TYPE's references as measured.
        let reference = Some(ParameterClass::Reference);
        assert_eq!(array.argument("GROUPBY", 6, reference, false), Place::Array);
        assert_eq!(array.argument("PIVOTBY", 9, reference, false), Place::Array);
        assert_eq!(array.argument("TRANSPOSE", 0, r, false), Place::Aggregate);
        assert!(intersects_reference(Place::Array, "TRANSPOSE", 0));
        assert!(intersects_reference(Place::Value, "TYPE", 0));
        assert!(!intersects_reference(Place::Array, "TYPE", 0));
        assert!(!intersects_reference(Place::Value, "SUM", 0));
    }
}
