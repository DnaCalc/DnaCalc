
use core::arch::asm;

const CW_CORE: u16 = 0x133F;

/// Public ISA FPATAN operation, selected by W111 black-box complex-argument
/// witnesses. No binary provenance inference is needed for this arithmetic graph.
pub(super) fn atan(x: f64) -> f64 {
    let mut result = 0.0_f64;
    let cw_core = CW_CORE;
    let mut cw_save = 0_u16;
    // SAFETY: the two pushes and FPATAN/store pops balance the x87 stack. All
    // pointers refer to live locals and the caller's control word is restored.
    unsafe {
        asm!(
            "fnstcw word ptr [{save}]",
            "fldcw word ptr [{core}]",
            "fld qword ptr [{x}]",
            "fld1",
            "fpatan",
            "fstp qword ptr [{res}]",
            "fldcw word ptr [{save}]",
            x = in(reg) &x,
            res = in(reg) &mut result,
            core = in(reg) &cw_core,
            save = in(reg) &mut cw_save,
            options(nostack, preserves_flags),
        );
    }
    result
}

/// W111 ATAN magnitude reduction selected by independent black-box counterexamples.
/// For |x| > 1, keep atan(1/|x|) and pi/2 extended until the final subtraction.
/// FPATAN takes the two operands directly, so no rounded reciprocal is introduced.
pub(super) fn atan_reduced(x: f64) -> f64 {
    let magnitude = x.abs();
    if magnitude <= 1.0 { return atan(x); }
    let mut result = 0.0_f64;
    let mut cw_save = 0_u16;
    let cw_core = CW_CORE;
    let two = 2.0_f64;
    // SAFETY: all operands are live locals; the two FPATAN operands reduce to
    // one angle, FLDPI adds one value, and subtraction/store empty the stack.
    // The caller's control word is restored before returning.
    unsafe {
        asm!(
            "fnstcw word ptr [{save}]", "fldcw word ptr [{core}]",
            "fld1", "fld qword ptr [{x}]", "fpatan",
            "fldpi", "fdiv qword ptr [{two}]", "fsubrp st(1), st(0)",
            "fstp qword ptr [{res}]", "fldcw word ptr [{save}]",
            x = in(reg) &magnitude, two = in(reg) &two, res = in(reg) &mut result,
            core = in(reg) &cw_core, save = in(reg) &mut cw_save,
            options(nostack, preserves_flags),
        );
    }
    result.copysign(x)
}

// Restricted historical implementation removed (dc-96l.7).

/// `RN53(RN64(a * b))` — one x87 `FMUL` at PC=64 then a `binary64` store: the
/// **double-rounded** product. Excel forms `POWER`'s `|y|·ln x` this way; a plain
/// SSE multiply is single-rounded and differs on double-rounding-window inputs.
pub(super) fn mul(a: f64, b: f64) -> f64 {
    let mut result: f64 = 0.0;
    let cw_core: u16 = CW_CORE;
    let mut cw_save: u16 = 0;
    // SAFETY: balanced x87 stack (2 push; `fmulp` and `fstp` pop both), pointer
    // I/O only, control word saved and restored.
    unsafe {
        asm!(
            "fnstcw word ptr [{save}]",
            "fldcw word ptr [{core}]",
            "fld qword ptr [{a}]",     // st0 = a
            "fld qword ptr [{b}]",     // st0 = b, st1 = a
            "fmulp st(1), st",         // st0 = RN64(a*b)
            "fstp qword ptr [{res}]",  // RN53 store
            "fldcw word ptr [{save}]",
            a = in(reg) &a,
            b = in(reg) &b,
            res = in(reg) &mut result,
            core = in(reg) &cw_core,
            save = in(reg) &mut cw_save,
        );
    }
    result
}

