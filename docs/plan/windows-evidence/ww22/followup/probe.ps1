param($OwnedExcel,[string]$Directory)
. "$PSScriptRoot/../helpers.ps1"
$excel.DisplayAlerts=$false
foreach($case in 'blank','cutcopy','filter','formats') {
    $b=Book
    try {
        $s=$b.Worksheets.Item(1)
        $s.Range('A1').Value2=3; $s.Range('A4').Value2=0
        if($case -eq 'blank') {
            foreach($skip in $false,$true) {
                foreach($op in 2..5) {
                    Observe "BlankOnBlank.$op.$skip" {
                        $null=$s.Range('D1:D5').Clear()
                        $s.Range('D3').Value2=7
                        $null=$s.Range('A1:A4').Copy()
                        $ret=$s.Range('D1').PasteSpecial(-4163,$op,$skip,$false)
                        @{returned=$ret;cells=@(Numbers $s.Range('D1:D4'))}
                    }
                }
            }
        }
        if($case -eq 'cutcopy') {
            $mod=$b.VBProject.VBComponents.Add(1)
            $mod.CodeModule.AddFromString(@"
Public Function CutProbe(ByVal assignment As String, ByVal initial As String) As String
Dim before As Long
On Error GoTo Failed
Application.CutCopyMode = False
Select Case initial
Case "copy": ActiveSheet.Range("A1").Copy
Case "cut": ActiveSheet.Range("A1").Cut
End Select
before = Application.CutCopyMode
Select Case assignment
Case "True": Application.CutCopyMode = True
Case "1": Application.CutCopyMode = 1
Case "2": Application.CutCopyMode = 2
Case "0.0": Application.CutCopyMode = CDbl(0)
Case "Empty": Application.CutCopyMode = Empty
End Select
CutProbe = CStr(before) & "|" & CStr(CLng(Application.CutCopyMode))
Exit Function
Failed:
CutProbe = CStr(Err.Number) & "|" & Err.Description
End Function
"@)
            foreach($as in 'True','1','2','0.0','Empty') {
                foreach($start in 'inactive','copy','cut') {
                    Observe "VBA.CutCopyMode.$as.$start" { $excel.Run("'$($b.Name)'!CutProbe",$as,$start) }
                }
            }
        }
        if($case -eq 'filter') {
            $s.Range('A1').Value2='Item';$s.Range('B1').Value2='Qty';$s.Range('A2:A4').Value2='a'
            $s.Range('B2').Value2=5;$s.Range('B3').Value2=15;$s.Range('B4').Value2=25
            foreach($criterion in 'known','unknown','computed') {
                Observe "AdvancedFilter.Stale.$criterion" {
                    $s.Range('D1:D2').Clear()
                    $s.Range('G1:G6').Value2='old';$s.Range('G8').Value2='isolated';$s.Range('G1:G8').NumberFormat='0.000'
                    $s.Range('H2').Value2='near'
                    switch($criterion) {
                        'known' {$s.Range('D1').Value2='Qty'; $s.Range('D2').Value2='>20'}
                        'unknown' {$s.Range('D1').Value2='Other'; $s.Range('D2').Value2='>20'}
                        'computed' {$s.Range('D1').Value2='Other'; $s.Range('D2').Formula='=B2>20'}
                    }
                    $ret=$s.Range('A1:B4').AdvancedFilter(2,$s.Range('D1:D2'),$s.Range('G1'))
                    @{returned=$ret;cells=@(Numbers $s.Range('G1:H8'))}
                }
            }
        }
        if($case -eq 'formats') {
            $s.Range('A1').Formula='=1+2';$s.Range('A1').NumberFormat='0.00'
            $s.Range('A1').HorizontalAlignment=-4108;$s.Range('A1').Font.Bold=$true;$s.Range('A1').Interior.Color=255
            $s.Range('A1').Locked=$false;$s.Range('A1').Borders.Item(7).LineStyle=1
            foreach($paste in 7,11,12) {
                Observe "Paste.Format.$paste" {
                    $d=$s.Range("E$paste"); $d.HorizontalAlignment=-4152;$d.Borders.Item(7).LineStyle=-4115
                    $null=$s.Range('A1').Copy()
                    $ret=$d.PasteSpecial($paste)
                    @{ret=$ret;cell=@(Numbers $d);alignment=$d.HorizontalAlignment;bold=$d.Font.Bold;fill=$d.Interior.Color;locked=$d.Locked;border=$d.Borders.Item(7).LineStyle}
                }
            }
        }
    } finally {$excel.CutCopyMode=$false; $b.Close($false)}
}
