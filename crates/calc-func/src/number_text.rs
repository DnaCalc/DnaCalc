//! A number's text when the & operator reads the number as text, as Excel gives it with `.` as
//! the decimal separator and no column width. Its first 15-digit rounding is also used by
//! ROUND, AMORDEGRC, _EUROTOOL.EUROCONVERT, FIXED/DOLLAR/TEXT and general formatting; their
//! formatting or rounding after that step remains separate.
//!
//! The rule, as measured in Excel 16.0 (builds 20430 and 20527) under the Windows locale en-ZA,
//! whose decimal separator `.` is en-US's (en-US itself was not measured); the text holds no
//! thousands separator:
//!
//! - Zero of either sign is `0`. A subnormal number is `0` too: Excel holds none, and a cell
//!   written as one reads back as +0.
//! - The magnitude is converted to 15 significant digits by Excel's two-branch `Q0.64` fixed-point
//!   pre-addition program (dc-b1l.25), and trailing zeros are dropped:
//!   - **Branch 1 (`1.0 <= |x| < 65536.0 = 2^16`, i.e. `0 <= exp2 < 16`):** exact `Q16.64` split
//!     into integer `w = m53 >> (52 - exp2)` (`d` digits in `1..=5`) and exact `Q0.64` fraction
//!     `f = m53 << (12 + exp2)`, pre-adding the truncated half-ulp table entry
//!     `Q64_HALF_POW10[15 - d] = floor(2^63 / 10^(15 - d))` (with `u64` carry incrementing `w` and
//!     updating `d` if `w` crosses a power of 10), then extracting `15 - d` fractional digits via
//!     `(u128::from(f) * 10) >> 64`.
//!   - **Branch 2 (`|x| < 1.0` or `|x| >= 65536.0`):** from `frexp_k = exp2 + 1`, computes
//!     `p = ceil(frexp_k / 3.3219)` (`0` at `frexp_k == 0`, matching `decimal.rs`), scales `|x|`
//!     by `10^-p` via the low-nibble-first x87 PC=64 RN-even chain (`scale_nibble_pc64`), steps
//!     once by `10^-1` (`p += 1`) when `ev = pow + 63 >= 0` (the `0.1%` slivers `[1e-146, 2^-485)`
//!     and `[1e-292, 2^-970)`), truncates right-shifted into `Q0.64` (`f = sig >> (-1 - ev)`),
//!     selects `k` (`15`, `16` for one leading zero, `17` for two leading zeros in `exp2 in
//!     {484, 969}`), pre-adds `Q64_HALF_POW10[k]`, handles `u64` carry (`10^14`, `e_out = p`),
//!     skips leading zero digits, and extracts 15 digits.
//! - When those digits written without an exponent take at most 20 characters, the sign aside,
//!   that is the text: `1000000000000000`, `99999999999999900000`, `0.000123456789012345`,
//!   `0.000000000000000001`.
//! - Otherwise the text is `d.dddE+dd`: the 15 digits are rounded again, a tie away from zero, to
//!   15 digits when the exponent of the 15-digit value is between -98 and 98 and to 14 when it is
//!   99 or -99 or beyond, trailing zeros dropped, and the exponent written with a sign and at least
//!   two digits: `1E+20`, `1.23456789012345E-05`, `1.2345678901235E+99`, `1.7976931348623E+308`.
//!   A second rounding that carries moves the exponent and keeps the digit count
//!   (9.99999999999995E-99 gives `1E-98`).
//! - A negative number's text is `-` and its magnitude's text.
//!
//! Every step uses portable integer (`u64`/`u128`) arithmetic reproducing x87 PC=64 rounding bit
//! for bit, so the text does not depend on the numeric mode or the target platform.

use crate::decimal::{ExactDecimal, Natural};
use std::cmp::Ordering;

/// Significant digits of the first rounding.
const SIGNIFICANT_DIGITS: usize = 15;

