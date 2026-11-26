param($OwnedExcel,[string]$Directory)
. "$PSScriptRoot/../helpers.ps1"
$excel.DisplayAlerts=$false
foreach($operation in 2..5) {
    $b=Book
    try {
        $s=$b.Worksheets.Item(1); $s.Range('A1').Value2='source';$s.Range('A2').Value2=2;$s.Range('A3').Value2=2;$s.Range('A4').Value2=0
        $s.Range('A1:A4').NumberFormat='0.000';$s.Range('A1:A4').Font.Bold=$true
        $s.Range('E1').Value2=7;$s.Range('E2').Formula='=2+3';$s.Range('E3').Value2='original';$s.Range('E4').Value2=9
        Observe "Paste.Operation.FormulaText.$operation" {
            $null=$s.Range('A1:A4').Copy()
            $ret=$s.Range('E1').PasteSpecial(-4104,$operation)
            $null=$s.Calculate(); @{returned=$ret;cells=@(Numbers $s.Range('E1:E4'));bold=$s.Range('E1:E4').Font.Bold}
        }
    } finally {$excel.CutCopyMode=$false;$b.Close($false)}
}
