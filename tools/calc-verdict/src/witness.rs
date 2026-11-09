//! Witness readings: Excel's result of one worksheet function on given arguments, judged bit for
//! bit against a calculated result (bead dc-ymjf.2).
//!
//! A witness row is OxFunc's WitnessSet row, the shape `migration/witnesses/` already uses:
//! `{"id", "args", "expected_bits"}`. An argument is `"0x<16 hex digits>"` (binary64 bits), a JSON
//! boolean (a logical), any other JSON string (text), or a list of such scalars, or a list of
//! lists (an array, row by row; a flat list is one row). A result is `"0x<16 hex digits>"`,
//! `"error:<Variant>"` (`error:Num`, `error:Div0`, ...), `"logical:True"`/`"logical:False"` or
//! `"text:<text>"`. A row of a function that returns an array names the cell it was read from with
//! `"result_index": [row, column]`, 1-based.
//!
//! A [`WitnessSet`] file (`dnacalc-witness-set/1`, the pinned sets under
//! `crates/calc-func/tests/witnesses/`) adds to those rows the readings they came from: each row
//! names one entry of `readings`, which records the source file, the Excel build, the platform,
//! the bitness and the CPU wherever the capture recorded them (`null` where it did not). A row
//! that the calculation does not yet match carries `"calc_func"`: the result calculated when the
//! set was pinned, encoded like a result, or `fault:<name>` for a refusal. A pinned set therefore
//! fails on any change of a calculated result, a regression or a gain.
//!
//! [`Severity`] is OxFunc's ODR-FN-005 classing of one row: exact, last bit (1 to 4 units in the
//! last place, or +0 against -0), numeric (5 to 1024), gross (opposite signs, a non-finite value, or
//! more than 1024) and structural (a different kind of value, or a different error or text). A
//! function takes the class of its worst row. The distance is [`crate::compare`]'s.

use crate::typed::{MismatchClass, compare};
use crate::value::{CalcArray, CalcValue, CoreValue, ExcelText, WorksheetErrorCode};
use serde_json::{Map, Value, json};

/// The format string of a [`WitnessSet`] file.
pub const WITNESS_SET_FORMAT: &str = "dnacalc-witness-set/1";

/// How one calculated result compares with Excel's, worst last.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    Exact,
    LastBit,
    Numeric,
    Gross,
    Structural,
}

impl Severity {
    /// The class name the status table writes.
    pub fn name(self) -> &'static str {
        match self {
            Severity::Exact => "exact",
            Severity::LastBit => "last_bit",
            Severity::Numeric => "numeric",
            Severity::Gross => "gross",
            Severity::Structural => "structural",
        }
    }

    /// The class of `actual` against `expected`, and the distance in units in the last place when
    /// both are finite numbers.
    pub fn of(expected: &CalcValue, actual: &CalcValue) -> (Severity, Option<u64>) {
        match compare(expected, actual) {
            None => (Severity::Exact, None),
            Some(MismatchClass::Number { ulp: None }) => (Severity::Gross, None),
            Some(MismatchClass::Number { ulp: Some(ulp) }) => {
                let (e, a) = (
                    expected.as_number().unwrap_or(0.0),
                    actual.as_number().unwrap_or(0.0),
                );
                let opposite = e != 0.0 && a != 0.0 && e.is_sign_negative() != a.is_sign_negative();
                let class = if opposite || ulp > 1024 {
                    Severity::Gross
                } else if ulp > 4 {
                    Severity::Numeric
                } else {
                    Severity::LastBit
                };
                (class, Some(ulp))
            }
            Some(_) => (Severity::Structural, None),
        }
    }
}

fn bits(text: &str) -> Option<u64> {
    let hex = text.strip_prefix("0x")?;
    if hex.len() != 16 {
        return None;
    }
    u64::from_str_radix(hex, 16).ok()
}

fn error_from_variant(name: &str) -> Option<WorksheetErrorCode> {
    WorksheetErrorCode::all()
        .into_iter()
        .find(|code| format!("{code:?}").eq_ignore_ascii_case(name))
}