/// The longest text without an exponent, the sign aside.
const WIDTH: usize = 20;

/// 64-bit RN-even `(significand, pow2)` for `10^1..=10^15`, `10^16..=10^240` (step 16), `10^256`.
const POS_POW10: [(u64, i16); 31] = [
    (0xa000_0000_0000_0000, -60),
    (0xc800_0000_0000_0000, -57),
    (0xfa00_0000_0000_0000, -54),
    (0x9c40_0000_0000_0000, -50),
    (0xc350_0000_0000_0000, -47),
    (0xf424_0000_0000_0000, -44),
    (0x9896_8000_0000_0000, -40),
    (0xbebc_2000_0000_0000, -37),
    (0xee6b_2800_0000_0000, -34),
    (0x9502_f900_0000_0000, -30),
    (0xba43_b740_0000_0000, -27),
    (0xe8d4_a510_0000_0000, -24),
    (0x9184_e72a_0000_0000, -20),
    (0xb5e6_20f4_8000_0000, -17),
    (0xe35f_a931_a000_0000, -14),
    (0x8e1b_c9bf_0400_0000, -10),
    (0x9dc5_ada8_2b70_b59e, 43),
    (0xaf29_8d05_0e43_95d7, 96),
    (0xc278_1f49_ffcf_a6d5, 149),
    (0xd7e7_7a8f_87da_f7fc, 202),
    (0xefb3_ab16_c59b_14a3, 255),
    (0x850f_adc0_9923_329e, 309),
    (0x93ba_47c9_80e9_8ce0, 362),
    (0xa402_b9c5_a8d3_a6e7, 415),
    (0xb616_a12b_7fe6_17aa, 468),
    (0xca28_a291_859b_bf93, 521),
    (0xe070_f78d_3927_556b, 574),
    (0xf92e_0c35_3782_6146, 627),
    (0x8a52_96ff_e33c_c930, 681),
    (0x9991_a6f3_d6bf_1766, 734),
    (0xaa7e_ebfb_9df9_de8e, 787),
];

/// 64-bit RN-even `(significand, pow2)` for `10^-1..=10^-15`, `10^-16..=10^-240`, `10^-256`.
const NEG_POW10: [(u64, i16); 31] = [
    (0xcccc_cccc_cccc_cccd, -67),
    (0xa3d7_0a3d_70a3_d70a, -70),
    (0x8312_6e97_8d4f_df3b, -73),
    (0xd1b7_1758_e219_652c, -77),
    (0xa7c5_ac47_1b47_8423, -80),
    (0x8637_bd05_af6c_69b6, -83),
    (0xd6bf_94d5_e57a_42bc, -87),
    (0xabcc_7711_8461_cefd, -90),
    (0x8970_5f41_36b4_a597, -93),
    (0xdbe6_fece_bded_d5bf, -97),
    (0xafeb_ff0b_cb24_aaff, -100),
    (0x8cbc_cc09_6f50_88cc, -103),
    (0xe12e_1342_4bb4_0e13, -107),
    (0xb424_dc35_095c_d80f, -110),
    (0x901d_7cf7_3ab0_acd9, -113),
    (0xe695_94be_c44d_e15b, -117),
    (0xcfb1_1ead_4539_94ba, -170),
    (0xbb12_7c53_b17e_c159, -223),
    (0xa87f_ea27_a539_e9a5, -276),
    (0x97c5_60ba_6b09_19a6, -329),
    (0x88b4_02f7_fd75_539b, -382),
    (0xf643_35bc_f065_d37d, -436),
    (0xddd0_467c_64bc_e4a1, -489),
    (0xc7ca_ba6e_7c53_82c9, -542),
    (0xb3f4_e093_db73_a093, -595),
    (0xa217_27db_38cb_0030, -648),
    (0x91ff_8377_5423_cc06, -701),
    (0x8380_dea9_3da4_bc60, -754),
    (0xece5_3cec_4a31_4ebe, -808),
    (0xd560_5fcd_cf32_e1d7, -861),
    (0xc031_4325_637a_193a, -914),
];

