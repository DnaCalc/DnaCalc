# Building the Windows binaries on a Linux host (cargo-xwin)

Owner rule (2026-11-11): Rust does not compile on the Windows laptop. The Windows executables and
the Windows test binaries are cross-built on a Linux host (`dna-firehorse`) for
`x86_64-pc-windows-msvc` with `cargo xwin`, copied to Windows, hash-checked there and run there.
Nothing is compiled on Windows.

## Licence acceptance

cargo-xwin downloads the Microsoft C runtime and Windows SDK files (through the `xwin` crate).
The owner accepted those licence terms for this purpose on 2026-11-11. The accept flag is set
with `XWIN_ACCEPT_LICENSE=1`. The files are cached in `~/.cache/cargo-xwin` on the Linux host and
are not redistributed or committed.

## One-time install (Ubuntu host)

```sh
sudo apt-get install -y clang lld llvm        # clang, lld-link, llvm-rc, llvm-lib (LLVM 20)
rustup target add x86_64-pc-windows-msvc       # from inside the clone: rust-toolchain.toml pins 1.94.1
CARGO_TARGET_DIR=~/xwin-tmp cargo install --locked cargo-xwin   # run outside the clone
```

Notes:
- `cargo install` puts its build directory under `/tmp`; on dna-firehorse `/tmp` is a 32 GB
  tmpfs that other work can fill. Setting `CARGO_TARGET_DIR` to a disk path avoids
  "No space left on device".
- `llvm-rc` is on `PATH` through `/usr/bin`; `clang-cl` and `lld-link` live in `/usr/lib/llvm-20/bin`,
  so put that first on `PATH` for the build.

## Build commands

Run in `~/projects/DnaCalc/DnaCalc`. A dedicated target directory (`target/xwin`) keeps the Linux
`target/debug` untouched. The binaries land in `target/xwin/x86_64-pc-windows-msvc/debug/`.

```sh
export XWIN_ACCEPT_LICENSE=1 PATH=/usr/lib/llvm-20/bin:$PATH
T="--locked --target x86_64-pc-windows-msvc --target-dir target/xwin"
# executables, the xtask launcher and harness, the fixture XLL and the XLCALL32 companion
cargo xwin build $T -p dna-onecalc-desktop -p dna-calc-desktop -p xtask -p xtask-harness \
    -p calc-xll-fixture -p calc-xll-shim
# test binaries (not run on Linux): the add-in host crates and the two desktop apps
cargo xwin test $T --no-run -p calc-xll-host -p calc-xll-abi -p calc-com-host -p calc-addin-host \
    -p calc-xll-fixture -p calc-xll-shim -p dna-onecalc-desktop -p dna-calc-desktop
```

`calc-xll-reference` is not a Cargo package (measurement tools and an Excel-DNA add-in).
The page bundle for the OneCalc acceptance is built with the native xtask, it is not a Windows
binary: `cargo xtask app bundle onecalc --out ~/xwin-bundle`.

Executables produced: `dna-onecalc-desktop.exe`, `dna-calc-desktop.exe`, `xtask.exe`,
`xtask-harness.exe`, `calc_xll_fixture.dll` (the X1 XLL, used as `fixture.xll`) and `xlcall32.dll`.
Test executables are in `deps/` (`calc_xll_host-*.exe`, `calc_xll_abi-*.exe`, `calc_com_host-*.exe`,
`calc_addin_host-*.exe`, `calc_xll_fixture-*.exe`, `xlcall32-*.exe`, `dna_onecalc_desktop-*.exe`,
`dna_calc_desktop-*.exe`, and two `xll-*.exe`: the integration tests of `dna-calc-desktop` (14
tests) and `dna-onecalc-desktop` (21 tests); the one with 21 tests is the OneCalc one).

## Copy and verify

On the Linux host collect the files and hash them, then copy to a new UTC-stamped folder on Windows
and re-check the hashes there (Git Bash on Windows has `sha256sum`):

```sh
# Linux host
mkdir -p ~/xwin-stage/deps && cd target/xwin/x86_64-pc-windows-msvc/debug
cp dna-calc-desktop.exe dna-onecalc-desktop.exe xtask.exe xtask-harness.exe calc_xll_fixture.dll xlcall32.dll ~/xwin-stage/
cp deps/{calc_addin_host,calc_com_host,calc_xll_abi,calc_xll_fixture,calc_xll_host,xlcall32,dna_calc_desktop,dna_onecalc_desktop}-*.exe deps/xll-*.exe ~/xwin-stage/deps/
cd ~/xwin-stage && find . -type f \( -name '*.exe' -o -name '*.dll' -o -name '*.xll' \) | sort | xargs sha256sum > SHA256SUMS.txt

# Windows (Git Bash)
D=/c/Work/DnaCalc/xwin-builds/$(date -u +%Y%m%dT%H%M%SZ); mkdir -p $D
scp -r dna-firehorse:xwin-stage/. $D/ && (cd $D && sha256sum -c SHA256SUMS.txt)
```

## Running the cross-built tests on Windows

The integration tests (`xll-*.exe`) read `env!("CARGO_BIN_EXE_...")` and `env!("CARGO_MANIFEST_DIR")`
at compile time, so they look for files under the Linux paths
(`/home/ubuntu/projects/DnaCalc/DnaCalc/...`). On Windows a path starting with `/` means the root
of the current drive. Recreate that tree on a `subst` drive and run from it:

