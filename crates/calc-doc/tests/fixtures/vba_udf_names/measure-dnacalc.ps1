# Native probe only: the product libraries and the existing fixture helper supply the route.
param([string]$OutDir = '', [switch]$NoRecord)
$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '../../../../..')).Path
$scratch = Join-Path $repo 'target/udfnames/cargo-probe'
New-Item -ItemType Directory -Force $scratch | Out-Null
$root = $repo.Replace('\', '/')
$source = (Join-Path $PSScriptRoot 'probe.rs').Replace('\', '/')
@"
[package]
name = "udf-names-measurement"
version = "0.0.0"
edition = "2024"
[workspace]
[[bin]]
name = "udf-names-measurement"
path = "$source"
[dependencies]
calc-doc = { path = "$root/crates/calc-doc" }
calc-fml = { path = "$root/crates/calc-fml" }
calc-func = { path = "$root/crates/calc-func" }
miniz_oxide = "=0.8.9"
serde_json = "=1.0.154"
"@ | Set-Content -Encoding utf8 (Join-Path $scratch 'Cargo.toml')
$commit = git -C $repo rev-parse HEAD
$dirty = if (git -C $repo status --porcelain) { 'true' } else { 'false' }
$output = if ($OutDir) { [IO.Path]::GetFullPath($OutDir) } else { Join-Path $repo 'target/udfnames/product' }
cargo run --manifest-path (Join-Path $scratch 'Cargo.toml') --target-dir (Join-Path $repo 'target') -- $PSScriptRoot $output $commit $dirty
if ($LASTEXITCODE -ne 0) { throw 'Product measurement failed' }
if ($NoRecord) { exit 0 }
Copy-Item -LiteralPath (Join-Path $output 'product-observations.json') -Destination $PSScriptRoot -Force
foreach ($pair in @(
    @('collision-xlsm-reentered.xlsm', 'dna-collision-reentered.xlsm'),
    @('local-xlsb-saved.xlsm', 'dna-local-from-binary.xlsm'),
    @('metadata-custom-vba-xlsb-saved.xlsm', 'dna-metadata-from-binary.xlsm')
)) {
    Copy-Item -LiteralPath (Join-Path $output $pair[0]) -Destination (Join-Path $PSScriptRoot $pair[1]) -Force
}
