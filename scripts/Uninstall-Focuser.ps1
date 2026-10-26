param([switch]$Silent)
$ErrorActionPreference = 'Stop'
# Installed removals require live service authorization. If installed files are
# absent, delegate to the explicit administrator recovery and its stored checks.
$programFiles = [Environment]::GetFolderPath([Environment+SpecialFolder]::ProgramFiles)
if (-not [Environment]::Is64BitProcess) {
    throw 'Run this launcher with 64-bit Windows PowerShell.'
}
$installation = Join-Path $programFiles 'Focuser'
$uninstaller = Join-Path $installation 'uninstall.exe'
$helper = Join-Path $installation 'focuser-service.exe'
if(-not (Test-Path -LiteralPath $installation)) {
    Write-Output 'Installed files are absent. Starting explicit administrator leftovers recovery; stored commitments are still checked.'
    & (Join-Path $PSScriptRoot 'Recover-FocuserLeftovers.ps1')
    return
}
foreach ($path in @($installation, $uninstaller, $helper)) {
    if (-not (Test-Path -LiteralPath $path)) {
        throw "Focuser installation is damaged or absent: $path. Nothing was removed. Use the explicit administrator recovery procedure in docs/service-phase2.md; do not erase retained policy to bypass an unavailable authority."
    }
    if ((Get-Item -LiteralPath $path -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) {
        throw "Refusing a redirected installation path: $path"
    }
}
$arguments = @('/DELETE_DATA')
$capabilities = @(& $helper cleanup-capabilities 2>&1)
if ($LASTEXITCODE -ne 0 -or $capabilities -notcontains 'focuser.complete-removal.v1') {
    throw 'This installed version does not support complete removal. Nothing was removed. Use its supported maintenance/recovery procedure; this launcher cannot upgrade or bypass it.'
}
if ($Silent) { $arguments += '/S' }
$process = Start-Process -FilePath $uninstaller -ArgumentList $arguments -Verb RunAs -PassThru -Wait
if ($process.ExitCode -ne 0) {
    throw "Complete uninstall failed or was refused (exit $($process.ExitCode)). Read the uninstaller Details. No fallback cleanup was attempted."
}
# NSIS can run a temporary child uninstaller. Do not interpret the initial
# launcher's exit code alone as proof that its child authorized/completed removal.
if ((Test-Path -LiteralPath $installation) -or
    (Test-Path -LiteralPath 'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Focuser')) {
    throw 'Uninstall was refused, cancelled or incomplete. Installed files or registration remain; read the uninstaller Details. No fallback cleanup was attempted.'
}
Write-Output 'The installed uninstaller completed successfully.'