```
<dest>\root\home\ubuntu\projects\DnaCalc\DnaCalc\
    target\xwin\x86_64-pc-windows-msvc\debug\   (the .exe and .dll files and deps\)
    crates\calc-xll-reference\fixture\vba\, crates\calc-doc\tests\fixtures\vba_udf_names\   (fixtures)
    apps\dna-calc-desktop\, apps\dna-onecalc-desktop\tests\   (source paths the test panics name)
subst X: <dest>\root          # then run the test exe with X:\ as the current directory
```

`subst` changes no security setting. Remove it with `subst X: /D` when done.

## Running the OneCalc add-in page acceptance with the cross-built binaries

`xtask-harness.exe app xll-accept onecalc ...` runs as is (it starts the desktop exe, attaches
WebDriver to WebView2 and loads the add-ins). Two points:
- The harness calls `cargo metadata --no-deps` to find the workspace root. To avoid running cargo on
  Windows, set `CARGO` to a shim `.cmd` that prints a saved copy of that JSON with the Linux root
  path replaced by the Windows clone's path
  (`cargo metadata --no-deps --format-version 1 --locked | sed 's|/home/ubuntu/projects/DnaCalc/DnaCalc|C:/Work/DnaCalc/DnaCalc-win|g'`).
  The shim holds only `type "%~dp0metadata.json"`.
- Run it through `tools/excel-session/on-private-desktop.ps1` (own desktop name, `-SessionRoot` inside
  the destination folder), so no window reaches the owner's desktop. The `--var` values are those of
  the latest run (see `onecalc-m4.log` of `codex-runs/opus-addins`), with `--exe` and the bundle from
  the cross build. The Excel answers are the recorded `m4-excel-all` controller files; no Excel is
  started by this acceptance.

## Known differences from a Windows build

1. **Resource compiler.** `apps/dna-calc-desktop/version-resource.rs` used `rc.exe` from the Windows
   SDK through `ProgramFiles(x86)`. On a non-Windows build host it now runs `llvm-rc` (or the program
   in `LLVM_RC`); `version.rc` is self-contained, so the icon and version resource link as before.
2. **Export table alignment.** `lld-link` can place a DLL's export address table off a 4-byte
   boundary; MSVC `link` does not. `calc-xll-fixture`'s `DynamicSlot` test code dereferenced an
   export slot as `*mut u32`, which the debug build's alignment check aborts on. It now uses
   `read_unaligned` and `write_unaligned` (test fixture only, no product code).
3. **Compile-time paths.** Test executables embed Linux paths (see above).
4. **Debug builds only so far.** The commands above build the `dev` profile. A release or `perf`
   build is the same command with `--release` or `--profile perf`; its results are not recorded here.
5. **libffi.** `libffi-sys` compiles its C and GNU-syntax assembly (`ffiw64.c`, `win64_gnu.S`) with clang-cl
   in a cross build; a Windows build uses MSVC's flavour. See the open difference below.
6. The Windows SDK/CRT files come from the `xwin` download, not from a Visual Studio install, so
   the binaries link against the redistributable CRT import libraries of that snapshot.

## Results of the first trial (2026-11-11, Excel not started)

Binaries and hashes: `C:\Work\DnaCalc\xwin-builds\20261111T092342Z\SHA256SUMS.txt`; all 16 hashes
matched after the copy. Run on Windows from the cross-built files:
- Unit tests: calc-addin-host 2, calc-com-host 13, calc-xll-abi 5, calc-xll-fixture 0, calc-xll-host 12,
  xlcall32 1, dna-calc-desktop 6, dna-onecalc-desktop 19: all pass.
- Integration tests (`xll-*.exe`): dna-calc-desktop 14 of 14, dna-onecalc-desktop 21 of 21 pass (after the
  export-alignment fix above; before it, 0 of 21 and most of the 14 aborted).
- OneCalc XLL page acceptance (`m4-acceptance.json`, all ten sets, recorded Excel answers):
  352 of 353 matched; the known figure from the native build is 353 of 353. The framework host route
  (333/333) was not run (it needs Excel sessions and a different harness route).

**Open difference (not fixed).** Set `xloil-utils-sql`, case `=INDEX(xloSort({3;1;2}),1,1)`: the
cross-built `dna-onecalc-desktop.exe` returns 3 (sorted descending); the native build returns 1 (the
omitted direction argument is read as "D" instead of the default "A"). It is deterministic (3 of 3 runs),
and an explicit `"a"` or `"d"` argument gives the right answer in both builds. A native exe with the
cross-built `xlcall32.dll` is correct, a cross-built exe with the native `xlcall32.dll` is wrong, so
the cause is in the exe. The 289 own-fixture cases and every other set match. Prime suspect: the
libffi call path (item 5), the one native-code component built differently; not confirmed.
Until it is resolved, do not take a cross-built acceptance as equal to a native one.

## What still needs a Windows-side build

The open difference above: a native Windows build is the reference until it is explained.
Everything else in the add-in tests and the page acceptance ran from cross-built files. `cargo xtask check`,
`sweep` and anything that starts cargo on Windows still need the Windows toolchain, and the xtask
launcher (`xtask.exe`) builds the harness with cargo, so the harness is run directly as above.
