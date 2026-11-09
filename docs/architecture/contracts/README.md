# Crate contract documents: drafts and how to keep them current

Drafts written 2026-11-09 by Claude Fable 5.2 (claude-fable-5-2) and three read-only subagents, from DnaCalc
`origin/master` cb0b76a5, committed unreviewed as `crates/<crate>/CONTRACT.md` (bead dc-kapl.2). First batch: `calc-value-CONTRACT.md`,
`calc-fml-CONTRACT.md`, `calc-engine-CONTRACT.md` (the crates the evaluator rework touches). All eight main crates are drafted;
the five later ones (calc-func, calc-catalogue, calc-host, calc-doc-model, calc-doc) end with "Findings for the tracker".

## What a contract is for

An agent that has to call a crate, or change it, reads the contract first and the code second. Each one says:
purpose and place in the layering; public API by concept with paths; guarantees to callers and invariants relied on;
what the crate must never do; the Excel behaviours it owns with their measurement; how it is tested; open items.
Under 150 lines. A line that cites `path:line` was read there; a line marked "(inferred)" was not.

## Where they should live

`crates/<crate>/CONTRACT.md`, beside the code, so a change to the crate and to its contract is one diff. A one-line
pointer in each crate's `lib.rs` module doc (`//! Contract: CONTRACT.md`) is enough for an agent to find it.
AGENTS.md "Read first" gets one line: "before working in a crate, read its CONTRACT.md".

## Keeping them current cheaply

No per-commit check (owner ruling: cheap-to-fix rules are checked periodically, not per commit).

1. **Weekly, in `cargo xtask health`:** for each crate with a CONTRACT.md, compare the last commit that touched the
   contract with the last commit that changed the crate's public surface. Public surface, cheaply: the sorted list
   of `pub` item lines in `src/**/*.rs` outside `#[cfg(test)]` (a text scan, no compiler), hashed. The contract
   records that hash in its last line (`Surface: <hash> at <commit>`). When the hash differs, health lists the
   crate and the added and removed `pub` lines in its existing drift table. It files nothing by itself beyond the
   existing P2 "Code health drift" bead.
2. **Path and line citations:** the same scan checks that every `path` cited in a contract still exists. Line
   numbers are allowed to drift; a missing file is reported.
3. **Who updates:** the lane whose package changed the surface updates the contract in the same package, as it
   updates tests. The read-only review agent at package close reads the contract's diff against the code diff
   (AGENTS.md step 5 already has that reviewer). No separate documentation lane.
4. **Refresh by regeneration, not by editing prose:** when a contract has drifted for more than two reports, an
   agent redrafts it from the code with the prompt in this folder's history (the three prompts were: rules for
   reading at `origin/master`, the seven sections, `path:line` for verified facts, "(inferred)" for the rest, a
   closing verified/inferred/not-read note). A redraft costs one subagent run per crate.
5. **Bead ids:** open items cite bead ids; health can list ids that are closed in the tracker but still named as
   open in a contract (`br show` per id), in the same weekly table.

## What not to put in a contract

History, effort, plans (the rework plan and RECALC_CONTRACT.md hold those), anything already checked mechanically
(`tools/xtask/architecture.json` is cited, not copied), and measured Excel facts beyond a pointer to their evidence.
