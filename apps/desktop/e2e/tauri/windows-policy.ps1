# E2E-only registry transaction helpers. The caller supplies the registry root
# so tests can execute the same transaction against an isolated HKCU key.
function Save-WebViewPolicyState {
    param([Parameter(Mandatory)]$State, [Parameter(Mandatory)][string]$StatePath)
    $pending = "$StatePath.pending"
    $State | Export-Clixml -LiteralPath $pending -Depth 5
    Move-Item -LiteralPath $pending -Destination $StatePath -Force
}

function Enable-WebViewPolicy {
    param(
        [Parameter(Mandatory)][Microsoft.Win32.RegistryKey]$RegistryRoot,
        [Parameter(Mandatory)][string]$SubKeyPath,
        [Parameter(Mandatory)][string]$ValueName,
        [Parameter(Mandatory)][string]$Arguments,
        [Parameter(Mandatory)][string]$StatePath
    )
    if (Test-Path -LiteralPath $StatePath) {
        throw "A policy recovery file already exists; restore it before enabling again: $StatePath"
    }
    $key = $RegistryRoot.OpenSubKey($SubKeyPath)
    try {
        $exists = $null -ne $key -and $key.GetValueNames() -contains $ValueName
        $state = [pscustomobject]@{
            version = 1
            subKeyPath = $SubKeyPath
            valueName = $ValueName
            originalExists = $exists
            originalKind = if ($exists) { $key.GetValueKind($ValueName).ToString() } else { $null }
            originalValue = $null
            appliedArguments = $Arguments
            restored = $false
        }
        if ($exists) {
            $state.originalValue = $key.GetValue($ValueName, $null,
                [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
        }
    } finally { if ($key) { $key.Dispose() } }
    # Persist recovery before the first registry mutation. A later always()
    # step can restore it even if this process is interrupted after SetValue.
    Save-WebViewPolicyState -State $state -StatePath $StatePath
    $key = $RegistryRoot.CreateSubKey($SubKeyPath, $true)
    try {
        $key.SetValue($ValueName, $Arguments, [Microsoft.Win32.RegistryValueKind]::String)
        if ($key.GetValue($ValueName) -cne $Arguments) { throw 'WebView2 policy readback did not match' }
    } finally { if ($key) { $key.Dispose() } }
    return $state
}

function Restore-WebViewPolicy {
    param(
        [Parameter(Mandatory)][Microsoft.Win32.RegistryKey]$RegistryRoot,
        [Parameter(Mandatory)][string]$SubKeyPath,
        [Parameter(Mandatory)][string]$ValueName,
        [Parameter(Mandatory)][string]$StatePath
    )
    if (-not (Test-Path -LiteralPath $StatePath)) { return $null }
    $state = Import-Clixml -LiteralPath $StatePath
    if ($state.version -ne 1 -or $state.subKeyPath -cne $SubKeyPath -or $state.valueName -cne $ValueName) {
        throw 'Policy recovery file does not match the expected application and registry key'
    }
    if ($state.restored) { return $state }
    $key = if ($state.originalExists) {
        $RegistryRoot.CreateSubKey($SubKeyPath, $true)
    } else {
        $RegistryRoot.OpenSubKey($SubKeyPath, $true)
    }
    try {
        if ($state.originalExists) {
            $kind = [Microsoft.Win32.RegistryValueKind]$state.originalKind
            $original = switch ($kind) {
                Binary { ,([byte[]]$state.originalValue) }
                MultiString { ,([string[]]$state.originalValue) }
                DWord { [int]$state.originalValue }
                QWord { [long]$state.originalValue }
                default { $state.originalValue }
            }
            $key.SetValue($ValueName, $original, $kind)
            $actual = $key.GetValue($ValueName, $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
            # CLIXML preserves registry scalar/array types, including unexpanded
            # REG_EXPAND_SZ contents. Compare serialized values for arrays too.
            if ($key.GetValueKind($ValueName) -ne $kind -or
                (ConvertTo-Json -InputObject $actual -Compress) -cne
                (ConvertTo-Json -InputObject $state.originalValue -Compress)) {
                throw 'Original WebView2 policy was not restored exactly'
            }
        } elseif ($key) {
            # Never remove the key: it may contain policies for other apps.
            $key.DeleteValue($ValueName, $false)
            if ($key.GetValueNames() -contains $ValueName) { throw 'Temporary WebView2 policy was not removed' }
        }
    } finally { if ($key) { $key.Dispose() } }
    $state.restored = $true
    Save-WebViewPolicyState -State $state -StatePath $StatePath
    return $state
}