fn scalar(node: &Value, path: &str) -> Result<CalcValue, String> {
    match node {
        Value::String(text) => Ok(match bits(text) {
            Some(b) => CalcValue::number(f64::from_bits(b)),
            None => CalcValue::text(ExcelText::from_interop_assignment(text)),
        }),
        Value::Bool(b) => Ok(CalcValue::logical(*b)),
        other => Err(format!(
            "{path}: an argument is a hex string, a boolean or text, not {other}"
        )),
    }
}

/// One argument of a witness row as a value.
pub fn decode_argument(node: &Value, path: &str) -> Result<CalcValue, String> {
    let Value::Array(items) = node else {
        return scalar(node, path);
    };
    let rows: Vec<Vec<CalcValue>> = if items.iter().all(|item| item.is_array()) {
        items
            .iter()
            .enumerate()
            .map(|(r, row)| {
                row.as_array()
                    .into_iter()
                    .flatten()
                    .enumerate()
                    .map(|(c, cell)| scalar(cell, &format!("{path}[{r}][{c}]")))
                    .collect()
            })
            .collect::<Result<_, _>>()?
    } else {
        vec![
            items
                .iter()
                .enumerate()
                .map(|(c, cell)| scalar(cell, &format!("{path}[{c}]")))
                .collect::<Result<_, _>>()?,
        ]
    };
    CalcArray::from_rows(rows)
        .map(CalcValue::array)
        .ok_or_else(|| format!("{path}: an array argument has rows of different lengths or none"))
}

/// A witness result as a value: Excel's reading or a pinned calculated result. A pinned refusal
/// (`fault:...`) is not a value and is an error here.
pub fn decode_result(text: &str) -> Result<CalcValue, String> {
    if let Some(b) = bits(text) {
        return Ok(CalcValue::number(f64::from_bits(b)));
    }
    if let Some(name) = text.strip_prefix("error:") {
        return error_from_variant(name)
            .map(CalcValue::error)
            .ok_or_else(|| format!("unknown error {text}"));
    }
    if let Some(name) = text.strip_prefix("logical:") {
        return match name.to_ascii_lowercase().as_str() {
            "true" => Ok(CalcValue::logical(true)),
            "false" => Ok(CalcValue::logical(false)),
            _ => Err(format!("unknown logical {text}")),
        };
    }
    if let Some(body) = text.strip_prefix("text:") {
        return Ok(CalcValue::text(ExcelText::from_interop_assignment(body)));
    }
    Err(format!("not a witness result: {text}"))
}

/// A value in the result encoding, or `None` for a value that has none (an array, a reference, an
/// empty or missing value).
pub fn encode_result(value: &CalcValue) -> Option<String> {
    match value.core() {
        CoreValue::Number(n) => Some(format!("0x{:016x}", n.to_bits())),
        CoreValue::Error(code) => Some(format!("error:{code:?}")),
        CoreValue::Logical(b) => Some(if *b { "logical:True" } else { "logical:False" }.to_owned()),
        CoreValue::Text(t) => Some(format!("text:{}", t.to_string_lossy())),
        _ => None,
    }
}

/// The scalar reading of Excel's `INDEX(result, row, column)`, or the result itself.
/// Indices are 1-based. A zero axis selects the whole row/column in Excel; the scalar capture
/// reads its first cell. Worksheet errors propagate through INDEX at any selected position.
pub fn select_result(value: &CalcValue, index: Option<(usize, usize)>) -> Option<CalcValue> {
    if matches!(value.core(), CoreValue::Error(_)) {
        return Some(value.clone());
    }
    match (index, value.as_array()) {
        (Some((r, c)), Some(array)) => array.get(r.saturating_sub(1), c.saturating_sub(1)).cloned(),
        (Some((0..=1, 0..=1)), None) => Some(value.clone()),
        (Some(_), None) => None,
        (None, Some(array)) if array.cell_count() == 1 => array.get(0, 0).cloned(),
        (None, _) => Some(value.clone()),
    }
}

/// Invocation facts recorded by a capture. Missing fields stay missing; a build or
/// platform does not establish an argument transport, locale or workbook setting.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WitnessInvocation {
    pub input_transport: Option<String>,
    pub input_sheet_state: Option<String>,
    pub locale: Option<String>,
    pub decimal_separator: Option<String>,
    pub thousands_separator: Option<String>,
    pub date_system: Option<String>,
    pub compatibility_version: Option<String>,
}

