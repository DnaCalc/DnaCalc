# Programme: using the pro-9 GPT-6 Pro answers

Five single-shot GPT-6 Pro questions were asked on 2026-11-10 and answered the same day. Prompts and answers are kept
next to this note in `pro/`:

| # | Topic | Pinned commit | Prompt | Answer |
|---|---|---|---|---|
| 9 | Gamma family last bits (GAMMA, GAMMALN, incomplete gamma, CHI, POISSON, inverses) | b2a80850 | `pro/9-gamma-family-prompt.md` | `pro/9-gamma-family-answer.md` |
| 10 | Beta, F and t family (BETA.DIST, FDIST, F.TEST, TDIST, inverses, discrete wrappers) | b2a80850 | `pro/10-beta-family-prompt.md` | `pro/10-beta-family-answer.md` |
| 11 | Solvers (YIELD, IRR, BINOM.INV, CUMIPMT/CUMPRINC, DURATION/MDURATION/PRICE) | b2a80850 | `pro/11-solvers-prompt.md` | `pro/11-solvers-answer.md` |
| 12 | FORECAST.ETS reconstruction (952 refused cells) | 1ece3d0f | `pro/12-ets-prompt.md` | `pro/12-ets-answer.md` |
| 13 | Hosting Excel add-ins (XLL, COM, RTD) in OneCalc and DNA Calc | 1ece3d0f | `pro/13-addin-hosting-prompt.md` | `pro/13-addin-hosting-answer.md` |

Tracker: epic **dc-6332**. Children:

| Bead | Work | Model |
|---|---|---|
| dc-6332.1 | Careful read of each answer: harvest index and claim check (`pro/harvest/<n>-harvest.md`) | Opus, one reader per answer |
| dc-6332.2 | Solvers: replay the answer 11 candidates on the full bank | Sonnet, Opus checks the close |
| dc-6332.3 | Gamma family: replay the answer 9 candidates | Sonnet, Opus checks the close |
| dc-6332.4 | Beta family: research context interface and the answer 10 substitutions | Sonnet; Opus reviews the interface first |
| dc-6332.5 | FORECAST.ETS: p2,n4 routing and the identification harness | Sonnet, Opus oversight |
| dc-6332.6 | Solver Excel experiments | Codex or headless Fable 5.2, Opus oversight |
| dc-6332.7 | Gamma family Excel experiments | Codex or headless Fable 5.2, Opus oversight |
| dc-6332.8 | Beta family Excel experiments | Codex or headless Fable 5.2, Opus oversight |
| dc-6332.9 | FORECAST.ETS Excel experiments | Codex or headless Fable 5.2, Opus oversight |
| dc-6332.10 | Add-in hosting steps 0-1: comparison profile, ABI boundary, ownership | Opus (Windows) |
| dc-6332.11 | Add-in hosting step 2: status 64 and ordered volatility | Opus (Windows) |
| dc-6332.12 | Add-in hosting steps 3-4: COM, QueueAsMacro, exact RTD | Sonnet with Opus review (Windows) |
| dc-6332.13 | Deferred parts, revisited each usage week | orchestrator |
| dc-6332.14 | XLL invocation-time volatility events (split from .11; waits on dc-hsa5.20.2) | Opus (Windows) |
| dc-6332.15 | Add-in surrogate: boundary design note | Opus |
| dc-6332.16 | COM identity, apartments and a no-Office lane (deferred) | later |
| dc-6332.17 | HYPGEOM.DIST cumulative: bound the per-call work at large populations | Sonnet |

The careful reads (dc-6332.1, closed) are in `pro/harvest/<n>-harvest.md`: every section's disposition, each claim
checked against our code, the experiments with call counts, and Python replays of candidates on the witness traces.
Brief every lane with its answer's harvest file as well as the answer. Each child bead carries the work order from its
harvest file as a comment.

## What the answers are

None of the numeric answers claims a complete bit-exact algorithm, and each says so plainly. Their value is in three
kinds of content:

- **Replayable candidates** that need no new Excel answers: exact computations with predicted bits for rows already
  in the bank (for example, the YIELD total coupon formed as `n * rate`, the IRR raw-guess initial residual, the stored
  generic GAMMA seed for negative half-integers, the three `bgrat` substitutions, the ETS p2,n4 routing).
- **Discriminating Excel experiments**: input sets with a stated outcome per hypothesis, sized in Excel calls (most
  batches are tens to a few thousand calls, which the Excel answer service handles in seconds).
- **Method and corrections**: where our earlier conclusions were overstated, and how to read rounded outputs as
  constraints on hidden intermediates.

Answer 13 is different: a gap analysis, an architecture critique and a staged delivery order (steps 0-11) with an
oracle experiment per step.

## Schedule

Ordered by the usage windows we have; nothing here has a calendar estimate of effort.

1. **This week, before the Claude reset (Thu 2026-11-12 22:00 local).** Archive (this commit), beads, and the five
   careful reads (dc-6332.1). The orchestrator then updates the child beads from the harvest files. If Claude room
   allows, start dc-6332.2 (solvers), which has the most rows reachable without new Excel answers.
2. **From the Codex reset (Fri 2026-11-13 11:21 local).** The Excel experiment rounds, solvers first (dc-6332.6),
   then gamma, beta and ETS (dc-6332.7-.9), on the Linux hosts through the Excel answer service. One Terra evaluation
   at a time, logged in `codex-runs/model-eval.md`.
3. **The next Claude week.** Sonnet replays for gamma, beta and ETS (dc-6332.3-.5); add-in hosting steps 0-1
   (dc-6332.10) when local Rust work on the Windows machine is allowed, then step 2 and steps 3-4.
4. **Each new usage week.** Review dc-6332.13: move items whose prerequisites have landed into new beads.

## Rules that apply to every bead here

- Landing rule for function changes: a clear net gain on the full bank with a computation that makes sense; rows lost
  are listed in the commit and close note. Zero loss is not required.
- Code-shape method: find the shape first, then fit constants. Freeze predictions before asking Excel.
- Source tiers from `docs/plan/RESEARCH_SOURCES.md` apply to anything an agent adds beyond the answers.
- Close notes quote numbers from tool output that the closing agent ran.
