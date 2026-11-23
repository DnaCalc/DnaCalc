//! Native XLL adapter. The application exports MdCallBack12 and calls [`dispatch`].
//! Formula parsing, binding and evaluation remain in CalcFml through UserFunctions.
#[cfg(target_arch = "wasm32")]
compile_error!("calc-xll-host is native-only");
#[cfg(all(windows, not(target_arch = "x86_64")))]
compile_error!("the first XLL host supports Windows x64 only");

#[cfg(windows)]
mod call;
#[cfg(not(target_arch = "wasm32"))]
mod callback_abi;
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
pub use calc_func::{RtdConnection, RtdConnector, RtdLease, TopicKey};
#[cfg(windows)]
pub use calc_xll_abi::error_number;
#[cfg(all(not(windows), not(target_arch = "wasm32")))]
pub use callback_abi::dispatch;
#[cfg(windows)]
pub use callbacks::dispatch;
#[cfg(windows)]
pub use native::{AsyncReturn, RegistrationDocs, RegistrationInfo, XllHost};
/// Owning-STA RTD service; worksheet results require admitted input reads. A topic is Excel's
/// identity (ProgID, server, strings): the service shares one connection among every caller.
#[cfg(windows)]
pub trait RtdService {
    /// Join (or start) the connection of `topic` for `caller`. Returns its lease: the
    /// connection incarnation, never reused.
    fn subscribe(
        &self,
        caller: calc_value::CellKey,
        topic: &calc_func::TopicKey,
    ) -> Result<calc_func::RtdLease, String>;
    /// The exact ConnectData observation while it still awaits owner admission.
    /// This is a provisional C API return, not a certified worksheet input. The
    /// context still records its old/missing cut and normal admission invalidates it.
    fn initial_value(
        &self,
        _topic: &calc_func::TopicKey,
        _lease: calc_func::RtdLease,
    ) -> Option<(u64, calc_value::CalcValue)> {
        None
    }
    fn close(&self) -> Result<(), String>;
    fn subscriptions(&self, caller: calc_value::CellKey) -> Vec<calc_func::RtdLease>;
    /// Recover only the inputs of this completed call when native code returns
    /// from its own cache without another xlfRtd callback. New leases
    /// still need an ordinary admitted input read before publishing that result.
    fn cached_call_inputs(
        &self,
        _provider: RtdProvider,
        _caller: calc_value::CellKey,
        _arguments: &[calc_value::CalcValue],
    ) -> Result<Vec<(calc_func::TopicKey, calc_func::RtdLease)>, String> {
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
        _declared: &[calc_func::RtdLease],
        _inputs: &[calc_func::RtdLease],
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

#[cfg(all(test, not(windows), not(target_arch = "wasm32")))]
mod tests {
    use calc_xll_abi::{FAILED, INVALID_COUNT, INVALID_OPER, XlOper12};

    /// Native-only: exercises the C API operand envelope on a thread with no active frame.
    #[test]
    fn dispatch_validates_the_operand_envelope_before_requiring_a_frame() {
        let mut output = XlOper12::number(94.25);
        let args = [std::ptr::null_mut()];

        for (count, operands, expected) in [
            (-1, std::ptr::null(), INVALID_COUNT),
            (256, std::ptr::null(), INVALID_COUNT),
            (1, std::ptr::null(), INVALID_OPER),
            (1, args.as_ptr(), INVALID_OPER),
            (0, std::ptr::null(), FAILED),
        ] {
            assert_eq!(
                unsafe { crate::dispatch(4, count, operands, &mut output) },
                expected
            );
            assert_eq!(output.xltype, calc_xll_abi::XL_NUM);
            assert_eq!(
                unsafe { output.value.number }.to_bits(),
                94.25_f64.to_bits()
            );
        }
    }

    /// Native-only: checks the exact XLOPER12 result write shared with the Windows callback.
    #[test]
    fn callback_replies_preserve_status_and_operand_write_rules() {
        use crate::callback_abi::{CallbackReply, ResultWrite};
        use calc_xll_abi::{INVALID_FUNCTION, NOT_THREAD_SAFE, SUCCESS, XL_ERR, XL_NUM};

        let mut output = XlOper12::number(94.25);
        for status in [INVALID_COUNT, INVALID_OPER] {
            assert_eq!(
                unsafe { CallbackReply::status(status).write_result(&mut output) },
                status
            );
            assert_eq!(output.xltype, XL_NUM);
            assert_eq!(
                unsafe { output.value.number }.to_bits(),
                94.25_f64.to_bits()
            );
        }
        for status in [INVALID_FUNCTION, FAILED, NOT_THREAD_SAFE] {
            let reply = CallbackReply::status(status);
            assert!(reply.refusal.is_none());
            assert_eq!(unsafe { reply.write_result(&mut output) }, status);
            assert_eq!(output.xltype, XL_ERR);
            assert_eq!(unsafe { output.value.integer }, 15);
        }
        let reply = CallbackReply::operand(XlOper12::number(7.5));
        assert_eq!(unsafe { reply.write_result(&mut output) }, SUCCESS);
        assert_eq!(output.xltype, XL_NUM);
        assert_eq!(unsafe { output.value.number }.to_bits(), 7.5_f64.to_bits());

        let reply = CallbackReply {
            status: SUCCESS,
            write: ResultWrite::ZeroAll,
            refusal: None,
        };
        assert_eq!(unsafe { reply.write_result(&mut output) }, SUCCESS);
        let bytes = unsafe {
            std::slice::from_raw_parts(
                std::ptr::from_ref(&output).cast::<u8>(),
                std::mem::size_of::<XlOper12>(),
            )
        };
        assert!(bytes.iter().all(|&byte| byte == 0));
    }
}
