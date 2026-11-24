//! Generic per-function domain spec generator from `calc-catalogue` metadata and `dc-dfl.2.6`
//! per-parameter lifting, merged with per-family hint files (`hints.rs`, `dc-ztvv.26.1`).

use super::hints::{
    EdgeHint, ErrorEdgeHint, HintArgKind, HintBank, ParsedRelation, RegimeHint, RelOp, RelOperand,
    hint_value_to_witness, normalize_hint_param_name, parse_error_edge_triggers, parse_relations,
    value2_safe_f64,
};
use super::oracle::json_arg_to_touchstone;
use super::rng::{Rng, hex_f64, is_value2_exact, parse_hex_bits, pow2_f64, step_ulps};
use calc_catalogue::{
    ArgumentGroups, Availability, ContextFacility, DeclarationRef, EntryClass, Implementation,
    LegacyContext, Lifting, LiftingMode, LiftingOrigin, Notation, ParameterClass, ResultForm,
    parameter_lifting,
};
use serde_json::{Value, json};
use std::collections::BTreeSet;

/// Practical fuzzing arity cap for variadic functions (e.g. `SUM`, `SUMIFS`, `CHOOSE`).
pub const MAX_FUZZ_ARITY: usize = 12;

/// Per-parameter lifting summary extracted from `calc-catalogue` (`dc-dfl.2.6`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParamLiftingInfo {
    pub dynamic_array: Lifting,
    pub dynamic_range: Lifting,
    pub dynamic_ref_array: Lifting,
    pub legacy_root_range: Lifting,
    pub kernel_lifts: bool,
}

impl ParamLiftingInfo {
    pub fn for_param(decl: DeclarationRef, index: usize) -> Self {
        let kernel_lifts = decl.parameter(index).is_some_and(|p| p.kernel_lifts);
        Self {
            dynamic_array: parameter_lifting(decl, index, LiftingMode::Dynamic, LiftingOrigin::Array)
                .unwrap_or(Lifting::Unmeasured),
            dynamic_range: parameter_lifting(decl, index, LiftingMode::Dynamic, LiftingOrigin::Range)
                .unwrap_or(Lifting::Unmeasured),
            dynamic_ref_array: parameter_lifting(
                decl,
                index,
                LiftingMode::Dynamic,
                LiftingOrigin::ReferenceArray,
            )
            .unwrap_or(Lifting::Unmeasured),
            legacy_root_range: parameter_lifting(
                decl,
                index,
                LiftingMode::Legacy(LegacyContext::Value),
                LiftingOrigin::Range,
            )
            .unwrap_or(Lifting::Unmeasured),
            kernel_lifts,
        }
    }

    /// True when the parameter consumes a whole array/range rather than lifting elementwise.
    pub fn takes_whole_array(self) -> bool {
        self.dynamic_array == Lifting::Whole && !self.kernel_lifts
    }

    /// True when supplying an array triggers elementwise lifting (in CalcFml or in the kernel).
    pub fn lifts_elementwise(self) -> bool {
        matches!(
            self.dynamic_array,
            Lifting::Elementwise | Lifting::ElementwiseWhenLifted
        ) || self.kernel_lifts
    }

    pub fn to_json(self) -> Value {
        json!({
            "dynamic_array": self.dynamic_array.name(),
            "dynamic_range": self.dynamic_range.name(),
            "dynamic_ref_array": self.dynamic_ref_array.name(),
            "legacy_root_range": self.legacy_root_range.name(),
            "kernel_lifts": self.kernel_lifts,
            "takes_whole_array": self.takes_whole_array(),
            "lifts_elementwise": self.lifts_elementwise(),
        })
    }
}

/// Complete per-parameter domain specification (generic catalogue + optional merged hint).
#[derive(Clone, Debug)]
pub struct GenericArgSpec {
    pub index: usize,
    pub name: String,
    pub class: ParameterClass,
    pub entry_class: EntryClass,
    pub kind: HintArgKind,
    pub semantic_tag: &'static str,
    pub optional: bool,
    pub repeating: bool,
    pub lifting: ParamLiftingInfo,
    pub valid_ranges: Vec<[f64; 2]>,
    pub specials: Vec<Value>,
    pub edges: Vec<EdgeHint>,
    pub hinted: bool,
}

impl GenericArgSpec {
    /// True when Excel / CalcFunc requires a worksheet reference (not a value array or scalar)
    /// at this argument position.
    pub fn is_reference_only(&self) -> bool {
        self.class == ParameterClass::Reference || self.entry_class == EntryClass::Reference
    }

    pub fn to_json(&self) -> Value {
        json!({
            "index": self.index,
            "name": self.name,
            "class": format!("{:?}", self.class),
            "entry_class": format!("{:?}", self.entry_class),
            "kind": self.kind.as_str(),
            "semantic_tag": self.semantic_tag,
            "optional": self.optional,
            "repeating": self.repeating,
            "hinted": self.hinted,
            "lifting": self.lifting.to_json(),
            "valid_ranges": self.valid_ranges,
            "specials_count": self.specials.len(),
            "edges_count": self.edges.len(),
        })
    }
}

/// Complete merged specification for one catalogue function.
#[derive(Clone, Debug)]
pub struct MergedSpec {
    pub function: String,
    pub implementation: Implementation,
    pub availability: Availability,
    pub may_return_reference: bool,
    pub uses_callables: bool,
    pub min_arity: usize,
    pub max_arity: usize,
    pub groups: Option<ArgumentGroups>,
    pub args: Vec<GenericArgSpec>,
    pub has_hint_file: bool,
    pub hint_family: Option<String>,
    pub hint_source_file: Option<String>,
    pub relations: Vec<String>,
    pub regimes: Vec<RegimeHint>,
    pub error_edges: Vec<ErrorEdgeHint>,
    pub notes: String,
    pub parsed_relations: Vec<ParsedRelation>,
    pub parsed_error_edges: Vec<ParsedRelation>,
}

impl MergedSpec {
    /// Whether `arity` satisfies `min_arity..=max_arity` and repeating `groups`.
    pub fn is_valid_arity(&self, arity: usize) -> bool {
        if arity < self.min_arity || arity > self.max_arity {
            return false;
        }
        !self.groups.is_some_and(|g| g.incomplete(arity))
    }

    /// Pick a deterministic valid arity respecting `min_arity..=max_arity` and repeating `groups`.
    pub fn pick_valid_arity(&self, rng: &mut Rng) -> usize {
        if self.max_arity <= self.min_arity {
            return self.min_arity;
        }
        if self.groups.is_some() {
            let valid: Vec<usize> = (self.min_arity..=self.max_arity)
                .filter(|&n| self.is_valid_arity(n))
                .collect();
            if !valid.is_empty() {
                return *rng.pick(&valid);
            }
        }
        self.min_arity + rng.gen_range(self.max_arity - self.min_arity + 1)
    }

    pub fn has_whole_array_params(&self) -> bool {
        self.args.iter().any(|a| {
            a.lifting.takes_whole_array()
                || matches!(a.kind, HintArgKind::Array | HintArgKind::Reference)
        })
    }

    pub fn has_hint_anchors(&self) -> bool {
        self.has_hint_file
            && self
                .args
                .iter()
                .any(|a| a.hinted && (!a.specials.is_empty() || !a.edges.is_empty()))
    }

    pub fn has_hint_ranges_or_relations(&self) -> bool {
        self.has_hint_file
            && (!self.parsed_relations.is_empty()
                || self.args.iter().any(|a| a.hinted && !a.valid_ranges.is_empty()))
    }

    pub fn has_hint_error_edges(&self) -> bool {
        self.has_hint_file && !self.parsed_error_edges.is_empty()
    }

    /// Static metadata reason to skip a function in the standing sweep (e.g. deferred/unheld
    /// implementation or higher-order callable/LAMBDA parameters with no Touchstone transport).
    pub fn sweep_static_skip_reason(&self) -> Option<String> {
        if self.implementation != Implementation::Held {
            return Some(format!("implementation:{:?}", self.implementation));
        }
        if self.uses_callables {
            return Some("requires-callable-lambda".to_owned());
        }
        None
    }

    pub fn to_json(&self) -> Value {
        json!({
            "function": self.function,
            "implementation": format!("{:?}", self.implementation),
            "availability": format!("{:?}", self.availability),
            "min_arity": self.min_arity,
            "max_arity": self.max_arity,
            "has_hint_file": self.has_hint_file,
            "hint_family": self.hint_family,
            "hint_source_file": self.hint_source_file,
            "relations": self.relations,
            "regimes_count": self.regimes.len(),
            "error_edges_count": self.error_edges.len(),
            "notes": self.notes,
            "args": self.args.iter().map(GenericArgSpec::to_json).collect::<Vec<_>>(),
        })
    }
}

/// Region generators supplied by `spec_gen.rs`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpecRegionKind {
    /// Generic domain-aware sampling from `calc-catalogue` metadata + lifting + defaults.
    GenericDomain,
    /// Per-parameter lifting (`dc-dfl.2.6`) and array-shape variations.
    LiftingAndShapes,
    /// Hint `specials` and `edges` (plus ±1..4 ULP perturbations around numeric edges).
    HintAnchorsAndEdges,
    /// Hint `ranges` (inside + just outside bounds) and `relations` (both honoured and broken).
    HintRangesAndRelations,
    /// Hint `error_edges` conditions driven directly to exercise Excel error boundaries.
    HintErrorEdges,
}

