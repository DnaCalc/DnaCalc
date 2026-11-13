#!/usr/bin/env bash
# Builds the per-user MSIs of DNA OneCalc and DNA Calc on Linux (dc-fk66, route in
# docs/plan/WINDOWS_INSTALLERS.md) from the package folders `cargo xtask package onecalc` makes
# (apps/PACKAGING.md): DNA-OneCalc/ and DNA-Calc/. Nothing is signed here (the owner's step);
# sign the executables before this script and the MSIs after it.
#
#   deploy/windows/build-msi.sh PACKAGE_DIR [OUT_DIR]     default OUT_DIR: target/windows-msi
#
# PACKAGE_DIR holds DNA-OneCalc/ and/or DNA-Calc/. The MSI version is the Cargo workspace version
# with any pre-release part removed (MSI versions are numeric); the commit goes into the summary
# Comments as <version>+<commit12>. VERSION=<x.y.z> overrides the version (for an upgrade test
# only; a release uses the workspace version). Needs wixl (msitools 0.106 tested): `WIXL=/path/to/wixl`
# or wixl on PATH.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
pkg=$(cd "${1:?usage: build-msi.sh PACKAGE_DIR [OUT_DIR]}" && pwd)
out=${2:-$root/target/windows-msi}
wixl=${WIXL:-wixl}
cd "$root"
version=${VERSION:-$(sed -n '/^\[workspace.package\]/,/^\[/s/^version *= *"\(.*\)"/\1/p' Cargo.toml)}
msi_version=${version%%-*}
commit=$(git rev-parse --short=12 HEAD)
mkdir -p "$out"

build() { # folder wxs icon msi-name
  local dir=$pkg/$1
  [ -d "$dir" ] || { echo "skip $1: no $dir"; return 0; }
  "$wixl" -a x64 -D Version="$msi_version" -D Commit="$commit" -D Src="$dir" -D Icon="$3" \
    -o "$out/$4" "$here/$2"
}
build DNA-OneCalc dna-onecalc.wxs apps/dna-onecalc-desktop/icon/onecalc.ico "DNA-OneCalc-$version-win-x64.msi"
build DNA-Calc dna-calc.wxs apps/dna-calc-desktop/icon/calc.ico "DNA-Calc-$version-win-x64.msi"
(cd "$out" && ls ./*.msi >/dev/null && sha256sum ./*.msi | tee SHA256SUMS)
