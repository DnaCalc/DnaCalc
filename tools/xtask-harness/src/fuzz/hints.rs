//! Loader and validator for per-family fuzz domain hint files (`dc-ztvv.26.1`).
//!
//! Hint files live in `tools/xtask-harness/fuzz/hints/<family>.json` (schema version 1) and are
//! merged on top of the generic per-function specs generated from `calc-catalogue` metadata and
//! `dc-dfl.2.6` per-parameter lifting (`spec_gen.rs`).

use super::rng::{hex_f64, parse_hex_bits};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Repository-relative directory holding per-family hint JSON files.
pub const DEFAULT_HINTS_REL_DIR: &str = "tools/xtask-harness/fuzz/hints";

/// Supported schema version for hint JSON files.
pub const HINT_SCHEMA_VERSION: u32 = 1;

/// Canonical argument kind vocabulary shared by all domain hint files and `spec_gen.rs`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HintArgKind {
    Number,
    Integer,
    DateSerial,
    TimeFraction,
    Text,
    Logical,
    Reference,
    Array,
    Criteria,
    Any,
}

impl HintArgKind {
    /// Stable kebab-case spelling matching the shared hint format.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Number => "number",
            Self::Integer => "integer",
            Self::DateSerial => "date-serial",
            Self::TimeFraction => "time-fraction",
            Self::Text => "text",
            Self::Logical => "logical",
            Self::Reference => "reference",
            Self::Array => "array",
            Self::Criteria => "criteria",
            Self::Any => "any",
        }
    }

    /// Whether this argument kind is fundamentally numeric (can take intervals, ULPs, relations).
    pub const fn is_numeric(self) -> bool {
        matches!(
            self,
            Self::Number | Self::Integer | Self::DateSerial | Self::TimeFraction
        )
    }
}

/// One documented or code-derived boundary value where behaviour changes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EdgeHint {
    pub value: Value,
    #[serde(default)]
    pub why: String,
    #[serde(default)]
    pub source: String,
}

/// Per-argument domain hints inside a function hint entry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArgHint {
    pub name: String,
    pub kind: HintArgKind,
    #[serde(default)]
    pub specials: Vec<Value>,
    #[serde(default)]
    pub ranges: Vec<[f64; 2]>,
    #[serde(default)]
    pub edges: Vec<EdgeHint>,
}

/// Named region of the input space where a different formula or code route applies.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegimeHint {
    pub name: String,
    #[serde(default)]
    pub how: String,
    #[serde(default)]
    pub source: String,
}

/// Input condition where Excel returns a worksheet error.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorEdgeHint {
    pub when: String,
    #[serde(default)]
    pub excel: String,
    #[serde(default)]
    pub source: String,
}

/// Hint specification for one worksheet function.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FunctionHint {
    #[serde(default)]
    pub args: Vec<ArgHint>,
    #[serde(default)]
    pub relations: Vec<String>,
    #[serde(default)]
    pub regimes: Vec<RegimeHint>,
    #[serde(default)]
    pub error_edges: Vec<ErrorEdgeHint>,
    #[serde(default)]
    pub notes: String,
}

/// Top-level JSON document for one function family (`<family>.json`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HintFile {
    pub family: String,
    pub version: u32,
    pub functions: BTreeMap<String, FunctionHint>,
}

/// Loaded hint entry retaining provenance (family name and source file).
#[derive(Clone, Debug, PartialEq)]
pub struct LoadedFunctionHint {
    pub family: String,
    pub source_file: String,
    pub hint: FunctionHint,
}

/// Deterministic in-memory collection of all loaded hint files.
#[derive(Clone, Debug, Default)]
pub struct HintBank {
    functions: BTreeMap<String, LoadedFunctionHint>,
    loaded_files: Vec<String>,
}

impl HintBank {
    /// Empty hint bank (every function uses its generic catalogue spec alone).
    pub fn empty() -> Self {
        Self::default()
    }

    /// Look up hints for `function` (ASCII case-insensitive).
    pub fn get(&self, function: &str) -> Option<&LoadedFunctionHint> {
        let key = function.trim().to_ascii_uppercase();
        self.functions.get(&key)
    }

    /// Number of functions covered by loaded hint files.
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.functions.len()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.functions.is_empty()
    }

    /// Sorted list of loaded source filenames.
    pub fn loaded_files(&self) -> &[String] {
        &self.loaded_files
    }

    /// Iterate over all loaded function hints in deterministic spelling order.
    #[allow(dead_code)]
    pub fn iter(&self) -> impl Iterator<Item = (&str, &LoadedFunctionHint)> {
        self.functions.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// Parse one hint JSON string and validate its schema and catalogue references.
    pub fn parse_file_str(source_label: &str, raw: &str) -> Result<HintFile, String> {
        let file: HintFile = serde_json::from_str(raw)
            .map_err(|e| format!("{source_label}: invalid hint JSON: {e}"))?;
        validate_hint_file(source_label, &file)?;
        Ok(file)
    }

    /// Assemble and validate a [`HintBank`] from `(source_label, HintFile)` pairs.
    pub fn from_files(files: Vec<(String, HintFile)>) -> Result<Self, String> {
        let mut bank = Self::empty();
        for (source_label, file) in files {
            validate_hint_file(&source_label, &file)?;
            bank.loaded_files.push(source_label.clone());
            for (raw_name, hint) in file.functions {
                let name = raw_name.trim().to_ascii_uppercase();
                if let Some(prev) = bank.functions.get(&name) {
                    return Err(format!(
                        "{source_label}: duplicate hint for function {name} (already defined in {})",
                        prev.source_file
                    ));
                }
                bank.functions.insert(
                    name,
                    LoadedFunctionHint {
                        family: file.family.clone(),
                        source_file: source_label.clone(),
                        hint,
                    },
                );
            }
        }
        Ok(bank)
    }

    /// Load every `*.json` file in `dir` in deterministic sorted filename order.
    /// Returns an empty bank if `dir` does not exist yet.
    pub fn load_dir(dir: &Path) -> Result<Self, String> {
        if !dir.exists() {
            return Ok(Self::empty());
        }
        if !dir.is_dir() {
            return Err(format!("{}: hints path is not a directory", dir.display()));
        }
        let mut paths: Vec<PathBuf> = fs::read_dir(dir)
            .map_err(|e| format!("read {}: {e}", dir.display()))?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|p| {
                p.is_file()
                    && p.extension()
                        .and_then(|ext| ext.to_str())
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
            })
            .collect();
        paths.sort();

        let mut parsed = Vec::with_capacity(paths.len());
        for path in paths {
            let label = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown.json")
                .to_owned();
            let raw = fs::read_to_string(&path)
                .map_err(|e| format!("read {}: {e}", path.display()))?;
            let file = Self::parse_file_str(&label, &raw)?;
            parsed.push((label, file));
        }
        Self::from_files(parsed)
    }
}