/// Return every named `Notation::Call` function in `calc-catalogue` in sorted spelling order.
pub fn all_catalogue_call_functions() -> Vec<String> {
    let mut names: Vec<String> = calc_catalogue::declarations()
        .filter(|d| d.notation() == Notation::Call)
        .map(|d| d.spelling().to_owned())
        .collect();
    names.sort();
    names.dedup();
    names
}

fn effective_max_arity(decl: DeclarationRef) -> usize {
    let arity = decl.arity();
    let cap = MAX_FUZZ_ARITY.max(arity.min);
    let raw_max = arity.max.min(cap);
    if let Some(groups) = decl.groups()
        && raw_max > groups.from
        && raw_max < arity.max
    {
        // Keep the capped max aligned to a complete group or declared tail.
        let mut best = raw_max;
        while best > arity.min {
            let rem = (best - groups.from) % groups.size;
            if rem == 0 || rem == groups.tail {
                return best;
            }
            best -= 1;
        }
    }
    raw_max
}

fn normalize_name(s: &str) -> String {
    s.trim()
        .trim_matches(|c| matches!(c, '[' | ']'))
        .to_ascii_lowercase()
        .replace([' ', '-'], "_")
}

/// Infer `(HintArgKind, semantic_tag)` from function spelling, parameter name, class, entry class,
/// and `dc-dfl.2.6` lifting metadata.
fn infer_kind_and_tag(
    function: &str,
    param_name: &str,
    class: ParameterClass,
    entry_class: EntryClass,
    lifting: ParamLiftingInfo,
) -> (HintArgKind, &'static str) {
    let lower = normalize_name(param_name);

    // 1. Explicit criteria parameters (note: SUMIF/COUNTIF criteria have kernel_lifts == true).
    if lower.contains("criteria") && !lower.contains("range") {
        return (HintArgKind::Criteria, "criteria");
    }

    // 2. Reference-only entry slots or Reference parameter class.
    if entry_class == EntryClass::Reference || class == ParameterClass::Reference {
        return (HintArgKind::Reference, "reference");
    }

    // 3. Function-specific text/complex/radix domains where the parameter name alone is "number".
    if matches!(
        function,
        "BIN2DEC" | "BIN2HEX" | "BIN2OCT" | "OCT2BIN" | "OCT2DEC" | "OCT2HEX" | "HEX2BIN"
            | "HEX2DEC" | "HEX2OCT"
    ) && lower == "number"
    {
        return (HintArgKind::Text, "radix-text");
    }
    if function == "ARABIC" || (function == "DECIMAL" && lower == "text") {
        return (HintArgKind::Text, "radix-text");
    }
    if function.starts_with("IM") && (lower.contains("inumber") || lower == "number") {
        return (HintArgKind::Text, "complex-text");
    }
    if function == "CONVERT" && (lower.contains("unit") || lower == "from_unit" || lower == "to_unit") {
        return (HintArgKind::Text, "unit-text");
    }
    if matches!(function, "HOUR" | "MINUTE" | "SECOND") && lower.contains("serial") {
        return (HintArgKind::TimeFraction, "time-fraction");
    }

    // 4. Whole-array / range-consuming ValueOrReference parameters (guided by dc-dfl.2.6 lifting).
    if class == ParameterClass::ValueOrReference
        && lifting.takes_whole_array()
        && (lower.contains("array")
            || lower.contains("values")
            || lower.contains("range")
            || lower.contains("table")
            || lower.contains("matrix")
            || lower.contains("known_")
            || lower.contains("schedule")
            || lower.contains("dates")
            || lower.contains("bins")
            || lower.contains("data")
            || lower.contains("vector")
            || lower.contains("holidays")
            || lower.contains("database")
            || lower.contains("by_array")
            || lower.contains("include")
            || lower.contains("row_fields")
            || lower.contains("col_fields"))
    {
        return (HintArgKind::Array, "array");
    }

    // 5. Text parameters.
    if lower.contains("text")
        || lower.contains("string")
        || lower.contains("find")
        || lower.contains("within")
        || lower.contains("delim")
        || lower.contains("format")
        || lower.contains("pattern")
        || lower.contains("replace")
        || lower.contains("inumber")
        || lower.contains("prefix")
        || lower.contains("suffix")
        || lower.contains("url")
        || lower.contains("xpath")
        || lower.contains("xml")
        || lower.contains("currency")
        || lower.contains("separator")
        || lower.contains("connection")
        || lower.contains("member_expression")
        || lower.contains("set_expression")
        || lower.contains("caption")
        || lower.contains("kpi_")
        || lower.contains("link_location")
        || lower.contains("friendly_name")
        || lower == "info_type"
        || lower == "type_text"
        || lower == "ref_text"
        || lower == "old"
        || lower == "new"
        || lower == "char"
        || lower == "weekend"
        || lower == "progid"
        || lower == "server"
        || lower.starts_with("topic")
    {
        let tag = if lower.contains("format") {
            "format-text"
        } else if lower.contains("pattern") {
            "regex-pattern"
        } else if lower.contains("delim") {
            "delimiter"
        } else if lower.contains("inumber") {
            "complex-text"
        } else if lower == "info_type" {
            "info-type"
        } else if lower == "ref_text" {
            "ref-text"
        } else if lower == "date_text" {
            "date-text"
        } else if lower == "time_text" {
            "time-text"
        } else {
            "text"
        };
        return (HintArgKind::Text, tag);
    }

    // 6. Logical parameters.
    if matches!(
        lower.as_str(),
        "cumulative"
            | "logical"
            | "logical1"
            | "logical2"
            | "logical_test"
            | "a1"
            | "range_lookup"
            | "const"
            | "stats"
            | "no_commas"
            | "ignore_empty"
            | "ignore_case"
            | "by_col"
            | "exactly_once"
            | "no_switch"
            | "exact_match"
            | "case_sensitivity"
            | "strict"
            | "scan_by_column"
    ) {
        return (HintArgKind::Logical, "logical");
    }

    // 7. Date serial & time fraction parameters.
    if lower.contains("date")
        || lower.contains("settlement")
        || lower.contains("maturity")
        || lower.contains("issue")
        || lower.contains("first_interest")
        || lower.contains("last_interest")
        || lower.contains("first_coupon")
        || lower.contains("serial_number")
    {
        return (HintArgKind::DateSerial, "date-serial");
    }
    if lower.contains("time") && !lower.contains("times") {
        return (HintArgKind::TimeFraction, "time-fraction");
    }

    // 8. Bounded integer / enum-like parameters (especially output-size & loop-bound params).
    if matches!(
        lower.as_str(),
        "rows" | "columns" | "row_num" | "col_num" | "row_offset" | "col_offset" | "height" | "width"
            | "number_times" | "wrap_count"
    ) {
        return (HintArgKind::Integer, "size-bounded");
    }
    if lower == "type" {
        return (HintArgKind::Integer, "timing");
    }
    if lower == "frequency" {
        return (HintArgKind::Integer, "frequency");
    }
    if lower == "basis" {
        return (HintArgKind::Integer, "basis");
    }
    if matches!(lower.as_str(), "match_type" | "match_mode") {
        return (HintArgKind::Integer, "match-mode");
    }
    if lower == "search_mode" {
        return (HintArgKind::Integer, "search-mode");
    }
    if lower == "return_type" {
        return (HintArgKind::Integer, "return-type");
    }
    if lower == "radix" {
        return (HintArgKind::Integer, "radix");
    }
    if lower == "year" {
        return (HintArgKind::Integer, "year");
    }
    if lower == "month" || lower == "months" {
        return (HintArgKind::Integer, "month");
    }
    if lower == "day" || lower == "days" {
        return (HintArgKind::Integer, "day");
    }
    if lower == "hour" {
        return (HintArgKind::Integer, "hour");
    }
    if lower == "minute" {
        return (HintArgKind::Integer, "minute");
    }
    if lower == "second" {
        return (HintArgKind::Integer, "second");
    }
    if lower.contains("digits") || lower == "places" || lower == "min_length" {
        return (HintArgKind::Integer, "digits");
    }
    if lower == "tails" {
        return (HintArgKind::Integer, "tails");
    }
    if lower == "form" {
        return (HintArgKind::Integer, "roman-form");
    }
    if lower == "function_num" {
        return (HintArgKind::Integer, "function-num");
    }
    if lower == "options" {
        return (HintArgKind::Integer, "options");
    }
    if lower == "quart" {
        return (HintArgKind::Integer, "quart");
    }
    if matches!(
        lower.as_str(),
        "num_chars"
            | "num_bytes"
            | "start_num"
            | "instance_num"
            | "index_num"
            | "area_num"
            | "k"
            | "n"
            | "r"
            | "number_chosen"
            | "trials"
            | "number_s"
            | "deg_freedom"
            | "deg_freedom1"
            | "deg_freedom2"
            | "shift_amount"
            | "sort_index"
            | "sort_order"
            | "order"
            | "mode"
            | "abs_num"
            | "method"
            | "occurrence"
            | "field_headers"
            | "total_depth"
    ) {
        return (HintArgKind::Integer, "integer");
    }

    // 9. Numeric semantic sub-tags.
    if lower == "guess" {
        return (HintArgKind::Number, "guess");
    }
    if lower.contains("prob") || lower == "alpha" || lower == "significance" {
        return (HintArgKind::Number, "probability");
    }
    if lower.contains("rate") || lower == "yld" || lower == "discount" || lower == "coupon" {
        return (HintArgKind::Number, "rate");
    }
    if lower == "nper" || lower == "per" || lower.contains("period") || lower == "life" || lower == "npery" {
        return (HintArgKind::Number, "periods");
    }
    if matches!(
        lower.as_str(),
        "pmt" | "pv" | "fv" | "pr" | "redemption" | "par" | "cost" | "salvage" | "principal" | "investment"
    ) {
        return (HintArgKind::Number, "amount");
    }

    // 10. Polymorphic / Any slots.
    if matches!(
        lower.as_str(),
        "value"
            | "val"
            | "value1"
            | "value2"
            | "lookup_value"
            | "value_if_true"
            | "value_if_false"
            | "value_if_error"
            | "value_if_na"
            | "if_not_found"
            | "if_empty"
            | "default"
            | "pad_with"
            | "expression"
            | "result"
            | "initial_value"
            | "field"
    ) {
        return (HintArgKind::Any, "any");
    }

    // 11. Whole-array numeric aggregate slots (e.g. SUM's `number1`).
    if class == ParameterClass::ValueOrReference && lifting.takes_whole_array() {
        return (HintArgKind::Array, "numeric-or-array");
    }

    // 12. Default: general binary64 number.
    (HintArgKind::Number, "number")
}

