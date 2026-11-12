//! `cargo xtask classify <dir>... [--recursive] [--out <dir>] [--jobs <n>] [--no-host] [--keep-names]`:
//! a structure profile of every workbook in the folders, read with our own reader (bead dc-ztvv
//! corpus classification).
//!
//! The file walk is the corpus command's (`corpus::folder_tree`); nothing is calculated and
//! Excel is never started. Per file it records the format and size, the package's parts, the
//! sheets and their used ranges, every formula cell with its distinct functions and the formula
//! features (dynamic arrays and spill, LET/LAMBDA, structured references, 3-D and external
//! references, array and shared formulas, volatile functions), the defined names and their
//! scopes, the calculation settings, and every other structure the package holds (tables, pivots,
//! charts, drawings, validation, conditional formats, comments, hyperlinks, protection, VBA,
//! embedded objects, connections, custom XML). It then opens the file through the host route
//! (`calc_host::HostState::open`, which is `calc_doc::open` and `load`) and records what the
//! reader refused, warned about or held as not loaded.
//!
//! Output, below `--out` (default `target/classify`): `files.jsonl`, one JSON object per file,
//! and `summary.md`, the counts over all files. The JSON lines hold file names and sheet names,
//! so a corpus of other people's files stays outside the repository; sheet names are left out of
//! them unless `--keep-names` is given. The summary holds counts and hashes only.
//!
//! Admission uses a shared 1 GiB working-data allowance (at most two readers), 64 MiB per
//! source, 128 MiB of expanded parts and 32 MiB per part. Oversized files are recorded with
//! `admission_refusal`, before reading or hashing them. JSONL is written in bounded batches;
//! only aggregate counts, distinct keys/hashes and the size histogram survive each batch.
//!
//! Exit codes: 0 when the output was written, 1 when it cannot be, 2 on a usage error.

use crate::corpus::folder_tree;
use calc_doc::{MemorySource, OpenPolicy, XmlEvent, XmlPart};
use calc_verdict::sha256_hex;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::io::{BufRead, Read, Write as IoWrite};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Instant;

pub const USAGE: &str = "usage: cargo xtask classify <dir>... [--recursive] [--out <dir>] [--jobs <n>] [--no-host] [--keep-names]";

type Counts = BTreeMap<String, u64>;

fn bump(map: &mut Counts, key: impl Into<String>, by: u64) {
    *map.entry(key.into()).or_default() += by;
}

/// Functions whose result can change without any precedent changing.
const VOLATILE: &[&str] = &[
    "NOW", "TODAY", "RAND", "RANDBETWEEN", "RANDARRAY", "OFFSET", "INDIRECT", "INFO", "CELL",
];

/// What one formula text uses: function call counts and feature flags (each flag counts once).
#[derive(Default, Clone, Debug, PartialEq)]
pub(crate) struct FormulaStats {
    pub(crate) functions: Counts,
    /// Per call, `NAME(arg tokens)`: see `formula_shapes`.
    pub(crate) shapes: Counts,
    pub(crate) flags: Counts,
    pub(crate) length: usize,
    pub(crate) depth: usize,
}

impl FormulaStats {
    fn add_scaled(&self, into: &mut FormulaStats, times: u64) {
        for (k, v) in &self.functions {
            bump(&mut into.functions, k.clone(), v * times);
        }
        for (k, v) in &self.shapes {
            bump(&mut into.shapes, k.clone(), v * times);
        }
        for (k, v) in &self.flags {
            bump(&mut into.flags, k.clone(), v * times);
        }
        into.length = into.length.max(self.length);
        into.depth = into.depth.max(self.depth);
    }
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '.' | '\\')
}

/// Bound checked before allocating a formula's character buffer.
const MAX_FORMULA_BYTES: usize = 1 << 20;

/// A scan that cannot certify the whole formula never supplies capture eligibility.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScanRefusal {
    WorkBudget,
    MalformedFormula,
}

impl std::fmt::Display for ScanRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::WorkBudget => "formula_work_budget",
            Self::MalformedFormula => "malformed_formula",
        })
    }
}

#[derive(Debug)]
enum ScanError {
    Xml(String),
    Formula(ScanRefusal),
    MissingSharedMaster,
}
impl ScanError {
    fn kind(&self) -> &'static str {
        match self {
            Self::Xml(_) => "xml_scan_error",
            Self::Formula(ScanRefusal::WorkBudget) => "formula_work_budget",
            Self::Formula(ScanRefusal::MalformedFormula) => "malformed_formula",
            Self::MissingSharedMaster => "missing_shared_formula_master",
        }
    }
    fn record(&self) -> Value {
        json!({"kind": self.kind(), "message": self.to_string()})
    }
}
impl std::fmt::Display for ScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Xml(message) => f.write_str(message),
            _ => f.write_str(self.kind()),
        }
    }
}

/// A lexical scan, not an evaluator: strings and quoted sheet names are skipped. Shape work
/// has a linear allowance; an incomplete scan returns a typed refusal instead of partial data.
pub(crate) fn analyze_formula(text: &str) -> Result<FormulaStats, ScanRefusal> {
    if text.len() > MAX_FORMULA_BYTES {
        return Err(ScanRefusal::WorkBudget);
    }
    let chars: Vec<char> = text.chars().collect();
    let mut out = FormulaStats {
        length: chars.len(),
        ..FormulaStats::default()
    };
    let flag = |out: &mut FormulaStats, name: &str| bump(&mut out.flags, name, 1);
    let mut flagged: BTreeSet<&'static str> = BTreeSet::new();
    let mut note = |out: &mut FormulaStats, name: &'static str| {
        if flagged.insert(name) {
            flag(out, name);
        }
    };
    let mut calls = Vec::new();
    let (mut i, mut depth) = (0usize, 0usize);
    while i < chars.len() {
        let c = chars[i];
        match c {
            '"' => {
                i += 1;
                while i < chars.len() {
                    if chars[i] == '"' {
                        if chars.get(i + 1) == Some(&'"') {
                            i += 2;
                            continue;
                        }
                        break;
                    }
                    i += 1;
                }
                i += 1;
            }
            '\'' => {
                let start = i + 1;
                i += 1;
                while i < chars.len() {
                    if chars[i] == '\'' {
                        if chars.get(i + 1) == Some(&'\'') {
                            i += 2;
                            continue;
                        }
                        break;
                    }
                    i += 1;
                }
                let quoted: String = chars[start..i.min(chars.len())].iter().collect();
                i += 1;
                if chars.get(i) == Some(&'!') {
                    if quoted.starts_with('[') {
                        note(&mut out, "external_reference");
                    }
                    if quoted.contains(':') {
                        note(&mut out, "three_d_reference");
                    }
                }
            }
            '(' => {
                depth += 1;
                out.depth = out.depth.max(depth);
                i += 1;
            }
            ')' => {
                depth = depth.saturating_sub(1);
                i += 1;
            }
            '{' => {
                note(&mut out, "array_constant");
                i += 1;
            }
            '@' => {
                note(&mut out, "implicit_intersection_operator");
                i += 1;
            }
            '[' => {
                // `[1]Sheet!A1` (an external workbook index) or a structured reference.
                let digits = chars[i + 1..]
                    .iter()
                    .take_while(|c| c.is_ascii_digit())
                    .count();
                let close = i + 1 + digits;
                if digits > 0
                    && chars.get(close) == Some(&']')
                    && chars
                        .get(close + 1)
                        .is_some_and(|c| is_ident(*c) || *c == '\'')
                {
                    note(&mut out, "external_reference");
                } else {
                    note(&mut out, "structured_reference");
                }
                i += 1;
            }
            '#' => {
                let prev = i.checked_sub(1).map(|p| chars[p]);
                let next = chars.get(i + 1).copied();
                if prev.is_some_and(|p| p.is_alphanumeric() || p == '$' || p == ')')
                    && !next.is_some_and(|n| n.is_alphanumeric())
                {
                    note(&mut out, "spill_reference");
                }
                i += 1;
            }
            c if is_ident(c) => {
                let start = i;
                while i < chars.len() && is_ident(chars[i]) {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                match chars.get(i) {
                    Some('(') => {
                        let upper = word.to_ascii_uppercase();
                        let mut name = upper.as_str();
                        loop {
                            if let Some(rest) = name
                                .strip_prefix("_XLFN.")
                                .or_else(|| name.strip_prefix("_XLWS."))
                            {
                                name = rest;
                            } else {
                                break;
                            }
                        }
                        let name = name.to_owned();
                        if matches!(name.as_str(), "LET") {
                            note(&mut out, "let");
                        }
                        if matches!(name.as_str(), "LAMBDA") {
                            note(&mut out, "lambda");
                        }
                        if name == "ANCHORARRAY" {
                            note(&mut out, "spill_reference");
                        }
                        if name.starts_with("_XLL.") {
                            note(&mut out, "add_in_call");
                        }
                        if name.starts_with("_XLUDF.") {
                            note(&mut out, "future_or_user_defined_function");
                        }
                        if VOLATILE.contains(&name.as_str()) {
                            note(&mut out, "volatile");
                        }
                        calls.push((i, name.clone()));
                        bump(&mut out.functions, name, 1);
                    }
                    Some('!') => {
                        // `Sheet1:Sheet3!A1`: two names around a colon before the bang.
                        if word.contains(':') {
                            note(&mut out, "three_d_reference");
                        }
                    }
                    Some(':') => {
                        // The first name of `Sheet1:Sheet3!`: look ahead for the bang.
                        let mut j = i + 1;
                        while j < chars.len() && is_ident(chars[j]) {
                            j += 1;
                        }
                        if chars.get(j) == Some(&'!') && !word.chars().all(|c| c.is_ascii_digit()) {
                            note(&mut out, "three_d_reference");
                        }
                    }
                    _ => {}
                }
            }
            _ => i += 1,
        }
    }
    out.shapes = crate::formula_shapes::call_shapes(&chars, &calls)?;
    Ok(out)
}

