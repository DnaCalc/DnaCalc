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
    foreach($key in 'Calculation','CalculateBeforeSave','Iteration','MaxIterations','MaxChange','EnableEvents','CalculationState','AutomationSecurity'){$v=Get-ComProperty $excel $key;if($key -eq 'MaxChange'){$v=[BitConverter]::DoubleToInt64Bits([double]$v).ToString('x16')};$s[$key]=$v}
    $s['mtc_enabled']=Get-ComProperty $excel.MultiThreadedCalculation 'Enabled'
    $s['mtc_count']=Get-ComProperty $excel.MultiThreadedCalculation 'ThreadCount'
    $s['mtc_mode']=Get-ComProperty $excel.MultiThreadedCalculation 'ThreadMode'
    return $s
}
function Snap($Workbook){
    $cells=@()
    $entries = @($script:package.formulas)
    if($req.PSObject.Properties['only_observe_cells'] -and $req.only_observe_cells){$entries=@()}
    if($req.PSObject.Properties['observe_cells']){
        foreach($at in $req.observe_cells){
            $place=Split-Reference $at.address
            $entries+=@{sheet=$at.sheet;row=$place.row;column=$place.column}
        }
    }
    $seen=@{}
    foreach($entry in $entries){
        $key=Get-Key $entry.sheet $entry.row $entry.column
        if($seen.ContainsKey($key)){continue};$seen[$key]=$true
        $cell=Read-Cell $Workbook $entry;$runtimeType=$null
        $value=Read-Value $cell ([ref]$runtimeType)
        $cells+=@{sheet=$entry.sheet;row=$entry.row;column=$entry.column;formula=(Get-ComProperty $cell 'Formula2');value=$value;value2_runtime_type=$runtimeType;metadata=(Read-FormulaMetadata $cell);display_text=(Get-ComProperty $cell 'Text')}
    }
    return ,$cells
}
function Stage([string]$Label,$Workbook=$null){
    $bookSettings=[ordered]@{}
    if($Workbook){foreach($key in 'Date1904','PrecisionAsDisplayed','ForceFullCalculation'){ $bookSettings[$key]=Get-ComProperty $Workbook $key }}
    $item=[ordered]@{stage=$Label;utc=[DateTime]::UtcNow.ToString('O');settings=(Settings);workbook_settings=$bookSettings;active_sheet=$(if($Workbook){[string]$Workbook.ActiveSheet.Name}else{$null});cells=$(if($Workbook){Snap $Workbook}else{@()})}
    if($Workbook){
        $item['active_cell']=[string]$excel.ActiveCell.Address()
        $item['identity']=@{name=[string]$Workbook.Name;path=[string]$Workbook.Path;file_format=[int]$Workbook.FileFormat;saved=[bool]$Workbook.Saved}
    }
    if($Workbook -and $req.action -eq 'names'){
        $item['names']=@($Workbook.Names | ForEach-Object {
            $n=$_;$fields=@{}
            foreach($property in 'Name','RefersTo','RefersToR1C1','Value'){
                try{$fields[$property]=@{value=(Get-ComProperty $n $property)}}catch{$fields[$property]=@{unread=$_.Exception.ToString();hresult=$_.Exception.HResult}}
            }
            try{$range=Get-ComProperty $n 'RefersToRange';$fields['RefersToRange']=@{address=[string]$range.Address();sheet=[string]$range.Parent.Name;value=(Read-Value $range)}}catch{$fields['RefersToRange']=@{unread=$_.Exception.ToString();hresult=$_.Exception.HResult}}
            $fields
        })
    }
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
            foreach($entry in $formulas){
                if((Get-ComProperty $sheet.Range($entry.Name) 'Formula2') -cne [string]$entry.Value){throw ('Authored Formula2 readback differs for '+$entry.Name)}
            }
            $path=Join-Path $req.inputs_directory ($condition.id+'.xlsx')
            $book.SaveAs($path,51);$book.Close($false);$book=$null
            $stored=@{};foreach($entry in $condition.seeds.PSObject.Properties){$stored['S!'+$entry.Name]=$entry.Value}
            $zip=[IO.Compression.ZipFile]::OpenRead($path);$hasChain=$null -ne $zip.GetEntry('xl/calcChain.xml');$zip.Dispose()
            Set-StoredValues $path $stored ([bool]$condition.reverse_chain -and $hasChain)
            $storedForms=@{}
            $zip=[IO.Compression.ZipFile]::OpenRead($path)
            try{
                $parts=Get-SheetParts $zip;$document=Read-Part $zip $parts['S']
                $names=[Xml.XmlNamespaceManager]::new($document.NameTable);$names.AddNamespace('x',$MainNs)
                foreach($entry in $formulas){
                    $node=$document.SelectSingleNode(("/x:worksheet/x:sheetData/x:row/x:c[@r='"+$entry.Name+"']/x:f"),$names)
                    if($null -eq $node){throw ('Stored formula absent for '+$entry.Name)}
                    $storedForms[$entry.Name]=$node.InnerText
                }
            }finally{$zip.Dispose()}
            $prepared+=@{id=$condition.id;path=$path;stored_formulas=$storedForms;sha256=(Get-FileHash -LiteralPath $path).Hash.ToLowerInvariant();reverse_chain_requested=[bool]$condition.reverse_chain;chain_present=$hasChain;success=$true}
            }catch{
                $prepared+=@{id=$condition.id;success=$false;phase=$phase;error=$_.Exception.ToString();requested_change_bits=$condition.change_bits;settings_readback=(Settings)}
                if($book){$book.Close($false);$book=$null}
            }
        }
        Write-Json $Result @{fatal=$null;prepared=$prepared;generator_sha256=(Get-FileHash $generator).Hash}
        return
    }
    if($req.action -eq 'names'){
        $condition=$req.condition;$excel.Iteration=$false
        $book=$excel.Workbooks.Add(-4167);$first=$book.Worksheets.Item(1);$first.Name='Sheet1'
        $other=Invoke-ComMethod $book.Worksheets 'Add' @($Missing,$first);$other.Name='Other'
        $first.Activate()
        $bootstrap.Close($false);$bootstrap=$null
        $script:package=@{formulas=@()}
        foreach($entry in $condition.cells.PSObject.Properties){
            $parts=$entry.Name.Split('!');$sheet=$book.Worksheets.Item($parts[0]);$at=Split-Reference $parts[1]
            if($entry.Value.PSObject.Properties['formula']){$script:package.formulas+=@{sheet=$parts[0];row=$at.row;column=$at.column}}
            else{
                $cell=$sheet.Range($parts[1]);$cell.Value2=[double]$entry.Value
                $actual=Read-Value $cell;$expected=Convert-Value ([double]$entry.Value)
                if(-not(Same-Raw $actual $expected)){throw ('Named-condition typed seed differs for '+$entry.Name)}
            }
        }
        Stage 'constants-before-names' $book
        foreach($name in $condition.names){
            $scope=if($name.scope -eq 'workbook'){$book}else{$book.Worksheets.Item([string]$name.scope)}
            $spelling=if($name.scope -eq 'workbook'){[string]$name.name}else{"'"+([string]$name.scope).Replace("'","''")+"'!"+[string]$name.name}
            $created=Invoke-ComMethod $scope.Names 'Add' @($spelling,[string]$name.refers_to)
            $observations.Add(@{operation='Names.Add';requested_scope=$name.scope;requested_name=$name.name;requested_refers_to=$name.refers_to;actual_name=[string]$created.Name;actual_refers_to=[string]$created.RefersTo})
            Stage ('after-name-'+$name.scope+'-'+$name.name) $book
        }
        foreach($entry in $condition.cells.PSObject.Properties){
            if(-not $entry.Value.PSObject.Properties['formula']){continue}
            $parts=$entry.Name.Split('!');$cell=$book.Worksheets.Item($parts[0]).Range($parts[1])
            Stage ('before-entry-'+$entry.Name) $book
            try{$cell.Formula2=[string]$entry.Value.formula;$observations.Add(@{operation='formula-entry';caller=$entry.Name;authored=$entry.Value.formula;success=$true})}
            catch{$observations.Add(@{operation='formula-entry';caller=$entry.Name;authored=$entry.Value.formula;success=$false;error=$_.Exception.ToString();hresult=$_.Exception.HResult})}
            Stage ('after-entry-'+$entry.Name) $book
        }
        $settings=Settings
        if($settings.Calculation -ne -4135 -or $settings.Iteration -or $settings.mtc_enabled -or $settings.EnableEvents){throw 'Named-condition settings differ'}
        Stage 'before-CalculateFull' $book;$excel.CalculateFull();Stage 'after-CalculateFull' $book
        $excel.CalculateFullRebuild();Stage 'after-CalculateFullRebuild' $book
        if($condition.id -eq 'eng19-P12-shadowed-explicit-evaluate' -or $condition.PSObject.Properties['evaluate_formula']){
            foreach($sheetName in 'Sheet1','Other'){
                $sheet=$book.Worksheets.Item($sheetName);$sheet.Activate();Stage ('evaluate-context-'+$sheetName) $book
                $evaluations=@($(if($condition.PSObject.Properties['evaluate_formula']){@($condition.evaluate_formula)}else{'=Choice','=Alias','=Sheet1!Choice'}) | ForEach-Object {@{expression=$_;selected_name=$null;selected_refers_to=$null}})
                foreach($selected in $book.Names){$evaluations+=@{expression=[string]$selected.RefersTo;selected_name=[string]$selected.Name;selected_refers_to=[string]$selected.RefersTo}}
                foreach($evaluation in $evaluations){
                    $expression=[string]$evaluation.expression
                    foreach($target in @(@('Worksheet',$sheet),@('Application',$excel))){
                        try{$v=Invoke-ComMethod $target[1] 'Evaluate' @($expression);$observations.Add(@{operation='Evaluate';target=$target[0];sheet=$sheetName;expression=$expression;selected_name=$evaluation.selected_name;selected_refers_to=$evaluation.selected_refers_to;value=(Convert-Value $v);runtime_type=$(if($null -eq $v){$null}else{$v.GetType().FullName})})}
                        catch{$observations.Add(@{operation='Evaluate';target=$target[0];sheet=$sheetName;expression=$expression;selected_name=$evaluation.selected_name;selected_refers_to=$evaluation.selected_refers_to;error=$_.Exception.ToString();hresult=$_.Exception.HResult})}
                    }
                }
            }
        }
        Stage 'before-save' $book;$saved=Join-Path $outputDirectory 'saved.xlsx';$book.SaveAs($saved,51);Stage 'after-save' $book
        $book.Close($false);$book=$null;$book=$excel.Workbooks.Open($saved,0,$true);Stage 'after-reopen-separate' $book
        Write-Json $Result @{fatal=$null;condition=$condition;trial=$req.trial;observations=$observations;excel_version=[string]$excel.Version;excel_build=[string]$excel.Build;limitations='Authored names and COM Formula2/Evaluate observations. Hidden worksheet activation changes internal name context only. Save/reopen observations are separate.'}
        return
    }
    if($req.action -eq 'peek'){
        if((Get-FileHash -LiteralPath $req.input).Hash.ToLowerInvariant() -cne $req.input_sha256){throw 'Original Peek input hash differs'}
        $script:package=Read-Package $req.input
        $input=Join-Path $outputDirectory 'input.xlsm';Copy-Item -LiteralPath $req.input -Destination $input
        $book=$excel.Workbooks.Open($input,0,$true);$bootstrap.Close($false);$bootstrap=$null
        Stage 'after-open-before-setup' $book
        $module=$book.VBProject.VBComponents.Item('OracleProbe').CodeModule
        $readback=[string]$module.Lines(1,$module.CountOfLines)
        [IO.File]::WriteAllText((Join-Path $outputDirectory 'original-OracleProbe.bas'),$readback,[Text.UTF8Encoding]::new($false))
        foreach($expression in '=PeekB1()','=PeekSeveral()'){
            try{$v=Invoke-ComMethod $book.Worksheets.Item('S') 'Evaluate' @($expression);$observations.Add(@{operation='Worksheet.Evaluate';expression=$expression;value=(Convert-Value $v);runtime_type=$(if($null -eq $v){$null}else{$v.GetType().FullName})})}
            catch{$observations.Add(@{operation='Worksheet.Evaluate';expression=$expression;error=$_.Exception.ToString();hresult=$_.Exception.HResult})}
        }
        Stage 'after-live-Peek-before-setup' $book
        Write-Json $Result @{fatal=$null;condition=$req.condition;trial=$req.trial;input_sha256=$req.input_sha256;observations=$observations;excel_version=[string]$excel.Version;excel_build=[string]$excel.Build;limitations='Original code unchanged. No setup arguments or event history invented; original-setup and later reopen remain separate unrun obligations.'}
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
                $wantedStored=if($condition.PSObject.Properties['stored_formulas']){[string]$condition.stored_formulas.($declared.Name)}else{([string]$declared.Value).TrimStart('=')}
                if($null -eq $formula -or $formula.InnerText -cne $wantedStored){throw "Stored formula differs from its prepared spelling for $($declared.Name)"}

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
    $wantedMode=if($condition.PSObject.Properties['calculation_mode'] -and $condition.calculation_mode -eq 'automatic'){-4105}else{-4135}
    if($condition.PSObject.Properties['calculate_before_save']){$excel.CalculateBeforeSave=[bool]$condition.calculate_before_save}
    if($excel.Calculation -ne $wantedMode){$excel.Calculation=$wantedMode}
    Stage 'before-open'
    $input=Join-Path $outputDirectory 'input.xlsx'
    Copy-Item -LiteralPath $req.input -Destination $input
    if((Get-FileHash -LiteralPath $input).Hash.ToLowerInvariant() -cne $req.input_sha256){throw 'Copied input differs from its frozen hash.'}
    $book=$excel.Workbooks.Open($input,0,$true)
    $bootstrap.Close($false);$bootstrap=$null
    Stage 'after-open-before-calculation' $book
    $actualSettings=Settings
    if($actualSettings.Calculation -ne $wantedMode -or $actualSettings.Iteration -ne [bool]$condition.iteration -or
       $actualSettings.MaxIterations -ne [int]$condition.iterations -or $actualSettings.MaxChange -cne $condition.change_bits -or
       $actualSettings.EnableEvents -ne $false -or $actualSettings.mtc_enabled -ne $false){throw 'Post-open settings differ from the requested condition; no command was run.'}
    if($req.PSObject.Properties['full_precision'] -and $req.full_precision -and (Get-ComProperty $book 'PrecisionAsDisplayed')){throw 'Requested full precision differs after Open'}
    if($condition.PSObject.Properties['calculate_before_save'] -and $actualSettings.CalculateBeforeSave -ne [bool]$condition.calculate_before_save){throw 'CalculateBeforeSave differs after Open'}
    # Validate every formula cache in the original seeded profile.
    if(-not($req.PSObject.Properties['unseeded_observation'] -and $req.unseeded_observation)){
    foreach($entry in $script:package.formulas){
        $key=Get-Key $entry.sheet $entry.row $entry.column
        $cache=$script:package.caches[$key]
        if($null -eq $cache){throw "No stored seed for $key"}
        $live=Read-Value (Read-Cell $book $entry)
        $expected=Cache-Value $cache
        $left=ConvertTo-Json -InputObject $live -Compress;$right=ConvertTo-Json -InputObject $expected -Compress
        if($left -cne $right){throw "Seed changed for ${key}: live=$left stored=$right; condition aborted."}
    }
    }
    if($condition.PSObject.Properties['formulas']){
        foreach($declared in $condition.formulas.PSObject.Properties){
            if((Get-ComProperty $book.Worksheets.Item('S').Range($declared.Name) 'Formula2') -cne [string]$declared.Value){throw ('Live authored Formula2 differs for '+$declared.Name)}
        }
    }
    Stage $(if($req.PSObject.Properties['unseeded_observation'] -and $req.unseeded_observation){'startup-observed-no-declared-seeds'}else{'seeds-verified'}) $book
    if($condition.PSObject.Properties['calculate_before_save']){$excel.CalculateBeforeSave=[bool]$condition.calculate_before_save}
    if($condition.PSObject.Properties['calculation_mode'] -and $condition.calculation_mode -eq 'automatic' -and $excel.Calculation -ne -4105){throw 'Requested Automatic mode changed before calculation'}
    if($condition.PSObject.Properties['active_sheet']){$book.Worksheets.Item($condition.active_sheet).Activate();Stage 'after-activate' $book}
    for($command=1;$command -le [int]$condition.commands;$command++){
        Stage "before-command-$command" $book
        $operation=if($condition.PSObject.Properties['command_sequence']){$condition.command_sequence[$command-1]}else{$condition.operation}
        switch($operation){
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
    if($condition.PSObject.Properties['persistence'] -and $condition.persistence){
        $snapshot=Join-Path $outputDirectory 'immediate-saved-package.xlsx'
        $sourceStream=[IO.File]::Open($saved,[IO.FileMode]::Open,[IO.FileAccess]::Read,([IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete))
        try{$snapshotStream=[IO.File]::Create($snapshot);try{$sourceStream.CopyTo($snapshotStream)}finally{$snapshotStream.Dispose()}}finally{$sourceStream.Dispose()}
        $savedPackage=Read-Package $snapshot
        Write-Json (Join-Path $outputDirectory 'saved-package-caches.json') $savedPackage.caches
        Write-Json (Join-Path $outputDirectory 'saved-package-hash.json') @{snapshot=$snapshot;sha256=(Get-FileHash $snapshot).Hash.ToLowerInvariant()}
    }
    $book.Close($false);$book=$null
    Stage 'after-close'
    if($condition.PSObject.Properties['persistence'] -and $condition.persistence){Write-Json (Join-Path $outputDirectory 'post-close-saved-hash.json') @{sha256=(Get-FileHash $saved).Hash.ToLowerInvariant()}}
    $bootstrap=$excel.Workbooks.Add();$excel.Calculation=$(if($condition.PSObject.Properties['persistence'] -and $condition.persistence -and $condition.calculation_mode -eq 'automatic'){-4105}else{-4135})
    $book=$excel.Workbooks.Open($saved,0,$true)
    Stage 'after-reopen' $book
    if($condition.PSObject.Properties['persistence'] -and $condition.persistence){
        Stage 'before-final-rebuild' $book;$excel.CalculateFullRebuild();Stage 'after-final-rebuild' $book
    }
    $book.Close($false);$book=$null
}catch{$fatal=$_.Exception.ToString()}
finally{
    if($book){$book.Close($false)}
    if($bootstrap){$bootstrap.Close($false)}
}
if($req.action -eq 'prepare'){
    Write-Json $Result @{fatal=$fatal;prepared=$prepared;generator_sha256=(Get-FileHash $generator).Hash}
 }elseif($req.action -in @('names','peek')){
    Write-Json $Result @{fatal=$fatal;condition=$req.condition;trial=$req.trial;observations=$observations;excel_version=[string]$excel.Version;excel_build=[string]$excel.Build}
}else{
    $zip=[IO.Compression.ZipFile]::OpenRead($req.input)
    $parts=[ordered]@{}
    foreach($part in 'xl/workbook.xml','xl/calcChain.xml'){$entry=$zip.GetEntry($part);if($entry){$reader=[IO.StreamReader]::new($entry.Open());$parts[$part]=$reader.ReadToEnd();$reader.Dispose()}else{$parts[$part]=$null}}
    $zip.Dispose()
    $excelPath=(Get-Process -Id (Get-ExcelPid $excel)).Path
    Write-Json $Result @{fatal=$fatal;condition=$req.condition;trial=$req.trial;input_sha256=(Get-FileHash $req.input).Hash.ToLowerInvariant();input_parts=$parts;observations=$observations;excel_version=[string]$excel.Version;excel_build=[string]$excel.Build;excel_file_version=(Get-Item $excelPath).VersionInfo.FileVersion;excel_machine=(Get-Machine $excelPath);excel_channel=(Get-Channel);cpu=(Get-CimInstance Win32_Processor|Select-Object -First 1 -ExpandProperty Name);locale=(Get-Culture).Name;generator_sha256=(Get-FileHash $generator).Hash;limitations='Pure COM lane, no observer UDF or event instrumentation. Save and reopen are separate recorded operations; initial stored seeds are verified before calculation.'}
}
