//! Fixture test target `acceptance`, copied into `tools/ci-matrix-fixture/tests/` of a copy of the
//! workspace by the infra-m0.2 cases. `cargo xtask ci` leaves every acceptance target to
//! `cargo xtask accept`, so this case must never run in a CI package test run: if it does, it
//! leaves a marker and fails.

#![allow(non_snake_case)]

#[test]
fn fixture_ci_matrix__never_in_package_runs() {
    if let Some(dir) = std::env::var_os("CI_MATRIX_MARKERS") {
        let dir = std::path::Path::new(&dir);
        let _ = std::fs::create_dir_all(dir);
        let _ = std::fs::write(dir.join("acceptance-ran"), "ran");
    }
    panic!("the acceptance target ran in a package test run of cargo xtask ci");
}
