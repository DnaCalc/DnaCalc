//! The DNA Calc workspace's repository tooling, run as `cargo xtask <command>` through the alias
//! in `.cargo/config.toml`.
//!
//! `cargo xtask accept --id <bead> --platform windows|linux|wasm|excel [--require-all]` runs the
//! architecture checks, then the acceptance cases that the per-package `acceptance.json`
//! manifests register for the bead on that platform, and writes a machine-readable receipt
//! (`accept.rs`).
//!
//! `cargo xtask ci windows|linux|wasm|licenses|desktop` is the one command each CI job runs: the
//! package tests of one target (natively, or under wasm-bindgen-test-runner for Chrome and
//! Firefox) with a machine-readable executed-target manifest, the dependency source and license
//! check, or a release build of the native-only packages, which packages nothing until `dc-x704`
//! (`ci.rs`). `cargo xtask ci-audit --require-executed-targets windows,linux,wasm (--run <id> |
//! --from <dir>)... [--allow-dirty]` merges those manifests and fails unless every portable package
//! and both app cores were built and had their test harness run on each required target, from a
//! clean tree unless `--allow-dirty` (`ci_audit.rs`).
//!
//! `cargo xtask check [--wasm]` builds the workspace and runs `cargo test` for every package,
//! printing one summary line per package; with `--wasm` it also runs the portable packages' tests
//! in headless Chrome and Firefox (`check.rs`).
//!
//! `cargo xtask evidence verify [--all | --sample N] [--seed S]` and `cargo xtask evidence restore
//! <dir> [--sample N] [--seed S]` run the evidence repository's own `archive-evidence.py` over the
//! backup of OxFunc's smart-fuzzer evidence, after printing how the checkout compares with the
//! pinned commit, and exit with its exit code (`evidence.rs`); `cargo xtask evidence labels
//! [--write]` recomputes the acquisition labels of `migration/evidence/oxfunc-smart-fuzzer.json`
//! and prints how the record differs or writes them into it (`evidence_labels.rs`).
//!
//! `cargo xtask witness judge [--func F] [--bank pinned|full]` judges calc-func function by
//! function, bit for bit, on Excel's readings (the pinned sets under `crates/calc-func/tests/witnesses`
//! or every bank of the evidence repository) and rewrites `crates/calc-func/witness-status.tsv`;
//! `cargo xtask witness pin` writes the pinned sets from the full banks, and `cargo xtask witness
//! read --func F` takes new readings of a pinned set's rows on this host's Excel (`witness.rs`,
//! dc-ymjf.2 and dc-ymjf.3).
//!
//! `cargo xtask verdict --capture <file> --results <file> --evaluations <file> --out <file>
//! [--evidence <record>] [--exact-target <excel build>,<channel>,<compatibility>,<numeric
//! environment>]` judges a canonical CaptureRecord, result set and evaluation record through
//! calc-verdict, the one verdict path, and writes the CellVerdict report (`verdict.rs`).
//!
//! `cargo xtask compare --fixture <xlsx> --capture <capture.json> --out <dir> [--exact-target
//! ...]` opens the workbook through calc-host, recalculates and projects it, judges every formula
//! cell of the capture's inventory through calc-verdict against an evaluation record built from
//! the evaluator's own reports, classifies every cell, and writes `report.json`,
//! `evaluations.json` and `triage.json` with a one-screen summary (`compare.rs`,
//! `compare_triage.rs`).
//!
//! `cargo xtask capture --fixture <xlsx|xlsm> --out <capture.json> [--timeout <s>] [--case <id>]
//! [--verdict <report.json>] ...` takes one live capture through calc-oracle-excel, the one Excel
//! capture adapter, writes the CaptureRecord version 2 and, with `--verdict`, the verdict record of
//! its file-cache replay (`capture.rs`). It needs Excel on Windows; on the owner's machine any
//! agent may run it (AGENTS.md, Excel on this machine).
//!
//! `cargo xtask corpus [<dir>] [--oracle] [--out <dir>] [--cache <dir>] [--import
//! <capture.json>]... [--no-capture] [--timeout <s>]` compares every workbook of a folder, and with
//! `--oracle` the oracle workbooks committed here, with Excel: each captured once and cached
//! outside the repository by workbook hash and Excel build, compared by a release `cargo xtask
//! compare` process, and summarised in `summary.md` and `summary.json` with the failures grouped
//! by root cause across the corpus (`corpus.rs`, `corpus_summary.rs`).
//!
//! `cargo xtask bench <dir> [--runs <n>] [--out <dir>] [--cache <dir>] [--edit
//! <workbook>=<sheet>!<cell>]... [--excel] [--no-dna] [--timeout <s>]` times DNA Calc's open, full
//! rebuild, full recalculation and a one-cell edit's recalculation of every workbook of a folder,
//! each run in its own release process through calc-host, and with `--excel` Excel's own open,
//! `CalculateFullRebuild`, `CalculateFull` and the same edit in an isolated Excel instance, with
//! peak memory and the machine's load, writing the medians and spreads to `bench.md` and every
//! sample to `bench.json` (`bench.rs`, bead dc-10d9).
//!
//! `cargo xtask app bundle onecalc|calc [--out <dir>]` builds DNA OneCalc's or DNA Calc's browser
//! bundle (the app's wasm entry through wasm-bindgen, and its page) into a folder, and `cargo xtask
//! app smoke onecalc|calc [--bundle <dir>] [--browser chromium|firefox] [--workbook <file> [--edit
//! <sheet>!<cell>=<text>]]` serves it on 127.0.0.1 and drives it in headless Chrome and Firefox
//! through WebDriver, failing on a wrong answer or any panic or uncaught error the page records
//! (`app.rs`); `cargo xtask onecalc bundle|smoke` is the same for DNA OneCalc.
//!
//! `cargo xtask site pack|serve|smoke|verify|publish|status|rollback onecalc|calc ...` packs an
//! app's release for dnacalc.com (hashed assets, build.json, LICENSE and third-party notices, the
//! tar the host receives), serves and smoke-tests it under the site's headers, verifies a served
//! site against its build.json, and gives the owner the publish, status and rollback commands
//! (`site.rs`, dc-ysl).
//!
//! Native only: it drives cargo and git on Windows and Linux hosts, and it refuses to compile for
//! wasm32.

