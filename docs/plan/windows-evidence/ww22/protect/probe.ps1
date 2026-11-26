param($OwnedExcel,[string]$Directory)
. "$PSScriptRoot/../helpers.ps1"
$excel.DisplayAlerts=$false
foreach($mode in 'default','allow','uiOnly') {
    foreach($case in 'table-unlocked','table-locked','unlist','delete-table','filter-off','show-all','insert','delete','create-filter','paste-width') {
        $b=Book
        try {
            $s=$b.Worksheets.Item(1); $s.Range('A1').Value2='Name'; $s.Range('B1').Value2='Qty'; $s.Range('A2:A4').Value2='item'
            $s.Range('B2').Value2=5; $s.Range('B3').Value2=15; $s.Range('B4').Value2=25
            $s.Range('D11').Locked=$false; $s.Range('D12').Value2=99
            $table=$null
            if($case -in 'unlist','delete-table') { $table=$s.ListObjects.Add(1,$s.Range('A1:B4'),$missing,1) }
            if($case -in 'filter-off','show-all') { $null=$s.Range('A1:B4').AutoFilter(2,'>10') }
            if($case -eq 'create-filter') { $s.Range('A1').Locked=$false }
            if($case -eq 'table-unlocked') { $s.Range('A1:B4').Locked=$false }
            $allow=$mode -eq 'allow'
            $s.Protect('', $true,$true,$true,($mode -eq 'uiOnly'),$allow,$allow,$allow,$allow,$allow,$allow,$allow,$allow,$allow,$allow,$allow)
            Observe "Protected.$mode.$case" {
                try {
                    $returned=switch($case) {
                        'table-unlocked' {$s.ListObjects.Add(1,$s.Range('A1:B4'),$missing,1).Name}
                        'table-locked' {$s.ListObjects.Add(1,$s.Range('A1:B4'),$missing,1).Name}
                        'unlist' {$table.Unlist()}
                        'delete-table' {$table.Delete()}
                        'filter-off' {$s.AutoFilterMode=$false}
                        'show-all' {$s.ShowAllData()}
                        'insert' {$s.Range('D11').Insert(-4121)}
                        'delete' {$s.Range('D11').Delete(-4162)}
                        'create-filter' {$s.Range('A1').AutoFilter()}
                        'paste-width' {$s.Range('B1').Copy(); $s.Range('D11').PasteSpecial(8)}
                    }
                    $err=$null
                } catch { $err=@{hresult=('0x{0:X8}' -f $_.Exception.HResult);message=$_.Exception.Message} }
                @{error=$err;returned=$returned;tables=$s.ListObjects.Count;autofilter=$s.AutoFilterMode;filter=$s.FilterMode;row2hidden=$s.Rows.Item(2).Hidden;A1=$s.Range('A1').Value2;D11=$s.Range('D11').Value2;D12=$s.Range('D12').Value2;D13=$s.Range('D13').Value2;Dwidth=$s.Range('D11').ColumnWidth}
            }
        } finally { $excel.CutCopyMode=0; $b.Close($false) }
    }
}
foreach($kind in 'omitted','empty-text','integer','double','text') {
    $b=Book
    try {
        $s=$b.Worksheets.Item(1)
        $password=switch($kind){'omitted'{$missing};'empty-text'{''};'integer'{[int]123};'double'{[double]12.5};'text'{'abc'}}
        $path="$Directory/$kind.xlsx"
        Observe "Protect.SaveReopen.$kind" {
            $s.Protect($password,$false,$true,$false,$true,$true)
            $before=@{drawing=$s.ProtectDrawingObjects;contents=$s.ProtectContents;scenarios=$s.ProtectScenarios;uiOnly=$s.ProtectionMode;formatCells=$s.Protection.AllowFormattingCells}
            $b.SaveAs($path,51); $b.Close($false)
            $reb=$excel.Workbooks.Open($path)
            try {
                $r=$reb.Worksheets.Item(1)
                $after=@{drawing=$r.ProtectDrawingObjects;contents=$r.ProtectContents;scenarios=$r.ProtectScenarios;uiOnly=$r.ProtectionMode;formatCells=$r.Protection.AllowFormattingCells}
                $right=switch($kind){'integer'{'123'};'double'{'12.5'};'text'{'abc'};default{''}}
                try {$r.Unprotect($right);$err=$null} catch {$err=$_.Exception.Message}
                @{before=$before;after=$after;matching_unprotect_error=$err;contents_after_unprotect=$r.ProtectContents}
            } finally {$reb.Close($false)}
        }
    } finally {try {$b.Close($false)} catch {}}
}
