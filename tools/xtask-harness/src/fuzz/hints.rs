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
        if calc_catalogue::named(&name).is_none() {
            return Err(format!(
                "{source_label}: function {raw_name:?} is not a named call in calc-catalogue"
            ));
        }
        for arg in &fhint.args {
            if arg.name.trim().is_empty() {
                return Err(format!(
                    "{source_label}: {name} has an argument hint with an empty name"
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
            if parse_hex_bits(value).is_some() {
                return Some(value.clone());
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
                return None;
            }
            // Convert 1D or 2D numeric arrays into rectangular 2D hex-f64 arrays for Touchstone.
            if items[0].is_array() {
                let cols = items[0].as_array()?.len();
                if cols == 0 {
                    return None;
                }
                let mut rows = Vec::with_capacity(items.len());
                for row in items {
                    let cells = row.as_array()?;
                    if cells.len() != cols {
                        return None;
                    }
                    let mut out_row = Vec::with_capacity(cols);
                    for cell in cells {
                        let f = json_number_or_hex(cell)?;
                        out_row.push(hex_f64(f));
                    }
                    rows.push(Value::Array(out_row));
                }
                Some(Value::Array(rows))
            } else {
                let mut rows = Vec::with_capacity(items.len());
                for cell in items {
                    let f = json_number_or_hex(cell)?;
                    rows.push(Value::Array(vec![hex_f64(f)]));
                }
                Some(Value::Array(rows))
            }
        }
    }
}

fn json_number_or_hex(v: &Value) -> Option<f64> {
    if let Some(bits) = parse_hex_bits(v) {
        return value2_safe_f64(f64::from_bits(bits));
    }
    value2_safe_f64(v.as_f64()?)
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

/// Operand of a parsed binary relation: either a parameter index or a numeric constant.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RelOperand {
    Param(usize),
    Const(f64),
}

/// One parsed binary constraint extracted from a hint `relations` or `error_edges` string.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParsedRelation {
    pub lhs: RelOperand,
    pub op: RelOp,
    pub rhs: RelOperand,
}

fn normalize_param_ident(s: &str) -> String {
    s.trim()
        .trim_matches(|c| matches!(c, '[' | ']' | '(' | ')' | '`' | '"' | '\''))
        .to_ascii_lowercase()
        .replace([' ', '-'], "_")
}

fn resolve_operand(token: &str, arg_names: &[String]) -> Option<RelOperand> {
    let trimmed = token
        .trim()
        .trim_matches(|c| matches!(c, '(' | ')' | '`'));
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(num) = trimmed.parse::<f64>()
        && num.is_finite()
    {
        return Some(RelOperand::Const(num));
    }
    let norm = normalize_param_ident(trimmed);
    for (idx, name) in arg_names.iter().enumerate() {
        if normalize_param_ident(name) == norm {
            return Some(RelOperand::Param(idx));
        }
    }
    None
}

/// Parse simple binary and chained comparisons (`a < b`, `issue < settlement < maturity`,
/// `rate >= 0`, `0 < prob < 1`, `x != 0`) against `arg_names`.
/// Free-text clauses that cannot be parsed as numeric comparisons are silently skipped.
pub fn parse_relation_expr(expr: &str, arg_names: &[String]) -> Vec<ParsedRelation> {
    let mut out = Vec::new();
    // Split compound clauses joined by `and` / `&&` / `,` / `;`
    for clause in expr.split([';', ',']).flat_map(|s| s.split("&&")).flat_map(|s| s.split(" and ")) {
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
            if let (Some(lhs), Some(rhs)) = (
                resolve_operand(&tokens[pair_idx], arg_names),
                resolve_operand(&tokens[pair_idx + 1], arg_names),
            ) {
                // Keep only constraints referencing at least one parameter.
                if matches!(lhs, RelOperand::Param(_)) || matches!(rhs, RelOperand::Param(_)) {
                    out.push(ParsedRelation {
                        lhs,
                        op: ops[pair_idx],
                        rhs,
                    });
                }
            }
        }
    }
    out
}

/// Parse all relations in `relations` against `arg_names`.
pub fn parse_relations(relations: &[String], arg_names: &[String]) -> Vec<ParsedRelation> {
    let mut out = Vec::new();
    for r in relations {
        out.extend(parse_relation_expr(r, arg_names));
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
}
