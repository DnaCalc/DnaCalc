//! Live Excel comparison via Touchstone (`touchstone-client`), adaptive densification loop,
//! multi-worker bulk + interactive pipelining, and parallel 64-point Excel frontier bisection
//! (`dc-ztvv.21`).
//!
//! Predictions are ALWAYS written and SHA-256-hashed BEFORE any Touchstone request is submitted.
//! Only rows where Excel's answer genuinely differed (and input readback matched bit-for-bit)
//! are recorded as discrepancies and exported to `bank-rows`.

use super::boundary::{
    Frontier, expand_frontier_windows_with_refs, get_axis_bits, predicate_tag,
    record_excel_window_flips, set_axis_f64,
};
use super::cluster::{
    DiscrepancyRow, RegionStats, RowOutcome, build_clusters, cluster_key_for, dna_fault_kind,
    is_dna_fault, judge_pair,
};
use super::domain::{DomainSpec, ParamKind};
use super::fractal::{
    FractalConfig, FractalDiscoveryStats, coarse_discrepancy_probe_points, fractal_discover_local,
};
use super::hints;
use super::rng::{
    Rng, StreamCaseHasher, canonical_case_key, compact_to_f64, f64_to_compact, parse_hex_bits,
    step_ulps,
};
use super::steer::{
    CoverageTracker, FuzzCase, evaluate_traced, generate_steered_round, touchstone_bulk_context,
};
use super::{FuzzConfig, FuzzSummary, freeze_predictions, main_round_budget, write_outputs};
use calc_verdict::Severity;
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use touchstone_client::{Answer, Arg, Client, Error, Handle, Priority, RangeCell, Request};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(300);
const INTERACTIVE_CHUNK_SIZE: usize = 96;
const MAX_FRONTIER_SEEDS: usize = 8_000;
const MAX_DISCREPANCY_PARENTS: usize = 2_048;

const SWEEP_WAIT: Duration = Duration::from_secs(15);

#[derive(Clone, Debug)]
pub(super) struct SweepPolicy {
    pause: PathBuf,
    bank_prefix: PathBuf,
    next_chunk: Cell<usize>,
    envs: RefCell<BTreeMap<String, Value>>,
    banked_keys: RefCell<HashSet<String>>,
    recorded_files: RefCell<Vec<PathBuf>>,
    exported_rows: Cell<usize>,
}

impl SweepPolicy {
    pub fn new(pause: PathBuf, bank_prefix: PathBuf) -> Self {
        Self {
            pause,
            bank_prefix,
            next_chunk: Cell::new(0),
            envs: RefCell::new(BTreeMap::new()),
            banked_keys: RefCell::new(HashSet::new()),
            recorded_files: RefCell::new(Vec::new()),
            exported_rows: Cell::new(0),
        }
    }

    /// Build a sweep policy for `(function, seed, attempt)` that pre-loads any discrepancy keys
    /// already written by earlier interrupted attempts of the same `(function, seed)` recipe so
    /// resuming never writes duplicate bank-row WitnessSets.
    pub fn for_attempt(
        pause: PathBuf,
        bank_dir: &Path,
        function: &str,
        seed: u64,
        attempt: u64,
    ) -> Result<Self, String> {
        let mut banked_keys = HashSet::new();
        let mut recorded_files = Vec::new();
        let mut exported_rows = 0usize;
        let recipe_prefix = format!("{function}-{seed}-");
        if bank_dir.is_dir() {
            let mut prior_paths: Vec<PathBuf> = std::fs::read_dir(bank_dir)
                .map_err(|e| format!("read {}: {e}", bank_dir.display()))?
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| {
                    path.is_file()
                        && path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .is_some_and(|n| n.starts_with(&recipe_prefix) && n.ends_with(".json"))
                })
                .collect();
            prior_paths.sort();
            for path in prior_paths {
                let bytes = std::fs::read(&path)
                    .map_err(|e| format!("read {}: {e}", path.display()))?;
                let ws = calc_verdict::WitnessSet::decode(&bytes)
                    .map_err(|e| format!("decode {}: {e}", path.display()))?;
                for w in &ws.witnesses {
                    if banked_keys.insert(canonical_case_key(&ws.function, &w.args)) {
                        exported_rows += 1;
                    }
                }
                recorded_files.push(path);
            }
        }
        Ok(Self {
            pause,
            bank_prefix: bank_dir.join(format!("{function}-{seed}-{attempt:08}")),
            next_chunk: Cell::new(0),
            envs: RefCell::new(BTreeMap::new()),
            banked_keys: RefCell::new(banked_keys),
            recorded_files: RefCell::new(recorded_files),
            exported_rows: Cell::new(exported_rows),
        })
    }

    pub(super) fn exported_rows(&self) -> usize {
        self.exported_rows.get()
    }

    pub(super) fn recorded_files(&self) -> Vec<PathBuf> {
        let mut files = self.recorded_files.borrow().clone();
        files.sort();
        files
    }

    fn record(
        &self,
        cfg: &FuzzConfig,
        envs: &BTreeMap<String, Value>,
        differences: &[DiscrepancyRow],
    ) -> Result<(), String> {
        let mut all = self.envs.borrow_mut();
        let count = all.len();
        all.extend(envs.clone());
        if all.len() != count {
            super::sweep::write_json(&cfg.out_dir.join("excel-environments.json"), &*all)?;
        }
        if differences.is_empty() {
            return Ok(());
        }
        let mut banked = self.banked_keys.borrow_mut();
        let fresh: Vec<&DiscrepancyRow> = differences
            .iter()
            .filter(|d| banked.insert(canonical_case_key(&cfg.function, &d.case.args)))
            .collect();
        if fresh.is_empty() {
            return Ok(());
        }
        let index = self.next_chunk.get();
        self.next_chunk.set(index + 1);
        let name = self
            .bank_prefix
            .file_name()
            .ok_or("missing sweep bank prefix")?
            .to_string_lossy();
        let path = self
            .bank_prefix
            .with_file_name(format!("{name}-{index:04}.json"));
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let encoded = super::cluster::encode_witness_set_json(
            &cfg.function,
            &cfg.lane,
            &cfg.out_dir.join("attempt.json").display().to_string(),
            "Touchstone sweep discrepancies",
            &super::utc_timestamp_now(),
            &all,
            &touchstone_bulk_context(),
            &fresh,
        )?;
        let temp = path.with_extension("tmp");
        std::fs::write(&temp, encoded)
            .and_then(|_| std::fs::rename(temp, &path))
            .map_err(|e| format!("{}: {e}", path.display()))?;
        self.recorded_files.borrow_mut().push(path);
        self.exported_rows
            .set(self.exported_rows.get().saturating_add(fresh.len()));
        Ok(())
    }
}

fn sweep_paused(config: Option<&FuzzConfig>) -> bool {
    config
        .and_then(|cfg| cfg.sweep.as_ref())
        .is_some_and(|policy| policy.pause.exists())
}

fn error_variant_to_excel(name: &str) -> Result<&'static str, String> {
    match name {
        "Null" | "#NULL!" => Ok("#NULL!"),
        "Div0" | "#DIV/0!" => Ok("#DIV/0!"),
        "Value" | "#VALUE!" => Ok("#VALUE!"),
        "Ref" | "#REF!" => Ok("#REF!"),
        "Name" | "#NAME?" => Ok("#NAME?"),
        "Num" | "#NUM!" => Ok("#NUM!"),
        "NA" | "#N/A" => Ok("#N/A"),
        other => Err(format!("unknown Excel error variant {other:?}")),
    }
}

fn excel_error_to_encoded(code: &str) -> String {
    let variant = match code {
        "#NULL!" => "Null",
        "#DIV/0!" => "Div0",
        "#VALUE!" => "Value",
        "#REF!" => "Ref",
        "#NAME?" => "Name",
        "#NUM!" => "Num",
        "#N/A" => "NA",
        other => other.trim_start_matches('#'),
    };
    format!("error:{variant}")
}

fn json_cell_to_range_cell(cell: &Value) -> Result<RangeCell, String> {
    match cell {
        Value::Null => Ok(RangeCell::Blank),
        Value::Bool(b) => Ok(RangeCell::Boolean(*b)),
        Value::String(s) => {
            if let Some(bits) = parse_hex_bits(cell) {
                Ok(RangeCell::Number(bits))
            } else {
                Ok(RangeCell::Text(s.clone()))
            }
        }
        Value::Object(map) => {
            if let Some(code) = map.get("error").and_then(Value::as_str) {
                Ok(RangeCell::Error(error_variant_to_excel(code)?.to_owned()))
            } else {
                Err(format!("unsupported object cell in range: {cell}"))
            }
        }
        other => Err(format!("unsupported cell in range: {other}")),
    }
}

/// Convert one WitnessSet-encoded JSON argument to a Touchstone `Arg`.
pub fn json_arg_to_touchstone(value: &Value) -> Result<Arg, String> {
    match value {
        Value::Null => Ok(Arg::Blank),
        Value::Bool(b) => Ok(Arg::Boolean(*b)),
        Value::String(s) => {
            if let Some(bits) = parse_hex_bits(value) {
                Ok(Arg::Number(bits))
            } else {
                Ok(Arg::Text(s.clone()))
            }
        }
        Value::Object(map) => {
            if let Some(code) = map.get("error").and_then(Value::as_str) {
                Ok(Arg::Error(error_variant_to_excel(code)?.to_owned()))
            } else if let Some(range) = map.get("range").and_then(Value::as_array) {
                if range.is_empty() {
                    return Err("empty range argument".into());
                }
                let (rows, cols, cells) = if range[0].is_array() {
                    let rows = range.len();
                    let cols = range[0].as_array().map_or(0, Vec::len);
                    if cols == 0 {
                        return Err("zero-column range argument".into());
                    }
                    let mut cells = Vec::with_capacity(rows * cols);
                    for row in range {
                        let row_cells = row
                            .as_array()
                            .ok_or_else(|| "ragged range argument".to_owned())?;
                        if row_cells.len() != cols {
                            return Err("ragged range argument".into());
                        }
                        for cell in row_cells {
                            cells.push(json_cell_to_range_cell(cell)?);
                        }
                    }
                    (rows, cols, cells)
                } else {
                    let mut cells = Vec::with_capacity(range.len());
                    for cell in range {
                        cells.push(json_cell_to_range_cell(cell)?);
                    }
                    (range.len(), 1, cells)
                };
                if rows > 32 || cols > 32 || rows * cols > 128 {
                    return Err(format!(
                        "range {rows}x{cols} exceeds Touchstone rRxC bound (32x32, <=128 cells)"
                    ));
                }
                Ok(Arg::Range { rows, cols, cells })
            } else {
                Err(format!("unsupported object argument: {value}"))
            }
        }
        Value::Array(items) => {
            if items.is_empty() {
                return Err("empty array argument".into());
            }
            if items[0].is_array() {
                let rows = items.len();
                let cols = items[0]
                    .as_array()
                    .map_or(0, Vec::len);
                if cols == 0 {
                    return Err("zero-column array argument".into());
                }
                let mut bits = Vec::with_capacity(rows * cols);
                for row in items {
                    let cells = row
                        .as_array()
                        .ok_or_else(|| "ragged array argument".to_owned())?;
                    if cells.len() != cols {
                        return Err("ragged array argument".into());
                    }
                    for cell in cells {
                        let b = parse_hex_bits(cell)
                            .ok_or_else(|| format!("non-numeric cell in numeric array: {cell}"))?;
                        bits.push(b);
                    }
                }
                Ok(Arg::Array { rows, cols, bits })
            } else {
                let mut bits = Vec::with_capacity(items.len());
                for cell in items {
                    let b = parse_hex_bits(cell)
                        .ok_or_else(|| format!("non-numeric cell in 1D array: {cell}"))?;
                    bits.push(b);
                }
                Ok(Arg::Array {
                    rows: items.len(),
                    cols: 1,
                    bits,
                })
            }
        }
        other => Err(format!("unsupported JSON argument: {other}")),
    }
}

/// Convert a Touchstone `Answer` into WitnessSet encoded form, or a terminal Unread/Crashed outcome.
pub fn answer_to_encoded(answer: &Answer) -> Result<String, RowOutcome> {
    match answer {
        Answer::Number(bits) => Ok(format!("0x{bits:016x}")),
        Answer::Boolean(b) => Ok(if *b { "logical:True" } else { "logical:False" }.to_owned()),
        Answer::Text(s) => Ok(format!("text:{s}")),
        Answer::Error(code) => Ok(excel_error_to_encoded(code)),
        Answer::Blank => Ok("empty".to_owned()),
        Answer::Unread { reason } => Err(RowOutcome::Unread {
            reason: reason.clone(),
        }),
        Answer::Crashed { detail } => Err(RowOutcome::Crashed {
            detail: detail.clone(),
        }),
        Answer::Array { .. } => Err(RowOutcome::Unread {
            reason: "unexpected spilled array output".to_owned(),
        }),
    }
}

fn echo_matches_range_cell(sent: &RangeCell, echoed: &Answer) -> bool {
    match (sent, echoed) {
        (RangeCell::Number(a), Answer::Number(b)) => a == b,
        (RangeCell::Text(a), Answer::Text(b)) => a == b,
        (RangeCell::Boolean(a), Answer::Boolean(b)) => a == b,
        (RangeCell::Error(a), Answer::Error(b)) => a.eq_ignore_ascii_case(b),
        (RangeCell::Blank, Answer::Blank) => true,
        _ => false,
    }
}

/// Verify that Excel's echoed input row matches the arguments we asked for bit-for-bit.
///
/// Touchstone's `echo` wire format (`Program.cs`, `PROTOCOL.md`) flattens `Arg::Array` (`aRxC`)
/// and `Arg::Range` (`rRxC`) into `R * C` consecutive scalar `Answer` cells in `ity`/`iv`, while
/// synthetic tests may echo an `Answer::Array`; accept both faithfully.
pub fn echo_row_matches_sent(sent: &[Arg], echoed: &[Answer]) -> bool {
    let mut pos = 0usize;
    for arg in sent {
        match arg {
            Arg::Array { bits, .. } => {
                if let Some(cell @ Answer::Array { .. }) = echoed.get(pos) {
                    if !echo_matches_arg(arg, cell) {
                        return false;
                    }
                    pos += 1;
                } else {
                    if pos + bits.len() > echoed.len() {
                        return false;
                    }
                    for (expected_bits, cell) in bits.iter().zip(&echoed[pos..pos + bits.len()]) {
                        if !matches!(cell, Answer::Number(got) if got == expected_bits) {
                            return false;
                        }
                    }
                    pos += bits.len();
                }
            }
            Arg::Range { cells, .. } => {
                if pos + cells.len() > echoed.len() {
                    return false;
                }
                for (expected_cell, got) in cells.iter().zip(&echoed[pos..pos + cells.len()]) {
                    if !echo_matches_range_cell(expected_cell, got) {
                        return false;
                    }
                }
                pos += cells.len();
            }
            _ => {
                let Some(cell) = echoed.get(pos) else {
                    return false;
                };
                if !echo_matches_arg(arg, cell) {
                    return false;
                }
                pos += 1;
            }
        }
    }
    pos == echoed.len()
}

/// Verify that Excel's echoed input matches the argument we asked for bit-for-bit.
pub fn echo_matches_arg(sent: &Arg, echoed: &Answer) -> bool {
    match (sent, echoed) {
        (Arg::Number(a), Answer::Number(b)) => a == b,
        (Arg::Boolean(a), Answer::Boolean(b)) => a == b,
        (Arg::Text(a) | Arg::TextLiteral(a), Answer::Text(b)) => a == b,
        (Arg::Error(a), Answer::Error(b)) => a.eq_ignore_ascii_case(b),
        (Arg::Blank, Answer::Blank) => true,
        (
            Arg::Array { rows, cols, bits },
            Answer::Array {
                rows: er,
                cols: ec,
                cells,
            },
        ) => {
            rows == er
                && cols == ec
                && cells.len() == bits.len()
                && bits
                    .iter()
                    .zip(cells)
                    .all(|(b, c)| matches!(c, Answer::Number(got) if got == b))
        }
        _ => false,
    }
}

pub(crate) fn connect_client(endpoint: Option<&Path>) -> Result<Client, String> {
    let client = match endpoint {
        None => Client::connect(),
        Some(path) => {
            let is_json = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("json"));
            if is_json {
                Client::connect_endpoint(path)
            } else {
                #[cfg(unix)]
                {
                    Client::connect_unix(path)
                }
                #[cfg(not(unix))]
                {
                    Client::connect_endpoint(path)
                }
            }
        }
    };
    client.map_err(|e| format!("touchstone connect failed: {e}"))
}

#[derive(Clone, Debug)]
pub(crate) struct JudgedRow {
    pub outcome: RowOutcome,
    pub excel_encoded: Option<String>,
    pub env_id: String,
}

pub(crate) struct BatchReplyAggregate {
    pub rows: Vec<JudgedRow>,
    pub envs: BTreeMap<String, Value>,
    pub worker_calls: BTreeMap<String, usize>,
    pub excel_seconds: f64,
}

struct PendingChunk {
    submitted: Instant,
    lane_tag: usize, // 0 = primary/bulk cases, 1 = secondary/interactive cases
    indices: Vec<usize>,
    sent_rows: Vec<Vec<Arg>>,
    handle: Handle,
}

fn prepare_sendable(cases: &[FuzzCase]) -> (Vec<(usize, Vec<Arg>)>, Vec<JudgedRow>) {
    let mut judged = vec![
        JudgedRow {
            outcome: RowOutcome::Unread {
                reason: "not sent".to_owned(),
            },
            excel_encoded: None,
            env_id: "unknown".to_owned(),
        };
        cases.len()
    ];
    let mut sendable = Vec::with_capacity(cases.len());
    for (idx, c) in cases.iter().enumerate() {
        // A DNA-side fault (`fault:*`) is a harness/coverage gap, never an Excel discrepancy:
        // record it locally without spending Touchstone Excel calls, bank rows, or cluster beads.
        if let Some(fault) = dna_fault_kind(&c.prediction) {
            judged[idx].outcome = RowOutcome::HarnessFault {
                fault: fault.to_owned(),
            };
            continue;
        }
        let mut converted = Vec::with_capacity(c.args.len());
        let mut bad = None;
        for a in &c.args {
            match json_arg_to_touchstone(a) {
                Ok(arg) => converted.push(arg),
                Err(e) => {
                    bad = Some(e);
                    break;
                }
            }
        }
        if let Some(reason) = bad {
            judged[idx].outcome = RowOutcome::Unread { reason };
        } else {
            sendable.push((idx, converted));
        }
    }
    (sendable, judged)
}

