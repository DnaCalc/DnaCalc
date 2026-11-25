//! The continuous, low-priority queue uses the ordinary live fuzz run for each recipe.

use super::{FuzzConfig, rng::GENERATOR_VERSION, utc_timestamp_now};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::cell::OnceCell;
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::Duration;

const USAGE: &str = "cargo xtask fuzz sweep [--plan] [--order <tsv>] [--out <dir>] [--runs <N>] [--seed <N>] [--endpoint <path>] [--bank-rows-out <dir>] [--tracker <dedicated master clone>] [--function-timeout-seconds <N>]";
const RETRY_DELAY: Duration = Duration::from_secs(15);
const DEFAULT_FUNCTION_TIMEOUT: Duration = Duration::from_secs(300);
const MAX_FAILED_ATTEMPTS: u64 = 3;
const BASE_BACKOFF_RUNS: u64 = 4;
const MAX_BACKOFF_SHIFT: u32 = 7;

#[derive(Debug)]
struct Options {
    order: PathBuf,
    out: PathBuf,
    endpoint: Option<PathBuf>,
    bank: Option<PathBuf>,
    tracker: Option<PathBuf>,
    runs: Option<usize>,
    seed: u64,
    plan: bool,
    function_timeout: Duration,
}

impl Options {
    fn parse(raw: &[String]) -> Result<Self, String> {
        let mut options = Self {
            order: "docs/fuzz/order.tsv".into(),
            out: "target/fuzz/sweep".into(),
            endpoint: None,
            bank: None,
            tracker: None,
            runs: None,
            seed: 20261123,
            plan: false,
            function_timeout: DEFAULT_FUNCTION_TIMEOUT,
        };
        let mut args = raw.iter();
        while let Some(flag) = args.next() {
            if flag == "--plan" {
                options.plan = true;
                continue;
            }
            let value = args.next().ok_or_else(|| format!("{flag} needs a value"))?;
            match flag.as_str() {
                "--order" => options.order = value.into(),
                "--out" => options.out = value.into(),
                "--endpoint" => options.endpoint = Some(value.into()),
                "--bank-rows-out" => options.bank = Some(value.into()),
                "--tracker" => options.tracker = Some(value.into()),
                "--runs" => options.runs = Some(value.parse().map_err(|_| "invalid --runs")?),
                "--seed" => options.seed = value.parse().map_err(|_| "invalid --seed")?,
                "--function-timeout-seconds" => {
                    let seconds: std::num::NonZeroU64 =
                        value.parse().map_err(|_| "invalid --function-timeout-seconds")?;
                    options.function_timeout = Duration::from_secs(seconds.get());
                }
                _ => return Err(format!("unknown sweep option {flag}")),
            }
        }
        Ok(options)
    }
}

#[derive(Debug)]
struct Entry {
    function: String,
    rank: usize,
    class: String,
    priority: u64,
}

fn parse_order(contents: &str) -> Result<Vec<Entry>, String> {
    let mut lines = contents
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty());
    let header: Vec<_> = lines
        .next()
        .ok_or("empty sweep order")?
        .split('\t')
        .collect();
    let columns = ["function", "rank", "bank_class", "priority_score"].map(|name| {
        header
            .iter()
            .position(|column| *column == name)
            .ok_or_else(|| format!("missing {name} column"))
    });
    let [function, rank, class, priority] = columns;
    let (function, rank, class, priority) = (function?, rank?, class?, priority?);
    let mut entries = Vec::new();
    let mut names = BTreeSet::new();
    let mut ranks = BTreeSet::new();
    for line in lines {
        let row: Vec<_> = line.split('\t').collect();
        let name = row.get(function).copied().unwrap_or_default();
        let rank: usize = row
            .get(rank)
            .and_then(|v| v.parse().ok())
            .ok_or("invalid rank")?;
        let score = row.get(priority).ok_or("missing priority score")?;
        let (whole, fraction) = score.split_once('.').unwrap_or((score, ""));
        // The committed scores have four fractional digits. Scale with integers throughout.
        if fraction.len() > 4 || !fraction.bytes().all(|b| b.is_ascii_digit()) {
            return Err(format!("invalid priority score for {name}"));
        }
        let whole: u64 = whole
            .parse()
            .map_err(|_| format!("invalid priority score for {name}"))?;
        let fraction: u64 = format!("{fraction:0<4}")
            .parse()
            .map_err(|_| "invalid score fraction")?;
        let priority = whole
            .checked_mul(10_000)
            .and_then(|v| v.checked_add(fraction))
            .ok_or("priority score overflow")?;
        if rank == 0
            || name.is_empty()
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_')
            || !names.insert(name)
            || !ranks.insert(rank)
        {
            return Err(format!("invalid or duplicate function or rank for {name}"));
        }
        entries.push(Entry {
            function: name.to_owned(),
            rank,
            class: row.get(class).ok_or("missing bank class")?.to_string(),
            priority,
        });
    }
    if entries.is_empty() {
        return Err("sweep order contains no functions".into());
    }
    Ok(entries)
}

impl Entry {
    fn budget(&self, max_priority: u64) -> usize {
        // Every function gets coverage. The most intensive recipe still fits a small sweep.
        64 + (448 * u128::from(self.priority) / u128::from(max_priority.max(1))) as usize
    }

    fn class_order(&self) -> u8 {
        match self.class.as_str() {
            "gross" | "structural" => 0,
            "numeric" => 1,
            _ => 2,
        }
    }
}

fn ordered<'a>(
    entries: &'a [Entry],
    last: &BTreeMap<String, u64>,
    backoff_until: &BTreeMap<String, u64>,
    next_sequence: u64,
) -> Vec<&'a Entry> {
    let mut queue: Vec<_> = entries.iter().collect();
    // Ready functions come before backed-off ones; within ready, unvisited first, then oldest.
    queue.sort_by_key(|e| {
        let deferred = backoff_until
            .get(&e.function)
            .copied()
            .filter(|&until| until > next_sequence);
        (
            deferred.is_some(),
            deferred.unwrap_or(0),
            last.get(&e.function).copied(),
            Reverse(e.priority),
            e.class_order(),
            e.rank,
        )
    });
    queue
}