/// Everything one worksheet part says.
#[derive(Default, Debug)]
struct SheetScan {
    dimension: Option<String>,
    rows: u64,
    cells: u64,
    cell_types: Counts,
    formula_cells: u64,
    formula_kinds: Counts,
    formula_cached_errors: u64,
    dynamic_array_cells: u64,
    stats: FormulaStats,
    elements: Counts,
    cf_rule_types: Counts,
    ext_elements: Counts,
    max_formula_len: usize,
}

fn local(e: &calc_doc::XmlElement) -> &str {
    e.name.local.as_str()
}

/// Elements of a worksheet that are counted by name.
const SHEET_ELEMENTS: &[&str] = &[
    "mergeCell", "dataValidation", "conditionalFormatting", "hyperlink", "sheetProtection",
    "protectedRange", "legacyDrawing", "legacyDrawingHF", "drawing", "tablePart", "autoFilter",
    "sortState", "pane", "rowBreaks", "colBreaks", "pageSetup", "headerFooter", "picture",
    "oleObject", "control", "controls", "customSheetView", "sheetCalcPr", "scenarios", "dataConsolidate",
    "phoneticPr", "ignoredError", "cellWatch", "smartTag", "webPublishItem", "extLst", "cols",
    "sheetPr", "tabColor", "outlinePr", "printOptions", "rowBreaks", "ignoredErrors", "sheetFormatPr",
    "sheetView", "selection", "userShape", "customProperty",
];

fn scan_sheet(xml: &XmlPart) -> Result<SheetScan, ScanError> {
    let mut scan = SheetScan::default();
    let mut reader = xml.reader();
    let mut depth = 0u32;
    let mut ext_depth: Option<u32> = None;
    // Current cell state.
    let mut cell: Option<(String, bool, bool)> = None; // (type, has formula, dynamic)
    let mut in_f = false;
    let mut f_text = String::new();
    let mut f_kind = String::new();
    let mut f_si: Option<u32> = None;
    let mut in_v = false;
    let mut shared_masters: BTreeMap<u32, FormulaStats> = BTreeMap::new();
    let mut shared_members: BTreeMap<u32, u64> = BTreeMap::new();
    let mut pending_member: Option<u32> = None;
    while let Some(event) = reader.next_event().map_err(|e| ScanError::Xml(e.to_string()))? {
        match event {
            XmlEvent::Start(e) => {
                depth += 1;
                let name = local(&e).to_owned();
                if ext_depth.is_some() {
                    if name == "ext" {
                        let uri = e.attribute("uri").unwrap_or("?").to_owned();
                        bump(&mut scan.ext_elements, uri, 1);
                    }
                    continue;
                }
                match name.as_str() {
                    "dimension" => scan.dimension = e.attribute("ref").map(str::to_owned),
                    "row" => scan.rows += 1,
                    "c" => {
                        scan.cells += 1;
                        let ty = e.attribute("t").unwrap_or("n").to_owned();
                        bump(&mut scan.cell_types, ty.clone(), 1);
                        let dynamic = e.attribute("cm").is_some();
                        cell = Some((ty, false, dynamic));
                    }
                    "f" if cell.is_some() => {
                        in_f = true;
                        f_text.clear();
                        f_kind = e.attribute("t").unwrap_or("normal").to_owned();
                        f_si = e.attribute("si").and_then(|s| s.parse().ok());
                        if let Some(c) = cell.as_mut() {
                            c.1 = true;
                        }
                    }
                    "v" if cell.as_ref().is_some_and(|c| c.1) => in_v = true,
                    "cfRule" => {
                        bump(&mut scan.cf_rule_types, e.attribute("type").unwrap_or("?"), 1);
                        bump(&mut scan.elements, "cfRule", 1);
                    }
                    "extLst" => {
                        ext_depth = Some(depth);
                        bump(&mut scan.elements, "extLst", 1);
                    }
                    n if SHEET_ELEMENTS.contains(&n) => {
                        let key = n.to_owned();
                        if n == "sheetProtection" || n == "hyperlink" || n == "mergeCell" || n == "dataValidation" {
                            bump(&mut scan.elements, key, 1);
                        } else {
                            bump(&mut scan.elements, key, 1);
                        }
                        if n == "dataValidation"
                            && let Some(t) = e.attribute("type")
                        {
                            bump(&mut scan.elements, format!("dataValidation:{t}"), 1);
                        }
                    }
                    _ => {}
                }
            }
            XmlEvent::Text(t) => {
                if in_f {
                    f_text.push_str(&t);
                }
                let _ = in_v;
            }
            XmlEvent::End(name) => {
                let n = name.local.as_str();
                if let Some(at) = ext_depth {
                    if n == "extLst" && depth == at {
                        ext_depth = None;
                    }
                    depth -= 1;
                    continue;
                }
                match n {
                    "f" if in_f => {
                        in_f = false;
                        scan.formula_cells += 1;
                        bump(&mut scan.formula_kinds, f_kind.clone(), 1);
                        let trimmed = f_text.trim_start_matches('=');
                        scan.max_formula_len = scan.max_formula_len.max(trimmed.chars().count());
                        if f_kind == "shared" {
                            match (f_si, trimmed.is_empty()) {
                                (Some(si), false) => {
                                    let stats = analyze_formula(trimmed).map_err(ScanError::Formula)?;
                                    stats.add_scaled(&mut scan.stats, 1);
                                    shared_masters.insert(si, stats);
                                }
                                (Some(si), true) => *shared_members.entry(si).or_default() += 1,
                                _ => {}
                            }
                            pending_member = f_si;
                        } else if f_kind == "dataTable" {
                            // No text: a what-if table's cells hold no formula text.
                        } else if !trimmed.is_empty() {
                            analyze_formula(trimmed).map_err(ScanError::Formula)?.add_scaled(&mut scan.stats, 1);
                        }
                        if f_kind == "array" {
                            bump(&mut scan.stats.flags, "array_formula", 1);
                        }
                    }
                    "v" => in_v = false,
                    "c" => {
                        if let Some((ty, has_f, dynamic)) = cell.take() {
                            if has_f {
                                if dynamic {
                                    scan.dynamic_array_cells += 1;
                                }
                                if ty == "e" {
                                    scan.formula_cached_errors += 1;
                                }
                            }
                        }
                        pending_member = None;
                    }
                    _ => {}
                }
                let _ = pending_member;
                depth -= 1;
            }
        }
    }
    // A shared formula's members use the master's functions and features.
    for (si, members) in shared_members {
        if let Some(master) = shared_masters.get(&si) {
            master.add_scaled(&mut scan.stats, members);
        } else {
            return Err(ScanError::MissingSharedMaster);
        }
    }
    Ok(scan)
}

/// Per-cell cached values need the `<v>` that follows a formula; counted in a second light pass.
fn count_cached_formula_values(xml: &XmlPart) -> u64 {
    // A formula cell holds a cached value when its `<c>` element contains a `<v>` element.
    let mut reader = xml.reader();
    let (mut in_c, mut has_f, mut n) = (false, false, 0u64);
    while let Ok(Some(event)) = reader.next_event() {
        match event {
            XmlEvent::Start(e) => match local(&e) {
                "c" => (in_c, has_f) = (true, false),
                "f" if in_c => has_f = true,
                "v" if in_c && has_f => n += 1,
                _ => {}
            },
            XmlEvent::End(name) if name.local.as_str() == "c" => in_c = false,
            _ => {}
        }
    }
    n
}

#[derive(Default)]
struct NamesScan {
    total: u64,
    hidden: u64,
    workbook_scope: u64,
    sheet_scope: u64,
    builtin: Counts,
    stats: FormulaStats,
    refs_error: u64,
    functions_in_names: u64,
}

#[derive(Default)]
struct WorkbookScan {
    sheets: Vec<(String, String, String)>, // name, state, r:id
    names: NamesScan,
    date1904: bool,
    iterate: bool,
    calc_mode: Option<String>,
    full_calc_on_load: bool,
    ref_mode: Option<String>,
    compat: Option<String>,
    external_references: u64,
    pivot_caches: u64,
    workbook_protection: bool,
    file_sharing: bool,
    code_name: Option<String>,
    function_groups: u64,
    web_publishing: bool,
}

