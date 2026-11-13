# Packaged desktop apps

`cargo xtask package onecalc [--out DIR] [--no-zip] [--without-calc] [--target x86_64-pc-windows-msvc]` builds the release folders
(C runtime linked statically, each page embedded in its executable, no console window):

- `DNA-OneCalc/`: `dna-onecalc-desktop.exe`, `dna-calc-desktop.exe` (the companion that "Open in DNA Calc"
  starts), `XLCALL32.DLL`, `README.txt`, `LICENSE`; and `DNA-OneCalc-<version>-win-x64.zip`.
- `DNA-Calc/`: `dna-calc-desktop.exe`, `XLCALL32.DLL`, `README.txt`, `LICENSE`; and `DNA-Calc-<version>-win-x64.zip`.
- `package-manifest.json`: sizes, SHA-256 of every file and zip, executable imports, commit.

On Linux the Windows executables use `cargo xwin build --release`; on Windows they use `cargo build`
with the same explicit Windows target. See [WINDOWS_CROSS_BUILD.md](../docs/plan/WINDOWS_CROSS_BUILD.md)
for xwin prerequisites and the already accepted SDK licence. Page builds still use the web profile.
Both page and Windows builds explicitly use this workspace's `target` directory, matching the
artifact paths the packager reads even when `CARGO_TARGET_DIR` is inherited from the caller.

`cargo xtask package installer [--out DIR] [--wixl PATH]` builds those folders, then two unsigned
per-user MSIs with wixl (msitools 0.106). To package existing folders without rebuilding, pass
`--package DIR`; both folders and their `package-manifest.json` must be present. The command checks
every input size and SHA-256, takes the version and commit from that manifest, and writes:

- `DNA-Calc-<version>-win-x64.msi` and `DNA-OneCalc-<version>-win-x64.msi`;
- `Install.ps1`, the online WebView2 prerequisite installer and per-user MSI launcher;
- `installer-manifest.json` (input hashes, output hashes, version, commit, ProductCodes) and `SHA256SUMS`.

Run `powershell -NoProfile -ExecutionPolicy Bypass -File .\Install.ps1` from a normal, unelevated
64-bit console for unsigned test artifacts (the execution policy applies to that process only).
`-Product DNA-Calc` or `-Product DNA-OneCalc` selects one product; `-RuntimeOnly` checks/installs
WebView2 without installing either app. A missing runtime is downloaded from Microsoft's Evergreen
bootstrapper URL, checked for a valid Microsoft Authenticode signature, installed, then checked again.
This follows [Microsoft's runtime deployment guidance](https://learn.microsoft.com/microsoft-edge/webview2/concepts/distribution).
The MSIs themselves check the documented HKCU/HKLM runtime keys and explain how to run the bootstrap
when missing. Uninstall and maintenance do not require the runtime. No MSI custom action downloads code.

The MSIs install in `%LOCALAPPDATA%\Programs\DNA Calc` and `...\DNA OneCalc`, register Start menu
shortcuts and Open-with entries, and appear in Apps and Features. Newer versions replace the installed
program files; settings in `%LOCALAPPDATA%\DNA Calc` and `...\DNA OneCalc` remain through upgrade
and uninstall. Excel's default associations are not changed. ProductCodes are stable per product and
numeric MSI version; never ship changed payloads under an already released version. MSI timestamps
and PackageCodes can differ between builds; reproducible MSI bytes are not claimed.

These outputs are unsigned. The owner signs payloads first, updates their entries in the package
manifest to the signed bytes, and rebuilds with `--package`; after signing the resulting MSIs, the owner
also updates the MSI hashes in `installer-manifest.json` and regenerates `SHA256SUMS` before distribution.
`Install.ps1` verifies those MSI hashes. Agents do not access signing material.

`cargo xtask package onecalc-clean-run --package DNA-OneCalc --work EMPTY --out DIR --x1 X1.xll --x5 X5Native64.xll`
copies the folder to an empty directory outside the repository and drives it (formulas, add-ins, save and
reopen, window place, close prompts, WebView2 missing, DNA Calc started from the folder). Run it through
`tools/excel-session/on-private-desktop.ps1` so no window reaches the desktop in use.

Both executables need the Microsoft Edge WebView2 Runtime and say so, with the download link, when it is
missing. A start-up failure shows a message box instead of silently ending (test flags report on stderr only).
`--version` prints the version. Settings and the browser profile live under `%LOCALAPPDATA%\DNA OneCalc`
and `%LOCALAPPDATA%\DNA Calc\webview2`.
