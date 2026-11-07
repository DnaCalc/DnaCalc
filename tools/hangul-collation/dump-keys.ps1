# Dumps the Windows sort keys (LCMapStringEx, LCMAP_SORTKEY, NORM_IGNORECASE, en-ZA) that
# generate_rust.py turns into hangul_collation.rs. Run on Windows:
#   powershell -NoProfile -File dump-keys.ps1 -OutDir <directory>
param([string]$OutDir = ".")
. (Join-Path $PSScriptRoot "nls.ps1")
[void][IO.Directory]::CreateDirectory($OutDir)
function Dump($name, $codes, $suffix) {
    $lines = foreach ($c in $codes) {
        "{0:X4} {1}" -f $c, [NLS]::Key(([string][char]$c) + $suffix, 1)
    }
    $lines | Set-Content -Encoding ascii (Join-Path $OutDir $name)
}
$jamo = (0x1100..0x11FF) + (0xA960..0xA97F) + (0xD7B0..0xD7FF)
$compat = (0x3130..0x318F) + (0xFFA0..0xFFDC)
Dump "single.txt" ($jamo + $compat) ""
Dump "syllables.txt" (0xAC00..0xD7A3) ""
# An iteration mark after each jamo shows the primary 04xx it takes.
Dump "iteration.txt" $jamo ([string][char]0x30FC)