fn scan_workbook(xml: &XmlPart) -> Result<WorkbookScan, ScanError> {
    let mut w = WorkbookScan::default();
    let mut reader = xml.reader();
    let mut in_name: Option<(bool, bool)> = None;
    let mut text = String::new();
    while let Some(event) = reader.next_event().map_err(|e| ScanError::Xml(e.to_string()))? {
        match event {
            XmlEvent::Start(e) => match local(&e) {
                "workbookPr" => {
                    w.date1904 = matches!(e.attribute("date1904"), Some("1" | "true"));
                    w.code_name = e.attribute("codeName").map(str::to_owned);
                }
                "calcPr" => {
                    w.iterate = matches!(e.attribute("iterate"), Some("1" | "true"));
                    w.calc_mode = e.attribute("calcMode").map(str::to_owned);
                    w.full_calc_on_load = matches!(e.attribute("fullCalcOnLoad"), Some("1" | "true"));
                    w.ref_mode = e.attribute("refMode").map(str::to_owned);
                    w.compat = e.attribute("calcId").map(str::to_owned);
                }
                "sheet" => w.sheets.push((
                    e.attribute("name").unwrap_or("").to_owned(),
                    e.attribute("state").unwrap_or("visible").to_owned(),
                    e.attributes
                        .iter()
                        .find(|a| a.name.local.as_str() == "id")
                        .map(|a| a.value.clone())
                        .unwrap_or_default(),
                )),
                "definedName" => {
                    w.names.total += 1;
                    let hidden = matches!(e.attribute("hidden"), Some("1" | "true"));
                    if hidden {
                        w.names.hidden += 1;
                    }
                    let scoped = e.attribute("localSheetId").is_some();
                    if scoped {
                        w.names.sheet_scope += 1;
                    } else {
                        w.names.workbook_scope += 1;
                    }
                    if let Some(n) = e.attribute("name")
                        && let Some(b) = n.strip_prefix("_xlnm.")
                    {
                        bump(&mut w.names.builtin, b, 1);
                    }
                    in_name = Some((hidden, scoped));
                    text.clear();
                }
                "externalReference" => w.external_references += 1,
                "pivotCache" => w.pivot_caches += 1,
                "workbookProtection" => w.workbook_protection = true,
                "fileSharing" => w.file_sharing = true,
                "functionGroup" => w.function_groups += 1,
                "webPublishing" => w.web_publishing = true,
                _ => {}
            },
            XmlEvent::Text(t) if in_name.is_some() => text.push_str(&t),
            XmlEvent::End(name) if name.local.as_str() == "definedName" && in_name.is_some() => {
                in_name = None;
                if text.contains("#REF!") {
                    w.names.refs_error += 1;
                }
                let stats = analyze_formula(text.trim_start_matches('=')).map_err(ScanError::Formula)?;
                if !stats.functions.is_empty() {
                    w.names.functions_in_names += 1;
                }
                stats.add_scaled(&mut w.names.stats, 1);
            }
            _ => {}
        }
    }
    Ok(w)
}

fn relationship_target(package: &calc_doc::Package, source: &str, id: &str) -> Option<String> {
    let rels = package.relationships_of(Some(source).filter(|s| !s.is_empty()))?;
    rels.items().iter().find(|r| r.id == id)?.resolved.clone()
}

/// A short category for a package part: its folder, or for a part at the top of `xl/` its file
/// name without digits.
fn part_category(name: &str) -> String {
    let n = name.trim_start_matches('/');
    if n.ends_with(".rels") {
        return "relationships".into();
    }
    match n.rsplit_once('/') {
        Some(("xl", file)) => {
            let stem = file.split('.').next().unwrap_or(file).trim_end_matches(|c: char| c.is_ascii_digit());
            if file.ends_with(".bin") { format!("xl/{stem}.bin") } else { format!("xl/{stem}") }
        }
        Some((dir, _)) => dir.to_owned(),
        None => n.to_owned(),
    }
}

/// The VBA modules of a project, counted from the project stream's `Module=`, `Document=`,
/// `Class=` and `BaseClass=` lines, which Excel stores uncompressed. A heuristic over the raw
/// compound file; it does not decode the project.
fn vba_modules(bytes: &[u8]) -> BTreeMap<&'static str, u64> {
    let mut found = BTreeMap::new();
    for (key, needle) in [
        ("module", "Module="),
        ("document", "Document="),
        ("class", "Class="),
        ("baseclass", "BaseClass="),
    ] {
        let n = needle.as_bytes();
        let mut count = 0u64;
        let mut i = 0;
        while i + n.len() <= bytes.len() {
            if &bytes[i..i + n.len()] == n && (i == 0 || bytes[i - 1] == b'\n') {
                count += 1;
                i += n.len();
            } else {
                i += 1;
            }
        }
        found.insert(key, count);
    }
    found
}

fn magic_format(bytes: &[u8], extension: &str) -> &'static str {
    if bytes.starts_with(&[0xD0, 0xCF, 0x11, 0xE0]) {
        return "ole-compound (xls/other)";
    }
    if bytes.starts_with(b"PK") {
        return match extension {
            "xlsb" => "xlsb",
            "xlsm" => "xlsm",
            "xlsx" => "xlsx",
            "xls" => "zip-named-xls",
            _ => "zip",
        };
    }
    if bytes.starts_with(b"<?xml") || bytes.starts_with(b"<html") {
        return "xml/html text";
    }
    "unknown"
}

/// The profile of one workbook's bytes.
pub(crate) fn classify_bytes(bytes: &[u8], name: &str, keep_names: bool, host: bool) -> Value {
    classify_shared(Arc::from(bytes), name, keep_names, host)
}

