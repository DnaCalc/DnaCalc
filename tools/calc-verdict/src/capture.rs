//! The canonical CaptureRecord: what Excel was observed to produce for one workbook case, and the
//! independently enumerated formula inventory that is the verdict's denominator.
//!
//! A file-cache replay, a live Excel capture and an import of legacy evidence all produce this one
//! record; only its `acquisition` and the origin of each observation differ. The JSON form:
//!
//! ```text
//! {"format": "dnacalc-capture-record", "version": 1,
//!  "case": "<case id>", "source": "<content hash of the input workbook>",
//!  "acquisition": {"kind": "live_capture" | "file_cache_replay"}
//!               | {"kind": "legacy_evidence", "entry": "<entry of the legacy evidence record>"},
//!  "environment": null | {"excel_build": "...", "channel": "...", "compatibility": "...",
//!                         "numeric_environment": "...", "settings": {"<name>": "<value>", ...}},
//!  "sheets": [{"name": "Sheet1", "visibility": "visible" | "hidden" | "very_hidden"}, ...],
//!  "formulas": [{"sheet": "Sheet1", "row": 1, "column": 1,
//!                "kind": "ordinary" | "shared_master" | "shared_child", "group": <shared only>}, ...],
//!  "observations": [{"sheet": "Sheet1", "row": 1, "column": 1,
//!                    "excel_raw": <value>
//!                  | "excel_saved_cache": {"t": "n" | "b" | "e" | "str", "v": "<the <v> text>"}
//!                    (`t` is required, `"n"` included)
//!                  | "unknown_freshness_cache": {"t": ..., "v": ...}}, ...]}
//! ```
//!
//! `<value>` is the canonical typed value of `typed.rs` (numbers as binary64 bits in hex). Rows and
//! columns are 1-based. Each formula cell of the workbook is listed once, hidden sheets, every cell
//! of a shared-formula range (its master and each child) and error-valued formulas included: the
//! list is enumerated from the workbook itself, never from what an evaluator returned.

use crate::DecodeError;
use crate::json::{self, Fields, Node};
use crate::typed::{decode_value, error_from_literal};
use crate::value::{CalcValue, ExcelText, MAX_COLUMNS, MAX_ROWS, NumericTargetId};

pub const CAPTURE_FORMAT: &str = "dnacalc-capture-record";
pub const CAPTURE_VERSION: u64 = 1;

/// A cell named by its sheet's name and a 1-based row and column within Excel's grid.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CellAddress {
    sheet: String,
    row: u32,
    column: u32,
}

impl CellAddress {
    /// The address, or `None` when the sheet name is empty or the row or column is outside 1 to
    /// [`MAX_ROWS`] or 1 to [`MAX_COLUMNS`].
    pub fn new(sheet: &str, row: u32, column: u32) -> Option<CellAddress> {
        if sheet.is_empty()
            || !(1..=MAX_ROWS).contains(&row)
            || !(1..=MAX_COLUMNS).contains(&column)
        {
            return None;
        }
        Some(CellAddress {
            sheet: sheet.to_owned(),
            row,
            column,
        })
    }

    pub fn sheet(&self) -> &str {
        &self.sheet
    }

    pub fn row(&self) -> u32 {
        self.row
    }

    pub fn column(&self) -> u32 {
        self.column
    }

    /// The A1 form of the row and column (`B7`), without the sheet.
    pub fn a1(&self) -> String {
        let mut letters = Vec::new();
        let mut n = self.column;
        while n > 0 {
            let digit = (n - 1) % 26;
            letters.push(char::from(b'A' + digit as u8));
            n = (n - 1) / 26;
        }
        letters.iter().rev().collect::<String>() + &self.row.to_string()
    }

