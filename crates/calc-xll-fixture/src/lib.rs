//! Source-built X1 fixture. It uses the real host export and companion DLL in both hosts.
#[cfg(target_arch = "wasm32")]
compile_error!("calc-xll-fixture is native-only");
#[cfg(windows)]
mod fixture;
pub fn abi_version() -> u32 {
    12
}
