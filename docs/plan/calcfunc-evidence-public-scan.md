# CalcFunc-Evidence public-readiness scan

The scanned snapshot needs provenance and privacy cleanup before publication. The archive contains
vendor-derived algorithm material, reference-library outputs and identifying metadata alongside the
recorded Excel answers.

## Scope and checks

The read-only checkout and the remote HEAD both resolved to
`4ef2c244d73c6c4ca9b69641f9d95b72c806ed7e`. The scan covered that commit's entire tree and
used the archive's `archive-evidence.py restore` command on each of the four sets. Paths below
refer to restored logical names. In Git, those files have an added `.gz` extension.

| Scope | Files | Uncompressed bytes |
| --- | ---: | ---: |
| `manifest.tsv` (`cache/` and `runs/`) | 28,333 | 2,703,552,956 |
| `w109-manifest.tsv` | 1,363 | 2,086,703,406 |
| `studies-manifest.tsv` | 29 | 1,085,819,508 |
| `discrepancies-manifest.tsv` | 24 | 22,384,811 |
| Tracked files outside the four manifests | 2,349 | |
| **Total** | **32,098** | |

All 29,749 manifest entries restored and passed the archive's raw size and SHA-256 checks.
An independent comparison against the manifest gzip sizes and SHA-256 digests also found zero
failures. Each entry mapped to one tracked blob. The remaining 2,349 tracked files included
plain readings, probes, recaptures, function-lane notes, manifests and exclusion lists. The scan
opened 16 workbook ZIP packages as data and inspected all 500 members. The packages contained
468 XML parts and 32 relationship parts. No external relationship targets, VBA projects,
embedded objects or external-link parts were present.
The scan decoded UTF-16 text and screened the one Python bytecode file as bytes.

Every file was screened for credential markers, identifiers, third-party source and licence
markers, algorithm transcriptions, and Microsoft material. Targeted review of flagged research
notes, code snapshots, runtime outputs and package metadata supplied the findings below.
Pattern screening identifies candidates for review. Origin and redistribution rights for material
without identifying markers require source records.

## Findings

