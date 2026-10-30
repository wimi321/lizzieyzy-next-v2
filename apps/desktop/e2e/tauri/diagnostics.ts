import { execFile } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import { basename, join } from "node:path";

// Run before deleting a session or killing the driver: a post-cleanup process
// list cannot tell whether the application survived a POST /session timeout.
export async function recordWindowsProcesses(directory: string, scenario: string, application: string): Promise<void> {
  const script = `
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
$names = @($env:TAURI_E2E_PROCESS_NAME, 'msedgedriver.exe', 'msedgewebview2.exe',
  'tauri-driver.exe', 'e2e-native-driver.exe', 'fake-katago.exe')
$processes = @(Get-CimInstance Win32_Process | Where-Object { $_.Name -in $names } |
  Select-Object Name, ProcessId, ParentProcessId, ExecutablePath, CommandLine)
[ordered]@{
  capturedAt = [DateTime]::UtcNow.ToString('o')
  applicationAlive = @($processes | Where-Object Name -eq $env:TAURI_E2E_PROCESS_NAME).Count -gt 0
  nativeDriverAlive = @($processes | Where-Object Name -eq 'msedgedriver.exe').Count -gt 0
  processes = $processes
} | ConvertTo-Json -Depth 4
`;
  const snapshot = await new Promise<string>((resolve) => {
    execFile("powershell.exe", ["-NoLogo", "-NoProfile", "-NonInteractive", "-Command", script], {
      env: { ...process.env, TAURI_E2E_PROCESS_NAME: basename(application) },
      timeout: 5_000,
      maxBuffer: 1024 * 1024,
      windowsHide: true,
      encoding: "utf8",
    }, (error, stdout, stderr) => {
      // Preserve partial output even if CIM fails or exceeds its safety limit.
      resolve(error ? JSON.stringify({ capturedAt: new Date().toISOString(), error: error.message, stdout, stderr }, null, 2)
        : stdout);
    });
  });
  console.log(`Windows processes before cleanup (${scenario}):\n${snapshot}`);
  await mkdir(directory, { recursive: true });
  await writeFile(join(directory, `${scenario}-processes.json`), snapshot);
}
