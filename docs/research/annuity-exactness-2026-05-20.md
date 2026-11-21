# Annuity exactness witness ledger (`PMT`, `PPMT`, `RATE`) (2026-05-20)

Extracted from [Foundation `EXACTNESS_HIGHER_LEVEL_ESCALATION_CANDIDATES.md`][source].
In DnaCalc, the other two families in that Foundation document are already closed and
documented separately: `NORM.INV` in [normal-inverse-central-2026-11-17.md](normal-inverse-central-2026-11-17.md)
and `IRR` in [irr-publication-2026-11-12.md](irr-publication-2026-11-12.md). Tiny-rate `FV`/`PV`/`RATE`
(`dc-j9k.51`) and `CUMPRINC`/`CUMIPMT` (`dc-j9k.49`) also closed on 2026-11-12; remaining `RATE`
edge rows are tracked in `dc-oqc1`.

## 1. `PMT`, `IPMT`, `PPMT`, and `CUMIPMT`/`CUMPRINC` witnesses

### Pinned witness bits (Excel vs legacy `OxFunc` baseline)

| Case / Formula | Legacy `OxFunc` | Excel target value | Excel binary64 bits |
| --- | --- | --- | --- |
| `FTC-0377`: `=PMT(0.05/12,360,200000)` | `-1073.6432460242763` (7 ULP off) | `-1073.6432460242781` | `0xC090C692AF15F63A` |
| `=IPMT(0.05/12,1,360,200000)` | - | - | `0xC08A0AAAAAAAAAAB` |
| `FTC-0391`: `=PPMT(0.05/12,1,360,200000)` | `-240.30991269094295` | `-240.3099126909447` | `0xC06E09EACE050723` |
| `=PMT(0.05,10,1000,0,0)` | matched | - | `0xc06030257a65e089` |
| `=PMT(0.08/12,10,10000,0,0)` | matched | - | `0xc0903420dc087094` |

Widened `PMT` control formulas used in the Foundation pass: `=PMT(0.05/12,120,100000)`,
`=PMT(0.08/12,360,250000)`, `=PMT(0.04/12,360,200000)`, `=PMT(0.05,10,100)`, and `=PMT(0.01,48,8000)`.

### Observed annuity identities

- First-period `IPMT` equals `-pv * rate` (`0xC08A0AAAAAAAAAAB` for `200000, 0.05/12`), and
  `PPMT = PMT - first_period_interest`.
- For the 12-month active mortgage case, Excel gives `12 * PMT - CUMPRINC` bit-identical to
  `CUMIPMT`, and `CUMIPMT + CUMPRINC - 12 * PMT = 0` exactly.

### Legacy `PMT` algebraic regroupings that failed

- **Missed `FTC-0377`, preserved broader pinned rows:** baseline direct form (`1/6` on widened set), `H5` FMA numerator (`1/6`, bit-identical to baseline on blocked rows).
- **Hit `FTC-0377`, regressed broader pinned rows:** grouped `ln1p/expm1` scale form (`group_b`).
- **Dominated / regressed (`0/6` on widened set):** `H2` merge-rate-into-numerator, `H3` discount-factor form, `H2+H5`, `H3+H5`, recurrence, `recurrence_kahan`, `log1p`, `expm1`, `powf`, `exp2`, ratio-over-term, quotient-difference, split-sum regroupings.

## 2. `RATE` (`FTC-0381`) witness and failed solver/conditioning variants

- **Formula:** `=RATE(360,-1073.64,200000)`
- **Excel target:** `0.004166644536345589`, bits `0x3f71110b2048574f` (residual `1.0244548320770264e-8` under legacy `balance_equation`).
- **Legacy `1e-7` tolerance stop (`OxFunc`):** `0.0041666445363460975`, bits `0x3f71110b20485999` (residual `6.05359673500061e-8`).
- **Tighter tolerance floor (`1e-10`, `1e-13`, `1e-16`, or `2/4/16`-ULP rate-stability exits):** `0.0041666445363455415`, bits `0x3f71110b20485718` (still misses Excel target).
- **Balance-equation conditioning variants tried without hitting `0x3f71110b2048574f`:**
  - `fv - adjust + factor * (pv + adjust)` -> `0x3f71110b20485718`
  - `mul_add` balance form -> `0x3f71110b20485718`
  - `log1p/exp` balance form -> `0x3f71110b20485719` / `0x3f71110b2048571a`
  - integer-period power-sum balance form -> `0x3f71110b20485780`

[source]: https://github.com/DnaCalc/Foundation/blob/4736b348a2f88864e17364de44a430095cce9f5e/reference/test-corpus/workspace/monitoring/EXACTNESS_HIGHER_LEVEL_ESCALATION_CANDIDATES.md