/// `RN53(RN64(a + b))` — one x87 `FADD` at PC=64 then a `binary64` store: the
/// double-rounded sum. Excel's legacy x87-compiled function bodies produce
/// this for every `t = a + b;` with `t` spilled to memory (W109: XNPV's
/// `1 + rate` and its running `total += term` accumulate this way).
pub(super) fn add(a: f64, b: f64) -> f64 {
    let mut result: f64 = 0.0;
    let cw_core: u16 = CW_CORE;
    let mut cw_save: u16 = 0;
    // SAFETY: balanced x87 stack (one push; `fadd` folds in memory operand,
    // `fstp` pops), pointer I/O only, control word saved and restored.
    unsafe {
        asm!(
            "fnstcw word ptr [{save}]",
            "fldcw word ptr [{core}]",
            "fld qword ptr [{a}]",     // st0 = a
            "fadd qword ptr [{b}]",    // st0 = RN64(a+b)
            "fstp qword ptr [{res}]",  // RN53 store
            "fldcw word ptr [{save}]",
            a = in(reg) &a,
            b = in(reg) &b,
            res = in(reg) &mut result,
            core = in(reg) &cw_core,
            save = in(reg) &mut cw_save,
        );
    }
    result
}

/// `RN53(RN64(a - b))` — one x87 `FSUB` at PC=64 then a `binary64` store: the
/// double-rounded difference (W109: NPER's `tf·pmt - fv·rate`).
pub(super) fn sub(a: f64, b: f64) -> f64 {
    let mut result: f64 = 0.0;
    let cw_core: u16 = CW_CORE;
    let mut cw_save: u16 = 0;
    // SAFETY: balanced x87 stack (one push; `fsub` folds in memory operand,
    // `fstp` pops), pointer I/O only, control word saved and restored.
    unsafe {
        asm!(
            "fnstcw word ptr [{save}]",
            "fldcw word ptr [{core}]",
            "fld qword ptr [{a}]",     // st0 = a
            "fsub qword ptr [{b}]",    // st0 = RN64(a-b)
            "fstp qword ptr [{res}]",  // RN53 store
            "fldcw word ptr [{save}]",
            a = in(reg) &a,
            b = in(reg) &b,
            res = in(reg) &mut result,
            core = in(reg) &cw_core,
            save = in(reg) &mut cw_save,
        );
    }
    result
}

/// `RN53(RN64(a / b))` — one x87 `FDIV` at PC=64 then a `binary64` store: the
/// double-rounded quotient (W109: XNPV's per-term `value / pow`).
pub(super) fn div(a: f64, b: f64) -> f64 {
    let mut result: f64 = 0.0;
    let cw_core: u16 = CW_CORE;
    let mut cw_save: u16 = 0;
    // SAFETY: balanced x87 stack (one push; `fdiv` folds in memory operand,
    // `fstp` pops), pointer I/O only, control word saved and restored.
    unsafe {
        asm!(
            "fnstcw word ptr [{save}]",
            "fldcw word ptr [{core}]",
            "fld qword ptr [{a}]",     // st0 = a
            "fdiv qword ptr [{b}]",    // st0 = RN64(a/b)
            "fstp qword ptr [{res}]",  // RN53 store
            "fldcw word ptr [{save}]",
            a = in(reg) &a,
            b = in(reg) &b,
            res = in(reg) &mut result,
            core = in(reg) &cw_core,
            save = in(reg) &mut cw_save,
        );
    }
    result
}

/// `RN53(RN64(1.0 / x))` — one x87 `FDIV` at PC=64 then a `binary64` store: the
/// double-rounded reciprocal Excel uses to stage `POWER(x, y)` for `y < 0`
/// (compute the positive power, store it, then reciprocate).
pub(super) fn recip(x: f64) -> f64 {
    let mut result: f64 = 0.0;
    let cw_core: u16 = CW_CORE;
    let mut cw_save: u16 = 0;
    // SAFETY: balanced x87 stack (`fld1` pushes, `fstp` pops), pointer I/O only.
    unsafe {
        asm!(
            "fnstcw word ptr [{save}]",
            "fldcw word ptr [{core}]",
            "fld1",                    // st0 = 1
            "fdiv qword ptr [{x}]",    // st0 = RN64(1/x)
            "fstp qword ptr [{res}]",  // RN53 store
            "fldcw word ptr [{save}]",
            x = in(reg) &x,
            res = in(reg) &mut result,
            core = in(reg) &cw_core,
            save = in(reg) &mut cw_save,
        );
    }
    result
}

