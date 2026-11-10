# Work sets for continuing agents

This is the list of self-contained work sets for agents (Claude, Codex or others) taking over from
here. Read `AGENTS.md` first; it is the contract. Then read the bead of your work set (`br show
<id>`) and the documents it names. One agent per work set, each in its own full clone
(`C:\Work\DnaCalc\DnaCalc-<lane>` on the Windows machine, or any clone elsewhere), on master.

## How to work

- **Capability first.** Report what the product can now do, then the evidence, then what is open.
  Pass = matched Excel; refused, mismatched, failed and missing count as fail; cells that read
  another workbook's saved values or that a circular-reference rule produced are listed on their
  own, never as pass.
- **Inner loop stays fast.** Per commit: `cargo xtask check` (build, tests of what you touched,
  the marker scans). Slow validation (browsers, the acceptance suite, architecture
  scans, Excel captures) is run on purpose with `cargo xtask sweep` or the capture command, never
  as a step that must pass before you can commit. Do not add new per-commit checks.
- **Excel.** Only the Windows machine `C:\Work\DnaCalc` has Excel. Work sets marked *Excel* need
  it for new measurements; others can run anywhere. Excel's results are assumed stable across
  builds; every reading records Excel build, platform and CPU.
- **Light process.** No tracker comments that only log decisions. A short handover note on a bead
  you leave open is useful. Close a bead with `br close <id> --reason "<what works; evidence;
  still open>"`.
- **Licence.** No third-party reference code in our repositories (see
  `docs/plan/THIRD_PARTY_REVIEW.md`). What you may read or port when working out an algorithm,
  and how facts from restricted sources may reach an implementation, is in
  `docs/plan/RESEARCH_SOURCES.md` (source tiers, analyst and implementer roles). A calc-func file that is not own work gets a ledger entry
  (`migration/licence-ledger.json`, exceptions only). Microsoft's function descriptions are kept, attributed, with
  their source URLs.

## Work sets

| # | Work set | Beads | Excel | Notes |
|---|---|---|---|---|
| 1 | Per-function bit-exact status | dc-ymjf.2, dc-ymjf.3 | partly | `cargo xtask witness judge`, pinned witness sets, `crates/calc-func/witness-status.tsv`; OxFunc's Excel answers (in CalcFunc-Evidence) are imported as readings. Plan: `parked/assess/bit-exact.md` §4 (on the Windows machine). |
| 2 | Algorithm search research | dc-ymjf.4 and its units | no (results are confirmed on the Windows machine) | A public research repository with the search tools; one unit per root cause: BINOM/POISSON mass (clean-room: never read the deleted `discrete_dist_family.rs`, its history or R nmath), ERFC body, PPMT/IPMT, the incomplete beta tail, RATE/IRR solvers, GAMMA.DIST's three cumulative cells. Good for other machines and models. |
| 3 | Test migration from the legacy repositories | dc-jnt.5 to dc-jnt.16 | J1-J3 no (import), others partly | `docs/plan/TEST_MIGRATION_PLAN.md`. Bring useful OxFunc/OxFml/OxCalc/OxDoc tests over against calc-* APIs. Then dc-jnt.2 and dc-jnt.4 (archive, owner confirms each). |
| 4 | Circular references | dc-y2o, dc-y2o.1, dc-y2o.2, dc-86f | yes | Iteration off as an engine rule with its own provenance; iteration on in Excel's order. Read the handover notes on the beads. |
| 5 | Remaining function gaps | dc-h9u, dc-b1l, dc-ylr (open edges in their notes) | yes | Small, measured cases; the catalogue oracle is the one place for Excel rows. |
| 6 | Text residue | dc-zsn.2 (iteration marks after Han, jamo), dc-zsn.3, dc-zsn.4 | yes | dc-zsn.3 needs a machine set to a double-byte Office language. |
| 7 | Functions handbook | dc-dfl, dc-dfl.1, dc-vqo | no | The ExcelFunctionsHandbook repository: coverage to every function, pages checked against Excel readings, version/platform/CPU differences (F19). The authoritative reference, built continuously. |
| 8 | Add-ins | dc-snz.4, then dc-snz.1 to dc-snz.3 | yes | Owner 2026-10-31: defer renames; extend the pinned OxVba UDF API (docs/plan/VBA_UDFS.md): native JIT sessions/cache, interpreter browser route, Optional/ParamArray and bounded live objects; precise scheduling and wider host parity remain dc-snz.5. UDF registration lives at calc-func/calc-fml level, so DNA OneCalc (engine-free by design) hosts and tests it. |
| 9 | Pivot tables | dc-8wp | yes | After 4 and 5. |
| 10 | Performance | dc-dxm, dc-u5c | no | After the first public publish. |

