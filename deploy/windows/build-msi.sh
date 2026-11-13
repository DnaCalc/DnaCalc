#!/usr/bin/env bash
# Builds the per-user MSIs of DNA OneCalc and DNA Calc on Linux (dc-fk66, route in
# docs/plan/WINDOWS_INSTALLERS.md) from the package folders `cargo xtask package onecalc` makes
# (apps/PACKAGING.md): DNA-OneCalc/ and DNA-Calc/. Nothing is signed here (the owner's step);
# sign the executables before this script and the MSIs after it.
#
#   deploy/windows/build-msi.sh PACKAGE_DIR [OUT_DIR]     default OUT_DIR: target/windows-msi
#
# Compatibility entry point only: all packaging and provenance checks live in xtask.
# PACKAGE_DIR must contain both folders and their package-manifest.json. Needs wixl
# (msitools 0.106 tested): WIXL=/path/to/wixl or wixl on PATH.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
pkg=$(cd "${1:?usage: build-msi.sh PACKAGE_DIR [OUT_DIR]}" && pwd)
out=${2:-$root/target/windows-msi}
cd "$root"
exec cargo xtask package installer --package "$pkg" --out "$out"
