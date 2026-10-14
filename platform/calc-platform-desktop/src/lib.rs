//! The native-only package for the desktop platform adapter: native filesystem and operating-system services for the desktop application.
//!
//! Native only: it refuses to compile for wasm32, so it never enters portable library or browser builds.

#[cfg(target_arch = "wasm32")]
compile_error!(
    "calc-platform-desktop is native-only and must not be built for wasm32; portable and browser builds exclude it"
);
