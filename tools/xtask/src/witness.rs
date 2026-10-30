//! `cargo xtask witness judge [--func F] [--bank pinned|full]` and `cargo xtask witness pin [--func
//! F] [--rows N]` (bead dc-ymjf.2): calc-func judged bit for bit, function by function, on Excel's
//! readings. `cargo xtask witness read --func F[,F...]` (bead dc-ymjf.3) takes new readings of a
//! pinned set's rows on this host's Excel.
//!
//! - `--bank pinned` (the default) reads the pinned sets `crates/calc-func/tests/witnesses/*.json`,
//!   which calc-func's portable test `witnesses` also judges.
//! - `--bank full` reads every bank of the evidence repository (`<legacy root>/OxFunc-Evidence`,
//!   straight from its gzip blobs, no restore): the oracle cache `cache/oracle*/build-*/*.jsonl`
//!   and every WitnessSet file (`{"function", "witnesses": [{"id", "args", "expected_bits"}]}`) of
//!   `manifest.tsv` and `w109-manifest.tsv` named `answers*.json` (the runners' answer files;
//!   predictions, synthetic answers and research files of the same shape hold no Excel reading).
//!
//! Each reading keeps the environment its capture recorded: Excel build, platform, bitness and CPU
//! (`?` where nothing was recorded). A reading holds across builds unless another reading of the
//! same function, arguments and result cell disagrees: then both are kept and written to
//! `crates/calc-func/witness-differences.tsv`, as a version or platform difference when their
//! environments differ and as a same-environment disagreement when they do not. Such a row is
//! judged against the reading of the latest build, and the difference asks for a targeted recheck.
//!
//! Every row runs through `calc_func::invoke` under the application's numeric mode (Fast), in a
//! workbook environment with the 1900 date system (the one OxFunc's captures used), and is
//! classed with calc-verdict's [`Severity`] (OxFunc's ODR-FN-005). A refusal or a function the
//! catalogue does not hold is a structural miss: refused counts as failed. The command rewrites
//! the rows of the bank it judged in `crates/calc-func/witness-status.tsv` (function, bank, Excel
//! builds, rows, matched, worst ULP, class, calc-func commit). `--bank full` also compares each
//! function with OxFunc's last ledger (`function-lane/ledgers/EXCEL_PARITY_LEDGER.csv` of the
//! evidence repository) and writes `target/witness/full-vs-oxfunc.tsv`.
//!
//! `pin` judges the full banks as `judge --bank full` does, writes what it writes, and then the
//! pinned sets: per function, the first rows of each bank
//! (the generators write their edge rows first) and an evenly spaced sample of the rest, at most
//! `--rows` (default 120) in all; a row the calculation does not match carries the calculated
//! result (`calc_func`), so the portable test fails on a regression and on a gain alike. Rows on
//! which calc-func panics, calculates without end or takes over 50 ms are left out and listed,
//! since the wasm test cannot recover from the first two and runs unoptimised. A row ID that holds a marker of the licence ledger's scan is replaced by its index.
//!
//! `read` writes each pinned row's arguments into an Excel started under calc-oracle-excel's
//! isolation (its bulk witness mode) and reads the results through `Range.Value2`; it retries a
//! run on RPC_E_CALL_REJECTED and stops only the Excel it started. The readings go, as a
//! WitnessSet with their environment, to `readings/excel-<build>/<F>.json` of the evidence
//! repository, which `--bank full` reads as one more bank: a reading that disagrees with a banked
//! one then shows as a difference. This is the targeted recheck a difference asks for, and the way
//! new readings are taken; nothing re-reads the banks wholesale on each new build.