fn drain_one_chunk(
    chunk: PendingChunk,
    primary_cases: &[FuzzCase],
    secondary_cases: &[FuzzCase],
    primary_judged: &mut [JudgedRow],
    secondary_judged: &mut [JudgedRow],
    envs: &mut BTreeMap<String, Value>,
    worker_calls: &mut BTreeMap<String, usize>,
    config: Option<&FuzzConfig>,
) -> Result<(), String> {
    let sweep = config.and_then(|cfg| cfg.sweep.as_ref().map(|policy| (cfg, policy)));
    let timeout = if sweep.is_some() {
        SWEEP_WAIT.saturating_sub(chunk.submitted.elapsed())
    } else {
        REQUEST_TIMEOUT
    };
    let (reply, error) = match chunk.handle.wait_timeout_with_partial(timeout) {
        Ok(reply) => (Some(Box::new(reply)), None),
        Err((error, partial)) => (partial, Some(error)),
    };
    let terminal = match &error {
        Some(Error::Service { code, detail }) if sweep.is_some() => match code.as_str() {
            "refused" => Some(RowOutcome::Unread {
                reason: detail.clone(),
            }),
            "excel-crashed" | "excel-crash-budget" => Some(RowOutcome::Crashed {
                detail: detail.clone(),
            }),
            _ => None,
        },
        _ => None,
    };
    let (cases, judged) = if chunk.lane_tag == 0 {
        (primary_cases, primary_judged)
    } else {
        (secondary_cases, secondary_judged)
    };
    if let Some(terminal) = &terminal {
        for &index in &chunk.indices {
            judged[index].outcome = terminal.clone();
        }
    }
    if let Some(reply) = reply {
        for (k, v) in reply.envs {
            envs.entry(k).or_insert(v);
        }
        for (w, count) in reply.worker_calls {
            *worker_calls.entry(w).or_insert(0) += count;
        }
        let fallback_env = reply.env_id.unwrap_or_else(|| "unknown".to_owned());
        let inputs = reply.inputs.as_deref();
        for (pos, &case_idx) in chunk.indices.iter().enumerate() {
            let env_id = reply
                .env_by_row
                .get(pos)
                .and_then(Option::as_ref)
                .cloned()
                .unwrap_or_else(|| fallback_env.clone());
            let Some(answer) = reply.rows.get(pos) else {
                judged[case_idx] = JudgedRow {
                    outcome: RowOutcome::Unread {
                        reason: "missing answer row in Touchstone reply".to_owned(),
                    },
                    excel_encoded: None,
                    env_id,
                };
                continue;
            };
            let encoded = match answer_to_encoded(answer) {
                Ok(enc) => enc,
                Err(outcome) => {
                    judged[case_idx] = JudgedRow {
                        outcome: terminal.clone().unwrap_or(outcome),
                        excel_encoded: None,
                        env_id,
                    };
                    continue;
                }
            };
            // A sweep only banks verified answers, including replies recovered after an interruption.
            let echo = inputs.and_then(|rows| rows.get(pos));
            if echo.is_some_and(|row| !echo_row_matches_sent(&chunk.sent_rows[pos], row))
                || (sweep.is_some() && echo.is_none())
            {
                let sent = &chunk.sent_rows[pos];
                let echoed = echo.map_or(0, Vec::len);
                judged[case_idx] = JudgedRow {
                    outcome: RowOutcome::Unread {
                        reason: format!(
                            "echo input readback differed (sent {} args, got {} echo cells)",
                            sent.len(),
                            echoed
                        ),
                    },
                    excel_encoded: None,
                    env_id,
                };
                continue;
            }
            let pred = &cases[case_idx].prediction;
            let outcome = if let Some(fault) = dna_fault_kind(pred) {
                RowOutcome::HarnessFault {
                    fault: fault.to_owned(),
                }
            } else {
                let (severity, ulp) = judge_pair(&encoded, pred);
                if severity == Severity::Exact {
                    RowOutcome::Matched
                } else {
                    RowOutcome::Discrepancy {
                        expected: encoded.clone(),
                        severity,
                        ulp,
                    }
                }
            };
            judged[case_idx] = JudgedRow {
                outcome,
                excel_encoded: Some(encoded),
                env_id,
            };
        }
        if let Some((cfg, policy)) = sweep {
            let differences: Vec<_> = chunk
                .indices
                .iter()
                .filter_map(|&idx| {
                    let row = &judged[idx];
                    let RowOutcome::Discrepancy {
                        expected,
                        severity,
                        ulp,
                    } = &row.outcome
                    else {
                        return None;
                    };
                    let case = &cases[idx];
                    Some(DiscrepancyRow {
                        case: case.clone(),
                        expected: expected.clone(),
                        severity: *severity,
                        ulp: *ulp,
                        env_id: row.env_id.clone(),
                        cluster_key: cluster_key_for(
                            &cfg.function,
                            case,
                            expected,
                            *severity,
                            *ulp,
                        ),
                    })
                })
                .collect();
            policy.record(cfg, envs, &differences)?;
        }
    }
    if let Some(error) = error
        && terminal.is_none()
    {
        return Err(format!("touchstone request failed: {error}"));
    }
    Ok(())
}

/// Submit a primary `Bulk` round AND optional secondary `Interactive` boundary probes concurrently
/// on the same Touchstone client so bulk workers (`w1`, `w2`) and the interactive reserve (`w3`)
/// run simultaneously.
fn ask_bulk_and_interactive(
    client: &Client,
    function: &str,
    bulk_cases: &[FuzzCase],
    interactive_cases: &[FuzzCase],
    bulk_batch_size: usize,
    in_flight: usize,
    config: Option<&FuzzConfig>,
) -> Result<(BatchReplyAggregate, BatchReplyAggregate), String> {
    let started = Instant::now();
    let (bulk_sendable, mut bulk_judged) = prepare_sendable(bulk_cases);
    let (inter_sendable, mut inter_judged) = prepare_sendable(interactive_cases);

    let has_range = bulk_sendable
        .iter()
        .chain(inter_sendable.iter())
        .any(|(_, row)| row.iter().any(|arg| matches!(arg, Arg::Range { .. })));
    let sweep = config.is_some_and(|cfg| cfg.sweep.is_some());
    // Touchstone's `rRxC` path (`EvaluateSpecial`) runs one call at a time; in sweep mode, keep
    // range chunks short and serial so queued requests never hit the 12s sweep-deferral timeout.
    let effective_bulk_batch = if sweep && has_range {
        bulk_batch_size.clamp(1, 8)
    } else {
        bulk_batch_size.max(1)
    };
    let mut work_queue = VecDeque::new();
    // Interleave interactive chunks (<= 96 rows each) right after the first two bulk chunks
    // so w1 and w2 grab bulk work while w3 picks up interactive work in parallel.
    let bulk_chunks: Vec<(Vec<usize>, Vec<Vec<Arg>>)> = bulk_sendable
        .chunks(effective_bulk_batch)
        .map(|slice| {
            let mut idxs = Vec::with_capacity(slice.len());
            let mut rows = Vec::with_capacity(slice.len());
            for (i, r) in slice {
                idxs.push(*i);
                rows.push(r.clone());
            }
            (idxs, rows)
        })
        .collect();
    let inter_chunks: Vec<(Vec<usize>, Vec<Vec<Arg>>)> = inter_sendable
        .chunks(
            config
                .filter(|cfg| cfg.sweep.is_some())
                .map_or(INTERACTIVE_CHUNK_SIZE, |cfg| cfg.batch_size),
        )
        .map(|slice| {
            let mut idxs = Vec::with_capacity(slice.len());
            let mut rows = Vec::with_capacity(slice.len());
            for (i, r) in slice {
                idxs.push(*i);
                rows.push(r.clone());
            }
            (idxs, rows)
        })
        .collect();

    let mut b_it = bulk_chunks.into_iter();
    let mut i_it = inter_chunks.into_iter();
    for _ in 0..2 {
        if let Some((idxs, rows)) = b_it.next() {
            work_queue.push_back((0, Priority::Bulk, idxs, rows));
        }
    }
    loop {
        let mut pushed = false;
        if let Some((idxs, rows)) = i_it.next() {
            work_queue.push_back((1, Priority::Interactive, idxs, rows));
            pushed = true;
        }
        if let Some((idxs, rows)) = b_it.next() {
            work_queue.push_back((0, Priority::Bulk, idxs, rows));
            pushed = true;
        }
        if !pushed {
            break;
        }
    }

    let max_pending = if sweep && has_range {
        1
    } else {
        in_flight.max(1) + usize::from(!sweep && !interactive_cases.is_empty())
    };
    let (envs, worker_calls) = dispatch_chunks(
        client,
        function,
        work_queue,
        [bulk_cases, interactive_cases],
        [&mut bulk_judged, &mut inter_judged],
        max_pending,
        config,
    )?;

    let elapsed = started.elapsed().as_secs_f64();
    let bulk_agg = BatchReplyAggregate {
        rows: bulk_judged,
        envs: envs.clone(),
        worker_calls: worker_calls.clone(),
        excel_seconds: elapsed,
    };
    let inter_agg = BatchReplyAggregate {
        rows: inter_judged,
        envs,
        worker_calls,
        excel_seconds: 0.0,
    };
    Ok((bulk_agg, inter_agg))
}

/// Ask a single slice of cases with one priority (e.g. interactive boundary bisection).
pub(crate) fn ask_single_priority(
    client: &Client,
    function: &str,
    cases: &[FuzzCase],
    batch_size: usize,
    in_flight: usize,
    priority: Priority,
    config: Option<&FuzzConfig>,
) -> Result<BatchReplyAggregate, String> {
    if cases.is_empty() {
        return Ok(BatchReplyAggregate {
            rows: Vec::new(),
            envs: BTreeMap::new(),
            worker_calls: BTreeMap::new(),
            excel_seconds: 0.0,
        });
    }
    let started = Instant::now();
    let (sendable, mut judged) = prepare_sendable(cases);
    let (mut batch_size, mut in_flight) = config
        .filter(|cfg| cfg.sweep.is_some())
        .map_or((batch_size, in_flight), |cfg| {
            (cfg.batch_size, cfg.in_flight)
        });
    if config.is_some_and(|cfg| cfg.sweep.is_some())
        && sendable
            .iter()
            .any(|(_, row)| row.iter().any(|arg| matches!(arg, Arg::Range { .. })))
    {
        batch_size = batch_size.clamp(1, 8);
        in_flight = 1;
    }
    let chunks = sendable
        .chunks(batch_size.max(1))
        .map(|slice| {
            let (indices, rows) = slice.iter().cloned().unzip();
            (0, priority, indices, rows)
        })
        .collect();
    let (envs, worker_calls) = dispatch_chunks(
        client,
        function,
        chunks,
        [cases, &[]],
        [&mut judged, &mut []],
        in_flight.max(1),
        config,
    )?;
    Ok(BatchReplyAggregate {
        rows: judged,
        envs,
        worker_calls,
        excel_seconds: started.elapsed().as_secs_f64(),
    })
}

type RequestChunk = (usize, Priority, Vec<usize>, Vec<Vec<Arg>>);
type BatchProvenance = (BTreeMap<String, Value>, BTreeMap<String, usize>);

