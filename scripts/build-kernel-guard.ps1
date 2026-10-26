param(
 [Parameter(Mandatory=$true)][ValidatePattern('^[0-9]+(\.[0-9]+)?$')][string]$AllocatedAltitude,
 [Parameter(Mandatory=$true)][string]$OutputDirectory,
 [string]$MSBuild,
 [switch]$Production
)
$ErrorActionPreference='Stop'
if($Production){throw 'kernel_guard_not_certified: release blockers in drivers/focuser-guard/README.md are unresolved. No production driver package can be built.'}
if(-not $MSBuild){
 $vswhere=Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
 if(Test-Path -LiteralPath $vswhere){$MSBuild=(& $vswhere -latest -products * -requires Microsoft.Component.MSBuild -find 'MSBuild\**\Bin\MSBuild.exe'|Select-Object -First 1)}
}
if(-not $MSBuild -or -not (Test-Path -LiteralPath $MSBuild)){throw 'kernel_guard_toolchain_missing: MSVC/MSBuild with Windows Driver Kit is required; no driver was built or installed.'}
$root=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$output=[IO.Path]::GetFullPath($OutputDirectory)
if(-not [IO.Path]::IsPathRooted($OutputDirectory) -or $output -eq [IO.Path]::GetPathRoot($output)){throw 'An absolute dedicated output directory is required.'}
if($output.StartsWith($env:WINDIR,[StringComparison]::OrdinalIgnoreCase) -or $output.StartsWith($env:ProgramFiles,[StringComparison]::OrdinalIgnoreCase) -or $output.StartsWith((Join-Path $env:ProgramData 'FocuserAuthority'),[StringComparison]::OrdinalIgnoreCase)){throw 'Build output must not target Windows, installed binaries or authoritative storage.'}
[IO.Directory]::CreateDirectory($output)|Out-Null
& $MSBuild (Join-Path $root 'drivers\focuser-guard\FocuserGuard.vcxproj') /m:1 /p:Configuration=Release /p:Platform=x64 '/p:FocuserGuardExperimental=true' ('/p:FocuserGuardAltitude='+$AllocatedAltitude) ('/p:OutDir='+$output+'/') ('/p:IntDir='+$output+'/obj/') /p:SignMode=Off
if($LASTEXITCODE -ne 0){throw 'WDK compilation failed; no install/load operation was attempted.'}
$driver=Join-Path $output 'FocuserGuard.sys'
if(-not (Test-Path -LiteralPath $driver)){throw 'The WDK did not produce FocuserGuard.sys.'}
[ordered]@{experimental=$true;production_ready=$false;driver_sha256=(Get-FileHash -LiteralPath $driver).Hash;altitude=$AllocatedAltitude;installed=$false;signed=$false}|ConvertTo-Json|Set-Content -LiteralPath (Join-Path $output 'experimental-build.json') -Encoding UTF8
Write-Output 'Experimental build only. No INF/catalog deployment, signing, installation or driver load performed.'