use crate::cargo::Workspace;
use crate::legacy::legacy_root;
use calc_func::locale::{CalcEnvironment, DateSystem};
use calc_func::value::{CalcValue, NumericMode};
use calc_func::{
    EvalServices, FunctionCallScratch, FunctionFault, Notation, PreparedArguments, catalogue,
    invoke,
};
use calc_verdict::{
    ReadingSource, Severity, WitnessRow, WitnessSet, decode_argument, decode_result,
    encode_result, select_result,
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

pub const USAGE: &str = "cargo xtask witness judge [--func <F>] [--bank pinned|full] | cargo xtask witness pin [--func <F>] [--rows <N>] | cargo xtask witness read --func <F>[,<F>...] [--timeout <s>]";

const CHECKOUT: &str = "OxFunc-Evidence";
const MANIFESTS: [&str; 2] = ["manifest.tsv", "w109-manifest.tsv"];
const PINNED_DIR: &str = "crates/calc-func/tests/witnesses";
const STATUS: &str = "crates/calc-func/witness-status.tsv";
const DIFFERENCES: &str = "crates/calc-func/witness-differences.tsv";
const LEDGER: &str = "function-lane/ledgers/EXCEL_PARITY_LEDGER.csv";
const EDGE_ROWS: usize = 4;
/// New readings taken by `cargo xtask witness read`, in the evidence repository.
const READINGS: &str = "readings";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Bank {
    Pinned,
    Full,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Verb {
    Judge,
    Pin,
    Read,
}

struct Request {
    verb: Verb,
    function: Option<String>,
    bank: Bank,
    rows: usize,
    timeout: u64,
}

fn parse_args(raw: &[String]) -> Result<Request, String> {
    let (verb, rest) = raw.split_first().ok_or("a subcommand is required")?;
    let verb = match verb.as_str() {
        "judge" => Verb::Judge,
        "pin" => Verb::Pin,
        "read" => Verb::Read,
        other => return Err(format!("unknown subcommand {other}")),
    };
    let mut request = Request {
        verb,
        function: None,
        bank: if verb == Verb::Pin { Bank::Full } else { Bank::Pinned },
        rows: 120,
        timeout: 600,
    };
    let mut it = rest.iter();
    while let Some(arg) = it.next() {
        let mut value = || it.next().cloned().ok_or(format!("{arg} needs a value"));
        match arg.as_str() {
            "--func" => request.function = Some(value()?.to_ascii_uppercase()),
            "--bank" if verb == Verb::Judge => {
                request.bank = match value()?.as_str() {
                    "pinned" => Bank::Pinned,
                    "full" => Bank::Full,
                    other => return Err(format!("--bank is pinned or full, not {other}")),
                }
            }
            "--rows" if verb == Verb::Pin => {
                request.rows = value()?
                    .parse()
                    .map_err(|_| "--rows needs a number".to_owned())?
            }
            "--timeout" if verb == Verb::Read => {
                request.timeout = value()?
                    .parse()
                    .map_err(|_| "--timeout needs a number of seconds".to_owned())?
            }
            other => return Err(format!("unknown argument {other}")),
        }
    }
    if verb == Verb::Read && request.function.is_none() {
        return Err("read needs --func".to_owned());
    }
    Ok(request)
}

/// One reading: Excel's result of `function` on `args`, from bank file `source`.
struct Reading {
    function: String,
    id: String,
    args: Vec<Value>,
    index: Option<(usize, usize)>,
    expected: String,
    source: usize,
}

impl Reading {
    fn key(&self) -> String {
        let index = self.index.map_or(String::new(), |(r, c)| format!("@{r},{c}"));
        format!("{}|{}{index}", self.function, Value::Array(self.args.clone()))
    }
}

/// A bank file and the environment its capture recorded.
struct Source {
    path: String,
    env: ReadingSource,
}

struct Banks {
    sources: Vec<Source>,
    readings: Vec<Reading>,
    files_read: usize,
    import_notes: Vec<String>,
    unread_rows: BTreeMap<String, usize>,
}

// ---------------------------------------------------------------------------------------------
// Reading the evidence repository's gzip blobs

fn blob_bytes(checkout: &Path, rel: &str) -> Result<Vec<u8>, String> {
    let single = checkout.join(format!("{rel}.gz"));
    if single.is_file() {
        return fs::read(&single).map_err(|e| format!("{}: {e}", single.display()));
    }
    let mut bytes = Vec::new();
    for part in 0.. {
        let path = checkout.join(format!("{rel}.gz.part{part:03}"));
        match fs::read(&path) {
            Ok(b) => bytes.extend(b),
            Err(_) if part > 0 => break,
            Err(e) => return Err(format!("{}: {e}", single.display())),
        }
    }
    Ok(bytes)
}

/// The first 64 KiB of a blob, enough to inflate the first 4 KiB of its file for a look at what
/// it is (a gzip stream read only in part; [`gunzip`] with a limit stops before its end).
fn blob_head(checkout: &Path, rel: &str) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let single = checkout.join(format!("{rel}.gz"));
    let path = if single.is_file() { single } else { checkout.join(format!("{rel}.gz.part000")) };
    let mut head = Vec::new();
    fs::File::open(&path)
        .and_then(|f| f.take(64 * 1024).read_to_end(&mut head))
        .map_err(|e| format!("{}: {e}", path.display()))?;
    // gunzip drops an 8-byte trailer; a head cut short has none, so pad it.
    head.extend([0u8; 8]);
    Ok(head)
}

/// The deflate stream of a gzip member (RFC 1952 header, with its optional fields skipped).
fn deflate_body(gz: &[u8]) -> Result<&[u8], String> {
    if gz.len() < 18 || gz[0] != 0x1f || gz[1] != 0x8b || gz[2] != 8 {
        return Err("not a gzip stream".to_owned());
    }
    let flags = gz[3];
    let mut at = 10;
    if flags & 4 != 0 {
        let extra = usize::from(gz[at]) | usize::from(gz[at + 1]) << 8;
        at += 2 + extra;
    }
    for bit in [8u8, 16] {
        if flags & bit != 0 {
            while gz.get(at).is_some_and(|b| *b != 0) {
                at += 1;
            }
            at += 1;
        }
    }
    if flags & 2 != 0 {
        at += 2;
    }
    gz.get(at..gz.len() - 8).ok_or_else(|| "a truncated gzip stream".to_owned())
}

fn gunzip(gz: &[u8], limit: Option<usize>) -> Result<Vec<u8>, String> {
    let body = deflate_body(gz)?;
    match limit {
        None => miniz_oxide::inflate::decompress_to_vec(body).map_err(|e| format!("{e:?}")),
        Some(limit) => match miniz_oxide::inflate::decompress_to_vec_with_limit(body, limit) {
            Ok(out) => Ok(out),
            // A limit reached, or a head cut short: what was inflated is the look asked for.
            Err(e) if !e.output.is_empty() => Ok(e.output),
            Err(e) => Err(format!("{e:?}")),
        },
    }
}

fn manifest_paths(checkout: &Path, name: &str) -> Result<Vec<String>, String> {
    let path = checkout.join(name);
    let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(text
        .lines()
        .skip(1)
        .filter_map(|line| line.split('\t').next())
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .collect())
}

fn text_of(value: &Value) -> Option<String> {
    match value {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// The environment a WitnessSet's `capture_provenance` or an oracle record's `environment` names.
/// Every OxFunc capture went through Excel's COM interface on Windows, so the platform is Windows
/// whether or not the capture wrote it down.
fn environment(id: String, source: String, env: Option<&Value>, fallback: Option<&Value>) -> ReadingSource {
    let field = |name: &str| {
        env.and_then(|e| e.get(name))
            .and_then(text_of)
            .or_else(|| fallback.and_then(|e| e.get(name)).and_then(text_of))
    };
    ReadingSource {
        id,
        source,
        excel_build: field("excel_build"),
        platform: field("platform").or_else(|| Some("Windows".to_owned())),
        bitness: field("excel_bitness"),
        cpu: field("cpu_id"),
    }
}

/// An oracle-cache argument (`{"kind": "number", "bits_hex"}`, `{"kind": "matrix", "rows", "cols",
/// "bits_hex_row_major"}`, ...) in the witness encoding, or `None` for a kind it has no form for.
fn oracle_argument(node: &Value) -> Option<Value> {
    match node.get("kind")?.as_str()? {
        "number" => Some(Value::String(node.get("bits_hex")?.as_str()?.to_owned())),
        "logical" | "boolean" | "bool" => Some(Value::Bool(node.get("value")?.as_bool()?)),
        "text" | "string" => Some(Value::String(node.get("value")?.as_str()?.to_owned())),
        "matrix" => {
            let cols = node.get("cols")?.as_u64()? as usize;
            let flat: Vec<Value> = node
                .get("bits_hex_row_major")?
                .as_array()?
                .iter()
                .flat_map(|row| row.as_array().cloned().unwrap_or_else(|| vec![row.clone()]))
                .collect();
            if cols == 0 || flat.len() % cols != 0 {
                return None;
            }
            Some(Value::Array(
                flat.chunks(cols).map(|row| Value::Array(row.to_vec())).collect(),
            ))
        }
        _ => None,
    }
}

fn oracle_outcome(node: &Value) -> Option<String> {
    match node.get("kind")?.as_str()? {
        "number" => Some(node.get("bits_hex")?.as_str()?.to_owned()),
        "error" => {
            let code = node
                .get("code")
                .or_else(|| node.get("value"))
                .and_then(Value::as_str)?;
            let variant = match code.trim_start_matches('#').trim_end_matches(['!', '?']) {
                "NUM" | "Num" => "Num",
                "DIV/0" | "Div0" => "Div0",
                "VALUE" | "Value" => "Value",
                "N/A" | "NA" => "NA",
                "REF" | "Ref" => "Ref",
                "NAME" | "Name" => "Name",
                "NULL" | "Null" => "Null",
                _ => return None,
            };
            Some(format!("error:{variant}"))
        }
        "logical" | "boolean" => Some(
            if node.get("value")?.as_bool()? { "logical:True" } else { "logical:False" }
                .to_owned(),
        ),
        "text" | "string" => Some(format!("text:{}", node.get("value")?.as_str()?)),
        _ => None,
    }
}

/// Some historical banks formatted an already-prefixed numeric bit string a second time.
/// Repair only that exact representation; the sixteen recorded bits remain unchanged.
fn normalize_bank_hex(node: &mut Value) {
    match node {
        Value::String(text) => {
            if let Some(hex) = text.strip_prefix("0x0x") {
                if hex.len() == 16 && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
                    *text = format!("0x{hex}");
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(normalize_bank_hex),
        _ => {}
    }
}

/// PowerShell's older UTF-8 output includes a BOM. It is an encoding marker, not JSON data.
fn bank_json(bytes: &[u8]) -> Result<Value, String> {
    serde_json::from_slice(bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes))
        .map_err(|e| e.to_string())
}

/// Graph banks are lowered copies of worksheet readings, not calls of their named function.
/// Omit one only when every id and answer is present in its worksheet companion. The original
/// worksheet calls are still loaded and judged, including every failure.
fn derived_graph_companion(checkout: &Path, rel: &str, doc: &Value) -> Option<(String, usize)> {
    if doc.get("capture_provenance").is_some() {
        return None;
    }
    let (prefix, suffix) = rel.rsplit_once("-graph")?;
    if !(suffix == ".json" || suffix.starts_with('-') && suffix.ends_with(".json")) {
        return None;
    }
    let companion = format!("{prefix}-excel.json");
    let bytes = gunzip(&blob_bytes(checkout, &companion).ok()?, None).ok()?;
    let original = bank_json(&bytes).ok()?;
    if original.get("function") != doc.get("function") || doc.get("function").is_none() {
        return None;
    }
    graph_rows_linked(doc, &original).then(|| (companion, doc["witnesses"].as_array().unwrap().len()))
}

fn graph_rows_linked(derived: &Value, original: &Value) -> bool {
    let (Some(rows), Some(calls)) = (derived["witnesses"].as_array(), original["witnesses"].as_array()) else {
        return false;
    };
    let readings: BTreeMap<&str, &Value> = calls.iter().filter_map(|r| {
        Some((r["id"].as_str()?, r.get("expected_bits")?))
    }).collect();
    !rows.is_empty() && rows.iter().all(|r| {
        r["id"].as_str().and_then(|id| readings.get(id)).is_some_and(|expected| {
            r.get("expected_bits") == Some(*expected)
        })
    })
}

/// Bulk answer files sometimes omit `result_index`, but their batch's probe records it.
/// Recover selection by id AND exact arguments, never by guessing a cell from its answer.
fn batch_path(rel: &str) -> Option<String> {
    let (dir, name) = rel.rsplit_once('/')?;
    let tail = name.strip_prefix("answers-")?;
    Some(format!("{dir}/batch-{tail}"))
}

fn batch_selections(checkout: &Path, rel: &str) -> Vec<Value> {
    let Some(batch) = batch_path(rel) else { return Vec::new() };
    let parsed = blob_bytes(checkout, &batch).and_then(|b| gunzip(&b, None))
        .and_then(|b| bank_json(&b));
    let Ok(doc) = parsed else { return Vec::new() };
    doc["probes"].as_array().into_iter().flatten().map(|p| {
        p.get("probe").unwrap_or(p).clone()
    }).filter(|p| p.get("result_index").is_some()).collect()
}

fn recovered_selection(row: &Value, probes: &[Value]) -> Option<(usize, usize)> {
    let id = row["id"].as_str()?;
    let args = row.get("args")?;
    let mut selected = None;
    for probe in probes.iter().filter(|p| p["id"].as_str() == Some(id) && p.get("args") == Some(args)) {
        let index = calc_verdict::result_index(probe.as_object()?)?;
        if selected.is_some_and(|previous| previous != index) {
            return None;
        }
        selected = Some(index);
    }
    selected
}

fn load_full(checkout: &Path, only: Option<&str>) -> Result<(Banks, Vec<String>), String> {
    let mut banks = Banks {
        sources: Vec::new(),
        readings: Vec::new(),
        files_read: 0,
        import_notes: Vec::new(),
        unread_rows: BTreeMap::new(),
    };
    let mut unread = Vec::new();
    let mut oracle_env: BTreeMap<String, Value> = BTreeMap::new();
    let mut paths = Vec::new();
    for manifest in MANIFESTS {
        paths.extend(manifest_paths(checkout, manifest)?);
    }
    for rel in &paths {
        if rel.starts_with("cache/oracle") && rel.ends_with("/env.json") {
            let bytes = gunzip(&blob_bytes(checkout, rel)?, None)?;
            if let Ok(env) = bank_json(&bytes) {
                let dir = rel.trim_end_matches("env.json").to_owned();
                oracle_env.insert(dir, env);
            }
        }
    }
    for rel in &paths {
        let name = rel.rsplit('/').next().unwrap_or(rel);
        let oracle = rel.starts_with("cache/oracle") && rel.contains("/build-") && name.ends_with(".jsonl");
        if oracle {
            let function = name.trim_end_matches(".jsonl");
            if only.is_some_and(|f| f != function) {
                continue;
            }
            let bytes = gunzip(&blob_bytes(checkout, rel)?, None)?;
            banks.files_read += 1;
            let fallback = oracle_env
                .iter()
                .filter(|(dir, _)| rel.starts_with(dir.as_str()))
                .max_by_key(|(dir, _)| dir.len())
                .map(|(_, env)| env.clone());
            let mut source_of: BTreeMap<String, usize> = BTreeMap::new();
            for (line_no, line) in bytes.split(|b| *b == b'\n').enumerate() {
                if line.iter().all(u8::is_ascii_whitespace) {
                    continue;
                }
                let Ok(record) = bank_json(line) else {
                    unread.push(format!("{rel}:{}: not JSON", line_no + 1));
                    *banks.unread_rows.entry(function.to_ascii_uppercase()).or_default() += 1;
                    continue;
                };
                let call = record["args"].as_array().and_then(|a| a.first()).and_then(Value::as_array);
                let args: Option<Vec<Value>> = call.map(|c| c.iter().map(oracle_argument).collect()).flatten();
                let (Some(args), Some(expected)) = (args, oracle_outcome(&record["outcome"])) else {
                    unread.push(format!("{rel}:{}: an argument or outcome kind with no witness form", line_no + 1));
                    *banks.unread_rows.entry(function.to_ascii_uppercase()).or_default() += 1;
                    continue;
                };
                let env = record.get("environment");
                let label = environment(String::new(), String::new(), env, fallback.as_ref()).environment();
                let source = *source_of.entry(label).or_insert_with(|| {
                    banks.sources.push(Source {
                        path: rel.clone(),
                        env: environment(String::new(), format!("{CHECKOUT} {rel}"), env, fallback.as_ref()),
                    });
                    banks.sources.len() - 1
                });
                let index = record["result_index"].as_array().and_then(|p| match p.as_slice() {
                    [r, c] => Some((r.as_u64()? as usize, c.as_u64()? as usize)),
                    _ => None,
                });
                banks.readings.push(Reading {
                    function: function.to_ascii_uppercase(),
                    id: format!("oc{}", line_no + 1),
                    args,
                    index,
                    expected,
                    source,
                });
            }
            continue;
        }
        // Only Excel's answers: a bank file is named answers*.json. Predictions, synthetic answers,
        // audit inputs and research files share the WitnessSet shape but hold no Excel reading.
        if !name.ends_with(".json") || !name.starts_with("answers") {
            continue;
        }
        let head = gunzip(&blob_head(checkout, rel)?, Some(4096))?;
        let head = String::from_utf8_lossy(&head);
        if !head.contains("\"witnesses\"") || !head.contains("\"function\"") {
            continue;
        }
        if let Some(f) = only {
            // A set names its function in its first lines; skip the others undecoded.
            if !head.contains(&format!("\"{f}\"")) {
                continue;
            }
        }
        let bytes = gunzip(&blob_bytes(checkout, rel)?, None)?;
        let Ok(doc) = bank_json(&bytes) else {
            unread.push(format!("{rel}: not JSON"));
            continue;
        };
        let (Some(function), Some(rows)) = (doc["function"].as_str(), doc["witnesses"].as_array()) else {
            continue;
        };
        let function = function.to_ascii_uppercase();
        if only.is_some_and(|f| f != function) || rows.first().is_none_or(|r| r.get("expected_bits").is_none()) {
            continue;
        }
        if let Some((companion, count)) = derived_graph_companion(checkout, rel, &doc) {
            banks.import_notes.push(format!("{rel}: {count} derived graph rows are copies of worksheet readings in {companion}; not counted as additional calls"));
            continue;
        }
        let probes = batch_selections(checkout, rel);
        let (mut selections, mut hex_repairs) = (0, 0);
        banks.files_read += 1;
        let env = doc.get("capture_provenance").and_then(|p| p.get("environment"));
        banks.sources.push(Source {
            path: rel.clone(),
            env: environment(String::new(), format!("{CHECKOUT} {rel}"), env, None),
        });
        let source = banks.sources.len() - 1;
        for (i, row) in rows.iter().enumerate() {
            let (Some(args), Some(expected)) = (row["args"].as_array(), row["expected_bits"].as_str()) else {
                unread.push(format!("{rel}: row {i} has no args or expected_bits"));
                *banks.unread_rows.entry(function.clone()).or_default() += 1;
                continue;
            };
            let index = row.as_object().and_then(calc_verdict::result_index).or_else(|| {
                let selected = recovered_selection(row, &probes);
                selections += usize::from(selected.is_some());
                selected
            });
            let mut args = Value::Array(args.clone());
            normalize_bank_hex(&mut args);
            let mut normalized_expected = Value::String(expected.to_owned());
            normalize_bank_hex(&mut normalized_expected);
            hex_repairs += usize::from(args != row["args"] || normalized_expected != row["expected_bits"]);
            banks.readings.push(Reading {
                function: function.clone(),
                id: row["id"].as_str().map_or_else(|| format!("row{i}"), str::to_owned),
                args: args.as_array().unwrap().clone(),
                index,
                expected: normalized_expected.as_str().unwrap().to_owned(),
                source,
            });
        }
        if selections > 0 || hex_repairs > 0 {
            banks.import_notes.push(format!("{rel}: recovered {selections} result selections from batch probes; normalized duplicated hex prefixes in {hex_repairs} rows"));
        }
        if selections > 0 {
            banks.sources[source].env.source.push_str(&format!(
                "; result selection: {CHECKOUT} {}", batch_path(rel).unwrap()
            ));
        }
    }
    // New readings (`cargo xtask witness read`), kept as plain WitnessSet files.
    for path in plain_readings(&checkout.join(READINGS)) {
        let rel = path
            .strip_prefix(checkout)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        let parsed = fs::read(&path)
            .map_err(|e| e.to_string())
            .and_then(|b| bank_json(&b));
        let Ok(doc) = parsed else {
            unread.push(format!("{rel}: not JSON"));
            continue;
        };
        let (Some(function), Some(rows)) = (doc["function"].as_str(), doc["witnesses"].as_array()) else {
            continue;
        };
        let function = function.to_ascii_uppercase();
        if only.is_some_and(|f| f != function) {
            continue;
        }
        banks.files_read += 1;
        let env = doc.get("capture_provenance").and_then(|p| p.get("environment"));
        banks.sources.push(Source {
            path: rel.clone(),
            env: environment(String::new(), format!("{CHECKOUT} {rel}"), env, None),
        });
        let source = banks.sources.len() - 1;
        for (i, row) in rows.iter().enumerate() {
            let (Some(args), Some(expected)) = (row["args"].as_array(), row["expected_bits"].as_str()) else {
                unread.push(format!("{rel}: row {i} has no args or expected_bits"));
                *banks.unread_rows.entry(function.clone()).or_default() += 1;
                continue;
            };
            banks.readings.push(Reading {
                function: function.clone(),
                id: row["id"].as_str().map_or_else(|| format!("row{i}"), str::to_owned),
                args: args.clone(),
                index: row.as_object().and_then(calc_verdict::result_index),
                expected: expected.to_owned(),
                source,
            });
        }
    }
    Ok((banks, unread))
}

/// Every `.json` file under `dir`, sorted.
fn plain_readings(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = fs::read_dir(&d) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "json") {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

// ---------------------------------------------------------------------------------------------
// Calculating

struct Given(Vec<CalcValue>);

impl PreparedArguments for Given {
    fn count(&self) -> usize {
        self.0.len()
    }

    fn argument(&mut self, index: usize) -> CalcValue {
        self.0[index].clone()
    }
}

fn fault_name(fault: &FunctionFault) -> String {
    let text = format!("{fault:?}");
    let end = text.find([' ', '{', '(']).unwrap_or(text.len());
    format!("fault:{}", &text[..end])
}

/// calc-func's result of `function` on `args`, in the result encoding (`fault:<name>` for a
/// refusal, `fault:NotCatalogued`, `fault:Panic`, or `fault:Unreadable` for arguments with no
/// value form).
fn calculate(function: &str, args: &[Value], index: Option<(usize, usize)>) -> String {
    let Ok(target) = catalogue().resolve_surface(Notation::Call, function) else {
        return "fault:NotCatalogued".to_owned();
    };
    let Ok(values) = args
        .iter()
        .enumerate()
        .map(|(i, a)| decode_argument(a, &format!("args[{i}]")))
        .collect::<Result<Vec<_>, _>>()
    else {
        return "fault:Unreadable".to_owned();
    };
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        let mode = NumericMode::Fast;
        let environment = CalcEnvironment::dates(DateSystem::System1900);
        invoke(
            target,
            &mut Given(values),
            &mut EvalServices::new(&mode).with_environment(&environment),
            &mut FunctionCallScratch::new(),
        )
    }));
    match outcome {
        Err(_) => "fault:Panic".to_owned(),
        Ok(Err(fault)) => fault_name(&fault),
        Ok(Ok(value)) => select_result(&value, index)
            .and_then(|v| encode_result(&v))
            .unwrap_or_else(|| "fault:ResultShape".to_owned()),
    }
}

/// How long one row may calculate before it is given up as `fault:Timeout`.
const ROW_SECONDS: u64 = 20;
/// A row that takes longer than this (in a release build) is judged but not pinned: the portable
/// test runs unoptimised and in browsers, where it would take a hundred times as long.
const SLOW_MILLIS: u64 = 50;
const SLOW: &str = "slow:";

/// [`calculate`] for every job, on a worker thread: a row that gives no result within
/// [`ROW_SECONDS`] is `fault:Timeout`, its thread is left behind, and the rows after it go on in a
/// new thread (calc-func's IRR calculates without end on some inputs).
fn calculate_all(function: &str, jobs: &[(Vec<Value>, Option<(usize, usize)>)]) -> Vec<String> {
    use std::sync::mpsc::{RecvTimeoutError, channel};
    let mut out = Vec::with_capacity(jobs.len());
    while out.len() < jobs.len() {
        let (tx, rx) = channel();
        let name = function.to_owned();
        let todo = jobs[out.len()..].to_vec();
        std::thread::spawn(move || {
            for (args, index) in todo {
                let started = std::time::Instant::now();
                let result = calculate(&name, &args, index);
                let result = if started.elapsed() > std::time::Duration::from_millis(SLOW_MILLIS) {
                    format!("{SLOW}{result}")
                } else {
                    result
                };
                if tx.send(result).is_err() {
                    break;
                }
            }
        });
        loop {
            match rx.recv_timeout(std::time::Duration::from_secs(ROW_SECONDS)) {
                Ok(result) => {
                    out.push(result);
                    if out.len() == jobs.len() {
                        break;
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    out.push("fault:Timeout".to_owned());
                    break;
                }
                Err(RecvTimeoutError::Disconnected) => {
                    out.push("fault:Panic".to_owned());
                    break;
                }
            }
        }
    }
    out
}

/// The class of a calculated result against Excel's, with the ULP distance of two numbers.
fn judge(expected: &str, actual: &str) -> (Severity, Option<u64>) {
    if expected == actual {
        return (Severity::Exact, None);
    }
    match (decode_result(expected), decode_result(actual)) {
        (Ok(e), Ok(a)) => Severity::of(&e, &a),
        _ => (Severity::Structural, None),
    }
}

// ---------------------------------------------------------------------------------------------
// Grouping readings, differences, status

/// One judged row: a key with its readings (several only when they disagree).
struct Row<'a> {
    reading: &'a Reading,
    /// Calculated in more than [`SLOW_MILLIS`]; not pinned.
    slow: bool,
    builds: BTreeSet<String>,
    actual: String,
    severity: Severity,
    ulp: Option<u64>,
}

struct Difference {
    function: String,
    key: String,
    readings: Vec<(String, String, String)>,
    across_environments: bool,
}

fn build_order(build: &Option<String>) -> u64 {
    build.as_deref().and_then(|b| b.parse().ok()).unwrap_or(0)
}

fn group<'a>(banks: &'a Banks) -> (BTreeMap<String, Vec<Vec<&'a Reading>>>, Vec<Difference>) {
    // function -> key -> readings
    let mut by_key: BTreeMap<String, BTreeMap<String, Vec<&Reading>>> = BTreeMap::new();
    for r in &banks.readings {
        by_key.entry(r.function.clone()).or_default().entry(r.key()).or_default().push(r);
    }
    let mut differences = Vec::new();
    let mut out = BTreeMap::new();
    for (function, keys) in by_key {
        let mut rows = Vec::new();
        for (key, readings) in keys {
            let distinct: BTreeSet<&str> = readings.iter().map(|r| r.expected.as_str()).collect();
            if distinct.len() > 1 {
                let mut seen = BTreeSet::new();
                let listed: Vec<(String, String, String)> = readings
                    .iter()
                    .filter(|r| seen.insert((r.expected.clone(), banks.sources[r.source].env.environment())))
                    .map(|r| {
                        let s = &banks.sources[r.source];
                        (r.expected.clone(), s.env.environment(), s.path.clone())
                    })
                    .collect();
                let envs: BTreeSet<&str> = listed.iter().map(|(_, e, _)| e.as_str()).collect();
                differences.push(Difference {
                    function: function.clone(),
                    key: key.clone(),
                    across_environments: envs.len() > 1,
                    readings: listed,
                });
            }
            rows.push(readings);
        }
        out.insert(function, rows);
    }
    (out, differences)
}

fn calc_func_commit(root: &Path) -> String {
    let git = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
            .unwrap_or_default()
    };
    let head = git(&["rev-parse", "--short=10", "HEAD"]);
    let dirty = !git(&["status", "--porcelain", "--", "crates/calc-func", "crates/calc-value"]).is_empty();
    if dirty { format!("{head}+changes") } else { head }
}