impl WitnessInvocation {
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }

    pub fn decode(value: &Value, path: &str) -> Result<Self, String> {
        let node = value
            .as_object()
            .ok_or_else(|| format!("{path}: an object is required"))?;
        let mut result = Self::default();
        for (key, value) in node {
            let field = match key.as_str() {
                "input_transport" => &mut result.input_transport,
                "input_sheet_state" => &mut result.input_sheet_state,
                "locale" => &mut result.locale,
                "decimal_separator" => &mut result.decimal_separator,
                "thousands_separator" => &mut result.thousands_separator,
                "date_system" => &mut result.date_system,
                "compatibility_version" => &mut result.compatibility_version,
                _ => return Err(format!("{path}.{key}: unknown invocation field")),
            };
            *field = if value.is_null() {
                None
            } else {
                Some(
                    value
                        .as_str()
                        .ok_or_else(|| format!("{path}.{key}: a string is required"))?
                        .to_owned(),
                )
            };
        }
        Ok(result)
    }

    pub fn to_json(&self) -> Value {
        let mut node = Map::new();
        for (key, value) in [
            ("input_transport", &self.input_transport),
            ("input_sheet_state", &self.input_sheet_state),
            ("locale", &self.locale),
            ("decimal_separator", &self.decimal_separator),
            ("thousands_separator", &self.thousands_separator),
            ("date_system", &self.date_system),
            ("compatibility_version", &self.compatibility_version),
        ] {
            if let Some(value) = value {
                node.insert(key.to_owned(), json!(value));
            }
        }
        Value::Object(node)
    }
}

/// Where a set's rows were read: one capture or bank, and the environment it recorded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadingSource {
    pub id: String,
    /// The bank file, as `<repository> <path>`.
    pub source: String,
    pub excel_build: Option<String>,
    /// Windows, Mac or web.
    pub platform: Option<String>,
    pub bitness: Option<String>,
    pub cpu: Option<String>,
    pub invocation: Option<WitnessInvocation>,
}

impl ReadingSource {
    /// The environment as one label: build, platform, bitness and CPU, `?` for each unknown.
    pub fn environment(&self) -> String {
        let part = |v: &Option<String>| v.clone().unwrap_or_else(|| "?".to_owned());
        format!(
            "{}/{}/{}/{}",
            part(&self.excel_build),
            part(&self.platform),
            part(&self.bitness),
            part(&self.cpu)
        )
    }
}

/// One row of a [`WitnessSet`].
#[derive(Clone, Debug, PartialEq)]
pub struct WitnessRow {
    pub id: String,
    pub args: Vec<Value>,
    pub result_index: Option<(usize, usize)>,
    /// Excel's result, in the result encoding.
    pub expected: String,
    /// The `id` of the reading source.
    pub reading: String,
    /// The calculated result when the set was pinned, for a row it did not match.
    pub calc_func: Option<String>,
}

/// A pinned witness set of one function.
#[derive(Clone, Debug, PartialEq)]
pub struct WitnessSet {
    pub function: String,
    pub selection: String,
    pub readings: Vec<ReadingSource>,
    pub witnesses: Vec<WitnessRow>,
}

fn text_field(node: &Map<String, Value>, name: &str, path: &str) -> Result<String, String> {
    node.get(name)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("{path}.{name}: a string is required"))
}

fn optional_text(node: &Map<String, Value>, name: &str) -> Option<String> {
    node.get(name).and_then(Value::as_str).map(str::to_owned)
}

/// The `result_index` of a row object, 1-based, when it has one.
pub fn result_index(row: &Map<String, Value>) -> Option<(usize, usize)> {
    let pair = row.get("result_index")?.as_array()?;
    match pair.as_slice() {
        [r, c] => Some((r.as_u64()? as usize, c.as_u64()? as usize)),
        _ => None,
    }
}

