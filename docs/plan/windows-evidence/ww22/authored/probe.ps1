param($OwnedExcel,[string]$Directory)
. "$PSScriptRoot/../helpers.ps1"
$excel.DisplayAlerts=$false
function Snapshot($b) {
    $sheets=for($i=1;$i -le $b.Worksheets.Count;$i++) { $s=$b.Worksheets.Item($i); @{sheet=$s.Name;code=$s.CodeName} }
    $modules=foreach($m in $b.VBProject.VBComponents) {
        $n=$m.CodeModule.CountOfLines
        @{name=$m.Name;type=$m.Type;source=if($n){$m.CodeModule.Lines(1,$n)}}
    }
    @{sheets=@($sheets);modules=@($modules);format=$b.FileFormat;name=$b.Name;active=$excel.ActiveSheet.Name}
}
foreach($name in 'delete','delete-codename-reference','move','move-codename-reference','move-group','move-group-codename-reference') {
    $case="dc-2g5fn-$name"
    $b=$excel.Workbooks.Open("$Directory/inputs/$case.xlsm",0,$true)
    try {
        Observe "Authored.$case" {
            $before=Snapshot $b
            $v=$excel.Run("'$($b.Name)'!Module1.Probe")
            $other=$excel.ActiveWorkbook
            $different=$other.Name -ne $b.Name
            $source=Snapshot $b
            $destination=$null
            if($different) {
                try {
                    $destination=Snapshot $other
                    $mod=$other.VBProject.VBComponents.Add(1)
                    $codename=$other.Worksheets.Item('Transfer').CodeName
                    $mod.CodeModule.AddFromString(@"
Public Function ReadSheetMarker() As Variant
ReadSheetMarker = $codename.SheetMarker()
End Function
"@)
                    try { $destination.marker=$excel.Run("'$($other.Name)'!ReadSheetMarker") }
                    finally {$other.VBProject.VBComponents.Remove($mod)}
                    $other.SaveAs("$Directory/$case-destination.xlsm",52)
                } finally {$other.Close($false)}
            }
            $b.SaveAs("$Directory/$case-source.xlsm",52)
            @{returned=$v;type=$v.GetType().Name;before=$before;source=$source;destination=$destination}
        }
    } finally {$b.Close($false)}
}
foreach($name in 'application-match','redim-existing-as','variable-name') {
    $case="dc-711ye-$name"
    $b=$excel.Workbooks.Open("$Directory/inputs/$case.xlsm",0,$true)
    try {
        Observe "Authored.$case" {
            $answer=$excel.Run("'$($b.Name)'!Module1.Probe")
            @{answer=$answer;type=$answer.GetType().FullName}
        }
    } finally {$b.Close($false)}
}
