param([string]$OutDirectory, [int]$Trials = 10, [int]$FullTrials = 5, [string]$Only = '', $OwnedExcel = $null)
# Evaluator-suspension probe (eval-1): what Excel does when a formula reads a cell that is still
# uncalculated in the same calculation. Each trial is a new workbook with the Probe module (Tk logs
# each call), Manual calculation, formulas entered with Formula2 in the order listed (the reader
# first, so the calc chain starts reversed). Stages after entry:
#   s1: C1 := 6, then Application.Calculate (op calculate) or CalculateFull (op full)
#   s2: C1 := 7, Application.Calculate (the chain as s1 left it)
#   s3: no change, Application.Calculate (clean recalculation: volatile cells only)
# Run without -OwnedExcel, it submits itself to the owned session queue in DNACALC_EXCEL_SESSION.
$ErrorActionPreference = 'Stop'
if ($null -eq $OwnedExcel) {
    $parameters = @{ OutDirectory = $OutDirectory; Trials = $Trials; FullTrials = $FullTrials; Only = $Only }
    & (Join-Path $PSScriptRoot '../../../../../../tools/excel-session/submit.ps1') -Script $PSCommandPath -Parameters ($parameters | ConvertTo-Json -Compress) -Environment 'eval1:suspension' -TimeoutSeconds 1800 -LeakEvery 1000000
    return
}
$excel = $OwnedExcel
New-Item -ItemType Directory -Force -Path $OutDirectory | Out-Null
$module = ([IO.File]::ReadAllText((Join-Path $PSScriptRoot 'Probe.bas')) -replace "`r?`n", "`r`n") -replace '(?m)^Attribute VB_Name.*\r\n', ''
$cases = [ordered]@{
    chain     = @(@('B1', '=C1*2'), @('B2', '=B1+1'), @('B3', '=B2+1'), @('A1', '=Tk("pre",1)+INDIRECT("B3")+Tk("post",2)'))
    chain_static = @(@('B1', '=C1*2'), @('B2', '=B1+1'), @('B3', '=B2+1'), @('A1', '=Tk("pre",1)+B3+Tk("post",2)'))
    chain_off = @(@('B1', '=C1*2'), @('B2', '=B1+1'), @('B3', '=B2+1'), @('A1', '=Tk("pre",1)+OFFSET(B1,2,0)+Tk("post",2)'))
    chain_rand = @(@('B1', '=C1*2'), @('B2', '=B1+1'), @('B3', '=B2+1'), @('A1', '=Tk("pre",RAND())+INDIRECT("B3")+Tk("post",RAND())'))
    chain_randonly = @(@('B1', '=C1*2'), @('B2', '=B1+1'), @('B3', '=B2+1'), @('A1', '=RAND()+INDIRECT("B3")*0'), @('A2', '=Tk("a1",A1)'))
    chain_now = @(@('B1', '=C1*2'), @('B2', '=B1+1'), @('B3', '=B2+1'), @('A1', '=Tk("pre",NOW())+INDIRECT("B3")*0+Tk("post",NOW())'))
    chain_nest = @(@('B1', '=C1*2'), @('B2', '=B1+1'), @('B3', '=B2+1'), @('A1', '=Tk("outer",Tk("pre",1)+INDIRECT("B3"))+Tk("post",2)'))
    chain_sum = @(@('B1', '=C1*2'), @('B2', '=B1+1'), @('B3', '=B2+1'), @('A1', '=SUM(Tk("pre",1),INDIRECT("B3"),Tk("post",2))'))
    chain_if  = @(@('B1', '=C1*2'), @('B2', '=B1+1'), @('B3', '=B2+1'), @('A1', '=IF(Tk("cond",1)>0,INDIRECT("B3"),0)+Tk("post",2)'))
    chain_let = @(@('B1', '=C1*2'), @('B2', '=B1+1'), @('B3', '=B2+1'), @('A1', '=LET(x,Tk("pre",1),x+INDIRECT("B3")+Tk("post",2))'))
    chain_expr = @(@('B1', '=C1*2'), @('B2', '=B1+1'), @('B3', '=B2+1'), @('A1', '=Tk("pre",1)+Tk("arg",INDIRECT("B3")+0)'))
    chain_two = @(@('B1', '=C1*2'), @('B2', '=B1+1'), @('B3', '=B2+1'), @('E1', '=C1*3'), @('E2', '=E1+1'), @('E3', '=E2+1'), @('A1', '=Tk("pre",1)+INDIRECT("B3")+Tk("mid",2)+INDIRECT("E3")+Tk("post",3)'))
    chain_deep = @(@('B1', '=C1*2'), @('B2', '=B1+1'), @('B3', '=B2+1'), @('B4', '=B3+1'), @('B5', '=B4+1'), @('B6', '=B5+1'), @('B7', '=B6+1'), @('B8', '=B7+1'), @('A1', '=Tk("pre",1)+INDIRECT("B8")+Tk("post",2)'))
    chain_after = @(@('A1', '=Tk("pre",1)+INDIRECT("B3")+Tk("post",2)'), @('B1', '=C1*2'), @('B2', '=B1+1'), @('B3', '=B2+1'))
}
$result = [ordered]@{
    excel_version = [string]$excel.Version; excel_build = [string]$excel.Build
    calculation_version = $excel.CalculationVersion
    mtc_enabled = $excel.MultiThreadedCalculation.Enabled; mtc_threads = $excel.MultiThreadedCalculation.ThreadCount
    automation_security = $excel.AutomationSecurity; started_utc = [DateTime]::UtcNow.ToString('O'); rows = @(); errors = @()
}
function Snapshot($sheet, $book, [string[]]$cells) {
    $values = [ordered]@{}
    foreach ($c in $cells) { $v = $sheet.Range($c).Value2; $values[$c] = if ($null -eq $v) { $null } else { '{0:R}' -f $v } }
    [ordered]@{ seq = [string]$excel.Run("'" + $book.Name + "'!GetSeq"); values = $values }
}
foreach ($op in 'calculate', 'full') {
    $count = if ($op -eq 'full') { $FullTrials } else { $Trials }
    foreach ($name in @($cases.Keys)) {
        if ($Only -and $name -ne $Only) { continue }
        for ($trial = 1; $trial -le $count; $trial++) {
            $row = [ordered]@{ case = $name; op = $op; trial = $trial }
            $book = $null
            try {
                $step='add'; $book = $excel.Workbooks.Add()
                $step='calculation'; $excel.Calculation = -4135
                $step='vbproject'
                $component = $book.VBProject.VBComponents.Add(1)
                $component.Name = 'Probe'
                $component.CodeModule.AddFromString($module)
                $step='enter'; $sheet = $book.Worksheets.Item(1)
                $sheet.Range('C1').Value2 = 5
                $cells = @('C1')
                foreach ($entry in $cases[$name]) { $sheet.Range($entry[0]).Formula2 = $entry[1]; $cells += $entry[0] }
                $row.formulas = @($cases[$name] | ForEach-Object { $_[0] + ' ' + $_[1] })
                $step='run'; $row.entry = Snapshot $sheet $book $cells
                [void]$excel.Run("'" + $book.Name + "'!ResetSeq")
                $sheet.Range('C1').Value2 = 6
                if ($name -eq 'switch') { $sheet.Range('D1').Value2 = 'B2' }
                if ($op -eq 'full') { $excel.CalculateFull() } else { $excel.Calculate() }
                $row.s1 = Snapshot $sheet $book $cells
                [void]$excel.Run("'" + $book.Name + "'!ResetSeq")
                $sheet.Range('C1').Value2 = 7
                $excel.Calculate()
                $row.s2 = Snapshot $sheet $book $cells
                [void]$excel.Run("'" + $book.Name + "'!ResetSeq")
                $excel.Calculate()
                $row.s3 = Snapshot $sheet $book $cells
            } catch { $row.error = $_.Exception.Message; $row.step = $step }
            finally { if ($book) { try { $book.Close($false) } catch { $row.close_error = $_.Exception.Message } } }
            $result.rows += $row
        }
    }
}
$result.finished_utc = [DateTime]::UtcNow.ToString('O')
$result | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $OutDirectory 'suspension.json') -Encoding utf8NoBOM
