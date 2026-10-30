//! Check (f): the scan for the two banned words of
//! `docs/plan/ULTRACODE_WORK_PROMPT_OPUS_5_5.md` section 0 over `docs/`, `AGENTS.md`,
//! `README.md`, the tracker text in `.beads/issues.jsonl` (titles, descriptions, notes, close
//! reasons and comments) and the commit messages reachable from `HEAD`, read through `git log`.
//!
//! `tools/xtask/word-scan.json` holds the words' forms and the fixed identifiers excluded by name
//! (acceptance case names of later beads, which are never renamed), each as the hex of its ASCII
//! bytes so that neither the configuration nor this package's tests and fixtures carry the words
//! themselves; and the counted baseline of occurrences recorded on 2026-10-14, owned by the
//! follow-up bead that rewords them. Every run reports each file, record and commit with an
//! occurrence or a baseline entry, and how many occurrences each excluded identifier masked; any
//! count above its baseline (zero when absent) fails.
//!
//! Matching is case-insensitive over runs of ASCII letters and digits: a single-word form matches
//! a whole run, and a two-word form matches two runs joined only by spaces, line breaks, hyphens or
//! underscores. An excluded identifier is removed only where it stands whole, so the same word
//! inside any other identifier still counts.

use crate::arch::CheckResult;
use crate::text;
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

pub const CONFIG_PATH: &str = "tools/xtask/word-scan.json";

/// What one scan reads, relative to the root it scans: every file under `docs`, each of `files`,
/// the tracker JSONL at `tracker`, and the commit messages of the root's git repository.
pub struct Inputs<'a> {
    pub docs: &'a str,
    pub files: &'a [&'a str],
    pub tracker: &'a str,
}

/// The repository's inputs.
pub const REPOSITORY_INPUTS: Inputs<'static> = Inputs {
    docs: "docs",
    files: &["AGENTS.md", "README.md"],
    tracker: ".beads/issues.jsonl",
};

const TRACKER_FIELDS: [&str; 4] = ["title", "description", "notes", "close_reason"];

#[derive(Default)]
pub struct Baseline {
    owner: String,
    recorded: String,
    files: BTreeMap<String, usize>,
    tracker: BTreeMap<String, usize>,
    commits: BTreeMap<String, usize>,
}

/// One fixed identifier excluded by name.
pub struct Excluded {
    identifier: String,
    hex: String,
    reason: String,
}

pub struct WordConfig {
    rule: String,
    forms: Vec<Vec<String>>,
    excluded: Vec<Excluded>,
    baseline: Baseline,
}

/// The ASCII text whose bytes `hex` spells in hexadecimal.
pub fn unhex(hex: &str) -> Result<String, String> {
    let digits = hex.as_bytes();
    if digits.is_empty() || !digits.len().is_multiple_of(2) {
        return Err(format!("{hex:?} is not an even number of hex digits"));
    }
    let mut bytes = Vec::with_capacity(digits.len() / 2);
    for pair in digits.chunks(2) {
        let pair = std::str::from_utf8(pair).map_err(|_| format!("{hex:?} is not hex"))?;
        bytes.push(u8::from_str_radix(pair, 16).map_err(|_| format!("{hex:?} is not hex"))?);
    }
    if !bytes.is_ascii() {
        return Err(format!("{hex:?} does not spell ASCII text"));
    }
    String::from_utf8(bytes).map_err(|_| format!("{hex:?} does not spell ASCII text"))
}

fn counts_map(value: &Value, what: &str) -> Result<BTreeMap<String, usize>, String> {
    value
        .as_object()
        .ok_or_else(|| format!("{what} is not an object"))?
        .iter()
        .map(|(key, count)| {
            count
                .as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .map(|n| (key.clone(), n))
                .ok_or_else(|| format!("{what} entry {key} is not a count"))
        })
        .collect()
}