#[derive(Debug, Serialize, Deserialize)]
struct GaveUp {
    function: String,
    attempts: u64,
    last_attempt: u64,
    finished_utc: String,
    reason: String,
    error: String,
    consecutive_give_ups: u32,
    backoff_runs: u64,
    retry_after_sequence: u64,
    calc_func_commit: String,
}

fn backoff_runs_for(consecutive_give_ups: u32) -> u64 {
    let shift = consecutive_give_ups.saturating_sub(1).min(MAX_BACKOFF_SHIFT);
    BASE_BACKOFF_RUNS << shift
}

fn persistent_failures(run: &Path) -> Result<(u64, u64, Option<String>), String> {
    if !run.is_dir() {
        return Ok((0, 0, None));
    }
    let mut count = 0u64;
    let mut last_attempt = 0u64;
    let mut last_error = None;
    for (n, dir) in numbered(run)? {
        let interrupted = dir.join("interrupted.json");
        if interrupted.is_file() {
            let doc: Value = read_json(&interrupted)?;
            let error = doc["error"].as_str();
            if doc["harness_fault"] != "FunctionDeadline"
                && error.is_some_and(is_transient_infra_error)
            {
                continue;
            }
            count += 1;
            last_attempt = n;
            last_error = error.map(str::to_owned);
        }
    }
    Ok((count, last_attempt, last_error))
}

fn is_transient_infra_error(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("connection lost")
        || lower.contains("timed out")
        || lower.contains("worker-down")
        || lower.contains("sweep-deferred")
        || lower.contains("isolation-stalled")
        || lower == "sweep paused"
        || lower.contains("broken pipe")
        || lower.contains("connection refused")
}

fn sanitize_reason(attempts: u64, error: &str) -> String {
    let compact: String = error
        .chars()
        .map(|c| match c {
            '\t' | '\r' | '\n' => ' ',
            _ => c,
        })
        .collect();
    let trimmed = compact.trim();
    let prefix = format!("gave-up:{attempts}-attempts: ");
    if trimmed.is_empty() {
        format!("gave-up:{attempts}-attempts")
    } else if trimmed.len() > 200 {
        let mut end = 200;
        while end > 0 && !trimmed.is_char_boundary(end) {
            end -= 1;
        }
        format!("{prefix}{}...", &trimmed[..end])
    } else {
        format!("{prefix}{trimmed}")
    }
}

fn record_give_up(
    run: &Path,
    sequence: u64,
    recipe: &Recipe,
    versions: &Versions,
    attempts: u64,
    last_attempt: u64,
    error: &str,
    last: &mut BTreeMap<String, u64>,
    give_ups: &mut BTreeMap<String, u32>,
    backoff_until: &mut BTreeMap<String, u64>,
) -> Result<GaveUp, String> {
    let consecutive_give_ups = give_ups.get(&recipe.function).copied().unwrap_or(0) + 1;
    let backoff_runs = backoff_runs_for(consecutive_give_ups);
    let retry_after_sequence = sequence.saturating_add(backoff_runs);
    let gave_up = GaveUp {
        function: recipe.function.clone(),
        attempts,
        last_attempt,
        finished_utc: utc_timestamp_now(),
        reason: sanitize_reason(attempts, error),
        error: error.to_owned(),
        consecutive_give_ups,
        backoff_runs,
        retry_after_sequence,
        calc_func_commit: versions.calc_func_commit.clone(),
    };
    write_json(&run.join("gave_up.json"), &gave_up)?;
    last.insert(recipe.function.clone(), sequence);
    give_ups.insert(recipe.function.clone(), consecutive_give_ups);
    backoff_until.insert(recipe.function.clone(), retry_after_sequence);
    eprintln!(
        "sweep {} seed {} gave up after {} failed attempts (backoff {} runs, retry after seq {}): {}",
        recipe.function,
        recipe.seed,
        attempts,
        backoff_runs,
        retry_after_sequence,
        gave_up.reason
    );
    Ok(gave_up)
}

fn refresh_coverage(options: &Options) -> Result<(), String> {
    let beads = options
        .tracker
        .as_ref()
        .map(|t| t.join(".beads/issues.jsonl"))
        .filter(|p| p.is_file());
    super::coverage::write_sweep_coverage(
        &options.out,
        beads.as_deref(),
        &options.out.join("coverage.tsv"),
    )
}

#[derive(Debug, Serialize, Deserialize)]
struct Recipe {
    function: String,
    seed: u64,
    budget: usize,
    rank: usize,
    bank_class: String,
    order_sha256: String,
}

#[derive(Debug, Serialize)]
struct Attempt<'a> {
    recipe: &'a Recipe,
    generator_version: &'static str,
    versions: &'a Versions,
    started_utc: String,
    function_timeout_seconds: u64,
    batch_size: usize,
    in_flight: usize,
    round_size: usize,
    min_budget: usize,
    max_frontiers: usize,
    fractal_depth: usize,
    frontier_window: i64,
    dry_rounds: usize,
    lane: &'a str,
}

#[derive(Debug, Serialize)]
struct Versions {
    calc_func_commit: String,
    harness_sha256: String,
}

impl Versions {
    fn current() -> Result<Self, String> {
        let calc_func_commit = command(Path::new("."), "git", &["rev-parse", "HEAD"])?;
        let executable = std::env::current_exe().map_err(|e| e.to_string())?;
        let bytes = fs::read(&executable).map_err(|e| format!("{}: {e}", executable.display()))?;
        Ok(Self {
            calc_func_commit,
            harness_sha256: calc_verdict::sha256_hex(&bytes),
        })
    }
}

fn read(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))
}

pub(super) fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    fs::write(&tmp, bytes)
        .and_then(|()| fs::rename(tmp, path))
        .map_err(|error| format!("{}: {error}", path.display()))
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    serde_json::from_str(&read(path)?).map_err(|error| format!("{}: {error}", path.display()))
}

fn numbered(dir: &Path) -> Result<Vec<(u64, PathBuf)>, String> {
    let paths = fs::read_dir(dir).map_err(|error| format!("{}: {error}", dir.display()))?;
    let mut rows = Vec::new();
    for entry in paths {
        let entry = entry.map_err(|e| e.to_string())?;
        if let Ok(number) = entry.file_name().to_string_lossy().parse::<u64>()
            && entry.file_type().map_err(|e| e.to_string())?.is_dir()
        {
            rows.push((number, entry.path()));
        }
    }
    rows.sort_by_key(|entry| entry.0);
    Ok(rows)
}

