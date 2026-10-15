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
//! `cargo xtask pin-sources` pins the legacy repositories and the port inputs the beads cite in
//! `migration/source-manifest.json`, with each input's numeric provenance from the reviewed
//! `migration/numeric-provenance.json`, and writes the release source allowlist
//! `migration/release-source-allowlist.json` (`pin.rs`, `provenance.rs`); `cargo xtask
//! pin-sources --lookup <Repo>/<path>` prints one pinned input, or why there is none, with an exit
//! code per disposition (`pin.rs`).
//!
//! Native only: it drives cargo and git on Windows and Linux hosts, and it refuses to compile for
//! wasm32.

#[cfg(target_arch = "wasm32")]
compile_error!(
    "xtask is native-only and must not be built for wasm32; portable and browser builds exclude it"
);

mod accept;
mod arch;
mod cargo;
mod ci;
mod ci_audit;
mod manifest;
mod pin;
mod pin_cite;
mod pin_git;
mod pin_rust;
mod provenance;
mod rust_source;
mod seams;
mod text;
mod words;

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.split_first() {
        Some((command, rest)) if command == "accept" => accept::run(rest),
        Some((command, rest)) if command == "pin-sources" => pin::run(rest),
        Some((command, rest)) if command == "ci" => ci::run(rest),
        Some((command, rest)) if command == "ci-audit" => ci_audit::run(rest),
        _ => {
            eprintln!(
                "xtask: unknown or missing command {args:?}\n{}\n{}\n{}\n{}",
                accept::USAGE,
                pin::USAGE,
                ci::USAGE,
                ci_audit::USAGE
            );
            ExitCode::from(2)
        }
    }
}