fn classify_shared(bytes: Arc<[u8]>, name: &str, keep_names: bool, host: bool) -> Value {
    let started = Instant::now();
    let source = Arc::new(MemorySource::new(bytes.clone()).named(name));
    let policy = classify_policy();
    let extension = Path::new(name)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let mut profile = json!({
        "file": name,
        "sha256": sha256_hex(&bytes),
        "bytes": bytes.len(),
        "extension": extension,
        "format": magic_format(&bytes, &extension),
        "scan_complete": false,
    });
    let object = profile.as_object_mut().expect("object");
    let opened = calc_doc::open(source.clone(), &policy);
    let document = match opened {
        Ok(d) => d,
        Err(e) => {
            object.insert(
                "package".into(),
                json!({"opened": false, "error": format!("{e}")}),
            );
            object.insert(
                "scan_ms".into(),
                json!(started.elapsed().as_millis() as u64),
            );
            return profile;
        }
    };
    let package = document.package();
    // Package.
    let mut categories = Counts::new();
    let mut category_bytes = Counts::new();
    for part in package.parts() {
        let cat = part_category(part.name());
        bump(&mut categories, cat.clone(), 1);
        bump(&mut category_bytes, cat, part.size());
    }
    let main_type = package
        .relationships_of(None)
        .and_then(|r| {
            r.items()
                .iter()
                .find(|i| i.kind.ends_with("/officeDocument"))
        })
        .and_then(|i| i.resolved.clone())
        .and_then(|p| {
            package
                .part(&p)
                .and_then(|p| p.content_type().map(str::to_owned))
        });
    object.insert(
        "package".into(),
        json!({
            "opened": true,
            "parts": package.parts().len(),
            "part_categories": categories,
            "part_category_bytes": category_bytes,
            "main_content_type": main_type,
            "binary_converted": extension == "xlsb",
        }),
    );
    // Workbook and sheets.
    let workbook_part = "xl/workbook.xml";
    let workbook_part = package
        .relationships_of(None)
        .and_then(|r| {
            r.items()
                .iter()
                .find(|i| i.kind.ends_with("/officeDocument"))
                .and_then(|i| i.resolved.clone())
        })
        .unwrap_or_else(|| workbook_part.to_owned());
    let mut wb = WorkbookScan::default();
    match package
        .read_xml(&workbook_part)
        .map_err(|e| ScanError::Xml(e.to_string()))
        .and_then(|x| scan_workbook(&x))
    {
        Ok(w) => wb = w,
        Err(e) => {
            object.insert("workbook_error".into(), e.record());
        }
    }
    let mut sheet_total = SheetScan::default();
    let mut sheets_json = Vec::new();
    let mut sheet_kinds = Counts::new();
    let mut scan_errors = Vec::new();
    let mut cached_formula_values = 0u64;
    for (index, (sheet_name, state, rid)) in wb.sheets.iter().enumerate() {
        let target = relationship_target(package, &workbook_part, rid);
        let kind = package
            .relationships_of(Some(&workbook_part))
            .and_then(|r| {
                r.items()
                    .iter()
                    .find(|i| i.id == *rid)
                    .map(|i| i.kind.rsplit('/').next().unwrap_or("").to_owned())
            })
            .unwrap_or_default();
        bump(&mut sheet_kinds, kind.clone(), 1);
        bump(&mut sheet_kinds, format!("state:{state}"), 1);
        let mut entry = json!({"index": index, "state": state, "kind": kind});
        if keep_names {
            entry["name"] = json!(sheet_name);
        }
        if let Some(part) = target {
            if kind == "worksheet" || kind == "macrosheet" || kind == "dialogsheet" {
                match package.read_xml(&part) {
                    Ok(xml) => match scan_sheet(&xml) {
                        Ok(scan) => {
                            cached_formula_values += count_cached_formula_values(&xml);
                            entry["dimension"] = json!(scan.dimension);
                            entry["cells"] = json!(scan.cells);
                            entry["formula_cells"] = json!(scan.formula_cells);
                            sheet_total.rows += scan.rows;
                            sheet_total.cells += scan.cells;
                            sheet_total.formula_cells += scan.formula_cells;
                            sheet_total.dynamic_array_cells += scan.dynamic_array_cells;
                            sheet_total.formula_cached_errors += scan.formula_cached_errors;
                            sheet_total.max_formula_len =
                                sheet_total.max_formula_len.max(scan.max_formula_len);
                            for (k, v) in &scan.cell_types {
                                bump(&mut sheet_total.cell_types, k.clone(), *v);
                            }
                            for (k, v) in &scan.formula_kinds {
                                bump(&mut sheet_total.formula_kinds, k.clone(), *v);
                            }
                            for (k, v) in &scan.elements {
                                bump(&mut sheet_total.elements, k.clone(), *v);
                            }
                            for (k, v) in &scan.cf_rule_types {
                                bump(&mut sheet_total.cf_rule_types, k.clone(), *v);
                            }
                            for (k, v) in &scan.ext_elements {
                                bump(&mut sheet_total.ext_elements, k.clone(), *v);
                            }
                            scan.stats.add_scaled(&mut sheet_total.stats, 1);
                        }
                        Err(e) => scan_errors.push(json!({"sheet": index, "refusal": e.record()})),
                    },
                    Err(e) => scan_errors.push(
                        json!({"sheet": index, "refusal": ScanError::Xml(e.to_string()).record()}),
                    ),
                }
            }
        } else {
            scan_errors
                .push(json!({"sheet": index, "refusal": {"kind": "missing_relationship_target"}}));
        }
        sheets_json.push(entry);
    }
    object.insert(
        "sheets".into(),
        json!({"count": wb.sheets.len(), "kinds": sheet_kinds, "list": sheets_json}),
    );
    object.insert(
        "cells".into(),
        json!({
            "rows": sheet_total.rows,
            "cells": sheet_total.cells,
            "types": sheet_total.cell_types,
            "formula_cells": sheet_total.formula_cells,
            "formula_kinds": sheet_total.formula_kinds,
            "formula_cached_values": cached_formula_values,
            "formula_cached_errors": sheet_total.formula_cached_errors,
            "dynamic_array_cells": sheet_total.dynamic_array_cells,
            "max_formula_chars": sheet_total.max_formula_len,
        }),
    );
    object.insert("formula_features".into(), json!(sheet_total.stats.flags));
    object.insert("functions".into(), json!(sheet_total.stats.functions));
    object.insert(
        "function_shapes".into(),
        json!(capped_shapes(&sheet_total.stats.shapes)),
    );
    object.insert("sheet_elements".into(), json!(sheet_total.elements));
    object.insert(
        "conditional_rule_types".into(),
        json!(sheet_total.cf_rule_types),
    );
    object.insert("sheet_extensions".into(), json!(sheet_total.ext_elements));
    object.insert(
        "names".into(),
        json!({
            "total": wb.names.total,
            "hidden": wb.names.hidden,
            "workbook_scope": wb.names.workbook_scope,
            "sheet_scope": wb.names.sheet_scope,
            "builtin": wb.names.builtin,
            "with_functions": wb.names.functions_in_names,
            "ref_errors": wb.names.refs_error,
            "features": wb.names.stats.flags,
            "functions": wb.names.stats.functions,
        }),
    );
    object.insert(
        "workbook".into(),
        json!({
            "date1904": wb.date1904,
            "iterate": wb.iterate,
            "calc_mode": wb.calc_mode,
            "full_calc_on_load": wb.full_calc_on_load,
            "ref_mode": wb.ref_mode,
            "calc_id": wb.compat,
            "external_references": wb.external_references,
            "pivot_caches": wb.pivot_caches,
            "workbook_protection": wb.workbook_protection,
            "file_sharing": wb.file_sharing,
            "code_name": wb.code_name.is_some(),
        }),
    );
    // Producer.
    if let Ok(app) = package.read_xml("docProps/app.xml") {
        let mut reader = app.reader();
        let (mut cur, mut application, mut version) = (String::new(), None, None);
        while let Ok(Some(event)) = reader.next_event() {
            match event {
                XmlEvent::Start(e) => cur = local(&e).to_owned(),
                XmlEvent::Text(t) if cur == "Application" => application = Some(t),
                XmlEvent::Text(t) if cur == "AppVersion" => version = Some(t),
                XmlEvent::End(_) => cur.clear(),
                _ => {}
            }
        }
        object.insert(
            "producer".into(),
            json!({"application": application, "app_version": version}),
        );
    }
    // VBA.
    if let Some(vba) = package
        .parts()
        .iter()
        .find(|p| p.name().ends_with("vbaProject.bin"))
        && let Ok(bin) = package.read_part(vba.name())
    {
        object.insert(
            "vba".into(),
            json!({"bytes": bin.len(), "project_lines": vba_modules(&bin)}),
        );
    }
    object.insert(
        "scan_complete".into(),
        json!(scan_errors.is_empty() && !object.contains_key("workbook_error")),
    );
    if !scan_errors.is_empty() {
        object.insert("scan_errors".into(), json!(scan_errors));
    }
    // The reader's own outcome.
    drop(document);
    if host {
        object.insert("reader".into(), reader_outcome(source, &policy));
    }
    object.insert(
        "scan_ms".into(),
        json!(started.elapsed().as_millis() as u64),
    );
    profile
}

fn reader_outcome(source: Arc<MemorySource>, policy: &OpenPolicy) -> Value {
    use calc_host::{HostOptions, HostState};
    let opened = HostState::open(
        source,
        policy,
        HostOptions {
            formula_cell_budget: None,
            ..HostOptions::default()
        },
    );
    let host = match opened {
        Ok(h) => h,
        Err(e) => {
            return json!({
                "opened": false,
                "key": crate::compare::open_key(&e),
                "message": e.to_string().chars().take(200).collect::<String>(),
            });
        }
    };
    let loaded = host.loaded();
    let open_report = host.open_report();
    let mut formulas = 0u64;
    let (mut validations, mut cond, mut merged, mut notes, mut links) =
        (0u64, 0u64, 0u64, 0u64, 0u64);
    let (mut protected, mut macro_sheets) = (0u64, 0u64);
    let mut unsupported = Counts::new();
    let mut conditional_extensions = 0u64;
    for sheet in loaded.sheets() {
        formulas += sheet.formulas().count() as u64;
        validations += sheet.validations().len() as u64;
        cond += sheet.conditional_formats().len() as u64;
        merged += sheet.merged_areas().len() as u64;
        notes += sheet.review().notes.len() as u64;
        links += sheet.review().links.len() as u64;
        protected += u64::from(sheet.protected());
        macro_sheets += u64::from(sheet.is_macro_sheet());
        conditional_extensions += u64::from(sheet.review().conditional_extensions);
        for u in &sheet.review().unsupported {
            bump(&mut unsupported, u.clone(), 1);
        }
    }
    let mut unloaded = Counts::new();
    for (_, why) in host.model().definitions().unloaded_cells() {
        let text = format!("{why:?}");
        let key = text
            .split(|c: char| !c.is_alphanumeric())
            .next()
            .unwrap_or("")
            .to_owned();
        bump(&mut unloaded, key, 1);
    }
    json!({
        "opened": true,
        "repair_count": open_report.repairs.len(),
        "repairs": open_report.repair_counts,
        "date_system": format!("{:?}", loaded.date_system()),
        "calc_mode": format!("{:?}", loaded.calc_mode()),
        "sheets": loaded.sheets().len(),
        "formula_cells_loaded": formulas,
        "validations": validations,
        "conditional_formats": cond,
        "merged_areas": merged,
        "notes": notes,
        "links": links,
        "protected_sheets": protected,
        "macro_sheets": macro_sheets,
        "conditional_extension_sheets": conditional_extensions,
        "has_vba_project": loaded.has_vba_project(),
        "layout_warnings": loaded.layout_warnings().len(),
        "defined_names_not_loaded": loaded.defined_names_not_loaded(),
        "unloaded_cells": unloaded,
        "unsupported": unsupported,
    })
}

struct Args {
    dirs: Vec<PathBuf>,
    recursive: bool,
    out: PathBuf,
    jobs: usize,
    host: bool,
    keep_names: bool,
}

fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut parsed = Args {
        dirs: Vec::new(),
        recursive: false,
        out: PathBuf::from("target/classify"),
        jobs: std::thread::available_parallelism().map_or(1, |n| n.get()),
        host: true,
        keep_names: false,
    };
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--recursive" => parsed.recursive = true,
            "--no-host" => parsed.host = false,
            "--keep-names" => parsed.keep_names = true,
            "--out" => {
                parsed.out = PathBuf::from(rest.next().ok_or("--out needs a value")?);
            }
            "--jobs" => {
                parsed.jobs = rest
                    .next()
                    .and_then(|v| v.parse().ok())
                    .filter(|n| *n > 0)
                    .ok_or("--jobs needs a positive number")?;
            }
            flag if flag.starts_with("--") => return Err(format!("unknown flag {flag}")),
            dir => parsed.dirs.push(PathBuf::from(dir)),
        }
    }
    if parsed.dirs.is_empty() {
        return Err("at least one folder is needed".into());
    }
    Ok(parsed)
}

pub fn run(args: &[String]) -> ExitCode {
    let args = match parse_args(args) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("xtask classify: {e}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let mut files = Vec::new();
    for dir in &args.dirs {
        match folder_tree(dir, args.recursive) {
            Ok(found) => files.extend(found.into_iter().map(|e| (e.path, e.name))),
            Err(e) => {
                eprintln!("xtask classify: {e}");
                return ExitCode::from(1);
            }
        }
    }
    files.sort();
    files.dedup();
    if let Err(e) = std::fs::create_dir_all(&args.out) {
        eprintln!("xtask classify: cannot make {}: {e}", args.out.display());
        return ExitCode::from(1);
    }
    let output = match std::fs::File::create(args.out.join("files.jsonl")) {
        Ok(file) => file,
        Err(e) => {
            eprintln!("xtask classify: cannot write files.jsonl: {e}");
            return ExitCode::from(1);
        }
    };
    let started = Instant::now();
    let jobs = args
        .jobs
        .min(files.len())
        .min((MEMORY_BUDGET_BYTES / WORKER_MEMORY_BYTES) as usize)
        .max(1);
    let budget = MemoryBudget::new(MEMORY_BUDGET_BYTES, jobs);
    let mut writer = std::io::BufWriter::new(output);
    let mut summary = ProfileSummary::default();
    let written = std::thread::scope(|scope| -> std::io::Result<()> {
        // A bounded batch preserves the walk order and keeps at most `jobs` results alive.
        // A slow first file cannot make the other workers accumulate the rest of the corpus.
        for batch in files.chunks(jobs) {
            let tasks: Vec<_> = batch
                .iter()
                .map(|(path, name)| {
                    let budget = &budget;
                    let args = &args;
                    scope.spawn(move || {
                        let permit = budget.reserve(WORKER_MEMORY_BYTES);
                        (
                            permit,
                            classify_file(path, name, args.keep_names, args.host),
                        )
                    })
                })
                .collect();
            for task in tasks {
                let (_permit, value) = task.join().expect("classification worker");
                serde_json::to_writer(&mut writer, &value).map_err(std::io::Error::other)?;
                writer.write_all(b"\n")?;
                summary.add(&value);
            }
            // Persist each batch even if a later workbook or write fails.
            writer.flush()?;
        }
        Ok(())
    });
    let elapsed = started.elapsed();
    let text = summary.finish(elapsed.as_secs_f64(), jobs);
    let written = written.and_then(|()| std::fs::write(args.out.join("summary.md"), &text));
    if let Err(e) = written {
        eprintln!("xtask classify: cannot write the output: {e}");
        return ExitCode::from(1);
    }
    println!("{text}");
    println!(
        "classified {} files in {:.1}s with {jobs} jobs ({} MiB shared admission budget); wrote {}",
        summary.n,
        elapsed.as_secs_f64(),
        MEMORY_BUDGET_BYTES >> 20,
        args.out.display()
    );
    ExitCode::SUCCESS
}

/// The repository revision the reader outcomes belong to (`git rev-parse --short HEAD`).
fn reader_build() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_else(|| "unknown".into())
}

/// Shared admission accounting for source bytes, bounded expansion and reader working data.
/// The reader's allocations are input-dependent; this is conservative admission accounting,
/// not an OS-enforced RSS limit. Never multiply the package limits by unbounded CPU count.
pub(crate) const MEMORY_BUDGET_BYTES: u64 = 1 << 30;
const WORKER_MEMORY_BYTES: u64 = 512 << 20;
const MAX_FILE_BYTES: u64 = 64 << 20;
pub(crate) const MAX_PROFILE_BYTES: u64 = 1 << 20;

fn classify_policy() -> OpenPolicy {
    let mut policy = OpenPolicy::default();
    policy.package.max_package_bytes = MAX_FILE_BYTES;
    policy.package.max_expanded_bytes = 128 << 20;
    policy.package.max_part_bytes = 32 << 20;
    policy
}

pub(crate) struct MemoryBudget {
    bytes: u64,
    jobs: usize,
    used: Mutex<(u64, usize)>,
    available: Condvar,
}

pub(crate) struct MemoryPermit<'a> {
    budget: &'a MemoryBudget,
    bytes: u64,
}
impl MemoryBudget {
    pub(crate) fn new(bytes: u64, jobs: usize) -> Self {
        Self {
            bytes,
            jobs,
            used: Mutex::new((0, 0)),
            available: Condvar::new(),
        }
    }
    pub(crate) fn try_reserve(&self, bytes: u64) -> Option<MemoryPermit<'_>> {
        let mut used = self.used.lock().expect("budget lock");
        if bytes > self.bytes - used.0 || used.1 >= self.jobs {
            return None;
        }
        used.0 += bytes;
        used.1 += 1;
        Some(MemoryPermit {
            budget: self,
            bytes,
        })
    }
    fn reserve(&self, bytes: u64) -> MemoryPermit<'_> {
        assert!(
            bytes <= self.bytes && self.jobs > 0,
            "reservation must fit the budget"
        );
        let mut used = self.used.lock().expect("budget lock");
        while bytes > self.bytes - used.0 || used.1 >= self.jobs {
            used = self.available.wait(used).expect("budget lock");
        }
        used.0 += bytes;
        used.1 += 1;
        MemoryPermit {
            budget: self,
            bytes,
        }
    }
}
impl Drop for MemoryPermit<'_> {
    fn drop(&mut self) {
        let mut used = self.budget.used.lock().expect("budget lock");
        used.0 -= self.bytes;
        used.1 -= 1;
        self.budget.available.notify_all();
    }
}

/// Streaming JSONL for both candidate sets and covered sets. A growing/unbroken record is
/// bounded during the read, before JSON parsing allocates its object graph.
pub(crate) fn read_profiles(
    mut reader: impl BufRead,
    limit: u64,
    mut visit: impl FnMut(Value) -> Result<(), String>,
) -> Result<(), String> {
    let mut line = Vec::new();
    let mut index = 0;
    loop {
        line.clear();
        let n = reader
            .by_ref()
            .take(limit + 1)
            .read_until(b'\n', &mut line)
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Ok(());
        }
        index += 1;
        if n as u64 > limit {
            return Err(format!("line {index}: record_size_limit ({limit} bytes)"));
        }
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        let profile = serde_json::from_slice(&line).map_err(|e| format!("line {index}: {e}"))?;
        visit(profile)?;
    }
}

fn classify_file(path: &Path, name: &str, keep_names: bool, host: bool) -> Value {
    let mut profile = classify_file_inner(path, name, keep_names, host);
    // Stamp before serialisation, including refused/no-host/read-error records.
    static BUILD: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    profile["reader_build"] = json!(BUILD.get_or_init(reader_build));
    profile
}

fn classify_file_inner(path: &Path, name: &str, keep_names: bool, host: bool) -> Value {
    let read = || -> Result<Arc<[u8]>, Value> {
        let file = std::fs::File::open(path)
            .map_err(|e| json!({"file": name, "read_error": e.to_string()}))?;
        let size = file
            .metadata()
            .map_err(|e| json!({"file": name, "read_error": e.to_string()}))?
            .len();
        let refused = || {
            json!({"file": name, "bytes": size, "scan_complete": false,
            "admission_refusal": {"kind": "file_size_limit", "limit": MAX_FILE_BYTES}})
        };
        if size > MAX_FILE_BYTES {
            return Err(refused());
        }
        let mut bytes = Vec::new();
        file.take(MAX_FILE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| json!({"file": name, "read_error": e.to_string()}))?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(refused());
        }
        Ok(bytes.into())
    };
    let bytes = match read() {
        Ok(bytes) => bytes,
        Err(error) => return error,
    };
    match std::panic::catch_unwind(|| classify_shared(bytes.clone(), name, keep_names, host)) {
        Ok(v) => v,
        Err(_) => {
            json!({"file": name, "sha256": sha256_hex(&bytes), "bytes": bytes.len(), "panic": true, "scan_complete": false})
        }
    }
}

/// Distinct shape keys kept per file; the rest are summed under `(other)`.
const SHAPES_PER_FILE: usize = 400;

/// The shapes of one file, most frequent first and cut at [`SHAPES_PER_FILE`].
fn capped_shapes(shapes: &Counts) -> Counts {
    if shapes.len() <= SHAPES_PER_FILE {
        return shapes.clone();
    }
    let mut ranked: Vec<(&String, &u64)> = shapes.iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    let mut kept = Counts::new();
    for (k, v) in ranked.iter().take(SHAPES_PER_FILE) {
        kept.insert((*k).clone(), **v);
    }
    let rest: u64 = ranked.iter().skip(SHAPES_PER_FILE).map(|(_, v)| **v).sum();
    kept.insert("(other)".into(), rest);
    kept
}