// ============================================================================
// W109 research primitives (`research-x87` feature)
//
// Raw x87 instruction wrappers for the calculation-graph search tooling. The
// production Excel-parity chains above stay untouched; everything below is a
// composable instruction set the racer assembles into *candidate* graphs.
//
// The core type is [`raw::Ext80`]: an 80-bit extended-precision temporary held
// in memory via `fld tbyte`/`fstp tbyte`, which preserve the full register
// image (no rounding). Chaining ops through `Ext80` is therefore bit-identical
// to keeping the value on the x87 register stack, so a candidate graph can mix
// "x87-continuous" stretches (Ext80 all the way) with explicit `binary64`
// store barriers (`ext_to_f64` then `ext_from_f64`) at any node boundary —
// exactly the store-mask search axis. Every arithmetic op takes the control
// word explicitly so precision-control hypotheses (PC=64/53/24) are searchable
// too.
// ============================================================================
pub(super) mod raw {
    use core::arch::asm;

    pub const CW_PC64_RN: u16 = 0x133F;
    /// PC=53 (double), round-to-nearest-even, all exceptions masked.
    pub const CW_PC53_RN: u16 = 0x123F;
    /// PC=24 (single), round-to-nearest-even, all exceptions masked.
    pub const CW_PC24_RN: u16 = 0x103F;

    /// The extended value's memory image, shared with the software x87 (`fld tbyte` /
    /// `fstp tbyte` move the full register image with no rounding).
    pub use super::super::soft87::Ext80;

    /// `f64 -> Ext80` — exact (every binary64 is representable in extended).
    pub fn ext_from_f64(x: f64) -> Ext80 {
        let mut out = Ext80([0u8; 10]);
        // SAFETY: one push, one pop; pointer I/O only. No CW dependence: the
        // widening load is exact under every rounding/precision mode.
        unsafe {
            asm!(
                "fld qword ptr [{x}]",
                "fstp tbyte ptr [{out}]",
                x = in(reg) &x,
                out = in(reg) out.0.as_mut_ptr(),
            );
        }
        out
    }

    /// `Ext80 -> f64` — the explicit `binary64` store barrier (`RN53` under the
    /// given control word's rounding mode; precision control is irrelevant to
    /// stores).
    pub fn ext_to_f64(x: &Ext80, cw: u16) -> f64 {
        let mut result: f64 = 0.0;
        let mut cw_save: u16 = 0;
        // SAFETY: one push, one pop; pointer I/O only; CW saved and restored.
        unsafe {
            asm!(
                "fnstcw word ptr [{save}]",
                "fldcw word ptr [{cw}]",
                "fld tbyte ptr [{x}]",
                "fstp qword ptr [{res}]",
                x = in(reg) x.0.as_ptr(),
                res = in(reg) &mut result,
                cw = in(reg) &cw,
                save = in(reg) &mut cw_save,
            );
        }
        result
    }