/// Normalize a parameter name for matching hint entries against `calc-catalogue` declarations.
pub fn normalize_hint_param_name(s: &str) -> String {
    s.trim()
        .trim_matches(|c| matches!(c, '[' | ']' | '(' | ')' | '`' | '"' | '\''))
        .to_ascii_lowercase()
        .replace([' ', '-'], "_")
}

fn strip_trailing_digits(s: &str) -> &str {
    s.trim_end_matches(|c: char| c.is_ascii_digit())
}

/// Check whether a hint argument name matches a parameter declared on `decl` in `calc-catalogue`
/// (exact normalized match, or numbered repeat of a variadic/grouped parameter such as `logical2`
/// for `logical1` when `decl.arity().max > decl.parameters().len()`).
pub fn hint_param_matches_decl(decl: calc_catalogue::DeclarationRef, arg_name: &str) -> bool {
    let norm = normalize_hint_param_name(arg_name);
    if norm.is_empty() {
        return false;
    }
    let params = decl.parameters();
    if params
        .iter()
        .any(|p| normalize_hint_param_name(p.name) == norm)
    {
        return true;
    }
    // Allow numbered repeats (e.g. `logical2`, `criteria_range2`, `criteria2`) on variadic/grouped
    // declarations when the stem matches a repeating parameter in `decl.parameters()`.
    if decl.arity().max > params.len() && !params.is_empty() {
        let stem = strip_trailing_digits(&norm);
        if !stem.is_empty() && stem.len() < norm.len() {
            let repeat_start = decl
                .groups()
                .map_or(params.len().saturating_sub(1), |g| g.from.min(params.len() - 1));
            if params[repeat_start..]
                .iter()
                .any(|p| strip_trailing_digits(&normalize_hint_param_name(p.name)) == stem)
            {
                return true;
            }
        }
    }
    false
}

/// Validate a single parsed [`HintFile`] against schema rules and `calc-catalogue`.
pub fn validate_hint_file(source_label: &str, file: &HintFile) -> Result<(), String> {
    if file.version != HINT_SCHEMA_VERSION {
        return Err(format!(
            "{source_label}: unsupported hint schema version {} (expected {HINT_SCHEMA_VERSION})",
            file.version
        ));
    }
    if file.family.trim().is_empty() {
        return Err(format!("{source_label}: hint file family must not be empty"));
    }
    for (raw_name, fhint) in &file.functions {
        let name = raw_name.trim().to_ascii_uppercase();
        let Some(decl) = calc_catalogue::named(&name) else {
            return Err(format!(
                "{source_label}: function {raw_name:?} is not a named call in calc-catalogue"
            ));
        };
        for arg in &fhint.args {
            if arg.name.trim().is_empty() {
                return Err(format!(
                    "{source_label}: {name} has an argument hint with an empty name"
                ));
            }
            if !hint_param_matches_decl(decl, &arg.name) {
                let valid: Vec<&str> = decl.parameters().iter().map(|p| p.name).collect();
                return Err(format!(
                    "{source_label}: {name} has unknown parameter hint {:?} (calc-catalogue parameters: {:?})",
                    arg.name, valid
                ));
            }
            for &[lo, hi] in &arg.ranges {
                if !lo.is_finite() || !hi.is_finite() {
                    return Err(format!(
                        "{source_label}: {name}.{} has non-finite range [{lo}, {hi}]",
                        arg.name
                    ));
                }
                if lo > hi {
                    return Err(format!(
                        "{source_label}: {name}.{} has inverted range [{lo}, {hi}]",
                        arg.name
                    ));
                }
            }
            for (idx, sv) in arg.specials.iter().enumerate() {
                let Some(w) = hint_value_to_witness(sv, arg.kind) else {
                    return Err(format!(
                        "{source_label}: {name}.{} special[{idx}] cannot be converted to a Value2/Touchstone witness: {sv}",
                        arg.name
                    ));
                };
                super::oracle::json_arg_to_touchstone(&w).map_err(|e| {
                    format!(
                        "{source_label}: {name}.{} special[{idx}] failed Touchstone encoding ({w}): {e}",
                        arg.name
                    )
                })?;
            }
            for (idx, edge) in arg.edges.iter().enumerate() {
                let Some(w) = hint_value_to_witness(&edge.value, arg.kind) else {
                    return Err(format!(
                        "{source_label}: {name}.{} edge[{idx}] value cannot be converted to a Value2/Touchstone witness: {}",
                        arg.name, edge.value
                    ));
                };
                super::oracle::json_arg_to_touchstone(&w).map_err(|e| {
                    format!(
                        "{source_label}: {name}.{} edge[{idx}] failed Touchstone encoding ({w}): {e}",
                        arg.name
                    )
                })?;
            }
        }
        for (idx, rel) in fhint.relations.iter().enumerate() {
            if rel.trim().is_empty() {
                return Err(format!(
                    "{source_label}: {name} relation[{idx}] must not be empty"
                ));
            }
        }
        for (idx, ee) in fhint.error_edges.iter().enumerate() {
            if ee.when.trim().is_empty() {
                return Err(format!(
                    "{source_label}: {name} error_edges[{idx}].when must not be empty"
                ));
            }
        }
        let mut param_names: Vec<String> = decl
            .parameters()
            .iter()
            .map(|p| p.name.to_owned())
            .collect();
        for arg in &fhint.args {
            let norm = normalize_hint_param_name(&arg.name);
            if !param_names
                .iter()
                .any(|p| normalize_hint_param_name(p) == norm)
            {
                param_names.push(arg.name.clone());
            }
        }
        let parsed_rels = parse_relations(&fhint.relations, &param_names);
        validate_conjunctive_relations(source_label, &name, &param_names, &parsed_rels)?;
    }
    Ok(())
}