#[cfg(target_arch = "wasm32")]
compile_error!(
    "xtask is native-only and must not be built for wasm32; portable and browser builds exclude it"
);

mod accept;
mod app;
mod arch;
// The bench command times Excel through calc-oracle-excel, like the capture command.
#[cfg(not(target_arch = "wasm32"))]
mod bench;
// The collation command asks Windows for sort keys (dc-zsn.1).
#[cfg(not(target_arch = "wasm32"))]
mod collation;
#[cfg(not(target_arch = "wasm32"))]
mod fidelity;
// calc-oracle-excel, which the capture command calls, is not a dependency of a wasm32 build of
// xtask (it refuses wasm32 itself), so neither is this command: such a build then fails through
// xtask's own refusal alone, and compiles when that refusal is disabled.
#[cfg(not(target_arch = "wasm32"))]
mod capture;
mod cargo;
mod check;
mod ci;
mod ci_audit;
mod compare;
mod compare_triage;
// The corpus command takes captures through calc-oracle-excel, like the capture command.
#[cfg(not(target_arch = "wasm32"))]
mod corpus;
mod corpus_summary;
mod evidence;
mod evidence_labels;
mod legacy;
mod licence;
mod manifest;
mod owners;
mod rust_source;
mod site;
mod text;
mod verdict;
// The witness command reads Excel through calc-oracle-excel, like the capture command.
#[cfg(not(target_arch = "wasm32"))]
mod witness;
mod words;

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.split_first() {
        Some((command, rest)) if command == "accept" => accept::run(rest),
        Some((command, rest)) if command == "check" => check::run(rest),
        Some((command, rest)) if command == "ci" => ci::run(rest),
        Some((command, rest)) if command == "ci-audit" => ci_audit::run(rest),
        Some((command, rest)) if command == "evidence" => evidence::run(rest),
        Some((command, rest)) if command == "verdict" => verdict::run(rest),
        Some((command, rest)) if command == "compare" => compare::run(rest),
        Some((command, rest)) if command == "app" => app::run(rest),
        Some((command, rest)) if command == "onecalc" => app::run_onecalc(rest),
        Some((command, rest)) if command == "site" => site::run(rest),
        #[cfg(not(target_arch = "wasm32"))]
        Some((command, rest)) if command == "witness" => witness::run(rest),
        #[cfg(not(target_arch = "wasm32"))]
        Some((command, rest)) if command == "capture" => capture::run(rest),
        #[cfg(not(target_arch = "wasm32"))]
        Some((command, rest)) if command == "corpus" => corpus::run(rest),
        #[cfg(not(target_arch = "wasm32"))]
        Some((command, rest)) if command == "bench" => bench::run(rest),
        #[cfg(not(target_arch = "wasm32"))]
        Some((command, rest)) if command == "fidelity" => fidelity::run(rest),
        #[cfg(not(target_arch = "wasm32"))]
        Some((command, rest)) if command == "collation" => collation::run(rest),
        _ => {
            #[allow(unused_mut)]
            let mut usages = vec![
                accept::USAGE,
                check::USAGE,
                ci::USAGE,
                ci_audit::USAGE,
                evidence::USAGE,
                verdict::USAGE,
                compare::USAGE,
                app::USAGE,
                site::USAGE,
            ];
            #[cfg(not(target_arch = "wasm32"))]
            usages.push(capture::USAGE);
            #[cfg(not(target_arch = "wasm32"))]
            usages.push(corpus::USAGE);
            #[cfg(not(target_arch = "wasm32"))]
            usages.push(bench::USAGE);
            #[cfg(not(target_arch = "wasm32"))]
            usages.push(fidelity::USAGE);
            #[cfg(not(target_arch = "wasm32"))]
            usages.push(collation::USAGE);
            #[cfg(not(target_arch = "wasm32"))]
            usages.push(witness::USAGE);
            eprintln!(
                "xtask: unknown or missing command {args:?}\n{}",
                usages.join("\n")
            );
            ExitCode::from(2)
        }
    }
}
