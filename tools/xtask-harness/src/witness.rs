//! `cargo xtask witness judge [--func F] [--bank pinned|full]` and `cargo xtask witness pin [--func
//! F] [--rows N]` (bead dc-ymjf.2): calc-func judged bit for bit, function by function, on Excel's
//! readings. `cargo xtask witness read --func F[,F...]` (bead dc-ymjf.3) takes new readings of a
//! pinned set's rows on this host's Excel.
//!
//! - `--bank pinned` (the default) reads the pinned sets `crates/calc-func/tests/witnesses/*.json`,
//!   which calc-func's portable test `witnesses` also judges.
//! - `--bank full` reads every bank of the evidence repository (`<legacy root>/CalcFunc-Evidence`,
//!   straight from its gzip blobs, no restore): the oracle cache `cache/oracle*/build-*/*.jsonl`
//!   and every WitnessSet file (`{"function", "witnesses": [{"id", "args", "expected_bits"}]}`) of
//!   `manifest.tsv` and `w109-manifest.tsv` named `answers*.json` (the runners' answer files;
//!   predictions, synthetic answers and research files of the same shape hold no Excel reading).
//!
//! Each reading keeps its recorded build, platform, bitness, CPU and optional invocation facts:
//! argument transport, input-sheet state, locale/separators, date system and CompatibilityVersion.
//! Missing facts stay missing. Invocation facts are part of selection identity. A reading holds
//! across builds unless another reading of the same function, arguments, invocation and result cell disagrees: then both are kept and written to
//! `crates/calc-func/witness-differences.tsv`, as a version or platform difference when their
//! environments differ and as a same-environment disagreement when they do not. Such a row is
//! judged against the reading of the latest build, and the difference asks for a targeted recheck.
//!
//! Every row runs through `calc_func::invoke` under the application's numeric mode (Fast), in a
//! workbook environment with the captured date system and explicit complete locale when present
//! (historical date default: 1900). Scalar cell transport restores reference origin; unknown
//! positions are refused, and matrix-delegate layouts are not reconstructed. The caller applies
//! recorded CompatibilityVersion alongside those facts. Each result is classed with calc-verdict's [`Severity`] (OxFunc's ODR-FN-005). A refusal or a function the
//! catalogue does not hold is a structural miss: refused counts as failed. The command rewrites
//! the rows of the bank it judged in `crates/calc-func/witness-status.tsv` (function, bank, Excel
//! builds, rows, matched, worst ULP, class, calc-func commit). `--bank full` also compares each
//! function with OxFunc's last ledger (`function-lane/ledgers/EXCEL_PARITY_LEDGER.csv` of the
//! evidence repository) and writes `target/witness/full-vs-oxfunc.tsv`.
//! `judge --trace <path>` also writes the selected input, expected and calculated result of
//! every decoded row as JSONL, for comparisons that check individual gains and losses.
//! Unread rows still count as failures in the summary; they have no decoded trace row.
//! A reading file's explicit `unread` array also counts, including inputs Excel changed
//! during the Value2 write. Those responses are not answers to the requested calls.
//! `witness compare --before <trace> --after <trace>` streams two judge traces, rejects
//! any change to reading identity or recorded answers, and counts gains and losses.
//! It reports decoded rows only: use the judge summaries for unread failures. `judge --values-only`
//! keeps those same identities and recorded facts but uses the old bare-value invocation, so
//! context recovery can be measured separately from product-function changes.
//! `witness refresh [--func F]` recalculates the original pins and updates only
//! calculated predictions, preserving the sample and Excel's answers. It refuses
//! any newly lost pinned match, unless `--classified-losses <tsv>` lists it with its class
//! (function, id, class per line; a listed row that is not a new loss is refused too). `refresh --context` first restores facts from each pin's original
//! source, verifying the exact arguments, result selection, environment and retained answer;
//! conflicting contexts are refused. It preserves the original sample and can split a source
//! when its rows used different transports. Run the full-bank comparison before retaining a change.
//!
//! `judge --report <json>` writes counts without refreshing committed status or differences.
//! `--release` and `--profile perf` select speed/profiler builds in the standard target folder.
//!
//! `pin` judges the full banks as `judge --bank full` does, writes what it writes, and then the
//! pinned sets: per function, the first rows of each bank
//! (the generators write their edge rows first) and an evenly spaced sample of the rest, at most
//! `--rows` (default 120) in all; a row the calculation does not match carries the calculated
//! result (`calc_func`), so the portable test fails on a regression and on a gain alike. Rows on
//! which calc-func panics, calculates without end or takes over 50 ms are left out and listed,
//! since the wasm test cannot recover from the first two and runs unoptimised.
//!
//! `read` writes each pinned row's arguments into an Excel started under calc-oracle-excel's
//! isolation (its bulk witness mode) and reads the results through `Range.Value2`; it retries a
//! run on RPC_E_CALL_REJECTED and stops only the Excel it started. The readings go, as a
//! WitnessSet with their environment, to `readings/excel-<build>/<F>.json` of the evidence
//! repository, which `--bank full` reads as one more bank: a reading that disagrees with a banked
//! one then shows as a difference. This is the targeted recheck a difference asks for, and the way
//! new readings are taken; nothing re-reads the banks wholesale on each new build.
//!
//! `read --inputs <json>` accepts a function and witnesses (id, scalar args, optional result_index).
//! It calculates and syncs a labelled prediction file before Excel starts. `--new-only` refuses
//! any argument identity already in that function's loaded banks, or a bank with unread rows.
//! Each captured row retains its frozen prediction beside the actual Excel answer. The reader
//! creates a blank workbook and preserves AutomationSecurity rather than writing macro settings.

use crate::cargo::Workspace;
use crate::legacy::legacy_root;
use calc_func::value::CalcValue;
use calc_verdict::{
    ReadingSource, Severity, WitnessRow, WitnessSet, decode_argument, decode_result, encode_result,
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

pub const USAGE: &str = "cargo xtask witness judge [--func <F>] [--bank pinned|full] [--release | --profile dev|release|perf] [--report <json>] [--trace <path>] [--values-only] | cargo xtask witness compare --before <trace> --after <trace> | cargo xtask witness refresh [--func <F>] [--context] [--classified-losses <tsv>] | cargo xtask witness pin [--func <F>] [--rows <N>] | cargo xtask witness read --func <F>[,<F>...] [--timeout <s>] [--inputs <json> [--new-only] [--repeat-calls]] | cargo xtask witness export --report <jsonl> [--func <F>]";

use crate::evidence::CHECKOUT;
const MANIFESTS: [&str; 2] = ["manifest.tsv", "w109-manifest.tsv"];
const PINNED_DIR: &str = "crates/calc-func/tests/witnesses";
const STATUS: &str = "crates/calc-func/witness-status.tsv";
const DIFFERENCES: &str = "crates/calc-func/witness-differences.tsv";
const WEEKNUM_EXCLUSION: &str = "migration/evidence/dc-1943-weeknum.json";
const TEXTSPLIT_REPLAY: &str = "migration/evidence/dc-1943-textsplit-replay.json.gz";
const MINVERSE_REPLAY: &str = "migration/evidence/dc-1943-minverse-replay.json.gz";
const UNIQUE_RECAPTURE: &str = "migration/evidence/dc-ymjf-2-10-unique-recapture.json.gz";
const UNIQUE_SCALAR_CAPTURE: &str = "readings/excel-20527/bank2-answers-unique-spill.json";
const BINOM_REPLAY: &str = "migration/evidence/dc-1943-binom-replay.json.gz";
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
    Compare,
    Refresh,
    Pin,
    Read,
    Export,
}

struct Request {
    verb: Verb,
    function: Option<String>,
    bank: Bank,
    rows: usize,
    timeout: u64,
    trace: Option<PathBuf>,
    before: Option<PathBuf>,
    after: Option<PathBuf>,
    inputs: Option<PathBuf>,
    new_only: bool,
    repeat_calls: bool,
    values_only: bool,
    refresh_context: bool,
    classified_losses: Option<PathBuf>,
    report: Option<PathBuf>,
    profile: Option<String>,
}

