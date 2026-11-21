# The last bit: Excel's number-to-text conversion

*Researched in Carbon round `cb9` and landed in Neon round `nx8` (2026-11-21), closing bead `dc-b1l.25`.*

Every worksheet consumer of general number-to-text conversion in Excel—infix `&`, `CONCAT`, `CONCATENATE`, `VALUETOTEXT`, `ARRAYTOTEXT`, `LEFT`, `RIGHT`, `MID`, `LEN`, `TRIM`, `UPPER`, `LOWER`, `ENCODEURL`, `FIXED`, `DOLLAR`, `TEXT`, `TEXTJOIN`, and general cell formatting—passes through one 15-significant-digit routine (`first_precision`).

Everything below was inferred from recorded and live Excel answers (build `16.0.20527.20196` via Touchstone) and public IEEE 754 / x87 extended-precision arithmetic—no Microsoft code was read or disassembled.

## Where we started

Before `nx8` (`8b73f852`), DNA Calc already matched Excel's Layer 2 formatting (`shaped`: fixed-point up to 20 characters, scientific notation otherwise with 15 digits for `|e| < 99` and a second half-up rounding to 14 digits for `|e| >= 99`). For the 15-digit rounding step itself, however, DNA Calc used a `scaled_x87_above_midpoint` heuristic with a guessed `NEAR_TIE_BITS` rule that left puzzling empirical anomalies across `dc-b1l.25` and earlier `nc1` / `sx1` probes:

- **Exact binary ties below `65,536` (`0 <= e <= 4`) never rounded up**, and inputs in `[1, 10)` rounded up only past `+0.07204` `ulp64(v)`.
- **Inputs in `[0.5, 1)` and `e >= 5` below a power of two** had a round-up threshold near `+0.37204` `ulp64(v)`.
- **Above `|x| = 65,536 = 2^16`**, or whenever `2^k <= |x| < 10^(e + 1)`, the round-up threshold jumped into `[+3.372, +12.122]` `ulp64(10v)` with a **period-8 oscillation** in the binary significand.

That earlier code missed `29` oracle-corpus cells (`5` in `dc_q9rb` marked as deliberate departures, `4` in `catalogue`, and `20` in `fn7_format_context`), `5` rows in `CONCAT` (`2,900 / 2,905`), `6` rows in `CONCATENATE` (`2,891 / 2,897`), and `1,511` of `21,377` probe and `W111` rows.

## The `Q0.64` program behind every anomaly

Carbon `cb9` showed that Excel generates the 15 significant digits of any positive normal `binary64` `x = m53 * 2^(exp2 - 52)` (`2^52 <= m53 < 2^53`) with zero big-integers. It splits into two branches that converge on an unsigned fixed-point **`Q0.64`** (`u64` units of `2^-64`) pre-addition of a **truncated** half-ulp table `Q64_HALF_POW10[k] = floor(2^63 / 10^k)`, followed by digit extraction via `(u128::from(f) * 10) >> 64`:

1. **Branch 1 (`0 <= exp2 < 16`, i.e. `1.0 <= |x| < 65,536.0`) — Exact `Q16.64` integer-split fast path:** Shifts `m53` directly into a 16-bit integer part `w = (m53 >> (52 - exp2)) as u32` (`1..=65535`, `d` decimal digits in `1..=5`) and an exact `Q0.64` fraction `f = m53 << (12 + exp2)`, with zero FPU scaling. It adds `Q64_HALF_POW10[15 - d]` (`k in 10..=14`) to `f`; on `u64` overflow carry it increments `w` (updating `d` and `k` if `w` crosses `10^d`), then extracts `15 - d` fractional digits.
2. **Branch 2 (`exp2 < 0` or `exp2 >= 16`) — x87 `PC = 64` low-nibble-first scaling into `Q0.64`:** From `frexp_k = exp2 + 1`, Excel estimates the decimal shift `p = ceil(frexp_k / 3.3219)` (`0` when `frexp_k == 0`, so `[0.5, 1.0)` is unscaled) and multiplies by `10^-p` using the **same low-nibble-first x87 `PC = 64` round-to-nearest-even table chain** (`10^±1..15`, `10^±16..240`, `10^±256`) already established for `VALUE`, `ROUNDUP`/`ROUNDDOWN`/`TRUNC`, and `COMPLEX`. If the scaled exponent satisfies `pow + 63 >= 0` (`v >= 1.0`), one `10^-1` step (`NEG_POW10[0]`) is applied and `p` increments by `1`. It then shifts the 64-bit significand `mv` into `Q0.64` by a **truncating right shift** `f = mv >> (-1 - ev)`, counts leading zero decimal digits (`k = 15`, `16`, or `17` so `v` scales cleanly into 15 digits), adds `Q64_HALF_POW10[k]` (`u64` carry emits `10^14` at exponent `p`), skips up to two leading `0` digits, and extracts 15 digits.

