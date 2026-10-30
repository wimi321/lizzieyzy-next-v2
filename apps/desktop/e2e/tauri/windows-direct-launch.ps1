param(
    [string]$Application = (Join-Path $PSScriptRoot '../../../../target/debug/lizzieyzy-desktop.exe'),
    [string]$Sgf = (Join-Path $PSScriptRoot 'fixtures/mainline.sgf'),
    [string]$OutputDirectory = (Join-Path $PSScriptRoot '../../test-results/tauri/direct-launch')
)

# This is a cold-start observation, not a WebDriver readiness test. Do not add
# automation flags, a debug port, or a user-data override to this baseline.
$env:PATH = "$HOME/.cargo/bin;$env:PATH"
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'The direct-launch diagnostic requires Windows.' }

$Application = [IO.Path]::GetFullPath($Application)
$Sgf = [IO.Path]::GetFullPath($Sgf)
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
$configuration = Join-Path ([IO.Path]::GetTempPath()) "lizzie direct launch $([Guid]::NewGuid())"
$userDataFolder = Join-Path $env:LOCALAPPDATA 'io.github.wimi321.lizzieyzy.next2'
$webViewProfile = Join-Path $userDataFolder 'EBWebView'
$reportPath = Join-Path $OutputDirectory 'report.json'
$report = [ordered]@{
    startedAt = [DateTime]::UtcNow.ToString('o')
    stage = 'before launch'
    application = $Application
    arguments = @("--config-dir=$configuration", "--open=$Sgf")
    observationSeconds = 15
    interpretation = 'No debug port was added. A missing DevToolsActivePort alone does not establish a startup failure.'
    shellProcessId = $PID
    shellSessionId = (Get-Process -Id $PID).SessionId
    environment = [ordered]@{}
    errors = [System.Collections.Generic.List[string]]::new()
}
$child = $null
$observedProcesses = @()

function Save-Report {
    $report | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $reportPath -Encoding utf8
}

function Record-Error([string]$Stage, $Failure) {
    $report.errors.Add("${Stage}: $Failure")
    Write-Warning "${Stage}: $Failure"
}

function Get-ProfileSnapshot {
    $snapshot = [ordered]@{
        capturedAt = [DateTime]::UtcNow.ToString('o')
        path = $webViewProfile
        exists = Test-Path -LiteralPath $webViewProfile -PathType Container
        entries = @()
        userDataFolder = $userDataFolder
        userDataFolderExists = Test-Path -LiteralPath $userDataFolder -PathType Container
        userDataFolderEntries = @()
        devToolsActivePorts = @()
    }
    if ($snapshot.exists) {
        # List only the top level; do not upload cookies or the browser profile.
        $snapshot.entries = @(Get-ChildItem -LiteralPath $webViewProfile -Force |
            Select-Object Name, PSIsContainer, Length, LastWriteTimeUtc)
    }
    if ($snapshot.userDataFolderExists) {
        $snapshot.userDataFolderEntries = @(Get-ChildItem -LiteralPath $userDataFolder -Force |
            Select-Object Name, PSIsContainer, Length, LastWriteTimeUtc)
        $scanErrors = @()
        # Also search siblings of EBWebView: the API userDataFolder is its
        # parent, and a debugging file there must not be reported as missing.
        $snapshot.devToolsActivePorts = @(Get-ChildItem -LiteralPath $userDataFolder -Force -Recurse -File `
            -Filter 'DevToolsActivePort' -ErrorAction SilentlyContinue -ErrorVariable +scanErrors | ForEach-Object {
                [ordered]@{
                    path = $_.FullName
                    insideEBWebView = $_.FullName.StartsWith("$webViewProfile$([IO.Path]::DirectorySeparatorChar)",
                        [StringComparison]::OrdinalIgnoreCase)
                    modifiedAt = $_.LastWriteTimeUtc.ToString('o')
                    content = Get-Content -LiteralPath $_.FullName -Raw `
                        -ErrorAction SilentlyContinue -ErrorVariable +scanErrors
                }
            })
        foreach ($failure in $scanErrors) { Record-Error 'profile scan' $failure }
    }
    return $snapshot
}

foreach ($name in @('WEBVIEW2_USER_DATA_FOLDER', 'WEBVIEW2_BROWSER_EXECUTABLE_FOLDER',
    'WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS', 'WEBVIEW2_PIPE_FOR_SCRIPT_DEBUGGER',
    'TAURI_AUTOMATION', 'TAURI_WEBVIEW_AUTOMATION')) {
    $report.environment[$name] = [Environment]::GetEnvironmentVariable($name)
}
Save-Report

