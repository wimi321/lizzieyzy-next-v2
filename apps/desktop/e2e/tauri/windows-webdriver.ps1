param([ValidateSet('Run', 'Restore')][string]$Action = 'Run')

$env:PATH = "$HOME/.cargo/bin;$env:PATH"
$ErrorActionPreference = 'Stop'
if (-not $IsWindows -or $env:GITHUB_ACTIONS -ne 'true' -or -not $env:RUNNER_TEMP) {
    throw 'The WebView2 debug policy is restricted to Windows GitHub Actions runners.'
}
if ($env:GITHUB_RUN_ID -notmatch '^\d+$' -or $env:GITHUB_RUN_ATTEMPT -notmatch '^\d+$') {
    throw 'GitHub run identity is required for policy recovery.'
}
. (Join-Path $PSScriptRoot 'windows-policy.ps1')

$desktop = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$diagnostics = Join-Path $desktop 'test-results/tauri'
New-Item -ItemType Directory -Path $diagnostics -Force | Out-Null
$statePath = Join-Path $env:RUNNER_TEMP "lizzie-webview2-policy-${env:GITHUB_RUN_ID}-${env:GITHUB_RUN_ATTEMPT}.clixml"
$reportPath = Join-Path $diagnostics "windows-policy-$($Action.ToLowerInvariant()).json"
$subKeyPath = 'SOFTWARE\Policies\Microsoft\Edge\WebView2\AdditionalBrowserArguments'
$valueName = 'lizzieyzy-desktop.exe'
# HKLM is honored by elevated WebView2 hosts. Match this executable only;
# never install a wildcard policy or change production Tauri configuration.
# https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/security#for-an-elevated-host-app-use-appropriate-override-flags
# https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/webview2-idl#createcorewebview2environmentwithoptions
$arguments = '--remote-debugging-address=127.0.0.1 --remote-debugging-port=9222'
$registry = [Microsoft.Win32.RegistryKey]::OpenBaseKey(
    [Microsoft.Win32.RegistryHive]::LocalMachine, [Microsoft.Win32.RegistryView]::Registry64)
$report = [ordered]@{
    registryPath = "HKLM\$subKeyPath"
    application = $valueName
    arguments = $arguments
    action = $Action
    restored = $false
}

function Write-PolicyReport {
    param($State)
    if ($null -ne $State) {
        $report.originalExists = $State.originalExists
        $report.originalKind = $State.originalKind
        $report.originalValue = $State.originalValue
        $report.restored = $State.restored
    }
    $report.updatedAt = [DateTime]::UtcNow.ToString('o')
    $report | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $reportPath -Encoding utf8
}

try {
    if ($Action -eq 'Run') {
        # Do not overwrite a previous run's recovery record.
        if (Test-Path -LiteralPath $statePath) { throw "Restore pending WebView2 policy first: $statePath" }
        try {
            $state = Enable-WebViewPolicy -RegistryRoot $registry -SubKeyPath $subKeyPath `
                -ValueName $valueName -Arguments $arguments -StatePath $statePath
            Write-PolicyReport $state
            & pnpm -C $desktop e2e:tauri
            if ($LASTEXITCODE -ne 0) { throw "Desktop E2E exited with $LASTEXITCODE" }
        } finally {
            $state = Restore-WebViewPolicy -RegistryRoot $registry -SubKeyPath $subKeyPath `
                -ValueName $valueName -StatePath $statePath
            Write-PolicyReport $state
        }
    } else {
        # Idempotent fallback for the workflow's always() step, including when
        # Run was interrupted before its finally block could execute.
        $state = Restore-WebViewPolicy -RegistryRoot $registry -SubKeyPath $subKeyPath `
            -ValueName $valueName -StatePath $statePath
        $report.noRecoveryNeeded = $null -eq $state
        Write-PolicyReport $state
    }
} catch {
    $report.error = $_.Exception.Message
    Write-PolicyReport $null
    throw
} finally { $registry.Dispose() }
