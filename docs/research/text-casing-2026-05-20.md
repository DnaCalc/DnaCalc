# Worksheet text casing (`UPPER`, `LOWER`, `PROPER`) (2026-05-20)

Extracted from [Foundation `EXCEL_TEXT_CASING_THEORY_NOTE.md`][source] (`FTC-0667`
investigation and widened `OxXlPlay` empirical probe matrix).

## 1. Excel environment

- Excel `16.0` build `19822`, Windows 11 Pro x64, `South Africa Standard Time`, `use_system_separators = true` (`.` decimal, `NBSP` thousands).

## 2. Observed Excel matrix

| Formula | Observed Excel result | Behavior demonstrated |
| --- | --- | --- |
| `UPPER("straße")` | `STRAßE` | Sharp s (`ß`) is **not** expanded to `SS`. |
| `LOWER("STRAẞE")` | `straẞe` | Capital sharp s (`ẞ`) is **not** lowercased to `ß`. |
| `LOWER("WEIẞ")` / `LOWER("WEIß")` | `weiẞ` / `weiß` | Both `ẞ` and `ß` remain unchanged under `LOWER`. |
| `UPPER("ẞ")` / `LOWER("ß")` | `ẞ` / `ß` | Identity on `ẞ` under `UPPER` and `ß` under `LOWER`. |
| `UPPER("weiß")` / `PROPER("straße")` / `PROPER("weiß")` | `WEIß` / `Straße` / `Weiß` | `ß` preserved in non-initial positions. |
| `UPPER("İstanbul")` / `LOWER("İSTANBUL")` | `İSTANBUL` / `istanbul` | No combining dot above added when lowercasing `İ` (not Turkish-locale). |
| `LOWER("İ")` / `UPPER("İ")` | `i` / `İ` | `İ` maps to plain `i` under `LOWER`. |
| `UPPER("istanbul")` / `LOWER("Istanbul")` | `ISTANBUL` / `istanbul` | ASCII `i` -> `I` (not Turkish dotted `İ`). |
| `UPPER("ı")` / `LOWER("ı")` | `I` / `ı` | Dotless `ı` uppercases to ASCII `I`. |
| `LOWER("I")` / `UPPER("i")` | `i` / `I` | Standard ASCII mapping. |
| `PROPER("İSTANBUL")` | `Istanbul` | Capital `İ` followed by lowercased `STANBUL`. |
| `UPPER("κόσμος")` / `LOWER("ΟΣ")` | `ΚΟΣΜΟΣ` / `ος` | Greek final sigma (`ς`) handled on `LOWER`. |
| `LOWER("ΟΣΟΣ")` / `LOWER("ΣΟΣ")` | `οσος` / `σος` | Medial `Σ` -> `σ`, final `Σ` -> `ς`. |
| `UPPER("μαΐου")` / `LOWER("ΜΑΪΟΥ")` / `UPPER("αϊ")` / `LOWER("ΑΪ")` | `ΜΑΪΟΥ` / `μαϊου` / `ΑΪ` / `αϊ` | Greek accent/diaeresis pairs. |
| `UPPER("café")` / `UPPER("cafe\u{0301}")` | `CAFÉ` / `CAFE\u{0301}` | Precomposed and decomposed accents both preserved without NFC normalization. |
| `UPPER("æther")` / `LOWER("ÆTHER")` / `UPPER("œuvre")` / `LOWER("ŒUVRE")` | `ÆTHER` / `æther` / `ŒUVRE` / `œuvre` | Single-codepoint extended Latin pairs. |
| `UPPER("ijssel")` / `PROPER("ijssel")` | `IJSSEL` / `Ijssel` | Digraph treated as ordinary ASCII letters. |
| `UPPER("ﬀoo")` / `LOWER("ﬀOO")` | `ﬀOO` / `ﬀoo` | Compatibility ligature `ﬀ` is **not** expanded to `FF`/`ff`. |
| `UPPER("ǆuro")` / `LOWER("ǄURO")` / `PROPER("ǆURO")` | `ǄURO` / `ǆuro` / `Ǆuro` | Titlecase/single-codepoint ligature `ǆ`/`Ǆ` mapped directly without expansion. |

## 3. Working family model and open matrix

- Excel worksheet `UPPER`, `LOWER`, and `PROPER` do **not** implement full Unicode SpecialCasing (no 1-to-many expansions, no NFC/NFD normalization) and do **not** apply Turkish-locale dotted/dotless-I rules in the tested path.
- They behave as simple single-codepoint case mapping with selected script-aware exceptions (notably Greek final sigma `ς`).
- Open: check whether host Windows locale (e.g. Turkish UI/regional locale) changes any of these outputs, plus broader Unicode script/combining-mark boundaries.

[source]: https://github.com/DnaCalc/Foundation/blob/4736b348a2f88864e17364de44a430095cce9f5e/reference/test-corpus/workspace/monitoring/EXCEL_TEXT_CASING_THEORY_NOTE.md