fn parse_args(raw: &[String]) -> Result<Request, String> {
    let (verb, rest) = raw.split_first().ok_or("a subcommand is required")?;
    let verb = match verb.as_str() {
        "judge" => Verb::Judge,
        "compare" => Verb::Compare,
        "refresh" => Verb::Refresh,
        "pin" => Verb::Pin,
        "read" => Verb::Read,
        "export" => Verb::Export,
        other => return Err(format!("unknown subcommand {other}")),
    };
    let mut request = Request {
        verb,
        function: None,
        bank: if matches!(verb, Verb::Pin | Verb::Export) {
            Bank::Full
        } else {
            Bank::Pinned
        },
        rows: 120,
        timeout: 600,
        trace: None,
        before: None,
        after: None,
        inputs: None,
        new_only: false,
        repeat_calls: false,
        values_only: false,
        refresh_context: false,
        classified_losses: None,
        report: None,
        profile: None,
    };
    let mut it = rest.iter();
    while let Some(arg) = it.next() {
        let mut value = || it.next().cloned().ok_or(format!("{arg} needs a value"));
        match arg.as_str() {
            "--func" if verb != Verb::Compare => {
                request.function = Some(value()?.to_ascii_uppercase())
            }
            "--before" if verb == Verb::Compare => request.before = Some(PathBuf::from(value()?)),
            "--after" if verb == Verb::Compare => request.after = Some(PathBuf::from(value()?)),
            "--inputs" if verb == Verb::Read => request.inputs = Some(PathBuf::from(value()?)),
            "--new-only" if verb == Verb::Read => request.new_only = true,
            "--repeat-calls" if verb == Verb::Read => request.repeat_calls = true,
            "--context" if verb == Verb::Refresh => request.refresh_context = true,
            "--classified-losses" if verb == Verb::Refresh => {
                request.classified_losses = Some(PathBuf::from(value()?))
            }
            "--report" if matches!(verb, Verb::Judge | Verb::Export) => {
                request.report = Some(PathBuf::from(value()?))
            }
            "--release" if verb == Verb::Judge && request.profile.is_none() => {
                request.profile = Some("release".into())
            }
            "--profile" if verb == Verb::Judge && request.profile.is_none() => {
                let profile = value()?;
                if !matches!(profile.as_str(), "dev" | "release" | "perf") {
                    return Err("--profile needs dev, release or perf".into());
                }
                request.profile = Some(profile);
            }
            "--values-only" if verb == Verb::Judge => request.values_only = true,
            "--trace" if verb == Verb::Judge => request.trace = Some(PathBuf::from(value()?)),
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
    if verb == Verb::Export && request.report.is_none() {
        return Err("export needs --report <jsonl>".into());
    }
    if verb == Verb::Read && request.function.is_none() {
        return Err("read needs --func".to_owned());
    }
    if verb == Verb::Compare && (request.before.is_none() || request.after.is_none()) {
        return Err("compare needs --before and --after".to_owned());
    }
    if request.verb == Verb::Read
        && request.function.as_deref().is_some_and(|names| {
            names.split(',').any(|name| {
                name.is_empty()
                    || !name
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_')
            })
        })
    {
        return Err(
            "--func needs worksheet function names (letters, digits, dots or underscores)".into(),
        );
    }
    if request.new_only && request.inputs.is_none() {
        return Err("--new-only requires --inputs".into());
    }
    if request.repeat_calls && request.inputs.is_none() {
        return Err("--repeat-calls requires --inputs".into());
    }
    if request.inputs.is_some() && request.function.as_deref().is_some_and(|f| f.contains(',')) {
        return Err("--inputs reads one function at a time".into());
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

fn reading_key(function: &str, args: &[Value], index: Option<(usize, usize)>) -> String {
    let index = index.map_or(String::new(), |(r, c)| format!("@{r},{c}"));
    format!("{function}|{}{index}", Value::Array(args.to_vec()))
}

fn contextual_key(
    function: &str,
    args: &[Value],
    index: Option<(usize, usize)>,
    context: Option<&calc_verdict::WitnessInvocation>,
) -> String {
    let key = reading_key(function, args, index);
    context.map_or_else(
        || key.clone(),
        |context| format!("{key}|{}", context.to_json()),
    )
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

fn record_unread_rows(
    doc: &Value,
    source: &str,
    function: &str,
    banks: &mut Banks,
    unread: &mut Vec<String>,
) -> Result<(), String> {
    if let Some(recorded) = doc.get("unread") {
        let rows = recorded
            .as_array()
            .ok_or_else(|| format!("{source}: unread must be an array of recorded failures"))?;
        for (i, row) in rows.iter().enumerate() {
            let detail = row
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| row.to_string());
            unread.push(format!("{source}: unread row {i}: {detail}"));
            *banks.unread_rows.entry(function.to_owned()).or_default() += 1;
        }
    }
    Ok(())
}

/// Transport experiments have their own observable contract. They never become
/// typed-argument semantic witnesses when their labels would be discarded.
fn record_transport_audits(doc: &Value, source: &str, banks: &mut Banks) -> Result<(), String> {
    if let Some(recorded) = doc.get("transport_audit_readings") {
        let rows = recorded
            .as_array()
            .ok_or_else(|| format!("{source}: transport_audit_readings must be an array"))?;
        for row in rows {
            if row["input_transport"] != "formula_literals_with_cell_echo"
                || row["semantic_witness"] != false
                || !row["id"].is_string()
                || !row["args"].is_array()
                || !row["observed_raw"].is_object()
                || row
                    .get("reading_failure")
                    .is_none_or(|v| !v.is_null() && !v.is_string())
            {
                return Err(format!("{source}: malformed transport audit observation"));
            }
        }
        let failures = rows
            .iter()
            .filter(|r| !r["reading_failure"].is_null())
            .count();
        banks.import_notes.push(format!("{source}: {} transport audit observations, {failures} failed audit obligations; retained separately from typed-argument semantic witnesses", rows.len()));
    }
    Ok(())
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
    let path = if single.is_file() {
        single
    } else {
        checkout.join(format!("{rel}.gz.part000"))
    };
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
    gz.get(at..gz.len() - 8)
        .ok_or_else(|| "a truncated gzip stream".to_owned())
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
fn environment(
    id: String,
    source: String,
    env: Option<&Value>,
    fallback: Option<&Value>,
    settings: Option<&Value>,
) -> ReadingSource {
    let field = |name: &str| {
        env.and_then(|e| e.get(name))
            .and_then(text_of)
            .or_else(|| fallback.and_then(|e| e.get(name)).and_then(text_of))
    };
    let setting = |name: &str| settings.and_then(|v| v.get(name)).and_then(text_of);
    let mut invocation = calc_verdict::WitnessInvocation {
        input_transport: field("excel_input_plumbing"),
        input_sheet_state: None,
        locale: field("locale_profile"),
        decimal_separator: setting("DecimalSeparator").or_else(|| field("decimal_separator")),
        thousands_separator: setting("ThousandsSeparator").or_else(|| field("thousands_separator")),
        date_system: settings
            .and_then(|v| v.get("Date1904"))
            .and_then(Value::as_bool)
            .map(|v| if v { "1904" } else { "1900" }.to_owned())
            .or_else(|| field("workbook_date_system"))
            .or_else(|| field("date_system")),
        compatibility_version: setting("CompatibilityVersion")
            .or_else(|| field("workbook_compatibility_version"))
            .or_else(|| field("workbook_compatibility")),
    };
    // dc-1943: all 989 FIXED calls in this particular bank were replayed on the same
    // en-ZA host, with identical results. The captured replay is retained under
    // migration/evidence/dc-1943-fixed-replay.json.gz. Other banks keep their own context.
    if source == format!("{CHECKOUT} cache/oracle-20430/build-20430/FIXED.jsonl") {
        invocation.locale.get_or_insert_with(|| "en-ZA".to_owned());
        invocation
            .decimal_separator
            .get_or_insert_with(|| ".".to_owned());
        invocation
            .thousands_separator
            .get_or_insert_with(|| " ".to_owned());
    }
    ReadingSource {
        id,
        source,
        excel_build: field("excel_build"),
        platform: field("platform").or_else(|| Some("Windows".to_owned())),
        bitness: field("excel_bitness"),
        cpu: field("cpu_id"),
        invocation: (!invocation.is_empty()).then_some(invocation),
    }
}

// A row's audit is authoritative over a file's default transport. Verify its exact
// requested, submitted and read-back scalar values, rather than trusting a repeated id.
fn audited_transport(doc: &Value, row: &Value) -> Option<String> {
    let mut matching = doc["input_audits"]
        .as_array()?
        .iter()
        .filter(|audit| audit["id"] == row["id"]);
    let audit = matching.next()?;
    if matching.next().is_some() || !audit["reading_failure"].is_null() {
        return None;
    }
    let args = row["args"].as_array()?;
    let mut requested = Vec::new();
    let mut read_back = Vec::new();
    for (i, arg) in args.iter().enumerate() {
        let value = decode_argument(arg, &format!("args[{i}]")).ok()?;
        let (request, read) = match value.core_ref() {
            calc_func::value::CoreRef::Number(number) => {
                let encoded = serde_json::json!({"number": format!("{:016x}", number.to_bits())});
                (encoded.clone(), encoded)
            }
            calc_func::value::CoreRef::Logical(value) => {
                let encoded = serde_json::json!({"logical": value});
                (encoded.clone(), encoded)
            }
            calc_func::value::CoreRef::Text(text) => (
                serde_json::json!({"text": text.to_string_lossy()}),
                serde_json::json!({"text": text.utf16_code_units().iter().map(|unit| format!("{unit:04x}")).collect::<String>()}),
            ),
            _ => return None,
        };
        requested.push(request);
        read_back.push(read);
    }
    let audit = &audit["audit"];
    if audit["requested"] != Value::Array(requested)
        || audit["submitted_value2"] != Value::Array(read_back.clone())
        || audit["read_back"] != Value::Array(read_back)
    {
        return None;
    }
    audit["transport"].as_str().map(str::to_owned)
}

fn row_environment(mut source: ReadingSource, doc: &Value, row: &Value) -> ReadingSource {
    if doc.get("input_audits").is_some() {
        source
            .invocation
            .get_or_insert_with(Default::default)
            .input_transport = audited_transport(doc, row);
    }
    // The named legacy runner and this exact modern worker create their own blank,
    // visible, unfiltered worksheet and write constants into the argument cells.
    // A transport label alone does not establish any of those workbook facts.
    static MODERN_WORKER: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    let modern = MODERN_WORKER.get_or_init(|| {
        format!(
            "witness worker sha256:{}",
            calc_oracle_excel::sha256_hex(calc_oracle_excel::WITNESS_SCRIPT.as_bytes())
        )
    });
    let provenance = &doc["capture_provenance"];
    let known_worker = provenance["runner"]["name"] == "Run-W109BulkBatch.ps1"
        && provenance["runner"]["version"] == "w109-bulk-batch-v2"
        || provenance["reader"]
            .as_str()
            .is_some_and(|reader| reader.contains(modern.as_str()));
    if known_worker
        && let Some(context) = source.invocation.as_mut()
        && context.input_transport.as_deref() == Some("cell_value2_bulk")
    {
        context.input_sheet_state = Some("visible-constant-inputs".to_owned());
    }
    if source.invocation.as_ref().is_some_and(|c| c.is_empty()) {
        source.invocation = None;
    }
    source
}

fn retain_source(
    banks: &mut Banks,
    by_context: &mut BTreeMap<String, usize>,
    path: &str,
    env: ReadingSource,
) -> usize {
    let key = format!(
        "{}|{}",
        env.environment(),
        env.invocation.as_ref().map_or(Value::Null, |c| c.to_json())
    );
    *by_context.entry(key).or_insert_with(|| {
        banks.sources.push(Source {
            path: path.to_owned(),
            env,
        });
        banks.sources.len() - 1
    })
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
                flat.chunks(cols)
                    .map(|row| Value::Array(row.to_vec()))
                    .collect(),
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
            if node.get("value")?.as_bool()? {
                "logical:True"
            } else {
                "logical:False"
            }
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
    graph_rows_linked(doc, &original)
        .then(|| (companion, doc["witnesses"].as_array().unwrap().len()))
}

fn graph_rows_linked(derived: &Value, original: &Value) -> bool {
    let (Some(rows), Some(calls)) = (
        derived["witnesses"].as_array(),
        original["witnesses"].as_array(),
    ) else {
        return false;
    };
    let readings: BTreeMap<&str, &Value> = calls
        .iter()
        .filter_map(|r| Some((r["id"].as_str()?, r.get("expected_bits")?)))
        .collect();
    !rows.is_empty()
        && rows.iter().all(|r| {
            r["id"]
                .as_str()
                .and_then(|id| readings.get(id))
                .is_some_and(|expected| r.get("expected_bits") == Some(*expected))
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
    let Some(batch) = batch_path(rel) else {
        return Vec::new();
    };
    let parsed = blob_bytes(checkout, &batch)
        .and_then(|b| gunzip(&b, None))
        .and_then(|b| bank_json(&b));
    let Ok(doc) = parsed else { return Vec::new() };
    doc["probes"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|p| p.get("probe").unwrap_or(p).clone())
        .filter(|p| p.get("result_index").is_some())
        .collect()
}

fn recovered_selection(row: &Value, probes: &[Value]) -> Option<(usize, usize)> {
    let id = row["id"].as_str()?;
    let args = row.get("args")?;
    let mut selected = None;
    for probe in probes
        .iter()
        .filter(|p| p["id"].as_str() == Some(id) && p.get("args") == Some(args))
    {
        let index = calc_verdict::result_index(probe.as_object()?)?;
        if selected.is_some_and(|previous| previous != index) {
            return None;
        }
        selected = Some(index);
    }
    selected
}

/// GROWTH/TREND return the shape of new_x. A flat legacy vector loses its
/// orientation, but a batch selecting several rows in one column supplies it.
/// Require a single predictor and consistent selections; never inspect answers.
fn prediction_column_inputs(function: &str, probes: &[Value]) -> BTreeSet<String> {
    if !matches!(function, "GROWTH" | "TREND") {
        return BTreeSet::new();
    }
    let mut layouts: BTreeMap<String, (bool, bool)> = BTreeMap::new();
    for probe in probes {
        let Some(args) = probe["args"].as_array() else {
            continue;
        };
        let flat = |i: usize| {
            args.get(i)
                .and_then(Value::as_array)
                .filter(|cells| !cells.is_empty() && cells.iter().all(|v| !v.is_array()))
        };
        let (Some(y), Some(x), Some(new_x)) = (flat(0), flat(1), flat(2)) else {
            continue;
        };
        if y.len() < 2 || y.len() != x.len() || new_x.len() < 2 {
            continue;
        }
        let Some((row, col)) = probe.as_object().and_then(calc_verdict::result_index) else {
            continue;
        };
        let flags = layouts
            .entry(Value::Array(args.clone()).to_string())
            .or_default();
        flags.0 |= row > 1 && row <= new_x.len() && col == 1;
        flags.1 |= col != 1 || row == 0 || row > new_x.len();
    }
    layouts
        .into_iter()
        .filter_map(|(args, (column, conflict))| (column && !conflict).then_some(args))
        .collect()
}

fn restore_prediction_column(args: &mut Value, columns: &BTreeSet<String>) -> bool {
    if !columns.contains(&args.to_string()) {
        return false;
    }
    let Some(cells) = args[2].as_array() else {
        return false;
    };
    args[2] = Value::Array(
        cells
            .iter()
            .map(|cell| Value::Array(vec![cell.clone()]))
            .collect(),
    );
    true
}

fn array_replay_args(case: &Value, function: &str) -> Result<Vec<Value>, String> {
    let id = case["id"].as_str().ok_or("array replay has no id")?;
    let inputs = case["inputs"]
        .as_array()
        .ok_or("array replay has no input readback")?;
    let expected_inputs = if function == "TEXTSPLIT" { 2 } else { 16 };
    if inputs.len() < expected_inputs {
        return Err(format!("{id}: insufficient input readbacks"));
    }
    let mut input = Vec::new();
    for (i, cell) in inputs[..expected_inputs].iter().enumerate() {
        let address = if function == "TEXTSPLIT" {
            format!("A{}", i + 1)
        } else {
            format!("{}{}", (b'A' + (i % 4) as u8) as char, i / 4 + 1)
        };
        if cell["a"] != address {
            return Err(format!("{id}: unexpected input address"));
        }
        let value = &cell["read_back"];
        input.push(match value["type"].as_str() {
            Some("text") => {
                Value::String(value["text"].as_str().ok_or("text readback")?.to_owned())
            }
            Some("number") => Value::String(format!(
                "0x{}",
                value["bits"].as_str().ok_or("number readback")?
            )),
            _ => return Err(format!("{id}: unsupported input readback")),
        });
    }
    Ok(if function == "TEXTSPLIT" {
        input
    } else {
        vec![Value::Array(
            input
                .chunks(4)
                .map(|row| Value::Array(row.to_vec()))
                .collect(),
        )]
    })
}

/// The free-layout replays retain full arrays. Add each observed cell through the same
/// result-selection path as other witnesses, with layout in the source identity.
fn add_array_replay(
    banks: &mut Banks,
    function: &str,
    path: &str,
    doc: &Value,
) -> Result<(), String> {
    let cases = doc["cases"].as_array().ok_or("array replay has no cases")?;
    let selected: Vec<_> = cases
        .iter()
        .filter(|case| {
            function == "TEXTSPLIT"
                || case["id"]
                    .as_str()
                    .is_some_and(|id| id.starts_with("MINVERSE|m4x4:"))
        })
        .collect();
    let expected_cases = if function == "TEXTSPLIT" { 2 } else { 28 };
    if selected.len() != expected_cases {
        return Err(format!("{path}: expected {expected_cases} cases"));
    }
    let source = banks.sources.len();
    banks.sources.push(Source {
        path: path.to_owned(),
        env: ReadingSource {
            id: String::new(),
            source: format!("DnaCalc {path}"),
            excel_build: Some("20527".into()),
            platform: Some("Windows".into()),
            bitness: None,
            cpu: doc["cpu"].as_str().map(str::to_owned),
            invocation: Some(calc_verdict::WitnessInvocation {
                input_transport: Some("recaptured_free_layout".into()),
                ..Default::default()
            }),
        },
    });
    banks.files_read += 1;
    for case in selected {
        let id = case["id"].as_str().ok_or("array replay has no id")?;
        let args = array_replay_args(case, function)?;
        let formula = case["formulas"]
            .as_array()
            .and_then(|f| f.first())
            .ok_or("array replay has no formula")?;
        let (height, width) = if function == "TEXTSPLIT" {
            (1, 3)
        } else {
            (4, 4)
        };
        if formula["rows"].as_u64() != Some(height) || formula["cols"].as_u64() != Some(width) {
            return Err(format!("{id}: unexpected array shape"));
        }
        let cells = formula["cells"]
            .as_array()
            .ok_or("array replay has no cells")?;
        if cells.len() != (height * width) as usize {
            return Err(format!("{id}: incomplete array"));
        }
        let mut positions = BTreeSet::new();
        for cell in cells {
            let r = cell["r"].as_u64().ok_or("array cell row")? as usize;
            let c = cell["c"].as_u64().ok_or("array cell column")? as usize;
            if r == 0 || r > height as usize || c == 0 || c > width as usize {
                return Err(format!("{id}: invalid cell"));
            }
            if !positions.insert((r, c)) {
                return Err(format!("{id}: repeated cell"));
            }
            let expected = match cell["type"].as_str() {
                Some("text") => format!("text:{}", cell["text"].as_str().ok_or("array text")?),
                Some("number") => format!("0x{}", cell["bits"].as_str().ok_or("array number")?),
                _ => return Err(format!("{id}: unsupported array cell")),
            };
            banks.readings.push(Reading {
                function: function.to_owned(),
                id: format!("{id}@{r},{c}"),
                args: args.clone(),
                index: Some((r, c)),
                expected,
                source,
            });
        }
    }
    Ok(())
}

fn recovered_minverse_spill(
    function: &str,
    path: &str,
    record: &Value,
    inputs: &BTreeMap<&str, Value>,
) -> bool {
    let Some((matrix, _)) = record["key"].as_str().and_then(|key| key.rsplit_once("|@")) else {
        return false;
    };
    function == "MINVERSE"
        && path == "cache/oracle/build-20131/MINVERSE.jsonl"
        && inputs.get(matrix).is_some_and(|expected| {
            record["args"][0][0]
                .as_object()
                .and_then(|_| oracle_argument(&record["args"][0][0]))
                .as_ref()
                == Some(expected)
        })
        && record["outcome"]["code"] == "ExcelErrorType9"
        && record["result_index"][0].as_u64().is_some_and(|r| r < 4)
        && record["result_index"][1] == 0
}

/// Retain all Q6 calls, including repeated observations. Grouping later combines identical
/// inputs, while the original capture still carries each call and its Excel answer.
fn add_binomial_replay(banks: &mut Banks, function: &str, doc: &Value) -> Result<(), String> {
    let source = banks.sources.len();
    banks.sources.push(Source {
        path: BINOM_REPLAY.to_owned(),
        env: ReadingSource {
            id: String::new(),
            source: format!("DnaCalc {BINOM_REPLAY}"),
            excel_build: Some("20527".into()),
            platform: Some("Windows".into()),
            bitness: None,
            cpu: doc["cpu"].as_str().map(str::to_owned),
            invocation: Some(calc_verdict::WitnessInvocation {
                input_transport: Some("cell_value2_bulk".into()),
                ..Default::default()
            }),
        },
    });
    banks.files_read += 1;
    for (section, count) in [("alpha_neighbours", 54), ("ulp_scan_plus_minus_4", 135)] {
        let rows = doc[section]
            .as_array()
            .ok_or("binomial replay has no section")?;
        if rows.len() != count {
            return Err(format!("{section}: expected {count} calls"));
        }
        for (i, row) in rows.iter().enumerate() {
            let n = row["n"].as_f64().ok_or("binomial trials")?;
            let p = row["p"].as_f64().ok_or("binomial probability")?;
            let alpha = row["alpha_bits"].as_str().ok_or("binomial alpha bits")?;
            if alpha.len() != 16 || u64::from_str_radix(alpha, 16).is_err() {
                return Err("invalid alpha bits".into());
            }
            let args = vec![
                serde_json::json!(format!("0x{:016x}", n.to_bits())),
                serde_json::json!(format!("0x{:016x}", p.to_bits())),
                serde_json::json!(format!("0x{alpha}")),
            ];
            if let Some(readback) = row.get("input_bits_read_back") {
                let expected = serde_json::json!([
                    format!("{:016x}", n.to_bits()),
                    format!("{:016x}", p.to_bits()),
                    alpha
                ]);
                if readback != &expected {
                    return Err(format!("{section} {i}: input readback differs"));
                }
            }
            let result = |name: &str| -> Option<u64> {
                let value = &row[name];
                value
                    .as_str()
                    .and_then(|s| u64::from_str_radix(s, 16).ok())
                    .or_else(|| value.as_f64().map(f64::to_bits))
            };
            let answer = result(function).ok_or("binomial replay has no numeric answer")?;
            if result("BINOM.INV") != result("CRITBINOM") {
                return Err(format!("{section} {i}: functions disagree"));
            }
            banks.readings.push(Reading {
                function: function.to_owned(),
                id: format!("q6-{section}-{i:03}"),
                args,
                index: None,
                expected: format!("0x{answer:016x}"),
                source,
            });
        }
    }
    Ok(())
}

/// Replace only the obstructed scalar-layout observations with independently captured
/// free-layout observations of the same calls. The original evidence stays in the bank.
fn unique_recaptures(doc: &Value) -> Result<BTreeMap<String, (Vec<Value>, String, Value)>, String> {
    let fail = || "invalid UNIQUE array recapture".to_owned();
    let ids = doc["original_ids"].as_array().ok_or_else(fail)?;
    let requests = doc["requests"].as_array().ok_or_else(fail)?;
    let replies = doc["replies"].as_array().ok_or_else(fail)?;
    if ids.len() != 1024 || requests.len() != ids.len() {
        return Err(fail());
    }
    let mut out = BTreeMap::new();
    let mut i = 0;
    for reply in replies {
        let rows = reply["rows"].as_array().ok_or_else(fail)?;
        let inputs = reply["inputs"].as_array().ok_or_else(fail)?;
        if reply["ok"] != true || rows.len() != inputs.len() || reply["excel_build"] != "20527" {
            return Err(fail());
        }
        for (answer, input) in rows.iter().zip(inputs) {
            let request = requests.get(i).ok_or_else(fail)?;
            if request != input
                || answer["type"] != "array"
                || answer["rows"] != 2
                || answer["cols"] != 2
            {
                return Err(fail());
            }
            let matrix = request
                .as_array()
                .and_then(|a| (a.len() == 1).then(|| &a[0]))
                .ok_or_else(fail)?;
            if matrix["type"] != "array" {
                return Err(fail());
            }
            let cells = matrix["values"].as_array().ok_or_else(fail)?;
            if cells.len() != 2
                || cells
                    .iter()
                    .any(|r| r.as_array().is_none_or(|r| r.len() != 2))
            {
                return Err(fail());
            }
            let mut args = Vec::new();
            for row in cells {
                let cells = row.as_array().ok_or_else(fail)?;
                let mut converted = Vec::new();
                for cell in cells {
                    if cell["type"] != "number" {
                        return Err(fail());
                    }
                    let bits = cell["bits"]
                        .as_str()
                        .filter(|b| b.len() == 16 && b.bytes().all(|c| c.is_ascii_hexdigit()))
                        .ok_or_else(fail)?;
                    converted.push(Value::String(format!("0x{bits}")));
                }
                args.push(Value::Array(converted));
            }
            let values = answer["values"].as_array().ok_or_else(fail)?;
            if values.len() != 2
                || values
                    .iter()
                    .any(|r| r.as_array().is_none_or(|r| r.len() != 2))
            {
                return Err(fail());
            }
            for cell in values.iter().flat_map(|row| row.as_array().unwrap()) {
                if cell["type"] != "number"
                    || cell["bits"].as_str().is_none_or(|bits| {
                        bits.len() != 16 || !bits.bytes().all(|c| c.is_ascii_hexdigit())
                    })
                {
                    return Err(fail());
                }
            }
            let cell = &values[0][0];
            let bits = cell["bits"]
                .as_str()
                .filter(|b| b.len() == 16 && b.bytes().all(|c| c.is_ascii_hexdigit()))
                .ok_or_else(fail)?;
            let id = ids.get(i).and_then(Value::as_str).ok_or_else(fail)?;
            let provenance = serde_json::json!({
                "excel_build": reply["excel_build"],
                "calculation_settings": reply["calculation_settings"],
                "env_id": reply["env_id"],
                "timestamp": reply["timestamp"],
            });
            if out
                .insert(
                    id.to_owned(),
                    (vec![Value::Array(args)], format!("0x{bits}"), provenance),
                )
                .is_some()
            {
                return Err(fail());
            }
            i += 1;
        }
    }
    if i != ids.len() {
        return Err(fail());
    }
    Ok(out)
}

fn load_full(checkout: &Path, only: Option<&str>) -> Result<(Banks, Vec<String>), String> {
    let mut banks = Banks {
        sources: Vec::new(),
        readings: Vec::new(),
        files_read: 0,
        import_notes: Vec::new(),
        unread_rows: BTreeMap::new(),
    };
    let inverse_replay = if only.is_none_or(|f| f == "MINVERSE") {
        Some(bank_json(&gunzip(
            include_bytes!("../../../migration/evidence/dc-1943-minverse-replay.json.gz"),
            None,
        )?)?)
    } else {
        None
    };
    let mut inverse_inputs = BTreeMap::new();
    if let Some(doc) = &inverse_replay {
        for case in doc["cases"]
            .as_array()
            .ok_or("MINVERSE replay has no cases")?
        {
            let Some(id) = case["id"]
                .as_str()
                .filter(|id| id.starts_with("MINVERSE|m4x4:"))
            else {
                continue;
            };
            let args = array_replay_args(case, "MINVERSE")?;
            if inverse_inputs.insert(id, args[0].clone()).is_some() {
                return Err(format!("{id}: repeated matrix"));
            }
        }
        if inverse_inputs.len() != 28 {
            return Err("MINVERSE replay must hold 28 matrices".to_owned());
        }
    }
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
        let oracle =
            rel.starts_with("cache/oracle") && rel.contains("/build-") && name.ends_with(".jsonl");
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
                    *banks
                        .unread_rows
                        .entry(function.to_ascii_uppercase())
                        .or_default() += 1;
                    continue;
                };
                let call = record["args"]
                    .as_array()
                    .and_then(|a| a.first())
                    .and_then(Value::as_array);
                let args: Option<Vec<Value>> = call
                    .map(|c| c.iter().map(oracle_argument).collect())
                    .flatten();
                let expected = oracle_outcome(&record["outcome"]).or_else(|| {
                    recovered_minverse_spill(function, rel, &record, &inverse_inputs)
                        .then(|| "error:Spill".to_owned())
                });
                let (Some(args), Some(expected)) = (args, expected) else {
                    unread.push(format!(
                        "{rel}:{}: an argument or outcome kind with no witness form",
                        line_no + 1
                    ));
                    *banks
                        .unread_rows
                        .entry(function.to_ascii_uppercase())
                        .or_default() += 1;
                    continue;
                };
                let env = record.get("environment");
                let source = retain_source(
                    &mut banks,
                    &mut source_of,
                    rel,
                    environment(
                        String::new(),
                        format!("{CHECKOUT} {rel}"),
                        env,
                        fallback.as_ref(),
                        record.get("calculation_settings"),
                    ),
                );
                let index = record["result_index"]
                    .as_array()
                    .and_then(|p| match p.as_slice() {
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
        let (Some(function), Some(rows)) = (doc["function"].as_str(), doc["witnesses"].as_array())
        else {
            continue;
        };
        let function = function.to_ascii_uppercase();
        if only.is_some_and(|f| f != function)
            || rows
                .first()
                .is_none_or(|r| r.get("expected_bits").is_none())
        {
            continue;
        }
        if let Some((companion, count)) = derived_graph_companion(checkout, rel, &doc) {
            banks.import_notes.push(format!("{rel}: {count} derived graph rows are copies of worksheet readings in {companion}; not counted as additional calls"));
            continue;
        }
        let probes = batch_selections(checkout, rel);
        let columns = prediction_column_inputs(&function, &probes);
        let (mut selections, mut hex_repairs, mut column_repairs) = (0, 0, 0);
        banks.files_read += 1;
        let env = doc
            .get("capture_provenance")
            .and_then(|p| p.get("environment"));
        let captured = environment(
            String::new(),
            format!("{CHECKOUT} {rel}"),
            env,
            None,
            doc.get("capture_provenance")
                .and_then(|p| p.get("calculation_settings")),
        );
        let mut source_of = BTreeMap::new();
        record_unread_rows(&doc, &rel, &function, &mut banks, &mut unread)?;
        for (i, row) in rows.iter().enumerate() {
            let (Some(args), Some(expected)) =
                (row["args"].as_array(), row["expected_bits"].as_str())
            else {
                unread.push(format!("{rel}: row {i} has no args or expected_bits"));
                *banks.unread_rows.entry(function.clone()).or_default() += 1;
                continue;
            };
            let index = row
                .as_object()
                .and_then(calc_verdict::result_index)
                .or_else(|| {
                    let selected = recovered_selection(row, &probes);
                    selections += usize::from(selected.is_some());
                    selected
                });
            let mut args = Value::Array(args.clone());
            column_repairs += usize::from(restore_prediction_column(&mut args, &columns));
            normalize_bank_hex(&mut args);
            let mut normalized_expected = Value::String(expected.to_owned());
            normalize_bank_hex(&mut normalized_expected);
            let mut original_args = row["args"].clone();
            normalize_bank_hex(&mut original_args);
            hex_repairs += usize::from(
                original_args != row["args"] || normalized_expected != row["expected_bits"],
            );
            let source = retain_source(
                &mut banks,
                &mut source_of,
                &rel,
                row_environment(captured.clone(), &doc, row),
            );
            banks.readings.push(Reading {
                function: function.clone(),
                id: row["id"]
                    .as_str()
                    .map_or_else(|| format!("row{i}"), str::to_owned),
                args: args.as_array().unwrap().clone(),
                index,
                expected: normalized_expected.as_str().unwrap().to_owned(),
                source,
            });
        }
        if selections > 0 || hex_repairs > 0 || column_repairs > 0 {
            banks.import_notes.push(format!("{rel}: recovered {selections} result selections and {column_repairs} prediction-column layouts from batch probes; normalized duplicated hex prefixes in {hex_repairs} rows"));
        }
        if selections > 0 || column_repairs > 0 {
            for source in source_of.values() {
                banks.sources[*source].env.source.push_str(&format!(
                    "; result selection/layout: {CHECKOUT} {}",
                    batch_path(rel).unwrap()
                ));
            }
        }
    }
    let mut unique = if only.is_none_or(|f| f == "UNIQUE") {
        let bytes = gunzip(
            include_bytes!("../../../migration/evidence/dc-ymjf-2-10-unique-recapture.json.gz"),
            None,
        )?;
        unique_recaptures(&bank_json(&bytes)?)?
    } else {
        BTreeMap::new()
    };
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
        let (Some(function), Some(rows)) = (doc["function"].as_str(), doc["witnesses"].as_array())
        else {
            continue;
        };
        let function = function.to_ascii_uppercase();
        if only.is_some_and(|f| f != function) {
            continue;
        }
        banks.files_read += 1;
        let env = doc
            .get("capture_provenance")
            .and_then(|p| p.get("environment"));
        let captured = environment(
            String::new(),
            format!("{CHECKOUT} {rel}"),
            env,
            None,
            doc.get("capture_provenance")
                .and_then(|p| p.get("calculation_settings")),
        );
        let mut source_of = BTreeMap::new();
        let mut recapture_source_of = BTreeMap::new();
        record_unread_rows(&doc, &rel, &function, &mut banks, &mut unread)?;
        record_transport_audits(&doc, &rel, &mut banks)?;
        for (i, row) in rows.iter().enumerate() {
            let (Some(args), Some(expected)) =
                (row["args"].as_array(), row["expected_bits"].as_str())
            else {
                unread.push(format!("{rel}: row {i} has no args or expected_bits"));
                *banks.unread_rows.entry(function.clone()).or_default() += 1;
                continue;
            };
            let id = row["id"]
                .as_str()
                .map_or_else(|| format!("row{i}"), str::to_owned);
            let index = row.as_object().and_then(calc_verdict::result_index);
            let mut selected = expected.to_owned();
            let mut recaptured = false;
            let mut selected_env = row_environment(captured.clone(), &doc, row);
            if function == "UNIQUE" && rel == UNIQUE_SCALAR_CAPTURE {
                if let Some((observed_args, observed, reply)) = unique.remove(&id) {
                    if observed_args != *args || expected != "error:Spill" || index != Some((1, 1))
                    {
                        return Err(format!("{rel}: recapture identity differs for {id}"));
                    }
                    selected = observed;
                    recaptured = true;
                    let env = serde_json::json!({"excel_build": reply["excel_build"], "platform": "Windows", "excel_input_plumbing": "cell_value2_bulk"});
                    selected_env = environment(
                        String::new(),
                        format!(
                            "DnaCalc {UNIQUE_RECAPTURE} env {}; supersedes obstructed scalar capture {CHECKOUT} {rel}",
                            reply["env_id"].as_str().unwrap_or("unknown")
                        ),
                        Some(&env),
                        None,
                        reply.get("calculation_settings"),
                    );
                }
            }
            let source = if recaptured {
                // Different service environment IDs can share the build and settings.
                *recapture_source_of
                    .entry(selected_env.source.clone())
                    .or_insert_with(|| {
                        banks.sources.push(Source {
                            path: UNIQUE_RECAPTURE.to_owned(),
                            env: selected_env,
                        });
                        banks.sources.len() - 1
                    })
            } else {
                retain_source(&mut banks, &mut source_of, &rel, selected_env)
            };
            banks.readings.push(Reading {
                function: function.clone(),
                id,
                args: args.clone(),
                index,
                expected: selected,
                source,
            });
        }
    }
    if !unique.is_empty() {
        return Err(format!(
            "{UNIQUE_RECAPTURE}: {} original rows not found",
            unique.len()
        ));
    }
    if only.is_none_or(|f| f == "TEXTSPLIT") {
        let doc = bank_json(&gunzip(
            include_bytes!("../../../migration/evidence/dc-1943-textsplit-replay.json.gz"),
            None,
        )?)?;
        add_array_replay(&mut banks, "TEXTSPLIT", TEXTSPLIT_REPLAY, &doc)?;
    }
    if let Some(doc) = &inverse_replay {
        add_array_replay(&mut banks, "MINVERSE", MINVERSE_REPLAY, doc)?;
    }
    for function in ["BINOM.INV", "CRITBINOM"] {
        if only.is_none_or(|f| f == function) {
            let doc = bank_json(&gunzip(
                include_bytes!("../../../migration/evidence/dc-1943-binom-replay.json.gz"),
                None,
            )?)?;
            add_binomial_replay(&mut banks, function, &doc)?;
        }
    }
    Ok((banks, unread))
}

/// Every `.json` file under `dir`, sorted.
fn plain_readings(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = fs::read_dir(&d) else {
            continue;
        };
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

#[path = "../../../crates/calc-func/tests/witnesses/call.rs"]
mod witness_call;

/// Resolve only syntax that is invalid regardless of the workbook and formula position. The
/// bank does not retain either, so the CalcFml binder must refuse all references needing them.
struct UnlocatedReferenceText;

impl calc_func::ReferenceText for UnlocatedReferenceText {
    fn resolve(&mut self, text: &[u16], a1: bool) -> calc_func::TextReference {
        let notation = if a1 {
            calc_fml::reference::ReferenceNotation::A1
        } else {
            calc_fml::reference::ReferenceNotation::R1C1
        };
        calc_fml::reference::bind_text_without_context(text, notation)
    }
}

fn calculate_with_context(
    function: &str,
    args: &[Value],
    index: Option<(usize, usize)>,
    context: Option<&calc_verdict::WitnessInvocation>,
    apply: bool,
) -> String {
    catch_unwind(AssertUnwindSafe(|| {
        if function.eq_ignore_ascii_case("INDIRECT") {
            let mut reference_text = UnlocatedReferenceText;
            witness_call::calculate_with_reference_text(
                function, args, index, context, apply, Some(&mut reference_text),
            )
        } else {
            witness_call::calculate(function, args, index, context, apply)
        }
    }))
    .unwrap_or_else(|_| "fault:Panic".to_owned())
}

/// How long one row may calculate before it is given up as `fault:Timeout`.
const ROW_SECONDS: u64 = 20;
/// A row that takes longer than this (in a release build) is judged but not pinned: the portable
/// test runs unoptimised and in browsers, where it would take a hundred times as long.
const SLOW_MILLIS: u64 = 50;
const SLOW: &str = "slow:";

// These functions have bank inputs that can run indefinitely. A child runs the same
// witness invocation and can be stopped on timeout without leaving a CPU-bound thread.
fn isolated_calculation(
    function: &str,
    args: &[Value],
    index: Option<(usize, usize)>,
    context: Option<&calc_verdict::WitnessInvocation>,
    apply: bool,
) -> String {
    use std::process::Stdio;
    let result = (|| -> Result<String, String> {
        let payload = serde_json::json!({
            "function": function, "args": args, "index": index,
            "context": context.map(|c| c.to_json()), "apply": apply,
        });
        let mut child = Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
            .args(["witness", "__row_worker"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;
        let input = child.stdin.take().ok_or("missing worker stdin")?;
        if let Err(error) = serde_json::to_writer(input, &payload) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error.to_string());
        }
        let started = std::time::Instant::now();
        loop {
            let state = match child.try_wait() {
                Ok(state) => state,
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(error.to_string());
                }
            };
            match state {
                Some(status) if status.success() => {
                    let mut output = String::new();
                    child
                        .stdout
                        .take()
                        .ok_or("missing worker stdout")?
                        .read_to_string(&mut output)
                        .map_err(|e| e.to_string())?;
                    return Ok(output);
                }
                Some(_) => return Err("worker exited unsuccessfully".into()),
                None if started.elapsed() >= std::time::Duration::from_secs(ROW_SECONDS) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Ok("fault:Timeout".into());
                }
                None => std::thread::sleep(std::time::Duration::from_millis(2)),
            }
        }
    })();
    result.unwrap_or_else(|_| "fault:WitnessWorker".into())
}

fn row_worker() -> Result<(), String> {
    let payload: Value = serde_json::from_reader(std::io::stdin()).map_err(|e| e.to_string())?;
    let function = payload["function"].as_str().ok_or("missing function")?;
    let args = payload["args"].as_array().ok_or("missing args")?;
    let index = payload["index"]
        .as_array()
        .and_then(|v| match v.as_slice() {
            [r, c] => Some((r.as_u64()? as usize, c.as_u64()? as usize)),
            _ => None,
        });
    let context = payload
        .get("context")
        .filter(|v| !v.is_null())
        .map(|v| calc_verdict::WitnessInvocation::decode(v, "context"))
        .transpose()?;
    let actual = calculate_with_context(
        function,
        args,
        index,
        context.as_ref(),
        payload["apply"] == true,
    );
    std::io::stdout()
        .write_all(actual.as_bytes())
        .map_err(|e| e.to_string())
}

/// [`calculate`] for every job, on a worker thread: a row that gives no result within
/// [`ROW_SECONDS`] is `fault:Timeout`, its thread is left behind, and the rows after it go on in a
/// new thread (calc-func's IRR calculates without end on some inputs).
fn calculate_all(
    function: &str,
    jobs: &[(
        Vec<Value>,
        Option<(usize, usize)>,
        Option<std::sync::Arc<calc_verdict::WitnessInvocation>>,
    )],
    apply_context: bool,
) -> Vec<String> {
    if matches!(function, "MULTINOMIAL" | "IRR") {
        return jobs
            .iter()
            .map(|(args, index, context)| {
                let started = std::time::Instant::now();
                let result =
                    isolated_calculation(function, args, *index, context.as_deref(), apply_context);
                if started.elapsed() > std::time::Duration::from_millis(SLOW_MILLIS) {
                    format!("{SLOW}{result}")
                } else {
                    result
                }
            })
            .collect();
    }
    use std::sync::mpsc::{RecvTimeoutError, channel};
    let mut out = Vec::with_capacity(jobs.len());
    while out.len() < jobs.len() {
        let (tx, rx) = channel();
        let name = function.to_owned();
        let todo = jobs[out.len()..].to_vec();
        std::thread::spawn(move || {
            for (args, index, context) in todo {
                let started = std::time::Instant::now();
                let result =
                    calculate_with_context(&name, &args, index, context.as_deref(), apply_context);
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

fn write_trace_row(writer: &mut impl Write, row: &Row<'_>, source: &Source) -> Result<(), String> {
    let r = row.reading;
    let mut value = serde_json::json!({
        "function": r.function,
        "id": r.id,
        "args": r.args,
        "result_index": r.index,
        "expected": r.expected,
        "actual": row.actual,
        "source": source.path,
        "reading": {
            "excel_build": source.env.excel_build,
            "platform": source.env.platform,
            "bitness": source.env.bitness,
            "cpu": source.env.cpu,
        },
    });
    if let Some(context) = &source.env.invocation {
        value["reading"]["invocation"] = context.to_json();
    }
    if let Some(reason) = row.actual.strip_prefix("excluded:") {
        let evidence = match r.function.as_str() {
            "WEEKNUM" => WEEKNUM_EXCLUSION,
            "TEXTSPLIT" => TEXTSPLIT_REPLAY,
            "MINVERSE" => MINVERSE_REPLAY,
            _ => "",
        };
        value["exclusion"] = serde_json::json!({"reason": reason, "evidence": evidence});
    }
    serde_json::to_writer(&mut *writer, &value).map_err(|e| format!("witness trace: {e}"))?;
    writer
        .write_all(b"\n")
        .map_err(|e| format!("witness trace: {e}"))
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

/// Q3's serial calls show that these return types depend on a preceding WEEKNUM call.
/// The bank does not retain that evaluation order, so a single recorded value cannot
/// judge a stateless invocation. Keep the row in the failure denominator and report it.
fn order_dependent_weeknum(reading: &Reading) -> bool {
    if reading.function != "WEEKNUM" {
        return false;
    }
    let Some(value) = reading
        .args
        .get(1)
        .and_then(|arg| decode_argument(arg, "return_type").ok())
        .and_then(|arg| arg.as_number())
    else {
        return false;
    };
    if !value.is_finite() {
        return false;
    }
    let upper = value.ceil();
    let tolerance = if upper > 0.0 {
        2.0_f64.powi(-22) + 2.0_f64.powi(-33)
    } else {
        2.0_f64.powi(-23) + 2.0_f64.powi(-34)
    };
    let kind = if upper - value <= tolerance {
        upper
    } else {
        value.floor()
    };
    !(0.0..22.0).contains(&kind)
}

fn exclusion_reason(reading: &Reading, source: &Source) -> Option<&'static str> {
    if order_dependent_weeknum(reading) {
        return Some("order-dependent");
    }
    if reading.function == "TEXTSPLIT"
        && source.path == "readings/excel-20527/TEXTSPLIT.json"
        && matches!(
            reading.id.as_str(),
            "fn1-textsplit-004" | "fn1-textsplit-005"
        )
        && reading.expected == "error:Spill"
    {
        return Some("spill-layout");
    }
    if reading.function == "MINVERSE"
        && source.path == "cache/oracle/build-20131/MINVERSE.jsonl"
        && reading.expected == "error:Spill"
        && reading.index.is_some_and(|(_, c)| c == 0)
    {
        return Some("spill-layout");
    }
    None
}

fn group<'a>(banks: &'a Banks) -> (BTreeMap<String, Vec<Vec<&'a Reading>>>, Vec<Difference>) {
    // function -> key -> readings
    let mut by_key: BTreeMap<String, BTreeMap<String, Vec<&Reading>>> = BTreeMap::new();
    for r in &banks.readings {
        by_key
            .entry(r.function.clone())
            .or_default()
            .entry(contextual_key(
                &r.function,
                &r.args,
                r.index,
                banks.sources[r.source].env.invocation.as_ref(),
            ))
            .or_default()
            .push(r);
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
                    .filter(|r| {
                        seen.insert((
                            r.expected.clone(),
                            banks.sources[r.source].env.environment(),
                        ))
                    })
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

/// Preserve cross-context replay observations as explicit pairs. They do not enter the
/// selection groups: the replay recorded a new workbook context, not the original one.
fn replay_difference(
    record: &Value,
    original: &Reading,
    source: &Source,
) -> Result<Difference, String> {
    let bank = &record["bank"];
    let fail = || format!("invalid replay pair for {} {}", source.path, original.id);
    if record["category"] != "different"
        || bank["function"] != original.function
        || bank["source"] != source.path
        || bank["id"] != original.id
        || bank["args"] != Value::Array(original.args.clone())
        || bank["expected"] != original.expected
        || !bank["result_index"].is_null()
        || original.index.is_some()
        || bank["build"].as_str() != source.env.excel_build.as_deref()
        || bank["cpu"].as_str() != source.env.cpu.as_deref()
        || bank["invocation"]
            != source
                .env
                .invocation
                .as_ref()
                .map_or(Value::Null, |v| v.to_json())
        || !bank["exclusion"].is_null()
    {
        return Err(fail());
    }
    let inputs = record["inputs"].as_array().ok_or_else(fail)?;
    if inputs.len() != original.args.len()
        || inputs.iter().zip(&original.args).any(|(seen, requested)| {
            seen["type"] != "number"
                || seen["bits"]
                    .as_str()
                    .zip(requested.as_str())
                    .is_none_or(|(bits, request)| request.strip_prefix("0x") != Some(bits))
        })
    {
        return Err(fail());
    }
    let observed = &record["observed"];
    let result = oracle_outcome(&serde_json::json!({"kind": observed["type"], "code": observed["code"], "bits_hex": observed["bits"]}))
        .ok_or_else(fail)?;
    let env = &record["environment"];
    if result == original.expected
        || env["engine"] != "excel"
        || env["build"] != "20527"
        || !env["os"]
            .as_str()
            .is_some_and(|s| s.starts_with("Microsoft Windows"))
        || env["cpu"].as_str().is_none()
        || env["date1904"].as_bool().is_none()
        || env["compatibility_version"].as_u64().is_none()
    {
        return Err(fail());
    }
    let current_context = serde_json::json!({
        "input_transport": "cell_value2_bulk", "date1904": env["date1904"],
        "compatibility_version": env["compatibility_version"],
        "decimal_separator": env["decimal_separator"], "thousands_separator": env["thousands_separator"],
    });
    Ok(Difference {
        function: original.function.clone(),
        key: contextual_key(
            &original.function,
            &original.args,
            original.index,
            source.env.invocation.as_ref(),
        ),
        readings: vec![
            (
                original.expected.clone(),
                source.env.environment(),
                format!("{}#{}", source.path, original.id),
            ),
            (
                result,
                format!(
                    "20527/Windows/?/{} context={current_context}",
                    env["cpu"].as_str().unwrap()
                ),
                format!(
                    "migration/evidence/dc-ymjf3-20527-date-differences.jsonl.gz#{}:{}",
                    source.path, original.id
                ),
            ),
        ],
        across_environments: true,
    })
}

// A bank can retain several observations of one input and invocation. Keep each source
// while reporting that input once in the existing difference index.
fn coalesce_replay_differences(pairs: Vec<Difference>) -> Vec<Difference> {
    let mut by_key: BTreeMap<String, Difference> = BTreeMap::new();
    for mut pair in pairs {
        match by_key.entry(pair.key.clone()) {
            std::collections::btree_map::Entry::Vacant(slot) => {
                slot.insert(pair);
            }
            std::collections::btree_map::Entry::Occupied(mut slot) => {
                slot.get_mut().readings.append(&mut pair.readings);
            }
        }
    }
    by_key.into_values().collect()
}

fn replay_differences(banks: &Banks) -> Result<Vec<Difference>, String> {
    let bytes = gunzip(
        include_bytes!("../../../migration/evidence/dc-ymjf3-20527-date-differences.jsonl.gz"),
        None,
    )?;
    let mut originals = BTreeMap::new();
    for reading in banks.readings.iter().filter(|r| r.function == "DATE") {
        let source = &banks.sources[reading.source];
        if originals
            .insert((source.path.as_str(), reading.id.as_str()), reading)
            .is_some()
        {
            return Err(format!(
                "duplicate original reading {} {}",
                source.path, reading.id
            ));
        }
    }
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for line in bytes.split(|b| *b == b'\n').filter(|line| !line.is_empty()) {
        let row = bank_json(line)?;
        let source = row["bank"]["source"]
            .as_str()
            .ok_or("replay source missing")?;
        let id = row["bank"]["id"].as_str().ok_or("replay ID missing")?;
        if !seen.insert((source.to_owned(), id.to_owned())) {
            return Err(format!("duplicate replay {source} {id}"));
        }
        let original = originals
            .get(&(source, id))
            .ok_or_else(|| format!("replay original missing: {source} {id}"))?;
        out.push(replay_difference(
            &row,
            original,
            &banks.sources[original.source],
        )?);
    }
    Ok(coalesce_replay_differences(out))
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
    let dirty = !git(&[
        "status",
        "--porcelain",
        "--",
        "crates/calc-func",
        "crates/calc-value",
    ])
    .is_empty();
    if dirty {
        format!("{head}+changes")
    } else {
        head
    }
}

const STATUS_HEADER: &str =
    "function\tbank\texcel_builds\trows\tmatched\tworst_ulp\tclass\tcalc_func_commit\texcluded";

struct Status {
    function: String,
    builds: String,
    rows: usize,
    matched: usize,
    excluded: usize,
    worst_ulp: Option<u64>,
    class: Severity,
}

fn status_class(status: &Status) -> &'static str {
    if status.excluded > 0 && status.matched + status.excluded == status.rows {
        "excluded"
    } else {
        status.class.name()
    }
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
        matched: rows
            .iter()
            .filter(|r| r.severity == Severity::Exact)
            .count(),
        excluded: rows
            .iter()
            .filter(|r| r.actual.starts_with("excluded:"))
            .count(),
        worst_ulp: rows.iter().filter_map(|r| r.ulp).max(),
        class: rows
            .iter()
            .map(|r| r.severity)
            .max()
            .unwrap_or(Severity::Exact),
    }
}

fn write_status(
    root: &Path,
    bank: &str,
    statuses: &[Status],
    only: Option<&str>,
) -> Result<(), String> {
    let path = root.join(STATUS);
    let commit = calc_func_commit(root);
    let mut lines: BTreeMap<(String, String), String> = BTreeMap::new();
    if let Ok(text) = fs::read_to_string(&path) {
        for line in text.lines().skip(1) {
            let mut cols = line.split('\t');
            let (Some(f), Some(b)) = (cols.next(), cols.next()) else {
                continue;
            };
            let replaced = b == bank && only.is_none_or(|o| o == f);
            if !replaced {
                let line = if line.split('\t').count() == 8 {
                    format!("{line}\t0")
                } else {
                    line.to_owned()
                };
                lines.insert((f.to_owned(), b.to_owned()), line);
            }
        }
    }
    for s in statuses {
        let line = format!(
            "{}\t{bank}\t{}\t{}\t{}\t{}\t{}\t{commit}\t{}",
            s.function,
            if s.builds.is_empty() { "?" } else { &s.builds },
            s.rows,
            s.matched,
            s.worst_ulp.map_or("-".to_owned(), |u| u.to_string()),
            status_class(s),
            s.excluded
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

fn write_differences(
    root: &Path,
    differences: &[Difference],
    only: Option<&str>,
) -> Result<(), String> {
    let path = root.join(DIFFERENCES);
    let mut kept: Vec<String> = Vec::new();
    if let (Some(only), Ok(text)) = (only, fs::read_to_string(&path)) {
        kept.extend(
            text.lines()
                .skip(1)
                .filter(|l| !l.starts_with(&format!("{only}\t")))
                .map(str::to_owned),
        );
    }
    for d in differences {
        let readings: Vec<String> = d
            .readings
            .iter()
            .map(|(e, env, src)| format!("{e} on {env} ({src})"))
            .collect();
        kept.push(format!(
            "{}\t{}\t{}\t{}",
            d.function,
            if d.across_environments {
                "version_or_platform"
            } else {
                "same_environment"
            },
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
        chosen = (0..budget)
            .map(|i| chosen[(i as f64 * stride) as usize])
            .collect();
    }
    chosen
}

fn write_pinned(
    root: &Path,
    banks: &Banks,
    judged: &BTreeMap<String, Vec<Row>>,
    budget: usize,
) -> Result<(usize, Vec<String>), String> {
    let dir = root.join(PINNED_DIR);
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut bytes = 0;
    let mut panics = Vec::new();
    for (function, rows) in judged {
        let usable: Vec<Row> = rows
            .iter()
            .filter(|r| {
                if r.actual == "fault:Panic" || r.actual == "fault:Timeout" || r.slow {
                    panics.push(format!(
                        "{function} {}",
                        Value::Array(r.reading.args.clone())
                    ));
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
        for r in chosen.iter() {
            let source = &banks.sources[r.reading.source];
            let id = match readings.iter().position(|s| {
                s.source == source.env.source
                    && s.environment() == source.env.environment()
                    && s.invocation == source.env.invocation
            }) {
                Some(at) => readings[at].id.clone(),
                None => {
                    let mut env = source.env.clone();
                    env.id = format!("r{}", readings.len());
                    readings.push(env);
                    readings.last().map(|e| e.id.clone()).unwrap_or_default()
                }
            };
            witnesses.push(WitnessRow {
                id: r.reading.id.clone(),
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
// Refreshing calculated predictions without selecting new pins

/// Restore only facts attested by the pinned row's original source. Neither the
/// sample nor its Excel answers is selected again. A conflicting source is an error.
fn restore_pinned_context(
    sets: &mut [(PathBuf, WitnessSet)],
    banks: &Banks,
) -> Result<usize, String> {
    // The public archive rename keeps the same source paths and recorded answers.
    // Compare that one alias without rewriting immutable witness provenance.
    let source_path = |source: &str| {
        let path = source.split(';').next().unwrap_or(source);
        path.strip_prefix("OxFunc-Evidence ")
            .map_or_else(|| path.to_owned(), |rest| format!("{CHECKOUT} {rest}"))
    };
    let key = |function: &str, source: &str, args: &[Value], index| {
        format!(
            "{}|{}",
            source_path(source),
            reading_key(function, args, index)
        )
    };
    let mut candidates: BTreeMap<String, Vec<&Reading>> = BTreeMap::new();
    for (_, set) in sets.iter() {
        for row in &set.witnesses {
            let source = set
                .readings
                .iter()
                .find(|s| s.id == row.reading)
                .ok_or("pin has no reading source")?;
            candidates
                .entry(key(
                    &set.function,
                    &source.source,
                    &row.args,
                    row.result_index,
                ))
                .or_default();
        }
    }
    for row in &banks.readings {
        let source = &banks.sources[row.source].env;
        if let Some(list) =
            candidates.get_mut(&key(&row.function, &source.source, &row.args, row.index))
        {
            list.push(row);
        }
    }
    let mut restored = 0;
    for (_, set) in sets.iter_mut() {
        let original = set.readings.clone();
        let mut assigned: BTreeMap<(String, String), String> = BTreeMap::new();
        let mut used = BTreeSet::new();
        for row in &mut set.witnesses {
            let source = original
                .iter()
                .find(|s| s.id == row.reading)
                .ok_or("pin has no original source")?;
            let list = candidates
                .get(&key(
                    &set.function,
                    &source.source,
                    &row.args,
                    row.result_index,
                ))
                .ok_or("pin is not in its source bank")?;
            let same_id = list.iter().any(|r| {
                r.id == row.id && banks.sources[r.source].env.environment() == source.environment()
            });
            let mut contexts = BTreeMap::new();
            for candidate in list.iter().filter(|r| !same_id || r.id == row.id) {
                let recorded = &banks.sources[candidate.source].env;
                if recorded.environment() != source.environment() {
                    continue;
                }
                if candidate.expected != row.expected {
                    return Err(format!(
                        "{} {}: original source answer differs from pin",
                        set.function, row.id
                    ));
                }
                contexts.insert(
                    recorded
                        .invocation
                        .as_ref()
                        .map_or(Value::Null, |c| c.to_json())
                        .to_string(),
                    recorded.invocation.clone(),
                );
            }
            if contexts.len() != 1 {
                return Err(format!(
                    "{} {}: {} possible source contexts",
                    set.function,
                    row.id,
                    contexts.len()
                ));
            }
            let (encoded, context) = contexts.into_iter().next().unwrap();
            restored += usize::from(context != source.invocation);
            let assigned_key = (source.id.clone(), encoded);
            if let Some(id) = assigned.get(&assigned_key) {
                row.reading = id.clone();
                continue;
            }
            let mut updated = source.clone();
            updated.invocation = context;
            if used.insert(source.id.clone()) {
                *set.readings.iter_mut().find(|s| s.id == source.id).unwrap() = updated.clone();
            } else {
                let mut suffix = 1;
                loop {
                    updated.id = format!("{}-context-{suffix}", source.id);
                    if !set.readings.iter().any(|s| s.id == updated.id) {
                        break;
                    }
                    suffix += 1;
                }
                set.readings.push(updated.clone());
            }
            row.reading = updated.id.clone();
            assigned.insert(assigned_key, updated.id);
        }
    }
    Ok(restored)
}

/// Compare recorded data after expanding reading ids to their source records. Only
/// invocation facts, their source-id grouping, and calculated predictions may change.
fn without_context_and_prediction(mut value: Value) -> Result<Value, String> {
    let readings = value["readings"].as_array().ok_or("missing readings")?;
    let mut sources = BTreeMap::new();
    let mut distinct = BTreeMap::new();
    for reading in readings {
        let mut source = reading.clone();
        let object = source.as_object_mut().ok_or("reading is not an object")?;
        let id = object
            .remove("id")
            .and_then(|v| v.as_str().map(str::to_owned))
            .ok_or("missing reading id")?;
        object.remove("invocation");
        sources.insert(id, source.clone());
        distinct.insert(source.to_string(), source);
    }
    value["readings"] = Value::Array(distinct.into_values().collect());
    for row in value["witnesses"]
        .as_array_mut()
        .ok_or("missing witnesses")?
    {
        row.as_object_mut()
            .ok_or("witness is not an object")?
            .remove("calc_func");
        let id = row["reading"].as_str().ok_or("missing row reading")?;
        row["reading"] = sources.get(id).ok_or("unknown row reading")?.clone();
    }
    Ok(value)
}

/// A pin that matched Excel and no longer does is refused unless `loss_class` names why: a
/// classified loss (owner ruling 2026-11-04: a structurally shown step lands with its losses
/// classified, each class a named next target). A listed row that is not such a loss is refused,
/// so the list cannot grow stale silently.
fn refresh_prediction(
    row: &mut WitnessRow,
    actual: &str,
    loss_class: Option<&str>,
) -> Result<bool, String> {
    if matches!(actual, "fault:Panic" | "fault:Timeout") {
        return Err(format!("cannot refresh {}: {actual}", row.id));
    }
    let new_match = judge(&row.expected, actual).0 == Severity::Exact;
    let old = row.calc_func.as_deref().unwrap_or(&row.expected);
    let lost = judge(&row.expected, old).0 == Severity::Exact && !new_match;
    match (lost, loss_class) {
        (true, None) => {
            return Err(format!(
                "cannot refresh {}: a previously matched pin now fails",
                row.id
            ));
        }
        (false, Some(class)) => {
            return Err(format!(
                "{} is listed as a classified loss ({class}) but is not one",
                row.id
            ));
        }
        (true, Some(class)) => println!("xtask witness: classified loss {}: {class}", row.id),
        (false, None) => {}
    }
    let prediction = (!new_match).then(|| actual.to_owned());
    let changed = row.calc_func != prediction;
    row.calc_func = prediction;
    Ok(changed)
}

fn refresh_pinned(
    sets: Vec<(PathBuf, WitnessSet)>,
    banks: &Banks,
    judged: &BTreeMap<String, Vec<Row>>,
    context_refresh: bool,
    losses: &BTreeMap<(String, String), String>,
) -> Result<(), String> {
    let mut updates = Vec::new();
    let mut changed = 0;
    for (path, mut set) in sets {
        let rows = judged
            .get(&set.function)
            .ok_or(format!("{} was not judged", set.function))?;
        let actuals: BTreeMap<_, _> = rows
            .iter()
            .map(|r| {
                (
                    contextual_key(
                        &r.reading.function,
                        &r.reading.args,
                        r.reading.index,
                        banks.sources[r.reading.source].env.invocation.as_ref(),
                    ),
                    r.actual.as_str(),
                )
            })
            .collect();
        let mut count = 0;
        for row in &mut set.witnesses {
            let context = set
                .readings
                .iter()
                .find(|r| r.id == row.reading)
                .and_then(|r| r.invocation.as_ref());
            let key = contextual_key(&set.function, &row.args, row.result_index, context);
            let actual = actuals
                .get(&key)
                .ok_or(format!("no calculation for {} {}", set.function, row.id))?;
            let class = losses
                .get(&(set.function.clone(), row.id.clone()))
                .map(String::as_str);
            count += usize::from(refresh_prediction(row, actual, class)?);
        }
        let text = set.encode();
        let original = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut before: Value = serde_json::from_slice(&original).map_err(|e| e.to_string())?;
        let mut after: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        // Preserve every other field, including metadata unknown to WitnessSet.
        if context_refresh {
            before = without_context_and_prediction(before)?;
            after = without_context_and_prediction(after)?;
        } else {
            for value in [&mut before, &mut after] {
                for row in value["witnesses"]
                    .as_array_mut()
                    .ok_or("missing witnesses")?
                {
                    row.as_object_mut()
                        .ok_or("a witness is not an object")?
                        .remove("calc_func");
                }
            }
        }
        if before != after {
            return Err(format!(
                "{}: refresh would change recorded data or the original sample",
                path.display()
            ));
        }
        if original == text.as_bytes() {
            continue;
        }
        changed += count;
        updates.push((path, text));
    }
    // Validate the complete batch before writing any prediction.
    for (path, text) in &updates {
        fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    println!(
        "xtask witness: refreshed {changed} calculated predictions in {} original pinned sets; recorded inputs, Excel answers and sample unchanged; context restoration {context_refresh}",
        updates.len()
    );
    Ok(())
}

/// `--classified-losses`: tab-separated `function`, `id`, `class` lines (`#` starts a comment).
fn read_classified_losses(
    path: Option<&Path>,
) -> Result<BTreeMap<(String, String), String>, String> {
    let mut out = BTreeMap::new();
    let Some(path) = path else { return Ok(out) };
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let mut fields = line.splitn(3, '\t');
        let (Some(function), Some(id), Some(class)) = (fields.next(), fields.next(), fields.next())
        else {
            return Err(format!(
                "{} line {}: needs function, id and class",
                path.display(),
                i + 1
            ));
        };
        if class.trim().is_empty() {
            return Err(format!(
                "{} line {}: a loss needs its class",
                path.display(),
                i + 1
            ));
        }
        out.insert(
            (function.to_ascii_uppercase(), id.to_owned()),
            class.trim().to_owned(),
        );
    }
    Ok(out)
}

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
        ledger.insert(
            name,
            (
                f[st].clone(),
                f[sev].clone(),
                f[rj].clone(),
                f[ra].clone(),
                f[wu].clone(),
            ),
        );
    }
    let mut out = String::from(
        "function\tdnacalc_rows\tdnacalc_matched\tdnacalc_rate\tdnacalc_class\tdnacalc_worst_ulp\toxfunc_status\toxfunc_severity\toxfunc_rows\toxfunc_agree\toxfunc_rate\toxfunc_worst_ulp\tworse\n",
    );
    for s in statuses {
        let rate = s.matched as f64 / s.rows.max(1) as f64;
        let (status, severity, rows, agree, worst) =
            ledger.get(&s.function).cloned().unwrap_or_default();
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
            status_class(s),
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
    if raw == ["__row_worker"] {
        return match row_worker() {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("witness row worker: {e}");
                ExitCode::FAILURE
            }
        };
    }
    let request = match parse_args(raw) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("xtask witness: {e}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    // Profile selection is a speed option. Child judging follows the same route and receives
    // the same arguments after removing only the profile selector.
    let result = if let Some(profile) = &request.profile {
        (|| {
            let ws = Workspace::load(&std::env::current_dir().map_err(|e| e.to_string())?)?;
            let program = xtask::harness::profile_executable(&ws, profile)?;
            let _copy = xtask::harness::CopyCleanup(program.clone());
            let mut child = std::process::Command::new(&program);
            child.arg("witness");
            let mut args = raw.iter();
            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "--release" => (),
                    "--profile" => {
                        args.next();
                    }
                    _ => {
                        child.arg(arg);
                        if matches!(arg.as_str(), "--func" | "--bank" | "--trace" | "--report") {
                            child.arg(args.next().ok_or("missing validated argument value")?);
                        }
                    }
                }
            }
            let status = child.status().map_err(|e| e.to_string());
            Ok(ExitCode::from(
                status?.code().unwrap_or(1).try_into().unwrap_or(1),
            ))
        })()
    } else {
        run_request(&request)
    };
    match result {
        Ok(code) => code,
        Err(e) => {
            eprintln!("xtask witness: {e}");
            ExitCode::from(1)
        }
    }
}

#[derive(Default, Debug, PartialEq, Eq)]
struct TraceCounts {
    rows: usize,
    before: usize,
    after: usize,
    gained: usize,
    lost: usize,
    changed: usize,
}

impl TraceCounts {
    fn record(&mut self, expected: &str, before: &str, after: &str) {
        let old_match = judge(expected, before).0 == Severity::Exact;
        let new_match = judge(expected, after).0 == Severity::Exact;
        self.rows += 1;
        self.before += usize::from(old_match);
        self.after += usize::from(new_match);
        self.gained += usize::from(new_match && !old_match);
        self.lost += usize::from(old_match && !new_match);
        self.changed += usize::from(before != after);
    }
}

/// Read both traces completely, with bounded memory. Every field except the calculated
/// result must be identical, including unknown fields added by later judge versions.
fn compare_traces(
    before: impl BufRead,
    after: impl BufRead,
) -> Result<BTreeMap<String, TraceCounts>, String> {
    let mut counts: BTreeMap<String, TraceCounts> = BTreeMap::new();
    let mut old_lines = before.lines();
    let mut new_lines = after.lines();
    let mut line = 0;
    loop {
        line += 1;
        let pair = match (old_lines.next(), new_lines.next()) {
            (None, None) => break,
            (Some(a), Some(b)) => (a, b),
            _ => return Err(format!("trace lengths differ at row {line}")),
        };
        let read =
            |text: std::io::Result<String>| -> Result<serde_json::Map<String, Value>, String> {
                let text = text.map_err(|e| format!("trace row {line}: {e}"))?;
                let Value::Object(value) =
                    serde_json::from_str(&text).map_err(|e| format!("trace row {line}: {e}"))?
                else {
                    return Err(format!("trace row {line} is not an object"));
                };
                for field in ["function", "id", "expected", "actual", "source"] {
                    if !value.get(field).is_some_and(Value::is_string) {
                        return Err(format!("trace row {line} needs string {field}"));
                    }
                }
                if !value.get("args").is_some_and(Value::is_array)
                    || !value.get("reading").is_some_and(Value::is_object)
                    || !value.contains_key("result_index")
                {
                    return Err(format!(
                        "trace row {line} lacks input, selection or environment"
                    ));
                }
                Ok(value)
            };
        let mut old = read(pair.0)?;
        let mut new = read(pair.1)?;
        let old_actual = old.remove("actual").unwrap();
        let new_actual = new.remove("actual").unwrap();
        if old != new {
            return Err(format!(
                "reading identity or recorded answer differs at row {line}"
            ));
        }
        let function = old["function"].as_str().unwrap();
        counts.entry(function.to_owned()).or_default().record(
            old["expected"].as_str().unwrap(),
            old_actual.as_str().unwrap(),
            new_actual.as_str().unwrap(),
        );
    }
    if counts.is_empty() {
        return Err("traces contain no decoded rows".to_owned());
    }
    Ok(counts)
}

fn run_trace_comparison(request: &Request) -> Result<ExitCode, String> {
    let open = |path: &Option<PathBuf>| {
        let path = path.as_ref().ok_or("missing trace path".to_owned())?;
        fs::File::open(path)
            .map(BufReader::new)
            .map_err(|e| format!("{}: {e}", path.display()))
    };
    let counts = compare_traces(open(&request.before)?, open(&request.after)?)?;
    let mut total = TraceCounts::default();
    for (function, c) in &counts {
        if c.changed != 0 {
            println!(
                "xtask witness compare: {function}: {} -> {} matched / {}, +{} / -{}, {} actuals changed",
                c.before, c.after, c.rows, c.gained, c.lost, c.changed
            );
        }
        total.rows += c.rows;
        total.before += c.before;
        total.after += c.after;
        total.gained += c.gained;
        total.lost += c.lost;
        total.changed += c.changed;
    }
    println!(
        "xtask witness compare: {} decoded rows; all input/selection/answer/source/environment fields unchanged",
        total.rows
    );
    println!(
        "xtask witness compare: {} -> {} matched, +{} / -{}; {} actuals changed; unread failures are only in judge summaries",
        total.before, total.after, total.gained, total.lost, total.changed
    );
    Ok(if total.lost == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

fn run_request(request: &Request) -> Result<ExitCode, String> {
    if request.verb == Verb::Compare {
        return run_trace_comparison(request);
    }
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let ws = Workspace::load(&cwd)?;
    let only = request.function.as_deref();
    let mut trace = request
        .trace
        .as_ref()
        .map(|path| {
            fs::File::create(path)
                .map(BufWriter::new)
                .map_err(|e| format!("{}: {e}", path.display()))
        })
        .transpose()?;
    let checkout = legacy_root(&ws.root).path.join(CHECKOUT);
    if request.verb == Verb::Read {
        return run_read(&ws.root, &checkout, request);
    }

    // The readings of the bank asked for.
    let mut original_pins = None;
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
            let mut banks = Banks {
                sources: Vec::new(),
                readings: Vec::new(),
                files_read: 0,
                import_notes: Vec::new(),
                unread_rows: BTreeMap::new(),
            };
            let mut sets = pinned_sets(&ws.root, only)?;
            if request.refresh_context {
                let (full, _) = load_full(&checkout, only)?;
                let restored = restore_pinned_context(&mut sets, &full)?;
                println!(
                    "xtask witness: restored source-attested invocation facts for {restored} original pinned rows"
                );
            }
            original_pins = Some(sets.clone());
            for (_, set) in sets {
                banks.files_read += 1;
                let first = banks.sources.len();
                for r in &set.readings {
                    banks.sources.push(Source {
                        path: r.source.clone(),
                        env: r.clone(),
                    });
                }
                for w in set.witnesses {
                    let source = first
                        + set
                            .readings
                            .iter()
                            .position(|r| r.id == w.reading)
                            .unwrap_or(0);
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
    if request.verb == Verb::Export {
        let path = request.report.as_ref().expect("validated export path");
        let mut out =
            BufWriter::new(fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?);
        for r in &banks.readings {
            let source = &banks.sources[r.source];
            let value = serde_json::json!({
                "function": r.function, "id": r.id, "args": r.args,
                "result_index": r.index, "expected": r.expected,
                "source": source.path, "build": source.env.excel_build,
                "cpu": source.env.cpu, "invocation": source.env.invocation.as_ref().map(|v| v.to_json()),
                "exclusion": exclusion_reason(r, source),
            });
            serde_json::to_writer(&mut out, &value).map_err(|e| e.to_string())?;
            out.write_all(b"\n").map_err(|e| e.to_string())?;
        }
        out.flush().map_err(|e| e.to_string())?;
        println!(
            "xtask witness export: {} readings, {} unread records, {} files; {}",
            banks.readings.len(),
            unread.len(),
            banks.files_read,
            path.display()
        );
        for line in &unread {
            eprintln!("xtask witness export: unread: {line}");
        }
        return Ok(ExitCode::SUCCESS);
    }
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
        println!(
            "xtask witness: {unread_files} bank files could not be decoded; their row counts are unknown and are additional to the counted failures"
        );
    }
    if request.values_only {
        println!(
            "xtask witness: diagnostic values-only invocation; source context remains in identities and trace"
        );
    }
    if !request.values_only
        && banks.sources.iter().any(|s| {
            s.env
                .invocation
                .as_ref()
                .is_some_and(|c| c.compatibility_version.is_some())
        })
    {
        println!(
            "xtask witness: CompatibilityVersion is retained in source identity and applied by the context-aware caller"
        );
    }
    let (mut grouped, mut differences) = group(&banks);
    if request.bank == Bank::Full && only.is_none_or(|name| name == "DATE") {
        differences.extend(replay_differences(&banks)?);
    }
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
        let contexts: Vec<_> = banks
            .sources
            .iter()
            .map(|s| s.env.invocation.clone().map(std::sync::Arc::new))
            .collect();
        let excluded = |r: &Reading| {
            if request.verb == Verb::Judge {
                exclusion_reason(r, &banks.sources[r.source])
            } else {
                None
            }
        };
        let jobs: Vec<_> = chosen
            .iter()
            .filter(|r| excluded(r).is_none())
            .map(|r| (r.args.clone(), r.index, contexts[r.source].clone()))
            .collect();
        let mut actuals = calculate_all(function, &jobs, !request.values_only).into_iter();
        let mut rows = Vec::new();
        for (readings, reading) in keys.iter().zip(chosen) {
            let reason = excluded(reading);
            let actual = if let Some(reason) = reason {
                format!("excluded:{reason}")
            } else {
                actuals.next().expect("one result per judged job")
            };
            let (slow, actual) = match actual.strip_prefix(SLOW) {
                Some(rest) => (true, rest.to_owned()),
                None => (false, actual),
            };
            let builds: BTreeSet<String> = readings
                .iter()
                .map(|r| {
                    banks.sources[r.source]
                        .env
                        .excel_build
                        .clone()
                        .unwrap_or_else(|| "?".to_owned())
                })
                .collect();
            if actual == "fault:Timeout" {
                println!(
                    "xtask witness: {function}: no result within {ROW_SECONDS} s on {}",
                    Value::Array(reading.args.clone())
                );
            }
            let (severity, ulp) = if reason.is_some() {
                (Severity::Structural, None)
            } else {
                judge(&reading.expected, &actual)
            };
            let row = Row {
                reading,
                builds,
                actual,
                severity,
                ulp,
                slow,
            };
            if let Some(writer) = trace.as_mut() {
                write_trace_row(writer, &row, &banks.sources[reading.source])?;
            }
            rows.push(row);
        }
        let mut status = status_of(function, &rows);
        let unread = banks.unread_rows.get(function).copied().unwrap_or(0);
        status.rows += unread;
        if unread > 0 {
            status.class = Severity::Structural;
            println!("xtask witness: {function}: {unread} unread recorded rows count as failures");
        }
        if status.excluded > 0 {
            println!(
                "xtask witness: {function}: {} rows excluded from judgment, retained as failures; the trace names their evidence",
                status.excluded
            );
        }
        println!(
            "xtask witness: {:<22} {:>8} rows {:>8} matched  worst {:>6} ULP  {}",
            function,
            status.rows,
            status.matched,
            status.worst_ulp.map_or("-".to_owned(), |u| u.to_string()),
            status_class(&status)
        );
        statuses.push(status);
        judged.insert(function.clone(), rows);
    }
    let exact = statuses
        .iter()
        .filter(|s| s.class == Severity::Exact)
        .count();
    if let Some(writer) = trace.as_mut() {
        writer.flush().map_err(|e| format!("witness trace: {e}"))?;
    }
    let rows: usize = statuses.iter().map(|s| s.rows).sum();
    let matched: usize = statuses.iter().map(|s| s.matched).sum();
    let excluded: usize = statuses.iter().map(|s| s.excluded).sum();
    println!(
        "xtask witness: {} functions, {exact} exact on every counted row; {matched} of {rows} rows matched, {} failed (mismatched, refused, unread or excluded; {unread_rows} unread, {excluded} excluded)",
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
        println!(
            "xtask witness: {} rows left out of the pinned sets",
            panics.len()
        );
        for p in panics.iter().take(30) {
            println!(
                "xtask witness: left out, calc-func panics, takes over {SLOW_MILLIS} ms or does not finish: {p}"
            );
        }
    }
    if request.verb == Verb::Refresh {
        let losses = read_classified_losses(request.classified_losses.as_deref())?;
        refresh_pinned(
            original_pins.ok_or("refresh requires the original pins")?,
            &banks,
            &judged,
            request.refresh_context,
            &losses,
        )?;
    }
    let bank = match request.bank {
        Bank::Pinned => "pinned",
        Bank::Full => "full",
    };
    if let Some(path) = &request.report {
        let functions: Vec<Value> = statuses.iter().map(|s| serde_json::json!({
            "function": s.function, "rows": s.rows, "matched": s.matched,
            "failed": s.rows - s.matched, "excluded": s.excluded, "worst_ulp": s.worst_ulp, "class": status_class(s),
        })).collect();
        let summary = serde_json::json!({
            "format": "dnacalc-witness-summary", "bank": bank,
            "totals": {"functions": statuses.len(), "exact": exact, "rows": rows,
                "matched": matched, "failed": rows - matched, "unread": unread_rows, "excluded": excluded},
            "functions": functions,
        });
        fs::write(path, serde_json::to_vec_pretty(&summary).unwrap())
            .map_err(|e| format!("{}: {e}", path.display()))?;
        println!("xtask witness: wrote read-only report {}", path.display());
    } else {
        write_status(&ws.root, bank, &statuses, only)?;
        println!("xtask witness: wrote {STATUS} ({bank})");
        if request.bank == Bank::Full {
            // `pin` judged the full banks too, so it writes their status as judge does.
            write_differences(&ws.root, &differences, only)?;
            println!("xtask witness: wrote {DIFFERENCES}");
        }
    }
    if request.bank == Bank::Full {
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

/// An input identity ignores numeric-hex letter case and result selection. A new input is
/// new even before a particular result cell is selected, so any banked use excludes it.
fn input_key(args: &[Value]) -> String {
    fn canonical(v: &Value) -> Value {
        match v {
            Value::String(s) => match s
                .strip_prefix("0x")
                .filter(|h| h.len() == 16)
                .and_then(|h| u64::from_str_radix(h, 16).ok())
            {
                Some(bits) => Value::String(format!("0x{bits:016x}")),
                None => v.clone(),
            },
            Value::Array(a) => Value::Array(a.iter().map(canonical).collect()),
            _ => v.clone(),
        }
    }
    Value::Array(args.iter().map(canonical).collect()).to_string()
}

fn supplied_inputs(
    bytes: &[u8],
    function: &str,
    repeat_calls: bool,
) -> Result<(Value, Vec<WitnessRow>), String> {
    let doc: Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if doc["function"]
        .as_str()
        .is_none_or(|f| !f.eq_ignore_ascii_case(function))
    {
        return Err("the input document's function differs from --func".into());
    }
    let rows = doc["witnesses"]
        .as_array()
        .filter(|r| !r.is_empty())
        .ok_or("inputs need a nonempty witnesses array")?;
    let mut ids = BTreeSet::new();
    let mut calls = BTreeSet::new();
    let mut out = Vec::new();
    for row in rows {
        let id = row["id"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("an input needs an id")?;
        if !ids.insert(id.to_owned()) {
            return Err(format!("duplicate input id {id}"));
        }
        let args = row["args"].as_array().ok_or("an input needs args")?.clone();
        for (i, arg) in args.iter().enumerate() {
            let value = decode_argument(arg, &format!("{id}.args[{i}]"))?;
            if value.as_array().is_some() || value.as_number().is_some_and(|n| !n.is_finite()) {
                return Err(format!(
                    "{id}: the bulk reader needs finite scalar arguments"
                ));
            }
        }
        let index = row.as_object().and_then(calc_verdict::result_index);
        if row.get("result_index").is_some() && index.is_none_or(|(r, c)| r == 0 || c == 0) {
            return Err(format!("{id}: result_index needs two positive integers"));
        }
        let literal_transport = match row.get("input_transport").and_then(Value::as_str) {
            None | Some("cell_value2_bulk")
                if row.get("input_transport").is_none_or(|v| v.is_string()) =>
            {
                false
            }
            Some("formula_literals") => true,
            _ => return Err(format!("{id}: unknown input_transport")),
        };
        if literal_transport
            && args
                .iter()
                .any(|a| decode_argument(a, id).unwrap().as_number().is_none())
        {
            return Err(format!("{id}: formula_literals requires finite numbers"));
        }
        if !calls.insert((input_key(&args), index, literal_transport)) && !repeat_calls {
            return Err(format!("duplicate input call {id}"));
        }
        out.push(WitnessRow {
            id: id.into(),
            args,
            result_index: index,
            expected: String::new(),
            reading: "prediction".into(),
            calc_func: None,
        });
    }
    Ok((doc, out))
}

/// Calculate and durably freeze every prediction before starting Excel. The frozen file is
/// explicitly model output, never an Excel reading. Actual readings stay in the existing layout.
fn predict_inputs(
    root: &Path,
    checkout: &Path,
    function: &str,
    path: &Path,
    new_only: bool,
    repeat_calls: bool,
) -> Result<(WitnessSet, Value), String> {
    let bytes = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let (doc, mut rows) = supplied_inputs(&bytes, function, repeat_calls)?;
    let novelty = if new_only {
        let (banks, unread_files) = load_full(checkout, Some(function))?;
        if !unread_files.is_empty() {
            return Err("cannot establish novelty: some bank records could not be read".into());
        }
        let unread = banks.unread_rows.get(function).copied().unwrap_or(0);
        if unread > 0 {
            return Err(format!(
                "cannot establish novelty: {unread} unread bank rows"
            ));
        }
        let known: BTreeSet<_> = banks.readings.iter().map(|r| input_key(&r.args)).collect();
        for row in &rows {
            if known.contains(&input_key(&row.args)) {
                return Err(format!(
                    "{} already occurs in the recorded {function} banks",
                    row.id
                ));
            }
        }
        serde_json::json!({"bank_rows": banks.readings.len(), "distinct_inputs": known.len(), "bank_files": banks.files_read, "unread_rows": unread})
    } else {
        Value::Null
    };
    let jobs: Vec<_> = rows
        .iter()
        .map(|r| (r.args.clone(), r.result_index, None))
        .collect();
    let actuals = calculate_all(function, &jobs, true);
    for (row, actual) in rows.iter_mut().zip(actuals) {
        row.expected = actual.strip_prefix(SLOW).unwrap_or(&actual).to_owned();
    }
    let commit = crate::cargo::git(root, &["rev-parse", "HEAD"])?;
    let diff = crate::cargo::git(
        root,
        &[
            "diff",
            "--binary",
            "HEAD",
            "--",
            "crates/calc-func/src",
            "crates/calc-value/src",
        ],
    )?;
    let frozen = serde_json::json!({
        "format": "dnacalc-frozen-function-predictions/1", "function": function,
        "frozen_utc": crate::accept::utc_now(), "calc_func_commit": commit.trim(),
        "model_source_diff_sha256": calc_oracle_excel::sha256_hex(diff.as_bytes()),
        "model_source_dirty": !diff.is_empty(), "numeric_mode": "Fast",
        "inputs_path": path.to_string_lossy(), "inputs_sha256": calc_oracle_excel::sha256_hex(&bytes),
        "input_document": doc, "novelty": novelty, "repeat_calls": repeat_calls,
        "predictions": rows.iter().map(|r| serde_json::json!({"id": r.id, "args": r.args,
            "result_index": r.result_index, "calc_func_prediction": r.expected})).collect::<Vec<_>>()
    });
    let dir = root.join("target/witness/frozen");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let frozen_path = dir.join(format!(
        "{function}-{}.json",
        crate::accept::utc_now().replace([':', '-'], "")
    ));
    let frozen_bytes = serde_json::to_vec_pretty(&frozen).map_err(|e| e.to_string())?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&frozen_path)
        .map_err(|e| e.to_string())?;
    file.write_all(&frozen_bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())?;
    let metadata = serde_json::json!({"frozen_path": frozen_path.to_string_lossy(),
        "frozen_sha256": calc_oracle_excel::sha256_hex(&frozen_bytes),
        "inputs_path": path.to_string_lossy(), "inputs_sha256": calc_oracle_excel::sha256_hex(&bytes),
        "selection": doc["selection"], "calc_func_commit": commit.trim(), "novelty": novelty,
        "formula_literal_ids": doc["witnesses"].as_array().unwrap().iter()
            .filter(|r| r["input_transport"].as_str() == Some("formula_literals"))
            .map(|r| r["id"].clone()).collect::<Vec<_>>()});
    println!(
        "xtask witness: {function}: froze {} model predictions before Excel at {}",
        rows.len(),
        frozen_path.display()
    );
    Ok((
        WitnessSet {
            function: function.into(),
            selection: "caller inputs; frozen model predictions, not Excel answers".into(),
            readings: vec![ReadingSource {
                id: "prediction".into(),
                source: format!("calc-func {} prediction", commit.trim()),
                excel_build: None,
                platform: None,
                bitness: None,
                cpu: None,
                invocation: None,
            }],
            witnesses: rows,
        },
        metadata,
    ))
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
        let (set, prediction) = match &request.inputs {
            Some(path) => {
                let (set, metadata) = predict_inputs(
                    root,
                    checkout,
                    &function,
                    path,
                    request.new_only,
                    request.repeat_calls,
                )?;
                (set, Some(metadata))
            }
            None => {
                let sets = pinned_sets(root, Some(&function))?;
                let Some((_, set)) = sets.into_iter().next() else {
                    return Err(format!("no pinned set of {function} to read; pin it first"));
                };
                (set, None)
            }
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
                        formula_literals: prediction.as_ref().is_some_and(|p| {
                            p["formula_literal_ids"]
                                .as_array()
                                .is_some_and(|ids| ids.iter().any(|id| id.as_str() == Some(&w.id)))
                        }),
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
        let mut transport_audits = Vec::new();
        for ((reading, row), call) in outcome.readings.iter().zip(&rows).zip(&calls) {
            if call.formula_literals {
                transport_audits.push(serde_json::json!({
                    "id": row.id, "args": row.args, "result_index": row.result_index,
                    "input_transport": "formula_literals_with_cell_echo",
                    "input_audit": reading.input_audit, "observed_raw": reading.observed_raw,
                    "reading_failure": reading.value.as_ref().err(),
                    "typed_argument_prediction": row.expected,
                    "semantic_witness": false
                }));
                continue;
            }
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
            if prediction.is_some() {
                node.insert(
                    "calc_func_prediction".into(),
                    Value::String(row.expected.clone()),
                );
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
            "unread": unread,
            "transport_audit_readings": transport_audits,
            "input_audits": outcome.readings.iter().map(|r| serde_json::json!({
                "id": r.id, "audit": r.input_audit,
                "observed_raw": r.observed_raw,
                "reading_failure": r.value.as_ref().err()
            })).collect::<Vec<_>>(),
            "capture_provenance": {
                "schema_version": "dnacalc-witness-reading/1",
                "captured_utc": crate::accept::utc_now(),
                "reader": reader,
                "inputs": request.inputs.as_ref().map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|| format!("the rows of the pinned set crates/calc-func/tests/witnesses/{function}.json")),
                "frozen_model_predictions": prediction,
                "calculation_settings": outcome.calculation_settings,
                "automation_security_before": env.automation_security_before,
                "automation_security_after": env.automation_security_after,
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
        let comparison = if request.inputs.is_some() {
            "frozen model prediction"
        } else {
            "banked reading"
        };
        let audit_failures = transport_audits
            .iter()
            .filter(|r| !r["reading_failure"].is_null())
            .count();
        failed |= !unread.is_empty() || skipped > 0 || audit_failures > 0;
        println!(
            "xtask witness: {function}: read {} rows on Excel {} build {build} ({}, {}, {}): {agree} equal the {comparison}, {} differ, {} unread; {} transport audit observations ({audit_failures} failed audit obligations); {skipped} not asked (array arguments); wrote {}",
            outcome.readings.len(),
            env.excel_version,
            env.bitness,
            env.platform,
            env.cpu.as_deref().unwrap_or("CPU unknown"),
            differ.len(),
            unread.len(),
            transport_audits.len(),
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
    #[test]
    fn indirect_unlocated_uses_real_binder_without_inventing_workbook_context() {
        use serde_json::json;
        for args in [
            vec![json!("" )], vec![json!(" A1")], vec![json!("=A1")],
            vec![json!("(A1)")], vec![json!("A1+1")], vec![json!("1")],
            // dc_51ka row 80 and the separate jnt_lookup row 76 captured 1 and 123.
            vec![json!("0x3ff0000000000000")], vec![json!("0x405ec00000000000")], vec![json!(true)],
            vec![json!("R0C1"), json!(false)],
        ] {
            assert_eq!(super::calculate_with_context("INDIRECT", &args, None, None, true), "error:Ref", "{args:?}");
        }
        for text in ["A1", "Rate", "data!A1", "[book]sheet!A1"] {
            assert_eq!(super::calculate_with_context("INDIRECT", &[json!(text)], None, None, true), "fault:NotImplemented", "{text}");
        }
        assert_eq!(
            super::calculate_with_context("INDIRECT", &[json!("0x3ff0000000000000"), json!({"error": "NA"})], None, None, true),
            "error:NA"
        );
    }

    #[test]
    fn unique_recapture_requires_matching_input_readback() {
        let bytes = super::gunzip(
            include_bytes!("../../../migration/evidence/dc-ymjf-2-10-unique-recapture.json.gz"),
            None,
        )
        .unwrap();
        let mut doc = super::bank_json(&bytes).unwrap();
        let rows = super::unique_recaptures(&doc).unwrap();
        assert_eq!(rows.len(), 1024);
        doc["replies"][0]["inputs"][0][0]["values"][0][0]["bits"] = "0000000000000000".into();
        assert!(super::unique_recaptures(&doc).is_err());
    }

    #[test]
    fn repeated_replay_observations_keep_each_source_on_one_input() {
        let pair = |source: &str| super::Difference {
            function: "DATE".into(),
            key: "same input and invocation".into(),
            readings: vec![
                ("old".into(), "old environment".into(), source.into()),
                ("new".into(), "new environment".into(), source.into()),
            ],
            across_environments: true,
        };
        let groups = super::coalesce_replay_differences(vec![pair("first"), pair("second")]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].readings.len(), 4);
        assert_eq!(groups[0].readings[0].2, "first");
        assert_eq!(groups[0].readings[2].2, "second");
    }

    #[test]
    fn replay_pairs_require_original_identity_and_exact_input_readback() {
        let bytes = super::gunzip(
            include_bytes!("../../../migration/evidence/dc-ymjf3-20527-date-differences.jsonl.gz"),
            None,
        )
        .unwrap();
        let record = super::bank_json(bytes.split(|b| *b == b'\n').next().unwrap()).unwrap();
        let bank = &record["bank"];
        let source = super::Source {
            path: bank["source"].as_str().unwrap().into(),
            env: super::ReadingSource {
                id: String::new(),
                source: String::new(),
                excel_build: bank["build"].as_str().map(str::to_owned),
                platform: Some("Windows".into()),
                bitness: None,
                cpu: bank["cpu"].as_str().map(str::to_owned),
                invocation: Some(
                    calc_verdict::WitnessInvocation::decode(&bank["invocation"], "invocation")
                        .unwrap(),
                ),
            },
        };
        let original = super::Reading {
            function: bank["function"].as_str().unwrap().into(),
            id: bank["id"].as_str().unwrap().into(),
            args: bank["args"].as_array().unwrap().clone(),
            index: None,
            expected: bank["expected"].as_str().unwrap().into(),
            source: 0,
        };
        let difference = super::replay_difference(&record, &original, &source).unwrap();
        assert_eq!(difference.readings.len(), 2);
        assert_ne!(difference.readings[0].0, difference.readings[1].0);
        let mut changed = record.clone();
        changed["inputs"][0]["bits"] = serde_json::json!("0000000000000000");
        assert!(super::replay_difference(&changed, &original, &source).is_err());
        changed = record.clone();
        changed["bank"]["expected"] = serde_json::json!("error:Num");
        assert!(super::replay_difference(&changed, &original, &source).is_err());
        changed = record;
        changed["environment"]["build"] = serde_json::json!("20430");
        assert!(super::replay_difference(&changed, &original, &source).is_err());
    }
    use super::*;

    #[test]
    fn binomial_replay_retains_every_call_and_its_exact_inputs() {
        let doc = bank_json(
            &gunzip(
                include_bytes!("../../../migration/evidence/dc-1943-binom-replay.json.gz"),
                None,
            )
            .unwrap(),
        )
        .unwrap();
        let mut banks = Banks {
            sources: Vec::new(),
            readings: Vec::new(),
            files_read: 0,
            import_notes: Vec::new(),
            unread_rows: BTreeMap::new(),
        };
        for function in ["BINOM.INV", "CRITBINOM"] {
            add_binomial_replay(&mut banks, function, &doc).unwrap();
        }
        assert_eq!(banks.readings.len(), 378);
        let (grouped, differences) = group(&banks);
        assert!(differences.is_empty());
        assert_eq!(grouped["BINOM.INV"].len(), 156);
        assert_eq!(grouped["CRITBINOM"].len(), 156);
    }

    #[test]
    fn free_layout_replays_compare_every_observed_array_cell() {
        let mut banks = Banks {
            sources: Vec::new(),
            readings: Vec::new(),
            files_read: 0,
            import_notes: Vec::new(),
            unread_rows: BTreeMap::new(),
        };
        for (function, path, bytes, count) in [
            (
                "TEXTSPLIT",
                TEXTSPLIT_REPLAY,
                &include_bytes!("../../../migration/evidence/dc-1943-textsplit-replay.json.gz")[..],
                6,
            ),
            (
                "MINVERSE",
                MINVERSE_REPLAY,
                &include_bytes!("../../../migration/evidence/dc-1943-minverse-replay.json.gz")[..],
                448,
            ),
        ] {
            let doc = bank_json(&gunzip(bytes, None).unwrap()).unwrap();
            let start = banks.readings.len();
            add_array_replay(&mut banks, function, path, &doc).unwrap();
            assert_eq!(banks.readings.len() - start, count);
        }
        for reading in &banks.readings {
            let context = banks.sources[reading.source].env.invocation.as_ref();
            assert_eq!(
                calculate_with_context(
                    &reading.function,
                    &reading.args,
                    reading.index,
                    context,
                    true
                ),
                reading.expected,
                "{}",
                reading.id
            );
            if reading.index == Some((1, 1)) {
                let (height, width) = if reading.function == "TEXTSPLIT" {
                    (1, 3)
                } else {
                    (4, 4)
                };
                for index in [(height + 1, 1), (1, width + 1)] {
                    assert_eq!(
                        calculate_with_context(
                            &reading.function,
                            &reading.args,
                            Some(index),
                            context,
                            true
                        ),
                        "fault:ResultShape",
                        "{} {index:?}",
                        reading.id
                    );
                }
            }
        }
        let doc = bank_json(
            &gunzip(
                include_bytes!("../../../migration/evidence/dc-1943-minverse-replay.json.gz"),
                None,
            )
            .unwrap(),
        )
        .unwrap();
        let args = array_replay_args(&doc["cases"][0], "MINVERSE").unwrap();
        let inputs = BTreeMap::from([("MINVERSE|m4x4:28c4018eb542fe5f", args[0].clone())]);
        let mut record = serde_json::json!({"key":"MINVERSE|m4x4:28c4018eb542fe5f|@1,0",
            "args":[[{"kind":"matrix","cols":4,"bits_hex_row_major":args[0]}]],
            "outcome":{"code":"ExcelErrorType9"},"result_index":[1,0]});
        assert!(recovered_minverse_spill(
            "MINVERSE",
            "cache/oracle/build-20131/MINVERSE.jsonl",
            &record,
            &inputs
        ));
        record["args"][0][0]["bits_hex_row_major"][0][0] = serde_json::json!("0x0000000000000000");
        assert!(!recovered_minverse_spill(
            "MINVERSE",
            "cache/oracle/build-20131/MINVERSE.jsonl",
            &record,
            &inputs
        ));
        record["args"][0][0]["bits_hex_row_major"] = args[0].clone();
        record["result_index"] = serde_json::json!([1, 1]);
        assert!(!recovered_minverse_spill(
            "MINVERSE",
            "cache/oracle/build-20131/MINVERSE.jsonl",
            &record,
            &inputs
        ));
    }

    #[test]
    fn weeknum_order_dependent_rows_remain_counted_and_carry_evidence() {
        let metadata: Value = serde_json::from_str(include_str!(
            "../../../migration/evidence/dc-1943-weeknum.json"
        ))
        .unwrap();
        assert_eq!(metadata["paired_calls"], 2000);
        assert!(
            metadata["evidence"]
                .as_str()
                .unwrap()
                .ends_with("/Q3-WEEKNUM.json")
        );
        let reading = |kind: f64| Reading {
            function: "WEEKNUM".into(),
            id: "row".into(),
            args: vec![
                serde_json::json!("0x40e5676000000000"),
                serde_json::json!(format!("0x{:016x}", kind.to_bits())),
            ],
            index: None,
            expected: "error:Num".into(),
            source: 0,
        };
        for kind in [0.0, 3.0, 10.0, 18.0, 20.0, 21.0, 11.0 - 2.3e-7] {
            assert!(!order_dependent_weeknum(&reading(kind)), "{kind}");
        }
        for kind in [-1.0, 22.0, 99.0, 22.0 - 1e-7] {
            assert!(order_dependent_weeknum(&reading(kind)), "{kind}");
        }
        let r = reading(22.0);
        let row = Row {
            reading: &r,
            slow: false,
            builds: BTreeSet::new(),
            actual: "excluded:order-dependent".into(),
            severity: Severity::Structural,
            ulp: None,
        };
        let mut trace = Vec::new();
        let source = Source {
            path: "bank".into(),
            env: environment("r0".into(), "bank".into(), None, None, None),
        };
        write_trace_row(&mut trace, &row, &source).unwrap();
        let trace: Value = serde_json::from_slice(&trace).unwrap();
        assert_eq!(trace["exclusion"]["evidence"], WEEKNUM_EXCLUSION);
        let status = status_of("WEEKNUM", &[row]);
        assert_eq!((status.rows, status.matched, status.excluded), (1, 0, 1));
        assert_eq!(status_class(&status), "excluded");
    }

    #[test]
    fn fixed_replay_attests_the_profile_only_for_its_bank() {
        let bytes = include_bytes!("../../../migration/evidence/dc-1943-fixed-replay.json.gz");
        let replay: Value = serde_json::from_slice(&gunzip(bytes, None).unwrap()).unwrap();
        assert_eq!(replay["excel_build"], "20527");
        let rows: Vec<_> = replay["rows"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["tag"] == "bank")
            .collect();
        assert_eq!(rows.len(), 989);
        assert!(rows.iter().all(|row| row["same"] == true
            && row["input_kept"] == true
            && row["input_bits"][2] == "3ff0000000000000"
            && row["excel_text"] == row["bank_text"]));
        let source = format!("{CHECKOUT} cache/oracle-20430/build-20430/FIXED.jsonl");
        let context = environment("r0".into(), source, None, None, None)
            .invocation
            .unwrap();
        assert_eq!(context.locale.as_deref(), Some("en-ZA"));
        assert_eq!(context.decimal_separator.as_deref(), Some("."));
        assert_eq!(context.thousands_separator.as_deref(), Some(" "));
        assert!(
            environment("r0".into(), "another bank".into(), None, None, None)
                .invocation
                .is_none()
        );
    }

    /// W111 broad ISREF writes each scalar into a cell and calls through that reference.
    #[test]
    fn captured_origin_row_audits_override_file_defaults_and_keys_retain_context() {
        let env = serde_json::json!({"excel_input_plumbing": "cell_value2_bulk"});
        let base = environment("r0".into(), "bank".into(), Some(&env), None, None);
        let audit = |transport: &str| {
            serde_json::json!({
                "transport":transport, "requested":[{"number":"0000000000000000"}],
                "submitted_value2":[{"number":"0000000000000000"}], "read_back":[{"number":"0000000000000000"}]
            })
        };
        let mut doc = serde_json::json!({"input_audits": [
            {"id": "literal", "reading_failure": null, "audit": audit("formula_literals_with_cell_echo")},
            {"id": "cell", "reading_failure": null, "audit": audit("cell_value2_bulk")},
            {"id": "failed", "reading_failure": "changed input", "audit": audit("cell_value2_bulk")}
        ]});
        let row = |id: &str| serde_json::json!({"id":id,"args":["0x0000000000000000"]});
        let cell = row_environment(base.clone(), &doc, &row("cell"));
        let literal = row_environment(base.clone(), &doc, &row("literal"));
        assert_ne!(
            contextual_key(
                "ISREF",
                &[serde_json::json!(0)],
                None,
                cell.invocation.as_ref()
            ),
            contextual_key(
                "ISREF",
                &[serde_json::json!(0)],
                None,
                literal.invocation.as_ref()
            )
        );
        for id in ["absent", "failed"] {
            let source = row_environment(base.clone(), &doc, &row(id));
            assert!(source.invocation.is_none());
        }
        let duplicate = doc["input_audits"][1].clone();
        doc["input_audits"].as_array_mut().unwrap().push(duplicate);
        assert!(
            row_environment(base.clone(), &doc, &row("cell"))
                .invocation
                .is_none()
        );
        doc["input_audits"].as_array_mut().unwrap().pop();
        doc["input_audits"][1]["audit"]["read_back"][0]["number"] =
            serde_json::json!("0000000000000001");
        assert!(
            row_environment(base.clone(), &doc, &row("cell"))
                .invocation
                .is_none()
        );
        let settings = serde_json::json!({"Date1904":true,"CompatibilityVersion":2,"DecimalSeparator":",","ThousandsSeparator":"."});
        let context = environment(
            "r0".into(),
            "bank".into(),
            Some(&env),
            None,
            Some(&settings),
        )
        .invocation
        .unwrap();
        assert!(context.locale.is_none());
        assert_eq!(context.compatibility_version.as_deref(), Some("2"));
        assert_eq!(context.date_system.as_deref(), Some("1904"));
    }

    #[test]
    fn captured_origin_isref_retains_the_scalar_cell_reference() {
        let context = calc_verdict::WitnessInvocation {
            input_transport: Some("cell_value2_bulk".into()),
            ..Default::default()
        };
        assert_eq!(
            calculate_with_context(
                "ISREF",
                &[serde_json::json!("0x0000000000000000")],
                None,
                Some(&context),
                true
            ),
            "logical:True"
        );
    }

    /// Fn1 MATCH input_audits retain Formula2R1C1 references and exact typed read-back.
    #[test]
    fn captured_origin_match_searches_the_held_scalar_cell() {
        let context = calc_verdict::WitnessInvocation {
            input_transport: Some("cell_value2_bulk".into()),
            ..Default::default()
        };
        assert_eq!(
            calculate_with_context(
                "MATCH",
                &[
                    serde_json::json!("é*"),
                    serde_json::json!("Élan"),
                    serde_json::json!("0x0000000000000000")
                ],
                None,
                Some(&context),
                true
            ),
            "0x3ff0000000000000"
        );
    }

    use serde_json::json;

    fn args(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn judge_defaults_to_the_pinned_bank_and_pin_to_the_full_one() {
        let r = parse_args(&args("judge")).unwrap();
        assert!(r.verb == Verb::Judge && r.bank == Bank::Pinned && r.function.is_none());
        let r = parse_args(&args("judge --bank full --func pmt")).unwrap();
        assert!(r.bank == Bank::Full && r.function.as_deref() == Some("PMT"));
        let reported =
            parse_args(&args("judge --bank full --release --report counts.json")).unwrap();
        assert_eq!(reported.profile.as_deref(), Some("release"));
        assert_eq!(reported.report, Some(PathBuf::from("counts.json")));
        assert_eq!(
            parse_args(&args("judge --profile perf"))
                .unwrap()
                .profile
                .as_deref(),
            Some("perf")
        );
        assert!(parse_args(&args("judge --release --profile perf")).is_err());
        assert!(parse_args(&args("judge --profile unknown")).is_err());
        assert!(parse_args(&args("pin --report counts.json")).is_err());
        let r = parse_args(&args("judge --trace rows.jsonl")).unwrap();
        assert_eq!(r.trace, Some(PathBuf::from("rows.jsonl")));
        let r = parse_args(&args("pin --rows 100")).unwrap();
        assert!(r.verb == Verb::Pin && r.bank == Bank::Full && r.rows == 100);
        let r = parse_args(&args("read --func gammaln --timeout 60")).unwrap();
        assert!(
            r.verb == Verb::Read && r.function.as_deref() == Some("GAMMALN") && r.timeout == 60
        );
        assert!(parse_args(&args("read")).is_err());
        assert!(parse_args(&args("pin --bank pinned")).is_err());
        assert!(parse_args(&args("judge --rows 3")).is_err());
        assert!(parse_args(&args("pin --trace rows.jsonl")).is_err());
        assert!(parse_args(&args("read --func GAMMA --trace rows.jsonl")).is_err());
        assert!(parse_args(&args("export")).is_err());
        let export = parse_args(&args("export --report rows.jsonl")).unwrap();
        assert!(export.verb == Verb::Export && export.bank == Bank::Full);
        assert!(parse_args(&args("race")).is_err());
        let r = parse_args(&args("compare --before old.jsonl --after new.jsonl")).unwrap();
        assert_eq!(r.verb, Verb::Compare);
        assert_eq!(r.before, Some(PathBuf::from("old.jsonl")));
        assert_eq!(r.after, Some(PathBuf::from("new.jsonl")));
        assert!(parse_args(&args("compare --before old.jsonl")).is_err());
        assert!(parse_args(&args("judge --after new.jsonl")).is_err());
        assert!(parse_args(&args("compare --before a --after b --func GAMMA")).is_err());
        let r = parse_args(&args("refresh --func erfc")).unwrap();
        assert!(r.verb == Verb::Refresh && r.bank == Bank::Pinned);
        assert_eq!(r.function.as_deref(), Some("ERFC"));
        assert!(parse_args(&args("refresh --rows 12")).is_err());
        assert!(parse_args(&args("refresh --bank full")).is_err());
    }

    #[test]
    fn context_refresh_preserves_the_sample_and_splits_same_argument_transports() {
        let args = vec![json!("0x0000000000000000")];
        let base = environment("r0".into(), "OxFunc-Evidence bank".into(), None, None, None);
        let mut banks = Banks {
            sources: vec![],
            readings: vec![],
            files_read: 1,
            import_notes: vec![],
            unread_rows: BTreeMap::new(),
        };
        let mut witnesses = Vec::new();
        for (id, transport, expected) in [
            ("cell", "cell_value2_bulk", "logical:True"),
            (
                "literal",
                "formula_literals_with_cell_echo",
                "logical:False",
            ),
        ] {
            let mut env = base.clone();
            env.source = format!("{CHECKOUT} bank");
            env.invocation = Some(calc_verdict::WitnessInvocation {
                input_transport: Some(transport.into()),
                ..Default::default()
            });
            let source = banks.sources.len();
            banks.sources.push(Source {
                path: "bank".into(),
                env,
            });
            banks.readings.push(Reading {
                function: "ISREF".into(),
                id: id.into(),
                args: args.clone(),
                index: None,
                expected: expected.into(),
                source,
            });
            witnesses.push(WitnessRow {
                id: id.into(),
                args: args.clone(),
                result_index: None,
                expected: expected.into(),
                reading: "r0".into(),
                calc_func: None,
            });
        }
        let set = WitnessSet {
            function: "ISREF".into(),
            selection: "original sample".into(),
            readings: vec![base],
            witnesses,
        };
        let original: Value = serde_json::from_str(&set.encode()).unwrap();
        let mut sets = vec![(PathBuf::from("ISREF.json"), set)];
        assert_eq!(restore_pinned_context(&mut sets, &banks).unwrap(), 2);
        let updated = &sets[0].1;
        assert_ne!(updated.witnesses[0].reading, updated.witnesses[1].reading);
        let enriched: Value = serde_json::from_str(&updated.encode()).unwrap();
        assert_eq!(
            without_context_and_prediction(original.clone()).unwrap(),
            without_context_and_prediction(enriched.clone()).unwrap()
        );
        let mut changed_answer = enriched.clone();
        changed_answer["witnesses"][0]["expected_bits"] = json!("logical:False");
        assert_ne!(
            without_context_and_prediction(original.clone()).unwrap(),
            without_context_and_prediction(changed_answer).unwrap()
        );
        let mut changed_source = enriched;
        changed_source["readings"][0]["source"] = json!("another bank");
        assert_ne!(
            without_context_and_prediction(original).unwrap(),
            without_context_and_prediction(changed_source).unwrap()
        );
        assert_eq!(restore_pinned_context(&mut sets, &banks).unwrap(), 0);
    }

    #[test]
    fn refresh_updates_only_predictions_and_refuses_new_pinned_losses() {
        let mut row = WitnessRow {
            id: "original".into(),
            args: vec![serde_json::json!("0x8000000000000000")],
            result_index: Some((1, 2)),
            expected: "0x3ff0000000000000".into(),
            reading: "r0".into(),
            calc_func: Some("0x3ff0000000000003".into()),
        };
        let original = row.clone();
        assert!(refresh_prediction(&mut row, "0x3ff0000000000002", None).unwrap());
        let prediction = row.calc_func.clone();
        row.calc_func = original.calc_func.clone();
        assert_eq!(row, original);
        row.calc_func = prediction;
        assert!(refresh_prediction(&mut row, "0x3ff0000000000000", None).unwrap());
        assert!(row.calc_func.is_none());
        let matched = row.clone();
        assert!(refresh_prediction(&mut row, "0x3ff0000000000001", None).is_err());
        assert!(refresh_prediction(&mut row, "fault:Timeout", None).is_err());
        assert!(refresh_prediction(&mut row, "fault:Panic", None).is_err());
        assert_eq!(row, matched);
        // A classified loss is accepted and keeps the calculation as the prediction; a listed
        // row that did not lose its match is refused.
        assert!(
            refresh_prediction(&mut row, "0x3ff0000000000001", Some("compensating error")).unwrap()
        );
        assert_eq!(row.calc_func.as_deref(), Some("0x3ff0000000000001"));
        assert!(
            refresh_prediction(&mut row, "0x3ff0000000000002", Some("compensating error")).is_err()
        );
    }

    fn comparison_row(id: usize, expected: &str, actual: &str) -> Value {
        serde_json::json!({
            "function": "GAMMA", "id": id.to_string(), "args": ["0x3ff0000000000000"],
            "result_index": null, "expected": expected, "actual": actual,
            "source": "recorded-bank", "reading": {"excel_build": "20430"}
        })
    }

    fn trace_text(rows: &[Value]) -> String {
        rows.iter().map(|r| format!("{r}\n")).collect()
    }

    #[test]
    fn comparison_counts_gains_losses_and_refusals_with_the_existing_verdict() {
        let expected = "0x3ff0000000000000";
        let old = trace_text(&[
            comparison_row(0, expected, "fault:NotImplemented"),
            comparison_row(1, expected, expected),
            comparison_row(2, expected, "0x3ff0000000000001"),
            comparison_row(3, "error:Num", "error:Num"),
        ]);
        let new = trace_text(&[
            comparison_row(0, expected, expected),
            comparison_row(1, expected, "0x3ff0000000000001"),
            comparison_row(2, expected, "fault:NotImplemented"),
            comparison_row(3, "error:Num", "error:Num"),
        ]);
        let counts = compare_traces(old.as_bytes(), new.as_bytes()).unwrap();
        assert_eq!(
            counts["GAMMA"],
            TraceCounts {
                rows: 4,
                before: 2,
                after: 2,
                gained: 1,
                lost: 1,
                changed: 3,
            }
        );
    }

    #[test]
    fn comparison_rejects_changed_identity_answers_lengths_and_malformed_rows() {
        let row = comparison_row(0, "0x3ff0000000000000", "0x3ff0000000000001");
        let old = trace_text(&[row.clone()]);
        for (field, value) in [
            ("expected", serde_json::json!("0x3ff0000000000001")),
            ("args", serde_json::json!(["0x4000000000000000"])),
            ("id", serde_json::json!("different")),
            ("source", serde_json::json!("other-bank")),
            ("reading", serde_json::json!({"excel_build": "20131"})),
            ("result_index", serde_json::json!([1, 0])),
            (
                "future_metadata",
                serde_json::json!("must also remain fixed"),
            ),
        ] {
            let mut changed = row.clone();
            changed[field] = value;
            assert!(
                compare_traces(old.as_bytes(), trace_text(&[changed]).as_bytes()).is_err(),
                "{field}"
            );
        }
        assert!(compare_traces(old.as_bytes(), b"".as_slice()).is_err());
        assert!(compare_traces(b"".as_slice(), old.as_bytes()).is_err());
        assert!(compare_traces(b"".as_slice(), b"".as_slice()).is_err());
        assert!(compare_traces(b"{}\n".as_slice(), b"{}\n".as_slice()).is_err());
        assert!(compare_traces(b"broken\n".as_slice(), b"broken\n".as_slice()).is_err());
    }

    #[test]
    fn supplied_inputs_are_explicit_and_novelty_uses_argument_bits() {
        let r = parse_args(&args(
            "read --func erf.precise --inputs fresh.json --new-only",
        ))
        .unwrap();
        assert!(r.new_only && r.inputs == Some(PathBuf::from("fresh.json")));
        assert!(parse_args(&args("read --func GAMMA --new-only")).is_err());
        assert!(parse_args(&args("read --func ../outside --inputs fresh.json")).is_err());
        assert!(parse_args(&args("judge --inputs fresh.json")).is_err());
        assert!(parse_args(&args("read --func GAMMA,ERF --inputs fresh.json")).is_err());
        assert_eq!(
            input_key(&[json!("0x3FF0000000000000")]),
            input_key(&[json!("0x3ff0000000000000")])
        );
        assert_ne!(
            input_key(&[json!("0x0000000000000000")]),
            input_key(&[json!("0x8000000000000000")])
        );
        let mut doc =
            json!({"function":"GAMMA", "witnesses":[{"id":"a", "args":["0x3ff0000000000000"]}]});
        assert_eq!(
            supplied_inputs(&serde_json::to_vec(&doc).unwrap(), "GAMMA", false)
                .unwrap()
                .1
                .len(),
            1
        );
        assert!(supplied_inputs(&serde_json::to_vec(&doc).unwrap(), "ERF.PRECISE", false).is_err());
        doc["witnesses"][0]["args"][0] = json!("0x7ff0000000000000");
        assert!(supplied_inputs(&serde_json::to_vec(&doc).unwrap(), "GAMMA", false).is_err());
        doc["witnesses"][0]["args"][0] = json!("0x3ff0000000000000");
        doc["witnesses"][0]["result_index"] = json!([0, 1]);
        assert!(supplied_inputs(&serde_json::to_vec(&doc).unwrap(), "GAMMA", false).is_err());
        doc["witnesses"][0]
            .as_object_mut()
            .unwrap()
            .remove("result_index");
        let duplicate = doc["witnesses"][0].clone();
        doc["witnesses"].as_array_mut().unwrap().push(duplicate);
        assert!(
            supplied_inputs(&serde_json::to_vec(&doc).unwrap(), "GAMMA", false)
                .unwrap_err()
                .contains("duplicate input id")
        );
        doc["witnesses"][1]["id"] = json!("b");
        assert!(
            supplied_inputs(&serde_json::to_vec(&doc).unwrap(), "GAMMA", false)
                .unwrap_err()
                .contains("duplicate input call")
        );
        let repeated = supplied_inputs(&serde_json::to_vec(&doc).unwrap(), "GAMMA", true)
            .unwrap()
            .1;
        assert_eq!(
            repeated.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            ["a", "b"]
        );
        assert_eq!(repeated[0].args, repeated[1].args);
        assert!(
            parse_args(&args(
                "read --func GAMMA --inputs repeated.json --repeat-calls"
            ))
            .unwrap()
            .repeat_calls
        );
        assert!(parse_args(&args("read --func GAMMA --repeat-calls")).is_err());
        doc["witnesses"][1]["input_transport"] = json!("formula_literals");
        assert_eq!(
            supplied_inputs(&serde_json::to_vec(&doc).unwrap(), "GAMMA", false)
                .unwrap()
                .1
                .len(),
            2
        );
        doc["witnesses"][1]["args"][0] = json!(true);
        assert!(
            supplied_inputs(&serde_json::to_vec(&doc).unwrap(), "GAMMA", false)
                .unwrap_err()
                .contains("requires finite numbers")
        );
        doc["witnesses"][1]["input_transport"] = json!("unknown");
        assert!(
            supplied_inputs(&serde_json::to_vec(&doc).unwrap(), "GAMMA", false)
                .unwrap_err()
                .contains("unknown input_transport")
        );
    }

    #[test]
    fn trace_preserves_bits_selection_and_refusal_without_changing_the_reading() {
        let r = Reading {
            function: "MINVERSE".into(),
            id: "recorded\nrow".into(),
            args: vec![serde_json::json!([
                "0x8000000000000000",
                "0x0000000000000001"
            ])],
            index: Some((1, 0)),
            expected: "0x3ff0000000000000".into(),
            source: 0,
        };
        let row = Row {
            reading: &r,
            slow: false,
            builds: BTreeSet::new(),
            actual: "fault:ResultShape".into(),
            severity: Severity::Structural,
            ulp: None,
        };
        let source = Source {
            path: "recorded/bank".into(),
            env: environment("r0".into(), "recorded/bank".into(), None, None, None),
        };
        let mut bytes = Vec::new();
        write_trace_row(&mut bytes, &row, &source).unwrap();
        assert_eq!(bytes.iter().filter(|&&b| b == b'\n').count(), 1);
        let value: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["args"], Value::Array(r.args.clone()));
        assert_eq!(value["result_index"], serde_json::json!([1, 0]));
        assert_eq!(value["expected"], r.expected);
        assert_eq!(value["id"], r.id);
        assert_eq!(value["actual"], "fault:ResultShape");
        assert_eq!(value["source"], source.path);
        assert!(value["reading"]["cpu"].is_null());
    }

    #[test]
    fn a_gzip_member_inflates_with_or_without_a_limit() {
        let text = b"hello hello hello hello
"
        .repeat(400);
        let mut gz = vec![0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 3];
        gz.extend(miniz_oxide::deflate::compress_to_vec(&text, 6));
        gz.extend([0u8; 8]);
        assert_eq!(gunzip(&gz, None).unwrap(), text);
        assert_eq!(gunzip(&gz, Some(100)).unwrap(), text[..100]);
        assert!(deflate_body(&gz[..10]).is_err());
    }

    #[test]
    fn oracle_records_take_the_witness_encoding() {
        let n: Value =
            serde_json::json!({"kind": "number", "value": 1.0, "bits_hex": "0x3ff0000000000000"});
        assert_eq!(
            oracle_argument(&n),
            Some(Value::String("0x3ff0000000000000".into()))
        );
        let m: Value = serde_json::json!({"kind": "matrix", "rows": 2, "cols": 2, "bits_hex_row_major": [["0x1", "0x2", "0x3", "0x4"]]});
        assert_eq!(
            oracle_argument(&m),
            Some(serde_json::json!([["0x1", "0x2"], ["0x3", "0x4"]]))
        );
        assert_eq!(
            oracle_outcome(&serde_json::json!({"kind": "number", "bits_hex": "0x0"})).as_deref(),
            Some("0x0")
        );
        assert_eq!(
            oracle_outcome(&serde_json::json!({"kind": "error", "code": "#NUM!"})).as_deref(),
            Some("error:Num")
        );
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
        assert_eq!(
            recovered_selection(&row, &[probe.clone(), conflicting]),
            None
        );
        row["args"][0][0][0] = serde_json::json!("0x4000000000000000");
        assert_eq!(recovered_selection(&row, &[probe]), None);
    }

    #[test]
    fn prediction_columns_require_matching_inputs_and_consistent_batch_selections() {
        let args = serde_json::json!([["y1", "y2"], ["x1", "x2"], ["n1", "n2"], true]);
        let first = serde_json::json!({"id": "first", "args": args, "result_index": [1, 1]});
        let second = serde_json::json!({"id": "second", "args": args, "result_index": [2, 1]});
        let columns = prediction_column_inputs("GROWTH", &[first.clone(), second.clone()]);
        let mut restored = args.clone();
        assert!(restore_prediction_column(&mut restored, &columns));
        assert_eq!(
            restored,
            serde_json::json!([["y1", "y2"], ["x1", "x2"], [["n1"], ["n2"]], true])
        );
        let mut other = args.clone();
        other[2][0] = serde_json::json!("other input");
        assert!(!restore_prediction_column(&mut other, &columns));
        assert!(prediction_column_inputs("GROWTH", &[first.clone()]).is_empty());
        assert!(prediction_column_inputs("LOGEST", &[second.clone()]).is_empty());
        let mut contradictory = second.clone();
        contradictory["result_index"] = serde_json::json!([1, 2]);
        assert!(prediction_column_inputs("GROWTH", &[first, second, contradictory]).is_empty());
    }

    #[test]
    fn duplicated_hex_prefixes_keep_the_recorded_bits() {
        let mut row = serde_json::json!([
            "0x0x3e22024955d1ccf5",
            ["0x0x8000000000000000"],
            "0x0xnot-a-number"
        ]);
        normalize_bank_hex(&mut row);
        assert_eq!(
            row,
            serde_json::json!([
                "0x3e22024955d1ccf5",
                ["0x8000000000000000"],
                "0x0xnot-a-number"
            ])
        );
    }

    #[test]
    fn powershell_bom_does_not_drop_recorded_rows() {
        let raw = b"\xef\xbb\xbf{\"function\":\"ERF.PRECISE\",\"witnesses\":[{\"id\":\"row\"}]}";
        let doc = bank_json(raw).unwrap();
        assert_eq!(doc["function"], "ERF.PRECISE");
        assert_eq!(doc["witnesses"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn full_bank_keeps_explicit_unread_capture_rows_as_failures() {
        let root =
            std::env::temp_dir().join(format!("xtask-witness-unread-{}", std::process::id()));
        let dir = root.join("readings/excel-20527");
        fs::create_dir_all(&dir).unwrap();
        for manifest in MANIFESTS {
            fs::write(root.join(manifest), "path\n").unwrap();
        }
        let path = dir.join("POISSON.DIST.json");
        let mut doc = json!({
            "function": "POISSON.DIST",
            "witnesses": [{"id": "valid", "args": [0, 1, false], "expected_bits": "0x3fd78b56362cef38"}],
            "unread": [
                {"id": "changed-input", "reason": "Excel changed an input through Value2"},
                "no result: worker did not return a value"
            ],
            "transport_audit_readings": [
                {"id":"literal", "args":["0x3ff0000000000000"], "observed_raw":{"number":"3ff0000000000000"},
                 "input_transport":"formula_literals_with_cell_echo", "semantic_witness":false,"reading_failure":null},
                {"id":"changed-literal", "args":["0x3ff0000000000000"], "observed_raw":{"number":"3ff0000000000001"},
                 "input_transport":"formula_literals_with_cell_echo", "semantic_witness":false,"reading_failure":"changed input"}
            ]
        });
        fs::write(&path, serde_json::to_vec(&doc).unwrap()).unwrap();
        let (banks, unread) = load_full(&root, Some("POISSON.DIST")).unwrap();
        assert_eq!(banks.files_read, 1);
        assert_eq!(banks.readings.len(), 1);
        assert_eq!(banks.unread_rows.get("POISSON.DIST"), Some(&2));
        assert_eq!(
            banks.readings.len() + banks.unread_rows.values().sum::<usize>(),
            3
        );
        assert_eq!(unread.len(), 2);
        assert!(unread[0].contains("changed-input"));
        assert!(
            banks
                .import_notes
                .iter()
                .any(|n| n.contains("2 transport audit observations, 1 failed audit obligations"))
        );
        assert!(
            !banks
                .readings
                .iter()
                .any(|r| r.id == "literal" || r.id == "changed-literal")
        );
        let (other, _) = load_full(&root, Some("GAMMA")).unwrap();
        assert!(other.readings.is_empty() && other.unread_rows.is_empty());
        doc["unread"] = json!("unrecognized capture summary");
        fs::write(&path, serde_json::to_vec(&doc).unwrap()).unwrap();
        let Err(error) = load_full(&root, Some("POISSON.DIST")) else {
            panic!("malformed unread count accepted")
        };
        assert!(error.contains("unread must be an array"));
        doc["unread"] = json!([]);
        doc["transport_audit_readings"][0]["semantic_witness"] = json!(true);
        fs::write(&path, serde_json::to_vec(&doc).unwrap()).unwrap();
        assert!(
            load_full(&root, Some("POISSON.DIST"))
                .err()
                .unwrap()
                .contains("malformed transport audit")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn csv_fields_keep_quoted_commas() {
        assert_eq!(csv_fields("a,\"b,c\",\"d\"\"e\""), vec!["a", "b,c", "d\"e"]);
    }
}
