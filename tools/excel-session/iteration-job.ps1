param($OwnedExcel, [string]$Request, [string]$Result)
# P14 questions use the existing package/cache helper and typed capture functions.
$ErrorActionPreference='Stop'
$req=Get-Content -LiteralPath $Request -Raw -Encoding utf8|ConvertFrom-Json
$jobResult=$Result;$jobExcel=$OwnedExcel
$repository=(Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
. (Join-Path $repository 'platform/calc-oracle-excel/worker/capture-worker.ps1')
$Result=$jobResult;$OwnedExcel=$jobExcel
$generator=Join-Path $repository 'crates/calc-engine/tests/fixtures/make-dc-y2o-oracles.ps1'
$tokens=$null;$errors=$null
$ast=[Management.Automation.Language.Parser]::ParseFile($generator,[ref]$tokens,[ref]$errors)
foreach($name in 'Get-SheetParts','Set-StoredValues'){
    $definition=$ast.Find({param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq $name},$true)
    . ([scriptblock]::Create($definition.Extent.Text))
}
$outputDirectory=Split-Path -Parent $Result
[void][IO.Directory]::CreateDirectory($outputDirectory)
$observations=[Collections.Generic.List[object]]::new()
$excel=$OwnedExcel
function Write-Json($Path,$Value){[IO.File]::WriteAllText($Path,((ConvertTo-Json -InputObject $Value -Depth 40)+"`n"),[Text.UTF8Encoding]::new($false))}
function Settings {
    $s=[ordered]@{}
    foreach($key in 'Calculation','Iteration','MaxIterations','MaxChange','EnableEvents','CalculationState','AutomationSecurity'){$v=Get-ComProperty $excel $key;if($key -eq 'MaxChange'){$v=[BitConverter]::DoubleToInt64Bits([double]$v).ToString('x16')};$s[$key]=$v}
    $s['mtc_enabled']=Get-ComProperty $excel.MultiThreadedCalculation 'Enabled'
    $s['mtc_count']=Get-ComProperty $excel.MultiThreadedCalculation 'ThreadCount'
    $s['mtc_mode']=Get-ComProperty $excel.MultiThreadedCalculation 'ThreadMode'
    return $s
}
function Snap($Workbook){
    $cells=@()
    foreach($entry in $script:package.formulas){
        $cell=Read-Cell $Workbook $entry
        $cells+=@{sheet=$entry.sheet;row=$entry.row;column=$entry.column;formula=(Get-ComProperty $cell 'Formula2');value=(Read-Value $cell)}
    }
    return ,$cells
}
function Stage([string]$Label,$Workbook=$null){
    $bookSettings=[ordered]@{}
    if($Workbook){foreach($key in 'Date1904','PrecisionAsDisplayed','ForceFullCalculation'){ $bookSettings[$key]=Get-ComProperty $Workbook $key }}
    $item=[ordered]@{stage=$Label;utc=[DateTime]::UtcNow.ToString('O');settings=(Settings);workbook_settings=$bookSettings;active_sheet=$(if($Workbook){[string]$Workbook.ActiveSheet.Name}else{$null});cells=$(if($Workbook){Snap $Workbook}else{@()})}
    $observations.Add($item)
    [IO.File]::AppendAllText((Join-Path $outputDirectory 'stages.jsonl'),(($item|ConvertTo-Json -Depth 30 -Compress)+"`n"))
}
function Cache-Value($Cache){
    switch($Cache.t){
        'str' {return @{text=(Get-Units $Cache.v)}}
        'b' {return @{logical=($Cache.v -eq '1')}}
        'e' {$code=@{'#NULL!'=2000;'#DIV/0!'=2007;'#VALUE!'=2015;'#REF!'=2023;'#NAME?'=2029;'#NUM!'=2036;'#N/A'=2042}[$Cache.v];return @{error_code=$code}}
        default {if([string]::IsNullOrEmpty($Cache.v)){return @{empty=$true}};return @{number=([BitConverter]::DoubleToInt64Bits([double]::Parse($Cache.v,[Globalization.CultureInfo]::InvariantCulture)).ToString('x16'))}}
    }
}
function Same-Raw($Left,$Right){(ConvertTo-Json -InputObject $Left -Compress)-ceq(ConvertTo-Json -InputObject $Right -Compress)}
$book=$null;$bootstrap=$null;$fatal=$null;$prepared=@()
try {
    $bootstrap=$excel.Workbooks.Add()
    $excel.Calculation=-4135
    $excel.EnableEvents=$false
    $excel.MultiThreadedCalculation.Enabled=$false
    $excel.CalculateBeforeSave=$false
    if($req.action -eq 'prepare'){
        foreach($condition in $req.conditions){
            $phase='settings'
            try{
            $excel.Iteration=$true;$excel.MaxIterations=[int]$condition.iterations
            $excel.MaxChange=[BitConverter]::Int64BitsToDouble([Convert]::ToInt64($condition.change_bits,16))
            $phase='construct-save-patch'
            $book=$excel.Workbooks.Add()
            $sheet=$book.Worksheets.Item(1);$sheet.Name='S'
            $formulas=@($condition.formulas.PSObject.Properties)
            if($condition.reverse_entry){[array]::Reverse($formulas)}
            foreach($entry in $formulas){$sheet.Range($entry.Name).Formula2=$entry.Value}
            if($condition.PSObject.Properties['values']){foreach($entry in $condition.values.PSObject.Properties){$sheet.Range($entry.Name).Value2=[double]$entry.Value}}
            $path=Join-Path $req.inputs_directory ($condition.id+'.xlsx')
            $book.SaveAs($path,51);$book.Close($false);$book=$null
            $stored=@{};foreach($entry in $condition.seeds.PSObject.Properties){$stored['S!'+$entry.Name]=$entry.Value}
            $zip=[IO.Compression.ZipFile]::OpenRead($path);$hasChain=$null -ne $zip.GetEntry('xl/calcChain.xml');$zip.Dispose()
            Set-StoredValues $path $stored ([bool]$condition.reverse_chain -and $hasChain)
            $prepared+=@{id=$condition.id;path=$path;sha256=(Get-FileHash -LiteralPath $path).Hash.ToLowerInvariant();reverse_chain_requested=[bool]$condition.reverse_chain;chain_present=$hasChain;success=$true}
            }catch{
                $prepared+=@{id=$condition.id;success=$false;phase=$phase;error=$_.Exception.ToString();requested_change_bits=$condition.change_bits;settings_readback=(Settings)}
                if($book){$book.Close($false);$book=$null}
            }
        }
        Write-Json $Result @{fatal=$null;prepared=$prepared;generator_sha256=(Get-FileHash $generator).Hash}
        return
    }
    $condition=$req.condition
    $excel.Iteration=[bool]$condition.iteration
    $excel.MaxIterations=[int]$condition.iterations
    $wantedChange=[BitConverter]::Int64BitsToDouble([Convert]::ToInt64($condition.change_bits,16))
    try{$excel.MaxChange=$wantedChange}catch{
        $observations.Add(@{stage='MaxChange setter rejected';requested_bits=$condition.change_bits;error=$_.Exception.ToString();readback=Settings})
        throw 'Requested MaxChange rejected; this condition was not calculated.'
    }
    if([BitConverter]::DoubleToInt64Bits([double](Get-ComProperty $excel 'MaxChange')).ToString('x16') -cne $condition.change_bits){throw 'MaxChange setter readback differs.'}
    if((Get-FileHash -LiteralPath $req.input).Hash.ToLowerInvariant() -cne $req.input_sha256){throw 'Input differs from its frozen preparation/source hash; condition aborted.'}
    $script:package=Read-Package $req.input
    if($condition.PSObject.Properties['seeds']){
        if(@($script:package.formulas).Count -ne @($condition.formulas.PSObject.Properties).Count){throw 'Formula inventory differs from the requested condition.'}
        $zip=[IO.Compression.ZipFile]::OpenRead($req.input)
        try{
            $parts=Get-SheetParts $zip;$document=Read-Part $zip $parts['S']
            $names=[Xml.XmlNamespaceManager]::new($document.NameTable);$names.AddNamespace('x',$MainNs)
            foreach($declared in $condition.formulas.PSObject.Properties){
                $formula=$document.SelectSingleNode(("/x:worksheet/x:sheetData/x:row/x:c[@r='"+$declared.Name+"']/x:f"),$names)
                if($null -eq $formula -or $formula.InnerText.TrimStart('=') -cne ([string]$declared.Value).TrimStart('=')){throw "Stored formula differs from requested $($declared.Name)"}
            }
        }finally{$zip.Dispose()}
        foreach($declared in $condition.seeds.PSObject.Properties){
            $at=Split-Reference $declared.Name;$key=Get-Key 'S' $at.row $at.column
            $cache=$script:package.caches[$key]
            if($null -eq $cache){throw "Requested seed absent for $key"}
            $seed=$declared.Value
            $expected=if($seed -is [array]){Cache-Value @{t=$seed[0];v=[string]$seed[1]}}else{@{number=([BitConverter]::DoubleToInt64Bits([double]$seed).ToString('x16'))}}
            if(-not(Same-Raw (Cache-Value $cache) $expected)){throw "Stored seed differs from declared seed for $key"}
        }
        if($condition.PSObject.Properties['values']){foreach($declared in $condition.values.PSObject.Properties){
            $at=Split-Reference $declared.Name;$key=Get-Key 'S' $at.row $at.column
            $expected=@{number=([BitConverter]::DoubleToInt64Bits([double]$declared.Value).ToString('x16'))}
            if(-not(Same-Raw (Cache-Value $script:package.caches[$key]) $expected)){throw "Stored input differs from declared value for $key"}
        }}
    }
    Stage 'before-open'
    $input=Join-Path $outputDirectory 'input.xlsx'
    Copy-Item -LiteralPath $req.input -Destination $input
    if((Get-FileHash -LiteralPath $input).Hash.ToLowerInvariant() -cne $req.input_sha256){throw 'Copied input differs from its frozen hash.'}
    $book=$excel.Workbooks.Open($input,0,$true)
    $bootstrap.Close($false);$bootstrap=$null
    Stage 'after-open-before-calculation' $book
    $actualSettings=Settings
    if($actualSettings.Calculation -ne -4135 -or $actualSettings.Iteration -ne [bool]$condition.iteration -or
       $actualSettings.MaxIterations -ne [int]$condition.iterations -or $actualSettings.MaxChange -cne $condition.change_bits -or
       $actualSettings.EnableEvents -ne $false -or $actualSettings.mtc_enabled -ne $false){throw 'Post-open settings differ from the requested condition; no command was run.'}
    # Validate every formula cache, not an assumed blank-sheet seed.
    foreach($entry in $script:package.formulas){
        $key=Get-Key $entry.sheet $entry.row $entry.column
        $cache=$script:package.caches[$key]
        if($null -eq $cache){throw "No stored seed for $key"}
        $live=Read-Value (Read-Cell $book $entry)
        $expected=Cache-Value $cache
        $left=ConvertTo-Json -InputObject $live -Compress;$right=ConvertTo-Json -InputObject $expected -Compress
        if($left -cne $right){throw "Seed changed for ${key}: live=$left stored=$right; condition aborted."}
    }
    Stage 'seeds-verified' $book
    if($condition.PSObject.Properties['active_sheet']){$book.Worksheets.Item($condition.active_sheet).Activate();Stage 'after-activate' $book}
    for($command=1;$command -le [int]$condition.commands;$command++){
        Stage "before-command-$command" $book
        switch($condition.operation){
            'Calculate' {$excel.Calculate()}
            'Full' {$excel.CalculateFull()}
            'Rebuild' {$excel.CalculateFullRebuild()}
            'Sheet' {$book.ActiveSheet.Calculate()}
            'Range' {$book.ActiveSheet.Range($condition.range).Calculate()}
            default {throw 'Unknown calculation operation'}
        }
        Stage "after-command-$command" $book
    }
    $saved=Join-Path $outputDirectory 'saved.xlsx'
    Stage 'before-save' $book
    $book.SaveAs($saved,51)
    Stage 'after-save' $book
    $book.Close($false);$book=$null
    Stage 'after-close'
    $bootstrap=$excel.Workbooks.Add();$excel.Calculation=-4135
    $book=$excel.Workbooks.Open($saved,0,$true)
    Stage 'after-reopen' $book
    $book.Close($false);$book=$null
}catch{$fatal=$_.Exception.ToString()}
finally{
    if($book){$book.Close($false)}
    if($bootstrap){$bootstrap.Close($false)}
}
if($req.action -eq 'prepare'){
    Write-Json $Result @{fatal=$fatal;prepared=$prepared;generator_sha256=(Get-FileHash $generator).Hash}
}else{
    $zip=[IO.Compression.ZipFile]::OpenRead($req.input)
    $parts=[ordered]@{}
    foreach($part in 'xl/workbook.xml','xl/calcChain.xml'){$entry=$zip.GetEntry($part);if($entry){$reader=[IO.StreamReader]::new($entry.Open());$parts[$part]=$reader.ReadToEnd();$reader.Dispose()}else{$parts[$part]=$null}}
    $zip.Dispose()
    $excelPath=(Get-Process -Id (Get-ExcelPid $excel)).Path
    Write-Json $Result @{fatal=$fatal;condition=$req.condition;trial=$req.trial;input_sha256=(Get-FileHash $req.input).Hash.ToLowerInvariant();input_parts=$parts;observations=$observations;excel_version=[string]$excel.Version;excel_build=[string]$excel.Build;excel_file_version=(Get-Item $excelPath).VersionInfo.FileVersion;excel_machine=(Get-Machine $excelPath);excel_channel=(Get-Channel);cpu=(Get-CimInstance Win32_Processor|Select-Object -First 1 -ExpandProperty Name);locale=(Get-Culture).Name;generator_sha256=(Get-FileHash $generator).Hash;limitations='Pure COM lane, no observer UDF or event instrumentation. Save and reopen are separate recorded operations; initial stored seeds are verified before calculation.'}
}