impl Baseline {
    fn parse(value: &Value) -> Result<Baseline, String> {
        let text = |field: &str| {
            value[field]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| format!("the baseline has no {field}"))
        };
        Ok(Baseline {
            owner: text("owner")?,
            recorded: text("recorded")?,
            files: counts_map(&value["files"], "baseline files")?,
            tracker: counts_map(&value["tracker"], "baseline tracker")?,
            commits: counts_map(&value["commits"], "baseline commits")?,
        })
    }
}

/// The occurrences found in some text, and how many occurrences each excluded identifier masked.
pub struct Found {
    pub offsets: Vec<usize>,
    pub masked: Vec<usize>,
}

fn add_masked(total: &mut [usize], masked: &[usize]) {
    for (sum, count) in total.iter_mut().zip(masked) {
        *sum += count;
    }
}

impl WordConfig {
    pub fn parse(text: &str) -> Result<WordConfig, String> {
        let value: Value =
            serde_json::from_str(text).map_err(|e| format!("{CONFIG_PATH} is not JSON: {e}"))?;
        let mut forms = Vec::new();
        for term in value["terms"].as_array().ok_or("terms is not an array")? {
            for form in term["forms_hex"]
                .as_array()
                .ok_or("a term has no forms_hex list")?
            {
                let decoded = unhex(form.as_str().ok_or("a form is not a hex string")?)?;
                let words: Vec<String> = decoded
                    .split_whitespace()
                    .map(str::to_ascii_lowercase)
                    .collect();
                if words.is_empty()
                    || words
                        .iter()
                        .any(|w| !w.chars().all(|c| c.is_ascii_alphanumeric()))
                {
                    return Err(format!("form {form} does not spell ASCII words"));
                }
                forms.push(words);
            }
        }
        let mut excluded = Vec::new();
        for entry in value["excluded_identifiers"]
            .as_array()
            .ok_or("excluded_identifiers is not an array")?
        {
            let hex = entry["identifier_hex"]
                .as_str()
                .ok_or("an excluded identifier has no identifier_hex")?;
            let identifier = unhex(hex)?;
            let reason = entry["reason"]
                .as_str()
                .filter(|reason| !reason.is_empty())
                .ok_or_else(|| format!("excluded identifier {hex} gives no reason"))?;
            excluded.push(Excluded {
                identifier,
                hex: hex.to_owned(),
                reason: reason.to_owned(),
            });
        }
        Ok(WordConfig {
            rule: value["rule"].as_str().unwrap_or_default().to_owned(),
            forms,
            excluded,
            baseline: Baseline::parse(&value["baseline"])?,
        })
    }

    /// The byte offsets of every occurrence in `text`, in order, and the masked counts.
    pub fn occurrences(&self, text: &str) -> Found {
        let identifiers: Vec<String> = self
            .excluded
            .iter()
            .map(|excluded| excluded.identifier.clone())
            .collect();
        let (masked_text, masked) = text::mask_identifiers(text, &identifiers);
        let pieces = text::pieces(&masked_text, false);
        let mut offsets: Vec<usize> = self
            .forms
            .iter()
            .flat_map(|form| text::find_words(&masked_text, &pieces, form, false))
            .map(|(start, _)| start)
            .collect();
        offsets.sort_unstable();
        Found { offsets, masked }
    }