Pure-Rust `u64` / `u128` arithmetic (`mul_pc64` and `scale_nibble_pc64`) reproduces hardware x87 `long double` C bit-for-bit (`1,000,458 / 1,000,458` inputs, `0` mismatches) across `x86_64`, `aarch64`, and `wasm32`.

## Why every empirical quirk followed from those lines

Once the `Q0.64` program is written down, every oddity observed earlier becomes an exact arithmetic identity:

- **Why exact `binary64` ties below `65,536` never round up, and `[1, 10)` stops at `+0.07204` `ulp64(v)`:** Branch 1 converts `(w, f)` with zero scaling error, and `Q64_HALF_POW10[k] = floor(2^63 / 10^k)` is strictly less than `2^63 / 10^k`. At `d = 1` (`[1, 10)`, `k = 14`), `92_233` vs `92_233.7204` leaves a `0.7204` deficit in `f` (`+0.07204` `ulp64(v)`), fixing `8` earlier misses at `e = 0`.
- **Why `[0.5, 1)` and `e >= 5` below `2^k` stop at `+0.372036854775808` `ulp64(v)`:** When `0.5 * 10^(e + 1) <= |x| < 2^k`, `p = e + 1` puts `v in [0.5, 1.0)` (`ev = -1`, `shr = 0`, `f = mv`). Adding `Q64_HALF_POW10[15] = 9_223` (`2^63 / 10^15 = 9_223.372036854775808`) forces `v` to exceed the midpoint by `+0.372036854775808` `ulp64(v)`, right inside the earlier measured `[+0.371988, +0.372704]` bracket.
- **Why `|x| = 65,536` switches regime and above-`2^k` oscillates with period `8`:** At `exp2 = 16`, `w` overflows 16 bits and falls into Branch 2. Whenever `2^k <= |x| < 10^(e + 1)`, `p = e + 2` lands `v in [0.05, 0.1)` (`ev = -4`, `shr = 3`). The truncating shift `mv >> 3` drops `r = mv & 7` (`0..=7`) units of `2^-64` (`1.25 * r` `ulp64(10v)`), and `Q64_HALF_POW10[16] = 922` (`922.337203685 - 922 = 0.337203685`, or `+3.37203685` `ulp64(10v)`) gives exact threshold **`+3.37203685 + 1.25 * (mv & 7)` `ulp64(10v)`**—literally `[+3.3720, +12.1220]`.

## Settling the 4-binade ambiguity (`32 / 32` frozen proof)

`cb9` left one narrow question open because Touchstone had disconnected before the final check: `ceil(frexp_k * log10(2))` and `ceil(frexp_k / 3.3219)` agree on `2,042` of the `2,046` normal `binary64` exponents, differing only in four binades (`exp2 in {-971, -486, +484, +969}`). `cb9` sealed two prospective holdout sets with SHA-256 hashes before Excel reconnected, and `nx8` scored them against live Excel `16.0.20527.20196` before touching a single line of Rust:

