//! Product-independent repository checks and process support shared with xtask-harness.
#[cfg(target_arch = "wasm32")]
compile_error!("xtask is native-only and must not be built for wasm32");

pub mod arch;
pub mod cargo;
pub mod check;
pub mod licence;
pub mod rust_source;
pub mod test_output;
pub mod text;
pub mod words;