const STATUS_HEADER: &str = "function\tbank\texcel_builds\trows\tmatched\tworst_ulp\tclass\tcalc_func_commit";

struct Status {
    function: String,
    builds: String,
    rows: usize,
    matched: usize,
    worst_ulp: Option<u64>,
    class: Severity,
}

fn status_of(function: &str, rows: &[Row]) -> Status {
    let mut builds = BTreeSet::new();
    for r in rows {
        builds.extend(r.builds.iter().cloned());
    }
    Status {
        function: function.to_owned(),
        builds: builds.into_iter().collect::<Vec<_>>().join(","),
        rows: rows.len(),
        matched: rows.iter().filter(|r| r.severity == Severity::Exact).count(),
        worst_ulp: rows.iter().filter_map(|r| r.ulp).max(),
        class: rows.iter().map(|r| r.severity).max().unwrap_or(Severity::Exact),
    }
}

fn write_status(root: &Path, bank: &str, statuses: &[Status], only: Option<&str>) -> Result<(), String> {
    let path = root.join(STATUS);
    let commit = calc_func_commit(root);
    let mut lines: BTreeMap<(String, String), String> = BTreeMap::new();
    if let Ok(text) = fs::read_to_string(&path) {
        for line in text.lines().skip(1) {
            let mut cols = line.split('\t');
            let (Some(f), Some(b)) = (cols.next(), cols.next()) else { continue };
            let replaced = b == bank && only.is_none_or(|o| o == f);
            if !replaced {
                lines.insert((f.to_owned(), b.to_owned()), line.to_owned());
            }
        }
    }
    for s in statuses {
        let line = format!(
            "{}\t{bank}\t{}\t{}\t{}\t{}\t{}\t{commit}",
            s.function,
            if s.builds.is_empty() { "?" } else { &s.builds },
            s.rows,
            s.matched,
            s.worst_ulp.map_or("-".to_owned(), |u| u.to_string()),
            s.class.name()
        );
        lines.insert((s.function.clone(), bank.to_owned()), line);
    }
    let mut text = String::from(STATUS_HEADER);
    text.push('\n');
    for line in lines.values() {
        text.push_str(line);
        text.push('\n');
    }
    fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
}

