//! Excel 12 ABI facts, written independently from documented layouts. No SDK source.
//! All pointer traversal is unsafe: an in-process XLL is trusted native code.
#[cfg(target_arch = "wasm32")]
compile_error!("calc-xll-abi is native-only");

mod signature;
mod value;
pub use signature::{XllSignature, XllType};
pub use value::{Arena, SheetMap, from_oper};

pub const XL_NUM: u32 = 1;
pub const XL_STR: u32 = 2;
pub const XL_BOOL: u32 = 4;
pub const XL_REF: u32 = 8;
pub const XL_ERR: u32 = 16;
pub const XL_FLOW: u32 = 32;
pub const XL_MULTI: u32 = 64;
pub const XL_MISSING: u32 = 128;
pub const XL_NIL: u32 = 256;
pub const XL_SREF: u32 = 1024;
pub const XL_INT: u32 = 2048;
pub const XL_BIG_DATA: u32 = XL_STR | XL_INT;
pub const XL_FREE: u32 = 0x1000;
pub const DLL_FREE: u32 = 0x4000;
pub const TYPE_MASK: u32 = 0x0fff;

/// Callback return codes: the published set, exact integers, never OR-ed together.
pub const SUCCESS: i32 = 0;
pub const ABORT: i32 = 1;
pub const INVALID_FUNCTION: i32 = 2;
pub const INVALID_COUNT: i32 = 4;
pub const INVALID_OPER: i32 = 8;
pub const STACK_OVERFLOW: i32 = 16;
pub const FAILED: i32 = 32;
pub const UNCALCED: i32 = 64;
pub const NOT_THREAD_SAFE: i32 = 128;
pub const INVALID_ASYNC: i32 = 256;
pub const NOT_CLUSTER_SAFE: i32 = 512;

/// Function-number flag bits: command (Cetab index), special (host service), international
/// (`0x2000`) and prompt. A callback number keeps them; nothing strips them silently.
pub const XL_COMMAND: i32 = 0x8000;
pub const XL_SPECIAL: i32 = 0x4000;
pub const XL_INTL: i32 = 0x2000;
pub const XL_PROMPT: i32 = 0x1000;

pub const XLF_REGISTER: i32 = 149;
pub const XLF_CALL: i32 = 150;
pub const XLF_REGISTER_ID: i32 = 267;
pub const XLF_UNREGISTER: i32 = 201;
pub const XLF_SET_NAME: i32 = 88;
pub const XLF_CALLER: i32 = 89;
pub const XLF_EVALUATE: i32 = 257;
pub const XLF_REFTEXT: i32 = 146;
pub const XLF_GET_WORKSPACE: i32 = 186;
pub const XLF_VOLATILE: i32 = 237;
pub const XLF_RTD: i32 = 379;
pub const XLC_MESSAGE: i32 = XL_COMMAND | 122;
pub const XL_UDF: i32 = 255;
pub const XL_FREE_FN: i32 = 16384;
pub const XL_STACK: i32 = 16385;
pub const XL_COERCE: i32 = 16386;
pub const XL_SET: i32 = 16387;
pub const XL_SHEET_ID: i32 = 16388;
pub const XL_SHEET_NM: i32 = 16389;
pub const XL_ABORT: i32 = 16390;
pub const XL_GET_INST: i32 = 16391;
pub const XL_GET_HWND: i32 = 16392;
pub const XL_GET_NAME: i32 = 16393;
pub const XL_GET_FMLA_INFO: i32 = 16398;
pub const XL_ASYNC_RETURN: i32 = 16400;
pub const XL_EVENT_REGISTER: i32 = 16401;
pub const XL_RUNNING_ON_CLUSTER: i32 = 16402;
pub const XL_GET_INST_PTR: i32 = 16403;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct XlRef12 {
    pub row_first: i32,
    pub row_last: i32,
    pub col_first: i32,
    pub col_last: i32,
}

