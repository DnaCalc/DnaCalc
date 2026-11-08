//! The product-independent command. Checks run here; other commands use the separately built
//! harness in the same target directory as the workspace builds and tests.
use xtask::harness;

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.split_first() {
        Some((command, rest)) if command == "check" => xtask::check::run(rest),
        _ => harness::run(&args),
    }
}