fn write_differences(root: &Path, differences: &[Difference], only: Option<&str>) -> Result<(), String> {
    let path = root.join(DIFFERENCES);
    let mut kept: Vec<String> = Vec::new();
    if let (Some(only), Ok(text)) = (only, fs::read_to_string(&path)) {
        kept.extend(text.lines().skip(1).filter(|l| !l.starts_with(&format!("{only}\t"))).map(str::to_owned));
    }
    for d in differences {
        let readings: Vec<String> = d.readings.iter().map(|(e, env, src)| format!("{e} on {env} ({src})")).collect();
        kept.push(format!(
            "{}\t{}\t{}\t{}",
            d.function,
            if d.across_environments { "version_or_platform" } else { "same_environment" },
            d.key,
            readings.join(" | ")
        ));
    }
    kept.sort();
    let mut text = String::from("function\tkind\tinputs\treadings\n");
    for line in kept {
        text.push_str(&line);
        text.push('\n');
    }
    fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
}

// ---------------------------------------------------------------------------------------------
// Pinned sets

fn pinned_sets(root: &Path, only: Option<&str>) -> Result<Vec<(PathBuf, WitnessSet)>, String> {
    let dir = root.join(PINNED_DIR);
    let mut sets = Vec::new();
    let Ok(entries) = fs::read_dir(&dir) else {
        return Ok(sets);
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    paths.sort();
    for path in paths {
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let bytes = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let set = WitnessSet::decode(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        if only.is_none_or(|f| f == set.function) {
            sets.push((path, set));
        }
    }
    Ok(sets)
}

fn file_name(function: &str) -> String {
    format!("{function}.json")
}

/// The licence ledger's scan markers (hex of their ASCII bytes).
fn markers(root: &Path) -> Vec<String> {
    let Ok(text) = fs::read_to_string(root.join("migration/licence-ledger.json")) else {
        return Vec::new();
    };
    let Ok(ledger) = serde_json::from_str::<Value>(&text) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for family in ledger["scan"]["markers"].as_array().into_iter().flatten() {
        for form in family["forms_hex"].as_array().into_iter().flatten() {
            let Some(hex) = form.as_str() else { continue };
            let bytes: Option<Vec<u8>> = (0..hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok())
                .collect();
            if let Some(text) = bytes.and_then(|b| String::from_utf8(b).ok()) {
                out.push(text);
            }
        }
    }
    out
}

fn holds_marker(text: &str, markers: &[String]) -> bool {
    markers.iter().any(|m| {
        text.match_indices(m.as_str()).any(|(at, _)| {
            let before = text[..at].chars().next_back();
            let after = text[at + m.len()..].chars().next();
            !before.is_some_and(|c| c.is_ascii_alphanumeric()) && !after.is_some_and(|c| c.is_ascii_alphanumeric())
        })
    })
}

/// The rows a pinned set keeps of one function's keys: per bank file its first rows and an evenly
/// spaced sample of the rest, the budget shared in proportion to the banks' sizes.
fn select<'a>(rows: &'a [Row<'a>], budget: usize) -> Vec<&'a Row<'a>> {
    let mut by_source: BTreeMap<usize, Vec<&Row>> = BTreeMap::new();
    for r in rows {
        by_source.entry(r.reading.source).or_default().push(r);
    }
    let total = rows.len().max(1);
    let mut chosen: Vec<&Row> = Vec::new();
    for list in by_source.values() {
        let quota = (budget * list.len()).div_ceil(total).max(1);
        let edge = EDGE_ROWS.min(list.len()).min(quota);
        chosen.extend(&list[..edge]);
        let rest = &list[edge..];
        let want = quota.saturating_sub(edge).min(rest.len());
        if want > 0 {
            let stride = rest.len() as f64 / want as f64;
            chosen.extend((0..want).map(|i| rest[(i as f64 * stride) as usize]));
        }
    }
    if chosen.len() > budget {
        let stride = chosen.len() as f64 / budget as f64;
        chosen = (0..budget).map(|i| chosen[(i as f64 * stride) as usize]).collect();
    }
    chosen
}

fn write_pinned(root: &Path, banks: &Banks, judged: &BTreeMap<String, Vec<Row>>, budget: usize) -> Result<(usize, Vec<String>), String> {
    let dir = root.join(PINNED_DIR);
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let markers = markers(root);
    let mut bytes = 0;
    let mut panics = Vec::new();
    for (function, rows) in judged {
        let usable: Vec<Row> = rows
            .iter()
            .filter(|r| {
                if r.actual == "fault:Panic" || r.actual == "fault:Timeout" || r.slow {
                    panics.push(format!("{function} {}", Value::Array(r.reading.args.clone())));
                    false
                } else {
                    true
                }
            })
            .map(|r| Row {
                reading: r.reading,
                builds: r.builds.clone(),
                actual: r.actual.clone(),
                severity: r.severity,
                ulp: r.ulp,
                slow: r.slow,
            })
            .collect();
        let chosen = select(&usable, budget);
        if chosen.is_empty() {
            continue;
        }
        let mut readings: Vec<ReadingSource> = Vec::new();
        let mut witnesses = Vec::new();
        for (i, r) in chosen.iter().enumerate() {
            let source = &banks.sources[r.reading.source];
            let id = match readings.iter().position(|s| s.source == source.env.source && s.environment() == source.env.environment()) {
                Some(at) => readings[at].id.clone(),
                None => {
                    let mut env = source.env.clone();
                    env.id = format!("r{}", readings.len());
                    readings.push(env);
                    readings.last().map(|e| e.id.clone()).unwrap_or_default()
                }
            };
            let row_id = if holds_marker(&r.reading.id, &markers) { format!("row-{i}") } else { r.reading.id.clone() };
            witnesses.push(WitnessRow {
                id: row_id,
                args: r.reading.args.clone(),
                result_index: r.reading.index,
                expected: r.reading.expected.clone(),
                reading: id,
                calc_func: (r.severity != Severity::Exact).then(|| r.actual.clone()),
            });
        }
        let set = WitnessSet {
            function: function.clone(),
            selection: format!(
                "cargo xtask witness pin: of {} distinct readings in {} bank files of {CHECKOUT}, the first {EDGE_ROWS} rows of each bank and an evenly spaced sample of the rest, the budget of {budget} rows shared by bank size; calc_func is the result calc-func calculated at pinning, on rows it did not match",
                rows.len(),
                readings.len()
            ),
            readings,
            witnesses,
        };
        let text = set.encode();
        bytes += text.len();
        let path = dir.join(file_name(function));
        fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    // The index the portable test includes every set by.
    let mut names: Vec<String> = fs::read_dir(&dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".json"))
        .collect();
    names.sort();
    let mut index = String::from(
        "// Written by `cargo xtask witness pin`: every pinned witness set, embedded for the portable test.\n\npub const SETS: &[(&str, &str)] = &[\n",
    );
    for n in &names {
        index.push_str(&format!("    ({n:?}, include_str!({n:?})),\n"));
    }
    index.push_str("];\n");
    fs::write(dir.join("index.rs"), index).map_err(|e| format!("index.rs: {e}"))?;
    Ok((bytes, panics))
}

// ---------------------------------------------------------------------------------------------
// OxFunc's ledger

fn csv_fields(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            ('"', _) => quoted = !quoted,
            (',', false) => out.push(std::mem::take(&mut field)),
            _ => field.push(c),
        }
    }
    out.push(field);
    out
}

