param($OwnedExcel,[string]$Directory)
. "$PSScriptRoot/../helpers.ps1"
$excel.DisplayAlerts=$false
foreach($file in "$Directory/orphan.xlsm","$Directory/orphan-resaved.xlsm") {
    Observe "Project.Open.$([IO.Path]::GetFileName($file))" {
        $b=$excel.Workbooks.Open($file,0,$true)
        try {
            $components=foreach($m in $b.VBProject.VBComponents) {
                $n=$m.CodeModule.CountOfLines
                @{name=$m.Name;type=$m.Type;lines=$n;source=if($n){$m.CodeModule.Lines(1,$n)}}
            }
            $sheets=for($i=1;$i -le $b.Sheets.Count;$i++) {
                $s=$b.Sheets.Item($i)
                @{name=$s.Name;code=$s.CodeName}
            }
            if($file -like '*/orphan.xlsm') {$b.SaveAs("$Directory/orphan-resaved.xlsm",52)}
            @{name=$b.Name;sheets=@($sheets);components=@($components);format=$b.FileFormat}
        } finally {$b.Close($false)}
    }
}
