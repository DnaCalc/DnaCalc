//! Excel's interface identities and vtable layout for the measured add-in surface.
//! No class registration, activation hook, type-library registration or registry write.
#[cfg(target_arch = "wasm32")]
compile_error!("calc-com-host is native-only");
#[cfg(all(windows, not(target_arch = "x86_64")))]
compile_error!("calc-com-host supports Windows x64 only");

#[cfg(windows)]
mod projection;
#[cfg(windows)]
pub use projection::Projection;