fn compare_with_ledger(checkout: &Path, root: &Path, statuses: &[Status]) -> Result<(), String> {
    let text = fs::read_to_string(checkout.join(LEDGER)).map_err(|e| format!("{LEDGER}: {e}"))?;
    let mut lines = text.lines();
    let header = csv_fields(lines.next().unwrap_or_default());
    let col = |name: &str| header.iter().position(|h| h == name);
    let (Some(fid), Some(st), Some(sev), Some(rj), Some(ra), Some(wu)) = (
        col("function_id"),
        col("status"),
        col("severity"),
        col("rows_judged"),
        col("rows_agree"),
        col("worst_ulp"),
    ) else {
        return Err(format!("{LEDGER}: missing columns"));
    };
    let mut ledger = BTreeMap::new();
    for line in lines {
        let f = csv_fields(line);
        if f.len() <= wu.max(ra) {
            continue;
        }
        let name = f[fid].trim_start_matches("FUNC.").to_ascii_uppercase();
        ledger.insert(name, (f[st].clone(), f[sev].clone(), f[rj].clone(), f[ra].clone(), f[wu].clone()));
    }
    let mut out = String::from("function\tdnacalc_rows\tdnacalc_matched\tdnacalc_rate\tdnacalc_class\tdnacalc_worst_ulp\toxfunc_status\toxfunc_severity\toxfunc_rows\toxfunc_agree\toxfunc_rate\toxfunc_worst_ulp\tworse\n");
    for s in statuses {
        let rate = s.matched as f64 / s.rows.max(1) as f64;
        let (status, severity, rows, agree, worst) = ledger.get(&s.function).cloned().unwrap_or_default();
        let ox_rate = match (rows.parse::<f64>(), agree.parse::<f64>()) {
            (Ok(r), Ok(a)) if r > 0.0 => Some(a / r),
            _ => None,
        };
        let worse = match ox_rate {
            Some(o) => rate + 1e-9 < o,
            None => status == "consistent" && s.matched < s.rows,
        };
        out.push_str(&format!(
            "{}\t{}\t{}\t{:.4}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            s.function,
            s.rows,
            s.matched,
            rate,
            s.class.name(),
            s.worst_ulp.map_or("-".to_owned(), |u| u.to_string()),
            status,
            severity,
            rows,
            agree,
            ox_rate.map_or("-".to_owned(), |r| format!("{r:.4}")),
            worst,
            if worse { "worse" } else { "" }
        ));
    }
    let dir = root.join("target/witness");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    fs::write(dir.join("full-vs-oxfunc.tsv"), out).map_err(|e| e.to_string())?;
    println!("xtask witness: wrote target/witness/full-vs-oxfunc.tsv");
    Ok(())
}