## First steps for the next session (state at 2026-10-30)

1. Run `cargo xtask sweep` once on a clean tree (never run in full since it was introduced) and
   `cargo xtask corpus --oracle --no-capture` for the figures after the per-section capture split
   and the circular-rule count.
2. Try the new capture flow live: add a row to one catalogue sheet, regenerate,
   `cargo xtask capture --pending --list`, then `cargo xtask capture --pending` (its first run
   against Excel), and commit what it writes.
3. dc-ymjf.2: the witness judge and 278 pinned sets are committed in the `DnaCalc-lic` clone
   (f9f5453, merge 81aa23d) but not pushed. That clone is in the middle of a merge with master:
   conflicts in `tools/xtask/Cargo.toml`, `tools/xtask/src/main.rs` and `tools/calc-verdict`
   (the per-section capture and sweep changes landed meanwhile). Resolve keeping both sides,
   make `cargo xtask check` pass there, push.
   Then open root-cause beads for the IRR hang (`IRR({-1000, 1000-e, tiny}, guess)` never
   returns), and the ERF.PRECISE, MINVERSE and GAMMA.DIST mismatches; the full-bank comparison
   with OxFunc's ledger is in that clone's `target/witness/full-vs-oxfunc.tsv`.
4. Recheck the Intel/AMD EXP and LN differences on one Excel build (the July runs used Intel
   build 18429 and AMD build 20131), and record the result in the handbook's F19 data.
5. Use a commit-message file name unique to your lane; agents sharing one file overwrote each
   other's messages.

Kept by the coordinator or the owner, not a work set: the licence history rewrite (dc-96l.7,
force pushes), each publish to dnacalc.com (dc-ysl), and archiving legacy repositories.

## Architecture rules worth knowing

- DNA OneCalc depends only on calc-fml, calc-func and below, never on calc-engine, calc-doc or
  calc-host: it is the check that the formula layer stays simple and separate. Shared UI lives in
  one library with the same limit. DNA Calc is the full product over calc-host.
- One evaluator, one catalogue oracle, one corpus harness, one verdict path: extend, never copy.

## Fresh numerical checks, 2026-10-31

Run4 blind firehorse answers were committed before predictions were opened:
[CalcFunc-Evidence 43022fef](https://github.com/DnaCalc/CalcFunc-Evidence/tree/43022fef/probes/2026-10-31-calcvba-run4),
then [frozen scores 14ce2184](https://github.com/DnaCalc/CalcFunc-Evidence/tree/14ce2184/probes/2026-10-31-calcvba-run4).
Retained frozen models match PMT78/132, GAMMA.DIST A34/128, BINOM.DIST25/120 and
POISSON.DIST31/161. Poisson includes four changed subnormal inputs as unread
failures. These targeted questions are separate from a whole-function pass rate.

`dc-ymjf.5` retained independent readings and pre-reading predictions in
[CalcFunc-Evidence a8a001a9](https://github.com/DnaCalc/CalcFunc-Evidence/tree/a8a001a9/probes/2026-10-31-calcvba-run2).
Against the frozen `379d51f` model, ERF.PRECISE matched 3908/4096 new inputs with
abs(x)<0.01 (188 misses, worst 3 ULP). GAMMA's `df1215e` changed-input rechecks
matched 2/97, and 194 new adjacent inputs matched 4; zero inputs changed or went
unread. All 97 rechecked Excel answers agree with their recorded banks. The
numerical lane retired that GAMMA fit in `17e42ce`; these frozen counts do not
describe the later kernel. Current full-bank status now includes the new answers:
ERF.PRECISE 617129/618437, GAMMA 442/12506, on native Windows at `e3c3669`.
Both numerical parity areas remain open; ERF's next work stays in `dc-j9k.6`.
