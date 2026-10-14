//! The package for the DNA Calc workspace's repository tooling (`cargo xtask`).
//!
//! Native only: it is meant to drive cargo and git on Windows and Linux hosts, and it refuses to
//! compile for wasm32.

#[cfg(target_arch = "wasm32")]
compile_error!(
    "xtask is native-only and must not be built for wasm32; portable and browser builds exclude it"
);

use std::process::ExitCode;

fn main() -> ExitCode {
    let requested: Vec<String> = std::env::args().skip(1).collect();
    eprintln!(
        "xtask: no subcommand is implemented (requested: {requested:?}); the acceptance runner `accept` is not built yet, \
         so each acceptance case runs as `cargo test -p <package> --test acceptance -- --exact <case> --nocapture` \
         and its transcript must report `1 passed`"
    );
    ExitCode::from(2)
}
