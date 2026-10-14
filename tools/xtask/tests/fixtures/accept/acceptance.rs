//! Fixture acceptance target. The infra-m0.3 cases copy it into `tools/xtask-fixture/tests/` of a
//! copy of the workspace, with `acceptance.json` beside the package as its manifest. Every case
//! first creates a file named after itself in the directory `XTASK_FIXTURE_MARKERS` names, so the
//! cases that drive the runner can see, independently of the runner, which cases really ran.

#![allow(non_snake_case)]

use std::path::Path;

fn ran(case: &str) {
    let dir = std::env::var_os("XTASK_FIXTURE_MARKERS")
        .expect("XTASK_FIXTURE_MARKERS names the marker directory");
    std::fs::write(Path::new(&dir).join(case), case).expect("the marker directory is writable");
}

#[test]
fn fixture_pass__alpha() {
    ran("fixture_pass__alpha");
}

#[test]
fn fixture_misspelled__alpha() {
    ran("fixture_misspelled__alpha");
}

#[test]
fn fixture_partial__alpha_case() {
    ran("fixture_partial__alpha_case");
}

#[test]
#[ignore = "fixture: an ignored case never counts as executed"]
fn fixture_ignored__alpha() {
    ran("fixture_ignored__alpha");
}

#[test]
fn fixture_failing__alpha() {
    ran("fixture_failing__alpha");
    panic!("fixture: this case fails after it ran");
}

#[test]
fn fixture_notrun__alpha() {
    ran("fixture_notrun__alpha");
    println!(
        "ACCEPT-NOT-RUN: fixture_notrun__alpha: the fixture host lacks the device this case needs"
    );
}

#[test]
fn fixture_notrun_stderr__alpha() {
    ran("fixture_notrun_stderr__alpha");
    eprintln!(
        "ACCEPT-NOT-RUN: fixture_notrun_stderr__alpha: the fixture host lacks the browser driver this case needs"
    );
}

#[test]
fn fixture_unregistered__alpha() {
    ran("fixture_unregistered__alpha");
}

#[test]
fn fixture_unregistered__beta() {
    ran("fixture_unregistered__beta");
}

#[test]
fn fixture_excel__alpha() {
    ran("fixture_excel__alpha");
}

#[test]
fn fixture_commands__alpha() {
    ran("fixture_commands__alpha");
}