// ---------------------------------------------------------------------------------------------

pub fn run(raw: &[String]) -> ExitCode {
    let request = match parse_args(raw) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("xtask witness: {e}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match run_request(&request) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("xtask witness: {e}");
            ExitCode::from(1)
        }
    }
}

fn run_request(request: &Request) -> Result<ExitCode, String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let ws = Workspace::load(&cwd)?;
    let only = request.function.as_deref();
    let checkout = legacy_root(&ws.root).path.join(CHECKOUT);
    if request.verb == Verb::Read {
        return run_read(&ws.root, &checkout, request);
    }

    // The readings of the bank asked for.
    let (banks, unread) = match request.bank {
        Bank::Full => {
            if !checkout.join(MANIFESTS[0]).is_file() {
                return Err(format!(
                    "the evidence checkout belongs at {} (<legacy root>/{CHECKOUT})",
                    checkout.display()
                ));
            }
            load_full(&checkout, only)?
        }
        Bank::Pinned => {
            let mut banks = Banks { sources: Vec::new(), readings: Vec::new(), files_read: 0, import_notes: Vec::new(), unread_rows: BTreeMap::new() };
            for (_, set) in pinned_sets(&ws.root, only)? {
                banks.files_read += 1;
                let first = banks.sources.len();
                for r in &set.readings {
                    banks.sources.push(Source { path: r.source.clone(), env: r.clone() });
                }
                for w in set.witnesses {
                    let source = first + set.readings.iter().position(|r| r.id == w.reading).unwrap_or(0);
                    banks.readings.push(Reading {
                        function: set.function.clone(),
                        id: w.id,
                        args: w.args,
                        index: w.result_index,
                        expected: w.expected,
                        source,
                    });
                }
            }
            (banks, Vec::new())
        }
    };
    println!(
        "xtask witness: {} readings of {} bank files",
        banks.readings.len(),
        banks.files_read
    );
    for note in &banks.import_notes {
        println!("xtask witness: import: {note}");
    }
    for line in unread.iter().take(20) {
        println!("xtask witness: unread: {line}");
    }
    if unread.len() > 20 {
        println!("xtask witness: unread: ... {} in all", unread.len());
    }

    let unread_rows: usize = banks.unread_rows.values().sum();
    let unread_files = unread.len().saturating_sub(unread_rows);
    if unread_files > 0 {
        println!("xtask witness: {unread_files} bank files could not be decoded; their row counts are unknown and are additional to the counted failures");
    }
    let (mut grouped, differences) = group(&banks);
    for function in banks.unread_rows.keys() {
        grouped.entry(function.clone()).or_default();
    }
    let mut judged: BTreeMap<String, Vec<Row>> = BTreeMap::new();
    let mut statuses = Vec::new();
    for (function, keys) in &grouped {
        // The reading of the latest build stands for a key whose readings disagree.
        let chosen: Vec<&Reading> = keys
            .iter()
            .map(|readings| {
                readings
                    .iter()
                    .max_by_key(|r| build_order(&banks.sources[r.source].env.excel_build))
                    .copied()
                    .unwrap_or(readings[0])
            })
            .collect();
        let jobs: Vec<(Vec<Value>, Option<(usize, usize)>)> =
            chosen.iter().map(|r| (r.args.clone(), r.index)).collect();
        let actuals = calculate_all(function, &jobs);
        let mut rows = Vec::new();
        for ((readings, reading), actual) in keys.iter().zip(chosen).zip(actuals) {
            let (slow, actual) = match actual.strip_prefix(SLOW) {
                Some(rest) => (true, rest.to_owned()),
                None => (false, actual),
            };
            let builds: BTreeSet<String> = readings
                .iter()
                .map(|r| banks.sources[r.source].env.excel_build.clone().unwrap_or_else(|| "?".to_owned()))
                .collect();
            if actual == "fault:Timeout" {
                println!("xtask witness: {function}: no result within {ROW_SECONDS} s on {}", Value::Array(reading.args.clone()));
            }
            let (severity, ulp) = judge(&reading.expected, &actual);
            rows.push(Row { reading, builds, actual, severity, ulp, slow });
        }
        let mut status = status_of(function, &rows);
        let unread = banks.unread_rows.get(function).copied().unwrap_or(0);
        status.rows += unread;
        if unread > 0 {
            status.class = Severity::Structural;
            println!("xtask witness: {function}: {unread} unread recorded rows count as failures");
        }
        println!(
            "xtask witness: {:<22} {:>8} rows {:>8} matched  worst {:>6} ULP  {}",
            function,
            status.rows,
            status.matched,
            status.worst_ulp.map_or("-".to_owned(), |u| u.to_string()),
            status.class.name()
        );
        statuses.push(status);
        judged.insert(function.clone(), rows);
    }
    let exact = statuses.iter().filter(|s| s.class == Severity::Exact).count();
    let rows: usize = statuses.iter().map(|s| s.rows).sum();
    let matched: usize = statuses.iter().map(|s| s.matched).sum();
    println!(
        "xtask witness: {} functions, {exact} exact on every counted row; {matched} of {rows} rows matched, {} failed (mismatched, refused or unread; {unread_rows} unread)",
        statuses.len(),
        rows - matched
    );
    let across = differences.iter().filter(|d| d.across_environments).count();
    println!(
        "xtask witness: {} inputs read differently: {across} across builds or platforms, {} within one environment",
        differences.len(),
        differences.len() - across
    );

    if request.verb == Verb::Pin {
        let (bytes, panics) = write_pinned(&ws.root, &banks, &judged, request.rows)?;
        println!("xtask witness: wrote pinned sets, {bytes} bytes, under {PINNED_DIR}");
        println!("xtask witness: {} rows left out of the pinned sets", panics.len());
        for p in panics.iter().take(30) {
            println!("xtask witness: left out, calc-func panics, takes over {SLOW_MILLIS} ms or does not finish: {p}");
        }
    }
    let bank = match request.bank {
        Bank::Pinned => "pinned",
        Bank::Full => "full",
    };
    write_status(&ws.root, bank, &statuses, only)?;
    println!("xtask witness: wrote {STATUS} ({bank})");
    if request.bank == Bank::Full {
        // `pin` judged the full banks too, so it writes their status as `judge --bank full` does.
        write_differences(&ws.root, &differences, only)?;
        println!("xtask witness: wrote {DIFFERENCES}");
        compare_with_ledger(&checkout, &ws.root, &statuses)?;
    }
    Ok(ExitCode::SUCCESS)
}

