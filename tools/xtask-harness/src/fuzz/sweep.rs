//! The continuous, low-priority queue uses the ordinary live fuzz run for each recipe.

use super::{FuzzConfig, rng::GENERATOR_VERSION, utc_timestamp_now};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::Duration;

const USAGE: &str = "cargo xtask fuzz sweep [--plan] [--order <tsv>] [--out <dir>] [--runs <N>] [--seed <N>] [--endpoint <path>] [--bank-rows-out <dir>] [--tracker <dedicated master clone>]";
const RETRY_DELAY: Duration = Duration::from_secs(15);

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

fn ordered<'a>(entries: &'a [Entry], last: &BTreeMap<String, u64>) -> Vec<&'a Entry> {
    let mut queue: Vec<_> = entries.iter().collect();
    // Unvisited functions come first. Thereafter least recently completed leads.
    queue.sort_by_key(|e| {
        (
            last.get(&e.function).copied(),
            Reverse(e.priority),
            e.class_order(),
            e.rank,
        )
    });
    queue
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
    batch_size: usize,
    in_flight: usize,
    round_size: usize,
    min_budget: usize,
    max_frontiers: usize,
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
    let mut cfg = FuzzConfig::default_for(&recipe.function);
    cfg.out_dir = out.clone();
    cfg.seed = recipe.seed;
    cfg.budget = recipe.budget;
    cfg.min_budget = cfg.budget;
    cfg.round_size = 128.min(cfg.budget);
    cfg.batch_size = 32;
    cfg.in_flight = 3;
    cfg.max_frontiers = 0;
    cfg.endpoint = options.endpoint.clone();
    cfg.sweep = Some(super::oracle::SweepPolicy::new(
        options.out.join("PAUSE"),
        bank.join(format!("{}-{}-{attempt:08}", recipe.function, recipe.seed)),
    ));
    write_json(
        &out.join("attempt.json"),
        &Attempt {
            recipe,
            generator_version: GENERATOR_VERSION,
            versions,
            started_utc: utc_timestamp_now(),
            batch_size: cfg.batch_size,
            in_flight: cfg.in_flight,
            round_size: cfg.round_size,
            min_budget: cfg.min_budget,
            max_frontiers: cfg.max_frontiers,
            frontier_window: cfg.frontier_window,
            dry_rounds: cfg.dry_rounds,
            lane: &cfg.lane,
        },
    )?;
    match super::oracle::execute_live_run(&cfg) {
        Ok(summary) => {
            release_predictions(&out, false)?;
            write_json(
                &run.join("done.json"),
                &json!({
                    "attempt": attempt,
                    "finished_utc": utc_timestamp_now(),
                }),
            )?;
            println!(
                "sweep {} seed {}: {} matched / {}, {} mismatched, {} unread, {} crashed",
                summary.function,
                summary.seed,
                summary.total_matched,
                summary.total_asked,
                summary.total_mismatched,
                summary.total_unread,
                summary.total_crashed
            );
            Ok(())
        }
        Err(error) => {
            release_predictions(&out, true)?;
            write_json(
                &out.join("interrupted.json"),
                &json!({"utc": utc_timestamp_now(), "error": error}),
            )?;
            Err(error)
        }
    }
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
        let incoming = tracker.join(".git/fuzz-incoming.jsonl");
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
        let external_ref = format!(
            "dnacalc-fuzz:{}",
            calc_verdict::sha256_hex(format!("{function}\n{key}").as_bytes())
        );
        if !seen.insert(external_ref.clone()) {
            continue;
        }
        let kind = cluster["kind"].as_str().unwrap_or("cluster");
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
                    "dc-ztvv.26",
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
            git(&["push"])?;
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
        for e in ordered(&entries, &BTreeMap::new()) {
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
    let mut pending = Vec::new();
    for (n, path) in numbered(&options.out)? {
        sequence = n;
        if path.join("done.json").exists() {
            let recipe: Recipe = read_json(&path.join("recipe.json"))?;
            last.insert(recipe.function, n);
            if !path.join("filed.json").exists() {
                pending.push(path);
            }
            sequence += 1;
        }
    }
    loop {
        if let Some(tracker) = &options.tracker {
            match file_clusters(tracker, &pending) {
                Ok(()) => pending.clear(),
                Err(error) if options.runs.is_some() => return Err(error),
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
            read_json(&recipe_path)?
        } else {
            let entry = ordered(&entries, &last)[0];
            let recipe = next_recipe(sequence, options, entry, &order_hash, max_priority);
            new_dir(&run)?;
            write_json(&recipe_path, &recipe)?;
            recipe
        };
        match attempt_run(options, &run, &recipe, &versions, &bank) {
            Ok(()) => {
                last.insert(recipe.function, sequence);
                sequence += 1;
                ran += 1;
                if options.tracker.is_some() {
                    pending.push(run);
                }
            }
            Err(error) if options.runs.is_some() => return Err(error),
            Err(error) => {
                eprintln!(
                    "sweep {} seed {} will retry: {error}",
                    recipe.function, recipe.seed
                );
                std::thread::sleep(RETRY_DELAY);
            }
        }
    }
}

pub(super) fn run(raw: &[String]) -> ExitCode {
    if raw == ["--help"] {
        println!("{USAGE}");
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
    fn sweep_priority_uses_usage_complexity_then_class_and_rotates_by_age() {
        let table = "rank\tfunction\tpriority_score\tbank_class\n1\tABS\t12\texact\n2\tINT\t12.0000\tgross\n3\tSUM\t4\tnumeric";
        let entries = parse_order(table).unwrap();
        let mut last = BTreeMap::new();
        let names = |last: &BTreeMap<String, u64>| {
            ordered(&entries, last)
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
}
