//! The portable zero-test fixture package of the infra-m0.2 cases: it has no items and no tests,
//! natively or as wasm-bindgen tests, so `cargo xtask ci` must record it as `zero_tests` on every
//! target, and on wasm wasm-bindgen-test-runner reports `no tests to run!` for it and starts no
//! browser. The product packages hold wasm-bindgen tests, so without this package no
//! package-browser pair of the copy would show how a zero-test package is recorded and audited.