/// `floor(2^63 / 10^k)` for `k = 10..=17` (`0.5 * 10^-k` truncated to Q0.64).
const Q64_HALF_POW10: [u64; 18] = [
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    922_337_203,
    92_233_720,
    9_223_372,
    922_337,
    92_233,
    9_223,
    922,
    92,
];

/// The text of a finite number, as ASCII (see the module documentation). It is always `Some`;
/// the option is kept for the callers that refuse a number without a text.
pub(crate) fn text(x: f64) -> Option<String> {
    convert(x, |_, _, _| {})
}

fn convert(x: f64, observe: impl FnOnce(FixedPointStep, &[u8], i32)) -> Option<String> {
    if !x.is_finite() {
        return None;
    }
    if x == 0.0 || x.is_subnormal() {
        return Some("0".to_owned());
    }
    let (digits, exponent, step) = first_precision(x.abs());
    observe(step, &digits, exponent);
    Some(shaped(x < 0.0, (digits, exponent)))
}

/// The shared first 15 significant digits (also used by FIXED, DOLLAR, TEXT and general
/// formatting).
pub(crate) fn decimal(x: f64) -> ExactDecimal {
    if x == 0.0 || x.is_subnormal() {
        return ExactDecimal::read(0.0);
    }
    let (digits, exponent, _) = first_precision(x.abs());
    ExactDecimal {
        negative: x < 0.0,
        coefficient: Natural::from_u64(ascii(&digits).parse().expect("at most 15 digits")),
        exponent: exponent - digits.len() as i32 + 1,
    }
}

/// The text of an Excel-qualified exponent-field-2047 overflow number (dc-j9k.57).
/// Excel 16.0.20527 runs its Q0.64 15-digit formatter on the stored exponent-2047 bits without
/// checking for NaN, and suppresses the minus sign because `NaN < 0` is false.
pub(crate) fn overflow_text(x: f64) -> String {
    debug_assert!(x.is_nan());
    let (digits, exponent, _) = first_precision_bits(x.to_bits());
    shaped(false, (digits, exponent))
}

/// The unsigned 15-significant-digit decimal of an Excel-qualified overflow number (dc-j9k.57).
pub(crate) fn overflow_decimal(x: f64) -> ExactDecimal {
    debug_assert!(x.is_nan());
    let (digits, exponent, _) = first_precision_bits(x.to_bits());
    ExactDecimal {
        negative: false,
        coefficient: Natural::from_u64(ascii(&digits).parse().expect("at most 15 digits")),
        exponent: exponent - digits.len() as i32 + 1,
    }
}

/// How the magnitude entered the shared Q0.64 rounding step.
#[derive(Clone, Copy)]
pub enum RoundingPath {
    /// The integer part was split off exactly before rounding.
    IntegerSplit {
        /// Integer part separated before rounding.
        whole: u32,
    },
    /// The x87 PC=64 scale was followed by a truncating binary shift.
    Scaled {
        /// Decimal power used to scale the magnitude.
        decimal_shift: i32,
        /// Significand after scaling.
        significand: u64,
        /// Binary shift used to truncate the scaled magnitude.
        right_shift: i32,
        /// Whether the scaled result needed a correction.
        corrected: bool,
    },
}

struct FixedPointStep {
    path: RoundingPath,
    binary_exponent: i32,
    fraction: u64,
    places: usize,
    sum: u64,
    carry: bool,
}

