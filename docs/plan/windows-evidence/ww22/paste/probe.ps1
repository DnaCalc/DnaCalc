param($OwnedExcel,[string]$Directory)
. "$PSScriptRoot/../helpers.ps1"
$excel.DisplayAlerts=$false
foreach($target in 'E1:F1','E1:H1','E1:G1','D1:D3','XFD1') {
    $b=Book
    try {
        $s=$b.Worksheets.Item(1); $s.Range('A1:B2').Value2=3; $s.Range('D1:H3').Value2=7
        $s.Columns.Item('A:A').ColumnWidth=20; $s.Columns.Item('B:B').ColumnWidth=5
        $s.Columns.Item('B:B').Hidden=$true
        $s.Columns.Item('D:H').ColumnWidth=12
        Observe "Widths.$target" {
            $s.Range('A1:B2').Copy()
            try { $result=$s.Range($target).PasteSpecial(8); $err=$null } catch { $result=$null; $err=$_.Exception.Message }
            $cols=foreach($l in 'D','E','F','G','H') {
                $col=$s.Columns.Item("$($l):$l")
                $hidden=$col.Hidden; $col.Hidden=$false
                @{column=$l;width=$col.ColumnWidth;hidden=$hidden;outline=$col.OutlineLevel}
            }
            @{result=$result;error=$err;columns=@($cols);cells=@(Numbers $s.Range('D1:H1'))}
        }
    } finally { $excel.CutCopyMode=0; $b.Close($false) }
}
foreach($skip in $false,$true) {
    foreach($op in 2..5) {
        $b=Book
        try {
            $s=$b.Worksheets.Item(1); $s.Range('A1').Value2=3; $s.Range('D2').Value2=7
            $s.Range('A4').Value2=0; $s.Range('D3').Value2=7
            $s.Range('A1:A4').Copy()
            Observe "BlankOperation.$op.$skip" { $ret=$s.Range('D1').PasteSpecial(-4163,$op,$skip,$false); @{returned=$ret;cells=@(Numbers $s.Range('D1:D4'))} }
        } finally { $excel.CutCopyMode=0; $b.Close($false) }
    }
}
foreach($paste in -4144,21) {
    foreach($sourceNote in $true,$false) {
        $b=Book
        try {
            $s=$b.Worksheets.Item(1); $s.Range('A1').Value2='source'; $d=$s.Range('D1'); $d.Value2='destination'
            if($sourceNote) { $null=$s.Range('A1').AddComment('source note') }
            $null=$d.AddComment('destination note')
            $s.Range('A1').Copy()
            Observe "Note.$paste.$sourceNote" {
                try { $ret=$d.PasteSpecial($paste); $err=$null } catch { $ret=$null; $err=@{message=$_.Exception.Message;hr=('0x{0:X8}' -f $_.Exception.HResult)} }
                @{returned=$ret;error=$err;note=if($d.Comment){$d.Comment.Text()}; value=$d.Value2}
            }
        } finally { $excel.CutCopyMode=0; $b.Close($false) }
    }
}
foreach($paste in 14,-4104) {
    foreach($sourceRule in $true,$false) {
        $b=Book
        try {
            $s=$b.Worksheets.Item(1); $s.Range('A1').Value2=4
            $d=$s.Range('E30')
            $dr=$s.Range('E29:E31').FormatConditions.Add(2,$missing,'=E29>9'); $dr.Interior.Color=255
            if($sourceRule) { $sr=$s.Range('A1:A2').FormatConditions.Add(2,$missing,'=A1>2'); $sr.Interior.Color=65280 }
            function Conditions($range) {
                foreach($rule in $range.FormatConditions) {
                    @{formula=$rule.Formula1;priority=$rule.Priority;applies=$rule.AppliesTo.Address();color=$rule.Interior.Color}
                }
            }
            Observe "Conditions.$paste.$sourceRule" {
                $before=@(Conditions $s.Cells)
                $s.Range('A1').Copy()
                $ret=$d.PasteSpecial($paste)
                @{before=$before;after=@(Conditions $s.Cells);value=$d.Value2;returned=$ret}
            }
        } finally { $excel.CutCopyMode=0; $b.Close($false) }
    }
}
foreach($assignment in 'True','1','2','0.0','Empty') {
    foreach($start in 'inactive','copy','cut') {
        $b=Book
        try {
            $s=$b.Worksheets.Item(1); $s.Range('A1').Value2=4
            switch($start) { 'copy' {$s.Range('A1').Copy()}; 'cut' {$s.Range('A1').Cut()} }
            Observe "CutCopyMode.$assignment.$start" {
                $before=$excel.CutCopyMode
                $value=switch($assignment) {'True' {$true}; '1' {1}; '2' {2}; '0.0' {[double]0}; 'Empty' {$null}}
                $excel.CutCopyMode=$value
                @{before=$before;after=$excel.CutCopyMode}
            }
        } finally { $excel.CutCopyMode=0; $b.Close($false) }
    }
}
