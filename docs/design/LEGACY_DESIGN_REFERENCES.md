# Legacy design and evidence

These sources record the designs and measurements from the repositories that preceded DNA Calc.
The local references carry forward decisions or evidence that can be used with the current
architecture. The legacy sources preserve the original design details and historical scope.

## Product design

| Topic | What carries forward | Pinned sources |
|---|---|---|
| Strand | The existing Strand document covers palette roles, contrast pairs, density and geometry. The `dna-strand` crate defines the shipped tokens. | [Local Strand design](../plan/STRAND.md), [Strand design language](https://github.com/DnaCalc/DnaTreeCalc/blob/9644443788b1596939ea2f04a546c4fcaa954d1b/docs/ux/redesign/STRAND_DESIGN_LANGUAGE.md) |
| Shell | The anatomy and keyboard atlas record how the shell, command surfaces and focus move together. Use them with the current desktop and skin boundaries. | [SHELL_SPEC](https://github.com/DnaCalc/DnaTreeCalc/blob/9644443788b1596939ea2f04a546c4fcaa954d1b/docs/ux/redesign/SHELL_SPEC.md) |
| Interaction mechanisms | The interaction traces describe navigation, previews, selection and visible provenance. They supply scenarios for reading the current design. | [MECHANISMS](https://github.com/DnaCalc/DnaTreeCalc/blob/9644443788b1596939ea2f04a546c4fcaa954d1b/docs/ux/redesign/MECHANISMS.md) |
| Parity and trust | Evidence states and their granularity describe what a user can verify about a particular cell, function or workbook. Current claims use CalcVerdict's provenance and the captured Excel result. | [PARITY_TRUST_UX](https://github.com/DnaCalc/DnaTreeCalc/blob/9644443788b1596939ea2f04a546c4fcaa954d1b/docs/ux/redesign/PARITY_TRUST_UX.md) |
| Flow | The first-use walkthrough and keyboard map capture the notebook-style exploration sequence. The old skin IR and build plan refer to the legacy host. | [FLOW_DESIGN](https://github.com/DnaCalc/DnaTreeCalc/blob/9644443788b1596939ea2f04a546c4fcaa954d1b/docs/ux/flow-skin/FLOW_DESIGN.md) |
| OneCalc formula editor | The local design note carries the native browser input model, caret measurements and revision rules into the shared formula UI context. | [Local formula editor notes](FORMULA_EDITOR_AND_ARRAY_RESULTS.md#formula-editor), [Formula editor UX](https://github.com/DnaCalc/DnaTreeCalc/blob/9644443788b1596939ea2f04a546c4fcaa954d1b/docs/onecalc/APP_UX_FORMULA_EDITOR_SPEC.md), [WS14 editor design](https://github.com/DnaCalc/DnaTreeCalc/blob/9644443788b1596939ea2f04a546c4fcaa954d1b/docs/onecalc/WS14_DESIGN_FORMULA_EDITOR.md) |
| OneCalc large-array results | The local design note carries the bounded typed preview, visible truncation and copy semantics. | [Local array result notes](FORMULA_EDITOR_AND_ARRAY_RESULTS.md#large-array-results), [WS14 large-array design](https://github.com/DnaCalc/DnaTreeCalc/blob/9644443788b1596939ea2f04a546c4fcaa954d1b/docs/onecalc/WS14_DESIGN_LARGE_ARRAY_RESULTS.md) |

The current design work is collected in the [front-end topic area](front-end/README.md).
Use the legacy designs as source material for that review.

## File formats and calculation evidence

| Topic | What carries forward | Pinned sources |
|---|---|---|
| OxDoc format references | The standards index, Foundation identifiers and CalcDoc fixture disposition are available in this repository. | [Local format references](../FORMAT_REFERENCES.md), [OxDoc FORMAT_REFERENCES](https://github.com/DnaCalc/OxDoc/blob/c1031833ad91b82b6e6d777df260c1eb34e62fb9/docs/FORMAT_REFERENCES.md) |
| OxCalc grid performance | The seeded workloads and counts of visited cells, edges and bytes give starting points for comparable workload measurements. DNA Calc runs one evaluator and uses the repository's corpus and benchmark commands. | [CORE_ENGINE_GRID_PERF_REGISTER](https://github.com/DnaCalc/OxCalc/blob/60c6af72065d280aae6dd3d0fe2d25aa0ee3e96e/docs/spec/core-engine/CORE_ENGINE_GRID_PERF_REGISTER.md) |
| OxCalc circular references | The W048 ledger identifies Excel build, locale, iteration settings, thread modes and retained outputs per packet. The core and bit-exact packets list 12 and 19 probes, respectively, on Excel 16.0.19929 on Windows with `en-ZA`. Keep the single-thread and multithread observations distinct when interpreting results. | [W048 Excel observation ledger](https://github.com/DnaCalc/OxCalc/blob/60c6af72065d280aae6dd3d0fe2d25aa0ee3e96e/docs/spec/core-engine/w048-cycles/W048_EXCEL_OBSERVATION_LEDGER.md) |
| OxFml name and call resolution | W074 distinguishes call position from bare value use, sheet scope from workbook scope, and ordinary names from table context. Its observed built-ins win the tested call collisions. Visible defined names win tested non-built-in UDF collisions. The retained cases also cover `LET` and `LAMBDA` locals and distinguish a scalar from a callable defined name. | [W074 handoff](https://github.com/DnaCalc/OxFml/blob/8bace71e8b25dbc35f1ab8681594f8f3c242457f/docs/handoffs/HANDOFF_CALC_005_W074_NAME_CALL_FREEZE.md), [precedence matrix](https://github.com/DnaCalc/OxFml/blob/8bace71e8b25dbc35f1ab8681594f8f3c242457f/docs/spec/formula-language/W074_CALC005_NAME_CALL_PRECEDENCE_ORACLE_MATRIX.csv), [table collisions](https://github.com/DnaCalc/OxFml/blob/8bace71e8b25dbc35f1ab8681594f8f3c242457f/docs/spec/formula-language/W074_CALC005_TABLE_NAME_COLLISION_ORACLE_20260522.md), [table names](https://github.com/DnaCalc/OxFml/blob/8bace71e8b25dbc35f1ab8681594f8f3c242457f/docs/spec/formula-language/W074_CALC005_TABLE_NAME_ORACLE_20260523.md), [non-table names](https://github.com/DnaCalc/OxFml/blob/8bace71e8b25dbc35f1ab8681594f8f3c242457f/docs/spec/formula-language/W074_CALC005_NON_TABLE_NAME_CALL_ORACLE_20260524.md) |
