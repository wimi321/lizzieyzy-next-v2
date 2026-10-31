param(
    [ValidateSet('Run', 'Restore')][string]$Action = 'Run',
    [string]$AcceptanceDir = '',
    [string]$Driver = ''
)

$env:PATH = "$HOME/.cargo/bin;$env:PATH"
$ErrorActionPreference = 'Stop'
if (-not $IsWindows -or $env:GITHUB_ACTIONS -ne 'true' -or -not $env:RUNNER_TEMP) {
    throw 'The temporary WebView2 policy is restricted to Windows GitHub Actions runners.'
}
if ($env:GITHUB_RUN_ID -notmatch '^\d+$' -or $env:GITHUB_RUN_ATTEMPT -notmatch '^\d+$') {
    throw 'GitHub run identity is required for policy recovery.'
}
# Reuse the existing, behavior-tested registry transaction. This file and the
# imported helper are CI-only and neither is included in bundle resources.
. (Join-Path $PSScriptRoot '../../apps/desktop/e2e/tauri/windows-policy.ps1')
$statePath = Join-Path $env:RUNNER_TEMP "lizzie-bundle-policy-${env:GITHUB_RUN_ID}-${env:GITHUB_RUN_ATTEMPT}.clixml"
$subKeyPath = 'SOFTWARE\Policies\Microsoft\Edge\WebView2\AdditionalBrowserArguments'
$valueName = 'lizzieyzy-desktop.exe'
$arguments = '--remote-debugging-address=127.0.0.1 --remote-debugging-port=9222'
$registry = [Microsoft.Win32.RegistryKey]::OpenBaseKey(
    [Microsoft.Win32.RegistryHive]::LocalMachine, [Microsoft.Win32.RegistryView]::Registry64)
try {
    if ($Action -eq 'Restore') {
        # Safe after any earlier workflow step failed; no driver is required.
        $state = Restore-WebViewPolicy -RegistryRoot $registry -SubKeyPath $subKeyPath `
            -ValueName $valueName -StatePath $statePath
        if ($null -ne $state -and -not $state.restored) { throw 'WebView2 policy restoration failed' }
        Write-Output 'Bundled smoke WebView2 policy restored (or no recovery was needed).'
    } else {
        if (-not $AcceptanceDir -or -not $Driver -or -not (Test-Path -LiteralPath $Driver)) {
            throw 'Run requires an acceptance directory and native Edge WebDriver.'
        }
        try {
            $null = Enable-WebViewPolicy -RegistryRoot $registry -SubKeyPath $subKeyPath `
                -ValueName $valueName -Arguments $arguments -StatePath $statePath
            & python (Join-Path $PSScriptRoot 'smoke.py') --acceptance-dir $AcceptanceDir --driver $Driver
            if ($LASTEXITCODE -ne 0) { throw "Bundled first-launch smoke failed with exit code $LASTEXITCODE" }
        } finally {
            $state = Restore-WebViewPolicy -RegistryRoot $registry -SubKeyPath $subKeyPath `
                -ValueName $valueName -StatePath $statePath
            if ($null -ne $state -and -not $state.restored) { throw 'WebView2 policy restoration failed' }
        }
    }
} finally { $registry.Dispose() }
