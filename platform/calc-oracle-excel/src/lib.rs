//! The native-only package for the Excel oracle adapter: live capture against an installed Excel.
//!
//! Native only: it refuses to compile for wasm32, so it never enters portable library or browser builds.

#[cfg(target_arch = "wasm32")]
compile_error!(
    "calc-oracle-excel is native-only and must not be built for wasm32; portable and browser builds exclude it"
);
