param($OwnedExcel,[string]$Directory)
. "$PSScriptRoot/../helpers.ps1"
$excel.DisplayAlerts=$false
$wb=$excel.Workbooks.Open("$Directory/e1.keep.xlsx")
try{
$ws=$wb.Worksheets.Item(1)
Observe "reopen" { @{hidden=@(1..6|%{$ws.Rows.Item($_).Hidden}); filterMode=$ws.FilterMode; afMode=$ws.AutoFilterMode; names=@($wb.Names|%{ $_.Name+'='+$_.RefersTo}) } }
Observe "showAll" { $ws.ShowAllData(); @{hidden=@(1..6|%{$ws.Rows.Item($_).Hidden}); filterMode=$ws.FilterMode} }
Observe "inplaceAgain" { $r=$ws.Range("A1:C6").AdvancedFilter(2,$ws.Range("E1:E2")); @{ret=$r;hidden=@(1..6|%{$ws.Rows.Item($_).Hidden});filterMode=$ws.FilterMode} }
Observe "unique-inplace" { $ws.ShowAllData(); $ws.Range("E2").Value2=''; $ws.Range("A1:C6").AdvancedFilter(1,[Type]::Missing,[Type]::Missing,$true) |Out-Null; @{hidden=@(1..6|%{$ws.Rows.Item($_).Hidden});filterMode=$ws.FilterMode} }
} finally { $wb.Close($false) }