    /// The `sheet`, `row` and `column` members of an object whose other members `fields` admits.
    pub(crate) fn decode(fields: &Fields<'_>) -> Result<CellAddress, DecodeError> {
        let sheet = json::name(fields.required("sheet")?, &fields.at("sheet"))?;
        let row = json::unsigned(fields.required("row")?, &fields.at("row"))?;
        let column = json::unsigned(fields.required("column")?, &fields.at("column"))?;
        u32::try_from(row)
            .ok()
            .zip(u32::try_from(column).ok())
            .and_then(|(row, column)| CellAddress::new(sheet, row, column))
            .ok_or_else(|| {
                DecodeError::new(
                    &fields.at("row"),
                    format!(
                        "row {row} and column {column} are not within 1 to {MAX_ROWS} and 1 to {MAX_COLUMNS}"
                    ),
                )
            })
    }
}

/// How a CaptureRecord was acquired.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Acquisition {
    /// An isolated Excel instance opened the workbook, recalculated it and was read (raw `Value2`
    /// observations, and the cache of the package it then saved).
    LiveCapture,
    /// The cell caches of a workbook file were read; no Excel ran. Such a record holds no raw
    /// observation.
    FileCacheReplay,
    /// Converted from an entry of a legacy evidence record (such as
    /// `migration/evidence/oxfunc-smart-fuzzer.json`), whose label decides what it may certify.
    LegacyEvidence { entry: String },
}

impl Acquisition {
    pub(crate) fn decode(node: &Node, path: &str) -> Result<Acquisition, DecodeError> {
        let fields = Fields::new(node, path, &["kind", "entry"])?;
        let kind = json::text(fields.required("kind")?, &fields.at("kind"))?;
        let entry = fields.optional("entry");
        match (kind, entry) {
            ("live_capture", None) => Ok(Self::LiveCapture),
            ("file_cache_replay", None) => Ok(Self::FileCacheReplay),
            ("legacy_evidence", Some(entry)) => Ok(Self::LegacyEvidence {
                entry: json::name(entry, &fields.at("entry"))?.to_owned(),
            }),
            ("legacy_evidence", None) => Err(DecodeError::new(
                path,
                "is legacy_evidence and has no member \"entry\"",
            )),
            ("live_capture" | "file_cache_replay", Some(_)) => Err(DecodeError::new(
                path,
                format!("is {kind} and has the member \"entry\", which only legacy_evidence takes"),
            )),
            (other, _) => Err(DecodeError::new(
                &fields.at("kind"),
                format!("{other:?} is not live_capture, file_cache_replay or legacy_evidence"),
            )),
        }
    }

    pub(crate) fn name(&self) -> &'static str {
        match self {
            Acquisition::LiveCapture => "live_capture",
            Acquisition::FileCacheReplay => "file_cache_replay",
            Acquisition::LegacyEvidence { .. } => "legacy_evidence",
        }
    }
}

/// The Excel that produced the raw observations: the Exact target it is (build, channel,
/// compatibility and numeric environment) and the settings it calculated under.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExcelEnvironment {
    pub target: NumericTargetId,
    /// Each setting the capture recorded (locale, date system, calculation mode and so on), by
    /// name, in the record's order. They are carried, not interpreted.
    pub settings: Vec<(String, String)>,
}

impl ExcelEnvironment {
    fn decode(node: &Node, path: &str) -> Result<Option<ExcelEnvironment>, DecodeError> {
        if let Node::Null = node {
            return Ok(None);
        }
        let fields = Fields::new(
            node,
            path,
            &[
                "excel_build",
                "channel",
                "compatibility",
                "numeric_environment",
                "settings",
            ],
        )?;
        let part = |name: &str| -> Result<&str, DecodeError> {
            json::name(fields.required(name)?, &fields.at(name))
        };
        let target = NumericTargetId::new(
            part("excel_build")?,
            part("channel")?,
            part("compatibility")?,
            part("numeric_environment")?,
        )
        .ok_or_else(|| DecodeError::new(path, "names an empty target part"))?;
        let settings_at = fields.at("settings");
        let settings = json::members(fields.required("settings")?, &settings_at)?
            .iter()
            .map(|(name, value)| {
                Ok((
                    name.clone(),
                    json::text(value, &format!("{settings_at}.{name}"))?.to_owned(),
                ))
            })
            .collect::<Result<Vec<_>, DecodeError>>()?;
        Ok(Some(ExcelEnvironment { target, settings }))
    }
}