fn validate_conjunctive_relations(
    source_label: &str,
    func_name: &str,
    param_names: &[String],
    rels: &[ParsedRelation],
) -> Result<(), String> {
    #[derive(Default)]
    struct Bounds {
        lo: Option<(f64, bool)>,
        hi: Option<(f64, bool)>,
        eq: Option<f64>,
        ne: Vec<f64>,
        abs_lo: Option<(f64, bool)>,
        abs_hi: Option<(f64, bool)>,
        abs_eq: Option<f64>,
        abs_ne: Vec<f64>,
    }

    fn tighten_lo(slot: &mut Option<(f64, bool)>, v: f64, strict: bool) {
        match *slot {
            None => *slot = Some((v, strict)),
            Some((cur, cur_strict)) => {
                if v > cur || (v == cur && strict && !cur_strict) {
                    *slot = Some((v, strict));
                }
            }
        }
    }

    fn tighten_hi(slot: &mut Option<(f64, bool)>, v: f64, strict: bool) {
        match *slot {
            None => *slot = Some((v, strict)),
            Some((cur, cur_strict)) => {
                if v < cur || (v == cur && strict && !cur_strict) {
                    *slot = Some((v, strict));
                }
            }
        }
    }

    let mut per_param: BTreeMap<usize, Bounds> = BTreeMap::new();
    for &rel in rels {
        match (rel.lhs, rel.rhs) {
            (RelOperand::Param(i), RelOperand::Param(j)) if i == j => {
                if matches!(rel.op, RelOp::Lt | RelOp::Gt | RelOp::Ne) {
                    let pname = param_names.get(i).map_or("?", String::as_str);
                    return Err(format!(
                        "{source_label}: {func_name} has unsatisfiable self-relation on parameter {pname:?}"
                    ));
                }
            }
            (RelOperand::AbsParam(i), RelOperand::AbsParam(j)) if i == j => {
                if matches!(rel.op, RelOp::Lt | RelOp::Gt | RelOp::Ne) {
                    let pname = param_names.get(i).map_or("?", String::as_str);
                    return Err(format!(
                        "{source_label}: {func_name} has unsatisfiable abs self-relation on parameter {pname:?}"
                    ));
                }
            }
            (RelOperand::Param(i), RelOperand::Const(c)) => {
                let b = per_param.entry(i).or_default();
                match rel.op {
                    RelOp::Gt => tighten_lo(&mut b.lo, c, true),
                    RelOp::Ge => tighten_lo(&mut b.lo, c, false),
                    RelOp::Lt => tighten_hi(&mut b.hi, c, true),
                    RelOp::Le => tighten_hi(&mut b.hi, c, false),
                    RelOp::Eq => {
                        if let Some(prev) = b.eq
                            && prev != c
                        {
                            let pname = param_names.get(i).map_or("?", String::as_str);
                            return Err(format!(
                                "{source_label}: {func_name} has contradictory equality relations on {pname:?}: == {prev} and == {c}"
                            ));
                        }
                        b.eq = Some(c);
                        tighten_lo(&mut b.lo, c, false);
                        tighten_hi(&mut b.hi, c, false);
                    }
                    RelOp::Ne => b.ne.push(c),
                }
            }
            (RelOperand::Const(c), RelOperand::Param(j)) => {
                let op = flip_op(rel.op);
                let b = per_param.entry(j).or_default();
                match op {
                    RelOp::Gt => tighten_lo(&mut b.lo, c, true),
                    RelOp::Ge => tighten_lo(&mut b.lo, c, false),
                    RelOp::Lt => tighten_hi(&mut b.hi, c, true),
                    RelOp::Le => tighten_hi(&mut b.hi, c, false),
                    RelOp::Eq => {
                        if let Some(prev) = b.eq
                            && prev != c
                        {
                            let pname = param_names.get(j).map_or("?", String::as_str);
                            return Err(format!(
                                "{source_label}: {func_name} has contradictory equality relations on {pname:?}: == {prev} and == {c}"
                            ));
                        }
                        b.eq = Some(c);
                        tighten_lo(&mut b.lo, c, false);
                        tighten_hi(&mut b.hi, c, false);
                    }
                    RelOp::Ne => b.ne.push(c),
                }
            }
            (RelOperand::AbsParam(i), RelOperand::Const(c)) => {
                let b = per_param.entry(i).or_default();
                match rel.op {
                    RelOp::Gt => tighten_lo(&mut b.abs_lo, c, true),
                    RelOp::Ge => tighten_lo(&mut b.abs_lo, c, false),
                    RelOp::Lt => tighten_hi(&mut b.abs_hi, c, true),
                    RelOp::Le => tighten_hi(&mut b.abs_hi, c, false),
                    RelOp::Eq => {
                        if let Some(prev) = b.abs_eq
                            && prev != c
                        {
                            let pname = param_names.get(i).map_or("?", String::as_str);
                            return Err(format!(
                                "{source_label}: {func_name} has contradictory abs equality relations on {pname:?}: == {prev} and == {c}"
                            ));
                        }
                        b.abs_eq = Some(c);
                        tighten_lo(&mut b.abs_lo, c, false);
                        tighten_hi(&mut b.abs_hi, c, false);
                    }
                    RelOp::Ne => b.abs_ne.push(c),
                }
            }
            (RelOperand::Const(c), RelOperand::AbsParam(j)) => {
                let op = flip_op(rel.op);
                let b = per_param.entry(j).or_default();
                match op {
                    RelOp::Gt => tighten_lo(&mut b.abs_lo, c, true),
                    RelOp::Ge => tighten_lo(&mut b.abs_lo, c, false),
                    RelOp::Lt => tighten_hi(&mut b.abs_hi, c, true),
                    RelOp::Le => tighten_hi(&mut b.abs_hi, c, false),
                    RelOp::Eq => {
                        if let Some(prev) = b.abs_eq
                            && prev != c
                        {
                            let pname = param_names.get(j).map_or("?", String::as_str);
                            return Err(format!(
                                "{source_label}: {func_name} has contradictory abs equality relations on {pname:?}: == {prev} and == {c}"
                            ));
                        }
                        b.abs_eq = Some(c);
                        tighten_lo(&mut b.abs_lo, c, false);
                        tighten_hi(&mut b.abs_hi, c, false);
                    }
                    RelOp::Ne => b.abs_ne.push(c),
                }
            }
            _ => {}
        }
    }

    for (idx, b) in per_param {
        let pname = param_names.get(idx).map_or("?", String::as_str);
        if let (Some((lo, lo_s)), Some((hi, hi_s))) = (b.lo, b.hi)
            && (lo > hi || (lo == hi && (lo_s || hi_s)))
        {
            return Err(format!(
                "{source_label}: {func_name} has contradictory conjunctive relations on {pname:?} (lo={lo}, hi={hi})"
            ));
        }
        if let Some(eq) = b.eq
            && b.ne.contains(&eq)
        {
            return Err(format!(
                "{source_label}: {func_name} has contradictory relations on {pname:?} (== {eq} and != {eq})"
            ));
        }
        if let Some((abs_hi, abs_hi_s)) = b.abs_hi
            && (abs_hi < 0.0 || (abs_hi == 0.0 && abs_hi_s))
        {
            return Err(format!(
                "{source_label}: {func_name} has unsatisfiable abs upper bound on {pname:?} ({abs_hi})"
            ));
        }
        if let (Some((alo, alo_s)), Some((ahi, ahi_s))) = (b.abs_lo, b.abs_hi)
            && (alo > ahi || (alo == ahi && (alo_s || ahi_s)))
        {
            return Err(format!(
                "{source_label}: {func_name} has contradictory conjunctive abs relations on {pname:?} (lo={alo}, hi={ahi})"
            ));
        }
        if let Some(aeq) = b.abs_eq
            && (aeq < 0.0 || b.abs_ne.contains(&aeq))
        {
            return Err(format!(
                "{source_label}: {func_name} has contradictory abs equality relations on {pname:?} ({aeq})"
            ));
        }
        // Check cross-contradiction between signed bounds [lo, hi] and |x| <= abs_hi.
        if let Some((ahi, ahi_s)) = b.abs_hi {
            if let Some((lo, lo_s)) = b.lo
                && (lo > ahi || (lo == ahi && (lo_s || ahi_s)))
            {
                return Err(format!(
                    "{source_label}: {func_name} has contradictory relations on {pname:?} (lo={lo} vs abs_hi={ahi})"
                ));
            }
            if let Some((hi, hi_s)) = b.hi
                && (hi < -ahi || (hi == -ahi && (hi_s || ahi_s)))
            {
                return Err(format!(
                    "{source_label}: {func_name} has contradictory relations on {pname:?} (hi={hi} vs -abs_hi={})",
                    -ahi
                ));
            }
        }
        // Check cross-contradiction between signed bounds [lo, hi] and |x| >= abs_lo.
        if let (Some((alo, alo_s)), Some((lo, lo_s)), Some((hi, hi_s))) = (b.abs_lo, b.lo, b.hi) {
            let lo_inside = lo > -alo || (lo == -alo && (lo_s || alo_s));
            let hi_inside = hi < alo || (hi == alo && (hi_s || alo_s));
            if lo_inside && hi_inside {
                return Err(format!(
                    "{source_label}: {func_name} has contradictory relations on {pname:?} ([{lo}, {hi}] inside |x| < {alo})"
                ));
            }
        }
    }
    Ok(())
}

