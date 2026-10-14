//! The DNA Calc workspace's repository tooling, run as `cargo xtask <command>` through the alias
//! in `.cargo/config.toml`.
//!
//! `cargo xtask accept --id <bead> --platform windows|linux|wasm|excel [--require-all]` runs the
//! architecture checks, then the acceptance cases that the per-package `acceptance.json`
//! manifests register for the bead on that platform, and writes a machine-readable receipt
//! (`accept.rs`).
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
mod manifest;
mod rust_source;
mod seams;
mod text;
mod words;

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.split_first() {
        Some((command, rest)) if command == "accept" => accept::run(rest),
        _ => {
            eprintln!(
                "xtask: unknown or missing command {args:?}\n{}",
                accept::USAGE
            );
            ExitCode::from(2)
        }
    }
}