impl WitnessSet {
    /// Reads a `dnacalc-witness-set/1` file.
    pub fn decode(bytes: &[u8]) -> Result<WitnessSet, String> {
        let root: Value = serde_json::from_slice(bytes).map_err(|e| format!("witness set: {e}"))?;
        let root = root.as_object().ok_or("witness set: not an object")?;
        if root.get("format").and_then(Value::as_str) != Some(WITNESS_SET_FORMAT) {
            return Err(format!("witness set: format is not {WITNESS_SET_FORMAT}"));
        }
        let function = text_field(root, "function", "set")?;
        let selection = text_field(root, "selection", "set")?;
        let mut readings = Vec::new();
        for (i, node) in root
            .get("readings")
            .and_then(Value::as_array)
            .ok_or("set.readings: a list is required")?
            .iter()
            .enumerate()
        {
            let path = format!("readings[{i}]");
            let node = node.as_object().ok_or(format!("{path}: not an object"))?;
            readings.push(ReadingSource {
                id: text_field(node, "id", &path)?,
                source: text_field(node, "source", &path)?,
                excel_build: optional_text(node, "excel_build"),
                platform: optional_text(node, "platform"),
                bitness: optional_text(node, "bitness"),
                cpu: optional_text(node, "cpu"),
                invocation: node
                    .get("invocation")
                    .filter(|v| !v.is_null())
                    .map(|v| WitnessInvocation::decode(v, &format!("{path}.invocation")))
                    .transpose()?
                    .filter(|c| !c.is_empty()),
            });
        }
        let mut witnesses = Vec::new();
        for (i, node) in root
            .get("witnesses")
            .and_then(Value::as_array)
            .ok_or("set.witnesses: a list is required")?
            .iter()
            .enumerate()
        {
            let path = format!("witnesses[{i}]");
            let node = node.as_object().ok_or(format!("{path}: not an object"))?;
            let reading = text_field(node, "reading", &path)?;
            if !readings.iter().any(|r| r.id == reading) {
                return Err(format!("{path}: reading {reading} is not listed"));
            }
            witnesses.push(WitnessRow {
                id: text_field(node, "id", &path)?,
                args: node
                    .get("args")
                    .and_then(Value::as_array)
                    .cloned()
                    .ok_or(format!("{path}.args: a list is required"))?,
                result_index: result_index(node),
                expected: text_field(node, "expected_bits", &path)?,
                reading,
                calc_func: optional_text(node, "calc_func"),
            });
        }
        Ok(WitnessSet {
            function,
            selection,
            readings,
            witnesses,
        })
    }

