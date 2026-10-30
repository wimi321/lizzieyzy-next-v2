# Execute real registry transactions in a unique HKCU key; no source scanning
# and no machine-wide policy writes are involved in these tests.
$env:PATH = "$HOME/.cargo/bin;$env:PATH"
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'Registry transaction tests require Windows.' }
. (Join-Path $PSScriptRoot 'windows-policy.ps1')

$registry = [Microsoft.Win32.RegistryKey]::OpenBaseKey(
    [Microsoft.Win32.RegistryHive]::CurrentUser, [Microsoft.Win32.RegistryView]::Registry64)
$subKeyPath = "SOFTWARE\LizzieYzyE2ETests\$([Guid]::NewGuid())"
$directory = Join-Path ([IO.Path]::GetTempPath()) "lizzie policy tests $([Guid]::NewGuid())"
New-Item -ItemType Directory -Path $directory | Out-Null
$valueName = 'lizzieyzy-desktop.exe'
$arguments = '--remote-debugging-address=127.0.0.1 --remote-debugging-port=9222'
$passed = 0

function Assert-Equal($Actual, $Expected, [string]$Message) {
    if ($Actual -cne $Expected) { throw "$Message; actual: $Actual; expected: $Expected" }
}

try {
    $key = $registry.CreateSubKey($subKeyPath, $true)
    try { $key.SetValue('other.exe', 'keep me') } finally { $key.Dispose() }
    $statePath = Join-Path $directory 'absent.clixml'
    $state = Enable-WebViewPolicy $registry $subKeyPath $valueName $arguments $statePath
    Assert-Equal $state.originalExists $false 'New policy must record absence'
    $key = $registry.OpenSubKey($subKeyPath)
    try { Assert-Equal $key.GetValue($valueName) $arguments 'Debug arguments must be applied' }
    finally { $key.Dispose() }
    $null = Restore-WebViewPolicy $registry $subKeyPath $valueName $statePath
    $null = Restore-WebViewPolicy $registry $subKeyPath $valueName $statePath
    $key = $registry.OpenSubKey($subKeyPath)
    try {
        Assert-Equal ($key.GetValueNames() -contains $valueName) $false 'Added app policy must be removed'
        Assert-Equal $key.GetValue('other.exe') 'keep me' 'Other app policy must survive'
    } finally { $key.Dispose() }
    $passed++

    $statePath = Join-Path $directory 'existing.clixml'
    $original = '--log-file=%TEMP%\a file.log'
    $key = $registry.OpenSubKey($subKeyPath, $true)
    try { $key.SetValue($valueName, $original, [Microsoft.Win32.RegistryValueKind]::ExpandString) }
    finally { $key.Dispose() }
    $failed = $false
    try {
        $null = Enable-WebViewPolicy $registry $subKeyPath $valueName $arguments $statePath
        throw 'Simulated E2E failure'
    } catch {
        Assert-Equal $_.Exception.Message 'Simulated E2E failure' 'Setup must succeed before the simulated failure'
        $failed = $true
    }
    finally { $null = Restore-WebViewPolicy $registry $subKeyPath $valueName $statePath }
    Assert-Equal $failed $true 'Failure path must execute'
    $key = $registry.OpenSubKey($subKeyPath)
    try {
        Assert-Equal $key.GetValueKind($valueName) ([Microsoft.Win32.RegistryValueKind]::ExpandString) 'Registry type must restore'
        Assert-Equal ($key.GetValue($valueName, $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)) `
            $original 'Unexpanded original contents must restore'
    } finally { $key.Dispose() }
    $passed++

    foreach ($case in @(
        @{ kind = [Microsoft.Win32.RegistryValueKind]::Binary; value = [byte[]]@(0, 128, 255) },
        @{ kind = [Microsoft.Win32.RegistryValueKind]::QWord; value = [long]9223372036854775806 },
        @{ kind = [Microsoft.Win32.RegistryValueKind]::MultiString; value = [string[]]@('a', 'b c') }
    )) {
        $key = $registry.OpenSubKey($subKeyPath, $true)
        try { $key.SetValue($valueName, $case.value, $case.kind) } finally { $key.Dispose() }
        $statePath = Join-Path $directory "$($case.kind).clixml"
        $null = Enable-WebViewPolicy $registry $subKeyPath $valueName $arguments $statePath
        $null = Restore-WebViewPolicy $registry $subKeyPath $valueName $statePath
        $key = $registry.OpenSubKey($subKeyPath)
        try {
            Assert-Equal $key.GetValueKind($valueName) $case.kind 'Original non-string type must restore'
            Assert-Equal (ConvertTo-Json -InputObject $key.GetValue($valueName) -Compress) `
                (ConvertTo-Json -InputObject $case.value -Compress) 'Original non-string value must restore'
        } finally { $key.Dispose() }
        $passed++
    }

    # Simulate a cancelled process: enable and restore in separate invocations
    # of the transaction API, using only its durable recovery file.
    $statePath = Join-Path $directory 'cancelled.clixml'
    $null = Enable-WebViewPolicy $registry $subKeyPath $valueName $arguments $statePath
    $refused = $false
    try { $null = Enable-WebViewPolicy $registry $subKeyPath $valueName $arguments $statePath }
    catch { $refused = $true }
    Assert-Equal $refused $true 'A pending backup must never be overwritten'
    $restored = Restore-WebViewPolicy $registry $subKeyPath $valueName $statePath
    Assert-Equal $restored.restored $true 'Recovery invocation must restore pending policy'
    $passed++

    $rejected = $false
    try { $null = Restore-WebViewPolicy $registry $subKeyPath 'unrelated.exe' $statePath }
    catch { $rejected = $true }
    Assert-Equal $rejected $true 'Recovery must reject a mismatched app'
    $passed++
    Write-Output "Windows policy transaction tests: $passed passed"
} finally {
    $registry.DeleteSubKeyTree($subKeyPath, $false)
    $registry.Dispose()
    Remove-Item -LiteralPath $directory -Recurse -Force
}