fn get_u64(v: &Value, path: &[&str]) -> u64 {
    let mut cur = v;
    for p in path {
        match cur.get(*p) {
            Some(n) => cur = n,
            None => return 0,
        }
    }
    cur.as_u64().unwrap_or(0)
}

/// Adds `object`'s numbers (a map of counts) into `into`, and the files in which a key occurs
/// into `files`.
fn merge_map(object: Option<&Value>, into: &mut Counts, files: &mut Counts) {
    if let Some(map) = object.and_then(Value::as_object) {
        for (k, v) in map {
            let n = v.as_u64().unwrap_or(0);
            if n > 0 {
                bump(into, k.clone(), n);
                bump(files, k.clone(), 1);
            }
        }
    }
}

fn table(out: &mut String, title: &str, head: [&str; 3], rows: Vec<(String, u64, u64)>, limit: usize) {
    if rows.is_empty() {
        return;
    }
    let _ = writeln!(out, "\n### {title}\n\n| {} | {} | {} |\n|---|---:|---:|", head[0], head[1], head[2]);
    for (k, a, b) in rows.into_iter().take(limit) {
        let _ = writeln!(out, "| {k} | {a} | {b} |");
    }
}

fn ranked(occurrences: &Counts, files: &Counts) -> Vec<(String, u64, u64)> {
    let mut rows: Vec<_> = occurrences
        .iter()
        .map(|(k, v)| (k.clone(), *files.get(k).unwrap_or(&0), *v))
        .collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1).then(b.2.cmp(&a.2)).then(a.0.cmp(&b.0)));
    rows
}

/// Only aggregate counts, distinct keys/hashes and a size histogram survive each record.
#[derive(Default)]
struct ProfileSummary {
    n: u64,
    bytes: u64,
    hashes: BTreeSet<String>,
    sizes: BTreeMap<u64, u64>,
    package_open: u64,
    host_open: u64,
    with_formulas: u64,
    formulas: u64,
    sheets: u64,
    vba_modules: u64,
    maps: BTreeMap<&'static str, (Counts, Counts)>,
    features: BTreeMap<String, (u64, u64)>,
    gaps: Counts,
}

// (profile path, table title, column names, row limit).
type MapSpec = (
    &'static [&'static str],
    &'static str,
    [&'static str; 3],
    usize,
);
const SUMMARY_MAPS: &[MapSpec] = &[
    (
        &["functions"],
        "Functions, top 60 by files (files; calls, shared members expanded)",
        ["function", "files", "calls"],
        60,
    ),
    (
        &["function_shapes"],
        "Argument shapes, top 40 by files (files; calls)",
        ["shape", "files", "calls"],
        40,
    ),
    (
        &["formula_features"],
        "Formula features (files; formula cells)",
        ["feature", "files", "cells"],
        30,
    ),
    (
        &["cells", "formula_kinds"],
        "Formula kinds in the file (files; formula cells)",
        ["kind", "files", "cells"],
        10,
    ),
    (
        &["cells", "types"],
        "Cell types (files; cells)",
        ["type", "files", "cells"],
        10,
    ),
    (
        &["names", "functions"],
        "Functions in defined names (files; calls)",
        ["function", "files", "calls"],
        15,
    ),
    (
        &["names", "builtin"],
        "Built-in defined names (files; names)",
        ["name", "files", "names"],
        15,
    ),
    (
        &["conditional_rule_types"],
        "Conditional-format rule types (files; rules)",
        ["type", "files", "rules"],
        15,
    ),
    (
        &["sheet_extensions"],
        "Worksheet extension (ext uri) blocks (files; blocks)",
        ["element", "files", "count"],
        20,
    ),
    (
        &["package", "part_categories"],
        "Package part categories (files; parts)",
        ["category", "files", "parts"],
        40,
    ),
    (
        &["reader", "unloaded_cells"],
        "Reader: cells held as not loaded (files; cells)",
        ["kind", "files", "cells"],
        15,
    ),
    (
        &["reader", "unsupported"],
        "Reader: unsupported notes per sheet (files; notes)",
        ["note", "files", "count"],
        20,
    ),
    (
        &["reader", "repairs"],
        "Reader repairs",
        ["kind", "files", "repairs"],
        20,
    ),
];

impl ProfileSummary {
    fn count(&mut self, table: &'static str, key: impl Into<String>) {
        let (total, files) = self.maps.entry(table).or_default();
        let key = key.into();
        bump(total, key.clone(), 1);
        bump(files, key, 1);
    }