fn default_valid_ranges(kind: HintArgKind, tag: &'static str, function: &str) -> Vec<[f64; 2]> {
    match tag {
        "size-bounded" => vec![[1.0, 12.0]],
        "timing" => vec![[0.0, 1.0]],
        "frequency" => vec![[1.0, 4.0]],
        "basis" => vec![[0.0, 4.0]],
        "match-mode" => vec![[-1.0, 2.0]],
        "search-mode" => vec![[-2.0, 2.0]],
        "return-type" => vec![[1.0, 17.0]],
        "radix" => vec![[2.0, 36.0]],
        "year" => vec![[1900.0, 2100.0]],
        "month" => vec![[1.0, 12.0]],
        "day" => vec![[1.0, 31.0]],
        "hour" => vec![[0.0, 23.0]],
        "minute" | "second" => vec![[0.0, 59.0]],
        "digits" => vec![[-6.0, 15.0]],
        "tails" => vec![[1.0, 2.0]],
        "roman-form" => vec![[0.0, 4.0]],
        "function-num" => vec![[1.0, 19.0]],
        "options" => vec![[0.0, 7.0]],
        "quart" => vec![[0.0, 4.0]],
        "probability" => vec![[0.0, 1.0]],
        "rate" => vec![[-0.5, 1.0]],
        "guess" => vec![[-0.99, 2.0]],
        "periods" => vec![[1.0, 360.0]],
        "amount" => vec![[-100_000.0, 100_000.0]],
        "date-serial" => vec![[1.0, 60_000.0], [1.0, 2_958_465.0]],
        "time-fraction" => vec![[0.0, 1.0]],
        _ => match kind {
            HintArgKind::Integer => {
                if function == "MULTINOMIAL" || function == "FACT" || function == "FACTDOUBLE" {
                    vec![[0.0, 30.0]]
                } else {
                    vec![[-20.0, 100.0]]
                }
            }
            HintArgKind::DateSerial => vec![[1.0, 60_000.0]],
            HintArgKind::TimeFraction => vec![[0.0, 1.0]],
            HintArgKind::Number => {
                if matches!(function, "MULTINOMIAL" | "FACT" | "FACTDOUBLE" | "ROMAN") {
                    vec![[0.0, 30.0]]
                } else {
                    vec![[-1_000.0, 1_000.0]]
                }
            }
            _ => Vec::new(),
        },
    }
}

fn default_specials(kind: HintArgKind, tag: &'static str) -> Vec<Value> {
    let nums = |xs: &[f64]| -> Vec<Value> { xs.iter().map(|&x| hex_f64(x)).collect() };
    let strs = |xs: &[&str]| -> Vec<Value> { xs.iter().map(|&s| Value::String(s.to_owned())).collect() };
    match tag {
        "size-bounded" => nums(&[0.0, 1.0, 2.0, 3.0, 5.0, -1.0]),
        "timing" => nums(&[0.0, 1.0, -1.0, 2.0, 0.5]),
        "frequency" => nums(&[1.0, 2.0, 4.0, 0.0, 3.0, -1.0]),
        "basis" => nums(&[0.0, 1.0, 2.0, 3.0, 4.0, -1.0, 5.0]),
        "match-mode" => nums(&[0.0, -1.0, 1.0, 2.0, 3.0]),
        "search-mode" => nums(&[1.0, -1.0, 2.0, -2.0, 0.0]),
        "return-type" => nums(&[1.0, 2.0, 3.0, 11.0, 17.0, 0.0, 99.0]),
        "radix" => nums(&[2.0, 8.0, 10.0, 16.0, 36.0, 1.0, 37.0]),
        "year" => nums(&[0.0, 1900.0, 1904.0, 2000.0, 2024.0, 9999.0, -1.0]),
        "month" => nums(&[0.0, 1.0, 2.0, 12.0, 13.0, -1.0]),
        "day" => nums(&[0.0, 1.0, 28.0, 29.0, 30.0, 31.0, 32.0, -1.0]),
        "hour" => nums(&[0.0, 12.0, 23.0, 24.0, -1.0]),
        "minute" | "second" => nums(&[0.0, 30.0, 59.0, 60.0, -1.0]),
        "digits" => nums(&[0.0, 1.0, 2.0, -1.0, -2.0, 10.0, 15.0]),
        "tails" => nums(&[1.0, 2.0, 0.0, 3.0]),
        "roman-form" => nums(&[0.0, 1.0, 2.0, 3.0, 4.0, -1.0, 5.0]),
        "function-num" => nums(&[1.0, 9.0, 101.0, 109.0, 0.0, 20.0]),
        "options" => nums(&[0.0, 1.0, 3.0, 5.0, 7.0, -1.0, 8.0]),
        "quart" => nums(&[0.0, 1.0, 2.0, 3.0, 4.0, -1.0, 5.0]),
        "probability" => nums(&[0.0, 1e-12, 0.05, 0.5, 0.95, 1.0 - 1e-12, 1.0, -0.1, 1.1]),
        "rate" => nums(&[0.0, 0.05, 0.1, -0.1, 1.0, -1.0, 1e-7]),
        "guess" => nums(&[0.1, 0.0, -0.5, -0.9999999, -1.0, 1.0, 10.0]),
        "periods" => nums(&[0.0, 1.0, -1.0, 0.5, 12.0, 360.0]),
        "date-serial" => nums(&[0.0, 1.0, 59.0, 60.0, 61.0, 36526.0, 45351.0, 2958465.0, -1.0]),
        "time-fraction" => nums(&[0.0, 0.25, 0.5, 0.75, 0.9999884259259259, 1.0, -0.1]),
        "criteria" => strs(&[">0", ">=10", "<0", "<>0", "=1", "a*", "*b?", ""]),
        "format-text" => strs(&["0", "0.00", "#,##0.00", "0%", "yyyy-mm-dd", "General", "@", ""]),
        "regex-pattern" => strs(&["[0-9]+", "^[a-z]+$", "(a)(b)", "\\d+", "", "["]),
        "delimiter" => strs(&[",", " ", "-", "|", "", "::"]),
        "complex-text" => strs(&["0", "1", "i", "-i", "3+4i", "3-4j", "1e-5+2e5i", "bad"]),
        "radix-text" => strs(&["0", "1", "10", "1010", "FF", "77", "IV", "MCMXCIV", ""]),
        "unit-text" => strs(&["m", "km", "in", "ft", "g", "kg", "C", "F", "bad"]),
        "info-type" => strs(&["address", "col", "row", "type", "contents", "filename", "bad"]),
        "ref-text" => strs(&["A1", "R1C1", "B2:C3", "invalid!", ""]),
        "date-text" => strs(&["2024-02-29", "1900-01-01", "2024-12-31", "01/15/2025", "not-a-date", ""]),
        "time-text" => strs(&["00:00:00", "12:30:45", "23:59:59", "6:00 PM", "not-a-time", ""]),
        _ => match kind {
            HintArgKind::Text => strs(&["", "a", "abc", "Hello World", "123", "-45.67", "  padded  ", "café"]),
            HintArgKind::Logical => vec![Value::Bool(true), Value::Bool(false), hex_f64(0.0), hex_f64(1.0)],
            HintArgKind::Criteria => strs(&[">0", "<=100", "=0", "<>\"\"", "a*", ""]),
            HintArgKind::Integer => nums(&[0.0, 1.0, -1.0, 2.0, 5.0, 10.0, 100.0]),
            HintArgKind::Number => nums(&[0.0, 1.0, -1.0, 0.5, -0.5, 0.1, 2.0, 10.0, 100.0, 1e-7, 1e10]),
            HintArgKind::DateSerial => nums(&[1.0, 59.0, 60.0, 61.0, 44927.0, 2958465.0]),
            HintArgKind::TimeFraction => nums(&[0.0, 0.25, 0.5, 0.75, 1.0]),
            HintArgKind::Any => vec![
                hex_f64(0.0),
                hex_f64(1.0),
                hex_f64(-1.0),
                Value::Bool(true),
                Value::Bool(false),
                Value::String("abc".into()),
                Value::String("".into()),
                Value::Null,
                json!({"error": "NA"}),
            ],
            HintArgKind::Array | HintArgKind::Reference => Vec::new(),
        },
    }
}