    /// The occurrences in each tracker record's text fields and comments, by record ID (records
    /// without any are left out), and the masked counts over all records.
    pub fn tracker_counts(
        &self,
        jsonl: &str,
        shown: &str,
    ) -> Result<(BTreeMap<String, usize>, Vec<usize>), String> {
        let mut counts = BTreeMap::new();
        let mut masked = vec![0; self.excluded.len()];
        for (index, line) in jsonl.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let record: Value = serde_json::from_str(line)
                .map_err(|e| format!("{shown} line {} is not JSON: {e}", index + 1))?;
            let id = record["id"]
                .as_str()
                .ok_or_else(|| format!("{shown} line {} has no id", index + 1))?;
            let comments = record["comments"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|comment| comment["text"].as_str());
            let mut count = 0;
            for text in TRACKER_FIELDS
                .iter()
                .filter_map(|field| record[*field].as_str())
                .chain(comments)
            {
                let found = self.occurrences(text);
                count += found.offsets.len();
                add_masked(&mut masked, &found.masked);
            }
            if count > 0 {
                counts.insert(id.to_owned(), count);
            }
        }
        Ok((counts, masked))
    }

    /// The occurrences in each commit message of a `git log --format=%H%x1f%B%x1e` listing, by
    /// commit (commits without any are left out), and the masked counts over all messages.
    pub fn commit_counts(&self, log: &str) -> (BTreeMap<String, usize>, Vec<usize>) {
        let mut counts = BTreeMap::new();
        let mut masked = vec![0; self.excluded.len()];
        for (hash, message) in log
            .split('\u{1e}')
            .filter_map(|record| record.trim_matches('\n').split_once('\u{1f}'))
        {
            let found = self.occurrences(message);
            add_masked(&mut masked, &found.masked);
            if !found.offsets.is_empty() {
                counts.insert(hash.trim().to_owned(), found.offsets.len());
            }
        }
        (counts, masked)
    }
}

/// What one run counted.
#[derive(Default)]
pub struct Counts {
    /// Each scanned file with occurrences, with the lines they are on.
    pub files: BTreeMap<String, Vec<usize>>,
    pub files_scanned: usize,
    pub tracker: BTreeMap<String, usize>,
    pub tracker_records: usize,
    pub commits: BTreeMap<String, usize>,
    pub commits_scanned: usize,
    /// How many occurrences each excluded identifier masked in the files, the tracker records and
    /// the commit messages.
    pub masked_files: Vec<usize>,
    pub masked_tracker: Vec<usize>,
    pub masked_commits: Vec<usize>,
}