fn dispatch_chunks(
    client: &Client,
    function: &str,
    mut chunks: VecDeque<RequestChunk>,
    [primary_cases, secondary_cases]: [&[FuzzCase]; 2],
    [primary_judged, secondary_judged]: [&mut [JudgedRow]; 2],
    max_pending: usize,
    config: Option<&FuzzConfig>,
) -> Result<BatchProvenance, String> {
    let mut pending = VecDeque::with_capacity(max_pending);
    let mut envs = BTreeMap::new();
    let mut worker_calls = BTreeMap::new();
    let mut failure = None;
    let sweep = config.is_some_and(|cfg| cfg.sweep.is_some());
    loop {
        while failure.is_none() && !sweep_paused(config) && pending.len() < max_pending {
            let Some((lane_tag, priority, indices, sent_rows)) = chunks.pop_front() else {
                break;
            };
            let mut request = Request::new(function, sent_rows.clone());
            request.priority = Some(if sweep { Priority::Sweep } else { priority });
            request.echo_inputs = true;
            let submitted = Instant::now();
            match client.submit(request) {
                Ok(handle) => pending.push_back(PendingChunk {
                    submitted,
                    lane_tag,
                    indices,
                    sent_rows,
                    handle,
                }),
                Err(error) => failure = Some(format!("touchstone submit failed: {error}")),
            }
        }
        // Let already submitted work finish when pausing, so any observed differences are banked.
        let Some(chunk) = pending.pop_front() else {
            break;
        };
        if let Err(error) = drain_one_chunk(
            chunk,
            primary_cases,
            secondary_cases,
            primary_judged,
            secondary_judged,
            &mut envs,
            &mut worker_calls,
            config,
        ) {
            failure.get_or_insert(error);
            if !sweep {
                break;
            }
        }
    }
    if let Some(error) = failure {
        return Err(error);
    }
    if !chunks.is_empty() {
        return Err("sweep paused".into());
    }
    Ok((envs, worker_calls))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ingest_judged_cases(
    function: &str,
    cases: &[FuzzCase],
    judged: &[JudgedRow],
    excel_answers: &mut HashMap<String, String>,
    region_stats: &mut BTreeMap<String, RegionStats>,
    known_clusters: &mut HashSet<String>,
    discrepancies: &mut Vec<DiscrepancyRow>,
    discrepancy_parents: &mut Vec<(String, Vec<Value>)>,
    frontier_seeds: &mut Vec<FuzzCase>,
    total_asked: &mut usize,
    total_matched: &mut usize,
    total_mismatched: &mut usize,
    total_harness_fault: &mut usize,
    harness_faults: &mut BTreeMap<String, usize>,
    total_unread: &mut usize,
    total_crashed: &mut usize,
) -> (usize, HashSet<String>) {
    let mut new_clusters = 0usize;
    let mut discrepant_regions: HashSet<String> = HashSet::new();
    for (case, row) in cases.iter().zip(judged.iter()) {
        if let Some(enc) = &row.excel_encoded {
            excel_answers.insert(canonical_case_key(function, &case.args), enc.clone());
        }
        *total_asked += 1;
        let st = region_stats.entry(case.region.clone()).or_default();
        st.asked += 1;
        match &row.outcome {
            RowOutcome::Matched => {
                *total_matched += 1;
                st.matched += 1;
            }
            RowOutcome::Discrepancy {
                expected,
                severity,
                ulp,
            } => {
                *total_mismatched += 1;
                st.mismatched += 1;
                if let Some(u) = *ulp {
                    st.worst_ulp = Some(st.worst_ulp.map_or(u, |prev| prev.max(u)));
                }
                let ckey = cluster_key_for(function, case, expected, *severity, *ulp);
                if known_clusters.insert(ckey.clone()) {
                    new_clusters += 1;
                    st.clusters_seen += 1;
                }
                let base_region = case
                    .region
                    .strip_prefix("densify:")
                    .unwrap_or(&case.region)
                    .to_owned();
                discrepant_regions.insert(base_region.clone());
                if discrepancy_parents.len() < MAX_DISCREPANCY_PARENTS {
                    discrepancy_parents.push((base_region, case.args.clone()));
                }
                if frontier_seeds.len() < MAX_FRONTIER_SEEDS {
                    frontier_seeds.push(case.clone());
                }
                discrepancies.push(DiscrepancyRow {
                    case: case.clone(),
                    expected: expected.clone(),
                    severity: *severity,
                    ulp: *ulp,
                    env_id: row.env_id.clone(),
                    cluster_key: ckey,
                });
            }
            RowOutcome::HarnessFault { fault } => {
                *total_harness_fault += 1;
                st.harness_fault += 1;
                *harness_faults.entry(fault.clone()).or_insert(0) += 1;
            }
            RowOutcome::Unread { .. } => {
                *total_unread += 1;
                st.unread += 1;
            }
            RowOutcome::Crashed { .. } => {
                *total_crashed += 1;
                st.crashed += 1;
            }
        }
    }
    (new_clusters, discrepant_regions)
}

/// Choose the opposite-predicate bracket `(lo_bits, lo_out, hi_bits, hi_out)` whose possible
/// adjacent flip position is closest in Value2-compact ULPs to `frontier.dna_lo_bits`, breaking
/// ties by matching DNA Calc's transition direction (`dna_lo_tag -> dna_hi_tag`) and then midpoint.
fn select_closest_flip_bracket(
    frontier: &Frontier,
    pts: &mut Vec<(u64, String)>,
) -> Option<(u64, String, u64, String)> {
    pts.sort_by_key(|(b, _)| f64_to_compact(f64::from_bits(*b)));
    pts.dedup_by_key(|(b, _)| *b);
    let dna_c = f64_to_compact(f64::from_bits(frontier.dna_lo_bits));
    let dna_lo_tag = predicate_tag(&frontier.dna_lo_outcome);
    let dna_hi_tag = predicate_tag(&frontier.dna_hi_outcome);
    let mut best: Option<(u128, u8, u128, usize)> = None;
    for (idx, w) in pts.windows(2).enumerate() {
        if is_dna_fault(&w[0].1) || is_dna_fault(&w[1].1) {
            continue;
        }
        let p0 = predicate_tag(&w[0].1);
        let p1 = predicate_tag(&w[1].1);
        if p0 == p1 {
            continue;
        }
        let c0 = f64_to_compact(f64::from_bits(w[0].0));
        let c1 = f64_to_compact(f64::from_bits(w[1].0));
        if c1 <= c0 {
            continue;
        }
        // Any adjacent flip [k, k + 1] inside [c0, c1] has k in [c0, c1 - 1].
        let hi_k = c1 - 1;
        let min_dist: u128 = if dna_c < c0 {
            (c0 - dna_c) as u128
        } else if dna_c > hi_k {
            (dna_c - hi_k) as u128
        } else {
            0
        };
        let dir_pen = u8::from(!(p0 == dna_lo_tag && p1 == dna_hi_tag));
        let mid_dist = ((c0 + hi_k) - 2 * dna_c).unsigned_abs();
        let key = (min_dist, dir_pen, mid_dist);
        if best.is_none_or(|(bd, bp, bm, _)| key < (bd, bp, bm)) {
            best = Some((min_dist, dir_pen, mid_dist, idx));
        }
    }
    best.map(|(_, _, _, idx)| {
        (
            pts[idx].0,
            pts[idx].1.clone(),
            pts[idx + 1].0,
            pts[idx + 1].1.clone(),
        )
    })
}

/// For any `predicate_flip` frontier whose consecutive window around DNA Calc's flip did NOT
/// contain Excel's flip, bracket Excel's flip with an exponential probe and then pinpoint it with
/// batched 64-point ordered-bit bisection (freezing predictions before every Touchstone probe).
/// Non-predicate frontiers (`route_switch`, `iteration_regime`, `outcome_regime`) never trigger
/// shifted-flip bisection: Excel is probed only where DNA Calc predicts a predicate flip or where
/// a coarse discrepancy was already observed.
#[allow(clippy::too_many_arguments)]
fn bisect_shifted_frontiers_excel(
    cfg: &FuzzConfig,
    client: &Client,
    frontiers: &mut [Frontier],
    seen_keys: &mut HashSet<String>,
    excel_answers: &mut HashMap<String, String>,
    case_hasher: &mut StreamCaseHasher,
    pred_hashes: &mut Vec<String>,
    all_envs: &mut BTreeMap<String, Value>,
    all_worker_calls: &mut BTreeMap<String, usize>,
    region_stats: &mut BTreeMap<String, RegionStats>,
    known_clusters: &mut HashSet<String>,
    discrepancies: &mut Vec<DiscrepancyRow>,
    discrepancy_parents: &mut Vec<(String, Vec<Value>)>,
    frontier_seeds: &mut Vec<FuzzCase>,
    next_id: &mut usize,
    total_generated: &mut usize,
    total_asked: &mut usize,
    total_matched: &mut usize,
    total_mismatched: &mut usize,
    total_harness_fault: &mut usize,
    harness_faults: &mut BTreeMap<String, usize>,
    total_unread: &mut usize,
    total_crashed: &mut usize,
    excel_seconds: &mut f64,
    round: usize,
) -> Result<(), String> {
    let ctx = touchstone_bulk_context();
    let shifted_indices: Vec<usize> = frontiers
        .iter()
        .enumerate()
        .filter(|(_, f)| {
            f.transition_kind == "predicate_flip"
                && f.excel_lo_bits.is_none()
                && !is_dna_fault(&f.dna_lo_outcome)
                && !is_dna_fault(&f.dna_hi_outcome)
        })
        .map(|(i, _)| i)
        .take(16)
        .collect();
    if shifted_indices.is_empty() {
        return Ok(());
    }

    // Step 1: Exponential bracket expansion around DNA Calc's flip in one batched probe.
    // Includes offset 0 as a bracket reference (reusing its cached Excel answer when already in
    // `seen_keys` rather than re-asking or double-banking it).
    let offsets: &[i64] = &[
        -1_099_511_627_776,
        -16_777_216,
        -65_536,
        -1_024,
        -256,
        -64,
        0,
        64,
        256,
        1_024,
        65_536,
        16_777_216,
        1_099_511_627_776,
    ];
    let mut bracket_cases: Vec<FuzzCase> = Vec::new();
    let mut bracket_refs: Vec<(usize, u64, String)> = Vec::new();
    for &fidx in &shifted_indices {
        let f = &frontiers[fidx];
        let anchor = f64::from_bits(f.dna_lo_bits);
        let mut pts = Vec::new();
        for &d in offsets {
            if let Some(v) = step_ulps(anchor, d, true) {
                pts.push(v);
            }
        }
        // Also try geometric probes around the anchor so wide shifts (e.g. -1.0 -> -0.99) bracket cleanly.
        for &k in &[-2.0_f64, -1.0, -0.5, 0.5, 0.9, 0.99, 1.01, 1.1, 2.0] {
            let v = anchor * k;
            if v.is_finite() && !v.is_subnormal() {
                pts.push(v);
            }
        }
        for &a in &[-1.0_f64, -0.1, -0.01, -0.001, 0.001, 0.01, 0.1, 1.0] {
            let v = anchor + a;
            if v.is_finite() && !v.is_subnormal() {
                pts.push(v);
            }
        }
        let mut local_bits = HashSet::new();
        for v in pts {
            let b = if v == 0.0 { 0u64 } else { v.to_bits() };
            if !local_bits.insert(b) {
                continue;
            }
            let mut args = f.base_args.clone();
            if !set_axis_f64(&mut args, f.axis, f.axis_cell, v) {
                continue;
            }
            let key = canonical_case_key(&f.function, &args);
            bracket_refs.push((fidx, b, key.clone()));
            if !seen_keys.insert(key) {
                continue;
            }
            let (pred, trace, outcome_class) = evaluate_traced(&f.function, &args, &ctx);
            *next_id += 1;
            let region = format!("boundary_bisect:{}", f.axis_name);
            case_hasher.update_case(&region, &args);
            bracket_cases.push(FuzzCase {
                id: format!("fuzz-{}-b{round:02}-{:06}", f.function, *next_id),
                region,
                round,
                args,
                prediction: pred,
                trace,
                outcome_class,
            });
        }
    }
    if !bracket_cases.is_empty() {
        *total_generated += bracket_cases.len();
        let pred_path = cfg.out_dir.join(format!("bisect-{round:02}-bracket-predictions.jsonl"));
        pred_hashes.push(freeze_predictions(&pred_path, &cfg.function, &bracket_cases)?);
        let agg = ask_single_priority(
            client,
            &cfg.function,
            &bracket_cases,
            INTERACTIVE_CHUNK_SIZE,
            2,
            Priority::Interactive,
            Some(cfg),
        )?;
        *excel_seconds += agg.excel_seconds;
        for (k, v) in agg.envs {
            all_envs.entry(k).or_insert(v);
        }
        for (w, c) in agg.worker_calls {
            *all_worker_calls.entry(w).or_insert(0) += c;
        }
        let _ = ingest_judged_cases(
            &cfg.function,
            &bracket_cases,
            &agg.rows,
            excel_answers,
            region_stats,
            known_clusters,
            discrepancies,
            discrepancy_parents,
            frontier_seeds,
            total_asked,
            total_matched,
            total_mismatched,
            total_harness_fault,
            harness_faults,
            total_unread,
            total_crashed,
        );
    }

    // Find closest opposite-predicate brackets [lo_bits, hi_bits] per shifted frontier.
    let mut by_f: BTreeMap<usize, Vec<(u64, String)>> = BTreeMap::new();
    for (fidx, bits, key) in bracket_refs {
        if let Some(enc) = excel_answers.get(&key) {
            by_f.entry(fidx).or_default().push((bits, enc.clone()));
        }
    }
    let mut active_brackets: Vec<(usize, u64, String, u64, String)> = Vec::new();
    for (fidx, mut pts) in by_f {
        if let Some((lo_b, lo_o, hi_b, hi_o)) =
            select_closest_flip_bracket(&frontiers[fidx], &mut pts)
        {
            active_brackets.push((fidx, lo_b, lo_o, hi_b, hi_o));
        }
    }

    // Step 2: Up to 11 rounds of 64-point parallel ordered-bit bisection (64^11 = 2^66 > 2^64,
    // guaranteeing exact adjacent-double pinpointing across the full IEEE-754 double space).
    for step in 0..11 {
        if active_brackets.is_empty() {
            break;
        }
        let mut step_cases: Vec<FuzzCase> = Vec::new();
        let mut step_refs: Vec<(usize, u64, String)> = Vec::new();
        let mut still_open = Vec::new();
        for (bi, (fidx, lo_b, lo_o, hi_b, hi_o)) in active_brackets.iter().enumerate() {
            let ca = f64_to_compact(f64::from_bits(*lo_b));
            let cb = f64_to_compact(f64::from_bits(*hi_b));
            if cb == ca + 1 {
                let f = &mut frontiers[*fidx];
                f.excel_lo_bits = Some(*lo_b);
                f.excel_hi_bits = Some(*hi_b);
                f.excel_lo_outcome = Some(lo_o.clone());
                f.excel_hi_outcome = Some(hi_o.clone());
                let shift = ca - f64_to_compact(f64::from_bits(f.dna_lo_bits));
                f.shifted_ulps = i64::try_from(shift).ok();
                continue;
            }
            if cb <= ca {
                continue;
            }
            still_open.push(bi);
            let f = &frontiers[*fidx];
            let span = (cb - ca) as u128;
            let k_pts = span.saturating_sub(1).min(63) as usize;
            let mut local_bits = HashSet::new();
            for j in 1..=k_pts {
                let c = ca + ((span * j as u128) / ((k_pts + 1) as u128)) as i128;
                let Some(val) = compact_to_f64(c) else {
                    continue;
                };
                let b = if val == 0.0 { 0u64 } else { val.to_bits() };
                if b == *lo_b || b == *hi_b || !local_bits.insert(b) {
                    continue;
                }
                let mut args = f.base_args.clone();
                if !set_axis_f64(&mut args, f.axis, f.axis_cell, val) {
                    continue;
                }
                let key = canonical_case_key(&f.function, &args);
                step_refs.push((bi, b, key.clone()));
                if !seen_keys.insert(key) {
                    continue;
                }
                let (pred, trace, outcome_class) = evaluate_traced(&f.function, &args, &ctx);
                *next_id += 1;
                let region = format!("boundary_bisect:{}", f.axis_name);
                case_hasher.update_case(&region, &args);
                step_cases.push(FuzzCase {
                    id: format!("fuzz-{}-bs{step}-{:06}", f.function, *next_id),
                    region,
                    round,
                    args,
                    prediction: pred,
                    trace,
                    outcome_class,
                });
            }
        }
        if still_open.is_empty() {
            break;
        }
        if !step_cases.is_empty() {
            *total_generated += step_cases.len();
            let pred_path = cfg.out_dir.join(format!("bisect-{round:02}-step{step}-predictions.jsonl"));
            pred_hashes.push(freeze_predictions(&pred_path, &cfg.function, &step_cases)?);
            let agg = ask_single_priority(
                client,
                &cfg.function,
                &step_cases,
                INTERACTIVE_CHUNK_SIZE,
                2,
                Priority::Interactive,
                Some(cfg),
            )?;
            *excel_seconds += agg.excel_seconds;
            for (k, v) in agg.envs {
                all_envs.entry(k).or_insert(v);
            }
            for (w, c) in agg.worker_calls {
                *all_worker_calls.entry(w).or_insert(0) += c;
            }
            let _ = ingest_judged_cases(
                &cfg.function,
                &step_cases,
                &agg.rows,
                excel_answers,
                region_stats,
                known_clusters,
                discrepancies,
                discrepancy_parents,
                frontier_seeds,
                total_asked,
                total_matched,
                total_mismatched,
                total_harness_fault,
                harness_faults,
                total_unread,
                total_crashed,
            );
        }

        let mut pts_by_bi: BTreeMap<usize, Vec<(u64, String)>> = BTreeMap::new();
        for &bi in &still_open {
            let (_, lo_b, lo_o, hi_b, hi_o) = &active_brackets[bi];
            pts_by_bi.insert(bi, vec![(*lo_b, lo_o.clone()), (*hi_b, hi_o.clone())]);
        }
        for (bi, bits, key) in step_refs {
            if let Some(enc) = excel_answers.get(&key) {
                pts_by_bi.entry(bi).or_default().push((bits, enc.clone()));
            }
        }
        let mut next_active = Vec::new();
        for (bi, mut pts) in pts_by_bi {
            let (fidx, prev_lo, _, prev_hi, _) = active_brackets[bi];
            if let Some((lo_b, lo_o, hi_b, hi_o)) =
                select_closest_flip_bracket(&frontiers[fidx], &mut pts)
                && (lo_b != prev_lo || hi_b != prev_hi)
            {
                next_active.push((fidx, lo_b, lo_o, hi_b, hi_o));
            }
        }
        active_brackets = next_active;
    }

    // Record exact adjacent brackets for any frontier that converged on the final step.
    for (fidx, lo_b, lo_o, hi_b, hi_o) in active_brackets {
        let ca = f64_to_compact(f64::from_bits(lo_b));
        let cb = f64_to_compact(f64::from_bits(hi_b));
        let f = &mut frontiers[fidx];
        if f.excel_lo_bits.is_none() && cb == ca + 1 {
            f.excel_lo_bits = Some(lo_b);
            f.excel_hi_bits = Some(hi_b);
            f.excel_lo_outcome = Some(lo_o);
            f.excel_hi_outcome = Some(hi_o);
            let shift = ca - f64_to_compact(f64::from_bits(f.dna_lo_bits));
            f.shifted_ulps = i64::try_from(shift).ok();
        }
    }
    Ok(())
}

/// Select the bracket `(lo_bits, lo_out, hi_bits, hi_out)` closest to `anchor_bits` where
/// either Excel's predicate flips OR Excel's agreement with DNA Calc flips (`matched <-> discrepant`).
fn select_closest_discrepancy_bracket(
    anchor_bits: u64,
    pts: &mut Vec<(u64, String, String)>, // (axis_bits, excel_out, dna_pred)
) -> Option<(u64, String, u64, String, bool)> {
    pts.sort_by_key(|(b, _, _)| f64_to_compact(f64::from_bits(*b)));
    pts.dedup_by_key(|(b, _, _)| *b);
    let anchor_c = f64_to_compact(f64::from_bits(anchor_bits));
    let mut best: Option<(u8, u128, u128, usize, bool)> = None;
    for (idx, w) in pts.windows(2).enumerate() {
        if is_dna_fault(&w[0].1)
            || is_dna_fault(&w[1].1)
            || is_dna_fault(&w[0].2)
            || is_dna_fault(&w[1].2)
        {
            continue;
        }
        let pred_flip = predicate_tag(&w[0].1) != predicate_tag(&w[1].1);
        let disc_flip = (judge_pair(&w[0].1, &w[0].2).0 == Severity::Exact)
            != (judge_pair(&w[1].1, &w[1].2).0 == Severity::Exact);
        if !pred_flip && !disc_flip {
            continue;
        }
        let c0 = f64_to_compact(f64::from_bits(w[0].0));
        let c1 = f64_to_compact(f64::from_bits(w[1].0));
        if c1 <= c0 {
            continue;
        }
        let hi_k = c1 - 1;
        let min_dist: u128 = if anchor_c < c0 {
            (c0 - anchor_c) as u128
        } else if anchor_c > hi_k {
            (anchor_c - hi_k) as u128
        } else {
            0
        };
        // Prefer predicate flip over pure numeric discrepancy onset when distances tie.
        let kind_pen = u8::from(!pred_flip);
        let mid_dist = ((c0 + hi_k) - 2 * anchor_c).unsigned_abs();
        let key = (kind_pen, min_dist, mid_dist);
        if best.is_none_or(|(bk, bd, bm, _, _)| key < (bk, bd, bm)) {
            best = Some((kind_pen, min_dist, mid_dist, idx, pred_flip));
        }
    }
    best.map(|(_, _, _, idx, pred_flip)| {
        (
            pts[idx].0,
            pts[idx].1.clone(),
            pts[idx + 1].0,
            pts[idx + 1].1.clone(),
            pred_flip,
        )
    })
}

/// Fractal coarse-difference escalation (`dc-ztvv.26.5`): when a coarse sweep found Excel
/// discrepancies (including in regions where DNA Calc predicted no local flip), probe Decade +
/// Binade stencils along numeric axes around distinct discrepant exemplars, select the closest
/// Excel flip bracket, and pinpoint exact adjacent binary64 doubles via batched 64-point bisection.
#[allow(clippy::too_many_arguments)]
pub fn fractal_refine_coarse_discrepancies_excel(
    cfg: &FuzzConfig,
    spec: &DomainSpec,
    client: &Client,
    frontiers: &mut Vec<Frontier>,
    fractal_stats: &mut FractalDiscoveryStats,
    seen_keys: &mut HashSet<String>,
    excel_answers: &mut HashMap<String, String>,
    case_hasher: &mut StreamCaseHasher,
    pred_hashes: &mut Vec<String>,
    all_envs: &mut BTreeMap<String, Value>,
    all_worker_calls: &mut BTreeMap<String, usize>,
    region_stats: &mut BTreeMap<String, RegionStats>,
    known_clusters: &mut HashSet<String>,
    discrepancies: &mut Vec<DiscrepancyRow>,
    discrepancy_parents: &mut Vec<(String, Vec<Value>)>,
    frontier_seeds: &mut Vec<FuzzCase>,
    next_id: &mut usize,
    total_generated: &mut usize,
    total_asked: &mut usize,
    total_matched: &mut usize,
    total_mismatched: &mut usize,
    total_harness_fault: &mut usize,
    harness_faults: &mut BTreeMap<String, usize>,
    total_unread: &mut usize,
    total_crashed: &mut usize,
    excel_seconds: &mut f64,
    round: usize,
    max_targets: usize,
) -> Result<usize, String> {
    if discrepancies.is_empty() || max_targets == 0 {
        return Ok(0);
    }
    let ctx = touchstone_bulk_context();
    // Pick up to `max_targets` distinct discrepancy exemplars (one per cluster, deterministic order).
    let mut seen_ckeys: HashSet<String> = HashSet::new();
    let mut exemplars: Vec<DiscrepancyRow> = Vec::new();
    for row in discrepancies.iter() {
        if seen_ckeys.insert(row.cluster_key.clone()) {
            exemplars.push(row.clone());
            if exemplars.len() >= max_targets {
                break;
            }
        }
    }
    if exemplars.is_empty() {
        return Ok(0);
    }

    // Collect all present numeric axes for each exemplar (up to 10 axes per exemplar so 5-9 arg
    // functions like RATE, YIELD, PV, FV, PMT, ODDFPRICE escalate along pv/fv/type/guess/rate/pr/basis).
    const MAX_COARSE_AXES_PER_EXEMPLAR: usize = 10;
    #[derive(Clone)]
    struct TargetAxis {
        base_args: Vec<Value>,
        axis: usize,
        cell: Option<(usize, usize)>,
        axis_name: String,
        anchor_bits: u64,
    }
    let mut targets: Vec<TargetAxis> = Vec::new();
    for ex in &exemplars {
        let mut added = 0usize;
        for (i, p) in spec.params.iter().enumerate() {
            if added >= MAX_COARSE_AXES_PER_EXEMPLAR {
                break;
            }
            let cell = if p.kind == ParamKind::ArrayNumeric {
                Some((0, 0))
            } else {
                None
            };
            if let Some(bits) = get_axis_bits(&ex.case.args, i, cell) {
                let axis_name = match cell {
                    None => p.name.clone(),
                    Some((r, 0)) => format!("{}[{r}]", p.name),
                    Some((r, c)) => format!("{}[{r},{c}]", p.name),
                };
                targets.push(TargetAxis {
                    base_args: ex.case.args.clone(),
                    axis: i,
                    cell,
                    axis_name,
                    anchor_bits: bits,
                });
                added += 1;
            }
        }
    }
    if targets.is_empty() {
        return Ok(0);
    }

    // Step 1: Decade + Binade Excel probe stencil around each discrepant coordinate.
    let mut probe_cases: Vec<FuzzCase> = Vec::new();
    let mut probe_refs: Vec<(usize, u64, String, String)> = Vec::new(); // (target_idx, bits, key, dna_pred)
    for (tidx, tgt) in targets.iter().enumerate() {
        let anchor = f64::from_bits(tgt.anchor_bits);
        let pts = coarse_discrepancy_probe_points(anchor);
        let mut local_bits = HashSet::new();
        for v in pts {
            let b = if v == 0.0 { 0u64 } else { v.to_bits() };
            if !local_bits.insert(b) {
                continue;
            }
            let mut args = tgt.base_args.clone();
            if !set_axis_f64(&mut args, tgt.axis, tgt.cell, v) {
                continue;
            }
            let key = canonical_case_key(&cfg.function, &args);
            let (pred, trace, outcome_class) = evaluate_traced(&cfg.function, &args, &ctx);
            probe_refs.push((tidx, b, key.clone(), pred.clone()));
            if !seen_keys.insert(key) {
                continue;
            }
            *next_id += 1;
            let region = format!("fractal_coarse:{}", tgt.axis_name);
            case_hasher.update_case(&region, &args);
            probe_cases.push(FuzzCase {
                id: format!("fuzz-{}-fc{round:02}-{:06}", cfg.function, *next_id),
                region,
                round,
                args,
                prediction: pred,
                trace,
                outcome_class,
            });
        }
    }
    if !probe_cases.is_empty() {
        *total_generated += probe_cases.len();
        let pred_path = cfg
            .out_dir
            .join(format!("fractal-coarse-{round:02}-bracket-predictions.jsonl"));
        pred_hashes.push(freeze_predictions(&pred_path, &cfg.function, &probe_cases)?);
        let agg = ask_single_priority(
            client,
            &cfg.function,
            &probe_cases,
            INTERACTIVE_CHUNK_SIZE,
            2,
            Priority::Interactive,
            Some(cfg),
        )?;
        *excel_seconds += agg.excel_seconds;
        for (k, v) in agg.envs {
            all_envs.entry(k).or_insert(v);
        }
        for (w, c) in agg.worker_calls {
            *all_worker_calls.entry(w).or_insert(0) += c;
        }
        let _ = ingest_judged_cases(
            &cfg.function,
            &probe_cases,
            &agg.rows,
            excel_answers,
            region_stats,
            known_clusters,
            discrepancies,
            discrepancy_parents,
            frontier_seeds,
            total_asked,
            total_matched,
            total_mismatched,
            total_harness_fault,
            harness_faults,
            total_unread,
            total_crashed,
        );
    }

    // Collect closest Excel flip bracket per target.
    let mut by_t: BTreeMap<usize, Vec<(u64, String, String)>> = BTreeMap::new();
    for (tidx, bits, key, dna_pred) in probe_refs {
        if let Some(excel_enc) = excel_answers.get(&key) {
            by_t.entry(tidx)
                .or_default()
                .push((bits, excel_enc.clone(), dna_pred));
        }
    }
    let mut active: Vec<(usize, u64, String, u64, String, bool)> = Vec::new();
    for (tidx, mut pts) in by_t {
        if let Some((lo_b, lo_o, hi_b, hi_o, pred_flip)) =
            select_closest_discrepancy_bracket(targets[tidx].anchor_bits, &mut pts)
        {
            active.push((tidx, lo_b, lo_o, hi_b, hi_o, pred_flip));
        }
    }

    // Step 2: Batched 64-point parallel ordered-bit bisection (up to 11 steps -> 64^11 > 2^64).
    let mut resolved: Vec<(usize, u64, String, u64, String, bool)> = Vec::new();
    for step in 0..11 {
        if active.is_empty() {
            break;
        }
        let mut step_cases: Vec<FuzzCase> = Vec::new();
        let mut step_refs: Vec<(usize, u64, String, String)> = Vec::new();
        let mut still_open = Vec::new();
        for (ai, (tidx, lo_b, lo_o, hi_b, hi_o, pred_flip)) in active.iter().enumerate() {
            let ca = f64_to_compact(f64::from_bits(*lo_b));
            let cb = f64_to_compact(f64::from_bits(*hi_b));
            if cb == ca + 1 {
                resolved.push((*tidx, *lo_b, lo_o.clone(), *hi_b, hi_o.clone(), *pred_flip));
                continue;
            }
            if cb <= ca {
                continue;
            }
            still_open.push(ai);
            let tgt = &targets[*tidx];
            let span = (cb - ca) as u128;
            let k_pts = span.saturating_sub(1).min(63) as usize;
            let mut local_bits = HashSet::new();
            for j in 1..=k_pts {
                let c = ca + ((span * j as u128) / ((k_pts + 1) as u128)) as i128;
                let Some(val) = compact_to_f64(c) else {
                    continue;
                };
                let b = if val == 0.0 { 0u64 } else { val.to_bits() };
                if b == *lo_b || b == *hi_b || !local_bits.insert(b) {
                    continue;
                }
                let mut args = tgt.base_args.clone();
                if !set_axis_f64(&mut args, tgt.axis, tgt.cell, val) {
                    continue;
                }
                let key = canonical_case_key(&cfg.function, &args);
                let (pred, trace, outcome_class) = evaluate_traced(&cfg.function, &args, &ctx);
                step_refs.push((ai, b, key.clone(), pred.clone()));
                if !seen_keys.insert(key) {
                    continue;
                }
                *next_id += 1;
                let region = format!("fractal_coarse:{}", tgt.axis_name);
                case_hasher.update_case(&region, &args);
                step_cases.push(FuzzCase {
                    id: format!("fuzz-{}-fcs{step}-{:06}", cfg.function, *next_id),
                    region,
                    round,
                    args,
                    prediction: pred,
                    trace,
                    outcome_class,
                });
            }
        }
        if still_open.is_empty() {
            break;
        }
        if !step_cases.is_empty() {
            *total_generated += step_cases.len();
            let pred_path = cfg
                .out_dir
                .join(format!("fractal-coarse-{round:02}-step{step}-predictions.jsonl"));
            pred_hashes.push(freeze_predictions(&pred_path, &cfg.function, &step_cases)?);
            let agg = ask_single_priority(
                client,
                &cfg.function,
                &step_cases,
                INTERACTIVE_CHUNK_SIZE,
                2,
                Priority::Interactive,
                Some(cfg),
            )?;
            *excel_seconds += agg.excel_seconds;
            for (k, v) in agg.envs {
                all_envs.entry(k).or_insert(v);
            }
            for (w, c) in agg.worker_calls {
                *all_worker_calls.entry(w).or_insert(0) += c;
            }
            let _ = ingest_judged_cases(
                &cfg.function,
                &step_cases,
                &agg.rows,
                excel_answers,
                region_stats,
                known_clusters,
                discrepancies,
                discrepancy_parents,
                frontier_seeds,
                total_asked,
                total_matched,
                total_mismatched,
                total_harness_fault,
                harness_faults,
                total_unread,
                total_crashed,
            );
        }

        let mut pts_by_ai: BTreeMap<usize, Vec<(u64, String, String)>> = BTreeMap::new();
        for &ai in &still_open {
            let (tidx, lo_b, lo_o, hi_b, hi_o, _) = &active[ai];
            let tgt = &targets[*tidx];
            let mut args_lo = tgt.base_args.clone();
            set_axis_f64(&mut args_lo, tgt.axis, tgt.cell, f64::from_bits(*lo_b));
            let (dna_lo, _, _) = evaluate_traced(&cfg.function, &args_lo, &ctx);
            let mut args_hi = tgt.base_args.clone();
            set_axis_f64(&mut args_hi, tgt.axis, tgt.cell, f64::from_bits(*hi_b));
            let (dna_hi, _, _) = evaluate_traced(&cfg.function, &args_hi, &ctx);
            pts_by_ai.insert(
                ai,
                vec![
                    (*lo_b, lo_o.clone(), dna_lo),
                    (*hi_b, hi_o.clone(), dna_hi),
                ],
            );
        }
        for (ai, bits, key, dna_pred) in step_refs {
            if let Some(excel_enc) = excel_answers.get(&key) {
                pts_by_ai
                    .entry(ai)
                    .or_default()
                    .push((bits, excel_enc.clone(), dna_pred));
            }
        }
        let mut next_active = Vec::new();
        for (ai, mut pts) in pts_by_ai {
            let (tidx, prev_lo, _, prev_hi, _, _) = active[ai];
            if let Some((lo_b, lo_o, hi_b, hi_o, pred_flip)) =
                select_closest_discrepancy_bracket(targets[tidx].anchor_bits, &mut pts)
                && (lo_b != prev_lo || hi_b != prev_hi)
            {
                next_active.push((tidx, lo_b, lo_o, hi_b, hi_o, pred_flip));
            }
        }
        active = next_active;
    }

    for (tidx, lo_b, lo_o, hi_b, hi_o, pred_flip) in active {
        let ca = f64_to_compact(f64::from_bits(lo_b));
        let cb = f64_to_compact(f64::from_bits(hi_b));
        if cb == ca + 1 {
            resolved.push((tidx, lo_b, lo_o, hi_b, hi_o, pred_flip));
        }
    }

    let mut existing: HashSet<String> = frontiers
        .iter()
        .map(|f| {
            format!(
                "{}|{:?}|{:016x}|{:016x}",
                f.axis,
                f.axis_cell,
                f.excel_lo_bits.unwrap_or(f.dna_lo_bits),
                f.excel_hi_bits.unwrap_or(f.dna_hi_bits)
            )
        })
        .collect();
    let mut added = 0usize;
    for (tidx, lo_b, lo_o, hi_b, hi_o, pred_flip) in resolved {
        let tgt = &targets[tidx];
        let dedup_key = format!("{}|{:?}|{:016x}|{:016x}", tgt.axis, tgt.cell, lo_b, hi_b);
        if !existing.insert(dedup_key) {
            continue;
        }
        let mut args_lo = tgt.base_args.clone();
        set_axis_f64(&mut args_lo, tgt.axis, tgt.cell, f64::from_bits(lo_b));
        let (dna_lo, _, _) = evaluate_traced(&cfg.function, &args_lo, &ctx);
        let mut args_hi = tgt.base_args.clone();
        set_axis_f64(&mut args_hi, tgt.axis, tgt.cell, f64::from_bits(hi_b));
        let (dna_hi, _, _) = evaluate_traced(&cfg.function, &args_hi, &ctx);
        let dna_pred_flip = !is_dna_fault(&dna_lo)
            && !is_dna_fault(&dna_hi)
            && predicate_tag(&dna_lo) != predicate_tag(&dna_hi);
        let matched_dna_flip = pred_flip && dna_pred_flip;
        let kind = if matched_dna_flip {
            format!("{}<->{}", predicate_tag(&lo_o), predicate_tag(&hi_o))
        } else if pred_flip {
            format!("excel_only:{}<->{}", predicate_tag(&lo_o), predicate_tag(&hi_o))
        } else {
            "discrepancy_onset".to_owned()
        };
        let transition_kind = if matched_dna_flip {
            "predicate_flip".to_owned()
        } else if pred_flip {
            "excel_only_predicate_flip".to_owned()
        } else {
            "discrepancy_onset".to_owned()
        };
        let shifted_ulps = matched_dna_flip.then_some(0);
        let f = Frontier {
            id: format!("frontier-{}-{:03}", cfg.function, frontiers.len() + 1),
            function: cfg.function.clone(),
            axis: tgt.axis,
            axis_cell: tgt.cell,
            axis_name: tgt.axis_name.clone(),
            kind,
            base_args: tgt.base_args.clone(),
            dna_lo_bits: lo_b,
            dna_hi_bits: hi_b,
            dna_lo_outcome: dna_lo,
            dna_hi_outcome: dna_hi,
            excel_lo_bits: Some(lo_b),
            excel_hi_bits: Some(hi_b),
            excel_lo_outcome: Some(lo_o),
            excel_hi_outcome: Some(hi_o),
            shifted_ulps,
            depth: 0,
            scale: super::fractal::FractalScale::CoarseDiscrepancy.as_str().to_owned(),
            transition_kind,
            parent_id: None,
        };
        fractal_stats.record_frontier(&f);
        frontiers.push(f);
        added += 1;
    }
    Ok(added)
}

fn record_window_flips_from_cache(
    frontiers: &mut [Frontier],
    window_refs: &[(usize, u64, String)],
    excel_answers: &HashMap<String, String>,
) {
    if window_refs.is_empty() {
        return;
    }
    let probed: Vec<(usize, u64, String)> = window_refs
        .iter()
        .filter_map(|(fidx, bits, key)| {
            excel_answers
                .get(key)
                .map(|enc| (*fidx, *bits, enc.clone()))
        })
        .collect();
    record_excel_window_flips(frontiers, &probed);
}

/// Run a live Excel-compared adaptive smart fuzzing sweep via Touchstone.
pub fn execute_live_run(cfg: &FuzzConfig) -> Result<FuzzSummary, String> {
    let started = Instant::now();
    let hints_bank = match &cfg.hints_dir {
        Some(dir) => hints::HintBank::load_dir(dir)?,
        None => hints::load_default_hints()?,
    };
    let spec = DomainSpec::for_function_with_hints(&cfg.function, &hints_bank)?;
    let ctx = touchstone_bulk_context();
    std::fs::create_dir_all(&cfg.out_dir)
        .map_err(|e| format!("{}: {e}", cfg.out_dir.display()))?;
    if cfg.sweep.is_some()
        && let Some(reason) = spec.merged.sweep_static_skip_reason()
    {
        let summary = FuzzSummary {
            function: cfg.function.clone(),
            seed: cfg.seed,
            budget: cfg.budget,
            dry_run: false,
            total_generated: 0,
            total_asked: 0,
            total_matched: 0,
            total_mismatched: 0,
            total_harness_fault: 0,
            harness_faults: BTreeMap::new(),
            total_unread: 0,
            total_crashed: 0,
            rounds_run: 0,
            stop_reason: format!("skipped:{reason}"),
            case_list_sha256: StreamCaseHasher::new(&cfg.function).finish_hex(),
            predictions_sha256: Vec::new(),
            elapsed_seconds: started.elapsed().as_secs_f64(),
            excel_seconds: 0.0,
            worker_calls: BTreeMap::new(),
            region_stats: BTreeMap::new(),
            clusters: Vec::new(),
            frontiers: Vec::new(),
            fractal_stats: FractalDiscoveryStats::default(),
            tracker: CoverageTracker::new(&cfg.function),
        };
        write_outputs(
            cfg,
            &spec,
            &summary,
            &[],
            &BTreeMap::new(),
            &[],
            &HashMap::new(),
        )?;
        return Ok(summary);
    }
    let client = connect_client(cfg.endpoint.as_deref())?;
    if cfg.sweep.is_some() {
        client.require_sweep().map_err(|e| e.to_string())?;
    }

    let mut rng = Rng::from_seed_and_tag(cfg.seed, &cfg.function);
    let mut tracker = CoverageTracker::new(&cfg.function);
    let mut seen_keys: HashSet<String> = HashSet::with_capacity(cfg.budget);
    let mut excel_answers: HashMap<String, String> = HashMap::with_capacity(cfg.budget);
    let mut region_weights: Vec<f64> = spec.regions.iter().map(|r| r.base_weight).collect();
    let mut region_stats: BTreeMap<String, RegionStats> = BTreeMap::new();
    let mut known_clusters: HashSet<String> = HashSet::new();
    let mut discrepancies: Vec<DiscrepancyRow> = Vec::new();
    let mut discrepancy_parents: Vec<(String, Vec<Value>)> = Vec::new();
    let mut frontier_seeds: Vec<FuzzCase> = Vec::new();
    let mut frontiers: Vec<Frontier> = Vec::new();
    let mut fractal_stats = FractalDiscoveryStats::default();
    let mut probed_frontier_count = 0usize;
    let mut all_envs: BTreeMap<String, Value> = BTreeMap::new();
    let mut all_worker_calls: BTreeMap<String, usize> = BTreeMap::new();
    let mut pred_hashes: Vec<String> = Vec::new();
    let mut case_hasher = StreamCaseHasher::new(&cfg.function);

    let mut next_id = 0usize;
    let mut round = 0usize;
    let mut dry_streak = 0usize;
    let mut total_generated = 0usize;
    let mut total_asked = 0usize;
    let mut total_matched = 0usize;
    let mut total_mismatched = 0usize;
    let mut total_harness_fault = 0usize;
    let mut harness_faults: BTreeMap<String, usize> = BTreeMap::new();
    let mut total_unread = 0usize;
    let mut total_crashed = 0usize;
    let mut excel_seconds = 0.0f64;

    // Reserve a slice of budget for boundary windows & Excel bisection (shared with dry-run).
    let main_budget = main_round_budget(cfg);
    let mut stop_reason = "budget-reached".to_owned();

    while total_asked < main_budget {
        let want = cfg.round_size.min(main_budget - total_asked);
        let bulk_batch = generate_steered_round(
            &spec,
            &region_weights,
            &discrepancy_parents,
            &mut tracker,
            &mut seen_keys,
            &ctx,
            round,
            want,
            &mut next_id,
            &mut rng,
        );
        if bulk_batch.is_empty() {
            stop_reason = "generator-exhausted".to_owned();
            break;
        }
        for c in &bulk_batch {
            case_hasher.update_case(&c.region, &c.args);
            if frontier_seeds.len() < MAX_FRONTIER_SEEDS {
                frontier_seeds.push(c.clone());
            }
        }
        total_generated += bulk_batch.len();

        // Freeze bulk predictions BEFORE sending to Touchstone.
        let pred_path = cfg.out_dir.join(format!("round-{round:02}-predictions.jsonl"));
        pred_hashes.push(freeze_predictions(&pred_path, &cfg.function, &bulk_batch)?);

        // Prepare interactive boundary window probes for any frontiers discovered after earlier rounds
        // so w3 executes them concurrently while w1/w2 process the bulk batch.
        let mut inter_cases: Vec<FuzzCase> = Vec::new();
        let mut inter_window_refs: Vec<(usize, u64, String)> = Vec::new();
        if probed_frontier_count < frontiers.len() {
            let expanded = expand_frontier_windows_with_refs(
                &frontiers[probed_frontier_count..],
                cfg.frontier_window,
                &ctx,
                &mut seen_keys,
                round,
                &mut next_id,
            );
            for (rel_idx, bits, key) in expanded.window_refs {
                inter_window_refs.push((probed_frontier_count + rel_idx, bits, key));
            }
            for (_, c) in expanded.cases {
                case_hasher.update_case(&c.region, &c.args);
                inter_cases.push(c);
            }
            probed_frontier_count = frontiers.len();
            if !inter_cases.is_empty() {
                total_generated += inter_cases.len();
                let bpath = cfg
                    .out_dir
                    .join(format!("round-{round:02}-boundary-predictions.jsonl"));
                pred_hashes.push(freeze_predictions(&bpath, &cfg.function, &inter_cases)?);
            }
        }

        let (bulk_agg, inter_agg) = ask_bulk_and_interactive(
            &client,
            &cfg.function,
            &bulk_batch,
            &inter_cases,
            cfg.batch_size,
            cfg.in_flight,
            Some(cfg),
        )?;
        excel_seconds += bulk_agg.excel_seconds;
        for (k, v) in bulk_agg.envs {
            all_envs.entry(k).or_insert(v);
        }
        for (w, c) in bulk_agg.worker_calls {
            *all_worker_calls.entry(w).or_insert(0) += c;
        }

        let (new_c1, disc_reg1) = ingest_judged_cases(
            &cfg.function,
            &bulk_batch,
            &bulk_agg.rows,
            &mut excel_answers,
            &mut region_stats,
            &mut known_clusters,
            &mut discrepancies,
            &mut discrepancy_parents,
            &mut frontier_seeds,
            &mut total_asked,
            &mut total_matched,
            &mut total_mismatched,
            &mut total_harness_fault,
            &mut harness_faults,
            &mut total_unread,
            &mut total_crashed,
        );
        let (new_c2, disc_reg2) = ingest_judged_cases(
            &cfg.function,
            &inter_cases,
            &inter_agg.rows,
            &mut excel_answers,
            &mut region_stats,
            &mut known_clusters,
            &mut discrepancies,
            &mut discrepancy_parents,
            &mut frontier_seeds,
            &mut total_asked,
            &mut total_matched,
            &mut total_mismatched,
            &mut total_harness_fault,
            &mut harness_faults,
            &mut total_unread,
            &mut total_crashed,
        );
        record_window_flips_from_cache(&mut frontiers, &inter_window_refs, &excel_answers);

        // Adapt per-region weights: densify discrepant regions, taper clean ones.
        for (idx, reg) in spec.regions.iter().enumerate() {
            let hit = disc_reg1.contains(reg.name) || disc_reg2.contains(reg.name);
            if hit {
                region_weights[idx] = (region_weights[idx] * 1.5).min(reg.base_weight * 6.0);
            } else {
                region_weights[idx] = (region_weights[idx] * 0.8).max(reg.base_weight * 0.25);
            }
        }

        let new_total = new_c1 + new_c2;
        if new_total == 0 {
            dry_streak += 1;
        } else {
            dry_streak = 0;
        }
        eprintln!(
            "[fuzz {} r{round:02}] asked={total_asked} matched={total_matched} mismatched={total_mismatched} harness_fault={total_harness_fault} unread={total_unread} crashed={total_crashed} clusters={} (+{new_total}) workers={:?}",
            cfg.function,
            known_clusters.len(),
            all_worker_calls,
        );
        if round == 0 && total_asked > 0 && total_harness_fault == total_asked {
            let top_fault = harness_faults
                .iter()
                .max_by_key(|&(_, count)| *count)
                .map_or("Unknown", |(k, _)| k.as_str());
            stop_reason = format!("skipped:dna-fault:{top_fault}");
            round += 1;
            break;
        }

        // Discover initial fractal frontiers after Round 0 so Round 1 overlaps them onto w3!
        if round == 0 && cfg.max_frontiers > 0 && frontiers.is_empty() {
            let initial_cap = (cfg.max_frontiers / 2).max(1);
            let fres = fractal_discover_local(
                &spec,
                &frontier_seeds,
                &ctx,
                &FractalConfig::new(initial_cap, cfg.fractal_depth),
            );
            frontiers = fres.frontiers;
            fractal_stats = fres.stats;
        }
        round += 1;
        if total_asked >= cfg.min_budget && dry_streak >= cfg.dry_rounds && round >= 2 {
            stop_reason = format!("dry-for-{}-rounds", cfg.dry_rounds);
            break;
        }
    }

    if !stop_reason.starts_with("skipped:dna-fault:") {
        // Final fractal frontier discovery (including discrepancy seeds!) + interactive consecutive windows.
        if frontiers.len() < cfg.max_frontiers {
            let fres = fractal_discover_local(
                &spec,
                &frontier_seeds,
                &ctx,
                &FractalConfig::new(cfg.max_frontiers, cfg.fractal_depth),
            );
            fractal_stats.local_evals += fres.stats.local_evals;
            fractal_stats.decade_brackets_found += fres.stats.decade_brackets_found;
            fractal_stats.binade_brackets_found += fres.stats.binade_brackets_found;
            let more = fres.frontiers;
            let existing: HashSet<String> = frontiers
                .iter()
                .map(|f| {
                    format!(
                        "{}|{:?}|{:016x}|{:016x}|{}",
                        f.axis, f.axis_cell, f.dna_lo_bits, f.dna_hi_bits, f.kind
                    )
                })
                .collect();
            for mut f in more {
                if frontiers.len() >= cfg.max_frontiers {
                    break;
                }
                let k = format!(
                    "{}|{:?}|{:016x}|{:016x}|{}",
                    f.axis, f.axis_cell, f.dna_lo_bits, f.dna_hi_bits, f.kind
                );
                if !existing.contains(&k) {
                    f.id = format!("frontier-{}-{:03}", spec.function, frontiers.len() + 1);
                    fractal_stats.ulp_frontiers_pinpointed += 1;
                    fractal_stats.record_frontier(&f);
                    frontiers.push(f);
                }
            }
        }
        if probed_frontier_count < frontiers.len() {
            let expanded = expand_frontier_windows_with_refs(
                &frontiers[probed_frontier_count..],
                cfg.frontier_window,
                &ctx,
                &mut seen_keys,
                round,
                &mut next_id,
            );
            let mut final_cases = Vec::new();
            let mut final_window_refs = Vec::new();
            for (rel_idx, bits, key) in expanded.window_refs {
                final_window_refs.push((probed_frontier_count + rel_idx, bits, key));
            }
            for (_, c) in expanded.cases {
                case_hasher.update_case(&c.region, &c.args);
                final_cases.push(c);
            }
            if !final_cases.is_empty() {
                total_generated += final_cases.len();
                let bpath = cfg
                    .out_dir
                    .join(format!("round-{round:02}-boundary-predictions.jsonl"));
                pred_hashes.push(freeze_predictions(&bpath, &cfg.function, &final_cases)?);
                let agg = ask_single_priority(
                    &client,
                    &cfg.function,
                    &final_cases,
                    INTERACTIVE_CHUNK_SIZE,
                    2,
                    Priority::Interactive,
                    Some(cfg),
                )?;
                excel_seconds += agg.excel_seconds;
                for (k, v) in agg.envs {
                    all_envs.entry(k).or_insert(v);
                }
                for (w, c) in agg.worker_calls {
                    *all_worker_calls.entry(w).or_insert(0) += c;
                }
                let _ = ingest_judged_cases(
                    &cfg.function,
                    &final_cases,
                    &agg.rows,
                    &mut excel_answers,
                    &mut region_stats,
                    &mut known_clusters,
                    &mut discrepancies,
                    &mut discrepancy_parents,
                    &mut frontier_seeds,
                    &mut total_asked,
                    &mut total_matched,
                    &mut total_mismatched,
                    &mut total_harness_fault,
                    &mut harness_faults,
                    &mut total_unread,
                    &mut total_crashed,
                );
                round += 1;
            }
            record_window_flips_from_cache(&mut frontiers, &final_window_refs, &excel_answers);
        }

        // Pinpoint Excel's flip via batched 64-point bisection for any shifted predicate_flip frontier.
        bisect_shifted_frontiers_excel(
            cfg,
            &client,
            &mut frontiers,
            &mut seen_keys,
            &mut excel_answers,
            &mut case_hasher,
            &mut pred_hashes,
            &mut all_envs,
            &mut all_worker_calls,
            &mut region_stats,
            &mut known_clusters,
            &mut discrepancies,
            &mut discrepancy_parents,
            &mut frontier_seeds,
            &mut next_id,
            &mut total_generated,
            &mut total_asked,
            &mut total_matched,
            &mut total_mismatched,
            &mut total_harness_fault,
            &mut harness_faults,
            &mut total_unread,
            &mut total_crashed,
            &mut excel_seconds,
            round,
        )?;

        // Escalate coarse discrepancies via Decade+Binade Excel probing + 64-point bisection when
        // discrepancies were found in regions not already covered by a predicted DNA Calc frontier.
        if cfg.max_frontiers > 0 && !discrepancies.is_empty() {
            let coarse_cap = (cfg.max_frontiers / 4).clamp(1, 4);
            let _ = fractal_refine_coarse_discrepancies_excel(
                cfg,
                &spec,
                &client,
                &mut frontiers,
                &mut fractal_stats,
                &mut seen_keys,
                &mut excel_answers,
                &mut case_hasher,
                &mut pred_hashes,
                &mut all_envs,
                &mut all_worker_calls,
                &mut region_stats,
                &mut known_clusters,
                &mut discrepancies,
                &mut discrepancy_parents,
                &mut frontier_seeds,
                &mut next_id,
                &mut total_generated,
                &mut total_asked,
                &mut total_matched,
                &mut total_mismatched,
                &mut total_harness_fault,
                &mut harness_faults,
                &mut total_unread,
                &mut total_crashed,
                &mut excel_seconds,
                round,
                coarse_cap,
            )?;
        }
    }

    let clusters = build_clusters(&cfg.function, &discrepancies);
    let summary = FuzzSummary {
        function: cfg.function.clone(),
        seed: cfg.seed,
        budget: cfg.budget,
        dry_run: false,
        total_generated,
        total_asked,
        total_matched,
        total_mismatched,
        total_harness_fault,
        harness_faults,
        total_unread,
        total_crashed,
        rounds_run: round,
        stop_reason,
        case_list_sha256: case_hasher.finish_hex(),
        predictions_sha256: pred_hashes,
        elapsed_seconds: started.elapsed().as_secs_f64(),
        excel_seconds,
        worker_calls: all_worker_calls,
        region_stats,
        clusters,
        frontiers,
        fractal_stats,
        tracker,
    };
    write_outputs(
        cfg,
        &spec,
        &summary,
        &discrepancies,
        &all_envs,
        &frontier_seeds,
        &excel_answers,
    )?;
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fuzz::rng::hex_f64;
    use calc_verdict::WitnessSet;
    use serde_json::json;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::thread;

    fn read_frame(stream: &mut TcpStream) -> Option<(u8, Value)> {
        let mut size = [0u8; 4];
        stream.read_exact(&mut size).ok()?;
        let size = u32::from_be_bytes(size) as usize;
        let mut body = vec![0u8; size];
        stream.read_exact(&mut body).ok()?;
        Some((body[0], serde_json::from_slice(&body[1..]).unwrap()))
    }

    fn write_frame(stream: &mut TcpStream, kind: u8, body: &Value) {
        let text = serde_json::to_vec(body).unwrap();
        stream
            .write_all(&((text.len() + 1) as u32).to_be_bytes())
            .unwrap();
        stream.write_all(&[kind]).unwrap();
        stream.write_all(&text).unwrap();
    }

    /// Stand-in broker that delegates to DNA Calc for RATE except for one controlled mismatch:
    /// when `timing != 0.0` (arg 4 nonzero) and DNA Calc returns a finite number, it adds 2 ULPs
    /// so we can test clustering, adaptive densification, worker attribution, and bank export.
    fn serve_fake_rate_broker(listener: TcpListener) {
        serve_fake_fuzz_broker(&listener, false, None);
    }

    fn serve_fake_fuzz_broker(listener: &TcpListener, sweep: bool, interrupt: Option<&Path>) {
        let ctx = touchstone_bulk_context();
        let (mut stream, _) = listener.accept().unwrap();
        while let Some((kind, body)) = read_frame(&mut stream) {
            match kind {
                b'H' => {
                    write_frame(
                        &mut stream,
                        b'H',
                        &json!({
                            "v": "touchstone/1",
                            "server": "fake-fuzzer-broker",
                            "sweep": sweep,
                            "envs": {
                                "e-bulk": {
                                    "engine": "excel",
                                    "version": "16.0",
                                    "build": "20625",
                                    "file_version": "16.0.20625.20108",
                                    "machine": "x86-64",
                                    "os": "Windows",
                                    "cpu": "Fake AMD Ryzen",
                                    "mt_calc": "manual:4"
                                },
                                "e-inter": {
                                    "engine": "excel",
                                    "version": "16.0",
                                    "build": "20625",
                                    "file_version": "16.0.20625.20108",
                                    "machine": "x86-64",
                                    "os": "Windows",
                                    "cpu": "Fake AMD Ryzen",
                                    "mt_calc": "off"
                                }
                            }
                        }),
                    );
                }
                b'S' => {
                    let id = body["id"].as_str().unwrap().to_string();
                    let prio = body["opts"]["priority"].as_str().unwrap_or("bulk");
                    if sweep && body["seq"] == json!(0) {
                        assert_eq!(prio, "sweep");
                        assert!(body["opts"]["calls"].as_u64().unwrap() <= 32);
                    }
                    let (worker, env_id) = if prio == "interactive" {
                        ("w3", "e-inter")
                    } else {
                        ("w1", "e-bulk")
                    };
                    let mut answers = Vec::new();
                    for g in body["groups"].as_array().unwrap() {
                        let tokens: Vec<&str> = g["k"].as_str().unwrap().split(',').collect();
                        let tok_width = |tok: &str| -> usize {
                            if let Some(rest) = tok.strip_prefix('a').or_else(|| tok.strip_prefix('r'))
                                && let Some((rs, cs)) = rest.split_once('x')
                            {
                                return rs.parse::<usize>().unwrap() * cs.parse::<usize>().unwrap();
                            }
                            1
                        };
                        let width: usize = tokens.iter().map(|t| tok_width(t)).sum();
                        let n = g["n"].as_u64().unwrap() as usize;
                        let flat = g["v"].as_array().unwrap();
                        let mut ty = String::with_capacity(n);
                        let mut out_v = Vec::with_capacity(n);
                        let mut ity = String::with_capacity(n * width);
                        let mut iv = Vec::with_capacity(n * width);
                        for row_i in 0..n {
                            let mut args_json = Vec::with_capacity(tokens.len());
                            let mut offset = 0usize;
                            for &tok in &tokens {
                                if let Some(rest) = tok.strip_prefix('r')
                                    && let Some((rs, cs)) = rest.split_once('x')
                                {
                                    let nr: usize = rs.parse().unwrap();
                                    let nc: usize = cs.parse().unwrap();
                                    let mut grid = Vec::with_capacity(nr);
                                    for _ in 0..nr {
                                        let mut rvec = Vec::with_capacity(nc);
                                        for _ in 0..nc {
                                            let cell = &flat[row_i * width + offset];
                                            offset += 1;
                                            match cell["type"].as_str().unwrap_or("blank") {
                                                "number" => {
                                                    let h = cell["bits"].as_str().unwrap();
                                                    rvec.push(Value::String(format!("0x{h}")));
                                                    ity.push('n');
                                                    iv.push(Value::String(h.to_owned()));
                                                }
                                                "text" => {
                                                    let v = cell["value"].clone();
                                                    rvec.push(v.clone());
                                                    ity.push('t');
                                                    iv.push(v);
                                                }
                                                "boolean" => {
                                                    let v = cell["value"].clone();
                                                    rvec.push(v.clone());
                                                    ity.push('b');
                                                    iv.push(v);
                                                }
                                                "error" => {
                                                    let code = cell["code"].as_str().unwrap();
                                                    let var = excel_error_to_encoded(code)
                                                        .strip_prefix("error:")
                                                        .unwrap_or("Value")
                                                        .to_owned();
                                                    rvec.push(json!({"error": var}));
                                                    ity.push('e');
                                                    iv.push(Value::String(code.to_owned()));
                                                }
                                                _ => {
                                                    rvec.push(Value::Null);
                                                    ity.push('z');
                                                    iv.push(Value::Null);
                                                }
                                            }
                                        }
                                        grid.push(Value::Array(rvec));
                                    }
                                    args_json.push(json!({ "range": grid }));
                                    continue;
                                }
                                if let Some(rest) = tok.strip_prefix('a')
                                    && let Some((rs, cs)) = rest.split_once('x')
                                {
                                    let nr: usize = rs.parse().unwrap();
                                    let nc: usize = cs.parse().unwrap();
                                    let mut grid = Vec::with_capacity(nr);
                                    for _ in 0..nr {
                                        let mut rvec = Vec::with_capacity(nc);
                                        for _ in 0..nc {
                                            let h = flat[row_i * width + offset].as_str().unwrap();
                                            offset += 1;
                                            rvec.push(Value::String(format!("0x{h}")));
                                            ity.push('n');
                                            iv.push(Value::String(h.to_owned()));
                                        }
                                        grid.push(Value::Array(rvec));
                                    }
                                    args_json.push(Value::Array(grid));
                                    continue;
                                }
                                let cell = &flat[row_i * width + offset];
                                offset += 1;
                                match tok {
                                    "n" => {
                                        let h = cell.as_str().unwrap();
                                        args_json.push(Value::String(format!("0x{h}")));
                                        ity.push('n');
                                        iv.push(Value::String(h.to_owned()));
                                    }
                                    "s" | "S" => {
                                        args_json.push(cell.clone());
                                        ity.push('t');
                                        iv.push(cell.clone());
                                    }
                                    "b" | "B" => {
                                        args_json.push(cell.clone());
                                        ity.push('b');
                                        iv.push(cell.clone());
                                    }
                                    "e" => {
                                        let code = cell.as_str().unwrap();
                                        let var = excel_error_to_encoded(code)
                                            .strip_prefix("error:")
                                            .unwrap_or("Value")
                                            .to_owned();
                                        args_json.push(json!({"error": var}));
                                        ity.push('e');
                                        iv.push(cell.clone());
                                    }
                                    "z" => {
                                        args_json.push(Value::Null);
                                        ity.push('z');
                                        iv.push(Value::Null);
                                    }
                                    _ => {
                                        args_json.push(Value::Null);
                                        ity.push('z');
                                        iv.push(Value::Null);
                                    }
                                }
                            }
                            let function = g["f"].as_str().unwrap();
                            let (dna, _, _) = evaluate_traced(function, &args_json, &ctx);
                            let timing_beg = function == "RATE"
                                && args_json
                                    .get(4)
                                    .and_then(parse_hex_bits)
                                    .map(f64::from_bits)
                                    .is_some_and(|t| t != 0.0);
                            let inject_bump = (sweep && function == "ABS") || timing_beg;
                            if let Some(hex) = dna.strip_prefix("0x") {
                                let mut bits = u64::from_str_radix(hex, 16).unwrap();
                                if inject_bump {
                                    let bumped = step_ulps(f64::from_bits(bits), 2, true)
                                        .unwrap_or(f64::from_bits(bits));
                                    bits = bumped.to_bits();
                                }
                                ty.push('n');
                                out_v.push(Value::String(format!("{bits:016x}")));
                            } else if let Some(err) = dna.strip_prefix("error:") {
                                ty.push('e');
                                out_v.push(Value::String(
                                    error_variant_to_excel(err).unwrap_or("#VALUE!").to_owned(),
                                ));
                            } else if let Some(text) = dna.strip_prefix("text:") {
                                ty.push('t');
                                out_v.push(Value::String(text.to_owned()));
                            } else if let Some(b) = dna.strip_prefix("logical:") {
                                ty.push('b');
                                out_v.push(Value::Bool(b.eq_ignore_ascii_case("true")));
                            } else {
                                ty.push('e');
                                out_v.push(Value::String("#VALUE!".to_owned()));
                            }
                        }
                        let mut g_out = json!({"ty": ty, "v": out_v, "ity": ity, "iv": iv});
                        if let Some(o) = g.get("o") {
                            g_out["o"] = o.clone();
                        } else {
                            g_out["i"] = g["i"].clone();
                        }
                        answers.push(g_out);
                    }
                    write_frame(
                        &mut stream,
                        b'A',
                        &json!({"id": id, "seq": body["seq"], "env": env_id, "w": worker, "g": answers}),
                    );
                    if let Some(pause) = interrupt {
                        std::fs::write(pause, "").unwrap();
                        // Half-close so queued requests from the client do not reset the answer stream.
                        stream.shutdown(std::net::Shutdown::Write).unwrap();
                        stream
                            .set_read_timeout(Some(Duration::from_secs(3)))
                            .unwrap();
                        while read_frame(&mut stream).is_some() {}
                        return;
                    }
                    if body["last"] == json!(true) {
                        write_frame(&mut stream, b'D', &json!({"id": id, "calls": 0}));
                    }
                }
                _ => {}
            }
        }
    }

    #[test]
    fn sweep_pauses_and_resumes_recipe_after_partial_fake_broker_reply() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let tmp = std::env::temp_dir().join(format!("dnacalc-fuzz-sweep-test-{port}"));
        std::fs::create_dir_all(&tmp).unwrap();
        let out = tmp.join("out");
        let pause = out.join("PAUSE");
        let endpoint = tmp.join("endpoint.json");
        std::fs::write(
            &endpoint,
            json!({"host": "127.0.0.1", "port": port}).to_string(),
        )
        .unwrap();
        let order = tmp.join("order.tsv");
        std::fs::write(
            &order,
            "rank\tfunction\tpriority_score\tbank_class\n1\tABS\t10.0\texact\n2\tINT\t5.0\tgross\n",
        )
        .unwrap();
        let worker_pause = pause.clone();
        let broker = std::thread::spawn(move || {
            serve_fake_fuzz_broker(&listener, true, Some(&worker_pause));
            serve_fake_fuzz_broker(&listener, true, None);
        });
        let command = [
            "sweep",
            "--runs",
            "1",
            "--order",
            order.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
            "--endpoint",
            endpoint.to_str().unwrap(),
        ]
        .map(str::to_owned);
        let run = out.join("00000001");
        let read_json = |path: &Path| -> Value {
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
        };
        assert_eq!(super::super::run(&command), std::process::ExitCode::FAILURE);
        let bank = out.join("bank-rows");
        let partial: Vec<_> = std::fs::read_dir(&bank)
            .unwrap()
            .map(|file| file.unwrap().path())
            .collect();
        assert!(!partial.is_empty());
        for file in &partial {
            WitnessSet::decode(&std::fs::read(file).unwrap()).unwrap();
        }
        assert!(run.join("00000001/interrupted.json").is_file());
        assert!(!run.join("done.json").exists());
        assert_eq!(super::super::run(&command), std::process::ExitCode::SUCCESS);
        assert!(
            !run.join("00000002").exists(),
            "pause submits no new attempt"
        );
        std::fs::remove_file(pause).unwrap();
        assert_eq!(super::super::run(&command), std::process::ExitCode::SUCCESS);
        let first = read_json(&run.join("00000001/attempt.json"));
        let second = read_json(&run.join("00000002/attempt.json"));
        assert_eq!(first["recipe"], second["recipe"]);
        assert_eq!(second["recipe"]["function"], "ABS");
        assert_eq!(read_json(&run.join("done.json"))["attempt"], 2);
        let summary = read_json(&run.join("00000002/summary.json"));
        assert!(summary["total_mismatched"].as_u64().unwrap() > 0);
        assert!(summary["total_matched"].as_u64().unwrap() > 0);
        assert_eq!(
            summary["excel_environments"]["e-bulk"]["cpu"],
            "Fake AMD Ryzen"
        );
        let prediction_hashes = read_json(&run.join("00000001/prediction-hashes.json"));
        assert_eq!(
            summary["predictions_sha256"][0],
            prediction_hashes["round-00-predictions.jsonl"]
        );
        assert!(partial.iter().all(|file| file.is_file()));
        assert!(!run.join("00000002/round-00-predictions.jsonl").exists());
        broker.join().unwrap();
        std::fs::remove_dir_all(tmp).unwrap();
    }

    #[test]
    fn sweep_resume_dedups_bank_rows_and_recipe_records_exported_files() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let tmp = std::env::temp_dir().join(format!("dnacalc-fuzz-sweep-dedup-test-{port}"));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let out = tmp.join("out");
        let pause = out.join("PAUSE");
        let endpoint = tmp.join("endpoint.json");
        std::fs::write(
            &endpoint,
            json!({"host": "127.0.0.1", "port": port}).to_string(),
        )
        .unwrap();
        let order = tmp.join("order.tsv");
        std::fs::write(
            &order,
            "rank\tfunction\tpriority_score\tbank_class\n1\tABS\t10.0\texact\n",
        )
        .unwrap();
        let worker_pause = pause.clone();
        let broker = std::thread::spawn(move || {
            serve_fake_fuzz_broker(&listener, true, Some(&worker_pause));
            serve_fake_fuzz_broker(&listener, true, None);
        });
        let command = [
            "sweep",
            "--runs",
            "1",
            "--order",
            order.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
            "--endpoint",
            endpoint.to_str().unwrap(),
        ]
        .map(str::to_owned);
        let run = out.join("00000001");
        assert_eq!(super::super::run(&command), std::process::ExitCode::FAILURE);
        std::fs::remove_file(&pause).unwrap();
        assert_eq!(super::super::run(&command), std::process::ExitCode::SUCCESS);

        let summary: Value =
            serde_json::from_slice(&std::fs::read(run.join("00000002/summary.json")).unwrap())
                .unwrap();
        let expected_mismatched = summary["total_mismatched"].as_u64().unwrap() as usize;
        assert!(expected_mismatched > 0);

        // Every bank-rows WitnessSet across attempt 1 and attempt 2 must have zero duplicate cases.
        let bank_dir = out.join("bank-rows");
        let mut seen_keys = HashSet::new();
        let mut total_bank_rows = 0usize;
        for entry in std::fs::read_dir(&bank_dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let ws = WitnessSet::decode(&std::fs::read(&path).unwrap()).unwrap();
            for w in &ws.witnesses {
                total_bank_rows += 1;
                let key = canonical_case_key(&ws.function, &w.args);
                assert!(
                    seen_keys.insert(key),
                    "duplicate bank row witness {} in {} across interrupted sweep resume",
                    w.id,
                    path.display()
                );
            }
        }
        assert_eq!(
            total_bank_rows, expected_mismatched,
            "total exported bank rows across attempts must equal total_mismatched"
        );

        // Attempt 2's recipe.json must record the scheduler's exported bank rows so --verify-storage audits them.
        let recipe: Value =
            serde_json::from_slice(&std::fs::read(run.join("00000002/recipe.json")).unwrap())
                .unwrap();
        assert_eq!(
            recipe["storage_policy"]["discrepancy_rows_exported"].as_u64(),
            Some(expected_mismatched as u64),
            "scheduler recipe.json must report exported discrepancy count"
        );
        assert!(
            recipe["storage_policy"]["bank_rows_path"].is_string(),
            "scheduler recipe.json must record bank_rows_path"
        );

        let audit = super::super::storage::verify_sweep_storage_policy(
            &run.join("00000002"),
            None,
        )
        .expect("verify_sweep_storage_policy");
        assert!(audit.ok, "storage audit violations: {:?}", audit.violations);
        assert_eq!(audit.discrepancy_rows, expected_mismatched);
        assert_eq!(audit.matching_rows_in_bank, 0);

        broker.join().unwrap();
        std::fs::remove_dir_all(tmp).unwrap();
    }

    #[test]
    fn live_run_against_fake_touchstone_broker_freezes_clusters_and_exports_discrepancies_only() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let tmp = std::env::temp_dir().join(format!("dnacalc-fuzz-live-test-{port}"));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let endpoint = tmp.join("endpoint.json");
        std::fs::write(
            &endpoint,
            json!({"host": "127.0.0.1", "port": port, "token": "t"}).to_string(),
        )
        .unwrap();

        let _server = thread::spawn(move || serve_fake_rate_broker(listener));

        let bank_path = tmp.join("bank-rows-RATE.json");
        let pin_path = tmp.join("pinned-RATE.json");
        let mut cfg = FuzzConfig::default_for("RATE");
        cfg.seed = 20261123;
        cfg.budget = 350;
        cfg.min_budget = 250;
        cfg.round_size = 120;
        cfg.batch_size = 60;
        cfg.in_flight = 2;
        cfg.max_frontiers = 4;
        cfg.frontier_window = 3;
        cfg.dry_rounds = 2;
        cfg.lane = "sf1".to_owned();
        cfg.out_dir = tmp.join("out");
        cfg.bank_rows_out = Some(bank_path.clone());
        cfg.pin_out = Some(pin_path.clone());
        cfg.endpoint = Some(endpoint);

        let summary = execute_live_run(&cfg).expect("live fuzz run succeeded");
        assert!(summary.total_asked >= 250, "asked={}", summary.total_asked);
        assert_eq!(summary.total_unread, 0);
        assert_eq!(summary.total_crashed, 0);
        assert!(summary.total_mismatched > 0, "expected injected 2-ULP mismatches");
        assert!(summary.total_matched > 0, "expected matched end-of-period rows");
        assert!(!summary.clusters.is_empty(), "expected non-empty clusters");
        assert!(summary.worker_calls.contains_key("w1"), "expected bulk worker w1");
        assert!(summary.worker_calls.contains_key("w3"), "expected interactive worker w3");
        assert!(!summary.predictions_sha256.is_empty());

        // Bank rows must decode via WitnessSet and contain ONLY discrepancies!
        let bank_bytes = std::fs::read(&bank_path).expect("bank-rows written");
        let bank = WitnessSet::decode(&bank_bytes).expect("valid WitnessSet");
        assert_eq!(bank.witnesses.len(), summary.total_mismatched);
        assert!(!bank.readings.is_empty(), "expected reading provenance records");
        for r in &bank.readings {
            assert_eq!(
                r.excel_build.as_deref(),
                Some("16.0.20625.20108"),
                "bank-rows reading should carry Touchstone file_version, not unknown"
            );
            assert_eq!(r.cpu.as_deref(), Some("Fake AMD Ryzen"));
            assert_eq!(r.bitness.as_deref(), Some("64-bit"));
            assert_eq!(r.platform.as_deref(), Some("Windows"));
        }
        for w in &bank.witnesses {
            assert_ne!(
                w.calc_func.as_deref(),
                Some(w.expected.as_str()),
                "matching row leaked into bank-rows!"
            );
        }
        let pin_bytes = std::fs::read(&pin_path).expect("pin-out written");
        let pinned = WitnessSet::decode(&pin_bytes).expect("valid pinned WitnessSet");
        assert!(!pinned.witnesses.is_empty());
        assert!(!pinned.readings.is_empty());
        for r in &pinned.readings {
            assert_eq!(r.excel_build.as_deref(), Some("16.0.20625.20108"));
            assert_eq!(r.cpu.as_deref(), Some("Fake AMD Ryzen"));
            assert_eq!(r.bitness.as_deref(), Some("64-bit"));
        }
        for f in &summary.frontiers {
            if let (Some(lo), Some(hi)) = (f.excel_lo_bits, f.excel_hi_bits) {
                assert_eq!(
                    f64_to_compact(f64::from_bits(hi)),
                    f64_to_compact(f64::from_bits(lo)) + 1,
                    "frontier {} recorded non-adjacent Excel bits 0x{lo:016x}..0x{hi:016x}",
                    f.id
                );
            }
        }

        // Storage policy audit (dc-ztvv.26.7): recipe.json must record seed, generator version,
        // CalcFunc commit, Excel build, CPU, counts, and pass discrepancies-only / no-matching-readings audit.
        let audit = crate::fuzz::storage::verify_sweep_storage_policy(
            &cfg.out_dir,
            Some(&bank_path),
        )
        .expect("storage audit");
        assert!(audit.ok, "storage policy violations: {:?}", audit.violations);
        assert_eq!(audit.discrepancy_rows, summary.total_mismatched);
        assert_eq!(audit.matching_rows_in_bank, 0);
        let recipe: crate::fuzz::storage::SweepRecipe = serde_json::from_str(
            &std::fs::read_to_string(cfg.out_dir.join("recipe.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(recipe.excel_builds, vec!["16.0.20625.20108".to_owned()]);
        assert_eq!(recipe.cpus, vec!["Fake AMD Ryzen".to_owned()]);
        assert!(!recipe.storage_policy.matching_readings_stored);
        assert!(recipe.storage_policy.discrepancies_only);

        // Texture catalogue (dc-ztvv.26.8): textures.json must be written and retain interesting
        // cases EVEN WHEN THEY MATCH Excel (matched_excel == Some(true)) while bank-rows stays
        // strictly discrepancies-only!
        let tex_path = cfg.out_dir.join("textures.json");
        assert!(tex_path.is_file(), "expected textures.json from live run");
        let tex_cat = crate::fuzz::texture::read_texture_catalogue(&tex_path).expect("read textures");
        assert!(!tex_cat.entries.is_empty(), "expected non-empty texture catalogue");
        assert!(
            tex_cat.entries.iter().any(|e| e.matched_excel == Some(true)),
            "expected matching interesting texture cases to be retained in textures.json"
        );
        let tex_val = crate::fuzz::texture::validate_texture_catalogue_file(&tex_path)
            .expect("validate live textures");
        assert_eq!(tex_val.matched_recorded, tex_val.total);
        assert!(tex_val.drifted.is_empty());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn frontier_window_reuses_cached_seen_answers_and_rejects_unread_holes() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let tmp = std::env::temp_dir().join(format!("dnacalc-fuzz-window-cache-test-{port}"));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let endpoint = tmp.join("endpoint.json");
        std::fs::write(
            &endpoint,
            json!({"host": "127.0.0.1", "port": port, "token": "t"}).to_string(),
        )
        .unwrap();

        let anchor = -1.0_f64;
        let hi_val = step_ulps(anchor, 1, true).unwrap();
        let hi_hex = format!("{:016x}", hi_val.to_bits());
        let pv_unread_hex = format!("{:016x}", 2000.0_f64.to_bits());
        let asked_pairs = std::sync::Arc::new(std::sync::Mutex::new(Vec::<(String, String)>::new()));
        let asked_pairs_srv = asked_pairs.clone();

        let _server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            while let Some((kind, body)) = read_frame(&mut stream) {
                match kind {
                    b'H' => write_frame(
                        &mut stream,
                        b'H',
                        &json!({"v": "touchstone/1", "server": "fake-window-cache", "envs": {"e1": {"file_version": "16.0.20625.20108", "cpu": "Fake", "machine": "x86-64"}}}),
                    ),
                    b'S' => {
                        let id = body["id"].as_str().unwrap().to_string();
                        let mut answers = Vec::new();
                        for g in body["groups"].as_array().unwrap() {
                            let tokens: Vec<&str> = g["k"].as_str().unwrap().split(',').collect();
                            let width = tokens.len();
                            let n = g["n"].as_u64().unwrap() as usize;
                            let flat = g["v"].as_array().unwrap();
                            let mut ty = String::new();
                            let mut out_v = Vec::new();
                            let mut ity = String::new();
                            let mut iv = Vec::new();
                            for r in 0..n {
                                let pv_hex = flat[r * width + 2].as_str().unwrap().to_owned();
                                let g_hex = flat[r * width + 5].as_str().unwrap().to_owned();
                                asked_pairs_srv
                                    .lock()
                                    .unwrap()
                                    .push((pv_hex.clone(), g_hex.clone()));
                                for c in 0..width {
                                    ity.push('n');
                                    // Corrupt echo on frontier 1's `hi_val` to force RowOutcome::Unread!
                                    if pv_hex == pv_unread_hex && g_hex == hi_hex && c == 5 {
                                        iv.push(Value::String(format!("{:016x}", anchor.to_bits())));
                                    } else {
                                        iv.push(flat[r * width + c].clone());
                                    }
                                }
                                let g_val = f64::from_bits(u64::from_str_radix(&g_hex, 16).unwrap());
                                if g_val <= anchor {
                                    ty.push('e');
                                    out_v.push(Value::String("#VALUE!".to_owned()));
                                } else {
                                    ty.push('n');
                                    out_v.push(Value::String(format!("{:016x}", 0.05_f64.to_bits())));
                                }
                            }
                            let mut g_out = json!({"ty": ty, "v": out_v, "ity": ity, "iv": iv});
                            if let Some(o) = g.get("o") {
                                g_out["o"] = o.clone();
                            } else {
                                g_out["i"] = g["i"].clone();
                            }
                            answers.push(g_out);
                        }
                        write_frame(
                            &mut stream,
                            b'A',
                            &json!({"id": id, "seq": body["seq"], "env": "e1", "w": "w3", "g": answers}),
                        );
                        if body["last"] == json!(true) {
                            write_frame(&mut stream, b'D', &json!({"id": id, "calls": 0}));
                        }
                    }
                    _ => {}
                }
            }
        });

        let client = connect_client(Some(&endpoint)).unwrap();
        let ctx = touchstone_bulk_context();
        let base_cached = vec![
            hex_f64(10.0),
            hex_f64(-120.0),
            hex_f64(1000.0),
            hex_f64(0.0),
            hex_f64(0.0),
            hex_f64(0.1),
        ];
        let base_unread = vec![
            hex_f64(10.0),
            hex_f64(-120.0),
            hex_f64(2000.0),
            hex_f64(0.0),
            hex_f64(0.0),
            hex_f64(0.1),
        ];
        let mut frontiers = vec![
            Frontier {
                id: "frontier-cached".to_owned(),
                function: "RATE".to_owned(),
                axis: 5,
                axis_cell: None,
                axis_name: "guess".to_owned(),
                kind: "error:Value<->number".to_owned(),
                base_args: base_cached.clone(),
                dna_lo_bits: anchor.to_bits(),
                dna_hi_bits: hi_val.to_bits(),
                dna_lo_outcome: "error:Value".to_owned(),
                dna_hi_outcome: format!("0x{:016x}", 0.05_f64.to_bits()),
                excel_lo_bits: None,
                excel_hi_bits: None,
                excel_lo_outcome: None,
                excel_hi_outcome: None,
                shifted_ulps: None,
                depth: 0,
                scale: "ulp".to_owned(),
                transition_kind: "predicate_flip".to_owned(),
                parent_id: None,
            },
            Frontier {
                id: "frontier-unread-hole".to_owned(),
                function: "RATE".to_owned(),
                axis: 5,
                axis_cell: None,
                axis_name: "guess".to_owned(),
                kind: "error:Value<->number".to_owned(),
                base_args: base_unread,
                dna_lo_bits: anchor.to_bits(),
                dna_hi_bits: hi_val.to_bits(),
                dna_lo_outcome: "error:Value".to_owned(),
                dna_hi_outcome: format!("0x{:016x}", 0.05_f64.to_bits()),
                excel_lo_bits: None,
                excel_hi_bits: None,
                excel_lo_outcome: None,
                excel_hi_outcome: None,
                shifted_ulps: None,
                depth: 0,
                scale: "ulp".to_owned(),
                transition_kind: "predicate_flip".to_owned(),
                parent_id: None,
            },
        ];

        // Pre-seed seen_keys + excel_answers with frontier 0's `dna_lo_bits` (`-1.0`), as if
        // Round 0 already probed that anchor.
        let mut seen_keys = HashSet::new();
        let mut excel_answers = HashMap::new();
        let mut pre_args = base_cached;
        set_axis_f64(&mut pre_args, 5, None, anchor);
        let pre_key = canonical_case_key("RATE", &pre_args);
        seen_keys.insert(pre_key.clone());
        excel_answers.insert(pre_key, "error:Value".to_owned());

        let mut next_id = 0usize;
        let expanded = expand_frontier_windows_with_refs(
            &frontiers,
            2,
            &ctx,
            &mut seen_keys,
            1,
            &mut next_id,
        );
        let cases: Vec<FuzzCase> = expanded.cases.into_iter().map(|(_, c)| c).collect();
        let agg = ask_single_priority(
            &client,
            "RATE",
            &cases,
            INTERACTIVE_CHUNK_SIZE,
            2,
            Priority::Interactive,
            None,
        )
        .unwrap();
        let mut rstats = BTreeMap::new();
        let mut known = HashSet::new();
        let mut discs = Vec::new();
        let mut dparents = Vec::new();
        let mut fseeds = Vec::new();
        let (mut asked, mut matched, mut mismatched, mut harness_fault, mut unread, mut crashed) =
            (0, 0, 0, 0, 0, 0);
        let mut harness_faults: BTreeMap<String, usize> = BTreeMap::new();
        ingest_judged_cases(
            "RATE",
            &cases,
            &agg.rows,
            &mut excel_answers,
            &mut rstats,
            &mut known,
            &mut discs,
            &mut dparents,
            &mut fseeds,
            &mut asked,
            &mut matched,
            &mut mismatched,
            &mut harness_fault,
            &mut harness_faults,
            &mut unread,
            &mut crashed,
        );
        record_window_flips_from_cache(&mut frontiers, &expanded.window_refs, &excel_answers);

        // Frontier 0: `-1.0` was NOT re-asked of the broker, yet cached answer recovered the exact flip.
        let pv0_hex = format!("{:016x}", 1000.0_f64.to_bits());
        let anchor_hex = format!("{:016x}", anchor.to_bits());
        assert!(
            !asked_pairs
                .lock()
                .unwrap()
                .iter()
                .any(|(pv, g)| pv == &pv0_hex && g == &anchor_hex),
            "cached anchor should not be re-asked of Touchstone"
        );
        assert_eq!(frontiers[0].excel_lo_bits, Some(anchor.to_bits()));
        assert_eq!(frontiers[0].excel_hi_bits, Some(hi_val.to_bits()));
        assert_eq!(frontiers[0].shifted_ulps, Some(0));

        // Frontier 1: `hi_val` was Unread, leaving a non-adjacent hole that must NOT be recorded.
        assert_eq!(unread, 1);
        assert_eq!(frontiers[1].excel_lo_bits, None);
        assert_eq!(frontiers[1].excel_hi_bits, None);
        assert_eq!(frontiers[1].shifted_ulps, None);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn dry_run_and_live_run_produce_bit_identical_round_0_predictions() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let tmp = std::env::temp_dir().join(format!("dnacalc-fuzz-dry-live-parity-{port}"));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let endpoint = tmp.join("endpoint.json");
        std::fs::write(
            &endpoint,
            json!({"host": "127.0.0.1", "port": port, "token": "t"}).to_string(),
        )
        .unwrap();

        let _server = thread::spawn(move || serve_fake_rate_broker(listener));

        // Set round_size >= budget so Round 0 consumes the full main_budget; any reserve mismatch
        // between execute_dry_run and execute_live_run immediately diverges Round 0.
        let mut dry_cfg = FuzzConfig::default_for("RATE");
        dry_cfg.seed = 424242;
        dry_cfg.budget = 200;
        dry_cfg.min_budget = 200;
        dry_cfg.round_size = 200;
        dry_cfg.batch_size = 100;
        dry_cfg.in_flight = 2;
        dry_cfg.max_frontiers = 4;
        dry_cfg.frontier_window = 3;
        dry_cfg.dry_run = true;
        dry_cfg.out_dir = tmp.join("dry");

        let mut live_cfg = dry_cfg.clone();
        live_cfg.dry_run = false;
        live_cfg.out_dir = tmp.join("live");
        live_cfg.endpoint = Some(endpoint);

        let dry_summary = crate::fuzz::execute_dry_run(&dry_cfg).expect("dry run");
        let live_summary = execute_live_run(&live_cfg).expect("live run");

        assert_eq!(
            dry_summary.predictions_sha256[0],
            live_summary.predictions_sha256[0],
            "dry-run and live-run must generate bit-identical Round 0 predictions for identical config"
        );
        let dry_r0 = std::fs::read_to_string(dry_cfg.out_dir.join("round-00-predictions.jsonl")).unwrap();
        let live_r0 = std::fs::read_to_string(live_cfg.out_dir.join("round-00-predictions.jsonl")).unwrap();
        assert_eq!(dry_r0.lines().count(), live_r0.lines().count());
        assert_eq!(dry_r0, live_r0);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn shifted_frontier_bisection_pinpoints_excel_boundary_in_batched_64_point_steps() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let tmp = std::env::temp_dir().join(format!("dnacalc-fuzz-bisect-test-{port}"));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let endpoint = tmp.join("endpoint.json");
        std::fs::write(
            &endpoint,
            json!({"host": "127.0.0.1", "port": port, "token": "t"}).to_string(),
        )
        .unwrap();

        // Fake broker shifts the RATE guess <= -1.0 #VALUE! boundary to guess <= -0.999.
        let shifted_cut = -0.999_f64;
        let _server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            while let Some((kind, body)) = read_frame(&mut stream) {
                match kind {
                    b'H' => write_frame(
                        &mut stream,
                        b'H',
                        &json!({"v": "touchstone/1", "server": "fake-bisect", "envs": {"e1": {"file_version": "16.0.20625.20108", "build": "20625", "cpu": "Fake", "machine": "x86-64"}}}),
                    ),
                    b'S' => {
                        let id = body["id"].as_str().unwrap().to_string();
                        let mut answers = Vec::new();
                        for g in body["groups"].as_array().unwrap() {
                            let tokens: Vec<&str> = g["k"].as_str().unwrap().split(',').collect();
                            let width = tokens.len();
                            let n = g["n"].as_u64().unwrap() as usize;
                            let flat = g["v"].as_array().unwrap();
                            let mut ty = String::new();
                            let mut out_v = Vec::new();
                            let mut ity = String::new();
                            let mut iv = Vec::new();
                            for r in 0..n {
                                for c in 0..width {
                                    ity.push('n');
                                    iv.push(flat[r * width + c].clone());
                                }
                                let g_hex = flat[r * width + 5].as_str().unwrap();
                                let g_val = f64::from_bits(u64::from_str_radix(g_hex, 16).unwrap());
                                if g_val <= shifted_cut {
                                    ty.push('e');
                                    out_v.push(Value::String("#VALUE!".to_owned()));
                                } else {
                                    ty.push('n');
                                    out_v.push(Value::String(format!("{:016x}", 0.05_f64.to_bits())));
                                }
                            }
                            let mut g_out = json!({"ty": ty, "v": out_v, "ity": ity, "iv": iv});
                            if let Some(o) = g.get("o") {
                                g_out["o"] = o.clone();
                            } else {
                                g_out["i"] = g["i"].clone();
                            }
                            answers.push(g_out);
                        }
                        write_frame(
                            &mut stream,
                            b'A',
                            &json!({"id": id, "seq": body["seq"], "env": "e1", "w": "w3", "g": answers}),
                        );
                        if body["last"] == json!(true) {
                            write_frame(&mut stream, b'D', &json!({"id": id, "calls": 0}));
                        }
                    }
                    _ => {}
                }
            }
        });

        let client = connect_client(Some(&endpoint)).unwrap();
        let mut cfg = FuzzConfig::default_for("RATE");
        cfg.out_dir = tmp.join("out");
        let base_args = vec![
            hex_f64(10.0),
            hex_f64(-120.0),
            hex_f64(1000.0),
            hex_f64(0.0),
            hex_f64(0.0),
            hex_f64(0.1),
        ];
        let mut frontiers = vec![Frontier {
            id: "frontier-RATE-001".to_owned(),
            function: "RATE".to_owned(),
            axis: 5,
            axis_cell: None,
            axis_name: "guess".to_owned(),
            kind: "error:Value<->number".to_owned(),
            base_args,
            dna_lo_bits: (-1.0_f64).to_bits(),
            dna_hi_bits: step_ulps(-1.0, 1, true).unwrap().to_bits(),
            dna_lo_outcome: "error:Value".to_owned(),
            dna_hi_outcome: "0x3fa999999999999a".to_owned(),
            excel_lo_bits: None,
            excel_hi_bits: None,
            excel_lo_outcome: None,
            excel_hi_outcome: None,
            shifted_ulps: None,
            depth: 0,
            scale: "ulp".to_owned(),
            transition_kind: "predicate_flip".to_owned(),
            parent_id: None,
        }];
        let mut seen = HashSet::new();
        let mut excel_answers = HashMap::new();
        let mut hasher = StreamCaseHasher::new("RATE");
        let mut pred_hashes = Vec::new();
        let mut envs = BTreeMap::new();
        let mut workers = BTreeMap::new();
        let mut rstats = BTreeMap::new();
        let mut known = HashSet::new();
        let mut discs = Vec::new();
        let mut dparents = Vec::new();
        let mut fseeds = Vec::new();
        let (mut next_id, mut generated, mut asked, mut matched, mut mismatched, mut harness_fault, mut unread, mut crashed) =
            (0, 0, 0, 0, 0, 0, 0, 0);
        let mut harness_faults: BTreeMap<String, usize> = BTreeMap::new();
        let mut excel_s = 0.0;

        bisect_shifted_frontiers_excel(
            &cfg,
            &client,
            &mut frontiers,
            &mut seen,
            &mut excel_answers,
            &mut hasher,
            &mut pred_hashes,
            &mut envs,
            &mut workers,
            &mut rstats,
            &mut known,
            &mut discs,
            &mut dparents,
            &mut fseeds,
            &mut next_id,
            &mut generated,
            &mut asked,
            &mut matched,
            &mut mismatched,
            &mut harness_fault,
            &mut harness_faults,
            &mut unread,
            &mut crashed,
            &mut excel_s,
            0,
        )
        .expect("bisect succeeded");

        let f = &frontiers[0];
        assert_eq!(f.excel_lo_bits, Some(shifted_cut.to_bits()));
        assert_eq!(
            f.excel_hi_bits,
            Some(step_ulps(shifted_cut, 1, true).unwrap().to_bits())
        );
        assert!(f.shifted_ulps.unwrap() > 0);
        assert!(!pred_hashes.is_empty());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn shifted_frontier_bisection_dedups_seen_keys_and_picks_closest_flip() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let tmp = std::env::temp_dir().join(format!("dnacalc-fuzz-bisect-closest-test-{port}"));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let endpoint = tmp.join("endpoint.json");
        std::fs::write(
            &endpoint,
            json!({"host": "127.0.0.1", "port": port, "token": "t"}).to_string(),
        )
        .unwrap();

        // Anchor at -1.0; place:
        // 1) A far-left decoy flip at -200_000 ULPs (number -> #VALUE!).
        // 2) An interior island inside [-65_536, -1024] at [-40_000, -30_000] (number).
        // 3) The true closest flip at -5_000 ULPs (#VALUE! -> number, matching DNA direction).
        // Both Step 1 (bracket choice) and Step 2 (64-point refinement) must choose the closest flip (-5_000)
        // rather than breaking on the leftmost flip (-200_000 or -40_000).
        let anchor = -1.0_f64;
        let far_cut = step_ulps(anchor, -200_000, true).unwrap();
        let island_lo = step_ulps(anchor, -40_000, true).unwrap();
        let island_hi = step_ulps(anchor, -30_000, true).unwrap();
        let close_cut = step_ulps(anchor, -5_000, true).unwrap();
        let seen_by_broker = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let seen_by_broker_srv = seen_by_broker.clone();

        let _server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            while let Some((kind, body)) = read_frame(&mut stream) {
                match kind {
                    b'H' => write_frame(
                        &mut stream,
                        b'H',
                        &json!({"v": "touchstone/1", "server": "fake-bisect-closest", "envs": {"e1": {"file_version": "16.0.20625.20108", "cpu": "Fake", "machine": "x86-64"}}}),
                    ),
                    b'S' => {
                        let id = body["id"].as_str().unwrap().to_string();
                        let mut answers = Vec::new();
                        for g in body["groups"].as_array().unwrap() {
                            let tokens: Vec<&str> = g["k"].as_str().unwrap().split(',').collect();
                            let width = tokens.len();
                            let n = g["n"].as_u64().unwrap() as usize;
                            let flat = g["v"].as_array().unwrap();
                            let mut ty = String::new();
                            let mut out_v = Vec::new();
                            let mut ity = String::new();
                            let mut iv = Vec::new();
                            for r in 0..n {
                                for c in 0..width {
                                    ity.push('n');
                                    iv.push(flat[r * width + c].clone());
                                }
                                let g_hex = flat[r * width + 5].as_str().unwrap();
                                seen_by_broker_srv.lock().unwrap().push(g_hex.to_owned());
                                let g_val = f64::from_bits(u64::from_str_radix(g_hex, 16).unwrap());
                                let is_num = g_val <= far_cut
                                    || (g_val > island_lo && g_val <= island_hi)
                                    || g_val > close_cut;
                                if is_num {
                                    ty.push('n');
                                    out_v.push(Value::String(format!("{:016x}", 0.05_f64.to_bits())));
                                } else {
                                    ty.push('e');
                                    out_v.push(Value::String("#VALUE!".to_owned()));
                                }
                            }
                            let mut g_out = json!({"ty": ty, "v": out_v, "ity": ity, "iv": iv});
                            if let Some(o) = g.get("o") {
                                g_out["o"] = o.clone();
                            } else {
                                g_out["i"] = g["i"].clone();
                            }
                            answers.push(g_out);
                        }
                        write_frame(
                            &mut stream,
                            b'A',
                            &json!({"id": id, "seq": body["seq"], "env": "e1", "w": "w3", "g": answers}),
                        );
                        if body["last"] == json!(true) {
                            write_frame(&mut stream, b'D', &json!({"id": id, "calls": 0}));
                        }
                    }
                    _ => {}
                }
            }
        });

        let client = connect_client(Some(&endpoint)).unwrap();
        let mut cfg = FuzzConfig::default_for("RATE");
        cfg.out_dir = tmp.join("out");
        let base_args = vec![
            hex_f64(10.0),
            hex_f64(-120.0),
            hex_f64(1000.0),
            hex_f64(0.0),
            hex_f64(0.0),
            hex_f64(0.1),
        ];
        let mut frontiers = vec![Frontier {
            id: "frontier-RATE-001".to_owned(),
            function: "RATE".to_owned(),
            axis: 5,
            axis_cell: None,
            axis_name: "guess".to_owned(),
            kind: "error:Value<->number".to_owned(),
            base_args: base_args.clone(),
            dna_lo_bits: anchor.to_bits(),
            dna_hi_bits: step_ulps(anchor, 1, true).unwrap().to_bits(),
            dna_lo_outcome: "error:Value".to_owned(),
            dna_hi_outcome: "0x3fa999999999999a".to_owned(),
            excel_lo_bits: None,
            excel_hi_bits: None,
            excel_lo_outcome: None,
            excel_hi_outcome: None,
            shifted_ulps: None,
            depth: 0,
            scale: "ulp".to_owned(),
            transition_kind: "predicate_flip".to_owned(),
            parent_id: None,
        }];
        // Pre-seed seen_keys + excel_answers with dna_lo_bits (already probed in
        // expand_frontier_windows) and an exact Step 2 interior sample point so bisection must
        // reuse cached answers and never re-ask or double-bank them.
        let mut seen = HashSet::new();
        let mut excel_answers = HashMap::new();
        let mut pre_lo_args = base_args.clone();
        set_axis_f64(&mut pre_lo_args, 5, None, anchor);
        let pre_lo_key = canonical_case_key("RATE", &pre_lo_args);
        seen.insert(pre_lo_key.clone());
        excel_answers.insert(pre_lo_key, format!("0x{:016x}", 0.05_f64.to_bits()));

        // First interior point of Step 2 over [-65_536, -1_024]:
        // ca + (span * 1) / 64 = -65_536 + 64_512 / 64 = -64_528.
        let pre_step_val = step_ulps(anchor, -64_528, true).unwrap();
        let mut pre_step_args = base_args.clone();
        set_axis_f64(&mut pre_step_args, 5, None, pre_step_val);
        let pre_step_key = canonical_case_key("RATE", &pre_step_args);
        seen.insert(pre_step_key.clone());
        excel_answers.insert(pre_step_key, "error:Value".to_owned());

        let mut hasher = StreamCaseHasher::new("RATE");
        let mut pred_hashes = Vec::new();
        let mut envs = BTreeMap::new();
        let mut workers = BTreeMap::new();
        let mut rstats = BTreeMap::new();
        let mut known = HashSet::new();
        let mut discs = Vec::new();
        let mut dparents = Vec::new();
        let mut fseeds = Vec::new();
        let (mut next_id, mut generated, mut asked, mut matched, mut mismatched, mut harness_fault, mut unread, mut crashed) =
            (0, 0, 0, 0, 0, 0, 0, 0);
        let mut harness_faults: BTreeMap<String, usize> = BTreeMap::new();
        let mut excel_s = 0.0;

        bisect_shifted_frontiers_excel(
            &cfg,
            &client,
            &mut frontiers,
            &mut seen,
            &mut excel_answers,
            &mut hasher,
            &mut pred_hashes,
            &mut envs,
            &mut workers,
            &mut rstats,
            &mut known,
            &mut discs,
            &mut dparents,
            &mut fseeds,
            &mut next_id,
            &mut generated,
            &mut asked,
            &mut matched,
            &mut mismatched,
            &mut harness_fault,
            &mut harness_faults,
            &mut unread,
            &mut crashed,
            &mut excel_s,
            0,
        )
        .expect("bisect succeeded");

        // 1) Must never re-ask the pre-seen anchor or Step 2 point, or ask any point twice.
        let asked_hexes = seen_by_broker.lock().unwrap().clone();
        let anchor_hex = format!("{:016x}", anchor.to_bits());
        let pre_step_hex = format!("{:016x}", pre_step_val.to_bits());
        assert!(
            !asked_hexes.contains(&anchor_hex),
            "bisect re-asked pre-seen dna_lo_bits {anchor_hex}"
        );
        assert!(
            !asked_hexes.contains(&pre_step_hex),
            "bisect re-asked pre-seen Step 2 interior point {pre_step_hex}"
        );
        let unique_asked: HashSet<&String> = asked_hexes.iter().collect();
        assert_eq!(
            unique_asked.len(),
            asked_hexes.len(),
            "bisect asked duplicate points to Touchstone"
        );
        let unique_disc_keys: HashSet<String> = discs
            .iter()
            .map(|d| canonical_case_key("RATE", &d.case.args))
            .collect();
        assert_eq!(
            unique_disc_keys.len(),
            discs.len(),
            "discrepancies contain duplicate canonical rows"
        );

        // 2) Must pick the closest flip (-5_000), not the leftmost flip (-200_000 or -40_000).
        let f = &frontiers[0];
        assert_eq!(
            f.excel_lo_bits,
            Some(close_cut.to_bits()),
            "expected closest flip at -5000 ULPs, got {:?}",
            f.shifted_ulps
        );
        assert_eq!(
            f.excel_hi_bits,
            Some(step_ulps(close_cut, 1, true).unwrap().to_bits())
        );
        assert_eq!(f.shifted_ulps, Some(-5_000));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn echo_row_matches_flattened_touchstone_array_and_rejects_mismatch() {
        let b0 = (-100.0_f64).to_bits();
        let b1 = (60.0_f64).to_bits();
        let b2 = (60.0_f64).to_bits();
        let g = (0.1_f64).to_bits();
        let sent = vec![
            Arg::Array {
                rows: 3,
                cols: 1,
                bits: vec![b0, b1, b2],
            },
            Arg::Number(g),
        ];
        // Real Touchstone wire echo flattens a3x1 + n into 4 scalar Answer::Number entries.
        let flat_echo = vec![
            Answer::Number(b0),
            Answer::Number(b1),
            Answer::Number(b2),
            Answer::Number(g),
        ];
        assert!(echo_row_matches_sent(&sent, &flat_echo));

        // Single-bit change in any array cell must be rejected as unread.
        let mut bad_echo = flat_echo.clone();
        bad_echo[1] = Answer::Number(b1 ^ 1);
        assert!(!echo_row_matches_sent(&sent, &bad_echo));

        // Missing trailing scalar arg must be rejected.
        assert!(!echo_row_matches_sent(&sent, &flat_echo[..3]));

        // Matrix-form echo is also accepted.
        let matrix_echo = vec![
            Answer::Array {
                rows: 3,
                cols: 1,
                cells: vec![Answer::Number(b0), Answer::Number(b1), Answer::Number(b2)],
            },
            Answer::Number(g),
        ];
        assert!(echo_row_matches_sent(&sent, &matrix_echo));
    }

    #[test]
    fn coarse_discrepancy_fractal_refinement_pinpoints_excel_only_boundary() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let tmp = std::env::temp_dir().join(format!("dnacalc-fuzz-fractal-coarse-{port}"));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let endpoint = tmp.join("endpoint.json");
        std::fs::write(
            &endpoint,
            json!({"host": "127.0.0.1", "port": port, "token": "t"}).to_string(),
        )
        .unwrap();

        // Synthetic Excel-only boundary inside a smooth PV region (DNA Calc has NO flip around rate = 0.05..0.5):
        // Excel matches DNA Calc for rate <= secret_cut, and returns #NUM! for rate > secret_cut.
        let secret_cut = 0.125_f64;
        let _server = thread::spawn(move || {
            let ctx = touchstone_bulk_context();
            let (mut stream, _) = listener.accept().unwrap();
            while let Some((kind, body)) = read_frame(&mut stream) {
                match kind {
                    b'H' => write_frame(
                        &mut stream,
                        b'H',
                        &json!({"v": "touchstone/1", "server": "fake-fractal-coarse", "envs": {"e1": {"file_version": "16.0.20625.20108", "cpu": "Fake", "machine": "x86-64"}}}),
                    ),
                    b'S' => {
                        let id = body["id"].as_str().unwrap().to_string();
                        let mut answers = Vec::new();
                        for g in body["groups"].as_array().unwrap() {
                            let tokens: Vec<&str> = g["k"].as_str().unwrap().split(',').collect();
                            let width = tokens.len();
                            let n = g["n"].as_u64().unwrap() as usize;
                            let flat = g["v"].as_array().unwrap();
                            let mut ty = String::new();
                            let mut out_v = Vec::new();
                            let mut ity = String::new();
                            let mut iv = Vec::new();
                            for r in 0..n {
                                let mut args_json = Vec::with_capacity(width);
                                for c in 0..width {
                                    let h = flat[r * width + c].as_str().unwrap();
                                    args_json.push(Value::String(format!("0x{h}")));
                                    ity.push('n');
                                    iv.push(Value::String(h.to_owned()));
                                }
                                let rate = f64::from_bits(
                                    u64::from_str_radix(flat[r * width].as_str().unwrap(), 16).unwrap(),
                                );
                                if rate > secret_cut {
                                    ty.push('e');
                                    out_v.push(Value::String("#NUM!".to_owned()));
                                } else {
                                    let (dna, _, _) = evaluate_traced("PV", &args_json, &ctx);
                                    if let Some(hex) = dna.strip_prefix("0x") {
                                        ty.push('n');
                                        out_v.push(Value::String(hex.to_owned()));
                                    } else {
                                        ty.push('e');
                                        out_v.push(Value::String("#NUM!".to_owned()));
                                    }
                                }
                            }
                            let mut g_out = json!({"ty": ty, "v": out_v, "ity": ity, "iv": iv});
                            if let Some(o) = g.get("o") {
                                g_out["o"] = o.clone();
                            } else {
                                g_out["i"] = g["i"].clone();
                            }
                            answers.push(g_out);
                        }
                        write_frame(
                            &mut stream,
                            b'A',
                            &json!({"id": id, "seq": body["seq"], "env": "e1", "w": "w3", "g": answers}),
                        );
                        if body["last"] == json!(true) {
                            write_frame(&mut stream, b'D', &json!({"id": id, "calls": 0}));
                        }
                    }
                    _ => {}
                }
            }
        });

        let client = connect_client(Some(&endpoint)).unwrap();
        let spec = DomainSpec::for_function("PV").unwrap();
        let ctx = touchstone_bulk_context();
        let mut cfg = FuzzConfig::default_for("PV");
        cfg.out_dir = tmp.join("out");

        // Start from a single coarse discrepancy at rate = 0.25 (above secret_cut = 0.125).
        let disc_args = vec![
            hex_f64(0.25),
            hex_f64(10.0),
            hex_f64(-100.0),
            hex_f64(0.0),
            hex_f64(0.0),
        ];
        let (pred, trace, oc) = evaluate_traced("PV", &disc_args, &ctx);
        let disc_case = FuzzCase {
            id: "fuzz-PV-r00-000001".to_owned(),
            region: "special_values".to_owned(),
            round: 0,
            args: disc_args.clone(),
            prediction: pred,
            trace,
            outcome_class: oc,
        };
        let mut discrepancies = vec![DiscrepancyRow {
            case: disc_case,
            expected: "error:Num".to_owned(),
            severity: Severity::Structural,
            ulp: None,
            env_id: "e1".to_owned(),
            cluster_key: "error:Num_vs_number|coarse".to_owned(),
        }];
        let mut frontiers = Vec::new();
        let mut fractal_stats = FractalDiscoveryStats::default();
        let mut seen = HashSet::new();
        let mut excel_answers = HashMap::new();
        let disc_key = canonical_case_key("PV", &disc_args);
        seen.insert(disc_key.clone());
        excel_answers.insert(disc_key, "error:Num".to_owned());
        let mut hasher = StreamCaseHasher::new("PV");
        let mut pred_hashes = Vec::new();
        let mut envs = BTreeMap::new();
        let mut workers = BTreeMap::new();
        let mut rstats = BTreeMap::new();
        let mut known = HashSet::new();
        let mut dparents = Vec::new();
        let mut fseeds = Vec::new();
        let (
            mut next_id,
            mut generated,
            mut asked,
            mut matched,
            mut mismatched,
            mut harness_fault,
            mut unread,
            mut crashed,
        ) = (1, 0, 0, 0, 0, 0, 0, 0);
        let mut harness_faults: BTreeMap<String, usize> = BTreeMap::new();
        let mut excel_s = 0.0;

        let added = fractal_refine_coarse_discrepancies_excel(
            &cfg,
            &spec,
            &client,
            &mut frontiers,
            &mut fractal_stats,
            &mut seen,
            &mut excel_answers,
            &mut hasher,
            &mut pred_hashes,
            &mut envs,
            &mut workers,
            &mut rstats,
            &mut known,
            &mut discrepancies,
            &mut dparents,
            &mut fseeds,
            &mut next_id,
            &mut generated,
            &mut asked,
            &mut matched,
            &mut mismatched,
            &mut harness_fault,
            &mut harness_faults,
            &mut unread,
            &mut crashed,
            &mut excel_s,
            1,
            2,
        )
        .expect("coarse fractal refinement succeeded");

        assert!(added >= 1, "expected Excel-only coarse discrepancy boundary to be pinpointed");
        let f = frontiers
            .iter()
            .find(|f| f.axis == 0 && f.scale == "coarse_discrepancy")
            .expect("rate axis coarse_discrepancy frontier");
        assert_eq!(f.excel_lo_bits, Some(secret_cut.to_bits()));
        assert_eq!(
            f.excel_hi_bits,
            Some(step_ulps(secret_cut, 1, true).unwrap().to_bits())
        );
        assert!(f.excel_lo_outcome.as_deref().unwrap().starts_with("0x"));
        assert_eq!(f.excel_hi_outcome.as_deref(), Some("error:Num"));
        assert!(!pred_hashes.is_empty());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn reference_functions_send_real_ranges_and_dna_faults_never_bank_or_cluster() {
        // 1. COUNTIF: ParameterClass::Reference must get real `{"range": [[...]]}` -> `rRxC`,
        // matching through Touchstone + witnesses/call.rs with 0 ReferenceRequired faults.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let tmp = std::env::temp_dir().join(format!("dnacalc-fuzz-range-fault-test-{port}"));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let endpoint = tmp.join("endpoint.json");
        std::fs::write(
            &endpoint,
            json!({"host": "127.0.0.1", "port": port, "token": "t"}).to_string(),
        )
        .unwrap();
        let broker = thread::spawn(move || {
            // Serve 3 runs: COUNTIF, VALUE, OFFSET
            for _ in 0..3 {
                serve_fake_fuzz_broker(&listener, true, None);
            }
        });

        let mut cfg = FuzzConfig::default_for("COUNTIF");
        cfg.seed = 20261124;
        cfg.budget = 64;
        cfg.min_budget = 64;
        cfg.round_size = 64;
        cfg.batch_size = 32;
        cfg.in_flight = 1;
        cfg.max_frontiers = 0;
        cfg.fractal_depth = 0;
        cfg.out_dir = tmp.join("countif");
        cfg.bank_rows_out = Some(tmp.join("countif-bank.json"));
        cfg.endpoint = Some(endpoint.clone());
        cfg.sweep = Some(SweepPolicy::new(
            tmp.join("PAUSE"),
            tmp.join("bank/COUNTIF"),
        ));
        let countif = execute_live_run(&cfg).expect("COUNTIF live run");
        assert!(
            countif.total_matched > 40,
            "COUNTIF should match real cell ranges, got {}/{} matched (faults={:?})",
            countif.total_matched,
            countif.total_asked,
            countif.harness_faults
        );
        assert!(
            !countif.harness_faults.contains_key("ReferenceRequired"),
            "COUNTIF must not hit ReferenceRequired: {:?}",
            countif.harness_faults
        );
        assert_eq!(countif.total_mismatched, 0);
        assert!(countif.clusters.is_empty());
        assert!(!tmp.join("countif-bank.json").exists());

        // 2. VALUE: partial DNA-side `fault:NotImplemented` (missing locale profile) must be
        // counted in `total_harness_fault` / `harness_faults`, never mismatched/banked/clustered.
        let mut val_cfg = cfg.clone();
        val_cfg.function = "VALUE".to_owned();
        val_cfg.out_dir = tmp.join("value");
        val_cfg.bank_rows_out = Some(tmp.join("value-bank.json"));
        val_cfg.sweep = Some(SweepPolicy::new(tmp.join("PAUSE"), tmp.join("bank/VALUE")));
        let val = execute_live_run(&val_cfg).expect("VALUE live run");
        assert!(val.total_matched > 0, "VALUE should match non-locale cases");
        assert!(
            val.total_harness_fault > 0 && val.harness_faults.contains_key("NotImplemented"),
            "VALUE should count NotImplemented harness faults: {:?}",
            val.harness_faults
        );
        assert_eq!(val.total_mismatched, 0);
        assert!(val.clusters.is_empty());
        assert!(!tmp.join("value-bank.json").exists());

        // 3. OFFSET (without error-edge hints): every generated call faults with
        // WitnessReferencePosition -> skipped cleanly after Round 0 with 0 Excel calls,
        // 0 bank rows, and 0 clusters.
        let mut off_cfg = cfg.clone();
        off_cfg.function = "OFFSET".to_owned();
        off_cfg.hints_dir = Some(tmp.join("no-hints"));
        off_cfg.out_dir = tmp.join("offset");
        off_cfg.bank_rows_out = Some(tmp.join("offset-bank.json"));
        off_cfg.sweep = Some(SweepPolicy::new(tmp.join("PAUSE"), tmp.join("bank/OFFSET")));
        let off = execute_live_run(&off_cfg).expect("OFFSET live run");
        assert!(
            off.stop_reason.starts_with("skipped:dna-fault:"),
            "expected OFFSET to record skipped reason, got {}",
            off.stop_reason
        );
        assert_eq!(off.total_mismatched, 0);
        assert_eq!(off.total_matched, 0);
        assert_eq!(off.total_harness_fault, off.total_asked);
        assert!(off.clusters.is_empty());
        assert!(off.worker_calls.is_empty(), "skipped OFFSET must spend 0 Excel calls");
        assert!(!tmp.join("offset-bank.json").exists());

        broker.join().unwrap();
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn bisect_shifted_frontiers_skips_iteration_and_outcome_regime_frontiers() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let tmp = std::env::temp_dir().join(format!("dnacalc-fuzz-bisect-regime-skip-{port}"));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let endpoint = tmp.join("endpoint.json");
        std::fs::write(
            &endpoint,
            json!({"host": "127.0.0.1", "port": port, "token": "t"}).to_string(),
        )
        .unwrap();

        let query_count = Arc::new(AtomicUsize::new(0));
        let query_count_srv = Arc::clone(&query_count);
        let broker = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            while let Some((kind, body)) = read_frame(&mut stream) {
                match kind {
                    b'H' => write_frame(
                        &mut stream,
                        b'H',
                        &json!({"v": "touchstone/1", "server": "fake-regime-broker", "envs": {"e1": {"file_version": "16.0.20625.20108", "build": "20625", "cpu": "Fake", "machine": "x86-64"}}}),
                    ),
                    b'S' => {
                        query_count_srv.fetch_add(1, Ordering::SeqCst);
                        let id = body["id"].as_str().unwrap().to_string();
                        let mut answers = Vec::new();
                        for g in body["groups"].as_array().unwrap() {
                            let tokens: Vec<&str> = g["k"].as_str().unwrap().split(',').collect();
                            let width = tokens.len();
                            let n = g["n"].as_u64().unwrap() as usize;
                            let flat = g["v"].as_array().unwrap();
                            let mut ty = String::new();
                            let mut out_v = Vec::new();
                            let mut ity = String::new();
                            let mut iv = Vec::new();
                            for r in 0..n {
                                for c in 0..width {
                                    ity.push('n');
                                    iv.push(flat[r * width + c].clone());
                                }
                                let g_hex = flat[r * width + 5].as_str().unwrap();
                                let g_val = f64::from_bits(u64::from_str_radix(g_hex, 16).unwrap());
                                if g_val < -1.0 {
                                    ty.push('e');
                                    out_v.push(Value::String("#VALUE!".to_owned()));
                                } else {
                                    ty.push('n');
                                    out_v.push(Value::String(format!("{:016x}", 0.05_f64.to_bits())));
                                }
                            }
                            let mut g_out = json!({"ty": ty, "v": out_v, "ity": ity, "iv": iv});
                            if let Some(o) = g.get("o") {
                                g_out["o"] = o.clone();
                            } else {
                                g_out["i"] = g["i"].clone();
                            }
                            answers.push(g_out);
                        }
                        write_frame(
                            &mut stream,
                            b'A',
                            &json!({"id": id, "seq": body["seq"], "env": "e1", "w": "w3", "g": answers}),
                        );
                        if body["last"] == json!(true) {
                            write_frame(&mut stream, b'D', &json!({"id": id, "calls": 0}));
                        }
                    }
                    _ => {}
                }
            }
        });

        let client = connect_client(Some(&endpoint)).unwrap();
        let mut cfg = FuzzConfig::default_for("RATE");
        cfg.out_dir = tmp.join("out");
        let base_args = vec![
            hex_f64(10.0),
            hex_f64(-120.0),
            hex_f64(1000.0),
            hex_f64(0.0),
            hex_f64(0.0),
            hex_f64(0.1),
        ];
        let lo = 0.1_f64.to_bits();
        let hi = step_ulps(0.1, 1, true).unwrap().to_bits();
        let mut frontiers = vec![
            Frontier {
                id: "frontier-RATE-001".to_owned(),
                function: "RATE".to_owned(),
                axis: 5,
                axis_cell: None,
                axis_name: "guess".to_owned(),
                kind: "iter:2<->3".to_owned(),
                base_args: base_args.clone(),
                dna_lo_bits: lo,
                dna_hi_bits: hi,
                dna_lo_outcome: "0x3fa999999999999a".to_owned(),
                dna_hi_outcome: "0x3fa999999999999a".to_owned(),
                excel_lo_bits: None,
                excel_hi_bits: None,
                excel_lo_outcome: None,
                excel_hi_outcome: None,
                shifted_ulps: None,
                depth: 0,
                scale: "ulp".to_owned(),
                transition_kind: "iteration_regime".to_owned(),
                parent_id: None,
            },
            Frontier {
                id: "frontier-RATE-002".to_owned(),
                function: "RATE".to_owned(),
                axis: 5,
                axis_cell: None,
                axis_name: "guess".to_owned(),
                kind: "regime:number<->zero".to_owned(),
                base_args: base_args.clone(),
                dna_lo_bits: lo,
                dna_hi_bits: hi,
                dna_lo_outcome: "0x3fa999999999999a".to_owned(),
                dna_hi_outcome: "0x0000000000000000".to_owned(),
                excel_lo_bits: None,
                excel_hi_bits: None,
                excel_lo_outcome: None,
                excel_hi_outcome: None,
                shifted_ulps: None,
                depth: 0,
                scale: "ulp".to_owned(),
                transition_kind: "outcome_regime".to_owned(),
                parent_id: None,
            },
            Frontier {
                id: "frontier-RATE-003".to_owned(),
                function: "RATE".to_owned(),
                axis: 5,
                axis_cell: None,
                axis_name: "guess".to_owned(),
                kind: "route_flip".to_owned(),
                base_args,
                dna_lo_bits: lo,
                dna_hi_bits: hi,
                dna_lo_outcome: "0x3fa999999999999a".to_owned(),
                dna_hi_outcome: "0x3fa999999999999a".to_owned(),
                excel_lo_bits: None,
                excel_hi_bits: None,
                excel_lo_outcome: None,
                excel_hi_outcome: None,
                shifted_ulps: None,
                depth: 0,
                scale: "ulp".to_owned(),
                transition_kind: "route_switch".to_owned(),
                parent_id: None,
            },
        ];

        let mut seen = HashSet::new();
        let mut excel_answers = HashMap::new();
        let mut hasher = StreamCaseHasher::new("RATE");
        let mut pred_hashes = Vec::new();
        let mut envs = BTreeMap::new();
        let mut workers = BTreeMap::new();
        let mut rstats = BTreeMap::new();
        let mut known = HashSet::new();
        let mut discs = Vec::new();
        let mut dparents = Vec::new();
        let mut fseeds = Vec::new();
        let (mut next_id, mut generated, mut asked, mut matched, mut mismatched, mut harness_fault, mut unread, mut crashed) =
            (0, 0, 0, 0, 0, 0, 0, 0);
        let mut harness_faults: BTreeMap<String, usize> = BTreeMap::new();
        let mut excel_s = 0.0;

        bisect_shifted_frontiers_excel(
            &cfg,
            &client,
            &mut frontiers,
            &mut seen,
            &mut excel_answers,
            &mut hasher,
            &mut pred_hashes,
            &mut envs,
            &mut workers,
            &mut rstats,
            &mut known,
            &mut discs,
            &mut dparents,
            &mut fseeds,
            &mut next_id,
            &mut generated,
            &mut asked,
            &mut matched,
            &mut mismatched,
            &mut harness_fault,
            &mut harness_faults,
            &mut unread,
            &mut crashed,
            &mut excel_s,
            0,
        )
        .expect("bisect succeeded");

        drop(client);
        broker.join().unwrap();
        let _ = std::fs::remove_dir_all(&tmp);

        assert_eq!(
            query_count.load(Ordering::SeqCst),
            0,
            "non-predicate frontiers (iteration_regime, outcome_regime, route_switch) must not probe Excel"
        );
        assert_eq!(asked, 0);
        assert_eq!(generated, 0);
        for f in &frontiers {
            assert_eq!(f.excel_lo_bits, None, "frontier {} must remain unshifted", f.id);
            assert_eq!(f.shifted_ulps, None, "frontier {} must not log bogus shift", f.id);
        }
    }

    #[test]
    fn coarse_discrepancy_escalation_probes_later_axes_and_marks_excel_only_unshifted() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let tmp = std::env::temp_dir().join(format!("dnacalc-fuzz-coarse-later-axis-{port}"));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let endpoint = tmp.join("endpoint.json");
        std::fs::write(
            &endpoint,
            json!({"host": "127.0.0.1", "port": port, "token": "t"}).to_string(),
        )
        .unwrap();

        // Excel-only #NUM! cutoff on RATE's 6th parameter (axis 5 = `guess`) at guess > 0.125.
        let secret_cut = 0.125_f64;
        let _server = thread::spawn(move || {
            let ctx = touchstone_bulk_context();
            let (mut stream, _) = listener.accept().unwrap();
            while let Some((kind, body)) = read_frame(&mut stream) {
                match kind {
                    b'H' => write_frame(
                        &mut stream,
                        b'H',
                        &json!({"v": "touchstone/1", "server": "fake-coarse-axis5", "envs": {"e1": {"file_version": "16.0.20625.20108", "build": "20625", "cpu": "Fake", "machine": "x86-64"}}}),
                    ),
                    b'S' => {
                        let id = body["id"].as_str().unwrap().to_string();
                        let mut answers = Vec::new();
                        for g in body["groups"].as_array().unwrap() {
                            let tokens: Vec<&str> = g["k"].as_str().unwrap().split(',').collect();
                            let width = tokens.len();
                            let n = g["n"].as_u64().unwrap() as usize;
                            let flat = g["v"].as_array().unwrap();
                            let mut ty = String::new();
                            let mut out_v = Vec::new();
                            let mut ity = String::new();
                            let mut iv = Vec::new();
                            for r in 0..n {
                                let mut args_json = Vec::with_capacity(width);
                                for c in 0..width {
                                    let h = flat[r * width + c].as_str().unwrap();
                                    ity.push('n');
                                    iv.push(Value::String(h.to_owned()));
                                    args_json.push(Value::String(format!("0x{h}")));
                                }
                                let guess = f64::from_bits(
                                    u64::from_str_radix(flat[r * width + 5].as_str().unwrap(), 16).unwrap(),
                                );
                                if guess > secret_cut {
                                    ty.push('e');
                                    out_v.push(Value::String("#NUM!".to_owned()));
                                } else {
                                    let (dna, _, _) = evaluate_traced("RATE", &args_json, &ctx);
                                    if let Some(hex) = dna.strip_prefix("0x") {
                                        ty.push('n');
                                        out_v.push(Value::String(hex.to_owned()));
                                    } else {
                                        ty.push('e');
                                        out_v.push(Value::String("#NUM!".to_owned()));
                                    }
                                }
                            }
                            let mut g_out = json!({"ty": ty, "v": out_v, "ity": ity, "iv": iv});
                            if let Some(o) = g.get("o") {
                                g_out["o"] = o.clone();
                            } else {
                                g_out["i"] = g["i"].clone();
                            }
                            answers.push(g_out);
                        }
                        write_frame(
                            &mut stream,
                            b'A',
                            &json!({"id": id, "seq": body["seq"], "env": "e1", "w": "w3", "g": answers}),
                        );
                        if body["last"] == json!(true) {
                            write_frame(&mut stream, b'D', &json!({"id": id, "calls": 0}));
                        }
                    }
                    _ => {}
                }
            }
        });

        let client = connect_client(Some(&endpoint)).unwrap();
        let spec = DomainSpec::for_function("RATE").unwrap();
        let ctx = touchstone_bulk_context();
        let mut cfg = FuzzConfig::default_for("RATE");
        cfg.out_dir = tmp.join("out");

        let disc_args = vec![
            hex_f64(10.0),
            hex_f64(-120.0),
            hex_f64(1000.0),
            hex_f64(0.0),
            hex_f64(0.0),
            hex_f64(0.25),
        ];
        let (pred, trace, oc) = evaluate_traced("RATE", &disc_args, &ctx);
        let disc_case = FuzzCase {
            id: "fuzz-RATE-r00-000001".to_owned(),
            region: "special_values".to_owned(),
            round: 0,
            args: disc_args.clone(),
            prediction: pred,
            trace,
            outcome_class: oc,
        };
        let mut discrepancies = vec![DiscrepancyRow {
            case: disc_case,
            expected: "error:Num".to_owned(),
            severity: Severity::Structural,
            ulp: None,
            env_id: "e1".to_owned(),
            cluster_key: "error:Num_vs_number|guess".to_owned(),
        }];
        let mut frontiers = Vec::new();
        let mut fractal_stats = FractalDiscoveryStats::default();
        let mut seen = HashSet::new();
        let mut excel_answers = HashMap::new();
        let disc_key = canonical_case_key("RATE", &disc_args);
        seen.insert(disc_key.clone());
        excel_answers.insert(disc_key, "error:Num".to_owned());
        let mut hasher = StreamCaseHasher::new("RATE");
        let mut pred_hashes = Vec::new();
        let mut envs = BTreeMap::new();
        let mut workers = BTreeMap::new();
        let mut rstats = BTreeMap::new();
        let mut known = HashSet::new();
        let mut dparents = Vec::new();
        let mut fseeds = Vec::new();
        let (mut next_id, mut generated, mut asked, mut matched, mut mismatched, mut harness_fault, mut unread, mut crashed) =
            (1, 0, 0, 0, 0, 0, 0, 0);
        let mut harness_faults: BTreeMap<String, usize> = BTreeMap::new();
        let mut excel_s = 0.0;

        let added = fractal_refine_coarse_discrepancies_excel(
            &cfg,
            &spec,
            &client,
            &mut frontiers,
            &mut fractal_stats,
            &mut seen,
            &mut excel_answers,
            &mut hasher,
            &mut pred_hashes,
            &mut envs,
            &mut workers,
            &mut rstats,
            &mut known,
            &mut discrepancies,
            &mut dparents,
            &mut fseeds,
            &mut next_id,
            &mut generated,
            &mut asked,
            &mut matched,
            &mut mismatched,
            &mut harness_fault,
            &mut harness_faults,
            &mut unread,
            &mut crashed,
            &mut excel_s,
            1,
            2,
        )
        .expect("coarse escalation succeeded");

        assert!(added >= 1, "expected coarse escalation to probe axis 5 (guess)");
        let f = frontiers
            .iter()
            .find(|f| f.axis == 5 && f.scale == "coarse_discrepancy")
            .expect("axis 5 (guess) coarse_discrepancy frontier must be discovered");
        assert_eq!(f.excel_lo_bits, Some(secret_cut.to_bits()));
        assert_eq!(
            f.excel_hi_bits,
            Some(step_ulps(secret_cut, 1, true).unwrap().to_bits())
        );
        assert_eq!(
            f.shifted_ulps, None,
            "Excel-only boundary where DNA Calc did not flip must NOT report shifted_ulps=Some(0)"
        );
        assert_eq!(f.transition_kind, "excel_only_predicate_flip");
        let j = f.to_json();
        assert!(j["dna_lo_bits"].is_null(), "Excel-only frontier JSON must not fabricate dna_lo_bits: {j}");
        assert!(j["dna_hi_bits"].is_null(), "Excel-only frontier JSON must not fabricate dna_hi_bits: {j}");
        assert!(j["shifted_ulps"].is_null(), "Excel-only frontier JSON must have shifted_ulps=null: {j}");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn dna_fault_skipped_run_never_reaches_fractal_or_excel_even_with_max_frontiers() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let tmp = std::env::temp_dir().join(format!("dnacalc-fuzz-dna-fault-skip-{port}"));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let endpoint = tmp.join("endpoint.json");
        std::fs::write(
            &endpoint,
            json!({"host": "127.0.0.1", "port": port, "token": "t"}).to_string(),
        )
        .unwrap();

        let submissions = Arc::new(AtomicUsize::new(0));
        let sub_srv = Arc::clone(&submissions);
        let broker = thread::spawn(move || {
            // Serve 2 runs: sweep mode and standalone mode, both 100% DNA-faulting OFFSET,
            // plus a 3rd partially faulting OFFSET run (with hints) through the standard fake broker.
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                while let Some((kind, body)) = read_frame(&mut stream) {
                    match kind {
                        b'H' => {
                            write_frame(
                                &mut stream,
                                b'H',
                                &json!({
                                    "v": "touchstone/1",
                                    "server": "fake-fault-guard-broker",
                                    "sweep": true,
                                    "envs": {}
                                }),
                            );
                        }
                        b'S' | b'B' => {
                            sub_srv.fetch_add(1, Ordering::SeqCst);
                            let id = body["id"].as_str().unwrap_or("x").to_owned();
                            write_frame(
                                &mut stream,
                                b'X',
                                &json!({"id": id, "message": "unexpected submission on skipped:dna-fault run"}),
                            );
                        }
                        _ => {}
                    }
                }
            }
            serve_fake_fuzz_broker(&listener, true, None);
        });

        // Case 1: Sweep mode with max_frontiers > 0 and fractal_depth > 0.
        // Budget 70 with max_frontiers = 8 yields main_round_budget = 65 -> Round 0 want = 64
        // (100% WitnessReferencePosition faults on OFFSET without hints).
        let mut cfg = FuzzConfig::default_for("OFFSET");
        cfg.seed = 20261124;
        cfg.budget = 70;
        cfg.min_budget = 70;
        cfg.round_size = 64;
        cfg.batch_size = 32;
        cfg.in_flight = 1;
        cfg.max_frontiers = 8;
        cfg.fractal_depth = 1;
        cfg.frontier_window = 4;
        cfg.hints_dir = Some(tmp.join("no-hints"));
        cfg.endpoint = Some(endpoint.clone());
        cfg.out_dir = tmp.join("offset-sweep");
        cfg.sweep = Some(SweepPolicy::new(tmp.join("PAUSE"), tmp.join("bank/OFFSET")));

        let sweep_summary = execute_live_run(&cfg).expect("OFFSET sweep run");
        assert!(
            sweep_summary.stop_reason.starts_with("skipped:dna-fault:"),
            "expected skipped:dna-fault:* stop_reason, got {}",
            sweep_summary.stop_reason
        );
        assert_eq!(
            sweep_summary.fractal_stats.local_evals, 0,
            "skipped:dna-fault run must never reach fractal_discover_local"
        );
        assert!(
            sweep_summary.frontiers.is_empty(),
            "skipped:dna-fault run must emit 0 frontiers, got {:?}",
            sweep_summary.frontiers
        );
        assert!(
            sweep_summary.worker_calls.is_empty(),
            "skipped:dna-fault run must spend 0 Excel calls, got {:?}",
            sweep_summary.worker_calls
        );
        assert_eq!(sweep_summary.total_matched, 0);
        assert_eq!(sweep_summary.total_mismatched, 0);
        assert!(sweep_summary.total_asked > 0);
        assert_eq!(sweep_summary.total_harness_fault, sweep_summary.total_asked);

        // Case 2: Standalone mode (sweep = None) with max_frontiers > 0 also skips cleanly.
        let mut standalone_cfg = cfg.clone();
        standalone_cfg.sweep = None;
        standalone_cfg.out_dir = tmp.join("offset-standalone");
        let standalone_summary = execute_live_run(&standalone_cfg).expect("OFFSET standalone run");
        assert!(
            standalone_summary.stop_reason.starts_with("skipped:dna-fault:"),
            "expected standalone skipped:dna-fault:*, got {}",
            standalone_summary.stop_reason
        );
        assert_eq!(standalone_summary.fractal_stats.local_evals, 0);
        assert!(standalone_summary.frontiers.is_empty());
        assert!(standalone_summary.worker_calls.is_empty());

        // Case 3: Partially faulting OFFSET (with default hints) and max_frontiers > 0
        // never emits any DNA-fault frontier or chases a fault edge on Excel.
        let mut partial_cfg = cfg.clone();
        partial_cfg.hints_dir = None;
        partial_cfg.out_dir = tmp.join("offset-partial");
        partial_cfg.sweep = Some(SweepPolicy::new(tmp.join("PAUSE"), tmp.join("bank/OFFSET-p")));
        let partial_summary = execute_live_run(&partial_cfg).expect("OFFSET partial run");
        assert!(
            partial_summary.total_matched > 0 && partial_summary.total_harness_fault > 0,
            "expected partially faulting OFFSET with hints (matched={}, fault={})",
            partial_summary.total_matched,
            partial_summary.total_harness_fault
        );
        for f in &partial_summary.frontiers {
            assert!(
                !is_dna_fault(&f.dna_lo_outcome)
                    && !is_dna_fault(&f.dna_hi_outcome)
                    && !f.kind.contains("fault:"),
                "partially faulting run emitted a DNA-fault frontier: {f:?}"
            );
        }

        broker.join().unwrap();
        assert_eq!(
            submissions.load(Ordering::SeqCst),
            0,
            "100% DNA-faulting runs must send 0 Touchstone submissions to Excel"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