/// Whether a sheet is shown. A hidden or very hidden sheet's formulas are in the denominator like
/// any other.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SheetVisibility {
    Visible,
    Hidden,
    VeryHidden,
}

impl SheetVisibility {
    fn decode(node: &Node, path: &str) -> Result<SheetVisibility, DecodeError> {
        match json::text(node, path)? {
            "visible" => Ok(Self::Visible),
            "hidden" => Ok(Self::Hidden),
            "very_hidden" => Ok(Self::VeryHidden),
            other => Err(DecodeError::new(
                path,
                format!("{other:?} is not visible, hidden or very_hidden"),
            )),
        }
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            SheetVisibility::Visible => "visible",
            SheetVisibility::Hidden => "hidden",
            SheetVisibility::VeryHidden => "very_hidden",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SheetEntry {
    pub name: String,
    pub visibility: SheetVisibility,
}

/// How a formula cell holds its formula in the workbook. A shared formula's group is its `si`,
/// which is scoped to its sheet; its master holds the formula text and each child refers to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormulaKind {
    Ordinary,
    SharedMaster { group: u64 },
    SharedChild { group: u64 },
}

impl FormulaKind {
    fn decode(fields: &Fields<'_>) -> Result<FormulaKind, DecodeError> {
        let kind = json::text(fields.required("kind")?, &fields.at("kind"))?;
        let group = fields
            .optional("group")
            .map(|node| json::unsigned(node, &fields.at("group")))
            .transpose()?;
        match (kind, group) {
            ("ordinary", None) => Ok(Self::Ordinary),
            ("shared_master", Some(group)) => Ok(Self::SharedMaster { group }),
            ("shared_child", Some(group)) => Ok(Self::SharedChild { group }),
            ("ordinary", Some(_)) => Err(DecodeError::new(
                &fields.at("group"),
                "is given for an ordinary formula",
            )),
            ("shared_master" | "shared_child", None) => Err(DecodeError::new(
                &fields.at("kind"),
                format!("is {kind} and has no group"),
            )),
            (other, _) => Err(DecodeError::new(
                &fields.at("kind"),
                format!("{other:?} is not ordinary, shared_master or shared_child"),
            )),
        }
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            FormulaKind::Ordinary => "ordinary",
            FormulaKind::SharedMaster { .. } => "shared_master",
            FormulaKind::SharedChild { .. } => "shared_child",
        }
    }

    pub(crate) fn group(self) -> Option<u64> {
        match self {
            FormulaKind::Ordinary => None,
            FormulaKind::SharedMaster { group } | FormulaKind::SharedChild { group } => Some(group),
        }
    }
}

/// One formula cell of the inventory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormulaCell {
    pub cell: CellAddress,
    pub kind: FormulaKind,
}

/// The `t` of a saved cell cache: what kind of value its `<v>` text is. The canonical record
/// always names it, and [`SavedCache`]'s decoder refuses a cache without one. A package cell with
/// no `t` attribute holds a number in OOXML, so the reader that converts such a cell writes `"n"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CacheCellType {
    /// `t="n"`: `<v>` is a decimal number.
    Number,
    /// `t="b"`: `<v>` is `0` or `1`.
    Logical,
    /// `t="e"`: `<v>` is an error literal such as `#DIV/0!`.
    Error,
    /// A text result (`t="str"`, or a shared or inline string resolved to its text).
    Text,
}

