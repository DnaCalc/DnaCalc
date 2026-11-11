# Packaged desktop apps

`cargo xtask package onecalc [--out DIR] [--no-zip] [--without-calc]` builds the release folders
(C runtime linked statically, each page embedded in its executable, no console window):

- `DNA-OneCalc/`: `dna-onecalc-desktop.exe`, `dna-calc-desktop.exe` (the companion that "Open in DNA Calc"
  starts), `XLCALL32.DLL`, `README.txt`, `LICENSE`; and `DNA-OneCalc-<version>-win-x64.zip`.
- `DNA-Calc/`: `dna-calc-desktop.exe`, `XLCALL32.DLL`, `README.txt`, `LICENSE`; and `DNA-Calc-<version>-win-x64.zip`.
- `package-manifest.json`: sizes, SHA-256 of every file and zip, executable imports, commit.

`cargo xtask package onecalc-clean-run --package DNA-OneCalc --work EMPTY --out DIR --x1 X1.xll --x5 X5Native64.xll`
copies the folder to an empty directory outside the repository and drives it (formulas, add-ins, save and
reopen, window place, close prompts, WebView2 missing, DNA Calc started from the folder). Run it through
`tools/excel-session/on-private-desktop.ps1` so no window reaches the desktop in use.

Both executables need the Microsoft Edge WebView2 Runtime and say so, with the download link, when it is
missing. A start-up failure shows a message box instead of silently ending (test flags report on stderr only).
`--version` prints the version. Settings and the browser profile live under `%LOCALAPPDATA%\DNA OneCalc`
and `%LOCALAPPDATA%\DNA Calc\webview2`.
