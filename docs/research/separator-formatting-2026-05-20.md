# Separator-sensitive worksheet number formatting (2026-05-20)

Extracted from [Foundation `EXCEL_SEPARATOR_FORMATTING_THEORY_NOTE.md`][source]
(`FTC-0288` investigation and follow-on COM probes `.tmp/ftc-0288-*`).

## 1. Anchor observation (`FTC-0288`)

For `=TEXT(1234567.89,"#,##0.00")`:
- Under host state with effective thousands separator `NBSP` (non-comma thousands), Excel rendered `1234,567.89`.
- Under forced comma-thousands (`ThousandsSeparator=","`), Excel rendered `1,234,567.89`.

Across all widened probes, `TEXT(x, fmt)` and direct worksheet `NumberFormat` formatting produced identical strings.

## 2. Observed formatting behaviors

1. **Comma as grouping/scaling vs picture marker:**
   - Comma behaves as semantic thousands grouping/scaling only when the active thousands separator is `,`.
   - When active thousands separator is not `,` (e.g. `NBSP`), commas act as picture/literal-position markers: digits fill the integer picture right-to-left with leftover high-order digits prepended to the first segment.
   - Widened comma/scaling patterns probed: `#,##0.00`, `#,,`, `0,,`, `#,##0,,`, `#,,,`, `0,,,`, `#,##0,,,`, `#,###`, `#,##`, `##,##0.00`, `#.0,`, `0.0,,"M"`.
2. **Grouping picture canonicalization on round-trip (comma-thousands mode):**
   - `#,##` -> `#,###`
   - `#,####` -> `##,###`
   - `###,##` -> `##,###`
   - `##,####` -> `###,###`
3. **Dot remapping under `DecimalSeparator=","`, `ThousandsSeparator="."`:**
   - `0.00` -> `1.235`
   - `#.##` -> `1.235`
   - `#,##0.00` -> `1234567,89000`
   - Quoted or escaped `.` and `,` remain literal and bypass separator remapping.

## 3. Open questions

- Complete formal grammar coverage across mixed decimal/grouping/scaling combinations and host/locale matrices.
- Whether date/time `/` and `:` tokens are literal or remapped when host effective date/time separators differ from `/` and `:`.

[source]: https://github.com/DnaCalc/Foundation/blob/4736b348a2f88864e17364de44a430095cce9f5e/reference/test-corpus/workspace/monitoring/EXCEL_SEPARATOR_FORMATTING_THEORY_NOTE.md