fn new_dir(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|error| format!("{}: {error}", path.display()))
}

fn next_recipe(
    sequence: u64,
    options: &Options,
    entry: &Entry,
    order_hash: &str,
    max_priority: u64,
) -> Recipe {
    let mut rng = super::rng::Rng::from_seed_and_tag(
        options.seed.wrapping_add(sequence - 1),
        &entry.function,
    );
    Recipe {
        function: entry.function.clone(),
        seed: rng.next_u64(),
        budget: entry.budget(max_priority),
        rank: entry.rank,
        bank_class: entry.class.clone(),
        order_sha256: order_hash.to_owned(),
    }
}

fn release_predictions(out: &Path, interrupted: bool) -> Result<(), String> {
    let mut paths = Vec::new();
    let mut hashes = BTreeMap::new();
    for file in fs::read_dir(out).map_err(|e| e.to_string())? {
        let file = file.map_err(|e| e.to_string())?;
        let name = file.file_name().to_string_lossy().into_owned();
        if !name.ends_with("-predictions.jsonl") {
            continue;
        }
        let path = file.path();
        if interrupted {
            let bytes = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            hashes.insert(name, calc_verdict::sha256_hex(&bytes));
        }
        paths.push(path);
    }
    if !hashes.is_empty() {
        write_json(&out.join("prediction-hashes.json"), &hashes)?;
    }
    for path in paths {
        fs::remove_file(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(())
}

fn attempt_config(recipe: &Recipe, out: &Path, endpoint: Option<PathBuf>) -> FuzzConfig {
    let mut cfg = FuzzConfig::default_for(&recipe.function);
    cfg.out_dir = out.to_owned();
    cfg.seed = recipe.seed;
    cfg.budget = recipe.budget;
    cfg.min_budget = cfg.budget;
    cfg.round_size = 128.min(cfg.budget);
    cfg.batch_size = 32;
    cfg.in_flight = 3;
    cfg.max_frontiers = 0;
    cfg.fractal_depth = 0;
    cfg.endpoint = endpoint;
    cfg
}

thread_local! {
    static ACTIVE_CASE: OnceCell<PathBuf> = const { OnceCell::new() };
}

pub(super) fn record_active_case(phase: &str, function: &str, args: &[Value], batch: &[&[Value]]) {
    ACTIVE_CASE.with(|path| {
        if let Some(path) = path.get()
            && let Err(error) = write_json(
                path,
                &json!({"phase": phase, "function": function, "args": args, "batch": batch}),
            )
        {
            eprintln!("sweep: cannot record active case: {error}");
        }
    });
}

fn execute_worker(out: &Path, bank: &Path, endpoint: Option<PathBuf>) -> Result<(), String> {
    let run = out.parent().ok_or("sweep attempt missing run directory")?;
    let sweep = run.parent().ok_or("sweep attempt missing sweep directory")?;
    let attempt = out
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.parse().ok())
        .ok_or("invalid sweep attempt number")?;
    let recipe: Recipe = read_json(&run.join("recipe.json"))?;
    let mut cfg = attempt_config(&recipe, out, endpoint);
    cfg.sweep = Some(super::oracle::SweepPolicy::for_attempt(
        sweep.join("PAUSE"),
        bank,
        &recipe.function,
        recipe.seed,
        attempt,
    )?);
    ACTIVE_CASE.with(|path| path.set(out.join("active-case.json")).unwrap());
    super::oracle::execute_live_run(&cfg)?;
    Ok(())
}

fn worker(out: &Path, bank: &Path, endpoint: Option<PathBuf>) -> ExitCode {
    match execute_worker(out, bank, endpoint) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            if let Err(failure) = write_json(&out.join("worker-error.json"), &error) {
                eprintln!("sweep worker: {error}; {failure}");
            }
            ExitCode::FAILURE
        }
    }
}

pub(super) fn run_worker(raw: &[String]) -> ExitCode {
    match raw {
        [out, bank, endpoint] => worker(Path::new(out), Path::new(bank), Some(endpoint.into())),
        [out, bank] => worker(Path::new(out), Path::new(bank), None),
        _ => ExitCode::from(2),
    }
}

fn worker_command(out: &Path, bank: &Path, endpoint: Option<&Path>) -> Result<Command, String> {
    let mut cmd = Command::new(std::env::current_exe().map_err(|e| e.to_string())?);
    #[cfg(not(test))]
    cmd.args(["fuzz", "sweep-worker"])
        .arg(out)
        .arg(bank)
        .args(endpoint);
    #[cfg(test)]
    cmd.args(["--exact", "fuzz::sweep::tests::worker_child", "--nocapture"])
        .env(
            "DNACALC_SWEEP_WORKER",
            serde_json::to_string(&(out, bank, endpoint)).map_err(|e| e.to_string())?,
        );
    Ok(cmd)
}

fn remove_worker_files(out: &Path) -> Result<(), String> {
    for name in [
        "active-case.json",
        "active-case.tmp",
        "worker-error.json",
        "worker-error.tmp",
        "worker.log",
    ] {
        let file = out.join(name);
        match fs::remove_file(&file) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("{}: {error}", file.display())),
        }
    }
    Ok(())
}

