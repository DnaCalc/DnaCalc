//! Witness readings: Excel's result of one worksheet function on given arguments, judged bit for
//! bit against a calculated result (bead dc-ymjf.2).
//!
//! A witness row is OxFunc's WitnessSet row, the shape `migration/witnesses/` already uses:
//! `{"id", "args", "expected_bits"}`. An argument is `"0x<16 hex digits>"` (binary64 bits), a JSON
//! boolean (a logical), any other JSON string (text), or a list of such scalars, or a list of
//! lists (an array, row by row; a flat list is one row). JSON `null` denotes a blank cell and
//! `{"error":"Num"}` denotes an error value, using the same variant names as results. A result is `"0x<16 hex digits>"`,
//! `"error:<Variant>"` (`error:Num`, `error:Div0`, ...), `"logical:True"`/`"logical:False"` or
//! `"text:<text>"`, `"empty"`, or `"array:"` followed by the canonical typed value JSON for an
//! entire spill. A scalar projection of an array names its cell with `"result_index": [row, column]`, 1-based.
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
//! more than 1024) and structural (a different kind of value, or a different error or text). A text
//! that differs only in the digits of the numbers it embeds takes the class of the farthest pair of
//! those numbers (COMPLEX and CONCATENATE of a number). A function takes the class of its worst row. The distance is [`crate::compare`]'s.

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
                (Self::of_numbers(ulp, opposite), Some(ulp))
            }
            Some(MismatchClass::Text) => match (expected.as_text(), actual.as_text()) {
                (Some(e), Some(a)) => {
                    text_number_distance(&e.to_string_lossy(), &a.to_string_lossy())
                        .map_or((Severity::Structural, None), |ulp| {
                            (Self::of_numbers(ulp, false), Some(ulp))
                        })
                }
                _ => (Severity::Structural, None),
            },
            Some(_) => (Severity::Structural, None),
        }
    }

    fn of_numbers(ulp: u64, opposite: bool) -> Severity {
        if opposite || ulp > 1024 {
            Severity::Gross
        } else if ulp > 4 {
            Severity::Numeric
        } else {
            Severity::LastBit
        }
    }
}

