//! Small XLCALL32 companion DLL. It resolves callbacks in the main image, just as XLLs do.
#[cfg(target_arch = "wasm32")]
compile_error!("calc-xll-shim is native-only");
#[cfg(not(target_arch = "wasm32"))]
mod bridge;
#[cfg(not(target_arch = "wasm32"))]
pub use bridge::*;