    macro_rules! ext_binop {
        ($(#[$m:meta])* $name:ident, $inst:literal) => {
            $(#[$m])*
            pub fn $name(a: &Ext80, b: &Ext80, cw: u16) -> Ext80 {
                let mut out = Ext80([0u8; 10]);
                let mut cw_save: u16 = 0;
                // SAFETY: two pushes, both popped ($inst pops one, the tbyte
                // store pops the other); pointer I/O only; CW saved/restored.
                unsafe {
                    asm!(
                        "fnstcw word ptr [{save}]",
                        "fldcw word ptr [{cw}]",
                        "fld tbyte ptr [{a}]",     // st0 = a
                        "fld tbyte ptr [{b}]",     // st0 = b, st1 = a
                        $inst,                     // st0 = a <op> b
                        "fstp tbyte ptr [{out}]",
                        "fldcw word ptr [{save}]",
                        a = in(reg) a.0.as_ptr(),
                        b = in(reg) b.0.as_ptr(),
                        out = in(reg) out.0.as_mut_ptr(),
                        cw = in(reg) &cw,
                        save = in(reg) &mut cw_save,
                    );
                }
                out
            }
        };
    }

    ext_binop!(
        /// `a + b` rounded per the control word's precision/rounding fields.
        ext_add, "faddp st(1), st"
    );
    ext_binop!(
        /// `a - b`.
        ext_sub, "fsubp st(1), st"
    );
    ext_binop!(
        /// `a * b`.
        ext_mul, "fmulp st(1), st"
    );
    ext_binop!(
        /// `a / b`.
        ext_div, "fdivp st(1), st"
    );

    macro_rules! ext_unop {
        ($(#[$m:meta])* $name:ident, $inst:literal) => {
            $(#[$m])*
            pub fn $name(x: &Ext80, cw: u16) -> Ext80 {
                let mut out = Ext80([0u8; 10]);
                let mut cw_save: u16 = 0;
                // SAFETY: one push, one pop; pointer I/O only; CW saved/restored.
                unsafe {
                    asm!(
                        "fnstcw word ptr [{save}]",
                        "fldcw word ptr [{cw}]",
                        "fld tbyte ptr [{x}]",
                        $inst,
                        "fstp tbyte ptr [{out}]",
                        "fldcw word ptr [{save}]",
                        x = in(reg) x.0.as_ptr(),
                        out = in(reg) out.0.as_mut_ptr(),
                        cw = in(reg) &cw,
                        save = in(reg) &mut cw_save,
                    );
                }
                out
            }
        };
    }

    ext_unop!(
        /// `|x|`.
        ext_abs, "fabs"
    );
    ext_unop!(
        /// `-x`.
        ext_chs, "fchs"
    );
    ext_unop!(
        /// `sqrt(x)` (x87 `FSQRT`, correctly rounded to the CW precision).
        ext_sqrt, "fsqrt"
    );
    ext_unop!(
        /// `rint(x)` per the CW rounding mode (x87 `FRNDINT`).
        ext_rndint, "frndint"
    );
    ext_unop!(
        /// `sin(x)` — x87 `FSIN` microcode, hardware 66-bit-π internal
        /// reduction. Caller must keep `|x| < 2^63` (C2 no-op otherwise).
        ext_sin, "fsin"
    );
    ext_unop!(
        /// `cos(x)` — x87 `FCOS`. Same `|x| < 2^63` caveat as [`ext_sin`].
        ext_cos, "fcos"
    );
    ext_unop!(
        /// `2^x - 1` — x87 `F2XM1`; requires `|x| <= 1` (else undefined).
        ext_f2xm1, "f2xm1"
    );

    /// `tan(x)` — x87 `FPTAN` microcode (pushes 1.0, which is popped here).
    /// Caller must keep `|x| < 2^63`.
    pub fn ext_tan(x: &Ext80, cw: u16) -> Ext80 {
        let mut out = Ext80([0u8; 10]);
        let mut cw_save: u16 = 0;
        // SAFETY: fptan pushes a 1.0 above the result; both are popped
        // (fstp st(0) drops the 1.0, the tbyte store pops the tangent).
        unsafe {
            asm!(
                "fnstcw word ptr [{save}]",
                "fldcw word ptr [{cw}]",
                "fld tbyte ptr [{x}]",     // st0 = x
                "fptan",                   // st0 = 1.0, st1 = tan(x)
                "fstp st(0)",              // drop the pushed 1.0
                "fstp tbyte ptr [{out}]",
                "fldcw word ptr [{save}]",
                x = in(reg) x.0.as_ptr(),
                out = in(reg) out.0.as_mut_ptr(),
                cw = in(reg) &cw,
                save = in(reg) &mut cw_save,
            );
        }
        out
    }

    /// `y * log2(x)` — x87 `FYL2X` microcode. `x` must be finite and `> 0`.
    pub fn ext_fyl2x(y: &Ext80, x: &Ext80, cw: u16) -> Ext80 {
        let mut out = Ext80([0u8; 10]);
        let mut cw_save: u16 = 0;
        // SAFETY: two pushes; fyl2x pops one, the tbyte store pops the other.
        unsafe {
            asm!(
                "fnstcw word ptr [{save}]",
                "fldcw word ptr [{cw}]",
                "fld tbyte ptr [{y}]",     // st0 = y
                "fld tbyte ptr [{x}]",     // st0 = x, st1 = y
                "fyl2x",                   // st0 = y*log2(x), pop
                "fstp tbyte ptr [{out}]",
                "fldcw word ptr [{save}]",
                y = in(reg) y.0.as_ptr(),
                x = in(reg) x.0.as_ptr(),
                out = in(reg) out.0.as_mut_ptr(),
                cw = in(reg) &cw,
                save = in(reg) &mut cw_save,
            );
        }
        out
    }

    /// `y * log2(1 + x)` — x87 `FYL2XP1` microcode; requires
    /// `|x| < 1 - sqrt(2)/2` (else undefined).
    pub fn ext_fyl2xp1(y: &Ext80, x: &Ext80, cw: u16) -> Ext80 {
        let mut out = Ext80([0u8; 10]);
        let mut cw_save: u16 = 0;
        // SAFETY: as in ext_fyl2x.
        unsafe {
            asm!(
                "fnstcw word ptr [{save}]",
                "fldcw word ptr [{cw}]",
                "fld tbyte ptr [{y}]",
                "fld tbyte ptr [{x}]",
                "fyl2xp1",
                "fstp tbyte ptr [{out}]",
                "fldcw word ptr [{save}]",
                y = in(reg) y.0.as_ptr(),
                x = in(reg) x.0.as_ptr(),
                out = in(reg) out.0.as_mut_ptr(),
                cw = in(reg) &cw,
                save = in(reg) &mut cw_save,
            );
        }
        out
    }

    /// The angle of `(x, y)`, `atan(y / x)` in the operands' quadrant — x87 `FPATAN`.
    pub fn ext_fpatan(y: &Ext80, x: &Ext80, cw: u16) -> Ext80 {
        let mut out = Ext80([0u8; 10]);
        let mut cw_save: u16 = 0;
        // SAFETY: two pushes; fpatan pops one, the tbyte store pops the other.
        unsafe {
            asm!(
                "fnstcw word ptr [{save}]",
                "fldcw word ptr [{cw}]",
                "fld tbyte ptr [{y}]",
                "fld tbyte ptr [{x}]",
                "fpatan",
                "fstp tbyte ptr [{out}]",
                "fldcw word ptr [{save}]",
                y = in(reg) y.0.as_ptr(),
                x = in(reg) x.0.as_ptr(),
                out = in(reg) out.0.as_mut_ptr(),
                cw = in(reg) &cw,
                save = in(reg) &mut cw_save,
            );
        }
        out
    }

    /// `x * 2^trunc(k)` — x87 `FSCALE`.
    pub fn ext_scale(x: &Ext80, k: &Ext80, cw: u16) -> Ext80 {
        let mut out = Ext80([0u8; 10]);
        let mut cw_save: u16 = 0;
        // SAFETY: two pushes; fscale leaves both, the store pops st0 and the
        // trailing fstp drops k.
        unsafe {
            asm!(
                "fnstcw word ptr [{save}]",
                "fldcw word ptr [{cw}]",
                "fld tbyte ptr [{k}]",     // st0 = k
                "fld tbyte ptr [{x}]",     // st0 = x, st1 = k
                "fscale",                  // st0 = x*2^trunc(k), st1 = k
                "fstp tbyte ptr [{out}]",
                "fstp st(0)",              // drop k
                "fldcw word ptr [{save}]",
                x = in(reg) x.0.as_ptr(),
                k = in(reg) k.0.as_ptr(),
                out = in(reg) out.0.as_mut_ptr(),
                cw = in(reg) &cw,
                save = in(reg) &mut cw_save,
            );
        }
        out
    }

    macro_rules! ext_prem {
        ($(#[$m:meta])* $name:ident, $inst:literal) => {
            $(#[$m])*
            pub fn $name(x: &Ext80, modulus: &Ext80, cw: u16) -> Ext80 {
                let mut out = Ext80([0u8; 10]);
                let mut cw_save: u16 = 0;
                // SAFETY: two pushes, both popped; the reduction loop repeats
                // the instruction until status C2 clears (complete reduction),
                // as the ISA requires for widely separated exponents. Clobbers
                // ax (fnstsw) and flags (test).
                unsafe {
                    asm!(
                        "fnstcw word ptr [{save}]",
                        "fldcw word ptr [{cw}]",
                        "fld tbyte ptr [{m}]",     // st0 = modulus
                        "fld tbyte ptr [{x}]",     // st0 = x, st1 = modulus
                        "2:",
                        $inst,                     // st0 = partial remainder
                        "fnstsw ax",
                        "test ah, 4",              // C2 (status bit 10) set -> incomplete
                        "jnz 2b",
                        "fstp tbyte ptr [{out}]",
                        "fstp st(0)",              // drop modulus
                        "fldcw word ptr [{save}]",
                        x = in(reg) x.0.as_ptr(),
                        m = in(reg) modulus.0.as_ptr(),
                        out = in(reg) out.0.as_mut_ptr(),
                        cw = in(reg) &cw,
                        save = in(reg) &mut cw_save,
                        out("ax") _,
                    );
                }
                out
            }
        };
    }

    ext_prem!(
        /// Truncating partial remainder `x rem_trunc modulus` — x87 `FPREM`
        /// (8087-compatible, quotient toward zero), iterated to completion.
        ext_prem, "fprem"
    );
    ext_prem!(
        /// IEEE remainder `x rem_nearest modulus` — x87 `FPREM1` (quotient
        /// rounded to nearest-even), iterated to completion.
        ext_prem1, "fprem1"
    );

    /// [`ext_prem1`] that also returns the low three quotient bits the ISA
    /// leaves in the status flags (bit0 = Q0 from C1, bit1 = Q1 from C3,
    /// bit2 = Q2 from C0) — the parity the legacy CRT trig chains use for the
    /// `sin(x) = (-1)^Q sin(r)` sign fixup after reducing by π.
    pub fn ext_prem1_quo(x: &Ext80, modulus: &Ext80, cw: u16) -> (Ext80, u8) {
        let mut out = Ext80([0u8; 10]);
        let mut cw_save: u16 = 0;
        let mut status: u16 = 0;
        // SAFETY: as in ext_prem1; additionally stores the final status word.
        unsafe {
            asm!(
                "fnstcw word ptr [{save}]",
                "fldcw word ptr [{cw}]",
                "fld tbyte ptr [{m}]",
                "fld tbyte ptr [{x}]",
                "2:",
                "fprem1",
                "fnstsw ax",
                "test ah, 4",
                "jnz 2b",
                "mov word ptr [{st}], ax",
                "fstp tbyte ptr [{out}]",
                "fstp st(0)",
                "fldcw word ptr [{save}]",
                x = in(reg) x.0.as_ptr(),
                m = in(reg) modulus.0.as_ptr(),
                out = in(reg) out.0.as_mut_ptr(),
                st = in(reg) &mut status,
                cw = in(reg) &cw,
                save = in(reg) &mut cw_save,
                out("ax") _,
            );
        }
        // Status: C0 = bit 8, C1 = bit 9, C3 = bit 14.
        let q0 = ((status >> 9) & 1) as u8;
        let q1 = ((status >> 14) & 1) as u8;
        let q2 = ((status >> 8) & 1) as u8;
        (out, q0 | (q1 << 1) | (q2 << 2))
    }

    macro_rules! ext_const {
        ($(#[$m:meta])* $name:ident, $inst:literal) => {
            $(#[$m])*
            pub fn $name() -> Ext80 {
                let mut out = Ext80([0u8; 10]);
                let mut cw_save: u16 = 0;
                let cw: u16 = CW_PC64_RN;
                // SAFETY: one push, one pop; CW pinned to PC=64/RN so the ROM
                // constant rounds exactly as in Excel's chains.
                unsafe {
                    asm!(
                        "fnstcw word ptr [{save}]",
                        "fldcw word ptr [{cw}]",
                        $inst,
                        "fstp tbyte ptr [{out}]",
                        "fldcw word ptr [{save}]",
                        out = in(reg) out.0.as_mut_ptr(),
                        cw = in(reg) &cw,
                        save = in(reg) &mut cw_save,
                    );
                }
                out
            }
        };
    }

    ext_const!(
        /// π as the x87 ROM constant (`FLDPI`, 66-bit source rounded to the
        /// 64-bit significand under RN) — the internal-reduction π candidate.
        ext_pi, "fldpi"
    );
    ext_const!(
        /// `log2(e)` ROM constant (`FLDL2E`).
        ext_l2e, "fldl2e"
    );
    ext_const!(
        /// `log2(10)` ROM constant (`FLDL2T`).
        ext_l2t, "fldl2t"
    );
    ext_const!(
        /// `ln 2` ROM constant (`FLDLN2`).
        ext_ln2, "fldln2"
    );
    ext_const!(
        /// `log10(2)` ROM constant (`FLDLG2`).
        ext_lg2, "fldlg2"
    );
    ext_const!(
        /// `1.0` (`FLD1`).
        ext_one, "fld1"
    );
}

/// Native-only: the x87 assembly these tests run compiles only on x86_64.
#[cfg(test)]
mod tests {
    use super::*;

    /// Full offline regression: every EXP/LN witness harvested from live Excel
    /// (see the module docs for provenance). x87 must reproduce Excel bit-for-bit.
    ///
    /// Host-CPU note: the ~1-in-30 hardest near-midpoint rows are `F2XM1`/`FYL2X`
    /// microcode-bound, so this pins Excel-parity *for the reference x86-64 host*
    /// (AMD Zen2). A failure on a different x87 microcode is the documented
    /// CPU-dependence caveat, not a regression in this code.
    const GROUND_TRUTH: &str = include_str!("x87_excel_ground_truth.tsv");

    fn parse(hex: &str) -> u64 {
        u64::from_str_radix(hex, 16).expect("hex")
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn matches_live_excel_bit_for_bit() {
        let (mut n, mut bad) = (0u32, Vec::new());
        for line in GROUND_TRUTH.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let mut it = line.split('\t');
            let func = it.next().unwrap();
            let xb = parse(it.next().unwrap());
            let exb = parse(it.next().unwrap());
            let x = f64::from_bits(xb);
            let got = match func {
                "EXP" => exp(x),
                "LN" => ln(x),
                _ => continue,
            }
            .to_bits();
            n += 1;
            if got != exb && bad.len() < 20 {
                bad.push(format!(
                    "{} x={:016x}: got {:016x} want {:016x}",
                    func, xb, got, exb
                ));
            } else if got != exb {
                bad.push("...".into());
            }
        }
        assert!(n >= 200, "expected the full corpus, saw {n} rows");
        assert!(
            bad.is_empty(),
            "{} mismatches:\n{}",
            bad.len(),
            bad.join("\n")
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn spot_values() {
        // exp(0)=1, exp(1)=E (both exact-agreeing with Excel and CR here).
        assert_eq!(exp(0.0).to_bits(), 1.0f64.to_bits());
        assert_eq!(exp(1.0).to_bits(), std::f64::consts::E.to_bits());
        // ln(1)=+0 (sign of zero), ln(E)=1, log10(1000)=3.
        assert_eq!(ln(1.0).to_bits(), 0.0f64.to_bits());
        assert_eq!(ln(std::f64::consts::E), 1.0);
        assert_eq!(log10(1000.0), 3.0);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn double_rounded_mul_and_recip() {
        // Exact cases.
        assert_eq!(mul(2.0, 3.0), 6.0);
        assert_eq!(recip(2.0).to_bits(), 0.5f64.to_bits());
        assert_eq!(recip(0.5), 2.0);
        // Double-rounding discriminator (POWER_LIVE_ROUND1): p = 0x3fffffffffffffff
        // (~1.9999999999999998). RN64(1/p) = 0.5 + 2^-54, then RN53 ties to even
        // -> exactly 0.5. A single-rounded SSE `1.0/p` gives 0x3fe0000000000001.
        let p = f64::from_bits(0x3fffffffffffffff);
        assert_eq!(recip(p).to_bits(), 0.5f64.to_bits());
        assert_ne!((1.0 / p).to_bits(), 0.5f64.to_bits());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn control_word_is_restored() {
        // A benign x87 op after our calls must see the caller's CW again; we
        // can at least confirm repeated calls stay stable and don't drift.
        let a = exp(2.0);
        let _ = ln(10.0);
        let b = exp(2.0);
        assert_eq!(a.to_bits(), b.to_bits());
    }
}
