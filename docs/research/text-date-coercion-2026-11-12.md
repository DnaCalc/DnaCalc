# US numeric date coercion (dc-3k5n), 2026-11-12

Implementer: Codex on dna-koderbot. Sources: existing DNA Calc code and Excel answer
service outputs (tier 1). No Microsoft source, disassembly or restricted code read.

## Predictions frozen before the first Excel request (before the 03:29 UTC file-queue answers)

For en-US, three numeric components use month/day/year, with a leading four-digit
year selecting year/month/day. Two-digit years 00..29 select 2000..2029 and
30..99 select 1930..1999. Month/day tokens longer than two digits and invalid
calendar days are rejected. Slash and hyphen separators are equivalent. Two
components select month/day of the calculation year, else month/year or year/month.

Predicted DATEVALUE results: 04/02/01 and 4/2/2001 = 36983; 12/31/29 = 47483;
1/1/30 = 10959; 13/02/01 and 2/30/01 = #VALUE!; 04/02 = April 2 of the
calculation year; 2001-04-02 = 36983. VALUE and arithmetic coercion should use
the same date grammar; YEAR/MONTH/DAY extract 2001/4/2 from 04/02/01.
A service using en-ZA/YMD instead should interpret 04/02/01 as 2004-02-01
and reject 4/2/2001: retain that locale fact rather than relabel its outputs US.

Further discrimination: 4/2/001, 004/02/2001, 4/002/2001, 2001/4/2,
4-2-2001, 4/2-2001, 2/29/1900, 2/29/2000, 2/29/2001,
4/2001, 2001/4, 13/2, 30/2, 2/30, 12/31/00, 1/1/99, 1/1/100,
04/02/01 12:30, 1/1/1904, 12/31/1899, and whitespace around a date.

## First observations (03:29–03:30 UTC)

The advertised Unix socket is absent. Automatic Client fallback uses the v1 file
queue, which answers promptly. The first process was interrupted after its first
two answers while its other calls were pending; those raw queue replies remain
retained. An explicit file-queue repeat at 03:30:26 UTC confirms DATEVALUE:
04/02/01 = 38018 (2004-02-01), 4/2/2001 = #VALUE!, 12/31/29 = #VALUE!,
1/1/30 = 36921 (2001-01-30), 13/02/01 = 41306 (2013-02-01),
2/30/01 = #VALUE!, 04/02 = 46114, 2001-04-02 = 36983.
Excel build 20527, env_id 29bedb2b6722, GOVERT-OMEN; compatibility 2,
1900 dates, precision-as-displayed false. Inputs were echoed unchanged.
The numeric interpretation establishes YMD, not US MDY. No MDY implementation
is justified by these observations. US-session availability requested from owner.
Raw answer: ~/codex-runs/fn2/date-probe-file-1.json.

## Literal-versus-cell control, frozen before query

The bead's exact formulas use literal text; the first batch transported text
through cells. Predict DATEVALUE/VALUE give the same YMD reading for literal
and cell transports. A difference would make that distinction a prerequisite
for the US interpretation. Ask both transports on the same discriminating
strings, preserving the literal flags and echoed inputs in the raw answer.

## Literal control result

DATEVALUE and VALUE agree bit for bit between cell and literal transports on
all eight discriminating strings, 32 calls. Raw: ~/codex-runs/fn2/date-literal-control.json.
Thus literal transport does not supply the missing US measurement. No date
parser change lands from these year-first answers; dc-3k5n remains open.

## Existing evidence audit

The sibling CalcFunc-Evidence run w111-locale-profile-20260929 (Excel 20430)
also records year-first numeric dates: 2026/9/2 is 46267, 26/9/29 is 46294,
and 1/2/2026 is #VALUE!. Its 32 DATEVALUE controls therefore do not supply the
missing MDY observations. Source hashes and selected inputs/outcomes are in
date-evidence-locale-audit.json. The service client has no locale-selection
parameter; no registry or regional setting was changed. The US bead stays open.

Later evidence: R2 dnacalc-evidence/research/round56/20261112-fn2-tdensity-0657/
(114 files, 178,812,671 bytes for the batch; checksum copy and one-way check,
zero differences). No additional implementation change is claimed here.

Final service recheck before stopping: freeze the prior YMD predictions
and ask the two distinguishing DATEVALUE inputs again. The local socket
is still absent; the working file queue still returns 38018 for 04/02/01
and #VALUE! for 4/2/2001. The Excel environment is unchanged. A US-MDY
session remains unavailable; no US parser rule or 18,008-cell gain is claimed.

The final recheck's text/type echo is verified after omitting only v1's
unreturned cell-mode metadata. Its observations and frozen predictions are
in the verified round60 table-batch evidence prefix (32 files, 145,160,194
bytes for that batch). No date-input value or parser expectation was changed.