fn attempt_run(
    options: &Options,
    run: &Path,
    recipe: &Recipe,
    versions: &Versions,
    bank: &Path,
) -> Result<(), String> {
    let attempt = numbered(run)?.last().map_or(1, |(n, _)| n + 1);
    let out = run.join(format!("{attempt:08}"));
    new_dir(&out)?;
    let cfg = attempt_config(recipe, &out, options.endpoint.clone());
    write_json(
        &out.join("attempt.json"),
        &Attempt {
            recipe,
            generator_version: GENERATOR_VERSION,
            versions,
            started_utc: utc_timestamp_now(),
            function_timeout_seconds: options.function_timeout.as_secs(),
            batch_size: cfg.batch_size,
            in_flight: cfg.in_flight,
            round_size: cfg.round_size,
            min_budget: cfg.min_budget,
            max_frontiers: cfg.max_frontiers,
            fractal_depth: cfg.fractal_depth,
            frontier_window: cfg.frontier_window,
            dry_rounds: cfg.dry_rounds,
            lane: &cfg.lane,
        },
    )?;
    let mut cmd = worker_command(&out, bank, options.endpoint.as_deref())?;
    let outcome =
        crate::corpus_batch::bounded(&mut cmd, &out.join("worker.log"), options.function_timeout);
    let mut interruption = json!({"utc": utc_timestamp_now()});
    let error = match outcome {
        Ok(status) if status.success() => {
            let summary: Value = read_json(&out.join("summary.json"))?;
            release_predictions(&out, false)?;
            remove_worker_files(&out)?;
            write_json(
                &run.join("done.json"),
                &json!({
                    "attempt": attempt,
                    "finished_utc": utc_timestamp_now(),
                }),
            )?;
            println!(
                "sweep {} seed {}: {} matched / {}, {} mismatched, {} harness_fault, {} unread, {} crashed ({})",
                recipe.function,
                recipe.seed,
                summary["total_matched"],
                summary["total_asked"],
                summary["total_mismatched"],
                summary["total_harness_fault"],
                summary["total_unread"],
                summary["total_crashed"],
                summary["stop_reason"].as_str().unwrap_or_default(),
            );
            return Ok(());
        }
        Ok(status) => read_json(&out.join("worker-error.json")).unwrap_or_else(|_| {
            // Preserve diagnostics when a panic or crash prevented the worker writing its error.
            interruption["worker_log"] = json!(read(&out.join("worker.log")).unwrap_or_default());
            format!("sweep worker exited with {status}")
        }),
        Err(failure) if failure.timed_out => {
            interruption["harness_fault"] = json!("FunctionDeadline");
            interruption["limit_seconds"] = json!(options.function_timeout.as_secs());
            let active = out.join("active-case.json");
            if active.is_file() {
                match read_json::<Value>(&active) {
                    Ok(case) => interruption["case"] = case,
                    Err(case_err) => interruption["case_error"] = json!(case_err),
                }
            }
            interruption["worker"] = json!(failure.message);
            format!(
                "function deadline exceeded ({} seconds)",
                options.function_timeout.as_secs()
            )
        }
        Err(failure) => failure.message,
    };
    interruption["error"] = json!(error);
    write_json(&out.join("interrupted.json"), &interruption)?;
    release_predictions(&out, true)?;
    remove_worker_files(&out)?;
    Err(error)
}

