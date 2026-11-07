# Writes random pairs of texts and the order CompareStringEx gives them (the format of
# crates/calc-func/src/catalogue/comparison/hangul_pairs.tsv): code units in hexadecimal, a tab,
# the other text, a tab, -1, 0 or 1. The pool holds conjoining jamo, syllables, compatibility jamo
# and one group of context units. Run on Windows:
#   powershell -NoProfile -File random-pairs.ps1 -Count 6000 -Seed 11 -Out pairs.tsv
param([int]$Count = 6000, [int]$Seed = 11, [string]$Out = "pairs.tsv",
      [int[]]$Context = @(0x200B, 0x00AD, 0x034F, 0x0301))
. (Join-Path $PSScriptRoot "nls.ps1")
$r = New-Object System.Random($Seed)
$jamo = (0x1100..0x11FF) + (0xA960..0xA97C) + (0xD7B0..0xD7C6) + (0xD7CB..0xD7FB)
$compat = (0x3131..0x318E) + (0xFFA1..0xFFBE)
$syllables = @(0xAC00, 0xAC01, 0xAC1C, 0xAE4B, 0xAE4C, 0xB2E3, 0xB2E4, 0xB2E5, 0xB098, 0xB099, 0xD7A3, 0xC544)
$core = @(0x1100, 0x1101, 0x1103, 0x1112, 0x1161, 0x1162, 0x1175, 0x11A8, 0x11A9, 0x11C2, 0x1160, 0x115F, 0x1113, 0x1176, 0x11C3, 0xD7DD)
function Pick() {
    $x = $r.Next(100)
    if ($x -lt 30) { return [char]$jamo[$r.Next($jamo.Count)] }
    elseif ($x -lt 55) { return [char]$core[$r.Next($core.Count)] }
    elseif ($x -lt 68) { return [char](0xAC00 + $r.Next(11172)) }
    elseif ($x -lt 78) { return [char]$syllables[$r.Next($syllables.Count)] }
    elseif ($x -lt 88) { return [char]$compat[$r.Next($compat.Count)] }
    else { return [char]$Context[$r.Next($Context.Count)] }
}
function Hex($s) { (($s.ToCharArray() | ForEach-Object { "{0:X4}" -f [int]$_ }) -join " ") }
$lines = for ($i = 0; $i -lt $Count; $i++) {
    $a = -join (1..($r.Next(1, 5)) | ForEach-Object { Pick })
    $b = -join (1..($r.Next(1, 5)) | ForEach-Object { Pick })
    "{0}`t{1}`t{2}" -f (Hex $a), (Hex $b), [NLS]::Cmp($a, $b, 1)
}
$lines | Set-Content -Encoding ascii $Out