/// Build the merged generic + hint specification for `function`.
pub fn build_merged_spec(function: &str, hints: &HintBank) -> Result<MergedSpec, String> {
    let name = function.trim().to_ascii_uppercase();
    let decl = calc_catalogue::named(&name)
        .ok_or_else(|| format!("function {name} is not in calc-catalogue"))?;
    let min_arity = decl.arity().min;
    let max_arity = effective_max_arity(decl);
    let named_len = decl.parameters().len();
    let loaded_hint = hints.get(&name);

    let mut args = Vec::with_capacity(max_arity);
    for i in 0..max_arity {
        let param = decl.parameter(i);
        let (pname, class, entry_class) = match param {
            Some(p) => (p.name.to_owned(), p.class, p.entry),
            None => (format!("arg{i}"), ParameterClass::Value, EntryClass::Any),
        };
        let lifting = ParamLiftingInfo::for_param(decl, i);
        let (mut kind, semantic_tag) = infer_kind_and_tag(&name, &pname, class, entry_class, lifting);
        let mut valid_ranges = default_valid_ranges(kind, semantic_tag, &name);
        let mut specials = default_specials(kind, semantic_tag);
        let mut edges = Vec::new();
        let mut hinted = false;

        if let Some(lh) = loaded_hint {
            let pnorm = normalize_hint_param_name(&pname);
            // Match strictly by normalized parameter name (no silent positional fallback).
            // For repeating/variadic slots (`i >= named_len`), prefer a numbered hint like
            // `logical2`/`criteria2` matching the slot's 1-based repeat index, falling back to
            // the declared repeating parameter name `pnorm`.
            let numbered_name = (i >= named_len).then(|| {
                let stem = pnorm.trim_end_matches(|c: char| c.is_ascii_digit());
                let rep_num = if let Some(g) = decl.groups() {
                    1 + (i.saturating_sub(g.from)) / g.size
                } else {
                    i + 1
                };
                format!("{stem}{rep_num}")
            });
            let matched_hint = numbered_name
                .as_deref()
                .and_then(|nn| {
                    lh.hint
                        .args
                        .iter()
                        .find(|ah| normalize_hint_param_name(&ah.name) == nn)
                })
                .or_else(|| {
                    lh.hint
                        .args
                        .iter()
                        .find(|ah| normalize_hint_param_name(&ah.name) == pnorm)
                });
            if let Some(ah) = matched_hint {
                hinted = true;
                kind = ah.kind;
                let is_ref_only =
                    class == ParameterClass::Reference || entry_class == EntryClass::Reference;
                let witness_kind = if is_ref_only {
                    HintArgKind::Reference
                } else {
                    kind
                };
                if !ah.ranges.is_empty() {
                    valid_ranges = ah.ranges.clone();
                }
                let mut seen: BTreeSet<String> = BTreeSet::new();
                let mut merged_specials = Vec::new();
                for sv in &ah.specials {
                    if let Some(w) = hint_value_to_witness(sv, witness_kind) {
                        let w = if is_ref_only || kind == HintArgKind::Reference {
                            wrap_as_reference(w)
                        } else {
                            w
                        };
                        if seen.insert(w.to_string()) {
                            merged_specials.push(w);
                        }
                    }
                }
                for eh in &ah.edges {
                    if let Some(w) = hint_value_to_witness(&eh.value, witness_kind) {
                        let w = if is_ref_only || kind == HintArgKind::Reference {
                            wrap_as_reference(w)
                        } else {
                            w
                        };
                        if seen.insert(w.to_string()) {
                            merged_specials.push(w);
                        }
                    }
                }
                for sv in specials {
                    let sv = if is_ref_only || kind == HintArgKind::Reference {
                        wrap_as_reference(sv)
                    } else {
                        sv
                    };
                    if seen.insert(sv.to_string()) {
                        merged_specials.push(sv);
                    }
                }
                specials = merged_specials;
                edges = ah.edges.clone();
            }
        }

        args.push(GenericArgSpec {
            index: i,
            name: pname,
            class,
            entry_class,
            kind,
            semantic_tag,
            optional: i >= min_arity,
            repeating: i >= named_len,
            lifting,
            valid_ranges,
            specials,
            edges,
            hinted,
        });
    }

    let arg_names: Vec<String> = args.iter().map(|a| a.name.clone()).collect();
    let (has_hint_file, hint_family, hint_source_file, relations, regimes, error_edges, notes) =
        match loaded_hint {
            Some(lh) => (
                true,
                Some(lh.family.clone()),
                Some(lh.source_file.clone()),
                lh.hint.relations.clone(),
                lh.hint.regimes.clone(),
                lh.hint.error_edges.clone(),
                lh.hint.notes.clone(),
            ),
            None => (false, None, None, Vec::new(), Vec::new(), Vec::new(), String::new()),
        };

    let parsed_relations = parse_relations(&relations, &arg_names);
    let error_whens: Vec<String> = error_edges.iter().map(|e| e.when.clone()).collect();
    let parsed_error_edges = parse_error_edge_triggers(&error_whens, &arg_names);

    Ok(MergedSpec {
        function: name,
        implementation: decl.implementation(),
        availability: decl.availability(),
        may_return_reference: decl.result() == ResultForm::MayBeReference,
        uses_callables: decl.uses_context(ContextFacility::Callables),
        min_arity,
        max_arity,
        groups: decl.groups(),
        args,
        has_hint_file,
        hint_family,
        hint_source_file,
        relations,
        regimes,
        error_edges,
        notes,
        parsed_relations,
        parsed_error_edges,
    })
}

// ============================================================================
// Deterministic argument generators for merged specs
// ============================================================================

fn gen_numeric_array(rows: usize, cols: usize, rng: &mut Rng, mode: u8) -> Value {
    let mut grid = Vec::with_capacity(rows);
    for r in 0..rows {
        let mut row = Vec::with_capacity(cols);
        for c in 0..cols {
            let mut v = match mode {
                0 => *rng.pick(&[-100.0, -10.0, -1.0, 0.0, 0.5, 1.0, 2.0, 10.0, 50.0, 100.0]),
                1 => rng.log_uniform(-16, 16, true),
                _ => rng.uniform(-500.0, 500.0),
            };
            if r == 0 && c == 0 && v > 0.0 && rng.chance(0.5) {
                v = -v;
            }
            row.push(hex_f64(value2_safe_f64(v).unwrap_or(0.0)));
        }
        grid.push(Value::Array(row));
    }
    Value::Array(grid)
}

fn sample_in_range(lo: f64, hi: f64, kind: HintArgKind, rng: &mut Rng) -> f64 {
    if lo >= hi {
        return value2_safe_f64(lo).unwrap_or(0.0);
    }
    let raw = match kind {
        HintArgKind::Integer | HintArgKind::DateSerial => {
            let ilo = lo.ceil().clamp(-1_000_000.0, 3_000_000.0) as i64;
            let ihi = hi.floor().clamp(ilo as f64, 3_000_000.0) as i64;
            let base = rng.gen_i64_inclusive(ilo, ihi) as f64;
            if kind == HintArgKind::DateSerial && rng.chance(0.15) {
                base + rng.uniform(0.0, 0.999)
            } else {
                base
            }
        }
        _ => {
            let span = hi - lo;
            if span.is_finite() && span > 0.0 {
                lo + span * rng.next_f64()
            } else {
                rng.log_uniform(-20, 20, true)
            }
        }
    };
    value2_safe_f64(raw).unwrap_or_else(|| value2_safe_f64(lo).unwrap_or(0.0))
}

fn sample_outside_range(lo: f64, hi: f64, kind: HintArgKind, rng: &mut Rng) -> f64 {
    let below = rng.chance(0.5);
    let anchor = if below { lo } else { hi };
    let sign = if below { -1.0 } else { 1.0 };
    let candidate = match kind {
        HintArgKind::Integer | HintArgKind::DateSerial => {
            anchor + sign * (*rng.pick(&[1.0, 2.0, 5.0, 0.5]))
        }
        _ => match rng.gen_range(3) {
            0 => step_ulps(anchor, if below { -1 } else { 1 }, true).unwrap_or(anchor),
            1 => {
                let step = anchor.abs().max(1.0) * (*rng.pick(&[1e-6, 1e-3, 0.1, 1.0]));
                anchor + sign * step
            }
            _ => anchor + sign * pow2_f64(-(rng.gen_i64_inclusive(1, 30) as i32)),
        },
    };
    value2_safe_f64(candidate).unwrap_or(if below { -1.0 } else { 1.0 })
}

fn wrap_as_reference(val: Value) -> Value {
    match val {
        Value::Object(ref map) if map.contains_key("range") => val,
        Value::Array(grid) => json!({ "range": grid }),
        scalar => json!({ "range": [[scalar]] }),
    }
}

