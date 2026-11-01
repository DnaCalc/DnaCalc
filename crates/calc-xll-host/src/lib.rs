//! Native XLL adapter. The application exports MdCallBack12 and calls [`dispatch`].
//! Formula parsing, binding and evaluation remain in CalcFml through UserFunctions.
#[cfg(target_arch = "wasm32")]
compile_error!("calc-xll-host is native-only");
#[cfg(all(windows, not(target_arch = "x86_64")))]
compile_error!("the first XLL host supports Windows x64 only");

#[cfg(windows)]
mod callbacks;
#[cfg(windows)]
mod international;
#[cfg(windows)]
mod invoke;
#[cfg(windows)]
mod native;
#[cfg(windows)]
mod worker;
#[cfg(windows)]
pub use calc_fml::reference::format_workbook_sheet_qualifier;
#[cfg(windows)]
pub use calc_func::UserCallableIdentity as RtdProvider;
#[cfg(windows)]
pub use callbacks::dispatch;
#[cfg(windows)]
pub use native::{AsyncReturn, RegistrationInfo, XllHost};
/// Owning-STA service; RTD returns admitted inputs, never an async call result.
#[cfg(windows)]
pub trait RtdService {
    fn subscribe(
        &self,
        provider: RtdProvider,
        caller: calc_value::CellKey,
        progid: &str,
        topics: &[String],
    ) -> Result<u64, String>;
    fn close(&self) -> Result<(), String>;
    fn subscriptions(&self, provider: RtdProvider, caller: calc_value::CellKey) -> Vec<u64>;
    fn has_live_servers(&self) -> bool;
}

#[cfg(not(windows))]
pub struct XllHost;
#[cfg(not(windows))]
impl XllHost {
    pub fn load(&self, _path: &std::path::Path) -> Result<(), String> {
        Err("dc-snz.2: XLL loading requires Windows x64".into())
    }
}