- **600-row frozen holdout** (SHA-256 `6169c9da581e2fb16575cb566a6f4852562ee79ff31c0a785ca314a6c8d11f00`, disjoint from all `1,503,226` prior inputs across five slices and five functions): **`600 / 600` (`100.00%`)** vs `0 / 600` on `origin/master`.
- **32-row binade-4 discriminator** (SHA-256 `8cd0fc3554d6ad7164ca94bf8450814aa451e47a0f06551bd3b23abff4322f1b`): `ceil(frexp_k / 3.3219)` matched **`32 / 32`** vs **`0 / 32`** for `ceil(frexp_k * log10(2))`.
- **20-row `0.1%` sliver check** (`exp2 in {-486, -971}` with `ev >= 0`): the `10^-1` correction step (`NEG_POW10[0]`) matched **`20 / 20`** (`0 / 20` for an alternative `Q16` `w = 1` branch).

## Corpus and witness-bank impact (`nx8`, `86427f7a`)

- **Probe & `W111` suites (`cb9`):** **`21,377 / 21,377` (`100.00%`)**, `+1,511 / -0` over `origin/master` (`19,866 / 21,377`).
- **Oracle corpus (`cargo xtask corpus --oracle --lean`, 63 workbooks, `78,296` cells):** `78,132 -> 78,161 / 78,296` (`+29 / -0`, `99.79% -> 99.83%`), fixing every remaining number-to-text corpus mismatch (`calc-func/catalogue` `+4` infix `&`, `calc-func/dc_q9rb` `+5` infix `&` to `86 / 86`, `calc-func/fn7_format_context` `+20` to `160 / 160` across `15` `INDEX` and `5` `HSTACK` cells) with `0` regressions.
- **Full witness bank (`506` functions, `4,749,860` rows before newly harvested study additions):** `4,747,053 -> 4,747,064 / 4,749,860` (`+11 / -0`), raising exact functions `438 -> 440 / 506` (`CONCAT` `2,900 -> 2,905 / 2,905` exact, `CONCATENATE` `2,891 -> 2,897 / 2,897` exact; `VALUETOTEXT` `2,475 / 2,475`, `FIXED` `3,723 / 3,723`, `TEXT` `4,632 / 4,632`, `DOLLAR` `2,176 / 2,176` all preserved exact).
- **Post-study witness bank (after `5afee188`):** `CONCAT` **`3,578 / 3,578`** exact (`+673` study rows), `CONCATENATE` **`3,133 / 3,133`** exact (`+236`), `FIXED` **`3,783 / 3,783`** exact (`+60`), and `VALUETOTEXT` **`2,571 / 2,571`** exact (`+96`).

## Method lessons

- **Read the rounding threshold as a function of low significand bits:** Period-`8` oscillation in `[+3.372, +12.122]` `ulp64(10v)` is not float noise—it is the direct fingerprint of a `>> 3` truncating shift dropping `mv & 7` before adding a truncated `Q0.64` half-ulp constant `floor(2^63 / 10^k)`.
- **Test the exact rare exponents where two formulas part ways:** `ceil(k * log10(2))` and `ceil(k / 3.3219)` agree on `99.8%` of binary64 exponents; isolating the `4` remaining binades with `32` frozen discriminators settled the exact integer formula in one round.
- **Keep one shared scaling chain:** `first_precision` uses the exact same low-nibble-first x87 `PC = 64` power-of-ten table chain already proven in `VALUE`, `ROUNDUP`/`ROUNDDOWN`/`TRUNC`, and `COMPLEX`, and replaces hardware x87 cleanly with portable `u64`/`u128` arithmetic.

## What stays open

- Nothing stays open on `dc-b1l.25` (`number_text` itself is complete). On the post-study `TEXT` bank (`4,668 / 4,800`), `132` newly harvested study rows (`0.00000000000000E+00` / `General`) return `fault:NotImplemented` due to format-string/locale handling owned by `dc-j9k.65` and `dc-0f1k.8`, unrelated to `number_text`.
