# Formatting specification constraints (2026-03-05)

Extracted from [Foundation `FORMATTING_FORMAL_FINDINGS.md`][findings] (run
`specproc-20260304-231650`, 2026-03-04 UTC, extracting `MS-OI29500`, `MS-OE376`, and `MS-XLSX`).
See [FORMAT_REFERENCES.md](../FORMAT_REFERENCES.md) for current publisher guidance.

## 1. Conditional-formatting formula constraints

Both `MS-OI29500` and `MS-OE376` specify four structural prohibitions for conditional
formatting (`<cfRule><formula>`) expressions:

| Constraint | `MS-OI29500` (`discovered-ms-oi29500-250218-d35cbb01`) | `MS-OE376` (`discovered-ms-oe376-220816-823374c7`) |
| --- | --- | --- |
| No array constants | `CONF-...-1453` | `CONF-...-1427` |
| No structured references | `CONF-...-1454` | `CONF-...-1428` |
| No union or intersection binary operators | `CONF-...-1455` | `CONF-...-1429` |
| No 3-D references | `CONF-...-1456` | `CONF-...-1430` |

Precedence-order rules for conditional-formatting evaluation appear in `MS-OI29500`
(`CONF-...-0099` / `CONF-...-0100`) and `MS-OE376` (`CONF-...-0111`).

## 2. Number-format grammar and underspecification anchors

- **Format-code ABNF:** `MS-OI29500` (`p:6366`, `CONF-...-0534`) and `MS-OE376` (`p:6780`,
  `CONF-...-0550`) anchor valid `formatCode` syntax to the specification ABNF.
- **International adjustment:** `MS-OI29500` (`p:6367`, `CONF-...-0535`) and `MS-OE376`
  (`p:6781`, `CONF-...-0551`) state that the ABNF must be adjusted for international number
  formats.
- **Explicit underspecifications in `MS-OI29500`:**
  - `SPEC-discovered-ms-oi29500-250218-d35cbb01-07122` (`p:6365`): no restriction on `formatCode` size or content is specified.
  - `SPEC-discovered-ms-oi29500-250218-d35cbb01-06258` (`p:5746`): no default for `numFmtId` is specified.
  - Parallel underspecification signals appear in `MS-OE376`.

[findings]: https://github.com/DnaCalc/Foundation/blob/4736b348a2f88864e17364de44a430095cce9f5e/reference/runs/20260305-ms-formatting-formal-pass-01/outputs/FORMATTING_FORMAL_FINDINGS.md