/// The first-precision rounding observed during a number-to-text conversion.
pub struct NumberTextRounding {
    /// Route used to obtain the fixed-point fraction.
    pub path: RoundingPath,
    /// Binary exponent of the input magnitude.
    pub binary_exponent: i32,
    /// Fraction before rounding in Q0.64 units.
    pub fraction: u64,
    /// Number of decimal places handled by this rounding step.
    pub places: usize,
    /// Half-increment used for the fixed-point rounding addition.
    pub half: u64,
    /// Low 64 bits of the rounding sum.
    pub sum: u64,
    /// Whether the fixed-point sum overflowed into its integer part.
    pub carry: bool,
    /// Input magnitude digits after truncation to first precision.
    pub truncated_digits: String,
    /// Base-ten exponent of the input's leading digit.
    pub input_exponent: i32,
    /// First discarded decimal digit.
    pub next_digit: u8,
    /// Whether the discarded tail is exactly halfway.
    pub exact_tie: bool,
    /// The rounded magnitude compared with the input truncated to 15 digits.
    pub decision: Ordering,
    /// Magnitude digits produced by first-precision rounding.
    pub rounded_digits: String,
    /// Base-ten exponent of the rounded leading digit.
    pub rounded_exponent: i32,
}

/// General number-to-text output and its rounding trace. Zero and subnormals skip rounding.
pub struct NumberTextInspection {
    /// Text returned by the worksheet number-to-text conversion.
    pub text: String,
    /// First-precision rounding details, when the conversion reaches that step.
    pub rounding: Option<NumberTextRounding>,
}

/// Inspect the same conversion used by the concatenation operator. Non-finite inputs have no text.
pub fn inspect(x: f64) -> Option<NumberTextInspection> {
    let mut rounding = None;
    let text = convert(x, |step, digits, exponent| {
        let exact = ExactDecimal::binary(x);
        let coefficient = exact.coefficient.digits();
        let input_exponent = exact.exponent + coefficient.len() as i32 - 1;
        let (truncated, rest) = coefficient.split_at(coefficient.len().min(SIGNIFICANT_DIGITS));
        let next_digit = rest.as_bytes().first().map_or(0, |digit| digit - b'0');
        let exact_tie = next_digit == 5 && rest.as_bytes()[1..].iter().all(|digit| *digit == b'0');
        let truncated_digits = format!("{truncated:0<15}");
        let rounded_digits = format!("{:0<15}", ascii(digits));
        let decision = (exponent, &rounded_digits).cmp(&(input_exponent, &truncated_digits));
        rounding = Some(NumberTextRounding {
            path: step.path,
            binary_exponent: step.binary_exponent,
            fraction: step.fraction,
            places: step.places,
            half: Q64_HALF_POW10[step.places],
            sum: step.sum,
            carry: step.carry,
            truncated_digits,
            input_exponent,
            next_digit,
            exact_tie,
            decision,
            rounded_digits,
            rounded_exponent: exponent,
        });
    })?;
    Some(NumberTextInspection { text, rounding })
}

/// 64-bit significand RN-even multiply reproducing x87 PC=64 (`CW = 0x133F`).
#[inline]
fn mul_pc64(a_sig: u64, a_pow: i32, b_sig: u64, b_pow: i32) -> (u64, i32) {
    let prod = u128::from(a_sig) * u128::from(b_sig);
    let shift: u32 = if (prod >> 127) != 0 { 64 } else { 63 };
    let top = (prod >> shift) as u64;
    let rem = prod & ((1u128 << shift) - 1);
    let half = 1u128 << (shift - 1);
    if rem > half || (rem == half && (top & 1) == 1) {
        let (inc, overflow) = top.overflowing_add(1);
        if overflow {
            return (1u64 << 63, a_pow + b_pow + shift as i32 + 1);
        }
        (inc, a_pow + b_pow + shift as i32)
    } else {
        (top, a_pow + b_pow + shift as i32)
    }
}