try {
    try {
        $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
        $principal = [Security.Principal.WindowsPrincipal]::new($identity)
        $report.isAdministrator = $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
        # Includes the Mandatory Label integrity SID (S-1-16-*), independent of
        # localization. Elevated WebView2 hosts may ignore WEBVIEW2_* variables.
        & whoami.exe /groups /fo csv | Set-Content -LiteralPath (Join-Path $OutputDirectory 'integrity-groups.csv')
        if ($LASTEXITCODE -ne 0) { throw "whoami exited with $LASTEXITCODE" }
    } catch { Record-Error 'identity' $_ }
    try { $report.profileBefore = Get-ProfileSnapshot } catch { Record-Error 'profile before launch' $_ }
    Save-Report

    try {
        if (-not (Test-Path -LiteralPath $Application -PathType Leaf)) { throw "Application missing: $Application" }
        if (-not (Test-Path -LiteralPath $Sgf -PathType Leaf)) { throw "SGF missing: $Sgf" }
        New-Item -ItemType Directory -Path $configuration | Out-Null
        # No real engine or existing settings are needed to observe WebView2.
        '{"version":1,"engine":null,"pondering":false}' |
            Set-Content -LiteralPath (Join-Path $configuration 'settings.json') -Encoding utf8
        # Start-Process joins ArgumentList into one native string. Quote each
        # complete equals-form argument so the temporary path's spaces survive.
        $arguments = '"--config-dir={0}" "--open={1}"' -f $configuration, $Sgf
        $child = Start-Process -FilePath $Application -ArgumentList $arguments -PassThru `
            -WorkingDirectory ([IO.Path]::GetDirectoryName($Application)) `
            -RedirectStandardOutput (Join-Path $OutputDirectory 'application-stdout.log') `
            -RedirectStandardError (Join-Path $OutputDirectory 'application-stderr.log')
        $report.applicationProcessId = $child.Id
        $report.stage = 'observing'
        Save-Report
        # Explicitly requested 15-second diagnostic sample, not synchronization
        # or a claim that elapsed time establishes readiness.
        Start-Sleep -Seconds 15
        $child.Refresh()
        $report.applicationAlive = -not $child.HasExited
        $report.applicationExitCode = if ($child.HasExited) { $child.ExitCode } else { $null }
        if (-not $child.HasExited) {
            $report.applicationSessionId = $child.SessionId
            $report.mainWindowHandle = $child.MainWindowHandle.ToInt64()
            $report.mainWindowTitle = $child.MainWindowTitle
            $report.responding = $child.Responding
        }
    } catch { Record-Error 'launch or observation' $_ }

    $report.stage = 'capturing'
    Save-Report
    try { $report.profileAfter = Get-ProfileSnapshot } catch { Record-Error 'profile after launch' $_ }
    try {
        $observedProcesses = @(Get-CimInstance Win32_Process -OperationTimeoutSec 5 |
            Where-Object { $_.Name -eq 'msedgewebview2.exe' -or ($child -and $_.ProcessId -eq $child.Id) } |
            Select-Object Name, ProcessId, ParentProcessId, SessionId, CreationDate, ExecutablePath, CommandLine)
        $report.processes = $observedProcesses
    } catch { Record-Error 'process snapshot' $_ }
    Save-Report

    $bitmap = $null
    $graphics = $null
    try {
        Add-Type -AssemblyName System.Windows.Forms
        Add-Type -AssemblyName System.Drawing
        $bounds = [Windows.Forms.SystemInformation]::VirtualScreen
        $report.screenBounds = [ordered]@{ x = $bounds.X; y = $bounds.Y; width = $bounds.Width; height = $bounds.Height }
        $bitmap = [Drawing.Bitmap]::new($bounds.Width, $bounds.Height)
        $graphics = [Drawing.Graphics]::FromImage($bitmap)
        $graphics.CopyFromScreen($bounds.Location, [Drawing.Point]::Empty, $bounds.Size)
        $bitmap.Save((Join-Path $OutputDirectory 'desktop.png'), [Drawing.Imaging.ImageFormat]::Png)
        $report.screenshot = 'desktop.png'
    } catch { Record-Error 'full desktop screenshot' $_ }
    finally {
        if ($graphics) { $graphics.Dispose() }
        if ($bitmap) { $bitmap.Dispose() }
    }
} finally {
    $report.stage = 'cleanup'
    Save-Report
    if ($child) {
        try {
            $child.Refresh()
            if (-not $child.HasExited) {
                # Only this launch's process tree, never all WebView2 processes.
                $child.Kill($true)
                if (-not $child.WaitForExit(5000)) { throw 'Application did not exit after process-tree cleanup' }
            }
            # A crashed parent may have left WebView2 descendants behind. Find
            # only descendants from our snapshot, checking creation time before
            # acting on a PID that Windows could have reused.
            $ownedIds = [System.Collections.Generic.HashSet[uint32]]::new()
            [void]$ownedIds.Add([uint32]$child.Id)
            do {
                $count = $ownedIds.Count
                foreach ($process in $observedProcesses) {
                    if ($ownedIds.Contains([uint32]$process.ParentProcessId)) {
                        [void]$ownedIds.Add([uint32]$process.ProcessId)
                    }
                }
            } while ($ownedIds.Count -ne $count)
            foreach ($process in $observedProcesses) {
                if ($process.ProcessId -eq $child.Id -or -not $ownedIds.Contains([uint32]$process.ProcessId)) { continue }
                $live = Get-Process -Id $process.ProcessId -ErrorAction SilentlyContinue
                if ($live) {
                    # CIM may round a Process timestamp; a sub-millisecond
                    # tolerance avoids silently leaving those children alive.
                    $difference = ($live.StartTime.ToUniversalTime() - $process.CreationDate.ToUniversalTime()).Duration()
                    if ($difference.TotalMilliseconds -lt 1) {
                        $live.Kill($true)
                        if (-not $live.WaitForExit(5000)) { throw "Descendant $($process.ProcessId) did not exit" }
                    } else {
                        Record-Error 'descendant cleanup' "PID $($process.ProcessId) has a different creation time; left untouched"
                    }
                }
            }
            $report.applicationStopped = $child.HasExited
        } catch { Record-Error 'process cleanup' $_ }
        finally { $child.Dispose() }
    }
    try {
        if (Test-Path -LiteralPath $configuration) { Remove-Item -LiteralPath $configuration -Recurse -Force }
    } catch { Record-Error 'temporary configuration cleanup' $_ }
    # Preserve the default WebView2 profile: it may predate this diagnostic.
    $report.stage = 'complete'
    $report.finishedAt = [DateTime]::UtcNow.ToString('o')
    Save-Report
    Get-Content -LiteralPath $reportPath -Raw | Write-Output
}

if ($report.errors.Count -gt 0 -or -not $report.applicationAlive) {
    throw 'Direct launch has errors or the application exited; inspect direct-launch/report.json.'
}