    /// The file, one row per line so that a diff shows each row that changed.
    pub fn encode(&self) -> String {
        let mut out = String::new();
        out.push_str("{\n");
        out.push_str(&format!("  \"format\": {},\n", json!(WITNESS_SET_FORMAT)));
        out.push_str(&format!("  \"function\": {},\n", json!(self.function)));
        out.push_str(&format!("  \"selection\": {},\n", json!(self.selection)));
        out.push_str("  \"readings\": [\n");
        for (i, r) in self.readings.iter().enumerate() {
            let mut node = json!({
                "id": r.id, "source": r.source, "excel_build": r.excel_build,
                "platform": r.platform, "bitness": r.bitness, "cpu": r.cpu,
            });
            if let Some(invocation) = &r.invocation {
                node["invocation"] = invocation.to_json();
            }
            let comma = if i + 1 < self.readings.len() { "," } else { "" };
            out.push_str(&format!("    {node}{comma}\n"));
        }
        out.push_str("  ],\n  \"witnesses\": [\n");
        for (i, w) in self.witnesses.iter().enumerate() {
            let mut node = Map::new();
            node.insert("id".into(), json!(w.id));
            node.insert("args".into(), Value::Array(w.args.clone()));
            if let Some((r, c)) = w.result_index {
                node.insert("result_index".into(), json!([r, c]));
            }
            node.insert("expected_bits".into(), json!(w.expected));
            node.insert("reading".into(), json!(w.reading));
            if let Some(pinned) = &w.calc_func {
                node.insert("calc_func".into(), json!(pinned));
            }
            let comma = if i + 1 < self.witnesses.len() {
                ","
            } else {
                ""
            };
            out.push_str(&format!("    {}{comma}\n", Value::Object(node)));
        }
        out.push_str("  ]\n}\n");
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn invocation_fields_round_trip_without_inventing_missing_facts() {
        let context = WitnessInvocation {
            input_transport: Some("cell_value2_bulk".into()),
            date_system: Some("1900".into()),
            compatibility_version: Some("2".into()),
            ..Default::default()
        };
        let json = context.to_json();
        assert_eq!(
            WitnessInvocation::decode(&json, "context").unwrap(),
            context
        );
        assert!(json.get("locale").is_none());
        assert!(json.get("decimal_separator").is_none());
        assert!(WitnessInvocation::decode(&json!({"locale": 1033}), "context").is_err());
        assert!(WitnessInvocation::decode(&json!({"invented": "en-US"}), "context").is_err());
    }

    fn n(bits: u64) -> CalcValue {
        CalcValue::number(f64::from_bits(bits))
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn severity_follows_the_odr_fn_005_classes() {
        let one = 0x3ff0000000000000;
        assert_eq!(Severity::of(&n(one), &n(one)), (Severity::Exact, None));
        assert_eq!(
            Severity::of(&n(one), &n(one + 4)),
            (Severity::LastBit, Some(4))
        );
        assert_eq!(
            Severity::of(&n(one), &n(one + 5)),
            (Severity::Numeric, Some(5))
        );
        assert_eq!(Severity::of(&n(one), &n(one + 1024)).0, Severity::Numeric);
        assert_eq!(Severity::of(&n(one), &n(one + 1025)).0, Severity::Gross);
        assert_eq!(
            Severity::of(&n(0), &n(1 << 63)),
            (Severity::LastBit, Some(0))
        );
        assert_eq!(Severity::of(&n(one), &n(one | 1 << 63)).0, Severity::Gross);
        assert_eq!(
            Severity::of(&n(one), &n(0x7ff0000000000000)).0,
            Severity::Gross
        );
        let num = CalcValue::error(WorksheetErrorCode::Num);
        assert_eq!(Severity::of(&num, &n(one)).0, Severity::Structural);
        assert_eq!(
            Severity::of(&num, &CalcValue::error(WorksheetErrorCode::Value)).0,
            Severity::Structural
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn results_round_trip_and_arguments_decode() {
        for text in [
            "0x3ff0000000000001",
            "error:Num",
            "error:Div0",
            "logical:True",
            "text:ab",
        ] {
            assert_eq!(
                encode_result(&decode_result(text).unwrap()).as_deref(),
                Some(text)
            );
        }
        assert!(decode_result("fault:NotModelled").is_err());
        let array = decode_argument(
            &json!([["0x3ff0000000000000", true], ["0x0000000000000000", "x"]]),
            "a",
        )
        .unwrap();
        assert_eq!(array.as_array().unwrap().shape().rows, 2);
        assert_eq!(
            decode_argument(&json!("Em"), "a")
                .unwrap()
                .as_text()
                .unwrap()
                .to_string_lossy(),
            "Em"
        );
        let flat =
            decode_argument(&json!(["0x3ff0000000000000", "0x4000000000000000"]), "a").unwrap();
        assert_eq!(flat.as_array().unwrap().shape().cols, 2);
        assert_eq!(
            select_result(&flat, Some((1, 2))),
            Some(n(0x4000000000000000))
        );
        assert_eq!(
            select_result(&flat, Some((0, 2))),
            Some(n(0x4000000000000000))
        );
        assert_eq!(
            select_result(&flat, Some((0, 0))),
            Some(n(0x3ff0000000000000))
        );
        let error = CalcValue::error(WorksheetErrorCode::Num);
        assert_eq!(select_result(&error, Some((4, 2))), Some(error));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_set_round_trips_and_refuses_an_unlisted_reading() {
        let set = WitnessSet {
            function: "EXP".into(),
            selection: "test".into(),
            readings: vec![ReadingSource {
                id: "r0".into(),
                source: "OxFunc-Evidence x".into(),
                excel_build: Some("20430".into()),
                platform: Some("Windows".into()),
                bitness: Some("64-bit".into()),
                cpu: None,
                invocation: None,
            }],
            witnesses: vec![WitnessRow {
                id: "a".into(),
                args: vec![json!("0x3ff0000000000000")],
                result_index: None,
                expected: "0x4005bf0a8b145769".into(),
                reading: "r0".into(),
                calc_func: Some("0x4005bf0a8b14576a".into()),
            }],
        };
        let text = set.encode();
        assert_eq!(WitnessSet::decode(text.as_bytes()).unwrap(), set);
        assert_eq!(set.readings[0].environment(), "20430/Windows/64-bit/?");
        let broken = text.replace("\"reading\":\"r0\"", "\"reading\":\"r9\"");
        assert!(WitnessSet::decode(broken.as_bytes()).is_err());
    }
}