fn files_under(dir: &Path, out: &mut Vec<std::path::PathBuf>) -> Result<(), String> {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .map_err(|e| format!("cannot list {}: {e}", dir.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            files_under(&path, out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}

/// Reads and counts every input under `root`; an input that cannot be read is a problem, because
/// a scan of nothing proves nothing. The commit messages come from `git log` in `root`.
pub fn scan(
    root: &Path,
    inputs: &Inputs,
    config: &WordConfig,
    problems: &mut Vec<String>,
) -> Counts {
    let excluded = config.excluded.len();
    let mut counts = Counts {
        masked_files: vec![0; excluded],
        masked_tracker: vec![0; excluded],
        masked_commits: vec![0; excluded],
        ..Counts::default()
    };
    let mut paths = Vec::new();
    if let Err(e) = files_under(&root.join(inputs.docs), &mut paths) {
        problems.push(e);
    }
    for name in inputs.files {
        paths.push(root.join(name));
    }
    for path in paths {
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        match fs::read(&path) {
            Err(e) => problems.push(format!("cannot read {rel}: {e}")),
            Ok(bytes) => {
                counts.files_scanned += 1;
                let content = String::from_utf8_lossy(&bytes);
                let found = config.occurrences(&content);
                add_masked(&mut counts.masked_files, &found.masked);
                let lines: Vec<usize> = found
                    .offsets
                    .into_iter()
                    .map(|offset| text::line_at(&content, offset))
                    .collect();
                if !lines.is_empty() {
                    counts.files.insert(rel, lines);
                }
            }
        }
    }
    match fs::read_to_string(root.join(inputs.tracker)) {
        Err(e) => problems.push(format!("cannot read {}: {e}", inputs.tracker)),
        Ok(jsonl) => {
            counts.tracker_records = jsonl.lines().filter(|l| !l.trim().is_empty()).count();
            match config.tracker_counts(&jsonl, inputs.tracker) {
                Ok((found, masked)) => {
                    counts.tracker = found;
                    counts.masked_tracker = masked;
                }
                Err(e) => problems.push(e),
            }
        }
    }
    match crate::cargo::git(root, &["log", "--format=%H%x1f%B%x1e", "HEAD"]) {
        Err(e) => problems.push(format!("cannot read the commit messages: {e}")),
        Ok(log) => {
            counts.commits_scanned = log.matches('\u{1e}').count();
            let (found, masked) = config.commit_counts(&log);
            counts.commits = found;
            counts.masked_commits = masked;
        }
    }
    counts
}

/// The findings (counts above their baseline) and the per-input report.
pub fn compare(counts: &Counts, baseline: &Baseline) -> (Vec<String>, Value) {
    let mut findings = Vec::new();
    let mut report = |kind: &str,
                      found: &BTreeMap<String, usize>,
                      base: &BTreeMap<String, usize>,
                      lines: Option<&BTreeMap<String, Vec<usize>>>| {
        let mut entries = Map::new();
        let keys: std::collections::BTreeSet<&String> = found.keys().chain(base.keys()).collect();
        for key in keys {
            let count = found.get(key).copied().unwrap_or(0);
            let allowed = base.get(key).copied().unwrap_or(0);
            if count > allowed {
                let at = lines
                    .and_then(|lines| lines.get(key))
                    .map(|lines| {
                        let shown: Vec<String> = lines.iter().map(usize::to_string).collect();
                        format!(" (lines {})", shown.join(", "))
                    })
                    .unwrap_or_default();
                findings.push(format!(
                    "{kind} {key}: {count} occurrences of the banned words, above its baseline of {allowed}{at}"
                ));
            }
            entries.insert(key.clone(), json!({ "count": count, "baseline": allowed }));
        }
        Value::Object(entries)
    };
    let file_counts: BTreeMap<String, usize> = counts
        .files
        .iter()
        .map(|(file, lines)| (file.clone(), lines.len()))
        .collect();
    let files = report("file", &file_counts, &baseline.files, Some(&counts.files));
    let tracker = report("tracker record", &counts.tracker, &baseline.tracker, None);
    let commits = report("commit", &counts.commits, &baseline.commits, None);
    let details = json!({
        "baseline_owner": baseline.owner,
        "baseline_recorded": baseline.recorded,
        "files": files,
        "tracker": tracker,
        "commits": commits,
    });
    (findings, details)
}

/// The excluded identifiers as the receipt records them: hex, reason and the occurrences each one
/// masked in each kind of input.
fn exclusions_report(config: &WordConfig, counts: &Counts) -> Value {
    let at = |list: &[usize], index: usize| list.get(index).copied().unwrap_or(0);
    Value::Array(
        config
            .excluded
            .iter()
            .enumerate()
            .map(|(index, excluded)| {
                json!({
                    "identifier_hex": excluded.hex,
                    "reason": excluded.reason,
                    "masked": {
                        "files": at(&counts.masked_files, index),
                        "tracker_records": at(&counts.masked_tracker, index),
                        "commit_messages": at(&counts.masked_commits, index),
                    },
                })
            })
            .collect(),
    )
}

pub fn check(root: &Path) -> CheckResult {
    let config = match fs::read_to_string(root.join(CONFIG_PATH))
        .map_err(|e| format!("cannot read {CONFIG_PATH}: {e}"))
        .and_then(|text| WordConfig::parse(&text))
    {
        Ok(config) => config,
        Err(e) => {
            return CheckResult::new(
                "word-scan",
                format!("not checked: {e}"),
                vec![e],
                Value::Null,
            );
        }
    };
    let mut findings = Vec::new();
    let counts = scan(root, &REPOSITORY_INPUTS, &config, &mut findings);
    let (above, mut details) = compare(&counts, &config.baseline);
    findings.extend(above);
    let total = |map: &BTreeMap<String, usize>| map.values().sum::<usize>();
    let file_total: usize = counts.files.values().map(Vec::len).sum();
    let masked_total: usize = counts
        .masked_files
        .iter()
        .chain(&counts.masked_tracker)
        .chain(&counts.masked_commits)
        .sum();
    let base = &config.baseline;
    let summary = format!(
        "{file_total} occurrences in {} of {} files, {} in {} of {} tracker records, {} in {} of {} commit messages; baseline {} / {} / {} owned by {} (recorded {}); {masked_total} occurrences masked by {} excluded identifiers",
        counts.files.len(),
        counts.files_scanned,
        total(&counts.tracker),
        counts.tracker.len(),
        counts.tracker_records,
        total(&counts.commits),
        counts.commits.len(),
        counts.commits_scanned,
        total(&base.files),
        total(&base.tracker),
        total(&base.commits),
        base.owner,
        base.recorded,
        config.excluded.len()
    );
    details["rule"] = json!(config.rule);
    details["forms"] = json!(config.forms.len());
    details["excluded_identifiers"] = exclusions_report(&config, &counts);
    details["scanned"] = json!({
        "files": counts.files_scanned,
        "tracker_records": counts.tracker_records,
        "commit_messages": counts.commits_scanned,
    });
    CheckResult::new("word-scan", summary, findings, details)
}

/// The commit new text is measured from: where `HEAD` meets `origin/master` (`HEAD` itself when
/// the repository has no `origin/master`), with how it was found.
pub fn new_text_base(root: &Path) -> (String, String) {
    match crate::cargo::git(root, &["merge-base", "HEAD", "origin/master"]) {
        Ok(base) => (
            base.trim().to_owned(),
            "where HEAD meets origin/master".to_owned(),
        ),
        Err(_) => (
            "HEAD".to_owned(),
            "HEAD (no origin/master: only uncommitted text is new)".to_owned(),
        ),
    }
}

/// Whether `rel` (with `/` separators) is one of the inputs the scan reads.
fn is_input(rel: &str, inputs: &Inputs) -> bool {
    rel.starts_with(&format!("{}/", inputs.docs))
        || inputs.files.contains(&rel)
        || rel == inputs.tracker
}

/// The per-commit word scan: only text that is new since [`new_text_base`] is read, so text that
/// is already pushed never fails it. New text is each line the working tree adds to the scan's
/// inputs (`docs/`, `AGENTS.md`, `README.md` and the tracker JSONL) against the base, every line
/// of an untracked file among them, and the message of each commit after the base. Any occurrence
/// of a banned word in new text is a finding; the baseline is not consulted, because it counts
/// pushed text.
pub fn check_new(root: &Path) -> CheckResult {
    const NAME: &str = "word-scan (new text)";
    let config = match fs::read_to_string(root.join(CONFIG_PATH))
        .map_err(|e| format!("cannot read {CONFIG_PATH}: {e}"))
        .and_then(|text| WordConfig::parse(&text))
    {
        Ok(config) => config,
        Err(e) => return CheckResult::new(NAME, format!("not checked: {e}"), vec![e], Value::Null),
    };
    let inputs = &REPOSITORY_INPUTS;
    let (base, how) = new_text_base(root);
    let mut findings = Vec::new();
    let (mut lines_read, mut files_read, mut commits_read) = (0usize, 0usize, 0usize);
    let check_text = |what: String, text: &str, findings: &mut Vec<String>| {
        let found = config.occurrences(text);
        if !found.offsets.is_empty() {
            findings.push(format!(
                "{what}: {} occurrences of the banned words in new text",
                found.offsets.len()
            ));
        }
    };
    let mut pathspecs = vec![inputs.docs, inputs.tracker];
    pathspecs.extend(inputs.files.iter().copied());
    let mut diff_args = vec!["diff", "--no-color", "--unified=0", base.as_str(), "--"];
    diff_args.extend(pathspecs.iter().copied());
    match crate::cargo::git(root, &diff_args) {
        Err(e) => findings.push(format!("cannot read the diff against {base}: {e}")),
        Ok(diff) => {
            let mut file = String::new();
            let mut added: BTreeMap<String, Vec<(usize, String)>> = BTreeMap::new();
            let mut line_no = 0usize;
            for line in diff.lines() {
                if let Some(path) = line.strip_prefix("+++ ") {
                    file = path.trim_start_matches("b/").to_owned();
                } else if line.starts_with("@@") {
                    // "@@ -a,b +c,d @@": the new side starts at line c.
                    line_no = line
                        .split_whitespace()
                        .find_map(|part| part.strip_prefix('+'))
                        .and_then(|part| part.split(',').next())
                        .and_then(|start| start.parse().ok())
                        .unwrap_or(0);
                } else if let Some(text) = line.strip_prefix('+') {
                    added
                        .entry(file.clone())
                        .or_default()
                        .push((line_no, text.to_owned()));
                    line_no += 1;
                }
            }
            for (file, lines) in &added {
                files_read += 1;
                for (at, text) in lines {
                    lines_read += 1;
                    check_text(format!("{file}:{at}"), text, &mut findings);
                }
            }
        }
    }
    match crate::cargo::git(root, &["ls-files", "--others", "--exclude-standard", "-z"]) {
        Err(e) => findings.push(format!("cannot list the untracked files: {e}")),
        Ok(listing) => {
            for rel in listing.split('\0').filter(|rel| is_input(rel, inputs)) {
                match fs::read(root.join(rel)) {
                    Ok(bytes) => {
                        files_read += 1;
                        let text = String::from_utf8_lossy(&bytes);
                        lines_read += text.lines().count();
                        check_text(format!("{rel} (untracked)"), &text, &mut findings);
                    }
                    Err(e) => findings.push(format!("cannot read {rel}: {e}")),
                }
            }
        }
    }
    let range = format!("{base}..HEAD");
    match crate::cargo::git(root, &["log", "--format=%H%x1f%B%x1e", &range]) {
        Err(e) => findings.push(format!("cannot read the commit messages after {base}: {e}")),
        Ok(log) => {
            commits_read = log.matches('\u{1e}').count();
            let (found, _) = config.commit_counts(&log);
            for (hash, count) in found {
                findings.push(format!(
                    "commit {hash}: {count} occurrences of the banned words in its message (not pushed yet: reword it before pushing)"
                ));
            }
        }
    }
    let summary = format!(
        "{lines_read} new lines in {files_read} files and {commits_read} unpushed commit messages since {how}; {} findings",
        findings.len()
    );
    let details =
        json!({ "base": base, "lines": lines_read, "files": files_read, "commits": commits_read });
    CheckResult::new(NAME, summary, findings, details)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::process::Command;

    fn fixture() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("word_scan")
    }

    /// The reviewed forms and exclusions with the fixture's own baseline.
    fn config() -> WordConfig {
        let real = Path::new(env!("CARGO_MANIFEST_DIR")).join("word-scan.json");
        let mut config = WordConfig::parse(&fs::read_to_string(real).unwrap()).unwrap();
        let baseline: Value =
            serde_json::from_str(&fs::read_to_string(fixture().join("baseline.json")).unwrap())
                .unwrap();
        config.baseline = Baseline::parse(&baseline).unwrap();
        config
    }

    /// A fixture template with each `{x:<hex>}` replaced by the text the hex spells, so that the
    /// fixture files hold none of the words they test.
    fn expand(name: &str) -> String {
        let template = fs::read_to_string(fixture().join(name)).unwrap();
        let mut out = String::new();
        let mut rest = template.as_str();
        while let Some(start) = rest.find("{x:") {
            out.push_str(&rest[..start]);
            let end = rest[start..].find('}').expect("a closed placeholder") + start;
            out.push_str(&unhex(&rest[start + 3..end]).unwrap());
            rest = &rest[end + 1..];
        }
        out.push_str(rest);
        out
    }

    /// The fixture inputs, named so that neither agent tooling nor br takes them for the real ones.
    const FIXTURE_INPUTS: Inputs<'static> = Inputs {
        docs: "docs",
        files: &["AGENTS.fixture.md", "README.fixture.md"],
        tracker: "tracker.fixture.jsonl",
    };

    /// Check (f): forms, case, joining, exclusion by whole name (with the masked count of each
    /// excluded identifier) and look-alike words.
    #[test]
    fn forms_and_exclusions_are_counted_exactly() {
        let config = config();
        let notes = expand("notes.tmpl");
        let found = config.occurrences(&notes);
        let lines: Vec<usize> = found
            .offsets
            .iter()
            .map(|offset| text::line_at(&notes, *offset))
            .collect();
        assert_eq!(lines, [2, 3, 4, 5, 6, 7, 8, 9, 10]);
        assert_eq!(found.masked, [1, 0, 0, 1]);
    }

    /// The configuration decodes to the reviewed forms and identifiers, and no file of this
    /// package (configuration, sources, tests, fixtures) holds an occurrence of either word. One
    /// excluded identifier, bead infra-m0.5's fixed acceptance case name, may appear where this
    /// package names that bead's cases: its acceptance manifest, its acceptance target and the
    /// bead's case module; every other excluded identifier masks nothing anywhere here.
    #[test]
    fn this_package_holds_none_of_the_words() {
        let config = config();
        assert_eq!(config.forms.len(), 5);
        assert_eq!(config.forms[0].len(), 2);
        assert_eq!(config.excluded.len(), 4);
        // infra-m0.5's case name, found by its bead prefix so this file does not hold it.
        let case_prefix = "infra_m0_5__";
        let allowed: Vec<usize> = config
            .excluded
            .iter()
            .enumerate()
            .filter(|(_, e)| e.identifier.starts_with(case_prefix))
            .map(|(index, _)| index)
            .collect();
        assert_eq!(
            allowed.len(),
            1,
            "one excluded identifier is {case_prefix}*"
        );
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut paths = Vec::new();
        files_under(root, &mut paths).unwrap();
        let mut scanned = 0;
        for path in paths {
            let text = String::from_utf8_lossy(&fs::read(&path).unwrap()).into_owned();
            let found = config.occurrences(&text);
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let names_the_case = rel == "acceptance.json"
                || rel == "tests/acceptance.rs"
                || rel == "tests/acceptance/infra_m0_5.rs";
            let masked_ok = found
                .masked
                .iter()
                .enumerate()
                .all(|(index, n)| *n == 0 || (names_the_case && allowed.contains(&index)));
            assert!(
                found.offsets.is_empty() && masked_ok,
                "{} holds {} occurrences and {:?} masked identifiers",
                path.display(),
                found.offsets.len(),
                found.masked
            );
            scanned += 1;
        }
        assert!(scanned >= 20, "only {scanned} files were scanned");
    }

    fn git(dir: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args([
                "-c",
                "user.name=xtask fixture",
                "-c",
                "user.email=xtask-fixture@invalid",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    /// Deletes a throwaway directory; git marks its object files read-only, which can stop a
    /// recursive delete on Windows.
    fn remove_tree(path: &Path) {
        for _ in 0..5 {
            if !path.exists() || fs::remove_dir_all(path).is_ok() {
                return;
            }
            clear_readonly(path);
        }
    }

    // Windows only, where this clears the read-only attribute rather than widening Unix modes.
    #[cfg(windows)]
    #[allow(clippy::permissions_set_readonly_false)]
    fn clear_readonly(path: &Path) {
        let mut pending = vec![path.to_path_buf()];
        while let Some(next) = pending.pop() {
            let Ok(meta) = fs::symlink_metadata(&next) else {
                continue;
            };
            if meta.is_dir() {
                if let Ok(entries) = fs::read_dir(&next) {
                    pending
                        .extend(entries.filter_map(|entry| entry.ok().map(|entry| entry.path())));
                }
            } else if meta.permissions().readonly() {
                let mut permissions = meta.permissions();
                permissions.set_readonly(false);
                let _ = fs::set_permissions(&next, permissions);
            }
        }
    }

    #[cfg(not(windows))]
    fn clear_readonly(_path: &Path) {}

    /// Check (f) through the real route: the fixture is written into a throwaway git repository in
    /// the system temporary directory (never inside this checkout), two commits are made, one of
    /// them with a banned word in its message, and `scan` reads them with `git log`. Files,
    /// tracker records and commit messages above their baseline fail; counts at or below it are
    /// reported and pass; each excluded identifier's masked count is reported.
    #[test]
    fn counts_above_the_baseline_fail_through_git_log() {
        let config = config();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or_default();
        let root =
            std::env::temp_dir().join(format!("xtask-word-scan-{}-{nanos}", std::process::id()));
        fs::create_dir_all(root.join("docs")).unwrap();
        fs::write(root.join("docs").join("notes.md"), expand("notes.tmpl")).unwrap();
        fs::write(root.join("AGENTS.fixture.md"), expand("agents.tmpl")).unwrap();
        fs::write(root.join("README.fixture.md"), expand("readme.tmpl")).unwrap();
        fs::write(root.join("tracker.fixture.jsonl"), expand("tracker.tmpl")).unwrap();
        git(&root, &["init", "--quiet"]);
        git(
            &root,
            &[
                "commit",
                "--quiet",
                "--allow-empty",
                "-m",
                "build the runner",
            ],
        );
        let subject = format!("one listed form: {}", unhex("6761746564").unwrap());
        git(
            &root,
            &[
                "commit",
                "--quiet",
                "--allow-empty",
                "-m",
                &subject,
                "-m",
                "body",
            ],
        );
        let head = git(&root, &["rev-parse", "HEAD"]).trim().to_owned();

        let mut problems = Vec::new();
        let counts = scan(&root, &FIXTURE_INPUTS, &config, &mut problems);
        let (findings, details) = compare(&counts, &config.baseline);
        let exclusions = exclusions_report(&config, &counts);
        remove_tree(&root);

        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(counts.files_scanned, 3);
        assert_eq!(counts.tracker_records, 3);
        assert_eq!(counts.commits_scanned, 2);
        assert_eq!(
            counts.tracker,
            BTreeMap::from([("fx-1".to_owned(), 4), ("fx-2".to_owned(), 1)])
        );
        assert_eq!(counts.commits, BTreeMap::from([(head.clone(), 1)]));
        assert_eq!(
            findings,
            [
                "file README.fixture.md: 1 occurrences of the banned words, above its baseline of 0 (lines 3)".to_owned(),
                "tracker record fx-1: 4 occurrences of the banned words, above its baseline of 3".to_owned(),
                "tracker record fx-2: 1 occurrences of the banned words, above its baseline of 0".to_owned(),
                format!("commit {head}: 1 occurrences of the banned words, above its baseline of 0"),
            ]
        );
        assert_eq!(
            details["files"]["docs/notes.md"],
            json!({"count": 9, "baseline": 9})
        );
        assert_eq!(
            details["files"]["docs/old.md"],
            json!({"count": 0, "baseline": 3})
        );
        assert_eq!(details["files"]["AGENTS.fixture.md"], Value::Null);
        assert_eq!(
            exclusions[0]["masked"],
            json!({"files": 1, "tracker_records": 1, "commit_messages": 0})
        );
        assert_eq!(
            exclusions[3]["masked"],
            json!({"files": 1, "tracker_records": 0, "commit_messages": 0})
        );
        assert_eq!(
            exclusions[0]["identifier_hex"],
            "696e6672615f6d305f355f5f71756172616e74696e655f67617465"
        );
    }

    #[test]
    fn hex_decodes_ascii_only() {
        assert_eq!(unhex("6162").unwrap(), "ab");
        assert!(unhex("616").is_err());
        assert!(unhex("zz").is_err());
        assert!(unhex("c3a9").is_err());
    }
}