fn command(directory: &Path, program: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new(program)
        .current_dir(directory)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|e| format!("{program}: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    } else {
        Err(format!(
            "{program} {}: {}",
            args.first().unwrap_or(&""),
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

fn complete_summary(run: &Path) -> Result<Value, String> {
    let marker: Value = read_json(&run.join("done.json"))?;
    let n = marker["attempt"]
        .as_u64()
        .ok_or("invalid sweep completion marker")?;
    read_json(&run.join(format!("{n:08}/summary.json")))
}

fn lock_file(path: &Path) -> Result<File, String> {
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    lock.try_lock()
        .map_err(|e| format!("lock {}: {e}", path.display()))?;
    Ok(lock)
}

fn tracker_commit(tracker: &Path, log: &str) -> Result<(), String> {
    let git = |args: &[&str]| command(tracker, "git", args);
    if read(&tracker.join(".beads/issues.jsonl"))?
        .lines()
        .any(|line| line.starts_with("<<<<<<<"))
    {
        return Err("tracker contains an unresolved merge conflict".into());
    }
    git(&["add", "--", ".beads/issues.jsonl"])?;
    git(&[
        "commit",
        "-m",
        "dc-ztvv.26: record new fuzz clusters",
        "--trailer",
        &format!("Agent-Log: {log}"),
        "-m",
        "Co-Authored-By: Codex GPT-6.2 Sol <noreply@openai.com>",
    ])?;
    Ok(())
}

fn pull_tracker(tracker: &Path, log: &str) -> Result<(), String> {
    let git = |args: &[&str]| command(tracker, "git", args);
    let pulled = git(&["pull", "--no-rebase", "--no-commit"]);
    if let Err(error) = pulled {
        if git(&["diff", "--name-only", "--diff-filter=U"])? != ".beads/issues.jsonl" {
            return Err(error);
        }
        // Same reconciliation as the keeper. The local DB already holds the unpublished rows.
        // Place the temporary incoming JSONL outside `.git/` so `br` safety invariant NGI-3 allows it.
        let incoming = std::env::temp_dir()
            .join(format!("dnacalc-fuzz-incoming-{}.jsonl", std::process::id()));
        let remote = git(&["show", "MERGE_HEAD:.beads/issues.jsonl"])?;
        fs::write(&incoming, format!("{remote}\n")).map_err(|e| e.to_string())?;
        let mut reconcile = Command::new("br");
        reconcile.current_dir(tracker).env("BEADS_JSONL", &incoming);
        if std::env::var_os("RUST_MIN_STACK").is_none() {
            reconcile.env("RUST_MIN_STACK", "67108864");
        }
        let result = reconcile
            .args([
                "sync",
                "--reconcile",
                "--allow-external-jsonl",
                "--no-auto-import",
                "--no-auto-flush",
            ])
            .output()
            .map_err(|e| e.to_string())?;
        if !result.status.success() {
            return Err(format!(
                "tracker reconciliation: {}",
                String::from_utf8_lossy(&result.stderr)
            ));
        }
        crate::check::br(
            tracker,
            &["sync", "--flush-only", "--force", "--no-auto-import"],
        )?;
        let _ = fs::remove_file(incoming);
    }
    if git(&["rev-parse", "--verify", "MERGE_HEAD"]).is_ok() {
        tracker_commit(tracker, log)?;
    }
    Ok(())
}

#[derive(Debug)]
struct ClusterIssue {
    title: String,
    priority: &'static str,
    description: String,
    external_ref: String,
}

fn new_cluster_issues(
    report: &Value,
    run: &Path,
    seen: &mut BTreeSet<String>,
) -> Result<Vec<ClusterIssue>, String> {
    let function = report["function"]
        .as_str()
        .ok_or("missing fuzz summary function")?;
    let clusters = report["clusters"]
        .as_array()
        .ok_or("missing fuzz clusters")?;
    let mut issues = Vec::new();
    for cluster in clusters {
        let key = cluster["key"].as_str().ok_or("missing fuzz cluster key")?;
        let kind = cluster["kind"].as_str().unwrap_or("cluster");
        let rep_calc = cluster["minimal_rep"]["calc_func"].as_str().unwrap_or("");
        // Defense in depth: DNA-side harness faults are coverage gaps, never Excel discrepancies.
        if super::cluster::is_dna_fault(rep_calc)
            || kind.contains("fault:")
            || key.contains("fault:")
        {
            continue;
        }
        let external_ref = format!(
            "dnacalc-fuzz:{}",
            calc_verdict::sha256_hex(format!("{function}\n{key}").as_bytes())
        );
        if !seen.insert(external_ref.clone()) {
            continue;
        }
        let priority = match cluster["worst_severity"].as_str() {
            Some("gross" | "structural") => "1",
            _ => "2",
        };
        issues.push(ClusterIssue {
            title: format!("Fuzz discrepancy: {function} {kind}"),
            priority,
            description: format!(
                "Cluster {key} in {} rows, seed {}. Sweep record {}.\n\nRepresentative: {}",
                cluster["count"],
                report["seed"],
                run.display(),
                cluster["minimal_rep"]
            ),
            external_ref,
        });
    }
    Ok(issues)
}

/// A pull that conflicted leaves the tracker clone mid-merge, and every later filing then fails on
/// "changes outside the pending batch" (dc-kze9w). Abort that merge; `pull_tracker` redoes the
/// reconciliation. Returns whether a merge was in progress.
fn abort_stalled_merge(tracker: &Path) -> Result<bool, String> {
    let merging = command(tracker, "git", &["rev-parse", "--verify", "-q", "MERGE_HEAD"]).is_ok();
    if merging {
        command(tracker, "git", &["merge", "--abort"])?;
    }
    Ok(merging)
}

const MAX_FILING_FAILURES: u32 = 2;

/// The visible alarm: set once filing has failed twice in a row, cleared by the owner deleting it.
fn stalled(out: &Path) -> Option<String> {
    fs::read_to_string(out.join("STALLED")).ok()
}

fn note_filing_failure(out: &Path, error: &str) {
    let counter = out.join("filing-failures");
    let failures = fs::read_to_string(&counter)
        .ok()
        .and_then(|text| text.trim().parse::<u32>().ok())
        .unwrap_or(0)
        + 1;
    let _ = fs::write(&counter, failures.to_string());
    if failures >= MAX_FILING_FAILURES {
        let _ = fs::write(
            out.join("STALLED"),
            format!(
                "fuzz sweep tracker filing failed {failures} times in a row; last error: {error}\n\
                 Fix the tracker clone, then delete this file and filing-failures.\n"
            ),
        );
    }
}

fn note_filing_success(out: &Path) {
    let _ = fs::remove_file(out.join("filing-failures"));
}

fn push_tracker(tracker: &Path, log: &str) -> Result<(), String> {
    if command(tracker, "git", &["push"]).is_ok() {
        return Ok(());
    }
    // Rejected: someone pushed first. Pull (with the tracker reconciliation) and push once more.
    pull_tracker(tracker, log)?;
    command(tracker, "git", &["push"]).map(|_| ())
}

fn file_clusters(tracker: &Path, pending: &[PathBuf]) -> Result<(), String> {
    if pending.is_empty() {
        return Ok(());
    }
    let reports = pending
        .iter()
        .map(|run| complete_summary(run).map(|report| (run, report)))
        .collect::<Result<Vec<_>, _>>()?;
    if reports
        .iter()
        .any(|(_, report)| report["cluster_count"].as_u64().unwrap_or(0) > 0)
    {
        let tracker =
            &fs::canonicalize(tracker).map_err(|e| format!("{}: {e}", tracker.display()))?;
        let _lock = lock_file(&tracker.join(".git/fuzz-cluster.lock"))?;
        let git = |args: &[&str]| command(tracker, "git", args);
        let br = |args: &[&str]| crate::check::br(tracker, args);
        if git(&["branch", "--show-current"])? != "master" {
            return Err("cluster filing requires a dedicated master clone".into());
        }
        let log = std::env::var("DNACALC_AGENT_LOG")
            .map_err(|_| "cluster filing requires DNACALC_AGENT_LOG")?;
        abort_stalled_merge(tracker)?;
        let dirty = git(&["status", "--porcelain", "--untracked-files=no"])?;
        let marker = tracker.join(".git/fuzz-cluster-pending");
        if !dirty.is_empty() {
            if !marker.exists()
                || dirty
                    .lines()
                    .any(|line| line.get(3..) != Some(".beads/issues.jsonl"))
            {
                return Err(
                    "tracker clone has changes outside the scheduler's pending batch".into(),
                );
            }
            if git(&["diff", "--name-only", "--diff-filter=U"])?.is_empty() {
                tracker_commit(tracker, &log)?;
            }
        }
        pull_tracker(tracker, &log)?;
        br(&["sync", "--import-only"])?;
        fs::write(&marker, "fuzz sweep tracker transaction\n").map_err(|e| e.to_string())?;
        let listed: Value = serde_json::from_str(&br(&[
            "list",
            "--label",
            "fuzz-sweep-cluster",
            "--status",
            "all",
            "--json",
        ])?)
        .map_err(|e| e.to_string())?;
        let mut seen: BTreeSet<_> = listed["issues"]
            .as_array()
            .ok_or("missing br issues list")?
            .iter()
            .filter_map(|issue| issue["external_ref"].as_str().map(str::to_owned))
            .collect();
        for (run, report) in &reports {
            for issue in new_cluster_issues(report, run, &mut seen)? {
                br(&[
                    "create",
                    &issue.title,
                    "--type",
                    "bug",
                    "--priority",
                    issue.priority,
                    "--parent",
                    "dc-j9k",
                    "--labels",
                    "fuzz-sweep-cluster",
                    "--external-ref",
                    &issue.external_ref,
                    "--description",
                    &issue.description,
                    "--no-auto-flush",
                ])?;
            }
        }
        br(&["sync", "--flush-only"])?;
        if !git(&["status", "--porcelain", "--", ".beads/issues.jsonl"])?.is_empty() {
            tracker_commit(tracker, &log)?;
        }
        let outgoing = git(&["diff", "--name-only", "@{upstream}", "HEAD"])?;
        if !outgoing.is_empty() {
            if outgoing.lines().any(|line| line != ".beads/issues.jsonl") {
                return Err("tracker clone has unpublished changes outside the tracker".into());
            }
            command(tracker, "cargo", &["xtask", "check"])?;
            if git(&["grep", "-c", "^<<<<<<<", "--", ".beads"]).is_ok() {
                return Err("tracker contains an unresolved merge conflict".into());
            }
            push_tracker(tracker, &log)?;
        }
        fs::remove_file(marker).map_err(|e| e.to_string())?;
    }
    for (run, _) in reports {
        write_json(
            &run.join("filed.json"),
            &json!({"filed_utc": utc_timestamp_now()}),
        )?;
    }
    Ok(())
}

fn execute(options: &Options) -> Result<(), String> {
    let order = read(&options.order)?;
    let order_hash = calc_verdict::sha256_hex(order.as_bytes());
    let entries = parse_order(&order)?;
    let max_priority = entries.iter().map(|e| e.priority).max().unwrap_or(0);
    if options.plan {
        for e in ordered(&entries, &BTreeMap::new(), &BTreeMap::new(), 1) {
            println!(
                "{}\t{}\t{}\t{}",
                e.rank,
                e.function,
                e.class,
                e.budget(max_priority)
            );
        }
        return Ok(());
    }
    let versions = Versions::current()?;
    new_dir(&options.out)?;
    let _lock = lock_file(&options.out.join("lock"))?;
    let bank = options
        .bank
        .clone()
        .unwrap_or_else(|| options.out.join("bank-rows"));
    let pause = options.out.join("PAUSE");
    let mut ran = 0;
    let mut sequence = 1;
    let mut last = BTreeMap::new();
    let mut give_ups = BTreeMap::new();
    let mut backoff_until = BTreeMap::new();
    let mut pending = Vec::new();
    for (n, path) in numbered(&options.out)? {
        sequence = n;
        if path.join("done.json").exists() {
            let recipe: Recipe = read_json(&path.join("recipe.json"))?;
            last.insert(recipe.function.clone(), n);
            give_ups.remove(&recipe.function);
            backoff_until.remove(&recipe.function);
            if !path.join("filed.json").exists() {
                pending.push(path);
            }
            sequence += 1;
        } else if path.join("gave_up.json").exists() {
            let gave_up: GaveUp = read_json(&path.join("gave_up.json"))?;
            last.insert(gave_up.function.clone(), n);
            give_ups.insert(gave_up.function.clone(), gave_up.consecutive_give_ups);
            backoff_until.insert(gave_up.function, gave_up.retry_after_sequence);
            sequence += 1;
        }
    }
    loop {
        if let Some(tracker) = &options.tracker {
            if let Some(text) = stalled(&options.out) {
                return Err(format!("STALLED, delete {}/STALLED to resume: {text}", options.out.display()));
            }
            match file_clusters(tracker, &pending) {
                Ok(()) => {
                    note_filing_success(&options.out);
                    pending.clear();
                }
                Err(error) if options.runs.is_some() => {
                    note_filing_failure(&options.out, &error);
                    return Err(error);
                }
                Err(error) => eprintln!("fuzz sweep filing will retry: {error}"),
            }
        }
        if options.runs.is_some_and(|limit| ran >= limit) {
            return Ok(());
        }
        if pause.exists() {
            if options.runs.is_some() {
                return Ok(());
            }
            std::thread::sleep(RETRY_DELAY);
            continue;
        }
        let run = options.out.join(format!("{sequence:08}"));
        let recipe_path = run.join("recipe.json");
        let recipe = if recipe_path.exists() {
            let existing: Recipe = read_json(&recipe_path)?;
            let (prior_failed, last_att, last_err) = persistent_failures(&run)?;
            if prior_failed >= MAX_FAILED_ATTEMPTS {
                let err = last_err.unwrap_or_else(|| "repeated failed attempts".to_owned());
                record_give_up(
                    &run,
                    sequence,
                    &existing,
                    &versions,
                    prior_failed,
                    last_att,
                    &err,
                    &mut last,
                    &mut give_ups,
                    &mut backoff_until,
                )?;
                refresh_coverage(options)?;
                sequence += 1;
                ran += 1;
                continue;
            }
            existing
        } else {
            let entry = ordered(&entries, &last, &backoff_until, sequence)[0];
            if let Some(&until) = backoff_until.get(&entry.function)
                && until > sequence
            {
                // When all entries are deferred, the queue starts with the earliest retry.
                sequence = until;
                continue;
            }
            let recipe = next_recipe(sequence, options, entry, &order_hash, max_priority);
            new_dir(&run)?;
            write_json(&recipe_path, &recipe)?;
            recipe
        };
        match attempt_run(options, &run, &recipe, &versions, &bank) {
            Ok(()) => {
                last.insert(recipe.function.clone(), sequence);
                give_ups.remove(&recipe.function);
                backoff_until.remove(&recipe.function);
                sequence += 1;
                ran += 1;
                refresh_coverage(options)?;
                if options.tracker.is_some() {
                    pending.push(run);
                }
            }
            Err(error) => {
                if pause.exists() || is_transient_infra_error(&error) {
                    if options.runs.is_some() {
                        return Err(error);
                    }
                    eprintln!(
                        "sweep {} seed {} waiting to retry: {error}",
                        recipe.function, recipe.seed
                    );
                    std::thread::sleep(RETRY_DELAY);
                    continue;
                }
                let (failed_count, last_att, last_error) = persistent_failures(&run)?;
                if failed_count >= MAX_FAILED_ATTEMPTS {
                    record_give_up(
                        &run,
                        sequence,
                        &recipe,
                        &versions,
                        failed_count,
                        last_att,
                        last_error.as_deref().unwrap_or(&error),
                        &mut last,
                        &mut give_ups,
                        &mut backoff_until,
                    )?;
                    refresh_coverage(options)?;
                    sequence += 1;
                    ran += 1;
                    continue;
                }
                eprintln!(
                    "sweep {} seed {} attempt {}/{} failed, will retry: {error}",
                    recipe.function, recipe.seed, failed_count, MAX_FAILED_ATTEMPTS
                );
                if options.runs.is_none() {
                    std::thread::sleep(RETRY_DELAY);
                }
            }
        }
    }
}

pub(super) fn run(raw: &[String]) -> ExitCode {
    if raw == ["--help"] {
        println!("{USAGE}\nEach function attempt gets {} seconds by default.", DEFAULT_FUNCTION_TIMEOUT.as_secs());
        return ExitCode::SUCCESS;
    }
    match Options::parse(raw) {
        Err(error) => {
            eprintln!("fuzz sweep: {error}\n{USAGE}");
            ExitCode::from(2)
        }
        Ok(options) => match execute(&options) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("fuzz sweep: {error}");
                ExitCode::FAILURE
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_child() {
        if let Ok(args) = std::env::var("DNACALC_SWEEP_WORKER") {
            let (out, bank, endpoint): (PathBuf, PathBuf, Option<PathBuf>) =
                serde_json::from_str(&args).unwrap();
            if worker(&out, &bank, endpoint) != ExitCode::SUCCESS {
                std::process::exit(1);
            }
        }
    }

    #[test]
    fn sweep_priority_uses_usage_complexity_then_class_and_rotates_by_age() {
        let table = "rank\tfunction\tpriority_score\tbank_class\n1\tABS\t12\texact\n2\tINT\t12.0000\tgross\n3\tSUM\t4\tnumeric";
        let entries = parse_order(table).unwrap();
        let mut last = BTreeMap::new();
        let names = |last: &BTreeMap<String, u64>| {
            ordered(&entries, last, &BTreeMap::new(), 1)
                .into_iter()
                .map(|entry| entry.function.as_str())
                .collect::<Vec<_>>()
        };
        assert_eq!(names(&last), ["INT", "ABS", "SUM"]);
        assert_eq!(entries[0].budget(120_000), 512);
        assert_eq!(entries[2].budget(120_000), 213);
        last.insert("INT".into(), 1);
        last.insert("ABS".into(), 2);
        assert_eq!(names(&last), ["SUM", "INT", "ABS"]);
        last.insert("SUM".into(), 3);
        assert_eq!(names(&last), ["INT", "ABS", "SUM"]);

        let mut backoff = BTreeMap::new();
        backoff.insert("INT".into(), 6);
        let deferred: Vec<_> = ordered(&entries, &last, &backoff, 4)
            .into_iter()
            .map(|e| e.function.as_str())
            .collect();
        assert_eq!(deferred, ["ABS", "SUM", "INT"]);
        let eligible: Vec<_> = ordered(&entries, &last, &backoff, 6)
            .into_iter()
            .map(|e| e.function.as_str())
            .collect();
        assert_eq!(eligible, ["INT", "ABS", "SUM"]);
        assert_eq!(backoff_runs_for(1), 4);
        assert_eq!(backoff_runs_for(2), 8);
        assert_eq!(backoff_runs_for(3), 16);
    }

    #[test]
    fn sweep_cluster_identity_survives_seed_and_local_cluster_number_changes() {
        let mut seen = BTreeSet::new();
        let mut report = json!({"function": "ABS", "seed": 123, "clusters": [
            {"key": "number_vs_error|generic", "kind": "number_vs_error", "id": "C01", "worst_severity": "structural"}
        ]});
        let path = Path::new("00000001");
        let first = new_cluster_issues(&report, path, &mut seen).unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].priority, "1");
        report["seed"] = json!(456);
        report["clusters"][0]["id"] = json!("C17");
        assert!(
            new_cluster_issues(&report, path, &mut seen)
                .unwrap()
                .is_empty()
        );
        report["function"] = json!("INT");
        assert_eq!(
            new_cluster_issues(&report, path, &mut seen).unwrap().len(),
            1
        );
    }

    #[test]
    fn sweep_cluster_issues_filter_dna_side_harness_faults() {
        let mut seen = BTreeSet::new();
        let report = json!({
            "function": "INDEX",
            "seed": 42,
            "clusters": [
                {
                    "key": "error:SPILL!_vs_fault:ResultShape|arity=2",
                    "kind": "error:SPILL!_vs_fault:ResultShape",
                    "id": "C01",
                    "count": 50,
                    "worst_severity": "structural",
                    "minimal_rep": {"excel": "error:SPILL!", "calc_func": "fault:ResultShape"}
                },
                {
                    "key": "number_vs_error:Value|arity=2",
                    "kind": "number_vs_error:Value",
                    "id": "C02",
                    "count": 4,
                    "worst_severity": "structural",
                    "minimal_rep": {"excel": "0x0000000000000000", "calc_func": "error:Value"}
                }
            ]
        });
        let issues = new_cluster_issues(&report, Path::new("00000009"), &mut seen).unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].title, "Fuzz discrepancy: INDEX number_vs_error:Value");
    }

    #[test]
    fn sweep_finalizes_preexisting_over_retried_run_without_new_attempt() {
        let tmp = std::env::temp_dir().join(format!(
            "dnacalc-fuzz-prefailed-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&tmp);
        let out = tmp.join("state");
        let run86 = out.join("00000086");
        for attempt in 1..=5u64 {
            let dir = run86.join(format!("{attempt:08}"));
            fs::create_dir_all(&dir).unwrap();
            write_json(
                &dir.join("interrupted.json"),
                &json!({
                    "utc": "2026-11-24T19:00:00Z",
                    "error": "touchstone request failed: excel-error: OutOfMemoryException [formulas small groups]"
                }),
            )
            .unwrap();
        }
        write_json(
            &run86.join("recipe.json"),
            &Recipe {
                function: "GETPIVOTDATA".to_owned(),
                seed: 17168990473040267555,
                budget: 266,
                rank: 86,
                bank_class: "-".to_owned(),
                order_sha256: "abc".to_owned(),
            },
        )
        .unwrap();
        let order = tmp.join("order.tsv");
        fs::write(
            &order,
            "rank\tfunction\tpriority_score\tbank_class\n86\tGETPIVOTDATA\t10.0\t-\n",
        )
        .unwrap();
        let endpoint = tmp.join("endpoint.json");
        fs::write(&endpoint, json!({"host": "127.0.0.1", "port": 1}).to_string()).unwrap();

        let args = [
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
        assert_eq!(run(&args), ExitCode::SUCCESS);
        assert!(run86.join("gave_up.json").is_file());
        assert!(
            !run86.join("00000006").exists(),
            "over-retried run must be finalized immediately without launching attempt 6"
        );
        let gave_up: GaveUp = read_json(&run86.join("gave_up.json")).unwrap();
        assert_eq!(gave_up.attempts, 5);
        assert_eq!(gave_up.last_attempt, 5);
        assert!(gave_up.reason.contains("OutOfMemoryException"));
        let _ = fs::remove_dir_all(&tmp);
    }

    fn single_function_sweep(name: &str) -> (PathBuf, PathBuf, [String; 6]) {
        let tmp = std::env::temp_dir().join(format!("dnacalc-fuzz-{name}-{}", std::process::id()));
        let out = tmp.join("state");
        let first = out.join("00000001");
        new_dir(&first).unwrap();
        write_json(
            &first.join("recipe.json"),
            &Recipe {
                function: "GETPIVOTDATA".into(),
                seed: 123,
                budget: 64,
                rank: 1,
                bank_class: "-".into(),
                order_sha256: "abc".into(),
            },
        )
        .unwrap();
        let order = tmp.join("order.tsv");
        fs::write(
            &order,
            "rank\tfunction\tpriority_score\tbank_class\n1\tGETPIVOTDATA\t10.0\t-\n",
        )
        .unwrap();
        let args = [
            "--runs",
            "1",
            "--order",
            order.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
        ]
        .map(str::to_owned);
        (tmp, first, args)
    }

    #[test]
    fn sweep_resumes_same_recipe_after_three_transient_failures() {
        let (tmp, first, args) = single_function_sweep("transient-resume");
        for (index, error) in ["Connection refused", "timed out", "worker-down"]
            .into_iter()
            .enumerate()
        {
            let attempt = first.join(format!("{:08}", index + 1));
            new_dir(&attempt).unwrap();
            write_json(&attempt.join("interrupted.json"), &json!({"error": error})).unwrap();
        }

        assert_eq!(run(&args), ExitCode::SUCCESS);
        assert!(!first.join("gave_up.json").exists());
        let done: Value = read_json(&first.join("done.json")).unwrap();
        assert_eq!(done["attempt"], 4);
        let _ = fs::remove_dir_all(tmp);
    }

    #[test]
    fn sweep_advances_to_first_eligible_sequence_when_all_entries_are_backed_off() {
        let (tmp, first, args) = single_function_sweep("all-backed-off");
        write_json(
            &first.join("gave_up.json"),
            &GaveUp {
                function: "GETPIVOTDATA".into(),
                attempts: 3,
                last_attempt: 3,
                finished_utc: utc_timestamp_now(),
                reason: "gave-up:3-attempts: excel-error".into(),
                error: "excel-error".into(),
                consecutive_give_ups: 1,
                backoff_runs: 4,
                retry_after_sequence: 5,
                calc_func_commit: "abc".into(),
            },
        )
        .unwrap();

        assert_eq!(run(&args), ExitCode::SUCCESS);
        let out = first.parent().unwrap();
        assert!(out.join("00000005/done.json").is_file());
        assert_eq!(numbered(out).unwrap().len(), 2);
        let _ = fs::remove_dir_all(tmp);
    }

    #[test]
    fn sweep_treats_isolation_stalled_as_transient_infra_error() {
        let msg = "touchstone request failed: isolation-stalled: the isolated queue stalled; its worker was restarted, resubmit the call";
        assert!(
            is_transient_infra_error(msg),
            "isolation-stalled must be treated as a transient infrastructure error"
        );
        let tmp = std::env::temp_dir().join(format!(
            "dnacalc-fuzz-isolation-stalled-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&tmp);
        let run = tmp.join("00000001");
        for attempt in 1..=3u64 {
            let dir = run.join(format!("{attempt:08}"));
            fs::create_dir_all(&dir).unwrap();
            write_json(&dir.join("interrupted.json"), &json!({"error": msg})).unwrap();
        }
        let (count, _, _) = persistent_failures(&run).unwrap();
        assert_eq!(count, 0, "isolation-stalled must not burn persistent_failures");
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn record_active_case_does_not_panic_on_missing_directory() {
        let missing = std::env::temp_dir()
            .join(format!("dnacalc-missing-active-case-{}", std::process::id()))
            .join("does-not-exist")
            .join("active-case.json");
        let handle = std::thread::spawn(move || {
            ACTIVE_CASE.with(|cell| {
                let _ = cell.set(missing);
            });
            record_active_case("dna", "ABS", &[json!("0x3ff0000000000000")], &[]);
        });
        assert!(
            handle.join().is_ok(),
            "record_active_case must not panic when writing active-case.json fails"
        );
    }

    fn git_in(dir: &Path, args: &[&str]) -> String {
        command(dir, "git", args).unwrap()
    }

    /// Native-only: drives git in a temporary repository.
    #[test]
    fn dc_kze9w_abort_stalled_merge_clears_a_conflicted_pull() {
        let dir = std::env::temp_dir().join(format!("dnacalc-kze9w-merge-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        git_in(&dir, &["init", "-q", "-b", "master"]);
        git_in(&dir, &["config", "user.email", "t@example.invalid"]);
        git_in(&dir, &["config", "user.name", "t"]);
        fs::write(dir.join("f"), "base\n").unwrap();
        git_in(&dir, &["add", "f"]);
        git_in(&dir, &["commit", "-q", "-m", "base"]);
        git_in(&dir, &["checkout", "-q", "-b", "other"]);
        fs::write(dir.join("f"), "theirs\n").unwrap();
        git_in(&dir, &["commit", "-q", "-am", "theirs"]);
        git_in(&dir, &["checkout", "-q", "master"]);
        fs::write(dir.join("f"), "ours\n").unwrap();
        git_in(&dir, &["commit", "-q", "-am", "ours"]);
        assert!(command(&dir, "git", &["merge", "other"]).is_err());
        assert!(git_in(&dir, &["status", "--porcelain"]).contains("UU"));
        assert!(abort_stalled_merge(&dir).unwrap());
        assert_eq!(git_in(&dir, &["status", "--porcelain"]), "");
        assert!(!abort_stalled_merge(&dir).unwrap());
        let _ = fs::remove_dir_all(&dir);
    }

    /// Native-only: uses the temporary directory.
    #[test]
    fn dc_kze9w_second_consecutive_filing_failure_writes_stalled() {
        let dir = std::env::temp_dir().join(format!("dnacalc-kze9w-stall-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        note_filing_failure(&dir, "first");
        assert!(!dir.join("STALLED").exists());
        note_filing_failure(&dir, "second");
        let text = fs::read_to_string(dir.join("STALLED")).unwrap();
        assert!(text.contains("second"));
        assert!(stalled(&dir).is_some());
        note_filing_success(&dir);
        assert!(!dir.join("filing-failures").exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
