dnacalc @VERSION@: the DNA Calc runner for Linux (x86-64, static, no dependencies)

Open an Excel workbook, recalculate it with the DNA Calc engine, and compare, read, edit or save.

  dnacalc diff   book.xlsx             recalculate; compare every formula cell with the file's cached values
  dnacalc recalc book.xlsx --out new.xlsx
  dnacalc get    book.xlsx Sheet1!A1 B2
  dnacalc set    book.xlsx A1 "=1+2" --out new.xlsx
  dnacalc run    book.xlsm --macro Module1.MyFunction --arg 4 --macro-level safe
  dnacalc serve                       open the printed URL in Firefox or Chromium; Ctrl+C stops it
  dnacalc --help | --version

Add --json for one JSON document on stdout. The file's VBA project runs only with
--macro-level safe (operating-system services refused) or unguarded; the default is off.
Exit status: 0 done (diff: all cells matched), 1 done with differing or refused cells, 2 usage
or a typed refusal. A formula the engine does not model is a counted refusal, never a pass.

serve carries the workbook page inside the executable. Its WebSocket uses the native workbook
worker. Open and save use your browser's file picker and downloads. It binds only to 127.0.0.1,
chooses a free port and a fresh private URL each run; keep that URL private. --port N selects a
port. Macros start off, as in the web app; XLL and COM add-ins are unavailable on Linux.

Install: unpack the tarball and copy `dnacalc` anywhere on PATH, or install the .deb
(`sudo apt install ./dnacalc_@VERSION@_amd64.deb`).

LICENSE is the MIT licence of DNA Calc. THIRD-PARTY-NOTICES.txt lists the notices of ported
code and the crates linked in. https://dnacalc.com
