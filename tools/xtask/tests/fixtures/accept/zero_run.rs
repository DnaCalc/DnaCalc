//! Fixture test target `zero_run`, built with `harness = false`. The infra-m0.3 cases copy it into
//! `tools/xtask-fixture/tests/` of a copy of the workspace. It stands for a harness that lists a
//! case and then executes nothing for it: under libtest's `--list` it lists
//! `fixture_zero_run__alpha`, and run with any other arguments it executes no test and prints
//! libtest's result line for 0 tests, exiting 0. When it is run (not listed) it first creates a
//! file named after the case, holding the arguments it was given, in the directory
//! `XTASK_FIXTURE_MARKERS` names, so the case that drives the runner can see, independently of
//! the runner, that the runner really ran it and with which filter.

use std::path::Path;

const CASE: &str = "fixture_zero_run__alpha";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--list") {
        println!("{CASE}: test");
        println!();
        println!("1 test, 0 benchmarks");
        return;
    }
    let dir = std::env::var_os("XTASK_FIXTURE_MARKERS")
        .expect("XTASK_FIXTURE_MARKERS names the marker directory");
    std::fs::write(Path::new(&dir).join(CASE), args.join(" "))
        .expect("the marker directory is writable");
    println!();
    println!("running 0 tests");
    println!();
    println!(
        "test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 0.00s"
    );
    println!();
}
