param($OwnedExcel,[string]$Directory)
. "$PSScriptRoot/../helpers.ps1"
$excel.DisplayAlerts=$false
foreach($method in 'Copy','Move') {
    $src=Book; $dest=Book
    try {
        $s=$src.Worksheets.Item(1); $s.Name='Data'
        $remain=$src.Worksheets.Add(); $remain.Name='Keep'; $remain.Range('A1').Value2=17
        $s.Range('A1').Formula='=Keep!A1'
        $target=$dest.Worksheets.Item(1); $target.Name='Target'
        Observe "$method.CrossWorkbook.Before" {
            if($method -eq 'Copy') { $s.Copy($target) } else { $s.Move($target) }
            @{source=@(Tabs $src); destination=@(Tabs $dest); active_book=$excel.ActiveWorkbook.Name; destination_book=$dest.Name; active_sheet=$excel.ActiveSheet.Name; cell=@(Numbers $dest.Worksheets.Item('Data').Range('A1')); links=@($dest.LinkSources(1))}
        }
    } finally { $src.Close($false); $dest.Close($false) }
}
foreach($case in 'before-inside','after-inside','reversed','new-copy-external','last-visible-move') {
    $b=Book
    try {
        $d=$b.Worksheets.Item(1); $d.Name='D'
        $c=$b.Worksheets.Add($d); $c.Name='C'
        $bs=$b.Worksheets.Add($c); $bs.Name='B'
        $a=$b.Worksheets.Add($bs); $a.Name='A'
        Observe "Group.$case" {
            $before=$b.Name
            switch($case) {
                'before-inside' { (Group $b @('B','C')).Move($bs) }
                'after-inside' { (Group $b @('B','C')).Move($missing,$c) }
                'reversed' { (Group $b @('C','B')).Move($a) }
                'new-copy-external' { $bs.Range('A1').Value2=17; $a.Range('A1').Formula='=B!A1'; $a.Copy() }
                'last-visible-move' { $a.Activate(); $bs.Visible=0; $c.Visible=0; $d.Visible=0; $a.Move() }
            }
            $active=$excel.ActiveWorkbook
            $row=@{source=@(Tabs $b);active=@(Tabs $active);active_sheet=$excel.ActiveSheet.Name;active_book=$active.Name;source_book=$before;books=$excel.Workbooks.Count}
            if($case -eq 'new-copy-external') { $row.cell=@(Numbers $active.Worksheets.Item(1).Range('A1')); $row.links=@($active.LinkSources(1)) }
            if($active.Name -ne $before) { $active.Close($false) }
            $row
        }
    } finally { $b.Close($false) }
}
$b=Book
try {
    $s=$b.Worksheets.Item(1); $s.Name='Sheet1'
    $t=$b.Worksheets.Add($missing,$s); $t.Name='Sheet2'
    Observe 'Worksheet.Names.Add.ForeignQualifier' { $n=$s.Names.Add('Sheet2!X','=1'); @{name=$n.Name;parent=$n.Parent.Name;sheet1=$s.Names.Count;sheet2=$t.Names.Count} }
    foreach($target in 'B2:D5','A1:B1') {
        Observe "Range.Activate.$target" { $s.Activate(); $s.Range('A1:B2').Select(); $s.Range($target).Activate(); @{selection=$excel.Selection.Address();active=$excel.ActiveCell.Address()} }
    }
    Observe 'Workbook.VBProject.DefaultProfile' { $project=$b.VBProject; @{name=$project.Name;count=$project.VBComponents.Count} }
} finally { $b.Close($false) }