fn gen_cell_range(rows: usize, cols: usize, rng: &mut Rng, mode: u8) -> Value {
    let rows = rows.clamp(1, 32);
    let cols = cols.clamp(1, (128 / rows).max(1).min(32));
    json!({ "range": gen_numeric_array(rows, cols, rng, mode) })
}

fn gen_arg_value(arg: &GenericArgSpec, rng: &mut Rng, mode: u8) -> Value {
    if arg.is_reference_only() || arg.kind == HintArgKind::Reference {
        if !arg.specials.is_empty() && (mode == 0 || rng.chance(0.45)) {
            return wrap_as_reference(rng.pick(&arg.specials).clone());
        }
        let (rows, cols) = match mode {
            0 => (1 + rng.gen_range(4), 1),
            1 => (1 + rng.gen_range(3), 1 + rng.gen_range(3)),
            _ => (2 + rng.gen_range(4), 1),
        };
        return gen_cell_range(rows, cols, rng, mode);
    }
    match arg.kind {
        HintArgKind::Array => {
            if !arg.specials.is_empty() && (mode == 0 || rng.chance(0.45)) {
                return rng.pick(&arg.specials).clone();
            }
            let (rows, cols) = match mode {
                0 => (1 + rng.gen_range(4), 1),
                1 => (1 + rng.gen_range(3), 1 + rng.gen_range(3)),
                _ => (2 + rng.gen_range(4), 1),
            };
            gen_numeric_array(rows, cols, rng, mode)
        }
        HintArgKind::Reference => unreachable!(),
        HintArgKind::Logical => {
            if !arg.specials.is_empty() && rng.chance(0.4) {
                rng.pick(&arg.specials).clone()
            } else {
                Value::Bool(rng.chance(0.5))
            }
        }
        HintArgKind::Text | HintArgKind::Criteria => {
            if !arg.specials.is_empty() && rng.chance(0.75) {
                rng.pick(&arg.specials).clone()
            } else if arg.kind == HintArgKind::Criteria {
                let op = *rng.pick(&[">", ">=", "<", "<=", "=", "<>"]);
                let n = rng.gen_i64_inclusive(-50, 100);
                Value::String(format!("{op}{n}"))
            } else {
                let len = rng.gen_range(12);
                const ALPHABET: &[u8] = b"abcXYZ012345 -_.,/";
                let mut s = String::with_capacity(len);
                for _ in 0..len {
                    s.push(*rng.pick(ALPHABET) as char);
                }
                Value::String(s)
            }
        }
        HintArgKind::Any => {
            if !arg.specials.is_empty() && rng.chance(0.5) {
                rng.pick(&arg.specials).clone()
            } else {
                match rng.gen_range(5) {
                    0 => hex_f64(rng.uniform(-100.0, 100.0)),
                    1 => Value::Bool(rng.chance(0.5)),
                    2 => Value::String((*rng.pick(&["a", "abc", "0", "12.5", ""])).to_owned()),
                    3 => Value::Null,
                    _ => json!({"error": *rng.pick(&["NA", "Value", "Div0", "Num"])}),
                }
            }
        }
        HintArgKind::Number
        | HintArgKind::Integer
        | HintArgKind::DateSerial
        | HintArgKind::TimeFraction => {
            if !arg.specials.is_empty() && (mode == 0 || rng.chance(0.35)) {
                return rng.pick(&arg.specials).clone();
            }
            if !arg.valid_ranges.is_empty() {
                let &[lo, hi] = rng.pick(&arg.valid_ranges);
                let val = if mode == 1 && rng.chance(0.3) {
                    sample_outside_range(lo, hi, arg.kind, rng)
                } else {
                    sample_in_range(lo, hi, arg.kind, rng)
                };
                return hex_f64(val);
            }
            let val = match arg.kind {
                HintArgKind::Integer => rng.gen_i64_inclusive(-20, 100) as f64,
                HintArgKind::DateSerial => rng.gen_i64_inclusive(1, 60_000) as f64,
                HintArgKind::TimeFraction => rng.uniform(0.0, 1.0),
                _ => rng.log_uniform(-20, 20, true),
            };
            hex_f64(val)
        }
    }
}

fn read_arg_f64(args: &[Value], idx: usize) -> Option<f64> {
    parse_hex_bits(args.get(idx)?).map(f64::from_bits)
}

fn enforce_relation(
    args: &mut [Value],
    arg_specs: &[GenericArgSpec],
    rel: ParsedRelation,
    rng: &mut Rng,
) {
    let op = rel.op;
    match (rel.lhs, rel.rhs) {
        (RelOperand::Param(i), RelOperand::Param(j)) => {
            if i >= args.len() || j >= args.len() || i == j {
                return;
            }
            let (Some(a), Some(b)) = (read_arg_f64(args, i), read_arg_f64(args, j)) else {
                return;
            };
            let step = if matches!(
                arg_specs[i].kind,
                HintArgKind::Integer | HintArgKind::DateSerial
            ) {
                rng.gen_i64_inclusive(1, 365) as f64
            } else {
                rng.log_uniform(-10, 4, false)
            };
            match op {
                RelOp::Lt => {
                    if !(a < b) {
                        if a > b {
                            args.swap(i, j);
                        } else if let Some(hi) = value2_safe_f64(a + step) {
                            args[j] = hex_f64(hi);
                        }
                    }
                }
                RelOp::Le => {
                    if a > b {
                        args.swap(i, j);
                    }
                }
                RelOp::Gt => {
                    if !(a > b) {
                        if a < b {
                            args.swap(i, j);
                        } else if let Some(hi) = value2_safe_f64(b + step) {
                            args[i] = hex_f64(hi);
                        }
                    }
                }
                RelOp::Ge => {
                    if a < b {
                        args.swap(i, j);
                    }
                }
                RelOp::Eq => {
                    args[j] = hex_f64(a);
                }
                RelOp::Ne => {
                    if a == b
                        && let Some(hi) = value2_safe_f64(a + step)
                    {
                        args[j] = hex_f64(hi);
                    }
                }
            }
        }
        (RelOperand::Param(i), RelOperand::Const(c)) => {
            apply_param_const(args, arg_specs, i, op, c, rng);
        }
        (RelOperand::Const(c), RelOperand::Param(j)) => {
            apply_param_const(args, arg_specs, j, flip_rel_op(op), c, rng);
        }
        (RelOperand::AbsParam(i), RelOperand::Const(c)) => {
            apply_abs_param_const(args, arg_specs, i, op, c, rng);
        }
        (RelOperand::Const(c), RelOperand::AbsParam(j)) => {
            apply_abs_param_const(args, arg_specs, j, flip_rel_op(op), c, rng);
        }
        (RelOperand::AbsParam(i), RelOperand::Param(j)) => {
            if let Some(b) = read_arg_f64(args, j) {
                apply_abs_param_const(args, arg_specs, i, op, b, rng);
            }
        }
        (RelOperand::Param(i), RelOperand::AbsParam(j)) => {
            if let Some(a) = read_arg_f64(args, i) {
                apply_abs_param_const(args, arg_specs, j, flip_rel_op(op), a, rng);
            }
        }
        (RelOperand::AbsParam(i), RelOperand::AbsParam(j)) => {
            if i != j
                && let Some(b) = read_arg_f64(args, j)
            {
                apply_abs_param_const(args, arg_specs, i, op, b.abs(), rng);
            }
        }
        (RelOperand::Const(_), RelOperand::Const(_)) => {}
    }
}

fn flip_rel_op(op: RelOp) -> RelOp {
    match op {
        RelOp::Lt => RelOp::Gt,
        RelOp::Le => RelOp::Ge,
        RelOp::Gt => RelOp::Lt,
        RelOp::Ge => RelOp::Le,
        RelOp::Eq => RelOp::Eq,
        RelOp::Ne => RelOp::Ne,
    }
}

fn apply_abs_param_const(
    args: &mut [Value],
    arg_specs: &[GenericArgSpec],
    idx: usize,
    op: RelOp,
    c: f64,
    rng: &mut Rng,
) {
    if idx >= args.len() || !c.is_finite() {
        return;
    }
    let cur = read_arg_f64(args, idx).unwrap_or(0.0);
    let abs_cur = cur.abs();
    let is_int = matches!(
        arg_specs[idx].kind,
        HintArgKind::Integer | HintArgKind::DateSerial
    );
    let delta = if is_int {
        rng.gen_i64_inclusive(1, 20) as f64
    } else {
        rng.log_uniform(-12, 3, false)
    };
    let sign = if rng.chance(0.5) { 1.0 } else { -1.0 };
    let next = match op {
        RelOp::Lt => {
            if c <= 0.0 {
                return;
            }
            if abs_cur < c {
                cur
            } else {
                let mag = if is_int {
                    let hi = (c.ceil() - 1.0).clamp(0.0, 1_000_000.0) as i64;
                    rng.gen_i64_inclusive(0, hi) as f64
                } else {
                    let candidate = c * rng.next_f64() * 0.999_999;
                    if candidate < c {
                        candidate
                    } else {
                        step_ulps(c, -1, true).unwrap_or(0.0).max(0.0)
                    }
                };
                if mag == 0.0 { 0.0 } else { sign * mag }
            }
        }
        RelOp::Le => {
            if c < 0.0 {
                return;
            }
            if abs_cur <= c {
                cur
            } else if c == 0.0 {
                0.0
            } else if rng.chance(0.3) {
                sign * if is_int { c.floor() } else { c }
            } else {
                let mag = if is_int {
                    let hi = c.floor().clamp(0.0, 1_000_000.0) as i64;
                    rng.gen_i64_inclusive(0, hi) as f64
                } else {
                    c * rng.next_f64()
                };
                if mag == 0.0 { 0.0 } else { sign * mag }
            }
        }
        RelOp::Gt => {
            if abs_cur > c {
                cur
            } else {
                let base = c.max(0.0);
                let mag = if is_int {
                    base.floor() + delta
                } else {
                    base + delta
                };
                sign * mag
            }
        }
        RelOp::Ge => {
            if abs_cur >= c {
                cur
            } else {
                let base = c.max(0.0);
                let mag = if rng.chance(0.3) {
                    if is_int { base.ceil() } else { base }
                } else if is_int {
                    base.ceil() + delta
                } else {
                    base + delta
                };
                if mag == 0.0 { 0.0 } else { sign * mag }
            }
        }
        RelOp::Eq => {
            if c < 0.0 {
                return;
            }
            if abs_cur == c {
                cur
            } else if c == 0.0 {
                0.0
            } else {
                sign * c
            }
        }
        RelOp::Ne => {
            if c >= 0.0 && abs_cur == c {
                let mag = if is_int { c.floor() + delta } else { c + delta };
                sign * mag
            } else {
                cur
            }
        }
    };
    if let Some(safe) = value2_safe_f64(next) {
        args[idx] = hex_f64(safe);
    }
}

