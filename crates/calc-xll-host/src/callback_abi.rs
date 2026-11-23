//! The native operand envelope shared by C API entry points, independent of the DLL loader.

#[cfg(windows)]
use calc_xll_abi::UNCALCED;
#[cfg(any(windows, test))]
use calc_xll_abi::{FAILED, INVALID_FUNCTION, NOT_THREAD_SAFE, SUCCESS};
use calc_xll_abi::{INVALID_COUNT, INVALID_OPER, XlOper12};

/// Validate count and null pointers before consulting the active frame. The caller guarantees
/// that a nonnull argument vector is readable for the declared count.
pub(crate) unsafe fn operands(
    count: i32,
    args: *const *mut XlOper12,
) -> Result<Vec<*mut XlOper12>, i32> {
    if !(0..=255).contains(&count) {
        return Err(INVALID_COUNT);
    }
    if count > 0 && args.is_null() {
        return Err(INVALID_OPER);
    }
    let pointers: Vec<_> = (0..count as usize)
        .map(|index| unsafe { args.add(index).read_unaligned() })
        .collect();
    if pointers.iter().any(|arg| arg.is_null()) {
        return Err(INVALID_OPER);
    }
    Ok(pointers)
}

/// Validate a C API entry on an unsupported loader platform. A valid callback without an
/// active command frame returns xlretFailed and preserves the caller's result operand.
///
/// # Safety
///
/// A nonnull argument vector must be readable for `count` entries when the count is in 0..=255.
#[cfg(not(windows))]
pub unsafe fn dispatch(
    _function: i32,
    count: i32,
    args: *const *mut XlOper12,
    _result: *mut XlOper12,
) -> i32 {
    match unsafe { operands(count, args) } {
        Ok(_) => calc_xll_abi::FAILED,
        Err(status) => status,
    }
}

/// What one callback answers: its exact status, what happens to the caller's result operand,
/// and whether the calling formula becomes a typed refusal. `dispatch` applies it in one place.
#[cfg(any(windows, test))]
pub(crate) struct CallbackReply {
    pub status: i32,
    pub write: ResultWrite,
    pub refusal: Option<String>,
}
#[cfg(any(windows, test))]
pub(crate) enum ResultWrite {
    /// The result operand is not touched.
    Unchanged,
    /// All 32 bytes are zeroed (xlAsyncReturn's accepted completion, Excel20527).
    ZeroAll,
    /// A result operand built by the host; its allocation is already registered.
    Operand(XlOper12),
    /// A failed callback inside a frame: Excel 16.0.20527 writes an xltypeErr into the result
    /// operand: #VALUE! for statuses 2, 32 and 128 (x13 boundary batch, every failed row in the
    /// normal, `#`, `$` and xlAutoOpen contexts), the source's own error when xlCoerce fails on
    /// an error value (x14). Other statuses leave it untouched.
    Error(i32),
}
#[cfg(any(windows, test))]
impl CallbackReply {
    pub(crate) fn status(status: i32) -> Self {
        Self {
            status,
            write: if matches!(status, INVALID_FUNCTION | FAILED | NOT_THREAD_SAFE) {
                ResultWrite::Error(15)
            } else {
                ResultWrite::Unchanged
            },
            refusal: None,
        }
    }
    pub(crate) fn operand(oper: XlOper12) -> Self {
        Self {
            status: SUCCESS,
            write: ResultWrite::Operand(oper),
            refusal: None,
        }
    }
    /// The first status 64 of an attempt: #VALUE! in the result operand (x17), no refusal.
    #[cfg(windows)]
    pub(crate) fn uncalced() -> Self {
        Self {
            status: UNCALCED,
            write: ResultWrite::Error(15),
            refusal: None,
        }
    }
    #[cfg(windows)]
    pub(crate) fn refused(status: i32, why: String) -> Self {
        Self {
            refusal: Some(why),
            ..Self::status(status)
        }
    }
    pub(crate) unsafe fn write_result(self, result: *mut XlOper12) -> i32 {
        if !result.is_null() {
            match self.write {
                ResultWrite::Unchanged => {}
                ResultWrite::ZeroAll => unsafe {
                    std::ptr::write_bytes(result.cast::<u8>(), 0, std::mem::size_of::<XlOper12>());
                },
                ResultWrite::Operand(oper) => unsafe { result.write_unaligned(oper) },
                ResultWrite::Error(code) => unsafe {
                    result.write_unaligned(XlOper12::error(code));
                },
            }
        }
        self.status
    }
}