/// Low-nibble-first x87 PC=64 scaling by `10^shift10` (`|shift10| <= 309`).
#[inline]
fn scale_nibble_pc64(mut sig: u64, mut pow: i32, shift10: i32) -> (u64, i32) {
    let tab = if shift10 < 0 { &NEG_POW10 } else { &POS_POW10 };
    let mag = shift10.unsigned_abs();
    let d0 = (mag & 0xf) as usize;
    if d0 != 0 {
        let (fs, fp) = tab[d0 - 1];
        (sig, pow) = mul_pc64(sig, pow, fs, i32::from(fp));
    }
    let d1 = ((mag >> 4) & 0xf) as usize;
    if d1 != 0 {
        let (fs, fp) = tab[14 + d1];
        (sig, pow) = mul_pc64(sig, pow, fs, i32::from(fp));
    }
    if (mag >> 8) != 0 {
        let (fs, fp) = tab[30];
        (sig, pow) = mul_pc64(sig, pow, fs, i32::from(fp));
    }
    (sig, pow)
}

fn first_precision(x: f64) -> (Vec<u8>, i32, FixedPointStep) {
    debug_assert!(x.is_finite() && x > 0.0 && !x.is_subnormal());
    first_precision_bits(x.to_bits())
}

fn first_precision_bits(bits: u64) -> (Vec<u8>, i32, FixedPointStep) {
    let exp2 = (((bits >> 52) & 0x7ff) as i32) - 1023;
    let m53 = (bits & ((1u64 << 52) - 1)) | (1u64 << 52);
    let (mut val, e_out, step) = if (0..16).contains(&exp2) {
        let mut w = (m53 >> (52 - exp2)) as u32;
        let mut f = m53.wrapping_shl((12 + exp2) as u32);
        let digits_u32 = |n: u32| {
            if n >= 100_000 {
                6
            } else if n >= 10_000 {
                5
            } else if n >= 1_000 {
                4
            } else if n >= 100 {
                3
            } else if n >= 10 {
                2
            } else {
                1
            }
        };
        let mut d = digits_u32(w);
        let mut k = SIGNIFICANT_DIGITS - d;
        let (sum, carry) = f.overflowing_add(Q64_HALF_POW10[k]);
        let step = FixedPointStep {
            path: RoundingPath::IntegerSplit { whole: w },
            binary_exponent: exp2,
            fraction: f,
            places: k,
            sum,
            carry,
        };
        f = sum;
        if carry {
            w += 1;
            d = digits_u32(w);
            k = SIGNIFICANT_DIGITS - d;
        }
        let mut acc = u64::from(w);
        for _ in 0..k {
            let prod = u128::from(f) * 10;
            acc = acc * 10 + (prod >> 64) as u64;
            f = prod as u64;
        }
        (acc, d as i32 - 1, step)
    } else {
        let frexp_k = exp2 + 1;
        // Exact integer form of ceil(frexp_k / 3.3219), 0 at frexp_k == 0.
        // Confirmed 32/32 against Excel 16.0.20527 on exp2 in {-971, -486, +484, +969}.
        let mut p = -((-frexp_k * 10_000).div_euclid(33_219));
        let (mut sig, mut pow) = scale_nibble_pc64(m53 << 11, exp2 - 63, -p);
        let corrected = pow + 63 >= 0;
        if corrected {
            let (fs, fp) = NEG_POW10[0];
            (sig, pow) = mul_pc64(sig, pow, fs, i32::from(fp));
            p += 1;
        }
        let shr = -1 - (pow + 63);
        let mut f = if shr <= 0 {
            sig
        } else if shr >= 64 {
            0
        } else {
            sig >> (shr as u32)
        };
        let lz1 = ((u128::from(f) * 10) >> 64) == 0;
        let lz2 = lz1 && ((u128::from(f) * 100) >> 64) == 0;
        let k = if lz2 {
            17
        } else if lz1 {
            16
        } else {
            15
        };
        let (sum, carry) = f.overflowing_add(Q64_HALF_POW10[k]);
        let step = FixedPointStep {
            path: RoundingPath::Scaled {
                decimal_shift: -p,
                significand: sig,
                right_shift: shr,
                corrected,
            },
            binary_exponent: exp2,
            fraction: f,
            places: k,
            sum,
            carry,
        };
        if carry {
            (100_000_000_000_000_u64, p, step)
        } else {
            f = sum;
            let (mut acc, mut z) = (0_u64, 0_i32);
            while z < 3 {
                let pt = u128::from(f) * 10;
                let dt = (pt >> 64) as u64;
                f = pt as u64;
                if dt != 0 {
                    acc = dt;
                    break;
                }
                z += 1;
            }
            for _ in 0..14 {
                let prod = u128::from(f) * 10;
                acc = acc * 10 + (prod >> 64) as u64;
                f = prod as u64;
            }
            (acc, p - 1 - z, step)
        }
    };
    let mut buf = [0u8; SIGNIFICANT_DIGITS];
    for i in (0..SIGNIFICANT_DIGITS).rev() {
        buf[i] = b'0' + (val % 10) as u8;
        val /= 10;
    }
    let mut len = SIGNIFICANT_DIGITS;
    while len > 1 && buf[len - 1] == b'0' {
        len -= 1;
    }
    (buf[..len].to_vec(), e_out, step)
}

