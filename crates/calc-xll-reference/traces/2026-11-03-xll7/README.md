R7 compares real Excel and DNA Calc boundary traces, with independent untraced
controls. Binary inputs remain outside the repository; their exact hashes are in
each environment record and the existing upstream/fixture manifests. No framework
binary, workbook answer or older failed capture was changed to obtain agreement.

| Profile | Measured result |
| --- | --- |
| pool | 6 cases; 15 native calls and 2 owned frees per host; two worker entries overlap |
| rtd | Original signaling generator; 8 ordered client calls per host |
| rtd-idle | Same generator plus 22,000ms idle; completed transport disconnects, no Heartbeat |
| rtd-numeric-idle | Persistent numeric topic; 7 calls including one Heartbeat per host |
| reload | Native Utils/SQL; 12 outcomes and 13 native calls before and after host reload |
| reload-new-host | Same measured native reload after replacing the host object |
| reload-override-refusal | Typed failure; deliberately not qualified for native cleanup |
| reload-python-refusal | Typed failure; Python runtime reinitialization remains unsupported |
| reload-renamed-failure | Original renamed loader crashed; no rename compatibility claim |

The original Excel reload fails in both controls. Its first load supplies the
complete reference calls; the failed second RegisterXLL, access violation, RPC
loss and debugger diagnostics remain explicit in both reload summaries. Security
was observed unchanged before teardown; the crashed process cannot supply an
afterward observation. Host traces require clean detach and complete call pairs.
They establish actual unload/reload cycles for the loader, core and both plugins.

The RTD destructor body is retained under the owning final COM Release, following
successful client termination. `negative-rtd-release/` changes only its final
reference count from 0 to 1; verification must fail with
`Internal RTD termination lacks the final owning Release`.

Run `../../verify.py` with each profile's Excel traced/control directories,
`--host`, `--host-control` and `--source-publication source-publication.json`.
Use `--workbook-pool-cases ../../fixture/x5-workbook-pool-cases.json` for pool;
`--rtd-cases ../../fixture/x5-rtd-cases.json` for ordinary RTD; the two retained
idle case files for their extensions; and
`--xloil-reload-cases ../../fixture/x3-xloil-cases.json` for reload. The new-host
comparison uses `reload/` for its Excel directories. Run these commands from this
directory, adjusting the verifier path to `../../verify.py` as shown.

`validation.json` and `validation/` retain the required per-commit checks.
`corpus-summary-comparison.json` separates the capture-only increase from incoming
function changes and linked-input provenance, with counts over all formula cells.
The last integration resolves the actual `cw-ceng/ylr/ylr-source.xlsx` package,
calculates its source workbook and connects its snapshot; the 73 formerly external
cache matches become calculated matches, with no destination-value change.
`corpus-linked-inputs.json` records the source path and SHA-256. All raw corpus payloads
and failed exploratory attempts remain in `C:/Work/DnaCalc/codex-runs/xll7`.
The unchanged R16 packet is in `codex-runs/excel-questions-r16`; its results are
in `codex-runs/excel-results-r16`, outside the repository.
The unmodified earlier R5/R6 failures remain in their original evidence folders.

These finite comparisons leave general Python/plugin teardown, topic reuse after
completion, remote/array RTD, spill concurrency and broader cancellation open.
No full sweep, Linux run or CI dispatch is claimed. Only scripts' owned Excel
processes were stopped; protected directories, unrelated Excel processes and Excel
COM/security registrations were untouched.