fn apply_param_const(
    args: &mut [Value],
    arg_specs: &[GenericArgSpec],
    idx: usize,
    op: RelOp,
    c: f64,
    rng: &mut Rng,
) {
    if idx >= args.len() {
        return;
    }
    let cur = read_arg_f64(args, idx).unwrap_or(c);
    let delta = if matches!(
        arg_specs[idx].kind,
        HintArgKind::Integer | HintArgKind::DateSerial
    ) {
        rng.gen_i64_inclusive(1, 20) as f64
    } else {
        rng.log_uniform(-12, 3, false)
    };
    let next = match op {
        RelOp::Lt if !(cur < c) => c - delta,
        RelOp::Le if !(cur <= c) => if rng.chance(0.5) { c } else { c - delta },
        RelOp::Gt if !(cur > c) => c + delta,
        RelOp::Ge if !(cur >= c) => if rng.chance(0.5) { c } else { c + delta },
        RelOp::Eq => c,
        RelOp::Ne if cur == c => c + if rng.chance(0.5) { delta } else { -delta },
        _ => cur,
    };
    if let Some(safe) = value2_safe_f64(next) {
        args[idx] = hex_f64(safe);
    }
}

/// Generate one deterministic argument vector for `merged` under `region`.
pub fn generate_spec_region(merged: &MergedSpec, region: SpecRegionKind, rng: &mut Rng) -> Vec<Value> {
    let arity = merged.pick_valid_arity(rng);
    if arity == 0 {
        return Vec::new();
    }
    match region {
        SpecRegionKind::GenericDomain => {
            let mut out: Vec<Value> = (0..arity)
                .map(|i| {
                    let arg = &merged.args[i];
                    if arg.optional && !arg.is_reference_only() && rng.chance(0.2) {
                        Value::Null
                    } else if merged.may_return_reference
                        && arg.class == ParameterClass::ValueOrReference
                        && arg.lifting.takes_whole_array()
                        && rng.chance(0.35)
                    {
                        let (r, c) = (2 + rng.gen_range(4), 1 + rng.gen_range(2));
                        gen_cell_range(r, c, rng, 2)
                    } else {
                        gen_arg_value(arg, rng, 2)
                    }
                })
                .collect();
            for &rel in &merged.parsed_relations {
                enforce_relation(&mut out, &merged.args, rel, rng);
            }
            out
        }
        SpecRegionKind::LiftingAndShapes => {
            // Exercise dc-dfl.2.6 lifting metadata: vary array/range shapes (1x1, 1xN, Nx1, RxC) on
            // whole-array/reference parameters while keeping elementwise-lifting parameters scalar
            // so single-cell Touchstone bulk calls do not spill unexpectedly.
            (0..arity)
                .map(|i| {
                    let arg = &merged.args[i];
                    if arg.is_reference_only() || arg.kind == HintArgKind::Reference {
                        let (r, c) = *rng.pick(&[(1, 1), (1, 3), (3, 1), (2, 2), (3, 2)]);
                        gen_cell_range(r, c, rng, 1)
                    } else if arg.lifting.takes_whole_array() || arg.kind == HintArgKind::Array {
                        let (r, c) = *rng.pick(&[(1, 1), (1, 3), (3, 1), (2, 2), (3, 2)]);
                        if merged.may_return_reference
                            && arg.class == ParameterClass::ValueOrReference
                            && rng.chance(0.5)
                        {
                            gen_cell_range(r, c, rng, 1)
                        } else {
                            gen_numeric_array(r, c, rng, 1)
                        }
                    } else {
                        gen_arg_value(arg, rng, 0)
                    }
                })
                .collect()
        }
        SpecRegionKind::HintAnchorsAndEdges => (0..arity)
            .map(|i| {
                let arg = &merged.args[i];
                if !arg.edges.is_empty() && rng.chance(0.5) {
                    let edge = rng.pick(&arg.edges);
                    let wkind = if arg.is_reference_only() {
                        HintArgKind::Reference
                    } else {
                        arg.kind
                    };
                    if let Some(w) = hint_value_to_witness(&edge.value, wkind) {
                        if arg.is_reference_only() || arg.kind == HintArgKind::Reference {
                            return wrap_as_reference(w);
                        }
                        if rng.chance(0.35)
                            && let Some(bits) = parse_hex_bits(&w)
                        {
                            let v = f64::from_bits(bits);
                            let d = *rng.pick(&[-4i64, -2, -1, 1, 2, 4]);
                            return hex_f64(step_ulps(v, d, true).unwrap_or(v));
                        }
                        return w;
                    }
                }
                gen_arg_value(arg, rng, 0)
            })
            .collect(),
        SpecRegionKind::HintRangesAndRelations => {
            let probe_outside = rng.chance(0.3);
            let mut out: Vec<Value> = (0..arity)
                .map(|i| {
                    let arg = &merged.args[i];
                    if !arg.valid_ranges.is_empty() && arg.kind.is_numeric() {
                        let &[lo, hi] = rng.pick(&arg.valid_ranges);
                        let val = if probe_outside && rng.chance(0.5) {
                            sample_outside_range(lo, hi, arg.kind, rng)
                        } else {
                            sample_in_range(lo, hi, arg.kind, rng)
                        };
                        hex_f64(val)
                    } else {
                        gen_arg_value(arg, rng, 1)
                    }
                })
                .collect();
            if !merged.parsed_relations.is_empty() {
                if rng.chance(0.65) {
                    // Honour all relations
                    for &rel in &merged.parsed_relations {
                        enforce_relation(&mut out, &merged.args, rel, rng);
                    }
                } else {
                    // Deliberately break one relation
                    let rel = *rng.pick(&merged.parsed_relations);
                    let broken = ParsedRelation {
                        lhs: rel.lhs,
                        op: rel.op.negate(),
                        rhs: rel.rhs,
                    };
                    enforce_relation(&mut out, &merged.args, broken, rng);
                }
            }
            out
        }
        SpecRegionKind::HintErrorEdges => {
            let mut out: Vec<Value> = (0..arity)
                .map(|i| gen_arg_value(&merged.args[i], rng, 2))
                .collect();
            if !merged.parsed_error_edges.is_empty() {
                let trigger = *rng.pick(&merged.parsed_error_edges);
                enforce_relation(&mut out, &merged.args, trigger, rng);
            }
            out
        }
    }
}

/// Validate that a generated argument vector has valid arity (including repeating `ArgumentGroups`),
/// Value2-exact doubles, and converts cleanly to Touchstone `Arg` values.
pub fn validate_generated_args(
    args: &[Value],
    min_arity: usize,
    max_arity: usize,
    groups: Option<ArgumentGroups>,
) -> Result<(), String> {
    if args.len() < min_arity || args.len() > max_arity {
        return Err(format!(
            "generated arity {} outside [{min_arity}, {max_arity}]",
            args.len()
        ));
    }
    if let Some(g) = groups
        && g.incomplete(args.len())
    {
        return Err(format!(
            "generated arity {} ends in an incomplete argument group (from={}, size={}, tail={})",
            args.len(),
            g.from,
            g.size,
            g.tail
        ));
    }
    for (idx, arg) in args.iter().enumerate() {
        json_arg_to_touchstone(arg)
            .map_err(|e| format!("arg {idx} failed Touchstone encoding: {e}"))?;
        if let Some(bits) = parse_hex_bits(arg) {
            let v = f64::from_bits(bits);
            if !is_value2_exact(v) {
                return Err(format!("arg {idx} is not Value2-exact: 0x{bits:016x}"));
            }
        }
    }
    Ok(())
}

/// Per-function result of a catalogue-wide dry run.
#[derive(Clone, Debug)]
pub struct FunctionDryRunEntry {
    pub function: String,
    pub min_arity: usize,
    pub max_arity: usize,
    pub regions: usize,
    pub has_hint_file: bool,
    pub hint_family: Option<String>,
    pub generated: usize,
    pub unique_generated: usize,
}