/// A text split into decimal numbers (digits, an optional fraction and an optional exponent) and
/// the text between them.
fn split_numbers(text: &str) -> (Vec<&str>, Vec<f64>) {
    let bytes = text.as_bytes();
    let (mut between, mut numbers) = (Vec::new(), Vec::new());
    let (mut start, mut i) = (0, 0);
    while i < bytes.len() {
        if !bytes[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let begin = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i + 1 < bytes.len() && bytes[i] == b'.' && bytes[i + 1].is_ascii_digit() {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
        }
        if i < bytes.len() && (bytes[i] == b'E' || bytes[i] == b'e') {
            let mut j = i + 1;
            if j < bytes.len() && (bytes[j] == b'+' || bytes[j] == b'-') {
                j += 1;
            }
            if j < bytes.len() && bytes[j].is_ascii_digit() {
                while j < bytes.len() && bytes[j].is_ascii_digit() {
                    j += 1;
                }
                i = j;
            }
        }
        between.push(&text[start..begin]);
        numbers.push(text[begin..i].parse().unwrap_or(f64::NAN));
        start = i;
    }
    between.push(&text[start..]);
    (between, numbers)
}

/// The largest ULP distance between the numbers embedded in two texts that differ only in those
/// numbers' digits (COMPLEX's `"1.23456789012345+2i"`, CONCATENATE of a number), or `None` when
/// the texts differ anywhere else: in the text between the numbers, in how many numbers they hold,
/// or only in how one value is written.
fn text_number_distance(expected: &str, actual: &str) -> Option<u64> {
    let (e_between, e_numbers) = split_numbers(expected);
    let (a_between, a_numbers) = split_numbers(actual);
    if e_between != a_between || e_numbers.len() != a_numbers.len() {
        return None;
    }
    let worst = e_numbers
        .iter()
        .zip(&a_numbers)
        .map(|(e, a)| crate::typed::ulp_distance(*e, *a))
        .collect::<Option<Vec<u64>>>()?
        .into_iter()
        .max()?;
    (worst > 0).then_some(worst)
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
        Value::Null => Ok(CalcValue::empty()),
        Value::Object(fields) if fields.len() == 1 => fields
            .get("error")
            .and_then(Value::as_str)
            .and_then(error_from_variant)
            .map(CalcValue::error)
            .ok_or_else(|| format!("{path}: unknown argument object {node}")),
        other => Err(format!(
            "{path}: an argument is a hex string, a boolean, text, blank or error, not {other}"
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
    if text == "empty" {
        return Ok(CalcValue::empty());
    }
    if let Some(body) = text.strip_prefix("array:") {
        let path = "witness result";
        let node = crate::json::parse(body.as_bytes(), path).map_err(|e| e.to_string())?;
        let value = crate::typed::decode_value(&node, path).map_err(|e| e.to_string())?;
        return value.as_array().is_some().then_some(value).ok_or_else(|| {
            "an array witness result must hold a rectangular array".to_owned()
        });
    }
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

/// A scalar or whole spill in the result encoding, or `None` for unrepresentable values.
pub fn encode_result(value: &CalcValue) -> Option<String> {
    match value.core() {
        CoreValue::Number(n) => Some(format!("0x{:016x}", n.to_bits())),
        CoreValue::Error(code) => Some(format!("error:{code:?}")),
        CoreValue::Logical(b) => Some(if *b { "logical:True" } else { "logical:False" }.to_owned()),
        CoreValue::Text(t) => Some(format!("text:{}", t.to_string_lossy())),
        CoreValue::Empty => Some("empty".into()),
        CoreValue::Array(_) if crate::typed::representable(value) => {
            Some(format!("array:{}", crate::typed::value_out(value).compact()))
        }
        _ => None,
    }
}

/// The scalar reading of Excel's `INDEX(result, row, column)`, or the displayed result at an
/// isolated worksheet anchor with room for the entire spill.
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
        (None, Some(array)) if array.cell_count() == 1 || array.is_boxed() => {
            array.get(0, 0).map(CalcValue::shown_as_spill_cell)
        }
        (None, Some(array)) => Some(CalcValue::array(array.shown_as_spill())),
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
    /// The serial date and time Excel's calculation read (NOW() asked in the capture's
    /// session), as binary64 bits in hex; a two-part date such as 15/3 takes its year.
    pub calculation_clock: Option<String>,
    /// The capture's attested calendar date (`YYYY-MM-DD`, e.g. from the capture response
    /// timestamp), supplying the year for year-less date text when no `calculation_clock` was
    /// recorded.
    pub capture_date: Option<String>,
    /// The add-in files open in the capture's Excel session, comma-separated ("" for none),
    /// as the environment record attests; a library-qualified add-in call needs its file open.
    pub addins_open: Option<String>,
    /// The workbook's visible defined names, comma-separated ("" for none), when attested.
    pub defined_names: Option<String>,
    /// Where the capture's worker placed argument cells, when attested (`touchstone/1-bulk`).
    pub cell_layout: Option<String>,
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
                "calculation_clock" => &mut result.calculation_clock,
                "capture_date" => &mut result.capture_date,
                "addins_open" => &mut result.addins_open,
                "defined_names" => &mut result.defined_names,
                "cell_layout" => &mut result.cell_layout,
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
            ("calculation_clock", &self.calculation_clock),
            ("capture_date", &self.capture_date),
            ("addins_open", &self.addins_open),
            ("defined_names", &self.defined_names),
            ("cell_layout", &self.cell_layout),
        ] {
            if let Some(value) = value {
                node.insert(key.to_owned(), json!(value));
            }
        }
        Value::Object(node)
    }

    /// The calculation clock as a 1900-system serial date: `calculation_clock` (explicit NOW()
    /// bits) when present, otherwise `capture_date` (`YYYY-MM-DD`) converted to its 1900 serial.
    pub fn clock_serial(&self) -> Result<Option<f64>, &'static str> {
        if let Some(clock) = &self.calculation_clock {
            let bits = u64::from_str_radix(clock, 16).map_err(|_| "fault:WitnessClock")?;
            let now = f64::from_bits(bits);
            return if now.is_finite() {
                Ok(Some(now))
            } else {
                Err("fault:WitnessClock")
            };
        }
        if let Some(date) = &self.capture_date {
            return parse_capture_date_serial_1900(date)
                .map(Some)
                .ok_or("fault:WitnessClock");
        }
        Ok(None)
    }
}