/// Locate the default repository hints directory (`tools/xtask-harness/fuzz/hints`).
pub fn default_hints_dir() -> PathBuf {
    let rel = PathBuf::from(DEFAULT_HINTS_REL_DIR);
    if rel.is_dir() {
        return rel;
    }
    let manifest_rel = Path::new(env!("CARGO_MANIFEST_DIR")).join("fuzz/hints");
    if manifest_rel.is_dir() {
        return manifest_rel;
    }
    rel
}

/// Load the default repository hint bank from `tools/xtask-harness/fuzz/hints/*.json`.
pub fn load_default_hints() -> Result<HintBank, String> {
    HintBank::load_dir(&default_hints_dir())
}

// ============================================================================
// Hint value normalization & simple algebraic relation parsing
// ============================================================================

fn normalize_excel_error_literal(text: &str) -> Option<&'static str> {
    match text.trim().to_ascii_uppercase().as_str() {
        "#NULL!" => Some("Null"),
        "#DIV/0!" => Some("Div0"),
        "#VALUE!" => Some("Value"),
        "#REF!" => Some("Ref"),
        "#NAME?" => Some("Name"),
        "#NUM!" => Some("Num"),
        "#N/A" => Some("NA"),
        _ => None,
    }
}

/// Normalize a finite f64 from a hint JSON file into a Touchstone/Value2-exact double.
pub fn value2_safe_f64(v: f64) -> Option<f64> {
    if !v.is_finite() {
        return None;
    }
    if v == 0.0 {
        return Some(0.0);
    }
    if v.is_subnormal() {
        return Some(if v.is_sign_negative() {
            -f64::MIN_POSITIVE
        } else {
            f64::MIN_POSITIVE
        });
    }
    Some(v)
}

fn hint_cell_to_witness(cell: &Value, kind: HintArgKind) -> Option<Value> {
    match cell {
        Value::Null => Some(Value::Null),
        Value::Bool(b) => Some(Value::Bool(*b)),
        Value::Number(n) => {
            let f = value2_safe_f64(n.as_f64()?)?;
            Some(hex_f64(f))
        }
        Value::String(s) => {
            if let Some(bits) = parse_hex_bits(cell) {
                let safe = value2_safe_f64(f64::from_bits(bits))?;
                return Some(hex_f64(safe));
            }
            if !matches!(kind, HintArgKind::Text | HintArgKind::Criteria)
                && let Some(err_var) = normalize_excel_error_literal(s)
            {
                return Some(json!({ "error": err_var }));
            }
            Some(Value::String(s.clone()))
        }
        Value::Object(map) => {
            if let Some(code) = map.get("error").and_then(Value::as_str) {
                let norm = normalize_excel_error_literal(code).unwrap_or(code);
                Some(json!({ "error": norm }))
            } else {
                None
            }
        }
        Value::Array(_) => None,
    }
}

/// Convert a JSON value from a hint file's `specials` or `edges[i].value` into a WitnessSet /
/// Touchstone-compatible argument JSON value.
pub fn hint_value_to_witness(value: &Value, kind: HintArgKind) -> Option<Value> {
    match value {
        Value::Null => Some(Value::Null),
        Value::Bool(b) => Some(Value::Bool(*b)),
        Value::Number(n) => {
            let f = value2_safe_f64(n.as_f64()?)?;
            Some(hex_f64(f))
        }
        Value::String(s) => {
            if let Some(bits) = parse_hex_bits(value) {
                let safe = value2_safe_f64(f64::from_bits(bits))?;
                return Some(hex_f64(safe));
            }
            if !matches!(kind, HintArgKind::Text | HintArgKind::Criteria)
                && let Some(err_var) = normalize_excel_error_literal(s)
            {
                return Some(json!({ "error": err_var }));
            }
            if kind.is_numeric()
                && let Ok(parsed) = s.trim().parse::<f64>()
                && let Some(safe) = value2_safe_f64(parsed)
            {
                return Some(hex_f64(safe));
            }
            Some(Value::String(s.clone()))
        }
        Value::Object(map) => {
            if let Some(code) = map.get("error").and_then(Value::as_str) {
                let norm = normalize_excel_error_literal(code).unwrap_or(code);
                Some(json!({ "error": norm }))
            } else {
                None
            }
        }
        Value::Array(items) => {
            if items.is_empty() {
                // Represent an empty array/holiday list hint as a 1x1 blank worksheet range,
                // which encodes cleanly in both Touchstone `r1x1` and DNA Calc witness replay.
                return Some(json!({ "range": [[Value::Null]] }));
            }
            let mut rows: Vec<Vec<Value>> = Vec::with_capacity(items.len());
            let mut all_numeric = true;
            if items[0].is_array() {
                let cols = items[0].as_array()?.len();
                if cols == 0 {
                    return Some(json!({ "range": [[Value::Null]] }));
                }
                for row in items {
                    let cells = row.as_array()?;
                    if cells.len() != cols {
                        return None;
                    }
                    let mut out_row = Vec::with_capacity(cols);
                    for cell in cells {
                        let converted = hint_cell_to_witness(cell, kind)?;
                        if parse_hex_bits(&converted).is_none() {
                            all_numeric = false;
                        }
                        out_row.push(converted);
                    }
                    rows.push(out_row);
                }
            } else {
                for cell in items {
                    let converted = hint_cell_to_witness(cell, kind)?;
                    if parse_hex_bits(&converted).is_none() {
                        all_numeric = false;
                    }
                    rows.push(vec![converted]);
                }
            }
            let grid: Vec<Value> = rows.into_iter().map(Value::Array).collect();
            if all_numeric && kind != HintArgKind::Reference {
                Some(Value::Array(grid))
            } else {
                Some(json!({ "range": grid }))
            }
        }
    }
}