/// Flexible array: the allocation holds `count` rectangles, starting at offset four.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct XlMRef12 {
    pub count: u16,
    pub rectangles: [XlRef12; 1],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct XlSRef12 {
    pub count: u16,
    pub rectangle: XlRef12,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct XlReference12 {
    pub table: *mut XlMRef12,
    pub sheet: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct XlArray12 {
    pub values: *mut XlOper12,
    pub rows: i32,
    pub columns: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct XlBigData {
    pub data: *mut u8,
    pub byte_count: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union XlValue12 {
    pub number: f64,
    pub string: *mut u16,
    pub integer: i32,
    pub array: XlArray12,
    pub reference: XlReference12,
    pub single_reference: XlSRef12,
    pub big_data: XlBigData,
    pub storage: [u8; 24],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct XlOper12 {
    pub value: XlValue12,
    pub xltype: u32,
}

impl Default for XlOper12 {
    fn default() -> Self {
        Self {
            value: XlValue12 { storage: [0; 24] },
            xltype: XL_NIL,
        }
    }
}
impl XlOper12 {
    pub fn number(number: f64) -> Self {
        Self {
            value: XlValue12 { number },
            xltype: XL_NUM,
        }
    }
    pub fn integer(integer: i32) -> Self {
        Self {
            value: XlValue12 { integer },
            xltype: XL_INT,
        }
    }
    pub fn error(code: i32) -> Self {
        Self {
            value: XlValue12 { integer: code },
            xltype: XL_ERR,
        }
    }
    pub fn boolean(value: bool) -> Self {
        Self {
            value: XlValue12 {
                integer: i32::from(value),
            },
            xltype: XL_BOOL,
        }
    }
}

/// Flexible array: `rows * columns` doubles follow the two dimensions.
#[repr(C)]
pub struct Fp12 {
    pub rows: i32,
    pub columns: i32,
    pub values: [f64; 1],
}

pub type Callback12 =
    unsafe extern "system" fn(i32, i32, *const *mut XlOper12, *mut XlOper12) -> i32;

#[cfg(test)]
mod tests {
    use super::*;
    /// Native-only: checks the native C ABI, which is deliberately absent from wasm.
    #[test]
    fn documented_offsets_and_sizes() {
        assert_eq!(std::mem::size_of::<XlOper12>(), 32);
        assert_eq!(std::mem::align_of::<XlOper12>(), 8);
        assert_eq!(std::mem::offset_of!(XlOper12, xltype), 24);
        assert_eq!(std::mem::offset_of!(XlMRef12, rectangles), 4);
        assert_eq!(std::mem::offset_of!(XlSRef12, rectangle), 4);
        assert_eq!(std::mem::size_of::<XlRef12>(), 16);
        assert_eq!(std::mem::offset_of!(Fp12, values), 8);
        assert_eq!(std::mem::offset_of!(Fp12, columns), 4);
        assert_eq!(std::mem::size_of::<XlSRef12>(), 20);
        // XLMREF12 rectangles have a 16-byte stride after the 16-bit count.
        assert_eq!(
            std::mem::size_of::<XlMRef12>() - std::mem::offset_of!(XlMRef12, rectangles),
            16
        );
        // BigData is a composite code that contains the Str bit: no code may test `& XL_STR`
        // to recognise a string; base kinds compare with `xltype & TYPE_MASK == XL_STR`.
        assert_ne!(XL_BIG_DATA & XL_STR, 0);
        assert_ne!(XL_BIG_DATA, XL_STR);
        assert_eq!(XLC_MESSAGE, 32890);
        assert_eq!(
            [ABORT, STACK_OVERFLOW, UNCALCED, NOT_CLUSTER_SAFE],
            [1, 16, 64, 512]
        );
        #[cfg(target_pointer_width = "64")]
        {
            assert_eq!(std::mem::size_of::<XlReference12>(), 16);
            assert_eq!(std::mem::offset_of!(XlArray12, rows), 8);
            assert_eq!(std::mem::offset_of!(XlArray12, columns), 12);
            assert_eq!(std::mem::offset_of!(XlReference12, sheet), 8);
            assert_eq!(std::mem::offset_of!(XlBigData, byte_count), 8);
            assert_eq!(std::mem::size_of::<XlBigData>(), 16);
            let data = XlBigData {
                data: std::ptr::null_mut(),
                byte_count: 0,
            };
            assert_eq!(std::mem::size_of_val(&data.byte_count), 4);
        }
    }

    /// Native-only: reads the C ABI field beside deliberately nonzero structure padding.
    #[test]
    fn big_data_count_is_a_signed_32_bit_field() {
        for count in [i32::MIN, -1, 0, 1, i32::MAX] {
            let mut bytes = [0xa5u8; std::mem::size_of::<XlBigData>()];
            let offset = std::mem::offset_of!(XlBigData, byte_count);
            bytes[offset..offset + 4].copy_from_slice(&count.to_ne_bytes());
            let field = unsafe {
                std::ptr::read_unaligned(std::ptr::addr_of!(
                    (*bytes.as_ptr().cast::<XlBigData>()).byte_count
                ))
            };
            assert_eq!(field, count);
        }
    }
}