| Path | Finding | Class | Proposed action |
| --- | --- | --- | --- |
| `function-lane/notes/w109/W109_GAMMALN_IDENTIFICATION_20260711.md:378` | An fdlibm polynomial is written out and the coefficients are explicitly attributed to fdlibm verbatim. | Third-party algorithm transcription | Remove the transcribed expression from the public notes or establish the source revision and carry its notice. Preserve the Excel measurements and the team's observations. |
| `function-lane/notes/w109/W109_G3-01_GRATIO_IDENTIFICATION_20260716.md:196,456`, `function-lane/notes/checkpoints-b2f4873e/G301_A2_TAYLOR_EXP_STAGING_PARTIAL_20260809.md:40` | The notes describe a transcribed TOMS 708/BRATIO dependency tree, TOMS 654/NSWC series used to stage candidates and bit-faithful AMD/Open64 transcriptions. They also report measurements from Microsoft CRT DLLs. Those references identify provenance for the candidate work. | Published algorithms and Microsoft runtime material | Trace the referenced candidates through retained notes, snapshots and history. Publish original discussion and Excel observations with clear provenance. Remove or separately clear copied expressions and runtime material. |
| `work/w109/G3-01-dist/agentK_decode_spec.md:143`, `function-lane/notes/w109/W109_ERFC_BODY_ATTACK_PLAN_20260823.md:23` and related W109 notes | The plans name AMD/Open64, Boost, Cephes and fdlibm implementations as comparison candidates. The Boost/Cephes keyword screen flagged 51 paths, including 28 W109 files. A name or benchmark alone is evidence of a reference, while the same notes can contain operation sequences or coefficient tables. | Candidate provenance review | Keep citations and original comparisons. Check embedded excerpts, coefficients and operation sequences against their sources before clearance. |
| `runs/w108a-reference-hunt/{ucrt_out.txt,glibc229_out.txt,vml_out.json,results.json}` and `ucrt_probe.c` | The retained outputs include UCRT, glibc and Intel VML reference runs. The C probe calls the runtime. The summary identifies the glibc run as built from a verbatim transcription. | Microsoft material other than recorded Excel answers, and other vendor measurements | Hold the UCRT reference output from the public answer set. If vendor comparison outputs are retained, document each source and give them an explicit comparison role. Review the probe as code. |
| `runs/w111-log-power-midpoint-20260929/baseline-source/power_fn.rs:81`, `runs/w111-*/candidate/`, `runs/w111-*/*.snapshot`, `cache/w111-depreciation-20260929/*.rs` | The archive contains 403 source-like paths across the restored sets and loose files, including OxFunc snapshots, probe scripts and generated candidates. The POWER comment attributes its algorithm to a reverse-engineering campaign with live Excel rows. A current product-code licence review cannot be carried over automatically to older copied snapshots. | Source provenance unresolved | Inventory the copied revisions and required notices, and remove or clear each retained snapshot. Treat the observed answer rows independently of the source copies. |
| `w109-excluded.tsv`, `w109-held.tsv`, `excluded.tsv` and `README.md` | The exclusion and hold lists contain 1,214 W109 exclusions, five explicitly held paths and 32 excluded Intel MKL files. None of those listed file paths overlaps the corresponding stored manifest. The W109 filter names CRT, Boost, Cephes, Numerical Recipes, AMD libm and AS algorithms. The README records earlier glibc and OxFunc source removals and says the oracle archive command can re-admit the legacy source copies. | Exclusion records and re-admission risk | Keep a sanitized exclusion record. Prevent re-admission in any regenerated public archive, and review current and historical blobs separately. |
| `runs/w108-b2-financial/__pycache__/fin.cpython-312.pyc` | Generated Python bytecode is present in the archived run. | Build artifact and provenance | Exclude the bytecode. Review and retain source only where its origin is established. |
| `probes/2026-11-03-{fn1,r16}/worker/capture/*/out/oracle.xlsx!/docProps/core.xml` | Six Excel-produced workbooks carry nonempty `lastModifiedBy` values. Their parts also carry creation and modification timestamps. | Personal metadata | Publish sanitized copies or approved answer exports. Preserve the original capture lineage and record hashes for any replacement packages. |
| `README.md`, `runs/`, `probes/`, `cache/`, `readings/`, `recaptures/` and `work/w109/` | The README identifies a capture machine. Host, author and path field screening flagged 3,047 files, including 151 W109 paths. Eighteen standalone files matched the stricter user-path or machine/author marker scan. | Personal and infrastructure metadata | Remove direct identifiers, replace local paths with relative provenance references, and preserve Excel build, CPU and capture date where needed for reproducibility. |
| All 32,098 paths and 500 package members | The credential-format, credential-assignment and email-address checks returned zero matches. Review of 83 credential-context hits found 82 workbook password-parameter references in worker scripts and one documentation reference in the harvester. | Credential screening | Repeat screening on the proposed release tree and its reachable history. |
| `LICENSE.md` and `README.md` | The licence assigns CC BY 4.0 to recorded answers, notes and generated evidence, and points scripts and probes to the handbook's MIT terms. The archive also contains older source snapshots and explicitly attributed material. | Licence scope | Identify the licensed material and any exceptions per path or class. Resolve licences and notices before applying the public licence statement to the rest of the archive. |

The 29 study blobs and 24 discrepancy blobs were included in the same screens. The study and
discrepancy sets had no credential, host/author field or source/transcription marker hits in those
checks. A separate discrepancy-file provenance hit came from its `reported_by_lane` field next
to the Excel build. The Microsoft document/source screen flagged 149 paths. Of those, 144
referenced the Excel executable in workers or related records, and five mentioned the Excel
binary in research policy text. The retained UCRT outputs were identified by their provenance
and filenames.

`dc-cfe-public-cleanup-tmoje` owns archive cleanup, notices, publication review and reachable
history checks. The checkout used for this scan was shallow. Historical versions and refs
outside the pinned tree require a separate scan, including the source paths the archive README
says were removed. This work produced a review of the pinned snapshot and left the evidence
archive untouched.