/// Comparison operator inside a parsed hint relation or `error_edges[i].when` clause.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelOp {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

impl RelOp {
    /// Logical negation of the comparison (used to deliberately break a relation).
    pub const fn negate(self) -> Self {
        match self {
            Self::Lt => Self::Ge,
            Self::Le => Self::Gt,
            Self::Gt => Self::Le,
            Self::Ge => Self::Lt,
            Self::Eq => Self::Ne,
            Self::Ne => Self::Eq,
        }
    }
}

/// Operand of a parsed binary relation: either a parameter index, `abs(param)`, or a numeric constant.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RelOperand {
    Param(usize),
    AbsParam(usize),
    Const(f64),
}

/// One parsed binary constraint extracted from a hint `relations` or `error_edges` string.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParsedRelation {
    pub lhs: RelOperand,
    pub op: RelOp,
    pub rhs: RelOperand,
}

fn strip_fn_wrapper<'a>(token: &'a str, wrappers: &[&str]) -> (&'a str, bool) {
    let mut cur = token.trim();
    let mut had_abs = false;
    loop {
        let lower = cur.to_ascii_lowercase();
        let mut unwrapped = false;
        for w in wrappers {
            if lower.starts_with(w) && cur.ends_with(')') {
                let inner = &cur[w.len()..cur.len() - 1];
                // Ensure parentheses inside `inner` stay balanced so `abs(a) + abs(b)` isn't mis-stripped.
                let mut depth = 0i32;
                let mut balanced = true;
                for ch in inner.chars() {
                    match ch {
                        '(' => depth += 1,
                        ')' => {
                            depth -= 1;
                            if depth < 0 {
                                balanced = false;
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                if balanced && depth == 0 {
                    if *w == "abs(" {
                        had_abs = true;
                    }
                    cur = inner.trim();
                    unwrapped = true;
                    break;
                }
            }
        }
        if !unwrapped {
            break;
        }
    }
    (cur, had_abs)
}

fn resolve_operand_with_abs(token: &str, arg_names: &[String]) -> Option<(RelOperand, bool)> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return None;
    }
    let (inner, is_abs) = strip_fn_wrapper(trimmed, &["abs(", "trunc(", "int(", "floor(", "ceil(", "round("]);
    let clean = inner.trim().trim_matches(|c| matches!(c, '(' | ')' | '`'));
    if clean.is_empty() {
        return None;
    }
    if let Ok(num) = clean.parse::<f64>()
        && num.is_finite()
    {
        return Some((RelOperand::Const(if is_abs { num.abs() } else { num }), false));
    }
    let norm = normalize_hint_param_name(clean);
    for (idx, name) in arg_names.iter().enumerate() {
        if normalize_hint_param_name(name) == norm {
            return Some((RelOperand::Param(idx), is_abs));
        }
    }
    None
}

fn flip_op(op: RelOp) -> RelOp {
    match op {
        RelOp::Lt => RelOp::Gt,
        RelOp::Le => RelOp::Ge,
        RelOp::Gt => RelOp::Lt,
        RelOp::Ge => RelOp::Le,
        RelOp::Eq => RelOp::Eq,
        RelOp::Ne => RelOp::Ne,
    }
}

fn emit_abs_relation(
    out: &mut Vec<ParsedRelation>,
    param_idx: usize,
    op: RelOp,
    bound: f64,
    split_or: bool,
) {
    if !bound.is_finite() {
        return;
    }
    let param = RelOperand::Param(param_idx);
    let abs_param = RelOperand::AbsParam(param_idx);
    match op {
        RelOp::Gt => {
            if bound < 0.0 {
                return;
            }
            if split_or {
                out.push(ParsedRelation {
                    lhs: param,
                    op: RelOp::Gt,
                    rhs: RelOperand::Const(bound),
                });
                out.push(ParsedRelation {
                    lhs: param,
                    op: RelOp::Lt,
                    rhs: RelOperand::Const(-bound),
                });
            } else if bound == 0.0 {
                out.push(ParsedRelation {
                    lhs: param,
                    op: RelOp::Ne,
                    rhs: RelOperand::Const(0.0),
                });
            } else {
                out.push(ParsedRelation {
                    lhs: abs_param,
                    op: RelOp::Gt,
                    rhs: RelOperand::Const(bound),
                });
            }
        }
        RelOp::Ge => {
            if bound <= 0.0 {
                return;
            }
            if split_or {
                out.push(ParsedRelation {
                    lhs: param,
                    op: RelOp::Ge,
                    rhs: RelOperand::Const(bound),
                });
                out.push(ParsedRelation {
                    lhs: param,
                    op: RelOp::Le,
                    rhs: RelOperand::Const(-bound),
                });
            } else {
                out.push(ParsedRelation {
                    lhs: abs_param,
                    op: RelOp::Ge,
                    rhs: RelOperand::Const(bound),
                });
            }
        }
        RelOp::Lt if bound > 0.0 => {
            // `abs(x) < bound` is a conjunction `-bound < x < bound`; emit a single atomic
            // AbsParam constraint in both conjunctive (`relations`) and disjunctive (`error_edges`)
            // modes so neither side is enforced in isolation or clobbered by sequential updates.
            out.push(ParsedRelation {
                lhs: abs_param,
                op: RelOp::Lt,
                rhs: RelOperand::Const(bound),
            });
        }
        RelOp::Le if bound >= 0.0 => {
            if bound == 0.0 {
                out.push(ParsedRelation {
                    lhs: param,
                    op: RelOp::Eq,
                    rhs: RelOperand::Const(0.0),
                });
            } else {
                out.push(ParsedRelation {
                    lhs: abs_param,
                    op: RelOp::Le,
                    rhs: RelOperand::Const(bound),
                });
            }
        }
        RelOp::Eq if bound >= 0.0 => {
            if bound == 0.0 {
                out.push(ParsedRelation {
                    lhs: param,
                    op: RelOp::Eq,
                    rhs: RelOperand::Const(0.0),
                });
            } else if split_or {
                out.push(ParsedRelation {
                    lhs: param,
                    op: RelOp::Eq,
                    rhs: RelOperand::Const(bound),
                });
                out.push(ParsedRelation {
                    lhs: param,
                    op: RelOp::Eq,
                    rhs: RelOperand::Const(-bound),
                });
            } else {
                out.push(ParsedRelation {
                    lhs: abs_param,
                    op: RelOp::Eq,
                    rhs: RelOperand::Const(bound),
                });
            }
        }
        RelOp::Ne if bound >= 0.0 => {
            if bound == 0.0 {
                out.push(ParsedRelation {
                    lhs: param,
                    op: RelOp::Ne,
                    rhs: RelOperand::Const(0.0),
                });
            } else {
                out.push(ParsedRelation {
                    lhs: abs_param,
                    op: RelOp::Ne,
                    rhs: RelOperand::Const(bound),
                });
            }
        }
        _ => {}
    }
}

fn split_clauses(expr: &str, split_or: bool) -> Vec<String> {
    let mut parts: Vec<String> = expr
        .split([';', ','])
        .flat_map(|s| s.split("&&"))
        .map(str::to_owned)
        .collect();
    if split_or {
        parts = parts
            .into_iter()
            .flat_map(|s| s.split("||").map(str::to_owned).collect::<Vec<_>>())
            .collect();
    }
    let mut out = Vec::new();
    for part in parts {
        // Split on case-insensitive word-delimited ` and ` (and ` or ` when `split_or`).
        let mut cur = String::new();
        let words: Vec<&str> = part.split_whitespace().collect();
        for w in words {
            let lw = w.to_ascii_lowercase();
            let is_sep = lw == "and" || (split_or && lw == "or");
            if is_sep {
                if !cur.trim().is_empty() {
                    out.push(cur.trim().to_owned());
                    cur.clear();
                }
            } else {
                if !cur.is_empty() {
                    cur.push(' ');
                }
                cur.push_str(w);
            }
        }
        if !cur.trim().is_empty() {
            out.push(cur.trim().to_owned());
        }
    }
    out
}

fn parse_relation_expr_impl(expr: &str, arg_names: &[String], split_or: bool) -> Vec<ParsedRelation> {
    let mut out = Vec::new();
    let mut last_lhs: Option<(RelOperand, bool)> = None;
    for clause in split_clauses(expr, split_or) {
        let chars: Vec<char> = clause.chars().collect();
        let mut tokens: Vec<String> = Vec::new();
        let mut ops: Vec<RelOp> = Vec::new();
        let mut start = 0usize;
        let mut i = 0usize;
        while i < chars.len() {
            let two = if i + 1 < chars.len() {
                Some((chars[i], chars[i + 1]))
            } else {
                None
            };
            let matched = match two {
                Some(('<', '=')) => Some((RelOp::Le, 2)),
                Some(('>', '=')) => Some((RelOp::Ge, 2)),
                Some(('!', '=')) | Some(('<', '>')) => Some((RelOp::Ne, 2)),
                Some(('=', '=')) => Some((RelOp::Eq, 2)),
                _ => match chars[i] {
                    '<' => Some((RelOp::Lt, 1)),
                    '>' => Some((RelOp::Gt, 1)),
                    '=' => Some((RelOp::Eq, 1)),
                    _ => None,
                },
            };
            if let Some((op, width)) = matched {
                tokens.push(chars[start..i].iter().collect());
                ops.push(op);
                i += width;
                start = i;
            } else {
                i += 1;
            }
        }
        tokens.push(chars[start..].iter().collect());
        if ops.is_empty() || tokens.len() != ops.len() + 1 {
            continue;
        }
        for pair_idx in 0..ops.len() {
            let lhs_opt = if pair_idx == 0 && tokens[0].trim().is_empty() {
                last_lhs
            } else {
                resolve_operand_with_abs(&tokens[pair_idx], arg_names)
            };
            let rhs_opt = resolve_operand_with_abs(&tokens[pair_idx + 1], arg_names);
            if let (Some((lhs, lhs_abs)), Some((rhs, rhs_abs))) = (lhs_opt, rhs_opt) {
                if pair_idx == 0 && matches!(lhs, RelOperand::Param(_)) {
                    last_lhs = Some((lhs, lhs_abs));
                }
                if lhs_abs
                    && let RelOperand::Param(i) = lhs
                    && let RelOperand::Const(c) = rhs
                {
                    emit_abs_relation(&mut out, i, ops[pair_idx], c, split_or);
                } else if rhs_abs
                    && let RelOperand::Param(j) = rhs
                    && let RelOperand::Const(c) = lhs
                {
                    emit_abs_relation(&mut out, j, flip_op(ops[pair_idx]), c, split_or);
                } else if matches!(lhs, RelOperand::Param(_)) || matches!(rhs, RelOperand::Param(_)) {
                    let lhs_op = match lhs {
                        RelOperand::Param(i) if lhs_abs => RelOperand::AbsParam(i),
                        other => other,
                    };
                    let rhs_op = match rhs {
                        RelOperand::Param(j) if rhs_abs => RelOperand::AbsParam(j),
                        other => other,
                    };
                    out.push(ParsedRelation {
                        lhs: lhs_op,
                        op: ops[pair_idx],
                        rhs: rhs_op,
                    });
                }
            }
        }
    }
    out
}

/// Parse simple, compound, and wrapped comparisons (`a < b`, `issue < settlement < maturity`,
/// `year < 0 or year >= 10000`, `abs(x) > 32767`, `trunc(end_date) < trunc(start_date)`,
/// `serial_number < 0 or >= 2958466`) against `arg_names`.
/// Free-text clauses that cannot be parsed as numeric comparisons are silently skipped.
pub fn parse_relation_expr(expr: &str, arg_names: &[String]) -> Vec<ParsedRelation> {
    parse_relation_expr_impl(expr, arg_names, true)
}

/// Parse conjunctive `relations` entries (splitting on `and`/`&&`/`,`/`;`, never enforcing
/// mutually exclusive `or` branches simultaneously).
pub fn parse_relations(relations: &[String], arg_names: &[String]) -> Vec<ParsedRelation> {
    let mut out = Vec::new();
    for r in relations {
        out.extend(parse_relation_expr_impl(r, arg_names, false));
    }
    out
}

/// Parse `error_edges[i].when` trigger conditions (splitting `or`/`||` disjuncts into individual
/// candidate triggers for `HintErrorEdges`).
pub fn parse_error_edge_triggers(whens: &[String], arg_names: &[String]) -> Vec<ParsedRelation> {
    let mut out = Vec::new();
    for w in whens {
        out.extend(parse_relation_expr_impl(w, arg_names, true));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_YIELD_HINTS: &str = r##"{
  "family": "financial-securities",
  "version": 1,
  "functions": {
    "YIELD": {
      "args": [
        {"name": "settlement", "kind": "date-serial", "specials": [1, 59, 60, 61, 2958465], "ranges": [[1, 2958465]],
         "edges": [{"value": 60, "why": "1900 leap-year bug day", "source": "handbook YIELD notes"}]},
        {"name": "rate", "kind": "number", "specials": [0, 1e-300], "ranges": [[0, 1]], "edges": []}
      ],
      "relations": ["settlement < maturity"],
      "regimes": [{"name": "one coupon left", "how": "maturity within one coupon period of settlement", "source": "Excel documentation"}],
      "error_edges": [{"when": "rate < 0", "excel": "#NUM!", "source": "Excel documentation"}],
      "notes": "short, facts only"
    }
  }
}"##;

    #[test]
    fn parses_format_example_and_all_ten_arg_kinds() {
        let file = HintBank::parse_file_str("financial-securities.json", SAMPLE_YIELD_HINTS)
            .expect("valid sample hint file");
        assert_eq!(file.family, "financial-securities");
        assert_eq!(file.version, 1);
        let bank = HintBank::from_files(vec![("financial-securities.json".into(), file)])
            .expect("build bank");
        assert_eq!(bank.len(), 1);
        let yld = bank.get("yield").expect("case-insensitive lookup");
        assert_eq!(yld.hint.args.len(), 2);
        assert_eq!(yld.hint.args[0].kind, HintArgKind::DateSerial);
        assert_eq!(yld.hint.args[1].kind, HintArgKind::Number);
        assert_eq!(yld.hint.args[0].edges.len(), 1);
        assert_eq!(yld.hint.relations, vec!["settlement < maturity"]);

        for (raw, expected) in [
            ("number", HintArgKind::Number),
            ("integer", HintArgKind::Integer),
            ("date-serial", HintArgKind::DateSerial),
            ("time-fraction", HintArgKind::TimeFraction),
            ("text", HintArgKind::Text),
            ("logical", HintArgKind::Logical),
            ("reference", HintArgKind::Reference),
            ("array", HintArgKind::Array),
            ("criteria", HintArgKind::Criteria),
            ("any", HintArgKind::Any),
        ] {
            let decoded: HintArgKind =
                serde_json::from_str(&format!("\"{raw}\"")).expect("decode arg kind");
            assert_eq!(decoded, expected);
            assert_eq!(decoded.as_str(), raw);
        }
    }

    #[test]
    fn rejects_bad_version_unknown_function_inverted_range_and_duplicates() {
        let bad_ver = SAMPLE_YIELD_HINTS.replace("\"version\": 1", "\"version\": 2");
        assert!(HintBank::parse_file_str("bad.json", &bad_ver).is_err());

        let unknown_fn = SAMPLE_YIELD_HINTS.replace("\"YIELD\"", "\"NOT_A_REAL_EXCEL_FUNC\"");
        assert!(HintBank::parse_file_str("unknown.json", &unknown_fn).is_err());

        let bad_range = SAMPLE_YIELD_HINTS.replace("[[0, 1]]", "[[2, 1]]");
        assert!(HintBank::parse_file_str("range.json", &bad_range).is_err());

        let f1 = HintBank::parse_file_str("a.json", SAMPLE_YIELD_HINTS).unwrap();
        let f2 = HintBank::parse_file_str("b.json", SAMPLE_YIELD_HINTS).unwrap();
        let dup = HintBank::from_files(vec![("a.json".into(), f1), ("b.json".into(), f2)]);
        assert!(dup.is_err());
    }

    #[test]
    fn parses_chained_and_constant_relations() {
        let names: Vec<String> = ["issue", "settlement", "maturity", "rate"]
            .into_iter()
            .map(str::to_owned)
            .collect();
        let rels = parse_relations(
            &[
                "issue < settlement < maturity".into(),
                "rate >= 0".into(),
                "free text note without comparison".into(),
            ],
            &names,
        );
        assert_eq!(rels.len(), 3);
        assert_eq!(
            rels[0],
            ParsedRelation {
                lhs: RelOperand::Param(0),
                op: RelOp::Lt,
                rhs: RelOperand::Param(1),
            }
        );
        assert_eq!(
            rels[1],
            ParsedRelation {
                lhs: RelOperand::Param(1),
                op: RelOp::Lt,
                rhs: RelOperand::Param(2),
            }
        );
        assert_eq!(
            rels[2],
            ParsedRelation {
                lhs: RelOperand::Param(3),
                op: RelOp::Ge,
                rhs: RelOperand::Const(0.0),
            }
        );
    }

    #[test]
    fn load_dir_handles_missing_dir_and_sorted_json_files() {
        let missing = std::env::temp_dir().join("dnacalc-hints-missing-dir-does-not-exist");
        let empty = HintBank::load_dir(&missing).expect("missing dir yields empty bank");
        assert!(empty.is_empty());

        let dir = std::env::temp_dir().join("dnacalc-hints-loader-test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("01-securities.json"), SAMPLE_YIELD_HINTS).unwrap();
        let pmt_json = r#"{
  "family": "financial-tvm",
  "version": 1,
  "functions": {
    "PMT": {
      "args": [{"name": "rate", "kind": "number", "specials": [0, 0.05], "ranges": [[-0.99, 10]], "edges": []}],
      "relations": ["nper != 0"],
      "regimes": [],
      "error_edges": [],
      "notes": ""
    }
  }
}"#;
        fs::write(dir.join("02-tvm.json"), pmt_json).unwrap();
        fs::write(dir.join("README.txt"), "ignored non-json").unwrap();

        let bank = HintBank::load_dir(&dir).expect("load dir");
        assert_eq!(bank.len(), 2);
        assert_eq!(
            bank.loaded_files(),
            &["01-securities.json".to_owned(), "02-tvm.json".to_owned()]
        );
        assert!(bank.get("YIELD").is_some());
        assert!(bank.get("PMT").is_some());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn validates_param_names_hex_literals_non_numeric_arrays_and_compound_error_edges() {
        // 1. Unknown parameter name must be rejected by validate_hint_file.
        let bad_param = SAMPLE_YIELD_HINTS.replace("\"settlement\"", "\"not_a_yield_param\"");
        assert!(
            HintBank::parse_file_str("bad-param.json", &bad_param).is_err(),
            "validate_hint_file must reject parameter names not in calc-catalogue"
        );

        // 2. Hex literals must pass through value2_safe_f64 (flush subnormals & -0.0, reject Inf/NaN).
        assert_eq!(
            hint_value_to_witness(&json!("0x8000000000000000"), HintArgKind::Number),
            Some(json!("0x0000000000000000")),
            "-0.0 hex literal must normalize to +0.0"
        );
        assert_eq!(
            hint_value_to_witness(&json!("0x0000000000000001"), HintArgKind::Number),
            Some(hex_f64(f64::MIN_POSITIVE)),
            "subnormal hex literal must flush to MIN_POSITIVE"
        );
        assert_eq!(
            hint_value_to_witness(&json!("0x7ff0000000000000"), HintArgKind::Number),
            None,
            "+Inf hex literal must be rejected"
        );
        assert_eq!(
            hint_value_to_witness(&json!("0x7ff8000000000000"), HintArgKind::Number),
            None,
            "NaN hex literal must be rejected"
        );

        // 3. Non-numeric and empty array specials must convert to Touchstone-compatible values (not None).
        for (raw, kind) in [
            (json!([]), HintArgKind::Array),
            (json!([true, false]), HintArgKind::Logical),
            (json!(["-", ","]), HintArgKind::Text),
            (json!([1, null, true]), HintArgKind::Any),
        ] {
            let w = hint_value_to_witness(&raw, kind)
                .unwrap_or_else(|| panic!("expected non-None witness for {raw} ({kind:?})"));
            super::super::oracle::json_arg_to_touchstone(&w)
                .unwrap_or_else(|e| panic!("converted witness {w} for {raw} failed Touchstone encoding: {e}"));
        }

        // 4. parse_relation_expr must handle `or`, `abs(...)`, `trunc(...)`, and shorthand RHS clauses.
        let names: Vec<String> = ["year", "start_date", "end_date", "x"]
            .into_iter()
            .map(str::to_owned)
            .collect();
        let r1 = parse_relation_expr("year < 0 or year >= 10000", &names);
        assert_eq!(r1.len(), 2, "expected 2 clauses from `or`: {r1:?}");
        let r2 = parse_relation_expr("trunc(end_date) < trunc(start_date) or start_date < 0", &names);
        assert_eq!(r2.len(), 2, "expected 2 clauses from `trunc(...)` + `or`: {r2:?}");
        let r3 = parse_relation_expr("abs(x) > 32767", &names);
        assert_eq!(r3.len(), 2, "expected 2 clauses from `abs(x) > 32767`: {r3:?}");
        let r4 = parse_relation_expr("start_date < 0 or >= 2958466", &names);
        assert_eq!(r4.len(), 2, "expected 2 clauses from shorthand `or >=`: {r4:?}");

        // 5. Across all committed repository hint files, zero specials/edges are silently dropped
        //    and compound `or`/`abs`/`trunc` error_edges parse cleanly.
        let bank = load_default_hints().expect("all committed hint files validate cleanly");
        let mut total_specials = 0usize;
        let mut dropped_specials = 0usize;
        let mut total_error_edges = 0usize;
        let mut parsed_error_edges = 0usize;
        for (func_name, loaded) in &bank.functions {
            let decl = calc_catalogue::named(func_name).unwrap();
            let arg_names: Vec<String> = decl.parameters().iter().map(|p| p.name.to_owned()).collect();
            for arg in &loaded.hint.args {
                for sv in &arg.specials {
                    total_specials += 1;
                    if hint_value_to_witness(sv, arg.kind).is_none() {
                        dropped_specials += 1;
                    }
                }
                for eh in &arg.edges {
                    total_specials += 1;
                    if hint_value_to_witness(&eh.value, arg.kind).is_none() {
                        dropped_specials += 1;
                    }
                }
            }
            for ee in &loaded.hint.error_edges {
                total_error_edges += 1;
                if !parse_error_edge_triggers(std::slice::from_ref(&ee.when), &arg_names).is_empty() {
                    parsed_error_edges += 1;
                }
            }
        }
        eprintln!(
            "committed hints audit: specials={total_specials} dropped={dropped_specials}, error_edges={parsed_error_edges}/{total_error_edges}"
        );
        assert_eq!(
            dropped_specials, 0,
            "zero committed hint specials/edges may be silently dropped"
        );
        assert!(
            parsed_error_edges * 2 >= total_error_edges,
            "expected >=50% of committed error_edges to parse (got {parsed_error_edges}/{total_error_edges})"
        );
    }

    #[test]
    fn validate_hint_file_rejects_bad_specials_edges_and_contradictory_relations() {
        let inf_special = SAMPLE_YIELD_HINTS.replace(
            "\"specials\": [0, 1e-300]",
            "\"specials\": [\"0x7ff0000000000000\"]",
        );
        assert!(
            HintBank::parse_file_str("inf-special.json", &inf_special).is_err(),
            "validate_hint_file must reject +Inf hex literal in specials"
        );

        let nan_edge = SAMPLE_YIELD_HINTS.replace(
            "\"edges\": [{\"value\": 60,",
            "\"edges\": [{\"value\": \"0x7ff8000000000000\",",
        );
        assert!(
            HintBank::parse_file_str("nan-edge.json", &nan_edge).is_err(),
            "validate_hint_file must reject NaN hex literal in edges"
        );

        let jagged_special = SAMPLE_YIELD_HINTS.replace(
            "\"specials\": [0, 1e-300]",
            "\"specials\": [[[1, 2], [3]]]",
        );
        assert!(
            HintBank::parse_file_str("jagged.json", &jagged_special).is_err(),
            "validate_hint_file must reject jagged array in specials"
        );

        let bad_edge_obj = SAMPLE_YIELD_HINTS.replace(
            "\"edges\": [{\"value\": 60,",
            "\"edges\": [{\"value\": {\"unsupported\": 1},",
        );
        assert!(
            HintBank::parse_file_str("bad-obj.json", &bad_edge_obj).is_err(),
            "validate_hint_file must reject unsupported object in edges"
        );

        let contradictory = SAMPLE_YIELD_HINTS.replace(
            "\"relations\": [\"settlement < maturity\"]",
            "\"relations\": [\"rate > 10\", \"rate < 5\"]",
        );
        assert!(
            HintBank::parse_file_str("contradictory.json", &contradictory).is_err(),
            "validate_hint_file must reject contradictory conjunctive relations on a parameter"
        );

        let contradictory_eq = SAMPLE_YIELD_HINTS.replace(
            "\"relations\": [\"settlement < maturity\"]",
            "\"relations\": [\"rate == 1\", \"rate == 2\"]",
        );
        assert!(
            HintBank::parse_file_str("contradictory-eq.json", &contradictory_eq).is_err(),
            "validate_hint_file must reject contradictory equality relations"
        );

        // parse_relations must not emit contradictory conjunctive bounds for abs(x) > c or == c.
        let names = vec!["x".to_owned()];
        let abs_gt = parse_relations(&["abs(x) > 10".to_owned()], &names);
        assert_eq!(
            abs_gt.len(),
            1,
            "conjunctive abs(x) > 10 must emit a single non-contradictory relation, got {abs_gt:?}"
        );
        let abs_eq = parse_relations(&["abs(x) == 5".to_owned()], &names);
        assert_eq!(
            abs_eq.len(),
            1,
            "conjunctive abs(x) == 5 must emit a single non-contradictory relation, got {abs_eq:?}"
        );

        // parse_error_edge_triggers must not split conjunctive abs(x) < c / <= c into incomplete disjuncts.
        let abs_lt_ee = parse_error_edge_triggers(&["abs(x) < 10".to_owned()], &names);
        assert_eq!(
            abs_lt_ee.len(),
            1,
            "error_edges abs(x) < 10 must emit a single atomic trigger, got {abs_lt_ee:?}"
        );
        let abs_le_ee = parse_error_edge_triggers(&["abs(x) <= 10".to_owned()], &names);
        assert_eq!(
            abs_le_ee.len(),
            1,
            "error_edges abs(x) <= 10 must emit a single atomic trigger, got {abs_le_ee:?}"
        );
    }
}