/// The text of the rounded digits `(digits, exponent)` of a magnitude, `-` first when `negative`.
fn shaped(negative: bool, (digits, exponent): (Vec<u8>, i32)) -> String {
    let sign = if negative { "-" } else { "" };
    let fixed = without_exponent(&digits, exponent);
    if fixed.len() <= WIDTH {
        return format!("{sign}{fixed}");
    }
    let kept = if exponent.unsigned_abs() < 99 {
        SIGNIFICANT_DIGITS
    } else {
        SIGNIFICANT_DIGITS - 1
    };
    let up = digits.get(kept).is_some_and(|&d| d >= b'5');
    let (digits, exponent) = rounded(&digits, exponent, kept, up);
    let (first, rest) = digits.split_at(1);
    let mantissa = if rest.is_empty() {
        ascii(first).to_owned()
    } else {
        format!("{}.{}", ascii(first), ascii(rest))
    };
    let exponent_sign = if exponent < 0 { '-' } else { '+' };
    format!(
        "{sign}{mantissa}E{exponent_sign}{:02}",
        exponent.unsigned_abs()
    )
}

/// `digits` (ASCII, the first not zero; the value d1.d2d3... * 10^exponent) cut to `kept`
/// significant digits, one added to the last when `up`, with trailing zeros dropped, and the
/// exponent of the result's leading digit, one more than `exponent` when adding carries into a new
/// leading digit.
fn rounded(digits: &[u8], exponent: i32, kept: usize, up: bool) -> (Vec<u8>, i32) {
    let mut exponent = exponent;
    let mut result = digits[..digits.len().min(kept)].to_vec();
    if up && !increment(&mut result) {
        // Every kept digit was 9: the result is 1 followed by zeros.
        result = vec![b'1'];
        exponent += 1;
    }
    while result.len() > 1 && result.last() == Some(&b'0') {
        result.pop();
    }
    (result, exponent)
}

/// Adds one to the last digit of `digits` in place; false when every digit was 9 (and is now 0).
fn increment(digits: &mut [u8]) -> bool {
    for digit in digits.iter_mut().rev() {
        if *digit == b'9' {
            *digit = b'0';
        } else {
            *digit += 1;
            return true;
        }
    }
    false
}

/// `digits` (d1.d2d3... * 10^exponent, no trailing zero) written without an exponent.
fn without_exponent(digits: &[u8], exponent: i32) -> String {
    let count = digits.len();
    match usize::try_from(exponent) {
        Ok(whole) if count <= whole + 1 => {
            format!("{}{}", ascii(digits), "0".repeat(whole + 1 - count))
        }
        Ok(whole) => format!(
            "{}.{}",
            ascii(&digits[..=whole]),
            ascii(&digits[whole + 1..])
        ),
        Err(_) => {
            let zeros = usize::try_from(-1 - exponent).expect("a negative exponent");
            format!("0.{}{}", "0".repeat(zeros), ascii(digits))
        }
    }
}

