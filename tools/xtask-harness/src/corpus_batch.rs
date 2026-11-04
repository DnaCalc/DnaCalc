//! Bounded comparison processes and content-addressed completion records for corpus.rs.
//! Reuse requires the same workbook, capture, executable and output bytes.
use calc_verdict::sha256_hex;
use serde_json::{Value, json};
use std::{
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    time::{Duration, Instant},
};

/// Hash folder inputs without allocating their whole contents in the parent.
/// Parsing and evaluation stay in the owned, timed child process.
pub(crate) fn file_digest(path: &Path) -> io::Result<String> {
    reader_digest(fs::File::open(path)?)
}

fn reader_digest(mut input: impl Read) -> io::Result<String> {
    let mut hash = calc_verdict::Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let size = match input.read(&mut buffer) {
            Ok(0) => return Ok(hash.finish_hex()),
            Ok(size) => size,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        hash.update(&buffer[..size]);
    }
}

/// Keep package inflation, model admission and source census inside the same
/// owned-process timeout as calculation. The parent never opens an untrusted
/// folder workbook through CalcHost merely to discover its settings.
pub(crate) fn census(
    program: &Path,
    fixture: &Path,
    snapshot: &str,
    out: &Path,
    executable: &str,
    resume: bool,
    timeout: Duration,
) -> Result<calc_verdict::CaptureRecord, String> {
    let out = out.join("inventory");
    fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let key = json!({"fixture":snapshot,"executable":executable,"operation":"source-inventory/1"});
    let record_path = out.join("inventory.record.json");
    let marker = out.join("complete.json");
    let decode = |bytes: &[u8]| -> Result<calc_verdict::CaptureRecord, String> {
        let record = calc_verdict::CaptureRecord::decode(bytes).map_err(|e| e.to_string())?;
        if record.source != format!("sha256:{snapshot}")
            || record.acquisition != calc_verdict::Acquisition::FileCacheReplay
            || !record.observations.is_empty()
        {
            return Err(
                "source inventory does not identify these bytes without observations".into(),
            );
        }
        Ok(record)
    };
    if resume
        && let Ok(bytes) = fs::read(&record_path)
        && let Ok(held) = fs::read(&marker).and_then(|bytes| {
            serde_json::from_slice::<Value>(&bytes).map_err(std::io::Error::other)
        })
        && held["key"] == key
        && held["record_sha256"] == sha256_hex(&bytes)
    {
        return decode(&bytes);
    }
    if timeout.is_zero() {
        return Err("workbook timeout before source inventory".into());
    }
    let log = attempt_log(&out)?;
    let payload = log.with_extension("");
    fs::create_dir(&payload).map_err(|e| e.to_string())?;
    let prior_failure = out.join("inventory-failure.json");
    if prior_failure.exists() {
        fs::remove_file(&prior_failure).map_err(|e| e.to_string())?;
    }
    let mut command = Command::new(program); // ubs:ignore -- compare_program supplies our own executable, never a corpus-supplied program.
    command
        .arg("compare")
        .arg("--fixture")
        .arg(fixture)
        .arg("--inventory-only")
        .arg("--out")
        .arg(&payload);
    let result = bounded(&mut command, &log, timeout);
    let outcome = match &result {
        Ok(status) => status.to_string(),
        Err(reason) => reason.clone(),
    };
    fs::write(
        log.with_extension("json"),
        serde_json::to_vec_pretty(
            &json!({"key":key,"outcome":outcome,"finished_utc":crate::accept::utc_now()}),
        )
        .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let failure = fs::read(payload.join("inventory-failure.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .filter(|failure| failure["source"] == format!("sha256:{snapshot}"));
    if let Some(failure) = &failure {
        fs::write(
            &prior_failure,
            serde_json::to_vec_pretty(failure).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
    }
    let status = result?;
    if !status.success() {
        return Err(format!(
            "source inventory ended with {status}: {}; log {}",
            failure
                .as_ref()
                .and_then(|f| f["message"].as_str())
                .unwrap_or("see attempt log"),
            log.display()
        ));
    }
    let bytes = fs::read(payload.join("inventory.record.json"))
        .map_err(|e| format!("source inventory missing: {e}"))?;
    let record = decode(&bytes)?;
    fs::write(&record_path, &bytes).map_err(|e| e.to_string())?;
    let partial = out.join("complete.partial.json");
    fs::write(
        &partial,
        serde_json::to_vec_pretty(&json!({"key":key,"record_sha256":sha256_hex(&bytes)}))
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    if marker.exists() {
        fs::remove_file(&marker).map_err(|e| e.to_string())?;
    }
    fs::rename(partial, marker).map_err(|e| e.to_string())?;
    Ok(record)
}

pub(crate) fn select(
    mut entries: Vec<super::corpus::Entry>,
    shard: Option<(u64, u64)>,
) -> (Vec<super::corpus::Entry>, Value) {
    // Prefer committed oracle captures when a folder also contains those bytes.
    entries.sort_by(|a, b| (a.oracle.is_none(), &a.name).cmp(&(b.oracle.is_none(), &b.name)));
    let mut seen = std::collections::BTreeMap::<String, String>::new();
    let mut selected = Vec::new();
    let mut inventory = Vec::new();
    for mut entry in entries {
        let read = file_digest(&entry.path);
        let digest = read.as_ref().ok().cloned();
        entry.batch_sha256 = digest.clone();
        let duplicate = digest.as_ref().and_then(|hash| seen.get(hash)).cloned();
        let shard_key = digest
            .clone()
            .unwrap_or_else(|| sha256_hex(entry.name.as_bytes()));
        let in_shard = shard.is_none_or(|(index, count)| shard_of(&shard_key, count) == index);
        let chosen = duplicate.is_none() && in_shard;
        inventory.push(
            json!({"name":entry.name, "path":entry.path, "sha256":digest,
            "duplicate_of":duplicate, "selected":chosen,
            "read_error":read.err().map(|e|e.to_string())}),
        );
        if let Some(digest) = digest {
            seen.entry(digest).or_insert_with(|| entry.name.clone());
        }
        if chosen {
            selected.push(entry);
        }
    }
    selected.sort_by(|a, b| a.name.cmp(&b.name));
    let keys:Vec<_>=inventory.iter().map(|row|json!({"name":row["name"],"sha256":row["sha256"],"duplicate_of":row["duplicate_of"]})).collect();
    let content_key = sha256_hex(&serde_json::to_vec(&keys).expect("inventory keys serialize"));
    let record = json!({"format":"dnacalc-corpus-inventory", "version":1,"content_key":content_key,
        "rule":"one representative per content hash; zero-based shard from first 64 SHA-256 bits",
        "shard":shard, "files":inventory, "unique_content":seen.len(),"selected":selected.len()});
    (selected, record)
}

/// A short, explicitly inferred problem list over the existing root-cause groups.
pub(crate) fn write_problem_functions(out: &Path, summary: &Value) -> Result<(), String> {
    let mut functions = std::collections::BTreeMap::<String, (u64, u64, u64)>::new();
    for cause in summary["root_causes"]["mismatches"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let name = cause["function"].as_str().unwrap_or("unknown").to_owned();
        let entry = functions.entry(name).or_default();
        entry.0 += cause["roots"].as_u64().unwrap_or(0);
        entry.1 += cause["downstream"].as_u64().unwrap_or(0);
        let shape = cause["shape"].as_str().unwrap_or("");
        if shape.contains("ulp apart") || shape.contains("equal to 15 significant digits") {
            entry.2 += cause["cells"].as_u64().unwrap_or(0);
        }
    }
    let mut functions: Vec<_> = functions.into_iter().collect();
    functions.sort_by(|a, b| {
        (b.1.0 + b.1.1)
            .cmp(&(a.1.0 + a.1.1))
            .then_with(|| a.0.cmp(&b.0))
    });
    let rows:Vec<_>=functions.into_iter().map(|(function,(roots,downstream,bit_level_cells))|json!({"function":function,"roots":roots,"downstream":downstream,"bit_level_cells":bit_level_cells})).collect();
    let record = json!({"format":"dnacalc-problem-functions","version":1,
        "attribution":"inferred from the last nonsynthetic executed call and mismatching reads; formula operators/context can also cause the difference; observed read types exclude literal argument types",
        "functions":rows});
    fs::write(
        out.join("problem-functions.json"),
        serde_json::to_vec_pretty(&record).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

fn sum_numbers(total: &mut Value, part: &Value) -> Result<(), String> {
    let object = part.as_object().ok_or("summary counts are not an object")?;
    for (key, value) in object {
        if key == "pass_rate" {
            continue;
        }
        if let Some(number) = value.as_u64() {
            let held = total[key].as_u64().unwrap_or(0);
            total[key] = json!(held.checked_add(number).ok_or("summary count overflow")?);
        } else if value.is_object() {
            if !total[key].is_object() {
                total[key] = json!({});
            }
            sum_numbers(&mut total[key], value)?;
        } else if !value.is_null() {
            return Err(format!("invalid summary count {key}"));
        }
    }
    Ok(())
}

/// Combine disjoint shard reports without re-running a workbook. A repeated hash
/// or a changed corpus/executable is refused, rather than counted a second time.
pub(crate) fn merge_summaries(paths: &[PathBuf], out: &Path) -> Result<(), String> {
    let mut reports = Vec::new();
    let mut identity = None;
    let mut seen = std::collections::BTreeSet::new();
    let mut shard_ids = std::collections::BTreeSet::new();
    let mut shard_count = None;
    let mut workbooks = Vec::new();
    let mut totals = json!({"workbooks":0,"compared":0,"not_compared":0,"whole_run_failed":0,"unknown_inventory_workbooks":0,"counts":{}});
    let mut causes = std::collections::BTreeMap::<(String, String), Value>::new();
    for path in paths {
        let report: Value = serde_json::from_slice(
            &fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?,
        )
        .map_err(|e| e.to_string())?;
        if report["format"] != "dnacalc-corpus-summary" || report["version"] != 2 {
            return Err("--merge requires version 2 corpus summaries".into());
        }
        let id = (
            report["run"]["corpus_inventory_sha256"]
                .as_str()
                .ok_or("summary has no corpus identity")?
                .to_owned(),
            report["run"]["executable_sha256"]
                .as_str()
                .ok_or("summary has no executable identity")?
                .to_owned(),
        );
        if let Some(held) = &identity {
            if held != &id {
                return Err("shard corpus or executable identities differ".into());
            }
        } else {
            identity = Some(id);
        }
        let shard = report["run"]["shard"]
            .as_array()
            .ok_or("merged input has no shard index/count")?;
        if shard.len() != 2 {
            return Err("invalid shard index/count".into());
        }
        let index = shard[0].as_u64().ok_or("invalid shard index")?;
        let count = shard[1].as_u64().ok_or("invalid shard count")?;
        if count == 0
            || index >= count
            || shard_count.is_some_and(|held| held != count)
            || !shard_ids.insert(index)
        {
            return Err("inconsistent or repeated shard index/count".into());
        }
        shard_count = Some(count);
        let cells = report["totals"]["counts"]["formula_cells"]
            .as_u64()
            .ok_or("missing formula count")?;
        let pass = report["totals"]["counts"]["pass"]
            .as_u64()
            .ok_or("missing pass count")?;
        let fail = report["totals"]["counts"]["fail"]
            .as_u64()
            .ok_or("missing fail count")?;
        if pass.checked_add(fail) != Some(cells) {
            return Err("shard failure denominator does not balance".into());
        }
        for workbook in report["workbooks"]
            .as_array()
            .ok_or("missing workbook reports")?
        {
            let key = workbook["sha256"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| format!("unknown:{}", workbook["name"]));
            if !seen.insert(key) {
                return Err("one workbook occurs in multiple shard summaries".into());
            }
            let mut workbook = workbook.clone();
            workbook["shard_summary"] = json!(path);
            workbooks.push(workbook);
        }
        let mut part = report["totals"].clone();
        part.as_object_mut()
            .ok_or("missing shard totals")?
            .remove("wall_time_s");
        sum_numbers(&mut totals, &part)?;
        for (kind, fields) in [
            ("refusals", ["owner", "reason"]),
            ("mismatches", ["function", "shape"]),
            ("other", ["class", "key"]),
            ("file_failures", ["class", "key"]),
        ] {
            let rows = if kind == "file_failures" {
                report["file_failures"].as_array()
            } else {
                Some(
                    report["root_causes"][kind]
                        .as_array()
                        .ok_or("missing root-cause groups")?,
                )
            };
            for row in rows.into_iter().flatten() {
                let key = serde_json::to_string(&[&row[fields[0]], &row[fields[1]]])
                    .map_err(|e| e.to_string())?;
                let held = causes.entry((kind.into(), key)).or_insert_with(|| {
                    let mut object = serde_json::Map::new();
                    for field in fields {
                        object.insert(field.into(), row[field].clone());
                    }
                    Value::Object(object)
                });
                for field in [
                    "roots",
                    "downstream",
                    "whole_workbook",
                    "cells",
                    "workbooks",
                ] {
                    let value = row[field].as_u64().ok_or("missing root-cause count")?;
                    held[field] = json!(
                        held[field]
                            .as_u64()
                            .unwrap_or(0)
                            .checked_add(value)
                            .ok_or("root-cause count overflow")?
                    );
                }
                if let Some(value) = row["unknown_inventories"].as_u64() {
                    held["unknown_inventories"] =
                        json!(held["unknown_inventories"].as_u64().unwrap_or(0) + value);
                }
            }
        }
        reports.push(json!(path));
    }
    let (inventory, executable) = identity.ok_or("no shard summaries")?;
    let count = shard_count.ok_or("no shard count")?;
    let missing: Vec<_> = (0..count).filter(|n| !shard_ids.contains(n)).collect();
    let cells = totals["counts"]["formula_cells"].as_u64().unwrap_or(0);
    let pass = totals["counts"]["pass"].as_u64().unwrap_or(0);
    totals["counts"]["pass_rate"] = if cells == 0 {
        Value::Null
    } else {
        json!(pass as f64 / cells as f64)
    };
    let mut root_causes = json!({"refusals":[],"mismatches":[],"other":[],"file_failures":[]});
    for ((kind, _), row) in causes {
        root_causes[&kind]
            .as_array_mut()
            .expect("initialized group")
            .push(row);
    }
    for kind in ["refusals", "mismatches", "other", "file_failures"] {
        root_causes[kind]
            .as_array_mut()
            .expect("initialized group")
            .sort_by_key(|row| std::cmp::Reverse(row["cells"].as_u64().unwrap_or(0)));
    }
    workbooks.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    let file_failures = root_causes
        .as_object_mut()
        .expect("group object")
        .remove("file_failures")
        .expect("file group");
    let summary = json!({"format":"dnacalc-corpus-summary","version":2,
        "rule":"pass requires a calculated value equal to live Excel; all other inventoried formulas fail; unknown inventories are explicit",
        "run":{"merged_summaries":reports,"finished_utc":crate::accept::utc_now(),"corpus_inventory_sha256":inventory,
            "executable_sha256":executable,"shards_complete":missing.is_empty(),"missing_shards":missing},
        "totals":totals,"workbooks":workbooks,"root_causes":root_causes,"file_failures":file_failures});
    fs::create_dir_all(out).map_err(|e| e.to_string())?;
    fs::write(
        out.join("summary.json"),
        serde_json::to_vec_pretty(&summary).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let markdown = format!(
        "# Corpus summary\n\nMerged {} shard reports; complete: {}. Missing shards: {}.\n\nFormula cells: {cells}; matched: {pass}; fail: {}; pass rate: {}. Unknown inventories: {}.\n\nFull per-file results and root causes are in summary.json; each workbook links its original shard summary.\n",
        paths.len(),
        summary["run"]["shards_complete"],
        summary["run"]["missing_shards"],
        totals["counts"]["fail"],
        super::corpus_summary::rate(pass, cells),
        totals["unknown_inventory_workbooks"]
    );
    fs::write(out.join("summary.md"), markdown).map_err(|e| e.to_string())?;
    write_problem_functions(out, &summary)?;
    println!(
        "xtask corpus: merged {} shards; {pass}/{cells} matched; fail {}; complete {}",
        paths.len(),
        totals["counts"]["fail"],
        summary["run"]["shards_complete"]
    );
    Ok(())
}

pub(crate) fn shard_of(digest: &str, count: u64) -> u64 {
    // SHA-256 hex supplied by the inventory, not a platform-dependent hasher.
    u64::from_str_radix(&digest[..16], 16).expect("SHA-256 prefix") % count
}

pub(crate) fn output_dir(root: &Path, name: &str, digest: &str) -> PathBuf {
    let name: String = super::corpus::file_safe(name).chars().take(96).collect();
    root.join("workbooks").join(format!("{name}-{digest}"))
}

pub(crate) fn key(
    fixture: &Path,
    capture: Option<&Path>,
    executable: &str,
    minimize: bool,
) -> Result<Value, String> {
    let digest =
        |path: &Path| file_digest(path).map_err(|e| format!("cannot hash {}: {e}", path.display()));
    Ok(json!({"format":"dnacalc-corpus-completion", "version":1,
        "fixture":digest(fixture)?, "capture":capture.map(digest).transpose()?,
        "executable":executable, "minimize":minimize}))
}

const OUTPUTS: &[&str] = &[
    "report.json",
    "evaluations.json",
    "triage.json",
    "linked-inputs.json",
    "source-caches.json",
    "summary.json",
];

pub(crate) fn completed(out: &Path, key: &Value) -> Option<Value> {
    let marker: Value = serde_json::from_slice(&fs::read(out.join("complete.json")).ok()?).ok()?;
    if marker.get("key")? != key {
        return None;
    }
    let payloads = marker.get("outputs")?.as_object()?;
    if !OUTPUTS.iter().all(|name| payloads.contains_key(*name)) {
        return None;
    }
    if key["capture"].is_null() && !payloads.contains_key("inventory.record.json") {
        return None;
    }
    for (name, hash) in payloads {
        // Completion files contain only relative file paths below this result directory.
        let path = Path::new(name);
        if path.is_absolute()
            || path
                .components()
                .any(|part| !matches!(part, std::path::Component::Normal(_)))
        {
            return None;
        }
        if file_digest(&out.join(path)).ok()? != hash.as_str()? {
            return None;
        }
    }
    let triage = serde_json::from_slice(&fs::read(out.join("triage.json")).ok()?).ok()?;
    super::corpus_summary::read_triage(&triage).ok()?;
    Some(triage)
}

fn output_hashes(
    out: &Path,
    dir: &Path,
    hashes: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
    for item in entries {
        let item = item.map_err(|e| e.to_string())?;
        let path = item.path();
        let ty = item.file_type().map_err(|e| e.to_string())?;
        if ty.is_dir() {
            output_hashes(out, &path, hashes)?;
        } else if ty.is_file() {
            let relative = path
                .strip_prefix(out)
                .expect("child output")
                .to_string_lossy()
                .replace('\\', "/");
            let digest = file_digest(&path).map_err(|e| e.to_string())?;
            hashes.insert(relative, json!(digest));
        }
    }
    Ok(())
}

pub(crate) fn finish(out: &Path, key: &Value) -> Result<(), String> {
    let mut hashes = serde_json::Map::new();
    for name in OUTPUTS {
        let digest = file_digest(&out.join(name)).map_err(|e| format!("missing {name}: {e}"))?;
        hashes.insert((*name).into(), json!(digest));
    }
    if key["capture"].is_null() {
        let digest = file_digest(&out.join("inventory.record.json")).map_err(|e| e.to_string())?;
        hashes.insert("inventory.record.json".into(), json!(digest));
    }
    if key["minimize"] == true {
        output_hashes(out, &out.join("cases"), &mut hashes)?;
    }
    let bytes = serde_json::to_vec_pretty(&json!({"key":key,"outputs":hashes}))
        .map_err(|e| e.to_string())?;
    let partial = out.join("complete.partial.json");
    fs::write(&partial, bytes)
        .and_then(|()| fs::rename(&partial, out.join("complete.json")))
        .map_err(|e| format!("cannot complete {}: {e}", out.display()))
}

/// Both streams go to files, so a full pipe cannot hide a timeout. Kill/reap only
/// this process; comparison runs no Excel or background process on this route.
pub(crate) fn bounded(
    command: &mut Command,
    log: &Path,
    timeout: Duration,
) -> Result<ExitStatus, String> {
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(log)
        .map_err(|e| format!("cannot create {}: {e}", log.display()))?;
    let err = file.try_clone().map_err(|e| e.to_string())?;
    let mut child = command
        .stdout(Stdio::from(file))
        .stderr(Stdio::from(err))
        .spawn()
        .map_err(|e| format!("cannot start comparison: {e}"))?;
    let begun = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) if begun.elapsed() < timeout => std::thread::sleep(Duration::from_millis(20)),
            state => {
                let killed = child.kill();
                let waited = child.wait();
                return Err(match state {
                    Err(e) => {
                        format!("cannot wait for comparison: {e}; kill={killed:?}; wait={waited:?}")
                    }
                    _ => format!(
                        "comparison timed out after {} seconds; kill={killed:?}; wait={waited:?}",
                        timeout.as_secs_f64()
                    ),
                });
            }
        }
    }
}

pub(crate) fn attempt_log(out: &Path) -> Result<PathBuf, String> {
    for n in 1u64.. {
        let path = out.join(format!("attempt-{n}.log"));
        if !path.exists() && !path.with_extension("").exists() {
            return Ok(path);
        }
    }
    Err("comparison attempt numbers exhausted".into())
}

/// Promote a completed attempt only after its fresh payload has been checked.
/// Partial attempts stay alongside their logs; old report files never prove a
/// new process produced an answer.
pub(crate) fn publish(attempt: &Path, out: &Path) -> Result<(), String> {
    for name in OUTPUTS {
        if !attempt.join(name).is_file() {
            return Err(format!("new comparison did not write {name}"));
        }
    }
    fn copy(source: &Path, destination: &Path) -> Result<(), String> {
        fs::create_dir_all(destination).map_err(|e| e.to_string())?;
        for entry in fs::read_dir(source).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            let target = destination.join(entry.file_name());
            if kind.is_dir() {
                copy(&entry.path(), &target)?;
            } else if kind.is_file() {
                fs::copy(entry.path(), target).map_err(|e| e.to_string())?;
            } else {
                return Err("comparison wrote an unsupported output entry".into());
            }
        }
        Ok(())
    }
    copy(attempt, out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn streamed_file_hashes_retry_interruptions_and_never_return_a_partial_digest() {
        struct Chunks {
            remaining: usize,
            calls: usize,
            fail: bool,
        }
        impl Read for Chunks {
            fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
                assert_eq!(buffer.len(), 64 * 1024, "bounded corpus hash buffer");
                self.calls += 1;
                if self.fail && self.calls == 4 {
                    return Err(io::Error::other("authored read failure"));
                }
                if self.calls.is_multiple_of(7) {
                    return Err(io::ErrorKind::Interrupted.into());
                }
                let n = self.remaining.min(257).min(buffer.len());
                buffer[..n].fill(b'a');
                self.remaining -= n;
                Ok(n)
            }
        }
        let expected = "9bc1b2a288b26af7257a36277ae3816a7d4f16e89c1e7e77d0a5c48bad62b360";
        assert_eq!(
            reader_digest(Chunks {
                remaining: 1048576,
                calls: 0,
                fail: false
            })
            .unwrap(),
            expected
        );
        let error = reader_digest(Chunks {
            remaining: 1048576,
            calls: 0,
            fail: true,
        })
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Other);
        let dir = scratch("streamed-hash");
        let path = dir.join("input.bin");
        {
            use std::io::Write;
            let mut file = fs::File::create(&path).unwrap();
            for _ in 0..256 {
                file.write_all(&[b'a'; 4096]).unwrap();
            }
        }
        assert_eq!(file_digest(&path).unwrap(), expected);
        assert_eq!(
            file_digest(&dir.join("missing.bin")).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
    }
    fn scratch(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("corpus-batch-{}-{name}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        path
    }
    #[test]
    fn old_outputs_cannot_complete_an_empty_new_attempt() {
        let out = scratch("fresh-output");
        let attempt = out.join("attempt-empty");
        fs::create_dir_all(&attempt).unwrap();
        for name in OUTPUTS {
            fs::write(out.join(name), b"old valid output").unwrap();
        }
        fs::write(attempt.join("triage.json"), br#"{"cells":[]}"#).unwrap();
        assert!(
            publish(&attempt, &out)
                .unwrap_err()
                .contains("did not write")
        );
        assert_eq!(
            fs::read(out.join("triage.json")).unwrap(),
            b"old valid output"
        );
        assert!(!out.join("complete.json").exists());
    }
    #[test]
    fn merged_shards_keep_failures_and_reject_overlap_or_changed_code() {
        let dir = scratch("merge");
        let out = dir.join("out");
        let fixture = |index: u64, hash: &str, binary: &str| {
            json!({
                "format":"dnacalc-corpus-summary","version":2,
                "run":{"shard":[index,2],"corpus_inventory_sha256":"one-corpus","executable_sha256":binary},
                "totals":{"workbooks":1,"compared":1,"not_compared":0,"whole_run_failed":0,"unknown_inventory_workbooks":0,
                    "counts":{"formula_cells":1,"pass":0,"fail":1,"failed_or_missing":1,"pass_rate":0.0}},
                "workbooks":[{"name":format!("{index}.xlsx"),"sha256":hash}],
                "root_causes":{"refusals":[],"mismatches":[],"other":[{"class":"capture","key":"unobserved","roots":1,"downstream":0,"whole_workbook":0,"cells":1,"workbooks":1}]},
            })
        };
        let a = dir.join("a.json");
        let b = dir.join("b.json");
        fs::write(
            &a,
            serde_json::to_vec(&fixture(0, "aaa", "code-a")).unwrap(),
        )
        .unwrap();
        fs::write(
            &b,
            serde_json::to_vec(&fixture(1, "bbb", "code-a")).unwrap(),
        )
        .unwrap();
        merge_summaries(std::slice::from_ref(&a), &out).unwrap();
        let read = || {
            serde_json::from_slice::<Value>(&fs::read(out.join("summary.json")).unwrap()).unwrap()
        };
        assert_eq!(read()["run"]["shards_complete"], false);
        merge_summaries(&[a.clone(), b.clone()], &out).unwrap();
        assert_eq!(read()["run"]["shards_complete"], true);
        assert_eq!(read()["totals"]["counts"]["fail"], 2);
        assert_eq!(read()["root_causes"]["other"][0]["cells"], 2);
        fs::write(
            &b,
            serde_json::to_vec(&fixture(1, "aaa", "code-a")).unwrap(),
        )
        .unwrap();
        assert!(
            merge_summaries(&[a.clone(), b.clone()], &out)
                .unwrap_err()
                .contains("multiple shard")
        );
        fs::write(
            &b,
            serde_json::to_vec(&fixture(1, "bbb", "code-b")).unwrap(),
        )
        .unwrap();
        assert!(
            merge_summaries(&[a, b], &out)
                .unwrap_err()
                .contains("identities differ")
        );
    }
    #[test]
    fn shards_partition_content_independently_of_file_order() {
        let mut shards = [0; 7];
        for n in 0..1000 {
            let hash = sha256_hex(n.to_string().as_bytes());
            let shard = shard_of(&hash, 7);
            shards[shard as usize] += 1;
            assert_eq!(shard, shard_of(&hash, 7));
        }
        assert_eq!(shards.iter().sum::<usize>(), 1000);
        assert!(shards.iter().all(|n| *n > 0));
        let hash = sha256_hex(b"same");
        assert_ne!(
            output_dir(Path::new("out"), "a b", &hash),
            output_dir(Path::new("out"), "a_b", &sha256_hex(b"different"))
        );
    }
    #[test]
    fn resume_rejects_changed_inputs_binary_options_and_partial_outputs() {
        let out = scratch("resume");
        let fixture = out.join("source.xlsx");
        let capture = out.join("capture.json");
        fs::write(&fixture, b"one").unwrap();
        fs::write(&capture, b"two").unwrap();
        let key = key(&fixture, Some(&capture), "binary-a", false).unwrap();
        for name in OUTPUTS {
            fs::write(out.join(name), b"{}").unwrap();
        }
        fs::write(out.join("triage.json"), br#"{"cells":[],"run":{}}"#).unwrap();
        finish(&out, &key).unwrap();
        assert!(completed(&out, &key).is_some());
        let mut changed = key.clone();
        changed["executable"] = json!("binary-b");
        assert!(completed(&out, &changed).is_none());
        changed = key.clone();
        changed["minimize"] = json!(true);
        assert!(completed(&out, &changed).is_none());
        fs::write(&capture, b"other").unwrap();
        assert!(
            completed(
                &out,
                &super::key(&fixture, Some(&capture), "binary-a", false).unwrap()
            )
            .is_none()
        );
        fs::write(out.join("report.json"), b"truncated").unwrap();
        assert!(completed(&out, &key).is_none());
    }
    #[test]
    fn child_fixture() {
        match std::env::var("DNACALC_CORPUS_CHILD").as_deref() {
            Ok("wait") => std::thread::sleep(Duration::from_secs(10)),
            Ok("fail") => std::process::exit(17),
            _ => {}
        }
    }
    #[test]
    fn timeout_and_crash_leave_logs_and_reap_only_the_owned_child() {
        let out = scratch("timeout");
        for (mode, timeout) in [
            ("wait", Duration::from_millis(100)),
            ("fail", Duration::from_secs(5)),
        ] {
            let mut child = Command::new(std::env::current_exe().unwrap());
            child
                .args([
                    "--exact",
                    "corpus_batch::tests::child_fixture",
                    "--nocapture",
                ])
                .env("DNACALC_CORPUS_CHILD", mode);
            let log = attempt_log(&out).unwrap();
            let started = Instant::now();
            let result = bounded(&mut child, &log, timeout);
            if mode == "wait" {
                assert!(result.unwrap_err().contains("timed out"));
            } else {
                assert_eq!(result.unwrap().code(), Some(17));
            }
            assert!(started.elapsed() < Duration::from_secs(6));
            assert!(log.is_file());
        }
        assert!(!out.join("complete.json").exists());
    }
}
