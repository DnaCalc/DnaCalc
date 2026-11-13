# Windows installers: format, build host and update path (dc-fk66.1)

Draft for the owner, 2026-11-13. Decision proposed: **per-user MSI, built on Linux with wixl
(msitools), signed by the owner with signtool on Windows.** An unsigned MSI was built on dna-firehorse
as proof (section 8). dc-fk66.2 builds the real ones, dc-fk66.3 tests them on a clean machine.

## 1. What is installed

Today `cargo xtask package onecalc` makes two folders (apps/PACKAGING.md):
- `DNA-OneCalc/`: `dna-onecalc-desktop.exe`, `dna-calc-desktop.exe` (the companion "Open in DNA Calc"
  starts), `XLCALL32.DLL`, `README.txt`, `LICENSE`;
- `DNA-Calc/`: `dna-calc-desktop.exe`, `XLCALL32.DLL`, `README.txt`, `LICENSE`.

The pages are embedded in the executables, the C runtime is linked statically (`package.rs` refuses
an exe that imports the VC++ redistributable), and no `WebView2Loader.dll` is imported (section 8). Settings and
the browser profile live under `%LOCALAPPDATA%\DNA OneCalc` and `%LOCALAPPDATA%\DNA Calc\webview2`.
So an installer only has to copy five files, add a Start menu entry and "Open with" entries, and
remove them again. Two MSIs, one per folder, with their own upgrade codes.

## 2. The choice

| | MSI by wixl (msitools 0.106) | MSI by WiX 7 | MSIX | Zip only (today) |
|---|---|---|---|---|
| Builds on Linux | **yes, measured** (section 8) | no: `wix 7.0.1` on this host prints "The WiX Toolset only supports Windows" | packing tool `makemsix` exists for Linux; not tried | yes |
| Licence | LGPL tool, output unrestricted | WiX 6 and 7 need the Open Source Maintenance Fee EULA accepted (`wix build` here: error WIX7015). Not accepted: the owner's decision | Windows SDK tool | none |
| Per-user, no admin | yes: `InstallScope="perUser"`; summary Word Count 10 = compressed + no elevation (measured) | yes | yes | yes |
| Upgrade | `MajorUpgrade`: Upgrade table and `RemoveExistingProducts` at 1401 (measured) | yes | yes, by package version | user replaces folder |
| Uninstall | Apps and Features; removes files, shortcut and registry values | yes | yes, and its data folders | user deletes folder |
| "Open with" without the default | HKCU `OpenWithProgids` and `Applications\<exe>` keys | the same | manifest file-type associations | none |
| Unsigned test install | yes (SmartScreen warns) | yes | no: Windows refuses unsigned packages outside developer setups (documented, not tried) | yes |
| Files the app writes | real `%LOCALAPPDATA%` paths | the same | packaged desktop apps get some AppData writes redirected (documented, not tried); the XLL and WebView2 profile paths would need testing | real paths |
| winget | `InstallerType: msi`, `Scope: user` | the same | `msix` | `zip` (portable) |

MSIX is the cleaner format on paper, but it cannot be tested unsigned, its file redirection is a risk
for add-in loading, and no MSIX tool was run here. WiX 7 needs Windows and a licence step. wixl
builds on the Linux hosts that already cross-build the executables, so the whole chain stays on Linux
until the owner signs. **Proposal: wixl MSI now; look at MSIX again only if the Store becomes a goal.**

wixl gaps found so far: it ignores `RemoveFolder/@Directory` (warned, dropped from the source), runs no
ICE validation, and leaves the File table's Version column empty (files are hashed instead; harmless
because the upgrade removes the old product first). ProductCode `*` is new on every build, so two
builds of one commit differ (section 7).

## 3. Per-user install, upgrade, uninstall