/// Converts a strict ISO calendar date `YYYY-MM-DD` (1900..=9999) to its Excel 1900 serial date.
pub fn parse_capture_date_serial_1900(date: &str) -> Option<f64> {
    let bytes = date.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let year: i64 = date[0..4].parse().ok()?;
    let month: i64 = date[5..7].parse().ok()?;
    let day: i64 = date[8..10].parse().ok()?;
    if !(1900..=9999).contains(&year) || !(1..=12).contains(&month) || day < 1 {
        return None;
    }
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if leap || year == 1900 {
                29
            } else {
                28
            }
        }
        _ => return None,
    };
    if day > max_day {
        return None;
    }
    if year == 1900 && month <= 2 {
        return Some((if month == 1 { day } else { 31 + day }) as f64);
    }
    // Days from civil date (Howard Hinnant's Gregorian algorithm), offset from 1899-12-30.
    let days_from_civil = |y: i64, m: i64, d: i64| -> i64 {
        let y = y - i64::from(m <= 2);
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    };
    let serial = days_from_civil(year, month, day) - days_from_civil(1899, 12, 30);
    Some(serial as f64)
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

        // dc-iuwct.44: capture_date round-trips and converts to its 1900 serial date; explicit
        // calculation_clock takes precedence when both are present.
        let dated = WitnessInvocation {
            capture_date: Some("2026-11-06".into()),
            ..Default::default()
        };
        assert_eq!(
            WitnessInvocation::decode(&dated.to_json(), "context").unwrap(),
            dated
        );
        assert_eq!(dated.clock_serial(), Ok(Some(46332.0)));
        assert_eq!(parse_capture_date_serial_1900("1900-01-01"), Some(1.0));
        assert_eq!(parse_capture_date_serial_1900("1900-02-28"), Some(59.0));
        assert_eq!(parse_capture_date_serial_1900("1900-02-29"), Some(60.0));
        assert_eq!(parse_capture_date_serial_1900("1900-03-01"), Some(61.0));
        assert_eq!(parse_capture_date_serial_1900("2024-02-29"), Some(45351.0));
        assert_eq!(parse_capture_date_serial_1900("2026-01-03"), Some(46025.0));
        assert_eq!(parse_capture_date_serial_1900("2026-11-20"), Some(46346.0));
        assert_eq!(parse_capture_date_serial_1900("2026-02-29"), None);
        assert_eq!(parse_capture_date_serial_1900("1899-12-31"), None);
        assert_eq!(parse_capture_date_serial_1900("not-a-date"), None);
        let both = WitnessInvocation {
            calculation_clock: Some(format!("{:016x}", 46346.5_f64.to_bits())),
            capture_date: Some("2026-11-06".into()),
            ..Default::default()
        };
        assert_eq!(both.clock_serial(), Ok(Some(46346.5)));
        let bad_date = WitnessInvocation {
            capture_date: Some("2026-02-29".into()),
            ..Default::default()
        };
        assert_eq!(bad_date.clock_serial(), Err("fault:WitnessClock"));
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
    fn a_text_differing_only_in_an_embedded_number_takes_that_numbers_class() {
        let t = |s: &str| CalcValue::text(ExcelText::from_interop_assignment(s));
        // The last of 15 digits near 9.9: 1e-14 is about 5.6 ULP; the two parsed values are 5 apart.
        assert_eq!(
            Severity::of(&t("9.99999999999999+2i"), &t("9.99999999999998+2i")),
            (Severity::Numeric, Some(5))
        );
        // One ULP apart, in an exponent form.
        let (a, b) = (1.25e-7_f64, f64::from_bits(1.25e-7_f64.to_bits() + 1));
        assert_eq!(
            Severity::of(&t(&format!("x{a:e}i")), &t(&format!("x{b:e}i"))),
            (Severity::LastBit, Some(1))
        );
        assert_eq!(Severity::of(&t("ab 3"), &t("ab 4")).0, Severity::Gross);
        // A different kind of answer stays structural: other text between the numbers, another
        // count of numbers, a sign, or only another way of writing one value.
        assert_eq!(Severity::of(&t("1+2i"), &t("1-2i")).0, Severity::Structural);
        assert_eq!(Severity::of(&t("1+2i"), &t("1+2j")).0, Severity::Structural);
        assert_eq!(Severity::of(&t("1+2i"), &t("1")).0, Severity::Structural);
        assert_eq!(Severity::of(&t("1.5"), &t("1.50")).0, Severity::Structural);
        assert_eq!(Severity::of(&t("1A"), &t("1B")).0, Severity::Structural);
        assert_eq!(
            Severity::of(&t("1"), &n(0x3ff0000000000000)).0,
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
            "empty",
        ] {
            assert_eq!(
                encode_result(&decode_result(text).unwrap()).as_deref(),
                Some(text)
            );
        }
        assert!(decode_result("fault:NotModelled").is_err());
        let spill = decode_argument(
            &json!([["0x8000000000000000", null], [true, {"error": "Spill"}]]),
            "spill",
        )
        .unwrap();
        let encoded = encode_result(&spill).unwrap();
        assert_eq!(compare(&spill, &decode_result(&encoded).unwrap()), None);
        assert!(decode_result(r#"array:{"empty":true}"#).is_err());
        assert_eq!(
            decode_argument(&json!(null), "a").unwrap(),
            CalcValue::empty()
        );
        assert_eq!(
            decode_argument(&json!({"error": "Div0"}), "a").unwrap(),
            CalcValue::error(WorksheetErrorCode::Div0)
        );
        assert!(decode_argument(&json!({"error": "unknown"}), "a").is_err());
        assert!(decode_argument(&json!({"error": "Num", "extra": true}), "a").is_err());
        let mixed = decode_argument(&json!([[null, {"error": "NA"}]]), "a").unwrap();
        assert_eq!(
            mixed.as_array().unwrap().get(0, 0),
            Some(&CalcValue::empty())
        );
        assert_eq!(
            mixed.as_array().unwrap().get(0, 1),
            Some(&CalcValue::error(WorksheetErrorCode::NA))
        );
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
