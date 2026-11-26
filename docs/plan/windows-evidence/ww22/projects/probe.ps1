param($OwnedExcel,[string]$Directory)
. "$PSScriptRoot/../helpers.ps1"
$excel.DisplayAlerts=$false
function Inventory($b) {
    $sheets=for($i=1;$i -le $b.Worksheets.Count;$i++) {
        $s=$b.Worksheets.Item($i)
        @{name=$s.Name;code=$s.CodeName}
    }
    $components=foreach($item in $b.VBProject.VBComponents) {
        @{name=$item.Name;type=$item.Type;lines=$item.CodeModule.CountOfLines}
    }
    $refs=foreach($item in $b.VBProject.References) { @{name=$item.Name;guid=$item.Guid;major=$item.Major;minor=$item.Minor} }
    @{name=$b.Name;format=$b.FileFormat;sheets=@($sheets);components=@($components);references=@($refs)}
}
foreach($case in 'delete','delete-add','move','move-empty','copy','move-ref','delete-ref') {
    $b=Book
    try {
        $s1=$b.Worksheets.Item(1)
        $s2=$b.Worksheets.Add($missing,$s1)
        $s3=$b.Worksheets.Add($missing,$s2)
        $s1.Name='First'; $s2.Name='Middle'; $s3.Name='Last'
        foreach($s in $s1,$s2,$s3) {
            if($case -eq 'move-empty' -and $s.Name -eq 'Middle') {continue}
            $b.VBProject.VBComponents.Item($s.CodeName).CodeModule.AddFromString(@"
Public Function SheetMarker() As String
SheetMarker = "$($s.Name)"
End Function
"@)
        }
        $m=$b.VBProject.VBComponents.Add(1)
        if($case -like '*ref') {
            $m.CodeModule.AddFromString(@"
Public Function ReferenceProbe() As String
ReferenceProbe = $($s2.CodeName).SheetMarker()
End Function
"@)
        } else { $m.CodeModule.AddFromString(@"
Public Function KeepMarker() As Long
KeepMarker = 42
End Function
"@) }
        Observe "Project.$case" {
            $b.SaveAs("$Directory/$case-before.xlsm",52)
            $initial=Inventory $b
            $code=$s2.CodeName
            if($case -like 'delete*') {
                $null=$s2.Delete()
                if($case -eq 'delete-add') {$ns=$b.Worksheets.Add(); $ns.Name='Added'}
            } elseif($case -eq 'copy') { $s2.Copy() } else { $s2.Move() }
            $after=Inventory $b
            $dest=$excel.ActiveWorkbook
            $newBook=$dest.Name -ne $b.Name
            $b.SaveAs("$Directory/$case-source.xlsm",52)
            $d=$null
            if($newBook) {
                try {
                    $active=Inventory $dest
                    if($case -ne 'move-empty') {
                        try {$marker=$excel.Run("'$($dest.Name)'!$code.SheetMarker");$markerError=$null} catch {$marker=$null;$markerError=$_.Exception.Message}
                    } else {$marker=$null;$markerError=$null}
                    $dest.SaveAs("$Directory/$case-destination.xlsm",52)
                    try { $dest.SaveAs("$Directory/$case-destination.xlsx",51); $xlsx=@{format=$dest.FileFormat;name=$dest.Name} } catch {$xlsx=@{error=$_.Exception.Message}}
                    $d=@{active=$active;marker=$marker;marker_error=$markerError;xlsx=$xlsx}
                } finally {$dest.Close($false)}
            }
            @{before=$initial;source=$after;destination=$d}
        }
    } finally {$b.Close($false)}
}
foreach($file in Get-ChildItem "$Directory/*.xls?") {
    Observe "Reopen.$($file.Name)" {
        $b=$excel.Workbooks.Open($file.FullName)
        try { Inventory $b } finally {$b.Close($false)}
    }
}