- **Where:** `%LOCALAPPDATA%\Programs\DNA Calc\` and `...\DNA OneCalc\`. No admin prompt.
  Registry under HKCU only: `Software\DNA Calc\<product>` (key paths) and `Software\Classes`.
- **Start menu:** one shortcut per product.
- **Upgrade:** a newer version (by ProductVersion, `0.MINOR.PATCH`, docs/plan/RELEASES.md) removes the
  older install first, then installs. A downgrade stops with "A newer DNA Calc is installed".
- **Uninstall:** Settings > Apps, or `msiexec /x <msi>`. It removes the program folder, shortcut and
  registry values. It leaves `%LOCALAPPDATA%\DNA Calc` and `...\DNA OneCalc` (settings, recent files,
  WebView2 profile), as the READMEs say; the user deletes those to forget them. Proposal: keep that.
- **Both installed:** they share nothing on disk. OneCalc's companion `dna-calc-desktop.exe` registers
  nothing, so it never fights the DNA Calc MSI over `Applications\dna-calc-desktop.exe`.

## 4. WebView2 runtime

Both executables need the Evergreen WebView2 Runtime. When it is missing they say so and open
Microsoft's download page (apps/PACKAGING.md; `calc_platform_desktop::RUNTIME_DOWNLOAD`).
Proposal for the first preview: **no bootstrapper in the MSI.** The MSI stays one file with no custom
actions, and the app's own message covers the missing case. Windows 11 includes the runtime. Whether
a clean Windows 10 machine has it is not measured; dc-fk66.3 records it. If it is missing there, the
next step is a launch condition that finds the runtime's registry key and points to the download.

## 5. "Open with", never the default

The MSI writes, under `HKCU\Software\Classes`:
- a ProgID (`DnaCalc.Workbook`, `DnaOneCalc.AddIn`) with an icon and `shell\open\command`;
- the ProgID as a value under `.<ext>\OpenWithProgids`, which lists it in "Open with" and does not
  change the extension's default;
- `Applications\<exe>\SupportedTypes` and `FriendlyAppName`.

| Extension | Opened by | Command |
|---|---|---|
| .xlsx .xlsm .xlsb | DNA Calc | `dna-calc-desktop.exe --open "%1"` (main.rs:71: `--open FILE` is app mode) |
| .xll .xlam | DNA OneCalc | `dna-onecalc-desktop.exe --app --xll "%1"` (app.rs options: `--app`, `--xll`) |

Not measured yet (dc-fk66.3): that `--app --xll` loads an `.xlam` from a double-click as the Add-ins
panel does; that Windows shows both entries; that Excel stays the default. DNA OneCalc does not take
a plain file argument today; `.xlsx` "Open with" for OneCalc needs that first.

## 6. Signing: the owner's steps

Agents never touch the certificate or its PIN. The owner signs on the Windows machine with signtool
from the Windows SDK (`C:\Program Files (x86)\Windows Kits\10\bin\<version>\x64\signtool.exe`).
These steps assume the code-signing certificate is in the owner's certificate store (a hardware
token or an imported file); `<thumbprint>` is its SHA-1 thumbprint (`certmgr.msc`, or
`Get-ChildItem Cert:\CurrentUser\My -CodeSigningCert`). `<ts>` is the timestamp URL the certificate
authority gives.

1. **Check what you sign.** In the candidate folder (RELEASES.md section 7) run
   `certutil -hashfile <file> SHA256` for each exe and DLL and compare with `SHA256SUMS`.
2. **Sign the executables and the DLL** (both folders, before the MSI is built, so the installed
   files carry a signature):
   ```
   signtool sign /fd SHA256 /sha1 <thumbprint> /tr <ts> /td SHA256 /d "DNA Calc" /du https://dnacalc.com ^
     DNA-Calc\dna-calc-desktop.exe DNA-Calc\XLCALL32.DLL ^
     DNA-OneCalc\dna-onecalc-desktop.exe DNA-OneCalc\dna-calc-desktop.exe DNA-OneCalc\XLCALL32.DLL
   signtool verify /pa /v DNA-Calc\dna-calc-desktop.exe
   ```
3. **Hand the signed folders back.** Copy them to dna-firehorse (`scp -r DNA-Calc DNA-OneCalc
   dna-firehorse:<candidate>/signed/`). An agent runs `deploy/windows/build-msi.sh <candidate>/signed`
   and returns the two MSIs with their hashes. Signing changes the exe bytes, so the hashes of step 1
   no longer apply; the agent records the signed hashes.
4. **Sign the MSIs:**
   ```
   signtool sign /fd SHA256 /sha1 <thumbprint> /tr <ts> /td SHA256 /d "DNA Calc" /du https://dnacalc.com ^
     DNA-Calc-0.2.0-win-x64.msi DNA-OneCalc-0.2.0-win-x64.msi
   signtool verify /pa /v DNA-Calc-0.2.0-win-x64.msi
   ```
5. **The zips** are rebuilt by the agent from the signed folders of step 2. A zip carries no
   signature of its own; the files inside it do.
6. Record the signed files' SHA-256 in the release notes, then publish (RELEASES.md section 7).

If the certificate is a cloud signing service rather than a store certificate, steps 2 and 4 change
to that service's signtool form; the order stays the same. Which certificate the owner holds is not
known to me.

## 7. What dc-fk66.2 still has to do

1. `cargo xtask package onecalc --target x86_64-pc-windows-msvc` on Linux: build with `cargo xwin`
   (docs/plan/WINDOWS_CROSS_BUILD.md) instead of the host cargo. Today the command builds the pages,
   then runs `cargo build --release` for the host, which on Linux is not the Windows exe.
2. Call `deploy/windows/build-msi.sh` from it, or from `cargo xtask release` (RELEASES.md section 9).
3. A ProductCode derived from UpgradeCode and version, so that one commit gives one MSI. Timestamps
   in the summary stream still differ; measure whether wixl can fix them.
4. The `ProductVersion` in `version.rc` (RELEASES.md section 3).
5. wixl in the build: either install `msitools` on the Linux hosts (owner's yes; it changes the
   host) or keep the unpacked .debs in a run folder as this trial did.

## 8. Proof: an unsigned MSI built on Linux (2026-11-13)

Built on dna-firehorse. Nothing was installed on the host: the msitools 0.106 .debs (wixl, wixl-data,
libmsi-1.0-0, libgcab-1.0-0, libgsf-1-114, libgsf-1-common, msitools) were downloaded with
`apt-get download` and unpacked into a run folder, and wixl ran from there with `LD_LIBRARY_PATH`.

1. Pages: `cargo xtask package onecalc --out <dir> --no-zip` on Linux wrote `page.block` (14 MB) and
   `calc-page.block` (33 MB), then stopped as expected at its host build ("cannot produce proc-macro
   ... x86_64-unknown-linux-gnu", the crt-static flag on a host build; item 1 of section 7).
2. Executables: `RUSTFLAGS="-C target-feature=+crt-static" DNACALC_ONECALC_PAGE=... DNACALC_CALC_PAGE=...
   cargo xwin build --release --locked --target x86_64-pc-windows-msvc --target-dir target/xwin
   -p dna-onecalc-desktop -p calc-xll-shim -p dna-calc-desktop`: 6 min 41 s.
   `dna-onecalc-desktop.exe` 39 MB, `dna-calc-desktop.exe` 68 MB, `xlcall32.dll` 197 KB. Imports
   (llvm-readobj) are system DLLs only, no `vcruntime` or `api-ms-win-crt`; the page file names are
   inside each exe.
3. Folders as `package.rs` lays them out (READMEs from its constants), then
   `deploy/windows/build-msi.sh <dir>`:

| File | Bytes | SHA-256 |
|---|---|---|
| `DNA-Calc-0.1.0-win-x64.msi` | 25,342,976 | `f79ce79100a20bae774447c93e6cbcea3bd187cd6c4f289df52c07f0a0122042` |
| `DNA-OneCalc-0.1.0-win-x64.msi` | 40,309,248 | `ebf1f247d8ec068dcc6c47f481e37d90914c7dfe2464828f94bcdb58996ec550` |

Checked on Linux with `msiinfo` and `msiextract`: ProductVersion 0.1.0, Comments `0.1.0+9ed5dcf5efe5`
(the executables' source is the same at 01e14c4d and 9ed5dcf5: only tracker and test data changed),
summary Word Count 10, the upgrade codes above, 13 and 11 registry rows, and every file extracted from
each MSI has the SHA-256 of its input (4 and 5 files). The cabinet is MSZIP-compressed.

Not measured: installing, upgrading and uninstalling on Windows. The steps are in
`~/codex-runs/op1/windows/RUN-ON-WINDOWS.txt` on dna-firehorse (with 0.1.1 MSIs for the upgrade
test), for a clean machine only (dc-fk66.3); they write HKCU keys, so not on the owner's desktop.

## 9. winget, later

After two previews with signed MSIs at stable URLs: a manifest in `microsoft/winget-pkgs` with
`InstallerType: msi`, `Scope: user`, the ProductCode, the MSI's URL and SHA-256, one per product
(`DnaCalc.DnaCalc`, `DnaCalc.DnaOneCalc`). Submitting the pull request is the owner's step. The
manifest is generated with the release notes.