/// Aggregate report of a dry run over every `Notation::Call` function in `calc-catalogue`.
#[derive(Clone, Debug)]
pub struct AllCatalogueDryRunReport {
    pub seed: u64,
    pub cases_per_function: usize,
    pub functions_total: usize,
    pub functions_with_hints: usize,
    pub hint_files_loaded: Vec<String>,
    pub total_cases_generated: usize,
    pub entries: Vec<FunctionDryRunEntry>,
}

impl AllCatalogueDryRunReport {
    pub fn to_json(&self) -> Value {
        json!({
            "seed": self.seed,
            "cases_per_function": self.cases_per_function,
            "functions_total": self.functions_total,
            "functions_with_hints": self.functions_with_hints,
            "hint_files_loaded": self.hint_files_loaded,
            "total_cases_generated": self.total_cases_generated,
            "functions": self.entries.iter().map(|e| json!({
                "function": e.function,
                "min_arity": e.min_arity,
                "max_arity": e.max_arity,
                "regions": e.regions,
                "has_hint_file": e.has_hint_file,
                "hint_family": e.hint_family,
                "generated": e.generated,
                "unique_generated": e.unique_generated,
            })).collect::<Vec<_>>(),
        })
    }
}

/// Generate and validate `cases_per_function` deterministic cases for EVERY `Notation::Call`
/// function in `calc-catalogue` using its merged [`super::domain::DomainSpec`].
pub fn run_all_catalogue_dry_run(
    hints: &HintBank,
    cases_per_function: usize,
    seed: u64,
) -> Result<AllCatalogueDryRunReport, String> {
    let functions = all_catalogue_call_functions();
    let mut entries = Vec::with_capacity(functions.len());
    let mut total_cases = 0usize;
    let mut with_hints = 0usize;

    for func in &functions {
        let spec = super::domain::DomainSpec::for_function_with_hints(func, hints)?;
        if spec.regions.is_empty() {
            return Err(format!("{func}: spec has no regions"));
        }
        let mut rng = Rng::from_seed_and_tag(seed, func);
        let mut unique: BTreeSet<String> = BTreeSet::new();
        for i in 0..cases_per_function {
            let region = &spec.regions[i % spec.regions.len()];
            let args = spec.generate(region.kind, &mut rng);
            validate_generated_args(&args, spec.min_arity, spec.max_arity, spec.merged.groups)
                .map_err(|e| format!("{func} case {i} (region {}): {e}", region.name))?;
            unique.insert(super::rng::canonical_case_key(func, &args));
        }
        if spec.has_hint_file {
            with_hints += 1;
        }
        total_cases += cases_per_function;
        entries.push(FunctionDryRunEntry {
            function: func.clone(),
            min_arity: spec.min_arity,
            max_arity: spec.max_arity,
            regions: spec.regions.len(),
            has_hint_file: spec.has_hint_file,
            hint_family: spec.merged.hint_family.clone(),
            generated: cases_per_function,
            unique_generated: unique.len(),
        });
    }

    Ok(AllCatalogueDryRunReport {
        seed,
        cases_per_function,
        functions_total: functions.len(),
        functions_with_hints: with_hints,
        hint_files_loaded: hints.loaded_files().to_vec(),
        total_cases_generated: total_cases,
        entries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generic_spec_covers_every_catalogue_function_with_lifting_and_groups() {
        let bank = HintBank::empty();
        let fns = all_catalogue_call_functions();
        assert!(
            fns.len() >= 500,
            "expected >=500 Notation::Call functions in calc-catalogue, got {}",
            fns.len()
        );

        // Verify dc-dfl.2.6 lifting metadata and repeating groups on SUMIFS & COUNTIF.
        let sumifs = build_merged_spec("SUMIFS", &bank).unwrap();
        assert!(sumifs.groups.is_some());
        assert_eq!(sumifs.args[0].kind, HintArgKind::Reference);
        assert!(sumifs.args[0].lifting.takes_whole_array());
        assert_eq!(sumifs.args[1].kind, HintArgKind::Reference);
        assert_eq!(sumifs.args[2].kind, HintArgKind::Criteria);
        assert!(sumifs.args[2].lifting.kernel_lifts);
        assert!(sumifs.args[2].lifting.lifts_elementwise());
        // Repeated group slots inherit criteria_range / criteria properly:
        assert_eq!(sumifs.args[3].kind, HintArgKind::Reference);
        assert_eq!(sumifs.args[4].kind, HintArgKind::Criteria);

        // Verify zero-arity function PI works cleanly.
        let pi = build_merged_spec("PI", &bank).unwrap();
        assert_eq!(pi.min_arity, 0);
        assert_eq!(pi.max_arity, 0);
        let mut rng = Rng::from_seed(1);
        let args = generate_spec_region(&pi, SpecRegionKind::GenericDomain, &mut rng);
        assert!(args.is_empty());
        validate_generated_args(&args, pi.min_arity, pi.max_arity, pi.groups).unwrap();
    }

    #[test]
    fn hint_merge_overrides_kinds_merges_specials_and_honours_and_breaks_relations() {
        let json = r##"{
  "family": "financial-securities",
  "version": 1,
  "functions": {
    "YIELD": {
      "args": [
        {"name": "settlement", "kind": "date-serial", "specials": [60], "ranges": [[40000, 45000]],
         "edges": [{"value": 60, "why": "1900 leap-year bug day", "source": "handbook"}]},
        {"name": "maturity", "kind": "date-serial", "specials": [46000], "ranges": [[40001, 50000]], "edges": []},
        {"name": "rate", "kind": "number", "specials": [0, 1e-300], "ranges": [[0, 1]], "edges": []}
      ],
      "relations": ["settlement < maturity"],
      "regimes": [{"name": "one coupon left", "how": "n == 1", "source": "Excel documentation"}],
      "error_edges": [{"when": "rate < 0", "excel": "#NUM!", "source": "Excel documentation"}],
      "notes": "facts"
    }
  }
}"##;
        let file = HintBank::parse_file_str("securities.json", json).unwrap();
        let bank = HintBank::from_files(vec![("securities.json".into(), file)]).unwrap();
        let merged = build_merged_spec("YIELD", &bank).unwrap();
        assert!(merged.has_hint_file);
        assert_eq!(merged.hint_family.as_deref(), Some("financial-securities"));
        assert!(merged.has_hint_anchors());
        assert!(merged.has_hint_ranges_or_relations());
        assert!(merged.has_hint_error_edges());

        let mut rng = Rng::from_seed(42);
        let mut honoured = 0usize;
        let mut broken = 0usize;
        for _ in 0..200 {
            let args = generate_spec_region(&merged, SpecRegionKind::HintRangesAndRelations, &mut rng);
            validate_generated_args(&args, merged.min_arity, merged.max_arity, merged.groups).unwrap();
            let s = f64::from_bits(parse_hex_bits(&args[0]).unwrap());
            let m = f64::from_bits(parse_hex_bits(&args[1]).unwrap());
            if s < m {
                honoured += 1;
            } else {
                broken += 1;
            }
        }
        assert!(honoured > 40, "expected honoured settlement < maturity cases, got {honoured}");
        assert!(broken > 15, "expected deliberately broken settlement >= maturity cases, got {broken}");

        // Error edge triggers rate < 0
        let mut neg_rates = 0usize;
        for _ in 0..50 {
            let args = generate_spec_region(&merged, SpecRegionKind::HintErrorEdges, &mut rng);
            validate_generated_args(&args, merged.min_arity, merged.max_arity, merged.groups).unwrap();
            let r = f64::from_bits(parse_hex_bits(&args[2]).unwrap());
            if r < 0.0 {
                neg_rates += 1;
            }
        }
        assert!(neg_rates >= 45, "expected error-edge rate < 0 triggers, got {neg_rates}/50");
    }

    #[test]
    fn order_tsv_lists_every_catalogue_function_with_unique_ranks() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/fuzz/order.tsv");
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let mut header_seen = false;
        let mut ranked_fns: BTreeSet<String> = BTreeSet::new();
        let mut expected_rank = 1usize;
        for line in text.lines() {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if !header_seen {
                assert!(line.starts_with("rank\tfunction\t"));
                header_seen = true;
                continue;
            }
            let cols: Vec<&str> = line.split('\t').collect();
            assert_eq!(cols.len(), 19, "unexpected column count in order.tsv line: {line}");
            let rank: usize = cols[0].parse().expect("numeric rank");
            assert_eq!(rank, expected_rank, "ranks must be contiguous 1..=N");
            expected_rank += 1;
            assert!(
                ranked_fns.insert(cols[1].to_owned()),
                "duplicate function in order.tsv: {}",
                cols[1]
            );
        }
        let catalogue_fns: BTreeSet<String> = all_catalogue_call_functions().into_iter().collect();
        assert_eq!(
            ranked_fns, catalogue_fns,
            "docs/fuzz/order.tsv must cover every Notation::Call function in calc-catalogue 1:1"
        );
    }

    #[test]
    fn all_catalogue_functions_generate_1000_valid_cases_without_error() {
        let bank = super::super::hints::load_default_hints().expect("load default hints");
        let report = run_all_catalogue_dry_run(&bank, 1_000, 20261124)
            .expect("1,000-case dry run across all catalogue functions must succeed");
        assert_eq!(report.functions_total, all_catalogue_call_functions().len());
        assert_eq!(
            report.total_cases_generated,
            report.functions_total * 1_000
        );
    }

    #[test]
    fn grouped_functions_only_generate_valid_arities_across_all_regions() {
        let bank = super::super::hints::load_default_hints().expect("load default hints");
        for func in ["SUMIFS", "AVERAGEIFS", "MAXIFS", "MINIFS", "COUNTIFS", "IFS", "SORTBY", "SWITCH"] {
            let spec = super::super::domain::DomainSpec::for_function_with_hints(func, &bank).unwrap();
            let groups = spec
                .merged
                .groups
                .unwrap_or_else(|| panic!("{func} should declare ArgumentGroups"));
            let mut rng = Rng::from_seed_and_tag(20261124, func);
            for region in &spec.regions {
                for _ in 0..40 {
                    let args = spec.generate(region.kind, &mut rng);
                    assert!(
                        !groups.incomplete(args.len()),
                        "{func} region {} generated incomplete grouped arity {} (groups={groups:?})",
                        region.name,
                        args.len()
                    );
                    validate_generated_args(&args, spec.min_arity, spec.max_arity, Some(groups))
                        .unwrap_or_else(|e| panic!("{func} region {} failed validation: {e}", region.name));
                }
            }
        }

        // Explicitly verify validate_generated_args rejects incomplete grouped arities and accepts valid ones.
        let sumifs = build_merged_spec("SUMIFS", &bank).unwrap();
        let four_args = vec![hex_f64(1.0); 4];
        assert!(
            validate_generated_args(&four_args, sumifs.min_arity, sumifs.max_arity, sumifs.groups).is_err(),
            "SUMIFS must reject even arity 4"
        );
        let three_args = vec![hex_f64(1.0); 3];
        assert!(
            validate_generated_args(&three_args, sumifs.min_arity, sumifs.max_arity, sumifs.groups).is_ok(),
            "SUMIFS must accept odd arity 3"
        );
        let countifs = build_merged_spec("COUNTIFS", &bank).unwrap();
        assert!(
            validate_generated_args(&three_args, countifs.min_arity, countifs.max_arity, countifs.groups).is_err(),
            "COUNTIFS must reject odd arity 3"
        );
        let sortby = build_merged_spec("SORTBY", &bank).unwrap();
        let two_args = vec![hex_f64(1.0); 2];
        assert!(
            validate_generated_args(&two_args, sortby.min_arity, sortby.max_arity, sortby.groups).is_ok(),
            "SORTBY must accept 2-arg form (array, by_array1)"
        );
        assert!(
            validate_generated_args(&four_args, sortby.min_arity, sortby.max_arity, sortby.groups).is_err(),
            "SORTBY must reject incomplete 4-arg form"
        );
    }

    #[test]
    fn array_and_reference_hint_specials_are_emitted_end_to_end() {
        use super::super::domain::{DomainSpec, RegionKind};

        let bank = super::super::hints::load_default_hints().expect("load default hints");
        let empty_range = json!({ "range": [[Value::Null]] });

        // All 7 functions with empty-array `[]` specials in committed hint files must actually
        // emit `{"range": [[null]]}` via DomainSpec::generate.
        for (func, arg_idx) in [
            ("WORKDAY", 2usize),
            ("WORKDAY.INTL", 3usize),
            ("NETWORKDAYS", 2usize),
            ("NETWORKDAYS.INTL", 3usize),
            ("Z.TEST", 0usize),
            ("ZTEST", 0usize),
            ("ARRAYTOTEXT", 0usize),
        ] {
            let spec = DomainSpec::for_function_with_hints(func, &bank).unwrap();
            let mut rng = Rng::from_seed_and_tag(20261124, func);
            let mut saw_empty = false;
            for _ in 0..300 {
                let args = spec.generate(RegionKind::SpecHintAnchorsAndEdges, &mut rng);
                validate_generated_args(&args, spec.min_arity, spec.max_arity, spec.merged.groups)
                    .unwrap();
                if args.get(arg_idx) == Some(&empty_range) {
                    saw_empty = true;
                    break;
                }
            }
            assert!(
                saw_empty,
                "{func} arg[{arg_idx}] never emitted empty-array special {empty_range} via DomainSpec::generate"
            );
        }

        // Reference parameter with hint specials must emit them as CellRange via DomainSpec::generate.
        let ref_hint_json = r#"{
  "family": "test-ref",
  "version": 1,
  "functions": {
    "SUMIF": {
      "args": [
        {"name": "range", "kind": "reference", "specials": [[[10, 20], [30, 40]], []], "ranges": [], "edges": []}
      ],
      "relations": [],
      "regimes": [],
      "error_edges": [],
      "notes": ""
    }
  }
}"#;
        let file = HintBank::parse_file_str("test-ref.json", ref_hint_json).unwrap();
        let ref_bank = HintBank::from_files(vec![("test-ref.json".into(), file)]).unwrap();
        let sumif = DomainSpec::for_function_with_hints("SUMIF", &ref_bank).unwrap();
        let expected_matrix = json!({
            "range": [
                [hex_f64(10.0), hex_f64(20.0)],
                [hex_f64(30.0), hex_f64(40.0)]
            ]
        });
        let mut rng = Rng::from_seed(77);
        let mut saw_matrix = false;
        let mut saw_blank = false;
        for _ in 0..100 {
            let args = sumif.generate(RegionKind::SpecHintAnchorsAndEdges, &mut rng);
            validate_generated_args(&args, sumif.min_arity, sumif.max_arity, sumif.merged.groups)
                .unwrap();
            if args[0] == expected_matrix {
                saw_matrix = true;
            }
            if args[0] == empty_range {
                saw_blank = true;
            }
        }
        assert!(saw_matrix, "SUMIF reference special 2x2 matrix was never emitted");
        assert!(saw_blank, "SUMIF reference special [] was never emitted");
    }

    #[test]
    fn abs_relations_and_error_edges_enforce_accurately() {
        // 1. Conjunctive abs(rate) > 10 and abs(pr) == 5 must never contradict each other.
        let conj_json = r#"{
  "family": "test-abs-conj",
  "version": 1,
  "functions": {
    "YIELD": {
      "args": [
        {"name": "rate", "kind": "number", "specials": [], "ranges": [[-50, 50]], "edges": []},
        {"name": "pr", "kind": "number", "specials": [], "ranges": [[-20, 20]], "edges": []},
        {"name": "redemption", "kind": "number", "specials": [], "ranges": [[-100, 100]], "edges": []}
      ],
      "relations": ["abs(rate) > 10", "abs(pr) == 5", "abs(redemption) < 2"],
      "regimes": [],
      "error_edges": [],
      "notes": ""
    }
  }
}"#;
        let file = HintBank::parse_file_str("abs-conj.json", conj_json).unwrap();
        let bank = HintBank::from_files(vec![("abs-conj.json".into(), file)]).unwrap();
        let merged = build_merged_spec("YIELD", &bank).unwrap();
        let mut rng = Rng::from_seed(123);
        let mut saw_pos_rate = false;
        let mut saw_neg_rate = false;
        for _ in 0..80 {
            let args = generate_spec_region(&merged, SpecRegionKind::GenericDomain, &mut rng);
            let rate = f64::from_bits(parse_hex_bits(&args[2]).unwrap());
            let pr = f64::from_bits(parse_hex_bits(&args[3]).unwrap());
            let red = f64::from_bits(parse_hex_bits(&args[4]).unwrap());
            assert!(rate.abs() > 10.0, "expected |rate| > 10, got {rate}");
            if rate > 10.0 {
                saw_pos_rate = true;
            }
            if rate < -10.0 {
                saw_neg_rate = true;
            }
            assert!(
                (pr.abs() - 5.0).abs() == 0.0,
                "expected |pr| == 5, got {pr}"
            );
            assert!(red.abs() < 2.0, "expected |redemption| < 2, got {red}");
        }
        assert!(saw_pos_rate && saw_neg_rate, "expected both positive and negative branches of |rate| > 10");

        // 2. Error edge `abs(rate) < 2` must enforce BOTH -2 < rate < 2 on every generated trigger.
        let ee_json = r##"{
  "family": "test-abs-ee",
  "version": 1,
  "functions": {
    "YIELD": {
      "args": [
        {"name": "rate", "kind": "number", "specials": [], "ranges": [[-1000, 1000]], "edges": []}
      ],
      "relations": [],
      "regimes": [],
      "error_edges": [{"when": "abs(rate) < 2", "excel": "#NUM!", "source": "test"}],
      "notes": ""
    }
  }
}"##;
        let ee_file = HintBank::parse_file_str("abs-ee.json", ee_json).unwrap();
        let ee_bank = HintBank::from_files(vec![("abs-ee.json".into(), ee_file)]).unwrap();
        let ee_merged = build_merged_spec("YIELD", &ee_bank).unwrap();
        for _ in 0..80 {
            let args = generate_spec_region(&ee_merged, SpecRegionKind::HintErrorEdges, &mut rng);
            let rate = f64::from_bits(parse_hex_bits(&args[2]).unwrap());
            assert!(
                rate.abs() < 2.0,
                "error_edge abs(rate) < 2 produced out-of-bounds rate = {rate}"
            );
        }
    }
}
