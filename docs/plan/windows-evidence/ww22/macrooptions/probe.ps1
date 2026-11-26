param($OwnedExcel,[string]$Directory)
. "$PSScriptRoot/../helpers.ps1"
$excel.DisplayAlerts=$false
$books=@()
try {
    foreach($name in 'First','Second') {
        $b=Book;$books+=,$b
        $m=$b.VBProject.VBComponents.Add(1);$m.Name='LaneOptions'
        $m.CodeModule.AddFromString(@"
Public Function SampleFn(ByVal arg As Double) As Double
SampleFn = arg
End Function
"@)
        $b.SaveAs("$Directory/$name.xlsm",52)
    }
    $books[1].Activate()
    foreach($mode in 'unqualified','qualified','missing','description','category','empty-description','empty-arguments') {
        Observe "MacroOptions.$mode" {
            $name=switch($mode) {
                'unqualified' {'SampleFn'}
                'missing' {'NoSuchProcedure'}
                default {"'$($books[0].Name)'!SampleFn"}
            }
            $desc=$missing;$category=$missing;$arg=$missing
            switch($mode) {
                'unqualified' {$desc='unqualified';$category='Lane Category';$arg=[object[]]@('unqualified arg')}
                'qualified' {$desc='qualified';$category=14;$arg=[object[]]@('qualified arg')}
                'description' {$desc='updated description'}
                'category' {$category='Renamed Category'}
                'empty-description' {$desc=''}
                'empty-arguments' {$arg=[object[]]@('')}
                'missing' {$desc='missing'}
            }
            try {$ret=$excel.MacroOptions($name,$desc,$missing,$missing,$missing,$missing,$category,$missing,$missing,$missing,$arg);$err=$null}
            catch {$ret=$null;$err=@{message=$_.Exception.Message;hr=('0x{0:X8}' -f $_.Exception.HResult)}}
            $books[0].SaveCopyAs("$Directory/$mode-First.xlsm")
            $books[1].SaveCopyAs("$Directory/$mode-Second.xlsm")
            @{name=$name;active=$excel.ActiveWorkbook.Name;result=$ret;error=$err}
        }
    }
    $project=$books[0].VBProject
    $component=$project.VBComponents.Add(2);$component.Name='CaseClass'
    foreach($name in 'laneoptions','CaseClass','absent',1,0) {
        Observe "VBIDE.Component.$name" {
            $m=$project.VBComponents.Item($name)
            $n=$m.CodeModule.CountOfLines
            @{name=$m.Name;type=$m.Type;count=$n;declarations=$m.CodeModule.CountOfDeclarationLines;lines=if($n){$m.CodeModule.Lines(1,$n)}}
        }
    }
} finally {foreach($b in $books) {$b.Close($false)}}
