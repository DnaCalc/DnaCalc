//! Integration suites for this package.
//! xtask accept selects named cases under acceptance_cases.

#[path = "acceptance.rs"]
mod acceptance_cases;

#[path = "excel_roundtrip.rs"]
mod excel_roundtrip;

#[path = "formatting.rs"]
mod formatting;

#[path = "rtd.rs"]
mod rtd;

#[path = "vba.rs"]
mod vba;

#[path = "vba_corpus.rs"]
mod vba_corpus;

#[cfg(not(target_arch = "wasm32"))]
#[path = "js.rs"]
mod js;
