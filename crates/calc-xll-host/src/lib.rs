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
/// Owning-STA RTD service; worksheet results require admitted input reads.
#[cfg(windows)]
pub trait RtdService {
    fn subscribe(
        &self,
        provider: RtdProvider,
        caller: calc_value::CellKey,
        progid: &str,
        topics: &[String],
    ) -> Result<u64, String>;
    /// The exact ConnectData observation while it still awaits owner admission.
    /// This is a provisional C API return, not a certified worksheet input. The
    /// context still records its old/missing cut and normal admission invalidates it.
    fn initial_value(
        &self,
        _provider: RtdProvider,
        _subscription: u64,
    ) -> Option<(u64, calc_value::CalcValue)> {
        None
    }
    fn close(&self) -> Result<(), String>;
    fn subscriptions(&self, provider: RtdProvider, caller: calc_value::CellKey) -> Vec<u64>;
    /// Recover only the inputs of this completed call when native code returns
    /// from its own cache without another xlfRtd callback. New logical leases
    /// still need an ordinary admitted input read before publishing that result.
    fn cached_call_inputs(
        &self,
        _provider: RtdProvider,
        _caller: calc_value::CellKey,
        _arguments: &[calc_value::CalcValue],
    ) -> Result<Vec<u64>, String> {
        Ok(Vec::new())
    }
    /// Record one completed native call. The owner reconciles the union of calls
    /// in a committed producing certificate, after the whole evaluation finishes.
    fn finish_call(
        &self,
        _provider: RtdProvider,
        _caller: calc_value::CellKey,
        _invocation: u64,
        _arguments: &[calc_value::CalcValue],
        _declared: &[u64],
        _inputs: &[u64],
    ) -> Result<(), String> {
        Ok(())
    }
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