fn ascii(digits: &[u8]) -> &str {
    std::str::from_utf8(digits).expect("ASCII digits")
}

#[cfg(test)]
mod tests {
    use super::{RoundingPath, inspect, text};
    use std::cmp::Ordering;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn inspection_shows_the_exact_tie_and_its_next_binary64_neighbor() {
        let midpoint = 1.000030517578125_f64;
        let tie = inspect(midpoint).unwrap().rounding.unwrap();
        assert!(matches!(tie.path, RoundingPath::IntegerSplit { whole: 1 }));
        assert!(tie.exact_tie);
        assert_eq!(tie.decision, Ordering::Equal);
        assert_eq!(tie.half, 92_233);
        assert_eq!(tie.rounded_digits, "100003051757812");

        let next = f64::from_bits(midpoint.to_bits() + 1);
        let above = inspect(next).unwrap().rounding.unwrap();
        assert!(!above.exact_tie);
        assert_eq!(above.decision, Ordering::Greater);
        assert_eq!(above.rounded_digits, "100003051757813");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn q64_two_branch_matches_excel_tie_and_binade_controls() {
        // Corpus and bank tie/near-tie cases previously departing (Excel 16.0.20527).
        let cases: &[(u64, &str)] = &[
            (0x42fd_a210_18b9_1418, "521310271410498"),
            (0x4294_f2e4_625b_ed80, "5758361573115.38"),
            (0xc296_d752_c3df_9780, "-6278515587045.88"),
            (0x425f_9e7f_ea05_0400, "543212677140.063"),
            (0x3ed6_038e_11b5_d39f, "5.24851955262677E-06"),
            (0x3fd6_1c45_da98_b974, "0.345475638840888"),
            // Downward controls preserved.
            (0x429a_3208_6160_ed80, "7200547821627.37"),
            (0x414e_cd9a_f7cd_46a5, "4037429.93595202"),
            // Sealed holdout controls (Excel 16.0.20527.20196).
            (0x411e_9928_5100_0000, "501322.079101563"),
            (0x3fe4_9494_8ece_a23c, "0.643137244148484"),
            // All 4 binade-4 discriminators confirming ceil(k / 3.3219).
            (0x8346_b63b_e47a_d061, "-7.1122459292295E-293"),
            (0xa191_6c8a_93e3_7090, "-5.4506679506426E-147"),
            (0x5e38_5176_e7fd_3ec7, "7.5915369950346E+145"),
            (0x7c89_2187_8a20_ddde, "7.837095588212E+291"),
            // 0.1% ev >= 0 slivers at exp2 = -486 and exp2 = -971.
            (0x219f_fda6_6804_cb9d, "1.0007543865916E-146"),
            (0x034f_fb13_4f69_8bc3, "1.0014817833731E-292"),
            // Extremes and zero/subnormal.
            (f64::MAX.to_bits(), "1.7976931348623E+308"),
            (f64::MIN_POSITIVE.to_bits(), "2.2250738585072E-308"),
            (0x0000_0000_0000_0000, "0"),
            (0x8000_0000_0000_0000, "0"),
            (0x0000_0000_0000_0001, "0"),
        ];
        for &(bits, expected) in cases {
            assert_eq!(
                text(f64::from_bits(bits)).as_deref(),
                Some(expected),
                "bits=0x{bits:016x}"
            );
        }
        assert_eq!(text(f64::NAN), None);
        assert_eq!(text(f64::INFINITY), None);
        assert_eq!(text(f64::NEG_INFINITY), None);
    }
}
