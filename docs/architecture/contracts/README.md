# Crate contracts

The eight core crates keep a `CONTRACT.md` beside their source. Read it for the crate's role,
entry points, invariants and tests. The original drafts and their verification notes are in
commit `61b9ae5c`. The layering rules are in `tools/xtask/architecture.json`.

Run `cargo xtask health --json` to get each contract's current `contract_surface`. It is a
stable hash of the sorted public declaration lines in the crate's non-test source. The final
`Surface:` line of each contract records the reviewed hash. The regular health table lists
each mismatch, and the weekly keeper uses the same report.

If the public declarations change, review the contract against the code, update its description
where needed, and record the current hash. The hash tracks the declaration lines. The contract's
behavioral guarantees still require review.