impl CacheCellType {
    fn decode(node: &Node, path: &str) -> Result<CacheCellType, DecodeError> {
        match json::text(node, path)? {
            "n" => Ok(Self::Number),
            "b" => Ok(Self::Logical),
            "e" => Ok(Self::Error),
            "str" => Ok(Self::Text),
            other => Err(DecodeError::new(
                path,
                format!("{other:?} is not n, b, e or str"),
            )),
        }
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            CacheCellType::Number => "n",
            CacheCellType::Logical => "b",
            CacheCellType::Error => "e",
            CacheCellType::Text => "str",
        }
    }
}

/// A saved cell cache: the `<v>` text a package holds for a formula cell, as written, and the
/// value it reads as. A decimal `<v>` is an observation of what Excel wrote, not a guaranteed
/// binary64 oracle: it reads as the nearest binary64, which need not be the value Excel held.
#[derive(Clone, Debug, PartialEq)]
pub struct SavedCache {
    cell_type: CacheCellType,
    text: String,
    value: CalcValue,
}

/// Whether `text` is a plain decimal number: an optional minus sign, digits, optionally a point
/// and digits, optionally an exponent.
fn is_decimal(text: &str) -> bool {
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let unsigned = text.strip_prefix('-').unwrap_or(text);
    let (mantissa, exponent) = match unsigned.find(['e', 'E']) {
        Some(i) => (&unsigned[..i], Some(&unsigned[i + 1..])),
        None => (unsigned, None),
    };
    let (whole, fraction) = match mantissa.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (mantissa, None),
    };
    digits(whole)
        && fraction.is_none_or(digits)
        && exponent.is_none_or(|e| digits(e.strip_prefix(['+', '-']).unwrap_or(e)))
}

impl SavedCache {
    /// The cache of type `cell_type` with `<v>` text `text`, or why that text is not one.
    pub fn new(cell_type: CacheCellType, text: &str) -> Result<SavedCache, String> {
        let value = match cell_type {
            CacheCellType::Number => {
                let number = is_decimal(text)
                    .then(|| text.parse::<f64>().ok())
                    .flatten()
                    .filter(|number| number.is_finite())
                    .ok_or_else(|| format!("{text:?} is not a finite decimal number"))?;
                CalcValue::number(number)
            }
            CacheCellType::Logical => match text {
                "0" => CalcValue::logical(false),
                "1" => CalcValue::logical(true),
                other => return Err(format!("{other:?} is not 0 or 1")),
            },
            CacheCellType::Error => CalcValue::error(
                error_from_literal(text)
                    .ok_or_else(|| format!("{text:?} is not an error literal"))?,
            ),
            CacheCellType::Text => CalcValue::text(ExcelText::from_utf16_code_units(
                text.encode_utf16().collect(),
            )),
        };
        Ok(SavedCache {
            cell_type,
            text: text.to_owned(),
            value,
        })
    }

    fn decode(node: &Node, path: &str) -> Result<SavedCache, DecodeError> {
        let fields = Fields::new(node, path, &["t", "v"])?;
        let cell_type = CacheCellType::decode(fields.required("t")?, &fields.at("t"))?;
        let text = json::text(fields.required("v")?, &fields.at("v"))?;
        SavedCache::new(cell_type, text).map_err(|e| DecodeError::new(&fields.at("v"), e))
    }

    pub fn cell_type(&self) -> CacheCellType {
        self.cell_type
    }

    /// The `<v>` text as the package holds it.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The value the text reads as.
    pub fn value(&self) -> &CalcValue {
        &self.value
    }
}

/// What was observed for one formula cell, with its origin. The three origins carry different
/// evidence strength and stay distinct (plan section 10.1).
#[derive(Clone, Debug, PartialEq)]
pub enum Observation {
    /// Excel's raw `Value2`, numbers as their binary64 bits, read from the Excel of the record's
    /// environment.
    ExcelRaw(CalcValue),
    /// The cache of a package Excel saved after it recalculated it.
    ExcelSavedCache(SavedCache),
    /// A cache whose freshness is unknown: a package not known to have been saved by Excel after a
    /// full recalculation.
    UnknownFreshnessCache(SavedCache),
}

