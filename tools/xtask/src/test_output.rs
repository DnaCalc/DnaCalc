//! Libtest counts and explicit cases that did not run.

/// The counts of one libtest result line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub passed: u64,
    pub failed: u64,
    pub ignored: u64,
}

impl Counts {
    pub fn add(&mut self, other: Counts) {
        self.passed += other.passed;
        self.failed += other.failed;
        self.ignored += other.ignored;
    }
}

/// The counts libtest's `test result: ...` line reports, or `None` when it lacks one of them.
pub fn result_counts(line: &str) -> Option<Counts> {
    let rest = line.trim().strip_prefix("test result: ")?;
    let count = |label: &str| {
        rest.split(';').find_map(|part| {
            let mut words = part.split_whitespace().rev();
            let named = words.next() == Some(label);
            let number = words.next().and_then(|n| n.parse::<u64>().ok());
            if named { number } else { None }
        })
    };
    Some(Counts {
        passed: count("passed")?,
        failed: count("failed")?,
        ignored: count("ignored")?,
    })
}

use std::collections::{BTreeMap, BTreeSet};

/// Counts from complete harnesses, with explicitly unavailable cases removed from passes.
#[derive(Debug, Default)]
pub struct Summary {
    pub counts: Counts,
    pub not_run: u64,
    pub harnesses: u64,
}

/// Reads libtest's captured successful output (`--show-output`) or a runner's `--nocapture`
/// transcript. Markers are deduplicated per actual test and per harness. A failed case remains
/// failed even when it emitted a marker before failing.
pub fn summarize(text: &str) -> Summary {
    let mut summary = Summary::default();
    let mut states = BTreeMap::new();
    let mut marked = BTreeSet::new();
    let mut owner = None::<String>;
    let mut pending = None::<String>;
    for line in text.lines() {
        let line = line.trim();
        if let Some(test) = line.strip_prefix("test ")
            && let Some((name, state)) = test.split_once(" ... ")
        {
            if matches!(state, "ok" | "FAILED" | "ignored") {
                states.insert(name.to_owned(), state.to_owned());
                pending = None;
                owner = None;
            } else {
                // With --nocapture a marker can interrupt libtest's result line. The
                // eventual standalone status still belongs to this named test.
                pending = Some(name.to_owned());
                owner = pending.clone();
            }
        } else if matches!(line, "ok" | "FAILED" | "ignored")
            && let Some(name) = pending.take()
        {
            states.insert(name, line.to_owned());
            owner = None;
        }
        if let Some(name) = line
            .strip_prefix("---- ")
            .and_then(|s| s.strip_suffix(" stdout ----"))
        {
            owner = Some(name.to_owned());
        } else if matches!(line, "successes:" | "failures:") {
            owner = None;
        }
        if let Some(at) = line.find("ACCEPT-NOT-RUN: ")
            && (at == 0 || line.starts_with("test "))
            && let Some((case, _)) = line[at + "ACCEPT-NOT-RUN: ".len()..].split_once(": ")
        {
            marked.insert(owner.clone().unwrap_or_else(|| case.to_owned()));
        }
        if let Some(mut counts) = result_counts(line) {
            let not_run = marked
                .iter()
                .filter(|name| {
                    let state = states.get(*name).or_else(|| {
                        states.iter().find_map(|(test, state)| {
                            test.ends_with(&format!("::{name}")).then_some(state)
                        })
                    });
                    state.is_none_or(|state| state == "ok")
                })
                .count() as u64;
            // A single captured test may describe more than one unavailable subcase. It can
            // remove at most one pass; uncaptured runner output is bounded by libtest's count.
            let not_run = not_run.min(counts.passed);
            counts.passed -= not_run;
            summary.counts.add(counts);
            summary.not_run += not_run;
            summary.harnesses += 1;
            states.clear();
            marked.clear();
            owner = None;
            pending = None;
        }
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    const RESULT: &str = "test result: ok. 2 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.01s\n";

    #[test]
    fn successful_output_is_not_a_pass_when_the_case_could_not_run() {
        let report = summarize(&format!(
            "test a ... ok\ntest b ... ok\nsuccesses:\n---- b stdout ----\nACCEPT-NOT-RUN: other_alias: no Excel\nACCEPT-NOT-RUN: other_alias: stderr duplicate\n{RESULT}"
        ));
        assert_eq!(
            report.counts,
            Counts {
                passed: 1,
                failed: 0,
                ignored: 1
            }
        );
        assert_eq!((report.not_run, report.harnesses), (1, 1));
    }

    #[test]
    fn each_harness_counts_its_own_case_and_duplicates_only_once() {
        let harness =
            format!("ACCEPT-NOT-RUN: a: no device\nACCEPT-NOT-RUN: a: same device\n{RESULT}");
        let report = summarize(&harness.repeat(2));
        assert_eq!(
            (report.counts.passed, report.not_run, report.harnesses),
            (2, 2, 2)
        );
    }

    #[test]
    fn a_failure_after_a_marker_remains_a_failure() {
        let report = summarize(
            "test a ... FAILED\ntest b ... ok\n---- a stdout ----\nACCEPT-NOT-RUN: alias: no device\ntest result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out\n",
        );
        assert_eq!(
            (report.counts.passed, report.counts.failed, report.not_run),
            (1, 1, 0)
        );
    }

    #[test]
    fn an_inline_marker_followed_by_failure_cannot_remove_another_tests_pass() {
        let report = summarize(
            "test a ... ACCEPT-NOT-RUN: a: no device\nFAILED\ntest b ... ok\ntest result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out\n",
        );
        assert_eq!(
            (report.counts.passed, report.counts.failed, report.not_run),
            (1, 1, 0)
        );
    }

    #[test]
    fn captured_subcase_markers_remove_only_the_actual_tests_pass() {
        let report = summarize(&format!(
            "test a ... ok\ntest b ... ok\n---- a stdout ----\nACCEPT-NOT-RUN: first: no device\nACCEPT-NOT-RUN: second: no capture\n{RESULT}"
        ));
        assert_eq!((report.counts.passed, report.not_run), (1, 1));
    }

    #[test]
    fn marker_text_in_a_report_is_not_an_execution_marker() {
        let report = summarize(&format!(
            "the fixture should print ACCEPT-NOT-RUN: a: no device\n{RESULT}"
        ));
        assert_eq!((report.counts.passed, report.not_run), (2, 0));
        assert_eq!(summarize("no tests to run!\n").harnesses, 0);
    }
}
