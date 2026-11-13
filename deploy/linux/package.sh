#!/usr/bin/env bash
# Builds the Linux packages of the `dnacalc` runner (dc-icq4.2): a release tarball and a .deb made
# from the same files. Nothing is published or signed here (the owner's step).
#
#   deploy/linux/package.sh [OUT_DIR]        default OUT_DIR: target/linux-packages
#
# Target x86_64-unknown-linux-musl: a static executable, no glibc needed on the install machine.
# Version: the Cargo workspace version (no scheme of its own; release streams are dc-ysl.11).
# Names follow the Windows zips (`DNA-Calc-<version>-win-x64.zip`, apps/PACKAGING.md):
#   dnacalc-<version>-linux-x64.tar.gz, dnacalc_<version>_amd64.deb
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
cd "$root"
out=${1:-target/linux-packages}
target=x86_64-unknown-linux-musl
version=$(cargo metadata --locked --no-deps --format-version 1 | jq -r '.packages[] | select(.name=="dnacalc") | .version')
commit=$(git rev-parse --short=12 HEAD)

cargo build --locked --release -p dnacalc --target "$target" --target-dir target/pkg
bin=target/pkg/$target/release/dnacalc

stage=$out/stage
rm -rf "$stage" && mkdir -p "$stage/dnacalc-$version"
dir=$stage/dnacalc-$version
install -m 0755 "$bin" "$dir/dnacalc"
install -m 0644 LICENSE "$dir/LICENSE"
sed "s/@VERSION@/$version/g" "$here/README.txt" > "$dir/README.txt"
chmod 0644 "$dir/README.txt"
# Notices: the ported-code notices the repository keeps, then the crate licences of what this
# executable links (names, versions, SPDX expressions from cargo metadata).
{
  cat deploy/notices/ported.txt
  printf '\n==============================================================================\n'
  printf 'Crates linked into dnacalc (name, version, licence expression, from cargo metadata)\n'
  printf '==============================================================================\n\n'
  cargo metadata --locked --format-version 1 --filter-platform "$target" |
    jq -r '[.packages[] | select(.source != null) | "\(.name) \(.version)\t\(.license // "see the crate")"] | unique[]'
  printf '\nThe full licence texts of these crates are not yet included (dc-ysl.2).\n'
} > "$dir/THIRD-PARTY-NOTICES.txt"
printf 'dnacalc %s\ncommit %s\ntarget %s\n' "$version" "$commit" "$target" > "$dir/BUILD"

tarball=dnacalc-$version-linux-x64.tar.gz
# Reproducible: sorted names, fixed owner and time (the commit time).
mtime=$(git show -s --format=%cI HEAD)
tar --sort=name --owner=0 --group=0 --numeric-owner --mtime="$mtime" -C "$stage" \
  -cf - "dnacalc-$version" | gzip -9n > "$out/$tarball"

# The .deb from the same files: /usr/bin/dnacalc, /usr/share/doc/dnacalc/{copyright,README,notices}.
deb=$stage/deb
mkdir -p "$deb/DEBIAN" "$deb/usr/bin" "$deb/usr/share/doc/dnacalc"
install -m 0755 "$dir/dnacalc" "$deb/usr/bin/dnacalc"
install -m 0644 "$dir/LICENSE" "$deb/usr/share/doc/dnacalc/copyright"
install -m 0644 "$dir/README.txt" "$deb/usr/share/doc/dnacalc/README"
install -m 0644 "$dir/THIRD-PARTY-NOTICES.txt" "$deb/usr/share/doc/dnacalc/THIRD-PARTY-NOTICES.txt"
size=$(du -sk "$deb/usr" | cut -f1)
cat > "$deb/DEBIAN/control" <<C
Package: dnacalc
Version: $version
Section: utils
Priority: optional
Architecture: amd64
Installed-Size: $size
Maintainer: DNA Kode, Inc. <noreply@dnakode.com>
Homepage: https://dnacalc.com
Description: DNA Calc headless workbook runner
 Opens an Excel workbook, recalculates it with the DNA Calc engine, compares the
 result with the values the file cached, reads and writes cells and runs the
 file's VBA macros. Static executable; no dependencies.
C
fakeroot dpkg-deb --root-owner-group -Zxz -b "$deb" "$out/dnacalc_${version}_amd64.deb" >/dev/null
rm -rf "$stage"
(cd "$out" && sha256sum "$tarball" "dnacalc_${version}_amd64.deb" | tee SHA256SUMS)