const OBSERVATION_ORIGINS: [&str; 3] =
    ["excel_raw", "excel_saved_cache", "unknown_freshness_cache"];

impl Observation {
    fn decode(fields: &Fields<'_>, path: &str) -> Result<Observation, DecodeError> {
        let present: Vec<&str> = OBSERVATION_ORIGINS
            .into_iter()
            .filter(|origin| fields.optional(origin).is_some())
            .collect();
        let [origin] = present[..] else {
            return Err(DecodeError::new(
                path,
                format!(
                    "has the origins {present:?}, expected exactly one of {OBSERVATION_ORIGINS:?}"
                ),
            ));
        };
        let node = fields.required(origin)?;
        let at = fields.at(origin);
        Ok(match origin {
            "excel_raw" => Self::ExcelRaw(decode_value(node, &at)?),
            "excel_saved_cache" => Self::ExcelSavedCache(SavedCache::decode(node, &at)?),
            _ => Self::UnknownFreshnessCache(SavedCache::decode(node, &at)?),
        })
    }

    pub(crate) fn origin(&self) -> &'static str {
        match self {
            Observation::ExcelRaw(_) => "excel_raw",
            Observation::ExcelSavedCache(_) => "excel_saved_cache",
            Observation::UnknownFreshnessCache(_) => "unknown_freshness_cache",
        }
    }

    /// The value the observation says the cell holds.
    pub fn expected(&self) -> &CalcValue {
        match self {
            Observation::ExcelRaw(value) => value,
            Observation::ExcelSavedCache(cache) | Observation::UnknownFreshnessCache(cache) => {
                cache.value()
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CellObservation {
    pub cell: CellAddress,
    pub observation: Observation,
}

/// A canonical CaptureRecord. Its consistency (one entry per cell, known sheets, one master per
/// shared group, observations only for inventory cells) is checked by [`crate::judge`], so a
/// record built in memory gets the same checks as one decoded from JSON.
#[derive(Clone, Debug, PartialEq)]
pub struct CaptureRecord {
    pub case: String,
    pub source: String,
    pub acquisition: Acquisition,
    pub environment: Option<ExcelEnvironment>,
    pub sheets: Vec<SheetEntry>,
    pub formulas: Vec<FormulaCell>,
    pub observations: Vec<CellObservation>,
}

/// Checks the `format` and `version` members of a record.
pub(crate) fn check_format(
    fields: &Fields<'_>,
    format: &str,
    version: u64,
) -> Result<(), DecodeError> {
    let found = json::text(fields.required("format")?, &fields.at("format"))?;
    if found != format {
        return Err(DecodeError::new(
            &fields.at("format"),
            format!("is {found:?}, expected {format:?}"),
        ));
    }
    let found = json::unsigned(fields.required("version")?, &fields.at("version"))?;
    if found != version {
        return Err(DecodeError::new(
            &fields.at("version"),
            format!("is {found}, expected {version}"),
        ));
    }
    Ok(())
}

impl CaptureRecord {
    /// The record in `bytes` (its JSON form, described in this module's documentation).
    pub fn decode(bytes: &[u8]) -> Result<CaptureRecord, DecodeError> {
        let root = json::parse(bytes, "capture")?;
        let fields = Fields::new(
            &root,
            "capture",
            &[
                "format",
                "version",
                "case",
                "source",
                "acquisition",
                "environment",
                "sheets",
                "formulas",
                "observations",
            ],
        )?;
        check_format(&fields, CAPTURE_FORMAT, CAPTURE_VERSION)?;
        let case = json::name(fields.required("case")?, &fields.at("case"))?.to_owned();
        let source = json::name(fields.required("source")?, &fields.at("source"))?.to_owned();
        let acquisition =
            Acquisition::decode(fields.required("acquisition")?, &fields.at("acquisition"))?;
        let environment =
            ExcelEnvironment::decode(fields.required("environment")?, &fields.at("environment"))?;

        let sheets_at = fields.at("sheets");
        let sheets = json::array(fields.required("sheets")?, &sheets_at)?
            .iter()
            .enumerate()
            .map(|(i, node)| {
                let path = format!("{sheets_at}[{i}]");
                let sheet = Fields::new(node, &path, &["name", "visibility"])?;
                Ok(SheetEntry {
                    name: json::name(sheet.required("name")?, &sheet.at("name"))?.to_owned(),
                    visibility: SheetVisibility::decode(
                        sheet.required("visibility")?,
                        &sheet.at("visibility"),
                    )?,
                })
            })
            .collect::<Result<Vec<_>, DecodeError>>()?;

        let formulas_at = fields.at("formulas");
        let formulas = json::array(fields.required("formulas")?, &formulas_at)?
            .iter()
            .enumerate()
            .map(|(i, node)| {
                let path = format!("{formulas_at}[{i}]");
                let formula =
                    Fields::new(node, &path, &["sheet", "row", "column", "kind", "group"])?;
                Ok(FormulaCell {
                    cell: CellAddress::decode(&formula)?,
                    kind: FormulaKind::decode(&formula)?,
                })
            })
            .collect::<Result<Vec<_>, DecodeError>>()?;

        let observations_at = fields.at("observations");
        let observations = json::array(fields.required("observations")?, &observations_at)?
            .iter()
            .enumerate()
            .map(|(i, node)| {
                let path = format!("{observations_at}[{i}]");
                let mut allowed = vec!["sheet", "row", "column"];
                allowed.extend(OBSERVATION_ORIGINS);
                let observation = Fields::new(node, &path, &allowed)?;
                Ok(CellObservation {
                    cell: CellAddress::decode(&observation)?,
                    observation: Observation::decode(&observation, &path)?,
                })
            })
            .collect::<Result<Vec<_>, DecodeError>>()?;

        Ok(CaptureRecord {
            case,
            source,
            acquisition,
            environment,
            sheets,
            formulas,
            observations,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn saved_cache_text_is_kept_and_read_as_the_nearest_binary64() {
        let cache = SavedCache::new(CacheCellType::Number, "0.10000000000000001").unwrap();
        assert_eq!(cache.text(), "0.10000000000000001");
        assert_eq!(
            cache.value().as_number().map(f64::to_bits),
            Some(0.1f64.to_bits())
        );
        let cache = SavedCache::new(CacheCellType::Number, "-1.5E-3").unwrap();
        assert_eq!(cache.value().as_number(), Some(-0.0015));
        for bad in [
            "", "1.", ".5", "+1", "1e", "inf", "NaN", "1e999", "0x10", " 1",
        ] {
            assert!(
                SavedCache::new(CacheCellType::Number, bad).is_err(),
                "{bad:?}"
            );
        }
        assert_eq!(
            SavedCache::new(CacheCellType::Logical, "1")
                .unwrap()
                .value(),
            &CalcValue::logical(true)
        );
        assert!(SavedCache::new(CacheCellType::Logical, "TRUE").is_err());
        assert_eq!(
            SavedCache::new(CacheCellType::Error, "#N/A")
                .unwrap()
                .value(),
            &CalcValue::error(crate::value::WorksheetErrorCode::NA)
        );
        assert!(SavedCache::new(CacheCellType::Error, "#n/a").is_err());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn addresses_are_checked_and_written_in_a1() {
        assert_eq!(CellAddress::new("S", 1, 1).unwrap().a1(), "A1");
        assert_eq!(CellAddress::new("S", 7, 28).unwrap().a1(), "AB7");
        assert_eq!(
            CellAddress::new("S", MAX_ROWS, MAX_COLUMNS).unwrap().a1(),
            "XFD1048576"
        );
        assert_eq!(CellAddress::new("S", 0, 1), None);
        assert_eq!(CellAddress::new("S", 1, MAX_COLUMNS + 1), None);
        assert_eq!(CellAddress::new("", 1, 1), None);
    }
}