    fn add(&mut self, p: &Value) {
        self.n += 1;
        let bytes = get_u64(p, &["bytes"]);
        self.bytes += bytes;
        *self.sizes.entry(bytes).or_default() += 1;
        if let Some(sha) = p["sha256"].as_str() {
            self.hashes.insert(sha.to_owned());
        }
        self.count(
            "Format by file magic and extension",
            p["format"].as_str().unwrap_or("unreadable"),
        );
        if p["package"]["opened"].as_bool() == Some(true) {
            self.package_open += 1;
        } else if let Some(e) = p["package"]["error"].as_str() {
            self.count(
                "Packages calc-doc refuses",
                e.chars().take(80).collect::<String>(),
            );
        }
        if let Some(kind) = p["admission_refusal"]["kind"].as_str() {
            self.count("Admission refusals", kind);
        }
        match p["reader"]["opened"].as_bool() {
            Some(true) => self.host_open += 1,
            Some(false) => self.count(
                "Host route refusals (open key)",
                p["reader"]["key"].as_str().unwrap_or("?"),
            ),
            None => {}
        }
        if p["scan_complete"].as_bool() != Some(true) {
            bump(&mut self.gaps, "incomplete formula/name scan", 1);
        }
        let formulas = get_u64(p, &["cells", "formula_cells"]);
        self.formulas += formulas;
        self.with_formulas += u64::from(formulas > 0);
        self.sheets += get_u64(p, &["sheets", "count"]);
        self.vba_modules += get_u64(p, &["vba", "project_lines", "module"]);
        for (path, title, _, _) in SUMMARY_MAPS {
            let value = path.iter().try_fold(p, |v, key| v.get(*key));
            let (counts, files) = self.maps.entry(title).or_default();
            merge_map(value, counts, files);
        }
        let app = p["producer"]["application"]
            .as_str()
            .unwrap_or("(no app.xml)");
        let ver = p["producer"]["app_version"].as_str().unwrap_or("");
        self.count(
            "Producer (app.xml Application and AppVersion)",
            if ver.is_empty() {
                app.to_owned()
            } else {
                format!("{app} {ver}")
            },
        );
        let counted = |name: &str, getter: &dyn Fn(&Value) -> u64| (name.to_owned(), getter(p));
        let features: Vec<(String, u64)> = vec![
            counted("tables", &|p| {
                get_u64(p, &["package", "part_categories", "xl/tables"])
            }),
            counted("pivot tables", &|p| {
                get_u64(p, &["package", "part_categories", "xl/pivotTables"])
            }),
            counted("pivot caches", &|p| {
                get_u64(p, &["package", "part_categories", "xl/pivotCache"])
            }),
            counted("charts (parts)", &|p| {
                get_u64(p, &["package", "part_categories", "xl/charts"])
            }),
            counted("drawings (parts)", &|p| {
                get_u64(p, &["package", "part_categories", "xl/drawings"])
            }),
            counted("images (media)", &|p| {
                get_u64(p, &["package", "part_categories", "xl/media"])
            }),
            counted("embedded objects", &|p| {
                get_u64(p, &["package", "part_categories", "xl/embeddings"])
            }),
            counted("comments parts", &|p| {
                get_u64(p, &["package", "part_categories", "xl/comments"])
            }),
            counted("threaded comments", &|p| {
                get_u64(p, &["package", "part_categories", "xl/threadedComments"])
            }),
            counted("VBA project", &|p| u64::from(p.get("vba").is_some())),
            counted("connections", &|p| {
                get_u64(p, &["package", "part_categories", "xl/connections"])
            }),
            counted("query tables", &|p| {
                get_u64(p, &["package", "part_categories", "xl/queryTables"])
            }),
            counted("custom XML items", &|p| {
                get_u64(p, &["package", "part_categories", "customXml"])
            }),
            counted("external links", &|p| {
                get_u64(p, &["package", "part_categories", "xl/externalLinks"])
            }),
            counted("calcChain part", &|p| {
                get_u64(p, &["package", "part_categories", "xl/calcChain"])
            }),
            counted("metadata part (dynamic arrays, rich values)", &|p| {
                get_u64(p, &["package", "part_categories", "xl/metadata"])
            }),
            counted("rich data parts", &|p| {
                get_u64(p, &["package", "part_categories", "xl/richData"])
            }),
            counted("slicers", &|p| {
                get_u64(p, &["package", "part_categories", "xl/slicers"])
            }),
            counted("timelines", &|p| {
                get_u64(p, &["package", "part_categories", "xl/timelines"])
            }),
            counted("data model", &|p| {
                get_u64(p, &["package", "part_categories", "xl/model"])
            }),
            counted("macro sheets", &|p| {
                get_u64(p, &["sheets", "kinds", "macrosheet"])
            }),
            counted("dialog sheets", &|p| {
                get_u64(p, &["sheets", "kinds", "dialogsheet"])
            }),
            counted("chart sheets", &|p| {
                get_u64(p, &["sheets", "kinds", "chartsheet"])
            }),
            counted("hidden sheets", &|p| {
                get_u64(p, &["sheets", "kinds", "state:hidden"])
            }),
            counted("very hidden sheets", &|p| {
                get_u64(p, &["sheets", "kinds", "state:veryHidden"])
            }),
            counted("date system 1904", &|p| {
                u64::from(p["workbook"]["date1904"].as_bool() == Some(true))
            }),
            counted("iterative calculation", &|p| {
                u64::from(p["workbook"]["iterate"].as_bool() == Some(true))
            }),
            counted("manual calculation", &|p| {
                u64::from(p["workbook"]["calc_mode"].as_str() == Some("manual"))
            }),
            counted("workbook protection", &|p| {
                u64::from(p["workbook"]["workbook_protection"].as_bool() == Some(true))
            }),
            counted("sheet protection", &|p| {
                get_u64(p, &["sheet_elements", "sheetProtection"])
            }),
            counted("data validation", &|p| {
                get_u64(p, &["sheet_elements", "dataValidation"])
            }),
            counted("conditional formatting blocks", &|p| {
                get_u64(p, &["sheet_elements", "conditionalFormatting"])
            }),
            counted("hyperlinks", &|p| {
                get_u64(p, &["sheet_elements", "hyperlink"])
            }),
            counted("merged cells", &|p| {
                get_u64(p, &["sheet_elements", "mergeCell"])
            }),
            counted("autofilter", &|p| {
                get_u64(p, &["sheet_elements", "autoFilter"])
            }),
            counted("legacy drawing (comments/forms)", &|p| {
                get_u64(p, &["sheet_elements", "legacyDrawing"])
            }),
            counted("OLE objects in sheets", &|p| {
                get_u64(p, &["sheet_elements", "oleObject"])
            }),
            counted("form controls in sheets", &|p| {
                get_u64(p, &["sheet_elements", "control"])
            }),
            counted("defined names", &|p| get_u64(p, &["names", "total"])),
            counted("hidden defined names", &|p| {
                get_u64(p, &["names", "hidden"])
            }),
            counted("sheet-scoped names", &|p| {
                get_u64(p, &["names", "sheet_scope"])
            }),
            counted("names that call functions", &|p| {
                get_u64(p, &["names", "with_functions"])
            }),
            counted("names with #REF!", &|p| {
                get_u64(p, &["names", "ref_errors"])
            }),
            counted("formula cells", &|p| {
                get_u64(p, &["cells", "formula_cells"])
            }),
            counted("formula cells with a cached value", &|p| {
                get_u64(p, &["cells", "formula_cached_values"])
            }),
            counted("dynamic-array cells (cm attribute)", &|p| {
                get_u64(p, &["cells", "dynamic_array_cells"])
            }),
        ];
        for (name, count) in features {
            if count > 0 {
                let (files, total) = self.features.entry(name).or_default();
                *files += 1;
                *total += count;
            }
        }
        if p["reader"]["opened"].as_bool() == Some(true) {
            for (label, scan, read) in [
                (
                    "data validations: scan more than reader",
                    &["sheet_elements", "dataValidation"][..],
                    &["reader", "validations"][..],
                ),
                (
                    "conditional-format blocks: scan more than reader",
                    &["sheet_elements", "conditionalFormatting"][..],
                    &["reader", "conditional_formats"][..],
                ),
                (
                    "merged cells: scan more than reader",
                    &["sheet_elements", "mergeCell"][..],
                    &["reader", "merged_areas"][..],
                ),
                (
                    "formula cells in the file beyond the loaded formula records (array members, data tables, unloaded)",
                    &["cells", "formula_cells"][..],
                    &["reader", "formula_cells_loaded"][..],
                ),
            ] {
                if get_u64(p, scan) > get_u64(p, read) {
                    bump(&mut self.gaps, label, 1);
                }
            }
            if get_u64(p, &["reader", "layout_warnings"]) > 0 {
                bump(&mut self.gaps, "layout warnings", 1);
            }
        }
    }

    fn finish(&self, seconds: f64, jobs: usize) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "{} files, {} unique by SHA-256, {} bytes; classified in {seconds:.1} s with {jobs} jobs ({:.0} files/s).",
            self.n,
            self.hashes.len(),
            self.bytes,
            self.n as f64 / seconds.max(0.001)
        );
        let pct = |q: u64| {
            let position = self.n.saturating_sub(1) * q / 100;
            let mut seen = 0;
            self.sizes
                .iter()
                .find_map(|(size, count)| {
                    seen += count;
                    (seen > position).then_some(*size)
                })
                .unwrap_or(0)
        };
        let _ = writeln!(
            out,
            "\nSize (bytes): median {}, p90 {}, p99 {}, max {}.\nPackages opened by calc-doc: {}; opened by the host route: {}. Files with formulas: {}; formula cells in all files: {}; sheets: {}.",
            pct(50),
            pct(90),
            pct(99),
            self.sizes.last_key_value().map_or(0, |(size, _)| *size),
            self.package_open,
            self.host_open,
            self.with_formulas,
            self.formulas,
            self.sheets
        );
        for (title, head, limit) in [
            (
                "Format by file magic and extension",
                ["format", "files", "-"],
                20,
            ),
            ("Packages calc-doc refuses", ["error", "files", "-"], 20),
            ("Host route refusals (open key)", ["key", "files", "-"], 20),
            ("Admission refusals", ["kind", "files", "-"], 20),
            (
                "Producer (app.xml Application and AppVersion)",
                ["producer", "files", "-"],
                15,
            ),
        ] {
            if let Some((counts, files)) = self.maps.get(title) {
                table(&mut out, title, head, ranked(counts, files), limit);
            }
        }
        table(
            &mut out,
            "Structures (files that have it; total count)",
            ["structure", "files", "total"],
            self.features
                .iter()
                .map(|(key, (files, total))| (key.clone(), *files, *total))
                .collect(),
            usize::MAX,
        );
        for (_, title, head, limit) in SUMMARY_MAPS {
            if let Some((counts, files)) = self.maps.get(title) {
                table(&mut out, title, *head, ranked(counts, files), *limit);
            }
        }
        let funcs = self
            .maps
            .get(SUMMARY_MAPS[0].1)
            .map_or(0, |(counts, _)| counts.len());
        let shapes = self.maps.get(SUMMARY_MAPS[1].1);
        let sensitive = shapes.map_or(0, |(counts, _)| {
            counts.keys().filter(|key| key.ends_with('!')).count()
        });
        let _ = writeln!(
            out,
            "\nDistinct functions seen: {funcs}.\nDistinct (function, argument shape) pairs seen: {} ({sensitive} position- or reference-sensitive).\nVBA modules by project-stream heuristic: {}.",
            shapes.map_or(0, |(counts, _)| counts.len()),
            self.vba_modules
        );
        table(
            &mut out,
            "Reader gaps seen by comparing the scan with the reader (files)",
            ["gap", "files", "-"],
            self.gaps
                .iter()
                .map(|(key, count)| (key.clone(), *count, 0))
                .collect(),
            20,
        );
        out
    }
}

