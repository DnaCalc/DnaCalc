# Formula entry and concrete language observations (2026-03-02)

Extracted from [Foundation `EXCEL_FORMULA_LANGUAGE_CONCRETE_RULES.md`][concrete-rules],
[EMP-0001][emp01], and [EMP-0002][emp02]. Current DnaCalc formula entry checking lives in
[FUNCTION_CATALOGUE.md](../architecture/FUNCTION_CATALOGUE.md) ("Entry checking: formulas Excel will not take")
and `crates/calc-fml/src/eval/compile.rs`.

## 1. Excel environment and provenance

- **Build:** Excel `16.0.19725.20126` (`EXCEL.EXE` SHA-256 `78E29AF2342C4120CAE3BAE64621E3BB2517D4DC122768F107E46528C177379E`), Windows Current Channel, captured 28 February and 2 March 2026 via Foundation `excel-probe`.
- **Source runs:**
  - Wave 1 (`20260228-180047-excel-compat-empirical-pass-01`): `formula_parse_wave1/` (`ECS-EB-028`, `ECS-EB-029`, `ECS-EB-030`).
  - Pass 2 (`20260302-070309-excel-formula-language-pass2-pack-01`): `formula_parse_pass2/` (37/37 rows executed: 35 accepted, 2 rejected, 0 mismatches).

## 2. Parse acceptance and ambiguity baseline (Wave 1)

| Case ID | Formula input | Seeded expected | Observed | Stored / displayed result |
| --- | --- | --- | --- | --- |
| FPCW1-001 | `=SUM(A1:B2)` | accepted | accepted | Baseline range operator. |
| FPCW1-002 | `=SUM((A1:A2,B1:B2))` | accepted | accepted | Union reference inside function call. |
| FPCW1-003 | `=SUM(A1:C1 A1:A3)` | accepted | accepted | Space intersection operator. |
| FPCW1-004 | `=SUM(A1::B2)` | rejected | rejected | Malformed double-colon range token. |
| FPCW1-005 | `=@A1:A3` | accepted | accepted | Explicit implicit intersection `@`. |
| FPCW1-006 | `=A1#` | accepted | accepted | Spill-reference suffix `#`. |
| FPCW1-007 | `=@A1#` | accepted | accepted | Combined `@` and `#` accepted. |
| FPCW1-008 | `=#A1` | rejected | rejected | Malformed prefix `#`. |
| FPCW1-009 | `=LET(x,1,x+2)` | accepted | accepted | LET baseline. |
| FPCW1-010 | `=LAMBDA(x,x+1)(2)` | accepted | accepted | Inline LAMBDA invocation. |
| FPCW1-011 | `=LAMBDA(x,x+1)(1,2` | rejected | rejected | Malformed LAMBDA invocation. |
| FPCW1-016 | `=SUM(TblParse[[#All],[Amount])` | rejected | rejected | Malformed structured-ref brackets. |
| FPCW1-020 (`EMP-0001`) | `=SUM(A1,,B1)` | rejected | accepted | Stored as entered; evaluated to `5` (missing arg preserved). |
| FPCW1-013 (`EMP-0002`) | `=A1.Price` | probe | accepted | Evaluated to `#FIELD!` in non-linked-type context. |
| FPCW1-019 | `=A1 B1` | accepted | accepted | Evaluated to `#NULL!` (disjoint single-cell intersection). |

## 3. Stored formula normalization (Wave 1 and Pass 2)

| Case / Lane | Entered formula | Stored formula | Observed evaluation |
| --- | --- | --- | --- |
| FPCW1-017 | `=sUm(a1:b2)` | `=SUM(A1:B2)` | Built-in function and A1 coords uppercased. |
| FPCW1-018 | `=sum(tblparse[amount])` | `=SUM(TblParse[Amount])` | Structured-ref table/column casing normalized to canonical names. |
| P2-FML-008 (`@`/`#`) | `=@A1#` | `=A1#` | Leading `@` stripped before spill-range reference; evaluated against anchor. |
| P2-FML-008 (`@`/`#`) | `=@SEQUENCE(3)` | `=SEQUENCE(3)` | Leading `@` stripped before dynamic-array call. |

## 4. Pass-2 behavior captures (`20260302-070309`)

1. **Argument gaps (`P2-FML-001`):** `SUM` and `LET` gap variants parsed and evaluated as accepted (`FMLP2-001..005`).
2. **Dot-field (`P2-FML-002`):** Both `=A1.Price` and `=FIELDVALUE(A1,"Price")` parsed and evaluated to `#FIELD!` without linked-data cell payloads (`FMLP2-006..009`; `allowed_error` on linked-data conversion in that environment).
3. **Operator precedence (`P2-FML-003` / `P2-FML-010`):**
   - `=-2^2` evaluated to `4` (unary `-` binds tighter than `^`).
   - `=1+2&3` evaluated to `"33"` (`+` binds tighter than `&`).
   - Observed precedence tiers (highest to lowest): (1) `:`, `,`, space intersection; (2) unary `+`, `-`; (3) `%`; (4) `^`; (5) `*`, `/`; (6) `+`, `-`; (7) `&`; (8) `=`, `<>`, `<`, `>`, `<=`, `>=`.
4. **Helper forms (`P2-FML-004`):** `MAP`, `BYROW`, `SCAN`, `BYCOL`, `REDUCE` parsed and evaluated as accepted; malformed LAMBDA shape rejected (`FMLP2-018`).
5. **Spill suffix on scalar (`P2-FML-008`):** `=A1#` on a non-spill scalar cell returned `#REF!`.
6. **Name scoping (`pass-2c`, `FMLP2-019/020`):** In a dual-scope workbook setup, unqualified `=MyName` resolved to the workbook-scoped name (`4`), while `=Sheet1!MyName` resolved to the sheet-scoped name (`1`) and stored with workbook qualification.
7. **External workbook reference (`pass-2c`, `FMLP2-021/022`, `EMP-0011`):** Both present and missing workbook references parsed; with support workbook explicitly open, `= [Book]Sheet!Cell` resolved to `77` under both `update_links=0` and `update_links=3`, while closed/missing support workbook yielded `#REF!`.

[concrete-rules]: https://github.com/DnaCalc/Foundation/blob/4736b348a2f88864e17364de44a430095cce9f5e/reference/conformance/excel-worksheet-engine/model/EXCEL_FORMULA_LANGUAGE_CONCRETE_RULES.md
[emp01]: https://github.com/DnaCalc/Foundation/blob/4736b348a2f88864e17364de44a430095cce9f5e/reference/empirical/findings/EMP-0001-formula-double-comma-accepted.md
[emp02]: https://github.com/DnaCalc/Foundation/blob/4736b348a2f88864e17364de44a430095cce9f5e/reference/empirical/findings/EMP-0002-dotfield-accepted-field-error.md
