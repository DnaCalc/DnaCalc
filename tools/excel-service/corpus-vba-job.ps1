param(
    [Parameter(Mandatory)] $OwnedExcel,
    [Parameter(Mandatory)][string] $Request,
    [Parameter(Mandatory)][string] $Result
)
$ErrorActionPreference = 'Stop'
$drivers = (Get-Content -LiteralPath $Request -Raw -Encoding utf8 | ConvertFrom-Json).drivers
$run = [ordered]@{
    excel_version = [string]$OwnedExcel.Version
    excel_build = [string]$OwnedExcel.Build
    culture = (Get-Culture).Name
    machine = [Environment]::MachineName
    automation_security = [int]$OwnedExcel.AutomationSecurity
    rows = [Collections.Generic.List[object]]::new()
}

# Preserve VBA subtype tags before the Automation marshaller converts the values.
$wrapper = @'
Public Function DcCaptureEnvelope() As Variant
    Dim value As Variant, result() As Variant, lower As Long, upper As Long
    Dim i As Long, slot As Long
    value = DcExample()
    If IsArray(value) Then
        lower = LBound(value)
        upper = UBound(value)
        ReDim result(0 To 2 + 2 * (upper - lower + 1))
        result(0) = VarType(value)
        result(1) = lower
        result(2) = upper
        For i = lower To upper
            slot = 3 + 2 * (i - lower)
            result(slot) = VarType(value(i))
            result(slot + 1) = DcCapturePayload(value(i))
        Next i
    Else
        ReDim result(0 To 1)
        result(0) = VarType(value)
        result(1) = DcCapturePayload(value)
    End If
    DcCaptureEnvelope = result
End Function

Private Function DcCapturePayload(ByVal value As Variant) As Variant
    Select Case VarType(value)
        Case vbDate
            DcCapturePayload = CDbl(value)
        Case vbError
            DcCapturePayload = CStr(value)
        Case vbNull, vbEmpty
            DcCapturePayload = ""
        Case Else
            DcCapturePayload = value
    End Select
End Function
'@

function Read-Scalar([int]$Tag, $Value) {
    $scalar = [ordered]@{ vartype = $Tag }
    switch ($Tag) {
        { $_ -in 0, 1 } { $scalar.value = $null }
        4 { $scalar.bits = [BitConverter]::SingleToInt32Bits([single]$Value).ToString('x8') }
        { $_ -in 5, 7 } { $scalar.bits = [BitConverter]::DoubleToInt64Bits([double]$Value).ToString('x16') }
        { $_ -in 6, 14 } { $scalar.value = ([decimal]$Value).ToString([Globalization.CultureInfo]::InvariantCulture) }
        { $_ -in 2, 3, 17, 20 } { $scalar.value = [long]$Value }
        { $_ -in 8, 10 } { $scalar.value = [string]$Value }
        11 { $scalar.value = [bool]$Value }
        default { throw "Unsupported VBA return subtype: $Tag" }
    }
    return $scalar
}

foreach ($driver in $drivers) {
    $row = [ordered]@{ id = $driver.id; source_sha256 = $driver.sha256 }
    $book = $null
    try {
        if ((Get-FileHash -LiteralPath $driver.path -Algorithm SHA256).Hash -ine $driver.sha256) {
            throw 'The statically reviewed module hash changed.'
        }
        $book = $OwnedExcel.Workbooks.Add(-4167) # xlWBATWorksheet, one fresh blank sheet
        $components = $book.VBProject.VBComponents
        $null = $components.Import([string]$driver.path)
        $capture = $components.Add(1) # vbext_ct_StdModule
        $capture.CodeModule.AddFromString($wrapper)
        $answer = $OwnedExcel.Run(("'" + $book.Name + "'!DcCaptureEnvelope"))
        if ($answer -isnot [Array] -or $answer.Rank -ne 1 -or $answer.Length -lt 2) {
            throw 'Excel did not return a VBA capture envelope.'
        }
        $offset = $answer.GetLowerBound(0)
        $tag = [int]$answer.GetValue($offset)
        if ($tag -band 8192) { # vbArray
            $lower = [int]$answer.GetValue($offset + 1)
            $upper = [int]$answer.GetValue($offset + 2)
            if ($answer.Length -ne (3 + 2 * ($upper - $lower + 1))) {
                throw 'VBA array bounds differ from the capture envelope.'
            }
            $values = @(
                for ($slot = 3; $slot -lt $answer.Length; $slot += 2) {
                    Read-Scalar ($answer.GetValue($offset + $slot)) ($answer.GetValue($offset + $slot + 1))
                }
            )
            $row.answer = @{ vartype = $tag; lower_bound = $lower; upper_bound = $upper; values = $values }
        } else {
            $row.answer = Read-Scalar $tag ($answer.GetValue($offset + 1))
        }
    } catch {
        $row.execution_error = @{ message = $_.Exception.Message; hresult = $_.Exception.HResult }
    } finally {
        if ($null -ne $book) { $book.Close($false) }
    }
    $run.rows.Add($row)
    # Keep progress even if the host's deadline interrupts a later module.
    $run | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath "$Result.tmp" -Encoding utf8NoBOM
    Move-Item -LiteralPath "$Result.tmp" -Destination $Result -Force
}

if ($run.rows | Where-Object { $_.Contains('execution_error') }) {
    throw 'One or more VBA drivers did not return a captured answer. See result.json.'
}