#[cfg(test)]
fn summarize(profiles: &[Value], seconds: f64, jobs: usize) -> String {
    let mut summary = ProfileSummary::default();
    for p in profiles {
        summary.add(p);
    }
    summary.finish(seconds, jobs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workers_share_one_memory_and_concurrency_budget() {
        let budget = MemoryBudget::new(100, 2);
        let a = budget.try_reserve(60).unwrap();
        assert!(budget.try_reserve(41).is_none());
        let b = budget.try_reserve(40).unwrap();
        assert!(
            budget.try_reserve(0).is_none(),
            "worker count is bounded too"
        );
        drop(a);
        let c = budget.try_reserve(60).unwrap();
        drop((b, c));
        assert!(budget.try_reserve(100).is_some());
    }

    #[test]
    fn jsonl_is_consumed_one_bounded_record_at_a_time() {
        let mut visited = 0;
        let data = std::io::Cursor::new(b"{\"a\":1}\ninvalid\n");
        let error = read_profiles(data, 32, |_| {
            visited += 1;
            Ok(())
        })
        .unwrap_err();
        assert_eq!(
            visited, 1,
            "a later error must not delay earlier processing"
        );
        assert!(error.contains("line 2"));
        let data = std::io::Cursor::new(vec![b' '; 100]);
        assert!(
            read_profiles(data, 8, |_| panic!("oversized record delivered"))
                .unwrap_err()
                .contains("record_size_limit")
        );
    }

    #[test]
    fn incremental_summary_counts_files_and_occurrences_separately() {
        let mut summary = ProfileSummary::default();
        for _ in 0..3 {
            summary.add(&json!({"sha256": "same", "bytes": 10, "functions": {"SUM": 2}}));
        }
        let text = summary.finish(1.0, 2);
        assert!(
            text.contains("3 files, 1 unique by SHA-256, 30 bytes"),
            "{text}"
        );
        assert!(text.contains("| SUM | 3 | 6 |"), "{text}");
        assert!(text.contains("median 10, p90 10, p99 10, max 10"), "{text}");
    }

    #[test]
    fn oversized_source_is_refused_before_reading_its_payload() {
        let path = std::env::temp_dir().join(format!("dc-mndh-large-{}.xlsx", std::process::id()));
        let file = std::fs::OpenOptions::new().write(true).create_new(true).open(&path).unwrap();
        file.set_len(OpenPolicy::default().package.max_package_bytes + 1)
            .unwrap();
        drop(file);
        let p = classify_file(&path, "large.xlsx", false, false);
        std::fs::remove_file(path).unwrap();
        assert_eq!(p["admission_refusal"]["kind"], "file_size_limit");
        assert!(
            p.get("sha256").is_none(),
            "must not read/hash a refused payload"
        );
    }

    #[test]
    fn malformed_formula_scans_have_a_typed_refusal_and_bounded_work() {
        for text in ["SUM(".repeat(30_000), "[".repeat(100_000)] {
            assert_eq!(
                analyze_formula(&text).unwrap_err(),
                ScanRefusal::MalformedFormula
            );
        }
        assert_eq!(
            analyze_formula(&"x".repeat(MAX_FORMULA_BYTES + 1)).unwrap_err(),
            ScanRefusal::WorkBudget
        );
        let nested = format!("{}1{}", "SUM(".repeat(10_000), ")".repeat(10_000));
        assert_eq!(
            analyze_formula(&nested).unwrap_err(),
            ScanRefusal::WorkBudget
        );
        let xml = format!(
            "<worksheet><sheetData><row><c><f>{}</f></c></row></sheetData></worksheet>",
            "SUM(".repeat(30_000)
        );
        assert!(
            scan_sheet(&XmlPart::from_bytes("sheet.xml", xml.into_bytes()))
                .unwrap_err()
                .to_string()
                .contains("malformed_formula")
        );
    }

    #[test]
    fn every_serialized_record_identifies_the_reader_build() {
        let path = std::env::temp_dir().join(format!("dc-mndh-build-{}.xlsx", std::process::id()));
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&path).unwrap();
        file.write_all(&calc_doc::blank_package()).unwrap();
        drop(file);
        for host in [false, true] {
            let p = classify_file(&path, "blank.xlsx", false, host);
            assert!(p["reader_build"].as_str().is_some_and(|b| !b.is_empty()));
        }
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn classification_reports_the_reader_repair_kinds_and_counts() {
        for (bytes, kind, count) in [
            (include_bytes!("../../../crates/calc-doc/tests/fixtures/reader_tolerance/dangling-rid.xlsx").as_slice(), "sheet_relationship_missing", 1),
            (include_bytes!("../../../crates/calc-doc/tests/fixtures/reader_tolerance/array-inner-formula.xlsx").as_slice(), "formula_inside_array", 1),
            (include_bytes!("../../../crates/calc-doc/tests/fixtures/reader_tolerance/duplicate-sheetid-triple.xlsx").as_slice(), "duplicate_sheet_id", 2),
        ] {
            let p = classify_bytes(bytes, "repaired.xlsx", false, true);
            assert_eq!(p["reader"]["repair_count"], count);
            assert_eq!(p["reader"]["repairs"][kind], count);
        }
        let p = classify_bytes(&calc_doc::blank_package(), "blank.xlsx", false, true);
        assert_eq!(p["reader"]["repair_count"], 0);
    }

    #[test]
    fn functions_and_features() {
        let s = analyze_formula(
            r#"LET(x,_xlfn.XLOOKUP("a(",Table1[Col],B1#),@A1+SUM(Sheet1:Sheet3!A1)+'[1]Ext'!A1+INDIRECT("A"&1)+{1,2})"#,
        ).unwrap();
        assert_eq!(s.functions.get("LET"), Some(&1));
        assert_eq!(s.functions.get("XLOOKUP"), Some(&1));
        assert_eq!(s.functions.get("SUM"), Some(&1));
        assert_eq!(s.functions.get("INDIRECT"), Some(&1));
        for f in [
            "let",
            "structured_reference",
            "spill_reference",
            "three_d_reference",
            "external_reference",
            "volatile",
            "array_constant",
            "implicit_intersection_operator",
        ] {
            assert_eq!(s.flags.get(f), Some(&1), "{f}");
        }
        assert!(!s.flags.contains_key("lambda"));
        assert_eq!(s.depth, 2);
    }

    #[test]
    fn strings_and_error_literals_are_not_features() {
        let s = analyze_formula(r#"IF(A1="SUM(B1)",#N/A,"it's")"#).unwrap();
        assert_eq!(s.functions.len(), 1);
        assert!(s.flags.is_empty(), "{:?}", s.flags);
    }

    #[test]
    fn quoted_three_d_and_prefixes() {
        let s = analyze_formula("_xlfn._xlws.SORT('Jan 1:Dec 31'!A1:A9)").unwrap();
        assert_eq!(s.functions.get("SORT"), Some(&1));
        assert_eq!(s.flags.get("three_d_reference"), Some(&1));
    }

    #[test]
    fn calls_record_their_argument_shapes_and_shared_members_count_them() {
        let s = analyze_formula(r#"IF(A1>0,ROUND(B2,2),"no")+SUM(C1:C9)+ROW()"#).unwrap();
        assert_eq!(s.shapes.get("IF(x,f,S)"), Some(&1));
        assert_eq!(s.shapes.get("ROUND(r,n)"), Some(&1));
        assert_eq!(s.shapes.get("SUM(R)"), Some(&1));
        assert_eq!(s.shapes.get("ROW()!"), Some(&1));
        let xml = br#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1"><f t="shared" ref="A1:A3" si="0">SUM(B1,2)</f><v>1</v></c></row><row r="2"><c r="A2"><f t="shared" si="0"/><v>1</v></c></row><row r="3"><c r="A3"><f t="shared" si="0"/><v>1</v></c></row></sheetData></worksheet>"#;
        let scan = scan_sheet(&XmlPart::from_bytes("sheet1.xml", xml.to_vec())).unwrap();
        assert_eq!(scan.stats.shapes.get("SUM(r,n)"), Some(&3));
    }

    #[test]
    fn shared_formula_members_count_the_master_functions() {
        let xml = br#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><dimension ref="A1:A3"/><sheetData><row r="1"><c r="A1"><f t="shared" ref="A1:A3" si="0">NOW()+SUM(B1)</f><v>1</v></c></row><row r="2"><c r="A2"><f t="shared" si="0"/><v>1</v></c></row><row r="3"><c r="A3" t="e"><f t="shared" si="0"/><v>#N/A</v></c></row></sheetData></worksheet>"#;
        let scan = scan_sheet(&XmlPart::from_bytes("sheet1.xml", xml.to_vec())).unwrap();
        assert_eq!(scan.formula_cells, 3);
        assert_eq!(scan.stats.functions.get("SUM"), Some(&3));
        assert_eq!(scan.stats.flags.get("volatile"), Some(&3));
        assert_eq!(scan.formula_cached_errors, 1);
        assert_eq!(scan.dimension.as_deref(), Some("A1:A3"));
        assert_eq!(
            count_cached_formula_values(&XmlPart::from_bytes("s", xml.to_vec())),
            3
        );
    }

    #[test]
    fn unreadable_bytes_are_profiled_not_fatal() {
        let p = classify_bytes(&[0xD0, 0xCF, 0x11, 0xE0, 0, 0], "old.xls", false, true);
        assert_eq!(p["format"], "ole-compound (xls/other)");
        assert_eq!(p["package"]["opened"], false);
        let summary = summarize(&[p], 1.0, 1);
        assert!(summary.contains("1 files"), "{summary}");
    }

    #[test]
    fn a_blank_workbook_is_profiled_through_the_reader() {
        let bytes = calc_doc::blank_package();
        let p = classify_bytes(&bytes, "blank.xlsx", false, true);
        assert_eq!(p["format"], "xlsx");
        assert_eq!(p["package"]["opened"], true);
        assert_eq!(p["sheets"]["count"], 1);
        assert_eq!(p["cells"]["formula_cells"], 0);
        assert_eq!(p["reader"]["opened"], true);
        assert_eq!(p["reader"]["sheets"], 1);
    }

    #[test]
    fn args() {
        let a = parse_args(&[
            "a".into(),
            "--recursive".into(),
            "--jobs".into(),
            "3".into(),
        ])
        .unwrap();
        assert!(a.recursive && a.jobs == 3 && a.dirs.len() == 1);
        assert!(parse_args(&[]).is_err());
        assert!(parse_args(&["a".into(), "--nope".into()]).is_err());
    }
}