// ---------------------------------------------------------------------------------------------
// Reading Excel: new readings and targeted rechecks

/// Whether a worker failure is Excel refusing a COM call because it is busy (another lane's Excel
/// can keep it so); such a run is taken again.
fn call_rejected(error: &calc_oracle_excel::CaptureError) -> bool {
    let text = error.to_string();
    text.contains("RPC_E_CALL_REJECTED")
        || text.contains("0x80010001")
        || text.contains("Call was rejected by callee")
}

/// `cargo xtask witness read`: Excel's reading of every row of each named function's pinned set,
/// on the Excel of this host, through calc-oracle-excel's bulk witness mode. The readings are
/// written as a WitnessSet with their environment under `readings/excel-<build>/` of the evidence
/// repository, where `--bank full` reads them as one more bank, and each row is compared with the
/// banked reading it re-reads.
fn run_read(root: &Path, checkout: &Path, request: &Request) -> Result<ExitCode, String> {
    let functions: Vec<String> = request
        .function
        .as_deref()
        .unwrap_or_default()
        .split(',')
        .filter(|f| !f.is_empty())
        .map(str::to_owned)
        .collect();
    let mut failed = false;
    for function in functions {
        let sets = pinned_sets(root, Some(&function))?;
        let Some((_, set)) = sets.into_iter().next() else {
            return Err(format!("no pinned set of {function} to read; pin it first"));
        };
        let mut calls = Vec::new();
        let mut rows = Vec::new();
        let mut skipped = 0;
        for w in &set.witnesses {
            let args: Result<Vec<CalcValue>, String> = w
                .args
                .iter()
                .enumerate()
                .map(|(i, a)| decode_argument(a, &format!("args[{i}]")))
                .collect();
            match args {
                Ok(args) if args.iter().all(|a| a.as_array().is_none()) => {
                    calls.push(calc_oracle_excel::WitnessCall {
                        id: w.id.clone(),
                        args,
                        result_index: w.result_index,
                    });
                    rows.push(w);
                }
                _ => skipped += 1,
            }
        }
        // Rows with the same argument count stay together, so the worker writes long runs.
        let mut order: Vec<usize> = (0..calls.len()).collect();
        order.sort_by_key(|&i| (calls[i].args.len(), calls[i].result_index));
        let calls: Vec<_> = order.iter().map(|&i| calls[i].clone()).collect();
        let rows: Vec<&WitnessRow> = order.iter().map(|&i| rows[i]).collect();
        let mut attempt = 0;
        let outcome = loop {
            attempt += 1;
            let work = root.join(format!(
                "target/witness/read/{function}-{}-{attempt}",
                crate::accept::utc_now().replace([':', '-'], "")
            ));
            let result = calc_oracle_excel::read_witnesses(&calc_oracle_excel::WitnessRequest {
                function: function.clone(),
                calls: calls.clone(),
                work_dir: work,
                timeout: std::time::Duration::from_secs(request.timeout),
            });
            match result {
                Err(e) if attempt < 4 && call_rejected(&e) => {
                    println!(
                        "xtask witness: {function}: Excel rejected a call (attempt {attempt}); taking the reading again"
                    );
                    std::thread::sleep(std::time::Duration::from_secs(5 * attempt));
                }
                other => break other,
            }
        };
        let outcome = match outcome {
            Ok(o) => o,
            Err(e) => {
                println!("xtask witness: {function}: no reading: {e}");
                failed = true;
                continue;
            }
        };
        let env = &outcome.environment;
        let build = env.excel_build.clone();
        let mut agree = 0;
        let mut differ = Vec::new();
        let mut unread = Vec::new();
        let mut witnesses = Vec::new();
        for (reading, row) in outcome.readings.iter().zip(&rows) {
            let value = match &reading.value {
                Ok(v) => match encode_result(v) {
                    Some(text) => text,
                    None => {
                        unread.push(format!("{}: a value with no witness form", row.id));
                        continue;
                    }
                },
                Err(why) => {
                    unread.push(format!("{}: {why}", row.id));
                    continue;
                }
            };
            let banked = set.readings.iter().find(|r| r.id == row.reading);
            if value == row.expected {
                agree += 1;
            } else {
                differ.push(format!(
                    "{} {}: {} on {} ({}), {value} on {build}",
                    row.id,
                    Value::Array(row.args.clone()),
                    row.expected,
                    banked.map_or("?".to_owned(), ReadingSource::environment),
                    banked.map_or("?", |r| r.source.as_str())
                ));
            }
            let mut node = serde_json::Map::new();
            node.insert("id".into(), Value::String(row.id.clone()));
            node.insert("args".into(), Value::Array(row.args.clone()));
            if let Some((r, c)) = row.result_index {
                node.insert("result_index".into(), serde_json::json!([r, c]));
            }
            node.insert("expected_bits".into(), Value::String(value));
            witnesses.push(Value::Object(node));
        }
        let reader = format!(
            "{}; witness worker sha256:{}",
            calc_oracle_excel::capture_tool_revision(None),
            calc_oracle_excel::sha256_hex(calc_oracle_excel::WITNESS_SCRIPT.as_bytes())
        );
        let doc = serde_json::json!({
            "function": function,
            "witnesses": witnesses,
            "capture_provenance": {
                "schema_version": "dnacalc-witness-reading/1",
                "captured_utc": crate::accept::utc_now(),
                "reader": reader,
                "inputs": format!("the rows of the pinned set crates/calc-func/tests/witnesses/{function}.json"),
                "environment": {
                    "excel_version": env.excel_version,
                    "excel_build": env.excel_build,
                    "excel_file_version": env.excel_file_version,
                    "excel_bitness": env.bitness,
                    "platform": env.platform,
                    "excel_operating_system": env.os_version,
                    "cpu_id": env.cpu,
                    "excel_input_plumbing": "cell_value2_bulk",
                }
            }
        });
        let dir = checkout.join(READINGS).join(format!("excel-{build}"));
        fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let mut path = dir.join(format!("{function}.json"));
        let mut n = 2;
        while path.exists() {
            path = dir.join(format!("{function}-{n}.json"));
            n += 1;
        }
        let text = serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())? + "\n";
        fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
        println!(
            "xtask witness: {function}: read {} rows on Excel {} build {build} ({}, {}, {}): {agree} equal the banked reading, {} differ, {} unread, {skipped} not asked (array arguments); wrote {}",
            outcome.readings.len(),
            env.excel_version,
            env.bitness,
            env.platform,
            env.cpu.as_deref().unwrap_or("CPU unknown"),
            differ.len(),
            unread.len(),
            path.display()
        );
        for d in &differ {
            println!("xtask witness: {function}: differs: {d}");
        }
        for u in unread.iter().take(10) {
            println!("xtask witness: {function}: unread: {u}");
        }
    }
    Ok(if failed {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn judge_defaults_to_the_pinned_bank_and_pin_to_the_full_one() {
        let r = parse_args(&args("judge")).unwrap();
        assert!(r.verb == Verb::Judge && r.bank == Bank::Pinned && r.function.is_none());
        let r = parse_args(&args("judge --bank full --func pmt")).unwrap();
        assert!(r.bank == Bank::Full && r.function.as_deref() == Some("PMT"));
        let r = parse_args(&args("pin --rows 100")).unwrap();
        assert!(r.verb == Verb::Pin && r.bank == Bank::Full && r.rows == 100);
        let r = parse_args(&args("read --func gammaln --timeout 60")).unwrap();
        assert!(r.verb == Verb::Read && r.function.as_deref() == Some("GAMMALN") && r.timeout == 60);
        assert!(parse_args(&args("read")).is_err());
        assert!(parse_args(&args("pin --bank pinned")).is_err());
        assert!(parse_args(&args("judge --rows 3")).is_err());
        assert!(parse_args(&args("race")).is_err());
    }

    #[test]
    fn a_gzip_member_inflates_with_or_without_a_limit() {
        let text = b"hello hello hello hello
".repeat(400);
        let mut gz = vec![0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 3];
        gz.extend(miniz_oxide::deflate::compress_to_vec(&text, 6));
        gz.extend([0u8; 8]);
        assert_eq!(gunzip(&gz, None).unwrap(), text);
        assert_eq!(gunzip(&gz, Some(100)).unwrap(), text[..100]);
        assert!(deflate_body(&gz[..10]).is_err());
    }

    #[test]
    fn oracle_records_take_the_witness_encoding() {
        let n: Value = serde_json::json!({"kind": "number", "value": 1.0, "bits_hex": "0x3ff0000000000000"});
        assert_eq!(oracle_argument(&n), Some(Value::String("0x3ff0000000000000".into())));
        let m: Value = serde_json::json!({"kind": "matrix", "rows": 2, "cols": 2, "bits_hex_row_major": [["0x1", "0x2", "0x3", "0x4"]]});
        assert_eq!(
            oracle_argument(&m),
            Some(serde_json::json!([["0x1", "0x2"], ["0x3", "0x4"]]))
        );
        assert_eq!(oracle_outcome(&serde_json::json!({"kind": "number", "bits_hex": "0x0"})).as_deref(), Some("0x0"));
        assert_eq!(oracle_outcome(&serde_json::json!({"kind": "error", "code": "#NUM!"})).as_deref(), Some("error:Num"));
    }

    #[test]
    fn graph_copies_require_every_recorded_id_and_answer_in_the_worksheet_bank() {
        let original = serde_json::json!({"witnesses": [
            {"id": "a", "args": ["worksheet inputs"], "expected_bits": "0x3ff0000000000000"}
        ]});
        let mut derived = serde_json::json!({"witnesses": [
            {"id": "a", "args": ["lowered inputs"], "expected_bits": "0x3ff0000000000000"}
        ]});
        assert!(graph_rows_linked(&derived, &original));
        derived["witnesses"][0]["expected_bits"] = serde_json::json!("0x3ff0000000000001");
        assert!(!graph_rows_linked(&derived, &original));
        derived["witnesses"][0]["id"] = serde_json::json!("unknown");
        assert!(!graph_rows_linked(&derived, &original));
    }

    #[test]
    fn array_selection_is_recovered_by_input_identity_and_not_by_output() {
        let probe = serde_json::json!({"id": "cell", "args": [[["0x3ff0000000000000"]]], "result_index": [4, 2]});
        let mut row = serde_json::json!({"id": "cell", "args": [[["0x3ff0000000000000"]]], "expected_bits": "0x3ff0000000000000"});
        assert_eq!(recovered_selection(&row, &[probe.clone()]), Some((4, 2)));
        let mut conflicting = probe.clone();
        conflicting["result_index"] = serde_json::json!([3, 2]);
        assert_eq!(recovered_selection(&row, &[probe.clone(), conflicting]), None);
        row["args"][0][0][0] = serde_json::json!("0x4000000000000000");
        assert_eq!(recovered_selection(&row, &[probe]), None);
    }

    #[test]
    fn duplicated_hex_prefixes_keep_the_recorded_bits() {
        let mut row = serde_json::json!(["0x0x3e22024955d1ccf5", ["0x0x8000000000000000"], "0x0xnot-a-number"]);
        normalize_bank_hex(&mut row);
        assert_eq!(row, serde_json::json!(["0x3e22024955d1ccf5", ["0x8000000000000000"], "0x0xnot-a-number"]));
    }

    #[test]
    fn powershell_bom_does_not_drop_recorded_rows() {
        let raw = b"\xef\xbb\xbf{\"function\":\"ERF.PRECISE\",\"witnesses\":[{\"id\":\"row\"}]}";
        let doc = bank_json(raw).unwrap();
        assert_eq!(doc["function"], "ERF.PRECISE");
        assert_eq!(doc["witnesses"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn a_marker_counts_only_between_non_alphanumerics() {
        let m = vec!["abc".to_owned()];
        assert!(holds_marker("x-abc-1", &m));
        assert!(!holds_marker("xabc", &m));
        assert!(!holds_marker("abc1", &m));
    }

    #[test]
    fn csv_fields_keep_quoted_commas() {
        assert_eq!(csv_fields("a,\"b,c\",\"d\"\"e\""), vec!["a", "b,c", "d\"e"]);
    }
}
