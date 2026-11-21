# Display versus machine numeric boundary (2026-05-20)

Extracted from [Foundation `EXCEL_DISPLAY_VS_MACHINE_NUMERIC_BOUNDARY_NOTE.md`][source].
This note preserves live Excel observations distinguishing formatted display text,
COM `Value2` / saved XML `<v>` machine binary64 payloads, and worksheet `=` comparison
canonicalization.

## 1. `=MULTINOMIAL(2,3,4)` three-surface split (`FTC-0254`)

Observed in live Excel (`OxXlPlay` probe `.tmp/ftc-0254-multinomial-probe/` and manual worksheet check):

| Surface | Observed Excel output |
| --- | --- |
| `Formula` / `Formula2` | `=MULTINOMIAL(2,3,4)` |
| `Text` (and expanded decimal display) | `1260` |
| COM `Value2` round-trip string | `1259.999999999999` |
| COM `Value2` binary64 bits | `0x4093AFFFFFFFFFFC` |
| Saved XML `<v>` | `1259.9999999999991` |
| Worksheet `=MULTINOMIAL(2,3,4)=1260` | `TRUE` |
| Worksheet `=SIGN(MULTINOMIAL(2,3,4)-1260)` | `-1` |

Consequence for bit-exact verification:
1. Display text (`1260`) and worksheet `=` (`TRUE`) both mask the sub-integer residue (`4` ULPs below `1260.0`), whereas arithmetic (`SIGN(...-1260)`) and `Value2` bits (`0x4093AFFFFFFFFFFC`) expose it.
2. Under DnaCalc's bit-exact policy ([BRIEFING.md](../architecture/BRIEFING.md) §4.4), the machine `Value2` payload (`0x4093AFFFFFFFFFFC`) is the authoritative target, not the rounded display text or worksheet `=` result.

## 2. Similar rounded-display vs machine-payload cases

| Case | Rounded display | Retained Excel machine numeric |
| --- | --- | --- |
| `FTC-0365` | `1` | `0.9999999999999998` |
| `FTC-0366` | `-1` | `-0.9999999999999998` |
| `FTC-0369` | `1` | `0.9999999999999996` |
| `FTC-0375` | `-1.2` | `-1.1999999999999984` |
| `FTC-0383` | `0.163405600688989` | `0.16340560068898924` |
| `FTC-0399` | `0.03` | `0.030000000000000027` |

## 3. JSON decimal lexeme canonicalization (`=MULTINOMIAL(1,2,3,4)`)

For `MULTI-WIT-5` (`=MULTINOMIAL(1,2,3,4)`), two decimal renderings appeared across JSON projections:
`12599.999999999995` (raw) and `12599.999999999996` (after JSON parse/reserialize). Both strings
parse to the identical binary64 bit pattern `0x40c89bfffffffffe`. Compare raw `f64::to_bits()`
rather than decimal text spellings.

[source]: https://github.com/DnaCalc/Foundation/blob/4736b348a2f88864e17364de44a430095cce9f5e/reference/test-corpus/workspace/monitoring/EXCEL_DISPLAY_VS_MACHINE_NUMERIC_BOUNDARY_NOTE.md
